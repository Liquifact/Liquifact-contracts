//! Validation and round-trip tests for yield-tier tables passed to `init`.
//!
//! Covers all four typed `EscrowError` rejection cases documented in
//! `docs/yield-tier-errors.md` (`TierLockNotIncreasing`, `TierYieldBelowBase`,
//! `TierYieldOutOfRange`, `TierYieldNotNonDecreasing`), as well as valid table
//! round-tripping through `get_yield_tiers` and `get_yield_tiers_page`.

use super::{assert_contract_error, deploy, LiquifactEscrowClient, TARGET};
use crate::{EscrowError, YieldTier};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec as SorobanVec};

fn tier(min_lock_secs: u64, yield_bps: i64) -> YieldTier {
    YieldTier {
        min_lock_secs,
        yield_bps,
    }
}

fn init_with_tiers<'a>(
    env: &'a Env,
    base_yield_bps: i64,
    tiers: Option<SorobanVec<YieldTier>>,
) -> LiquifactEscrowClient<'a> {
    env.mock_all_auths();
    let client = deploy(env);
    client.init(
        &Address::generate(env),
        &String::from_str(env, "YTSET"),
        &Address::generate(env),
        &TARGET,
        &base_yield_bps,
        &0u64,
        &Address::generate(env),
        &None,
        &Address::generate(env),
        &tiers,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    client
}

fn assert_init_error(
    env: &Env,
    base_yield_bps: i64,
    tiers: SorobanVec<YieldTier>,
    expected: EscrowError,
) {
    env.mock_all_auths();
    let client = deploy(env);
    let result = client.try_init(
        &Address::generate(env),
        &String::from_str(env, "YTERR"),
        &Address::generate(env),
        &TARGET,
        &base_yield_bps,
        &0u64,
        &Address::generate(env),
        &None,
        &Address::generate(env),
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
    assert_contract_error(result, expected);
}

#[test]
fn test_rejects_non_increasing_min_lock_secs() {
    // Duplicate min_lock_secs
    let env = Env::default();
    let duplicate = SorobanVec::from_array(&env, [tier(86_400, 900), tier(86_400, 1_000)]);
    assert_init_error(&env, 800, duplicate, EscrowError::TierLockNotIncreasing);

    // Strictly decreasing min_lock_secs
    let env = Env::default();
    let decreasing = SorobanVec::from_array(&env, [tier(172_800, 900), tier(86_400, 1_000)]);
    assert_init_error(&env, 800, decreasing, EscrowError::TierLockNotIncreasing);
}

#[test]
fn test_rejects_tier_yield_below_base() {
    let env = Env::default();
    let tiers = SorobanVec::from_array(&env, [tier(86_400, 799)]);
    assert_init_error(&env, 800, tiers, EscrowError::TierYieldBelowBase);
}

#[test]
fn test_rejects_tier_yield_out_of_range() {
    // Above 10_000 bps
    let env = Env::default();
    let too_high = SorobanVec::from_array(&env, [tier(86_400, 10_001)]);
    assert_init_error(&env, 800, too_high, EscrowError::TierYieldOutOfRange);

    // Below 0 bps
    let env = Env::default();
    let negative = SorobanVec::from_array(&env, [tier(86_400, -1)]);
    assert_init_error(&env, 0, negative, EscrowError::TierYieldOutOfRange);
}

#[test]
fn test_rejects_decreasing_tier_yield() {
    let env = Env::default();
    let tiers = SorobanVec::from_array(&env, [tier(86_400, 1_200), tier(172_800, 1_100)]);
    assert_init_error(&env, 800, tiers, EscrowError::TierYieldNotNonDecreasing);
}

#[test]
fn test_valid_table_round_trips_via_get_yield_tiers_and_page() {
    let env = Env::default();
    let expected = SorobanVec::from_array(
        &env,
        [
            tier(86_400, 800),
            tier(172_800, 1_000),
            tier(259_200, 1_000),
            tier(604_800, 10_000),
        ],
    );
    let client = init_with_tiers(&env, 800, Some(expected.clone()));

    assert_eq!(client.get_yield_tiers(), expected);

    let page_0 = client.get_yield_tiers_page(&0, &2);
    assert_eq!(
        page_0,
        SorobanVec::from_array(&env, [tier(86_400, 800), tier(172_800, 1_000)])
    );

    let page_1 = client.get_yield_tiers_page(&2, &2);
    assert_eq!(
        page_1,
        SorobanVec::from_array(&env, [tier(259_200, 1_000), tier(604_800, 10_000)])
    );

    assert_eq!(client.get_yield_tiers_page(&4, &2).len(), 0);
    assert_eq!(client.get_yield_tiers_page(&0, &0).len(), 0);
}

#[test]
fn test_large_table_round_trips_with_pagination() {
    // `validate_yield_tiers_table` currently has no hard length cap; verify a
    // 64-entry table initializes and paginates cleanly through `get_yield_tiers_page`.
    let env = Env::default();
    let mut tiers = SorobanVec::new(&env);
    for i in 0..64u64 {
        tiers.push_back(tier((i + 1) * 3_600, 800 + (i as i64) * 10));
    }

    let client = init_with_tiers(&env, 800, Some(tiers.clone()));
    assert_eq!(client.get_yield_tiers(), tiers);

    let first_page = client.get_yield_tiers_page(&0, &20);
    assert_eq!(first_page.len(), 20);
    assert_eq!(first_page.get_unchecked(0), tiers.get_unchecked(0));
    assert_eq!(first_page.get_unchecked(19), tiers.get_unchecked(19));

    let tail_page = client.get_yield_tiers_page(&60, &20);
    assert_eq!(tail_page.len(), 4);
    assert_eq!(tail_page.get_unchecked(0), tiers.get_unchecked(60));
    assert_eq!(tail_page.get_unchecked(3), tiers.get_unchecked(63));
}
