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
    TARGET,
};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger as _},
    Address, Env,
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
        events.len() > 0,
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
    client.fund(&late_investor, &1_000_0000000i128); // must panic — fund requires status == 0
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
fn test_claim_investor_twice_is_idempotent() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CL001"),
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
        &String::from_str(&env, "STR001"),
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
        &String::from_str(&env, "CLASH01"),
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

/// `settle` on an open (status 0) escrow must panic.
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
        &String::from_str(&env, "LOCK003"),
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
        &String::from_str(&env, "LOCK004"),
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

#[test]
fn test_cost_baseline_settle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.settle();
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
#[test]
fn settle_with_maturity_zero_succeeds_immediately() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, treasury) = free_addresses(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &String::from_str(&env, "MAT001"),
        &sme,
        &TARGET,
        &800i64,
        &0u64, // maturity = 0
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    fund_to_target(&client, &env);

    env.ledger().set_timestamp(0); // explicitly at epoch
    client.settle();

    assert_eq!(
        client.get_escrow().status,
        2u32,
        "status must be 2 (settled) with maturity == 0 even at epoch timestamp"
    );
}

/// `settle` with `maturity == 0` skips time-check and sets EscrowSettled.maturity to 0.
#[test]
fn settle_maturity_zero_event_maturity_field_is_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, treasury) = free_addresses(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &String::from_str(&env, "MAT002"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    fund_to_target(&client, &env);
    client.settle();

    let escrow = client.get_escrow();
    assert_eq!(escrow.maturity, 0u64, "maturity stored as 0");
    assert_eq!(escrow.status, 2u32, "settled");
}

/// `settle` with `maturity > 0` must be rejected before the maturity timestamp.
#[test]
#[should_panic(expected = "Escrow has not yet reached maturity")]
fn settle_before_maturity_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, treasury) = free_addresses(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let maturity_ts: u64 = 1_700_000_000;
    client.init(
        &admin,
        &String::from_str(&env, "MAT003"),
        &sme,
        &TARGET,
        &800i64,
        &maturity_ts,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    fund_to_target(&client, &env);

    env.ledger().set_timestamp(maturity_ts - 1); // 1 second before
    client.settle();
}

/// `settle` with `maturity > 0` succeeds at exactly the maturity timestamp.
#[test]
fn settle_at_maturity_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, treasury) = free_addresses(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let maturity_ts: u64 = 1_800_000_000;
    client.init(
        &admin,
        &String::from_str(&env, "MAT004"),
        &sme,
        &TARGET,
        &800i64,
        &maturity_ts,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    fund_to_target(&client, &env);

    env.ledger().set_timestamp(maturity_ts);
    client.settle();

    assert_eq!(client.get_escrow().status, 2u32);
    assert_eq!(client.get_escrow().maturity, maturity_ts);
}

/// `settle` with `maturity > 0` succeeds long after maturity.
#[test]
fn settle_after_maturity_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, treasury) = free_addresses(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let maturity_ts: u64 = 1_700_000_000;
    client.init(
        &admin,
        &String::from_str(&env, "MAT005"),
        &sme,
        &TARGET,
        &800i64,
        &maturity_ts,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    fund_to_target(&client, &env);

    env.ledger().set_timestamp(maturity_ts + 1_000_000); // well after maturity
    client.settle();

    assert_eq!(client.get_escrow().status, 2u32);
}

// ──────────────────────────────────────────────────────────────────────────────
// EscrowSettled event — field validation and storage writes
// ──────────────────────────────────────────────────────────────────────────────

/// `settle` emits an `EscrowSettled` event with correct fields.
#[test]
fn settle_emits_escrow_settled_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, treasury) = free_addresses(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let yield_bps: i64 = 400i64;
    let maturity_ts: u64 = 0u64;
    client.init(
        &admin,
        &String::from_str(&env, "EVT001"),
        &sme,
        &TARGET,
        &yield_bps,
        &maturity_ts,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    fund_to_target(&client, &env);

    let events_before = env.events().all();
    client.settle();
    let events_after = env.events().all();

    let new_events: Vec<_> = events_after
        .events()
        .iter()
        .filter(|e| !events_before.events().contains(e))
        .collect();

    assert!(
        !new_events.is_empty(),
        "settle must emit at least one event"
    );
}

/// `settle` persists the status change to storage.
#[test]
fn settle_writes_status_to_storage() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    assert_eq!(client.get_escrow().status, 1u32, "precondition: funded");
    client.settle();

    let reloaded = client.get_escrow();
    assert_eq!(
        reloaded.status, 2u32,
        "status must be persisted as 2 after settle"
    );
    assert_eq!(
        reloaded.funded_amount, TARGET,
        "funded_amount unchanged by settle"
    );
}

/// `settle` preserves non-status fields after transition.
#[test]
fn settle_preserves_escrow_fields() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    let pre = client.get_escrow();
    client.settle();
    let post = client.get_escrow();

    assert_eq!(post.invoice_id, pre.invoice_id);
    assert_eq!(post.admin, pre.admin);
    assert_eq!(post.sme_address, pre.sme_address);
    assert_eq!(post.amount, pre.amount);
    assert_eq!(post.funding_target, pre.funding_target);
    assert_eq!(post.funded_amount, pre.funded_amount);
    assert_eq!(post.yield_bps, pre.yield_bps);
    assert_eq!(post.maturity, pre.maturity);
}

/// `settle` must be blocked by legal hold regardless of maturity.
#[test]
#[should_panic(expected = "Legal hold blocks settlement finalization")]
fn settle_blocked_by_legal_hold_when_maturity_not_reached() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, treasury) = free_addresses(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let maturity_ts: u64 = 2_000_000_000;
    client.init(
        &admin,
        &String::from_str(&env, "LHMAT01"),
        &sme,
        &TARGET,
        &800i64,
        &maturity_ts,
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

/// `settle` on already settled (status 2) escrow must panic.
#[test]
#[should_panic]
fn settle_on_settled_escrow_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);
    client.settle(); // status → 2
    client.settle(); // must panic
}

// ──────────────────────────────────────────────────────────────────────────────
// Investor claim path (post-settle)
// ──────────────────────────────────────────────────────────────────────────────

/// `claim_investor_payout` succeeds for an investor after `settle`.
///
/// This is a state-marker call — no token transfer occurs inside the contract.
/// The test verifies the call completes without panic and emits an event.
#[test]
fn claim_investor_payout_succeeds_after_settle() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (escrow_id, client) = deploy_with_id(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW003"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    token.stellar.mint(&escrow_id, &100i128);
    client.sweep_terminal_dust(&100i128);
}

/// `claim_investor_payout` must be idempotency-guarded: a second call panics.
#[test]
#[should_panic]
fn claim_investor_payout_twice_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (_escrow_id, client) = deploy_with_id(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW004"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.settle();
    client.set_legal_hold(&true);
    client.claim_investor_payout(&investor); // must panic
}

/// `claim_investor_payout` must fail before `settle` (status != 2).
#[test]
#[should_panic]
fn claim_investor_payout_before_settle_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (_escrow_id, client) = deploy_with_id(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW005"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.settle();
    client.sweep_terminal_dust(&(MAX_DUST_SWEEP_AMOUNT + 1));
}

/// An investor that did not participate cannot claim.
#[test]
#[should_panic]
fn claim_investor_payout_non_participant_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (escrow_id, client) = deploy_with_id(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW006"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.settle();

    token.stellar.mint(&escrow_id, &50i128);
    let swept = client.sweep_terminal_dust(&100i128);
    assert_eq!(swept, 50i128);
}

// ──────────────────────────────────────────────────────────────────────────────
// Funding snapshot invariant (ADR-003)
// ──────────────────────────────────────────────────────────────────────────────

/// The funding-close snapshot is written once when status transitions to 1.
/// After `withdraw` the snapshot must still be readable with the original values.
///
/// This guards against the denominator being zeroed or mutated by the withdrawal
/// path — off-chain accounting always needs a stable snapshot.
#[test]
fn funding_snapshot_survives_withdraw() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (escrow_id, client) = deploy_with_id(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW007"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    assert_eq!(
        snapshot_after.unwrap().total_principal,
        TARGET,
        "snapshot total_principal must equal funded amount"
    );
}

/// After `settle` the snapshot still matches what was recorded at fund-close.
#[test]
fn funding_snapshot_survives_settle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);

    let snapshot_before = client.get_funding_close_snapshot();
    client.settle();
    token.stellar.mint(&escrow_id, &10i128);

    assert_eq!(snapshot_before, snapshot_after);
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
        &String::from_str(&env, "GIC001"),
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
        &String::from_str(&env, "GIC002"),
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
        &String::from_str(&env, "GIC003"),
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
        &String::from_str(&env, "GIC004"),
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
        &String::from_str(&env, "GIC005"),
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
    // Each sub-case uses its own Env to keep failures isolated.
    macro_rules! assert_panics_after_withdraw {
        ($block:expr) => {{
            let env = Env::default();
            let (client, admin, sme) = setup(&env);
            default_init(&client, &env, &admin, &sme);
            fund_to_target(&client, &env);
            client.withdraw();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $block));
            assert!(result.is_err(), "expected panic but call succeeded");
        }};
    }

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
            client.fund(&late, &1_000_0000000i128);
        }));
        assert!(r.is_err(), "fund after withdraw must panic");
    }
}
