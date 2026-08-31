use soroban_sdk::{contract, contracterror, contractimpl, symbol_short, Address, BytesN, Env, Symbol};

const YIELD_TIER_KEY: Symbol = symbol_short!("YLD_TIER");
const ADMIN_KEY: Symbol = symbol_short!("ADMIN");

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotAuthorized = 1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[soroban_sdk::contracttype]
pub enum YieldTierState {
    Unset,
    Tier1,
    Tier2,
    Tier3,
}

#[contract]
pub struct YieldTierContract;

#[contractimpl]
impl YieldTierContract {
    pub fn init(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN_KEY) {
            panic!("already initialized");
        }
        env.storage().instance().set(&ADMIN_KEY, &admin);
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), Error> {
        let admin: Address = env.storage().instance().get(&ADMIN_KEY).unwrap();
        admin.require_auth();

        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());
        env.events()
            .publish((symbol_short!("upgrade"),), (new_wasm_hash.clone(),));

        Ok(())
    }

    /// Returns the current yield-tier state without mutating contract storage.
    /// Returns `YieldTierState::Unset` as a default if no state has been initialized.
    pub fn get_yield_tier(env: Env) -> YieldTierState {
        env.storage()
            .instance()
            .get(&YIELD_TIER_KEY)
            .unwrap_or(YieldTierState::Unset)
    }

    /// Sets the yield-tier state (admin-only).
    ///
    /// Repeated writes of the same canonical value are intentionally treated as a no-op:
    /// they still require admin auth, but they do not rewrite storage or re-emit the
    /// mutation event for a value that is already active.
    pub fn set_yield_tier(env: Env, tier: YieldTierState) -> Result<(), Error> {
        let admin: Address = env.storage().instance().get(&ADMIN_KEY).unwrap();
        admin.require_auth();

        let current: Option<YieldTierState> = env.storage().instance().get(&YIELD_TIER_KEY);
        if current == Some(tier.clone()) {
            return Ok(());
        }

        env.storage().instance().set(&YIELD_TIER_KEY, &tier);
        env.events().publish((symbol_short!("tier_set"),), (tier.clone(),));
        Ok(())
    }
}

#[cfg(test)]
mod test;
