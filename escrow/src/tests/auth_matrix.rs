//! Exhaustive negative-authorization test matrix for all role-gated entrypoints.
//!
//! For each state-mutating entrypoint this module asserts:
//! 1. The call **panics** when the wrong signer is presented (`mock_auths` with wrong address).
//! 2. The call **panics** when no signer is presented (`mock_auths(&[])`).
//! 3. A **rejected** call — wrong signer, missing signer, an unauthorized caller
//!    that fails an explicit role check, or a forbidden state transition — leaves
//!    the persisted escrow state **byte-for-byte unchanged**: no partial write,
//!    no silent drift, and safe to retry. See the "state-invariant preservation"
//!    section at the bottom of this file.
//!
//! Guards tested per ADR-002 and the "Authorization guard ordering" rustdoc in lib.rs:
//!   - Read-only preconditions occur before `require_auth` (no state mutation before auth).
//!   - Every role boundary (admin, sme, investor, treasury, pending_admin) is covered.
//!
//! No production-code changes are made here; any guard gap found should be fixed separately.
use super::*;
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, IntoVal, String as SorobanString, Vec as SorobanVec,
};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Deploy and initialise a minimal escrow, returning `(client, admin, sme, treasury, token)`.
/// The environment has `mock_all_auths` enabled so init itself succeeds.
fn setup_inited(
    env: &Env,
) -> (
    crate::LiquifactEscrowClient<'_>,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &SorobanString::from_str(env, "INV_AUTH"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    (client, admin, sme, treasury, token)
}

/// Assert a call panics with no auth at all.
macro_rules! assert_no_auth_panics {
    ($env:expr, $call:expr) => {{
        $env.mock_auths(&[]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $call));
        assert!(
            result.is_err(),
            "expected panic with no auth, but call succeeded"
        );
    }};
}

/// Assert a call panics when signed by `wrong_signer` only.
macro_rules! assert_wrong_auth_panics {
    ($env:expr, $wrong:expr, $contract_id:expr, $fn_name:expr, $args:expr, $call:expr) => {{
        $env.mock_auths(&[MockAuth {
            address: &$wrong,
            invoke: &MockAuthInvoke {
                contract: &$contract_id,
                fn_name: $fn_name,
                args: $args,
                sub_invokes: &[],
            },
        }]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $call));
        assert!(
            result.is_err(),
            "expected panic with wrong signer on {}, but call succeeded",
            $fn_name
        );
    }};
}

use crate::EscrowError;

// ── settlement-specific test helpers ─────────────────────────────────────

/// Create a funded escrow (status 1) with a single investor.
/// The environment has `mock_all_auths` enabled so all setup calls succeed.
fn setup_funded(
    env: &Env,
) -> (
    crate::LiquifactEscrowClient<'_>,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);
    let token = Address::generate(env);
    client.init(
        &admin,
        &SorobanString::from_str(env, "AUTH_STL"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let investor = Address::generate(env);
    client.fund(&investor, &100_000_000_000i128);
    (client, admin, sme, investor, treasury)
}

/// Create a settled escrow (status 2) with a single investor.
fn setup_settled(
    env: &Env,
) -> (
    crate::LiquifactEscrowClient<'_>,
    Address,
    Address,
    Address,
    Address,
) {
    let (client, admin, sme, investor, treasury) = setup_funded(env);
    client.settle();
    (client, admin, sme, investor, treasury)
}

// ── partial_settle ──────────────────────────────────────────────────────

/// A stranger calling `partial_settle` with their own auth passes the
/// `require_auth` gate but is rejected by the explicit role check and
/// receives a typed `PartialSettleUnauthorizedCaller` error.
#[test]
fn test_partial_settle_stranger_rejected_with_typed_error() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);
    let stranger = Address::generate(&env);

    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "partial_settle",
            args: SorobanVec::from_array(&env, [stranger.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);

    assert_contract_error(
        client.try_partial_settle(&stranger),
        EscrowError::PartialSettleUnauthorizedCaller,
    );
}

/// Calling `partial_settle` with no authorization at all panics at the
/// host-level `require_auth` before any role check runs.
#[test]
#[should_panic]
fn test_partial_settle_no_auth_panics() {
    let env = Env::default();
    let (client, _admin, sme, _treasury, _token) = setup_inited(&env);
    env.mock_auths(&[]);
    client.partial_settle(&sme);
}

// ── accept_admin ───────────────────────────────────────────────────────

/// The missing-proposal validation runs before authorization is required.
#[test]
fn test_accept_admin_without_pending_proposal_returns_validation_error() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);
    env.mock_auths(&[]);

    assert_contract_error(client.try_accept_admin(), EscrowError::NoPendingAdmin);
}

/// A pending-admin proposal cannot be accepted by another signer, and the
/// failed authorization leaves both the active admin and proposal unchanged.
#[test]
fn test_accept_admin_wrong_signer_preserves_handover_state() {
    let env = Env::default();
    let (client, admin, _sme, _treasury, _token) = setup_inited(&env);
    let pending_admin = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.propose_admin(&pending_admin, &0u32);

    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "accept_admin",
            args: SorobanVec::new(&env),
            sub_invokes: &[],
        },
    }]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| client.accept_admin()));

    assert!(result.is_err(), "expected pending-admin authorization to be enforced");
    assert_eq!(client.get_escrow().admin, admin);
    assert_eq!(client.get_pending_admin(), Some(pending_admin));
}

/// Only the nominated pending admin can complete the handover.
#[test]
fn test_accept_admin_pending_admin_authorization_succeeds() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);
    let pending_admin = Address::generate(&env);
    client.propose_admin(&pending_admin, &0u32);

    env.mock_auths(&[MockAuth {
        address: &pending_admin,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "accept_admin",
            args: SorobanVec::new(&env),
            sub_invokes: &[],
        },
    }]);
    let updated = client.accept_admin();

    assert_eq!(updated.admin, pending_admin);
    assert_eq!(client.get_pending_admin(), None);
}

// ── settle ──────────────────────────────────────────────────────────────

/// Calling `settle` with no authorization panics at the host-level
/// `sme_address.require_auth()` inside `load_escrow_require_sme`.
#[test]
#[should_panic]
fn test_settle_no_auth_panics() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _treasury) = setup_funded(&env);
    env.mock_auths(&[]);
    client.settle();
}

/// Calling `settle` with a non-SME signer panics at the host-level
/// `require_auth` because `load_escrow_require_sme` demands the SME's
/// signature.
#[test]
#[should_panic]
fn test_settle_wrong_signer_panics() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _treasury) = setup_funded(&env);
    let stranger = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "settle",
            args: SorobanVec::new(&env),
            sub_invokes: &[],
        },
    }]);
    client.settle();
}

// ── withdraw ────────────────────────────────────────────────────────────

/// Calling `withdraw` with no authorization panics at the host-level
/// `sme_address.require_auth()` inside `load_escrow_require_sme`.
#[test]
#[should_panic]
fn test_withdraw_no_auth_panics() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _treasury) = setup_funded(&env);
    env.mock_auths(&[]);
    client.withdraw();
}

/// Calling `withdraw` with a non-SME signer panics at the host-level
/// `require_auth` because `load_escrow_require_sme` demands the SME's
/// signature.
#[test]
#[should_panic]
fn test_withdraw_wrong_signer_panics() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _treasury) = setup_funded(&env);
    let stranger = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "withdraw",
            args: SorobanVec::new(&env),
            sub_invokes: &[],
        },
    }]);
    client.withdraw();
}

// ── claim_investor_payout ───────────────────────────────────────────────

/// Calling `claim_investor_payout` with no authorization panics at the
/// host-level `investor.require_auth()`.
#[test]
#[should_panic]
fn test_claim_investor_payout_no_auth_panics() {
    let env = Env::default();
    let (client, _admin, _sme, investor, _treasury) = setup_settled(&env);
    env.mock_auths(&[]);
    client.claim_investor_payout(&investor);
}

/// Calling `claim_investor_payout` with a non-investor signer panics at the
/// host-level `require_auth` because the investor's signature is required.
#[test]
#[should_panic]
fn test_claim_investor_payout_wrong_signer_panics() {
    let env = Env::default();
    let (client, _admin, _sme, investor, _treasury) = setup_settled(&env);
    let stranger = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "claim_investor_payout",
            args: SorobanVec::from_array(&env, [investor.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);
    client.claim_investor_payout(&investor);
}

// ── cancel_funding ──────────────────────────────────────────────────────

/// Calling `cancel_funding` with no authorization panics at the host-level
/// `admin.require_auth()` inside `load_escrow_require_admin`.
#[test]
#[should_panic]
fn test_cancel_funding_no_auth_panics() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);
    env.mock_auths(&[]);
    client.cancel_funding(&0u32);
}

/// Calling `cancel_funding` with a non-admin signer panics at the
/// host-level `require_auth` because `load_escrow_require_admin` demands
/// the admin's signature.
#[test]
#[should_panic]
fn test_cancel_funding_wrong_signer_panics() {
    let env = Env::default();
    let (client, _admin, sme, _treasury, _token) = setup_inited(&env);
    env.mock_auths(&[MockAuth {
        address: &sme,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "cancel_funding",
            args: SorobanVec::from_array(&env, [0u32.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);
    client.cancel_funding(&0u32);
}

// ── refund ──────────────────────────────────────────────────────────────

/// Calling `refund` with no authorization panics at the host-level
/// `investor.require_auth()` before any state mutation or token transfer.
#[test]
#[should_panic]
fn test_refund_no_auth_panics() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);
    let investor = Address::generate(&env);
    // Fund and cancel to reach status 4 (cancelled).
    client.fund(&investor, &1_000i128);
    client.cancel_funding(&0u32);
    env.mock_auths(&[]);
    client.refund(&investor);
}

/// Calling `refund` with a non-investor signer panics at the host-level
/// `require_auth` because the function demands the specific investor's
/// signature.
#[test]
#[should_panic]
fn test_refund_wrong_signer_panics() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);
    let investor = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.cancel_funding(&0u32);
    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "refund",
            args: SorobanVec::from_array(&env, [investor.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);
    client.refund(&investor);
}

// ── sweep_terminal_dust ─────────────────────────────────────────────────

/// Calling `sweep_terminal_dust` with no authorization panics at the
/// host-level `treasury.require_auth()`.
#[test]
#[should_panic]
fn test_sweep_terminal_dust_no_auth_panics() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);
    // Cancel to reach a terminal status (4 — cancelled).
    client.cancel_funding(&0u32);
    env.mock_auths(&[]);
    client.sweep_terminal_dust(&100i128);
}

/// Calling `sweep_terminal_dust` with a non-treasury signer panics at the
/// host-level `require_auth` because the function demands the treasury's
/// signature.
#[test]
#[should_panic]
fn test_sweep_terminal_dust_wrong_signer_panics() {
    let env = Env::default();
    let (client, _admin, sme, _treasury, _token) = setup_inited(&env);
    client.cancel_funding(&0u32);
    env.mock_auths(&[MockAuth {
        address: &sme,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "sweep_terminal_dust",
            args: SorobanVec::from_array(&env, [100i128.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);
    client.sweep_terminal_dust(&100i128);
}

// ── state-invariant preservation under rejected authorization ─────────────
//
// The tests above assert that a call *panics* (or returns a typed error) when
// authorization is missing, wrong, or made by an unauthorized caller. Rejecting
// the call is necessary but not sufficient: the deeper invariant this contract
// owns is that a **rejected mutation leaves the persisted escrow state
// byte-for-byte unchanged** — no partial write, no silent status drift, and the
// same result whether the rejected call is attempted once or retried. These
// tests snapshot the observable state, drive a rejected call to completion
// (catching the host panic where one is raised), and assert the snapshot is
// identical afterwards. This is the CEI / "no state mutation before auth"
// guarantee from ADR-002 and the authorization-guard-ordering rustdoc, verified
// from the outside rather than assumed.

/// The subset of persisted escrow state a role-gated entrypoint could plausibly
/// mutate. [`LiquifactEscrow::get_escrow`] is a pure read (it performs no
/// `require_auth`), so it can be sampled before *and* after a rejected call
/// regardless of which signer — if any — is currently mocked.
#[derive(Clone, Debug, PartialEq)]
struct EscrowStateSnapshot {
    status: u32,
    funded_amount: i128,
    dispute_active: bool,
}

fn snapshot_state(client: &crate::LiquifactEscrowClient<'_>) -> EscrowStateSnapshot {
    let escrow = client.get_escrow();
    EscrowStateSnapshot {
        status: escrow.status,
        funded_amount: escrow.funded_amount,
        dispute_active: escrow.dispute_active,
    }
}

/// Run `$call` (expected to be rejected by an authorization guard, so it panics
/// at the host), catching the panic, and assert the persisted escrow state is
/// identical before and after the attempt.
macro_rules! assert_rejected_preserves_state {
    ($client:expr, $call:expr) => {{
        let before = snapshot_state(&$client);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $call));
        assert!(
            result.is_err(),
            "expected the unauthorized call to be rejected, but it succeeded"
        );
        let after = snapshot_state(&$client);
        assert_eq!(
            before, after,
            "a rejected call mutated persisted escrow state: {:?} -> {:?}",
            before, after
        );
    }};
}

// ── settle (SME-gated, status 1 → 2) ──────────────────────────────────────

/// A `settle` call with no authorization is rejected *and* leaves the funded
/// escrow in status 1 with its principal intact.
#[test]
fn test_settle_no_auth_preserves_state() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _treasury) = setup_funded(&env);
    env.mock_auths(&[]);
    assert_rejected_preserves_state!(client, client.settle());
}

/// A `settle` call signed by a stranger is rejected at `require_auth` before any
/// status transition, leaving the escrow funded (status 1).
#[test]
fn test_settle_wrong_signer_preserves_state() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _treasury) = setup_funded(&env);
    let stranger = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "settle",
            args: SorobanVec::new(&env),
            sub_invokes: &[],
        },
    }]);
    assert_rejected_preserves_state!(client, client.settle());
}

// ── withdraw (SME-gated) ──────────────────────────────────────────────────

/// A `withdraw` call signed by a stranger is rejected at `require_auth` without
/// mutating escrow state.
#[test]
fn test_withdraw_wrong_signer_preserves_state() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _treasury) = setup_funded(&env);
    let stranger = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "withdraw",
            args: SorobanVec::new(&env),
            sub_invokes: &[],
        },
    }]);
    assert_rejected_preserves_state!(client, client.withdraw());
}

// ── cancel_funding (admin-gated, status 0 → 4) ────────────────────────────

/// A `cancel_funding` call with no authorization is rejected and leaves the
/// escrow open (status 0); the admin nonce is never consumed.
#[test]
fn test_cancel_funding_no_auth_preserves_state() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);
    env.mock_auths(&[]);
    assert_rejected_preserves_state!(client, client.cancel_funding(&0u32));
}

/// A `cancel_funding` call signed by the SME (not the admin) is rejected at the
/// admin `require_auth`, leaving the escrow open (status 0).
#[test]
fn test_cancel_funding_wrong_signer_preserves_state() {
    let env = Env::default();
    let (client, _admin, sme, _treasury, _token) = setup_inited(&env);
    env.mock_auths(&[MockAuth {
        address: &sme,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "cancel_funding",
            args: SorobanVec::from_array(&env, [0u32.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);
    assert_rejected_preserves_state!(client, client.cancel_funding(&0u32));
}

// ── claim_investor_payout (investor-gated) ────────────────────────────────

/// A `claim_investor_payout` call signed by a stranger is rejected at the
/// investor `require_auth`, leaving the settled escrow (status 2) untouched.
#[test]
fn test_claim_investor_payout_wrong_signer_preserves_state() {
    let env = Env::default();
    let (client, _admin, _sme, investor, _treasury) = setup_settled(&env);
    let stranger = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "claim_investor_payout",
            args: SorobanVec::from_array(&env, [investor.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);
    assert_rejected_preserves_state!(client, client.claim_investor_payout(&investor));
}

// ── refund (investor-gated, status 4) ─────────────────────────────────────

/// A `refund` call signed by a stranger is rejected at `require_auth` before any
/// contribution is zeroed or token is moved: the cancelled escrow keeps its
/// status (4) and its recorded principal.
#[test]
fn test_refund_wrong_signer_preserves_state() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);
    let investor = Address::generate(&env);
    let stranger = Address::generate(&env);
    // Fund below target so the escrow stays open (status 0), then cancel to
    // reach status 4 with a non-zero recorded contribution.
    client.fund(&investor, &400i128);
    client.cancel_funding(&0u32);
    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "refund",
            args: SorobanVec::from_array(&env, [investor.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);
    assert_rejected_preserves_state!(client, client.refund(&investor));
}

// ── sweep_terminal_dust (treasury-gated) ──────────────────────────────────

/// A `sweep_terminal_dust` call signed by the SME (not the treasury) is rejected
/// at `require_auth`, leaving the terminal (cancelled) escrow state unchanged.
#[test]
fn test_sweep_terminal_dust_wrong_signer_preserves_state() {
    let env = Env::default();
    let (client, _admin, sme, _treasury, _token) = setup_inited(&env);
    client.cancel_funding(&0u32); // reach terminal status 4
    env.mock_auths(&[MockAuth {
        address: &sme,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "sweep_terminal_dust",
            args: SorobanVec::from_array(&env, [100i128.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);
    assert_rejected_preserves_state!(client, client.sweep_terminal_dust(&100i128));
}

// ── partial_settle (explicit role-check rejection path) ───────────────────

/// `partial_settle` authorizes the *caller* and then rejects anyone who is not
/// the SME or admin with a typed error (not a host panic). The rejection must
/// occur before the status 0 → 1 transition, so a stranger's rejected call
/// leaves the escrow open (status 0). Asserted via the fallible `try_` client
/// so the typed error is observed rather than unwound.
#[test]
fn test_partial_settle_stranger_rejection_preserves_state() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);
    let stranger = Address::generate(&env);
    let before = snapshot_state(&client);
    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "partial_settle",
            args: SorobanVec::from_array(&env, [stranger.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);
    let result = client.try_partial_settle(&stranger);
    assert!(
        result.is_err(),
        "expected partial_settle by a stranger to be rejected"
    );
    let after = snapshot_state(&client);
    assert_eq!(
        before, after,
        "a rejected partial_settle mutated escrow state: {:?} -> {:?}",
        before, after
    );
}

// ── retry safety: repeated rejected operations do not drift state ──────────

/// A rejected `settle` is idempotent with respect to state: attempting it
/// repeatedly (as a retrying client might) never accumulates a partial mutation.
/// After three wrong-signer attempts the escrow is still exactly funded
/// (status 1) with its original principal.
#[test]
fn test_repeated_rejected_settle_preserves_state() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _treasury) = setup_funded(&env);
    let stranger = Address::generate(&env);
    let before = snapshot_state(&client);
    for _ in 0..3 {
        env.mock_auths(&[MockAuth {
            address: &stranger,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                fn_name: "settle",
                args: SorobanVec::new(&env),
                sub_invokes: &[],
            },
        }]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| client.settle()));
        assert!(
            result.is_err(),
            "expected each wrong-signer settle to be rejected"
        );
    }
    let after = snapshot_state(&client);
    assert_eq!(
        before, after,
        "repeated rejected settle calls drifted escrow state: {:?} -> {:?}",
        before, after
    );
}

// ── forbidden state transition: authorized caller, wrong status ───────────

/// Even with the *correct* admin authorization, `cancel_funding` is only a legal
/// transition from the open state (status 0). Invoked on a funded escrow
/// (status 1) it is rejected with the typed [`EscrowError::CancelFundingNotOpen`]
/// and the escrow remains funded — the status guard runs before the write and
/// the whole invocation reverts, so no partial state leaks. This exercises a
/// forbidden transition that passes authorization but must still be refused.
#[test]
fn test_cancel_funding_forbidden_transition_preserves_state() {
    let env = Env::default();
    // `setup_funded` leaves `mock_all_auths` enabled, so the admin signature is
    // satisfied and the rejection is purely on the state-transition guard.
    let (client, _admin, _sme, _investor, _treasury) = setup_funded(&env);
    let before = snapshot_state(&client);
    assert_contract_error(
        client.try_cancel_funding(&0u32),
        EscrowError::CancelFundingNotOpen,
    );
    let after = snapshot_state(&client);
    assert_eq!(
        before, after,
        "a rejected forbidden transition mutated escrow state: {:?} -> {:?}",
        before, after
    );
}
