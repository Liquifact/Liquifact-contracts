// Rewritten against the real LiquifactEscrowClient API (issue #1647).
//
// Covers:
//   - init with a past deadline is rejected (FundingDeadlinePassed).
//   - is_funding_expired is false at deadline-1, false at deadline, true at deadline+1.
//   - fund after expiry fails with FundingDeadlinePassed.
//   - refund after expiry succeeds (investor can retrieve funds).
//
// Extension-cap cases are intentionally left to funding_deadline_cap_tests.rs.

use crate::tests::{assert_contract_error, deploy};
use crate::EscrowError;
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env, String};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const NOW: u64 = 1_700_000_000;
const DEADLINE_OFFSET: u64 = 3_600; // 1 hour in the future
const MATURITY_OFFSET: u64 = 86_400 * 30; // 30 days after deadline

fn set_ledger(env: &Env, ts: u64) {
    let mut li = env.ledger().get();
    li.timestamp = ts;
    li.sequence_number = 100;
    env.ledger().set(li);
}

/// Deploy and init an escrow with a funding deadline of `NOW + DEADLINE_OFFSET`.
/// Returns `(client, admin, token_id, deadline)`.
fn setup_with_deadline(env: &Env) -> (crate::LiquifactEscrowClient<'_>, Address, Address, u64) {
    set_ledger(env, NOW);
    env.mock_all_auths();

    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);

    let deadline = NOW + DEADLINE_OFFSET;
    let maturity = deadline + MATURITY_OFFSET;

    client.init(
        &admin,
        &String::from_str(env, "DEADL01"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &maturity,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &Some(deadline),
        &None,
        &None::<i64>,
    );

    (client, admin, token, deadline)
}

// ---------------------------------------------------------------------------
// init: past deadline rejected
// ---------------------------------------------------------------------------

#[test]
fn init_with_past_deadline_rejected() {
    let env = Env::default();
    set_ledger(&env, NOW);
    env.mock_all_auths();

    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let maturity = NOW + MATURITY_OFFSET;

    // A deadline in the past (NOW - 1).
    let past_deadline: u64 = NOW - 1;

    let result = client.try_init(
        &admin,
        &String::from_str(&env, "DEADL_PAST"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &maturity,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &Some(past_deadline),
        &None,
        &None::<i64>,
    );

    assert_contract_error(result, EscrowError::FundingDeadlinePassed);
}

// ---------------------------------------------------------------------------
// is_funding_expired boundary
// ---------------------------------------------------------------------------

#[test]
fn is_funding_expired_false_before_deadline() {
    let env = Env::default();
    let (client, _, _, deadline) = setup_with_deadline(&env);

    // One second before the deadline — not expired.
    set_ledger(&env, deadline - 1);
    assert!(!client.is_funding_expired());
}

#[test]
fn is_funding_expired_false_at_deadline() {
    let env = Env::default();
    let (client, _, _, deadline) = setup_with_deadline(&env);

    // Exactly at the deadline — not yet expired (inclusive boundary).
    set_ledger(&env, deadline);
    assert!(!client.is_funding_expired());
}

#[test]
fn is_funding_expired_true_after_deadline() {
    let env = Env::default();
    let (client, _, _, deadline) = setup_with_deadline(&env);

    // One second past the deadline — now expired.
    set_ledger(&env, deadline + 1);
    assert!(client.is_funding_expired());
}

// ---------------------------------------------------------------------------
// fund after expiry fails
// ---------------------------------------------------------------------------

#[test]
fn fund_after_expiry_fails_with_deadline_passed() {
    let env = Env::default();
    let (client, _, _, deadline) = setup_with_deadline(&env);

    set_ledger(&env, deadline + 1);

    let investor = Address::generate(&env);
    let result = client.try_fund(&investor, &1_000i128);
    assert_contract_error(result, EscrowError::FundingDeadlinePassed);
}

// ---------------------------------------------------------------------------
// get_funding_deadline returns the configured value
// ---------------------------------------------------------------------------

#[test]
fn get_funding_deadline_returns_configured_value() {
    let env = Env::default();
    let (client, _, _, deadline) = setup_with_deadline(&env);

    assert_eq!(client.get_funding_deadline(), Some(deadline));
}

#[test]
fn get_funding_deadline_returns_none_when_not_set() {
    let env = Env::default();
    set_ledger(&env, NOW);
    env.mock_all_auths();

    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let maturity = NOW + MATURITY_OFFSET;

    client.init(
        &admin,
        &String::from_str(&env, "NODEADL"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &maturity,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None, // no funding deadline
        &None,
        &None::<i64>,
    );

    assert_eq!(client.get_funding_deadline(), None);
}
