//! Funding-envelope bounds tests (issue #1535).
//!
//! Verifies the two new guards that keep every value feeding the payout math
//! inside the documented envelope:
//!   - `update_funding_target` rejects `new_target > escrow.amount`
//!     (`TargetExceedsInvoiceAmount` = 250).
//!   - `fund_impl` rejects `funded_amount > MAX_INVOICE_AMOUNT`
//!     (`FundedAmountExceedsSafeEnvelope` = 251).
//!
//! Together with the existing `init` cap (`amount ≤ MAX_INVOICE_AMOUNT`), this
//! establishes the chain `target ≤ amount ≤ MAX_INVOICE_AMOUNT` and
//! `funded_amount ≤ MAX_INVOICE_AMOUNT` so `compute_investor_payout` can never
//! trap with `ComputePayoutArithmeticOverflow` for any
//! `yield_bps ∈ [0, 10_000]`.

use super::*;
use crate::{InvoiceEscrow, MAX_INVOICE_AMOUNT};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

/// Lightweight init helper — no real token, suitable for tests that only
/// exercise guard rejection before any token transfer.
fn setup_no_token<'a>(env: &'a Env, invoice_id: &str, amount: i128) -> LiquifactEscrowClient<'a> {
    env.mock_all_auths();
    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let (token, treasury) = free_addresses(env);
    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &amount,
        &0i64,
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
    client
}

// ===========================================================================
// Error 250 — TargetExceedsInvoiceAmount
// ===========================================================================

/// `update_funding_target(amount + 1)` fails on the branch (acceptance criterion).
#[test]
fn update_funding_target_above_invoice_amount_rejected() {
    let env = Env::default();
    let amount = 100_000i128;
    let client = setup_no_token(&env, "ENV250A", amount);

    assert_contract_error(
        client.try_update_funding_target(&(amount + 1)),
        EscrowError::TargetExceedsInvoiceAmount,
    );
}

/// `update_funding_target(amount)` succeeds — boundary is inclusive.
#[test]
fn update_funding_target_equal_to_invoice_amount_succeeds() {
    let env = Env::default();
    let amount = 100_000i128;
    let client = setup_no_token(&env, "ENV250B", amount);

    let result = client.update_funding_target(&amount);
    assert_eq!(result.funding_target, amount);
}

/// `update_funding_target(amount / 2)` succeeds — lowering within bounds.
#[test]
fn update_funding_target_below_invoice_amount_succeeds() {
    let env = Env::default();
    let amount = 100_000i128;
    let client = setup_no_token(&env, "ENV250C", amount);

    let result = client.update_funding_target(&(amount / 2));
    assert_eq!(result.funding_target, amount / 2);
}

/// Lowering the target to exactly `funded_amount` still auto-promotes to
/// status 1 (existing path preserved after the new guard).
#[test]
fn update_funding_target_lower_to_funded_promotes_to_funded() {
    let env = Env::default();
    let amount = 100_000i128;
    let client = setup_no_token(&env, "ENV250D", amount);

    let investor = Address::generate(&env);
    client.fund(&investor, &(amount / 2));
    assert_eq!(client.get_escrow().status, 0);

    let result = client.update_funding_target(&(amount / 2));
    assert_eq!(result.status, 1);
    assert!(client.get_funding_close_snapshot().is_some());
}

// ===========================================================================
// Error 251 — FundedAmountExceedsSafeEnvelope
// ===========================================================================

/// Fund up to `MAX_INVOICE_AMOUNT` exactly (succeeds), then by one more unit
/// (fails with the new error). Balances are unchanged after the failure
/// (acceptance criterion).
///
/// We use a two-investor split: investor_a funds MAX_INVOICE_AMOUNT - 1 (stays
/// open), investor_b funds 1 (exact boundary, succeeds), investor_c tries to
/// fund 1 more but the escrow is now in status 1 so the guard is
/// `EscrowNotOpenForFunding`.  To exercise 251 directly, we need the final
/// `funded_amount` to exceed MAX_INVOICE_AMOUNT while status is still 0.
/// The test achieves this by setting `funding_target = MAX_INVOICE_AMOUNT`
/// and using two investors where total = MAX_INVOICE_AMOUNT (exactly at target)
/// then injecting an extra unit via storage to create a status-0 escrow with
/// funded_amount = MAX_INVOICE_AMOUNT, then funding 1 more.
#[test]
fn fund_at_max_invoice_amount_boundary_then_reject() {
    let env = Env::default();
    env.mock_all_auths();
    let sac = install_stellar_asset_token(&env);
    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    // Use target = MAX_INVOICE_AMOUNT - 1 so funding MAX_INVOICE_AMOUNT - 1 promotes
    // to status 1... that also won't work. Instead use a target far below MAX and
    // inject funded_amount directly.
    //
    // Strategy: init with amount = MAX_INVOICE_AMOUNT and funding_target = MAX_INVOICE_AMOUNT.
    // Use env.as_contract to set funded_amount = MAX_INVOICE_AMOUNT directly (bypassing
    // auto-promotion), keeping status = 0. Then fund 1 → FundedAmountExceedsSafeEnvelope.
    client.init(
        &admin,
        &String::from_str(&env, "ENV251A"),
        &sme,
        &MAX_INVOICE_AMOUNT,
        &0i64,
        &0u64,
        &sac.id,
        &None,
        &Address::generate(&env),
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

    // Inject funded_amount = MAX_INVOICE_AMOUNT while keeping status = 0 by
    // using a target above MAX_INVOICE_AMOUNT... blocked by 250.
    // Use env.as_contract to directly mutate the stored escrow.
    env.as_contract(&id, || {
        let mut escrow: InvoiceEscrow = env.storage().instance().get(&DataKey::Escrow).unwrap();
        escrow.funded_amount = MAX_INVOICE_AMOUNT;
        escrow.status = 0; // keep open
        env.storage().instance().set(&DataKey::Escrow, &escrow);
    });

    assert_eq!(client.get_escrow().funded_amount, MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_escrow().status, 0);

    let investor_c = Address::generate(&env);
    sac.stellar.mint(&investor_c, &1i128);

    // funded_amount would become MAX_INVOICE_AMOUNT + 1 — must fail with 251.
    let funded_before = client.get_escrow().funded_amount;
    assert_contract_error(
        client.try_fund(&investor_c, &1i128),
        EscrowError::FundedAmountExceedsSafeEnvelope,
    );
    // State unchanged after failure.
    assert_eq!(client.get_escrow().funded_amount, funded_before);
    assert_eq!(client.get_contribution(&investor_c), 0);
}

/// `fund_with_commitment` shares `fund_impl` so it also rejects past the envelope.
#[test]
fn fund_with_commitment_at_envelope_boundary_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let sac = install_stellar_asset_token(&env);
    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "ENV251B"),
        &sme,
        &MAX_INVOICE_AMOUNT,
        &0i64,
        &0u64,
        &sac.id,
        &None,
        &Address::generate(&env),
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

    // Inject funded_amount = MAX_INVOICE_AMOUNT, status = 0.
    env.as_contract(&id, || {
        let mut escrow: InvoiceEscrow = env.storage().instance().get(&DataKey::Escrow).unwrap();
        escrow.funded_amount = MAX_INVOICE_AMOUNT;
        escrow.status = 0;
        env.storage().instance().set(&DataKey::Escrow, &escrow);
    });

    let investor_b = Address::generate(&env);
    sac.stellar.mint(&investor_b, &1i128);

    // fund_with_commitment for 1 more must fail with 251.
    assert_contract_error(
        client.try_fund_with_commitment(&investor_b, &1i128, &0u64),
        EscrowError::FundedAmountExceedsSafeEnvelope,
    );
}

/// At the envelope boundary with `yield_bps = 10_000` (most aggressive),
/// `compute_investor_payout` must succeed without `ComputePayoutArithmeticOverflow`.
/// Deterministic spot-check of the proptest property in `properties.rs`.
#[test]
fn compute_investor_payout_no_overflow_at_max_invoice_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let sac = install_stellar_asset_token(&env);
    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "ENV251C"),
        &sme,
        &MAX_INVOICE_AMOUNT,
        &10_000i64, // most aggressive yield
        &0u64,
        &sac.id,
        &None,
        &Address::generate(&env),
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

    let investor = Address::generate(&env);
    sac.stellar.mint(&investor, &MAX_INVOICE_AMOUNT);

    // Fund to exactly MAX_INVOICE_AMOUNT — must succeed (boundary inclusive).
    client.fund(&investor, &MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_escrow().funded_amount, MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_escrow().status, 1);

    // Settle so payouts are claimable.
    client.settle();
    assert_eq!(client.get_escrow().status, 2);

    // compute_investor_payout must not trap.
    let payout = client.compute_investor_payout(&investor);
    assert!(payout > 0, "payout must be positive at 10 000 bps");
}
