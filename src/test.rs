#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::{Address as _, Events}, Address, BytesN, Env, IntoVal};

    #[test]
    fn test_get_yield_tier_returns_default_when_unset() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        // Verify that calling read view when unset returns default without panicking
        let state = client.get_yield_tier();
        assert_eq!(state, YieldTierState::Unset);
    }

    #[test]
    fn test_get_yield_tier_returns_stored_state() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        // Update state and verify read view returns exact stored value
        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);

        // Update to another boundary value
        client.set_yield_tier(&YieldTierState::Tier3);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier3);
    }

    #[test]
    fn test_upgrade_admin_allowed() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let new_wasm = BytesN::from_array(&env, &[1; 32]);
        client.upgrade(&new_wasm);

        assert_eq!(
            env.events().all().last().unwrap(),
            (
                contract_id,
                (symbol_short!("upgrade"),).into_val(&env),
                ().into_val(&env)
            )
        );
    }

    #[test]
    #[should_panic(expected = "HostError")]
    fn test_upgrade_non_admin_rejected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let new_wasm = BytesN::from_array(&env, &[1; 32]);
        
        // This will panic because we didn't mock auths, so require_auth() fails
        client.upgrade(&new_wasm);
    }
}

