// migration_errors.rs – tests for migration error handling

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Env, Address, testutils::Address as _};
    use crate::{EscrowClient, EscrowError, SCHEMA_VERSION};

    // Helper to deploy and init a contract for testing
    fn setup_client(env: &Env) -> EscrowClient {
        let client = deploy(env);
        let admin = Address::generate(env);
        let sme = Address::generate(env);
        client.init(
            &admin,
            &soroban_sdk::String::from_str(env, "MIG_TEST"),
            &sme,
            &1000i128,
            &500i64,
            &0u64,
            &Address::generate(env),
            &None,
            &Address::generate(env),
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
        );
        client
    }

    #[test]
    fn test_migration_version_mismatch() {
        let env = Env::default();
        let client = setup_client(&env);
        // stored version is 6, try migrating from a different version (e.g., 5)
        let err = client.try_migrate(&5u32).err().unwrap();
        assert_eq!(err, EscrowError::MigrationVersionMismatch);
    }

    #[test]
    fn test_already_current_schema_version() {
        let env = Env::default();
        let client = setup_client(&env);
        // from_version == current schema version (6)
        let err = client.try_migrate(&SCHEMA_VERSION).err().unwrap();
        assert_eq!(err, EscrowError::AlreadyCurrentSchemaVersion);
    }

    #[test]
    fn test_no_migration_path() {
        let env = Env::default();
        let client = setup_client(&env);
        // from_version lower than current without a defined migration path (e.g., 1)
        let err = client.try_migrate(&1u32).err().unwrap();
        assert_eq!(err, EscrowError::NoMigrationPath);
    }
}
