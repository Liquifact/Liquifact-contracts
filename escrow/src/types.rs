use soroban_sdk::{contracttype, Symbol};

/// Named fee schedule with explicit bounds.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FeeSchedule {
    pub name: Symbol,
    pub fee_bps: u32,
    pub min_bps: u32,
    pub max_bps: u32,
}

/// Storage state for the fee schedule lifecycle.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub(crate) struct FeeScheduleState {
    pub active: Option<FeeSchedule>,
    pub pending: Option<FeeSchedule>,
    /// Ledger at which `pending` becomes `active`.
    pub activation_ledger: Option<u32>,
    /// The schedule that was active before the current active.
    pub previous: Option<FeeSchedule>,
}

/// Storage keys used for fee-schedule state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FeeScheduleKey {
    State,
}
