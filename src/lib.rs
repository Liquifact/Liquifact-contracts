use soroban_sdk::{contractimpl, symbol_short, Env, Symbol};

const YIELD_TIER_KEY: Symbol = symbol_short!("YLD_TIER");

#[derive(Clone, Debug, PartialEq, Eq)]
#[soroban_sdk::contracttype]
pub enum YieldTierState {
    Unset,
    Tier1,
    Tier2,
    Tier3,
}

pub struct YieldTierContract;

#[contractimpl]
impl YieldTierContract {
    /// Returns the current yield-tier state without mutating contract storage.
    /// Returns `YieldTierState::Unset` as a default if no state has been initialized.
    pub fn get_yield_tier(env: Env) -> YieldTierState {
        env.storage()
            .instance()
            .get(&YIELD_TIER_KEY)
            .unwrap_or(YieldTierState::Unset) // Sensible default, never panics
    }

    /// Sets the yield-tier state (admin function).
    pub fn set_yield_tier(env: Env, tier: YieldTierState) {
        env.storage().instance().set(&YIELD_TIER_KEY, &tier);
    }
}
