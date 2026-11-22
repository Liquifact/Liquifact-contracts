//! CPU and memory budget regression tests for maximum-size batch entrypoints.
//!
//! Each of the five bounded batch entrypoints (`fund_batch`, `refund_batch`,
//! `push_investor_payouts`, `settle_batch`, and `batch_record_collateral`) is
//! exercised at its compile-time maximum (`MAX_*_BATCH = 50`) under worst-case
//! shape (all new investors, allowlist active, real SAC token transfers where
//! applicable). After resetting the Soroban host budget immediately before the
//! measured call, both CPU instructions and memory bytes are asserted to stay
//! below documented named regression thresholds.
//!
//! See `docs/escrow-gas-storage-notes.md` for the documented thresholds,
//! Soroban network limits, and measured baselines.

use super::{
    deploy_id, deploy_with_id, install_stellar_asset_token, setup, LiquifactEscrowClient,
    MAX_FUND_BATCH, MAX_SETTLE_BATCH,
};
use crate::{MAX_CLAIM_PUSH_BATCH, MAX_COLLATERAL_BATCH, MAX_REFUND_BATCH};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, EnvTestConfig, Ledger as _},
    Address, Env, String, Symbol, Vec as SorobanVec,
};

// ---------------------------------------------------------------------------
// Documented Soroban network limits and per-entrypoint regression thresholds
// ---------------------------------------------------------------------------

/// Soroban mainnet per-transaction CPU instruction limit (100M instructions).
pub(crate) const SOROBAN_NETWORK_CPU_LIMIT: u64 = 100_000_000;

/// Soroban mainnet per-transaction memory limit (40 MiB).
pub(crate) const SOROBAN_NETWORK_MEM_LIMIT: u64 = 40 * 1024 * 1024;

/// Regression margin multiplier (percentage of measured baseline).
/// 120% allows up to 20% variance from minor SDK/compiler changes while
/// catching accidental O(n^2) storage reads or loop regressions.
pub(crate) const BUDGET_MARGIN_PCT: u64 = 120;

const fn with_margin(baseline: u64) -> u64 {
    baseline * BUDGET_MARGIN_PCT / 100
}

// fund_batch (n = 50, all new investors, allowlist active, real SAC transfers)
// Measured baseline: ~159.9M CPU, ~50.2M bytes.
pub(crate) const FUND_BATCH_CPU_THRESHOLD: u64 = with_margin(160_000_000);
pub(crate) const FUND_BATCH_MEM_THRESHOLD: u64 = with_margin(51_000_000);

// refund_batch (n = 50, all new investors, allowlist active, real SAC transfers)
// Measured baseline: ~89.4M CPU, ~32.8M bytes.
pub(crate) const REFUND_BATCH_CPU_THRESHOLD: u64 = with_margin(90_000_000);
pub(crate) const REFUND_BATCH_MEM_THRESHOLD: u64 = with_margin(33_000_000);

// push_investor_payouts (n = 50, all new investors, allowlist active, real SAC transfers)
// Measured baseline: ~72.5M CPU, ~26.0M bytes.
pub(crate) const PUSH_PAYOUTS_CPU_THRESHOLD: u64 = with_margin(73_000_000);
pub(crate) const PUSH_PAYOUTS_MEM_THRESHOLD: u64 = with_margin(27_000_000);

// settle_batch (n = 50 funded child escrows)
// Measured baseline: ~36.0M CPU, ~19.2M bytes (under 50% of network limits).
pub(crate) const SETTLE_BATCH_CPU_THRESHOLD: u64 = with_margin(36_000_000);
pub(crate) const SETTLE_BATCH_MEM_THRESHOLD: u64 = with_margin(20_000_000);

// batch_record_collateral (n = 50 items)
// Measured baseline: ~1.13M CPU, ~98K bytes (under 2% of network limits).
pub(crate) const COLLATERAL_BATCH_CPU_THRESHOLD: u64 = with_margin(1_200_000);
pub(crate) const COLLATERAL_BATCH_MEM_THRESHOLD: u64 = with_margin(100_000);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn budget_env() -> Env {
    Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    })
}

fn assert_within_budget(entrypoint: &str, env: &Env, cpu_threshold: u64, mem_threshold: u64) {
    let budget = env.cost_estimate().budget();
    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();

    assert!(
        cpu <= cpu_threshold,
        "{entrypoint} CPU regression: used {cpu} instructions (threshold {cpu_threshold}, network limit {SOROBAN_NETWORK_CPU_LIMIT})"
    );
    assert!(
        mem <= mem_threshold,
        "{entrypoint} memory regression: used {mem} bytes (threshold {mem_threshold}, network limit {SOROBAN_NETWORK_MEM_LIMIT})"
    );
}

// ---------------------------------------------------------------------------
// 1. fund_batch at MAX_FUND_BATCH (50) — all new investors, allowlist active
// ---------------------------------------------------------------------------

#[test]
fn fund_batch_max_size_budget() {
    let env = budget_env();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();

    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);

    let per_investor: i128 = 1_000;
    let target: i128 = per_investor * (MAX_FUND_BATCH as i128);

    client.init(
        &admin,
        &String::from_str(&env, "BUDGET_FUND"),
        &sme,
        &target,
        &800i64,
        &0u64,
        &token.id,
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

    // Worst-case shape: allowlist active + MAX_FUND_BATCH brand-new investors.
    client.set_allowlist_active(&true);

    let mut entries: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    for _ in 0..MAX_FUND_BATCH {
        let investor = Address::generate(&env);
        client.set_investor_allowlisted(&investor, &true);
        token.stellar.mint(&investor, &per_investor);
        entries.push_back((investor, per_investor));
    }

    env.cost_estimate().budget().reset_unlimited();
    let escrow = client.fund_batch(&entries);
    assert_within_budget(
        "fund_batch",
        &env,
        FUND_BATCH_CPU_THRESHOLD,
        FUND_BATCH_MEM_THRESHOLD,
    );

    assert_eq!(escrow.funded_amount, target);
    assert_eq!(escrow.status, 1);
}

// ---------------------------------------------------------------------------
// 2. refund_batch at MAX_REFUND_BATCH (50) — all new investors, allowlist active
// ---------------------------------------------------------------------------

#[test]
fn refund_batch_max_size_budget() {
    let env = budget_env();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();

    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);

    let per_investor: i128 = 1_000;
    // Target strictly above total funded so funding remains open and cancellable.
    let target: i128 = per_investor * (MAX_REFUND_BATCH as i128) + 1;

    client.init(
        &admin,
        &String::from_str(&env, "BUDGET_REFUND"),
        &sme,
        &target,
        &800i64,
        &0u64,
        &token.id,
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

    client.set_allowlist_active(&true);

    let mut investors: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..MAX_REFUND_BATCH {
        let investor = Address::generate(&env);
        client.set_investor_allowlisted(&investor, &true);
        token.stellar.mint(&investor, &per_investor);
        client.fund(&investor, &per_investor);
        investors.push_back(investor);
    }

    client.cancel_funding();

    env.cost_estimate().budget().reset_unlimited();
    let (refunded_count, total_refunded) = client.refund_batch(&investors);
    assert_within_budget(
        "refund_batch",
        &env,
        REFUND_BATCH_CPU_THRESHOLD,
        REFUND_BATCH_MEM_THRESHOLD,
    );

    assert_eq!(refunded_count, MAX_REFUND_BATCH);
    assert_eq!(total_refunded, per_investor * (MAX_REFUND_BATCH as i128));
}

// ---------------------------------------------------------------------------
// 3. push_investor_payouts at MAX_CLAIM_PUSH_BATCH (50) — all new investors, allowlist active
// ---------------------------------------------------------------------------

#[test]
fn push_investor_payouts_max_size_budget() {
    let env = budget_env();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let token = install_stellar_asset_token(&env);
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    let per_investor: i128 = 1_000;
    let yield_bps: i64 = 400;
    let target: i128 = per_investor * (MAX_CLAIM_PUSH_BATCH as i128);

    client.init(
        &admin,
        &String::from_str(&env, "BUDGET_PAYOUT"),
        &sme,
        &target,
        &yield_bps,
        &0u64,
        &token.id,
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

    client.set_allowlist_active(&true);

    let mut investors: SorobanVec<Address> = SorobanVec::new(&env);
    let mut yield_total: i128 = 0;
    for _ in 0..MAX_CLAIM_PUSH_BATCH {
        let investor = Address::generate(&env);
        client.set_investor_allowlisted(&investor, &true);
        token.stellar.mint(&investor, &per_investor);
        client.fund(&investor, &per_investor);
        yield_total += per_investor * (yield_bps as i128) / 10_000;
        investors.push_back(investor);
    }

    token.stellar.mint(&contract_id, &yield_total);
    client.settle();

    env.cost_estimate().budget().reset_unlimited();
    let (paid_count, total_paid) = client.push_investor_payouts(&investors);
    assert_within_budget(
        "push_investor_payouts",
        &env,
        PUSH_PAYOUTS_CPU_THRESHOLD,
        PUSH_PAYOUTS_MEM_THRESHOLD,
    );

    assert_eq!(paid_count, MAX_CLAIM_PUSH_BATCH);
    assert_eq!(total_paid, target + yield_total);
}

// ---------------------------------------------------------------------------
// 4. settle_batch at MAX_SETTLE_BATCH (50) — 50 funded escrows
// ---------------------------------------------------------------------------

#[test]
fn settle_batch_max_size_budget() {
    let env = budget_env();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    env.mock_all_auths_allowing_non_root_auth();

    let mut info = env.ledger().get();
    info.timestamp = 1_000;
    env.ledger().set(info);

    let coordinator_id = deploy_id(&env);
    let coordinator = LiquifactEscrowClient::new(&env, &coordinator_id);

    let mut escrows: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..MAX_SETTLE_BATCH {
        let id = deploy_id(&env);
        let child = LiquifactEscrowClient::new(&env, &id);
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let token = Address::generate(&env);
        let treasury = Address::generate(&env);

        child.init(
            &admin,
            &String::from_str(&env, "BUDGET_SETTLE"),
            &sme,
            &1_000i128,
            &800i64,
            &0u64,
            &token,
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
        child.fund(&Address::generate(&env), &1_000i128);
        escrows.push_back(id);
    }

    env.cost_estimate().budget().reset_unlimited();
    let results = coordinator.settle_batch(&escrows);
    assert_within_budget(
        "settle_batch",
        &env,
        SETTLE_BATCH_CPU_THRESHOLD,
        SETTLE_BATCH_MEM_THRESHOLD,
    );

    assert_eq!(results.len(), MAX_SETTLE_BATCH);
    for i in 0..MAX_SETTLE_BATCH {
        assert_eq!(results.get(i).unwrap(), 0);
    }
}

// ---------------------------------------------------------------------------
// 5. batch_record_collateral at MAX_COLLATERAL_BATCH (50)
// ---------------------------------------------------------------------------

#[test]
fn batch_record_collateral_max_size_budget() {
    let env = budget_env();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();

    let (client, admin, sme) = setup(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "BUDGET_COLLAT"),
        &sme,
        &100_000i128,
        &800i64,
        &0u64,
        &token,
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

    let asset: Symbol = symbol_short!("USDC");
    let mut items: SorobanVec<(Symbol, i128)> = SorobanVec::new(&env);
    for i in 1..=MAX_COLLATERAL_BATCH {
        items.push_back((asset.clone(), (i as i128) * 1_000));
    }

    env.cost_estimate().budget().reset_unlimited();
    let commitment = client.batch_record_collateral(&items);
    assert_within_budget(
        "batch_record_collateral",
        &env,
        COLLATERAL_BATCH_CPU_THRESHOLD,
        COLLATERAL_BATCH_MEM_THRESHOLD,
    );

    assert_eq!(commitment.asset, asset);
    assert_eq!(commitment.amount, (MAX_COLLATERAL_BATCH as i128) * 1_000);
}
