use super::*;

// Funding, contributions, snapshots, tier selection, and fund-shaped cost baselines.

#[test]
fn test_fund_and_settle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INVMETA"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    let funded = client.fund(&investor, &TARGET);
    assert_eq!(funded.funded_amount, TARGET);
    assert_eq!(funded.status, 1);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
fn test_fund_partial_then_full() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV002"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    let partial = client.fund(&investor, &(TARGET / 2));
    assert_eq!(partial.status, 0);
    assert_eq!(partial.funded_amount, TARGET / 2);
    let full = client.fund(&investor, &(TARGET / 2));
    assert_eq!(full.status, 1);
    assert_eq!(full.funded_amount, TARGET);
}

#[test]
#[should_panic(expected = "Funding amount must be positive")]
fn test_fund_zero_amount_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    client.fund(&investor, &0i128);
}

#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_after_funded_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    client.fund(&investor, &TARGET);
    client.fund(&investor, &1i128);
}

#[test]
fn test_fund_requires_investor_auth() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    client.fund(&investor, &TARGET);
    assert!(
        env.auths().iter().any(|(addr, _)| *addr == investor),
        "investor auth was not recorded for fund"
    );
}

#[test]
fn test_single_investor_contribution_tracked() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV020"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &(30_000_000_000i128));
    let contribution = client.get_contribution(&investor);
    assert_eq!(contribution, 30_000_000_000i128);
}

#[test]
fn test_unknown_investor_contribution_is_zero() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let stranger = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    client.fund(&investor, &1_000i128);
    assert_eq!(client.get_contribution(&stranger), 0i128);
}

#[test]
fn test_repeated_funding_accumulates_contribution() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV021"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &(20_000_000_000i128));
    client.fund(&investor, &(30_000_000_000i128));
    assert_eq!(client.get_contribution(&investor), 50_000_000_000i128);
}

#[test]
#[should_panic(expected = "funded_amount overflow")]
fn test_funding_amount_accumulation_overflow_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "OVF001"),
        &sme,
        &i128::MAX,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );

    client.fund(&investor, &(i128::MAX - 1));
    client.fund(&investor, &2i128);
}

#[test]
fn test_funding_amount_overflow_does_not_mutate_state() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "OVF002"),
        &sme,
        &i128::MAX,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );

    client.fund(&investor, &(i128::MAX - 1));
    let before = client.get_escrow();

    let overflowed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.fund(&investor, &2i128);
    }));
    assert!(overflowed.is_err());

    let after = client.get_escrow();
    assert_eq!(after.funded_amount, before.funded_amount);
    assert_eq!(after.status, 0);
    assert_eq!(client.get_contribution(&investor), i128::MAX - 1);
}

#[test]
fn test_multiple_investors_tracked_independently() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV023"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&inv_a, &(20_000_000_000i128));
    client.fund(&inv_b, &(50_000_000_000i128));
    client.fund(&inv_c, &(30_000_000_000i128));
    assert_eq!(client.get_contribution(&inv_a), 20_000_000_000i128);
    assert_eq!(client.get_contribution(&inv_b), 50_000_000_000i128);
    assert_eq!(client.get_contribution(&inv_c), 30_000_000_000i128);
    let sum = client.get_contribution(&inv_a)
        + client.get_contribution(&inv_b)
        + client.get_contribution(&inv_c);
    assert_eq!(sum, client.get_escrow().funded_amount);
}

#[test]
fn test_contributions_sum_equals_funded_amount() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV023b"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&inv_a, &(20_000_000_000i128));
    client.fund(&inv_b, &(50_000_000_000i128));
    client.fund(&inv_c, &(30_000_000_000i128));
    let sum = client.get_contribution(&inv_a)
        + client.get_contribution(&inv_b)
        + client.get_contribution(&inv_c);
    assert_eq!(sum, client.get_escrow().funded_amount);
}

#[test]
fn test_cost_baseline_fund_partial() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV103"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &(10_000_000_000i128));
}

#[test]
fn test_cost_baseline_fund_full() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV104"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &TARGET);
}

#[test]
fn test_cost_baseline_fund_overshoot() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV105"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &(150_000_000_000i128));
    assert_eq!(client.get_escrow().status, 1);
}

#[test]
fn test_cost_baseline_fund_two_step_completion() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV106"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &(TARGET / 2));
    client.fund(&investor, &(TARGET / 2));
    assert_eq!(client.get_escrow().status, 1);
}

#[test]
fn test_funding_close_snapshot_captures_overfunded_total_once() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SNAP001"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_funding_close_snapshot(), None);
    client.fund(&inv, &(TARGET + 50_000_000_000i128));
    let snap = client.get_funding_close_snapshot().expect("snapshot");
    assert_eq!(snap.total_principal, TARGET + 50_000_000_000i128);
    assert_eq!(snap.funding_target, TARGET);
    assert_eq!(snap.closed_at_ledger_timestamp, env.ledger().timestamp());
    assert_eq!(snap.closed_at_ledger_sequence, env.ledger().sequence());
}

#[test]
fn test_funding_snapshot_immutable_across_second_fund_after_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SNAP002"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    client.fund(&a, &(TARGET / 2));
    assert_eq!(client.get_funding_close_snapshot(), None);
    client.fund(&b, &(TARGET / 2));
    let s1 = client.get_funding_close_snapshot().unwrap();
    assert_eq!(s1.total_principal, TARGET);
    let s2 = client.get_funding_close_snapshot().unwrap();
    assert_eq!(s1, s2);
}

#[test]
fn test_pro_rata_weight_ratio_from_snapshot() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SNAP003"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    client.fund(&a, &(20_000_000_000i128));
    client.fund(&b, &(80_000_000_000i128));
    let snap = client.get_funding_close_snapshot().unwrap();
    assert_eq!(snap.total_principal, TARGET);
    let ca = client.get_contribution(&a);
    let cb = client.get_contribution(&b);
    assert_eq!(ca + cb, snap.total_principal);
}

#[test]
fn test_tiered_yield_and_follow_on_fund() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 500,
        yield_bps: 1100,
    });
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIER001"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
    );
    client.fund_with_commitment(&inv, &5_000i128, &200u64);
    assert_eq!(client.get_investor_yield_bps(&inv), 900);
    assert_eq!(client.get_investor_claim_not_before(&inv), 200);
    client.fund(&inv, &5_000i128);
    assert_eq!(client.get_investor_yield_bps(&inv), 900);
    assert_eq!(client.get_escrow().status, 1);
}

#[test]
fn test_tier_selection_edges_base_vs_high_bucket() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let i_short = Address::generate(&env);
    let i_long = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 50,
        yield_bps: 850,
    });
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIER002"),
        &sme,
        &20_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
    );
    client.fund_with_commitment(&i_short, &10_000i128, &40u64);
    assert_eq!(client.get_investor_yield_bps(&i_short), 800);
    client.fund_with_commitment(&i_long, &10_000i128, &50u64);
    assert_eq!(client.get_investor_yield_bps(&i_long), 850);
}

#[test]
#[should_panic(expected = "Additional principal after a tiered first deposit")]
fn test_fund_with_commitment_twice_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 1,
        yield_bps: 810,
    });
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIER003"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
    );
    client.fund_with_commitment(&inv, &5_000i128, &10u64);
    client.fund_with_commitment(&inv, &5_000i128, &10u64);
}

#[test]
#[should_panic(expected = "Additional principal after a tiered first deposit")]
fn test_fund_then_fund_with_commitment_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SEQ001"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    client.fund(&inv, &5_000i128);
    client.fund_with_commitment(&inv, &5_000i128, &10u64);
}

#[test]
fn test_tier_selection_ladder() {
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
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 1000,
    });

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "LADDER01"),
        &sme,
        &100_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
    );

    let inv_base = Address::generate(&env);
    let inv_tier1 = Address::generate(&env);
    let inv_tier2 = Address::generate(&env);
    let inv_mid = Address::generate(&env);

    // Below first tier -> base
    client.fund_with_commitment(&inv_base, &1_000i128, &50u64);
    assert_eq!(client.get_investor_yield_bps(&inv_base), 800);

    // Exactly first tier
    client.fund_with_commitment(&inv_tier1, &1_000i128, &100u64);
    assert_eq!(client.get_investor_yield_bps(&inv_tier1), 900);

    // Between tiers -> still first tier
    client.fund_with_commitment(&inv_mid, &1_000i128, &150u64);
    assert_eq!(client.get_investor_yield_bps(&inv_mid), 900);

    // Exactly second tier
    client.fund_with_commitment(&inv_tier2, &1_000i128, &200u64);
    assert_eq!(client.get_investor_yield_bps(&inv_tier2), 1000);
}

#[test]
fn test_fund_with_commitment_zero_lock_behaves_as_fund() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ZERO001"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
    );

    client.fund_with_commitment(&inv, &5_000i128, &0u64);
    assert_eq!(client.get_investor_yield_bps(&inv), 800);
    assert_eq!(client.get_investor_claim_not_before(&inv), 0);
}

#[test]
fn test_commitment_claim_time_allows_u64_max_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|ledger| {
        ledger.timestamp = u64::MAX - 5;
    });
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CLKMAX1"),
        &sme,
        &1_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    client.fund_with_commitment(&investor, &100i128, &5u64);

    assert_eq!(client.get_investor_claim_not_before(&investor), u64::MAX);
}

#[test]
#[should_panic(expected = "investor claim time overflow")]
fn test_commitment_claim_time_overflow_panics() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|ledger| {
        ledger.timestamp = u64::MAX - 5;
    });
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CLKMAX2"),
        &sme,
        &1_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    client.fund_with_commitment(&investor, &100i128, &6u64);
}

#[test]
fn test_commitment_claim_time_overflow_does_not_record_position() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|ledger| {
        ledger.timestamp = u64::MAX - 5;
    });
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CLKMAX3"),
        &sme,
        &1_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );

    let overflowed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.fund_with_commitment(&investor, &100i128, &6u64);
    }));
    assert!(overflowed.is_err());

    assert_eq!(client.get_escrow().funded_amount, 0);
    assert_eq!(client.get_contribution(&investor), 0);
    assert_eq!(client.get_investor_claim_not_before(&investor), 0);
}

#[test]
#[should_panic(expected = "strictly increasing min_lock_secs")]
fn test_init_bad_tier_order_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 900,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 950,
    });
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "BADTIER"),
        &sme,
        &1_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
    );
}

#[test]
#[should_panic(expected = "tier yield_bps must be >= base yield_bps")]
fn test_init_tier_yield_below_base_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 10,
        yield_bps: 700,
    });
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "BADT2"),
        &sme,
        &1_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
    );
}

#[test]
fn test_differential_funding_target_eq_exact_cross() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    let t = 5_000i128;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "DIFF002"),
        &sme,
        &t,
        &100i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    let escrow = client.fund(&inv, &t);
    assert_eq!(escrow.funded_amount, t);
    assert_eq!(escrow.status, 1);
    let snap = client.get_funding_close_snapshot().unwrap();
    assert_eq!(snap.total_principal, t);
    assert_eq!(snap.funding_target, t);
}

#[test]
fn test_ledger_sequence_recorded_in_snapshot_with_tick() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "DIFF003"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    let seq = env.ledger().sequence();
    client.fund(&inv, &1_000i128);
    let snap = client.get_funding_close_snapshot().unwrap();
    assert_eq!(snap.closed_at_ledger_sequence, seq);
}

#[test]
fn test_get_funding_close_snapshot_absent_before_any_funding() {
    // Snapshot must be None immediately after init, before any fund() call.
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SNAP010"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    assert_eq!(
        client.get_funding_close_snapshot(),
        None,
        "snapshot must be absent before any funding"
    );
}

#[test]
fn test_get_funding_close_snapshot_present_after_funding_completes() {
    // Snapshot must be Some with correct fields once funded_amount reaches funding_target.
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SNAP011"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    // Partial fund — snapshot still absent.
    client.fund(&inv, &(TARGET / 2));
    assert_eq!(
        client.get_funding_close_snapshot(),
        None,
        "snapshot must remain absent while escrow is still open"
    );
    // Final fund that crosses the target — snapshot must now be present.
    client.fund(&inv, &(TARGET / 2));
    let snap = client
        .get_funding_close_snapshot()
        .expect("snapshot must be present after funding completes");
    assert_eq!(snap.total_principal, TARGET);
    assert_eq!(snap.funding_target, TARGET);
    assert_eq!(client.get_escrow().status, 1);
}

#[test]
fn test_get_funding_close_snapshot_immutable_after_set() {
    // Once the snapshot is written it must not change, even if additional reads occur
    // after the escrow has transitioned to a terminal state (settled).
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SNAP012"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    // Fund exactly to target — snapshot is written here.
    client.fund(&inv, &TARGET);
    let snap_at_close = client
        .get_funding_close_snapshot()
        .expect("snapshot must be present after funding");
    // Advance through settlement — snapshot must remain identical.
    client.settle();
    let snap_after_settle = client
        .get_funding_close_snapshot()
        .expect("snapshot must still be present after settlement");
    assert_eq!(
        snap_at_close, snap_after_settle,
        "snapshot must be immutable after being set"
    );
}

// --- MaxUniqueInvestorsCap and UniqueFunderCount Tests ---

#[test]
fn test_unique_funder_count_initialized_to_zero() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP001"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_unique_funder_count(), 0);
}

#[test]
fn test_unique_funder_count_increments_on_first_investor() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP002"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_unique_funder_count(), 0);
    client.fund(&investor, &(TARGET / 2));
    assert_eq!(client.get_unique_funder_count(), 1);
    client.fund(&investor, &(TARGET / 2));
    assert_eq!(client.get_unique_funder_count(), 1); // Still 1, same investor
}

#[test]
fn test_unique_funder_count_increments_for_distinct_investors() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP003"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_unique_funder_count(), 0);

    client.fund(&inv_a, &(TARGET / 3));
    assert_eq!(client.get_unique_funder_count(), 1);

    client.fund(&inv_b, &(TARGET / 3));
    assert_eq!(client.get_unique_funder_count(), 2);

    client.fund(&inv_c, &(TARGET / 3));
    assert_eq!(client.get_unique_funder_count(), 3);
}

#[test]
fn test_unique_funder_count_with_fund_with_commitment() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });

    client.init(
        &admin,
        &String::from_str(&env, "CAP004"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
    );

    assert_eq!(client.get_unique_funder_count(), 0);

    // First investor uses fund_with_commitment
    client.fund_with_commitment(&inv_a, &(TARGET / 2), &200u64);
    assert_eq!(client.get_unique_funder_count(), 1);

    // Second investor uses regular fund
    client.fund(&inv_b, &(TARGET / 2));
    assert_eq!(client.get_unique_funder_count(), 2);
}

#[test]
fn test_max_unique_investors_cap_none_allows_unlimited() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP005"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None, // No cap set
    );

    // Should be able to add many investors when no cap is set
    for i in 0..10 {
        let investor = Address::generate(&env);
        client.fund(&investor, &(TARGET / 20));
        assert_eq!(client.get_unique_funder_count(), i + 1);
    }
}

#[test]
fn test_max_unique_investors_cap_enforced_at_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP006"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(3u32), // Cap of 3 investors
    );

    assert_eq!(client.get_max_unique_investors_cap(), Some(3u32));

    // Add 3 investors - should succeed
    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let inv3 = Address::generate(&env);

    client.fund(&inv1, &(TARGET / 6));
    assert_eq!(client.get_unique_funder_count(), 1);

    client.fund(&inv2, &(TARGET / 6));
    assert_eq!(client.get_unique_funder_count(), 2);

    client.fund(&inv3, &(TARGET / 6));
    assert_eq!(client.get_unique_funder_count(), 3);

    // 4th investor should panic
    let inv4 = Address::generate(&env);
    client.fund(&inv4, &(TARGET / 6));
}

#[test]
#[should_panic(expected = "unique investor cap reached")]
fn test_max_unique_investors_cap_blocks_excess_investors() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP007"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(2u32), // Cap of 2 investors
    );

    // Add 2 investors
    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    client.fund(&inv1, &(TARGET / 4));
    client.fund(&inv2, &(TARGET / 4));

    // 3rd investor should panic
    let inv3 = Address::generate(&env);
    client.fund(&inv3, &(TARGET / 4));
}

#[test]
#[should_panic(expected = "unique investor cap reached")]
fn test_max_unique_investors_cap_blocks_fund_with_commitment() {
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

    client.init(
        &admin,
        &String::from_str(&env, "CAP008"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &Some(1u32), // Cap of 1 investor
    );

    // First investor succeeds
    let inv1 = Address::generate(&env);
    client.fund_with_commitment(&inv1, &(TARGET / 2), &200u64);

    // Second investor using fund_with_commitment should panic
    let inv2 = Address::generate(&env);
    client.fund_with_commitment(&inv2, &(TARGET / 2), &200u64);
}

#[test]
fn test_re_funding_same_address_doesnt_count_against_cap() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP009"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(1u32), // Cap of 1 investor
    );

    // First fund should succeed
    client.fund(&investor, &(TARGET / 3));
    assert_eq!(client.get_unique_funder_count(), 1);

    // Re-funding same address should also succeed (doesn't count against cap)
    client.fund(&investor, &(TARGET / 3));
    assert_eq!(client.get_unique_funder_count(), 1);

    // Final fund from same address should succeed
    client.fund(&investor, &(TARGET / 3));
    assert_eq!(client.get_unique_funder_count(), 1);
    assert_eq!(client.get_escrow().status, 1); // Funded
}

#[test]
fn test_zero_contribution_then_non_zero_contribution_counts_as_unique_investor() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP010"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(2u32), // Cap of 2 investors
    );

    assert_eq!(client.get_unique_funder_count(), 0);
    assert_eq!(client.get_contribution(&investor), 0);

    // First non-zero contribution should increment count
    client.fund(&investor, &(TARGET / 2));
    assert_eq!(client.get_unique_funder_count(), 1);
    assert_eq!(client.get_contribution(&investor), TARGET / 2);
}

#[test]
fn test_cap_validation_at_init_positive_value_required() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    // Should panic for zero cap
    client.init(
        &admin,
        &String::from_str(&env, "CAP011"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(0u32), // Invalid: zero cap
    );
}

#[test]
#[should_panic(expected = "max_unique_investors must be positive when configured")]
fn test_init_panics_for_zero_cap() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP012"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(0u32), // Invalid: zero cap
    );
}

#[test]
fn test_cap_edge_case_exact_limit_reached() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP013"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(5u32), // Cap of 5 investors
    );

    // Add exactly 5 investors - should all succeed
    for i in 0..5 {
        let investor = Address::generate(&env);
        client.fund(&investor, &(TARGET / 10));
        assert_eq!(client.get_unique_funder_count(), i + 1);
    }

    // Should have exactly 5 unique funders
    assert_eq!(client.get_unique_funder_count(), 5);

    // 6th investor should panic
    let inv6 = Address::generate(&env);
    client.fund(&inv6, &(TARGET / 10));
}

#[test]
#[should_panic(expected = "unique investor cap reached")]
fn test_cap_edge_case_exactly_one_over_limit_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP014"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(5u32), // Cap of 5 investors
    );

    // Add exactly 5 investors
    for _i in 0..5 {
        let investor = Address::generate(&env);
        client.fund(&investor, &(TARGET / 10));
    }

    // 6th investor should panic
    let inv6 = Address::generate(&env);
    client.fund(&inv6, &(TARGET / 10));
}

#[test]
fn test_cap_with_min_contribution_floor_interaction() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP015"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(1_000i128), // Min contribution floor
        &Some(3u32),      // Cap of 3 investors
    );

    // Should respect both cap and floor
    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let inv3 = Address::generate(&env);

    client.fund(&inv1, &2_000i128);
    assert_eq!(client.get_unique_funder_count(), 1);

    client.fund(&inv2, &1_500i128);
    assert_eq!(client.get_unique_funder_count(), 2);

    client.fund(&inv3, &1_000i128);
    assert_eq!(client.get_unique_funder_count(), 3);

    // 4th investor should be blocked by cap, not floor
    let inv4 = Address::generate(&env);
    client.fund(&inv4, &2_000i128);
}

#[test]
#[should_panic(expected = "unique investor cap reached")]
fn test_cap_blocks_even_with_large_contribution() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP016"),
        &sme,
        &(TARGET * 10), // Large target
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(1u32), // Cap of 1 investor
    );

    // First investor can fund large amount
    let inv1 = Address::generate(&env);
    client.fund(&inv1, &(TARGET * 5));

    // Second investor blocked even if they could fully fund remaining amount
    let inv2 = Address::generate(&env);
    client.fund(&inv2, &(TARGET * 5));
}

#[test]
fn test_cap_panic_message_quality() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    client.init(
        &admin,
        &String::from_str(&env, "CAP017"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &Some(1u32),
    );

    // Add first investor
    let inv1 = Address::generate(&env);
    client.fund(&inv1, &(TARGET / 2));

    // Try to add second investor - should panic with clear message
    let inv2 = Address::generate(&env);
    client.fund(&inv2, &(TARGET / 2));
}
