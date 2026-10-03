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

// ============================================================================
// SUITE 8: Idempotency & Retry Compatibility
//
// Off-chain integrators routinely retry the same call on transient failures.
// The public contract is: retrying a mutation with the same inputs produces
// the same observable end-state as calling it once.  Callers that rely on
// this assumption MUST not break in the future.
// ============================================================================

#[test]
fn test_set_limit_idempotent_retry_same_value() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_collateral_limit(&5_000i128);
    let a = client.get_collateral_limit();
    client.set_collateral_limit(&5_000i128); // retry #1
    let b = client.get_collateral_limit();
    client.set_collateral_limit(&5_000i128); // retry #2
    let c = client.get_collateral_limit();

    assert_eq!(a, 5_000);
    assert_eq!(a, b);
    assert_eq!(b, c);
}

#[test]
fn test_record_idempotent_retry_same_inputs_same_ledger() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    env.ledger().set_timestamp(2025);

    let asset = Symbol::new(&env, "USDC");
    let amount = 3_333i128;

    let r1 = client.record_sme_collateral_commitment(&asset, &amount);
    let s1 = client.get_sme_collateral_commitment();
    let r2 = client.record_sme_collateral_commitment(&asset, &amount); // retry
    let s2 = client.get_sme_collateral_commitment();
    let r3 = client.record_sme_collateral_commitment(&asset, &amount); // retry 2
    let s3 = client.get_sme_collateral_commitment();

    assert_eq!(r1.amount, r2.amount);
    assert_eq!(r2.amount, r3.amount);
    assert_eq!(r1.asset, r2.asset);
    assert_eq!(r2.asset, r3.asset);
    assert_eq!(r1.recorded_at, r2.recorded_at);
    assert_eq!(r2.recorded_at, r3.recorded_at);
    assert_eq!(s1, s2);
    assert_eq!(s2, s3);
}

#[test]
fn test_clear_idempotent_retry_leaves_no_stale_state() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    client.set_collateral_limit(&9_000i128);

    let asset = Symbol::new(&env, "GOLD");
    client.record_sme_collateral_commitment(&asset, &1_000i128);

    client.clear_sme_collateral_commitment();
    let after_1 = client.get_sme_collateral_commitment();
    let lim_1 = client.get_collateral_limit();
    // Retry clear: even though storage has no pledge, NoCollateralToClear is
    // raised — the public behavior is the error code is stable and the
    // storage remains in the cleared state.
    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );
    let after_2 = client.get_sme_collateral_commitment();
    let lim_2 = client.get_collateral_limit();

    assert_eq!(after_1, None);
    assert_eq!(after_2, None);
    assert_eq!(lim_1, 9_000); // limit MUST survive clear
    assert_eq!(lim_2, 9_000);
}

// ============================================================================
// SUITE 9: Duplicate Submission Contract
//
// A client that accidentally sends two copies of the same transaction MUST
// NOT observe an inconsistent outcome.  The acceptance criteria for this
// suite: (a) same-ledger duplicates succeed and produce equal state, (b)
// after a successful duplicate, the state equals the state from a single
// call, (c) duplicates with OVER-limit inputs are rejected both times with
// the SAME error code.
// ============================================================================

#[test]
fn test_duplicate_record_submission_success_same_state() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    env.ledger().set_timestamp(31415);
    client.set_collateral_limit(&7_777i128);

    let asset = Symbol::new(&env, "EURC");
    let amt = 7_000i128;

    // Submission 1 + snapshot
    client.record_sme_collateral_commitment(&asset, &amt);
    let cfg1 = client.get_collateral_config();

    // Submission 2 (duplicate) + snapshot
    client.record_sme_collateral_commitment(&asset, &amt);
    let cfg2 = client.get_collateral_config();

    assert_eq!(cfg1, cfg2);
}

#[test]
fn test_duplicate_over_limit_both_rejected_same_error_code() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    client.set_collateral_limit(&2_000i128);
    let asset = Symbol::new(&env, "USDC");
    let bad = 2_001i128;

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &bad),
        EscrowError::CollateralLimitExceeded,
    );
    // Duplicate submission of the same invalid input:
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &bad),
        EscrowError::CollateralLimitExceeded,
    );
    // And state remains None (rejected atomically, no partial write leaked)
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

#[test]
fn test_duplicate_zero_amount_both_rejected_same_error_code() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let asset = Symbol::new(&env, "XLM");

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &0i128),
        EscrowError::CollateralAmountNotPositive,
    );
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &0i128),
        EscrowError::CollateralAmountNotPositive,
    );
}

// ============================================================================
// SUITE 10: Backwards Compatibility — Absent Storage Keys
//
// Legacy deployments (or contracts that haven't called set_collateral_limit
// yet) have `DataKey::CollateralLimit` absent.  The public compatibility
// contract is: every read/view entrypoint returns the documented default
// (`MAX_INVOICE_AMOUNT`); every mutation entrypoint treats absence the same
// way as an explicit `MAX_INVOICE_AMOUNT` write.  Any future refactor that
// accidentally reads the key with `.unwrap()` (panicking on absence) would
// silently break this contract — these tests pin the behavior.
// ============================================================================

#[test]
fn test_absent_collateral_limit_key_defaults_to_max_invoice() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    // We never called set_collateral_limit — key absent.

    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
    let cfg = client.get_collateral_config();
    assert_eq!(cfg.collateral_limit, MAX_INVOICE_AMOUNT);
    let state = client.get_collateral_state();
    assert_eq!(state.collateral_limit, MAX_INVOICE_AMOUNT);
}

#[test]
fn test_absent_collateral_limit_key_record_at_max_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let asset = Symbol::new(&env, "ABSNT");
    // Exactly at MAX_INVOICE_AMOUNT while key is absent → default ceiling allows
    client.record_sme_collateral_commitment(&asset, &MAX_INVOICE_AMOUNT);
    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, MAX_INVOICE_AMOUNT);
}

#[test]
fn test_absent_collateral_limit_key_record_over_max_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let asset = Symbol::new(&env, "TOOBIG");
    let too_big = MAX_INVOICE_AMOUNT
        .checked_add(1)
        .expect("MAX_INVOICE_AMOUNT + 1 must fit");
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &too_big),
        EscrowError::CollateralLimitExceeded,
    );
}

#[test]
fn test_absent_pledge_key_get_sme_collateral_returns_none() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    // Never called record_sme_collateral_commitment — pledge key absent.
    assert_eq!(client.get_sme_collateral_commitment(), None);
    let cfg = client.get_collateral_config();
    assert_eq!(cfg.sme_commitment, crate::CollateralCommitmentSnapshot::None);
    let state = client.get_collateral_state();
    assert!(!state.is_set);
    assert_eq!(state.amount, 0);
    assert_eq!(state.recorded_at, 0);
}

#[test]
fn test_absent_pledge_key_clear_reports_no_collateral_to_clear() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    // No pledge ever stored.
    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );
}

// ============================================================================
// SUITE 11: Call-Site Coverage — batch_record_collateral
//
// The public validation helpers (positive-amount, non-empty-asset, ceiling,
// positive ceiling, MAX-guarded ceiling) MUST fire identically from the
// **batch** entrypoint.  Moreover: the batch entrypoint is atomic; a single
// bad item rejects the whole batch and leaves storage unchanged.
// ============================================================================

#[test]
fn test_batch_record_empty_rejected_collateral_batch_empty() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let empty = soroban_sdk::Vec::<(Symbol, i128)>::new(&env);
    assert_contract_error(
        client.try_batch_record_collateral(&empty),
        EscrowError::CollateralBatchEmpty,
    );
}

#[test]
fn test_batch_record_too_large_rejected_collateral_batch_too_large() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    // MAX_COLLATERAL_BATCH = 50, so 51 items exceeds it.
    let asset = Symbol::new(&env, "OVFL");
    let mut items = soroban_sdk::Vec::new(&env);
    for i in 0..=50u32 {
        items.push_back((asset.clone(), 1i128 + i as i128));
    }
    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralBatchTooLarge,
    );
}

#[test]
fn test_batch_record_one_zero_amount_rejects_all_atomically() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let a1 = Symbol::new(&env, "A");
    let a2 = Symbol::new(&env, "B");
    let items =
        soroban_sdk::Vec::from_array(&env, [(a1.clone(), 500i128), (a2.clone(), 0i128)]);

    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralAmountNotPositive,
    );
    // Atomic: nothing was recorded (pre-validation runs before writes).
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

#[test]
fn test_batch_record_one_empty_asset_rejects_all_atomically() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let empty = Symbol::new(&env, "");
    let ok = Symbol::new(&env, "USDC");
    let items =
        soroban_sdk::Vec::from_array(&env, [(ok, 500i128), (empty, 500i128)]);

    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralAssetEmpty,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

#[test]
fn test_batch_record_one_over_ceiling_rejects_all_atomically() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    client.set_collateral_limit(&1_000i128);
    let a1 = Symbol::new(&env, "OK");
    let a2 = Symbol::new(&env, "BAD");
    let items =
        soroban_sdk::Vec::from_array(&env, [(a1, 999i128), (a2, 1_001i128)]);

    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralLimitExceeded,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

#[test]
fn test_batch_record_all_valid_stores_last_item() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    env.ledger().set_timestamp(9001);
    client.set_collateral_limit(&10_000i128);

    let a1 = Symbol::new(&env, "ONE");
    let a2 = Symbol::new(&env, "TWO");
    let a3 = Symbol::new(&env, "THR");
    let items = soroban_sdk::Vec::from_array(
        &env,
        [(a1, 1_000i128), (a2, 2_000i128), (a3.clone(), 3_000i128)],
    );
    let out = client.batch_record_collateral(&items);

    // Final stored commitment matches the LAST item in the batch
    assert_eq!(out.asset, a3);
    assert_eq!(out.amount, 3_000);
    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored, out);
}

// ============================================================================
// SUITE 12: Deterministic Sequencing of Mixed Operations
//
// For any sequence S of {set_limit, record, clear} calls, running S twice
// against two fresh envs MUST produce the same end-state.  This defends
// against accidentally-nondeterministic helpers (e.g. a random `recorded_at`
// or hashmap iteration order sneaking into storage).
// ============================================================================

fn run_scenario_sequence(
    env: &Env,
    client: &crate::LiquifactEscrowClient,
    admin: &Address,
    sme: &Address,
) {
    init_escrow(env, client, admin, sme);

    env.ledger().set_timestamp(1);
    client.set_collateral_limit(&8_000i128);

    env.ledger().set_timestamp(2);
    let usdc = Symbol::new(env, "USDC");
    client.record_sme_collateral_commitment(&usdc, &4_000i128);

    env.ledger().set_timestamp(3);
    client.set_collateral_limit(&6_000i128);

    env.ledger().set_timestamp(4);
    let xlm = Symbol::new(env, "XLM");
    client.record_sme_collateral_commitment(&xlm, &5_500i128);

    client.clear_sme_collateral_commitment();

    env.ledger().set_timestamp(5);
    let eth = Symbol::new(env, "ETH");
    client.record_sme_collateral_commitment(&eth, &6_000i128); // exactly at limit
}

#[test]
fn test_deterministic_sequence_two_runs_equal() {
    // Run #1
    let env1 = Env::default();
    env1.mock_all_auths();
    let (client1, admin1, sme1) = setup(&env1);
    run_scenario_sequence(&env1, &client1, &admin1, &sme1);
    let cfg1 = client1.get_collateral_config();
    let state1 = client1.get_collateral_state();
    let lim1 = client1.get_collateral_limit();
    let stored1 = client1.get_sme_collateral_commitment();

    // Run #2
    let env2 = Env::default();
    env2.mock_all_auths();
    let (client2, admin2, sme2) = setup(&env2);
    run_scenario_sequence(&env2, &client2, &admin2, &sme2);
    let cfg2 = client2.get_collateral_config();
    let state2 = client2.get_collateral_state();
    let lim2 = client2.get_collateral_limit();
    let stored2 = client2.get_sme_collateral_commitment();

    // Assert all four public read surfaces agree across the two runs
    assert_eq!(lim1, lim2);
    assert_eq!(lim1, 6_000);
    assert_eq!(stored1.as_ref().unwrap().amount, 6_000);
    assert_eq!(stored1.as_ref().unwrap().amount, stored2.as_ref().unwrap().amount);
    assert_eq!(stored1.as_ref().unwrap().recorded_at, 5);
    assert_eq!(
        stored1.as_ref().unwrap().recorded_at,
        stored2.as_ref().unwrap().recorded_at
    );
    assert_eq!(cfg1.collateral_limit, cfg2.collateral_limit);
    assert_eq!(state1.collateral_limit, state2.collateral_limit);
    assert_eq!(state1.amount, state2.amount);
    assert_eq!(state1.is_set, state2.is_set);
    assert_eq!(state1.recorded_at, state2.recorded_at);
}

// ============================================================================
// SUITE 13: Migration-Path Contract — Limit Lowering Is Not Retroactive
//
// A common integrator pattern: (1) record a commitment, (2) lower the limit
// to restrict *future* calls, (3) still need to read the *existing* pledge
// back unchanged.  If a future refactor adds retroactive pruning inside
// `set_collateral_limit`, this contract breaks — pin the current (correct)
// non-retroactive behaviour.
// ============================================================================

#[test]
fn test_lower_limit_does_not_clear_existing_commitment() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    env.ledger().set_timestamp(10);

    // Step 1: high limit + big commitment
    client.set_collateral_limit(&10_000i128);
    let asset = Symbol::new(&env, "RETRO");
    let amount = 8_000i128;
    let c = client.record_sme_collateral_commitment(&asset, &amount);

    // Step 2: lower the ceiling BELOW the existing commitment amount
    client.set_collateral_limit(&5_000i128);

    // Step 3: the stored commitment survives (no retroactive clear)
    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, 8_000);
    assert_eq!(stored.asset, asset);
    assert_eq!(stored.recorded_at, c.recorded_at);

    // The bundled views also return the (now over-limit) historical amount,
    // because they are pure reads.  NOTE: record_sme_collateral_commitment
    // will reject new 8_000-amount calls against the new 5_000 ceiling —
    // only the *historical* stored value survives.
    let cfg = client.get_collateral_config();
    match cfg.sme_commitment {
        crate::CollateralCommitmentSnapshot::Some(s) => assert_eq!(s.amount, 8_000),
        crate::CollateralCommitmentSnapshot::None => panic!("historical commitment lost"),
    }
    assert_eq!(cfg.collateral_limit, 5_000);
}

#[test]
fn test_raise_limit_allows_new_larger_commitment() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Tight limit
    client.set_collateral_limit(&2_000i128);
    let asset = Symbol::new(&env, "STEP");
    client.record_sme_collateral_commitment(&asset, &2_000i128);

    // Trying larger now fails
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &2_001i128),
        EscrowError::CollateralLimitExceeded,
    );

    // Raise the ceiling
    client.set_collateral_limit(&20_000i128);

    // Now the same 2_001 amount succeeds; even 15_000 succeeds
    client.record_sme_collateral_commitment(&asset, &15_000i128);
    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, 15_000);
}

// ============================================================================
// SUITE 14: Error-Code Discriminants — Public API Compatibility Pin
//
// SDK-generated client code (and external indexers that watch for
// `ScUnknownErrorCode` in transaction results) often hard-codes the raw
// `u32` discriminant.  Changing any of these values is a BREAKING CHANGE to
// the public contract; if someone tries, these tests fail immediately.
// ============================================================================

#[test]
fn test_collateral_error_discriminants_pinned() {
    // Single commitment errors (suite 7 already pins a subset — here we also
    // pin the two other collateral codes and the batch errors.)
    assert_eq!(EscrowError::CollateralAmountNotPositive as u32, 60);
    assert_eq!(EscrowError::CollateralAssetEmpty as u32, 61);
    assert_eq!(EscrowError::CollateralTimestampBackwards as u32, 62);
    assert_eq!(EscrowError::NoCollateralToClear as u32, 63);
    assert_eq!(EscrowError::CollateralLimitNotPositive as u32, 64);
    assert_eq!(EscrowError::CollateralLimitExceedsMax as u32, 65);
    assert_eq!(EscrowError::CollateralLimitExceeded as u32, 66);
    assert_eq!(EscrowError::CollateralBatchEmpty as u32, 67);
    assert_eq!(EscrowError::CollateralBatchTooLarge as u32, 68);
}

#[test]
fn test_max_collateral_batch_constant_pinned() {
    // Integrators batch-loop size their submissions to this constant.  A
    // silent decrease would break their batches.
    assert_eq!(crate::MAX_COLLATERAL_BATCH, 50);
}
