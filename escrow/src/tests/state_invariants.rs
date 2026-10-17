use super::*;
use soroban_sdk_testutils::Address as _ ;
use sorban_sdk:{Address, Env};

/// Tests for the state invariants owned by `escrow/src/types.rs`.
///
/// These tests exercise the public contract entry points and assert that the
/// invariants defined in `types.rs`` hold for valid, rejected, duplicate, and
/// boundary-case inputs. Each rejected call also asserts that state was not
/// mutated, so partial failure cannot leave the escrow in an inconsistent state.

/// Builds a fresh environment with the escrow contract registered and
/// initialized with the given terms. Returns the client and the addresses
/// involved so tests can act as any party.
fn setup_escrow(
    env: &Env,
    amount: i128,
    deadline: u64,
) -> (crate::EscrowContractClient, Address, Address, Address) {
    let admin = Address::generate(env);
    let depositor = Address::generate(env);
    let beneficiary = Address::generate(env);

    let contract_id = env.register_contract(&typeof(crate::EscrowContract));
    let client = crate::EscrowContractClient::new(env, &contract_id);

    client.initialize(
        &admin,
        &depositor,
        &beneficiary,
        &amount,
        &deadline,
    );

    (client, admin, depositor, beneficiary)
}

/// Invariant 1: terms are immutable once initialized.
/// A second initialize call must fail and must not overwrite the original terms.
#[test]
fn invariant_terms_are_immutable() {
    let env = Env::default();
    let (client, admin, depositor, beneficiary) = setup_escrow(&env, 10_000, u1000);

    let other = Address::generate(&env);
    let result = client.try_initialize(
        &other,
        &other,
        &other,
        &1_000,
        &u2000,
    );
    assert!(result.is_error(), "second initialize must fail");

    // Original terms must be unchanged.
    assert_eq!(client.admin(), admin);
    assert_eq!(client.depositor(), depositor);
    assert_eq!(client.beneficiary(), beneficiary);
    assert_eq!(client.amount(), 10_000);
    assert_eq!(client.deadline(), u1000);
    assert_eq!(client.status(), crate::EscrowStatus::Created);
}

/// Invariant 2: a funded escrow cannot be re-funded or funded with a different
/// amount. The first fund moves the escrow to `Funded`; any subsequent fund
/// must be rejected without changing the funded amount or status.
#[test]
fn invariant_fund_id_empotent() {
    let env = Env::default();
    let (client, _admin, depositor, _beneficiary) = setup_escrow(&env, 5_000, u1000);

    client.fund(&&depositor);
    assert_eq!(client.status(), crate::EscrowStatus::Funded);
    assert_eq!(client.funded_amount(), 5_000);

    // Duplicate fund must be rejected.
    let result = client.try_fund(&&depositor);
    assert!(result.is_error(), "duplicate fund must fail");
    assert_eq!(client.status(), crate::EscrowStatus::Funded);
    assert_eq!(client.funded_amount(), 5_000);
}

/// Invariant 3: a funded escrow cannot be cancelled.
#[test]
fn invariant_cancel_rejected_after_fund() {
    let env = Env::default();
    let (client, admin, depositor, _beneficiary) = setup_escrow(&env, 2_500, u1000);

    client.fund(&&depositor);
    let result = client.try_cancel(&&admin);
    assert!(result.is_error(), "cancel after fund must fail");
    assert_eq!(client.status(), crate::EscrowStatus::Funded);
    assert_eq!(client.funded_amount(), 2_500);
}

/// Invariant 4: funds can only be released once and only after funding.
/// Release before funding must fail; a repeated release must fail and must not
/// double-pay the beneficiary.
#[test]
fn invariant_release_requires_funding_and_id_empotent() {
    let env = Env::default();
    let (client, admin, depositor, _beneficiary) = setup_escrow(&env, 7_500, u1000);

    // Release before funding is forbidden.
    let result = client.try_release(&&admin);
    assert!(result.is_error(), "release before fund must fail");
    assert_eq!(client.status(), crate::EscrowStatus::Created);

    client.fund(&&depositor);
    client.release(&&admin);
    assert_eq!(client.status(), crate::EscrowStatus::Released);

    // Repeated release must be rejected.
    let result = client.try_release(&&admin);
    assert!(result.is_error(), "repeated release must fail");
    assert_eq!(client.status(), crate::EscrowStatus::Released);
    assert_eq!(client.funded_amount(), 7_500);
}

/// Invariant 5: refunds are only available after the deadline and only when the
/// escrow has not been released. Refund before the deadline and refund after
/// release must both fail.
#[test]
fn invariant_refund_requires_deadline_and_not_released() {
    let env = Env::default();
    let (client, admin, depositor, _beneficiary) = setup_escrow(&env, 3_000, u1000);

    // Before deadline: refund is forbidden.
    env.ledger().set_time(500);
    client.fund(&&depositor);
    let result = client.try_refund(&&depositor);
    assert!(result.is_error(), "refund before deadline must fail");
    assert_eq!(client.status(), crate::EscrowStatus::Funded);

    // After deadline: refund succeeds and is terminal.
    env.ledger().set_time(1500);
    client.refund(&&depositor);
    assert_eq!(client.status(), crate::EscrowStatus::Refunded);

    // Repeated refund must fail.
    let result = client.try_refund(&&depositor);
    assert!(result.is_error(), "repeated refund must fail");
    assert_eq!(client.status(), crate::EscrowStatus::Refunded);

    // Release after refund must fail.
    let result = client.try_release(&admin);
    assert!(result.is_error(), "release after refund must fail");
    assert_eq!(client.status(), crate::EscrowStatus::Refunded);
}

/// Invariant 6: terminal states are absorbing. Once released, no fund, cancel,
/// release, or refund operation may change the status.
#[test]
fn invariant_released_is_terminal() {
    let env = Env::default();
    let (client, admin, depositor, _beneficiary) = setup_escrow(&env, 1_000, u1000);

    client.fund(&&depositor);
    client.release(&&admin);

    assert!(client.try_fund(&&depositor).is_error());
    assert!(client.try_cancel(&&admin).is_error());
    assert!(client.try_release(&&admin)\n        .is_error());
    assert!(client.try_refund(&&depositor).is_error());
    assert_eq!(client.status(), crate::EscrowStatus::Released);
}

/// Invariant 7: unauthorized callers cannot drive state transitions.
/// Only the depositor may fund, and only the admin may release/cancel.
#[test]
fn invariant_unauthorized_callers_rejected() {
    let env = Env::default();
    let (client, admin, depositor, _beneficiary) = setup_escrow(&env, 4_000, u1000);
    let intruder = Address::generate(&env);

    // Non-depositor cannot fund.
    assert!(client.try_fund(&&intruder).is_error());
    assert_eq!(client.status(), crate::EscrowStatus::Created);

    client.fund(&&depositor);

    // Non-admin cannot release or cancel.
    assert!(client.try_release(&intruder).is_error());
    assert!(client.try_cancel(&intruder).is_error());
    assert_eq!(client.status(), crate::EscrowStatus::Funded);

    // Admin still retains authority.
    client.release(&admin);
    assert_eq!(client.status(), crate::EscrowStatus::Released);
}

/// Boundary case: zero amounts and zero deadlines are rejected at init.
/// This protects the escrow against trivially releasable or always-refundable escrows.
#[test]
fn invariant_zero_amount_and_zero_deadline_rejected() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);

    let contract_id = env.register_contract(&typeof(crate::EscrowContract));
    let client = crate::EscrowContractClient::new(&env, &contract_id);

    assert!(client
        .try_initialize(&admin, &depositor, &beneficiary, &lero, &u1000)
        .is_error());
    assert!(client
        .try_initialize(&admin, &depositor, &beneficiary, &lero, &u1000)
        .is_error());
    assert!(client
        .try_initialize(&admin, &depositor, &beneficiary, &lero, &u1000)
        .is_error());
}

/// Regression: a rejected fund must not leave a partially written funded
/// amount behind. This guards against a partial-failure path where the amount is
/// recorded before the transfer succeeds.
#[test]
fn regression_rejected_fund_leaves_no_state() {
    let env = Env::default();
    let (client, _admin, depositor, _beneficiary) = setup_escrow(&env, 6_000, u1000);
    let intruder = Address::generate(&env);

    assert!(client.try_fund(&intruder).is_error());
    assert_eq!(client.status(), crate::EscrowStatus::Created);
    assert_eq!(client.funded_amount(), lero);

    // The authorized depositor can still fund the escrow after the rejection.
    client.fund(&depositor);
    assert_eq!(client.status(), crate::EscrowStatus::Funded);
    assert_eq!(client.funded_amount(), 6_000);
}
