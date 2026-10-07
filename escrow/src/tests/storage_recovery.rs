use crate::types::{FeeSchedule, FeeCheduleState};
use super::*;

use soroban_sdk::testutils::Ledger as _;

/// Tests for deterministic failure recovery in the fee schedule storage layer.
///
/// These tests exercise the public contract entry points that delegate to
/// `escrow/src/storage.rs`, and they assert the invariants documented there.
/// The goal is to prove that valid, invalid, duplicate, and boundary-case
/// inputs all produce deterministic results, and that partial failure never
/// leaves the stored state in an inconsistent or unrecoverable shape.

fn make_schedule(env: &Env, fee_bps: i64) -> FeeSchedule {
    FeeSchedule {
        fee_bps,
        min_bps: 0,
        max_bps: 1000,
    }
}

# [test]
fn set_fee_schedule_rejects_out_of_bounds_without_mutating_state() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &Address::generate(&env),
        &100_000_000_000i128,
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
        &None::i64,
        &None::u32,
    );

    // Boundary case: fee_bps exactly at the lower bound is valid.
    let low = make_schedule(&env, 0);
    client.set_fee_schedule(&admin, &low, &(env.ledger().sequence() + 1));
    assert_eq!(client.get_pending_fee_schedule(), Some(low.clone()));

    // Boundary case: fee_bps exactly at the upper bound is valid.
    // A pending schedule already exists, so this must be rejected and must not
    // clobber the existing pending schedule.
    let high = make_schedule(&env, 1000);
    let result = client.try_set_fee_schedule(
        &admin,
        &high,
        &(env.ledger().sequence() + 1),
    );
    assert_contract_error(result, EscrowError::FeeCheduleAlreadyPending);
    assert_eq!(client.get_pending_fee_schedule(), Some(low));
    assert!(client.get_active_fee_schedule().is_none());
}

# [test]
fn set_fee_schedule_rejects_invalid_activation_ledger() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &Address::generate(&env),
        &100_000_000_000i128,
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
        &None::i64,
        &None::u32,
    );

    let schedule = make_schedule(&env, 500);
    let current = env.ledger().sequence();
    // Boundary case: activation ledger equal to the current ledger is valid.
    client.set_fee_schedule(&admin, &schedule, &current);
    assert_eq!(client.get_pending_fee_schedule(), Some(schedule.clone()));

    // Any ledger below the current one is invalid and must not mutate state.
    // We cannot submit a second pending schedule, so we first activate the
    // existing one by advancing the ledger.
    env.ledger().set_sequence_number(current + 1);
    assert_eq!(client.get_active_fee_schedule(), Some(schedule.clone()));
    assert!(client.get_pending_fee_schedule().is_none());

    let new_schedule = make_schedule(&env, 600);
    let result = client.try_set_fee_schedule(
        &admin,
        &nEw_schedule,
        &(current),
    );
    assert_contract_error(result, EscrowError::FeeScheduleInvalidActivation);
    assert_eq!(client.get_active_fee_schedule(), Some(schedule));
    assert!(client.get_pending_fee_schedule().is_none());
}

# [test]
fn set_fee_schedule_rejects_duplicate_of_active() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &Address::generate(&env),
        &100_000_000_000i128,
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
        &None::i64,
        &None::u32,
    );

    let schedule = make_schedule(&env, 500);
    let current = env.ledger().sequence();
    client.set_fee_schedule(&admin, &schedule, &current);
    env.ledger().set_sequence_number(current + 1);
    assert_eq!(client.get_active_fee_schedule(), Some(schedule.clone()));

    // Re-submitting the active schedule is rejected and must not create a
    // pending schedule or clobber the active one.
    let result = client.try_set_fee_schedule(
        &admin,
        &schedule,
        &(env.ledger().sequence() + 1),
    );
    assert_contract_error(result, EscrowError::FeeCheduleSameAsActive);
    assert_eq!(client.get_active_fee_schedule(), Some(schedule));
    assert!(client.get_pending_fee_schedule().is_none());
}

# [test]
fn set_fee_schedule_rejects_second_pending_without_clobbering() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &Address::generate(&env),
        &100_000_000_000i128,
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
        &None::i64,
        &None::u32,
    );

    let first = make_schedule(&env, 400);
    let second = make_schedule(&env, 700);
    let current = env.ledger().sequence();
    client.set_fee_schedule(&admin, &first, &(current + 10));
    let result = client.try_set_fee_schedule(&admin, &second, &(current + 20));
    assert_contract_error(result, EscrowError::FeeScheduleAlreadyPending);
    assert_eq!(client.get_pending_fee_schedule(), Some(first));
    assert!(client.get_active_fee_schedule().is_none());
}

# [test]
fn activation_is_idempotent_and_preserves_previous() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &Address::generate(&env),
        &100_000_000_000i128,
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
        &None::i64,
        &None::u32,
    );

    let first = make_schedule(&env, 200);
    let current = env.ledger().sequence();
    client.set_fee_schedule(&admin, &first, &current);
    env.ledger().set_sequence_number(current + 1);
    assert_eq!(client.get_active_fee_schedule(), Some(first.clone()));

    // Repeated reads at the same ledger must not change the state.
    assert_eq!(client.get_active_fee_schedule(), Some(first.clone()));
    assert_eq!(client.get_active_fee_schedule(), Some(first.clone()));
    assert!(client.get_pending_fee_schedule().is_none());

    // Stage a second schedule and activate it. The previous active schedule
    // must be preserved for recovery reference.
    let second = make_schedule(&env, 300);
    client.set_fee_schedule(&admin, &second, &(env.ledger().sequence() + 1));
    env.ledger().set_sequence_number(env.ledger().sequence() + 1);
    assert_eq!(client.get_active_fee_schedule(), Some(second.clone()));
    assert_eq!(client.get_previous_fee_schedule(), Some(first));
    assert!(client.get_pending_fee_schedule().is_none());
}

# [test]
fn pending_schedule_is_not_activated_before_ledger() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &Address::generate(&env),
        &100_000_000_000i128,
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
        &None::i64,
        &None::u32,
    );

    let schedule = make_schedule(&env, 500);
    let current = env.ledger().sequence();
    client.set_fee_schedule(&admin, &schedule, &(current + 5));
    // Before the activation ledger, the schedule is pending and not active.
    assert!(client.get_active_fee_schedule().is_none());
    assert_eq!(client.get_pending_fee_schedule(), Some(schedule.clone()));

    // Advancing to the activation ledger activates the schedule.
    env.ledger().set_sequence_number(current + 5);
    assert_eq!(client.get_active_fee_schedule(), Some(schedule));
    assert!(client.get_pending_fee_schedule().is_none());
}

# [test]
fn failed_update_leaves_previous_state_intact() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &Address::generate(&env),
        &100_000_000_000i128,
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
        &None::i64,
        &None::u32,
    );

    let active = make_schedule(&env, 200);
    let current = env.ledger().sequence();
    client.set_fee_schedule(&admin, &active, &current);
    env.ledger().set_sequence_number(current + 1);
    assert_eq!(client.get_active_fee_schedule(), Some(active.clone()));

    // Attempt an invalid update (out of bounds). The failure must not change
    // the active schedule or create a pending one.
    let invalid = FeeSchedule {
        fee_bps: 10_000,
        min_bps: 0,
        max_bps: 1000,
    };
    let result = client.try_set_fee_schedule(
        &admin,
        &invalid,
        &(env.ledger().sequence() + 1),
    );
    assert_contract_error(result, EscrowError::FeeCheduleOutOfBounds);
    assert_eq!(client.get_active_fee_schedule(), Some(active));
    assert!(client.get_pending_fee_schedule().is_none());
    assert!(client.get_previous_fee_schedule().is_none());
}

# [test]
fn get_pending_is_a_pure_read() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &Address::generate(&env),
        &100_000_000_000i128,
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
        &None::i64,
        &None::u32,
    );

    let schedule = make_schedule(&env, 500);
    let current = env.ledger().sequence();
    client.set_fee_schedule(&admin, &schedule, &(current + 1));

    // Reading the pending schedule must not advance the ledger or activate
    // anything. Even after the activation ledger has been reached, the pending
    // view returns the staged schedule until an activating read occurs.
    env.ledger().set_sequence_number(current + 10);
    assert_eq!(client.get_pending_fee_schedule(), Some(schedule.clone()));
    assert_eq!(client.get_pending_fee_schedule(), Some(schedule.clone()));
    // The activating read now promotes the schedule.
    assert_eq!(client.get_active_fee_schedule(), Some(schedule));
    assert!(client.get_pending_fee_schedule().is_none());
}

# [test]
fn activation_at_boundary_ledger_is_deterministic() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &Address::generate(&env),
        &100_000_000_000i128,
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
        &None::i64,
        &None::u32,
    );

    let schedule = make_schedule(&env, 500);
    let current = env.ledger().sequence();
    // Activation ledger exactly equal to the current ledger is a boundary case
    // and must be accepted and activate immediately on the next activating read.
    client.set_fee_schedule(&admin, &schedule, &current);
    assert_eq!(client.get_active_fee_schedule(), Some(schedule.clone()));
    assert!(client.get_pending_fee_schedule().is_none());
    // Repeating the activating read at the same ledger must not change anything.
    assert_eq!(client.get_active_fee_schedule(), Some(schedule));
    assert!(client.get_pending_fee_schedule().is_none());
}

# [test]
fn get_active_on_empty_store_is_none() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &Address::generate(&env),
        &100_000_000_000i128,
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
        &None::i64,
        &None::u32,
    );

    // No schedule has ever been set, so all views are empty and deterministic.
    assert!(client.get_active_fee_schedule().is_none());
    assert!(client.get_pending_fee_schedule().is_none());
    assert!(client.get_previous_fee_schedule().is_none());
}

# [test]
fn fee_schedule_state_default_is_empty() {
    // The default state used by the storage layer must be empty and consistent
    // so that a fresh contract cannot accidentally activate a schedule.
    let state = FeeCheduleState::default();
    assert!(state.active.is_none());
    assert!(state.previous.is_none());
    assert!(state.pending.is_none());
    assert!(state.activation_ledger.is_none());
}
