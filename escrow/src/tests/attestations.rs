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

// ---------------------------------------------------------------------------
// load_attestation_log helper — consistent empty-log fallback across callers
// ---------------------------------------------------------------------------

/// All callers of `load_attestation_log` return an empty log before any append.
/// This exercises the `unwrap_or_else(|| Vec::new(env))` branch of the helper.
#[test]
fn test_load_attestation_log_empty_fallback_for_all_callers() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // get_attestation_append_log is a public wrapper over load_attestation_log
    assert_eq!(client.get_attestation_append_log().len(), 0);

    // revoke_attestation_digest must reject index 0 (out of range), not panic on missing key
    assert_contract_error(
        client.try_revoke_attestation_digest(&0),
        EscrowError::AttestationIndexOutOfRange,
    );

    // revoke_attestation_digests must reject index 0 (out of range) on empty log
    let indices = soroban_sdk::vec![&env, 0u32];
    assert_contract_error(
        client.try_revoke_attestation_digests(&indices),
        EscrowError::AttestationIndexOutOfRange,
    );

    // unrevoke_attestation_digest must reject index 0 (out of range) on empty log
    assert_contract_error(
        client.try_unrevoke_attestation_digest(&0),
        EscrowError::AttestationIndexOutOfRange,
    );
}

/// append_attestation_digest uses load_attestation_log; appending to a
/// never-written log creates the key and the first entry is readable.
#[test]
fn test_load_attestation_log_helper_creates_key_on_first_append() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // The log does not exist yet — load_attestation_log returns an empty Vec.
    assert_eq!(client.get_attestation_append_log().len(), 0);

    client.append_attestation_digest(&digest(&env, 0x01));

    // After the first append, load_attestation_log reads the now-present key.
    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 1);
    assert_eq!(log.get(0).unwrap(), digest(&env, 0x01));
}

// ---------------------------------------------------------------------------
// require_attestation_index_in_range helper — identical rejection in all callers
// ---------------------------------------------------------------------------

/// `revoke_attestation_digest` fires `AttestationIndexOutOfRange` at exactly
/// `index == log.len()` (first out-of-bounds position).
#[test]
fn test_require_index_in_range_revoke_at_exact_len_boundary() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01)); // log.len() == 1

    // index == 1 == log.len() → out of range
    assert_contract_error(
        client.try_revoke_attestation_digest(&1),
        EscrowError::AttestationIndexOutOfRange,
    );

    // index == 0 == log.len() - 1 → in range (should succeed)
    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));
}

/// `revoke_attestation_digests` fires `AttestationIndexOutOfRange` on the first
/// out-of-bounds index in a batch, rolling back any earlier valid entries.
#[test]
fn test_require_index_in_range_batch_revoke_partial_rollback() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01)); // index 0 valid
    client.append_attestation_digest(&digest(&env, 0x02)); // index 1 valid
                                                           // index 2 is out of range (log.len() == 2)

    let indices = soroban_sdk::vec![&env, 0u32, 2u32];
    assert_contract_error(
        client.try_revoke_attestation_digests(&indices),
        EscrowError::AttestationIndexOutOfRange,
    );

    // The whole batch must be rolled back — index 0 must NOT be revoked.
    assert!(!client.is_attestation_revoked(&0));
}

/// `unrevoke_attestation_digest` fires `AttestationIndexOutOfRange` at exactly
/// `index == log.len()` (first out-of-bounds position).
#[test]
fn test_require_index_in_range_unrevoke_at_exact_len_boundary() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01)); // log.len() == 1
    client.revoke_attestation_digest(&0);

    // index == 1 == log.len() → out of range
    assert_contract_error(
        client.try_unrevoke_attestation_digest(&1),
        EscrowError::AttestationIndexOutOfRange,
    );

    // index == 0 → in range; should succeed
    client.unrevoke_attestation_digest(&0);
    assert!(!client.is_attestation_revoked(&0));
}

/// The same `AttestationIndexOutOfRange` typed error code is returned by all three
/// callers of `require_attestation_index_in_range` when given an equal large index.
#[test]
fn test_require_index_in_range_same_error_code_across_all_callers() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));

    let out_of_range: u32 = 99;

    assert_contract_error(
        client.try_revoke_attestation_digest(&out_of_range),
        EscrowError::AttestationIndexOutOfRange,
    );

    let indices = soroban_sdk::vec![&env, out_of_range];
    assert_contract_error(
        client.try_revoke_attestation_digests(&indices),
        EscrowError::AttestationIndexOutOfRange,
    );

    assert_contract_error(
        client.try_unrevoke_attestation_digest(&out_of_range),
        EscrowError::AttestationIndexOutOfRange,
    );
}

/// All entrypoints return `AttestationIndexOutOfRange` when the log is empty
/// (index 0 is always out of range on an empty log).
#[test]
fn test_require_index_in_range_empty_log_index_zero_all_callers() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // No appends — log is empty.

    assert_contract_error(
        client.try_revoke_attestation_digest(&0),
        EscrowError::AttestationIndexOutOfRange,
    );

    let indices = soroban_sdk::vec![&env, 0u32];
    assert_contract_error(
        client.try_revoke_attestation_digests(&indices),
        EscrowError::AttestationIndexOutOfRange,
    );

    assert_contract_error(
        client.try_unrevoke_attestation_digest(&0),
        EscrowError::AttestationIndexOutOfRange,
    );
}

/// Appending exactly MAX entries then revoking and unrevoking the last index (31)
/// exercises the in-range boundary check at the maximum log size.
#[test]
fn test_require_index_in_range_last_valid_index_at_max_capacity() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8) {
        client.append_attestation_digest(&digest(&env, i));
    }
    let last = MAX_ATTESTATION_APPEND_ENTRIES - 1;

    // Revoke the last valid index — require_attestation_index_in_range must pass.
    client.revoke_attestation_digest(&last);
    assert!(client.is_attestation_revoked(&last));

    // Unrevoke the last valid index — require_attestation_index_in_range must pass again.
    client.unrevoke_attestation_digest(&last);
    assert!(!client.is_attestation_revoked(&last));

    // MAX_ATTESTATION_APPEND_ENTRIES itself (== log.len()) must be out of range.
    assert_contract_error(
        client.try_revoke_attestation_digest(&MAX_ATTESTATION_APPEND_ENTRIES),
        EscrowError::AttestationIndexOutOfRange,
    );
}

// ---------------------------------------------------------------------------
// append_attestation_digests — batch entrypoint (issue #61)
// ---------------------------------------------------------------------------

/// Happy path: batch of 3 digests appended atomically, all readable in order.
#[test]
fn test_batch_append_happy_path() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let d0 = digest(&env, 0x10);
    let d1 = digest(&env, 0x20);
    let d2 = digest(&env, 0x30);
    let batch = soroban_sdk::vec![&env, d0.clone(), d1.clone(), d2.clone()];

    client.append_attestation_digests(&batch);

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 3);
    assert_eq!(log.get(0).unwrap(), d0);
    assert_eq!(log.get(1).unwrap(), d1);
    assert_eq!(log.get(2).unwrap(), d2);
}

/// Batch append to a partially-filled log assigns correct indices starting at
/// the current log length.
#[test]
fn test_batch_append_starts_at_existing_log_length() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // Pre-fill two entries via single-entry entrypoint.
    client.append_attestation_digest(&digest(&env, 0xAA));
    client.append_attestation_digest(&digest(&env, 0xBB));

    let d2 = digest(&env, 0xCC);
    let d3 = digest(&env, 0xDD);
    let batch = soroban_sdk::vec![&env, d2.clone(), d3.clone()];
    client.append_attestation_digests(&batch);

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 4);
    assert_eq!(log.get(2).unwrap(), d2);
    assert_eq!(log.get(3).unwrap(), d3);
}

/// A single-element batch succeeds (minimum valid batch size).
#[test]
fn test_batch_append_single_element_succeeds() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let d = digest(&env, 0x01);
    let batch = soroban_sdk::vec![&env, d.clone()];
    client.append_attestation_digests(&batch);

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 1);
    assert_eq!(log.get(0).unwrap(), d);
}

/// A batch of exactly MAX_ATTESTATION_APPEND_BATCH entries succeeds (upper
/// boundary is inclusive when the log is empty).
#[test]
fn test_batch_append_max_size_succeeds() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let mut batch = soroban_sdk::Vec::new(&env);
    for i in 0u8..(MAX_ATTESTATION_APPEND_BATCH as u8) {
        batch.push_back(digest(&env, i));
    }

    client.append_attestation_digests(&batch);

    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_BATCH
    );
}

/// An empty batch returns `AttestationAppendBatchEmpty` (57).
#[test]
fn test_batch_append_empty_returns_typed_error() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let empty: soroban_sdk::Vec<soroban_sdk::BytesN<32>> = soroban_sdk::Vec::new(&env);
    assert_contract_error(
        client.try_append_attestation_digests(&empty),
        EscrowError::AttestationAppendBatchEmpty,
    );

    // Log must be unmodified.
    assert_eq!(client.get_attestation_append_log().len(), 0);
}

/// A batch exceeding MAX_ATTESTATION_APPEND_BATCH returns
/// `AttestationAppendBatchTooLarge` (58).
#[test]
fn test_batch_append_over_limit_returns_typed_error() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let mut oversized = soroban_sdk::Vec::new(&env);
    // MAX_ATTESTATION_APPEND_BATCH + 1 entries.
    for i in 0u8..=(MAX_ATTESTATION_APPEND_BATCH as u8) {
        oversized.push_back(digest(&env, i));
    }

    assert_contract_error(
        client.try_append_attestation_digests(&oversized),
        EscrowError::AttestationAppendBatchTooLarge,
    );

    // No partial write must have occurred.
    assert_eq!(client.get_attestation_append_log().len(), 0);
}

/// A batch that would push the log beyond MAX_ATTESTATION_APPEND_ENTRIES is
/// rejected atomically with `AttestationAppendLogCapacityReached` (51).
/// The pre-flight check means even a partially-fitting batch is fully rejected.
#[test]
fn test_batch_append_over_capacity_rejected_atomically() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // Fill log to 30 entries via single-entry calls.
    for i in 0u8..30 {
        client.append_attestation_digest(&digest(&env, i));
    }
    assert_eq!(client.get_attestation_append_log().len(), 30);

    // A batch of 3 would bring the total to 33, exceeding the limit of 32.
    let overflow_batch = soroban_sdk::vec![
        &env,
        digest(&env, 0xA0),
        digest(&env, 0xA1),
        digest(&env, 0xA2)
    ];

    assert_contract_error(
        client.try_append_attestation_digests(&overflow_batch),
        EscrowError::AttestationAppendLogCapacityReached,
    );

    // Log must still be at 30 — no partial append.
    assert_eq!(client.get_attestation_append_log().len(), 30);
}

/// Filling exactly to capacity in one batch call succeeds (boundary inclusive).
#[test]
fn test_batch_append_fills_log_exactly_to_capacity() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // Fill to exactly MAX_ATTESTATION_APPEND_ENTRIES in one shot.
    let mut batch = soroban_sdk::Vec::new(&env);
    for i in 0u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8) {
        batch.push_back(digest(&env, i));
    }
    client.append_attestation_digests(&batch);

    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES
    );

    // One more single append must fail.
    assert_contract_error(
        client.try_append_attestation_digest(&digest(&env, 0xFF)),
        EscrowError::AttestationAppendLogCapacityReached,
    );
}

/// Duplicate digests within a batch are allowed (log is an audit trail, not a set).
#[test]
fn test_batch_append_duplicate_digests_allowed() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let d = digest(&env, 0x42);
    let batch = soroban_sdk::vec![&env, d.clone(), d.clone(), d.clone()];
    client.append_attestation_digests(&batch);

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 3);
    for i in 0..3 {
        assert_eq!(log.get(i).unwrap(), d);
    }
}

/// Non-admin caller must not be able to use the batch append entrypoint.
#[test]
fn test_batch_append_non_admin_returns_error() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    env.mock_auths(&[]);
    let batch = soroban_sdk::vec![&env, digest(&env, 0x01)];
    assert!(client.try_append_attestation_digests(&batch).is_err());

    // Log must be unmodified.
    assert_eq!(client.get_attestation_append_log().len(), 0);
}

/// Batch append emits exactly one `att_app` event per digest with the correct
/// sequential index and digest value.
#[test]
fn test_batch_append_emits_events_with_correct_indices() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    let d0 = digest(&env, 0x10);
    let d1 = digest(&env, 0x20);
    let batch = soroban_sdk::vec![&env, d0.clone(), d1.clone()];

    // Clear any prior events emitted during setup/init.
    let events_before = env.events().all().len();
    client.append_attestation_digests(&batch);

    let all_events = env.events().all();
    let new_events: soroban_sdk::Vec<_> = {
        let mut v = soroban_sdk::Vec::new(&env);
        for i in events_before..all_events.len() {
            v.push_back(all_events.get(i).unwrap());
        }
        v
    };

    // Expect exactly two new events.
    assert_eq!(new_events.len(), 2);

    assert_eq!(
        new_events.get(0).unwrap(),
        AttestationDigestAppended {
            name: soroban_sdk::symbol_short!("att_app"),
            invoice_id: invoice_id.clone(),
            index: 0,
            digest: d0,
        }
        .to_xdr(&env, &contract_id)
    );

    assert_eq!(
        new_events.get(1).unwrap(),
        AttestationDigestAppended {
            name: soroban_sdk::symbol_short!("att_app"),
            invoice_id,
            index: 1,
            digest: d1,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Batch append events correctly offset indices when the log is already partially filled.
#[test]
fn test_batch_append_events_offset_by_existing_log_length() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    // Pre-fill two entries.
    client.append_attestation_digest(&digest(&env, 0x01));
    client.append_attestation_digest(&digest(&env, 0x02));

    let events_before = env.events().all().len();
    let d2 = digest(&env, 0xCC);
    let batch = soroban_sdk::vec![&env, d2.clone()];
    client.append_attestation_digests(&batch);

    let all_events = env.events().all();
    // The single new event must be at index 2.
    assert_eq!(
        all_events.get(events_before).unwrap(),
        AttestationDigestAppended {
            name: soroban_sdk::symbol_short!("att_app"),
            invoice_id,
            index: 2,
            digest: d2,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Mixing single-entry and batch-entry appends interleaves correctly, preserving
/// the full ordered audit trail.
#[test]
fn test_batch_append_interleaved_with_single_appends() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let d0 = digest(&env, 0x01);
    let d1 = digest(&env, 0x02);
    let d2 = digest(&env, 0x03);
    let d3 = digest(&env, 0x04);
    let d4 = digest(&env, 0x05);

    client.append_attestation_digest(&d0);
    let batch = soroban_sdk::vec![&env, d1.clone(), d2.clone()];
    client.append_attestation_digests(&batch);
    client.append_attestation_digest(&d3);
    let batch2 = soroban_sdk::vec![&env, d4.clone()];
    client.append_attestation_digests(&batch2);

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 5);
    assert_eq!(log.get(0).unwrap(), d0);
    assert_eq!(log.get(1).unwrap(), d1);
    assert_eq!(log.get(2).unwrap(), d2);
    assert_eq!(log.get(3).unwrap(), d3);
    assert_eq!(log.get(4).unwrap(), d4);
}

/// Batch-appended entries are independently revocable after insertion.
#[test]
fn test_batch_append_entries_are_revocable() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let batch = soroban_sdk::vec![
        &env,
        digest(&env, 0x01),
        digest(&env, 0x02),
        digest(&env, 0x03)
    ];
    client.append_attestation_digests(&batch);

    // Revoke the middle entry.
    client.revoke_attestation_digest(&1);
    assert!(!client.is_attestation_revoked(&0));
    assert!(client.is_attestation_revoked(&1));
    assert!(!client.is_attestation_revoked(&2));

    // Log contents must be unchanged.
    let log = client.get_attestation_append_log();
    assert_eq!(log.get(0).unwrap(), digest(&env, 0x01));
    assert_eq!(log.get(1).unwrap(), digest(&env, 0x02));
    assert_eq!(log.get(2).unwrap(), digest(&env, 0x03));
}

/// A failed batch append (over-limit) must not modify the log — verifies atomicity
/// by checking the log is exactly as it was before the failed call.
#[test]
fn test_batch_append_failed_call_leaves_log_unchanged() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // Seed a known state.
    client.append_attestation_digest(&digest(&env, 0xAA));

    let snapshot = client.get_attestation_append_log();

    // Attempt an over-limit batch — must fail atomically.
    let mut oversized = soroban_sdk::Vec::new(&env);
    for i in 0u8..=(MAX_ATTESTATION_APPEND_BATCH as u8) {
        oversized.push_back(digest(&env, i));
    }
    let _ = client.try_append_attestation_digests(&oversized);

    // Attempt a capacity-overflow batch — must also fail atomically.
    let mut overflow = soroban_sdk::Vec::new(&env);
    for i in 0u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8) {
        overflow.push_back(digest(&env, i));
    }
    let _ = client.try_append_attestation_digests(&overflow);

    // Log must still contain exactly the one seeded entry.
    let after = client.get_attestation_append_log();
    assert_eq!(after.len(), snapshot.len());
    assert_eq!(after.get(0).unwrap(), snapshot.get(0).unwrap());
}
