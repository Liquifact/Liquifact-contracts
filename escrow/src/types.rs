use crate::{FeeSchedule, FeeScheduleStorageKey};
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

impl FeeScheduleState {
    pub(crate) fn load(env: &Env) -> Self {
        let storage = env.storage().instance();
        Self {
            active: storage.get(&FeeScheduleStorageKey::Active),
            pending: storage.get(&FeeScheduleStorageKey::Pending),
            previous: storage.get(&FeeScheduleStorageKey::Previous),
        }
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
    pub(crate) fn persist(&self, env: &Env) {
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
