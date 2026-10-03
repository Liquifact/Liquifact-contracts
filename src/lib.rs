use soroban_sdk::{contract, contracterror, contractimpl, symbol_short, Address, BytesN, Env, Symbol};

const YIELD_TIER_KEY: Symbol = symbol_short!("YLD_TIER");
const ADMIN_KEY: Symbol = symbol_short!("ADMIN");

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotAuthorized = 1,
    AlreadyInitialized = 2,
    NotInitialized = 3,
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
    /// Initializes the yield tier contract with an admin address.
    /// Returns `Err(Error::AlreadyInitialized)` if initialization has already occurred,
    /// making retry/re-entry failure recovery deterministic and reviewable.
    pub fn init(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&ADMIN_KEY) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&ADMIN_KEY, &admin);
        Ok(())
    }

    /// Upgrades the contract WASM hash.
    /// Returns `Err(Error::NotInitialized)` if the contract has not been initialized.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), Error> {
        let admin: Address = match env.storage().instance().get(&ADMIN_KEY) {
            Some(a) => a,
            None => return Err(Error::NotInitialized),
        };
        admin.require_auth();

        env.deployer().update_current_contract_wasm(new_wasm_hash.clone());
        env.events().publish((symbol_short!("upgrade"),), (new_wasm_hash,));

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
    /// Returns `Err(Error::NotInitialized)` if the contract has not been initialized.
    pub fn set_yield_tier(env: Env, tier: YieldTierState) -> Result<(), Error> {
        let admin: Address = match env.storage().instance().get(&ADMIN_KEY) {
            Some(a) => a,
            None => return Err(Error::NotInitialized),
        };
        admin.require_auth();
        env.storage().instance().set(&YIELD_TIER_KEY, &tier);
        env.events().publish((symbol_short!("tier_set"),), (tier.clone(),));
        Ok(())
    }
}

mod test;
