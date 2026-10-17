use crate::{FeeSchedule, FeeScheduleStorageKey};
use soroban_sdk::contracterror;
use soroban_sdk::Env;

/// A transaction-local view of fee-schedule state.
///
/// The three values remain in their existing storage keys for compatibility.
/// Callers stage transitions here and persist once, so a failed or competing
/// invocation cannot expose a partially promoted schedule.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FeeScheduleState {
    pub(crate) active: Option<FeeSchedule>,
    pub(crate) pending: Option<FeeSchedule>,
    pub(crate) previous: Option<FeeSchedule>,
}

/// Errors raised when the in-memory fee-schedule state violates an invariant.
///
/// These are surfaced before any storage write so a caller can never persist a
/// partially promoted or otherwise inconsistent schedule. The variants are
/// intentionally coarse so they do not leak schedule contents.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FeeScheduleStateError {
    /// A pending schedule exists but its activation ledger is not in the future
    /// relative to the active schedule, so it can never be "pending" again.
    PendingNotInFuture = 1,
    /// The pending schedule is due but promotion was not performed first.
    PendingDueNotPromoted = 2,
    /// The previous schedule duplicates the active schedule; history is corrupt.
    PreviousEqualsActive = 3,
}

/// Invariants owned by `FeeScheduleState`:
///
/// 1. `pending`, when present, must have `activation_ledger` strictly greater
///    than the active schedule's `activation_ledger` (or any value if no active
///    schedule exists). This guarantees `pending_at`/`active_at` are disjoint.
/// 2. Once `activate_if_due` returns `true`, `pending` must be `None` and the
///    promoted schedule must be `active` with the old active in `previous`.
/// 3. `previous` must never equal `active`; otherwise rollback semantics are
///    ambiguous and a caller could silently lose the prior schedule.
///
/// `validate` enforces these before `persist` so a failed or competing
/// invocation cannot expose a partially promoted schedule.
impl FeeScheduleState {
    pub(crate) fn load(env: &Env) -> Self {
        let storage = env.storage().instance();
        Self {
            active: storage.get(&FeeScheduleStorageKey::Active),
            pending: storage.get(&FeeScheduleStorageKey::Pending),
            previous: storage.get(&FeeScheduleStorageKey::Previous),
        }
    }

    /// Check the invariants documented on `FeeScheduleState`.
    ///
    /// Returns `Err` if the in-memory state is inconsistent. Callers must treat
    /// this as a hard failure and must not call `persist` afterwards.
    pub(crate) fn validate(&self, current_ledger: u32) -> Result<(), FeeScheduleStateError> {
        if let Some(pending) = self.pending.as_ref() {
            if let Some(active) = self.active.as_ref() {
                if pending.activation_ledger <= active.activation_ledger {
                    return Err(FeeScheduleStateError::PendingNotInFuture);
                }
            }
            if pending.activation_ledger <= current_ledger {
                return Err(FeeScheduleStateError::PendingDueNotPromoted);
            }
        }
        if let (Some(previous), Some(active)) = (self.previous.as_ref(), self.active.as_ref()) {
            if previous == active {
                return Err(FeeScheduleStateError::PreviousEqualsActive);
            }
        }
        Ok(())
    }

    /// Promote a due pending schedule in memory. The caller persists only after
    /// all validation succeeds, keeping activation and subsequent writes atomic.
    pub(crate) fn activate_if_due(&mut self, current_ledger: u32) -> bool {
        let is_due = self
            .pending
            .as_ref()
            .map(|schedule| schedule.activation_ledger <= current_ledger)
            .unwrap_or(false);
        if !is_due {
            return false;
        }

        self.previous = self.active.take();
        self.active = self.pending.take();
        // Invariant: after promotion, previous must differ from the new active.
        if let (Some(previous), Some(active)) = (self.previous.as_ref(), self.active.as_ref()) {
            debug_assert!(previous != active);
        }
        true
    }

    pub(crate) fn active_at(&self, current_ledger: u32) -> Option<FeeSchedule> {
        match self.pending.as_ref() {
            Some(pending) if pending.activation_ledger <= current_ledger => Some(pending.clone()),
            _ => self.active.clone(),
        }
    }

    pub(crate) fn pending_at(&self, current_ledger: u32) -> Option<FeeSchedule> {
        match self.pending.as_ref() {
            Some(pending) if pending.activation_ledger > current_ledger => Some(pending.clone()),
            _ => None,
        }
    }

    pub(crate) fn previous_at(&self, current_ledger: u32) -> Option<FeeSchedule> {
        match self.pending.as_ref() {
            Some(pending) if pending.activation_ledger <= current_ledger => self.active.clone(),
            _ => self.previous.clone(),
        }
    }

    /// Persist to the original key layout; Soroban commits these writes as one
    /// invocation, so readers observe either the old or the complete new state.
    ///
    /// Callers must have called `validate` first; persisting an invalid state
    /// would break the invariants documented on `FeeScheduleState`. In debug
    /// builds we re-assert the invariant to catch misuse early.
    pub(crate) fn persist(&self, env: &Env) {
        debug_assert!(self.previous.as_ref().zip(self.active.as_ref()).map_or(true, |(p, a)| p != a));
        let storage = env.storage().instance();
        match &self.active {
            Some(schedule) => storage.set(&FeeScheduleStorageKey::Active, schedule),
            None => storage.remove(&FeeScheduleStorageKey::Active),
        }
        match &self.pending {
            Some(schedule) => storage.set(&FeeScheduleStorageKey::Pending, schedule),
            None => storage.remove(&FeeScheduleStorageKey::Pending),
        }
        match &self.previous {
            Some(schedule) => storage.set(&FeeScheduleStorageKey::Previous, schedule),
            None => storage.remove(&FeeScheduleStorageKey::Previous),
        }
    }
}
