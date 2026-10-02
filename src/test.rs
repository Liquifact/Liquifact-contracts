use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, BytesN, Env,
};

#[test]
fn test_get_yield_tier_returns_default_when_unset() {
    let env = Env::default();
    let contract_id = env.register(YieldTierContract, ());
    let client = YieldTierContractClient::new(&env, &contract_id);

    let state = client.get_yield_tier();
    assert_eq!(state, YieldTierState::Unset);
}

#[test]
fn test_get_yield_tier_returns_stored_state() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(YieldTierContract, ());
    let client = YieldTierContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.init(&admin);

    client.set_yield_tier(&YieldTierState::Tier2);
    assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);

    client.set_yield_tier(&YieldTierState::Tier3);
    assert_eq!(client.get_yield_tier(), YieldTierState::Tier3);
}

#[test]
fn test_init_deterministic_already_initialized() {
    let env = Env::default();
    let contract_id = env.register(YieldTierContract, ());
    let client = YieldTierContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.init(&admin);

    let res = client.try_init(&admin);
    assert_eq!(res, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn test_uninitialized_calls_return_not_initialized() {
    let env = Env::default();
    let contract_id = env.register(YieldTierContract, ());
    let client = YieldTierContractClient::new(&env, &contract_id);

    let new_wasm = BytesN::from_array(&env, &[1; 32]);
    let res_upgrade = client.try_upgrade(&new_wasm);
    assert_eq!(res_upgrade, Err(Ok(Error::NotInitialized)));

    let res_set = client.try_set_yield_tier(&YieldTierState::Tier1);
    assert_eq!(res_set, Err(Ok(Error::NotInitialized)));
}

#[test]
fn test_upgrade_non_admin_rejected() {
    let env = Env::default();
    let contract_id = env.register(YieldTierContract, ());
    let client = YieldTierContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.init(&admin);

    let new_wasm = BytesN::from_array(&env, &[1; 32]);
    let result = client.try_upgrade(&new_wasm);
    assert!(result.is_err());
}

#[test]
fn test_set_yield_tier_admin_authorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(YieldTierContract, ());
    let client = YieldTierContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.init(&admin);

    client.set_yield_tier(&YieldTierState::Tier1);
    assert_eq!(client.get_yield_tier(), YieldTierState::Tier1);
}

#[test]
fn test_set_yield_tier_non_admin_rejected() {
    let env = Env::default();
    let contract_id = env.register(YieldTierContract, ());
    let client = YieldTierContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.init(&admin);

    let result = client.try_set_yield_tier(&YieldTierState::Tier1);
    assert!(result.is_err());
}

#[test]
fn test_set_yield_tier_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(YieldTierContract, ());
    let client = YieldTierContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.init(&admin);

    client.set_yield_tier(&YieldTierState::Tier3);
    let all_events = env.events().all();
    let filtered = all_events.filter_by_contract(&contract_id);
    let events = filtered.events();
    assert!(!events.is_empty());
}
