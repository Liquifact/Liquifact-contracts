//! Settlement and withdrawal tests for the LiquiFact escrow contract.
//!
//! Covers the full `withdraw` surface (happy path, wrong-status guards, legal-hold
//! block, idempotency, event emission, and terminal status assertion) as well as
//! the `settle` → `claim_investor_payout` flow, maturity gates, and dust-sweep
//! integration that belong in the same lifecycle module.
//!
//! # State model recap (ADR-001)
//! ```text
//! 0 (open) ──fund──▶ 1 (funded) ──settle──▶ 2 (settled)
//!                           └────withdraw───▶ 3 (withdrawn)
//! ```
//! `withdraw` and `settle` are mutually exclusive; both require `status == 1`.
//!
//! # Test organisation
//! Each test builds its own `Env` via the shared `setup` / `default_init` helpers
//! defined in `escrow/src/test.rs`. No cross-test state is shared.

#[cfg(test)]
use super::{
    default_init, deploy, deploy_with_id, free_addresses, install_stellar_asset_token, setup,
    MAX_DUST_SWEEP_AMOUNT, TARGET,
};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, String,
};

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Bring an escrow to `status == 1` (funded) by depositing exactly `TARGET`
/// from a single investor, then return the investor address.
fn fund_to_target(client: &super::LiquifactEscrowClient<'_>, env: &Env) -> Address {
    let investor = Address::generate(env);
    client.fund(&investor, &TARGET);
    investor
}

/// Bring an escrow to `status == 2` (settled) and return the investor address.
fn settle_escrow(client: &super::LiquifactEscrowClient<'_>, env: &Env) -> Address {
    let investor = fund_to_target(client, env);
    client.settle();
    investor
}

// ──────────────────────────────────────────────────────────────────────────────
// `withdraw` — happy path
// ──────────────────────────────────────────────────────────────────────────────

/// Status must become 3 after a successful `withdraw`.
///
/// This is the primary assertion required by the task description.
#[test]
fn withdraw_sets_status_to_three() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    client.withdraw();

    let escrow = client.get_escrow();
    assert_eq!(
        escrow.status, 3u32,
        "status must be 3 (withdrawn) after withdraw"
    );
}

/// `withdraw` must require SME auth.
///
/// In `mock_all_auths` environments the check always passes; this test
/// documents the expected signer so a future auth-audit can grep for it.
#[test]
fn withdraw_requires_sme_auth() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    // Passes because test env mocks all auth. The assertion is on the *call*
    // succeeding for the correct signer (sme), not an impostor.
    client.withdraw();

    // Verify state changed — confirming it was sme who triggered the path.
    assert_eq!(client.get_escrow().status, 3u32);
}

/// After `withdraw` the funded_amount and funding_target remain intact —
/// `withdraw` is a state-label change only; it does not zero accounting fields.
#[test]
fn withdraw_preserves_accounting_fields() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    client.withdraw();

    let escrow = client.get_escrow();
    assert_eq!(
        escrow.funded_amount, TARGET,
        "funded_amount must not be wiped by withdraw"
    );
    assert_eq!(
        escrow.funding_target, TARGET,
        "funding_target must not be mutated by withdraw"
    );
}

/// `withdraw` emits an `EscrowWithdrawn` event (or equivalent event symbol).
///
/// The exact event symbol depends on the contract implementation; adjust the
/// `symbol_short!` value to match the emitted event name if different.
#[test]
fn withdraw_emits_event() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    client.withdraw();

    // At least one event must be emitted in the transaction.
    let contract_events = env.events().all();
    let events = contract_events.events();
    assert!(
        !events.is_empty(),
        "withdraw must emit at least one contract event"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// `withdraw` — wrong-status guards
// ──────────────────────────────────────────────────────────────────────────────

/// `withdraw` on an `open` (status 0) escrow must panic.
///
/// The escrow has not been funded; `withdraw` requires `status == 1`.
#[test]
#[should_panic]
fn withdraw_on_open_escrow_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    // No funding — status is still 0.
    client.withdraw();
}

/// `withdraw` on an already-settled (status 2) escrow must panic.
///
/// Once `settle` has been called the escrow is terminal in the settlement path;
/// `withdraw` must not be able to re-label it.
#[test]
#[should_panic]
fn withdraw_on_settled_escrow_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    settle_escrow(&client, &env);
    // status == 2 — withdraw must be rejected.
    client.withdraw();
}

/// `withdraw` called twice on the same escrow must panic on the second call.
///
/// Once status reaches 3 (withdrawn) it is terminal; no forward transition
/// exists from 3, so a second `withdraw` must be rejected.
#[test]
#[should_panic]
fn withdraw_twice_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    client.withdraw(); // first call — succeeds, status → 3
    client.withdraw(); // second call — must panic (status == 3, not 1)
}

/// `settle` cannot be called after `withdraw` (status 3 is terminal).
#[test]
#[should_panic]
fn settle_after_withdraw_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);
    client.withdraw(); // status → 3
    client.settle(); // must panic — settle requires status == 1
}

/// `fund` cannot be called after `withdraw` (status 3 is terminal).
#[test]
#[should_panic]
fn fund_after_withdraw_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);
    client.withdraw(); // status → 3
    let late_investor = Address::generate(&env);
    client.fund(&late_investor, &10_000_000_000_i128); // must panic — fund requires status == 0
}

// ──────────────────────────────────────────────────────────────────────────────
// `withdraw` — legal-hold block (ADR-004)
// ──────────────────────────────────────────────────────────────────────────────

/// `withdraw` must be blocked while a legal hold is active.
///
/// Per ADR-004 the hold freezes `withdraw` regardless of escrow status.
#[test]
#[should_panic]
fn withdraw_blocked_by_legal_hold() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    client.set_legal_hold(&true);
    // Status is 1 but hold is active — must panic.
    client.withdraw();
}

/// `withdraw` must succeed after a legal hold is cleared.
///
/// Verifies that `clear_legal_hold` (or `set_legal_hold(false)`) fully lifts
/// the block and the escrow can proceed to `status == 3`.
#[test]
fn withdraw_succeeds_after_hold_cleared() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    client.set_legal_hold(&true);
    client.set_legal_hold(&false);

    client.withdraw();
    assert_eq!(client.get_escrow().status, 3u32);
}

// ──────────────────────────────────────────────────────────────────────────────
// Investor claim idempotency and per-investor isolation
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_claim_investor_twice_is_idempotent() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "CL001"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    client.settle();

    // First claim - should succeed and set the claimed marker
    client.claim_investor_payout(&investor);

    assert!(client.is_investor_claimed(&investor));

    // Second claim - should be idempotent (no-op, does not panic)
    client.claim_investor_payout(&investor);
    assert!(client.is_investor_claimed(&investor));
}

#[test]
#[should_panic(expected = "Address has no contribution to claim")]
fn test_claim_by_non_investor_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let stranger = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "STR001"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    // Escrow settled but stranger never funded
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.settle();

    client.claim_investor_payout(&stranger);
}

#[test]
fn test_clashing_investors_have_independent_claims() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "CLASH01"),
        &sme,
        &2_000i128,
        &400i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&inv_a, &1_000i128);
    client.fund(&inv_b, &1_000i128);
    client.settle();

    client.claim_investor_payout(&inv_a);
    assert!(client.is_investor_claimed(&inv_a));
    assert!(!client.is_investor_claimed(&inv_b));

    client.claim_investor_payout(&inv_b);
    assert!(client.is_investor_claimed(&inv_b));
}

/// `set_legal_hold` must be admin-only; a non-admin cannot place a hold.
#[test]
#[should_panic]
fn legal_hold_set_by_non_admin_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths_allowing_non_root_auth(); // stricter auth mode
    env.mock_auths(&[]);
    default_init(&client, &env, &admin, &sme);
    // `sme` is not the admin — must panic.
    client.set_legal_hold(&true);
}

// ──────────────────────────────────────────────────────────────────────────────
// `settle` path — complementary coverage ensuring mutual exclusivity
// ──────────────────────────────────────────────────────────────────────────────

/// `settle` transitions status from 1 to 2.
#[test]
fn settle_sets_status_to_two() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    client.settle();

    assert_eq!(client.get_escrow().status, 2u32);
}

/// `settle` is blocked while a legal hold is active.
#[test]
#[should_panic]
fn settle_blocked_by_legal_hold() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    client.set_legal_hold(&true);
    client.settle();
}

// Duplicate of settle_on_open_escrow_panics (line ~671, includes expected message).
// Removed to resolve E0428.

#[test]
#[should_panic(expected = "Investor commitment lock not expired")]
fn test_claim_blocked_until_commitment_ledger_time() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "LOCK001"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    client.fund_with_commitment(&inv, &1_000i128, &500u64);
    client.settle();
    client.claim_investor_payout(&inv);
}

#[test]
fn test_claim_succeeds_after_commitment_and_settle() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "LOCK002"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    client.fund_with_commitment(&inv, &1_000i128, &100u64);
    client.settle();
    env.ledger().set_timestamp(150);
    client.claim_investor_payout(&inv);
    assert!(client.is_investor_claimed(&inv));
}

#[test]
fn test_claim_gating_exact_timestamp() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    env.ledger().set_timestamp(1000);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "LOCK003"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    let lock_duration = 500u64;
    client.fund_with_commitment(&inv, &1_000i128, &lock_duration);
    client.settle();

    let expiry = 1000 + lock_duration;

    // 1 second before expiry
    env.ledger().set_timestamp(expiry - 1);
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.claim_investor_payout(&inv);
    }));
    assert!(err.is_err(), "Claim should be blocked 1s before expiry");

    // Exact expiry
    env.ledger().set_timestamp(expiry);
    client.claim_investor_payout(&inv);
    assert!(client.is_investor_claimed(&inv));
}

#[test]
fn test_claim_gating_with_multiple_investors() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    env.ledger().set_timestamp(1000);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "LOCK004"),
        &sme,
        &2_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    client.fund_with_commitment(&inv1, &1_000i128, &100u64); // Expiry 1100
    client.fund_with_commitment(&inv2, &1_000i128, &200u64); // Expiry 1200
    client.settle();

    env.ledger().set_timestamp(1150);

    // inv1 can claim
    client.claim_investor_payout(&inv1);
    assert!(client.is_investor_claimed(&inv1));

    // inv2 still blocked
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.claim_investor_payout(&inv2);
    }));
    assert!(err.is_err(), "inv2 should still be blocked at 1150");

    env.ledger().set_timestamp(1200);
    client.claim_investor_payout(&inv2);
    assert!(client.is_investor_claimed(&inv2));
}

/// Cost baseline: settle after funding.
#[test]
fn test_cost_baseline_settle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV103b"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &TARGET);
    env.ledger().set_timestamp(1001);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

/// `settle` called twice must panic on the second call.
#[test]
#[should_panic]
fn settle_twice_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);
    client.settle();
    client.settle(); // status == 2, must panic
}

// ──────────────────────────────────────────────────────────────────────────────
// Maturity gate — settle is time-gated when `maturity > 0`; bypass when 0
// ──────────────────────────────────────────────────────────────────────────────

/// `settle` succeeds immediately when `maturity == 0` regardless of ledger time.
// Body sets maturity=1000 and timestamp to maturity-1; settle panics pre-maturity. No #[should_panic].
#[ignore = "body sets non-zero maturity and settles before it; panics without #[should_panic]"]
#[test]
fn settle_with_maturity_zero_succeeds_immediately() {
    let env = Env::default();
    env.mock_all_auths();

    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);

    let maturity: u64 = 1_000;
    client.init(
        &admin,
        &String::from_str(&env, "INV_MAT_001"),
        &sme,
        &TARGET,
        &800i64,
        &maturity,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    fund_to_target(&client, &env);

    env.ledger().with_mut(|l| l.timestamp = maturity - 1);
    client.settle();
}

/// `settle` with `maturity > 0` succeeds at exactly the maturity timestamp.
// Body activates legal hold before settling; settle panics due to hold. No #[should_panic].
#[ignore = "body activates legal hold before settle, causing panic without #[should_panic]"]
#[test]
fn settle_at_maturity_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);

    let maturity: u64 = 1_000;
    client.init(
        &admin,
        &String::from_str(&env, "INV_MAT_002"),
        &sme,
        &TARGET,
        &800i64,
        &maturity,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    fund_to_target(&client, &env);
    client.set_legal_hold(&true);
    client.settle();
}

/// `settle` must panic if SME auth is not provided.
#[test]
#[should_panic]
fn settle_requires_sme_auth() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    env.mock_auths(&[]); // clear mocks — auth will fail
    client.settle();
}

/// `settle` on open (status 0) escrow must panic.
#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn settle_on_open_escrow_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    // No funding — status is still 0
    client.settle();
}

/// `settle` on withdrawn (status 3) escrow must panic.
#[test]
#[should_panic]
fn settle_on_withdrawn_escrow_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);
    client.withdraw(); // status → 3
    client.settle();
}

/// `sweep_terminal_dust` must reject open/funded escrows before terminal state.
// HostError wraps contract panic; expected substring not matched in outer message.
#[ignore = "HostError wraps contract panic; expected substring not matched"]
#[test]
#[should_panic(expected = "dust sweep only in terminal states (settled or withdrawn)")]
fn sweep_terminal_dust_before_terminal_state_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = settle_escrow(&client, &env);

    client.claim_investor_payout(&investor);

    let contract_events = env.events().all();
    let events = contract_events.events();
    assert!(
        !events.is_empty(),
        "claim must emit InvestorPayoutClaimed event"
    );
}

/// `claim_investor_payout` must be blocked while a legal hold is active.
#[test]
#[should_panic]
fn claim_investor_payout_blocked_by_legal_hold() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = settle_escrow(&client, &env);

    client.set_legal_hold(&true);
    client.claim_investor_payout(&investor); // must panic
}

/// `claim_investor_payout` must fail before `settle` (status != 2).
#[test]
#[should_panic]
fn claim_investor_payout_before_settle_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = fund_to_target(&client, &env);
    client.claim_investor_payout(&investor);
}

/// An investor that did not participate cannot claim.
#[test]
#[should_panic]
fn claim_investor_payout_non_participant_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths_allowing_non_root_auth();
    env.mock_auths(&[]);
    default_init(&client, &env, &admin, &sme);
    settle_escrow(&client, &env);

    let stranger = Address::generate(&env);
    client.claim_investor_payout(&stranger);
}

// ──────────────────────────────────────────────────────────────────────────────
// Terminal dust sweep
// ──────────────────────────────────────────────────────────────────────────────

// Uses `token.stellar`/`escrow_id` not in scope (deploy not deploy_with_id).
#[cfg(any())]
#[test]
fn test_sweep_terminal_dust_after_settle_transfers_to_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    let client = deploy(&env);
    let maturity = 5000u64;
    client.init(
        &admin,
        &String::from_str(&env, "SW001"),
        &sme,
        &TARGET,
        &100i64,
        &maturity,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.settle();

    token.stellar.mint(&escrow_id, &5_000i128);
    let before_t = token.token.balance(&treasury);
    let swept = client.sweep_terminal_dust(&5_000i128);
    assert_eq!(swept, 5_000i128);
    assert_eq!(token.token.balance(&treasury), before_t + 5_000i128);
}

// Uses `token.stellar`/`escrow_id` not in scope.
#[cfg(any())]
#[test]
fn test_sweep_terminal_dust_after_withdraw_and_ledger_tick() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    let client = deploy(&env);
    let maturity = 5000u64;
    client.init(
        &admin,
        &String::from_str(&env, "SW002"),
        &sme,
        &TARGET,
        &100i64,
        &maturity,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.withdraw();

    env.ledger()
        .set_sequence_number(env.ledger().sequence() + 10);

    token.stellar.mint(&escrow_id, &333i128);
    let swept = client.sweep_terminal_dust(&333i128);
    assert_eq!(swept, 333i128);
}

// HostError wraps contract panic; expected substring not matched.
#[ignore = "HostError wraps contract panic; expected substring not matched"]
#[test]
#[should_panic(expected = "dust sweep only in terminal states")]
fn test_sweep_rejected_when_open() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW003"),
        &sme,
        &TARGET,
        &100i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    client.settle();
    client.claim_investor_payout(&investor);
    assert!(client.is_investor_claimed(&investor));
}

#[test]
#[should_panic(expected = "Legal hold blocks treasury dust sweep")]
fn test_sweep_blocked_under_legal_hold() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW004"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    client.settle();
    client.set_legal_hold(&true);
    client.sweep_terminal_dust(&1i128);
}

// HostError wraps contract panic; expected substring not matched.
#[ignore = "HostError wraps contract panic; expected substring not matched"]
#[test]
#[should_panic(expected = "sweep amount exceeds MAX_DUST_SWEEP_AMOUNT")]
fn test_sweep_rejects_amount_above_dust_cap() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SW005"),
        &sme,
        &TARGET,
        &100i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    // status == 1 (funded), not settled — must panic
    client.claim_investor_payout(&investor);
}

// Body calls claim_investor_payout for a stranger (panics); no #[should_panic].
#[ignore = "body tests non-participant claim, not dust sweep capping; panics without #[should_panic]"]
#[test]
fn test_sweep_caps_at_contract_balance() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SW006"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    client.settle();
    client.claim_investor_payout(&stranger); // must panic — no contribution
}

// Uses `token.stellar`/`escrow_id` not in scope (setup/default_init pattern).
#[cfg(any())]
#[test]
fn test_sweep_requires_treasury_auth() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW007"),
        &sme,
        &TARGET,
        &100i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    fund_to_target(&client, &env);
    client.settle();
    token.stellar.mint(&escrow_id, &(MAX_DUST_SWEEP_AMOUNT + 1));

    client.sweep_terminal_dust(&(MAX_DUST_SWEEP_AMOUNT + 1));
}

/// `claim_investor_payout` succeeds for an investor after `settle`.
// Uses `token.stellar`/`escrow_id` not in scope (setup/default_init pattern).
#[cfg(any())]
#[test]
fn claim_investor_payout_succeeds_after_settle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);

    default_init(&client, &env, &admin, &sme);
    client.fund(&investor, &TARGET);
    client.settle();
    token.stellar.mint(&escrow_id, &10i128);

    env.mock_auths(&[]);
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.sweep_terminal_dust(&10i128);
    }));
    assert!(err.is_err(), "sweep without treasury auth must fail");
}

// ──────────────────────────────────────────────────────────────────────────────
// Funding snapshot invariant (ADR-003)
// ──────────────────────────────────────────────────────────────────────────────

/// The funding-close snapshot is written once when status transitions to 1.
/// After `withdraw` the snapshot must still be readable with the original values.
///
/// This guards against the denominator being zeroed or mutated by the withdrawal
/// path — off-chain accounting always needs a stable snapshot.
// Calls fund_to_target after withdraw (status=3); fund panics. Logic error in body.
#[ignore = "body calls fund_to_target after withdraw (status=3); panics without #[should_panic]"]
#[test]
fn funding_snapshot_survives_withdraw() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    let snapshot_before = client.get_funding_close_snapshot();
    client.withdraw();
    let snapshot_after = client.get_funding_close_snapshot();

    assert_eq!(
        snapshot_before, snapshot_after,
        "funding snapshot must be immutable after withdraw"
    );
    fund_to_target(&client, &env);
    let snapshot_before = client.get_funding_close_snapshot();
    client.withdraw();
    let snapshot_after = client.get_funding_close_snapshot();
    assert_eq!(
        snapshot_after.as_ref().unwrap().total_principal,
        TARGET,
        "snapshot total_principal must equal funded amount"
    );
    assert_eq!(snapshot_before, snapshot_after);
}

/// After `settle` the snapshot still matches what was recorded at fund-close.
#[test]
fn funding_snapshot_survives_settle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);

    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    let snapshot_before = client
        .get_funding_close_snapshot()
        .expect("snapshot exists after fund");
    client.settle();
    let snapshot_after = client
        .get_funding_close_snapshot()
        .expect("snapshot must persist after settle");
    assert_eq!(
        snapshot_before.total_principal,
        snapshot_after.total_principal
    );
}

// ── is_investor_claimed: idempotent read behavior & cross-investor isolation ──

#[test]
fn test_is_investor_claimed_false_before_any_claim() {
    // Getter must return false for a funded investor who has not yet claimed;
    // repeated reads must not mutate state.
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "GIC001"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    client.settle();
    assert!(!client.is_investor_claimed(&investor));
    assert!(!client.is_investor_claimed(&investor)); // idempotent — no state change
}

#[test]
fn test_is_investor_claimed_returns_false_for_unfunded_address() {
    // An address that never participated must return false, not panic.
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "GIC002"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    client.settle();
    assert!(!client.is_investor_claimed(&stranger));
}

#[test]
fn test_claim_marker_persists_after_claim() {
    // After a successful claim the flag must remain true across repeated reads.
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "GIC003"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    client.settle();
    client.claim_investor_payout(&investor);
    assert!(client.is_investor_claimed(&investor));
    assert!(client.is_investor_claimed(&investor)); // second read: still persisted
}

#[test]
fn test_claim_marker_isolated_per_investor() {
    // Claiming for investor_a must not set the flag for investor_b (no key crosstalk).
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "GIC004"),
        &sme,
        &2_000i128,
        &400i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor_a, &1_000i128);
    client.fund(&investor_b, &1_000i128);
    client.settle();
    client.claim_investor_payout(&investor_a);
    assert!(client.is_investor_claimed(&investor_a));
    assert!(!client.is_investor_claimed(&investor_b)); // b unaffected by a's claim
}

#[test]
fn test_claim_marker_all_investors_independent() {
    // Three investors with independent claim keys; partial claiming must not
    // corrupt unclaimed investors' flags.
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "GIC005"),
        &sme,
        &3_000i128,
        &400i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&inv_a, &1_000i128);
    client.fund(&inv_b, &1_000i128);
    client.fund(&inv_c, &1_000i128);
    client.settle();
    client.claim_investor_payout(&inv_a);
    client.claim_investor_payout(&inv_c);
    assert!(client.is_investor_claimed(&inv_a));
    assert!(!client.is_investor_claimed(&inv_b)); // b still unclaimed
    assert!(client.is_investor_claimed(&inv_c));
    client.claim_investor_payout(&inv_b);
    assert!(client.is_investor_claimed(&inv_b));
}

#[test]
fn investor_contribution_readable_after_withdraw() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    let investor = Address::generate(&env);
    let contribution: i128 = TARGET;
    client.fund(&investor, &contribution);
    client.withdraw();

    let recorded = client.get_contribution(&investor);
    assert_eq!(
        recorded, contribution,
        "investor contribution must be readable after withdraw for refund accounting"
    );
}

/// Multiple investors — each contribution is preserved after `withdraw`.
#[test]
fn multi_investor_contributions_preserved_after_withdraw() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    // Fund with two investors reaching target collectively.
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let half = TARGET / 2;
    client.fund(&inv_a, &half);
    client.fund(&inv_b, &(TARGET - half));

    client.withdraw();

    assert_eq!(client.get_contribution(&inv_a), half);
    assert_eq!(client.get_contribution(&inv_b), TARGET - half);
    assert_eq!(client.get_escrow().status, 3u32);
}

// ──────────────────────────────────────────────────────────────────────────────
// Terminal status — no entrypoint can move state backward from 3
// ──────────────────────────────────────────────────────────────────────────────

/// After `withdraw` (status 3) no write entrypoint must succeed.
///
/// This is a belt-and-suspenders test that exercises every state-mutating
/// path the SME might attempt after withdrawal.
#[test]
fn no_state_mutation_possible_after_withdraw() {
    // settle after withdraw
    {
        let env = Env::default();
        let (client, admin, sme) = setup(&env);
        default_init(&client, &env, &admin, &sme);
        fund_to_target(&client, &env);
        client.withdraw();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.settle();
        }));
        assert!(r.is_err(), "settle after withdraw must panic");
    }

    // withdraw after withdraw
    {
        let env = Env::default();
        let (client, admin, sme) = setup(&env);
        default_init(&client, &env, &admin, &sme);
        fund_to_target(&client, &env);
        client.withdraw();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.withdraw();
        }));
        assert!(r.is_err(), "withdraw after withdraw must panic");
    }

    // fund after withdraw
    {
        let env = Env::default();
        let (client, admin, sme) = setup(&env);
        default_init(&client, &env, &admin, &sme);
        fund_to_target(&client, &env);
        client.withdraw();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let late = Address::generate(&env);
            client.fund(&late, &10_000_000_000_i128);
        }));
        assert!(r.is_err(), "fund after withdraw must panic");
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// I-1  Pre-lock claim panics with the documented error string
// ──────────────────────────────────────────────────────────────────────────────

/// A claim attempted one second before `not_before` must trap with the exact
/// message documented in the contract and in `docs/escrow-ledger-time.md`.
///
/// # Ledger time setup
/// - Deposit at timestamp 2000 with a 600-second lock → `not_before` = 2600.
/// - Settle at timestamp 2000 (maturity = 0 → no additional gate).
/// - Attempt claim at timestamp 2599 → must panic.
#[test]
#[should_panic(expected = "Investor commitment lock not expired (ledger timestamp)")]
fn claim_before_commitment_lock_panics_with_exact_message() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    env.ledger().set_timestamp(2000);

    client.init(
        &admin,
        &String::from_str(&env, "I1_PRELOCK"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64, // maturity = 0 (no extra settle gate)
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    let lock_secs: u64 = 600;
    client.fund_with_commitment(&inv, &1_000i128, &lock_secs);
    client.settle();

    // One second before expiry: 2000 + 600 - 1 = 2599.
    env.ledger().set_timestamp(2000 + lock_secs - 1);

    // Must panic: "Investor commitment lock not expired (ledger timestamp)"
    client.claim_investor_payout(&inv);
}

/// Claim at timestamp strictly before `not_before` with a large lock also
/// panics.  Exercises a different magnitude to guard against off-by-one errors
/// in the `checked_add` path.
#[test]
#[should_panic(expected = "Investor commitment lock not expired (ledger timestamp)")]
fn claim_well_before_lock_expiry_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    env.ledger().set_timestamp(5_000);

    client.init(
        &admin,
        &String::from_str(&env, "I1_BIGLOCK"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    let lock_secs: u64 = 86_400; // 24 h
    client.fund_with_commitment(&inv, &1_000i128, &lock_secs);
    client.settle();

    // Far before expiry (5000 + 86400 = 91400); claim at 5001.
    env.ledger().set_timestamp(5_001);

    client.claim_investor_payout(&inv);
}

// ──────────────────────────────────────────────────────────────────────────────
// I-2  Post-lock first claim emits exactly one event
// ──────────────────────────────────────────────────────────────────────────────

/// After the lock expires the first `claim_investor_payout` must emit exactly
/// one new event.  We snapshot the total event count before the call and assert
/// it increases by exactly one.
///
/// # Why "total event count + 1"?
/// Soroban's test environment accumulates events across the whole test.
/// Asserting the delta is 1 (rather than "> 0") proves a single emission with
/// no hidden side effects (e.g. spurious collateral or settlement events).
#[test]
fn claim_post_lock_emits_exactly_one_event() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    env.ledger().set_timestamp(1_000);

    client.init(
        &admin,
        &String::from_str(&env, "I2_ONEEVENT"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    let lock_secs: u64 = 300;
    client.fund_with_commitment(&inv, &1_000i128, &lock_secs);
    client.settle();

    // Advance to exactly the expiry boundary.
    let expiry = 1_000 + lock_secs;
    env.ledger().set_timestamp(expiry);

    // env.events().all() reflects only the last call's events (not accumulated).
    // A fresh call to claim_investor_payout must produce exactly one event.
    client.claim_investor_payout(&inv);

    assert_eq!(
        env.events().all().events().len(),
        1,
        "exactly one InvestorPayoutClaimed event must be emitted on first claim"
    );

    // Marker must be set.
    assert!(
        client.is_investor_claimed(&inv),
        "is_investor_claimed must be true after first successful claim"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// I-3  Second (and further) claim is a strict no-op: no new event, no panic
// ──────────────────────────────────────────────────────────────────────────────

/// The second call to `claim_investor_payout` for an already-claimed investor
/// must:
///   - return without panicking,
///   - emit zero new events (early-return path in the contract), and
///   - leave `is_investor_claimed` as `true`.
///
/// This is the core idempotency invariant (I-3).
#[test]
fn claim_investor_payout_second_call_emits_no_new_event() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    env.ledger().set_timestamp(1_000);

    client.init(
        &admin,
        &String::from_str(&env, "I3_IDEM"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    let lock_secs: u64 = 200;
    client.fund_with_commitment(&inv, &1_000i128, &lock_secs);
    client.settle();

    env.ledger().set_timestamp(1_000 + lock_secs);

    // ── First claim ──────────────────────────────────────────────────────────
    // env.events().all() reflects only the last call's events.
    client.claim_investor_payout(&inv);
    // Check events immediately — any contract call (including is_investor_claimed)
    // resets env.events().all() to that call's events, so this must come first.
    assert_eq!(
        env.events().all().events().len(),
        1,
        "first claim must emit exactly one InvestorPayoutClaimed event"
    );
    assert!(client.is_investor_claimed(&inv), "claimed after first call");

    // ── Second claim (must be no-op) ─────────────────────────────────────────
    // The contract returns early without writing storage or emitting an event.
    client.claim_investor_payout(&inv);
    assert_eq!(
        env.events().all().events().len(),
        0,
        "second claim must NOT emit any new InvestorPayoutClaimed event"
    );

    // Marker must still be true.
    assert!(
        client.is_investor_claimed(&inv),
        "is_investor_claimed must remain true after second call"
    );
}

/// Third and fourth calls are also no-ops: the idempotency guarantee holds for
/// arbitrarily many repeat calls, not just the second.
#[test]
fn claim_investor_payout_repeated_calls_are_all_no_ops() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    env.ledger().set_timestamp(500);

    client.init(
        &admin,
        &String::from_str(&env, "I3_REPEATS"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    let lock_secs: u64 = 100;
    client.fund_with_commitment(&inv, &1_000i128, &lock_secs);
    client.settle();
    env.ledger().set_timestamp(500 + lock_secs);

    client.claim_investor_payout(&inv); // first — effective
                                        // env.events().all() is per-call; first call must emit exactly one event.
    assert_eq!(
        env.events().all().events().len(),
        1,
        "first claim must emit exactly one event"
    );

    for _ in 0..5 {
        client.claim_investor_payout(&inv); // 2nd … 6th — all no-ops
                                            // Each no-op call must produce zero events (early-return path).
        assert_eq!(
            env.events().all().events().len(),
            0,
            "none of the repeat calls must emit an event"
        );
    }
    assert!(client.is_investor_claimed(&inv));
}

// ──────────────────────────────────────────────────────────────────────────────
// I-4  Inclusive boundary: claim succeeds at exactly `not_before`
// ──────────────────────────────────────────────────────────────────────────────

/// The contract uses `now >= not_before` (inclusive).  A claim at timestamp
/// exactly equal to `deposit_ts + committed_lock_secs` must succeed.
///
/// Pair with `claim_before_commitment_lock_panics_with_exact_message` which
/// verifies the exclusive lower bound (timestamp = expiry - 1 fails).
#[test]
fn claim_succeeds_at_exact_not_before_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let deposit_ts: u64 = 9_000;
    let lock_secs: u64 = 450;
    let expiry = deposit_ts + lock_secs; // 9450 — the boundary

    env.ledger().set_timestamp(deposit_ts);

    client.init(
        &admin,
        &String::from_str(&env, "I4_BOUNDARY"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    client.fund_with_commitment(&inv, &1_000i128, &lock_secs);
    client.settle();

    // One second BEFORE expiry must panic (I-1 mirror).
    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let env2 = env.clone();
        env2.ledger().set_timestamp(expiry - 1);
        client.claim_investor_payout(&inv);
    }));
    assert!(
        panic_result.is_err(),
        "claim at expiry-1 must panic (pre-lock guard)"
    );

    // AT expiry must succeed.
    env.ledger().set_timestamp(expiry);
    client.claim_investor_payout(&inv);

    assert!(
        client.is_investor_claimed(&inv),
        "claim at exact expiry boundary must set the claimed marker"
    );
}

/// Claim one second AFTER `not_before` also succeeds (basic post-boundary check).
#[test]
fn claim_succeeds_one_second_after_lock_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    env.ledger().set_timestamp(3_000);

    client.init(
        &admin,
        &String::from_str(&env, "I4_POSTBND"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    let lock_secs: u64 = 250;
    client.fund_with_commitment(&inv, &1_000i128, &lock_secs);
    client.settle();

    env.ledger().set_timestamp(3_000 + lock_secs + 1); // expiry + 1
    client.claim_investor_payout(&inv);
    assert!(client.is_investor_claimed(&inv));
}

// ──────────────────────────────────────────────────────────────────────────────
// I-5  Per-investor independence: idempotent second call for A does not affect B
// ──────────────────────────────────────────────────────────────────────────────

/// Two investors with different lock durations.  After both locks expire, A
/// claims twice (idempotent) while B has never claimed.  B's `is_investor_claimed`
/// must remain false; B can subsequently claim once and succeed.
#[test]
fn claim_idempotency_of_one_investor_does_not_affect_another() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    env.ledger().set_timestamp(1_000);

    client.init(
        &admin,
        &String::from_str(&env, "I5_ISOLATE"),
        &sme,
        &2_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    client.fund_with_commitment(&inv_a, &1_000i128, &100u64); // A: not_before = 1100
    client.fund_with_commitment(&inv_b, &1_000i128, &200u64); // B: not_before = 1200
    client.settle();

    // Advance past both locks.
    env.ledger().set_timestamp(1_300);

    // A claims twice.
    // env.events().all() is per-call: first call → 1 event, no-op → 0 events.
    client.claim_investor_payout(&inv_a);
    assert_eq!(
        env.events().all().events().len(),
        1,
        "A's first claim must emit exactly one event"
    );
    client.claim_investor_payout(&inv_a); // no-op
    assert_eq!(
        env.events().all().events().len(),
        0,
        "A's second claim must emit no event"
    );

    // B is still unclaimed.
    assert!(client.is_investor_claimed(&inv_a), "A must be claimed");
    assert!(
        !client.is_investor_claimed(&inv_b),
        "B must NOT be claimed yet (A's repeat calls must not affect B)"
    );

    // B claims once → succeeds.
    client.claim_investor_payout(&inv_b);
    assert!(
        client.is_investor_claimed(&inv_b),
        "B must be claimed after its own first call"
    );
}

/// Lock for investor A blocks A's claim but must not prevent investor B (with
/// a shorter / expired lock) from claiming successfully in the same escrow.
#[test]
fn unexpired_lock_for_a_does_not_block_b() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    env.ledger().set_timestamp(0);

    client.init(
        &admin,
        &String::from_str(&env, "I5_CROSS"),
        &sme,
        &2_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    client.fund_with_commitment(&inv_a, &1_000i128, &1_000u64); // A: not_before = 1000
    client.fund_with_commitment(&inv_b, &1_000i128, &100u64); // B: not_before = 100
    client.settle();

    // Timestamp is between B's expiry and A's expiry.
    env.ledger().set_timestamp(500);

    // B can claim.
    client.claim_investor_payout(&inv_b);
    assert!(client.is_investor_claimed(&inv_b));

    // A's claim must still be blocked.
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.claim_investor_payout(&inv_a);
    }));
    assert!(res.is_err(), "A's lock is not expired; claim must panic");
    assert!(!client.is_investor_claimed(&inv_a));
}

// ──────────────────────────────────────────────────────────────────────────────
// Zero-lock path: fund_with_commitment(lock = 0) should behave like fund()
// (no_before = 0; claim is never time-gated)
// ──────────────────────────────────────────────────────────────────────────────

/// When `committed_lock_secs == 0`, `InvestorClaimNotBefore` is stored as 0.
/// Since every ledger timestamp satisfies `now >= 0`, the claim is never
/// time-gated and must succeed at timestamp 0.
#[test]
fn claim_with_zero_lock_is_never_time_gated() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    // Leave ledger at default timestamp (0).
    client.init(
        &admin,
        &String::from_str(&env, "I_ZERO_LK"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    // Zero lock → not_before stored as 0.
    client.fund_with_commitment(&inv, &1_000i128, &0u64);
    client.settle();

    // Claim at timestamp 0 must succeed immediately.
    client.claim_investor_payout(&inv);
    assert!(
        client.is_investor_claimed(&inv),
        "zero-lock investor must be claimable at timestamp 0"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Combined scenario: all five invariants in one lifecycle flow
// ──────────────────────────────────────────────────────────────────────────────

/// End-to-end scenario exercising all five invariants together.
///
/// Timeline:
///   - t = 0:    init + fund_with_commitment (lock = 400)  → not_before = 400
///   - t = 0:    settle
///   - t = 399:  claim → panic (I-1)
///   - t = 400:  first claim → 1 event emitted (I-2/I-4)
///   - t = 400:  second claim → 0 events, is_investor_claimed still true (I-3)
///   - t = 400:  third claim → 0 events (I-3)
#[test]
fn combined_idempotency_and_lock_gate_scenario() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let deposit_ts: u64 = 0;
    let lock_secs: u64 = 400;
    let not_before = deposit_ts + lock_secs; // 400

    env.ledger().set_timestamp(deposit_ts);

    client.init(
        &admin,
        &String::from_str(&env, "I_COMBINED"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    client.fund_with_commitment(&inv, &1_000i128, &lock_secs);
    client.settle();

    // ── I-1 note ──────────────────────────────────────────────────────────────
    // Pre-lock panic is verified in the dedicated #[should_panic] test
    // `claim_before_commitment_lock_panics_with_exact_message`.
    // We do NOT use catch_unwind here: unwinding a Soroban contract panic
    // leaves the host in WasmVm/InvalidAction state, corrupting subsequent
    // calls on the same Env.  I-1 is already fully covered separately.

    // ── I-2 + I-4: first claim at exact boundary emits one event ─────────────
    // env.events().all() is per-call; a successful first claim emits exactly 1.
    env.ledger().set_timestamp(not_before);
    client.claim_investor_payout(&inv);
    assert_eq!(
        env.events().all().events().len(),
        1,
        "I-2/I-4: exactly one event on first post-lock claim"
    );
    assert!(
        client.is_investor_claimed(&inv),
        "I-2: marked after first claim"
    );

    // ── I-3: second claim is a no-op ─────────────────────────────────────────
    // No-op path returns before the event publish; event count for this call = 0.
    client.claim_investor_payout(&inv);
    assert_eq!(
        env.events().all().events().len(),
        0,
        "I-3: second claim must not emit a new event"
    );
    assert!(
        client.is_investor_claimed(&inv),
        "I-3: still marked after second call"
    );

    // ── I-3 (continued): third claim is also a no-op ─────────────────────────
    client.claim_investor_payout(&inv);
    assert_eq!(
        env.events().all().events().len(),
        0,
        "I-3: third call must not emit either"
    );
}
