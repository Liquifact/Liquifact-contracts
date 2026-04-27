//! Standalone test for MaxUniqueInvestorsCap and UniqueFunderCount functionality
//! This test file validates the core functionality without dependencies on other test modules

use super::*;
use soroban_sdk::{Address, Env, String};

#[test]
fn test_unique_funder_count_basic_functionality() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    // Initialize escrow with cap of 3 investors
    client.init(
        &admin,
        &String::from_str(&env, "CAP_TEST"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(3u32),
    );

    // Verify initial state
    assert_eq!(client.get_unique_funder_count(), 0);
    assert_eq!(client.get_max_unique_investors_cap(), Some(3u32));

    // Add first investor
    let inv1 = Address::generate(&env);
    client.fund(&inv1, &30_000_000_000i128);
    assert_eq!(client.get_unique_funder_count(), 1);
    assert_eq!(client.get_contribution(&inv1), 30_000_000_000i128);

    // Add second investor
    let inv2 = Address::generate(&env);
    client.fund(&inv2, &30_000_000_000i128);
    assert_eq!(client.get_unique_funder_count(), 2);
    assert_eq!(client.get_contribution(&inv2), 30_000_000_000i128);

    // Add third investor (reaches cap)
    let inv3 = Address::generate(&env);
    client.fund(&inv3, &40_000_000_000i128);
    assert_eq!(client.get_unique_funder_count(), 3);
    assert_eq!(client.get_contribution(&inv3), 40_000_000_000i128);
    assert_eq!(client.get_escrow().status, 1); // Funded
}

#[test]
#[should_panic(expected = "unique investor cap reached")]
fn test_cap_enforcement_blocks_excess_investors() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    // Initialize escrow with cap of 2 investors
    client.init(
        &admin,
        &String::from_str(&env, "CAP_TEST2"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(2u32),
    );

    // Add two investors (reaches cap)
    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    client.fund(&inv1, &50_000_000_000i128);
    client.fund(&inv2, &50_000_000_000i128);
    assert_eq!(client.get_unique_funder_count(), 2);

    // Try to add third investor - should panic
    let inv3 = Address::generate(&env);
    client.fund(&inv3, &1_000_000_000i128);
}

#[test]
fn test_re_funding_same_address_doesnt_count_against_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    // Initialize escrow with cap of 1 investor
    client.init(
        &admin,
        &String::from_str(&env, "CAP_TEST3"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(1u32),
    );

    let investor = Address::generate(&env);

    // First fund should succeed
    client.fund(&investor, &30_000_000_000i128);
    assert_eq!(client.get_unique_funder_count(), 1);

    // Re-funding same address should also succeed (doesn't count against cap)
    client.fund(&investor, &30_000_000_000i128);
    assert_eq!(client.get_unique_funder_count(), 1);

    // Final fund from same address should succeed
    client.fund(&investor, &40_000_000_000i128);
    assert_eq!(client.get_unique_funder_count(), 1);
    assert_eq!(client.get_escrow().status, 1); // Funded
}

#[test]
fn test_no_cap_allows_unlimited_investors() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    // Initialize escrow with no cap
    client.init(
        &admin,
        &String::from_str(&env, "CAP_TEST4"),
        &sme,
        &500_000_000_000i128, // Larger target for more investors
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None, // No cap set
    );

    assert_eq!(client.get_max_unique_investors_cap(), None);

    // Should be able to add many investors when no cap is set
    for i in 0..5 {
        let investor = Address::generate(&env);
        client.fund(&investor, &100_000_000_000i128);
        assert_eq!(client.get_unique_funder_count(), i + 1);
    }

    assert_eq!(client.get_unique_funder_count(), 5);
    assert_eq!(client.get_escrow().status, 1); // Funded
}

#[test]
fn test_cap_with_fund_with_commitment() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });

    // Initialize escrow with cap of 2 investors and tier system
    client.init(
        &admin,
        &String::from_str(&env, "CAP_TEST5"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &Some(2u32),
    );

    assert_eq!(client.get_unique_funder_count(), 0);

    // First investor uses fund_with_commitment
    let inv1 = Address::generate(&env);
    client.fund_with_commitment(&inv1, &50_000_000_000i128, &200u64);
    assert_eq!(client.get_unique_funder_count(), 1);
    assert_eq!(client.get_investor_yield_bps(&inv1), 900);

    // Second investor uses regular fund
    let inv2 = Address::generate(&env);
    client.fund(&inv2, &50_000_000_000i128);
    assert_eq!(client.get_unique_funder_count(), 2);
    assert_eq!(client.get_investor_yield_bps(&inv2), 800);

    assert_eq!(client.get_escrow().status, 1); // Funded
}
