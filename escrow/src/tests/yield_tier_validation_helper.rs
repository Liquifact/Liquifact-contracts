//! Unit coverage for the extracted [`crate::validate_yield_tier`] helper.
//!
//! These tests exercise the shared per-tier validation primitive directly (no
//! `Env` / contract deploy needed, since the helper is pure and returns a typed
//! [`EscrowError`]). They lock in that the extracted helper preserves the exact
//! rejection behaviour previously inlined in `validate_yield_tiers_table`:
//! yield-range, below-base, strictly-increasing lock, and non-decreasing yield,
//! including the order in which the checks fire.

use crate::{validate_yield_tier, EscrowError, YieldTier};

fn tier(min_lock_secs: u64, yield_bps: i64) -> YieldTier {
    YieldTier {
        min_lock_secs,
        yield_bps,
    }
}

// --- first tier (no predecessor) ---------------------------------------------

#[test]
fn accepts_tier_within_range_and_at_or_above_base() {
    // yield equal to base is allowed.
    assert_eq!(validate_yield_tier(&tier(100, 800), 800, None), Ok(()));
    // yield strictly above base is allowed.
    assert_eq!(validate_yield_tier(&tier(100, 900), 800, None), Ok(()));
}

#[test]
fn accepts_yield_range_boundaries() {
    // 0 and 10_000 are both inclusive bounds; base 0 makes them >= base.
    assert_eq!(validate_yield_tier(&tier(0, 0), 0, None), Ok(()));
    assert_eq!(validate_yield_tier(&tier(0, 10_000), 0, None), Ok(()));
}

#[test]
fn rejects_yield_below_zero() {
    assert_eq!(
        validate_yield_tier(&tier(100, -1), 0, None),
        Err(EscrowError::TierYieldOutOfRange)
    );
}

#[test]
fn rejects_yield_above_ten_thousand() {
    assert_eq!(
        validate_yield_tier(&tier(100, 10_001), 0, None),
        Err(EscrowError::TierYieldOutOfRange)
    );
}

#[test]
fn rejects_yield_below_base() {
    assert_eq!(
        validate_yield_tier(&tier(100, 700), 800, None),
        Err(EscrowError::TierYieldBelowBase)
    );
}

// --- ordering of checks ------------------------------------------------------

#[test]
fn range_check_precedes_base_check() {
    // yield is both out of range AND below base; the range error wins.
    assert_eq!(
        validate_yield_tier(&tier(100, -5), 800, None),
        Err(EscrowError::TierYieldOutOfRange)
    );
}

#[test]
fn base_check_precedes_predecessor_checks() {
    // yield below base is reported even when a predecessor would also fail the
    // lock ordering — base is validated before the predecessor comparison.
    let prev = tier(200, 900);
    assert_eq!(
        validate_yield_tier(&tier(100, 700), 800, Some(&prev)),
        Err(EscrowError::TierYieldBelowBase)
    );
}

// --- subsequent tiers (with predecessor) -------------------------------------

#[test]
fn accepts_strictly_increasing_lock_and_non_decreasing_yield() {
    let prev = tier(100, 900);
    // lock strictly greater, yield strictly greater.
    assert_eq!(validate_yield_tier(&tier(200, 1000), 800, Some(&prev)), Ok(()));
    // lock strictly greater, yield equal (non-decreasing allows equality).
    assert_eq!(validate_yield_tier(&tier(200, 900), 800, Some(&prev)), Ok(()));
}

#[test]
fn rejects_equal_lock_as_not_increasing() {
    let prev = tier(100, 900);
    assert_eq!(
        validate_yield_tier(&tier(100, 1000), 800, Some(&prev)),
        Err(EscrowError::TierLockNotIncreasing)
    );
}

#[test]
fn rejects_decreasing_lock() {
    let prev = tier(200, 900);
    assert_eq!(
        validate_yield_tier(&tier(100, 1000), 800, Some(&prev)),
        Err(EscrowError::TierLockNotIncreasing)
    );
}

#[test]
fn rejects_decreasing_yield_across_tiers() {
    let prev = tier(100, 1000);
    // lock increases, but yield drops below the predecessor.
    assert_eq!(
        validate_yield_tier(&tier(200, 900), 800, Some(&prev)),
        Err(EscrowError::TierYieldNotNonDecreasing)
    );
}

#[test]
fn lock_check_precedes_yield_non_decreasing_check() {
    // Both lock (equal) and yield (decreasing) rules are violated; the lock
    // error is reported first.
    let prev = tier(100, 1000);
    assert_eq!(
        validate_yield_tier(&tier(100, 900), 800, Some(&prev)),
        Err(EscrowError::TierLockNotIncreasing)
    );
}
