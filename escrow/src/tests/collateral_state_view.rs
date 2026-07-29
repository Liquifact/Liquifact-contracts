//! Tests for [`LiquifactEscrow::get_collateral_state`] — the flattened O(1) collateral read view.
//!
//! Coverage: unset default, recorded state, replacement, clear-then-read, agreement with the
//! existing getters, boundary amounts, and the absence of any storage mutation.

use super::super::{
    CollateralCommitmentSnapshot, CollateralState, LiquifactEscrow, LiquifactEscrowClient,
    MAX_INVOICE_AMOUNT,
};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, Symbol};

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn init_escrow(env: &Env, client: &LiquifactEscrowClient) -> (Address, Address) {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "COLSTATE1"),
        &sme,
        &10_000i128,
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
    (admin, sme)
}

/// Unset collateral must return the documented default, not a panic.
#[test]
fn test_collateral_state_defaults_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let state = client.get_collateral_state();

    assert!(!state.is_set);
    assert_eq!(state.asset, Symbol::new(&env, ""));
    assert_eq!(state.amount, 0);
    assert_eq!(state.recorded_at, 0);
    assert_eq!(state.collateral_limit, MAX_INVOICE_AMOUNT);
}

/// After init but before any commitment the view is still the unset default.
#[test]
fn test_collateral_state_defaults_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let state = client.get_collateral_state();

    assert_eq!(
        state,
        CollateralState {
            is_set: false,
            asset: Symbol::new(&env, ""),
            amount: 0,
            recorded_at: 0,
            collateral_limit: MAX_INVOICE_AMOUNT,
        }
    );
}

/// A recorded commitment is surfaced field-for-field, including the ledger timestamp.
#[test]
fn test_collateral_state_reflects_recorded_commitment() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 4_242;
    env.ledger().set(ledger_info);

    let asset = Symbol::new(&env, "USDC");
    let commitment = client.record_sme_collateral_commitment(&asset, &5_000i128);

    let state = client.get_collateral_state();

    assert!(state.is_set);
    assert_eq!(state.asset, commitment.asset);
    assert_eq!(state.amount, commitment.amount);
    assert_eq!(state.recorded_at, commitment.recorded_at);
    assert_eq!(state.recorded_at, 4_242);
    assert_eq!(state.collateral_limit, MAX_INVOICE_AMOUNT);
}

/// Replacing a commitment must be reflected immediately — no stale values.
#[test]
fn test_collateral_state_reflects_replacement() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.record_sme_collateral_commitment(&Symbol::new(&env, "USDC"), &1_000i128);

    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 900;
    env.ledger().set(ledger_info);

    client.record_sme_collateral_commitment(&Symbol::new(&env, "EURC"), &2_500i128);

    let state = client.get_collateral_state();

    assert!(state.is_set);
    assert_eq!(state.asset, Symbol::new(&env, "EURC"));
    assert_eq!(state.amount, 2_500i128);
    assert_eq!(state.recorded_at, 900);
}

/// Clearing returns the view to the unset default.
#[test]
fn test_collateral_state_returns_default_after_clear() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.record_sme_collateral_commitment(&Symbol::new(&env, "USDC"), &1_000i128);
    client.clear_sme_collateral_commitment();

    let state = client.get_collateral_state();

    assert!(!state.is_set);
    assert_eq!(state.asset, Symbol::new(&env, ""));
    assert_eq!(state.amount, 0);
    assert_eq!(state.recorded_at, 0);
}

/// The view must agree with the pre-existing getters — it reuses stored values, never recomputes.
#[test]
fn test_collateral_state_matches_existing_getters() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.record_sme_collateral_commitment(&Symbol::new(&env, "USDC"), &7_777i128);

    let state = client.get_collateral_state();
    let config = client.get_collateral_config();
    let limit = client.get_collateral_limit();
    let commitment = client
        .get_sme_collateral_commitment()
        .expect("commitment was just recorded");

    assert_eq!(state.collateral_limit, limit);
    assert_eq!(state.collateral_limit, config.collateral_limit);
    assert_eq!(
        config.sme_commitment,
        CollateralCommitmentSnapshot::Some(commitment.clone())
    );
    assert_eq!(state.asset, commitment.asset);
    assert_eq!(state.amount, commitment.amount);
    assert_eq!(state.recorded_at, commitment.recorded_at);
}

/// The minimum accepted amount (1) is surfaced without truncation.
#[test]
fn test_collateral_state_minimum_amount_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.record_sme_collateral_commitment(&Symbol::new(&env, "USDC"), &1i128);

    let state = client.get_collateral_state();
    assert!(state.is_set);
    assert_eq!(state.amount, 1i128);
}

/// The maximum accepted amount (the default limit) is surfaced without wraparound.
#[test]
fn test_collateral_state_maximum_amount_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.record_sme_collateral_commitment(&Symbol::new(&env, "USDC"), &MAX_INVOICE_AMOUNT);

    let state = client.get_collateral_state();
    assert!(state.is_set);
    assert_eq!(state.amount, MAX_INVOICE_AMOUNT);
    assert_eq!(state.collateral_limit, MAX_INVOICE_AMOUNT);
}

/// An admin-updated limit is reflected even while no commitment exists.
#[test]
fn test_collateral_state_reflects_updated_limit_when_unset() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.set_collateral_limit(&2_500i128);

    let state = client.get_collateral_state();
    assert!(!state.is_set);
    assert_eq!(state.collateral_limit, 2_500i128);
}

/// The view is read-only: repeated calls are stable and do not mutate stored state.
#[test]
fn test_collateral_state_is_read_only_and_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.record_sme_collateral_commitment(&Symbol::new(&env, "USDC"), &3_000i128);

    let first = client.get_collateral_state();
    let second = client.get_collateral_state();
    assert_eq!(first, second);

    // The underlying commitment is untouched by the reads.
    let commitment = client
        .get_sme_collateral_commitment()
        .expect("commitment must survive read-only views");
    assert_eq!(commitment.amount, 3_000i128);
    assert_eq!(second.amount, 3_000i128);
}
