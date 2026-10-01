// migration_errors.rs – compatibility regression for version / migrate / init bounds.
//
// Covers #1270: deterministic typed errors, empty-state defaults, duplicate
// handling, boundary versions, nonce gating, and storage persistence.

use super::*;

fn init_client(
    env: &Env,
    client: &LiquifactEscrowClient<'_>,
    admin: &Address,
    sme: &Address,
    id: &str,
) {
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, id),
        sme,
        &1_000i128,
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
        &None,
        &None::<i64>,
        &None::<u32>,
    );
}

#[test]
fn test_migration_version_mismatch() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    init_client(&env, &client, &admin, &sme, "MIGSMK1");
    // stored = SCHEMA_VERSION (6), from_version = 5 → mismatch
    assert_contract_error(
        client.try_migrate(&(SCHEMA_VERSION - 1), &0u32),
        EscrowError::MigrationVersionMismatch,
    );

    // Recovery invariant: the rejected migration must not have advanced or
    // otherwise mutated the stored version, so a corrected retry is safe.
    assert_version_unchanged(&env, &client.address, SCHEMA_VERSION);
}

#[test]
fn test_already_current_schema_version() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    init_client(&env, &client, &admin, &sme, "MIGSMK2");
    assert_contract_error(
        client.try_migrate(&SCHEMA_VERSION, &0u32),
        EscrowError::AlreadyCurrentSchemaVersion,
    );

    // Idempotent rejection: retrying the same call yields the same error and
    // leaves the version untouched.
    assert_contract_error(
        client.try_migrate(&SCHEMA_VERSION, &0u32),
        EscrowError::AlreadyCurrentSchemaVersion,
    );
    assert_version_unchanged(&env, &client.address, SCHEMA_VERSION);
}

#[test]
fn test_no_migration_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    init_client(&env, &client, &admin, &sme, "MIGSMK3");
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Version, &1u32);
    });
    assert_contract_error(
        client.try_migrate(&1u32, &0u32),
        EscrowError::NoMigrationPath,
    );
}

#[test]
fn test_get_version_zero_before_init_and_six_after() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    assert_eq!(client.get_version(), 0);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    init_client(&env, &client, &admin, &sme, "VER001");
    assert_eq!(client.get_version(), SCHEMA_VERSION);
    assert_eq!(client.get_version(), 6);
}

#[test]
fn test_get_version_idempotent_and_no_auth() {
    let env = Env::default();
    let client = deploy(&env);
    // No auth mock: pure read must succeed with default.
    assert_eq!(client.get_version(), 0);
    assert_eq!(client.get_version(), 0);
    assert_eq!(client.get_version(), 0);
}

#[test]
fn test_migrate_exhaustive_below_current_returns_92() {
    for from in [1u32, 2, 3, 4, 5] {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = deploy_with_id(&env);
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        init_client(&env, &client, &admin, &sme, "EXH001");
        env.as_contract(&contract_id, || {
            env.storage().instance().set(&DataKey::Version, &from);
        });
        assert_contract_error(
            client.try_migrate(&from, &0u32),
            EscrowError::NoMigrationPath,
        );
        // Persistence: failed migrate must not rewrite Version.
        let stored: u32 = env.as_contract(&contract_id, || {
            env.storage().instance().get(&DataKey::Version).unwrap_or(0)
        });
        assert_eq!(stored, from);
    }
}

#[test]
fn test_migrate_zero_from_version_returns_92() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    init_client(&env, &client, &admin, &sme, "ZERO001");
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Version, &0u32);
    });
    assert_contract_error(
        client.try_migrate(&0u32, &0u32),
        EscrowError::NoMigrationPath,
    );
}

#[test]
fn test_migrate_above_current_returns_91() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    init_client(&env, &client, &admin, &sme, "ABOVE001");
    assert_contract_error(
        client.try_migrate(&(SCHEMA_VERSION + 1), &0u32),
        EscrowError::MigrationVersionMismatch,
    );
    // Exact current also 91 (boundary).
    assert_contract_error(
        client.try_migrate(&SCHEMA_VERSION, &0u32),
        EscrowError::AlreadyCurrentSchemaVersion,
    );
}

#[test]
fn test_migrate_wrong_nonce_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    init_client(&env, &client, &admin, &sme, "NONCE001");
    // Current nonce is 0; future nonce must fail with AdminNonceMismatch.
    assert_contract_error(
        client.try_migrate(&SCHEMA_VERSION, &99u32),
        EscrowError::AdminNonceMismatch,
    );
    // Version unchanged after nonce failure.
    assert_eq!(client.get_version(), SCHEMA_VERSION);
}

#[test]
fn test_duplicate_init_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    init_client(&env, &client, &admin, &sme, "DUP001");
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &soroban_sdk::String::from_str(&env, "DUP001"),
            &sme,
            &1_000i128,
            &500i64,
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
        ),
        EscrowError::EscrowAlreadyInitialized,
    );
    assert_eq!(client.get_version(), SCHEMA_VERSION);
}

#[test]
fn test_empty_invoice_id_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &soroban_sdk::String::from_str(&env, ""),
            &sme,
            &1_000i128,
            &500i64,
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
        ),
        EscrowError::InvoiceIdInvalidLength,
    );
    assert_eq!(client.get_version(), 0);
}

#[test]
fn test_malformed_invoice_id_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &soroban_sdk::String::from_str(&env, "BAD-ID!"),
            &sme,
            &1_000i128,
            &500i64,
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
        ),
        EscrowError::InvoiceIdInvalidCharset,
    );
    assert_eq!(client.get_version(), 0);
}

#[test]
fn test_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &soroban_sdk::String::from_str(&env, "ZEROAMT"),
            &sme,
            &0i128,
            &500i64,
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
        ),
        EscrowError::AmountMustBePositive,
    );
}

#[test]
fn test_get_escrow_before_init_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    assert_contract_error(client.try_get_escrow(), EscrowError::EscrowNotInitialized);
}

#[test]
fn test_settlement_config_defaults_before_init() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let cfg = client.get_settlement_config();
    assert_eq!(cfg.yield_bps, 0);
    assert_eq!(cfg.maturity, 0);
    assert_eq!(cfg.protocol_fee_bps, 0);
}
