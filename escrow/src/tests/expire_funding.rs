//! Tests for the permissionless [`LiquifactEscrow::expire_funding`] transition.
//!
//! Covers: before-deadline rejection (including the inclusive boundary), no deadline set,
//! success by an unauthenticated caller after the deadline, funded escrows, refunds after
//! expiry, a repeat call, legal hold, operational pause, and the emitted events.

use super::*;
use crate::{FundingExpired, InvoiceEscrow};

const DEADLINE: u64 = 2_000;
const TARGET_AMOUNT: i128 = 10_000;

fn set_time(env: &Env, ts: u64) {
    env.ledger().set_timestamp(ts);
}

/// Open escrow backed by a real stellar asset token, with the given funding deadline.
fn open_escrow<'a>(
    env: &'a Env,
    deadline: Option<u64>,
) -> (LiquifactEscrowClient<'a>, StellarTestToken<'a>) {
    let (client, admin, sme) = setup(env);
    let token = install_stellar_asset_token(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &String::from_str(env, "EXPIRE01"),
        &sme,
        &TARGET_AMOUNT,
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
        &deadline,
        &None,
        &None::<i64>,
    );
    (client, token)
}

fn fund_with_real_tokens(
    env: &Env,
    client: &LiquifactEscrowClient<'_>,
    token: &StellarTestToken<'_>,
    amount: i128,
) -> Address {
    let investor = Address::generate(env);
    token.stellar.mint(&investor, &amount);
    client.fund(&investor, &amount);
    investor
}

#[test]
fn fails_before_deadline() {
    let env = Env::default();
    let (client, _token) = open_escrow(&env, Some(DEADLINE));

    set_time(&env, 0);
    assert_contract_error(client.try_expire_funding(), EscrowError::FundingNotExpired);
    assert_eq!(client.get_escrow().status, 0);
}

#[test]
fn fails_exactly_at_deadline() {
    // `is_funding_expired` is strictly `now > deadline`, so the deadline second is still open.
    let env = Env::default();
    let (client, _token) = open_escrow(&env, Some(DEADLINE));

    set_time(&env, DEADLINE);
    assert!(!client.is_funding_expired());
    assert_contract_error(client.try_expire_funding(), EscrowError::FundingNotExpired);
    assert_eq!(client.get_escrow().status, 0);
}

#[test]
fn fails_when_no_deadline_is_configured() {
    let env = Env::default();
    let (client, _token) = open_escrow(&env, None);

    set_time(&env, u64::MAX / 2);
    assert_contract_error(client.try_expire_funding(), EscrowError::FundingNotExpired);
    assert_eq!(client.get_escrow().status, 0);
}

#[test]
fn succeeds_after_deadline_without_any_authorization() {
    let env = Env::default();
    let (client, token) = open_escrow(&env, Some(DEADLINE));
    fund_with_real_tokens(&env, &client, &token, 1_000);

    set_time(&env, DEADLINE + 1);
    assert!(client.is_funding_expired());

    let escrow: InvoiceEscrow = client.expire_funding();

    assert_eq!(escrow.status, 4);
    assert_eq!(escrow.funded_amount, 1_000);
    assert_eq!(client.get_escrow().status, 4);
    // No `require_auth` was recorded: anyone can trigger the expiry.
    assert!(env.auths().is_empty());
}

#[test]
fn funded_escrow_cannot_expire() {
    let env = Env::default();
    let (client, token) = open_escrow(&env, Some(DEADLINE));
    fund_with_real_tokens(&env, &client, &token, TARGET_AMOUNT);
    assert_eq!(client.get_escrow().status, 1);

    set_time(&env, DEADLINE + 1);
    assert_contract_error(
        client.try_expire_funding(),
        EscrowError::CancelFundingNotOpen,
    );
    assert_eq!(client.get_escrow().status, 1);
}

#[test]
fn refunds_work_after_expiry() {
    let env = Env::default();
    let (client, token) = open_escrow(&env, Some(DEADLINE));
    let a = fund_with_real_tokens(&env, &client, &token, 1_500);
    let b = fund_with_real_tokens(&env, &client, &token, 2_500);

    // Refund is locked while the escrow is open.
    assert_contract_error(client.try_refund(&a), EscrowError::RefundNotCancelled);

    set_time(&env, DEADLINE + 1);
    client.expire_funding();

    client.refund(&a);
    assert_eq!(token.token.balance(&a), 1_500);
    assert_eq!(client.get_contribution(&a), 0);

    // `refund_batch` is permissionless; it returns (count, total).
    let investors = SorobanVec::from_array(&env, [b.clone()]);
    assert_eq!(client.refund_batch(&investors), (1, 2_500));
    assert_eq!(token.token.balance(&b), 2_500);
    assert_eq!(token.token.balance(&client.address), 0);
}

#[test]
fn second_call_fails_with_not_open() {
    let env = Env::default();
    let (client, _token) = open_escrow(&env, Some(DEADLINE));

    set_time(&env, DEADLINE + 1);
    client.expire_funding();

    assert_contract_error(
        client.try_expire_funding(),
        EscrowError::CancelFundingNotOpen,
    );
    assert_eq!(client.get_escrow().status, 4);
}

#[test]
fn cannot_expire_after_admin_cancel() {
    let env = Env::default();
    let (client, _token) = open_escrow(&env, Some(DEADLINE));

    client.cancel_funding();
    set_time(&env, DEADLINE + 1);
    assert_contract_error(
        client.try_expire_funding(),
        EscrowError::CancelFundingNotOpen,
    );
}

#[test]
fn legal_hold_blocks_expiry_like_cancel_funding() {
    let env = Env::default();
    let (client, _token) = open_escrow(&env, Some(DEADLINE));
    set_time(&env, DEADLINE + 1);

    client.set_legal_hold(&true);
    assert_contract_error(
        client.try_expire_funding(),
        EscrowError::LegalHoldBlocksCancelFunding,
    );
    assert_contract_error(
        client.try_cancel_funding(),
        EscrowError::LegalHoldBlocksCancelFunding,
    );
    assert_eq!(client.get_escrow().status, 0);
}

#[test]
fn operational_pause_does_not_block_expiry_like_cancel_funding() {
    let env = Env::default();
    let (client, _token) = open_escrow(&env, Some(DEADLINE));
    set_time(&env, DEADLINE + 1);

    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    assert!(client.is_paused());

    assert_eq!(client.expire_funding().status, 4);
}

#[test]
fn emits_funding_cancelled_then_funding_expired() {
    let env = Env::default();
    let (client, token) = open_escrow(&env, Some(DEADLINE));
    fund_with_real_tokens(&env, &client, &token, 1_000);
    let invoice_id = client.get_escrow().invoice_id;

    let now = DEADLINE + 5;
    set_time(&env, now);
    client.expire_funding();

    let all = env.events().all();
    let events = all.events();
    let n = events.len();
    assert!(
        n >= 2,
        "expected at least FundingCancelled and FundingExpired"
    );

    assert_eq!(
        events[n - 2],
        FundingCancelled {
            name: symbol_short!("fund_can"),
            invoice_id: invoice_id.clone(),
            funded_amount: 1_000,
        }
        .to_xdr(&env, &client.address)
    );
    assert_eq!(
        events[n - 1],
        FundingExpired {
            name: symbol_short!("fund_exp"),
            invoice_id,
            funded_amount: 1_000,
            deadline: DEADLINE,
            expired_at: now,
        }
        .to_xdr(&env, &client.address)
    );
}

#[test]
fn uninitialized_contract_fails() {
    let env = Env::default();
    let (client, _admin, _sme) = setup(&env);
    assert!(client.try_expire_funding().is_err());
}
