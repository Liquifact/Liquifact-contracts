//! Tests for [`LiquifactEscrow::get_attestation_config`].
//!
//! Covers:
//! - Default values before [`LiquifactEscrow::init`] is called.
//! - Values after init (before any attestation operations).
//! - After [`LiquifactEscrow::bind_primary_attestation_hash`] (`primary_bound` becomes `true`).
//! - After [`LiquifactEscrow::append_attestation_digest`] (`append_log_length` updates).
//! - Config matches the individual getters/state.
//! - Idempotency (pure read, no state mutation).
//! - Struct shape stability (destructuring).

use super::super::{
    AttestationConfig, LiquifactEscrow, LiquifactEscrowClient, MAX_ATTESTATION_APPEND_BATCH,
    MAX_ATTESTATION_APPEND_ENTRIES, MAX_ATTESTATION_READ_PAGE, MAX_ATTESTATION_REVOKE_BATCH,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};

// ── helpers ──────────────────────────────────────────────────────────────────

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn init_escrow(env: &Env, client: &LiquifactEscrowClient) -> Address {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "ATTCFG01"),
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
        &None::<u32>,
    );
    admin
}

// ── tests ────────────────────────────────────────────────────────────────────

/// Before `init`, every field must return its documented default.
#[test]
fn test_defaults_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let config = client.get_attestation_config();

    assert_eq!(
        config.max_append_entries, MAX_ATTESTATION_APPEND_ENTRIES,
        "max_append_entries should be MAX_ATTESTATION_APPEND_ENTRIES before init"
    );
    assert_eq!(
        config.max_revoke_batch, MAX_ATTESTATION_REVOKE_BATCH,
        "max_revoke_batch should be MAX_ATTESTATION_REVOKE_BATCH before init"
    );
    assert_eq!(
        config.max_append_batch, MAX_ATTESTATION_APPEND_BATCH,
        "max_append_batch should be MAX_ATTESTATION_APPEND_BATCH before init"
    );
    assert_eq!(
        config.max_read_page, MAX_ATTESTATION_READ_PAGE,
        "max_read_page should be MAX_ATTESTATION_READ_PAGE before init"
    );
    assert!(
        !config.primary_bound,
        "primary_bound should be false before init"
    );
    assert_eq!(
        config.append_log_length, 0,
        "append_log_length should be 0 before init"
    );
}

/// After `init` (but before any attestation operations), the config should
/// still reflect defaults for the live-state fields.
#[test]
fn test_values_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    init_escrow(&env, &client);

    let config = client.get_attestation_config();

    assert_eq!(config.max_append_entries, MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(config.max_revoke_batch, MAX_ATTESTATION_REVOKE_BATCH);
    assert_eq!(config.max_append_batch, MAX_ATTESTATION_APPEND_BATCH);
    assert_eq!(config.max_read_page, MAX_ATTESTATION_READ_PAGE);
    assert!(!config.primary_bound, "primary_bound should be false after init when no hash bound");
    assert_eq!(config.append_log_length, 0, "append_log_length should be 0 after init when no digests appended");
}

/// After binding a primary attestation hash, `primary_bound` must be `true`.
#[test]
fn test_primary_bound_true_after_bind() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0xabu8; 32]);
    client.bind_primary_attestation_hash(&hash);

    let config = client.get_attestation_config();
    assert!(config.primary_bound, "primary_bound should be true after binding");
}

/// After appending digests, `append_log_length` must reflect the append count.
#[test]
fn test_append_log_length_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash1 = BytesN::from_array(&env, &[1u8; 32]);
    let hash2 = BytesN::from_array(&env, &[2u8; 32]);
    let hash3 = BytesN::from_array(&env, &[3u8; 32]);

    // Initial: empty log.
    assert_eq!(client.get_attestation_config().append_log_length, 0);

    // Append one.
    client.append_attestation_digest(&hash1);
    assert_eq!(client.get_attestation_config().append_log_length, 1);

    // Append two more.
    client.append_attestation_digest(&hash2);
    client.append_attestation_digest(&hash3);
    assert_eq!(client.get_attestation_config().append_log_length, 3);
}

/// After appending and revoking, `append_log_length` must not change (revocation
/// does not remove entries from the log).
#[test]
fn test_append_log_length_unaffected_by_revoke() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0x42u8; 32]);
    client.append_attestation_digest(&hash);
    assert_eq!(client.get_attestation_config().append_log_length, 1);

    // Revoke index 0 — log length stays the same.
    client.revoke_attestation_digest(&0);
    assert_eq!(
        client.get_attestation_config().append_log_length,
        1,
        "revoke must not reduce append_log_length"
    );
}

/// `get_attestation_config` must match the individual authoritative sources.
#[test]
fn test_config_matches_individual_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0x99u8; 32]);
    client.bind_primary_attestation_hash(&hash);
    client.append_attestation_digest(&hash);

    let config = client.get_attestation_config();

    // Constants are compile-time — just verify they're wired through.
    assert_eq!(config.max_append_entries, MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(config.max_revoke_batch, MAX_ATTESTATION_REVOKE_BATCH);
    assert_eq!(config.max_append_batch, MAX_ATTESTATION_APPEND_BATCH);
    assert_eq!(config.max_read_page, MAX_ATTESTATION_READ_PAGE);

    // Live state matches individual getters.
    assert_eq!(
        config.primary_bound,
        client.get_primary_attestation_hash().is_some()
    );
    assert_eq!(
        config.append_log_length as u32,
        client.get_attestation_append_log().len()
    );
}

/// Config is idempotent (pure read, no state mutation).
#[test]
fn test_config_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0x55u8; 32]);
    client.bind_primary_attestation_hash(&hash);

    let first = client.get_attestation_config();
    let second = client.get_attestation_config();

    assert_eq!(first, second);
}

/// Defaults before init are also idempotent.
#[test]
fn test_defaults_idempotent_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let first = client.get_attestation_config();
    let second = client.get_attestation_config();

    assert_eq!(first, second);
}

/// `get_attestation_config` has the expected shape (all six fields present) —
/// verified via field-by-field destructuring so a future struct change causes a
/// compile error.
#[test]
fn test_config_struct_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0x11u8; 32]);
    client.bind_primary_attestation_hash(&hash);
    client.append_attestation_digest(&hash);

    let AttestationConfig {
        max_append_entries,
        max_revoke_batch,
        max_append_batch,
        max_read_page,
        primary_bound,
        append_log_length,
    } = client.get_attestation_config();

    assert_eq!(max_append_entries, MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(max_revoke_batch, MAX_ATTESTATION_REVOKE_BATCH);
    assert_eq!(max_append_batch, MAX_ATTESTATION_APPEND_BATCH);
    assert_eq!(max_read_page, MAX_ATTESTATION_READ_PAGE);
    assert!(primary_bound);
    assert_eq!(append_log_length, 1);
}
