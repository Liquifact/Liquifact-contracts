//! Attestation tests: `bind_primary_attestation_hash` (single-set) and
//! `append_attestation_digest` (bounded by [`MAX_ATTESTATION_APPEND_ENTRIES`]).
//!
//! These tests prove the two chain-anchor invariants:
//! 1. The primary hash is **write-once** — a second bind panics regardless of the digest value.
//! 2. The append log is **capacity-bounded** — the 33rd entry panics; the 32nd succeeds.
//!
//! Neither entrypoint stores ZK proofs or performs off-chain verification. They record a
//! 32-byte digest (e.g. SHA-256 of an IPFS CID or a KYC/KYB document bundle) so that
//! off-chain verifiers can confirm the on-chain anchor matches their document set.

use super::*;
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
        Err(Ok(error)) => assert_eq!(error, Error::from_contract_error(expected_code)),
        Err(Err(InvokeError::Contract(code))) => assert_eq!(code, expected_code),
        other => panic!("expected ContractError({expected_code}), got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn attestation_log_stats(client: &LiquifactEscrowClient<'_>) -> (u32, u32) {
    let used = client.get_attestation_append_log().len();
    (used, MAX_ATTESTATION_APPEND_ENTRIES.saturating_sub(used))
}

/// The number of free attestation append-log slots remaining.
fn remaining_attestation_slots(client: &LiquifactEscrowClient<'_>) -> u32 {
    let used = client.get_attestation_append_log().len();
    MAX_ATTESTATION_APPEND_ENTRIES.saturating_sub(used)
}

// ---------------------------------------------------------------------------
// bind_primary_attestation_hash — single-set invariant
// ---------------------------------------------------------------------------

/// Happy path: first bind succeeds and is readable via the getter.
#[test]
#[ignore = "upstream latent: escrow API/test drift"]
fn test_bind_primary_hash_stores_and_reads() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0xAB);
    client.bind_primary_attestation_hash(&d);
    // Assert the `att_bind` event was emitted (capture before additional calls)
    let all_events = env.events().all();
    let all_events_list = all_events.events();
    let last_event = all_events_list.last().unwrap();
    let contract_id = client.address.clone();

    assert_eq!(client.get_primary_attestation_hash(), Some(d.clone()));
    let invoice_id = client.get_escrow().invoice_id;
    assert_eq!(
        last_event.clone(),
        crate::PrimaryAttestationBound {
            name: symbol_short!("att_bind"),
            invoice_id,
            digest: d.clone(),
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Before any bind the getter returns `None`.
#[test]
fn test_get_primary_hash_none_before_bind() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert_eq!(client.get_primary_attestation_hash(), None);
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
    assert_eq!(client.get_primary_attestation_hash(), Some(d));
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
    assert_eq!(client.get_primary_attestation_hash(), Some(first));
}

/// Non-admin caller must not be able to bind the primary hash.
#[test]
fn test_bind_primary_hash_non_admin_fails() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Clear all mocks so auth is enforced for the next call.
    env.mock_auths(&[]);
    let d = digest(&env, 0xFF);

    assert!(client.try_bind_primary_attestation_hash(&d).is_err());
    assert_eq!(client.get_primary_attestation_hash(), None);
}

// ---------------------------------------------------------------------------
// append_attestation_digest — bounded log invariant
// ---------------------------------------------------------------------------

/// Empty log before any append.
#[test]
fn test_append_log_empty_before_first_append() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert_eq!(client.get_attestation_append_log().len(), 0);
}

/// The stats view reports zero used entries and the full remaining capacity before any append.
#[test]
fn test_attestation_log_stats_empty_before_first_append() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let (used, remaining) = attestation_log_stats(&client);
    assert_eq!(used, 0);
    assert_eq!(remaining, MAX_ATTESTATION_APPEND_ENTRIES);
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
    assert_eq!(used, 5);
    assert_eq!(
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
    assert_eq!(used, MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(remaining_attestation_slots(&client), 0);

    let result = client.try_append_attestation_digest(&digest(&env, 0xFF));
    assert_contract_error(result, EscrowError::AttestationAppendLogCapacityReached);

    let (used, remaining) = attestation_log_stats(&client);
    assert_eq!(used, MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(remaining_attestation_slots(&client), 0);
}

/// Single append is stored at index 0.
#[test]
fn test_append_single_entry_stored() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0x10);
    client.append_attestation_digest(&d);
    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 1);
    assert_eq!(log.get(0).unwrap(), d);
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
    assert_eq!(log.len(), 5);
    for i in 0u8..5 {
        assert_eq!(log.get(i as u32).unwrap(), digest(&env, i));
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
    assert_eq!(
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
    // Append MAX+1 entries; the last one must panic.
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
    assert_eq!(client.get_attestation_append_log().len(), 2);
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

// ---------------------------------------------------------------------------
// Interaction: primary hash and append log are independent
// ---------------------------------------------------------------------------

/// Binding the primary hash does not affect the append log.
#[test]
fn test_primary_bind_does_not_affect_append_log() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.bind_primary_attestation_hash(&digest(&env, 0xAA));
    assert_eq!(client.get_attestation_append_log().len(), 0);
}

/// Appending does not affect the primary hash.
#[test]
fn test_append_does_not_affect_primary_hash() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xBB));
    assert_eq!(client.get_primary_attestation_hash(), None);
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
    assert_eq!(client.get_primary_attestation_hash(), Some(primary));
    assert_eq!(client.get_attestation_append_log().len(), 4);
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
    assert_eq!(log.len(), 1);
    assert_eq!(log.get(0).unwrap(), d);
}

/// Revocation does not affect the primary attestation hash.
#[test]
fn test_revoke_does_not_affect_primary_hash() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let primary = digest(&env, 0xCC);
    client.bind_primary_attestation_hash(&primary);
    client.append_attestation_digest(&digest(&env, 0xDD));
    client.revoke_attestation_digest(&0);
    assert_eq!(client.get_primary_attestation_hash(), Some(primary));
}

// ---------------------------------------------------------------------------
// revoke_attestation_digest — typed EscrowError edge cases (issue #378)
// ---------------------------------------------------------------------------

/// index > log.len() (large value) returns `AttestationIndexOutOfRange`.
#[test]
fn test_revoke_large_index_out_of_range() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    assert_contract_error(
        client.try_revoke_attestation_digest(&99),
        EscrowError::AttestationIndexOutOfRange,
    );
}

/// Revoking the first entry (index 0) in a multi-entry log succeeds.
#[test]
fn test_revoke_first_entry() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    client.append_attestation_digest(&digest(&env, 0x02));
    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
}

/// Revoking the last entry in a multi-entry log succeeds.
#[test]
fn test_revoke_last_entry() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..3 {
        client.append_attestation_digest(&digest(&env, i));
    }
    client.revoke_attestation_digest(&2);
    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(client.is_attestation_revoked(&2));
}

/// Third revoke attempt on same index still returns `AttestationAlreadyRevoked`.
#[test]
fn test_repeated_revoke_returns_typed_error() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x10));
    client.revoke_attestation_digest(&0);
    assert_contract_error(
        client.try_revoke_attestation_digest(&0),
        EscrowError::AttestationAlreadyRevoked,
    );
    // A second retry also returns the same typed error.
    assert_contract_error(
        client.try_revoke_attestation_digest(&0),
        EscrowError::AttestationAlreadyRevoked,
    );
}

/// Non-admin `try_revoke_attestation_digest` returns an authorization error.
#[test]
fn test_revoke_non_admin_returns_error() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xFF));
    env.mock_auths(&[]);
    // Any error (not Ok) satisfies the auth-rejection requirement.
    assert!(client.try_revoke_attestation_digest(&0).is_err());
}

// ---------------------------------------------------------------------------
// unrevoke_attestation_digest — reversal of revocation
// ---------------------------------------------------------------------------

/// Happy path: revoke then unrevoke index 0; confirm `is_attestation_revoked`
/// flips back to `false` and the digest remains readable.
#[test]
fn test_unrevoke_single_entry() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0xAA);
    client.append_attestation_digest(&d);

    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));

    client.unrevoke_attestation_digest(&0);
    assert!(!client.is_attestation_revoked(&0));

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 1);
    assert_eq!(log.get(0).unwrap(), d);
}

/// Unrevoke emits `att_unrev` with the correct index.
#[test]
fn test_unrevoke_emits_event() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let contract_id = client.address.clone();
    let d = digest(&env, 0xBB);
    client.append_attestation_digest(&d);
    client.revoke_attestation_digest(&0);

    client.unrevoke_attestation_digest(&0);

    let all_events = env.events().all();
    let invoice_id = client.get_escrow().invoice_id;
    assert_eq!(
        all_events.events().last().unwrap().clone(),
        AttestationDigestUnrevoked {
            name: symbol_short!("att_unrev"),
            invoice_id,
            index: 0,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Unrevoking an index beyond the current log length returns
/// `AttestationIndexOutOfRange`.
#[test]
fn test_unrevoke_out_of_range() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Empty log — index 0 is out of range.
    assert_contract_error(
        client.try_unrevoke_attestation_digest(&0),
        EscrowError::AttestationIndexOutOfRange,
    );
}

/// Unrevoking an index equal to log length returns `AttestationIndexOutOfRange`.
#[test]
fn test_unrevoke_at_log_len() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x10));
    // log.len() == 1, index 1 is out of range.
    assert_contract_error(
        client.try_unrevoke_attestation_digest(&1),
        EscrowError::AttestationIndexOutOfRange,
    );
}

/// A large out-of-range index returns `AttestationIndexOutOfRange`.
#[test]
fn test_unrevoke_large_index_out_of_range() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    assert_contract_error(
        client.try_unrevoke_attestation_digest(&99),
        EscrowError::AttestationIndexOutOfRange,
    );
}

/// Unrevoking an index that was never revoked returns `AttestationNotRevoked`.
#[test]
fn test_unrevoke_not_revoked() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x42));
    assert_contract_error(
        client.try_unrevoke_attestation_digest(&0),
        EscrowError::AttestationNotRevoked,
    );
}

// ---------------------------------------------------------------------------
// get_attestation_state — read-only view
// ---------------------------------------------------------------------------

/// Returns default state when no attestation is set.
#[test]
fn test_get_attestation_state_default() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let state = client.get_attestation_state();
    assert_eq!(state.primary_hash, None);
    assert_eq!(state.append_log.len(), 0);
    assert_eq!(state.append_log_len, 0);
    assert_eq!(state.remaining_capacity, MAX_ATTESTATION_APPEND_ENTRIES);
}

/// Returns correct state after binding primary hash.
#[test]
fn test_get_attestation_state_with_primary() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0xAA);
    client.bind_primary_attestation_hash(&d);

    let state = client.get_attestation_state();
    assert_eq!(state.primary_hash, Some(d));
    assert_eq!(state.append_log.len(), 0);
    assert_eq!(state.append_log_len, 0);
    assert_eq!(state.remaining_capacity, MAX_ATTESTATION_APPEND_ENTRIES);
}

/// Returns correct state after appending digests.
#[test]
fn test_get_attestation_state_with_append_log() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..3 {
        client.append_attestation_digest(&digest(&env, i));
    }

    let state = client.get_attestation_state();
    assert_eq!(state.primary_hash, None);
    assert_eq!(state.append_log.len(), 3);
    assert_eq!(state.append_log_len, 3);
    assert_eq!(state.remaining_capacity, MAX_ATTESTATION_APPEND_ENTRIES - 3);

    // Verify log contents
    for i in 0u8..3 {
        assert_eq!(state.append_log.get(i as u32).unwrap(), digest(&env, i));
    }
}

/// Returns correct state with both primary and append log.
#[test]
fn test_get_attestation_state_full() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let primary = digest(&env, 0xCC);
    client.bind_primary_attestation_hash(&primary);

    for i in 0u8..5 {
        client.append_attestation_digest(&digest(&env, i));
    }

    let state = client.get_attestation_state();
    assert_eq!(state.primary_hash, Some(primary));
    assert_eq!(state.append_log.len(), 5);
    assert_eq!(state.append_log_len, 5);
    assert_eq!(state.remaining_capacity, MAX_ATTESTATION_APPEND_ENTRIES - 5);
}

/// Returns correct state after revocation.
#[test]
fn test_get_attestation_state_after_revoke() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..3 {
        client.append_attestation_digest(&digest(&env, i));
    }
    client.revoke_attestation_digest(&1);

    let state = client.get_attestation_state();
    assert_eq!(state.append_log.len(), 3);
    assert_eq!(state.append_log_len, 3);
    // Note: revocation doesn't remove from log, just marks as revoked
}

/// Returns correct state when append log is full.
#[test]
fn test_get_attestation_state_full_capacity() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8) {
        client.append_attestation_digest(&digest(&env, i));
    }

    let state = client.get_attestation_state();
    assert_eq!(state.append_log.len(), MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(state.append_log_len, MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(state.remaining_capacity, 0);
}

/// State view is consistent after multiple operations.
#[test]
fn test_get_attestation_state_consistency() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // Initial state
    let state1 = client.get_attestation_state();
    assert_eq!(state1.append_log_len, 0);

    // After append
    client.append_attestation_digest(&digest(&env, 0x01));
    let state2 = client.get_attestation_state();
    assert_eq!(state2.append_log_len, 1);

    // After revoke
    client.revoke_attestation_digest(&0);
    let state3 = client.get_attestation_state();
    assert_eq!(state3.append_log_len, 1); // Length unchanged

    // After unrevoke
    client.unrevoke_attestation_digest(&0);
    let state4 = client.get_attestation_state();
    assert_eq!(state4.append_log_len, 1); // Length still unchanged
}

/// State view correctly reflects capacity after unrevoke.
#[test]
fn test_get_attestation_state_capacity_after_unrevoke() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..3 {
        client.append_attestation_digest(&digest(&env, i));
    }

    let state1 = client.get_attestation_state();
    assert_eq!(
        state1.remaining_capacity,
        MAX_ATTESTATION_APPEND_ENTRIES - 3
    );

    client.revoke_attestation_digest(&1);
    let state2 = client.get_attestation_state();
    assert_eq!(
        state2.remaining_capacity,
        MAX_ATTESTATION_APPEND_ENTRIES - 3
    );

    client.unrevoke_attestation_digest(&1);
    let state3 = client.get_attestation_state();
    assert_eq!(
        state3.remaining_capacity,
        MAX_ATTESTATION_APPEND_ENTRIES - 3
    );
}

/// Unrevoking an index that was never revoked still returns
/// `AttestationNotRevoked` even after an unrelated index was revoked.
#[test]
fn test_unrevoke_not_revoked_while_other_revoked() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    client.append_attestation_digest(&digest(&env, 0x02));
    client.revoke_attestation_digest(&1);
    assert_contract_error(
        client.try_unrevoke_attestation_digest(&0),
        EscrowError::AttestationNotRevoked,
    );
}

/// Digest is preserved through revoke → unrevoke cycles.
#[test]
fn test_unrevoke_preserves_digest() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0xCA);
    client.append_attestation_digest(&d);

    client.revoke_attestation_digest(&0);
    client.unrevoke_attestation_digest(&0);

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 1);
    assert_eq!(log.get(0).unwrap(), d);
}

/// Multiple revoke → unrevoke cycles on the same index preserve the digest
/// and toggle the revoked flag each time.
#[test]
fn test_revoke_unrevoke_cycle() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0xDD);
    client.append_attestation_digest(&d);

    for _ in 0..3 {
        assert!(!client.is_attestation_revoked(&0));
        client.revoke_attestation_digest(&0);
        assert!(client.is_attestation_revoked(&0));
        client.unrevoke_attestation_digest(&0);
        assert!(!client.is_attestation_revoked(&0));
    }
    let log = client.get_attestation_append_log();
    assert_eq!(log.get(0).unwrap(), d);
}

/// Revoke → unrevoke → revoke again succeeds (full round-trip).
#[test]
fn test_revoke_unrevoke_revoke_again() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xEE));

    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));

    client.unrevoke_attestation_digest(&0);
    assert!(!client.is_attestation_revoked(&0));

    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));
}

/// Unrevoking one index does not affect the revocation state of others.
#[test]
fn test_unrevoke_does_not_affect_other_indices() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    client.append_attestation_digest(&digest(&env, 0x02));
    client.append_attestation_digest(&digest(&env, 0x03));

    client.revoke_attestation_digest(&0);
    client.revoke_attestation_digest(&2);
    assert!(client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(client.is_attestation_revoked(&2));

    client.unrevoke_attestation_digest(&0);

    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(client.is_attestation_revoked(&2));
}

/// Unrevoking all revoked entries sequentially clears every marker.
#[test]
fn test_unrevoke_all_entries() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..5 {
        client.append_attestation_digest(&digest(&env, i));
    }
    for i in 0u8..5 {
        client.revoke_attestation_digest(&(i as u32));
    }
    for i in 0u8..5 {
        assert!(client.is_attestation_revoked(&(i as u32)));
        client.unrevoke_attestation_digest(&(i as u32));
        assert!(!client.is_attestation_revoked(&(i as u32)));
    }
}

/// Unrevoked index correctly reports `false` via `is_attestation_revoked`
/// while other revoked indices remain `true`.
#[test]
fn test_unrevoke_mid_index() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..3 {
        client.append_attestation_digest(&digest(&env, i));
    }
    for i in 0u8..3 {
        client.revoke_attestation_digest(&(i as u32));
    }
    // Unrevoke only the middle entry.
    client.unrevoke_attestation_digest(&1);
    assert!(client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(client.is_attestation_revoked(&2));
}

/// Non-admin caller must not be able to unrevoke.
#[test]
#[should_panic]
fn test_unrevoke_non_admin_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xFF));
    client.revoke_attestation_digest(&0);
    env.mock_auths(&[]);
    client.unrevoke_attestation_digest(&0);
}

/// Non-admin `try_unrevoke_attestation_digest` returns an error.
#[test]
fn test_unrevoke_non_admin_returns_error() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xFF));
    client.revoke_attestation_digest(&0);
    env.mock_auths(&[]);
    assert!(client.try_unrevoke_attestation_digest(&0).is_err());
}

// ---------------------------------------------------------------------------
// get_attestation_digest_at
// ---------------------------------------------------------------------------

#[test]
fn test_get_attestation_digest_at_none_when_empty() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert_eq!(client.get_attestation_digest_at(&0), None);
    assert_eq!(client.get_attestation_digest_at(&1), None);
}

#[test]
fn test_get_attestation_digest_at_none_out_of_bounds() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    client.append_attestation_digest(&digest(&env, 0x02));

    assert_eq!(client.get_attestation_digest_at(&2), None);
    assert_eq!(client.get_attestation_digest_at(&100), None);
}

#[test]
fn test_get_attestation_digest_at_retrieves_unrevoked() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d0 = digest(&env, 0x10);
    let d1 = digest(&env, 0x20);
    client.append_attestation_digest(&d0);
    client.append_attestation_digest(&d1);

    let info0 = client.get_attestation_digest_at(&0).unwrap();
    assert_eq!(info0.digest, d0);
    assert!(!info0.revoked);

    let info1 = client.get_attestation_digest_at(&1).unwrap();
    assert_eq!(info1.digest, d1);
    assert!(!info1.revoked);
}

#[test]
fn test_get_attestation_digest_at_reflects_revocation_cycle() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0xAB);
    client.append_attestation_digest(&d);

    // Initial state: unrevoked
    let info = client.get_attestation_digest_at(&0).unwrap();
    assert_eq!(info.digest, d);
    assert!(!info.revoked);

    // Revoked state
    client.revoke_attestation_digest(&0);
    let info = client.get_attestation_digest_at(&0).unwrap();
    assert_eq!(info.digest, d);
    assert!(info.revoked);

    // Unrevoked state again
    client.unrevoke_attestation_digest(&0);
    let info = client.get_attestation_digest_at(&0).unwrap();
    assert_eq!(info.digest, d);
    assert!(!info.revoked);
}

// ── Issue #555: get_revoked_attestation_digests ──────────────────────────────

#[test]
fn test_revoked_digests_view_only_revoked_entries() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d0 = digest(&env, 0x01);
    let d1 = digest(&env, 0x02);
    let d2 = digest(&env, 0x03);
    client.append_attestation_digest(&d0);
    client.append_attestation_digest(&d1);
    client.append_attestation_digest(&d2);
    client.revoke_attestation_digest(&0);
    client.revoke_attestation_digest(&2);

    let page = client.get_revoked_attestation_digests(&0, &10);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap().digest, d0);
    assert!(page.get(0).unwrap().revoked);
    assert_eq!(page.get(1).unwrap().digest, d2);
}

#[test]
fn test_revoked_digests_view_excludes_unrevoked_after_unrevoke() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xAA));
    client.append_attestation_digest(&digest(&env, 0xBB));
    client.revoke_attestation_digest(&0);
    client.revoke_attestation_digest(&1);
    client.unrevoke_attestation_digest(&0);

    let page = client.get_revoked_attestation_digests(&0, &10);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap().digest, digest(&env, 0xBB));
}

#[test]
fn test_revoked_digests_view_pagination_and_empty_past_end() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..5 {
        client.append_attestation_digest(&digest(&env, i));
        client.revoke_attestation_digest(&(i as u32));
    }

    let page0 = client.get_revoked_attestation_digests(&0, &2);
    assert_eq!(page0.len(), 2);
    let page2 = client.get_revoked_attestation_digests(&2, &2);
    assert_eq!(page2.len(), 2);
    let past = client.get_revoked_attestation_digests(&100, &10);
    assert_eq!(past.len(), 0);
}

#[test]
#[ignore = "branch-specific latent failure"]
fn test_revoked_digests_view_caps_limit() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..10 {
        client.append_attestation_digest(&digest(&env, i));
        client.revoke_attestation_digest(&(i as u32));
    }
    let page = client.get_revoked_attestation_digests(&0, &100);
    assert_eq!(page.len(), crate::MAX_ATTESTATION_READ_PAGE);
}

// ===========================================================================
// Issue #699 — attestation boundary & rejection tests
// ===========================================================================

// ---------------------------------------------------------------------------
// append_attestation_digest — event content verification
// ---------------------------------------------------------------------------

/// `append_attestation_digest` emits `att_app` with the correct index and digest.
#[test]
fn test_append_emits_att_app_event_first_entry() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let contract_id = client.address.clone();
    let d = digest(&env, 0x11);

    client.append_attestation_digest(&d);

    let all_events = env.events().all();
    let invoice_id = client.get_escrow().invoice_id;
    assert_eq!(
        all_events.events().last().unwrap().clone(),
        AttestationDigestAppended {
            name: symbol_short!("att_app"),
            invoice_id,
            index: 0,
            digest: d,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Each successive `append_attestation_digest` emits `att_app` with a monotonically
/// increasing index.
#[test]
fn test_append_emits_att_app_event_sequential_indices() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let contract_id = client.address.clone();

    for i in 0u8..4 {
        let d = digest(&env, i);
        client.append_attestation_digest(&d);
        let all_events = env.events().all();
        let invoice_id = client.get_escrow().invoice_id;
        assert_eq!(
            all_events.events().last().unwrap().clone(),
            AttestationDigestAppended {
                name: symbol_short!("att_app"),
                invoice_id,
                index: i as u32,
                digest: d,
            }
            .to_xdr(&env, &contract_id)
        );
    }
}

// ---------------------------------------------------------------------------
// revoke_attestation_digest — event content verification and exact boundaries
// ---------------------------------------------------------------------------

/// `revoke_attestation_digest` emits `att_rev` with the correct index.
#[test]
fn test_revoke_emits_att_rev_event() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let contract_id = client.address.clone();
    client.append_attestation_digest(&digest(&env, 0x55));

    client.revoke_attestation_digest(&0);

    let all_events = env.events().all();
    let invoice_id = client.get_escrow().invoice_id;
    assert_eq!(
        all_events.events().last().unwrap().clone(),
        AttestationDigestRevoked {
            name: symbol_short!("att_rev"),
            invoice_id,
            index: 0,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// `revoke_attestation_digest` with `index == log.len()` (exactly at boundary, one past last
/// valid index) must return `AttestationIndexOutOfRange`.
#[test]
fn test_revoke_index_exactly_at_log_len_is_out_of_range() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    client.append_attestation_digest(&digest(&env, 0x02));
    // log.len() == 2; index 2 is one past the last valid index (1).
    assert_contract_error(
        client.try_revoke_attestation_digest(&2),
        EscrowError::AttestationIndexOutOfRange,
    );
}

/// `revoke_attestation_digest` with `index == 0` on a one-entry log is the exact lower
/// boundary — must succeed.
#[test]
fn test_revoke_index_zero_on_single_entry_log_succeeds() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xAA));
    // index 0 is both the first and last valid index.
    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));
}

/// `revoke_attestation_digest` on an empty log must return `AttestationIndexOutOfRange`
/// even for index 0.
#[test]
fn test_revoke_index_zero_on_empty_log_is_out_of_range() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // log is empty — any index is out of range.
    assert_contract_error(
        client.try_revoke_attestation_digest(&0),
        EscrowError::AttestationIndexOutOfRange,
    );
}

// ---------------------------------------------------------------------------
// revoke_attestation_digests (batch) — full coverage
// ---------------------------------------------------------------------------

/// Happy path: batch revoke two distinct indices, confirm both are revoked,
/// confirm two `att_rev` events are emitted in order.
#[test]
fn test_batch_revoke_happy_path_two_indices() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let contract_id = client.address.clone();
    for i in 0u8..3 {
        client.append_attestation_digest(&digest(&env, i));
    }

    let mut indices = SorobanVec::new(&env);
    indices.push_back(0u32);
    indices.push_back(2u32);
    client.revoke_attestation_digests(&indices);

    assert!(client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(client.is_attestation_revoked(&2));

    // Collect invoice_id before snapshotting events so the get_escrow() call
    // does not add entries that shift the indices we care about.
    let invoice_id = client.get_escrow().invoice_id;

    // Verify the two att_rev events emitted by the batch call.
    // Convert to a std::vec so we can index from the end without u32 underflow.
    let all_events_snapshot = env.events().all();
    let all_events: std::vec::Vec<_> = all_events_snapshot.events().iter().collect();
    let n = all_events.len();
    assert!(n >= 2, "expected at least 2 events, got {n}");
    assert_eq!(
        all_events[n - 2].clone(),
        AttestationDigestRevoked {
            name: symbol_short!("att_rev"),
            invoice_id: invoice_id.clone(),
            index: 0,
        }
        .to_xdr(&env, &contract_id)
    );
    assert_eq!(
        all_events[n - 1].clone(),
        AttestationDigestRevoked {
            name: symbol_short!("att_rev"),
            invoice_id,
            index: 2,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Batch of exactly one index succeeds (lower bound is 1).
#[test]
fn test_batch_revoke_single_element_succeeds() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x10));

    let mut indices = SorobanVec::new(&env);
    indices.push_back(0u32);
    client.revoke_attestation_digests(&indices);

    assert!(client.is_attestation_revoked(&0));
}

/// Batch of exactly `MAX_ATTESTATION_REVOKE_BATCH` (32) entries is the upper boundary
/// and must succeed.
#[test]
fn test_batch_revoke_exactly_max_batch_size_succeeds() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Append exactly MAX_ATTESTATION_REVOKE_BATCH entries (== MAX_ATTESTATION_APPEND_ENTRIES == 32).
    for i in 0u8..(MAX_ATTESTATION_REVOKE_BATCH as u8) {
        client.append_attestation_digest(&digest(&env, i));
    }

    let mut indices = SorobanVec::new(&env);
    for i in 0u32..MAX_ATTESTATION_REVOKE_BATCH {
        indices.push_back(i);
    }
    client.revoke_attestation_digests(&indices);

    for i in 0u32..MAX_ATTESTATION_REVOKE_BATCH {
        assert!(client.is_attestation_revoked(&i));
    }
}

/// Empty batch (zero indices) must return `AttestationBatchEmpty`.
#[test]
fn test_batch_revoke_empty_indices_returns_batch_empty() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));

    let indices: SorobanVec<u32> = SorobanVec::new(&env);
    assert_contract_error(
        client.try_revoke_attestation_digests(&indices),
        EscrowError::AttestationBatchEmpty,
    );
}

/// Batch with `MAX_ATTESTATION_REVOKE_BATCH + 1` entries must return `AttestationBatchTooLarge`.
#[test]
fn test_batch_revoke_oversized_returns_batch_too_large() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // We don't need a full log to trigger the size check — the guard fires before auth/log reads.
    client.append_attestation_digest(&digest(&env, 0x01));

    let mut indices: SorobanVec<u32> = SorobanVec::new(&env);
    for i in 0u32..=(MAX_ATTESTATION_REVOKE_BATCH) {
        // MAX + 1 entries
        indices.push_back(i);
    }
    assert_contract_error(
        client.try_revoke_attestation_digests(&indices),
        EscrowError::AttestationBatchTooLarge,
    );
}

/// Batch containing an out-of-range index (index >= log.len()) must return
/// `AttestationIndexOutOfRange` and leave no revocations applied (atomicity).
#[test]
fn test_batch_revoke_out_of_range_index_is_atomic() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    client.append_attestation_digest(&digest(&env, 0x02));
    // index 2 == log.len(); indices 0 is valid, 2 is not.
    let mut indices = SorobanVec::new(&env);
    indices.push_back(0u32);
    indices.push_back(2u32); // out of range — should roll back index 0 as well
    assert_contract_error(
        client.try_revoke_attestation_digests(&indices),
        EscrowError::AttestationIndexOutOfRange,
    );
    // Atomicity: index 0 must NOT be revoked.
    assert!(!client.is_attestation_revoked(&0));
}

/// Batch containing a duplicate index must return `AttestationAlreadyRevoked` on the
/// second occurrence and leave no partial state committed (atomicity).
#[test]
fn test_batch_revoke_duplicate_index_is_atomic() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..3 {
        client.append_attestation_digest(&digest(&env, i));
    }
    // Index 1 appears twice — second occurrence must fail.
    let mut indices = SorobanVec::new(&env);
    indices.push_back(0u32);
    indices.push_back(1u32);
    indices.push_back(1u32); // duplicate
    assert_contract_error(
        client.try_revoke_attestation_digests(&indices),
        EscrowError::AttestationAlreadyRevoked,
    );
    // Atomicity: neither index 0 nor index 1 should be revoked.
    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
}

/// Batch containing an already-revoked index (pre-revoked before the batch call) must
/// return `AttestationAlreadyRevoked` and roll back any other indices in the same batch.
#[test]
fn test_batch_revoke_already_revoked_index_is_atomic() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..3 {
        client.append_attestation_digest(&digest(&env, i));
    }
    // Pre-revoke index 1 via the single-index entrypoint.
    client.revoke_attestation_digest(&1);

    // Now attempt a batch that includes the already-revoked index 1.
    let mut indices = SorobanVec::new(&env);
    indices.push_back(0u32); // valid, not yet revoked
    indices.push_back(1u32); // already revoked — should trigger the error
    assert_contract_error(
        client.try_revoke_attestation_digests(&indices),
        EscrowError::AttestationAlreadyRevoked,
    );
    // Atomicity: index 0 must remain unrevoked.
    assert!(!client.is_attestation_revoked(&0));
    // Index 1 was already revoked before the batch.
    assert!(client.is_attestation_revoked(&1));
}

/// Non-admin caller must not be able to batch-revoke.
#[test]
fn test_batch_revoke_non_admin_returns_error() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    env.mock_auths(&[]);

    let mut indices = SorobanVec::new(&env);
    indices.push_back(0u32);
    assert!(client.try_revoke_attestation_digests(&indices).is_err());
    // No revocation was applied.
    assert!(!client.is_attestation_revoked(&0));
}

/// Batch revoke with `index == log.len()` (exactly at the boundary, one past the last
/// valid index) must return `AttestationIndexOutOfRange`.
#[test]
fn test_batch_revoke_index_exactly_at_log_len_is_out_of_range() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    // log.len() == 1; index 1 is exactly at boundary.
    let mut indices = SorobanVec::new(&env);
    indices.push_back(1u32);
    assert_contract_error(
        client.try_revoke_attestation_digests(&indices),
        EscrowError::AttestationIndexOutOfRange,
    );
}

// ---------------------------------------------------------------------------
// get_revoked_attestation_digests — limit cap (un-ignores the latent test)
// ---------------------------------------------------------------------------

/// `get_revoked_attestation_digests` with a limit larger than `MAX_ATTESTATION_READ_PAGE`
/// must be silently capped at `MAX_ATTESTATION_READ_PAGE` (= 20).
#[test]
fn test_revoked_digests_view_caps_limit_at_read_page_max() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Append and revoke enough entries to exceed the cap.
    let entry_count = MAX_ATTESTATION_READ_PAGE + 5; // 25 entries
    for i in 0u8..(entry_count as u8) {
        client.append_attestation_digest(&digest(&env, i));
        client.revoke_attestation_digest(&(i as u32));
    }
    // Request more than the page cap allows.
    let page = client.get_revoked_attestation_digests(&0, &(entry_count + 100));
    assert_eq!(page.len(), MAX_ATTESTATION_READ_PAGE);
}

/// `get_revoked_attestation_digests` with a limit of exactly `MAX_ATTESTATION_READ_PAGE`
/// (the boundary) returns exactly that many entries when enough revoked entries exist.
#[test]
fn test_revoked_digests_view_exactly_at_read_page_max() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..(MAX_ATTESTATION_READ_PAGE as u8) {
        client.append_attestation_digest(&digest(&env, i));
        client.revoke_attestation_digest(&(i as u32));
    }
    let page = client.get_revoked_attestation_digests(&0, &MAX_ATTESTATION_READ_PAGE);
    assert_eq!(page.len(), MAX_ATTESTATION_READ_PAGE);
}

/// `get_revoked_attestation_digests` with `limit == 0` returns an empty result
/// without touching storage (zero-limit early return).
#[test]
fn test_revoked_digests_view_zero_limit_returns_empty() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    client.revoke_attestation_digest(&0);

    let page = client.get_revoked_attestation_digests(&0, &0);
    assert_eq!(page.len(), 0);
}
