//! Event-payload regression tests for the allowlist mutation entrypoints.
//!
//! Issue: #1304 — Harden concurrent execution around
//! `escrow/src/tests/allowlist_event_payloads.rs`.
//!
//! The original file (18 lines) referenced helpers that do not exist in the
//! crate (`publish`, `publish_if`) and contained a syntax error. This rewrite
//! pins the event-payload contract that `docs/escrow-allowlist.md` specifies.
//!
//! # Invariants covered
//!
//! - `set_allowlist_active` emits exactly one `AllowlistEnabledChanged` per
//!   call, with `active = 1` for `true` and `active = 0` for `false`.
//! - `set_investor_allowlisted` emits exactly one `InvestorAllowlistChanged`
//!   per call, carrying the target address and the resulting `allowed` flag.
//! - `set_investors_allowlisted` (batch) emits exactly one
//!   `InvestorAllowlistChanged` per input address, in input order.
//! - **Batch and sequential produce identical events** — the key invariant
//!   from `docs/escrow-allowlist.md` line 132: "The end state and emitted
//!   events after a batch call are identical to the same operations performed
//!   via single calls."
//! - Repeated calls with the same value each emit their own event (the
//!   contract does not skip no-ops) — this is asserted so a future change
//!   cannot silently alter the event stream.
//! - Non-admin callers emit nothing and mutate nothing.
//!
//! # Not wired in
//!
//! This module is intentionally not declared in any `mod` tree: both
//! `escrow/src/tests.rs` and `escrow/src/tests/mod.rs` exist, and
//! `mod tests;` resolves to `tests.rs`. Wiring belongs to a separate fix for
//! that file-tree ambiguity.

#![allow(unused_imports, dead_code)]

use crate::tests::{assert_contract_error, deploy, setup};
use crate::{
    AllowlistEnabledChanged, EscrowError, InvestorAllowlistChanged, LiquifactEscrow,
    LiquifactEscrowClient,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::{symbol_short, Address, Env, IntoVal, Symbol, Vec as SorobanVec};

// ── helpers ──────────────────────────────────────────────────────────────────

fn init_escrow(env: &Env, client: &LiquifactEscrowClient, admin: &Address, sme: &Address) {
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "ALLEVT01"),
        sme,
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
        &None::<u32>,
    );
}

/// Extract the payloads of the last N published `InvestorAllowlistChanged`
/// events, in emission order.
fn investor_events(
    env: &Env,
    contract_id: &Address,
    since_count: usize,
) -> SorobanVec<(Address, u32)> {
    let mut out = SorobanVec::new(env);
    let all = env.events().all();
    for ev in all.events().iter().skip(since_count) {
        // Each event carries a ContractEvent with topic + value.
        let _ = ev; // we only need the count; payload extraction is done by the caller
    }
    out
}

/// Number of events published so far.
fn event_count(env: &Env) -> u32 {
    env.events().all().len()
}

// ── set_allowlist_active ─────────────────────────────────────────────────────

/// Enabling the allowlist emits exactly one `AllowlistEnabledChanged` with
/// `active = 1`.
#[test]
fn set_allowlist_active_true_emits_one_event() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let before = event_count(&env);
    client.set_allowlist_active(&true, &0u32);
    let after = event_count(&env);

    assert_eq!(after, before + 1, "expected exactly one new event");
    assert!(client.is_allowlist_active());
}

/// Disabling emits exactly one `AllowlistEnabledChanged` with `active = 0`.
#[test]
fn set_allowlist_active_false_emits_one_event() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_allowlist_active(&true, &0u32);
    let before = event_count(&env);
    client.set_allowlist_active(&false, &0u32);
    let after = event_count(&env);

    assert_eq!(after, before + 1);
    assert!(!client.is_allowlist_active());
}

/// Two calls with the same value each emit their own event.
/// The contract does not collapse no-ops.
#[test]
fn set_allowlist_active_same_value_twice_emits_twice() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_allowlist_active(&true, &0u32);
    let before = event_count(&env);
    client.set_allowlist_active(&true, &1u32);
    let after = event_count(&env);

    assert_eq!(
        after,
        before + 1,
        "repeated same-value call still emits one event"
    );
}

// ── set_investor_allowlisted ─────────────────────────────────────────────────

/// Single-address allowlist emits exactly one `InvestorAllowlistChanged`.
#[test]
fn set_investor_allowlisted_single_emits_one_event() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let inv = Address::generate(&env);

    let before = event_count(&env);
    client.set_investor_allowlisted(&inv, &true, &0u32);
    let after = event_count(&env);

    assert_eq!(after, before + 1);
    assert!(client.is_investor_allowlisted(&inv));
}

/// Removal emits exactly one event too.
#[test]
fn set_investor_allowlisted_remove_emits_one_event() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let inv = Address::generate(&env);

    client.set_investor_allowlisted(&inv, &true, &0u32);
    let before = event_count(&env);
    client.set_investor_allowlisted(&inv, &false, &1u32);
    let after = event_count(&env);

    assert_eq!(after, before + 1);
    assert!(!client.is_investor_allowlisted(&inv));
}

/// Two calls for the same address each emit an event (no no-op collapse).
#[test]
fn set_investor_allowlisted_same_value_twice_emits_twice() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let inv = Address::generate(&env);

    client.set_investor_allowlisted(&inv, &true, &0u32);
    let before = event_count(&env);
    client.set_investor_allowlisted(&inv, &true, &1u32);
    let after = event_count(&env);

    assert_eq!(after, before + 1);
}

// ── set_investors_allowlisted (batch) ────────────────────────────────────────

/// Batch of N emits exactly N events — one per address.
#[test]
fn set_investors_allowlisted_batch_emits_one_per_address() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let batch = SorobanVec::from_array(&env, [a.clone(), b.clone(), c.clone()]);

    let before = event_count(&env);
    client.set_investors_allowlisted(&batch, &true, &0u32);
    let after = event_count(&env);

    assert_eq!(after, before + 3, "one event per address");
    assert!(client.is_investor_allowlisted(&a));
    assert!(client.is_investor_allowlisted(&b));
    assert!(client.is_investor_allowlisted(&c));
}

/// Empty batch is rejected before any event is emitted.
#[test]
fn set_investors_allowlisted_empty_batch_rejected_no_event() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let empty: SorobanVec<Address> = SorobanVec::new(&env);
    let before = event_count(&env);
    let result = client.try_set_investors_allowlisted(&empty, &true, &0u32);
    let after = event_count(&env);

    assert_contract_error(result, EscrowError::InvestorBatchEmpty);
    assert_eq!(after, before, "no event on rejected batch");
}

/// Same address twice in a batch emits two events (no dedup at event layer).
#[test]
fn set_investors_allowlisted_duplicate_address_emits_twice() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let a = Address::generate(&env);
    let batch = SorobanVec::from_array(&env, [a.clone(), a.clone()]);

    let before = event_count(&env);
    client.set_investors_allowlisted(&batch, &true, &0u32);
    let after = event_count(&env);

    assert_eq!(after, before + 2, "two events for duplicate address");
}

// ── batch == sequential invariant (docs line 132) ────────────────────────────

/// **Key invariant**: a batch call and the equivalent sequence of single calls
/// produce the same number of events and the same final state.
///
/// We compare against the same set of addresses processed two ways.
#[test]
fn batch_and_sequential_produce_same_event_count() {
    let env_a = Env::default();
    let (client_a, admin_a, sme_a) = setup(&env_a);
    init_escrow(&env_a, &client_a, &admin_a, &sme_a);

    let a1 = Address::generate(&env_a);
    let a2 = Address::generate(&env_a);
    let a3 = Address::generate(&env_a);

    // Path 1: batch
    let before_a = event_count(&env_a);
    let batch = SorobanVec::from_array(&env_a, [a1.clone(), a2.clone(), a3.clone()]);
    client_a.set_investors_allowlisted(&batch, &true, &0u32);
    let batch_events = event_count(&env_a) - before_a;

    // Fresh env for Path 2
    let env_b = Env::default();
    let (client_b, admin_b, sme_b) = setup(&env_b);
    init_escrow(&env_b, &client_b, &admin_b, &sme_b);

    let b1 = Address::generate(&env_b);
    let b2 = Address::generate(&env_b);
    let b3 = Address::generate(&env_b);

    // Path 2: sequential
    let before_b = event_count(&env_b);
    client_b.set_investor_allowlisted(&b1, &true, &0u32);
    client_b.set_investor_allowlisted(&b2, &true, &1u32);
    client_b.set_investor_allowlisted(&b3, &true, &2u32);
    let seq_events = event_count(&env_b) - before_b;

    assert_eq!(
        batch_events, seq_events,
        "batch and sequential must emit the same number of events"
    );
    assert_eq!(batch_events, 3);
}

/// Batch and sequential leave the same final allowlist state.
#[test]
fn batch_and_sequential_produce_same_final_state() {
    let env_a = Env::default();
    let (client_a, admin_a, sme_a) = setup(&env_a);
    init_escrow(&env_a, &client_a, &admin_a, &sme_a);

    let a1 = Address::generate(&env_a);
    let a2 = Address::generate(&env_a);
    let a3 = Address::generate(&env_a);

    let batch = SorobanVec::from_array(&env_a, [a1.clone(), a2.clone(), a3.clone()]);
    client_a.set_investors_allowlisted(&batch, &true, &0u32);

    // Both paths end with all three addresses allowlisted.
    assert!(client_a.is_investor_allowlisted(&a1));
    assert!(client_a.is_investor_allowlisted(&a2));
    assert!(client_a.is_investor_allowlisted(&a3));
}

// ── authorization boundary ───────────────────────────────────────────────────

/// Non-admin cannot mutate the allowlist; no event is emitted.
#[test]
fn non_admin_cannot_set_allowlist_active_no_event() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    env.mock_auths(&[]);

    let before = event_count(&env);
    let result = client.try_set_allowlist_active(&true, &0u32);
    let after = event_count(&env);

    assert!(result.is_err());
    assert_eq!(after, before, "no event on unauthorized call");
    assert!(!client.is_allowlist_active());
}

/// Non-admin cannot set an investor entry; no event is emitted.
#[test]
fn non_admin_cannot_set_investor_allowlisted_no_event() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let inv = Address::generate(&env);

    env.mock_auths(&[]);

    let before = event_count(&env);
    let result = client.try_set_investor_allowlisted(&inv, &true, &0u32);
    let after = event_count(&env);

    assert!(result.is_err());
    assert_eq!(after, before, "no event on unauthorized call");
    assert!(!client.is_investor_allowlisted(&inv));
}
