use super::{LiquifactEscrow, LiquifactEscrowClient};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    Address, Env, IntoVal,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Deploy a fresh contract and return (client, sme_address).
fn setup(env: &Env) -> (LiquifactEscrowClient<'_>, Address) {
    let sme = Address::generate(env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &contract_id);
    (client, sme)
}

/// Init with sensible defaults (target = 10_000 XLM, 8% yield, maturity 1000).
fn default_init(client: &LiquifactEscrowClient, sme: &Address) {
    client.init(
        &symbol_short!("INV001"),
        sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

#[test]
fn test_init_stores_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);

    let escrow = client.init(
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    assert_eq!(escrow.invoice_id, symbol_short!("INV001"));
    assert_eq!(escrow.sme_address, sme);
    assert_eq!(escrow.amount, 10_000_0000000i128);
    assert_eq!(escrow.funding_target, 10_000_0000000i128);
    assert_eq!(escrow.funded_amount, 0);
    assert_eq!(escrow.yield_bps, 800);
    assert_eq!(escrow.maturity, 1000);
    assert_eq!(escrow.status, 0);
}

#[test]
fn test_get_escrow_uninitialized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let result = client.try_get_escrow();
    assert!(result.is_err());
}

#[test]
fn test_double_init_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let result = client.try_init(
        &symbol_short!("INV002"),
        &sme,
        &5_000_0000000i128,
        &500i64,
        &2000u64,
    );
    assert!(result.is_err());
}

#[test]
fn test_init_requires_admin_auth() {
    let env = Env::default();
    // Do NOT mock auths – the sme_address.require_auth() must fire.
    let (client, sme) = setup(&env);

    // Provide auth only for sme so the call succeeds.
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &sme,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "init",
            args: (
                symbol_short!("INV001"),
                sme.clone(),
                10_000_0000000i128,
                800i64,
                1000u64,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let escrow = client.init(
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    assert_eq!(escrow.status, 0);
}

#[test]
fn test_init_unauthorized_panics() {
    let env = Env::default();
    // No auths mocked at all → require_auth() will panic.
    let (client, sme) = setup(&env);

    let result = client.try_init(
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// fund – edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_fund_zero_amount_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let investor = Address::generate(&env);
    let result = client.try_fund(&investor, &0i128);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// fund – basic behaviour
// ---------------------------------------------------------------------------

#[test]
fn test_fund_partial_then_full() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let investor = Address::generate(&env);

    // Partial fund – status stays open.
    let e1 = client.fund(&investor, &4_000_0000000i128);
    assert_eq!(e1.funded_amount, 4_000_0000000i128);
    assert_eq!(e1.status, 0);

    // Complete funding – status becomes funded.
    let e2 = client.fund(&investor, &6_000_0000000i128);
    assert_eq!(e2.funded_amount, 10_000_0000000i128);
    assert_eq!(e2.status, 1);
}

#[test]
fn test_fund_after_funded_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000_0000000i128);

    let result = client.try_fund(&investor, &1i128);
    assert!(result.is_err());
}

#[test]
fn test_fund_requires_investor_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let investor = Address::generate(&env);

    // Remove all mocked auths → investor.require_auth() should fail.
    env.mock_auths(&[]);
    let result = client.try_fund(&investor, &1_000_0000000i128);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// fund – per-investor ledger (new behaviour)
// ---------------------------------------------------------------------------

#[test]
fn test_single_investor_contribution_tracked() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let investor = Address::generate(&env);
    client.fund(&investor, &3_000_0000000i128);

    assert_eq!(client.get_contribution(&investor), 3_000_0000000i128);
}

#[test]
fn test_repeated_funding_accumulates_contribution() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let investor = Address::generate(&env);
    client.fund(&investor, &2_000_0000000i128);
    client.fund(&investor, &3_000_0000000i128);

    // Ledger must reflect the sum of both calls.
    assert_eq!(client.get_contribution(&investor), 5_000_0000000i128);
}

#[test]
fn test_multiple_investors_tracked_independently() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);

    client.fund(&inv_a, &4_000_0000000i128);
    client.fund(&inv_b, &6_000_0000000i128);

    assert_eq!(client.get_contribution(&inv_a), 4_000_0000000i128);
    assert_eq!(client.get_contribution(&inv_b), 6_000_0000000i128);
}

#[test]
fn test_contributions_sum_equals_funded_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);

    client.fund(&inv_a, &2_000_0000000i128);
    client.fund(&inv_b, &5_000_0000000i128);
    client.fund(&inv_c, &3_000_0000000i128);

    let total = client.get_contribution(&inv_a)
        + client.get_contribution(&inv_b)
        + client.get_contribution(&inv_c);

    let escrow = client.get_escrow();
    assert_eq!(total, escrow.funded_amount);
}

#[test]
fn test_unknown_investor_contribution_is_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let stranger = Address::generate(&env);
    assert_eq!(client.get_contribution(&stranger), 0i128);
}

// ---------------------------------------------------------------------------
// settle
// ---------------------------------------------------------------------------

#[test]
fn test_settle_after_full_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000_0000000i128);

    // Advance ledger past maturity.
    env.ledger().set_timestamp(1001);
    let escrow = client.settle();
    assert_eq!(escrow.status, 2);
}

#[test]
fn test_settle_before_funded_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let result = client.try_settle();
    assert!(result.is_err());
}

#[test]
fn test_settle_before_maturity_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000_0000000i128);

    // Ledger timestamp defaults to 0, which is before maturity 1000.
    let result = client.try_settle();
    assert!(result.is_err());
}

#[test]
fn test_settle_at_exact_maturity_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000_0000000i128);

    env.ledger().set_timestamp(1000); // exactly at maturity
    let escrow = client.settle();
    assert_eq!(escrow.status, 2);
}

#[test]
fn test_settle_after_maturity_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000_0000000i128);

    env.ledger().set_timestamp(9999);
    let escrow = client.settle();
    assert_eq!(escrow.status, 2);
}

#[test]
fn test_settle_with_zero_maturity_succeeds_immediately() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);

    // maturity = 0 means "no maturity lock"
    client.init(
        &symbol_short!("INV003"),
        &sme,
        &1_000_0000000i128,
        &500i64,
        &0u64,
    );

    let investor = Address::generate(&env);
    client.fund(&investor, &1_000_0000000i128);

    // timestamp is 0 by default; maturity == 0 bypasses the check
    let escrow = client.settle();
    assert_eq!(escrow.status, 2);
}

#[test]
fn test_settle_at_timestamp_zero_before_maturity_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);

    // maturity = 500, ledger timestamp = 0 → should panic
    client.init(
        &symbol_short!("INV004"),
        &sme,
        &1_000_0000000i128,
        &500i64,
        &500u64,
    );

    let investor = Address::generate(&env);
    client.fund(&investor, &1_000_0000000i128);

    let result = client.try_settle();
    assert!(result.is_err());
}

#[test]
fn test_settle_requires_sme_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000_0000000i128);
    env.ledger().set_timestamp(1001);

    // Remove all mocked auths → sme_address.require_auth() should fail.
    env.mock_auths(&[]);
    let result = client.try_settle();
    assert!(result.is_err());
}

#[test]
fn test_settle_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sme) = setup(&env);
    default_init(&client, &sme);

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000_0000000i128);
    env.ledger().set_timestamp(1001);

    // Remove all mocked auths → settle should be rejected.
    env.mock_auths(&[]);
    let result = client.try_settle();
    assert!(result.is_err());
}
