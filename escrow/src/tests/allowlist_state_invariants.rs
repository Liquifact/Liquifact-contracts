use super::{default_init, setup};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

#[test]
fn single_updates_preserve_allowlist_index_transitions() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);

    client.set_investor_allowlisted(&investor, &true, &0u32);
    assert_eq!(client.get_allowlisted_investors_count(), 1);
    assert_eq!(client.get_allowlisted_investors(&0, &10).get(0), Some(investor.clone()));

    client.set_investor_allowlisted(&investor, &true, &1u32);
    assert_eq!(client.get_allowlisted_investors_count(), 1);

    client.set_investor_allowlisted(&investor, &false, &2u32);
    assert_eq!(client.get_allowlisted_investors_count(), 0);
    assert_eq!(client.get_allowlisted_investors(&0, &10).len(), 0);

    client.set_investor_allowlisted(&investor, &true, &3u32);
    assert_eq!(client.get_allowlisted_investors_count(), 1);
    assert_eq!(client.get_allowlisted_investors(&0, &10).get(0), Some(investor));
}

#[test]
fn batch_updates_persist_a_unique_index_and_consume_nonce() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);
    let mut investors = soroban_sdk::Vec::new(&env);
    investors.push_back(investor_a.clone());
    investors.push_back(investor_b.clone());
    investors.push_back(investor_a.clone());

    client.set_investors_allowlisted(&investors, &true, &0u32);
    assert_eq!(client.get_allowlisted_investors_count(), 2);
    let listed = client.get_allowlisted_investors(&0, &10);
    assert_eq!(listed.len(), 2);
    assert_eq!(listed.get(0), Some(investor_a.clone()));
    assert_eq!(listed.get(1), Some(investor_b.clone()));

    client.set_investors_allowlisted(&investors, &false, &1u32);
    assert_eq!(client.get_allowlisted_investors_count(), 0);
    assert_eq!(client.get_allowlisted_investors(&0, &10).len(), 0);
}