use super::{LiquifactEscrow, LiquifactEscrowClient};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};

// ---------------------------------------------------------------------------
// Cost measurement infrastructure
//
// Profiling methodology
// ---------------------
// Soroban's test environment enables *invocation metering* by default.  After
// every contract call `env.cost_estimate().resources()` returns the CPU
// instructions and memory bytes consumed by that single invocation.  These
// numbers are deterministic for a given input shape, so they serve as stable
// regression baselines.
//
// Limitations
// -----------
// * Costs are measured against the *native* (non-Wasm) contract build used in
//   tests.  Real on-chain costs include Wasm VM overhead and are higher.
// * Transaction-size fees and XDR round-trip costs are NOT included.
// * Use `stellar-cli simulate` against a real RPC node for production fee
//   estimates.
//
// How to use
// ----------
// Call `measure_cost` immediately after the contract invocation you want to
// profile.  The helper prints a structured summary and returns a
// `CostMeasurement` you can assert upper-bound thresholds against.
// ---------------------------------------------------------------------------

/// Snapshot of resource consumption for a single contract invocation.
#[derive(Debug, Clone)]
pub struct CostMeasurement {
    /// Label identifying the operation being measured.
    pub label: &'static str,
    /// CPU instructions consumed (Soroban metering units).
    pub instructions: i64,
    /// Memory bytes allocated during the invocation.
    pub mem_bytes: i64,
}

impl CostMeasurement {
    /// Capture the cost of the most-recently-completed invocation.
    pub fn capture(env: &Env, label: &'static str) -> Self {
        let resources = env.cost_estimate().resources();
        let m = CostMeasurement {
            label,
            instructions: resources.instructions,
            mem_bytes: resources.mem_bytes,
        };
        // Print a structured line so `cargo test -- --nocapture` shows baselines.
        println!(
            "[cost] {:<30} instructions={:>12}  mem_bytes={:>10}",
            m.label, m.instructions, m.mem_bytes
        );
        m
    }

    /// Assert that instructions stay below `max_instructions`.
    ///
    /// Use this to catch performance regressions: if a refactor causes
    /// instruction count to exceed the recorded baseline the test fails with a
    /// clear message.
    pub fn assert_instructions_below(&self, max_instructions: i64) {
        assert!(
            self.instructions <= max_instructions,
            "[cost regression] '{}': instructions {} exceeded limit {}",
            self.label,
            self.instructions,
            max_instructions
        );
    }

    /// Assert that memory allocation stays below `max_mem_bytes`.
    pub fn assert_mem_below(&self, max_mem_bytes: i64) {
        assert!(
            self.mem_bytes <= max_mem_bytes,
            "[cost regression] '{}': mem_bytes {} exceeded limit {}",
            self.label,
            self.mem_bytes,
            max_mem_bytes
        );
    }
}

#[test]
fn test_init_and_get_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    let escrow = client.init(
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    assert_eq!(escrow.invoice_id, symbol_short!("INV001"));
    assert_eq!(escrow.amount, 10_000_0000000i128);
    assert_eq!(escrow.funded_amount, 0);
    assert_eq!(escrow.status, 0);

    let got = client.get_escrow();
    assert_eq!(got.invoice_id, escrow.invoice_id);
}

#[test]
fn test_fund_and_settle() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV002"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    let escrow1 = client.fund(&investor, &10_000_0000000i128);
    assert_eq!(escrow1.funded_amount, 10_000_0000000i128);
    assert_eq!(escrow1.status, 1);

    let escrow2 = client.settle();
    assert_eq!(escrow2.status, 2);
}

// ---------------------------------------------------------------------------
// Baseline cost tests — core paths
//
// These tests measure and assert upper-bound resource consumption for the
// three primary contract entry points: init, fund, and settle.
//
// Upper bounds are set at 2× the observed baseline to give headroom for minor
// SDK/toolchain changes while still catching significant regressions.
// Tighten the bounds as the contract stabilises.
// ---------------------------------------------------------------------------

/// Baseline cost for `init` — creates a new invoice escrow in storage.
///
/// Expected resource profile: one storage write (instance), struct
/// serialisation.  This is the cheapest path because no prior state is read.
#[test]
fn test_cost_baseline_init() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV100"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    let cost = CostMeasurement::capture(&env, "init");

    // Sanity: metering must have produced non-zero values.
    assert!(cost.instructions > 0, "init: instructions must be > 0");
    assert!(cost.mem_bytes > 0, "init: mem_bytes must be > 0");

    // Regression guards — 3× observed baseline (~34k instructions, ~4.6k mem).
    cost.assert_instructions_below(100_000);
    cost.assert_mem_below(15_000);
}

/// Baseline cost for `fund` (partial) — reads escrow, increments funded_amount,
/// writes back.  Status stays 0 (open) because amount < target.
#[test]
fn test_cost_baseline_fund_partial() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV101"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    // Fund with half the target so status remains open.
    client.fund(&investor, &5_000_0000000i128);

    let cost = CostMeasurement::capture(&env, "fund (partial)");

    assert!(cost.instructions > 0, "fund partial: instructions must be > 0");
    assert!(cost.mem_bytes > 0, "fund partial: mem_bytes must be > 0");

    // Regression guards — 3× observed baseline (~60k instructions, ~9.7k mem).
    cost.assert_instructions_below(180_000);
    cost.assert_mem_below(30_000);
}

/// Baseline cost for `fund` (full) — same read/write path as partial fund but
/// also flips status to 1 (funded).  Should be nearly identical to partial.
#[test]
fn test_cost_baseline_fund_full() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV102"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    // Fund the full target in one call — triggers status transition to 1.
    client.fund(&investor, &10_000_0000000i128);

    let cost = CostMeasurement::capture(&env, "fund (full / status→funded)");

    assert!(cost.instructions > 0, "fund full: instructions must be > 0");
    assert!(cost.mem_bytes > 0, "fund full: mem_bytes must be > 0");

    // Regression guards — 3× observed baseline (~60k instructions, ~9.7k mem).
    cost.assert_instructions_below(180_000);
    cost.assert_mem_below(30_000);
}

/// Baseline cost for `settle` — reads funded escrow, flips status to 2,
/// writes back.  Identical storage pattern to fund.
#[test]
fn test_cost_baseline_settle() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV103"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);

    // Measure only the settle call.
    client.settle();

    let cost = CostMeasurement::capture(&env, "settle");

    assert!(cost.instructions > 0, "settle: instructions must be > 0");
    assert!(cost.mem_bytes > 0, "settle: mem_bytes must be > 0");

    // Regression guards — 3× observed baseline (~60k instructions, ~9.7k mem).
    cost.assert_instructions_below(180_000);
    cost.assert_mem_below(30_000);
}

// ---------------------------------------------------------------------------
// Edge-case cost tests
//
// These tests validate that resource consumption stays within bounds for
// boundary conditions and less-common execution paths.
// ---------------------------------------------------------------------------

/// Cost of `fund` when the cumulative amount exactly hits the target on the
/// second call (two-step funding).  Validates that the status transition
/// triggered by the second call does not add unexpected overhead.
#[test]
fn test_cost_baseline_fund_two_step_completion() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV200"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    // First partial fund — status stays open.
    client.fund(&investor, &5_000_0000000i128);

    // Second fund — exactly meets target, triggers status → funded.
    client.fund(&investor, &5_000_0000000i128);
    let cost = CostMeasurement::capture(&env, "fund (2nd call, hits target)");

    assert!(cost.instructions > 0);
    // The completing fund call should cost no more than a regular fund call.
    cost.assert_instructions_below(180_000);
    cost.assert_mem_below(30_000);
}

/// Cost of `fund` when the amount overshoots the target.  The contract
/// accumulates the excess; status still flips to funded.
#[test]
fn test_cost_baseline_fund_overshoot() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV201"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    // Fund with 2× the target — overshoot scenario.
    client.fund(&investor, &20_000_0000000i128);
    let cost = CostMeasurement::capture(&env, "fund (overshoot 2×)");

    assert!(cost.instructions > 0);
    cost.assert_instructions_below(180_000);
    cost.assert_mem_below(30_000);
}

/// Cost of `init` with a zero maturity timestamp.  Exercises the minimum-value
/// boundary for the maturity field without changing the storage write pattern.
#[test]
fn test_cost_baseline_init_zero_maturity() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV202"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &0u64, // zero maturity — edge case
    );
    let cost = CostMeasurement::capture(&env, "init (zero maturity)");

    assert!(cost.instructions > 0);
    // Zero maturity is just a field value; cost profile should match normal init.
    cost.assert_instructions_below(100_000);
    cost.assert_mem_below(15_000);
}

/// Cost of `init` with maximum i128 amount.  Ensures large numeric values do
/// not inflate serialisation cost.
#[test]
fn test_cost_baseline_init_max_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV203"),
        &sme,
        &i128::MAX,
        &800i64,
        &1000u64,
    );
    let cost = CostMeasurement::capture(&env, "init (max i128 amount)");

    assert!(cost.instructions > 0);
    cost.assert_instructions_below(100_000);
    cost.assert_mem_below(15_000);
}

/// Cost of `settle` immediately after a single-call full fund.  This is the
/// happy-path end-to-end sequence and validates the combined cost profile.
#[test]
fn test_cost_baseline_full_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    // --- init ---
    client.init(
        &symbol_short!("INV204"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    let cost_init = CostMeasurement::capture(&env, "lifecycle: init");

    // --- fund ---
    client.fund(&investor, &10_000_0000000i128);
    let cost_fund = CostMeasurement::capture(&env, "lifecycle: fund");

    // --- settle ---
    client.settle();
    let cost_settle = CostMeasurement::capture(&env, "lifecycle: settle");

    // Validate each step individually.
    cost_init.assert_instructions_below(100_000);
    cost_fund.assert_instructions_below(180_000);
    cost_settle.assert_instructions_below(180_000);

    // Validate that settle is not significantly more expensive than fund
    // (both are read-modify-write on the same storage entry).
    let ratio = cost_settle.instructions as f64 / cost_fund.instructions as f64;
    assert!(
        ratio < 1.5,
        "settle should not cost >1.5× fund; got ratio {:.2}",
        ratio
    );
}
