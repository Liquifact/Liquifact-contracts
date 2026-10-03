//! Bounded boundary coverage for yield-tier validation and selection.
//!
//! The production code currently has no explicit upper bound on tier-table
//! length. A 64-entry probe records that gap without creating an unbounded run.
//! It also accepts a first tier with `min_lock_secs == 0`, although selection
//! short-circuits lock zero to the base yield, making that tier unreachable at
//! its exact threshold.

use super::{assert_contract_error, deploy, LiquifactEscrowClient, TARGET};
use crate::{EscrowError, YieldTier, YieldTierPreview};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec as SorobanVec};

const BOUNDED_TIER_COUNT_PROBE: u32 = 64;

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
        &String::from_str(env, "YTBOUND"),
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
        &String::from_str(env, "YTERROR"),
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
fn test_yield_bps_accepts_zero_and_max_boundaries() {
    for yield_bps in [0, 10_000] {
        let env = Env::default();
        let tiers = SorobanVec::from_array(&env, [tier(1, yield_bps)]);
        let client = init_with_tiers(&env, 0, Some(tiers));

        assert_eq!(
            client.get_yield_tiers().get_unchecked(0).yield_bps,
            yield_bps
        );
    }
}

#[test]
fn test_yield_bps_rejects_just_outside_range_with_typed_error() {
    for yield_bps in [-1, 10_001] {
        let env = Env::default();
        let tiers = SorobanVec::from_array(&env, [tier(1, yield_bps)]);

        assert_init_error(&env, 0, tiers, EscrowError::TierYieldOutOfRange);
    }
}

#[test]
fn test_yield_bps_rejects_just_below_base_with_typed_error() {
    let env = Env::default();
    let tiers = SorobanVec::from_array(&env, [tier(1, 799)]);

    assert_init_error(&env, 800, tiers, EscrowError::TierYieldBelowBase);
}

#[test]
fn test_lock_boundaries_accept_zero_and_u64_max() {
    let env = Env::default();
    let tiers = SorobanVec::from_array(&env, [tier(0, 100), tier(u64::MAX, 10_000)]);
    let client = init_with_tiers(&env, 0, Some(tiers));

    let stored = client.get_yield_tiers();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored.get_unchecked(0).min_lock_secs, 0);
    assert_eq!(stored.get_unchecked(1).min_lock_secs, u64::MAX);

    assert_eq!(
        client.preview_yield_tier(&1i128, &0u64),
        YieldTierPreview {
            effective_yield_bps: 0,
            matched_lock_secs: 0
        }
    );
    assert_eq!(
        client.preview_yield_tier(&1i128, &u64::MAX),
        YieldTierPreview {
            effective_yield_bps: 10_000,
            matched_lock_secs: u64::MAX
        }
    );
}

#[test]
fn test_lock_order_rejects_equal_and_decreasing_boundaries() {
    for second_lock in [10, 9] {
        let env = Env::default();
        let tiers = SorobanVec::from_array(&env, [tier(10, 100), tier(second_lock, 200)]);

        assert_init_error(&env, 0, tiers, EscrowError::TierLockNotIncreasing);
    }
}

#[test]
fn test_yield_order_rejects_decrease_with_typed_error() {
    let env = Env::default();
    let tiers = SorobanVec::from_array(&env, [tier(1, 500), tier(2, 499)]);

    assert_init_error(&env, 0, tiers, EscrowError::TierYieldNotNonDecreasing);
}

#[test]
fn test_tier_table_accepts_zero_and_single_entry_lengths() {
    let empty_env = Env::default();
    let empty = SorobanVec::new(&empty_env);
    let empty_client = init_with_tiers(&empty_env, 0, Some(empty));
    assert_eq!(empty_client.get_yield_tiers().len(), 0);

    let single_env = Env::default();
    let single = SorobanVec::from_array(&single_env, [tier(1, 0)]);
    let single_client = init_with_tiers(&single_env, 0, Some(single));
    assert_eq!(single_client.get_yield_tiers().len(), 1);
}

#[test]
fn test_tier_table_has_no_explicit_length_cap_bounded_probe() {
    let env = Env::default();
    let mut tiers = SorobanVec::new(&env);
    for index in 0..BOUNDED_TIER_COUNT_PROBE {
        tiers.push_back(tier(u64::from(index) + 1, 800));
    }

    let client = init_with_tiers(&env, 800, Some(tiers));

    assert_eq!(client.get_yield_tiers().len(), BOUNDED_TIER_COUNT_PROBE);
}

#[test]
fn test_preview_selection_uses_bounded_boundary_matrix() {
    let env = Env::default();
    let tiers = SorobanVec::from_array(
        &env,
        [
            tier(1, 100),
            tier(10, 200),
            tier(100, 300),
            tier(u64::MAX, 10_000),
        ],
    );
    let client = init_with_tiers(&env, 0, Some(tiers));

    let cases = [
        (
            0,
            YieldTierPreview {
                effective_yield_bps: 0,
                matched_lock_secs: 0,
            },
        ),
        (
            1,
            YieldTierPreview {
                effective_yield_bps: 100,
                matched_lock_secs: 1,
            },
        ),
        (
            2,
            YieldTierPreview {
                effective_yield_bps: 100,
                matched_lock_secs: 1,
            },
        ),
        (
            9,
            YieldTierPreview {
                effective_yield_bps: 100,
                matched_lock_secs: 1,
            },
        ),
        (
            10,
            YieldTierPreview {
                effective_yield_bps: 200,
                matched_lock_secs: 10,
            },
        ),
        (
            11,
            YieldTierPreview {
                effective_yield_bps: 200,
                matched_lock_secs: 10,
            },
        ),
        (
            99,
            YieldTierPreview {
                effective_yield_bps: 200,
                matched_lock_secs: 10,
            },
        ),
        (
            100,
            YieldTierPreview {
                effective_yield_bps: 300,
                matched_lock_secs: 100,
            },
        ),
        (
            101,
            YieldTierPreview {
                effective_yield_bps: 300,
                matched_lock_secs: 100,
            },
        ),
        (
            u64::MAX - 1,
            YieldTierPreview {
                effective_yield_bps: 300,
                matched_lock_secs: 100,
            },
        ),
        (
            u64::MAX,
            YieldTierPreview {
                effective_yield_bps: 10_000,
                matched_lock_secs: u64::MAX,
            },
        ),
    ];

    for (lock, expected) in cases {
        assert_eq!(
            client.preview_yield_tier(&i128::MAX, &lock),
            expected,
            "unexpected tier resolution for lock={lock}"
        );
    }
}
