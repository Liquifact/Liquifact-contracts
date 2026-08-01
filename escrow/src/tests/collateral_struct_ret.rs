//! Struct return validation tests for the SME collateral commitment feature.
//!
//! Verifies that `get_collateral_config()` returns a `CollateralConfig` struct
//! with correctly typed, readable, and comparable fields — replacing the opaque
//! tuple interface described in issue #1086.

use super::super::{
    CollateralCommitmentSnapshot, CollateralConfig, EscrowError, LiquifactEscrow,
    LiquifactEscrowClient, SmeCollateralCommitment, MAX_INVOICE_AMOUNT,
};
use crate::tests::assert_contract_error;
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

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
        &soroban_sdk::String::from_str(env, "STRUCTRET01"),
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

// ── Field Presence Tests ──────────────────────────────────────────────────

/// `CollateralConfig` exposes a `collateral_limit` field of type `i128`.
#[test]
fn test_collateral_config_has_collateral_limit_field() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let config: CollateralConfig = client.get_collateral_config();
    // Field access compiles and returns the correct default.
    assert_eq!(config.collateral_limit, MAX_INVOICE_AMOUNT);
}

/// `CollateralConfig` exposes a `sme_commitment` field of type `CollateralCommitmentSnapshot`.
#[test]
fn test_collateral_config_has_sme_commitment_field() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let config: CollateralConfig = client.get_collateral_config();
    // Field access compiles; default state has no commitment.
    assert_eq!(config.sme_commitment, CollateralCommitmentSnapshot::None);
}

// ── Equality and Clone Tests ──────────────────────────────────────────────

/// Two calls to `get_collateral_config()` with no intervening mutations return
/// equal structs — structural equality holds.
#[test]
fn test_collateral_config_structural_equality() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let config_a = client.get_collateral_config();
    let config_b = client.get_collateral_config();
    assert_eq!(config_a, config_b);
}

/// After recording an SME commitment, `config.sme_commitment` reflects the
/// `CollateralCommitmentSnapshot::Some(_)` variant with the stored data.
#[test]
fn test_collateral_config_some_commitment_after_record() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let (_, _sme) = init_escrow(&env, &client);

    let asset = Symbol::new(&env, "USDC");
    let commitment = client.record_sme_collateral_commitment(&asset, &5_000i128);

    let config = client.get_collateral_config();
    match config.sme_commitment {
        CollateralCommitmentSnapshot::Some(c) => {
            assert_eq!(c.amount, 5_000i128);
        }
        CollateralCommitmentSnapshot::None => {
            panic!("expected Some commitment after record");
        }
    }
}

/// After clearing the commitment, `config.sme_commitment` reverts to `None`.
#[test]
fn test_collateral_config_none_after_clear() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let asset = Symbol::new(&env, "USDC");
    client.record_sme_collateral_commitment(&asset, &5_000i128);
    client.clear_sme_collateral_commitment();

    let config = client.get_collateral_config();
    assert_eq!(config.sme_commitment, CollateralCommitmentSnapshot::None);
}

// ── Struct vs Individual Getter Consistency ───────────────────────────────

/// `config.collateral_limit` matches `get_collateral_limit()` at all times.
#[test]
fn test_collateral_config_limit_matches_individual_getter() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    // Default state.
    let config = client.get_collateral_config();
    assert_eq!(config.collateral_limit, client.get_collateral_limit());

    // After update.
    client.set_collateral_limit(&3_000i128);
    let config_updated = client.get_collateral_config();
    assert_eq!(config_updated.collateral_limit, client.get_collateral_limit());
    assert_eq!(config_updated.collateral_limit, 3_000i128);
}

/// `config.sme_commitment` matches `get_sme_collateral_commitment()` at all times.
#[test]
fn test_collateral_config_commitment_matches_individual_getter() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    // No commitment: both should be absent.
    let config = client.get_collateral_config();
    let direct = client.get_sme_collateral_commitment();
    assert_eq!(config.sme_commitment, CollateralCommitmentSnapshot::None);
    assert_eq!(direct, None);

    // After recording: struct field and getter must agree.
    let asset = Symbol::new(&env, "USDC");
    let stored = client.record_sme_collateral_commitment(&asset, &7_500i128);
    let config_after = client.get_collateral_config();
    let direct_after = client.get_sme_collateral_commitment();

    assert_eq!(config_after.sme_commitment, CollateralCommitmentSnapshot::Some(stored));
    assert_eq!(direct_after, Some(stored));
}

// ── Pre-init Default Tests ────────────────────────────────────────────────

/// Before `init`, `get_collateral_config()` returns sensible defaults without panicking.
#[test]
fn test_collateral_config_default_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    // No auth needed for a read-only view.
    let config = client.get_collateral_config();
    assert_eq!(config.collateral_limit, MAX_INVOICE_AMOUNT);
    assert_eq!(config.sme_commitment, CollateralCommitmentSnapshot::None);
}

/// The returned struct value is self-consistent: `collateral_limit` is always
/// positive and `sme_commitment` is always a valid `CollateralCommitmentSnapshot`
/// variant — no partial / undefined state can leak through the struct boundary.
#[test]
fn test_collateral_config_struct_is_always_well_formed() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let config = client.get_collateral_config();
    assert!(config.collateral_limit > 0, "limit must be positive");
    // CollateralCommitmentSnapshot is a sealed enum — the match is exhaustive.
    let _: bool = match config.sme_commitment {
        CollateralCommitmentSnapshot::None => true,
        CollateralCommitmentSnapshot::Some(_) => true,
    };
}
