#[cfg(test)]
mod tests {
    use crate::{YieldTierContract, YieldTierContractClient, YieldTierState};
    use soroban_sdk::{
        testutils::{Address as _, Events},
        Address, BytesN, Env,
    };

    #[test]
    fn test_get_yield_tier_returns_default_when_unset() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let state = client.get_yield_tier();
        assert_eq!(state, YieldTierState::Unset);
    }

    #[test]
    fn test_get_yield_tier_returns_stored_state() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);

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
        let event_count_before = env.events().all().events().len();
        client.upgrade(&new_wasm);
        assert_eq!(env.events().all().events().len(), event_count_before + 1);
        assert_eq!(client.get_yield_tier(), YieldTierState::Unset);
    }

    #[test]
    fn test_upgrade_non_admin_rejected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // non-admin will fail auth because env.mock_all_auths is not set
        let new_wasm = BytesN::from_array(&env, &[1; 32]);
        let result = client.try_upgrade(&new_wasm);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_yield_tier_admin_authorized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let events_before = env.events().all().events().len();
        client.set_yield_tier(&YieldTierState::Tier1);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier1);
        assert_eq!(env.events().all().events().len(), events_before + 1);
    }

    #[test]
    fn test_set_yield_tier_noop_is_idempotent_and_silent() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        client.set_yield_tier(&YieldTierState::Tier2);
        let _ = env.events().all();

        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);
        assert_eq!(env.events().all().events().len(), 0);

        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(env.events().all().events().len(), 0);
    }

    #[test]
    fn test_set_yield_tier_noop_still_requires_admin_auth() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let event_count_before = env.events().all().events().len();
        let result = client.try_set_yield_tier(&YieldTierState::Tier1);
        assert!(result.is_err());
        assert_eq!(env.events().all().events().len(), event_count_before);
        assert_eq!(client.get_yield_tier(), YieldTierState::Unset);
    }

    #[test]
    fn test_set_yield_tier_non_admin_rejected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // non-admin will fail auth without mock_all_auths
        let result = client.try_set_yield_tier(&YieldTierState::Tier1);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_yield_tier_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let event_count_before = env.events().all().events().len();
        client.set_yield_tier(&YieldTierState::Tier3);
        assert_eq!(env.events().all().events().len(), event_count_before + 1);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier3);
    }
}
