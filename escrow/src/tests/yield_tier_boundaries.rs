use super::*;
use soroban_sdk::{testutils::Address as _, Vec as SorobanVec};

// ── Yield-tier boundary and rejection tests (issue #709) ─────────────────────
//
// Tests the exact boundary conditions for yield-tier validation and
// effective-yield selection, asserting typed error codes.

// ============================================================================
// validate_yield_tiers_table boundaries
// ============================================================================

/// Exactly at the upper bound: yield_bps = 10_000 is valid.
#[test]
fn test_tier_yield_upper_bound_exact() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 10_000,
    });

    let escrow = client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIERUB01"),
        &sme,
        &1_000i128,
        &0i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_eq!(escrow.yield_bps, 0);
}

/// One over the upper bound: yield_bps = 10_001 must reject.
#[test]
fn test_tier_yield_upper_bound_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 10_001,
    });

    let result = client.try_init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIERUB02"),
        &sme,
        &1_000i128,
        &0i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_contract_error(result, EscrowError::TierYieldOutOfRange);
}

/// Exactly at the lower bound: yield_bps = 0 is valid (when base_yield = 0).
#[test]
fn test_tier_yield_lower_bound_exact() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 0,
    });

    let escrow = client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIERLB01"),
        &sme,
        &1_000i128,
        &0i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_eq!(escrow.yield_bps, 0);
}

/// Negative yield_bps must reject.
#[test]
fn test_tier_yield_negative() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: -1,
    });

    let result = client.try_init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIERNEG01"),
        &sme,
        &1_000i128,
        &0i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_contract_error(result, EscrowError::TierYieldOutOfRange);
}

/// Tier yield exactly at base yield is valid (yield_bps == base_yield).
#[test]
fn test_tier_yield_exact_base() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 800,
    });

    let escrow = client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIERBASE01"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_eq!(escrow.yield_bps, 800);
}

/// Tier yield one below base yield must reject.
#[test]
fn test_tier_yield_one_below_base() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 799,
    });

    let result = client.try_init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIERBLW01"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_contract_error(result, EscrowError::TierYieldBelowBase);
}

/// Lock seconds strictly increasing is valid.
#[test]
fn test_tier_lock_strictly_increasing() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 800,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 900,
    });

    let escrow = client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIERLK01"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_eq!(escrow.yield_bps, 800);
}

/// Equal lock seconds must reject (not strictly increasing).
#[test]
fn test_tier_lock_equal() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 800,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });

    let result = client.try_init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIERLKEQ"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_contract_error(result, EscrowError::TierLockNotIncreasing);
}

/// Decreasing lock seconds must reject.
#[test]
fn test_tier_lock_decreasing() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 800,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });

    let result = client.try_init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIERLKDEC"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_contract_error(result, EscrowError::TierLockNotIncreasing);
}

/// Non-decreasing yield is valid (equal yields across tiers).
#[test]
fn test_tier_yield_equal_across_tiers() {
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
        yield_bps: 900,
    });

    let escrow = client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIERYEQ"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_eq!(escrow.yield_bps, 800);
}

/// Decreasing yield across tiers must reject.
#[test]
fn test_tier_yield_decreasing_across_tiers() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 1000,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 900,
    });

    let result = client.try_init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIERYDEC"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_contract_error(result, EscrowError::TierYieldNotNonDecreasing);
}

/// Single tier with yield_bps = 0 and base_yield = 0 is valid.
#[test]
fn test_tier_single_zero_yield_zero_base() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 0,
        yield_bps: 0,
    });

    let escrow = client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIERSGL0"),
        &sme,
        &1_000i128,
        &0i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_eq!(escrow.yield_bps, 0);
}

/// Empty tier table is accepted (returns early).
#[test]
fn test_tier_empty_table() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let tiers = SorobanVec::new(&env);

    let escrow = client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIEREMPTY"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_eq!(escrow.yield_bps, 800);
}

/// None yield tiers is accepted.
#[test]
fn test_tier_none() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    let escrow = client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TIERNONE"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_eq!(escrow.yield_bps, 800);
}

// ============================================================================
// effective_yield_for_commitment boundaries (via preview_yield_tier)
// ============================================================================

/// Commitment lock = 0 always returns base yield.
#[test]
fn test_effective_yield_lock_zero() {
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
        &soroban_sdk::String::from_str(&env, "EFFLK0"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let (eff, lock) = client.preview_yield_tier(&1_000i128, &0u64);
    assert_eq!(eff, 800, "lock=0 must return base yield");
    assert_eq!(lock, 0, "lock=0 must return matched_lock=0");
}

/// Commitment lock exactly at tier.min_lock_secs matches that tier.
#[test]
fn test_effective_yield_lock_exact_tier() {
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
        &soroban_sdk::String::from_str(&env, "EFFLK1"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let (eff, lock) = client.preview_yield_tier(&1_000i128, &100u64);
    assert_eq!(eff, 900, "lock=100 must match tier with min_lock=100");
    assert_eq!(lock, 100, "matched_lock must be tier's min_lock_secs");
}

/// Commitment lock one below tier.min_lock_secs does not match.
#[test]
fn test_effective_yield_lock_one_below_tier() {
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
        &soroban_sdk::String::from_str(&env, "EFFLKB1"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let (eff, lock) = client.preview_yield_tier(&1_000i128, &99u64);
    assert_eq!(eff, 800, "lock=99 must NOT match tier with min_lock=100");
    assert_eq!(lock, 0, "matched_lock must be 0 when no tier matched");
}

/// Commitment lock above all tiers returns the highest tier yield.
#[test]
fn test_effective_yield_lock_above_all_tiers() {
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
        &soroban_sdk::String::from_str(&env, "EFFLKA1"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let (eff, lock) = client.preview_yield_tier(&1_000i128, &500u64);
    assert_eq!(eff, 1000, "lock=500 must match highest tier (min_lock=200)");
    assert_eq!(lock, 200, "matched_lock must be 200");
}

/// Commitment lock between two tiers matches the lower tier.
#[test]
fn test_effective_yield_lock_between_tiers() {
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
        min_lock_secs: 300,
        yield_bps: 1200,
    });

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "EFFLKB2"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let (eff, lock) = client.preview_yield_tier(&1_000i128, &250u64);
    assert_eq!(eff, 900, "lock=250 must match tier 0 (min_lock=100)");
    assert_eq!(lock, 100, "matched_lock must be 100");
}

/// Commitment lock exactly at tier boundary: lock=200 with tiers [100, 200, 300].
#[test]
fn test_effective_yield_lock_exact_middle_tier() {
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
    tiers.push_back(YieldTier {
        min_lock_secs: 300,
        yield_bps: 1200,
    });

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "EFFLKM1"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let (eff, lock) = client.preview_yield_tier(&1_000i128, &200u64);
    assert_eq!(eff, 1000, "lock=200 must match tier 1");
    assert_eq!(lock, 200, "matched_lock must be 200");
}

/// No tier table: preview returns base yield.
#[test]
fn test_effective_yield_no_tier_table() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "EFFNOTI"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let (eff, lock) = client.preview_yield_tier(&1_000i128, &500u64);
    assert_eq!(eff, 800, "no tiers must return base yield");
    assert_eq!(lock, 0, "matched_lock must be 0");
}

// ============================================================================
// fund_with_commitment rejection tests
// ============================================================================

/// Second deposit via fund_with_commitment must reject (TieredSecondDeposit).
#[test]
fn test_tiered_second_deposit_rejects() {
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
        &soroban_sdk::String::from_str(&env, "TIER2ND"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    client.fund_with_commitment(&inv, &1_000i128, &100u64);

    let result = client.try_fund_with_commitment(&inv, &1_000i128, &200u64);
    assert_contract_error(result, EscrowError::TieredSecondDeposit);
}

/// Commitment lock exceeding maturity must reject (CommitmentLockExceedsMaturity).
#[test]
fn test_commitment_lock_exceeds_maturity() {
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
        &soroban_sdk::String::from_str(&env, "TIERMEX"),
        &sme,
        &10_000i128,
        &800i64,
        &1_000u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let result = client.try_fund_with_commitment(&inv, &1_000i128, &1_001u64);
    assert_contract_error(result, EscrowError::CommitmentLockExceedsMaturity);
}

/// Commitment lock exactly at maturity boundary: now + lock == maturity is valid.
#[test]
fn test_commitment_lock_exact_maturity_boundary() {
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
        &soroban_sdk::String::from_str(&env, "TIERMBD"),
        &sme,
        &10_000i128,
        &800i64,
        &1_000u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let escrow = client.fund_with_commitment(&inv, &1_000i128, &1_000u64);
    assert_eq!(
        escrow.status, 0,
        "funded_amount (1000) < funding_target (10000) so status stays open"
    );
}

/// Commitment lock one past maturity boundary: now + lock > maturity must reject.
#[test]
fn test_commitment_lock_one_past_maturity() {
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
        &soroban_sdk::String::from_str(&env, "TIERMXP"),
        &sme,
        &10_000i128,
        &800i64,
        &1_000u64,
        &tok,
        &None,
        &tre,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let result = client.try_fund_with_commitment(&inv, &1_000i128, &1_001u64);
    assert_contract_error(result, EscrowError::CommitmentLockExceedsMaturity);
}

/// Zero maturity (no lock) accepts any commitment lock.
#[test]
fn test_commitment_lock_zero_maturity_accepts_any() {
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
        &soroban_sdk::String::from_str(&env, "TIERZMT"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let escrow = client.fund_with_commitment(&inv, &1_000i128, &1_000_000u64);
    assert_eq!(escrow.status, 0);
}
