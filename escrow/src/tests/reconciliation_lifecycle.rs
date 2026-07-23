//! Lifecycle-spanning tests for [`get_reconciliation`] across every state the
//! escrow can occupy, asserting the liability invariant at each step.
//!
//! # Invariant
//! At every checkpoint the view's `outstanding_liability` must equal
//! `max(funded_amount - get_distributed_principal(), 0)` — the same formula
//! used by the [`sweep_terminal_dust`](crate::LiquifactEscrow::sweep_terminal_dust)
//! liability floor.
//!
//! # Coverage
//! | State | Description |
//! |-------|-------------|
//! | Fresh (status 0) | Initialized, never funded |
//! | Open / partial (status 0) | Funded below target |
//! | Funded (status 1) | Target reached, funding closed |
//! | Over-funded (status 1) | Funded above target |
//! | Settled (status 2) | No claims yet |
//! | Settled + partial claims (status 2) | One investor claimed |
//! | Settled + all claims (status 2) | All investors claimed, deficit |
//! | Withdrawn (status 3) | SME pulled liquidity |
//! | Cancelled (status 4) | No refunds yet |
//! | Cancelled + partial refund (status 4) | Surplus matches sweepable |
//! | Cancelled + full refund (status 4) | All principal returned |
//!
//! All arithmetic uses saturating ops so these tests never panic on the
//! invariant assertion; deficits are surfaced as negative `surplus`.

use super::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, String};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Assert the central liability invariant and check that `token_balance` matches
/// the live contract balance.
fn assert_invariant(client: &LiquifactEscrowClient<'_>, token: &StellarTestToken<'_>) {
    let view = client.get_reconciliation();
    let escrow = client.get_escrow();
    let distributed = client.get_distributed_principal();
    let expected_liability = escrow.funded_amount.saturating_sub(distributed).max(0);

    assert_eq!(
        view.outstanding_liability, expected_liability,
        "liability invariant: funded_amount={} - distributed_principal={} = expected {}",
        escrow.funded_amount, distributed, expected_liability,
    );
    assert_eq!(
        view.token_balance,
        token.token.balance(&client.address),
        "token_balance must match live contract balance",
    );
}

/// Deploy and initialise an escrow with a real Stellar Asset Contract token.
fn setup_escrow<'a>(
    env: &'a Env,
    target: i128,
    invoice_id: &str,
) -> (LiquifactEscrowClient<'a>, StellarTestToken<'a>, Address) {
    env.mock_all_auths();
    let (client, admin, sme) = setup(env);
    let token = install_stellar_asset_token(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &target,
        &800i64, // 8 % yield
        &0u64,   // no maturity gate
        &token.id,
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

    (client, token, sme)
}

/// Mint `amount` to `address`, then call `client.fund(investor, amount)`.
fn mint_and_fund(
    client: &LiquifactEscrowClient<'_>,
    token: &StellarTestToken<'_>,
    investor: &Address,
    amount: i128,
) {
    token.stellar.mint(investor, &amount);
    client.fund(investor, &amount);
}

// ── Test: fresh escrow ───────────────────────────────────────────────────────

#[test]
fn reconciliation_fresh_escrow() {
    let env = Env::default();
    let (client, token, _sme) = setup_escrow(&env, 1000, "FRESH01");

    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 0);
    assert_eq!(view.outstanding_liability, 0);
    assert_eq!(view.surplus, 0);
    assert_invariant(&client, &token);
}

// ── Test: settlement-path lifecycle ──────────────────────────────────────────
// Sequences: fresh → partial fund → full fund → settle → claim_A → claim_B

#[test]
fn reconciliation_lifecycle_settle_path() {
    let env = Env::default();
    let (client, token, _sme) = setup_escrow(&env, 1000, "LIFE_SETTLE");

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);

    // ── Step 1: Fresh (status 0, never funded) ───────────────────────────────
    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 0);
    assert_eq!(view.outstanding_liability, 0);
    assert_eq!(view.surplus, 0);
    assert_invariant(&client, &token);

    // ── Step 2: Partially funded open (status 0, 400 < target 1000) ──────────
    mint_and_fund(&client, &token, &inv_a, 400);
    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 400);
    assert_eq!(view.outstanding_liability, 400);
    assert_eq!(view.surplus, 0);
    assert_eq!(client.get_escrow().status, 0, "still open");
    assert_invariant(&client, &token);

    // ── Step 3: Fully funded (status 1, crosses target) ──────────────────────
    mint_and_fund(&client, &token, &inv_b, 600);
    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 1000);
    assert_eq!(view.outstanding_liability, 1000);
    assert_eq!(view.surplus, 0);
    assert_eq!(client.get_escrow().status, 1, "must be funded");
    assert_eq!(client.get_distributed_principal(), 0);
    assert_invariant(&client, &token);

    // ── Step 4: Settled (status 2, no claims yet) ────────────────────────────
    // coupon = 1000 * 800 / 10_000 = 80
    let yield_coupon = 80i128;
    token.stellar.mint(&client.address, &yield_coupon);
    client.settle();
    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 1000 + yield_coupon); // principal + yield
    assert_eq!(view.outstanding_liability, 1000); // DP unchanged in settled
    assert_eq!(view.surplus, yield_coupon); // surplus is the extra yield
    assert_eq!(client.get_escrow().status, 2, "must be settled");
    assert_eq!(client.get_distributed_principal(), 0);
    assert_invariant(&client, &token);

    // ── Step 5: Claim investor A (post-settlement, partial claim) ────────────
    // payout_A = 400 * (1000 + 80) / 1000 = 432
    client.claim_investor_payout(&inv_a);
    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 1080 - 432);
    assert_eq!(view.outstanding_liability, 1000); // DP still 0 in settled
    assert_eq!(view.surplus, (1080 - 432) - 1000); // deficit: -352
    assert_invariant(&client, &token);

    // ── Step 6: Claim investor B (post-settlement, all claimed) ──────────────
    // payout_B = 600 * 1080 / 1000 = 648
    client.claim_investor_payout(&inv_b);
    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 0); // all tokens paid out
    assert_eq!(view.outstanding_liability, 1000);
    assert_eq!(view.surplus, -1000i128); // deficit: owe 1000, have 0
    assert_invariant(&client, &token);
}

// ── Test: cancellation-path lifecycle ────────────────────────────────────────
// Sequences: fund A → fund B → cancel → mint dust → sweep → refund A →
//            mint dust → sweep (+ oversweep guard) → refund B

#[test]
fn reconciliation_lifecycle_cancel_path() {
    let env = Env::default();
    let (client, token, _sme) = setup_escrow(&env, 2000, "LIFE_CANCEL");

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);

    // ── Step 1: Fund A 800, fund B 700 (total 1500 < 2000, status 0) ─────────
    mint_and_fund(&client, &token, &inv_a, 800);
    mint_and_fund(&client, &token, &inv_b, 700);
    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 1500);
    assert_eq!(view.outstanding_liability, 1500);
    assert_eq!(view.surplus, 0);
    assert_eq!(client.get_escrow().status, 0, "still open");
    assert_eq!(client.get_distributed_principal(), 0);
    assert_invariant(&client, &token);

    // ── Step 2: Cancel → status 4, no refunds yet ────────────────────────────
    client.cancel_funding();
    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 1500);
    assert_eq!(view.outstanding_liability, 1500);
    assert_eq!(view.surplus, 0);
    assert_eq!(client.get_escrow().status, 4, "must be cancelled");
    assert_eq!(client.get_distributed_principal(), 0);
    assert_invariant(&client, &token);

    // ── Step 3: Mint dust → surplus appears, sweep succeeds ──────────────────
    let dust = 50i128;
    token.stellar.mint(&client.address, &dust);
    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 1550);
    assert_eq!(view.outstanding_liability, 1500);
    assert_eq!(view.surplus, dust);
    assert_invariant(&client, &token);

    let swept = client.sweep_terminal_dust(&view.surplus);
    assert_eq!(swept, dust);
    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 1500);
    assert_eq!(view.outstanding_liability, 1500);
    assert_eq!(view.surplus, 0);
    assert_invariant(&client, &token);

    // ── Step 4: Refund A → DP = 800, outstanding drops to 700 ────────────────
    client.refund(&inv_a);
    assert_eq!(client.get_distributed_principal(), 800);
    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 700); // 1500 - 800
    assert_eq!(view.outstanding_liability, 700); // 1500 - 800
    assert_eq!(view.surplus, 0);
    assert_invariant(&client, &token);

    // ── Step 5: Mint more dust, sweep it, verify oversweep blocked ───────────
    token.stellar.mint(&client.address, &30i128);
    let view = client.get_reconciliation();
    assert_eq!(view.surplus, 30);

    let swept2 = client.sweep_terminal_dust(&view.surplus);
    assert_eq!(swept2, 30);

    let view = client.get_reconciliation();
    assert_eq!(view.surplus, 0);
    // surplus + 1 would eat into principal → blocked by liability floor
    assert!(
        client.try_sweep_terminal_dust(&1i128).is_err(),
        "oversweep must be blocked by liability floor",
    );
    assert_invariant(&client, &token);

    // ── Step 6: Refund B → all principal returned, DP = 1500 ─────────────────
    client.refund(&inv_b);
    assert_eq!(client.get_distributed_principal(), 1500);
    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 0);
    assert_eq!(view.outstanding_liability, 0);
    assert_eq!(view.surplus, 0);
    assert_invariant(&client, &token);
}

// ── Test: over-funded escrow ─────────────────────────────────────────────────

#[test]
fn reconciliation_overfunded() {
    let env = Env::default();
    let (client, token, _sme) = setup_escrow(&env, 1000, "OVERFUND01");

    // Single fund that exceeds the target → status 1, funded_amount = 1500
    let investor = Address::generate(&env);
    mint_and_fund(&client, &token, &investor, 1500);

    assert_eq!(client.get_escrow().status, 1, "must be funded");
    assert_eq!(client.get_escrow().funded_amount, 1500);

    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 1500);
    assert_eq!(view.outstanding_liability, 1500);
    assert_eq!(view.surplus, 0);
    assert_invariant(&client, &token);
}

// ── Test: withdrawn escrow ───────────────────────────────────────────────────

#[test]
fn reconciliation_withdrawn() {
    let env = Env::default();
    let (client, token, _sme) = setup_escrow(&env, 1000, "WITHDRAWN01");

    // Fund to target → status 1, balance = 1000
    let investor = Address::generate(&env);
    mint_and_fund(&client, &token, &investor, 1000);
    assert_eq!(client.get_escrow().status, 1);

    // Withdraw → status 3, DP = 1000, balance = 0 (fee=0 → all to SME)
    client.withdraw();
    assert_eq!(client.get_escrow().status, 3, "must be withdrawn");
    assert_eq!(client.get_distributed_principal(), 1000);

    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 0);
    assert_eq!(view.outstanding_liability, 0); // DP = funded_amount
    assert_eq!(view.surplus, 0);
    assert_invariant(&client, &token);
}

// ── Test: withdrawn escrow with residual dust ────────────────────────────────
// Extra tokens minted before withdraw remain as surplus after withdrawal.

#[test]
fn reconciliation_withdrawn_with_dust() {
    let env = Env::default();
    let (client, token, _sme) = setup_escrow(&env, 1000, "WITHDUST01");

    let investor = Address::generate(&env);
    mint_and_fund(&client, &token, &investor, 1000);

    // Mint extra dust into the contract before withdraw
    token.stellar.mint(&client.address, &42i128);

    client.withdraw();
    assert_eq!(client.get_distributed_principal(), 1000);

    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 42);
    assert_eq!(view.outstanding_liability, 0);
    assert_eq!(view.surplus, 42);
    assert_invariant(&client, &token);
}

// ── Test: cancelled escrow, full refund with residual dust ───────────────────

#[test]
fn reconciliation_cancelled_full_refund_dust() {
    let env = Env::default();
    let (client, token, _sme) = setup_escrow(&env, 2000, "FULLREF01");

    let investor = Address::generate(&env);
    mint_and_fund(&client, &token, &investor, 1000);

    // Mint dust on top of principal
    let dust = 7i128;
    token.stellar.mint(&client.address, &dust);

    client.cancel_funding();
    assert_eq!(client.get_distributed_principal(), 0);

    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 1007);
    assert_eq!(view.outstanding_liability, 1000);
    assert_eq!(view.surplus, 7);
    assert_invariant(&client, &token);

    // Sweep the dust before refund
    let swept = client.sweep_terminal_dust(&view.surplus);
    assert_eq!(swept, 7);

    // Refund the investor → DP = 1000, balance = 1000
    client.refund(&investor);
    assert_eq!(client.get_distributed_principal(), 1000);

    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 0);
    assert_eq!(view.outstanding_liability, 0);
    assert_eq!(view.surplus, 0);
    assert_invariant(&client, &token);
}

// ── Test: deficit display documented in reconciliation ───────────────────────
// When contract holds fewer tokens than outstanding liability, surplus is
// negative.  The docstring explicitly calls this out so we test it.

#[test]
fn reconciliation_deficit_display() {
    let env = Env::default();
    let (client, token, _sme) = setup_escrow(&env, 1000, "DEFICIT01");

    let investor = Address::generate(&env);
    mint_and_fund(&client, &token, &investor, 1000);

    // Settle with no yield minted → balance = 1000, but payout needs
    // principal + yield so we mint the yield to make claims succeed.
    let yield_coupon = 80i128;
    token.stellar.mint(&client.address, &yield_coupon);
    client.settle();

    // Claim the only investor → all 1080 leaves the contract
    client.claim_investor_payout(&investor);
    assert_eq!(token.token.balance(&client.address), 0);

    // Reconciliation must show the deficit
    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 0);
    assert_eq!(view.outstanding_liability, 1000);
    assert!(view.surplus < 0, "surplus must be negative in deficit");
    assert_eq!(view.surplus, -1000i128);
    assert_invariant(&client, &token);
}
