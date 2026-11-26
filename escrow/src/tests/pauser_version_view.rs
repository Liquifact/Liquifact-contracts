use super::{default_init, setup};
use crate::{PauseReason, PauseScope, PauserChanged, SCHEMA_VERSION};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    Address, Env, Event,
};

#[test]
fn get_pauser_is_none_by_default() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    assert_eq!(client.get_pauser(), None);

    default_init(&client, &env, &admin, &sme);
    assert_eq!(client.get_pauser(), None);
}

#[test]
fn set_and_clear_pauser_emits_pauser_changed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let pauser = Address::generate(&env);

    client.set_pauser(&Some(pauser.clone()));
    assert_eq!(
        env.events().all().events().last().unwrap().clone(),
        PauserChanged {
            name: symbol_short!("pauser"),
            invoice_id: client.get_escrow().invoice_id,
            pauser: Some(pauser.clone()),
        }
        .to_xdr(&env, &client.address)
    );
    assert_eq!(client.get_pauser(), Some(pauser.clone()));

    client.set_pauser(&None);
    assert_eq!(
        env.events().all().events().last().unwrap().clone(),
        PauserChanged {
            name: symbol_short!("pauser"),
            invoice_id: client.get_escrow().invoice_id,
            pauser: None,
        }
        .to_xdr(&env, &client.address)
    );
    assert_eq!(client.get_pauser(), None);
}

#[test]
fn get_version_equals_schema_version_after_init() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    default_init(&client, &env, &admin, &sme);
    assert_eq!(client.get_version(), SCHEMA_VERSION);
}

#[test]
fn pause_state_transitions_with_pauser_and_admin_unpause() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let pauser = Address::generate(&env);

    client.set_pauser(&Some(pauser.clone()));
    assert_eq!(client.get_pause_state(), None);

    client.pause_as_pauser(&pauser, &PauseScope::Funding, &PauseReason::Incident);
    let state = client.get_pause_state().unwrap();
    assert_eq!(state.scope, PauseScope::Funding);
    assert_eq!(state.reason, PauseReason::Incident);

    client.set_paused(&false, &PauseScope::Funding, &PauseReason::Incident);
    assert_eq!(client.get_pause_state(), None);
}
