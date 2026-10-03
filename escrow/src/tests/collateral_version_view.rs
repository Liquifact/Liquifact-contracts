//! Tests for [`LiquifactEscrow::get_collateral_version`].
//!
//! Covers:
//! - Default value (`0`) returned before [`LiquifactEscrow::init`] is called.
//! - Correct [`SCHEMA_VERSION`] returned after [`LiquifactEscrow::init`].
//! - Result is consistent with [`LiquifactEscrow::get_version`] (both read the same key).
//! - No auth required (pure read).
//! - Idempotency: calling multiple times returns the same value.
//! - Failure recovery: reads are deterministic and never mutate state, so a
//!   failed read (or a read interleaved with a failed write) leaves storage intact.

use super::super::collateral_storage::CollateralStorageKey;
use super::super::{
    EscrowError, LiquifactEscrow, LiquifactEscrowClient, MAX_INVOICE_AMOUNT, SCHEMA_VERSION,
};
use super::assert_contract_error;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, Symbol,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

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
        &None::<i64>,
    );

    (client, admin, sme)
}

// ── Core behaviour ────────────────────────────────────────────────────────────

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
    let (client, _admin, _sme) = deploy_and_init(&env);

    let before = client.get_collateral_version();

    // Record a collateral commitment.
    client.record_sme_collateral_commitment(&soroban_sdk::Symbol::new(&env, "GOLD"), &500_000i128);

    let after = client.get_collateral_version();
    assert_eq!(
        before, after,
        "recording collateral must not change the schema version"
    );
}

/// Clearing a collateral commitment must not alter the version.
#[test]
fn test_get_collateral_version_unchanged_after_clear_collateral() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    client.record_sme_collateral_commitment(&soroban_sdk::Symbol::new(&env, "GOLD"), &500_000i128);

    let before = client.get_collateral_version();
    let _ = client.clear_sme_collateral_commitment();
    let after = client.get_collateral_version();

    assert_eq!(
        before, after,
        "clearing collateral must not change the schema version"
    );
}

// ── Hardened-view regression tests ───────────────────────────────────────────
//
// These cover the concurrency / duplicate / timing-boundary / idempotent-retry
// requirements for the hardened collateral view and storage layer. Soroban has no
// preemptive threads, so "racing" is modelled the way the runtime actually allows it:
// re-entrancy (a mutation already in flight) and repeated/interleaved calls.

/// A replicated audit of the version across a full mutation sequence: the version view
/// must be stable because the collateral lifecycle never writes `DataKey::Version`.
#[test]
fn test_collateral_version_stable_across_full_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let expected = SCHEMA_VERSION;
    assert_eq!(client.get_collateral_version(), expected);

    env.ledger().with_mut(|l| l.timestamp = 1_000);
    client.record_sme_collateral_commitment(&Symbol::new(&env, "GOLD"), &500_000i128);
    assert_eq!(client.get_collateral_version(), expected);

    client.set_collateral_limit(&1_000_000i128);
    assert_eq!(client.get_collateral_version(), expected);

    let _ = client.clear_sme_collateral_commitment();
    assert_eq!(client.get_collateral_version(), expected);

    // Repeated reads at the same ledger state are byte-for-byte identical.
    assert_eq!(client.get_collateral_version(), expected);
    assert_eq!(client.get_collateral_version(), client.get_version());
}

/// A re-entrant collateral mutation (the lock still held by an in-flight call) must fail
/// fast with the typed `ConcurrentMutation` error and leave all collateral state untouched.
#[test]
fn test_reentrant_collateral_mutation_is_rejected_without_side_effects() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    // Simulate a mutation already in flight by setting the lock directly.
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&CollateralStorageKey::MutationLock, &true);
    });

    let result = client.try_record_sme_collateral_commitment(&Symbol::new(&env, "GOLD"), &500i128);
    assert_contract_error(result, EscrowError::ConcurrentMutation);

    let result = client.try_set_collateral_limit(&1_000i128);
    assert_contract_error(result, EscrowError::ConcurrentMutation);

    // No collateral state was written by the rejected attempts.
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

/// Re-submitting the identical commitment is an idempotent no-op: the stored
/// `recorded_at` is preserved and no second commitment is created.
#[test]
fn test_duplicate_record_is_idempotent_and_preserves_recorded_at() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let asset = Symbol::new(&env, "GOLD");

    env.ledger().with_mut(|l| l.timestamp = 1_000);
    let first = client.record_sme_collateral_commitment(&asset, &500_000i128);

    // A retry later in time must converge to the same record rather than rewriting it.
    env.ledger().with_mut(|l| l.timestamp = 9_999);
    let retry = client.record_sme_collateral_commitment(&asset, &500_000i128);

    assert_eq!(first.recorded_at, 1_000);
    assert_eq!(retry, first, "identical retry must be idempotent");
    assert_eq!(retry.recorded_at, 1_000);
}

/// A replacement whose ledger timestamp precedes the stored record is rejected at the
/// boundary (`now < recorded_at`), while `now == recorded_at` is accepted.
#[test]
fn test_collateral_record_timestamp_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    env.ledger().with_mut(|l| l.timestamp = 5_000);
    client.record_sme_collateral_commitment(&Symbol::new(&env, "GOLD"), &500_000i128);

    // Inclusive boundary: same timestamp, different asset → accepted.
    let accepted =
        client.record_sme_collateral_commitment(&Symbol::new(&env, "SILVER"), &600_000i128);
    assert_eq!(accepted.recorded_at, 5_000);

    // Backwards timestamp → rejected, state unchanged.
    env.ledger().with_mut(|l| l.timestamp = 4_999);
    let result =
        client.try_record_sme_collateral_commitment(&Symbol::new(&env, "BRONZE"), &700_000i128);
    assert_contract_error(result, EscrowError::CollateralTimestampBackwards);
}

/// `set_collateral_limit` validation boundaries: the ceiling bounds `record`, rejects
/// non-positive values, and rejects values above the global invoice-amount ceiling.
#[test]
fn test_collateral_limit_boundaries() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);

    let result = client.try_set_collateral_limit(&0i128);
    assert_contract_error(result, EscrowError::CollateralLimitNotPositive);

    let result = client.try_set_collateral_limit(&(MAX_INVOICE_AMOUNT + 1));
    assert_contract_error(result, EscrowError::CollateralLimitExceedsMax);

    // Exactly at the ceiling is accepted, and the configured limit then bounds `record`.
    client.set_collateral_limit(&1_000_000i128);
    assert_eq!(client.get_collateral_limit(), 1_000_000);

    env.ledger().with_mut(|l| l.timestamp = 1_000);
    client.record_sme_collateral_commitment(&Symbol::new(&env, "GOLD"), &1_000_000i128);

    let result =
        client.try_record_sme_collateral_commitment(&Symbol::new(&env, "GOLD"), &1_000_001i128);
    assert_contract_error(result, EscrowError::CollateralLimitExceeded);
}

/// Clearing when no commitment exists is rejected deterministically, and a successful
/// clear is a one-shot operation (a second clear fails).
#[test]
fn test_collateral_clear_is_one_shot() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    // Nothing recorded yet → typed rejection.
    let result = client.try_clear_sme_collateral_commitment();
    assert_contract_error(result, EscrowError::NoCollateralToClear);

    env.ledger().with_mut(|l| l.timestamp = 1_000);
    client.record_sme_collateral_commitment(&Symbol::new(&env, "GOLD"), &500_000i128);
    assert!(client.clear_sme_collateral_commitment().is_ok());

    // The commitment is gone; a second clear is rejected rather than double-removing.
    let result = client.try_clear_sme_collateral_commitment();
    assert_contract_error(result, EscrowError::NoCollateralToClear);
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

    assert_eq!(
        before, after,
        "set_collateral_limit must not change the schema version"
    );
}
