//! Tests for [`LiquifactEscrow::get_settlement_config`].
//!
//! Covers:
//! - Default values before [`LiquifactEscrow::init`] is called.
//! - Values reflect what was passed to `init`.
//! - Values match the individual getters so the bundled view cannot drift.
//! - The bundled view is idempotent (pure read).
//! - The struct shape is pinned via field-by-field destructuring.

use super::super::{LiquifactEscrow, LiquifactEscrowClient, SettlementConfig};
use crate::DEFAULT_MATURITY_MAX_HORIZON_SECS;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

// ── helpers ──────────────────────────────────────────────────────────────────

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

/// Init with caller-supplied `yield_bps`, `maturity`, and `protocol_fee_bps`.
fn init_escrow(
    env: &Env,
    client: &LiquifactEscrowClient,
    yield_bps: i64,
    maturity: u64,
    protocol_fee_bps: Option<i64>,
) {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "SETCFG01"),
        &sme,
        &10_000i128,
        &yield_bps,
        &maturity,
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
        &protocol_fee_bps,
    );
}

// ── tests ────────────────────────────────────────────────────────────────────

/// Before `init`, every field must return its documented default.
#[test]
fn test_defaults_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let config = client.get_settlement_config();
    assert_eq!(config.yield_bps, 0);
    assert_eq!(config.maturity, 0);
    assert_eq!(config.protocol_fee_bps, 0);
    assert_eq!(config.yield_tiers.len(), 0);
    assert_eq!(
        config.maturity_max_horizon,
        DEFAULT_MATURITY_MAX_HORIZON_SECS
    );
    assert_eq!(config.funding_deadline, None);
    assert_eq!(config.min_contribution_floor, 0);
    assert_eq!(config.max_unique_investors_cap, None);
    assert_eq!(config.max_per_investor_cap, None);
}

/// After `init`, fields should reflect the values supplied at init time.
#[test]
fn test_values_after_init_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, 800i64, 0u64, None);

    let config = client.get_settlement_config();
    assert_eq!(config.yield_bps, 800);
    assert_eq!(config.maturity, 0);
    assert_eq!(config.protocol_fee_bps, 0);
}

/// Non-zero `maturity` must be reflected.
#[test]
fn test_values_after_init_with_maturity() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, 800i64, 2_000_000u64, None);

    let config = client.get_settlement_config();
    assert_eq!(config.maturity, 2_000_000);
}

/// Explicit `protocol_fee_bps` at init must be reflected.
#[test]
fn test_values_after_init_with_protocol_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, 800i64, 0u64, Some(500i64));

    let config = client.get_settlement_config();
    assert_eq!(config.protocol_fee_bps, 500);
}

/// `get_settlement_config` must match the individual getters so the bundled view
/// cannot drift from the authoritative single-key reads.
#[test]
fn test_config_matches_individual_getters() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, 800i64, 0u64, Some(500i64));

    let config = client.get_settlement_config();
    let escrow = client.get_escrow();
    assert_eq!(config.yield_bps, escrow.yield_bps);
    assert_eq!(config.maturity, escrow.maturity);
    assert_eq!(config.protocol_fee_bps, client.get_protocol_fee_bps());
}

/// `get_settlement_config` has the expected shape — verified via field-by-field
/// destructuring so a future struct change causes a compile error.
#[test]
fn test_config_struct_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, 800i64, 0u64, None);

    let SettlementConfig {
        yield_bps,
        maturity,
        protocol_fee_bps,
        yield_tiers,
        maturity_max_horizon,
        funding_deadline,
        min_contribution_floor,
        max_unique_investors_cap,
        max_per_investor_cap,
    } = client.get_settlement_config();

    assert_eq!(yield_bps, 800);
    assert_eq!(maturity, 0);
    assert_eq!(protocol_fee_bps, 0);
    assert_eq!(yield_tiers.len(), 0);
    assert_eq!(maturity_max_horizon, DEFAULT_MATURITY_MAX_HORIZON_SECS);
    assert_eq!(funding_deadline, None);
    assert_eq!(min_contribution_floor, 0);
    assert_eq!(max_unique_investors_cap, None);
    assert_eq!(max_per_investor_cap, None);
}

/// Zero `protocol_fee_bps` (omitted at init) returns `0`, not an error.
#[test]
fn test_zero_protocol_fee_default() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, 800i64, 0u64, None);

    assert_eq!(client.get_settlement_config().protocol_fee_bps, 0);
}

/// All fields remain stable between multiple calls (pure read, no state mutation).
#[test]
fn test_config_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, 800i64, 2_000_000u64, Some(500i64));

    let a = client.get_settlement_config();
    let b = client.get_settlement_config();
    assert_eq!(a.yield_bps, b.yield_bps);
    assert_eq!(a.maturity, b.maturity);
    assert_eq!(a.protocol_fee_bps, b.protocol_fee_bps);
    assert_eq!(a.maturity_max_horizon, b.maturity_max_horizon);
    assert_eq!(a.funding_deadline, b.funding_deadline);
    assert_eq!(a.min_contribution_floor, b.min_contribution_floor);
    assert_eq!(a.max_unique_investors_cap, b.max_unique_investors_cap);
    assert_eq!(a.max_per_investor_cap, b.max_per_investor_cap);
}

/// Defaults before init are also idempotent.
#[test]
fn test_defaults_idempotent_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let a = client.get_settlement_config();
    let b = client.get_settlement_config();
    assert_eq!(a.yield_bps, b.yield_bps);
    assert_eq!(a.maturity, b.maturity);
    assert_eq!(a.protocol_fee_bps, b.protocol_fee_bps);
    assert_eq!(a.maturity_max_horizon, b.maturity_max_horizon);
}
