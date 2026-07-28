//! Tests for [`LiquifactEscrow::get_settlement_config`].
//!
//! Covers:
//! - Default values before [`LiquifactEscrow::init`] is called.
//! - Values reflect what was passed to `init`.
//! - Values match the individual getters (`get_settlement_limit`, `get_protocol_fee_bps`,
//!   and `get_escrow().yield_bps` / `.maturity`).
//! - Reflects admin mutation via [`LiquifactEscrow::set_settlement_limit`].
//! - Edge cases: non-zero maturity, explicit protocol fee, min/max settlement limits.

use super::super::{
    LiquifactEscrow, LiquifactEscrowClient, SettlementConfig, DEFAULT_SETTLEMENT_LIMIT,
    MAX_SETTLEMENT_LIMIT, MIN_SETTLEMENT_LIMIT,
};
use soroban_sdk::testutils::Address as _;
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

    assert_eq!(
        config.settlement_limit,
        DEFAULT_SETTLEMENT_LIMIT,
        "settlement_limit should be DEFAULT_SETTLEMENT_LIMIT before init"
    );
    assert_eq!(config.yield_bps, 0, "yield_bps should be 0 before init");
    assert_eq!(
        config.protocol_fee_bps, 0,
        "protocol_fee_bps should be 0 before init"
    );
    assert_eq!(config.maturity, 0, "maturity should be 0 before init");
}

/// After `init`, fields should reflect the values supplied at init time.
#[test]
fn test_values_after_init_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    init_escrow(&env, &client, 800, 0, None);

    let config = client.get_settlement_config();

    assert_eq!(config.settlement_limit, DEFAULT_SETTLEMENT_LIMIT);
    assert_eq!(config.yield_bps, 800);
    assert_eq!(config.protocol_fee_bps, 0);
    assert_eq!(config.maturity, 0);
}

/// Non-zero `maturity` must be reflected.
#[test]
fn test_values_after_init_with_maturity() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    // Set ledger time so maturity > now passes validation.
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 1_000;
    env.ledger().set(ledger_info);

    let maturity: u64 = 1_000 + 60 * 60; // 1 hour from now
    init_escrow(&env, &client, 500, maturity, None);

    let config = client.get_settlement_config();
    assert_eq!(config.maturity, maturity);
    assert_eq!(config.yield_bps, 500);
}

/// Explicit `protocol_fee_bps` at init must be reflected.
#[test]
fn test_values_after_init_with_protocol_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    init_escrow(&env, &client, 300, 0, Some(250));

    let config = client.get_settlement_config();
    assert_eq!(config.protocol_fee_bps, 250);
    assert_eq!(config.yield_bps, 300);
}

/// `get_settlement_config` must match the individual getters so the bundled view
/// cannot drift from the authoritative single-key reads.
#[test]
fn test_config_matches_individual_getters() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    init_escrow(&env, &client, 750, 0, Some(100));

    let config = client.get_settlement_config();
    let escrow = client.get_escrow();

    assert_eq!(config.settlement_limit, client.get_settlement_limit());
    assert_eq!(config.yield_bps, escrow.yield_bps);
    assert_eq!(config.protocol_fee_bps, client.get_protocol_fee_bps());
    assert_eq!(config.maturity, escrow.maturity);
}

/// After `set_settlement_limit`, the view must reflect the updated value.
#[test]
fn test_settlement_limit_update_reflected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    init_escrow(&env, &client, 800, 0, None);

    // Starts at default.
    assert_eq!(
        client.get_settlement_config().settlement_limit,
        DEFAULT_SETTLEMENT_LIMIT
    );

    // Admin updates the limit.
    client.set_settlement_limit(&10);
    assert_eq!(client.get_settlement_config().settlement_limit, 10);

    // Update to min bound.
    client.set_settlement_limit(&MIN_SETTLEMENT_LIMIT);
    assert_eq!(
        client.get_settlement_config().settlement_limit,
        MIN_SETTLEMENT_LIMIT
    );

    // Update to max bound.
    client.set_settlement_limit(&MAX_SETTLEMENT_LIMIT);
    assert_eq!(
        client.get_settlement_config().settlement_limit,
        MAX_SETTLEMENT_LIMIT
    );
}

/// `get_settlement_config` has the expected shape (all four fields present) — verified
/// via field-by-field destructuring so a future struct change causes a compile error.
#[test]
fn test_config_struct_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    init_escrow(&env, &client, 600, 0, Some(50));
    client.set_settlement_limit(&20);

    let SettlementConfig {
        settlement_limit,
        yield_bps,
        protocol_fee_bps,
        maturity,
    } = client.get_settlement_config();

    assert_eq!(settlement_limit, 20);
    assert_eq!(yield_bps, 600);
    assert_eq!(protocol_fee_bps, 50);
    assert_eq!(maturity, 0);
}

/// Zero `protocol_fee_bps` (omitted at init) returns `0`, not an error.
#[test]
fn test_zero_protocol_fee_default() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    init_escrow(&env, &client, 1000, 0, None);

    let config = client.get_settlement_config();
    assert_eq!(config.protocol_fee_bps, 0);
}

/// All fields remain stable between multiple calls (pure read, no state mutation).
#[test]
fn test_config_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    init_escrow(&env, &client, 400, 0, Some(200));

    let first = client.get_settlement_config();
    let second = client.get_settlement_config();

    assert_eq!(first, second);
}

/// Defaults before init are also idempotent.
#[test]
fn test_defaults_idempotent_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let first = client.get_settlement_config();
    let second = client.get_settlement_config();

    assert_eq!(first, second);
}
