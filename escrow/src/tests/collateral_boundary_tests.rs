//! Validation-boundary tests for the SME collateral subsystem.
//!
//! Every mutating collateral entrypoint is pinned here across four input classes:
//! accepted values, rejected values, duplicate submissions, and the exact boundary
//! between them. The tests assert two invariants on every rejection:
//!
//! 1. the typed [`EscrowError`] code is the documented one (failures stay
//!    diagnosable for SDKs without string matching), and
//! 2. stored state is byte-for-byte what it was before the rejected call — a
//!    rejected write can never leave a partially applied pledge, ceiling, or
//!    batch behind.

use super::super::{
    CollateralCommitmentSnapshot, EscrowError, LiquifactEscrow, LiquifactEscrowClient,
    MAX_COLLATERAL_BATCH, MAX_INVOICE_AMOUNT,
};
use crate::tests::assert_contract_error;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, Symbol,
};

/// Registers a fresh escrow and initializes it with a `10_000` invoice amount so the
/// default ceiling (`MAX_INVOICE_AMOUNT`) and a known escrow state are both in play.
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

/// Registers an escrow **without** `init`, for the pre-initialization boundary cases.
fn setup_uninitialized(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

/// Builds the `(asset, amount)` vector consumed by `batch_record_collateral`.
fn batch_items(env: &Env, entries: &[(&str, i128)]) -> soroban_sdk::Vec<(Symbol, i128)> {
    let mut items = soroban_sdk::Vec::new(env);
    for (asset, amount) in entries {
        items.push_back((Symbol::new(env, asset), *amount));
    }
    items
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

// ── Duplicate submissions ─────────────────────────────────────────────────────
//
// The collateral pledge is a single-slot record, so a resubmitted duplicate must
// replace the slot — never append, never panic, never leave two records behind.

#[test]
fn test_duplicate_commitment_submission_replaces_single_pledge() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    let asset = Symbol::new(&env, "USDC");
    let first = client.record_sme_collateral_commitment(&asset, &1_000i128);
    assert_eq!(first.amount, 1_000i128);

    // Exact duplicate (same asset, same amount, same ledger timestamp) is accepted
    // as an idempotent replacement rather than rejected as "already recorded".
    let duplicate = client.record_sme_collateral_commitment(&asset, &1_000i128);
    assert_eq!(duplicate.amount, 1_000i128);

    // A single pledge slot remains — no append, no duplicate storage entry.
    let stored = client
        .get_sme_collateral_commitment()
        .expect("pledge exists");
    assert_eq!(stored.asset, asset);
    assert_eq!(stored.amount, 1_000i128);
    assert!(matches!(
        client.get_collateral_config().sme_commitment,
        CollateralCommitmentSnapshot::Some(_)
    ));

    // A different asset replaces the pledge outright.
    let other = Symbol::new(&env, "ETH");
    client.record_sme_collateral_commitment(&other, &2_000i128);
    let stored = client
        .get_sme_collateral_commitment()
        .expect("pledge exists");
    assert_eq!(stored.asset, other);
    assert_eq!(stored.amount, 2_000i128);
}

#[test]
fn test_duplicate_clear_is_rejected_and_state_stays_cleared() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    let asset = Symbol::new(&env, "USDC");
    client.record_sme_collateral_commitment(&asset, &1_000i128);

    client.clear_sme_collateral_commitment();
    assert_eq!(client.get_sme_collateral_commitment(), None);
    assert!(matches!(
        client.get_collateral_config().sme_commitment,
        CollateralCommitmentSnapshot::None
    ));

    // Second clear: there is nothing left to clear, and the error is the typed one.
    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);

    // Recovery path: recording again after a clear (and after a rejected clear) works.
    client.record_sme_collateral_commitment(&asset, &500i128);
    assert_eq!(
        client
            .get_sme_collateral_commitment()
            .expect("pledge exists")
            .amount,
        500i128
    );
}

#[test]
fn test_duplicate_collateral_limit_submission_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    client.set_collateral_limit(&5_000i128);
    // Re-submitting the identical limit is a no-op update, not an error.
    client.set_collateral_limit(&5_000i128);
    assert_eq!(client.get_collateral_limit(), 5_000i128);
    assert_eq!(client.get_collateral_config().collateral_limit, 5_000i128);

    // A rejected duplicate-then-invalid sequence leaves the last accepted value.
    assert_contract_error(
        client.try_set_collateral_limit(&0i128),
        EscrowError::CollateralLimitNotPositive,
    );
    assert_eq!(client.get_collateral_limit(), 5_000i128);
}

#[test]
fn test_batch_with_duplicate_asset_entries_keeps_last_entry() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // The same asset twice in one batch: allowed, last entry wins, still one pledge.
    let items = batch_items(&env, &[("USDC", 100), ("USDC", 250)]);
    let stored = client.batch_record_collateral(&items);
    assert_eq!(stored.asset, Symbol::new(&env, "USDC"));
    assert_eq!(stored.amount, 250i128);

    let stored = client
        .get_sme_collateral_commitment()
        .expect("pledge exists");
    assert_eq!(stored.amount, 250i128);
    assert!(matches!(
        client.get_collateral_config().sme_commitment,
        CollateralCommitmentSnapshot::Some(_)
    ));
}

// ── Batch size and atomicity boundaries ─────────────────────────────────────

#[test]
fn test_batch_at_max_size_is_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    let mut items = soroban_sdk::Vec::new(&env);
    for i in 0..MAX_COLLATERAL_BATCH {
        items.push_back((Symbol::new(&env, "USDC"), i as i128 + 1));
    }
    assert_eq!(items.len(), MAX_COLLATERAL_BATCH);

    let stored = client.batch_record_collateral(&items);
    assert_eq!(stored.amount, MAX_COLLATERAL_BATCH as i128);
}

#[test]
fn test_batch_over_max_size_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    let mut items = soroban_sdk::Vec::new(&env);
    for _ in 0..=MAX_COLLATERAL_BATCH {
        items.push_back((Symbol::new(&env, "USDC"), 1i128));
    }

    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralBatchTooLarge,
    );
    // Boundary rejection leaves no pledge behind.
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

#[test]
fn test_empty_batch_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    let items = batch_items(&env, &[]);
    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralBatchEmpty,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

#[test]
fn test_single_invalid_entry_rejects_entire_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Position 1 is invalid: the two valid neighbours must not be written.
    let items = batch_items(&env, &[("USDC", 100), ("ETH", 0), ("XLM", 300)]);
    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralAmountNotPositive,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);

    // Same guarantee for an empty asset symbol deep in the batch.
    let items = batch_items(&env, &[("USDC", 100), ("", 50)]);
    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralAssetEmpty,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
    assert!(matches!(
        client.get_collateral_config().sme_commitment,
        CollateralCommitmentSnapshot::None
    ));
}

#[test]
fn test_batch_cannot_bypass_configured_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    client.set_collateral_limit(&1_000i128);

    // One entry above the ceiling rejects the whole batch — a batch must never be
    // able to record what the single-entry path would have refused.
    let items = batch_items(&env, &[("USDC", 1_000), ("ETH", 1_001)]);
    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralLimitExceeded,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);

    // Boundary: an entry exactly at the ceiling is accepted.
    let items = batch_items(&env, &[("USDC", 1_000)]);
    let stored = client.batch_record_collateral(&items);
    assert_eq!(stored.amount, 1_000i128);
}

// ── Authorization boundaries ─────────────────────────────────────────────────

#[test]
fn test_record_requires_sme_authorization() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);
    env.mock_auths(&[]);

    let asset = Symbol::new(&env, "USDC");
    assert!(
        client
            .try_record_sme_collateral_commitment(&asset, &1_000i128)
            .is_err(),
        "unauthenticated record must fail"
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

#[test]
fn test_clear_requires_sme_authorization() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    let asset = Symbol::new(&env, "USDC");
    client.record_sme_collateral_commitment(&asset, &1_000i128);

    env.mock_auths(&[]);
    assert!(
        client.try_clear_sme_collateral_commitment().is_err(),
        "unauthenticated clear must fail"
    );
    // The pledge survives the unauthorized attempt.
    assert_eq!(
        client
            .get_sme_collateral_commitment()
            .expect("pledge survives")
            .amount,
        1_000i128
    );
}

#[test]
fn test_set_collateral_limit_requires_admin_authorization() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);
    env.mock_auths(&[]);

    assert!(client.try_set_collateral_limit(&1_000i128).is_err());
    assert_eq!(
        client.get_collateral_limit(),
        MAX_INVOICE_AMOUNT,
        "rejected setter must leave the ceiling untouched"
    );
}

// ── Numeric and temporal boundaries ───────────────────────────────────────────

#[test]
fn test_commitment_at_i128_min_is_rejected_without_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    let asset = Symbol::new(&env, "USDC");
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &i128::MIN),
        EscrowError::CollateralAmountNotPositive,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

#[test]
fn test_commitment_at_default_max_limit_is_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Upper boundary of the default ceiling: amount == MAX_INVOICE_AMOUNT passes.
    let asset = Symbol::new(&env, "USDC");
    let stored = client.record_sme_collateral_commitment(&asset, &MAX_INVOICE_AMOUNT);
    assert_eq!(stored.amount, MAX_INVOICE_AMOUNT);

    // One unit above it is out of range even before any admin override exists.
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceeded,
    );
    assert_eq!(
        client
            .get_sme_collateral_commitment()
            .expect("pledge unchanged")
            .amount,
        MAX_INVOICE_AMOUNT
    );
}

#[test]
fn test_lowering_limit_keeps_existing_commitment_but_gates_new_writes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    let asset = Symbol::new(&env, "USDC");
    client.record_sme_collateral_commitment(&asset, &5_000i128);

    client.set_collateral_limit(&1_000i128);

    // Historical metadata is never retro-invalidated by a config change.
    assert_eq!(
        client
            .get_sme_collateral_commitment()
            .expect("pledge survives")
            .amount,
        5_000i128
    );
    assert_eq!(client.get_collateral_config().collateral_limit, 1_000i128);

    // New writes are measured against the new ceiling.
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &1_001i128),
        EscrowError::CollateralLimitExceeded,
    );
    client.record_sme_collateral_commitment(&asset, &1_000i128);
    assert_eq!(
        client
            .get_sme_collateral_commitment()
            .expect("pledge updated")
            .amount,
        1_000i128
    );
}

#[test]
fn test_re_record_with_backward_timestamp_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    let asset = Symbol::new(&env, "GOLD");
    env.ledger().set_timestamp(5_000);
    client.record_sme_collateral_commitment(&asset, &100i128);

    // Rewinding the ledger must not let a resubmission "un-record" the pledge.
    env.ledger().set_timestamp(100);
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &200i128),
        EscrowError::CollateralTimestampBackwards,
    );

    let stored = client
        .get_sme_collateral_commitment()
        .expect("pledge unchanged");
    assert_eq!(stored.amount, 100i128);
    assert_eq!(stored.recorded_at, 5_000);
}

// ── Pre-initialization boundary ───────────────────────────────────────────────

#[test]
fn test_collateral_reads_return_documented_defaults_before_init() {
    let env = Env::default();
    let client = setup_uninitialized(&env);

    // Reads are ungated and default-safe, so dashboards never have to special-case
    // an uninitialized contract.
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
    assert_eq!(
        client.get_collateral_config().collateral_limit,
        MAX_INVOICE_AMOUNT
    );
    assert!(matches!(
        client.get_collateral_config().sme_commitment,
        CollateralCommitmentSnapshot::None
    ));
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

#[test]
fn test_collateral_mutations_before_init_are_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup_uninitialized(&env);

    assert_contract_error(
        client.try_set_collateral_limit(&1_000i128),
        EscrowError::EscrowNotInitialized,
    );
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&Symbol::new(&env, "USDC"), &1_000i128),
        EscrowError::EscrowNotInitialized,
    );
    // The clear path checks the pledge slot first: nothing exists, so it reports the
    // "nothing to clear" condition rather than an init failure.
    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );

    // No state was created by any of the rejected calls.
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

// ── Regression: typed error codes stay stable ─────────────────────────────────
//
// SDKs branch on `ContractError(code)`, so these numbers are append-only. A rename
// that silently renumbers one of them is a client-facing break; this test pins them.

#[test]
fn test_collateral_error_codes_are_stable() {
    assert_eq!(EscrowError::CollateralAmountNotPositive as u32, 60);
    assert_eq!(EscrowError::CollateralAssetEmpty as u32, 61);
    assert_eq!(EscrowError::CollateralTimestampBackwards as u32, 62);
    assert_eq!(EscrowError::CollateralLimitNotPositive as u32, 63);
    assert_eq!(EscrowError::CollateralLimitExceeded as u32, 64);
    assert_eq!(EscrowError::CollateralLimitExceedsMax as u32, 65);
    assert_eq!(EscrowError::CollateralBatchEmpty as u32, 66);
    assert_eq!(EscrowError::CollateralBatchTooLarge as u32, 67);
    assert_eq!(EscrowError::NoCollateralToClear as u32, 169);
}
