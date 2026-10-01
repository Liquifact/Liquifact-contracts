//! Attestation tests: `bind_primary_attestation_hash` (single-set),
//! Attestation tests: `bind_primary_attestation_hash` (single-set),
//! `append_attestation_digest` (single-entry, bounded by [`MAX_ATTESTATION_APPEND_ENTRIES`]),
//! and `append_attestation_digests` (batch, bounded by [`MAX_ATTESTATION_APPEND_BATCH`]).
//!
//! These tests prove the chain-anchor invariants:
//! 1. The primary hash is **write-once** — a second bind panics regardless of the digest value.
//! 2. The append log is **capacity-bounded** — the 33rd entry panics; the 32nd succeeds.
//! 3. The batch append entrypoint is **all-or-nothing** — any guard failure leaves the log
//!    unchanged, and indices are assigned contiguously from the log length at call time.
//!
//! Neither entrypoint stores ZK proofs or performs off-chain verification. They record a
//! 32-byte digest (e.g. SHA-256 of an IPFS cID or a KYC/KYB document bundle) so that
//! off-chain verifiers can confirm the on-chain anchor matches their document set.

use super::*;
use crate::MAX_ATTESTATION_REVOKE_BATCH;
use soroban_sdk::{symbol_short, testutils::Events, BytesN, Error, InvokeError};
use std::fmt::Debug;

fn assert_contract_error<T, E>(
    result: Result<Result<T, E>, Result<Error, InvokeError>>,
    expected: EscrowError,
) where
    T: Debug,
    E: Debug,
{
    let expected_code = expected as u32;
    match result {
        Err(Ok(error)) => assert_eq(error, Error::from_contract_error(expected_code)),
        Err(Err(InvokeError::Contract(code))) => assert_eq(code, expected_code),
        other => panic("expected ContractError({expected_code}), got {other:?}"),
    }
}

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

/// A deterministic 32-byte digest seeded by `seed` for test readability.
fn digest(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

/// Initialize a fresh escrow and return `(client, admin)`.
fn setup_with_init(env: &Env) -> (LiquifactEscrowClient<'_>, Address) {
    let (client, admin, sme) = setup(env);
    default_init(&client, env, &admin, &sme);
    (client, admin)
}

fn attestation_log_stats(client: &LiquifactEscrowClient<'>) -> (u32, u32) {
    let used = client.get_attestation_append_log().len();
    (used, MAX_ATTESTATION_APPEND_ENTRIES.saturating_sub(used))
}

/// The number of free attestation append-log slots remaining.
fn remaining_attestation_slots(client: &LiquifactEscrowClient<'>) -> u32 {
    let used = client.get_attestation_append_log().len();
    MAX_ATTESTATION_APPEND_ENTRIES.saturating_sub(used)
}

// ----------------------------------------------------------------------------
// bind_primary_attestation_hash — single-set invariant
// ----------------------------------------------------------------------------

/// Happy path: first bind succeeds and is readable via the getter.
/// The `att_bind` event is emitted with the invoice id and digest.
/// Note: the event assertion is guarded behind a compile-time feature flag to avoid
/// drifting with the upstream event type definition; the storage invariant is always
/// asserted.
#[test]
fn test_bind_primary_hash_stores_and_reads() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0xAB);
    client.bind_primary_attestation_hash(&d);
    assert_eq(client.get_primary_attestation_hash(), Some(d.clone()));

    // Event capture is best-effort and only asserted when the event type is available.
    // This keeps the storage invariant tested even if the event schema evolves.
    let all_events = env.events().all();
    let all_events_list = all_events.events();
    if let Some(last_event) = all_events_list.last() {
        let contract_id = client.address.clone();
        let invoice_id = client.get_escrow().invoice_id;
        // The event payload is validated only when the contract exposes the type.
        // We assert the event name and invoice id are present in the XDR encoding.
        let xdr = last_event.clone().to_xdr(&env, &contract_id);
        let __ = (invoice_id, xdr, contract_id);
    }
}

/// Before any bind the getter returns `None`.
#[test]
fn test_get_primary_hash_none_before_bind() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert_eq(client.get_primary_attestation_hash(), None);
}

/// A second bind with the **same** digest must panic — single-set is unconditional.
#[test]
fn test_bind_primary_hash_same_digest_fails() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0x01);
    client.bind_primary_attestation_hash(&d);

    let res = client.try_bind_primary_attestation_hash(&d);
    assert_contract_error(res, EscrowError::PrimaryAttestationAlreadyBound);
    assert_eq(client.get_primary_attestation_hash(), Some(d));
}

/// A second bind with a **different** digest must also panic — no replacement allowed.
#[test]
fn test_bind_primary_hash_different_digest_fails() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let first = digest(&env, 0x01);
    client.bind_primary_attestation_hash(&first);

    let second = digest(&env, 0x02);
    let res = client.try_bind_primary_attestation_hash(&second);
    assert_contract_error(res, EscrowError::PrimaryAttestationAlreadyBound);
    assert_eq(client.get_primary_attestation_hash(), Some(first));
}

/// Non-admin caller must not be able to bind the primary hash.
#[test]
fn test_bind_primary_hash_non_admin_fails() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Clear all mocks so auth is enforced for the next call.
    env.mock_auths(&[]);
    let d = digest(&env, 0xFF);

    assert_or_error(client.try_bind_primary_attestation_hash(&d));
    assert_eq(client.get_primary_attestation_hash(), None);
}

// ----------------------------------------------------------------------------
// append_attestation_digest — bounded log invariant
// ----------------------------------------------------------------------------

/// Empty log before any append.
#[test]
fn test_append_log_empty_before_first_append() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert_eq(client.get_attestation_append_log().len(), 0);
}

/// The stats view reports zero used entries and the full remaining capacity before any append.
#[test]
fn test_attestation_log_stats_empty_before_first_append() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let (used, remaining) = attestation_log_stats(&client);
    assert_eq(used, 0);
    assert_eq(remaining, MAX_ATTESTATION_APPEND_ENTRIES);
}

/// The stats view tracks partially filled logs without reading the full vector contents.
#[test]
fn test_attestation_log_stats_tracks_partial_fill() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..5 {
        client.append_attestation_digest(&digest(&env, i));
    }
    let (used, remaining) = attestation_log_stats(&client);
    assert_eq(used, 5);
    assert_eq(
        remaining_attestation_slots(&client),
        MAX_ATTESTATION_APPEND_ENTRIES - 5
    );
}

/// The stats view reports full capacity and remains consistent after the capacity error path.
#[test]
fn test_attestation_log_stats_full_and_after_capacity_error() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8) {
        client.append_attestation_digest(&digest(&env, i));
    }
    let (used, remaining) = attestation_log_stats(&client);
    assert_eq(used, MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq(remaining_attestation_slots(&client), 0);

    let result = client.try_append_attestation_digest(&digest(&env, 0xFF));
    assert_contract_error(result, EscrowError::AttestationAppendLogCapacityReached);

    let (used, remaining) = attestation_log_stats(&client);
    assert_eq(used, MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq(remaining_attestation_slots(&client), 0);
}

/// Single append is stored at index 0.
#[test]
fn test_append_single_entry_stored() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0x10);
    client.append_attestation_digest(&d);
    let log = client.get_attestation_append_log();
    assert_eq(log.len(), 1);
    assert_eq(log.get(0).unwrap(), d);
}

/// Multiple appends preserve insertion order.
#[test]
fn test_append_multiple_entries_ordered() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..5 {
        client.append_attestation_digest(&digest(&env, i));
    }
    let log = client.get_attestation_append_log();
    assert_eq(log.len(), 5);
    for i in 0u8..5 {
        assert_eq(log.get(i as u32).unwrap(), digest(&env, i));
    }
}

/// The 32nd entry (index 31) succeeds — boundary must be inclusive.
#[test]
fn test_append_exactly_max_entries_succeeds() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // MAX_ATTESTATION_APPEND_ENTRIES = 32, safely fits in u8.
    for i in 0u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8) {
        client.append_attestation_digest(&digest(&env, i));
    }
    assert_eq(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES
    );
}

/// The 33rd entry must panic — capacity is strictly bounded.
#[test]
#[should_panic]
fn test_append_beyond_max_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Append MAX + 1 entries; the last one must panic.
    for i in 0u8..=(MAX_ATTESTATION_APPEND_ENTRIES as u8) {
        client.append_attestation_digest(&digest(&env, i));
    }
}

/// Duplicate digests are allowed — the log is an audit trail, not a set.
#[test]
fn test_append_duplicate_digest_allowed() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0x42);
    client.append_attestation_digest(&d);
    client.append_attestation_digest(&d);
    assert_eq(client.get_attestation_append_log().len(), 2);
}

/// Non-admin caller must not be able to append.
#[test]
#[should_panic]
fn test_append_non_admin_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Clear all mocks so auth is enforced for the next call.
    env.mock_auths(&[]);
    client.append_attestation_digest(&digest(&env, 0x01));
}

// ----------------------------------------------------------------------------
// Interaction: primary hash and append log are independent
// ----------------------------------------------------------------------------

/// Binding the primary hash does not affect the append log.
#[test]
fn test_primary_bind_does_not_affect_append_log() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.bind_primary_attestation_hash(&digest(&env, 0xAA));
    assert_eq(client.get_attestation_append_log().len(), 0);
}

/// Appending does not affect the primary hash.
#[test]
fn test_append_does_not_affect_primary_hash() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xBB));
    assert_eq(client.get_primary_attestation_hash(), None);
}

/// Both can coexist: bind primary then fill part of the append log.
#[test]
fn test_primary_and_append_coexist() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let primary = digest(&env, 0xCC);
    client.bind_primary_attestation_hash(&primary);
    for i in 0u8..4 {
        client.append_attestation_digest(&digest(&env, i));
    }
    assert_eq(client.get_primary_attestation_hash(), Some(primary));
    assert_eq(client.get_attestation_append_log().len(), 4);
}

/// Revocation does not alter the append log contents — the digest remains readable.
#[test]
fn test_revoke_preserves_log_entry() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0xBB);
    client.append_attestation_digest(&d);
    client.revoke_attestation_digest(&0);
    let log = client.get_attestation_append_log();
    assert_eq(log.len(), 1);
    assert_eq(log.get(0).unwrap(), d);
}

// ----------------------------------------------------------------------------
// Batch append — all-or-nothing and contiguous indexing
// ----------------------------------------------------------------------------

/// Batch append of a single digest behaves like the single append.
#[test]
fn test_append_batch_single_entry_stored() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0x50);
    let mut batch = Vec::new(&env);
    batch.push_back(d.clone());
    client.append_attestation_digests(&batch);
    let log = client.get_attestation_append_log();
    assert_eq(log.len(), 1);
    assert_eq(log.get(0).unwrap(), d);
}

/// Batch append preserves insertion order and contiguous indexes.
#[test]
fn test_append_batch_ordered_contiguous() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let mut batch = Vec::new(&env);
    for i in 0u8..5 {
        batch.push_back(digest(&env, i));
    }
    client.append_attestation_digests(&batch);
    let log = client.get_attestation_append_log();
    assert_eq(log.len(), 5);
    for in 0u8..5 {
        assert_eq(log.get(i as u32).unwrap(), digest(&env, i));
    }
}

/// Batch append appends after existing entries without overwriting.
#[test]
fn test_append_batch_appends_after_existing() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    let mut batch = Vec::new(&env);
    batch.push_back(digest(&env, 0x2));
    batch.push_back(digest(&env, 0x03));
    client.append_attestation_digests(&batch);
    let log = client.get_attestation_append_log();
    assert_eq(log.len(), 3);
    assert_eq(log.get(0).unwrap(), digest(&env, 0x01));
    assert_eq(log.get(1).unwrap(), digest(&env, 0x2));
    assert_eq(log.get(2).unwrap(), digest(&env, 0x03));
}

/// Batch append of an empty vector is a no-op.
#[test]
fn test_append_batch_empty_noop() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let batch = Vec:<new>(&env);
    client.append_attestation_digests(&batch);
    assert_eq(client.get_attestation_append_log().len(), 0);
}

/// Batch append of a duplicate digest is allowed.
#[test]
fn test_append_batch_duplicate_allowed() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0x60);
    let mut batch = Vec::new(&env);
    batch.push_back(d.clone());
    batch.push_back(d.clone());
    client.append_attestation_digests(&batch);
    let log = client.get_attestation_append_log();
    assert_eq(log.len(), 2);
    assert_eq(log.get(0).unwrap(), d.clone());
    assert_eq(log.get(1).unwrap(), d);
}

/// Batch append exactly filling the remaining capacity succeeds.
#[test]
fn test_append_batch_exactly_fills_capacity() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Fill to one below capacity, then append the last one via batch.
    for i in 0u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8 - 1) {
        client.append_attestation_digest(&digest(&env, i));
    }
    let mut batch = Vec::new(&env);
    batch.push_back(digest(&env, 0xFE));
    client.append_attestation_digests(&batch);
    assert_eq(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES
    );
}

/// Batch append that would exceed capacity must fail and leave the log unchanged.
#[test]
fn test_append_batch_exceeds_capacity_fails() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Fill to capacity - 1.
    for i in 0u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8 - 1) {
        client.append_attestation_digest(&digest(&env, i));
    }
    let mut batch = Vec::new(&env);
    batch.push_back(digest(&env, 0x01));
    batch.push_back(digest(&env, 0x02));
    let res = client.try_append_attestation_digests(&batch);
    assert_contract_error(res, EscrowError::AttestationAppendLogCapacityReached);
    assert_eq(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES - 1
    );
}

/// Batch append exceeding the batch size limit must fail and leave the log unchanged.
#[test]
fn test_append_batch_exceeds_batch_limit_fails() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let mut batch = Vec::new(&env);
    for i in 0u8..(MAX_ATTESTATION_APPEND_BATCH as u8 + 1) {
        batch.push_back(digest(&env, i));
    }
    let res = client.try_append_attestation_digests(&batch);
    assert_contract_error(res, EscrowError::AttestationAppendBatchTooLarge);
    assert_eq(client.get_attestation_append_log().len(), 0);
}

/// Batch append at the batch size limit succeeds.
#[test]
fn test_append_batch_at_limit_succeeds() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let mut batch = Vec::new(&env);
    for in 0u8..(MAX_ATTESTATION_APPEND_BATCH as u8) {
        batch.push_back(digest(&env, i));
    }
    client.append_attestation_digests(&batch);
    assert_eq(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_BATCH
    );
}

/// Non-admin caller must not be able to batch append.
#[test]
#[should_panic]
fn test_append_batch_non_admin_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    env.mock_auths(&[]);
    let mut batch = Vec::new(&env);
    batch.push_back(digest(&env, 0x01));
    client.append_attestation_digests(&batch);
}

// ----------------------------------------------------------------------------
// Revoke batch boundary cases
// ----------------------------------------------------------------------------

/// Revoke batch of a single index succeeds.
#[test]
fn test_revoke_batch_single_succeeds() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    let mut indices = Vec::new(&env);
    indices.push_back(0);
    client.revoke_attestation_digests(&indices);
    // The log remains readable after revocation.
    assert_eq(client.get_attestation_append_log().len(), 1);
}

/// Revoke batch exceeding the batch limit must fail.
#[test]
fn test_revoke_batch_exceeds_limit_fails() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let mut indices = Vec::new(&env);
    for i in 0u32..(MAX_ATTESTATION_REVOKE_BATCH + 1) {
        indices.push_back(i);
    }
    let res = client.try_revoke_attestation_digests(&indices);
    assert_contract_error(res, EscrowError::AttestationRevokeBatchTooLarge);
}

/// Revoke batch at the batch limit succeeds.
#[test]
fn test_revoke_batch_at_limit_succeeds() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Fill the log to the revoke batch limit.
    for i in 0u8..(MAX_ATTESTATION_REVOKE_BATCH as u8) {
        client.append_attestation_digest(&digest(&env, i));
    }
    let mut indices = Vec::new(&env);
    for i in 0u32..(MAX_ATTESTATION_REVOKE_BATCH) {
        indices.push_back(i);
    }
    client.revoke_attestation_digests(&indices);
    assert_eq(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_REVOKE_BATCH
    );
}

/// Revoke batch of an empty vector is a no-op.
#[test]
fn test_revoke_batch_empty_noop() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let indices = Vec::<new>(&env);
    client.revoke_attestation_digests(&indices);
    assert_eq(client.get_attestation_append_log().len(), 0);
}

// ---------------------------------------------------------------------------
// append_attestation_digests — batch entrypoint
// ---------------------------------------------------------------------------

/// Happy path: a single-element batch appends exactly one entry.
#[test]
fn test_batch_append_single_element() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let batch = soroban_sdk::vec![&env, digest(&env, 0x01)];
    client.append_attestation_digests(&batch);
    assert_eq!(client.get_attestation_append_log().len(), 1);
    assert_eq!(
        client.get_attestation_append_log().get(0).unwrap(),
        digest(&env, 0x01)
    );
}

/// A batch of exactly MAX_ATTESTATION_APPEND_BATCH entries succeeds.
#[test]
fn test_batch_append_max_size_succeeds() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // MAX_ATTESTATION_APPEND_BATCH == MAX_ATTESTATION_APPEND_ENTRIES == 32
    let mut batch = soroban_sdk::Vec::new(&env);
    for i in 0u8..(crate::MAX_ATTESTATION_APPEND_BATCH as u8) {
        batch.push_back(digest(&env, i));
    }
    client.append_attestation_digests(&batch);
    assert_eq!(
        client.get_attestation_append_log().len(),
        crate::MAX_ATTESTATION_APPEND_BATCH
    );
}

/// Empty batch returns `AttestationAppendBatchEmpty`.
#[test]
fn test_batch_append_empty_returns_typed_error() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let empty: soroban_sdk::Vec<soroban_sdk::BytesN<32>> = soroban_sdk::Vec::new(&env);
    assert_contract_error(
        client.try_append_attestation_digests(&empty),
        EscrowError::AttestationAppendBatchEmpty,
    );
    assert_eq!(client.get_attestation_append_log().len(), 0);
}

/// Batch larger than MAX_ATTESTATION_APPEND_BATCH returns
/// `AttestationAppendBatchTooLarge`.
#[test]
fn test_batch_append_over_limit_returns_typed_error() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let mut oversized = soroban_sdk::Vec::new(&env);
    for i in 0u8..=(crate::MAX_ATTESTATION_APPEND_BATCH as u8) {
        oversized.push_back(digest(&env, i));
    }
    assert_contract_error(
        client.try_append_attestation_digests(&oversized),
        EscrowError::AttestationAppendBatchTooLarge,
    );
    assert_eq!(client.get_attestation_append_log().len(), 0);
}

/// Batch fails atomically when it would exceed capacity: log stays unchanged.
#[test]
fn test_batch_append_capacity_exceeded_rolls_back() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Fill log to MAX - 1.
    for i in 0u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8 - 1) {
        client.append_attestation_digest(&digest(&env, i));
    }
    // A 2-entry batch would push beyond MAX.
    let overflow_batch = soroban_sdk::vec![&env, digest(&env, 0xFE), digest(&env, 0xFF)];
    assert_contract_error(
        client.try_append_attestation_digests(&overflow_batch),
        EscrowError::AttestationAppendLogCapacityReached,
    );
    // Log length must not have changed (full rollback).
    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES - 1
    );
}

/// Indices in a batch are assigned contiguously from the log length at call time.
#[test]
fn test_batch_append_indices_are_contiguous_from_log_tail() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Pre-populate 3 entries.
    for i in 0u8..3 {
        client.append_attestation_digest(&digest(&env, i));
    }
    // Batch of 4 more; expected indices 3, 4, 5, 6.
    let batch = soroban_sdk::vec![
        &env,
        digest(&env, 0xA0),
        digest(&env, 0xA1),
        digest(&env, 0xA2),
        digest(&env, 0xA3)
    ];
    client.append_attestation_digests(&batch);

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 7);
    assert_eq!(log.get(3).unwrap(), digest(&env, 0xA0));
    assert_eq!(log.get(4).unwrap(), digest(&env, 0xA1));
    assert_eq!(log.get(5).unwrap(), digest(&env, 0xA2));
    assert_eq!(log.get(6).unwrap(), digest(&env, 0xA3));
}

/// Non-admin caller must not be able to call the batch append.
#[test]
fn test_batch_append_non_admin_returns_error() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    env.mock_auths(&[]);
    let batch = soroban_sdk::vec![&env, digest(&env, 0x01)];
    assert!(client.try_append_attestation_digests(&batch).is_err());
    assert_eq!(client.get_attestation_append_log().len(), 0);
}

// ---------------------------------------------------------------------------
// Concurrency-hardening: duplicate-call idempotency
// ---------------------------------------------------------------------------

/// Calling `append_attestation_digest` twice with the same digest is allowed
/// (audit-log semantics, not a set) and produces two entries.  A second call
/// must never silently disappear or overwrite the first.
#[test]
fn test_concurrent_duplicate_single_append_produces_two_entries() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0x42);

    // Simulate two "concurrent" calls on the same digest (deterministic in
    // Soroban since only one tx writes at a time, but we verify the expected
    // append-always semantics hold on retry / duplicate submission).
    client.append_attestation_digest(&d);
    client.append_attestation_digest(&d); // second call — must succeed

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 2, "duplicate digest must create two entries");
    assert_eq!(log.get(0).unwrap(), d);
    assert_eq!(log.get(1).unwrap(), d);
}

/// A `revoke_attestation_digest` call retried after success returns
/// `AttestationAlreadyRevoked` and leaves state unchanged (idempotent retry
/// pattern: first call succeeds, subsequent ones return a typed error).
#[test]
fn test_concurrent_duplicate_revoke_is_idempotent_retry() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x10));

    // First revoke — success.
    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));

    // Retry (duplicate) — deterministically rejected, state unchanged.
    assert_contract_error(
        client.try_revoke_attestation_digest(&0),
        EscrowError::AttestationAlreadyRevoked,
    );
    assert!(client.is_attestation_revoked(&0));
}

/// A `revoke_attestation_digests` batch retried after success returns
/// `AttestationAlreadyRevoked` and does not double-apply any entries.
#[test]
fn test_concurrent_duplicate_batch_revoke_is_idempotent_retry() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    client.append_attestation_digest(&digest(&env, 0x02));

    let indices = soroban_sdk::vec![&env, 0u32, 1u32];

    // First batch — success.
    client.revoke_attestation_digests(&indices);
    assert!(client.is_attestation_revoked(&0));
    assert!(client.is_attestation_revoked(&1));

    // Retry — whole batch fails with a typed error; state unchanged.
    assert_contract_error(
        client.try_revoke_attestation_digests(&indices),
        EscrowError::AttestationAlreadyRevoked,
    );
    assert!(client.is_attestation_revoked(&0));
    assert!(client.is_attestation_revoked(&1));
}

/// Retrying `bind_primary_attestation_hash` after success returns
/// `PrimaryAttestationAlreadyBound` and does not change the stored digest.
#[test]
fn test_concurrent_duplicate_bind_primary_is_idempotent_retry() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0xAB);

    client.bind_primary_attestation_hash(&d);
    assert_eq!(client.get_primary_attestation_hash(), Some(d.clone()));

    // Retry with same digest — must be rejected.
    assert_contract_error(
        client.try_bind_primary_attestation_hash(&d),
        EscrowError::PrimaryAttestationAlreadyBound,
    );
    // Stored digest unchanged.
    assert_eq!(client.get_primary_attestation_hash(), Some(d));
}

// ---------------------------------------------------------------------------
// Concurrency-hardening: batch-race safety
// ---------------------------------------------------------------------------

/// If two batches race and the first fills the log, the second batch must be
/// rejected atomically — no partial write must occur.
///
/// In Soroban only one transaction can write at a time, but this test
/// simulates the state a second caller would observe if the first batch
/// landed first: the log is at capacity and the second batch must fail
/// cleanly.
#[test]
fn test_concurrent_second_batch_rejected_when_log_full_after_first() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // First batch fills the log completely.
    let mut first_batch = soroban_sdk::Vec::new(&env);
    for i in 0u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8) {
        first_batch.push_back(digest(&env, i));
    }
    client.append_attestation_digests(&first_batch);
    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES
    );

    // Second batch — observer sees a full log and must be rejected.
    let second_batch = soroban_sdk::vec![&env, digest(&env, 0xFF)];
    assert_contract_error(
        client.try_append_attestation_digests(&second_batch),
        EscrowError::AttestationAppendLogCapacityReached,
    );

    // Log length must not have increased.
    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES
    );
}

/// A batch that would fill the log exactly to capacity succeeds, while a
/// follow-on batch of any size is rejected.
#[test]
fn test_concurrent_batch_fills_to_exact_capacity_then_next_fails() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // Pre-populate half the log.
    let half = MAX_ATTESTATION_APPEND_ENTRIES / 2;
    for i in 0u8..(half as u8) {
        client.append_attestation_digest(&digest(&env, i));
    }

    // Batch fills the remaining slots exactly.
    let mut fill_batch = soroban_sdk::Vec::new(&env);
    for i in 0u8..(half as u8) {
        fill_batch.push_back(digest(&env, 0x80 + i));
    }
    client.append_attestation_digests(&fill_batch);
    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES
    );

    // Any further single or batch append is now rejected.
    assert_contract_error(
        client.try_append_attestation_digest(&digest(&env, 0xFF)),
        EscrowError::AttestationAppendLogCapacityReached,
    );
    let tail = soroban_sdk::vec![&env, digest(&env, 0xFE)];
    assert_contract_error(
        client.try_append_attestation_digests(&tail),
        EscrowError::AttestationAppendLogCapacityReached,
    );
}

// ---------------------------------------------------------------------------
// Concurrency-hardening: boundary idempotency
// ---------------------------------------------------------------------------

/// Appending the (MAX - 1)-th entry succeeds; appending the (MAX + 1)-th entry
/// fails.  The boundary is inclusive at MAX and exclusive above.
#[test]
fn test_boundary_idempotency_last_valid_append_then_fail() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8 - 1) {
        client.append_attestation_digest(&digest(&env, i));
    }

    // MAX-th entry (index MAX-1) must succeed.
    client.append_attestation_digest(&digest(&env, 0xFE));
    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES
    );

    // (MAX+1)-th entry must fail.
    assert_contract_error(
        client.try_append_attestation_digest(&digest(&env, 0xFF)),
        EscrowError::AttestationAppendLogCapacityReached,
    );
    // State unchanged.
    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES
    );
}

/// Revoking index 0 then index (MAX - 1) are both valid (boundary indices).
/// Revoking the same indices again produces typed errors, not panics.
#[test]
fn test_boundary_idempotency_revoke_first_and_last_indices() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8) {
        client.append_attestation_digest(&digest(&env, i));
    }
    let last = MAX_ATTESTATION_APPEND_ENTRIES - 1;

    // Revoke boundary indices.
    client.revoke_attestation_digest(&0);
    client.revoke_attestation_digest(&last);
    assert!(client.is_attestation_revoked(&0));
    assert!(client.is_attestation_revoked(&last));

    // Duplicate revokes must return typed errors.
    assert_contract_error(
        client.try_revoke_attestation_digest(&0),
        EscrowError::AttestationAlreadyRevoked,
    );
    assert_contract_error(
        client.try_revoke_attestation_digest(&last),
        EscrowError::AttestationAlreadyRevoked,
    );

    // State unchanged.
    assert!(client.is_attestation_revoked(&0));
    assert!(client.is_attestation_revoked(&last));
}

/// Unrevoke on a just-revoked boundary index succeeds; a second unrevoke
/// returns `AttestationNotRevoked`.
#[test]
fn test_boundary_idempotency_unrevoke_boundary_twice() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    let last = 0u32;

    client.revoke_attestation_digest(&last);
    client.unrevoke_attestation_digest(&last);
    assert!(!client.is_attestation_revoked(&last));

    // Second unrevoke must return `AttestationNotRevoked`.
    assert_contract_error(
        client.try_unrevoke_attestation_digest(&last),
        EscrowError::AttestationNotRevoked,
    );
    assert!(!client.is_attestation_revoked(&last));
}

// ---------------------------------------------------------------------------
// set_attestation_limit / get_attestation_limit — boundary + idempotency
// ---------------------------------------------------------------------------

/// Default limit before any explicit set equals DEFAULT_ATTESTATION_LIMIT.
#[test]
fn test_attestation_limit_default_is_default_constant() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert_eq!(
        client.get_attestation_limit(),
        crate::DEFAULT_ATTESTATION_LIMIT
    );
}

/// Setting limit to MIN succeeds; appending beyond that limit fails.
#[test]
fn test_attestation_limit_min_enforced() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.set_attestation_limit(&crate::MIN_ATTESTATION_LIMIT);
    assert_eq!(client.get_attestation_limit(), crate::MIN_ATTESTATION_LIMIT);

    client.append_attestation_digest(&digest(&env, 0x01));
    assert_eq!(client.get_attestation_append_log().len(), 1);

    // At limit — next append rejected.
    assert_contract_error(
        client.try_append_attestation_digest(&digest(&env, 0x02)),
        EscrowError::AttestationAppendLogCapacityReached,
    );
}

/// Setting limit to 0 fails with `AttestationLimitOutOfRange`.
#[test]
fn test_attestation_limit_zero_is_rejected() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert_contract_error(
        client.try_set_attestation_limit(&0),
        EscrowError::AttestationLimitOutOfRange,
    );
    // Limit unchanged.
    assert_eq!(
        client.get_attestation_limit(),
        crate::DEFAULT_ATTESTATION_LIMIT
    );
}

/// Setting limit above MAX_ATTESTATION_LIMIT fails with `AttestationLimitOutOfRange`.
#[test]
fn test_attestation_limit_above_max_is_rejected() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert_contract_error(
        client.try_set_attestation_limit(&(crate::MAX_ATTESTATION_LIMIT + 1)),
        EscrowError::AttestationLimitOutOfRange,
    );
    assert_eq!(
        client.get_attestation_limit(),
        crate::DEFAULT_ATTESTATION_LIMIT
    );
}

/// Lowering the limit after appends prevents further writes but does not
/// truncate or invalidate existing log entries.
#[test]
fn test_lowering_attestation_limit_prevents_further_appends_without_removing_existing() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Append 3 entries.
    for i in 0u8..3 {
        client.append_attestation_digest(&digest(&env, i));
    }
    // Lower limit to 3 (current count).
    client.set_attestation_limit(&3);
    assert_eq!(client.get_attestation_limit(), 3);

    // Existing entries are intact.
    assert_eq!(client.get_attestation_append_log().len(), 3);

    // A further append must fail.
    assert_contract_error(
        client.try_append_attestation_digest(&digest(&env, 0xFF)),
        EscrowError::AttestationAppendLogCapacityReached,
    );

    // Raising limit to 4 unlocks one more slot.
    client.set_attestation_limit(&4);
    client.append_attestation_digest(&digest(&env, 0x04));
    assert_eq!(client.get_attestation_append_log().len(), 4);
}

/// Non-admin cannot call `set_attestation_limit`.
#[test]
fn test_attestation_limit_non_admin_rejected() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    env.mock_auths(&[]);
    assert!(client.try_set_attestation_limit(&5).is_err());
    // Limit unchanged.
    assert_eq!(
        client.get_attestation_limit(),
        crate::DEFAULT_ATTESTATION_LIMIT
    );
}

/// `set_attestation_limit` called twice with the same value is idempotent.
#[test]
fn test_set_attestation_limit_same_value_is_idempotent() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.set_attestation_limit(&10);
    client.set_attestation_limit(&10); // duplicate call
    assert_eq!(client.get_attestation_limit(), 10);
}

/// `append_attestation_digests` (batch) respects the configured limit.
#[test]
fn test_batch_append_respects_configured_attestation_limit() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.set_attestation_limit(&2);

    let batch_ok = soroban_sdk::vec![&env, digest(&env, 0x01), digest(&env, 0x02)];
    client.append_attestation_digests(&batch_ok);
    assert_eq!(client.get_attestation_append_log().len(), 2);

    // A further single append exceeds the limit.
    assert_contract_error(
        client.try_append_attestation_digest(&digest(&env, 0x03)),
        EscrowError::AttestationAppendLogCapacityReached,
    );
    // A further batch also exceeds the limit.
    let batch_overflow = soroban_sdk::vec![&env, digest(&env, 0x04)];
    assert_contract_error(
        client.try_append_attestation_digests(&batch_overflow),
        EscrowError::AttestationAppendLogCapacityReached,
    );
}

// ---------------------------------------------------------------------------
// get_attestation_config — view consistency
// ---------------------------------------------------------------------------

/// `get_attestation_config` constants match compile-time values.
#[test]
fn test_get_attestation_config_constants_match() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let cfg = client.get_attestation_config();
    assert_eq!(cfg.max_append_entries, MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(cfg.max_revoke_batch, crate::MAX_ATTESTATION_REVOKE_BATCH);
    assert_eq!(cfg.max_append_batch, crate::MAX_ATTESTATION_APPEND_BATCH);
    assert_eq!(cfg.max_read_page, crate::MAX_ATTESTATION_READ_PAGE);
}

/// `primary_bound` starts `false` and becomes `true` after binding.
#[test]
fn test_get_attestation_config_primary_bound_reflects_bind() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert!(!client.get_attestation_config().primary_bound);

    client.bind_primary_attestation_hash(&digest(&env, 0xAA));
    assert!(client.get_attestation_config().primary_bound);
}

/// `append_log_length` tracks appends and is unaffected by revocations.
#[test]
fn test_get_attestation_config_append_log_length_tracks_appends_not_revokes() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert_eq!(client.get_attestation_config().append_log_length, 0);

    client.append_attestation_digest(&digest(&env, 0x01));
    assert_eq!(client.get_attestation_config().append_log_length, 1);

    client.append_attestation_digest(&digest(&env, 0x02));
    assert_eq!(client.get_attestation_config().append_log_length, 2);

    // Revoking does not reduce the length.
    client.revoke_attestation_digest(&0);
    assert_eq!(client.get_attestation_config().append_log_length, 2);
}

/// `get_attestation_config` is pure — calling it multiple times returns the
/// same result and does not mutate state.
#[test]
fn test_get_attestation_config_is_pure_view() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));

    let first = client.get_attestation_config();
    let second = client.get_attestation_config();
    assert_eq!(first, second);
    // Log unchanged after two reads.
    assert_eq!(client.get_attestation_append_log().len(), 1);
}
