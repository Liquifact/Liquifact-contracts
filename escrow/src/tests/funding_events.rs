//! Tests for funding event topic symbols and payload fields.
//!
//! Covers:
//! - `EscrowFunded` topic symbol via direct `.topics()` extraction (not only via `to_xdr()`).
//! - `EscrowFunded` payload fields from plain [`LiquifactEscrow::fund`] (previously only tested via
//!   [`LiquifactEscrow::fund_with_commitment`]).
//! - `EscrowFunded` payload when the deposit triggers the 0 → 1 funded transition (`status == 1`).
//! - `FundingCancelled` event topic and payload (previously untested).
//! - `fund_batch` per-entry `EscrowFunded` content with correct per-entry `investor` / `amount`.
//! - No topic collision across funding lifecycle events (`EscrowFunded`, `FundingStateChanged`,
//!   `FundingCancelled`, `EscrowUnfunded`).
//!
//! Schema versioning: every lifecycle event appends a `u32` schema version as the final topic.
//! Existing event names, topic positions, and payload fields are unchanged so a consumer built
//! against the previous schema can still read events emitted by the new contract.

use super::*;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::Symbol;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Schema version appended as the final topic of every lifecycle event.
const EVENT_SCHEMA_VERSION: u32 = 1;

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

// ── EscrowFunded topic and payload tests ─────────────────────────────────────

/// Verify that `EscrowFunded` carries the topic symbol `"funded"` as its first
/// topic, extracted directly via `.topics().get(0)`. Existing tests verify this
/// only indirectly through `to_xdr()` struct comparison.
#[test]
fn test_escrow_funded_topic_via_direct_extraction() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client) = deploy_with_id(&env);

    let admin = init_for_funding(&env, &client, 100_000, "TOPIC01");
    let investor = Address::generate(&env);

    let _ = env.events().all(); // drain init events
    client.fund(&investor, &10_000);

    let all_events = env.events().all();
    let event = all_events
        .events()
        .last()
        .expect("expected at least one event after fund");

    let topics = event.topics();
    let topic0: Symbol = topics
        .get(0)
        .expect("event must have at least one topic")
        .try_into_val(&env)
        .expect("first topic must be convertible to Symbol");

    assert_eq!(topic0, symbol_short!("funded"));

    // Schema version is the final topic; the first topics are unchanged for old consumers.
    let version: u32 = topics
        .get(topics.len() - 1)
        .expect("event must include a schema version topic")
        .try_into_val(&env)
        .expect("schema version topic must be convertible to u32");
    assert_eq!(version, EVENT_SCHEMA_VERSION);
}

/// Verify full `EscrowFunded` payload from a plain [`LiquifactEscrow::fund`]
/// call. Existing yield-tier tests only cover [`LiquifactEscrow::fund_with_commitment`];
/// this test covers the simple `fund` code path.
#[test]
fn test_escrow_funded_payload_from_plain_fund() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let admin = init_for_funding(&env, &client, 100_000, "PLAIN01");
    let investor = Address::generate(&env);
    let invoice_id = client.get_escrow().invoice_id;

    let _ = env.events().all(); // drain init events
    client.fund(&investor, &10_000);

    let all_events = env.events().all();
    let event = all_events
        .events()
        .last()
        .expect("expected an event after fund");

    let expected = EscrowFunded {
        name: symbol_short!("funded"),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        amount: 10_000i128,
        funded_amount: 10_000i128,
        status: 0,
        investor_effective_yield_bps: 800,
        tier_lock_secs: 0,
        version: EVENT_SCHEMA_VERSION,
    };

    assert_eq!(
        *event,
        expected.to_xdr(&env, &contract_id),
        "EscrowFunded payload from plain fund() must match expected fields"
    );
}

/// Verify that an `EscrowFunded` event emitted by a deposit that crosses the
/// funding target has `status == 1` (funded transition). The existing yield-tier
/// tests only assert `status == 0` (still open).
#[test]
fn test_escrow_funded_status_one_on_funded_transition() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let target = 10_000i128;
    let admin = init_for_funding(&env, &client, target, "STAT01");
    let investor = Address::generate(&env);
    let invoice_id = client.get_escrow().invoice_id;

    let _ = env.events().all(); // drain init events
    client.fund(&investor, &target);

    let all_events = env.events().all();
    // The escrow should emit both EscrowFunded and FundingStateChanged.
    // Find the EscrowFunded event by matching its first topic.
    let funded_events: std::vec::Vec<_> = all_events
        .events()
        .iter()
        .filter(|e| {
            let expected = EscrowFunded {
                name: symbol_short!("funded"),
                invoice_id: invoice_id.clone(),
                investor: investor.clone(),
                amount: target,
                funded_amount: target,
                status: 1,
                investor_effective_yield_bps: 800,
                tier_lock_secs: 0,
                version: EVENT_SCHEMA_VERSION,
            };
            **e == expected.to_xdr(&env, &contract_id)
        })
        .collect();

    assert_eq!(
        funded_events.len(),
        1,
        "expected exactly one EscrowFunded event with status=1"
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

    let admin = init_for_funding(&env, &client, 100_000, "CAN01");
    let investor = Address::generate(&env);
    let invoice_id = client.get_escrow().invoice_id;

    // Partial fund so funded_amount > 0.
    client.fund(&investor, &30_000);

    let _ = env.events().all(); // drain events before cancel
    client.cancel_funding();

    let all_events = env.events().all();
    let event = all_events
        .events()
        .last()
        .expect("expected an event after cancel_funding");

    let topics = event.topics();
    let topic0: Symbol = topics
        .get(0)
        .expect("event must have at least one topic")
        .try_into_val(&env)
        .expect("first topic must be convertible to Symbol");

    assert_eq!(
        topic0,
        symbol_short!("fund_can"),
        "FundingCancelled first topic must be fund_can"
    );

    // Schema version is the final topic; the first topics are unchanged for old consumers.
    let version: u32 = topics
        .get(topics.len() - 1)
        .expect("FundingCancelled must include a schema version topic")
        .try_into_val(&env)
        .expect("schema version topic must be convertible to u32");
    assert_eq!(version, EVENT_SCHEMA_VERSION);

    assert_eq!(
        *event,
        FundingCancelled {
            name: symbol_short!("fund_can"),
            invoice_id,
            funded_amount: 30_000i128,
            version: EVENT_SCHEMA_VERSION,
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

    let admin = init_for_funding(&env, &client, 100_000, "CAN00");
    let invoice_id = client.get_escrow().invoice_id;

    let _ = env.events().all(); // drain init events
    client.cancel_funding();

    let all_events = env.events().all();
    let event = all_events
        .events()
        .last()
        .expect("expected an event after cancel_funding");

    assert_eq!(
        *event,
        FundingCancelled {
            name: symbol_short!("fund_can"),
            invoice_id,
            funded_amount: 0i128,
            version: EVENT_SCHEMA_VERSION,
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
    let admin = init_for_funding(&env, &client, target, "BATC01");
    let invoice_id = client.get_escrow().invoice_id;

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let inv3 = Address::generate(&env);

    let mut entries = SorobanVec::new(&env);
    entries.push_back((inv1.clone(), 10_000i128));
    entries.push_back((inv2.clone(), 20_000i128));
    entries.push_back((inv3.clone(), 30_000i128));

    let _ = env.events().all(); // drain init events
    client.fund_batch(&entries);

    let all_events = env.events().all();
    // Three EscrowFunded events, one per entry. No FundingStateChanged (total < target).
    let funded_events: std::vec::Vec<_> = all_events
        .events()
        .iter()
        .filter(|e| {
            let expected = EscrowFunded {
                name: symbol_short!("funded"),
                invoice_id: invoice_id.clone(),
                investor: inv1.clone(),
                amount: 10_000i128,
                funded_amount: 10_000i128,
                status: 0,
                investor_effective_yield_bps: 800,
                tier_lock_secs: 0,
                version: EVENT_SCHEMA_VERSION,
            };
            **e == expected.to_xdr(&env, &contract_id)
        })
        .collect();
    assert_eq!(
        funded_events.len(),
        1,
        "exactly one EscrowFunded for inv1 with 10_000"
    );

    let funded_events2: std::vec::Vec<_> = all_events
        .events()
        .iter()
        .filter(|e| {
            let expected = EscrowFunded {
                name: symbol_short!("funded"),
                invoice_id: invoice_id.clone(),
                investor: inv2.clone(),
                amount: 20_000i128,
                funded_amount: 30_000i128, // cumulative: first + second
                status: 0,
                investor_effective_yield_bps: 800,
                tier_lock_secs: 0,
                version: EVENT_SCHEMA_VERSION,
            };
            **e == expected.to_xdr(&env, &contract_id)
        })
        .collect();
    assert_eq!(
        funded_events2.len(),
        1,
        "exactly one EscrowFunded for inv2 with 20_000"
    );

    let funded_events3: std::vec::Vec<_> = all_events
        .events()
        .iter()
        .filter(|e| {
            let expected = EscrowFunded {
                name: symbol_short!("funded"),
                invoice_id,
                investor: inv3.clone(),
                amount: 30_000i128,
                funded_amount: 60_000i128, // cumulative: 10k + 20k + 30k
                status: 0,
                investor_effective_yield_bps: 800,
                tier_lock_secs: 0,
                version: EVENT_SCHEMA_VERSION,
            };
            **e == expected.to_xdr(&env, &contract_id)
        })
        .collect();
    assert_eq!(
        funded_events3.len(),
        1,
        "exactly one EscrowFunded for inv3 with 30_000"
    );

    // Total: exactly 3 EscrowFunded events.
    assert_eq!(all_events.events().len(), 3);
}

// ── Topic collision test ─────────────────────────────────────────────────────

/// Run through a funding lifecycle (fund, cancel) and assert that every event
/// topic symbol is pairwise distinct. This catches accidental reuse of the same
/// short symbol for different event types.
///
/// Known duplicates are explicitly allowed: `EscrowFunded` (`"funded"`) may appear
/// multiple times (e.g. across batch entries or separate fund calls).
#[test]
fn test_funding_event_topics_no_collision() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client) = deploy_with_id(&env);

    let admin = init_for_funding(&env, &client, 100_000, "NOCOLL1");
    let investor = Address::generate(&env);

    let _ = env.events().all(); // drain init events

    // Step 1: fund (emits EscrowFunded)
    client.fund(&investor, &50_000);

    // Step 2: cancel (emits FundingCancelled)
    client.cancel_funding();

    let all_events = env.events().all();

    // Collect all unique topic symbols (topic 0).
    let mut topics_seen: std::vec::Vec<Symbol> = std::vec::Vec::new();
    for event in all_events.events().iter() {
        let t = event.topics();
        if t.len() >= 1 {
            let sym: Symbol = t
                .get(0)
                .unwrap()
                .try_into_val(&env)
                .expect("first topic must be a Symbol");
            // Allow "funded" to appear multiple times (one per fund call).
            if sym != symbol_short!("funded") {
                assert!(
                    !topics_seen.iter().any(|s| s == &sym),
                    "Duplicate topic symbol found across different event types: {}",
                    sym
                );
            }
            if !topics_seen.iter().any(|s| s == &sym) {
                topics_seen.push(sym);
            }
        }
    }

    // We expect at least: funded, fund_can
    assert!(
        topics_seen.iter().any(|s| *s == symbol_short!("funded")),
        "must contain 'funded' topic"
    );
    assert!(
        topics_seen.iter().any(|s| *s == symbol_short!("fund_can")),
        "must contain 'fund_can' topic"
    );
}
