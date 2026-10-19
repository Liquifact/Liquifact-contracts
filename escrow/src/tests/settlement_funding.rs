//! Settlement pre-funding tests for the LiquiFact escrow contract.
//!
//! Covers the coupon shortfall collected from the SME on `settle`, the defensive
//! post-condition, the `SettlementFunded` event, and the read-only shortfall view.

use super::{
    assert_contract_error, default_init, deploy_with_id, install_stellar_asset_token, setup,
    LiquifactEscrowClient, StellarTestToken,
};
use crate::{EscrowError, EscrowSettled, SettlementFunded};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, Event, String,
};

fn setup_funded<'a>(
    env: &'a Env,
    invoice_id: &str,
    contributions: &[i128],
    yield_bps: i64,
) -> (
    LiquifactEscrowClient<'a>,
    StellarTestToken<'a>,
    Address,
    Vec<Address>,
) {
    env.mock_all_auths();
    let token = install_stellar_asset_token(env);
    let (_contract_id, client) = deploy_with_id(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);
    let target: i128 = contributions.iter().sum();

    client.init(
        &admin,
        &String::from_str(env, invoice_id),
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

    let mut investors = Vec::new();
    for amount in contributions {
        let investor = Address::generate(env);
        token.stellar.mint(&investor, amount);
        client.fund(&investor, amount);
        investors.push(investor);
    }

    (client, token, sme, investors)
}

#[test]
fn settle_pulls_coupon_shortfall_and_claims_are_solvent() {
    let env = Env::default();
    let (client, token, sme, investors) = setup_funded(&env, "SETTLE001", &[500, 500], 800);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id.clone();
    let a = investors[0].clone();
    let b = investors[1].clone();

    assert_eq!(client.get_escrow().status, 1);
    assert_eq!(client.get_settlement_shortfall(), 80);
    assert_eq!(token.token.balance(&contract_id), 1_000);

    token.stellar.mint(&sme, &80);
    client.settle();
    let events = env.events().all();

    assert_eq!(client.get_escrow().status, 2);
    assert_eq!(client.get_settlement_shortfall(), 0);
    assert_eq!(token.token.balance(&contract_id), 1_080);
    assert_eq!(token.token.balance(&sme), 0);

    let expected_funded = SettlementFunded {
        name: symbol_short!("set_fund"),
        invoice_id: invoice_id.clone(),
        payer: sme.clone(),
        amount: 80,
        settle_pool: 1_080,
    };
    assert!(events
        .events()
        .contains(&expected_funded.to_xdr(&env, &contract_id)));

    let expected_settled = EscrowSettled {
        name: symbol_short!("escrow_sd"),
        invoice_id,
        funded_amount: 1_000,
        yield_bps: 800,
        maturity: 0,
        settled_at_ledger_timestamp: env.ledger().timestamp(),
        settle_pool: 1_080,
    };
    assert!(events
        .events()
        .contains(&expected_settled.to_xdr(&env, &contract_id)));

    let payout_a = client.compute_investor_payout(&a);
    let payout_b = client.compute_investor_payout(&b);
    assert_eq!(payout_a, 540);
    assert_eq!(payout_b, 540);

    client.claim_investor_payout(&a);
    client.claim_investor_payout(&b);

    assert_eq!(token.token.balance(&a), 540);
    assert_eq!(token.token.balance(&b), 540);
    assert_eq!(token.token.balance(&contract_id), 0);
    assert!(token.token.balance(&contract_id) < 2);
}

#[test]
fn settle_does_not_transfer_or_emit_when_pool_is_prefunded() {
    let env = Env::default();
    let (client, token, sme, _investors) = setup_funded(&env, "SETTLE002", &[1_000], 800);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id.clone();

    token.stellar.mint(&contract_id, &80);
    assert_eq!(client.get_settlement_shortfall(), 0);
    let sme_before = token.token.balance(&sme);

    client.settle();
    let events = env.events().all();

    assert_eq!(client.get_escrow().status, 2);
    assert_eq!(token.token.balance(&contract_id), 1_080);
    assert_eq!(token.token.balance(&sme), sme_before);

    assert_eq!(
        events.events().len(),
        1,
        "pre-funded settle must emit only EscrowSettled"
    );
    let unexpected = SettlementFunded {
        name: symbol_short!("set_fund"),
        invoice_id,
        payer: sme,
        amount: 80,
        settle_pool: 1_080,
    };
    assert!(!events
        .events()
        .contains(&unexpected.to_xdr(&env, &contract_id)));
}

#[test]
fn settle_reverts_and_stays_funded_when_sme_cannot_fund_shortfall() {
    let env = Env::default();
    let (client, token, _sme, _investors) = setup_funded(&env, "SETTLE003", &[1_000], 800);
    let contract_id = client.address.clone();

    assert_eq!(client.get_settlement_shortfall(), 80);

    assert_contract_error(
        client.try_settle(),
        EscrowError::InsufficientTokenBalanceBeforeTransfer,
    );

    assert_eq!(client.get_escrow().status, 1);
    assert_eq!(client.get_settlement_shortfall(), 80);
    assert_eq!(token.token.balance(&contract_id), 1_000);
}

#[test]
fn settle_with_zero_yield_makes_no_transfer_and_emits_no_funding_event() {
    let env = Env::default();
    let (client, token, sme, _investors) = setup_funded(&env, "SETTLE004", &[1_000], 0);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id.clone();

    assert_eq!(client.get_settlement_shortfall(), 0);
    let sme_before = token.token.balance(&sme);

    client.settle();
    let events = env.events().all();

    assert_eq!(client.get_escrow().status, 2);
    assert_eq!(client.get_settlement_shortfall(), 0);
    assert_eq!(token.token.balance(&contract_id), 1_000);
    assert_eq!(token.token.balance(&sme), sme_before);

    assert_eq!(
        events.events().len(),
        1,
        "zero-yield settle must emit only EscrowSettled"
    );
    let unexpected = SettlementFunded {
        name: symbol_short!("set_fund"),
        invoice_id,
        payer: sme,
        amount: 0,
        settle_pool: 1_000,
    };
    assert!(!events
        .events()
        .contains(&unexpected.to_xdr(&env, &contract_id)));
}

#[test]
fn settlement_shortfall_view_is_zero_outside_funded_status() {
    // status 0 (open)
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    assert_eq!(client.get_escrow().status, 0);
    assert_eq!(client.get_settlement_shortfall(), 0);

    // status 1 (funded)
    let env = Env::default();
    let (client, token, sme, _investors) = setup_funded(&env, "SETTLE005", &[1_000], 800);
    assert_eq!(client.get_escrow().status, 1);
    assert_eq!(client.get_settlement_shortfall(), 80);

    // status 2 (settled)
    token.stellar.mint(&sme, &80);
    client.settle();
    assert_eq!(client.get_escrow().status, 2);
    assert_eq!(client.get_settlement_shortfall(), 0);
}

#[test]
fn claims_leave_less_than_one_unit_per_investor_as_rounding_residue() {
    let env = Env::default();
    let (client, token, sme, investors) = setup_funded(&env, "SETTLE006", &[333, 667], 800);
    let contract_id = client.address.clone();

    token.stellar.mint(&sme, &80);
    client.settle();
    assert_eq!(token.token.balance(&contract_id), 1_080);

    let mut sum = 0i128;
    for investor in &investors {
        let payout = client.compute_investor_payout(investor);
        sum += payout;
        client.claim_investor_payout(investor);
        assert!(token.token.balance(&contract_id) >= 0);
    }

    assert_eq!(sum, 1_079);
    assert_eq!(token.token.balance(&contract_id), 1);
    assert!(token.token.balance(&contract_id) < 2);
}

#[test]
fn settlement_pool_underfunded_error_code_is_appended() {
    assert_eq!(EscrowError::SettlementPoolUnderfunded as u32, 259);
}
