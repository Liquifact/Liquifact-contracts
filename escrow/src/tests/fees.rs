//! Tests for [`LiquifactEscrow::get_fees_version`].
//!
//! Covers:
//! - Default value (`0`) returned before [`LiquifactEscrow::init`] is called.
//! - Correct [`SCHEMA_VERSION`] returned after [`LiquifactEscrow::init`].
//! - Consistency with [`LiquifactEscrow::get_version`].
//! - Read-only semantics: no auth is required and fee updates do not alter the version.

use super::super::{LiquifactEscrow, LiquifactEscrowClient, SCHEMA_VERSION};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn deploy_and_init(env: &Env) -> LiquifactEscrowClient<'_> {
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "FEES001"),
        &sme,
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

    client
}

#[test]
fn test_get_fees_version_default_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    assert_eq!(client.get_fees_version(), 0);
}

#[test]
fn test_get_fees_version_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy_and_init(&env);

    assert_eq!(client.get_fees_version(), SCHEMA_VERSION);
}

#[test]
fn test_get_fees_version_matches_global_version_before_and_after_init() {
    let env = Env::default();
    let client = deploy(&env);
    assert_eq!(client.get_fees_version(), client.get_version());

    env.mock_all_auths();
    let client = deploy_and_init(&env);
    assert_eq!(client.get_fees_version(), client.get_version());
}

#[test]
fn test_get_fees_version_requires_no_auth() {
    let env = Env::default();
    let client = deploy(&env);

    assert_eq!(client.get_fees_version(), 0);
}

#[test]
fn test_get_fees_version_unchanged_after_set_protocol_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy_and_init(&env);

    let before = client.get_fees_version();
    client.set_protocol_fee_bps(&2500i64);
    let after = client.get_fees_version();

    assert_eq!(before, after);
    assert_eq!(after, SCHEMA_VERSION);
}
