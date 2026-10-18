//! Regression coverage for atomic and idempotent collateral updates.

use crate::tests::{assert_contract_error, setup};
use crate::{EscrowError, LiquifactEscrowClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol, Vec};

fn init_escrow(env: &Env, client: &LiquifactEscrowClient<'_>, admin: &Address, sme: &Address) {
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "BOUNDARY"),
        sme,
        &10_000i128,
        &800i64,
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
        &None,
        &None::<i64>,
        &None::<u32>,
    );
}

#[test]
fn failed_batch_does_not_publish_partial_collateral_state() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let original = client.record_sme_collateral_commitment(&Symbol::new(&env, "USDC"), &500);
    let mut items = Vec::new(&env);
    items.push_back((Symbol::new(&env, "ETH"), 700));
    items.push_back((Symbol::new(&env, "BTC"), 0));

    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralAmountNotPositive,
    );
    assert_eq!(client.get_sme_collateral_commitment(), Some(original));
}

#[test]
fn equal_timestamp_replacement_is_deterministic() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let first = client.record_sme_collateral_commitment(&Symbol::new(&env, "USDC"), &500);
    let second = client.record_sme_collateral_commitment(&Symbol::new(&env, "ETH"), &750);

    assert_eq!(first.recorded_at, second.recorded_at);
    assert_eq!(client.get_sme_collateral_commitment(), Some(second));
}

#[test]
fn backward_timestamp_rejection_preserves_last_commitment() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    env.ledger().set_timestamp(500);
    let original = client.record_sme_collateral_commitment(&Symbol::new(&env, "USDC"), &500);
    env.ledger().set_timestamp(499);

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&Symbol::new(&env, "ETH"), &750),
        EscrowError::CollateralTimestampBackwards,
    );
    assert_eq!(client.get_sme_collateral_commitment(), Some(original));
}
