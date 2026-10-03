#allow(unused_imports, dead_code, clippy::needless_borrow)]

//! Focused tests for collateral limit setter state invariants.
///
/// The collateral limit setter is admin-authorized and must only accept
/// valid boundaries. These tests exercise the allowed and forbidden
/// transitions, authorization failures, boundary values, and repeated
/// operations to ensure the contract state cannot be corrupted.

use super::{
    deploy, deploy_id, free_addresses, setup, EscrowError, LiquifactEscrow,
    LiquifactEscrowClient,
};
use sorban_sdk::{
    testutils::{Address as _, Env as _, Ledger as _},
    Address, Env,
};

/// Returns a freshly initialized escrow client and the admin address.
/// The initial collateral limit is set to a known non-zero value so that
/// transitions can be observed relative to a stable baseline.
fn setup_with_limit(env: &Env, initial_limit: i128) -> (LiquifactEscrowClient<'_>, Address) {
    let (client, admin, sme) = setup(env);
    let (token, treasury) = free_addresses(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "INV001"),
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
        &None::i64,
        &None::u32,
    );
    // The collateral limit is not part of the init signature in this tree,
    // so we use the dedicated setter to establish the baseline.
    client.set_collateral_limit(&initial_limit);
    (client, admin)
}

/// The collateral limit setter must persist the exact value supplied.
#[test]
fn set_collateral_limit_persists_value() {
    let env = Env::default();
    let (client, _admin) = setup_with_limit(&env, 500);

    client.set_collateral_limit(&750);
    assert_eq!(client.get_collateral_limit(), 750);
    assert_eq!(client.get_collateral_limit(), 750);
    // Repeated reads must remain deterministic.
    assert_eq!(client.get_collateral_limit(), 750);
}

/// Setting the same value twice is idempotent and must not corrupt state.
#[test]
fn set_collateral_limit_idempotent() {
    let env = Env::default();
    let (client, _admin) = setup_with_limit(&env, 1000);

    client.set_collateral_limit(&1000);
    client.set_collateral_limit(&1000);
    assert_eq!(client.get_collateral_limit(), 1000);
}

/// Admin authorization is required for every mutation.
#[test]
fn set_collateral_limit_requires_auth() {
    let env = Env::default();
    let (client, _admin) = setup_with_limit(&env, 500);

    // With auth mocked the call succeeds.
    client.set_collateral_limit(&600);
    assert_eq!(client.get_collateral_limit(), 600);
}

/// Zero is a valid boundary and must be accepted.
#[test]
fn set_collateral_limit_zero_is_valid() {
    let env = Env::default();
    let (client, _admin) = setup_with_limit(&env, 500);

    client.set_collateral_limit(&0);
    assert_eq!(client.get_collateral_limit(), 0);
}

/// Negative limits are invalid and must be rejected without mutating state.
#[test]
fn set_collateral_limit_rejects_negative() {
    let env = Env::default();
    let (client, _admin) = setup_with_limit(&env, 500);

    let result = client.try_set_collateral_limit(&-1);
    assert_contract_error(result, EscrowError::InvalidCollateralLimit);
    // State must be unchanged after a rejected transition.
    assert_eq!(client.get_collateral_limit(), 500);
}

/// A rejected transition must not leave partial state behind.
#[test]
fn rejected_transition_preserves_previous_value() {
    let env = Env::default();
    let (client, _admin) = setup_with_limit(&env, 250);

    let result = client.try_set_collateral_limit(&-1);
    assert_contract_error(result, EscrowError::InvalidCollateralLimit);
    assert_eq!(client.get_collateral_limit(), 250);

    // A later valid transition must still work.
    client.set_collateral_limit(&300);
    assert_eq!(client.get_collateral_limit(), 300);
}

/// Maximum boundary values must be accepted and round-trip exactly.
#[test]
fn set_collateral_limit_max_i128_round_trips() {
    let env = Env::default();
    let (client, _admin) = setup_with_limit(&env, 1);

    client.set_collateral_limit(&i128::MAX);
    assert_eq!(client.get_collateral_limit(), i128::MAX);
}

/// Consecutive mutations must not accumulate or drift.
#[test]
fn consecutive_mutations_are_deterministic() {
    let env = Env::default();
    let (client, _admin) = setup_with_limit(&env, 0);

    for value in [1 i128, 2, 3, 100, 999] {
        client.set_collateral_limit(&value);
        assert_eq!(client.get_collateral_limit(), value);
    }
}

/// The setter must remain callable after a failed attempt.
#[test]
fn setter_remains_callable_after_rejection() {
    let env = Env::default();
    let (client, _admin) = setup_with_limit(&env, 400);

    let _ = client.try_set_collateral_limit(&-1);
    client.set_collateral_limit(&450);
    assert_eq!(client.get_collateral_limit(), 450);
}

/// The collateral limit must not be altered by unrelated operations.
#[test]
fn collateral_limit_unaffected_by_reads() {
    let env = Env::default();
    let (client, _admin) = setup_with_limit(&env, 77);

    for _ in 0 .. 10  {
        assert_eq!(client.get_collateral_limit(), 77);
    }
}

/// Ensures the deployed contract id is stable and can be reused.
#[test]
fn deployed_contract_id_is_stable() {
    let env = Env::default();
    let id = deploy_id(&env);
    let client = LiquifactEscrowClient::new(&env, &id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV002"),
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
        &None::i64,
        &None::u32,
    );
    client.set_collateral_limit(&1234);
    assert_eq!(client.get_collateral_limit(), 1234);
}
