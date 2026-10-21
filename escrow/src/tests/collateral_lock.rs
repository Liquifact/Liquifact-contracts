//! Tests for issue #1563: Lock the SME collateral commitment once the escrow is funded.
//!
//! Coverage:
//! - `clear_sme_collateral_commitment` blocked after funding (non-terminal status) → `CollateralLockedAfterFunding`
//! - `clear_sme_collateral_commitment` succeeds in terminal status (withdrawn = 3)
//! - `record_sme_collateral_commitment` rejects asset change after funding → `CollateralAssetChangeAfterFunding`
//! - `record_sme_collateral_commitment` rejects amount decrease after funding → `CollateralDecreaseAfterFunding`
//! - `record_sme_collateral_commitment` allows amount increase after funding
//! - `batch_record_collateral` rejects mixed-asset batch → `CollateralBatchMixedAssets`
//! - `batch_record_collateral` rejects asset change after funding
//! - `batch_record_collateral` rejects amount decrease after funding
//! - `batch_record_collateral` allows amount increase after funding
//! - Legal hold blocks `record`, `clear`, and `batch` → `LegalHoldBlocksCollateralChange`
//! - Pre-funding behaviour is unchanged (regression)
//! - Same-asset, same-amount re-record after funding succeeds (no-op-equivalent)

use super::{assert_contract_error, init_and_fund_with_real_token, setup};
use crate::{EscrowError, LiquifactEscrowClient};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    Address, Env, Vec as SorobanVec,
};

const PLEDGE: i128 = 5_000;
const TARGET: i128 = 10_000;

// ─────────────────────────────────────────────────────────────────────────
// Helper: init an escrow and fund it to the target, returning the client.
// ─────────────────────────────────────────────────────────────────────────
fn funded_escrow(env: &Env) -> LiquifactEscrowClient<'_> {
    let (client, _escrow_id, _sme) = init_and_fund_with_real_token(env, TARGET, "COLLK001");
    client
}

/// Helper: init an escrow (status 0, no funding) with a dummy token.
fn open_escrow(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    let (client, admin, sme) = setup(env);
    let (token, treasury) = (Address::generate(env), Address::generate(env));
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "COLLK_OPEN"),
        &sme,
        &TARGET,
        &100i64,
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

// ─────────────────────────────────────────────────────────────────────────
// Clear after funding — blocked in non-terminal status
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_clear_blocked_after_funding_non_terminal() {
    let env = Env::default();
    env.mock_all_auths();
    let client = funded_escrow(&env);

    // Record a pledge before funding (funded_amount > 0 now).
    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Escrow is funded (status 1, funded_amount > 0) — clear must fail.
    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::CollateralLockedAfterFunding,
    );

    // Pledge must still be present.
    assert!(client.get_sme_collateral_commitment().is_some());
}

#[test]
fn test_clear_succeeds_in_terminal_status_withdrawn() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _id, _sme) = init_and_fund_with_real_token(&env, TARGET, "COLLK002");

    // Record pledge.
    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Withdraw (status 1 → 3 = withdrawn, terminal).
    client.withdraw();
    assert_eq!(client.get_escrow().status, 3);

    // Clear must succeed in terminal state.
    client.clear_sme_collateral_commitment();
    assert!(client.get_sme_collateral_commitment().is_none());
}

#[test]
fn test_clear_succeeds_in_terminal_status_cancelled() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _sme) = setup(&env);
    let (token, treasury) = (Address::generate(&env), Address::generate(&env));
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "COLLK003"),
        &Address::generate(&env),
        &TARGET,
        &100i64,
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

    // Record pledge.
    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Cancel (status 0 → 4 = cancelled, terminal).
    client.cancel_funding();
    assert_eq!(client.get_escrow().status, 4);

    // Clear must succeed in terminal state.
    client.clear_sme_collateral_commitment();
    assert!(client.get_sme_collateral_commitment().is_none());
}

#[test]
fn test_clear_succeeds_when_not_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = open_escrow(&env);

    // Record pledge (funded_amount == 0).
    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Clear must succeed — no funding lock.
    client.clear_sme_collateral_commitment();
    assert!(client.get_sme_collateral_commitment().is_none());
}

// ─────────────────────────────────────────────────────────────────────────
// Record after funding — asset change rejected
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_record_asset_change_rejected_after_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let client = funded_escrow(&env);

    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Try to change asset after funding.
    let new_asset = symbol_short!("GOLD");
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&new_asset, &PLEDGE),
        EscrowError::CollateralAssetChangeAfterFunding,
    );

    // Original pledge must be unchanged.
    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.asset, asset);
    assert_eq!(stored.amount, PLEDGE);
}

// ─────────────────────────────────────────────────────────────────────────
// Record after funding — amount decrease rejected
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_record_amount_decrease_rejected_after_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let client = funded_escrow(&env);

    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Try to decrease amount after funding.
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &(PLEDGE - 1)),
        EscrowError::CollateralDecreaseAfterFunding,
    );

    // Original pledge must be unchanged.
    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, PLEDGE);
}

// ─────────────────────────────────────────────────────────────────────────
// Record after funding — amount increase succeeds
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_record_amount_increase_succeeds_after_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let client = funded_escrow(&env);

    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Increase amount after funding — must succeed.
    let increased = PLEDGE + 1_000;
    let commitment = client.record_sme_collateral_commitment(&asset, &increased);
    assert_eq!(commitment.amount, increased);
    assert_eq!(commitment.asset, asset);

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, increased);
}

#[test]
fn test_record_same_amount_same_asset_succeeds_after_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let client = funded_escrow(&env);

    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Re-recording the same amount and asset after funding is allowed (≥, not >).
    let commitment = client.record_sme_collateral_commitment(&asset, &PLEDGE);
    assert_eq!(commitment.amount, PLEDGE);
    assert_eq!(commitment.asset, asset);
}

// ─────────────────────────────────────────────────────────────────────────
// Batch — mixed assets rejected
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_batch_mixed_assets_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = open_escrow(&env);

    let mut items = SorobanVec::new(&env);
    items.push_back((symbol_short!("USDC"), PLEDGE));
    items.push_back((symbol_short!("GOLD"), PLEDGE + 1_000));

    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralBatchMixedAssets,
    );

    // Nothing was stored.
    assert!(client.get_sme_collateral_commitment().is_none());
}

#[test]
fn test_batch_same_asset_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = open_escrow(&env);

    let asset = symbol_short!("USDC");
    let mut items = SorobanVec::new(&env);
    items.push_back((asset.clone(), PLEDGE));
    items.push_back((asset.clone(), PLEDGE + 1_000));

    let commitment = client.batch_record_collateral(&items);
    assert_eq!(commitment.asset, asset);
    assert_eq!(commitment.amount, PLEDGE + 1_000);
}

// ─────────────────────────────────────────────────────────────────────────
// Batch after funding — asset change rejected
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_batch_asset_change_rejected_after_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let client = funded_escrow(&env);

    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Batch with a different asset.
    let new_asset = symbol_short!("GOLD");
    let mut items = SorobanVec::new(&env);
    items.push_back((new_asset, PLEDGE));

    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralAssetChangeAfterFunding,
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Batch after funding — amount decrease rejected
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_batch_amount_decrease_rejected_after_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let client = funded_escrow(&env);

    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Batch with same asset but lower last amount.
    let mut items = SorobanVec::new(&env);
    items.push_back((asset, PLEDGE - 1));

    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralDecreaseAfterFunding,
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Batch after funding — amount increase succeeds
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_batch_amount_increase_succeeds_after_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let client = funded_escrow(&env);

    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Batch with same asset and higher final amount.
    let increased = PLEDGE + 2_000;
    let mut items = SorobanVec::new(&env);
    items.push_back((asset.clone(), PLEDGE + 500));
    items.push_back((asset.clone(), increased));

    let commitment = client.batch_record_collateral(&items);
    assert_eq!(commitment.asset, asset);
    assert_eq!(commitment.amount, increased);
}

// ─────────────────────────────────────────────────────────────────────────
// Legal hold blocks record, clear, and batch
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_legal_hold_blocks_record() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _sme) = open_escrow(&env);

    client.set_legal_hold(&true);

    let asset = symbol_short!("USDC");
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &PLEDGE),
        EscrowError::LegalHoldBlocksCollateralChange,
    );

    // Nothing stored.
    assert!(client.get_sme_collateral_commitment().is_none());
}

#[test]
fn test_legal_hold_blocks_clear() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = open_escrow(&env);

    // Record before hold.
    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Activate hold.
    client.set_legal_hold(&true);

    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::LegalHoldBlocksCollateralChange,
    );

    // Pledge must still be present.
    assert!(client.get_sme_collateral_commitment().is_some());
}

#[test]
fn test_legal_hold_blocks_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = open_escrow(&env);

    client.set_legal_hold(&true);

    let asset = symbol_short!("USDC");
    let mut items = SorobanVec::new(&env);
    items.push_back((asset, PLEDGE));

    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::LegalHoldBlocksCollateralChange,
    );

    // Nothing stored.
    assert!(client.get_sme_collateral_commitment().is_none());
}

#[test]
fn test_legal_hold_blocks_record_after_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let client = funded_escrow(&env);

    // Record before hold.
    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Activate hold and try to increase.
    client.set_legal_hold(&true);

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &(PLEDGE + 1_000)),
        EscrowError::LegalHoldBlocksCollateralChange,
    );

    // Amount unchanged.
    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, PLEDGE);
}

// ─────────────────────────────────────────────────────────────────────────
// Pre-funding regression: behaviour matches main (no lock)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_prefunding_clear_allowed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = open_escrow(&env);

    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Before funding, clear is allowed.
    client.clear_sme_collateral_commitment();
    assert!(client.get_sme_collateral_commitment().is_none());
}

#[test]
fn test_prefunding_asset_change_allowed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = open_escrow(&env);

    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Before funding, changing the asset is allowed (regression).
    let new_asset = symbol_short!("GOLD");
    let commitment = client.record_sme_collateral_commitment(&new_asset, &PLEDGE);
    assert_eq!(commitment.asset, new_asset);
}

#[test]
fn test_prefunding_amount_decrease_allowed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = open_escrow(&env);

    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Before funding, decreasing the amount is allowed (regression).
    let decreased = PLEDGE - 1_000;
    let commitment = client.record_sme_collateral_commitment(&asset, &decreased);
    assert_eq!(commitment.amount, decreased);
}

#[test]
fn test_prefunding_batch_mixed_assets_rejected() {
    // The mixed-assets guard applies even before funding (it's a batch
    // invariant, not a funding-lock check).
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = open_escrow(&env);

    let mut items = SorobanVec::new(&env);
    items.push_back((symbol_short!("USDC"), PLEDGE));
    items.push_back((symbol_short!("GOLD"), PLEDGE + 500));

    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralBatchMixedAssets,
    );
}

#[test]
fn test_prefunding_batch_asset_change_allowed() {
    // Before funding, a batch with a different asset than the existing pledge
    // is allowed (the post-funding asset-lock does not apply).
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = open_escrow(&env);

    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Batch with a different asset (pre-funding, no lock).
    let new_asset = symbol_short!("GOLD");
    let mut items = SorobanVec::new(&env);
    items.push_back((new_asset.clone(), PLEDGE));

    let commitment = client.batch_record_collateral(&items);
    assert_eq!(commitment.asset, new_asset);
}

// ─────────────────────────────────────────────────────────────────────────
// Batch mixed-assets guard fires before auth (all-or-nothing, no storage write)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_batch_mixed_assets_rejected_with_existing_pledge() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = open_escrow(&env);

    // Record an initial pledge.
    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Batch with mixed assets — must be rejected before any storage write.
    let mut items = SorobanVec::new(&env);
    items.push_back((symbol_short!("USDC"), PLEDGE + 100));
    items.push_back((symbol_short!("GOLD"), PLEDGE + 200));

    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralBatchMixedAssets,
    );

    // Original pledge must remain unchanged.
    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.asset, asset);
    assert_eq!(stored.amount, PLEDGE);
}

// ─────────────────────────────────────────────────────────────────────────
// Clear after funding succeeds in settled state (status 2)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_clear_succeeds_in_terminal_status_settled() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _id, _sme) = init_and_fund_with_real_token(&env, TARGET, "COLLK_SET");

    // Record pledge.
    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &PLEDGE);

    // Settle (status 1 → 2 = settled, terminal).
    client.settle();
    assert_eq!(client.get_escrow().status, 2);

    // Clear must succeed in terminal state.
    client.clear_sme_collateral_commitment();
    assert!(client.get_sme_collateral_commitment().is_none());
}
