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
