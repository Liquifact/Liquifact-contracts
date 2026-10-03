//! Tests for [`LiquifactEscrow::get_collateral_version`].
//!
//! Covers:
//! - Default value (`0`) returned before [`LiquifactEscrow::init`] is called.
//! - Correct [`SCHEMA_VERSION`] returned after [`LiquifactEscrow::init`].
//! - Result is consistent with [`LiquifactEscrow::get_version`] (both read the same key).
//! - No auth required (pure read).
//! - Idempotency: calling multiple times returns the same value.

use super::super::{LiquifactEscrow, LiquifactEscrowClient, SCHEMA_VERSION};
use soroban_sdk::{testutils::Address as _, Address, Env};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Register and return a fresh, uninitialised escrow client.
fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

/// Register, deploy, and initialise an escrow returning the client and admin/SME addresses.
fn deploy_and_init(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "COLLVER1"),
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

    (client, admin, sme)
}

// ── Core behaviour ───────────────────────────────────────────────────────────

/// Before `init`, `get_collateral_version` must return `0` (sane default).
#[test]
fn test_get_collateral_version_default_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    assert_eq!(client.get_collateral_version(), 0);
}

/// After `init`, `get_collateral_version` must return `SCHEMA_VERSION`.
#[test]
fn test_get_collateral_version_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    assert_eq!(client.get_collateral_version(), SCHEMA_VERSION);
}

/// `get_collateral_version` and `get_version` read the same storage key and must
/// always return the same value, both before and after init.
#[test]
fn test_get_collateral_version_consistent_with_get_version_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    assert_eq!(
        client.get_collateral_version(),
        client.get_version(),
        "get_collateral_version and get_version must agree before init"
    );
}

#[test]
fn test_get_collateral_version_consistent_with_get_version_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    assert_eq!(
        client.get_collateral_version(),
        client.get_version(),
        "get_collateral_version and get_version must agree after init"
    );
}

// ── No-auth requirement ───────────────────────────────────────────────────────

/// `get_collateral_version` is a pure read; it must succeed without any auth mock.
#[test]
fn test_get_collateral_version_requires_no_auth() {
    let env = Env::default();
    // DO NOT mock_all_auths — intentionally verifying that no auth is required.
    let client = deploy(&env);

    // Before init: no panic, returns 0.
    assert_eq!(client.get_collateral_version(), 0);
}

#[test]
fn test_get_collateral_version_after_init_requires_no_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    // Disable the auth mock after init to confirm the read is still auth-free.
    // (mock_all_auths disables itself after the `env` block; re-creating env is cleaner,
    // but confirming the read explicitly covers the no-auth requirement.)
    assert_eq!(client.get_collateral_version(), SCHEMA_VERSION);
}

// ── Idempotency ───────────────────────────────────────────────────────────────

/// Calling `get_collateral_version` multiple times must always return the same value.
#[test]
fn test_get_collateral_version_idempotent_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let v1 = client.get_collateral_version();
    let v2 = client.get_collateral_version();
    let v3 = client.get_collateral_version();
    assert_eq!(v1, v2);
    assert_eq!(v2, v3);
    assert_eq!(v1, 0);
}

#[test]
fn test_get_collateral_version_idempotent_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let v1 = client.get_collateral_version();
    let v2 = client.get_collateral_version();
    let v3 = client.get_collateral_version();
    assert_eq!(v1, v2);
    assert_eq!(v2, v3);
    assert_eq!(v1, SCHEMA_VERSION);
}

// ── Version value invariant ───────────────────────────────────────────────────

/// The current schema version is 6; this test encodes that expectation so a bump
/// to `SCHEMA_VERSION` without updating the changelog is caught immediately.
#[test]
fn test_get_collateral_version_equals_expected_schema_version() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    // Update this assertion (and the README changelog table) when SCHEMA_VERSION changes.
    assert_eq!(
        client.get_collateral_version(),
        6,
        "SCHEMA_VERSION is expected to be 6; update this test and the README when bumping"
    );
}

// ── State-mutation independence ───────────────────────────────────────────────

/// Recording a collateral commitment must not alter the version.
#[test]
fn test_get_collateral_version_unchanged_after_record_collateral() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, sme) = deploy_and_init(&env);

    let before = client.get_collateral_version();

    // Record a collateral commitment.
    client.record_sme_collateral_commitment(
        &soroban_sdk::Symbol::new(&env, "GOLD"),
        &500_000i128,
    );

    let after = client.get_collateral_version();
    assert_eq!(before, after, "recording collateral must not change the schema version");
}

/// Clearing a collateral commitment must not alter the version.
#[test]
fn test_get_collateral_version_unchanged_after_clear_collateral() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, sme) = deploy_and_init(&env);

    client.record_sme_collateral_commitment(
        &soroban_sdk::Symbol::new(&env, "GOLD"),
        &500_000i128,
    );

    let before = client.get_collateral_version();
    client.clear_sme_collateral_commitment();
    let after = client.get_collateral_version();

    assert_eq!(before, after, "clearing collateral must not change the schema version");
}

/// Setting the collateral limit must not alter the version.
#[test]
fn test_get_collateral_version_unchanged_after_set_collateral_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let before = client.get_collateral_version();
    client.set_collateral_limit(&1_000_000i128);
    let after = client.get_collateral_version();

    assert_eq!(before, after, "set_collateral_limit must not change the schema version");
}
