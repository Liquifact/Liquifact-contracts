//! Repayment-after-withdraw tests for the LiquiFact escrow contract.
//!
//! Covers the `3 -> 2` repayment edge: full and instalment repayment, guard ordering,
//! terminal transition, event payloads, and the sweep liability floor that protects
//! partially repaid funds.

use super::{
    assert_contract_error, default_init, deploy_with_id, install_stellar_asset_token, setup,
    LiquifactEscrowClient, StellarTestToken, MAX_DUST_SWEEP_AMOUNT,
};
use crate::{EscrowError, EscrowRepaid, EscrowSettled, PauseReason, PauseScope};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, Event, String,
};

const PRINCIPAL: i128 = 1_000_000_000;
const YIELD_BPS: i64 = 800;
const POOL: i128 = 1_080_000_000;

fn setup_funded_with<'a>(
    env: &'a Env,
    invoice_id: &str,
    contributions: &[i128],
) -> (
    LiquifactEscrowClient<'a>,
    StellarTestToken<'a>,
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
        &YIELD_BPS,
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

    (client, token, investors)
}

fn withdraw_and_fund_payer<'a>(
    env: &'a Env,
    invoice_id: &str,
    contributions: &[i128],
) -> (
    LiquifactEscrowClient<'a>,
    StellarTestToken<'a>,
    Vec<Address>,
    Address,
    i128,
) {
    let (client, token, investors) = setup_funded_with(env, invoice_id, contributions);
    client.withdraw();

    let payer = Address::generate(env);
    let pool = client.get_settlement_pool();
    token.stellar.mint(&payer, &pool);

    (client, token, investors, payer, pool)
}

#[test]
fn withdraw_repay_full_then_two_investors_claim() {
    let env = Env::default();
    let (client, token, investors, payer, pool) =
        withdraw_and_fund_payer(&env, "REPAY001", &[PRINCIPAL / 2, PRINCIPAL / 2]);
    let contract_id = client.address.clone();
    let a = investors[0].clone();
    let b = investors[1].clone();

    assert_eq!(client.get_escrow().status, 3);
    assert_eq!(client.get_repayment_status(), (0, pool));
    assert_eq!(pool, POOL);

    let outstanding = client.repay(&payer, &pool);
    assert_eq!(outstanding, 0);
    assert_eq!(client.get_escrow().status, 2);
    assert_eq!(client.get_repayment_status(), (pool, 0));
    assert_eq!(token.token.balance(&contract_id), pool);

    let payout_a = client.compute_investor_payout(&a);
    let payout_b = client.compute_investor_payout(&b);
    assert_eq!(payout_a, 540_000_000);
    assert_eq!(payout_b, 540_000_000);
    assert_eq!(payout_a + payout_b, pool);

    client.claim_investor_payout(&a);
    client.claim_investor_payout(&b);

    assert_eq!(token.token.balance(&a), payout_a);
    assert_eq!(token.token.balance(&b), payout_b);
    assert_eq!(token.token.balance(&contract_id), 0);
}

#[test]
fn instalments_keep_status_three_until_pool_is_reached() {
    let env = Env::default();
    let (client, token, _investors, payer, pool) =
        withdraw_and_fund_payer(&env, "REPAY002", &[PRINCIPAL / 2, PRINCIPAL / 2]);
    let contract_id = client.address.clone();

    let first = pool * 40 / 100;
    let outstanding = client.repay(&payer, &first);

    assert_eq!(outstanding, pool - first);
    assert_eq!(client.get_escrow().status, 3);
    assert_eq!(client.get_repayment_status(), (first, pool - first));
    assert_eq!(token.token.balance(&contract_id), first);
    assert_eq!(client.get_settled_at(), None);

    env.ledger().set_timestamp(12_345);
    let second = pool - first;
    let outstanding = client.repay(&payer, &second);

    assert_eq!(outstanding, 0);
    assert_eq!(client.get_escrow().status, 2);
    assert_eq!(client.get_repayment_status(), (pool, 0));
    assert_eq!(token.token.balance(&contract_id), pool);
    assert_eq!(client.get_settled_at(), Some(12_345));
}

#[test]
fn repay_rejects_overpayment_by_one_unit_without_state_changes() {
    let env = Env::default();
    let (client, token, _investors, payer, pool) =
        withdraw_and_fund_payer(&env, "REPAY003", &[PRINCIPAL / 2, PRINCIPAL / 2]);
    let contract_id = client.address.clone();

    token.stellar.mint(&payer, &1);

    assert_contract_error(
        client.try_repay(&payer, &(pool + 1)),
        EscrowError::RepaymentExceedsOutstanding,
    );
    assert_eq!(client.get_escrow().status, 3);
    assert_eq!(client.get_repayment_status(), (0, pool));
    assert_eq!(token.token.balance(&contract_id), 0);
    assert_eq!(token.token.balance(&payer), pool + 1);
}

#[test]
fn repay_rejects_zero_and_negative_amounts_without_state_changes() {
    let env = Env::default();
    let (client, token, _investors, payer, pool) =
        withdraw_and_fund_payer(&env, "REPAY004", &[PRINCIPAL / 2, PRINCIPAL / 2]);
    let contract_id = client.address.clone();

    assert_contract_error(
        client.try_repay(&payer, &0),
        EscrowError::RepaymentAmountNotPositive,
    );
    assert_contract_error(
        client.try_repay(&payer, &-1),
        EscrowError::RepaymentAmountNotPositive,
    );
    assert_eq!(client.get_escrow().status, 3);
    assert_eq!(client.get_repayment_status(), (0, pool));
    assert_eq!(token.token.balance(&contract_id), 0);
}

#[test]
fn repay_rejects_every_non_withdrawn_status() {
    // status 0 (open)
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let payer = Address::generate(&env);
    assert_contract_error(client.try_repay(&payer, &1), EscrowError::RepaymentNotOpen);
    assert_eq!(client.get_escrow().status, 0);

    // status 1 (funded)
    let env = Env::default();
    let (client, _token, _investors) = setup_funded_with(&env, "REPAY_ST1", &[PRINCIPAL]);
    let payer = Address::generate(&env);
    assert_contract_error(client.try_repay(&payer, &1), EscrowError::RepaymentNotOpen);
    assert_eq!(client.get_escrow().status, 1);

    // status 2 (settled via settle, no repayment)
    let env = Env::default();
    let (client, _token, _investors) = setup_funded_with(&env, "REPAY_ST2", &[PRINCIPAL]);
    client.settle();
    let payer = Address::generate(&env);
    assert_contract_error(client.try_repay(&payer, &1), EscrowError::RepaymentNotOpen);
    assert_eq!(client.get_escrow().status, 2);

    // status 4 (cancelled)
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.cancel_funding();
    let payer = Address::generate(&env);
    assert_contract_error(client.try_repay(&payer, &1), EscrowError::RepaymentNotOpen);
    assert_eq!(client.get_escrow().status, 4);
}

#[test]
fn repay_blocked_by_legal_hold() {
    let env = Env::default();
    let (client, token, _investors, payer, pool) =
        withdraw_and_fund_payer(&env, "REPAY_HOLD", &[PRINCIPAL / 2, PRINCIPAL / 2]);
    let contract_id = client.address.clone();

    client.set_legal_hold(&true);

    assert_contract_error(
        client.try_repay(&payer, &1),
        EscrowError::LegalHoldBlocksRepayment,
    );
    assert_eq!(client.get_escrow().status, 3);
    assert_eq!(client.get_repayment_status(), (0, pool));
    assert_eq!(token.token.balance(&contract_id), 0);
}

#[test]
fn repay_blocked_by_settlement_pause() {
    let env = Env::default();
    let (client, token, _investors, payer, pool) =
        withdraw_and_fund_payer(&env, "REPAY_PAUSE_SET", &[PRINCIPAL / 2, PRINCIPAL / 2]);
    let contract_id = client.address.clone();

    client.set_paused(&true, &PauseScope::Settlement, &PauseReason::Incident);

    assert_contract_error(
        client.try_repay(&payer, &1),
        EscrowError::PausedBlocksSettlement,
    );
    assert_eq!(client.get_escrow().status, 3);
    assert_eq!(client.get_repayment_status(), (0, pool));
    assert_eq!(token.token.balance(&contract_id), 0);
}

#[test]
fn repay_blocked_by_all_pause() {
    let env = Env::default();
    let (client, token, _investors, payer, pool) =
        withdraw_and_fund_payer(&env, "REPAY_PAUSE_ALL", &[PRINCIPAL / 2, PRINCIPAL / 2]);
    let contract_id = client.address.clone();

    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);

    assert_contract_error(
        client.try_repay(&payer, &1),
        EscrowError::PausedBlocksSettlement,
    );
    assert_eq!(client.get_escrow().status, 3);
    assert_eq!(client.get_repayment_status(), (0, pool));
    assert_eq!(token.token.balance(&contract_id), 0);
}

#[test]
fn repay_emits_repaid_and_settled_payloads() {
    let env = Env::default();
    let (client, _token, _investors, payer, pool) =
        withdraw_and_fund_payer(&env, "REPAY_EVENTS", &[PRINCIPAL / 2, PRINCIPAL / 2]);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id.clone();
    let first = pool / 2;

    env.ledger().set_timestamp(7_777);
    client.repay(&payer, &first);

    let events = env.events().all();
    let expected_partial_repaid = EscrowRepaid {
        name: symbol_short!("repaid"),
        invoice_id: invoice_id.clone(),
        payer: payer.clone(),
        amount: first,
        repaid_total: first,
        outstanding: pool - first,
    };
    let expected_settled = EscrowSettled {
        name: symbol_short!("escrow_sd"),
        invoice_id: invoice_id.clone(),
        funded_amount: PRINCIPAL,
        yield_bps: YIELD_BPS,
        maturity: 0,
        settled_at_ledger_timestamp: 7_777,
        settle_pool: pool,
    };

    assert!(events
        .events()
        .contains(&expected_partial_repaid.to_xdr(&env, &contract_id)));
    assert!(!events
        .events()
        .contains(&expected_settled.to_xdr(&env, &contract_id)));
    assert_eq!(
        events.events().last().unwrap().clone(),
        expected_partial_repaid.to_xdr(&env, &contract_id)
    );

    client.repay(&payer, &(pool - first));

    let events = env.events().all();
    let expected_full_repaid = EscrowRepaid {
        name: symbol_short!("repaid"),
        invoice_id: invoice_id.clone(),
        payer,
        amount: pool - first,
        repaid_total: pool,
        outstanding: 0,
    };

    assert!(events
        .events()
        .contains(&expected_settled.to_xdr(&env, &contract_id)));
    assert!(events
        .events()
        .contains(&expected_full_repaid.to_xdr(&env, &contract_id)));
    assert_eq!(
        events.events().last().unwrap().clone(),
        expected_full_repaid.to_xdr(&env, &contract_id)
    );
}

#[test]
fn partial_repayment_protects_repaid_funds_from_dust_sweep() {
    let env = Env::default();
    let (client, token, _investors, payer, pool) =
        withdraw_and_fund_payer(&env, "REPAY_SWEEP", &[PRINCIPAL / 2, PRINCIPAL / 2]);
    let contract_id = client.address.clone();
    let repaid = pool * 40 / 100;

    client.repay(&payer, &repaid);
    assert_eq!(client.get_escrow().status, 3);
    assert_eq!(client.get_repayment_status(), (repaid, pool - repaid));
    assert_eq!(token.token.balance(&contract_id), repaid);

    assert_contract_error(
        client.try_sweep_terminal_dust(&1),
        EscrowError::SweepExceedsLiabilityFloor,
    );
    assert_contract_error(
        client.try_sweep_terminal_dust(&MAX_DUST_SWEEP_AMOUNT),
        EscrowError::SweepExceedsLiabilityFloor,
    );
    assert_eq!(client.get_escrow().status, 3);
    assert_eq!(token.token.balance(&contract_id), repaid);
}

#[test]
fn full_repayment_claims_sum_to_pool_minus_rounding_residue() {
    let env = Env::default();
    let (client, token, investors) =
        setup_funded_with(&env, "REPAY_RESIDUE", &[333_333_333, 666_666_667]);
    let contract_id = client.address.clone();
    client.withdraw();

    let payer = Address::generate(&env);
    let pool = client.get_settlement_pool();
    token.stellar.mint(&payer, &pool);
    client.repay(&payer, &pool);

    let mut sum = 0i128;
    for investor in &investors {
        let payout = client.compute_investor_payout(investor);
        sum += payout;
        client.claim_investor_payout(investor);
        assert!(token.token.balance(&contract_id) >= 0);
    }

    assert_eq!(sum, pool - 1);
    assert_eq!(token.token.balance(&contract_id), 1);
}

#[test]
fn new_repayment_error_codes_are_appended() {
    assert_eq!(EscrowError::LegalHoldBlocksRepayment as u32, 260);
    assert_eq!(EscrowError::RepaymentNotOpen as u32, 261);
    assert_eq!(EscrowError::RepaymentAmountNotPositive as u32, 262);
    assert_eq!(EscrowError::RepaymentExceedsOutstanding as u32, 263);
}
