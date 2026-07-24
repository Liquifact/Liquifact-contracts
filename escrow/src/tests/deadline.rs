// Tests for funding-deadline expiry and cancellation lifecycle.
//
// Covers:
//   - Funding before deadline succeeds (well before and at exactly the deadline)
//   - Funding exactly at the deadline is allowed (inclusive boundary)
//   - Funding one ledger past the deadline is rejected with FundingDeadlinePassed
//   - Funding well past the deadline is rejected with FundingDeadlinePassed
//   - An expired escrow (under-funded, past deadline) is cancellable by admin
//   - Investors receive full principal back after cancellation + refund
//   - Refund correctness: zeroes contribution, marks investor refunded, actual token transfer
//   - Multiple investors can all independently refund after deadline-triggered cancel
//   - FundingDeadlinePassed is not emitted when no deadline is configured
//   - init rejects a deadline at-or-before current ledger timestamp
//   - get_funding_deadline and is_funding_expired read correctly across boundary
//   - Legal hold still blocks cancel even after the deadline passes
//   - fund_with_commitment is also blocked after the deadline

use soroban_sdk::{
    testutils::Address as _, testutils::Ledger as _, Address, Env, String as SorobanString,
};

use crate::{
    tests::{deploy, free_addresses, install_stellar_asset_token, setup, TARGET},
    EscrowError, LiquifactEscrowClient,
};

// ── local helpers ─────────────────────────────────────────────────────────────

/// The deadline used by most tests.
/// setup() sets timestamp = 0; DEADLINE = 20_000 gives plenty of headroom.
const DEADLINE: u64 = 20_000;

/// Local error-assertion helper (mirrors the one in tests.rs).
fn assert_contract_error<T, E>(
    result: Result<Result<T, E>, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
    expected: EscrowError,
) where
    T: std::fmt::Debug,
    E: std::fmt::Debug,
{
    let expected_code = expected as u32;
    match result {
        Err(Ok(err)) => {
            assert_eq!(
                err,
                soroban_sdk::Error::from_contract_error(expected_code),
                "expected ContractError({expected_code})"
            );
        }
        Err(Err(soroban_sdk::InvokeError::Contract(code))) => {
            assert_eq!(
                code, expected_code,
                "expected ContractError({expected_code})"
            );
        }
        other => panic!("expected ContractError({expected_code}), got {other:?}"),
    }
}

/// Init an escrow with a real SAC token and a funding deadline.
/// Returns `(client, investor_addr, token_wrapper, treasury_addr)`.
fn init_with_deadline<'a>(
    env: &'a Env,
    deadline: u64,
) -> (
    LiquifactEscrowClient<'a>,
    Address,
    crate::tests::StellarTestToken<'a>,
    Address,
) {
    let (client, admin, sme) = setup(env);
    let token = install_stellar_asset_token(env);
    let treasury = Address::generate(env);
    let investor = Address::generate(env);

    client.init(
        &admin,
        &SorobanString::from_str(env, "DL001"),
        &sme,
        &TARGET,
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
        &None,              // maturity_max_horizon
        &Some(deadline),    // funding_deadline
        &None,              // allowlist_active
        &None::<i64>,       // protocol_fee_bps
    );

    (client, investor, token, treasury)
}

/// Mint `amount` tokens directly into the escrow contract so that `refund()` can transfer them.
fn mint_into_escrow(
    token: &crate::tests::StellarTestToken<'_>,
    escrow_addr: &Address,
    amount: i128,
) {
    token.stellar.mint(escrow_addr, &amount);
}

// ── Section 1: deadline boundary on fund() ───────────────────────────────────

/// Funding well before the deadline must succeed.
#[test]
fn deadline_fund_well_before_deadline_succeeds() {
    let env = Env::default();
    let (client, investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    // setup() sets timestamp = 0; DEADLINE = 20_000 → plenty of headroom.
    let result = client.fund(&investor, &(TARGET / 2));
    assert_eq!(result.funded_amount, TARGET / 2);
}

/// Funding at exactly the deadline timestamp is allowed (inclusive boundary).
#[test]
fn deadline_fund_at_exactly_deadline_succeeds() {
    let env = Env::default();
    let (client, investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    env.ledger().with_mut(|l| l.timestamp = DEADLINE);
    let result = client.fund(&investor, &(TARGET / 2));
    assert_eq!(result.funded_amount, TARGET / 2);
}

/// Funding one second past the deadline must be rejected with FundingDeadlinePassed.
#[test]
fn deadline_fund_one_past_deadline_rejected() {
    let env = Env::default();
    let (client, investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    assert_contract_error(
        client.try_fund(&investor, &(TARGET / 2)),
        EscrowError::FundingDeadlinePassed,
    );
}

/// Funding well past the deadline must also be rejected with FundingDeadlinePassed.
#[test]
fn deadline_fund_well_past_deadline_rejected() {
    let env = Env::default();
    let (client, investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 100_000);
    assert_contract_error(
        client.try_fund(&investor, &(TARGET / 2)),
        EscrowError::FundingDeadlinePassed,
    );
}

// ── Section 2: deadline boundary on fund_with_commitment() ───────────────────

/// fund_with_commitment at exactly the deadline is allowed.
#[test]
fn deadline_fund_with_commitment_at_deadline_succeeds() {
    let env = Env::default();
    let (client, investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    env.ledger().with_mut(|l| l.timestamp = DEADLINE);
    let result = client.fund_with_commitment(&investor, &(TARGET / 2), &0u64);
    assert_eq!(result.funded_amount, TARGET / 2);
}

/// fund_with_commitment one second past the deadline must be rejected.
#[test]
fn deadline_fund_with_commitment_past_deadline_rejected() {
    let env = Env::default();
    let (client, investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    assert_contract_error(
        client.try_fund_with_commitment(&investor, &(TARGET / 2), &0u64),
        EscrowError::FundingDeadlinePassed,
    );
}

// ── Section 3: get_funding_deadline and is_funding_expired ──────────────────

/// get_funding_deadline returns None when no deadline was configured.
#[test]
fn deadline_get_funding_deadline_none_when_unset() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (tok, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &SorobanString::from_str(&env, "NODL001"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &tok,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,        // maturity_max_horizon
        &None,        // funding_deadline: none
        &None,        // allowlist_active
        &None::<i64>, // protocol_fee_bps
    );
    assert_eq!(client.get_funding_deadline(), None);
}

/// get_funding_deadline returns the stored deadline when configured.
#[test]
fn deadline_get_funding_deadline_returns_stored_value() {
    let env = Env::default();
    let (client, _investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    assert_eq!(client.get_funding_deadline(), Some(DEADLINE));
}

/// is_funding_expired is false before the deadline.
#[test]
fn deadline_is_funding_expired_false_before_deadline() {
    let env = Env::default();
    let (client, _investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    // timestamp = 0, DEADLINE = 20_000
    assert!(!client.is_funding_expired());
}

/// is_funding_expired is false exactly at the deadline.
#[test]
fn deadline_is_funding_expired_false_at_deadline() {
    let env = Env::default();
    let (client, _investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    env.ledger().with_mut(|l| l.timestamp = DEADLINE);
    assert!(!client.is_funding_expired());
}

/// is_funding_expired is true one second past the deadline.
#[test]
fn deadline_is_funding_expired_true_one_past_deadline() {
    let env = Env::default();
    let (client, _investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    assert!(client.is_funding_expired());
}

/// is_funding_expired is false when no deadline is configured.
#[test]
fn deadline_is_funding_expired_false_when_no_deadline() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (tok, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &SorobanString::from_str(&env, "NODL002"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &tok,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,        // maturity_max_horizon
        &None,        // funding_deadline: none
        &None,        // allowlist_active
        &None::<i64>, // protocol_fee_bps
    );
    env.ledger().with_mut(|l| l.timestamp = u64::MAX / 2);
    assert!(!client.is_funding_expired());
}

// ── Section 4: init rejects stale deadline ───────────────────────────────────

/// init must reject a deadline equal to the current ledger timestamp.
#[test]
fn deadline_init_rejects_deadline_equal_to_current_timestamp() {
    let env = Env::default();
    // setup() sets timestamp = 0; advance to a known value first.
    let (_, admin, sme) = setup(&env);
    env.ledger().with_mut(|l| l.timestamp = 5_000);
    let client = deploy(&env);
    let (tok, treasury) = free_addresses(&env);
    // deadline == current timestamp (5_000) → must fail
    assert_contract_error(
        client.try_init(
            &admin,
            &SorobanString::from_str(&env, "STALE01"),
            &sme,
            &TARGET,
            &800i64,
            &0u64,
            &tok,
            &None,
            &treasury,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,          // maturity_max_horizon
            &Some(5_000u64), // deadline == now
            &None,
            &None::<i64>,
        ),
        EscrowError::FundingDeadlinePassed,
    );
}

/// init must reject a deadline strictly before the current ledger timestamp.
#[test]
fn deadline_init_rejects_deadline_before_current_timestamp() {
    let env = Env::default();
    let (_, admin, sme) = setup(&env);
    env.ledger().with_mut(|l| l.timestamp = 5_000);
    let client = deploy(&env);
    let (tok, treasury) = free_addresses(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &SorobanString::from_str(&env, "STALE02"),
            &sme,
            &TARGET,
            &800i64,
            &0u64,
            &tok,
            &None,
            &treasury,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,         // maturity_max_horizon
            &Some(100u64), // 100 < 5_000
            &None,
            &None::<i64>,
        ),
        EscrowError::FundingDeadlinePassed,
    );
}

// ── Section 5: cancel_funding after deadline expiry ─────────────────────────

/// An under-funded escrow past its deadline is cancellable (status 0 → 4).
#[test]
fn deadline_expired_underfunded_escrow_is_cancellable() {
    let env = Env::default();
    let (client, investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    // Partially fund while still open
    client.fund(&investor, &(TARGET / 2));
    assert_eq!(client.get_escrow().status, 0);

    // Advance past the deadline
    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);

    // Cancel should succeed
    let result = client.cancel_funding();
    assert_eq!(result.status, 4);
}

/// After deadline expiry, cancel_funding preserves the funded_amount in the escrow snapshot.
#[test]
fn deadline_cancel_preserves_funded_amount_after_expiry() {
    let env = Env::default();
    let (client, investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    let partial = TARGET / 3;
    client.fund(&investor, &partial);

    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    let cancelled = client.cancel_funding();
    assert_eq!(cancelled.funded_amount, partial);
    assert_eq!(cancelled.status, 4);
}

/// A fully-funded escrow (status 1) cannot be cancelled even after the deadline.
#[test]
fn deadline_fully_funded_escrow_cannot_be_cancelled_after_deadline() {
    let env = Env::default();
    let (client, investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    client.fund(&investor, &TARGET);
    assert_eq!(client.get_escrow().status, 1);

    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    assert_contract_error(
        client.try_cancel_funding(),
        EscrowError::CancelFundingNotOpen,
    );
}

/// Legal hold still blocks cancel_funding even when the deadline has expired.
#[test]
fn deadline_legal_hold_blocks_cancel_after_expiry() {
    let env = Env::default();
    let (client, investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    client.fund(&investor, &(TARGET / 2));
    client.set_legal_hold(&true);

    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    assert_contract_error(
        client.try_cancel_funding(),
        EscrowError::LegalHoldBlocksCancelFunding,
    );
}

// ── Section 6: refund correctness after deadline-triggered cancel ─────────────

/// Single investor receives exact principal back via refund after deadline cancel.
#[test]
fn deadline_refund_single_investor_correct_amount() {
    let env = Env::default();
    let (client, investor, token, _treasury) = init_with_deadline(&env, DEADLINE);
    let principal = TARGET / 2;
    mint_into_escrow(&token, &client.address, principal);
    client.fund(&investor, &principal);

    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    client.cancel_funding();

    let before = token.token.balance(&investor);
    client.refund(&investor);
    let after = token.token.balance(&investor);
    assert_eq!(after - before, principal);
}

/// Refund zeroes the investor's stored contribution.
#[test]
fn deadline_refund_zeroes_contribution() {
    let env = Env::default();
    let (client, investor, token, _treasury) = init_with_deadline(&env, DEADLINE);
    let principal = TARGET / 4;
    mint_into_escrow(&token, &client.address, principal);
    client.fund(&investor, &principal);

    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    client.cancel_funding();
    client.refund(&investor);

    assert_eq!(client.get_contribution(&investor), 0);
}

/// Refund marks investor as refunded (is_investor_refunded returns true).
#[test]
fn deadline_refund_marks_investor_refunded() {
    let env = Env::default();
    let (client, investor, token, _treasury) = init_with_deadline(&env, DEADLINE);
    let principal = TARGET / 4;
    mint_into_escrow(&token, &client.address, principal);
    client.fund(&investor, &principal);

    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    client.cancel_funding();

    assert!(!client.is_investor_refunded(&investor));
    client.refund(&investor);
    assert!(client.is_investor_refunded(&investor));
}

/// Double refund is rejected (NoContributionToRefund on second call).
#[test]
fn deadline_refund_double_spend_rejected() {
    let env = Env::default();
    let (client, investor, token, _treasury) = init_with_deadline(&env, DEADLINE);
    let principal = TARGET / 4;
    mint_into_escrow(&token, &client.address, principal);
    client.fund(&investor, &principal);

    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    client.cancel_funding();
    client.refund(&investor);

    assert_contract_error(
        client.try_refund(&investor),
        EscrowError::NoContributionToRefund,
    );
}

/// Refund before cancel (escrow still open) is rejected with RefundNotCancelled.
#[test]
fn deadline_refund_before_cancel_rejected() {
    let env = Env::default();
    let (client, investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    client.fund(&investor, &(TARGET / 2));

    // Do NOT cancel — refund call must fail
    assert_contract_error(
        client.try_refund(&investor),
        EscrowError::RefundNotCancelled,
    );
}

/// A non-investor address cannot refund after deadline cancel.
#[test]
fn deadline_refund_non_investor_rejected() {
    let env = Env::default();
    let (client, investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    client.fund(&investor, &(TARGET / 2));

    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    client.cancel_funding();

    let stranger = Address::generate(&env);
    assert_contract_error(
        client.try_refund(&stranger),
        EscrowError::NoContributionToRefund,
    );
}

// ── Section 7: multiple investors refund independently after deadline cancel ──

/// All investors can independently refund after deadline-triggered cancellation,
/// and each receives their exact contribution back.
#[test]
fn deadline_multiple_investors_all_refunded_correctly() {
    let env = Env::default();
    let (client, _, token, _treasury) = init_with_deadline(&env, DEADLINE);

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);

    let amt_a = TARGET / 5;
    let amt_b = TARGET / 6;
    let amt_c = TARGET / 7;
    let total = amt_a + amt_b + amt_c;

    mint_into_escrow(&token, &client.address, total);

    client.fund(&inv_a, &amt_a);
    client.fund(&inv_b, &amt_b);
    client.fund(&inv_c, &amt_c);

    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    client.cancel_funding();

    let before_a = token.token.balance(&inv_a);
    let before_b = token.token.balance(&inv_b);
    let before_c = token.token.balance(&inv_c);

    client.refund(&inv_a);
    client.refund(&inv_b);
    client.refund(&inv_c);

    assert_eq!(token.token.balance(&inv_a) - before_a, amt_a);
    assert_eq!(token.token.balance(&inv_b) - before_b, amt_b);
    assert_eq!(token.token.balance(&inv_c) - before_c, amt_c);
}

/// After one investor refunds, the other's refund is unaffected (independence).
#[test]
fn deadline_one_investor_refund_does_not_affect_other() {
    let env = Env::default();
    let (client, _, token, _treasury) = init_with_deadline(&env, DEADLINE);

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let amt_a = TARGET / 4;
    let amt_b = TARGET / 4;
    mint_into_escrow(&token, &client.address, amt_a + amt_b);

    client.fund(&inv_a, &amt_a);
    client.fund(&inv_b, &amt_b);

    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    client.cancel_funding();

    // inv_a refunds first
    client.refund(&inv_a);
    assert_eq!(client.get_contribution(&inv_a), 0);
    // inv_b contribution is intact
    assert_eq!(client.get_contribution(&inv_b), amt_b);

    // inv_b refunds
    let before_b = token.token.balance(&inv_b);
    client.refund(&inv_b);
    assert_eq!(token.token.balance(&inv_b) - before_b, amt_b);
}

// ── Section 8: no deadline configured — no FundingDeadlinePassed ─────────────

/// Without a deadline, funding succeeds at any ledger timestamp.
#[test]
fn no_deadline_fund_succeeds_at_arbitrary_high_timestamp() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let (tok, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &SorobanString::from_str(&env, "NODL003"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &tok,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,        // maturity_max_horizon
        &None,        // funding_deadline: none
        &None,        // allowlist_active
        &None::<i64>, // protocol_fee_bps
    );

    env.ledger().with_mut(|l| l.timestamp = 999_999_999u64);
    let result = client.fund(&investor, &(TARGET / 2));
    assert_eq!(result.funded_amount, TARGET / 2);
}

// ── Section 9: deadline does not affect non-funding entrypoints ──────────────

/// Settling a fully-funded escrow is unrelated to the deadline —
/// it should succeed normally even if timestamp is past the deadline.
#[test]
fn deadline_does_not_block_settle_on_funded_escrow() {
    let env = Env::default();
    let (client, investor, _token, _treasury) = init_with_deadline(&env, DEADLINE);
    // Fund to target before deadline
    client.fund(&investor, &TARGET);
    assert_eq!(client.get_escrow().status, 1);

    // Advance past deadline
    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 500);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

// ── Section 10: full lifecycle test ─────────────────────────────────────────

/// Complete deadline-expiry lifecycle:
///   1. Init with deadline
///   2. Partial funding before deadline
///   3. Failed fund attempt after deadline
///   4. Admin cancels
///   5. Investor refunds and gets principal back
#[test]
fn deadline_full_lifecycle_partial_fund_expire_cancel_refund() {
    let env = Env::default();
    let (client, investor, token, _treasury) = init_with_deadline(&env, DEADLINE);

    // 1. Fund partially well before deadline
    let partial = TARGET / 3;
    mint_into_escrow(&token, &client.address, partial);
    let funded = client.fund(&investor, &partial);
    assert_eq!(funded.status, 0, "escrow must still be open (under-funded)");
    assert_eq!(funded.funded_amount, partial);

    // 2. Advance to exactly the deadline — still open, fund OK
    env.ledger().with_mut(|l| l.timestamp = DEADLINE);
    assert!(!client.is_funding_expired(), "at deadline: not yet expired");

    // 3. Advance one second past deadline — fund now rejected
    env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    assert!(client.is_funding_expired(), "one past deadline: expired");
    let second_investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&second_investor, &(TARGET / 4)),
        EscrowError::FundingDeadlinePassed,
    );

    // 4. Admin cancels the expired escrow
    let cancelled = client.cancel_funding();
    assert_eq!(cancelled.status, 4, "status must be 4 (cancelled)");
    assert_eq!(
        cancelled.funded_amount, partial,
        "funded_amount must be preserved"
    );

    // 5. Investor refunds and receives exact principal
    let before = token.token.balance(&investor);
    client.refund(&investor);
    let after = token.token.balance(&investor);
    assert_eq!(
        after - before,
        partial,
        "investor must receive exact principal back"
    );
    assert_eq!(
        client.get_contribution(&investor),
        0,
        "contribution must be zeroed"
    );
    assert!(
        client.is_investor_refunded(&investor),
        "investor must be marked as refunded"
    );
}
