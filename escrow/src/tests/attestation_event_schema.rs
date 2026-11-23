//! Event schema tests for attestation lifecycle events:
//! - [`AttestationDigestAppended`] (`att_app`)
//! - [`AttestationDigestRevoked`] (`att_rev`)
//! - [`AttestationDigestUnrevoked`] (`att_unrev`)
//!
//! These tests verify the exact emitted Soroban `#[contractevent]` XDR structure,
//! topic symbols, payload fields (`invoice_id`, `index`, `digest`), and the
//! contract's [`EVENT_SCHEMA_VERSION`] constant for off-chain indexers.

use super::*;
use crate::EVENT_SCHEMA_VERSION;
use soroban_sdk::{
    symbol_short,
    testutils::Events,
    xdr::{ContractEvent, ContractEventBody, ScMapEntry, ScVal},
    BytesN, TryFromVal,
};

fn setup_initialized(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Symbol) {
    let (client, admin, sme) = setup(env);
    default_init(&client, env, &admin, &sme);
    let invoice_id = client.get_escrow().invoice_id;
    (client, admin, invoice_id)
}

fn sample_digest(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn extract_v0(event: &ContractEvent) -> (&[ScVal], &[ScMapEntry]) {
    let ContractEventBody::V0(v0) = &event.body;
    let topics = v0.topics.as_slice();
    let entries = match &v0.data {
        ScVal::Map(Some(map)) => map.as_slice(),
        other => panic!("expected ScVal::Map payload, got {other:?}"),
    };
    (topics, entries)
}

fn map_field<'a>(entries: &'a [ScMapEntry], key: &str) -> &'a ScVal {
    entries
        .iter()
        .find(|entry| match &entry.key {
            ScVal::Symbol(sym) => sym.to_utf8_string_lossy() == key,
            _ => false,
        })
        .map(|entry| &entry.val)
        .unwrap_or_else(|| panic!("missing payload field `{key}`"))
}

fn assert_topic_symbol(topic: &ScVal, expected: &str) {
    match topic {
        ScVal::Symbol(sym) => assert_eq!(sym.to_utf8_string_lossy(), expected),
        other => panic!("expected Symbol topic `{expected}`, got {other:?}"),
    }
}

#[test]
fn event_schema_version_is_v1() {
    assert_eq!(EVENT_SCHEMA_VERSION, 1);
}

#[test]
fn attestation_digest_appended_event_schema() {
    let env = Env::default();
    let (client, _, invoice_id) = setup_initialized(&env);
    let contract_id = client.address.clone();
    let digest = sample_digest(&env, 0xAA);

    client.append_attestation_digest(&digest);

    let all_events = env.events().all();
    let event = all_events.events().last().unwrap().clone();

    assert_eq!(
        event,
        AttestationDigestAppended {
            name: symbol_short!("att_app"),
            invoice_id: invoice_id.clone(),
            index: 0,
            digest: digest.clone(),
        }
        .to_xdr(&env, &contract_id)
    );

    let (topics, entries) = extract_v0(&event);
    assert_eq!(topics.len(), 2);
    assert_topic_symbol(&topics[0], "attestation_digest_appended");
    assert_topic_symbol(&topics[1], "att_app");

    assert_eq!(entries.len(), 3);
    assert_eq!(
        Symbol::try_from_val(&env, map_field(entries, "invoice_id")).unwrap(),
        invoice_id
    );
    assert_eq!(*map_field(entries, "index"), ScVal::U32(0));
    assert_eq!(
        BytesN::<32>::try_from_val(&env, map_field(entries, "digest")).unwrap(),
        digest
    );
}

#[test]
fn attestation_digest_revoked_event_schema() {
    let env = Env::default();
    let (client, _, invoice_id) = setup_initialized(&env);
    let contract_id = client.address.clone();
    let digest = sample_digest(&env, 0xBB);

    client.append_attestation_digest(&digest);
    client.revoke_attestation_digest(&0);

    let all_events = env.events().all();
    let event = all_events.events().last().unwrap().clone();

    assert_eq!(
        event,
        AttestationDigestRevoked {
            name: symbol_short!("att_rev"),
            invoice_id: invoice_id.clone(),
            index: 0,
        }
        .to_xdr(&env, &contract_id)
    );

    let (topics, entries) = extract_v0(&event);
    assert_eq!(topics.len(), 2);
    assert_topic_symbol(&topics[0], "attestation_digest_revoked");
    assert_topic_symbol(&topics[1], "att_rev");

    assert_eq!(entries.len(), 2);
    assert_eq!(
        Symbol::try_from_val(&env, map_field(entries, "invoice_id")).unwrap(),
        invoice_id
    );
    assert_eq!(*map_field(entries, "index"), ScVal::U32(0));
}

#[test]
fn attestation_digest_unrevoked_event_schema() {
    let env = Env::default();
    let (client, _, invoice_id) = setup_initialized(&env);
    let contract_id = client.address.clone();
    let digest = sample_digest(&env, 0xCC);

    client.append_attestation_digest(&digest);
    client.revoke_attestation_digest(&0);
    client.unrevoke_attestation_digest(&0);

    let all_events = env.events().all();
    let event = all_events.events().last().unwrap().clone();

    assert_eq!(
        event,
        AttestationDigestUnrevoked {
            name: symbol_short!("att_unrev"),
            invoice_id: invoice_id.clone(),
            index: 0,
        }
        .to_xdr(&env, &contract_id)
    );

    let (topics, entries) = extract_v0(&event);
    assert_eq!(topics.len(), 2);
    assert_topic_symbol(&topics[0], "attestation_digest_unrevoked");
    assert_topic_symbol(&topics[1], "att_unrev");

    assert_eq!(entries.len(), 2);
    assert_eq!(
        Symbol::try_from_val(&env, map_field(entries, "invoice_id")).unwrap(),
        invoice_id
    );
    assert_eq!(*map_field(entries, "index"), ScVal::U32(0));
}

#[test]
fn attestation_append_revoke_unrevoke_roundtrip_events() {
    let env = Env::default();
    let (client, _, invoice_id) = setup_initialized(&env);
    let contract_id = client.address.clone();
    let digest_0 = sample_digest(&env, 0x01);
    let digest_1 = sample_digest(&env, 0x02);

    assert_eq!(EVENT_SCHEMA_VERSION, 1);

    client.append_attestation_digest(&digest_0);
    client.append_attestation_digest(&digest_1);
    let append_events = env.events().all();
    let append_evt = append_events.events().last().unwrap().clone();
    assert_eq!(
        append_evt,
        AttestationDigestAppended {
            name: symbol_short!("att_app"),
            invoice_id: invoice_id.clone(),
            index: 1,
            digest: digest_1.clone(),
        }
        .to_xdr(&env, &contract_id)
    );

    client.revoke_attestation_digest(&1);
    let revoke_events = env.events().all();
    let revoke_evt = revoke_events.events().last().unwrap().clone();
    assert_eq!(
        revoke_evt,
        AttestationDigestRevoked {
            name: symbol_short!("att_rev"),
            invoice_id: invoice_id.clone(),
            index: 1,
        }
        .to_xdr(&env, &contract_id)
    );

    client.unrevoke_attestation_digest(&1);
    let unrevoke_events = env.events().all();
    let unrevoke_evt = unrevoke_events.events().last().unwrap().clone();
    assert_eq!(
        unrevoke_evt,
        AttestationDigestUnrevoked {
            name: symbol_short!("att_unrev"),
            invoice_id,
            index: 1,
        }
        .to_xdr(&env, &contract_id)
    );
}
