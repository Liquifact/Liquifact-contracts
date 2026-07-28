// Comprehensive tests for collateral validation helper functions.
//
// These tests verify that the extracted validation helpers (validate_collateral_commitment
// and validate_collateral_limit) produce identical behavior to the original inline validation
// they replaced, and that they are correctly used by all collateral call sites.

use crate::tests::{assert_contract_error, setup};
use crate::{EscrowError, MAX_INVOICE_AMOUNT};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Symbol};

fn init_escrow(env: &Env, client: &crate::LiquifactEscrowClient, admin: &Address, sme: &Address) {
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "VALTEST"),
        sme,
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
}

// ============================================================================
// SUITE 1: Collateral Commitment Validation — Valid Inputs Pass
// ============================================================================

#[test]
fn test_validate_collateral_commitment_valid_positive_amount() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Record a valid collateral commitment with positive amount
    let asset = Symbol::new(&env, "USDC");
    let commitment = client.record_sme_collateral_commitment(&asset, &5_000i128);

    assert_eq!(commitment.amount, 5_000i128);
    assert_eq!(commitment.asset, asset);
}

#[test]
fn test_validate_collateral_commitment_valid_minimum_positive_amount() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Minimum valid value: amount = 1 (just above zero)
    let asset = Symbol::new(&env, "ETH");
    let commitment = client.record_sme_collateral_commitment(&asset, &1i128);

    assert_eq!(commitment.amount, 1i128);
}

#[test]
fn test_validate_collateral_commitment_valid_large_amount() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Large amount well within limits
    let asset = Symbol::new(&env, "BTC");
    let large_amount = 100_000_000i128;
    let commitment = client.record_sme_collateral_commitment(&asset, &large_amount);

    assert_eq!(commitment.amount, large_amount);
}

#[test]
fn test_validate_collateral_commitment_valid_non_empty_asset() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Various valid non-empty asset symbols
    let assets = vec![
        (Symbol::new(&env, "USDC"), "USDC"),
        (Symbol::new(&env, "ETH"), "ETH"),
        (Symbol::new(&env, "GOLD"), "GOLD"),
        (symbol_short!("XLM"), "XLM"),
    ];

    for (asset, name) in assets {
        let commitment = client.record_sme_collateral_commitment(&asset, &1_000i128);
        assert_eq!(commitment.asset, asset, "Asset {} should be recorded", name);
    }
}

#[test]
fn test_validate_collateral_commitment_valid_within_configured_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Set a specific limit
    client.set_collateral_limit(&10_000i128);

    // Record exactly at the limit
    let asset = Symbol::new(&env, "USDC");
    let commitment = client.record_sme_collateral_commitment(&asset, &10_000i128);

    assert_eq!(commitment.amount, 10_000i128);

    // Record below the limit
    let asset2 = Symbol::new(&env, "ETH");
    let commitment2 = client.record_sme_collateral_commitment(&asset2, &5_000i128);

    assert_eq!(commitment2.amount, 5_000i128);
}

// ============================================================================
// SUITE 2: Collateral Commitment Validation — Each Rejection Condition
// ============================================================================

#[test]
fn test_validate_collateral_commitment_rejects_zero_amount() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let asset = Symbol::new(&env, "USDC");

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &0i128),
        EscrowError::CollateralAmountNotPositive,
    );
}

#[test]
fn test_validate_collateral_commitment_rejects_negative_amount() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let asset = Symbol::new(&env, "USDC");

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &-100i128),
        EscrowError::CollateralAmountNotPositive,
    );

    // Also test with i128::MIN to ensure no overflow tricks
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &i128::MIN),
        EscrowError::CollateralAmountNotPositive,
    );
}

#[test]
fn test_validate_collateral_commitment_rejects_empty_asset() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let empty_asset = Symbol::new(&env, "");

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&empty_asset, &5_000i128),
        EscrowError::CollateralAssetEmpty,
    );
}

#[test]
fn test_validate_collateral_commitment_rejects_amount_exceeding_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Set a low limit
    client.set_collateral_limit(&1_000i128);

    let asset = Symbol::new(&env, "USDC");

    // Exactly at limit succeeds
    client.record_sme_collateral_commitment(&asset, &1_000i128);

    // One unit above limit fails
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &1_001i128),
        EscrowError::CollateralLimitExceeded,
    );

    // Significantly above limit fails
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &10_000i128),
        EscrowError::CollateralLimitExceeded,
    );
}

#[test]
fn test_validate_collateral_commitment_rejects_timestamp_backwards() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let asset = Symbol::new(&env, "GOLD");

    // Set initial timestamp to 5000
    env.ledger().set_timestamp(5000);

    // First record succeeds
    client.record_sme_collateral_commitment(&asset, &100i128);

    // Move timestamp backward to 100 (before the recorded_at)
    env.ledger().set_timestamp(100);

    // Attempt replacement with backward timestamp fails
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &200i128),
        EscrowError::CollateralTimestampBackwards,
    );
}

// ============================================================================
// SUITE 3: Collateral Limit Validation — Valid Inputs Pass
// ============================================================================

#[test]
fn test_validate_collateral_limit_valid_positive_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Set various valid limits
    client.set_collateral_limit(&1i128);
    assert_eq!(client.get_collateral_limit(), 1i128);

    client.set_collateral_limit(&100_000i128);
    assert_eq!(client.get_collateral_limit(), 100_000i128);

    client.set_collateral_limit(&1_000_000_000i128);
    assert_eq!(client.get_collateral_limit(), 1_000_000_000i128);
}

#[test]
fn test_validate_collateral_limit_valid_minimum_positive_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Minimum valid: limit = 1
    client.set_collateral_limit(&1i128);
    assert_eq!(client.get_collateral_limit(), 1i128);
}

#[test]
fn test_validate_collateral_limit_valid_maximum_invoice_amount() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Maximum allowed value is MAX_INVOICE_AMOUNT
    client.set_collateral_limit(&MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

// ============================================================================
// SUITE 4: Collateral Limit Validation — Each Rejection Condition
// ============================================================================

#[test]
fn test_validate_collateral_limit_rejects_zero_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    assert_contract_error(
        client.try_set_collateral_limit(&0i128),
        EscrowError::CollateralLimitNotPositive,
    );
}

#[test]
fn test_validate_collateral_limit_rejects_negative_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    assert_contract_error(
        client.try_set_collateral_limit(&-1i128),
        EscrowError::CollateralLimitNotPositive,
    );

    assert_contract_error(
        client.try_set_collateral_limit(&-100_000i128),
        EscrowError::CollateralLimitNotPositive,
    );
}

#[test]
fn test_validate_collateral_limit_rejects_limit_exceeding_max_invoice_amount() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // One unit above MAX_INVOICE_AMOUNT fails
    assert_contract_error(
        client.try_set_collateral_limit(&(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceedsMax,
    );

    // Significantly above fails
    assert_contract_error(
        client.try_set_collateral_limit(&i128::MAX),
        EscrowError::CollateralLimitExceedsMax,
    );
}

// ============================================================================
// SUITE 5: Boundary Values — Exact Limits
// ============================================================================

#[test]
fn test_validate_collateral_limit_boundary_just_below_minimum() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Just below minimum (0) must be rejected
    assert_contract_error(
        client.try_set_collateral_limit(&0i128),
        EscrowError::CollateralLimitNotPositive,
    );
}

#[test]
fn test_validate_collateral_limit_boundary_exactly_at_minimum() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Exactly at minimum (1) must succeed
    client.set_collateral_limit(&1i128);
    assert_eq!(client.get_collateral_limit(), 1i128);
}

#[test]
fn test_validate_collateral_limit_boundary_just_below_maximum() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Just below maximum must succeed
    let just_below_max = MAX_INVOICE_AMOUNT - 1;
    client.set_collateral_limit(&just_below_max);
    assert_eq!(client.get_collateral_limit(), just_below_max);
}

#[test]
fn test_validate_collateral_limit_boundary_exactly_at_maximum() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Exactly at maximum must succeed
    client.set_collateral_limit(&MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

#[test]
fn test_validate_collateral_commitment_boundary_just_below_minimum() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let asset = Symbol::new(&env, "USDC");

    // Just below minimum (0) must be rejected
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &0i128),
        EscrowError::CollateralAmountNotPositive,
    );
}

#[test]
fn test_validate_collateral_commitment_boundary_exactly_at_minimum() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let asset = Symbol::new(&env, "USDC");

    // Exactly at minimum (1) must succeed
    let commitment = client.record_sme_collateral_commitment(&asset, &1i128);
    assert_eq!(commitment.amount, 1i128);
}

#[test]
fn test_validate_collateral_commitment_boundary_just_below_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_collateral_limit(&1_000i128);

    let asset = Symbol::new(&env, "USDC");

    // Just below limit must succeed
    let just_below_limit = 999i128;
    let commitment = client.record_sme_collateral_commitment(&asset, &just_below_limit);
    assert_eq!(commitment.amount, just_below_limit);
}

#[test]
fn test_validate_collateral_commitment_boundary_exactly_at_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_collateral_limit(&1_000i128);

    let asset = Symbol::new(&env, "USDC");

    // Exactly at limit must succeed
    let commitment = client.record_sme_collateral_commitment(&asset, &1_000i128);
    assert_eq!(commitment.amount, 1_000i128);
}

#[test]
fn test_validate_collateral_commitment_boundary_just_above_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_collateral_limit(&1_000i128);

    let asset = Symbol::new(&env, "USDC");

    // Just above limit must fail
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &1_001i128),
        EscrowError::CollateralLimitExceeded,
    );
}

// ============================================================================
// SUITE 6: Identical Behavior — Helper Produces Same Results as Original
// ============================================================================

#[test]
fn test_collateral_commitment_helper_rejects_same_errors_as_original() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let asset = Symbol::new(&env, "USDC");

    // Error 1: CollateralAmountNotPositive when amount <= 0
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &0i128),
        EscrowError::CollateralAmountNotPositive,
    );

    // Error 2: CollateralAssetEmpty when asset is empty
    let empty = Symbol::new(&env, "");
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&empty, &5_000i128),
        EscrowError::CollateralAssetEmpty,
    );

    // Error 3: CollateralLimitExceeded when amount > limit
    client.set_collateral_limit(&1_000i128);
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &1_001i128),
        EscrowError::CollateralLimitExceeded,
    );
}

#[test]
fn test_collateral_limit_helper_rejects_same_errors_as_original() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Error 1: CollateralLimitNotPositive when limit <= 0
    assert_contract_error(
        client.try_set_collateral_limit(&0i128),
        EscrowError::CollateralLimitNotPositive,
    );

    // Error 2: CollateralLimitExceedsMax when limit > MAX_INVOICE_AMOUNT
    assert_contract_error(
        client.try_set_collateral_limit(&(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceedsMax,
    );
}

#[test]
fn test_collateral_commitment_error_codes_unchanged() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let asset = Symbol::new(&env, "USDC");

    // Verify exact error code: 60 for CollateralAmountNotPositive
    let result = client.try_record_sme_collateral_commitment(&asset, &0i128);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), EscrowError::CollateralAmountNotPositive);

    // Verify exact error code: 61 for CollateralAssetEmpty
    let empty = Symbol::new(&env, "");
    let result = client.try_record_sme_collateral_commitment(&empty, &5_000i128);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), EscrowError::CollateralAssetEmpty);

    // Verify exact error code: 64 for CollateralLimitExceeded
    client.set_collateral_limit(&1_000i128);
    let result = client.try_record_sme_collateral_commitment(&asset, &1_001i128);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), EscrowError::CollateralLimitExceeded);
}

#[test]
fn test_collateral_limit_error_codes_unchanged() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Verify exact error code: 63 for CollateralLimitNotPositive
    let result = client.try_set_collateral_limit(&0i128);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), EscrowError::CollateralLimitNotPositive);

    // Verify exact error code: 65 for CollateralLimitExceedsMax
    let result = client.try_set_collateral_limit(&(MAX_INVOICE_AMOUNT + 1));
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), EscrowError::CollateralLimitExceedsMax);
}

// ============================================================================
// SUITE 7: Integration — Call Sites Use Helper Correctly
// ============================================================================

#[test]
fn test_record_sme_collateral_commitment_uses_validation_helper_correctly() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Test that the function properly rejects invalid combinations through the helper

    let asset = Symbol::new(&env, "USDC");

    // Invalid: amount is 0
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &0i128),
        EscrowError::CollateralAmountNotPositive,
    );

    // Invalid: asset is empty
    let empty = Symbol::new(&env, "");
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&empty, &5_000i128),
        EscrowError::CollateralAssetEmpty,
    );

    // Valid: amount and asset both valid
    let commitment = client.record_sme_collateral_commitment(&asset, &5_000i128);
    assert_eq!(commitment.amount, 5_000i128);
    assert_eq!(commitment.asset, asset);
}

#[test]
fn test_set_collateral_limit_uses_validation_helper_correctly() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Invalid: limit is 0
    assert_contract_error(
        client.try_set_collateral_limit(&0i128),
        EscrowError::CollateralLimitNotPositive,
    );

    // Invalid: limit exceeds MAX_INVOICE_AMOUNT
    assert_contract_error(
        client.try_set_collateral_limit(&(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceedsMax,
    );

    // Valid: limit is positive and within bounds
    client.set_collateral_limit(&5_000i128);
    assert_eq!(client.get_collateral_limit(), 5_000i128);
}

#[test]
fn test_collateral_commitment_helper_prevents_invalid_state() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Set a restrictive limit
    client.set_collateral_limit(&1_000i128);

    let asset = Symbol::new(&env, "USDC");

    // Attempt to record above the limit fails
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &1_001i128),
        EscrowError::CollateralLimitExceeded,
    );

    // State is unchanged — commitment not recorded
    let config = client.get_collateral_config();
    assert_eq!(config.sme_commitment, crate::CollateralCommitmentSnapshot::None);

    // Now record a valid amount
    client.record_sme_collateral_commitment(&asset, &1_000i128);

    // State is now updated
    let config = client.get_collateral_config();
    assert!(matches!(
        config.sme_commitment,
        crate::CollateralCommitmentSnapshot::Some(_)
    ));
}

#[test]
fn test_multiple_collateral_operations_use_helpers_consistently() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let asset1 = Symbol::new(&env, "USDC");
    let asset2 = Symbol::new(&env, "ETH");

    // First operation: record at default limit
    client.record_sme_collateral_commitment(&asset1, &(MAX_INVOICE_AMOUNT - 1));

    // Lower the limit
    client.set_collateral_limit(&10_000i128);

    // Now attempts to record above the new limit fail
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset2, &10_001i128),
        EscrowError::CollateralLimitExceeded,
    );

    // But recording within the new limit still succeeds
    client.record_sme_collateral_commitment(&asset2, &10_000i128);

    // All helpers used consistently across the operations
}
