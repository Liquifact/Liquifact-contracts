// migration_errors.rs – standalone smoke tests for migrate() typed-error branches.
//
// These tests are intentionally minimal: they deploy a fresh contract, init it,
// and verify that each documented error branch is reachable. Comprehensive
// coverage (including DataKey::Version immutability, historical-version sweeps,
// and auth-first ordering) lives in the anchoring suite in tests/admin.rs.

use super::*;

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
    );

    // stored = SCHEMA_VERSION (6), from_version = 5 → mismatch
    assert_contract_error(
        client.try_migrate(&(SCHEMA_VERSION - 1)),
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
    );

    assert_contract_error(
        client.try_migrate(&SCHEMA_VERSION),
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
    );

    // Set stored version to 1 so from_version=1 matches
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Version, &1u32);
    });

    assert_contract_error(client.try_migrate(&1u32), EscrowError::NoMigrationPath);
}
