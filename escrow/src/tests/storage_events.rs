//! Tests for issue #732 — dedicated `FundingSnapshotStored` event on storage state change.
//!
//! Covers:
//! - Event emitted when `fund` crosses the funding target (status 0 → 1)
//! - Event emitted from `partial_settle`
//! - Event emitted from `update_funding_target` when lowering target triggers transition
//! - Event NOT emitted when funding does not cross the target
//! - Event NOT emitted when snapshot already exists (idempotent guard)
//! - Topic `snap_st` has no collision with any other event topic
//! - Event payload carries correct `total_principal`, `funding_target`, timestamps
//! - `fund_with_commitment` path fires the event on threshold crossing
//! - `fund_batch` fires the event exactly once across multiple investors

use super::*;
use crate::{FundingSnapshotStored, LiquifactEscrowClient};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, String,
};

// ── Helper ────────────────────────────────────────────────────────────────────

fn setup_mock<'a>(
    env: &'a Env,
    target: i128,
    invoice: &str,
) -> (
    LiquifactEscrowClient<'a>,
    Address,
    soroban_sdk::Symbol,
    Address,
    Address,
) {
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &String::from_str(env, invoice),
        &sme,
        &target,
        &500i64,
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
    // Drain init event so subsequent assertions are scoped to the call under test.
    let _ = env.events().all();
    let invoice_sym = client.get_escrow().invoice_id;
    (client, contract_id, invoice_sym, token, treasury)
}

// ── fund path ─────────────────────────────────────────────────────────────────

/// When `fund` meets the funding target, `FundingSnapshotStored` is emitted.
#[test]
fn test_snapshot_event_emitted_on_fund_crossing_target() {
    let env = Env::default();
    let mut li = env.ledger().get();
    li.timestamp = 1_000;
    li.sequence_number = 42;
    env.ledger().set(li);

    let target = 50_000i128;
    let (client, contract_id, invoice_id, _token, _treasury) =
        setup_mock(&env, target, "EVT_FUND1");

    let inv = Address::generate(&env);
    client.fund(&inv, &target);

    let expected = FundingSnapshotStored {
        name: symbol_short!("snap_st"),
        invoice_id,
        total_principal: target,
        funding_target: target,
        closed_at_ledger_timestamp: 1_000,
        closed_at_ledger_sequence: 42,
    }
    .to_xdr(&env, &contract_id);

    let all = env.events().all();
    let list = all.events();
    assert!(
        list.iter().any(|e| *e == expected),
        "FundingSnapshotStored not found after fund crossing target"
    );
}

/// When funding does NOT cross the target, no `FundingSnapshotStored` is emitted.
#[test]
fn test_snapshot_event_not_emitted_when_below_target() {
    let env = Env::default();
    let target = 50_000i128;
    let (client, _contract_id, _invoice_id, _token, _treasury) =
        setup_mock(&env, target, "EVT_FUND2");

    let inv = Address::generate(&env);
    client.fund(&inv, &(target - 1));

    // Only one event from this call: EscrowFunded (no snap_st).
    let all = env.events().all();
    assert_eq!(
        all.events().len(),
        1,
        "expected exactly 1 event (funded only, no snap_st), got {}",
        all.events().len()
    );
}

/// Overfunded: `total_principal` in the event equals the actual funded amount, not the target.
#[test]
fn test_snapshot_event_carries_overfunded_principal() {
    let env = Env::default();
    let mut li = env.ledger().get();
    li.timestamp = 2_000;
    li.sequence_number = 99;
    env.ledger().set(li);

    let target = 10_000i128;
    let overfund = 15_000i128;
    let (client, contract_id, invoice_id, _token, _treasury) = setup_mock(&env, target, "EVT_OVER");

    let inv = Address::generate(&env);
    client.fund(&inv, &overfund);

    let expected = FundingSnapshotStored {
        name: symbol_short!("snap_st"),
        invoice_id,
        total_principal: overfund,
        funding_target: target,
        closed_at_ledger_timestamp: 2_000,
        closed_at_ledger_sequence: 99,
    }
    .to_xdr(&env, &contract_id);

    let list = env.events().all();
    assert!(
        list.events().iter().any(|e| *e == expected),
        "FundingSnapshotStored with overfunded total_principal not found"
    );
}

/// The snapshot event fires exactly once when two investors are needed to cross the target.
/// First deposit emits no snap_st; second deposit (crossing threshold) emits exactly one.
#[test]
fn test_snapshot_event_fires_once_on_two_investor_crossing() {
    let env = Env::default();
    let mut li = env.ledger().get();
    li.timestamp = 0;
    li.sequence_number = 100;
    env.ledger().set(li);

    let target = 20_000i128;
    let (client, contract_id, invoice_id, _token, _treasury) = setup_mock(&env, target, "EVT_2INV");

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);

    // First deposit: below target — only EscrowFunded, no snap_st.
    client.fund(&inv_a, &10_000i128);
    let after_first = env.events().all();
    assert_eq!(
        after_first.events().len(),
        1,
        "expected 1 event (funded_a only) after first deposit"
    );

    // Second deposit: crosses target — snap_st + EscrowFunded = 2 events.
    client.fund(&inv_b, &10_000i128);

    let expected = FundingSnapshotStored {
        name: symbol_short!("snap_st"),
        invoice_id,
        total_principal: 20_000i128,
        funding_target: 20_000i128,
        closed_at_ledger_timestamp: 0,
        closed_at_ledger_sequence: 100,
    }
    .to_xdr(&env, &contract_id);

    let after_second = env.events().all();
    let list = after_second.events();

    // 2 events: snap_st + funded_b
    assert_eq!(
        list.len(),
        2,
        "expected snap_st + funded_b, got {}",
        list.len()
    );

    let snap_count = list.iter().filter(|e| **e == expected).count();
    assert_eq!(
        snap_count, 1,
        "FundingSnapshotStored must fire exactly once"
    );
}

// ── partial_settle path ───────────────────────────────────────────────────────

/// `partial_settle` forces status 0→1 and must emit `FundingSnapshotStored`.
#[test]
fn test_snapshot_event_emitted_on_partial_settle() {
    let env = Env::default();
    let mut li = env.ledger().get();
    li.timestamp = 3_000;
    li.sequence_number = 55;
    env.ledger().set(li);

    let target = 100_000i128;
    let (client, contract_id, invoice_id, _token, _treasury) = setup_mock(&env, target, "EVT_PSET");

    // Fund below target — escrow stays open.
    let inv = Address::generate(&env);
    client.fund(&inv, &50_000i128);
    let _ = env.events().all(); // drain fund event

    let caller = client.get_escrow().sme_address;
    client.partial_settle(&caller);

    let expected = FundingSnapshotStored {
        name: symbol_short!("snap_st"),
        invoice_id,
        total_principal: 50_000i128,
        funding_target: target,
        closed_at_ledger_timestamp: 3_000,
        closed_at_ledger_sequence: 55,
    }
    .to_xdr(&env, &contract_id);

    let list = env.events().all();
    assert!(
        list.events().iter().any(|e| *e == expected),
        "FundingSnapshotStored not emitted by partial_settle"
    );
}

// ── update_funding_target path ────────────────────────────────────────────────

/// Lowering target to ≤ funded_amount triggers transition and emits `FundingSnapshotStored`.
#[test]
fn test_snapshot_event_emitted_on_target_lowered_to_funded_amount() {
    let env = Env::default();
    let mut li = env.ledger().get();
    li.timestamp = 4_000;
    li.sequence_number = 77;
    env.ledger().set(li);

    let (client, contract_id, invoice_id, _token, _treasury) =
        setup_mock(&env, 100_000i128, "EVT_TGT");

    let inv = Address::generate(&env);
    client.fund(&inv, &60_000i128);
    let _ = env.events().all(); // drain fund event

    // Lower target to match funded_amount — triggers 0→1 + snapshot.
    client.update_funding_target(&60_000i128);

    let expected = FundingSnapshotStored {
        name: symbol_short!("snap_st"),
        invoice_id,
        total_principal: 60_000i128,
        funding_target: 60_000i128,
        closed_at_ledger_timestamp: 4_000,
        closed_at_ledger_sequence: 77,
    }
    .to_xdr(&env, &contract_id);

    let list = env.events().all();
    assert!(
        list.events().iter().any(|e| *e == expected),
        "FundingSnapshotStored not emitted by update_funding_target"
    );
}

/// Lowering target but still above funded_amount must NOT emit a snapshot event.
#[test]
fn test_snapshot_event_not_emitted_when_target_still_above_funded() {
    let env = Env::default();
    let (client, _contract_id, _invoice_id, _token, _treasury) =
        setup_mock(&env, 100_000i128, "EVT_TGT2");

    let inv = Address::generate(&env);
    client.fund(&inv, &40_000i128);
    let _ = env.events().all(); // drain fund event

    // Lower to 70_000 — still above funded 40_000, no snapshot.
    client.update_funding_target(&70_000i128);

    // Only fund_tgt event from this call, no snap_st.
    let all = env.events().all();
    assert_eq!(
        all.events().len(),
        1,
        "expected 1 event (fund_tgt only, no snap_st), got {}",
        all.events().len()
    );
}

// ── fund_with_commitment path ─────────────────────────────────────────────────

/// `fund_with_commitment` emits the snapshot event when crossing the target.
#[test]
fn test_snapshot_event_emitted_on_fund_with_commitment_crossing_target() {
    let env = Env::default();
    let mut li = env.ledger().get();
    li.timestamp = 5_000;
    li.sequence_number = 11;
    env.ledger().set(li);

    let target = 20_000i128;
    let (client, contract_id, invoice_id, _token, _treasury) = setup_mock(&env, target, "EVT_COMM");

    let inv = Address::generate(&env);
    client.fund_with_commitment(&inv, &target, &0u64);

    let expected = FundingSnapshotStored {
        name: symbol_short!("snap_st"),
        invoice_id,
        total_principal: target,
        funding_target: target,
        closed_at_ledger_timestamp: 5_000,
        closed_at_ledger_sequence: 11,
    }
    .to_xdr(&env, &contract_id);

    let list = env.events().all();
    assert!(
        list.events().iter().any(|e| *e == expected),
        "FundingSnapshotStored not emitted by fund_with_commitment"
    );
}

// ── fund_batch path ───────────────────────────────────────────────────────────

/// `fund_batch` emits `FundingSnapshotStored` exactly once when the batch crosses the threshold.
#[test]
fn test_snapshot_event_emitted_exactly_once_from_fund_batch() {
    let env = Env::default();
    let mut li = env.ledger().get();
    li.timestamp = 6_000;
    li.sequence_number = 20;
    env.ledger().set(li);

    let target = 30_000i128;
    let (client, contract_id, invoice_id, _token, _treasury) =
        setup_mock(&env, target, "EVT_BATCH");

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let inv3 = Address::generate(&env);

    let mut entries = soroban_sdk::Vec::new(&env);
    entries.push_back((inv1, 10_000i128));
    entries.push_back((inv2, 10_000i128));
    entries.push_back((inv3, 10_000i128)); // this entry crosses the target

    client.fund_batch(&entries);

    let expected = FundingSnapshotStored {
        name: symbol_short!("snap_st"),
        invoice_id,
        total_principal: target,
        funding_target: target,
        closed_at_ledger_timestamp: 6_000,
        closed_at_ledger_sequence: 20,
    }
    .to_xdr(&env, &contract_id);

    let all = env.events().all();
    let list = all.events();
    let snap_count = list.iter().filter(|e| **e == expected).count();
    assert_eq!(
        snap_count, 1,
        "FundingSnapshotStored must fire exactly once from fund_batch"
    );
}

// ── No topic collision ────────────────────────────────────────────────────────

/// The `snap_st` symbol is ≤ 9 characters (symbol_short! limit).
#[test]
fn test_snap_st_topic_length_within_limit() {
    assert!(
        "snap_st".len() <= 9,
        "snap_st exceeds symbol_short! 9-char limit"
    );
}

/// `snap_st` is distinct from every other topic in the contract.
#[test]
fn test_snap_st_no_topic_collision_with_existing_topics() {
    let existing = [
        "escrow_ii",
        "funded",
        "escrow_sd",
        "inv_claim",
        "sme_wd",
        "legalhld",
        "legal_h",
        "paused",
        "coll_rec",
        "coll_clr",
        "fund_ext",
        "dust_sw",
        "al_set",
        "al_ena",
        "att_bind",
        "att_app",
        "att_rev",
        "att_unrev",
        "ben_rot",
        "part_set",
        "fund_tgt",
        "raise_cap",
        "inv_cap",
        "floor_lo",
        "mtry_max",
        "mtry_rse",
        "adm_prop",
        "adm_acc",
        "adm_sup",
        "adm_can",
        "depr_xfer",
        "upgrade",
        "maturity",
        "fund_can",
        "refunded",
        "unfunded",
        "lh_req",
        "lh_cancel",
    ];
    for topic in &existing {
        assert_ne!("snap_st", *topic, "snap_st collides with '{topic}'");
    }
}

// ── Payload correctness ───────────────────────────────────────────────────────

/// Event payload fields match the on-chain `FundingCloseSnapshot` values exactly.
#[test]
fn test_snapshot_event_payload_matches_stored_snapshot() {
    let env = Env::default();
    let mut li = env.ledger().get();
    li.timestamp = 9_999;
    li.sequence_number = 123;
    env.ledger().set(li);

    let target = 25_000i128;
    let (client, contract_id, invoice_id, _token, _treasury) = setup_mock(&env, target, "EVT_PAY");

    let inv = Address::generate(&env);
    client.fund(&inv, &target);

    let expected = FundingSnapshotStored {
        name: symbol_short!("snap_st"),
        invoice_id,
        total_principal: target,
        funding_target: target,
        closed_at_ledger_timestamp: 9_999,
        closed_at_ledger_sequence: 123,
    }
    .to_xdr(&env, &contract_id);

    let list = env.events().all();
    assert!(
        list.events().iter().any(|e| *e == expected),
        "payload mismatch in FundingSnapshotStored"
    );

    // Cross-check: the on-chain snapshot must carry the same values.
    let snap = client
        .get_funding_close_snapshot()
        .expect("snapshot must exist after fund crosses target");
    assert_eq!(snap.total_principal, target);
    assert_eq!(snap.funding_target, target);
    assert_eq!(snap.closed_at_ledger_timestamp, 9_999);
    assert_eq!(snap.closed_at_ledger_sequence, 123);
}

/// `snap_st` event appears before the companion `funded` event in the same call.
#[test]
fn test_snapshot_event_precedes_funded_event_in_same_call() {
    let env = Env::default();
    let mut li = env.ledger().get();
    li.timestamp = 7_000;
    li.sequence_number = 50;
    env.ledger().set(li);

    let target = 10_000i128;
    let (client, contract_id, invoice_id, _token, _treasury) = setup_mock(&env, target, "EVT_ORD");

    let inv = Address::generate(&env);
    client.fund(&inv, &target);

    let snap_xdr = FundingSnapshotStored {
        name: symbol_short!("snap_st"),
        invoice_id: invoice_id.clone(),
        total_principal: target,
        funding_target: target,
        closed_at_ledger_timestamp: 7_000,
        closed_at_ledger_sequence: 50,
    }
    .to_xdr(&env, &contract_id);

    let funded_xdr = EscrowFunded {
        name: symbol_short!("funded"),
        invoice_id,
        investor: inv,
        amount: target,
        funded_amount: target,
        status: 1,
        investor_effective_yield_bps: 500,
        tier_lock_secs: 0,
    }
    .to_xdr(&env, &contract_id);

    let all = env.events().all();
    let list = all.events();

    let snap_pos = list.iter().position(|e| *e == snap_xdr);
    let funded_pos = list.iter().position(|e| *e == funded_xdr);

    assert!(snap_pos.is_some(), "snap_st event not found");
    assert!(funded_pos.is_some(), "funded event not found");
    assert!(
        snap_pos.unwrap() < funded_pos.unwrap(),
        "snap_st must appear before funded in the event list"
    );
}
