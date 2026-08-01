#![cfg(test)]
//! Tests for the admin-guarded yield-tier setter (issue #1090).
//!
//! Coverage:
//! - an in-bounds ladder is accepted, persisted, and read back through `get_yield_tiers`
//! - each individual bound is rejected with `EscrowError::YieldTierTableInvalid`
//! - a rejected call leaves the previously stored ladder untouched
//! - a caller without admin authorization is rejected

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use super::{assert_contract_error, default_init, setup, YieldTier};
use crate::EscrowError;

/// A well-formed two-tier ladder: strictly increasing locks, non-decreasing bps, in range.
fn valid_tiers(env: &Env) -> soroban_sdk::Vec<YieldTier> {
    vec![
        env,
        YieldTier {
            min_lock_secs: 30 * 86_400,
            yield_bps: 500,
        },
        YieldTier {
            min_lock_secs: 90 * 86_400,
            yield_bps: 900,
        },
    ]
}

#[test]
fn set_yield_tiers_accepts_in_bounds_table() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    let tiers = valid_tiers(&env);
    client.set_yield_tiers(&tiers);

    let stored = client.get_yield_tiers();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored.get(0).unwrap().min_lock_secs, 30 * 86_400);
    assert_eq!(stored.get(0).unwrap().yield_bps, 500);
    assert_eq!(stored.get(1).unwrap().min_lock_secs, 90 * 86_400);
    assert_eq!(stored.get(1).unwrap().yield_bps, 900);
}

#[test]
fn set_yield_tiers_replaces_the_whole_table() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    client.set_yield_tiers(&valid_tiers(&env));
    assert_eq!(client.get_yield_tiers().len(), 2);

    // A shorter ladder must replace, not merge with, the previous one.
    client.set_yield_tiers(&vec![
        &env,
        YieldTier {
            min_lock_secs: 1,
            yield_bps: 0,
        },
    ]);

    let stored = client.get_yield_tiers();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored.get(0).unwrap().min_lock_secs, 1);
    assert_eq!(stored.get(0).unwrap().yield_bps, 0);
}

#[test]
fn set_yield_tiers_accepts_boundary_values() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    // yield_bps == 0 and yield_bps == 10_000 are both inclusive bounds.
    client.set_yield_tiers(&vec![
        &env,
        YieldTier {
            min_lock_secs: 1,
            yield_bps: 0,
        },
        YieldTier {
            min_lock_secs: 2,
            yield_bps: 10_000,
        },
    ]);

    let stored = client.get_yield_tiers();
    assert_eq!(stored.get(0).unwrap().yield_bps, 0);
    assert_eq!(stored.get(1).unwrap().yield_bps, 10_000);
}

#[test]
fn set_yield_tiers_rejects_empty_table() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_contract_error(
        client.try_set_yield_tiers(&vec![&env]),
        EscrowError::YieldTierTableInvalid,
    );
}

#[test]
fn set_yield_tiers_rejects_yield_bps_above_max() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_contract_error(
        client.try_set_yield_tiers(&vec![
            &env,
            YieldTier {
                min_lock_secs: 1,
                yield_bps: 10_001,
            },
        ]),
        EscrowError::YieldTierTableInvalid,
    );
}

#[test]
fn set_yield_tiers_rejects_negative_yield_bps() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_contract_error(
        client.try_set_yield_tiers(&vec![
            &env,
            YieldTier {
                min_lock_secs: 1,
                yield_bps: -1,
            },
        ]),
        EscrowError::YieldTierTableInvalid,
    );
}

#[test]
fn set_yield_tiers_rejects_zero_first_lock() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    // prev_lock starts at 0, so the first tier must declare a strictly positive lock.
    assert_contract_error(
        client.try_set_yield_tiers(&vec![
            &env,
            YieldTier {
                min_lock_secs: 0,
                yield_bps: 100,
            },
        ]),
        EscrowError::YieldTierTableInvalid,
    );
}

#[test]
fn set_yield_tiers_rejects_non_increasing_locks() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_contract_error(
        client.try_set_yield_tiers(&vec![
            &env,
            YieldTier {
                min_lock_secs: 100,
                yield_bps: 100,
            },
            YieldTier {
                min_lock_secs: 100,
                yield_bps: 200,
            },
        ]),
        EscrowError::YieldTierTableInvalid,
    );
}

#[test]
fn set_yield_tiers_rejects_decreasing_yield_bps() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_contract_error(
        client.try_set_yield_tiers(&vec![
            &env,
            YieldTier {
                min_lock_secs: 100,
                yield_bps: 900,
            },
            YieldTier {
                min_lock_secs: 200,
                yield_bps: 500,
            },
        ]),
        EscrowError::YieldTierTableInvalid,
    );
}

#[test]
fn rejected_set_leaves_previous_table_intact() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    client.set_yield_tiers(&valid_tiers(&env));

    // Second tier is out of range, so the whole call must be rejected atomically.
    assert_contract_error(
        client.try_set_yield_tiers(&vec![
            &env,
            YieldTier {
                min_lock_secs: 10,
                yield_bps: 100,
            },
            YieldTier {
                min_lock_secs: 20,
                yield_bps: 10_001,
            },
        ]),
        EscrowError::YieldTierTableInvalid,
    );

    let stored = client.get_yield_tiers();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored.get(0).unwrap().yield_bps, 500);
    assert_eq!(stored.get(1).unwrap().yield_bps, 900);
}

#[test]
fn set_yield_tiers_rejects_non_admin_caller() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    let _intruder = Address::generate(&env);

    // Drop every mocked authorization so the admin require_auth gate is actually exercised.
    env.set_auths(&[]);

    let result = client.try_set_yield_tiers(&valid_tiers(&env));
    assert!(
        result.is_err(),
        "set_yield_tiers must reject a caller lacking admin authorization"
    );

    // And the ladder configured at init must be unchanged.
    env.mock_all_auths();
    assert!(client.get_yield_tiers().is_empty());
}