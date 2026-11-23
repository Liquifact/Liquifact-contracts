//! Tests for funding event topic symbols and payload fields.
//!
//! Covers:
//! - `EscrowFunded` event name and routing topic via direct XDR topic extraction.
//! - `EscrowFunded` payload fields from plain [`LiquifactEscrow::fund`] (previously only tested via
//!   [`LiquifactEscrow::fund_with_commitment`]).
//! - `EscrowFunded` and `FundingStateChanged` when a deposit triggers the 0 → 1 transition.
//! - `FundingCancelled` event topic and payload (previously untested).
//! - `fund_batch` per-entry `EscrowFunded` content with correct per-entry `investor` / `amount`.
//! - No routing-topic collision across `EscrowFunded`, `FundingStateChanged`,
//!   `FundingCancelled`, and `EscrowUnfunded`.

use super::*;
use crate::FundingStateChanged;
use soroban_sdk::{
    testutils::Events as _,
    xdr::{ContractEvent, ContractEventBody, ScVal},
    Symbol, TryFromVal,
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn init_for_funding(
    env: &Env,
    client: &LiquifactEscrowClient,
    target: i128,
    invoice_str: &str,
) -> Address {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let (tok, tre) = free_addresses(env);
    client.init(
        &admin,
        &String::from_str(env, invoice_str),
        &sme,
        &target,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    admin
}

/// Snapshot only the escrow's events from the most recent successful invocation.
fn emitted_events(env: &Env, contract_id: &Address) -> std::vec::Vec<ContractEvent> {
    env.events()
        .all()
        .filter_by_contract(contract_id)
        .events()
        .to_vec()
}

/// Extract the topic array from a SDK 25 `ContractEvent` XDR body.
fn event_topics(event: &ContractEvent) -> &[ScVal] {
    match &event.body {
        ContractEventBody::V0(v0) => v0.topics.as_slice(),
        other => panic!("expected a V0 contract event, got {other:?}"),
    }
}

/// Topic 0 is the fixed snake-case event name; topic 1 is its short routing symbol.
fn routing_symbol(env: &Env, event: &ContractEvent) -> Symbol {
    let topics = event_topics(event);
    assert!(topics.len() >= 2, "event must include a routing topic");
    Symbol::try_from_val(env, &topics[1]).expect("routing topic must be convertible to Symbol")
}

fn record_unique_routing_topics(
    env: &Env,
    contract_id: &Address,
    topics_seen: &mut std::vec::Vec<Symbol>,
) {
    for event in emitted_events(env, contract_id) {
        let symbol = routing_symbol(env, &event);
        assert!(
            !topics_seen.iter().any(|seen| *seen == symbol),
            "duplicate routing symbol across different event types: {symbol}"
        );
        topics_seen.push(symbol);
    }
}

// ── EscrowFunded topic and payload tests ─────────────────────────────────────

/// Verify the fixed event name and short routing symbol directly from the
/// SDK 25 XDR topic array, independently of the `to_xdr()` struct comparison.
#[test]
fn test_escrow_funded_topic_via_direct_extraction() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let _admin = init_for_funding(&env, &client, 100_000, "TOPIC01");
    let investor = Address::generate(&env);

    client.fund(&investor, &10_000);

    let events = emitted_events(&env, &contract_id);
    let event = events.last().expect("expected an event after fund");
    let topics = event_topics(event);
    assert_eq!(topics.len(), 4);

    let event_name = Symbol::try_from_val(&env, &topics[0])
        .expect("event name topic must be convertible to Symbol");
    assert_eq!(event_name, Symbol::new(&env, "escrow_funded"));
    assert_eq!(routing_symbol(&env, event), symbol_short!("funded"));
}

/// Verify full `EscrowFunded` payload from a plain [`LiquifactEscrow::fund`]
/// call. Existing yield-tier tests only cover [`LiquifactEscrow::fund_with_commitment`];
/// this test covers the simple `fund` code path.
#[test]
fn test_escrow_funded_payload_from_plain_fund() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let _admin = init_for_funding(&env, &client, 100_000, "PLAIN01");
    let investor = Address::generate(&env);
    let invoice_id = client.get_escrow().invoice_id;

    client.fund(&investor, &10_000);

    let events = emitted_events(&env, &contract_id);
    let event = events.last().expect("expected an event after fund");

    let expected = EscrowFunded {
        name: symbol_short!("funded"),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        amount: 10_000i128,
        funded_amount: 10_000i128,
        status: 0,
        investor_effective_yield_bps: 800,
        tier_lock_secs: 0,
    };

    assert_eq!(
        *event,
        expected.to_xdr(&env, &contract_id),
        "EscrowFunded payload from plain fund() must match expected fields"
    );
}

/// A deposit that crosses the funding target emits one `0 -> 1` state change
/// followed by an `EscrowFunded` event whose payload reports `status == 1`.
#[test]
fn test_escrow_funded_status_one_on_funded_transition() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let target = 10_000i128;
    let _admin = init_for_funding(&env, &client, target, "STAT01");
    let investor = Address::generate(&env);
    let invoice_id = client.get_escrow().invoice_id;

    client.fund(&investor, &target);

    let events = emitted_events(&env, &contract_id);
    assert_eq!(events.len(), 2, "one state change and one funded event");
    assert_eq!(
        events[0],
        FundingStateChanged {
            name: Symbol::new(&env, "fund_st_ch"),
            invoice_id: invoice_id.clone(),
            old_status: 0,
            new_status: 1,
        }
        .to_xdr(&env, &contract_id)
    );
    assert_eq!(
        events[1],
        EscrowFunded {
            name: symbol_short!("funded"),
            invoice_id,
            investor,
            amount: target,
            funded_amount: target,
            status: 1,
            investor_effective_yield_bps: 800,
            tier_lock_secs: 0,
        }
        .to_xdr(&env, &contract_id)
    );
}

// ── FundingCancelled event tests ─────────────────────────────────────────────

/// Verify that [`LiquifactEscrow::cancel_funding`] emits a [`FundingCancelled`]
/// event with the correct topic symbol `"fund_can"` and `funded_amount` payload.
#[test]
fn test_funding_cancelled_event_topic_and_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let _admin = init_for_funding(&env, &client, 100_000, "CAN01");
    let investor = Address::generate(&env);
    let invoice_id = client.get_escrow().invoice_id;

    // Partial fund so funded_amount > 0.
    client.fund(&investor, &30_000);
    client.cancel_funding();

    let events = emitted_events(&env, &contract_id);
    assert_eq!(
        events.len(),
        2,
        "one state change and one cancellation event"
    );
    assert_eq!(
        events[0],
        FundingStateChanged {
            name: Symbol::new(&env, "fund_st_ch"),
            invoice_id: invoice_id.clone(),
            old_status: 0,
            new_status: 4,
        }
        .to_xdr(&env, &contract_id)
    );

    let event = events
        .last()
        .expect("expected an event after cancel_funding");
    let topics = event_topics(event);
    assert_eq!(topics.len(), 3);
    let event_name = Symbol::try_from_val(&env, &topics[0])
        .expect("event name topic must be convertible to Symbol");
    assert_eq!(event_name, Symbol::new(&env, "funding_cancelled"));
    assert_eq!(routing_symbol(&env, event), symbol_short!("fund_can"));

    assert_eq!(
        *event,
        FundingCancelled {
            name: symbol_short!("fund_can"),
            invoice_id,
            funded_amount: 30_000i128,
        }
        .to_xdr(&env, &contract_id),
        "FundingCancelled payload must match expected fields"
    );
}

/// Verify that `cancel_funding` with zero funded amount emits `funded_amount: 0`.
#[test]
fn test_funding_cancelled_event_zero_funded_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let _admin = init_for_funding(&env, &client, 100_000, "CAN00");
    let invoice_id = client.get_escrow().invoice_id;

    client.cancel_funding();

    let events = emitted_events(&env, &contract_id);
    let event = events
        .last()
        .expect("expected an event after cancel_funding");

    assert_eq!(
        *event,
        FundingCancelled {
            name: symbol_short!("fund_can"),
            invoice_id,
            funded_amount: 0i128,
        }
        .to_xdr(&env, &contract_id),
        "FundingCancelled with zero funded_amount must report 0"
    );
}

// ── fund_batch per-entry event tests ──────────────────────────────────────────

/// Verify that [`LiquifactEscrow::fund_batch`] emits one [`EscrowFunded`] event
/// per batch entry, each with the correct per-entry `investor` and `amount`.
/// The existing test only checks the event count.
#[test]
fn test_fund_batch_per_entry_escrow_funded_content() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let target = 100_000i128;
    let _admin = init_for_funding(&env, &client, target, "BATC01");
    let invoice_id = client.get_escrow().invoice_id;

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let inv3 = Address::generate(&env);

    let mut entries = SorobanVec::new(&env);
    entries.push_back((inv1.clone(), 10_000i128));
    entries.push_back((inv2.clone(), 20_000i128));
    entries.push_back((inv3.clone(), 30_000i128));

    client.fund_batch(&entries);

    let events = emitted_events(&env, &contract_id);
    let expected_events = [
        EscrowFunded {
            name: symbol_short!("funded"),
            invoice_id: invoice_id.clone(),
            investor: inv1,
            amount: 10_000i128,
            funded_amount: 10_000i128,
            status: 0,
            investor_effective_yield_bps: 800,
            tier_lock_secs: 0,
        },
        EscrowFunded {
            name: symbol_short!("funded"),
            invoice_id: invoice_id.clone(),
            investor: inv2,
            amount: 20_000i128,
            funded_amount: 30_000i128,
            status: 0,
            investor_effective_yield_bps: 800,
            tier_lock_secs: 0,
        },
        EscrowFunded {
            name: symbol_short!("funded"),
            invoice_id,
            investor: inv3,
            amount: 30_000i128,
            funded_amount: 60_000i128,
            status: 0,
            investor_effective_yield_bps: 800,
            tier_lock_secs: 0,
        },
    ]
    .map(|event| event.to_xdr(&env, &contract_id));

    // Exactly three EscrowFunded events in entry order; no state change below target.
    assert_eq!(events.as_slice(), expected_events);
}

// ── Topic collision test ─────────────────────────────────────────────────────

/// Run through a funding lifecycle and assert that all four routing symbols are
/// pairwise distinct. Each invocation is snapshotted separately because SDK 25's
/// `Events::all()` exposes only the most recent successful contract invocation.
#[test]
fn test_funding_event_topics_no_collision() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let _admin = init_for_funding(&env, &client, 100_000, "NOCOLL1");
    let investor = Address::generate(&env);
    let mut topics_seen: std::vec::Vec<Symbol> = std::vec::Vec::new();

    client.fund(&investor, &50_000);
    record_unique_routing_topics(&env, &contract_id, &mut topics_seen);

    client.unfund(&investor, &10_000);
    record_unique_routing_topics(&env, &contract_id, &mut topics_seen);

    client.cancel_funding();
    record_unique_routing_topics(&env, &contract_id, &mut topics_seen);

    assert_eq!(topics_seen.len(), 4);
    for expected in [
        symbol_short!("funded"),
        Symbol::new(&env, "fund_st_ch"),
        symbol_short!("fund_can"),
        symbol_short!("unfunded"),
    ] {
        assert!(
            topics_seen.iter().any(|seen| *seen == expected),
            "missing funding lifecycle routing topic: {expected}"
        );
    }
}
