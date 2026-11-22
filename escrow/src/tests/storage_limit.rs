use super::*;
use crate::{
    EscrowError, INSTANCE_TTL_MIN_EXTENSION_LEDGERS, MAX_STORAGE_LIMIT_LEDGERS,
    MIN_STORAGE_LIMIT_LEDGERS,
};

#[test]
fn storage_limit_defaults_and_accepts_boundaries() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_eq!(
        client.get_storage_limit(),
        INSTANCE_TTL_MIN_EXTENSION_LEDGERS
    );

    assert_eq!(
        client.set_storage_limit(&MIN_STORAGE_LIMIT_LEDGERS),
        MIN_STORAGE_LIMIT_LEDGERS
    );
    assert_eq!(client.get_storage_limit(), MIN_STORAGE_LIMIT_LEDGERS);

    assert_eq!(
        client.set_storage_limit(&MAX_STORAGE_LIMIT_LEDGERS),
        MAX_STORAGE_LIMIT_LEDGERS
    );
    assert_eq!(client.get_storage_limit(), MAX_STORAGE_LIMIT_LEDGERS);
}

#[test]
fn storage_limit_rejects_out_of_range_values() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_contract_error(
        client.try_set_storage_limit(&0),
        EscrowError::StorageLimitOutOfRange,
    );
    assert_contract_error(
        client.try_set_storage_limit(&(MAX_STORAGE_LIMIT_LEDGERS + 1)),
        EscrowError::StorageLimitOutOfRange,
    );
}

#[test]
fn storage_limit_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin, _sme) = setup(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    env.mock_auths(&[]);
    assert!(client
        .try_set_storage_limit(&MIN_STORAGE_LIMIT_LEDGERS)
        .is_err());
}
