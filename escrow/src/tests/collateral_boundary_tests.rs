//! Boundary and fuzz-style tests for collateral functionality.
//!
//! These tests validate numeric and length boundaries for collateral operations,
//! including min, max, zero, and over-limit inputs.

use super::super::{
    CollateralCommitmentSnapshot, EscrowError, LiquifactEscrow, LiquifactEscrowClient,
    MAX_INVOICE_AMOUNT,
};
use crate::tests::assert_contract_error;
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

fn setup_escrow(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "BOUND01"),
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

    (client, admin, sme)
}

// ── Collateral Limit Boundary Tests ─────────────────────────────────────────

#[test]
fn test_collateral_limit_at_max_invoice_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    // Set limit exactly at MAX_INVOICE_AMOUNT (should succeed)
    client.set_collateral_limit(&MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

#[test]
fn test_collateral_limit_at_max_minus_one() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    // Set limit at MAX_INVOICE_AMOUNT - 1 (should succeed)
    let limit = MAX_INVOICE_AMOUNT - 1;
    client.set_collateral_limit(&limit);
    assert_eq!(client.get_collateral_limit(), limit);
}

#[test]
fn test_collateral_limit_exceeds_max_by_one() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    // Set limit at MAX_INVOICE_AMOUNT + 1 (should be rejected)
    assert_contract_error(
        client.try_set_collateral_limit(&(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceedsMax,
    );

    // Limit should remain unchanged
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

#[test]
fn test_collateral_limit_exceeds_max_by_large_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    // Set limit at a very large value (should be rejected)
    let large_value = i128::MAX / 2;
    assert_contract_error(
        client.try_set_collateral_limit(&large_value),
        EscrowError::CollateralLimitExceedsMax,
    );

    // Limit should remain unchanged
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

#[test]
fn test_collateral_limit_at_min_positive_value() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    // Set limit at 1 (minimum positive value, should succeed)
    client.set_collateral_limit(&1i128);
    assert_eq!(client.get_collateral_limit(), 1i128);
}

#[test]
fn test_collateral_limit_at_zero_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    // Set limit at 0 (should be rejected)
    assert_contract_error(
        client.try_set_collateral_limit(&0i128),
        EscrowError::CollateralLimitNotPositive,
    );

    // Limit should remain unchanged
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

#[test]
fn test_collateral_limit_negative_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    // Set limit at -1 (should be rejected)
    assert_contract_error(
        client.try_set_collateral_limit(&-1i128),
        EscrowError::CollateralLimitNotPositive,
    );

    // Limit should remain unchanged
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

// ── SME Collateral Commitment Boundary Tests ───────────────────────────────

#[test]
fn test_sme_commitment_at_configured_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    // Set a custom limit
    let limit = 5_000i128;
    client.set_collateral_limit(&limit);

    let asset = Symbol::new(&env, "USDC");

    // Record at exactly the limit (should succeed)
    client.record_sme_collateral_commitment(&asset, &limit);

    let commitment = client.get_sme_collateral_commitment();
    assert!(commitment.is_some());
}

#[test]
fn test_sme_commitment_at_limit_minus_one() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    // Set a custom limit
    let limit = 5_000i128;
    client.set_collateral_limit(&limit);

    let asset = Symbol::new(&env, "USDC");

    // Record at limit - 1 (should succeed)
    client.record_sme_collateral_commitment(&asset, &(limit - 1));

    let commitment = client.get_sme_collateral_commitment();
    assert!(commitment.is_some());
}

#[test]
fn test_sme_commitment_exceeds_limit_by_one() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    // Set a custom limit
    let limit = 5_000i128;
    client.set_collateral_limit(&limit);

    let asset = Symbol::new(&env, "USDC");

    // Record at limit + 1 (should be rejected)
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &(limit + 1)),
        EscrowError::CollateralLimitExceeded,
    );

    // Commitment should remain None
    let commitment = client.get_sme_collateral_commitment();
    assert_eq!(commitment, None);
}

#[test]
fn test_sme_commitment_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    let asset = Symbol::new(&env, "USDC");

    // Record at 0 (should be rejected)
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &0i128),
        EscrowError::CollateralAmountNotPositive,
    );

    // Commitment should remain None
    let commitment = client.get_sme_collateral_commitment();
    assert_eq!(commitment, None);
}

#[test]
fn test_sme_commitment_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    let asset = Symbol::new(&env, "USDC");

    // Record at -1 (should be rejected)
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &-1i128),
        EscrowError::CollateralAmountNotPositive,
    );

    // Commitment should remain None
    let commitment = client.get_sme_collateral_commitment();
    assert_eq!(commitment, None);
}

#[test]
fn test_sme_commitment_empty_asset_symbol_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    let asset = Symbol::new(&env, "");

    // Record with empty asset symbol (should be rejected)
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &1_000i128),
        EscrowError::CollateralAssetEmpty,
    );

    // Commitment should remain None
    let commitment = client.get_sme_collateral_commitment();
    assert_eq!(commitment, None);
}

#[test]
fn test_sme_commitment_very_large_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    // Set a custom limit
    let limit = 5_000i128;
    client.set_collateral_limit(&limit);

    let asset = Symbol::new(&env, "USDC");

    // Record at a very large amount (should be rejected)
    let large_amount = i128::MAX / 2;
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &large_amount),
        EscrowError::CollateralLimitExceeded,
    );

    // Commitment should remain None
    let commitment = client.get_sme_collateral_commitment();
    assert_eq!(commitment, None);
}

// ── Collateral Config View Boundary Tests ──────────────────────────────────

#[test]
fn test_collateral_config_after_multiple_limit_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    // Start with default
    let config = client.get_collateral_config();
    assert_eq!(config.collateral_limit, MAX_INVOICE_AMOUNT);

    // Update to a lower limit
    client.set_collateral_limit(&3_000i128);
    let config = client.get_collateral_config();
    assert_eq!(config.collateral_limit, 3_000i128);

    // Update to a higher limit (still within bounds)
    client.set_collateral_limit(&8_000i128);
    let config = client.get_collateral_config();
    assert_eq!(config.collateral_limit, 8_000i128);

    // Update back to max
    client.set_collateral_limit(&MAX_INVOICE_AMOUNT);
    let config = client.get_collateral_config();
    assert_eq!(config.collateral_limit, MAX_INVOICE_AMOUNT);
}

#[test]
fn test_collateral_commitment_clear_after_rejection() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup_escrow(&env);

    let limit = 5_000i128;
    client.set_collateral_limit(&limit);
    let asset = Symbol::new(&env, "USDC");

    // Record a valid commitment
    client.record_sme_collateral_commitment(&asset, &2_000i128);
    let commitment = client.get_sme_collateral_commitment();
    assert!(commitment.is_some());

    // Try to record an invalid commitment (should reject)
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &(limit + 1)),
        EscrowError::CollateralLimitExceeded,
    );

    // Previous commitment should remain unchanged
    let commitment_after = client.get_sme_collateral_commitment();
    assert_eq!(commitment, commitment_after);
}
