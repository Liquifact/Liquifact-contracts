//! Standalone test for MaxUniqueInvestorsCap and UniqueFunderCount functionality
//! This test file validates the core functionality without dependencies on other test modules
use super::*;
use crate::MaxUniqueInvestorsCapLowered;
use crate::{MaxUniqueInvestorsCapLowered, MinContributionFloorLowered};
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
        &None,
        &None,
        &None,
        &None,
        &None,
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
#[should_panic]
fn test_cap_enforcement_blocks_excess_investors() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    // Use a target (200B) larger than inv1+inv2 (50B+50B=100B) so the escrow
    // remains open (status=0) when the third investor hits the cap gate.
    client.init(
        &admin,
        &String::from_str(&env, "CAP_TEST2"),
        &sme,
        &200_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(2u32),
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Add two investors — reaches the investor cap but NOT the funding target.
    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    client.fund(&inv1, &50_000_000_000i128);
    client.fund(&inv2, &50_000_000_000i128);
    assert_eq!(client.get_unique_funder_count(), 2);
    assert_eq!(client.get_escrow().status, 0); // still open

    // Third investor hits the cap — must panic "unique investor cap reached".
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
        &None,
        &None,
        &None,
        &None,
        &None,
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
        &None, // No distinct-investor cap set
        &None,
        &None,
        &None,
        &None,
        &None,
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
#[should_panic]
fn test_max_per_investor_cap_blocks_excess_principal() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_TEST6"),
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
        &Some(50_000_000_000i128),
        &None,
        &None,
        &None,
        &None,
    );

    let inv1 = Address::generate(&env);
    client.fund(&inv1, &30_000_000_000i128);
    assert_eq!(client.get_contribution(&inv1), 30_000_000_000i128);

    // Second contribution would exceed the per-investor cap.
    client.fund(&inv1, &21_000_000_000i128);
}

#[test]
#[should_panic]
fn test_init_zero_max_per_investor_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_TEST7"),
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
        &Some(0i128),
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic(expected = "FundingBelowMinContribution")]
fn test_min_contribution_floor_below_value_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let floor = 1_000_000_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "CAP_BOUND_FLOOR1"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(floor),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &(floor - 1));
}

#[test]
fn test_min_contribution_floor_exact_value_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let floor = 1_000_000_000i128;
    let inv = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP_BOUND_FLOOR2"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(floor),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&inv, &floor);
    assert_eq!(client.get_contribution(&inv), floor);
    assert_eq!(client.get_unique_funder_count(), 1);
}

#[test]
#[should_panic(expected = "FundingBelowMinContribution")]
fn test_min_contribution_floor_follow_on_below_value_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let floor = 1_000_000_000i128;
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP_BOUND_FLOOR3"),
        &sme,
        &200_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(floor),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&investor, &floor);
    client.fund(&investor, &(floor - 1));
}

#[test]
fn test_per_investor_cap_exact_cumulative_value_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let cap = 50_000_000_000i128;
    let inv = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP_BOUND_INV1"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(cap),
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&inv, &30_000_000_000i128);
    client.fund(&inv, &20_000_000_000i128);
    assert_eq!(client.get_contribution(&inv), cap);
    assert_eq!(client.get_unique_funder_count(), 1);
}

#[test]
#[should_panic(expected = "InvestorContributionExceedsCap")]
fn test_per_investor_cap_one_over_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let cap = 50_000_000_000i128;
    let inv = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP_BOUND_INV2"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(cap),
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&inv, &30_000_000_000i128);
    client.fund(&inv, &20_000_000_001i128);
}

#[test]
fn test_unique_investor_cap_exact_value_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_BOUND_UNIQ1"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &10_000_000_000i128);
    client.fund(&Address::generate(&env), &10_000_000_000i128);
    client.fund(&Address::generate(&env), &10_000_000_000i128);
    assert_eq!(client.get_unique_funder_count(), 3);
}

#[test]
#[should_panic(expected = "UniqueInvestorCapReached")]
fn test_unique_investor_cap_new_funder_one_over_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_BOUND_UNIQ2"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &10_000_000_000i128);
    client.fund(&Address::generate(&env), &10_000_000_000i128);
    client.fund(&Address::generate(&env), &10_000_000_000i128);
    client.fund(&Address::generate(&env), &1_000_000_000i128);
}

#[test]
fn test_unique_investor_cap_existing_investor_follow_on_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let inv = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP_BOUND_UNIQ3"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&inv, &10_000_000_000i128);
    client.fund(&inv, &10_000_000_000i128);
    assert_eq!(client.get_contribution(&inv), 20_000_000_000i128);
    assert_eq!(client.get_unique_funder_count(), 1);
}

#[test]
#[should_panic(expected = "MinContributionNotPositive")]
fn test_init_min_contribution_not_positive_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_BOUND_INIT1"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(0i128),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic(expected = "MinContributionExceedsAmount")]
fn test_init_min_contribution_exceeds_amount_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_BOUND_INIT2"),
        &sme,
        &10_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(20_000_000_000i128),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic(expected = "MaxUniqueInvestorsNotPositive")]
fn test_init_zero_max_unique_investors_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_BOUND_INIT3"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(0u32),
        &None,
        &None,
        &None,
        &None,
        &None,
    );
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
        &None,
        &None,
        &None,
        &None,
        &None,
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

// --- lower_max_unique_investors (#255) ---

#[test]
fn test_lower_max_unique_investors_success() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_LOWER"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(5u32),
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    client.fund(&inv1, &20_000_000_000i128);
    client.fund(&inv2, &20_000_000_000i128);
    assert_eq!(client.get_unique_funder_count(), 2);

    let new_cap = client.lower_max_unique_investors(&2u32);
    assert_eq!(new_cap, 2);
    assert_eq!(client.get_max_unique_investors_cap(), Some(2));
}

#[test]
#[should_panic]
fn test_lower_cap_blocks_new_investors_at_lowered_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_LOWER2"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &20_000_000_000i128);
    client.fund(&Address::generate(&env), &20_000_000_000i128);
    client.lower_max_unique_investors(&2u32);

    client.fund(&Address::generate(&env), &1_000_000_000i128);
}

#[test]
fn test_lower_cap_existing_investors_may_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_LOWER3"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    client.fund(&inv1, &30_000_000_000i128);
    client.fund(&inv2, &30_000_000_000i128);
    client.lower_max_unique_investors(&2u32);

    client.fund(&inv1, &20_000_000_000i128);
    client.fund(&inv2, &20_000_000_000i128);
    assert_eq!(client.get_unique_funder_count(), 2);
}

#[test]
#[should_panic]
fn test_lower_cap_rejects_raise() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_LOWER4"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.lower_max_unique_investors(&4u32);
}

#[test]
#[should_panic]
fn test_lower_cap_rejects_below_funder_count() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_LOWER5"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(5u32),
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &10_000_000_000i128);
    client.fund(&Address::generate(&env), &10_000_000_000i128);
    client.fund(&Address::generate(&env), &10_000_000_000i128);
    client.lower_max_unique_investors(&2u32);
}

#[test]
#[should_panic]
fn test_lower_cap_rejects_non_open_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_LOWER6"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &100_000_000_000i128);
    assert_eq!(client.get_escrow().status, 1);
    client.lower_max_unique_investors(&2u32);
}

#[test]
#[should_panic]
fn test_lower_cap_rejects_unlimited_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_LOWER7"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.lower_max_unique_investors(&10u32);
}

#[test]
fn test_lower_cap_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_LOWER8"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(5u32),
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.lower_max_unique_investors(&3u32);
    assert!(
        env.auths().iter().any(|(addr, _)| *addr == admin),
        "admin auth was not recorded for lower_max_unique_investors"
    );
}

#[test]
#[should_panic]
fn test_lower_cap_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_LOWER9"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(5u32),
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    env.mock_auths(&[]);
    client.lower_max_unique_investors(&3u32);
}

#[test]
fn test_lower_cap_emits_event() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);
    let contract_id = client.address.clone();

    client.init(
        &admin,
        &String::from_str(&env, "CAP_EVT"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(5u32),
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &10_000_000_000i128);
    client.lower_max_unique_investors(&3u32);

    assert_eq!(
        env.events().all(),
        std::vec![MaxUniqueInvestorsCapLowered {
            name: symbol_short!("inv_cap"),
            invoice_id: client.get_escrow().invoice_id,
            old_cap: 5,
            new_cap: 3,
        }
        .to_xdr(&env, &contract_id)]
    );
}

#[test]
fn test_get_remaining_investor_slots() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let client_no_cap = deploy(&env);
    client_no_cap.init(
        &admin,
        &String::from_str(&env, "CAP_REM_1"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_eq!(client_no_cap.get_remaining_investor_slots(), None);

    let client_cap = deploy(&env);
    client_cap.init(
        &admin,
        &String::from_str(&env, "CAP_REM_2"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    assert_eq!(client_cap.get_remaining_investor_slots(), Some(3));

    client_cap.fund(&Address::generate(&env), &10_000_000_000i128);
    assert_eq!(client_cap.get_remaining_investor_slots(), Some(2));

    client_cap.fund(&Address::generate(&env), &10_000_000_000i128);
    assert_eq!(client_cap.get_remaining_investor_slots(), Some(1));

    client_cap.fund(&Address::generate(&env), &10_000_000_000i128);
    assert_eq!(client_cap.get_remaining_investor_slots(), Some(0));
}

#[test]
fn test_get_remaining_investor_slots_post_lower_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP_REM_3"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(5u32),
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &10_000_000_000i128);
    client.fund(&Address::generate(&env), &10_000_000_000i128);
    client.fund(&Address::generate(&env), &10_000_000_000i128);

    assert_eq!(client.get_remaining_investor_slots(), Some(2));

    client.lower_max_unique_investors(&3u32);

    assert_eq!(client.get_remaining_investor_slots(), Some(0));
}

// --- lower_min_contribution_floor (#493) ---

#[test]
fn test_lower_min_contribution_floor_success() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let initial_floor = 10_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "FLOOR_LOWER"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(initial_floor),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    assert_eq!(client.get_min_contribution_floor(), initial_floor);

    let new_floor = client.lower_min_contribution_floor(&5_000i128);
    assert_eq!(new_floor, 5_000i128);
    assert_eq!(client.get_min_contribution_floor(), 5_000i128);
}

#[test]
fn test_lower_floor_enforces_new_floor() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let initial_floor = 10_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "FLOOR_ENF"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(initial_floor),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Lower the floor
    client.lower_min_contribution_floor(&5_000i128);
    assert_eq!(client.get_min_contribution_floor(), 5_000i128);

    // Fund at the new floor level must succeed
    let inv = Address::generate(&env);
    client.fund(&inv, &5_000i128);
    assert_eq!(client.get_contribution(&inv), 5_000i128);

    // Fund below the new floor must be rejected
    let inv2 = Address::generate(&env);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.fund(&inv2, &4_999i128);
    }))
    .is_err());
}

#[test]
#[should_panic]
fn test_lower_floor_rejects_raise() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "FLOOR_RAISE"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(10_000i128),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Attempt to raise the floor
    client.lower_min_contribution_floor(&20_000i128);
}

#[test]
#[should_panic]
fn test_lower_floor_rejects_same_floor() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "FLOOR_SAME"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(10_000i128),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Attempt to set the same floor
    client.lower_min_contribution_floor(&10_000i128);
}

#[test]
#[should_panic]
fn test_lower_floor_rejects_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "FLOOR_ZERO"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(10_000i128),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Non-positive floor must be rejected
    client.lower_min_contribution_floor(&0i128);
}

#[test]
#[should_panic]
fn test_lower_floor_rejects_negative() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "FLOOR_NEG"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(10_000i128),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Negative floor must be rejected
    client.lower_min_contribution_floor(&-1i128);
}

#[test]
#[should_panic]
fn test_lower_floor_rejects_non_open_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "FLOOR_STATE"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(10_000i128),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Fund to close the escrow
    client.fund(&Address::generate(&env), &100_000_000_000i128);
    assert_eq!(client.get_escrow().status, 1);

    // Cannot lower floor in funded state
    client.lower_min_contribution_floor(&5_000i128);
}

#[test]
fn test_lower_floor_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "FLOOR_AUTH"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(10_000i128),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.lower_min_contribution_floor(&5_000i128);
    assert!(
        env.auths().iter().any(|(addr, _)| *addr == admin),
        "admin auth was not recorded for lower_min_contribution_floor"
    );
}

#[test]
#[should_panic]
fn test_lower_floor_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &String::from_str(&env, "FLOOR_UNAUTH"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(10_000i128),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    env.mock_auths(&[]);
    client.lower_min_contribution_floor(&5_000i128);
}

#[test]
fn test_lower_floor_emits_event() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);
    let contract_id = client.address.clone();

    let initial_floor = 10_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "FLOOR_EVT"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(initial_floor),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.lower_min_contribution_floor(&5_000i128);

    assert_eq!(
        env.events().all(),
        std::vec![crate::MinContributionFloorLowered {
            name: symbol_short!("floor_lo"),
            invoice_id: client.get_escrow().invoice_id,
            old_floor: 10_000i128,
            new_floor: 5_000i128,
        }
        .to_xdr(&env, &contract_id)]
    );
}

#[test]
fn test_lower_floor_twice_successive() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "FLOOR_TWICE"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(10_000i128),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    assert_eq!(client.get_min_contribution_floor(), 10_000i128);
    client.lower_min_contribution_floor(&7_000i128);
    assert_eq!(client.get_min_contribution_floor(), 7_000i128);
    client.lower_min_contribution_floor(&5_000i128);
    assert_eq!(client.get_min_contribution_floor(), 5_000i128);
    client.lower_min_contribution_floor(&1_000i128);
    assert_eq!(client.get_min_contribution_floor(), 1_000i128);
}

#[test]
fn test_lower_floor_fund_at_old_floor_still_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "FLOOR_OLD"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(10_000i128),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Lower floor from 10,000 to 5,000
    client.lower_min_contribution_floor(&5_000i128);

    // Fund at exactly the new floor (5,000) must succeed
    let inv1 = Address::generate(&env);
    client.fund(&inv1, &5_000i128);
    assert_eq!(client.get_contribution(&inv1), 5_000i128);

    // Existing investor follow-on deposit at the new floor
    client.fund(&inv1, &5_000i128);
    assert_eq!(client.get_contribution(&inv1), 10_000i128);

    // New investor at the old floor (10,000) must also succeed (it's above new floor)
    let inv2 = Address::generate(&env);
    client.fund(&inv2, &10_000i128);
    assert_eq!(client.get_contribution(&inv2), 10_000i128);
}

#[test]
fn test_lower_floor_unconfigured_succeeds_if_positive() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    // Init with no min_contribution floor (defaults to 0)
    client.init(
        &admin,
        &String::from_str(&env, "FLOOR_NOCONF"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Floor defaults to 0
    assert_eq!(client.get_min_contribution_floor(), 0);

    // Cannot lower to a positive value since 0 is the current floor and new floor must be < old
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.lower_min_contribution_floor(&5_000i128);
    }))
    .is_err());
}

// --- raise_max_per_investor (#492) ---

#[test]
fn test_raise_max_per_investor_success() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let initial_cap = 50_000_000_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "RAISE_CAP1"),
        &sme,
        &200_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(initial_cap),
        &None,
        &None,
        &None,
        &None,
    );

    let inv1 = Address::generate(&env);
    client.fund(&inv1, &30_000_000_000i128);
    assert_eq!(client.get_contribution(&inv1), 30_000_000_000i128);

    // Raise the cap
    let new_cap = 80_000_000_000i128;
    let result = client.raise_max_per_investor(&new_cap);
    assert_eq!(result, new_cap);
    assert_eq!(client.get_max_per_investor_cap(), Some(new_cap));

    // Existing investor can now contribute more
    client.fund(&inv1, &40_000_000_000i128);
    assert_eq!(client.get_contribution(&inv1), 70_000_000_000i128);

    // New investor can contribute up to new cap
    let inv2 = Address::generate(&env);
    client.fund(&inv2, &75_000_000_000i128);
    assert_eq!(client.get_contribution(&inv2), 75_000_000_000i128);
}

#[test]
#[should_panic]
fn test_raise_cap_rejects_lower() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let initial_cap = 50_000_000_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "RAISE_CAP2"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(initial_cap),
        &None,
        &None,
        &None,
        &None,
    );

    // Attempt to lower the cap
    client.raise_max_per_investor(&40_000_000_000i128);
}

#[test]
#[should_panic]
fn test_raise_cap_rejects_equal() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let initial_cap = 50_000_000_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "RAISE_CAP3"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(initial_cap),
        &None,
        &None,
        &None,
        &None,
    );

    // Attempt to set same cap
    client.raise_max_per_investor(&50_000_000_000i128);
}

#[test]
#[should_panic]
fn test_raise_cap_rejects_unconfigured() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    // Init with NO per-investor cap
    client.init(
        &admin,
        &String::from_str(&env, "RAISE_CAP4"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None, // No cap
        &None,
        &None,
        &None,
        &None,
    );

    assert_eq!(client.get_max_per_investor_cap(), None);
    client.raise_max_per_investor(&50_000_000_000i128);
}

#[test]
#[should_panic]
fn test_raise_cap_rejects_non_open_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let initial_cap = 50_000_000_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "RAISE_CAP5"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(initial_cap),
        &None,
        &None,
        &None,
        &None,
    );

    // Fund to close the escrow
    client.fund(&Address::generate(&env), &100_000_000_000i128);
    assert_eq!(client.get_escrow().status, 1);

    // Cannot raise cap in funded state
    client.raise_max_per_investor(&75_000_000_000i128);
}

#[test]
fn test_raise_cap_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let initial_cap = 50_000_000_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "RAISE_CAP6"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(initial_cap),
        &None,
        &None,
        &None,
        &None,
    );

    client.raise_max_per_investor(&75_000_000_000i128);
    assert!(
        env.auths().iter().any(|(addr, _)| *addr == admin),
        "admin auth was not recorded for raise_max_per_investor"
    );
}

#[test]
#[should_panic]
fn test_raise_cap_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let initial_cap = 50_000_000_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "RAISE_CAP7"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(initial_cap),
        &None,
        &None,
        &None,
        &None,
    );

    env.mock_auths(&[]);
    client.raise_max_per_investor(&75_000_000_000i128);
}

#[test]
fn test_raise_cap_emits_event() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);
    let contract_id = client.address.clone();

    let initial_cap = 50_000_000_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "RAISE_EVT"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(initial_cap),
        &None,
        &None,
        &None,
        &None,
    );

    let new_cap = 75_000_000_000i128;
    client.raise_max_per_investor(&new_cap);

    assert_eq!(
        env.events().all(),
        std::vec![crate::MaxPerInvestorCapRaised {
            name: symbol_short!("inv_cap"),
            invoice_id: client.get_escrow().invoice_id,
            old_cap: initial_cap,
            new_cap,
        }
        .to_xdr(&env, &contract_id)]
    );
}

#[test]
fn test_raise_cap_enforced_on_new_deposits() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let initial_cap = 50_000_000_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "RAISE_ENF"),
        &sme,
        &200_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(initial_cap),
        &None,
        &None,
        &None,
        &None,
    );

    let inv1 = Address::generate(&env);
    client.fund(&inv1, &50_000_000_000i128);
    assert_eq!(client.get_contribution(&inv1), 50_000_000_000i128);

    // Raise cap
    let new_cap = 100_000_000_000i128;
    client.raise_max_per_investor(&new_cap);

    // Can now deposit up to new cap
    client.fund(&inv1, &50_000_000_000i128);
    assert_eq!(client.get_contribution(&inv1), 100_000_000_000i128);

    // But not exceed it
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.fund(&inv1, &1_000_000_000i128);
    }))
    .is_err());
}

#[test]
fn test_raise_cap_twice_successive() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let initial_cap = 50_000_000_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "RAISE_2X"),
        &sme,
        &300_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(initial_cap),
        &None,
        &None,
        &None,
        &None,
    );

    assert_eq!(client.get_max_per_investor_cap(), Some(initial_cap));

    client.raise_max_per_investor(&75_000_000_000i128);
    assert_eq!(client.get_max_per_investor_cap(), Some(75_000_000_000i128));

    client.raise_max_per_investor(&100_000_000_000i128);
    assert_eq!(client.get_max_per_investor_cap(), Some(100_000_000_000i128));
}

#[test]
fn test_raise_cap_existing_investor_above_old_cap_can_add_more() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let initial_cap = 50_000_000_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "RAISE_OLD"),
        &sme,
        &200_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(initial_cap),
        &None,
        &None,
        &None,
        &None,
    );

    let inv1 = Address::generate(&env);
    // Investor at old cap limit
    client.fund(&inv1, &50_000_000_000i128);
    assert_eq!(client.get_contribution(&inv1), 50_000_000_000i128);

    // Raise cap
    client.raise_max_per_investor(&100_000_000_000i128);

    // Same investor can now add more
    client.fund(&inv1, &30_000_000_000i128);
    assert_eq!(client.get_contribution(&inv1), 80_000_000_000i128);
}

#[test]
#[should_panic]
fn test_raise_cap_rejects_negative() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let initial_cap = 50_000_000_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "RAISE_NEG"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(initial_cap),
        &None,
        &None,
        &None,
        &None,
    );

    // Negative cap must be rejected (new_cap <= old_cap)
    client.raise_max_per_investor(&-1i128);
}
