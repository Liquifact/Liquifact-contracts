//! Boundary tests for protocol fee invariants.
//!
//! These tests validate the `0..=10_000` basis point envelope for
//! `set_protocol_fee_bps` / `get_protocol_fee_bps`, asserting the typed
//! [`EscrowError::ProtocolFeeBpsOutOfRange`] for inputs at min, max,
//! zero, and over-limit positions, and that the stored value is unchanged
//! after a rejected call.

use super::{assert_contract_error, deploy, LiquifactEscrowClient, TARGET};
use crate::EscrowError;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup_fee_env<'a>(env: &'a Env) -> (LiquifactEscrowClient<'a>, Address) {
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &String::from_str(env, "FEEBOUND"),
        &sme,
        &TARGET,
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
        &None::<bool>,
        &None::<i64>,
    );

    (client, admin)
}

#[test]
fn test_protocol_fee_bps_accepts_zero() {
    let env = Env::default();
    let (client, _admin) = setup_fee_env(&env);

    // 0 bps = no fee (valid boundary)
    let updated = client.set_protocol_fee_bps(&0i64);
    assert_eq!(updated, 0i64);
    assert_eq!(client.get_protocol_fee_bps(), 0i64);
}

#[test]
fn test_protocol_fee_bps_accepts_max() {
    let env = Env::default();
    let (client, _admin) = setup_fee_env(&env);

    // 10_000 bps = 100% fee (valid upper boundary)
    let updated = client.set_protocol_fee_bps(&10_000i64);
    assert_eq!(updated, 10_000i64);
    assert_eq!(client.get_protocol_fee_bps(), 10_000i64);
}

#[test]
fn test_protocol_fee_bps_rejects_above_max() {
    let env = Env::default();
    let (client, _admin) = setup_fee_env(&env);

    // Set a valid value first
    client.set_protocol_fee_bps(&5000i64);
    assert_eq!(client.get_protocol_fee_bps(), 5000i64);

    // 10_001 is out of range
    assert_contract_error(
        client.try_set_protocol_fee_bps(&10_001i64),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );

    // State must remain unchanged after rejection
    assert_eq!(client.get_protocol_fee_bps(), 5000i64);
}

#[test]
fn test_protocol_fee_bps_rejects_negative() {
    let env = Env::default();
    let (client, _admin) = setup_fee_env(&env);

    // Set a valid value first
    client.set_protocol_fee_bps(&5000i64);
    assert_eq!(client.get_protocol_fee_bps(), 5000i64);

    // -1 is out of range
    assert_contract_error(
        client.try_set_protocol_fee_bps(&-1i64),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );

    // State must remain unchanged after rejection
    assert_eq!(client.get_protocol_fee_bps(), 5000i64);
}

#[test]
fn test_protocol_fee_bps_rejects_extreme_negative() {
    let env = Env::default();
    let (client, _admin) = setup_fee_env(&env);

    // Set a valid value first
    client.set_protocol_fee_bps(&100i64);
    assert_eq!(client.get_protocol_fee_bps(), 100i64);

    // i64::MIN is far out of range (fuzz-style extreme)
    assert_contract_error(
        client.try_set_protocol_fee_bps(&i64::MIN),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );

    // State must remain unchanged after rejection
    assert_eq!(client.get_protocol_fee_bps(), 100i64);
}

#[test]
fn test_protocol_fee_bps_rejects_extreme_positive() {
    let env = Env::default();
    let (client, _admin) = setup_fee_env(&env);

    // Set a valid value first
    client.set_protocol_fee_bps(&100i64);
    assert_eq!(client.get_protocol_fee_bps(), 100i64);

    // i64::MAX is far out of range (fuzz-style extreme)
    assert_contract_error(
        client.try_set_protocol_fee_bps(&i64::MAX),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );

    // State must remain unchanged after rejection
    assert_eq!(client.get_protocol_fee_bps(), 100i64);
}

#[test]
#[should_panic]
fn test_protocol_fee_bps_requires_admin_auth() {
    let env = Env::default();
    let (client, _admin) = setup_fee_env(&env);

    // Set a valid value first
    client.set_protocol_fee_bps(&2500i64);
    assert_eq!(client.get_protocol_fee_bps(), 2500i64);

    // Remove admin auth
    env.mock_auths(&[]);

    // Non-admin call must fail (will panic due to auth requirement)
    client.set_protocol_fee_bps(&3000i64);
}

#[test]
fn test_protocol_fee_bps_state_unchanged_on_rejection_from_zero() {
    let env = Env::default();
    let (client, _admin) = setup_fee_env(&env);

    // Default is 0 (no fee set at init)
    assert_eq!(client.get_protocol_fee_bps(), 0i64);

    // Attempt to set invalid value
    assert_contract_error(
        client.try_set_protocol_fee_bps(&-1i64),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );

    // State must remain at default (0)
    assert_eq!(client.get_protocol_fee_bps(), 0i64);
}

#[test]
fn test_protocol_fee_bps_state_unchanged_on_multiple_rejections() {
    let env = Env::default();
    let (client, _admin) = setup_fee_env(&env);

    // Set a valid value
    client.set_protocol_fee_bps(&7500i64);
    assert_eq!(client.get_protocol_fee_bps(), 7500i64);

    // Multiple rejections in sequence
    assert_contract_error(
        client.try_set_protocol_fee_bps(&-100i64),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );
    assert_contract_error(
        client.try_set_protocol_fee_bps(&10_001i64),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );
    assert_contract_error(
        client.try_set_protocol_fee_bps(&i64::MAX),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );

    // State must remain unchanged after all rejections
    assert_eq!(client.get_protocol_fee_bps(), 7500i64);
}
