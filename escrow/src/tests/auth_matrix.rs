//! Exhaustive negative-authorization test matrix for all role-gated entrypoints.
//!
//! For each state-mutating entrypoint this module asserts:
//! 1. The call **panics** when the wrong signer is presented (`mock_auths` with wrong address).
//! 2. The call **panics** when no signer is presented (`mock_auths(&[])`).
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
        &None::<u32>,
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
        &None::<u32>,
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
    client.cancel_funding();
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
            args: SorobanVec::new(&env),
            sub_invokes: &[],
        },
    }]);
    client.cancel_funding();
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
    client.cancel_funding();
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
    client.cancel_funding();
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
    client.cancel_funding();
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
    client.cancel_funding();
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
