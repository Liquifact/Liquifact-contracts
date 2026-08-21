//! Tests for the admin-guarded, bounded settlement-limit setter.
//!
//! Covers the issue #1202 edge cases:
//! - in-bounds set by admin is applied and emits an event;
//! - out-of-bounds values are rejected with a typed error;
//! - non-admin callers are rejected;
//! - the read view reflects the change;
//! - the emitted event carries the old and new values.

use crate::{
    EscrowError, LiquifactEscrow, LiquifactEscrowClient, SettlementLimitUpdated,
    DEFAULT_SETTLEMENT_LIMIT, MAX_SETTLEMENT_LIMIT, MIN_SETTLEMENT_LIMIT,
};
use soroban_sdk::{
    symbol_short, testutils::Address as _, Address, Env, Error, Event, InvokeError, String,
    Vec as SorobanVec,
};

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

/// Register a fresh escrow with admin authorization mocked for all addresses.
fn setup(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    (client, admin, sme)
}

fn init_escrow(env: &Env, client: &LiquifactEscrowClient<'_>, admin: &Address, sme: &Address) {
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &String::from_str(env, "SETL001"),
        sme,
        &10_000i128,
        &800i64,
        &0u64,
        &token,
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
}

/// Assert that a `try_*` client call failed with the expected typed contract error.
fn assert_contract_error<T, E>(
    result: Result<Result<T, E>, Result<Error, InvokeError>>,
    expected: EscrowError,
) where
    T: std::fmt::Debug,
    E: std::fmt::Debug,
{
    let expected_code = expected as u32;
    match result {
        Err(Ok(error)) => assert_eq!(error, Error::from_contract_error(expected_code)),
        Err(Err(InvokeError::Contract(code))) => assert_eq!(code, expected_code),
        other => panic!("expected ContractError({expected_code}), got {other:?}"),
    }
}

/// The read view returns the documented default before any admin override.
#[test]
fn test_get_settlement_limit_default_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    assert_eq!(client.get_settlement_limit(), DEFAULT_SETTLEMENT_LIMIT);
}

/// An in-bounds value set by the admin is applied and emits the event with old/new values.
#[test]
fn test_set_settlement_limit_in_bounds_applied_and_emits_event() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let contract_id = client.address.clone();
    init_escrow(&env, &client, &admin, &sme);

    let new_limit = client.set_settlement_limit(&75u32);
    assert_eq!(new_limit, 75u32);

    // Snapshot events immediately: `env.events().all()` only retains the most
    // recent invocation's events.
    let events = env.events().all();
    assert_eq!(
        events.events().last().unwrap().clone(),
        SettlementLimitUpdated {
            name: symbol_short!("settl_lim"),
            invoice_id: client.get_escrow().invoice_id,
            old_limit: DEFAULT_SETTLEMENT_LIMIT,
            new_limit: 75u32,
        }
        .to_xdr(&env, &contract_id)
    );

    assert_eq!(client.get_settlement_limit(), 75u32);
}

/// A second change emits an event whose `old_limit` is the previously stored value.
#[test]
fn test_set_settlement_limit_event_carries_old_and_new() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let contract_id = client.address.clone();
    init_escrow(&env, &client, &admin, &sme);

    client.set_settlement_limit(&75u32);

    let new_limit = client.set_settlement_limit(&40u32);
    assert_eq!(new_limit, 40u32);

    let events = env.events().all();
    assert_eq!(
        events.events().last().unwrap().clone(),
        SettlementLimitUpdated {
            name: symbol_short!("settl_lim"),
            invoice_id: client.get_escrow().invoice_id,
            old_limit: 75u32,
            new_limit: 40u32,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Out-of-bounds values (below min / above max) are rejected with a typed error.
#[test]
fn test_set_settlement_limit_out_of_range_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    assert_contract_error(
        client.try_set_settlement_limit(&(MIN_SETTLEMENT_LIMIT - 1)),
        EscrowError::SettlementLimitOutOfRange,
    );
    assert_contract_error(
        client.try_set_settlement_limit(&(MAX_SETTLEMENT_LIMIT + 1)),
        EscrowError::SettlementLimitOutOfRange,
    );

    // The stored value is unchanged after the rejected calls.
    assert_eq!(client.get_settlement_limit(), DEFAULT_SETTLEMENT_LIMIT);
}

/// A non-admin caller cannot change the limit.
#[test]
#[should_panic]
fn test_set_settlement_limit_non_admin_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    env.mock_auths(&[]);
    client.set_settlement_limit(&50u32);
}

/// The read view reflects the change, including the bundled settlement config view.
#[test]
fn test_read_view_reflects_change() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_settlement_limit(&MIN_SETTLEMENT_LIMIT);
    assert_eq!(client.get_settlement_limit(), MIN_SETTLEMENT_LIMIT);
    assert_eq!(
        client.get_settlement_config().settlement_limit,
        MIN_SETTLEMENT_LIMIT
    );

    client.set_settlement_limit(&MAX_SETTLEMENT_LIMIT);
    assert_eq!(client.get_settlement_limit(), MAX_SETTLEMENT_LIMIT);
    assert_eq!(
        client.get_settlement_config().settlement_limit,
        MAX_SETTLEMENT_LIMIT
    );
}

/// `settle_batch` enforces the configured (not hard-coded) settlement limit.
#[test]
fn test_settle_batch_respects_configured_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_settlement_limit(&2u32);

    let batch = SorobanVec::from_array(
        &env,
        [
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ],
    );

    assert_contract_error(
        client.try_settle_batch(&batch),
        EscrowError::SettlementBatchTooLarge,
    );
}
