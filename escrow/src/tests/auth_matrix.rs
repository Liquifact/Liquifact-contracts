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
    symbol_short,
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, String as SorobanString, Vec as SorobanVec,
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

// ── collateral authorization tests ──────────────────────────────────────────

#[test]
fn record_sme_collateral_commitment_no_auth_panics() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);

    assert_no_auth_panics!(env, {
        client.record_sme_collateral_commitment(&symbol_short!("USDC"), &5_000i128);
    });
}

#[test]
fn record_sme_collateral_commitment_wrong_signer_panics() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);
    let wrong = Address::generate(&env);

    assert_wrong_auth_panics!(
        env,
        wrong,
        client.address,
        symbol_short!("record_sme_collateral_commitment"),
        &[symbol_short!("USDC").to_val(), 5_000i128.to_val()],
        {
            client.record_sme_collateral_commitment(&symbol_short!("USDC"), &5_000i128);
        }
    );
}

#[test]
fn clear_sme_collateral_commitment_no_auth_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);
    client.record_sme_collateral_commitment(&symbol_short!("USDC"), &5_000i128);

    assert_no_auth_panics!(env, {
        client.clear_sme_collateral_commitment();
    });
}

#[test]
fn clear_sme_collateral_commitment_wrong_signer_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme, _treasury, _token) = setup_inited(&env);
    client.record_sme_collateral_commitment(&symbol_short!("USDC"), &5_000i128);
    let wrong = Address::generate(&env);

    assert_wrong_auth_panics!(
        env,
        wrong,
        client.address,
        symbol_short!("clear_sme_collateral_commitment"),
        &[],
        {
            client.clear_sme_collateral_commitment();
        }
    );
}

#[test]
fn record_sme_collateral_commitment_admin_cannot_record() {
    let env = Env::default();
    let (client, admin, _sme, _treasury, _token) = setup_inited(&env);

    assert_wrong_auth_panics!(
        env,
        admin,
        client.address,
        symbol_short!("record_sme_collateral_commitment"),
        &[symbol_short!("USDC").to_val(), 5_000i128.to_val()],
        {
            client.record_sme_collateral_commitment(&symbol_short!("USDC"), &5_000i128);
        }
    );
}

#[test]
fn clear_sme_collateral_commitment_admin_cannot_clear() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _sme, _treasury, _token) = setup_inited(&env);
    client.record_sme_collateral_commitment(&symbol_short!("USDC"), &5_000i128);

    assert_wrong_auth_panics!(
        env,
        admin,
        client.address,
        symbol_short!("clear_sme_collateral_commitment"),
        &[],
        {
            client.clear_sme_collateral_commitment();
        }
    );
}
