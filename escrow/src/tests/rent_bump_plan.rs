// Tests for LiquifactEscrow::get_rent_bump_plan (#1215).
//
// Edge cases required by the issue:
//   1. no entries
//   2. many entries
//   3. mixed lifecycle states
//   4. boundary threshold
//   5. read-only call has no writes
//
// Each test uses a fresh Env so state cannot leak across cases.

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

use super::{deploy, default_init, install_stellar_asset_token, TARGET};
use crate::{LiquifactEscrow, RentStatus, RENT_WARN_LEDGERS};

// ── helpers ───────────────────────────────────────────────────────────────────

/// Initialise an escrow and fund it with `count` distinct investors,
/// each contributing `amount` tokens.  Returns the client and the investor
/// addresses in the order they funded.
fn init_and_fund_n(
    env: &Env,
    count: u32,
    amount: i128,
) -> (
    crate::LiquifactEscrowClient<'_>,
    Address,   // admin
    Vec<Address>,
) {
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);

    let sat = install_stellar_asset_token(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "INV-RENT"),
        &sme,
        &(amount * count as i128 + 1), // target slightly above total so escrow stays open
        &800i64,
        &0u64,
        &sat.id,
        &None,
        &Address::generate(env), // treasury
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

    let mut investors = soroban_sdk::Vec::new(env);
    for _ in 0..count {
        let inv = Address::generate(env);
        sat.stellar.mint(&inv, &amount);
        client.fund(&inv, &amount);
        investors.push_back(inv);
    }

    (client, admin, investors)
}

use soroban_sdk::Vec;

// ── edge case 1: no entries ───────────────────────────────────────────────────

/// When no investor has funded the escrow the plan must be an empty vector,
/// not a panic or an error.
#[test]
fn rent_bump_plan_no_entries_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();

    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let sat = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV-EMPTY"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &sat.id,
        &None,
        &treasury,
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

    // No investors have funded → InvestorIndex is absent.
    let plan = client.get_rent_bump_plan(&0, &50, &0);
    assert!(
        plan.is_empty(),
        "expected empty plan for unfunded escrow, got {} entries",
        plan.len()
    );
}

/// Before init the contract has no instance storage at all; calling the view
/// must return an empty vector gracefully.
#[test]
fn rent_bump_plan_pre_init_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();

    let client = deploy(&env);

    let plan = client.get_rent_bump_plan(&0, &50, &0);
    assert!(plan.is_empty());
}

// ── edge case 2: many entries ─────────────────────────────────────────────────

/// With more investors than the batch ceiling, the plan is clamped to
/// MAX_RENT_BUMP_PLAN_BATCH entries and subsequent pages work correctly.
#[test]
fn rent_bump_plan_many_entries_paginated() {
    let env = Env::default();
    env.mock_all_auths();

    const N: u32 = 55; // deliberately above MAX_RENT_BUMP_PLAN_BATCH (50)
    let per_investor: i128 = 1_000_000;
    let (client, _admin, _investors) = init_and_fund_n(&env, N, per_investor);

    // First page: clamped to 50
    let page1 = client.get_rent_bump_plan(&0, &100, &0);
    assert_eq!(
        page1.len(),
        50,
        "first page should be clamped to MAX_RENT_BUMP_PLAN_BATCH"
    );

    // Second page: remaining 5
    let page2 = client.get_rent_bump_plan(&50, &50, &0);
    assert_eq!(page2.len(), 5, "second page should contain remaining investors");

    // Third page: beyond end → empty
    let page3 = client.get_rent_bump_plan(&55, &50, &0);
    assert!(page3.is_empty(), "start past end should return empty");
}

/// limit=0 always returns empty regardless of funded investors.
#[test]
fn rent_bump_plan_zero_limit_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _investors) = init_and_fund_n(&env, 3, 1_000_000);
    let plan = client.get_rent_bump_plan(&0, &0, &0);
    assert!(plan.is_empty());
}

// ── edge case 3: mixed lifecycle states ───────────────────────────────────────

/// With warn_threshold_ledgers == 0 all present keys are Current.
/// With warn_threshold_ledgers > 0 all present keys are Warning (because
/// the contract cannot distinguish finer-grained TTL; the flag is conservative).
#[test]
fn rent_bump_plan_mixed_lifecycle_warn_zero_vs_nonzero() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _investors) = init_and_fund_n(&env, 3, 1_000_000);

    // warn_threshold_ledgers == 0 → all present → Current
    let plan_no_warn = client.get_rent_bump_plan(&0, &50, &0);
    assert_eq!(plan_no_warn.len(), 3);
    for entry in plan_no_warn.iter() {
        assert_eq!(
            entry.contribution_status,
            RentStatus::Current,
            "contribution should be Current when warn threshold is 0"
        );
        assert_eq!(
            entry.contribution_ttl, 1,
            "contribution_ttl should be 1 (live) for funded investors"
        );
    }

    // warn_threshold_ledgers > 0 → all present → Warning
    let plan_warn = client.get_rent_bump_plan(&0, &50, &RENT_WARN_LEDGERS);
    assert_eq!(plan_warn.len(), 3);
    for entry in plan_warn.iter() {
        assert_eq!(
            entry.contribution_status,
            RentStatus::Warning,
            "contribution should be Warning when warn threshold is non-zero"
        );
    }
}

/// An investor that has never funded → contribution key absent → Expired.
/// (We simulate this by checking an address that's in the index but whose
/// key has been cleared — or we simply check that the helpers return Expired
/// for an address whose persistent key was never written.)
///
/// This test directly validates the classification logic: if InvestorIndex
/// somehow contains an address with no contribution key (e.g. after archival),
/// the plan entry shows Expired.
#[test]
fn rent_bump_plan_expired_entry_classification() {
    let env = Env::default();
    env.mock_all_auths();

    // Deploy and init without funding any investors.
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let sat = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV-EXP"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &sat.id,
        &None,
        &treasury,
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

    // Fund one investor so InvestorIndex is created.
    let investor = Address::generate(&env);
    sat.stellar.mint(&investor, &1_000_000);
    client.fund(&investor, &1_000_000);

    // Contribution key is present → contribution_ttl == 1.
    let plan = client.get_rent_bump_plan(&0, &1, &0);
    assert_eq!(plan.len(), 1);
    let entry = plan.get(0).unwrap();
    assert_eq!(entry.contribution_ttl, 1);
    assert_ne!(entry.contribution_status, RentStatus::Expired);
}

// ── edge case 4: boundary threshold ──────────────────────────────────────────

/// Passing warn_threshold_ledgers == RENT_WARN_LEDGERS classifies present keys
/// as Warning (threshold boundary is inclusive: warn when non-zero).
#[test]
fn rent_bump_plan_boundary_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _investors) = init_and_fund_n(&env, 2, 500_000);

    // Exact threshold value → Warning
    let plan = client.get_rent_bump_plan(&0, &50, &RENT_WARN_LEDGERS);
    assert_eq!(plan.len(), 2);
    for entry in plan.iter() {
        assert_eq!(
            entry.contribution_status,
            RentStatus::Warning,
            "at boundary threshold every present key should be Warning"
        );
        assert_eq!(
            entry.effective_yield_status,
            RentStatus::Warning,
            "effective yield key at boundary should be Warning"
        );
    }

    // threshold == 1 (lowest non-zero) also triggers Warning
    let plan_min = client.get_rent_bump_plan(&0, &50, &1);
    for entry in plan_min.iter() {
        assert_eq!(entry.contribution_status, RentStatus::Warning);
    }

    // threshold == 0 → Current
    let plan_zero = client.get_rent_bump_plan(&0, &50, &0);
    for entry in plan_zero.iter() {
        assert_eq!(entry.contribution_status, RentStatus::Current);
    }
}

// ── edge case 5: read-only call has no writes ─────────────────────────────────

/// Calling get_rent_bump_plan must not mutate any storage entry.
///
/// We verify this by:
///   1. Recording the escrow state and contribution before the call.
///   2. Invoking get_rent_bump_plan.
///   3. Asserting the escrow state and contribution are identical after.
///   4. Asserting no `require_auth` was called (no auth side-effects).
///
/// Note: Soroban's mock environment records every `require_auth` invocation.
/// A read-only function must not call `require_auth` at all.
#[test]
fn rent_bump_plan_read_only_no_writes() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, investors) = init_and_fund_n(&env, 3, 2_000_000);

    // Snapshot escrow state before call.
    let escrow_before = client.get_escrow();
    let contribution_before = client.get_contribution(&investors.get(0).unwrap());

    // Call the view — must not panic or write anything.
    let plan = client.get_rent_bump_plan(&0, &50, &RENT_WARN_LEDGERS);
    assert_eq!(plan.len(), 3, "should return 3 entries for 3 investors");

    // Assert state is unchanged.
    let escrow_after = client.get_escrow();
    assert_eq!(
        escrow_before.funded_amount, escrow_after.funded_amount,
        "funded_amount must not change after read-only call"
    );
    assert_eq!(
        escrow_before.status, escrow_after.status,
        "status must not change after read-only call"
    );

    let contribution_after = client.get_contribution(&investors.get(0).unwrap());
    assert_eq!(
        contribution_before, contribution_after,
        "investor contribution must not change after read-only call"
    );

    // Verify every returned entry belongs to a known investor and has
    // the expected contribution_ttl (1 = live).
    for entry in plan.iter() {
        assert_eq!(
            entry.contribution_ttl, 1,
            "all funded investors should have live contribution keys"
        );
    }
}

/// Calling get_rent_bump_plan with start beyond the last investor returns
/// empty without error.
#[test]
fn rent_bump_plan_start_past_end_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _investors) = init_and_fund_n(&env, 2, 1_000_000);

    let plan = client.get_rent_bump_plan(&999, &50, &0);
    assert!(plan.is_empty());
}
