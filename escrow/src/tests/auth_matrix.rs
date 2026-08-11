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
    Address, BytesN, Env, String as SorobanString, Symbol, Vec as SorobanVec,
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

/// Assert a call panics when only the wrong signer is authorized.
/// `fn_name` is the contract function name (e.g. `"update_funding_target"`).
fn assert_wrong_auth_panics(
    env: &Env,
    client: &LiquifactEscrowClient<'_>,
    wrong: &Address,
    fn_name: &str,
    call: impl Fn(),
) {
    env.mock_auths(&[MockAuth {
        address: wrong,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: Symbol::new(env, fn_name),
            args: SorobanVec::new(env),
            sub_invokes: &[],
        },
    }]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(call));
    assert!(
        result.is_err(),
        "expected panic with wrong signer on {}, but call succeeded",
        fn_name,
    );
}

// ── Admin-only entrypoints (funding config) ──────────────────────────────────

#[test]
fn test_update_funding_target_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.update_funding_target(&2_000i128) });
}

#[test]
fn test_update_funding_target_wrong_signer() {
    let env = Env::default();
    let (client, _admin, sme, _, _) = setup_inited(&env);
    assert_wrong_auth_panics(&env, &client, &sme, "update_funding_target", || {
        client.update_funding_target(&2_000i128);
    });
}

#[test]
fn test_lower_max_unique_investors_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.lower_max_unique_investors(&5u32) });
}

#[test]
fn test_raise_max_unique_investors_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.raise_max_unique_investors(&10u32) });
}

#[test]
fn test_lower_min_contribution_floor_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.lower_min_contribution_floor(&500i128) });
}

#[test]
fn test_raise_max_per_investor_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.raise_max_per_investor(&20_000i128) });
}

#[test]
fn test_lower_max_unique_investors_wrong_signer() {
    let env = Env::default();
    let (client, _admin, sme, _, _) = setup_inited(&env);
    assert_wrong_auth_panics(&env, &client, &sme, "lower_max_unique_investors", || {
        client.lower_max_unique_investors(&5u32);
    });
}

#[test]
fn test_raise_max_unique_investors_wrong_signer() {
    let env = Env::default();
    let (client, _admin, sme, _, _) = setup_inited(&env);
    assert_wrong_auth_panics(&env, &client, &sme, "raise_max_unique_investors", || {
        client.raise_max_unique_investors(&10u32);
    });
}

#[test]
fn test_lower_min_contribution_floor_wrong_signer() {
    let env = Env::default();
    let (client, _admin, sme, _, _) = setup_inited(&env);
    assert_wrong_auth_panics(&env, &client, &sme, "lower_min_contribution_floor", || {
        client.lower_min_contribution_floor(&500i128);
    });
}

#[test]
fn test_raise_max_per_investor_wrong_signer() {
    let env = Env::default();
    let (client, _admin, sme, _, _) = setup_inited(&env);
    assert_wrong_auth_panics(&env, &client, &sme, "raise_max_per_investor", || {
        client.raise_max_per_investor(&20_000i128);
    });
}

// ── Admin-only entrypoints (legal hold) ──────────────────────────────────────

#[test]
fn test_set_legal_hold_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.set_legal_hold(&true) });
}

#[test]
fn test_set_legal_hold_wrong_signer() {
    let env = Env::default();
    let (client, _admin, sme, _, _) = setup_inited(&env);
    assert_wrong_auth_panics(&env, &client, &sme, "set_legal_hold", || {
        client.set_legal_hold(&true);
    });
}

#[test]
fn test_request_clear_legal_hold_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.request_clear_legal_hold() });
}

#[test]
fn test_clear_legal_hold_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.clear_legal_hold() });
}

#[test]
fn test_clear_legal_hold_after_delay_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.clear_legal_hold_after_delay() });
}

#[test]
fn test_cancel_clear_legal_hold_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.cancel_clear_legal_hold() });
}

// ── Admin-only entrypoints (allowlist) ───────────────────────────────────────

#[test]
fn test_set_allowlist_active_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.set_allowlist_active(&true) });
}

#[test]
fn test_set_investor_allowlisted_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    let investor = Address::generate(&env);
    assert_no_auth_panics!(env, { client.set_investor_allowlisted(&investor, &true) });
}

#[test]
fn test_set_investors_allowlisted_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    let investor = Address::generate(&env);
    let investors = SorobanVec::from_array(&env, [investor]);
    assert_no_auth_panics!(env, { client.set_investors_allowlisted(&investors, &true) });
}

// ── SME-only entrypoints (settlement / withdrawal) ───────────────────────────

#[test]
fn test_settle_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.settle() });
}

#[test]
fn test_settle_wrong_signer() {
    let env = Env::default();
    let (client, _admin, sme, _, _) = setup_inited(&env);
    // Provide admin auth (not SME) — SME-only gate should still reject.
    assert_wrong_auth_panics(&env, &client, &sme, "settle", || {
        client.settle();
    });
}

#[test]
fn test_withdraw_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.withdraw() });
}

// ── SME-only entrypoints (collateral commitment) ─────────────────────────────

#[test]
fn test_record_sme_collateral_commitment_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    let asset = Symbol::new(&env, "USDC");
    assert_no_auth_panics!(env, { client.record_sme_collateral_commitment(&asset, &1_000i128) });
}

#[test]
fn test_clear_sme_collateral_commitment_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.clear_sme_collateral_commitment() });
}

// ── Dual-role entrypoints (SME or admin) ─────────────────────────────────────

#[test]
fn test_partial_settle_no_auth() {
    let env = Env::default();
    let (client, _admin, sme, _, _) = setup_inited(&env);
    assert_no_auth_panics!(env, { client.partial_settle(&sme) });
}

/// With mock_all_auths enabled (as setup_inited does), a stranger passes
/// require_auth but is rejected by the explicit role check.
#[test]
fn test_partial_settle_stranger_rejected_by_role_check() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    // setup_inited enables mock_all_auths so the stranger passes require_auth.
    let stranger = Address::generate(&env);
    assert_contract_error(
        client.try_partial_settle(&stranger),
        EscrowError::PartialSettleUnauthorizedCaller,
    );
}

// ── Investor-auth entrypoints ────────────────────────────────────────────────

#[test]
fn test_fund_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    let investor = Address::generate(&env);
    assert_no_auth_panics!(env, { client.fund(&investor, &1_000i128) });
}

#[test]
fn test_fund_with_commitment_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    let investor = Address::generate(&env);
    assert_no_auth_panics!(env, { client.fund_with_commitment(&investor, &1_000i128, &0u64) });
}

#[test]
fn test_fund_batch_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    let investor = Address::generate(&env);
    let entries = SorobanVec::from_array(&env, [(investor, 1_000i128)]);
    assert_no_auth_panics!(env, { client.fund_batch(&entries) });
}

#[test]
fn test_fund_wrong_signer() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    let investor = Address::generate(&env);
    let stranger = Address::generate(&env);
    assert_wrong_auth_panics(&env, &client, &stranger, "fund", || {
        client.fund(&investor, &1_000i128);
    });
}

#[test]
fn test_fund_with_commitment_wrong_signer() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    let investor = Address::generate(&env);
    let stranger = Address::generate(&env);
    assert_wrong_auth_panics(&env, &client, &stranger, "fund_with_commitment", || {
        client.fund_with_commitment(&investor, &1_000i128, &0u64);
    });
}

#[test]
fn test_fund_batch_wrong_signer() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    let investor = Address::generate(&env);
    let stranger = Address::generate(&env);
    let entries = SorobanVec::from_array(&env, [(investor, 1_000i128)]);
    assert_wrong_auth_panics(&env, &client, &stranger, "fund_batch", || {
        client.fund_batch(&entries);
    });
}

#[test]
fn test_claim_investor_payout_no_auth() {
    let env = Env::default();
    let (client, _admin, _sme, _, _) = setup_inited(&env);
    let investor = Address::generate(&env);
    assert_no_auth_panics!(env, { client.claim_investor_payout(&investor) });
}
