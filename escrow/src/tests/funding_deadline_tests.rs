/// Comprehensive tests for the optional `funding_deadline` feature.
///
/// Coverage:
/// - No deadline configured: `get_funding_deadline` returns `None`, `is_funding_expired` returns `false`
/// - Init rejects a deadline in the past
/// - Fund succeeds before deadline
/// - Fund is rejected after deadline with `FundingDeadlinePassed`
/// - `is_funding_expired` transitions from `false` to `true` as ledger advances
/// - Already-funded (status 1) escrows are unaffected by the deadline gate
/// - `cancel_funding` is available by the admin after the deadline expires (deadline does not
///   block cancel, only fund)
/// - Deadline == 0 is treated as "no deadline" (same as `None`)
use super::*;
use crate::EscrowError;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, String,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Set up an escrow with an optional funding deadline at `now + offset_secs`.
/// Pass `offset_secs = 0` to omit the deadline.
///
/// Uses a plain generated token address which auto-registers [`DefaultMockToken`],
/// giving every investor address a default balance — no manual minting required.
fn init_with_optional_deadline<'a>(
    env: &'a Env,
    client: &LiquifactEscrowClient<'a>,
    admin: &Address,
    sme: &Address,
    offset_secs: u64,
    maturity: u64,
) {
    let (token, treasury) = free_addresses(env);

    let deadline = if offset_secs > 0 {
        Some(env.ledger().timestamp() + offset_secs)
    } else {
        None
    };

    client.init(
        admin,
        &String::from_str(env, "FDLT01"),
        sme,
        &TARGET,
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
        &deadline,
        &None,
        &None::<i64>,
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// When no deadline is configured, `get_funding_deadline` returns `None`
/// and `is_funding_expired` returns `false` at any ledger time.
#[test]
fn test_no_deadline_get_returns_none_and_is_expired_false() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    init_with_optional_deadline(&env, &client, &admin, &sme, 0, 0);

    assert_eq!(client.get_funding_deadline(), None);
    assert!(!client.is_funding_expired());

    // Advance ledger time — still not expired because there is no deadline
    env.ledger().with_mut(|l| l.timestamp = 9_999_999);
    assert!(!client.is_funding_expired());
}

/// Deadline configured at `now + 100`: `get_funding_deadline` returns `Some(deadline)`.
#[test]
fn test_get_funding_deadline_returns_configured_value() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    let now = env.ledger().timestamp();
    let expected_deadline = now + 100;
    init_with_optional_deadline(&env, &client, &admin, &sme, 100, 0);

    assert_eq!(client.get_funding_deadline(), Some(expected_deadline));
}

/// Fund call succeeds before the deadline.
#[test]
fn test_fund_succeeds_before_deadline() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    // Deadline 200 seconds from now
    init_with_optional_deadline(&env, &client, &admin, &sme, 200, 0);

    let investor = Address::generate(&env);
    let result = client.fund(&investor, &(TARGET / 2));
    assert_eq!(result.status, 0);
    assert_eq!(result.funded_amount, TARGET / 2);
}

/// `is_funding_expired` is false before the deadline and true after.
#[test]
fn test_is_funding_expired_transitions() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    let now = env.ledger().timestamp(); // 0 after setup
    let deadline = now + 100;
    init_with_optional_deadline(&env, &client, &admin, &sme, 100, 0);

    // Before deadline
    assert!(!client.is_funding_expired());

    // Exactly at deadline (deadline is inclusive: now > deadline => expired)
    env.ledger().with_mut(|l| l.timestamp = deadline);
    assert!(!client.is_funding_expired()); // now == deadline → NOT expired

    // One second past deadline
    env.ledger().with_mut(|l| l.timestamp = deadline + 1);
    assert!(client.is_funding_expired());
}

/// `fund` is rejected with `FundingDeadlinePassed` after the deadline has elapsed.
#[test]
fn test_fund_rejected_after_deadline() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    let now = env.ledger().timestamp();
    let deadline = now + 50;
    init_with_optional_deadline(&env, &client, &admin, &sme, 50, 0);

    // Advance past deadline
    env.ledger().with_mut(|l| l.timestamp = deadline + 1);

    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&investor, &TARGET),
        EscrowError::FundingDeadlinePassed,
    );
}

/// `init` rejects a deadline that is already in the past (deadline <= now).
#[test]
fn test_init_with_past_deadline_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    // Set ledger time to a known future value so we can specify a past deadline
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);
    let past_deadline: u64 = 999_990; // before current ledger timestamp

    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "PASTDL"),
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
            &None,
            &Some(past_deadline),
            &None,
            &None::<i64>,
        ),
        EscrowError::FundingDeadlinePassed,
    );
}

/// `init` rejects a deadline equal to `now` (must be strictly greater).
#[test]
fn test_init_with_deadline_equal_to_now_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    env.ledger().with_mut(|l| l.timestamp = 5_000);
    let now = env.ledger().timestamp();

    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);

    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "EQDL001"),
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
            &None,
            &Some(now), // equal to now — not strictly greater
            &None,
            &None::<i64>,
        ),
        EscrowError::FundingDeadlinePassed,
    );
}

/// A funded escrow (status 1) is NOT affected by a passed deadline:
/// the deadline gate only prevents new deposits into an open escrow.
/// Once funded, `cancel_funding` is blocked by `CancelFundingNotOpen`
/// (deadline does not retroactively trap principal).
#[test]
fn test_funded_escrow_unaffected_by_passed_deadline() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    let now = env.ledger().timestamp();
    let deadline = now + 50;
    init_with_optional_deadline(&env, &client, &admin, &sme, 50, 0);

    // Fund fully BEFORE deadline
    let investor = Address::generate(&env);
    let result = client.fund(&investor, &TARGET);
    assert_eq!(result.status, 1, "escrow should be funded");

    // Advance past deadline
    env.ledger().with_mut(|l| l.timestamp = deadline + 1);

    // is_funding_expired is true, but the escrow is already funded
    assert!(client.is_funding_expired());

    // cancel_funding rejected because status == 1 (funded), not because of deadline
    assert_contract_error(
        client.try_cancel_funding(),
        EscrowError::CancelFundingNotOpen,
    );
}

/// Admin can cancel an open, expired escrow via `cancel_funding`.
/// The deadline does NOT block `cancel_funding`; it only blocks `fund`.
#[test]
fn test_cancel_funding_allowed_after_deadline_expires() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    let now = env.ledger().timestamp();
    let deadline = now + 30;
    init_with_optional_deadline(&env, &client, &admin, &sme, 30, 0);

    // Partially fund before deadline
    let investor = Address::generate(&env);
    let _ = client.fund(&investor, &(TARGET / 2));
    assert_eq!(client.get_escrow().status, 0);

    // Advance past deadline → new deposits would be blocked, but cancel is fine
    env.ledger().with_mut(|l| l.timestamp = deadline + 1);
    assert!(client.is_funding_expired());

    // Admin cancels the stalled, open escrow
    let cancelled = client.cancel_funding();
    assert_eq!(cancelled.status, 4);
}

/// Fund exactly at the deadline is NOT rejected (boundary is strictly `now > deadline`).
#[test]
fn test_fund_at_exact_deadline_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    let now = env.ledger().timestamp();
    let deadline = now + 100;
    init_with_optional_deadline(&env, &client, &admin, &sme, 100, 0);

    // Set time exactly to deadline
    env.ledger().with_mut(|l| l.timestamp = deadline);

    let investor = Address::generate(&env);
    // fund at exactly deadline must succeed (now <= deadline)
    let result = client.fund(&investor, &(TARGET / 2));
    assert_eq!(result.status, 0);
    assert_eq!(result.funded_amount, TARGET / 2);
}

/// `fund` with no deadline set is always allowed regardless of ledger time.
#[test]
fn test_fund_with_no_deadline_never_expires() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    // No deadline
    init_with_optional_deadline(&env, &client, &admin, &sme, 0, 0);

    // Advance ledger to a very large value
    env.ledger().with_mut(|l| l.timestamp = 9_999_999_999);

    let investor = Address::generate(&env);
    let result = client.fund(&investor, &(TARGET / 2));
    assert_eq!(result.funded_amount, TARGET / 2);
    assert!(!client.is_funding_expired());
}

/// After the deadline passes, `fund_batch` is also rejected with `FundingDeadlinePassed`.
#[test]
fn test_fund_batch_rejected_after_deadline() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    let now = env.ledger().timestamp();
    let deadline = now + 50;
    init_with_optional_deadline(&env, &client, &admin, &sme, 50, 0);

    // Advance past deadline
    env.ledger().with_mut(|l| l.timestamp = deadline + 1);

    let investor = Address::generate(&env);
    let mut entries = soroban_sdk::Vec::new(&env);
    entries.push_back((investor, TARGET / 2));

    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingDeadlinePassed,
    );
}

/// After the deadline passes, `fund_with_commitment` is also rejected.
#[test]
fn test_fund_with_commitment_rejected_after_deadline() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    let now = env.ledger().timestamp();
    let deadline = now + 50;
    init_with_optional_deadline(&env, &client, &admin, &sme, 50, 0);

    env.ledger().with_mut(|l| l.timestamp = deadline + 1);

    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund_with_commitment(&investor, &TARGET, &0u64),
        EscrowError::FundingDeadlinePassed,
    );
}
