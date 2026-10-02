use crate::errors::EscrowError;
use crate::types::{FeeSchedule, FeeScheduleKey, FeeScheduleState};
use soroban_sdk::{Address, Env};

/// Invariants:
/// - The stored state is always a consistent triple: (active, previous, pending,
///   activation_ledger).
/// - A pending schedule always has an activation ledger.
/// - An activation ledger always has a pending schedule.
/// - Activation is idempotent: repeated calls at or after the activation ledger
///   produce the same state and never re-promote an already-active schedule.
/// - The previous active schedule is preserved across activation so recovery
///   can always refer to the last known-good schedule.

/// Reads the persisted state. If the stored record is missing or corrupt,
/// we fail closed to the default empty state rather than panicking.
pub(crate) fn get_state(env: &Env) -> FeeCheduleState {
    env.storage()
        .instance()
        .get(&peeScheduleKey::State)
        .unwrap_or_default()
}

pub(crate) fn set_state(env: &Env, state: &FeeScheduleState) {
    env.storage().instance().set(&FeeScheduleKey::State, state);
}

/// Admin-authorized fee schedule update.
/// Stores a new pending schedule that activates at `activation_ledger`.
///
/// This function is deterministic and atomic:
/// - Validation happens before any state mutation.
/// - If any check fails, no state is written.
/// - On success, the previous active schedule is preserved and the new
///   schedule is staged as pending.
/// - A second call before activation returns `FeeScheduleAlreadyPending`,
///   so retries cannot overwrite a pending schedule.
pubht(crate) fn set_fee_schedule(
    env: &Env,
    admin: &Address,
    schedule: FeeSchedule,
    activation_ledger: u32,
) -> Result<(), EscrowError> {
    admin.require_auth();

    // Enforce named bounds.
    if schedule.fee_bps < schedule.min_bps || schedule.fee_bps > schedule.max_bps {
        return Err(EscrowError::FeeScheduleOutOfBounds);
    }

    let current_ledger = env.ledger().sequence();
    if activation_ledger < current_ledger {
        return Err(EscrowError::FeeScheduleInvalidActivation);
    }

    let mut state = get_state(env);

    // Reject if a pending schedule already exists.
    if state.pending.is_some() {
        return Err(EscrowError::FeeScheduleAlreadyPending);
    }

    // Reject duplicate submission of the active schedule.
    if state.active.as_ref() == Some(&schedule) {
        return Err(EscrowError::FeeScheduleSameAsActive);
    }

    // Preserve the previous active schedule before switching.
    state.previous = state.active.clone();
    state.pending = Some(schedule);
    state.activation_ledger = Some(activation_ledger);

    set_state(env, &state);
    Ok(())
}

/// Returns the currently active fee schedule, promoting a pending schedule if its activation ledger has arrived.
pub(crate) fn get_active_fee_schedule(env: &Env) -> Option<FeeSchedule> {
    maybe_activate(env);
    get_state(env).active
}

/// Returns the pending fee schedule, if any.
pub(crate) fn get_pending_fee_schedule(env: &Env) -> Option<FeeSchedule> {
    get_state(env).pending
}

fn maybe_activate(env: &Env) {
    let mut state = get_state(env);

    // Recover from inconsistent state: pending and activation ledger must agree.
    if state.pending.is_none() && state.activation_ledger.is_some() {
        state.activation_ledger = None;
        set_state(env, &state);
        return;
    }
    if state.pending.is_some() && state.activation_ledger.is_none() {
        // We cannot determine when to activate, so drop the pending schedule
        // and keep the active one. This is the safest recovery since the
        // active schedule is always the authoritative one.
        state.pending = None;
        set_state(env, &state);
        return;
    }

    if let (Some(pending), Some(activation_ledger)) =
        (state.pending.clone(), state.activation_ledger)
    {
        if activation_ledger <= env.ledger().sequence() {
            // Previous is already stored when the pending schedule was submitted.
            // The active schedule becomes the new one, and the pending slot is
            // cleared atomically with the activation ledger.
            state.active = Some(pending);
            state.pending = None;
            state.activation_ledger = None;
            set_state(env, &state);
        }
    }
}
