use super::*;
use crate::{
    FeeSchedule, FeeScheduleActivated, FeeScheduleCancelled, FeeScheduleSubmitted,
    MAX_FEE_SCHEDULE_ACTIVATION_DELAY_LEDGERS,
};

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Vec as SorobanVec,
};

fn schedule(activation_ledger: u32, fee_bps: u32) -> FeeSchedule {
    FeeSchedule {
        fee_bps,
        min_fee_bps: 0,
        max_fee_bps: 10_000,
        activation_ledger,
    }
}

fn initialized(env: &Env) -> (LiquifactEscrowClient<'_>, Address) {
    let (client, admin, sme) = setup(env);
    default_init(&client, env, &admin, &sme);
    (client, admin)
}

#[test]
fn fee_schedule_uses_unique_escrow_errors() {
    let env = Env::default();
    let (client, _) = initialized(&env);
    let mut invalid = schedule(101, 10_001);
    invalid.max_fee_bps = 10_000;
    assert_contract_error(
        client.try_submit_fee_schedule(&invalid),
        EscrowError::FeeScheduleOutOfBounds,
    );
    assert_ne!(
        EscrowError::FeeScheduleOutOfBounds as u32,
        EscrowError::AmountMustBePositive as u32,
    );
}

#[test]
fn fee_schedule_requires_initialized_escrow() {
    let env = Env::default();
    let client = deploy(&env);
    assert_contract_error(
        client.try_submit_fee_schedule(&schedule(101, 1)),
        EscrowError::EscrowNotInitialized,
    );
}

#[test]
fn fee_schedule_activation_delay_boundaries() {
    let env = Env::default();
    let (client, _) = initialized(&env);
    let current = env.ledger().sequence();

    assert_contract_error(
        client.try_submit_fee_schedule(&schedule(current, 1)),
        EscrowError::FeeScheduleActivationInvalid,
    );
    client.submit_fee_schedule(&schedule(current + 1, 1));
    assert_eq!(
        client.cancel_pending_fee_schedule(),
        schedule(current + 1, 1)
    );

    client.submit_fee_schedule(&schedule(
        current + MAX_FEE_SCHEDULE_ACTIVATION_DELAY_LEDGERS,
        1,
    ));
    assert_eq!(
        client.cancel_pending_fee_schedule(),
        schedule(current + MAX_FEE_SCHEDULE_ACTIVATION_DELAY_LEDGERS, 1)
    );
    assert_contract_error(
        client.try_submit_fee_schedule(&schedule(
            current + MAX_FEE_SCHEDULE_ACTIVATION_DELAY_LEDGERS + 1,
            1,
        )),
        EscrowError::FeeScheduleActivationInvalid,
    );
}

#[test]
fn fee_schedule_pending_and_cancellation_lifecycle() {
    let env = Env::default();
    let (client, _) = initialized(&env);
    let first = schedule(101, 100);
    let second = schedule(102, 200);
    client.submit_fee_schedule(&first);
    env.events().all();
    assert_contract_error(
        client.try_submit_fee_schedule(&second),
        EscrowError::FeeSchedulePendingExists,
    );
    assert!(env.events().all().events().is_empty());
    assert_eq!(client.cancel_pending_fee_schedule(), first);
    assert_eq!(client.get_pending_fee_schedule(), None);
    client.submit_fee_schedule(&second);
    assert_eq!(client.get_pending_fee_schedule(), Some(second));
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

#[test]
fn fee_schedule_cancel_without_pending_fails() {
    let env = Env::default();
    let (client, _) = initialized(&env);
    assert_contract_error(
        client.try_cancel_pending_fee_schedule(),
        EscrowError::FeeScheduleNotPending,
    );
    assert!(env.events().all().events().is_empty());
}

#[test]
#[should_panic]
fn non_admin_cannot_cancel_pending_fee_schedule() {
    let env = Env::default();
    let (client, _) = initialized(&env);
    client.submit_fee_schedule(&schedule(101, 100));
    let stranger = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "cancel_pending_fee_schedule",
            args: SorobanVec::new(&env),
            sub_invokes: &[],
        },
    }]);
    client.cancel_pending_fee_schedule();
}

#[test]
fn fee_schedule_events_and_idempotency() {
    let env = Env::default();
    let (client, _) = initialized(&env);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;
    let first = schedule(101, 100);
    client.submit_fee_schedule(&first);
    assert_eq!(
        env.events().all().events().last().unwrap().clone(),
        FeeScheduleSubmitted {
            name: symbol_short!("fee_sub"),
            invoice_id: invoice_id.clone(),
            fee_bps: 100,
            activation_ledger: 101,
        }
        .to_xdr(&env, &contract_id),
    );

    client.submit_fee_schedule(&first);
    assert!(
        env.events().all().events().is_empty(),
        "identical resubmission must emit no event"
    );

    env.ledger().set_sequence_number(101);
    assert!(client.activate_fee_schedule());
    assert_eq!(
        env.events().all().events().last().unwrap().clone(),
        FeeScheduleActivated {
            name: symbol_short!("fee_act"),
            invoice_id: invoice_id.clone(),
            fee_bps: 100
        }
        .to_xdr(&env, &contract_id),
    );

    let pending = schedule(102, 200);
    client.submit_fee_schedule(&pending);
    assert_eq!(client.cancel_pending_fee_schedule(), pending);
    assert_eq!(
        env.events().all().events().last().unwrap().clone(),
        FeeScheduleCancelled {
            name: symbol_short!("fee_can"),
            invoice_id,
            fee_bps: 200
        }
        .to_xdr(&env, &contract_id),
    );
}

#[test]
fn later_submission_lazily_activates_and_emits_once() {
    let env = Env::default();
    let (client, _) = initialized(&env);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;
    client.submit_fee_schedule(&schedule(101, 100));
    env.events().all();
    env.ledger().set_sequence_number(101);
    client.submit_fee_schedule(&schedule(102, 200));
    let events = env.events().all();
    assert_eq!(events.events().len(), 2);
    assert_eq!(
        events.events()[0].clone(),
        FeeScheduleActivated {
            name: symbol_short!("fee_act"),
            invoice_id: invoice_id.clone(),
            fee_bps: 100
        }
        .to_xdr(&env, &contract_id),
    );
    assert_eq!(
        events.events()[1].clone(),
        FeeScheduleSubmitted {
            name: symbol_short!("fee_sub"),
            invoice_id,
            fee_bps: 200,
            activation_ledger: 102
        }
        .to_xdr(&env, &contract_id),
    );
}
