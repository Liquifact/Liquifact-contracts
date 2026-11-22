//! Tests for SME collateral commitment state views and transitions.
//!
//! Reflects the metadata-only state machine documented in `docs/collateral-states.md`:
//! - `None` before any record (`NoPledge`)
//! - `Some` with recorded fields after `record_sme_collateral_commitment` (`Pledged`)
//! - Replacement updates the stored commitment and sets `prior_amount` in `CollateralRecordedEvt`
//! - `None` after `clear_sme_collateral_commitment` (`NoPledge`)
//! - Clearing with nothing recorded fails with `EscrowError::NoCollateralToClear`

use super::{assert_contract_error, default_init, deploy_with_id};
use crate::{
    CollateralClearedEvt, CollateralRecordedEvt, EscrowError, LiquifactEscrowClient,
    SmeCollateralCommitment,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, Event,
};

fn init_open_escrow(env: &Env) -> (Address, LiquifactEscrowClient<'_>) {
    env.mock_all_auths();
    let mut ledger = env.ledger().get();
    ledger.timestamp = 1_000;
    ledger.sequence_number = 100;
    env.ledger().set(ledger);

    let (contract_id, client) = deploy_with_id(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    default_init(&client, env, &admin, &sme);
    (contract_id, client)
}

#[test]
fn test_collateral_commitment_is_none_before_any_record() {
    let env = Env::default();
    let (_contract_id, client) = init_open_escrow(&env);

    assert_eq!(client.get_sme_collateral_commitment(), None);
}

#[test]
fn test_record_collateral_commitment_sets_some_and_emits_event() {
    let env = Env::default();
    let (contract_id, client) = init_open_escrow(&env);
    let invoice_id = client.get_escrow().invoice_id;

    let asset = symbol_short!("USDC");
    let amount = 5_000i128;

    let returned = client.record_sme_collateral_commitment(&asset, &amount);
    let events = env.events().all();

    let expected = SmeCollateralCommitment {
        asset: asset.clone(),
        amount,
        recorded_at: 1_000,
    };

    assert_eq!(returned, expected);
    assert_eq!(client.get_sme_collateral_commitment(), Some(expected));
    assert_eq!(
        events,
        std::vec![CollateralRecordedEvt {
            name: symbol_short!("coll_rec"),
            invoice_id,
            amount,
            prior_amount: 0,
        }
        .to_xdr(&env, &contract_id)]
    );
}

#[test]
fn test_replacement_updates_commitment_and_prior_amount_in_event() {
    let env = Env::default();
    let (contract_id, client) = init_open_escrow(&env);
    let invoice_id = client.get_escrow().invoice_id;

    let first_asset = symbol_short!("USDC");
    let first_amount = 5_000i128;
    client.record_sme_collateral_commitment(&first_asset, &first_amount);

    let mut ledger = env.ledger().get();
    ledger.timestamp = 2_500;
    env.ledger().set(ledger);

    let second_asset = symbol_short!("XLM");
    let second_amount = 12_500i128;
    let returned = client.record_sme_collateral_commitment(&second_asset, &second_amount);
    let events = env.events().all();

    let expected = SmeCollateralCommitment {
        asset: second_asset.clone(),
        amount: second_amount,
        recorded_at: 2_500,
    };

    assert_eq!(returned, expected);
    assert_eq!(client.get_sme_collateral_commitment(), Some(expected));
    assert_eq!(
        events,
        std::vec![CollateralRecordedEvt {
            name: symbol_short!("coll_rec"),
            invoice_id,
            amount: second_amount,
            prior_amount: first_amount,
        }
        .to_xdr(&env, &contract_id)]
    );
}

#[test]
fn test_clear_collateral_commitment_returns_to_none() {
    let env = Env::default();
    let (contract_id, client) = init_open_escrow(&env);
    let invoice_id = client.get_escrow().invoice_id;

    let asset = symbol_short!("USDC");
    let amount = 7_500i128;
    client.record_sme_collateral_commitment(&asset, &amount);
    assert!(client.get_sme_collateral_commitment().is_some());

    client.clear_sme_collateral_commitment();
    let events = env.events().all();

    assert_eq!(client.get_sme_collateral_commitment(), None);
    assert_eq!(
        events,
        std::vec![CollateralClearedEvt {
            name: symbol_short!("coll_clr"),
            invoice_id,
            asset,
            amount,
            recorded_at: 1_000,
        }
        .to_xdr(&env, &contract_id)]
    );

    // Clearing again when already cleared must fail with NoCollateralToClear.
    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );
}

#[test]
fn test_clear_without_recorded_collateral_fails_with_no_collateral_to_clear() {
    let env = Env::default();
    let (_contract_id, client) = init_open_escrow(&env);

    assert_eq!(client.get_sme_collateral_commitment(), None);
    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

#[test]
fn test_full_collateral_state_lifecycle_and_status_orthogonality() {
    let env = Env::default();
    let (contract_id, client) = init_open_escrow(&env);
    let escrow = client.get_escrow();
    let initial_status = escrow.status;
    let invoice_id = escrow.invoice_id;

    // 1. None before any record.
    assert_eq!(client.get_sme_collateral_commitment(), None);

    // 2. Some after initial record.
    client.record_sme_collateral_commitment(&symbol_short!("USDC"), &4_000i128);
    assert_eq!(
        client.get_sme_collateral_commitment(),
        Some(SmeCollateralCommitment {
            asset: symbol_short!("USDC"),
            amount: 4_000,
            recorded_at: 1_000,
        })
    );
    assert_eq!(client.get_escrow().status, initial_status);

    // 3. Replacement updates stored commitment and emits prior_amount.
    let mut ledger = env.ledger().get();
    ledger.timestamp = 2_000;
    env.ledger().set(ledger);

    client.record_sme_collateral_commitment(&symbol_short!("USDC"), &9_000i128);
    let replace_events = env.events().all();

    assert_eq!(
        client.get_sme_collateral_commitment(),
        Some(SmeCollateralCommitment {
            asset: symbol_short!("USDC"),
            amount: 9_000,
            recorded_at: 2_000,
        })
    );
    assert_eq!(
        replace_events,
        std::vec![CollateralRecordedEvt {
            name: symbol_short!("coll_rec"),
            invoice_id,
            amount: 9_000,
            prior_amount: 4_000,
        }
        .to_xdr(&env, &contract_id)]
    );
    assert_eq!(client.get_escrow().status, initial_status);

    // 4. None after clear.
    client.clear_sme_collateral_commitment();
    assert_eq!(client.get_sme_collateral_commitment(), None);
    assert_eq!(client.get_escrow().status, initial_status);
}
