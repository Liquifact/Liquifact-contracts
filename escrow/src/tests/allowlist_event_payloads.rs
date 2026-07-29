//! Tests for allowlist event topic symbols and payload fields.
//!
//! Verifies that `AllowlistEnabledChanged`, `InvestorAllowlistChanged`, and
//! `InvestorAllowlistBatchApplied` are emitted with the correct topic symbol and payload
//! on every entrypoint that mutates the allowlist gate.
//!
//! Covers:
//! - `al_ena` topic and `active` field for `set_allowlist_active`
//! - `al_set` topic and `allowed` / `investor` fields for single and batch setters
//! - `al_batch` summary event emitted after per-investor events for batch setters
//! - Topic non-collision: `al_ena`, `al_set`, `al_batch` are pairwise distinct
//! - `al_batch` is NOT emitted by the single-address setter

use super::super::{
    AllowlistEnabledChanged, InvestorAllowlistBatchApplied, InvestorAllowlistChanged,
    LiquifactEscrow, LiquifactEscrowClient,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    Address, Env, Vec as SorobanVec,
};

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn init(env: &Env, client: &LiquifactEscrowClient) {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "ALEVPAY01"),
        &sme,
        &10_000i128,
        &800i64,
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
    );
}

// ── AllowlistEnabledChanged (al_ena) ─────────────────────────────────────────

/// `set_allowlist_active(true)` emits exactly one `al_ena` event with `active = 1`.
#[test]
fn event_al_ena_enable_topic_and_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    client.set_allowlist_active(&true);
    let events = env.events().all();

    let expected = AllowlistEnabledChanged {
        name: symbol_short!("al_ena"),
        invoice_id,
        active: 1,
    };
    assert_eq!(events, std::vec![expected.to_xdr(&env, &contract_id)]);
}

/// `set_allowlist_active(false)` emits exactly one `al_ena` event with `active = 0`.
#[test]
fn event_al_ena_disable_topic_and_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    // Enable then immediately disable; capture the disable event only.
    client.set_allowlist_active(&true);
    client.set_allowlist_active(&false);
    let events = env.events().all();

    let expected = AllowlistEnabledChanged {
        name: symbol_short!("al_ena"),
        invoice_id,
        active: 0,
    };
    assert_eq!(events, std::vec![expected.to_xdr(&env, &contract_id)]);
}

/// The `al_ena` topic symbol does not collide with `al_set`.
#[test]
fn event_al_ena_topic_distinct_from_al_set() {
    let al_ena = symbol_short!("al_ena");
    let al_set = symbol_short!("al_set");
    assert_ne!(al_ena, al_set, "al_ena and al_set topic symbols must differ");
}

// ── InvestorAllowlistChanged (al_set) ────────────────────────────────────────

/// `set_investor_allowlisted(investor, true)` emits exactly one `al_set` event with `allowed = 1`.
#[test]
fn event_al_set_add_topic_and_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;
    let investor = Address::generate(&env);

    client.set_investor_allowlisted(&investor, &true);
    let events = env.events().all();

    let expected = InvestorAllowlistChanged {
        name: symbol_short!("al_set"),
        invoice_id,
        investor,
        allowed: 1,
    };
    assert_eq!(events, std::vec![expected.to_xdr(&env, &contract_id)]);
}

/// `set_investor_allowlisted(investor, false)` emits exactly one `al_set` event with `allowed = 0`.
#[test]
fn event_al_set_remove_topic_and_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;
    let investor = Address::generate(&env);

    client.set_investor_allowlisted(&investor, &true);
    client.set_investor_allowlisted(&investor, &false);
    let events = env.events().all();

    let expected = InvestorAllowlistChanged {
        name: symbol_short!("al_set"),
        invoice_id,
        investor,
        allowed: 0,
    };
    assert_eq!(events, std::vec![expected.to_xdr(&env, &contract_id)]);
}

/// `set_investor_allowlisted` emits the correct `investor` address in the `al_set` payload:
/// allowlisting investor A must not leak investor B's address into the event.
#[test]
fn event_al_set_investor_field_matches_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    let a = Address::generate(&env);
    let b = Address::generate(&env);

    client.set_investor_allowlisted(&a, &true);
    let events_a = env.events().all();

    client.set_investor_allowlisted(&b, &true);
    let events_b = env.events().all();

    let expected_a = InvestorAllowlistChanged {
        name: symbol_short!("al_set"),
        invoice_id: invoice_id.clone(),
        investor: a,
        allowed: 1,
    };
    let expected_b = InvestorAllowlistChanged {
        name: symbol_short!("al_set"),
        invoice_id,
        investor: b,
        allowed: 1,
    };
    assert_eq!(events_a, std::vec![expected_a.to_xdr(&env, &contract_id)]);
    assert_eq!(events_b, std::vec![expected_b.to_xdr(&env, &contract_id)]);
}

/// Batch `set_investors_allowlisted` emits one `al_set` per investor in input order.
#[test]
fn event_al_set_batch_one_per_investor_in_order() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    let a = Address::generate(&env);
    let b = Address::generate(&env);

    let mut batch: SorobanVec<Address> = SorobanVec::new(&env);
    batch.push_back(a.clone());
    batch.push_back(b.clone());

    client.set_investors_allowlisted(&batch, &true);
    let events = env.events().all();

    // 2 al_set events (indices 0, 1) + 1 al_batch event (index 2).
    assert!(
        events.len() >= 2,
        "expected at least 2 al_set events, got {}",
        events.len()
    );

    let expected_a = InvestorAllowlistChanged {
        name: symbol_short!("al_set"),
        invoice_id: invoice_id.clone(),
        investor: a,
        allowed: 1,
    };
    let expected_b = InvestorAllowlistChanged {
        name: symbol_short!("al_set"),
        invoice_id,
        investor: b,
        allowed: 1,
    };
    assert_eq!(events.get(0).unwrap(), expected_a.to_xdr(&env, &contract_id));
    assert_eq!(events.get(1).unwrap(), expected_b.to_xdr(&env, &contract_id));
}

// ── InvestorAllowlistBatchApplied (al_batch) ─────────────────────────────────

/// `set_investors_allowlisted` emits a single `al_batch` summary event after all `al_set` events.
#[test]
fn event_al_batch_emitted_once_after_per_investor_events() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);

    let mut batch: SorobanVec<Address> = SorobanVec::new(&env);
    batch.push_back(a);
    batch.push_back(b);
    batch.push_back(c);

    client.set_investors_allowlisted(&batch, &true);
    let events = env.events().all();

    // 3 al_set + 1 al_batch = 4 events total.
    assert_eq!(
        events.len(),
        4,
        "expected 3 al_set + 1 al_batch = 4 events, got {}",
        events.len()
    );

    let expected_batch = InvestorAllowlistBatchApplied {
        name: symbol_short!("al_batch"),
        invoice_id,
        batch_size: 3,
        allowed: 1,
    };
    // al_batch must be the last event.
    assert_eq!(
        events.get(3).unwrap(),
        expected_batch.to_xdr(&env, &contract_id)
    );
}

/// `al_batch` carries `allowed = 0` when the batch is a revocation.
#[test]
fn event_al_batch_revoke_payload_allowed_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    let a = Address::generate(&env);
    let b = Address::generate(&env);

    let mut batch: SorobanVec<Address> = SorobanVec::new(&env);
    batch.push_back(a);
    batch.push_back(b);

    client.set_investors_allowlisted(&batch, &true);
    // Revoke the same batch.
    client.set_investors_allowlisted(&batch, &false);
    let events = env.events().all();

    // Last event is the al_batch from the revocation call.
    let expected_batch = InvestorAllowlistBatchApplied {
        name: symbol_short!("al_batch"),
        invoice_id,
        batch_size: 2,
        allowed: 0,
    };
    let last_idx = (events.len() - 1) as u32;
    assert_eq!(
        events.get(last_idx).unwrap(),
        expected_batch.to_xdr(&env, &contract_id)
    );
}

/// `al_batch` is NOT emitted when `set_investor_allowlisted` (single-address) is called.
#[test]
fn event_al_batch_not_emitted_by_single_address_setter() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let investor = Address::generate(&env);

    client.set_investor_allowlisted(&investor, &true);
    let events = env.events().all();

    // Single-address setter emits only one al_set event, never al_batch.
    assert_eq!(
        events.len(),
        1,
        "single-address setter must emit exactly 1 event (al_set only), got {}",
        events.len()
    );
}

// ── Topic non-collision matrix ────────────────────────────────────────────────

/// All three allowlist topic symbols (`al_ena`, `al_set`, `al_batch`) are pairwise distinct.
#[test]
fn event_allowlist_all_topic_symbols_distinct() {
    let al_ena = symbol_short!("al_ena");
    let al_set = symbol_short!("al_set");
    let al_batch = symbol_short!("al_batch");

    assert_ne!(al_ena, al_set, "al_ena must differ from al_set");
    assert_ne!(al_ena, al_batch, "al_ena must differ from al_batch");
    assert_ne!(al_set, al_batch, "al_set must differ from al_batch");
}
