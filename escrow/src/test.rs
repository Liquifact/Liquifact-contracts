use super::{LiquifactEscrow, LiquifactEscrowClient, SCHEMA_VERSION};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};
//

// ── helpers ───────────────────────────────────────────────────────────────────

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn default_init(client: &LiquifactEscrowClient, admin: &Address, sme: &Address) {
    client.init(
        admin,
        &symbol_short!("INV001"),
        sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
}

// ── init ──────────────────────────────────────────────────────────────────────

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn deploy(env: &Env) -> (Address, LiquifactEscrowClient<'_>) {
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &contract_id);
    (contract_id, client)
}

// ──────────────────────────────────────────────────────────────────────────────
// init
// ──────────────────────────────────────────────────────────────────────────────

/// After `init` the escrow must be open (status 0) with zero funded_amount,
/// and `get_escrow` must return an identical snapshot.
#[test]
fn test_init_sets_version() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let escrow = client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    assert_eq!(escrow.version, SCHEMA_VERSION);
    assert_eq!(client.get_version(), SCHEMA_VERSION);
}

#[test]
fn test_init_and_get_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let escrow = client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &admin,
        &10_000_0000000i128,
        &800i64,
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
#[should_panic(expected = "Escrow amount must be positive")]
fn test_init_with_zero_fails() {
    let env = Env::default();
    let (client, sme, id) = setup_test(&env);
    client.init(&id, &sme, &0, &800, &10000);
}

    // get_escrow must match what init returned
    let got = client.get_escrow();
    assert_eq!(got.invoice_id, escrow.invoice_id);
    assert_eq!(got.admin, admin);
}

/// `init` must emit exactly one `EscrowInitialized` event whose payload
/// matches the returned snapshot.
///
/// `env.events().all()` captures events from the last invocation only — this
/// works perfectly since init is the only call in this test.
#[test]
#[should_panic(expected = "Escrow already initialized")]
fn test_reinit_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    default_init(&client, &admin, &sme);
    default_init(&client, &admin, &sme); // must panic
}

// ── fund & settle ─────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Funding amount must be positive")]
fn test_fund_with_zero_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let admin = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV002"),
        &sme,
        &admin,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    let e1 = client.fund(&investor, &10_000_0000000i128);
    assert_eq!(e1.funded_amount, 10_000_0000000i128);
    assert_eq!(e1.status, 1);

    let e2 = client.settle();
    assert_eq!(e2.status, 2);
}

#[test]
fn test_partial_fund_stays_open() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV003"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    let partial = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(partial.status, 0);
    assert_eq!(partial.funded_amount, 5_000_0000000i128);

    let full = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(full.status, 1);
    assert_eq!(full.funded_amount, 10_000_0000000i128);
}

#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_after_funded_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV004"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128); // status -> 1
    client.fund(&investor, &1i128); // must panic
}

#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_before_funded_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    default_init(&client, &admin, &sme);
    client.settle(); // must panic — status is still 0
}

// ── auth checks ───────────────────────────────────────────────────────────────

#[test]
fn test_fund_records_investor_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV005"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.fund(&investor, &1_000i128);

    assert!(
        env.auths().iter().any(|(addr, _)| *addr == investor),
        "investor auth was not recorded for fund"
    );
}

#[test]
fn test_settle_records_sme_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV006"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.fund(&investor, &1_000i128);
    client.settle();

    assert!(
        env.auths().iter().any(|(addr, _)| *addr == sme),
        "sme auth was not recorded for settle"
    );
}

// ── get_escrow uninitialized ──────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Escrow not initialized")]
fn test_get_escrow_uninitialized_panics() {
    let env = Env::default();
    let client = deploy(&env);
    client.get_escrow();
}

// ── migration guards ──────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Already at current schema version")]
fn test_migrate_at_current_version_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    default_init(&client, &admin, &sme);
    client.migrate(&SCHEMA_VERSION);
}

#[test]
#[should_panic(expected = "from_version does not match stored version")]
fn test_migrate_wrong_from_version_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    default_init(&client, &admin, &sme);
    client.migrate(&99u32);
}

use proptest::prelude::*;

proptest! {
    // Escrow Property Invariants

    #[test]
    fn prop_funded_amount_non_decreasing(
        amount1 in 0..10_000_0000000i128,
        amount2 in 0..10_000_0000000i128
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let sme = Address::generate(&env);
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);

        let contract_id = env.register(LiquifactEscrow, ());
        let client = LiquifactEscrowClient::new(&env, &contract_id);

        let target_amount = 20_000_0000000i128;

        client.init(
            &symbol_short!("INVTST"),
            &sme,
            &target_amount,
            &800i64,
            &1000u64,
        );

        // First funding
        let pre_funding_amount = client.get_escrow().funded_amount;
        client.fund(&investor1, &amount1);
        let post_funding1 = client.get_escrow().funded_amount;

        // Invariant: Funding amount acts monotonically
        assert!(post_funding1 >= pre_funding_amount, "Funded amount should be non-decreasing");

        // Skip second funding if status already flipped
        if client.get_escrow().status == 0 {
            client.fund(&investor2, &amount2);
            let post_funding2 = client.get_escrow().funded_amount;
            assert!(post_funding2 >= post_funding1, "Funded amount should be non-decreasing on successive funds");
        }
    }

    #[test]
    fn prop_bounded_status_transitions(
        amount in 0..50_000_0000000i128,
        target_amount in 100..10000_000000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let sme = Address::generate(&env);
        let investor = Address::generate(&env);

        let contract_id = env.register(LiquifactEscrow, ());
        let client = LiquifactEscrowClient::new(&env, &contract_id);

        let escrow = client.init(
            &symbol_short!("INVSTA"),
            &sme,
            &target_amount,
            &800i64,
            &1000u64,
        );

        // Initial status is 0
        assert_eq!(escrow.status, 0);

        // Status bounds check
        assert!(escrow.status <= 2);

        let funded_escrow = client.fund(&investor, &amount);

        // Mid-status bounds check
        assert!(funded_escrow.status <= 2);

        // Ensure status 1 is reached ONLY if target met
        if amount >= target_amount {
            assert_eq!(funded_escrow.status, 1);

            // Only funded escrows can be settled
            let settled_escrow = client.settle();
            assert_eq!(settled_escrow.status, 2);
        } else {
            // Unfunded remains 0
            assert_eq!(funded_escrow.status, 0);
        }
    }
}

#[test]
#[should_panic(expected = "Escrow not initialized")]
fn test_fund_fails_when_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    let investor = Address::generate(&env);
    client.fund(&investor, &1000);
}

#[test]
#[should_panic(expected = "Escrow not initialized")]
fn test_settle_fails_when_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.settle();
}

#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_fails_when_not_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    let sme = Address::generate(&env);
    client.init(&symbol_short!("INV001"), &sme, &1000, &800, &1000);
    client.settle();
}

#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_fails_when_already_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);

    client.init(&symbol_short!("INV001"), &sme, &1000, &800, &1000);
    client.fund(&investor, &1000);
    // Escrow is now funded status = 1.
    client.fund(&investor, &500); // Should panic
}

#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_fails_when_already_settled() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);

    client.init(&symbol_short!("INV001"), &sme, &1000, &800, &1000);
    client.fund(&investor, &1000);
    client.settle();

    // Already settled status = 2, status != 1 so expect panic
    client.settle();
}

#[test]
fn test_fund_does_not_enforce_investor_auth() {
    let env = Env::default();
    // SECURITY: We do not call env.mock_all_auths() here to prove that
    // the contract does *not* enforce require_auth() on the investor.
    // If it did, this test would fail because there are no mocked auths.

    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);

    client.init(&symbol_short!("INV001"), &sme, &1000, &800, &1000);
    let escrow = client.fund(&investor, &1000);

    assert_eq!(escrow.funded_amount, 1000);
    assert_eq!(escrow.status, 1);
}

#[test]
fn test_settle_does_not_enforce_auth() {
    let env = Env::default();
    // SECURITY: Proves settle can be called by anyone without require_auth().

    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);

    client.init(&symbol_short!("INV001"), &sme, &1000, &800, &1000);
    client.fund(&investor, &1000);
    let escrow = client.settle();

    assert_eq!(escrow.status, 2);
}

#[test]
fn test_reinit_overwrites_escrow() {
    let env = Env::default();
    // SECURITY: Show that init can be called again by anyone to overwrite the escrow.
    env.mock_all_auths();

    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    let sme1 = Address::generate(&env);
    let sme2 = Address::generate(&env);

    client.init(&symbol_short!("INV001"), &sme1, &1000, &800, &1000);
    let escrow1 = client.get_escrow();
    assert_eq!(escrow1.sme_address, sme1);

    // Someone else overwrites it
    client.init(&symbol_short!("ATTACK"), &sme2, &9999, &999, &9999);
    let escrow2 = client.get_escrow();
    assert_eq!(escrow2.sme_address, sme2);
    assert_eq!(escrow2.invoice_id, symbol_short!("ATTACK"));
}

#[test]
fn test_partial_settlement_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let expected_event = EscrowInitialized {
        name: symbol_short!("escrow_ii"),
        escrow: escrow.clone(),
    };

    assert_eq!(
        env.events().all(),
        std::vec![expected_event.to_xdr(&env, &contract_id)],
        "EscrowInitialized event must match the returned InvoiceEscrow snapshot"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// fund
// ──────────────────────────────────────────────────────────────────────────────

/// Partial funding keeps status at 0; full funding flips status to 1.
#[test]
fn test_partial_then_full_fund() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = deploy(&env);
    let sme = Address::generate(&env);
    let admin = Address::generate(&env);
    let investor = Address::generate(&env);

    client.init(
        &symbol_short!("INV_P1"),
        &sme,
        &admin,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    client.fund(&investor, &10_000_0000000i128);

    let interest = (10_000_0000000i128 * 800) / 10000;
    let total_due = 10_000_0000000i128 + interest; // 10,800,000,000

    // First partial: 5,000,000,000
    let e1 = client.settle(&5_000_0000000i128);
    assert_eq!(e1.settled_amount, 5_000_0000000i128);
    assert_eq!(e1.status, 1);

    // Second partial: 5,000,000,000
    let e2 = client.settle(&5_000_0000000i128);
    assert_eq!(e2.settled_amount, 10_000_0000000i128);
    assert_eq!(e2.status, 1);

    // Final settlement: 800,000,000
    let e3 = client.settle(&800_0000000i128);
    assert_eq!(e3.settled_amount, total_due);
    assert_eq!(e3.status, 2);
}

#[test]
#[should_panic(expected = "Settlement amount exceeds total due")]
fn test_over_settlement_failure() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let admin = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV_O1"),
        &sme,
        &admin,
        &10_000_0000000i128,
        &800i64,
        &2000u64,
    );

    // Fund exactly the target in one shot
    let after_fund = client.fund(&investor, &10_000_0000000i128);
    assert_eq!(after_fund.funded_amount, 10_000_0000000i128);
    assert_eq!(after_fund.status, 1, "should be funded");

    // Settle
    let after_settle = client.settle();
    assert_eq!(after_settle.status, 2, "should be settled");
}

#[test]
fn test_partial_funding_multiple_investors() {
    let (env, client) = setup();
    let sme = Address::generate(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);

    client.init(
        &symbol_short!("INV003"),
        &sme,
        &9_000_0000000i128,
        &500i64,
        &3000u64,
    );

    // Three partial contributions
    let s1 = client.fund(&inv_a, &3_000_0000000i128);
    assert_eq!(s1.status, 0, "still open after first tranche");

    let s2 = client.fund(&inv_b, &3_000_0000000i128);
    assert_eq!(s2.status, 0, "still open after second tranche");

    let s3 = client.fund(&inv_c, &3_000_0000000i128);
    assert_eq!(s3.funded_amount, 9_000_0000000i128);
    assert_eq!(s3.status, 1, "funded after third tranche completes target");
}

#[test]
fn test_overfunding_still_funded() {
    let (env, client) = setup();
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);

    client.init(
        &symbol_short!("INV004"),
        &sme,
        &5_000_0000000i128,
        &300i64,
        &4000u64,
    );

    // Fund more than the target
    let after = client.fund(&investor, &7_000_0000000i128);
    assert_eq!(after.funded_amount, 7_000_0000000i128);
    assert_eq!(after.status, 1, "over-funded escrow must still be status=1");
}

#[test]
fn test_init_field_integrity() {
    let (env, client) = setup();
    let sme = Address::generate(&env);

    let escrow = client.init(
        &symbol_short!("INV005"),
        &sme,
        &1_500_0000000i128,
        &1200i64,
        &9999u64,
    );

    // funding_target must mirror amount
    assert_eq!(escrow.funding_target, escrow.amount);
    // sme_address must be preserved
    assert_eq!(escrow.sme_address, sme);
}

#[test]
fn test_yield_bps_stored() {
    let (env, client) = setup();
    let sme = Address::generate(&env);

    client.init(
        &symbol_short!("INV006"),
        &sme,
        &1_000_0000000i128,
        &1500i64, // 15%
        &5000u64,
    );

    assert_eq!(client.get_escrow().yield_bps, 1500);
}

#[test]
fn test_maturity_stored() {
    let (env, client) = setup();
    let sme = Address::generate(&env);

    client.init(
        &symbol_short!("INV007"),
        &sme,
        &1_000_0000000i128,
        &800i64,
        &u64::MAX,
    );

    assert_eq!(client.get_escrow().maturity, u64::MAX);
}

#[test]
fn test_minimum_amount_escrow() {
    let (env, client) = setup();
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);

    client.init(&symbol_short!("INV008"), &sme, &1i128, &0i64, &1u64);

    let after = client.fund(&investor, &1i128);
    assert_eq!(after.status, 1);

    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
fn test_zero_amount_fund_no_status_change() {
    let (env, client) = setup();
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);

    client.init(
        &symbol_short!("INV009"),
        &sme,
        &1_000_0000000i128,
        &800i64,
        &1000u64,
    );

    // A zero-amount fund call should not flip status
    let after = client.fund(&investor, &0i128);
    assert_eq!(after.status, 0, "zero-amount fund must not change status");
    assert_eq!(after.funded_amount, 0);
}

// ---------------------------------------------------------------------------
// Failure / panic tests
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_after_funded_panics() {
    let (env, client) = setup();
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);

    client.init(
        &symbol_short!("INV010"),
        &sme,
        &1_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &1_000_0000000i128); // reaches status=1
    client.fund(&investor, &1i128); // must panic
}

    client.fund(&investor, &10_000_0000000i128);

    let interest = (10_000_0000000i128 * 800) / 10000;
    let total_due = 10_000_0000000i128 + interest;

    // Try to settle more than due
    client.settle(&(total_due + 1));
}

#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_not_funded() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let admin = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV_NF"),
        &sme,
        &admin,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    // Not funded, should panic
    client.settle(&1000i128);
}

#[test]
fn test_update_maturity_success() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let admin = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV003"),
        &sme,
        &admin,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    let new_maturity = 2000u64;
    let escrow = client.update_maturity(&new_maturity);
    assert_eq!(escrow.maturity, new_maturity);

    // Verify state is still Open
    assert_eq!(escrow.status, 0);
}

#[test]
#[should_panic]
fn test_update_maturity_unauthorized() {
    let env = Env::default();
    // No mock_all_auths() here to manually set auths if needed, 
    // or use mock_all_auths and then try to call from a different address if the client allows it.
    // In Soroban tests, client.update_maturity() will use the address that registered the contract or default.
    // Actually, client calls in tests usually don't have a "caller" unless specified.
    
    let sme = Address::generate(&env);
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV004"),
        &sme,
        &admin,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    // Attempt to call from attacker. 
    // In Soroban SDK tests, you can switch the address using set_auths or similar, 
    // but a simpler way is to use `env.as_contract(&attacker, || client.update_maturity(&2000))`
    // Wait, the client is bound to the contract, not the caller.
    
    env.as_contract(&attacker, || {
        client.update_maturity(&2000u64);
    });
}

#[test]
#[should_panic(expected = "Maturity can only be updated in Open state")]
fn test_update_maturity_wrong_state() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let admin = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV005"),
        &sme,
        &admin,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    // Fund the escrow to change state to 1 (Funded)
    client.fund(&investor, &10_000_0000000i128);
    
    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 1);

    // This should panic
    client.update_maturity(&2000u64);
}

#[test]
fn test_full_funding_updates_status() {
    let env = Env::default();
    let (client, sme, id) = setup_test(&env);
    let investor = Address::generate(&env);
    
    client.init(&id, &sme, &1000, &800, &10000);
    client.fund(&investor, &1000);
    
    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 1); // Status 1 = Funded
}