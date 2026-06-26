//! Attestation tests: `bind_primary_attestation_hash` (single-set) and
//! `append_attestation_digest` (bounded by [`MAX_ATTESTATION_APPEND_ENTRIES`]).
//!
//! These tests prove the two chain-anchor invariants:
//! 1. The primary hash is **write-once** ÔÇö a second bind panics regardless of the digest value.
//! 2. The append log is **capacity-bounded** ÔÇö the 33rd entry panics; the 32nd succeeds.
//!
//! Neither entrypoint stores ZK proofs or performs off-chain verification. They record a
//! 32-byte digest (e.g. SHA-256 of an IPFS CID or a KYC/KYB document bundle) so that
//! off-chain verifiers can confirm the on-chain anchor matches their document set.

use super::*;
use soroban_sdk::{BytesN, Error, InvokeError};
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

// ---------------------------------------------------------------------------
// bind_primary_attestation_hash ÔÇö single-set invariant
// ---------------------------------------------------------------------------

/// Happy path: first bind succeeds and is readable via the getter.
#[test]
fn test_bind_primary_hash_stores_and_reads() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0xAB);
    client.bind_primary_attestation_hash(&d);
    assert_eq!(client.get_primary_attestation_hash(), Some(d));
}

/// Before any bind the getter returns `None`.
#[test]
fn test_get_primary_hash_none_before_bind() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert_eq!(client.get_primary_attestation_hash(), None);
}

/// A second bind with the **same** digest must panic ÔÇö single-set is unconditional.
#[test]
#[should_panic]
fn test_bind_primary_hash_same_digest_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0x01);
    client.bind_primary_attestation_hash(&d);
    client.bind_primary_attestation_hash(&d);
}

/// A second bind with a **different** digest must also panic ÔÇö no replacement allowed.
#[test]
#[should_panic]
fn test_bind_primary_hash_different_digest_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.bind_primary_attestation_hash(&digest(&env, 0x01));
    client.bind_primary_attestation_hash(&digest(&env, 0x02));
}

/// Non-admin caller must not be able to bind the primary hash.
#[test]
#[should_panic]
fn test_bind_primary_hash_non_admin_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Clear all mocks so auth is enforced for the next call.
    env.mock_auths(&[]);
    client.bind_primary_attestation_hash(&digest(&env, 0xFF));
}

// ---------------------------------------------------------------------------
// append_attestation_digest ÔÇö bounded log invariant
// ---------------------------------------------------------------------------

/// Empty log before any append.
#[test]
fn test_append_log_empty_before_first_append() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert_eq!(client.get_attestation_append_log().len(), 0);
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

/// The 32nd entry (index 31) succeeds ÔÇö boundary must be inclusive.
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

/// The 33rd entry must panic ÔÇö capacity is strictly bounded.
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

/// Duplicate digests are allowed ÔÇö the log is an audit trail, not a set.
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

// ---------------------------------------------------------------------------
// revoke_attestation_digest ÔÇö revocation tombstone invariant
// ---------------------------------------------------------------------------

/// Happy path: revoke index 0 and confirm via `is_attestation_revoked`.
#[test]
fn test_revoke_single_entry() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xAA));

    assert!(!client.is_attestation_revoked(&0));
    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));
}

/// Revoking index 1 (after two appends) leaves index 0 unaffected.
#[test]
fn test_revoke_later_index_does_not_affect_earlier() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    client.append_attestation_digest(&digest(&env, 0x02));

    client.revoke_attestation_digest(&1);
    assert!(!client.is_attestation_revoked(&0));
    assert!(client.is_attestation_revoked(&1));
}

/// Revoking all entries sequentially succeeds.
#[test]
fn test_revoke_all_entries() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..5 {
        client.append_attestation_digest(&digest(&env, i));
    }
    for i in 0u8..5 {
        assert!(!client.is_attestation_revoked(&(i as u32)));
        client.revoke_attestation_digest(&(i as u32));
        assert!(client.is_attestation_revoked(&(i as u32)));
    }
}

/// Revoking the same index twice returns `AttestationAlreadyRevoked`.
#[test]
fn test_double_revoke_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x42));
    client.revoke_attestation_digest(&0);
    assert_contract_error(
        client.try_revoke_attestation_digest(&0),
        EscrowError::AttestationAlreadyRevoked,
    );
}

/// Revoking an index beyond the current log length returns `AttestationIndexOutOfRange`.
#[test]
fn test_revoke_out_of_range_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    // Empty log, index 0 is out of range.
    assert_contract_error(
        client.try_revoke_attestation_digest(&0),
        EscrowError::AttestationIndexOutOfRange,
    );
}

/// Revoking an index equal to log length returns `AttestationIndexOutOfRange` (0-indexed).
#[test]
fn test_revoke_at_log_len_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x10));
    // log.len() == 1, so index 1 is out of range.
    assert_contract_error(
        client.try_revoke_attestation_digest(&1),
        EscrowError::AttestationIndexOutOfRange,
    );
}

/// `is_attestation_revoked` returns `false` for any index on an empty log.
#[test]
fn test_is_revoked_empty_log() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&99));
}

/// Non-admin caller must not be able to revoke.
#[test]
#[should_panic]
fn test_revoke_non_admin_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xFF));
    env.mock_auths(&[]);
    client.revoke_attestation_digest(&0);
}

/// Revocation does not alter the append log contents ÔÇö the digest remains readable.
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
// revoke_attestation_digests — batch revocation entrypoint
// ---------------------------------------------------------------------------

/// Happy path: revoke multiple indices atomically.
#[test]
fn test_batch_revoke_happy_path() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..5 {
        client.append_attestation_digest(&digest(&env, i));
    }

    client.revoke_attestation_digests(&soroban_sdk::vec![&env, 0u32, 2u32, 4u32]);

    assert!(client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(client.is_attestation_revoked(&2));
    assert!(!client.is_attestation_revoked(&3));
    assert!(client.is_attestation_revoked(&4));
}

/// Revoke all entries in one batch.
#[test]
fn test_batch_revoke_all_entries() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for i in 0u8..10 {
        client.append_attestation_digest(&digest(&env, i));
    }

    let indices = soroban_sdk::vec![
        &env,
        0u32, 1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32
    ];
    client.revoke_attestation_digests(&indices);

    for i in 0u8..10 {
        assert!(client.is_attestation_revoked(&(i as u32)));
    }
}

/// Empty batch panics with `AttestationBatchEmpty`.
#[test]
fn test_batch_revoke_empty_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xAA));

    assert_contract_error(
        client.try_revoke_attestation_digests(&soroban_sdk::vec![&env]),
        EscrowError::AttestationBatchEmpty,
    );
}

/// Oversized batch (above MAX_ATTESTATION_REVOKE_BATCH) panics with `AttestationBatchTooLarge`.
#[test]
fn test_batch_revoke_oversized_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    for _ in 0..MAX_ATTESTATION_REVOKE_BATCH + 1 {
        client.append_attestation_digest(&digest(&env, 0xAA));
    }

    let mut oversized = soroban_sdk::vec![&env];
    for i in 0..=MAX_ATTESTATION_REVOKE_BATCH {
        oversized.push_back(i);
    }

    assert_contract_error(
        client.try_revoke_attestation_digests(&oversized),
        EscrowError::AttestationBatchTooLarge,
    );
}

/// Batch at exactly MAX_ATTESTATION_REVOKE_BATCH succeeds.
#[test]
fn test_batch_revoke_max_size_succeeds() {
    use soroban_sdk::testutils::cost_estimate::{CostEstimate, NetworkInvocationResourceLimits};
    let env = Env::default();
    env.cost_estimate().disable_resource_limits();
    let (client, _) = setup_with_init(&env);
    for _ in 0..MAX_ATTESTATION_REVOKE_BATCH {
        client.append_attestation_digest(&digest(&env, 0xAA));
    }

    let mut indices = soroban_sdk::vec![&env];
    for i in 0..MAX_ATTESTATION_REVOKE_BATCH {
        indices.push_back(i);
    }

    client.revoke_attestation_digests(&indices);

    for i in 0..MAX_ATTESTATION_REVOKE_BATCH {
        assert!(client.is_attestation_revoked(&i));
    }
}

/// Out-of-range index in batch returns `AttestationIndexOutOfRange`.
#[test]
fn test_batch_revoke_out_of_range_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xAA));
    client.append_attestation_digest(&digest(&env, 0xBB));

    // index 5 is beyond the log (len=2)
    assert_contract_error(
        client.try_revoke_attestation_digests(&soroban_sdk::vec![
            &env,
            0u32,
            5u32,
        ]),
        EscrowError::AttestationIndexOutOfRange,
    );
}

/// Already-revoked index in batch returns `AttestationAlreadyRevoked`.
#[test]
fn test_batch_revoke_already_revoked_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xAA));
    client.append_attestation_digest(&digest(&env, 0xBB));

    client.revoke_attestation_digest(&0);

    // index 0 is already revoked
    assert_contract_error(
        client.try_revoke_attestation_digests(&soroban_sdk::vec![
            &env,
            1u32,
            0u32,
        ]),
        EscrowError::AttestationAlreadyRevoked,
    );
}

/// Duplicate index in batch: first occurrence succeeds, second hits `AttestationAlreadyRevoked`.
#[test]
fn test_batch_revoke_duplicate_index_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xAA));

    assert_contract_error(
        client.try_revoke_attestation_digests(&soroban_sdk::vec![
            &env,
            0u32,
            0u32,
        ]),
        EscrowError::AttestationAlreadyRevoked,
    );
    // index 0 remains unrevoked because the entire batch rolled back
    assert!(!client.is_attestation_revoked(&0));
}

/// Non-admin caller is rejected for batch revoke.
#[test]
fn test_batch_revoke_non_admin_panics() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0xFF));

    env.mock_auths(&[]);
    assert!(client
        .try_revoke_attestation_digests(&soroban_sdk::vec![&env, 0u32])
        .is_err());
}

/// Batch revoke preserves log contents (digests remain readable).
#[test]
fn test_batch_revoke_preserves_log_entries() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d0 = digest(&env, 0xA0);
    let d1 = digest(&env, 0xB1);
    let d2 = digest(&env, 0xC2);
    client.append_attestation_digest(&d0);
    client.append_attestation_digest(&d1);
    client.append_attestation_digest(&d2);

    client.revoke_attestation_digests(&soroban_sdk::vec![&env, 0u32, 2u32]);

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 3);
    assert_eq!(log.get(0).unwrap(), d0);
    assert_eq!(log.get(1).unwrap(), d1);
    assert_eq!(log.get(2).unwrap(), d2);
}

/// Batch revoke emits one `att_rev` event per index.
#[test]
fn test_batch_revoke_emits_events() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    client.append_attestation_digest(&digest(&env, 0x02));
    client.append_attestation_digest(&digest(&env, 0x03));

    client.revoke_attestation_digests(&soroban_sdk::vec![&env, 0u32, 2u32]);

    let contract_events = env.events().all().events();
    assert_eq!(contract_events.len(), 2, "expected 2 att_rev events");
}

/// Batch revoke is atomic: a mid-batch failure rolls back all prior revocations.
#[test]
fn test_batch_revoke_atomic_rollback() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    client.append_attestation_digest(&digest(&env, 0x02));

    // index 0 is valid, index 99 is out of range — entire batch should roll back
    assert_contract_error(
        client.try_revoke_attestation_digests(&soroban_sdk::vec![
            &env,
            0u32,
            99u32,
        ]),
        EscrowError::AttestationIndexOutOfRange,
    );
    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
}
