use super::{LiquifactEscrow, LiquifactEscrowClient, SCHEMA_VERSION};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

use super::{LiquifactEscrow, LiquifactEscrowClient, SCHEMA_VERSION, DataKey, InvoiceEscrow};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Map};

fn deploy(env: &Env) -> (LiquifactEscrowClient<'_>, Address) {
    let id = env.register(LiquifactEscrow, ());
    (LiquifactEscrowClient::new(env, &id), id)
}

fn setup(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    (client, admin, sme)
}

fn setup_escrow(_env: &Env, client: &LiquifactEscrowClient, admin: &Address, sme: &Address) {
    client.init(
        admin,
        &symbol_short!("INV001"),
        sme,
        &10_000i128,
        &800u32,
        &1000u64,
    );
}

// ── init ──────────────────────────────────────────────────────────────────────

#[test]
fn test_init_stores_escrow() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let escrow = client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000i128,
        &800u32,
        &1000u64,
    );
    assert_eq!(escrow.invoice_id, symbol_short!("INV001"));
    assert_eq!(escrow.admin, admin);
    assert_eq!(escrow.sme_address, sme);
    assert_eq!(escrow.amount, 10_000_0000000i128);
    assert_eq!(escrow.funding_target, 10_000_0000000i128);
    assert_eq!(escrow.funded_amount, 0);
    assert_eq!(escrow.yield_bps, 800);
    assert_eq!(escrow.maturity, 1000);
    assert_eq!(escrow.status, 0);
}

#[test]
fn test_init_and_get_escrow() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let escrow = client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    let got = client.get_escrow();
    assert_eq!(got.invoice_id, escrow.invoice_id);
    assert_eq!(got.admin, admin);
    assert_eq!(got.sme_address, sme);
    assert_eq!(got.amount, escrow.amount);
    assert_eq!(got.status, 0);
}

#[test]
fn test_init_requires_admin_auth() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    assert!(
        env.auths().iter().any(|(addr, _)| *addr == admin),
        "admin auth was not recorded for init"
    );
}

#[test]
fn test_init_unauthorized_panics() {
    let env = Env::default();
    // No mock_all_auths — admin.require_auth() will panic
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.init(
            &admin,
            &symbol_short!("INV001"),
            &sme,
            &1_000i128,
            &800i64,
            &1000u64,
        );
    }));
    assert!(result.is_err(), "Expected panic without auth");
}

#[test]
#[should_panic(expected = "Escrow already initialized")]
fn test_double_init_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &admin, &sme);
    default_init(&client, &admin, &sme);
}

#[test]
#[should_panic(expected = "Escrow not initialized")]
fn test_get_escrow_uninitialized_panics() {
    let env = Env::default();
    let client = deploy(&env);
    client.get_escrow();
}

// ── fund ──────────────────────────────────────────────────────────────────────

#[test]
fn test_fund_and_settle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV002"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &0u64,
    );
    let funded = client.fund(&investor, &10_000_0000000i128);
    assert_eq!(funded.funded_amount, 10_000_0000000i128);
    assert_eq!(funded.status, 1);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
fn test_fund_partial_then_full() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV003"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &0u64,
    );
    let partial = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(partial.status, 0);
    assert_eq!(partial.funded_amount, 5_000_0000000i128);
    let full = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(full.status, 1);
    assert_eq!(full.funded_amount, 10_000_0000000i128);
}

/// `get_escrow` panics before initialization
#[test]
#[should_panic(expected = "Funding amount must be positive")]
fn test_fund_zero_amount_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &admin, &sme);
    client.fund(&investor, &0i128);
}

#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_after_funded_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &admin, &sme);
    client.fund(&investor, &10_000_0000000i128);
    client.fund(&investor, &1i128);
}

#[test]
fn test_fund_requires_investor_auth() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &admin, &sme);
    client.fund(&investor, &10_000_0000000i128);
    assert!(
        env.auths().iter().any(|(addr, _)| *addr == investor),
        "investor auth was not recorded for fund"
    );
}

/// Funding reaches target and transitions to funded status
#[test]
fn test_single_investor_contribution_tracked() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV020"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &3_000_0000000i128);
    let contribution = client.get_contribution(&investor);
    assert_eq!(contribution, 3_000_0000000i128);
}

#[test]
fn test_unknown_investor_contribution_is_zero() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let stranger = Address::generate(&env);
    default_init(&client, &admin, &sme);
    client.fund(&investor, &1_000i128);
    assert_eq!(client.get_contribution(&stranger), 0i128);
}

// ── event: init ───────────────────────────────────────────────────────────────

#[test]
fn test_repeated_funding_accumulates_contribution() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV021"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&investor, &2_000_0000000i128);
    client.fund(&investor, &3_000_0000000i128);
    assert_eq!(client.get_contribution(&investor), 5_000_0000000i128);
}

// ── event: fund (partial) ─────────────────────────────────────────────────────

/// Partial funding maintains open status
#[test]
fn test_multiple_investors_tracked_independently() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV023"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&inv_a, &2_000_0000000i128);
    client.fund(&inv_b, &5_000_0000000i128);
    client.fund(&inv_c, &3_000_0000000i128);
    assert_eq!(client.get_contribution(&inv_a), 2_000_0000000i128);
    assert_eq!(client.get_contribution(&inv_b), 5_000_0000000i128);
    assert_eq!(client.get_contribution(&inv_c), 3_000_0000000i128);
    let sum = client.get_contribution(&inv_a)
        + client.get_contribution(&inv_b)
        + client.get_contribution(&inv_c);
    assert_eq!(sum, client.get_escrow().funded_amount);
}

// ──────────────────────────────────────────────────────────────────────────────
// Settlement Tests
// ──────────────────────────────────────────────────────────────────────────────

/// Settlement transitions escrow to settled status
#[test]
fn test_contributions_sum_equals_funded_amount() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV023"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    client.fund(&inv_a, &2_000_0000000i128);
    client.fund(&inv_b, &5_000_0000000i128);
    client.fund(&inv_c, &3_000_0000000i128);
    let sum = client.get_contribution(&inv_a)
        + client.get_contribution(&inv_b)
        + client.get_contribution(&inv_c);
    assert_eq!(sum, client.get_escrow().funded_amount);
}

// ── settle ────────────────────────────────────────────────────────────────────

#[test]
fn test_settle_after_full_funding() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV003"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &0u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

// ── event: settle ─────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_before_funded_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &symbol_short!("INV011"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.settle();
}

// ──────────────────────────────────────────────────────────────────────────────
// Update Maturity Tests
// ──────────────────────────────────────────────────────────────────────────────

/// Admin can update maturity in open state
#[test]
fn test_settle_requires_sme_auth() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV006"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
    );
    client.fund(&investor, &1_000i128);
    client.settle();
    assert!(
        env.auths().iter().any(|(addr, _)| *addr == sme),
        "sme auth was not recorded for settle"
    );
}

#[test]
#[should_panic]
fn test_settle_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);
    client.init(
        &admin,
        &symbol_short!("INV008"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
    );
    client.fund(&investor, &1_000i128);
    // Clear all auths so settle fails
    env.mock_auths(&[]);
    client.settle();
}

#[test]
#[should_panic(expected = "Escrow has not yet reached maturity")]
fn test_settle_before_maturity_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV032"),
        &sme,
        &1_000i128,
        &500i64,
        &1000u64,
    );
    client.fund(&investor, &1_000i128);
    // ledger timestamp is 0, maturity is 1000 — should panic
    client.settle();
}

#[test]
fn test_settle_after_maturity_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV033"),
        &sme,
        &1_000i128,
        &500i64,
        &1000u64,
    );
    client.fund(&investor, &1_000i128);
    env.ledger().set_timestamp(1001);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
fn test_settle_at_exact_maturity_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV034"),
        &sme,
        &1_000i128,
        &500i64,
        &1000u64,
    );
    client.fund(&investor, &1_000i128);
    env.ledger().set_timestamp(1000);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
fn test_settle_with_zero_maturity_succeeds_immediately() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV035"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
    );
    client.fund(&investor, &1_000i128);
    // timestamp is 0, maturity is 0 — skip check
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
fn test_settle_at_timestamp_zero_before_maturity_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV036"),
        &sme,
        &1_000i128,
        &500i64,
        &500u64,
    );
    client.fund(&investor, &1_000i128);
    // timestamp is 0, maturity is 500 — should panic
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.settle();
    }));
    assert!(
        result.is_err(),
        "Expected panic when settling before maturity"
    );
}

// ── update_maturity ───────────────────────────────────────────────────────────

#[test]
fn test_update_maturity_success() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &symbol_short!("INV006"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
    );
    let updated = client.update_maturity(&2000u64);
    assert_eq!(updated.maturity, 2000u64);
    assert_eq!(updated.status, 0);
}

#[test]
#[should_panic(expected = "Maturity can only be updated in Open state")]
fn test_update_maturity_wrong_state() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV007"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
    );
    client.fund(&investor, &1_000i128); // status -> 1
    client.update_maturity(&2000u64);
}

#[test]
#[should_panic]
fn test_update_maturity_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);
    client.init(
        &admin,
        &symbol_short!("INV009"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
    );
    // Remove all auths so admin.require_auth() fails
    env.mock_auths(&[]);
    client.update_maturity(&2000u64);
}

// ── transfer_admin ────────────────────────────────────────────────────────────

#[test]
fn test_transfer_admin_updates_admin() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let new_admin = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("T001"),
        &sme,
        &10_000_0000000i128,
        &800u64,
        &1000u64,
    );
    let updated = client.transfer_admin(&new_admin);
    assert_eq!(updated.admin, new_admin);
    assert_eq!(client.get_escrow().admin, new_admin);
}

#[test]
#[should_panic(expected = "New admin must differ from current admin")]
fn test_transfer_admin_same_address_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &symbol_short!("T002"),
        &sme,
        &admin,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.transfer_admin(&admin);
}

#[test]
#[should_panic(expected = "Escrow not initialized")]
fn test_transfer_admin_uninitialized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin);
}

// ── migrate ───────────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Already at current schema version")]
fn test_migrate_at_current_version_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &admin, &sme);
    client.migrate(&SCHEMA_VERSION);
}

#[test]
#[should_panic(expected = "from_version does not match stored version")]
fn test_migrate_wrong_from_version_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &admin, &sme);
    client.migrate(&99u32);
}

// ── cost baselines ────────────────────────────────────────────────────────────

#[test]
fn test_cost_baseline_init() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &symbol_short!("INV100"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
}

#[test]
fn test_cost_baseline_init_zero_maturity() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &symbol_short!("INV101"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &0u64,
    );
}

#[test]
fn test_cost_baseline_init_max_amount() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &symbol_short!("INV102"),
        &sme,
        &i128::MAX,
        &800i64,
        &1000u64,
    );
}

#[test]
fn test_cost_baseline_fund_partial() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV103"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &1_000_0000000i128);
}

#[test]
fn test_cost_baseline_fund_full() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV104"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
}

#[test]
fn test_cost_baseline_fund_overshoot() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV105"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &15_000_0000000i128);
    assert_eq!(client.get_escrow().status, 1);
}

#[test]
fn test_cost_baseline_fund_two_step_completion() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV106"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &5_000_0000000i128);
    client.fund(&investor, &5_000_0000000i128);
    assert_eq!(client.get_escrow().status, 1);
}

#[test]
fn test_cost_baseline_settle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV103"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    env.ledger().set_timestamp(1001);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
fn test_cost_baseline_full_lifecycle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INV110"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    env.ledger().set_timestamp(1000);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

// ── property-based tests ──────────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_funded_amount_non_decreasing(
        amount1 in 1i128..5_000_0000000i128,
        amount2 in 1i128..5_000_0000000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);
        let client = deploy(&env);

        // Use a large target so both fundings can happen
        let target = 20_000_0000000i128;
        client.init(&admin, &symbol_short!("INVTST"), &sme, &target, &800i64, &0u64);

        let before = client.get_escrow().funded_amount;
        client.fund(&investor1, &amount1);
        let after1 = client.get_escrow().funded_amount;
        prop_assert!(after1 >= before, "funded_amount must be non-decreasing");

        if client.get_escrow().status == 0 {
            client.fund(&investor2, &amount2);
            let after2 = client.get_escrow().funded_amount;
            prop_assert!(after2 >= after1, "funded_amount must be non-decreasing on successive funds");
        }
    }

    #[test]
    fn prop_status_only_increases(
        amount in 1i128..10_000_0000000i128,
        target in 1i128..10_000_0000000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let investor = Address::generate(&env);
        let client = deploy(&env);

        let escrow = client.init(&admin, &symbol_short!("INVSTA"), &sme, &target, &800i64, &0u64);
        prop_assert_eq!(escrow.status, 0);

        let after_fund = client.fund(&investor, &amount);
        prop_assert!(after_fund.status >= escrow.status, "status must not decrease");
        prop_assert!(after_fund.status <= 3, "status must be in valid range");

        if amount >= target {
            prop_assert_eq!(after_fund.status, 1);
            let after_settle = client.settle();
            prop_assert_eq!(after_settle.status, 2);
        } else {
            prop_assert_eq!(after_fund.status, 0);
        }
    }
}

── END LEGACY ────────────────────────────────────────────────────────────── */
