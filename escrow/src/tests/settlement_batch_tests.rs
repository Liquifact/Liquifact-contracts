//! Boundary and happy-path coverage for `settle_batch` (issue #1099).

use super::*;

use crate::EscrowError;

#[test]
fn test_settle_batch_empty_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup(&env);
    let escrows: SorobanVec<Address> = SorobanVec::new(&env);
    assert_contract_error(
        client.try_settle_batch(&escrows),
        EscrowError::SettlementBatchEmpty,
    );
}

#[test]
fn test_settle_batch_too_large_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup(&env);
    let mut escrows: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..(crate::MAX_SETTLE_BATCH + 1) {
        escrows.push_back(Address::generate(&env));
    }
    assert_contract_error(
        client.try_settle_batch(&escrows),
        EscrowError::SettlementBatchTooLarge,
    );
}

#[test]
fn test_settle_batch_rejects_when_any_entry_not_settleable() {
    let env = Env::default();
    env.mock_all_auths();

    let (id_a, client_a) = deploy_with_id(&env);
    let admin_a = Address::generate(&env);
    let sme_a = Address::generate(&env);
    default_init(&client_a, &env, &admin_a, &sme_a);
    let investor_a = Address::generate(&env);
    client_a.fund(&investor_a, &TARGET);

    // Second instance is never funded, so it is not settleable.
    let (id_b, client_b) = deploy_with_id(&env);
    let admin_b = Address::generate(&env);
    let sme_b = Address::generate(&env);
    default_init(&client_b, &env, &admin_b, &sme_b);

    let mut escrows: SorobanVec<Address> = SorobanVec::new(&env);
    escrows.push_back(id_a.clone());
    escrows.push_back(id_b.clone());

    assert_contract_error(
        client_a.try_settle_batch(&escrows),
        EscrowError::SettlementNotFunded,
    );

    // Atomicity: the funded entry must be untouched by the rejected batch.
    assert_eq!(client_a.get_escrow().status, 1);
}

#[test]
fn test_settle_batch_settles_all_entries() {
    let env = Env::default();
    env.mock_all_auths();

    let (id_a, client_a) = deploy_with_id(&env);
    let admin_a = Address::generate(&env);
    let sme_a = Address::generate(&env);
    default_init(&client_a, &env, &admin_a, &sme_a);
    let investor_a = Address::generate(&env);
    client_a.fund(&investor_a, &TARGET);

    let (id_b, client_b) = deploy_with_id(&env);
    let admin_b = Address::generate(&env);
    let sme_b = Address::generate(&env);
    default_init(&client_b, &env, &admin_b, &sme_b);
    let investor_b = Address::generate(&env);
    client_b.fund(&investor_b, &TARGET);

    let mut escrows: SorobanVec<Address> = SorobanVec::new(&env);
    escrows.push_back(id_a);
    escrows.push_back(id_b);

    let results = client_a.settle_batch(&escrows);
    assert_eq!(results.len(), 2);
    assert_eq!(client_a.get_escrow().status, 2);
    assert_eq!(client_b.get_escrow().status, 2);
}
