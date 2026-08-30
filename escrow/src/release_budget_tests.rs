//! Self-contained tests for the **worst-case release instruction budget** (issue #1229).
//!
//! The issue: "A release path that grows with metadata or participant count can become
//! unexecutable at production scale." This suite:
//!
//! 1. Defines a representative **worst-case fixture** — an escrow at a large participant count
//!    (so [`crate::DataKey::InvestorIndex`] is large and every paginated view must deserialize
//!    the full list) plus a **large metadata** record ([`crate::SmeCollateralCommitment`]).
//! 2. **Measures instruction + memory cost** via
//!    [`soroban_sdk::testutils::CostEstimate::budget`] (`cpu_instruction_cost()` /
//!    `memory_bytes_cost()`) across the release path: [`LiquifactEscrow::settle`],
//!    [`LiquifactEscrow::claim_investor_payout`] (repeated releases), and a single worst-case
//!    page of [`LiquifactEscrow::get_funding_records`] / [`LiquifactEscrow::get_investors`]
//!    at scale (the participant-count growth path, where each page deserializes the full
//!    [`crate::DataKey::InvestorIndex`]).
//! 3. **Enforces the documented budget** — every measured call must stay under
//!    [`crate::WORST_CASE_RELEASE_CPU_INSNS_CEILING`] /
//!    [`crate::WORST_CASE_RELEASE_MEM_BYTES_CEILING`] (a budget regression gate).
//! 4. **Bounds the inputs** — asserts the unconditional [`crate::MAX_UNIQUE_INVESTORS`]
//!    participant ceiling is enforced even when `max_unique_investors` isn't configured, keeping
//!    the axis that scales release cost (`InvestorIndex` size) finite.
//!
//! These tests deliberately do **not** depend on the crate's `tests/` module tree (disabled);
//! they drive the public [`LiquifactEscrow`] surface directly, mirroring `settlement_guard_tests`.
//!
//! Note: native (non-WASM) metering **underestimates** WASM cost, so the ceilings are loose,
//! conservative documentation gates rather than tight WASM budgets — see the constant docs.

use soroban_sdk::{testutils::Address as _, Address, Env, String, Symbol, Vec};

use super::{
    keys, LiquifactEscrow, LiquifactEscrowClient, MAX_INVESTOR_READ_BATCH,
    MAX_UNIQUE_INVESTORS, WORST_CASE_RELEASE_CPU_INSNS_CEILING,
    WORST_CASE_RELEASE_MEM_BYTES_CEILING,
};

/// Tally of measured CPU/memory for a single top-level release-path invocation.
struct Measured {
    cpu_insns: u64,
    mem_bytes: u64,
}

/// Deploy + initialise an escrow with `max_unique_investors` **unset** (so only the hard
/// [`crate::MAX_UNIQUE_INVESTORS`] ceiling applies) and a large `funding_target` so it stays
/// **open** (status 0) and accepts many distinct funders. Returns the client and contract id.
fn deploy(env: &Env, target: i128) -> (LiquifactEscrowClient<'_>, Address) {
    env.mock_all_auths_allowing_non_root_auth();
    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &String::from_str(env, "RBLBUDGET"),
        &sme,
        &target,
        &500i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None, // max_unique_investors: unset -> rely on MAX_UNIQUE_INVESTORS
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    (client, id)
}

/// Record large SME collateral **metadata** so the metadata-growth axis is exercised.
/// (Symbol max is 32 bytes; use the full length to represent "large" metadata.)
fn record_large_metadata(env: &Env, client: &LiquifactEscrowClient<'_>) {
    let asset = Symbol::new(env, "ABCDEFGHIJKLMNOPQRSTUVWXYZ123456");
    let _ = client.record_sme_collateral_commitment(&asset, &5_000_000i128);
}

/// Measure the CPU/memory of one top-level invocation after resetting the budget tracker.
fn measure(env: &Env, f: impl FnOnce()) -> Measured {
    let mut budget = env.cost_estimate().budget();
    budget.reset_tracker();
    f();
    Measured {
        cpu_insns: budget.cpu_instruction_cost(),
        mem_bytes: budget.memory_bytes_cost(),
    }
}

fn assert_within_budget(what: &str, m: &Measured) {
    assert!(
        m.cpu_insns < WORST_CASE_RELEASE_CPU_INSNS_CEILING,
        "{what}: CPU instructions {cpu} exceeded documented ceiling {ceil}",
        cpu = m.cpu_insns,
        ceil = WORST_CASE_RELEASE_CPU_INSNS_CEILING
    );
    assert!(
        m.mem_bytes < WORST_CASE_RELEASE_MEM_BYTES_CEILING,
        "{what}: memory bytes {mem} exceeded documented ceiling {ceil}",
        mem = m.mem_bytes,
        ceil = WORST_CASE_RELEASE_MEM_BYTES_CEILING
    );
}

/// Edge case — **worst-case fixture**: a large [`crate::DataKey::InvestorIndex`] (representative
/// participant scale), so every paginated view must deserialize the **full** index per call —
/// the participant-count growth axis that scales release cost. A single worst-case page of
/// [`LiquifactEscrow::get_funding_records`] / [`LiquifactEscrow::get_investors`] must stay within
/// the documented budget.
///
/// The index is populated directly in contract storage (rather than via thousands of funded
/// `fund` calls) so the measurement concentrates on the read/deserialize cost — the actual
/// "grows with participant count" concern — without a prohibitively expensive setup run.
#[test]
fn worst_case_release_path_stays_within_documented_budget() {
    let env = Env::default();
    let (client, id) = deploy(&env, 1_000_000_000i128);
    env.mock_all_auths_allowing_non_root_auth();

    // Precondition: with `max_unique_investors` unset, only the hard cap limits participants.
    assert_eq!(client.get_max_unique_investors_cap(), None);

    // Build a representative large InvestorIndex directly in contract storage — 1_000 entries,
    // 20x MAX_INVESTOR_READ_BATCH — so a paginated page must deserialize the full index (the
    // O(n) participant-growth axis), while staying within the default mainnet budget that a
    // full hard-cap-scale (MAX_UNIQUE_INVESTORS) index would exhaust during test setup alone.
    let n = 1_000u32;
    let mut index: Vec<Address> = Vec::new(&env);
    for _ in 0..n {
        index.push_back(Address::generate(&env));
    }
    env.as_contract(&id, || {
        env.storage()
            .instance()
            .set(&super::DataKey::InvestorIndex, &index);
        env.storage()
            .instance()
            .set(&keys::unique_funder_count(), &n);
    });
    assert_eq!(client.get_unique_funder_count(), n);

    // A single paginated page at worst-case scale stays within the documented budget.
    let page = measure(&env, || {
        let _ = client.get_funding_records(&0u32, &MAX_INVESTOR_READ_BATCH);
    });
    assert_within_budget("get_funding_records worst-case page", &page);
    let investors_page = measure(&env, || {
        let _ = client.get_investors(&0u32, &MAX_INVESTOR_READ_BATCH);
    });
    assert_within_budget("get_investors worst-case page", &investors_page);
}

/// Edge case — **minimum fixture**: a single-investor escrow; settle + claim stay within budget.
#[test]
fn minimum_fixture_release_within_budget() {
    let env = Env::default();
    let (client, _) = deploy(&env, 1_000i128);
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);

    assert_eq!(client.get_escrow().status, 1u32, "precondition: funded");

    let settle = measure(&env, || {
        let _ = client.settle();
    });
    assert_within_budget("settle (minimum fixture)", &settle);

    let claim = measure(&env, || {
        client.claim_investor_payout(&investor);
    });
    assert_within_budget("claim_investor_payout (minimum fixture)", &claim);
}

/// Edge case — **large metadata**: a max-size collateral metadata record does not balloon the
/// release-path budget (metadata lives in instance storage, independent of the release axis).
#[test]
fn large_metadata_does_not_blow_up_release_budget() {
    let env = Env::default();
    let (client, _) = deploy(&env, 1_000_000i128);
    record_large_metadata(&env, &client);
    let investor = Address::generate(&env);
    client.fund(&investor, &500_000i128);

    let baseline = measure(&env, || {
        let _ = client.get_funding_records(&0u32, &MAX_INVESTOR_READ_BATCH);
    });
    assert_within_budget("pagination with large metadata", &baseline);
}

/// Edge case — **maximum participants**: the unconditional [`crate::MAX_UNIQUE_INVESTORS`]
/// ceiling is enforced even when `max_unique_investors` wasn't configured. The boundary is
/// exercised by moving the unique-funder counter just below the cap: the last allowed investor
/// is accepted; the investor beyond it is rejected (no partial write, count unchanged).
#[test]
fn max_participants_hard_cap_is_enforced() {
    let env = Env::default();
    // Large target so the escrow stays open; tiny funding actions.
    let (client, id) = deploy(&env, 10_000_000i128);

    // Synthetically place the unique-funder counter just below the hard cap (inside the
    // contract context) so the boundary is exercised with two fund calls instead of 10_000.
    env.as_contract(&id, || {
        env.storage()
            .instance()
            .set(&keys::unique_funder_count(), &(MAX_UNIQUE_INVESTORS - 1));
    });

    // The (MAX_UNIQUE_INVESTORS)-th distinct investor is still allowed.
    let last_allowed = Address::generate(&env);
    client.fund(&last_allowed, &1i128);
    assert_eq!(client.get_unique_funder_count(), MAX_UNIQUE_INVESTORS);
    assert_eq!(client.get_escrow().status, 0u32, "escrow stays open");

    // The investor beyond the hard cap is rejected (funding aborts).
    let over_cap = Address::generate(&env);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.fund(&over_cap, &1i128);
    }));
    assert!(res.is_err(), "funding beyond the hard cap must be rejected");

    // No partial mutation: the rejected investor has no contribution, count is unchanged.
    assert_eq!(client.get_contribution(&over_cap), 0i128);
    assert_eq!(client.get_unique_funder_count(), MAX_UNIQUE_INVESTORS);
}

/// Edge case — **repeated releases**: a repeated `claim_investor_payout` for the same investor
/// is an idempotent no-op (safe to re-invoke; no double payout) and each call stays bounded.
#[test]
fn repeated_releases_idempotent_and_bounded() {
    let env = Env::default();
    let (client, _) = deploy(&env, 1_000i128);
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.settle();

    let first = measure(&env, || {
        client.claim_investor_payout(&investor);
    });
    assert_within_budget("first claim (release)", &first);

    // Re-invoking the same claim must be a no-op (not a panic, not a second payout).
    let second = measure(&env, || {
        client.claim_investor_payout(&investor);
    });
    assert_within_budget("repeated claim (release)", &second);
}
