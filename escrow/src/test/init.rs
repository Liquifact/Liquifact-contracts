//! Tests for `LiquifactEscrow::init` invariants.
//!
//! Each test is named after the invariant number from the module-level table
//! so reviewers can cross-reference the spec directly.
//!
//! Run coverage locally:
//! ```sh
//! cargo llvm-cov --package liquifact_escrow --features testutils --fail-under-lines 95 --html
//! ```

// This file is only compiled when the "testutils" feature is active.
// The feature is set automatically by `cargo test` when the crate declares
// it (see Cargo.toml), and by cargo-llvm-cov via the --features flag.
#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{
    EscrowError, EscrowParams, LiquifactEscrow, LiquifactEscrowClient, YieldTier, MAX_BPS,
    MAX_TIERS,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return a minimal *valid* `EscrowParams` that passes all invariants.
/// Individual tests override specific fields to trigger errors.
fn valid_params(env: &Env) -> EscrowParams {
    EscrowParams {
        depositor: Address::generate(env),
        recipient: Address::generate(env),
        amount: 1_000_000,
        yield_bps: 500,
        floor_bps: 100,
        target_bps: 500,
        cap_bps: 1_000,
        tiers: vec![env],
    }
}

fn new_client(env: &Env) -> (Env, LiquifactEscrowClient) {
    let env = env.clone();
    env.mock_all_auths();
    let contract = env.register_contract(None, LiquifactEscrow);
    let client = LiquifactEscrowClient::new(&env, &contract);
    (env, client)
}

/// Build a tier vec with `n` monotonically-increasing entries, all valid bps.
fn monotonic_tiers(env: &Env, n: u32) -> soroban_sdk::Vec<YieldTier> {
    let mut tiers = vec![env];
    for i in 0..n {
        tiers.push_back(YieldTier {
            min_amount: (i as i128 + 1) * 1_000,
            bps: 100 * (i + 1),
        });
    }
    tiers
}

// ---------------------------------------------------------------------------
// Happy-path
// ---------------------------------------------------------------------------

#[test]
fn test_init_minimal_valid_no_tiers() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    assert!(client.try_init(&valid_params(&env)).is_ok());
}

#[test]
fn test_init_with_tiers_valid() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.tiers = monotonic_tiers(&env, 3);
    assert!(client.try_init(&p).is_ok());
}

#[test]
fn test_init_max_tiers_valid() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.tiers = monotonic_tiers(&env, MAX_TIERS);
    assert!(client.try_init(&p).is_ok());
}

#[test]
fn test_init_yield_bps_zero_valid() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.yield_bps = 0;
    p.floor_bps = 0;
    p.target_bps = 0;
    p.cap_bps = 0;
    assert!(client.try_init(&p).is_ok());
}

#[test]
fn test_init_all_bps_at_max_valid() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.yield_bps = MAX_BPS;
    p.floor_bps = MAX_BPS;
    p.target_bps = MAX_BPS;
    p.cap_bps = MAX_BPS;
    assert!(client.try_init(&p).is_ok());
}

#[test]
fn test_get_state_round_trips_params() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let p = valid_params(&env);
    let dep = p.depositor.clone();
    let rec = p.recipient.clone();
    client.init(&p);
    let s = client.get_state();
    assert_eq!(s.depositor, dep);
    assert_eq!(s.recipient, rec);
    assert_eq!(s.amount, 1_000_000);
    assert_eq!(s.yield_bps, 500);
    assert_eq!(s.floor_bps, 100);
    assert_eq!(s.target_bps, 500);
    assert_eq!(s.cap_bps, 1_000);
}

// ---------------------------------------------------------------------------
// INV-1 — positive amount
// ---------------------------------------------------------------------------

#[test]
fn test_inv1_amount_zero_rejected() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.amount = 0;
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::InvalidAmount.into()
    );
}

#[test]
fn test_inv1_amount_negative_rejected() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.amount = -1;
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::InvalidAmount.into()
    );
}

#[test]
fn test_inv1_amount_i128_min_rejected() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.amount = i128::MIN;
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::InvalidAmount.into()
    );
}

#[test]
fn test_inv1_amount_one_accepted() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.amount = 1;
    assert!(client.try_init(&p).is_ok());
}

#[test]
fn test_inv1_amount_i128_max_accepted() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.amount = i128::MAX;
    assert!(client.try_init(&p).is_ok());
}

// ---------------------------------------------------------------------------
// INV-2 — yield_bps range
// ---------------------------------------------------------------------------

#[test]
fn test_inv2_yield_bps_above_max_rejected() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.yield_bps = MAX_BPS + 1;
    p.cap_bps = MAX_BPS + 1; // keep cap ≥ target to isolate this error
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::InvalidYieldBps.into()
    );
}

#[test]
fn test_inv2_yield_bps_at_max_accepted() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.yield_bps = MAX_BPS;
    p.floor_bps = MAX_BPS;
    p.target_bps = MAX_BPS;
    p.cap_bps = MAX_BPS;
    assert!(client.try_init(&p).is_ok());
}

// ---------------------------------------------------------------------------
// INV-9 — cap_bps range
// ---------------------------------------------------------------------------

#[test]
fn test_inv9_cap_above_max_rejected() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.cap_bps = MAX_BPS + 1;
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::CapOutOfRange.into()
    );
}

#[test]
fn test_inv9_cap_at_max_accepted() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.cap_bps = MAX_BPS;
    p.target_bps = MAX_BPS;
    p.floor_bps = MAX_BPS;
    assert!(client.try_init(&p).is_ok());
}

// ---------------------------------------------------------------------------
// INV-7 — floor ≤ target
// ---------------------------------------------------------------------------

#[test]
fn test_inv7_floor_exceeds_target_rejected() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.floor_bps = 600;
    p.target_bps = 500;
    p.cap_bps = 1_000;
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::FloorExceedsTarget.into()
    );
}

#[test]
fn test_inv7_floor_equals_target_accepted() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.floor_bps = 500;
    p.target_bps = 500;
    p.cap_bps = 500;
    assert!(client.try_init(&p).is_ok());
}

// ---------------------------------------------------------------------------
// INV-8 — target ≤ cap
// ---------------------------------------------------------------------------

#[test]
fn test_inv8_target_exceeds_cap_rejected() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.floor_bps = 100;
    p.target_bps = 900;
    p.cap_bps = 800;
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::TargetExceedsCap.into()
    );
}

#[test]
fn test_inv8_target_equals_cap_accepted() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.floor_bps = 500;
    p.target_bps = 800;
    p.cap_bps = 800;
    assert!(client.try_init(&p).is_ok());
}

// ---------------------------------------------------------------------------
// INV-4 — tier table size
// ---------------------------------------------------------------------------

#[test]
fn test_inv4_tier_table_too_large_rejected() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.tiers = monotonic_tiers(&env, MAX_TIERS + 1);
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::TierTableTooLarge.into()
    );
}

#[test]
fn test_inv4_exactly_max_tiers_accepted() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.tiers = monotonic_tiers(&env, MAX_TIERS);
    assert!(client.try_init(&p).is_ok());
}

// ---------------------------------------------------------------------------
// INV-5 — tier table monotonicity
// ---------------------------------------------------------------------------

#[test]
fn test_inv5_duplicate_min_amount_rejected() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.tiers = vec![
        &env,
        YieldTier {
            min_amount: 1_000,
            bps: 100,
        },
        YieldTier {
            min_amount: 1_000,
            bps: 200,
        },
    ];
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::TierTableNotMonotonic.into()
    );
}

#[test]
fn test_inv5_descending_min_amount_rejected() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.tiers = vec![
        &env,
        YieldTier {
            min_amount: 5_000,
            bps: 200,
        },
        YieldTier {
            min_amount: 1_000,
            bps: 100,
        },
    ];
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::TierTableNotMonotonic.into()
    );
}

#[test]
fn test_inv5_single_tier_accepted() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.tiers = vec![
        &env,
        YieldTier {
            min_amount: 1_000,
            bps: 100,
        },
    ];
    assert!(client.try_init(&p).is_ok());
}

// ---------------------------------------------------------------------------
// INV-6 — per-tier bps range
// ---------------------------------------------------------------------------

#[test]
fn test_inv6_tier_bps_above_max_rejected() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.tiers = vec![
        &env,
        YieldTier {
            min_amount: 1_000,
            bps: MAX_BPS + 1,
        },
    ];
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::InvalidTierBps.into()
    );
}

#[test]
fn test_inv6_tier_bps_at_max_accepted() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.tiers = vec![
        &env,
        YieldTier {
            min_amount: 1_000,
            bps: MAX_BPS,
        },
    ];
    assert!(client.try_init(&p).is_ok());
}

#[test]
fn test_inv6_tier_bps_zero_accepted() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.tiers = vec![
        &env,
        YieldTier {
            min_amount: 1_000,
            bps: 0,
        },
    ];
    assert!(client.try_init(&p).is_ok());
}

#[test]
fn test_inv6_invalid_bps_in_second_tier() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.tiers = vec![
        &env,
        YieldTier {
            min_amount: 1_000,
            bps: 100,
        },
        YieldTier {
            min_amount: 2_000,
            bps: MAX_BPS + 5,
        },
    ];
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::InvalidTierBps.into()
    );
}

// ---------------------------------------------------------------------------
// INV-11 — one-shot guarantee
// ---------------------------------------------------------------------------

#[test]
fn test_inv11_double_init_rejected() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    client.init(&valid_params(&env));
    assert_eq!(
        client.try_init(&valid_params(&env)).unwrap_err().unwrap(),
        EscrowError::AlreadyInitialized.into()
    );
}

// ---------------------------------------------------------------------------
// Error priority
// ---------------------------------------------------------------------------

#[test]
fn test_error_priority_amount_before_yield_bps() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.amount = 0;
    p.yield_bps = MAX_BPS + 1;
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::InvalidAmount.into()
    );
}

#[test]
fn test_error_priority_cap_before_floor_target() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.cap_bps = MAX_BPS + 1;
    p.floor_bps = 9_000;
    p.target_bps = 100;
    assert_eq!(
        client.try_init(&p).unwrap_err().unwrap(),
        EscrowError::CapOutOfRange.into()
    );
}

#[test]
fn test_edge_all_bps_zero_accepted() {
    let env = Env::default();
    let (_, client) = new_client(&env);
    let mut p = valid_params(&env);
    p.yield_bps = 0;
    p.floor_bps = 0;
    p.target_bps = 0;
    p.cap_bps = 0;
    assert!(client.try_init(&p).is_ok());
}
