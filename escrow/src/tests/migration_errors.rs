// migration_errors.rs – standalone smoke tests for migrate() typed-error branches.
//
// These tests are intentionally minimal: they deploy a fresh contract, init it,
// and verify that each documented error branch is reachable. Comprehensive
// coverage (including DataKey::Version immutability, historical-version sweeps,
// and auth-first ordering) lives in the anchoring suite in tests/admin.rs.

use super::*;
use crate::LEGACY_VERSION;

/// Calling migrate(stored_version - 1) with the correct stored version
/// must raise MigrationVersionMismatch (stored != from_version).
#[test]
fn test_migration_version_mismatch() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MIGSMK1"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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

    // stored = SCHEMA_VERSION (6), from_version = 5 → mismatch
    assert_contract_error(
        client.try_migrate(&(SCHEMA_VERSION - 1), &0u32),
        EscrowError::MigrationVersionMismatch,
    );
}

/// Calling migrate(SCHEMA_VERSION) with stored=SCHEMA_VERSION must raise
/// AlreadyCurrentSchemaVersion (from_version >= SCHEMA_VERSION after mismatch passes).
#[test]
fn test_already_current_schema_version() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MIGSMK2"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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

    assert_contract_error(
        client.try_migrate(&SCHEMA_VERSION, &0u32),
        EscrowError::AlreadyCurrentSchemaVersion,
    );
}

/// Calling migrate(1) when stored version is manually set to 1 must raise
/// NoMigrationPath (from_version < SCHEMA_VERSION, no migration branch).
#[test]
fn test_no_migration_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MIGSMK3"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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

    // Set stored version to 1 so from_version=1 matches
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Version, &1u32);
    });

    assert_contract_error(
        client.try_migrate(&1u32, &0u32),
        EscrowError::NoMigrationPath,
    );
}

#[test]
fn test_known_legacy_layout_migration() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MIGSMK4"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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

    // Simulate known legacy layout by removing the version marker
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Version);
    });

    let new_version = client.migrate(&LEGACY_VERSION, &0u32);
    assert_eq!(new_version, SCHEMA_VERSION);
    assert_eq!(client.get_version(), SCHEMA_VERSION);
}

#[test]
fn test_ambiguous_legacy_storage_missing_marker() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MIGSMK5"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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

    // Make it ambiguous by removing both Version and Treasury
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Version);
        env.storage().instance().remove(&DataKey::Treasury);
    });

    assert_contract_error(
        client.try_migrate(&LEGACY_VERSION, &0u32),
        EscrowError::AmbiguousLegacyStorage,
    );
}

#[test]
fn test_unknown_marker() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MIGSMK6"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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

    // Set unknown marker (e.g. 999)
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Version, &999u32);
    });

    // Caller guesses LEGACY_VERSION
    assert_contract_error(
        client.try_migrate(&LEGACY_VERSION, &0u32),
        EscrowError::MigrationVersionMismatch,
    );
}

#[test]
fn test_migration_repeated() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MIGSMK7"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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

    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Version);
    });

    client.migrate(&LEGACY_VERSION, &0u32);

    // Repeated migration with from_version = LEGACY_VERSION should fail
    assert_contract_error(
        client.try_migrate(&LEGACY_VERSION, &1u32),
        EscrowError::MigrationVersionMismatch,
    );
    // Repeated migration with from_version = SCHEMA_VERSION should fail with AlreadyCurrentSchemaVersion
    assert_contract_error(
        client.try_migrate(&SCHEMA_VERSION, &2u32),
        EscrowError::AlreadyCurrentSchemaVersion,
    );
}

#[test]
fn test_partial_migration() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MIGSMK8"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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

    // Partial migration: missing version marker, but FundingToken also missing
    // We already tested missing Treasury, let us test missing FundingToken
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Version);
        env.storage().instance().remove(&DataKey::FundingToken);
    });

    assert_contract_error(
        client.try_migrate(&LEGACY_VERSION, &0u32),
        EscrowError::AmbiguousLegacyStorage,
    );
}
