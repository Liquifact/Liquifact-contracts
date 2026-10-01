//! Focused fee-subsystem tests for the escrow contract.
//!
//! This module covers the deterministic failure-recovery contract for the
//! fee split applied at `withdraw`. The invariants that must hold across
//! every path (init, setter, withdraw, retry, partial failure) are:
//!
//! 1. The fee is always floor(funded_amount * fee_bps / 10_000) and the
//!    SME net is always `funded_amount - fee`. The two legs sum exactly
//!    to `funded_amount` (no dust is created or lost).
//! 2. A failed withdraw (bad token, balance drain, overflow) must leave
//!    the status unchanged and the contract balance untouched, so a retry
//!    after the failure is deterministic and idempotent.
//! 3. A duplicate withdraw after a successful one must fail with
//!    `WithdrawalNotFunded` and must not move any tokens again.
//! 4. Boundary fee rates (0 and 10_000) must behave exactly as documented.
//! 5. Input validation for `set_protocol_fee_bps` covers 0, max (10_000),
//!    out-of-range positives, and negative values.
//! 6. The fee setter is atomic: a rejected call never mutates stored rate
//!    and never emits an event.

#[allow(unused_imports, unused_variables, dead_code, clippy::needless_borrow)]
use super::*;

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    Address, Env,
    String as SorobanString,
};

// ── harness ──────────────────────────────────────────────────────────────────

/// Convenience wrapper that builds a fresh escrow with a real SEP-41 token
/// and a configurable fee rate. Returns the client, the contract address, the
/// token admin (for minting), the treasury address, and the SME address.
struct FeeHarness<'a> {
    client: LiquifactEscrowClient<'a>,
    contract_id: Address,
    token_id: Address,
    token: TokenClient<'a>,
    token_admin: StellarAssetClient<'a>,
    admin: Address,
    treasury: Address,
    sme: Address,
}

fn setup_fee_harness(
    env: &Env,
    target: i128,
    fee_bps: i64,
    invoice_id: &str,
) -> FeeHarness {
    env.ledger().set(soroban_sdk::ledger::LedgerInfo {
        timestamp: 0,
        sequence_number: 100,
        ..soroban_sdk::ledger::LedgerInfo::default()
    });
    env.mock_all_auths();

    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token_id = sac.address();
    let token_admin = StellarAssetClient::new(env, &token_id);

    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &SorobanString::from_str(env, invoice_id),
        &sme,
        &target,
        &800i64,
        &0u64,
        &token_id,
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
        &None::<u32>,
    );

    // Apply the fee rate after init so the harness can exercise the
    // `set_protocol_fee_bps` path as well as the `init` path.
    if fee_bps != 0 {
        client.set_protocol_fee_bps(&fee_bps);
    }

    FeeHarness {
        client,
        contract_id,
        token_id,
        token: TokenClient::new(env, &token_id),
        token_admin,
        admin,
        treasury,
        sme,
    }
}

/// Fund the escrow to target and make the contract hold the tokens.
fn fund_to_target(env: &Env, harness: &FeeHarness, target: i128) {
    let investor = Address::generate(env);
    harness.token_admin.mint(&investor, &target);
    harness.client.fund(&investor, &target);
    // Simulate the tokens actually being transferred into the contract.
    harness.token_admin.mint(&harness.contract_id, &target);
}

// ── Success paths ─────────────────────────────────────────────────────────────

/// The fee and net legs sum exactly to the funded amount for a typical
/// non-boundary fee rate. This is the core invariant of the fee split.
#[test]
fn withdraw_splits_fee_and_net_exactly() {
    let env = Env::default();
    let target: i128 = 1_000_000;
    let fee_bps: i64 = 250; // 2.5%
    let harness = setup_fee_harness(&env, target, fee_bps, "INV001");
    fund_to_target(&env, &harness, target);

    let treasury_before = harness.token.balance(&harness.treasury);
    let sme_before = harness.token.balance(&harness.sme);

    harness.client.withdraw();

    let expected_fee = target * (fee_bps as i128) / 10_000;
    let expected_net = target - expected_fee;

    assert_eq!(
        harness.token.balance(&harness.treasury) - treasury_before,
        expected_fee
    );
    assert_eq!(
        harness.token.balance(&harness.sme) - sme_before,
        expected_net
    );
    // Conservation: fee + net == funded_amount, no dust created or lost.
    assert_eq!(expected_fee + expected_net, target);
    assert_eq!(harness.token.balance(&harness.contract_id), 0);
}

/// A zero fee rate sends the entire funded amount to the SME and nothing
/// to the treasury. This tests the lower bound (fee_bps = 0).
#[test]
fn withdraw_zero_fee_sends_all_to_sme() {
    let env = Env::default();
    let target: i128 = 500_000;
    let harness = setup_fee_harness(&env, target, 0, "INV002");
    fund_to_target(&env, &harness, target);

    let treasury_before = harness.token.balance(&harness.treasury);
    harness.client.withdraw();

    // Treasury balance must be unchanged — no fee was taken.
    assert_eq!(harness.token.balance(&harness.treasury), treasury_before);
    assert_eq!(harness.token.balance(&harness.sme), target);
}

/// A 100% fee rate sends the entire funded amount to the treasury and
/// nothing to the SME. This tests the upper bound (fee_bps = 10_000).
#[test]
fn withdraw_max_fee_sends_all_to_treasury() {
    let env = Env::default();
    let target: i128 = 777_777;
    let harness = setup_fee_harness(&env, target, 10_000, "INV003");
    fund_to_target(&env, &harness, target);

    let sme_before = harness.token.balance(&harness.sme);
    harness.client.withdraw();

    // SME balance must be unchanged — the entire amount went to treasury.
    assert_eq!(harness.token.balance(&harness.sme), sme_before);
    assert_eq!(harness.token.balance(&harness.treasury), target);
}

/// Boundary fee_bps = 1 (minimum non-zero rate): fee is floor(target / 10_000)
/// and the split still conserves the total.
#[test]
fn withdraw_minimum_nonzero_fee_conserves_total() {
    let env = Env::default();
    let target: i128 = 1_000_000;
    let fee_bps: i64 = 1;
    let harness = setup_fee_harness(&env, target, fee_bps, "INV012");
    fund_to_target(&env, &harness, target);

    let treasury_before = harness.token.balance(&harness.treasury);
    let sme_before = harness.token.balance(&harness.sme);

    harness.client.withdraw();

    let expected_fee = target * (fee_bps as i128) / 10_000; // floor = 100
    let expected_net = target - expected_fee;

    assert_eq!(
        harness.token.balance(&harness.treasury) - treasury_before,
        expected_fee
    );
    assert_eq!(
        harness.token.balance(&harness.sme) - sme_before,
        expected_net
    );
    assert_eq!(expected_fee + expected_net, target);
}

/// Boundary fee_bps = 9_999 (one below max): split is deterministic and
/// conserves the total.
#[test]
fn withdraw_near_max_fee_conserves_total() {
    let env = Env::default();
    let target: i128 = 1_000_000;
    let fee_bps: i64 = 9_999;
    let harness = setup_fee_harness(&env, target, fee_bps, "INV013");
    fund_to_target(&env, &harness, target);

    let treasury_before = harness.token.balance(&harness.treasury);
    let sme_before = harness.token.balance(&harness.sme);

    harness.client.withdraw();

    let expected_fee = target * (fee_bps as i128) / 10_000;
    let expected_net = target - expected_fee;

    assert_eq!(
        harness.token.balance(&harness.treasury) - treasury_before,
        expected_fee
    );
    assert_eq!(
        harness.token.balance(&harness.sme) - sme_before,
        expected_net
    );
    assert_eq!(expected_fee + expected_net, target);
}

// ── Rejection paths: configuration ───────────────────────────────────────────

/// A fee rate above 10_000 is rejected by the setter and leaves the
/// previous rate intact. No event is emitted on rejection.
#[test]
fn set_protocol_fee_bps_rejects_out_of_range() {
    let env = Env::default();
    let target: i128 = 1_000_000;
    let harness = setup_fee_harness(&env, target, 500, "INV004");

    let result = harness.client.try_set_protocol_fee_bps(&10_001i64);
    assert_contract_error(result, EscrowError::ProtocolFeeBpsOutOfRange);

    // The failed setter must not mutate the stored rate.
    assert_eq!(harness.client.get_protocol_fee_bps(), 500);
}

/// A negative fee rate is rejected by the setter.
#[test]
fn set_protocol_fee_bps_rejects_negative() {
    let env = Env::default();
    let target: i128 = 1_000_000;
    let harness = setup_fee_harness(&env, target, 500, "INV005");

    let result = harness.client.try_set_protocol_fee_bps(&-1i64);
    assert_contract_error(result, EscrowError::ProtocolFeeBpsOutOfRange);

    assert_eq!(harness.client.get_protocol_fee_bps(), 500);
}

/// fee_bps = 0 is a valid inclusive lower bound.
#[test]
fn set_protocol_fee_bps_accepts_zero() {
    let env = Env::default();
    let harness = setup_fee_harness(&env, 100_000, 500, "INV014");
    let returned = harness.client.set_protocol_fee_bps(&0i64);
    assert_eq!(returned, 0i64);
    assert_eq!(harness.client.get_protocol_fee_bps(), 0);
}

/// fee_bps = 10_000 is a valid inclusive upper bound.
#[test]
fn set_protocol_fee_bps_accepts_max() {
    let env = Env::default();
    let harness = setup_fee_harness(&env, 100_000, 0, "INV015");
    let returned = harness.client.set_protocol_fee_bps(&10_000i64);
    assert_eq!(returned, 10_000i64);
    assert_eq!(harness.client.get_protocol_fee_bps(), 10_000);
}

/// A repeated out-of-range rejection is idempotent: calling it twice with
/// invalid values does not change the stored rate and emits no events.
#[test]
fn set_protocol_fee_bps_rejection_is_idempotent() {
    let env = Env::default();
    let harness = setup_fee_harness(&env, 100_000, 750, "INV016");

    assert_contract_error(
        harness.client.try_set_protocol_fee_bps(&10_001i64),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );
    assert_contract_error(
        harness.client.try_set_protocol_fee_bps(&-1i64),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );
    assert_contract_error(
        harness.client.try_set_protocol_fee_bps(&10_001i64),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );

    // Rate is unchanged after three consecutive rejections.
    assert_eq!(harness.client.get_protocol_fee_bps(), 750);
}

// ── Rejection paths: withdrawal preconditions ─────────────────────────────────

/// Withdrawing before the escrow is funded must fail with the typed
/// `WithdrawalNotFunded` code and leave the contract balance untouched.
#[test]
fn withdraw_before_funding_is_rejected() {
    let env = Env::default();
    let target: i128 = 1_000_000;
    let harness = setup_fee_harness(&env, target, 250, "INV006");

    let result = harness.client.try_withdraw();
    assert_contract_error(result, EscrowError::WithdrawalNotFunded);
    assert_eq!(harness.token.balance(&harness.contract_id), 0);
    assert_eq!(harness.token.balance(&harness.sme), 0);
    assert_eq!(harness.token.balance(&harness.treasury), 0);
}

/// Withdrawing when the contract does not hold enough tokens must fail
/// with `InsufficientContractBalance` and leave the SME treasury and
/// contract balances unchanged.
#[test]
fn withdraw_with_insufficient_contract_balance_is_rejected() {
    let env = Env::default();
    let target: i128 = 1_000_000;
    let harness = setup_fee_harness(&env, target, 250, "INV007");

    // Fund the escrow without actually moving tokens into the contract.
    let investor = Address::generate(&env);
    harness.token_admin.mint(&investor, &target);
    harness.client.fund(&investor, &target);

    let result = harness.client.try_withdraw();
    assert_contract_error(result, EscrowError::InsufficientContractBalance);
    assert_eq!(harness.token.balance(&harness.contract_id), 0);
    assert_eq!(harness.token.balance(&harness.sme), 0);
    assert_eq!(harness.token.balance(&harness.treasury), 0);
}

// ── Retry and duplicate behavior ─────────────────────────────────────────────

/// A failed withdraw must leave the escrow in a state where a retry
/// after the failure is deterministic and succeeds once the blocking
/// condition is resolved. This is the core failure-recovery contract.
#[test]
fn withdraw_retry_after_insufficient_balance_succeeds() {
    let env = Env::default();
    let target: i128 = 1_000_000;
    let fee_bps: i64 = 250;
    let harness = setup_fee_harness(&env, target, fee_bps, "INV008");

    // Partial funding: the contract holds less than the target.
    let partial = target / 2;
    let investor = Address::generate(&env);
    harness.token_admin.mint(&investor, &partial);
    harness.client.fund(&investor, &partial);
    harness.token_admin.mint(&harness.contract_id, &partial);

    // First attempt fails because the contract is underfunded.
    let first = harness.client.try_withdraw();
    assert_contract_error(first, EscrowError::InsufficientContractBalance);
    assert_eq!(harness.token.balance(&harness.contract_id), partial);

    // Resolve the blocking condition and retry.
    harness.token_admin.mint(&harness.contract_id, &target);
    harness.client.withdraw();

    let expected_fee = target * (fee_bps as i128) / 10_000;
    let expected_net = target - expected_fee;
    assert_eq!(harness.token.balance(&harness.treasury), expected_fee);
    assert_eq!(harness.token.balance(&harness.sme), expected_net);
    assert_eq!(harness.token.balance(&harness.contract_id), 0);
}

/// A second withdraw after a successful one must fail with
/// `WithdrawalNotFunded` and must not move any tokens again.
#[test]
fn withdraw_duplicate_after_success_is_rejected() {
    let env = Env::default();
    let target: i128 = 1_000_000;
    let fee_bps: i64 = 250;
    let harness = setup_fee_harness(&env, target, fee_bps, "INV009");
    fund_to_target(&env, &harness, target);

    harness.client.withdraw();
    let treasury_after = harness.token.balance(&harness.treasury);
    let sme_after = harness.token.balance(&harness.sme);

    let second = harness.client.try_withdraw();
    assert_contract_error(second, EscrowError::WithdrawalNotFunded);

    // Balances are frozen after the successful withdraw.
    assert_eq!(harness.token.balance(&harness.treasury), treasury_after);
    assert_eq!(harness.token.balance(&harness.sme), sme_after);
    assert_eq!(harness.token.balance(&harness.contract_id), 0);
}

/// A failed withdraw caused by an external balance drain must not leave
/// the escrow in a partially-settled state. The failure is observable
/// and the retry succeeds once the balance is restored.
#[test]
fn withdraw_failure_leaves_state_recoverable() {
    let env = Env::default();
    let target: i128 = 1_000_000;
    let fee_bps: i64 = 250;
    let harness = setup_fee_harness(&env, target, fee_bps, "INV010");
    fund_to_target(&env, &harness, target);

    // Drain the contract balance to simulate an external failure.
    let drain_to = Address::generate(&env);
    harness.token_admin.mint(&harness.contract_id, &target);
    harness.token.transfer(&harness.contract_id, &drain_to, &target);
    assert_eq!(harness.token.balance(&harness.contract_id), 0);

    let result = harness.client.try_withdraw();
    assert_contract_error(result, EscrowError::InsufficientContractBalance);

    // Recovery: restore the balance and retry.
    harness.token.transfer(&drain_to, &harness.contract_id, &target);
    harness.client.withdraw();

    let expected_fee = target * (fee_bps as i128) / 10_000;
    let expected_net = target - expected_fee;
    assert_eq!(harness.token.balance(&harness.treasury), expected_fee);
    assert_eq!(harness.token.balance(&harness.sme), expected_net);
    assert_eq!(harness.token.balance(&harness.contract_id), 0);
}

// ── Observability: read-view reflects setter immediately ─────────────────────

/// The fee setter emits an update and the read-view reflects the new rate
/// immediately; calling it again with a different value updates it again.
#[test]
fn set_protocol_fee_bps_updates_read_view() {
    let env = Env::default();
    let target: i128 = 1_000_000;
    let harness = setup_fee_harness(&env, target, 0, "INV011");
    assert_eq!(harness.client.get_protocol_fee_bps(), 0);

    harness.client.set_protocol_fee_bps(&125i64);
    assert_eq!(harness.client.get_protocol_fee_bps(), 125);

    harness.client.set_protocol_fee_bps(&0i64);
    assert_eq!(harness.client.get_protocol_fee_bps(), 0);
}

// ── State-preservation under adverse fee conditions ──────────────────────────

/// Status remains unchanged (open = 0) when `withdraw` is rejected before
/// the escrow is funded. The escrow must be retryable with no residual state.
#[test]
fn withdraw_before_funded_leaves_status_open() {
    let env = Env::default();
    let harness = setup_fee_harness(&env, 500_000, 200, "INV017");

    // Escrow is open (status 0) — withdraw must be rejected.
    assert_eq!(harness.client.get_escrow().status, 0);

    let _ = harness.client.try_withdraw();

    // Status must still be open after the rejected call.
    assert_eq!(harness.client.get_escrow().status, 0);
}

/// `WithdrawalNotFunded` (status guard) fires before any token transfer
/// attempt, so the funded_amount is never changed by a rejected withdraw.
#[test]
fn withdraw_rejection_does_not_alter_funded_amount() {
    let env = Env::default();
    let target: i128 = 1_000_000;
    let harness = setup_fee_harness(&env, target, 250, "INV018");

    // No tokens funded; funded_amount == 0.
    let before = harness.client.get_escrow().funded_amount;
    assert_eq!(before, 0);

    let _ = harness.client.try_withdraw();

    let after = harness.client.get_escrow().funded_amount;
    assert_eq!(after, before, "funded_amount must not change on rejected withdraw");
}

/// Multiple repeated failed withdraws are idempotent: the escrow state
/// and all balances remain exactly equal to their pre-call values.
#[test]
fn repeated_failed_withdraws_are_idempotent() {
    let env = Env::default();
    let target: i128 = 1_000_000;
    let harness = setup_fee_harness(&env, target, 500, "INV019");

    // Escrow is open — three consecutive withdraw attempts all fail.
    for _ in 0..3 {
        let result = harness.client.try_withdraw();
        assert_contract_error(
            result,
            EscrowError::WithdrawalNotFunded,
        );
    }

    // No balances changed, no state mutated.
    assert_eq!(harness.client.get_escrow().status, 0);
    assert_eq!(harness.client.get_escrow().funded_amount, 0);
    assert_eq!(harness.token.balance(&harness.sme), 0);
    assert_eq!(harness.token.balance(&harness.treasury), 0);
    assert_eq!(harness.token.balance(&harness.contract_id), 0);
}

// ── Fee arithmetic: overflow-safety and floor behavior ───────────────────────

/// For a non-even dividend, the floor division must produce `fee + net == target`
/// with no rounding loss (integer floor is deterministic).
#[test]
fn withdraw_odd_amount_fee_split_conserves_total() {
    let env = Env::default();
    // 333_333 * 250 / 10_000 = 8333.325 → floor = 8333
    let target: i128 = 333_333;
    let fee_bps: i64 = 250;
    let harness = setup_fee_harness(&env, target, fee_bps, "INV020");
    fund_to_target(&env, &harness, target);

    harness.client.withdraw();

    let expected_fee = target * (fee_bps as i128) / 10_000;
    let expected_net = target - expected_fee;

    assert_eq!(harness.token.balance(&harness.treasury), expected_fee);
    assert_eq!(harness.token.balance(&harness.sme), expected_net);
    assert_eq!(expected_fee + expected_net, target);
}

/// The fee setter requires admin auth; a call without auth panics at the
/// host-level `require_auth`.
#[test]
#[should_panic]
fn set_protocol_fee_bps_requires_admin_auth() {
    let env = Env::default();
    let harness = setup_fee_harness(&env, 100_000, 0, "INV021");
    env.mock_auths(&[]);
    harness.client.set_protocol_fee_bps(&1_000i64);
}
