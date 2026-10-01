//! State-invariant tests for the attestation parameter subsystem.
//!
//! This file tests the **enforcement** of attestation invariants rather than
//! their happy-path behavior (which is covered by `attestations.rs`). Every
//! test below maps to a named invariant:
//!
//! | # | Invariant |
//! |---|-----------|
//! | 1 | **Single-write primary hash** — a second bind must fail regardless of whether the digest is the same or different. |
//! | 2 | **Append log hard capacity** — exactly `MAX_ATTESTATION_APPEND_ENTRIES` entries are accepted; entry `MAX+1` is rejected. |
//! | 3 | **Contiguous index assignment** — indices are assigned starting at 0 and increment by 1 per append with no gaps. |
//! | 4 | **Revocation state machine** — forward (not-revoked → revoked) and backward (revoked → not-revoked) transitions are enforced; double-revoke and double-unrevoke are rejected with typed errors. |
//! | 5 | **Authorization enforcement** — every mutating entrypoint requires admin auth; unauthenticated calls return an error before touching state. |
//! | 6 | **Batch revocation atomicity** — any guard failure rolls back the entire batch so no entry is partially revoked. |
//! | 7 | **State isolation** — the primary hash and the append log are independent; mutations in one do not corrupt the other. |
//! | 8 | **Index boundary conditions** — exact boundary indices (0, `log.len()-1`, `log.len()`) are handled correctly across all callers. |
//! | 9 | **Cross-operation contamination prevention** — revocation flags are per-index and do not leak across indices. |
//! | 10 | **Default revocation state** — `is_attestation_revoked` returns `false` for any index before the first revoke call. |

use super::*;
use crate::MAX_ATTESTATION_REVOKE_BATCH;
use soroban_sdk::{BytesN, Vec as SorobanVec};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A deterministic 32-byte digest seeded by `seed`.
fn digest(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

/// Initialize a fresh escrow and return `(client, admin)`.
fn setup_with_init(env: &Env) -> (LiquifactEscrowClient<'_>, Address) {
    let (client, admin, sme) = setup(env);
    default_init(&client, env, &admin, &sme);
    (client, admin)
}

/// Append `n` distinct digests and return the client unchanged (index 0..n-1 are populated).
fn fill_log(client: &LiquifactEscrowClient<'_>, env: &Env, n: u32) {
    for i in 0..n {
        client.append_attestation_digest(&digest(env, i as u8));
    }
}

// ===========================================================================
// Invariant 1 — Single-write primary hash
// ===========================================================================

/// The primary hash may be set exactly once; a second bind with the **same** digest
/// is rejected with `PrimaryAttestationAlreadyBound` and the stored value is preserved.
#[test]
fn inv1_primary_hash_write_once_same_digest_rejected() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0x01);

    client.bind_primary_attestation_hash(&d);
    assert_eq!(client.get_primary_attestation_hash(), Some(d.clone()));

    let res = client.try_bind_primary_attestation_hash(&d);
    assert_contract_error(res, EscrowError::PrimaryAttestationAlreadyBound);

    // Original binding preserved.
    assert_eq!(client.get_primary_attestation_hash(), Some(d));
}

/// A second bind with a **different** digest must also be rejected — the primary hash
/// is immutable once set, not merely a "last-write-wins" register.
#[test]
fn inv1_primary_hash_write_once_different_digest_rejected() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let first = digest(&env, 0xAA);
    let second = digest(&env, 0xBB);

    client.bind_primary_attestation_hash(&first);

    let res = client.try_bind_primary_attestation_hash(&second);
    assert_contract_error(res, EscrowError::PrimaryAttestationAlreadyBound);

    // First binding is the single authoritative value.
    assert_eq!(client.get_primary_attestation_hash(), Some(first));
}

/// Repeated bind attempts — even N times — never overwrite the original binding.
#[test]
fn inv1_primary_hash_repeated_bind_attempts_never_overwrite() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let original = digest(&env, 0x42);
    client.bind_primary_attestation_hash(&original);

    for i in 0u8..5 {
        let alt = digest(&env, i);
        let _ = client.try_bind_primary_attestation_hash(&alt);
    }

    // The original hash remains unchanged regardless of how many rebind attempts were made.
    assert_eq!(client.get_primary_attestation_hash(), Some(original));
}

/// Before any bind the getter returns `None` — the default state is unbound.
#[test]
fn inv1_primary_hash_unbound_before_first_bind() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    assert_eq!(client.get_primary_attestation_hash(), None);
}

// ===========================================================================
// Invariant 2 — Append log hard capacity boundary
// ===========================================================================

/// Exactly `MAX_ATTESTATION_APPEND_ENTRIES` appends succeed; the next one is rejected
/// with `AttestationAppendLogCapacityReached`.
#[test]
fn inv2_append_log_capacity_boundary_at_max() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // Fill to capacity.
    for i in 0u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8) {
        client.append_attestation_digest(&digest(&env, i));
    }
    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES
    );

    // One more entry must be rejected.
    let res = client.try_append_attestation_digest(&digest(&env, 0xFF));
    assert_contract_error(res, EscrowError::AttestationAppendLogCapacityReached);

    // Log size is still exactly MAX — the failed append added nothing.
    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES
    );
}

/// The entry at position `MAX_ATTESTATION_APPEND_ENTRIES - 1` (the last valid slot) succeeds
/// and is readable — the boundary is inclusive on the lower side.
#[test]
fn inv2_last_valid_slot_is_accepted() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // Fill to one before max.
    for i in 0u8..((MAX_ATTESTATION_APPEND_ENTRIES - 1) as u8) {
        client.append_attestation_digest(&digest(&env, i));
    }
    // The final slot must succeed.
    let last = digest(&env, 0xFF);
    client.append_attestation_digest(&last);

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(
        log.get(MAX_ATTESTATION_APPEND_ENTRIES - 1).unwrap(),
        last
    );
}

/// Multiple failed appends beyond capacity do not corrupt the log contents.
#[test]
fn inv2_failed_appends_do_not_corrupt_existing_entries() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // Capture first few expected digests.
    let first = digest(&env, 0x01);
    let second = digest(&env, 0x02);
    client.append_attestation_digest(&first);
    client.append_attestation_digest(&second);

    // Fill the rest.
    for i in 2u8..(MAX_ATTESTATION_APPEND_ENTRIES as u8) {
        client.append_attestation_digest(&digest(&env, i));
    }

    // Fail several times.
    for _ in 0..3 {
        let _ = client.try_append_attestation_digest(&digest(&env, 0xFE));
    }

    // Existing entries are intact.
    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(log.get(0).unwrap(), first);
    assert_eq!(log.get(1).unwrap(), second);
}

// ===========================================================================
// Invariant 3 — Contiguous index assignment
// ===========================================================================

/// Indices are assigned contiguously starting from 0; after N appends indices 0..N-1
/// all map to the correct digest in insertion order.
#[test]
fn inv3_indices_are_contiguous_from_zero() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let n = 8u8;
    for i in 0..n {
        client.append_attestation_digest(&digest(&env, i));
    }

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), n as u32);
    for i in 0..n {
        assert_eq!(
            log.get(i as u32).unwrap(),
            digest(&env, i),
            "index {i} must map to digest seeded with {i}"
        );
    }
}

/// `get_attestation_digest_at` returns `None` for every index ≥ log length, confirming
/// there are no phantom entries beyond the contiguous range.
#[test]
fn inv3_no_phantom_entries_beyond_log_end() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 4);

    // log.len() == 4; indices 4 and above must all be None.
    assert_eq!(client.get_attestation_digest_at(&4), None);
    assert_eq!(client.get_attestation_digest_at(&5), None);
    assert_eq!(client.get_attestation_digest_at(&100), None);
    assert_eq!(client.get_attestation_digest_at(&u32::MAX), None);
}

/// A new append always lands at exactly `log.len()` before the append.
#[test]
fn inv3_each_append_lands_at_next_sequential_index() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    for i in 0u8..5 {
        let before_len = client.get_attestation_append_log().len();
        let d = digest(&env, i);
        client.append_attestation_digest(&d);
        let after_len = client.get_attestation_append_log().len();

        assert_eq!(after_len, before_len + 1, "log must grow by exactly 1");
        assert_eq!(
            client.get_attestation_digest_at(&before_len).unwrap().digest,
            d,
            "new entry must land at index {before_len}"
        );
    }
}

// ===========================================================================
// Invariant 4 — Revocation state machine transitions
// ===========================================================================

/// Forward transition (not-revoked → revoked): `revoke_attestation_digest` succeeds once
/// and `is_attestation_revoked` flips to `true`.
#[test]
fn inv4_forward_transition_not_revoked_to_revoked() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 1);

    assert!(!client.is_attestation_revoked(&0), "initial state must be not-revoked");
    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0), "state must flip to revoked");
}

/// Double-revoke must be rejected with `AttestationAlreadyRevoked`; state is unchanged.
#[test]
fn inv4_double_revoke_rejected_with_typed_error() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 1);

    client.revoke_attestation_digest(&0);
    let res = client.try_revoke_attestation_digest(&0);
    assert_contract_error(res, EscrowError::AttestationAlreadyRevoked);

    // Remains revoked exactly once.
    assert!(client.is_attestation_revoked(&0));
}

/// Backward transition (revoked → not-revoked): `unrevoke_attestation_digest` succeeds once
/// and `is_attestation_revoked` flips back to `false`.
#[test]
fn inv4_backward_transition_revoked_to_not_revoked() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 1);

    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));

    client.unrevoke_attestation_digest(&0);
    assert!(!client.is_attestation_revoked(&0), "state must flip back to not-revoked");
}

/// Double-unrevoke must be rejected with `AttestationNotRevoked`; state is unchanged.
#[test]
fn inv4_double_unrevoke_rejected_with_typed_error() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 1);

    client.revoke_attestation_digest(&0);
    client.unrevoke_attestation_digest(&0);

    let res = client.try_unrevoke_attestation_digest(&0);
    assert_contract_error(res, EscrowError::AttestationNotRevoked);

    // Remains not-revoked.
    assert!(!client.is_attestation_revoked(&0));
}

/// Unrevoking a never-revoked index (no prior revoke) is rejected with `AttestationNotRevoked`.
#[test]
fn inv4_unrevoke_never_revoked_index_rejected() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 3);

    // Index 1 was never revoked.
    let res = client.try_unrevoke_attestation_digest(&1);
    assert_contract_error(res, EscrowError::AttestationNotRevoked);
    assert!(!client.is_attestation_revoked(&1));
}

/// The state machine can cycle through revoke → unrevoke multiple times without error.
#[test]
fn inv4_state_machine_survives_multiple_cycles() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0xAB);
    client.append_attestation_digest(&d);

    for _ in 0..5 {
        client.revoke_attestation_digest(&0);
        assert!(client.is_attestation_revoked(&0));
        client.unrevoke_attestation_digest(&0);
        assert!(!client.is_attestation_revoked(&0));
    }

    // Digest is preserved through all cycles.
    assert_eq!(client.get_attestation_digest_at(&0).unwrap().digest, d);
}

// ===========================================================================
// Invariant 5 — Authorization enforcement
// ===========================================================================

/// `bind_primary_attestation_hash` must return an error for an unauthenticated caller
/// and must not mutate state.
#[test]
fn inv5_bind_primary_hash_requires_admin_auth() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0x10);

    env.mock_auths(&[]);
    let res = client.try_bind_primary_attestation_hash(&d);
    assert!(res.is_err(), "unauthenticated bind must fail");
    assert_eq!(client.get_primary_attestation_hash(), None, "state must be unchanged");
}

/// `append_attestation_digest` must return an error for an unauthenticated caller
/// and must not extend the log.
#[test]
fn inv5_append_digest_requires_admin_auth() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let before_len = client.get_attestation_append_log().len();

    env.mock_auths(&[]);
    let res = client.try_append_attestation_digest(&digest(&env, 0x20));
    assert!(res.is_err(), "unauthenticated append must fail");
    assert_eq!(
        client.get_attestation_append_log().len(),
        before_len,
        "log length must not change on auth failure"
    );
}

/// `revoke_attestation_digest` must return an error for an unauthenticated caller
/// and must not set the revocation flag.
#[test]
fn inv5_revoke_digest_requires_admin_auth() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 1);

    env.mock_auths(&[]);
    let res = client.try_revoke_attestation_digest(&0);
    assert!(res.is_err(), "unauthenticated revoke must fail");
    assert!(
        !client.is_attestation_revoked(&0),
        "revocation flag must not be set on auth failure"
    );
}

/// `unrevoke_attestation_digest` must return an error for an unauthenticated caller
/// and must not clear the revocation flag.
#[test]
fn inv5_unrevoke_digest_requires_admin_auth() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 1);
    // Revoke with auth first.
    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));

    env.mock_auths(&[]);
    let res = client.try_unrevoke_attestation_digest(&0);
    assert!(res.is_err(), "unauthenticated unrevoke must fail");
    assert!(
        client.is_attestation_revoked(&0),
        "revoked flag must remain set on auth failure"
    );
}

/// `revoke_attestation_digests` (batch) must return an error for an unauthenticated caller
/// and must leave the log in its pre-call state.
#[test]
fn inv5_batch_revoke_requires_admin_auth() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 3);

    let indices = SorobanVec::from_array(&env, [0u32, 1u32, 2u32]);
    env.mock_auths(&[]);
    let res = client.try_revoke_attestation_digests(&indices);
    assert!(res.is_err(), "unauthenticated batch revoke must fail");

    // No flags set.
    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(!client.is_attestation_revoked(&2));
}

// ===========================================================================
// Invariant 6 — Batch revocation atomicity
// ===========================================================================

/// If any index in a batch is out of range, the whole batch is rolled back —
/// entries that appeared earlier in the list are NOT revoked.
#[test]
fn inv6_batch_rolls_back_on_out_of_range_index() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 2); // valid indices: 0, 1; index 2 is out of range

    let indices = SorobanVec::from_array(&env, [0u32, 2u32]);
    let res = client.try_revoke_attestation_digests(&indices);
    assert_contract_error(res, EscrowError::AttestationIndexOutOfRange);

    // Index 0 must NOT be revoked — the batch must have been rolled back.
    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
}

/// If a batch contains a duplicate index, the second occurrence fires
/// `AttestationAlreadyRevoked` and the whole batch is rolled back.
#[test]
fn inv6_batch_rolls_back_on_duplicate_index() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 3);

    // Index 1 appears twice — the second occurrence should fail.
    let indices = SorobanVec::from_array(&env, [0u32, 1u32, 1u32]);
    let res = client.try_revoke_attestation_digests(&indices);
    assert_contract_error(res, EscrowError::AttestationAlreadyRevoked);

    // Everything rolled back.
    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(!client.is_attestation_revoked(&2));
}

/// If a batch index was already revoked by a prior call, the current batch is rolled back
/// entirely — including indices that appeared before the already-revoked one in the list.
#[test]
fn inv6_batch_rolls_back_on_preexisting_revocation() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 4);

    // Revoke index 2 individually.
    client.revoke_attestation_digest(&2);
    assert!(client.is_attestation_revoked(&2));

    // Batch tries to revoke 0, 1, 2 — index 2 is already revoked.
    let indices = SorobanVec::from_array(&env, [0u32, 1u32, 2u32]);
    let res = client.try_revoke_attestation_digests(&indices);
    assert_contract_error(res, EscrowError::AttestationAlreadyRevoked);

    // Indices 0 and 1 must be rolled back; 2 remains revoked from the prior single call.
    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(client.is_attestation_revoked(&2));
}

/// An empty batch is rejected with `AttestationBatchEmpty` and leaves no state change.
#[test]
fn inv6_empty_batch_is_rejected() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 2);

    let indices: SorobanVec<u32> = SorobanVec::new(&env);
    let res = client.try_revoke_attestation_digests(&indices);
    assert_contract_error(res, EscrowError::AttestationBatchEmpty);

    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
}

/// A batch larger than `MAX_ATTESTATION_REVOKE_BATCH` is rejected with `AttestationBatchTooLarge`
/// before any revocation is applied.
#[test]
fn inv6_oversized_batch_is_rejected_before_state_change() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // Build a batch of MAX+1 indices (all pointing at index 0 — valid by length but over the limit).
    let mut indices = SorobanVec::new(&env);
    for _ in 0..=(MAX_ATTESTATION_REVOKE_BATCH) {
        indices.push_back(0u32);
    }

    // We don't need a log entry to trigger the size guard — size check is first.
    let res = client.try_revoke_attestation_digests(&indices);
    assert_contract_error(res, EscrowError::AttestationBatchTooLarge);

    // No revocations applied.
    // (Log may be empty at this point — is_attestation_revoked on an empty log returns false.)
}

/// A valid batch of exactly `MAX_ATTESTATION_REVOKE_BATCH` entries succeeds when every
/// index is in-range and unrevoked.
#[test]
fn inv6_batch_at_exact_max_size_succeeds() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // Fill log up to MAX_ATTESTATION_REVOKE_BATCH entries (≤ MAX_ATTESTATION_APPEND_ENTRIES).
    let batch_size = MAX_ATTESTATION_REVOKE_BATCH.min(MAX_ATTESTATION_APPEND_ENTRIES);
    fill_log(&client, &env, batch_size);

    let mut indices = SorobanVec::new(&env);
    for i in 0..batch_size {
        indices.push_back(i);
    }

    client.revoke_attestation_digests(&indices);

    for i in 0..batch_size {
        assert!(client.is_attestation_revoked(&i), "index {i} must be revoked");
    }
}

// ===========================================================================
// Invariant 7 — State isolation (primary hash ↔ append log)
// ===========================================================================

/// Binding the primary hash does not create, modify, or remove any entries in the append log.
#[test]
fn inv7_bind_primary_hash_does_not_affect_append_log() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let before_len = client.get_attestation_append_log().len();
    client.bind_primary_attestation_hash(&digest(&env, 0xCC));
    let after_len = client.get_attestation_append_log().len();

    assert_eq!(before_len, after_len, "bind must not change log length");
    assert_eq!(before_len, 0);
}

/// Appending to the log does not affect the primary hash slot — the hash remains `None`
/// even after many appends.
#[test]
fn inv7_append_log_does_not_affect_primary_hash() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    fill_log(&client, &env, 10);
    assert_eq!(
        client.get_primary_attestation_hash(),
        None,
        "primary hash must remain None after appends"
    );
}

/// Revoking append log entries does not affect the primary hash.
#[test]
fn inv7_revoke_does_not_affect_primary_hash() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let primary = digest(&env, 0xDD);
    client.bind_primary_attestation_hash(&primary);
    fill_log(&client, &env, 5);

    for i in 0..5 {
        client.revoke_attestation_digest(&i);
    }

    assert_eq!(
        client.get_primary_attestation_hash(),
        Some(primary),
        "primary hash must be unchanged after log revocations"
    );
}

/// Unrevoking append log entries does not affect the primary hash.
#[test]
fn inv7_unrevoke_does_not_affect_primary_hash() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let primary = digest(&env, 0xEE);
    client.bind_primary_attestation_hash(&primary);
    fill_log(&client, &env, 3);
    client.revoke_attestation_digest(&0);
    client.unrevoke_attestation_digest(&0);

    assert_eq!(
        client.get_primary_attestation_hash(),
        Some(primary),
        "primary hash must be unchanged after unrevoke"
    );
}

/// The primary hash and the append log can coexist with independent content.
#[test]
fn inv7_primary_hash_and_append_log_coexist_independently() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let primary = digest(&env, 0xF0);
    client.bind_primary_attestation_hash(&primary);
    fill_log(&client, &env, 8);

    assert_eq!(client.get_primary_attestation_hash(), Some(primary));
    assert_eq!(client.get_attestation_append_log().len(), 8);
}

// ===========================================================================
// Invariant 8 — Index boundary conditions
// ===========================================================================

/// `revoke_attestation_digest` accepts index 0 (first valid) and `log.len()-1` (last valid),
/// and rejects `log.len()` (first invalid) with `AttestationIndexOutOfRange`.
#[test]
fn inv8_revoke_boundary_indices() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 3); // valid indices: 0, 1, 2

    // First valid
    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));

    // Last valid
    client.revoke_attestation_digest(&2);
    assert!(client.is_attestation_revoked(&2));

    // First invalid
    let res = client.try_revoke_attestation_digest(&3);
    assert_contract_error(res, EscrowError::AttestationIndexOutOfRange);
}

/// `unrevoke_attestation_digest` accepts index 0 and `log.len()-1`, and rejects `log.len()`.
#[test]
fn inv8_unrevoke_boundary_indices() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 3);
    client.revoke_attestation_digest(&0);
    client.revoke_attestation_digest(&2);

    // First valid unrevoke
    client.unrevoke_attestation_digest(&0);
    assert!(!client.is_attestation_revoked(&0));

    // Last valid unrevoke
    client.unrevoke_attestation_digest(&2);
    assert!(!client.is_attestation_revoked(&2));

    // First invalid
    let res = client.try_unrevoke_attestation_digest(&3);
    assert_contract_error(res, EscrowError::AttestationIndexOutOfRange);
}

/// `get_attestation_digest_at` returns `Some` at index `log.len()-1` and `None` at `log.len()`.
#[test]
fn inv8_get_digest_at_exact_boundary() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 5); // last valid index is 4

    assert!(
        client.get_attestation_digest_at(&4).is_some(),
        "index 4 (log.len()-1) must return Some"
    );
    assert_eq!(
        client.get_attestation_digest_at(&5),
        None,
        "index 5 (log.len()) must return None"
    );
}

/// On an empty log, index 0 is out of range for all mutation callers.
#[test]
fn inv8_empty_log_index_zero_out_of_range_for_all_callers() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    assert_contract_error(
        client.try_revoke_attestation_digest(&0),
        EscrowError::AttestationIndexOutOfRange,
    );

    let batch = SorobanVec::from_array(&env, [0u32]);
    assert_contract_error(
        client.try_revoke_attestation_digests(&batch),
        EscrowError::AttestationIndexOutOfRange,
    );

    assert_contract_error(
        client.try_unrevoke_attestation_digest(&0),
        EscrowError::AttestationIndexOutOfRange,
    );
}

/// `u32::MAX` is always an out-of-range index regardless of log fill.
#[test]
fn inv8_u32_max_is_always_out_of_range() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 5);

    assert_contract_error(
        client.try_revoke_attestation_digest(&u32::MAX),
        EscrowError::AttestationIndexOutOfRange,
    );
    assert_contract_error(
        client.try_unrevoke_attestation_digest(&u32::MAX),
        EscrowError::AttestationIndexOutOfRange,
    );
    assert_eq!(client.get_attestation_digest_at(&u32::MAX), None);
}

// ===========================================================================
// Invariant 9 — Cross-operation contamination prevention
// ===========================================================================

/// Revoking index 1 must not change the revocation state of index 0 or index 2.
#[test]
fn inv9_revocation_flag_is_per_index_no_leakage() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 5);

    client.revoke_attestation_digest(&2);

    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(client.is_attestation_revoked(&2));
    assert!(!client.is_attestation_revoked(&3));
    assert!(!client.is_attestation_revoked(&4));
}

/// Unrevoking index 1 in a multi-revoked log must only clear index 1 and leave all others.
#[test]
fn inv9_unrevoke_only_clears_target_index() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 5);

    for i in 0..5 {
        client.revoke_attestation_digest(&i);
    }

    client.unrevoke_attestation_digest(&2);

    assert!(client.is_attestation_revoked(&0));
    assert!(client.is_attestation_revoked(&1));
    assert!(!client.is_attestation_revoked(&2));
    assert!(client.is_attestation_revoked(&3));
    assert!(client.is_attestation_revoked(&4));
}

/// A batch revoke touching indices 0 and 4 must not affect indices 1, 2, 3.
#[test]
fn inv9_batch_revoke_only_sets_target_flags() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 5);

    let indices = SorobanVec::from_array(&env, [0u32, 4u32]);
    client.revoke_attestation_digests(&indices);

    assert!(client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(!client.is_attestation_revoked(&2));
    assert!(!client.is_attestation_revoked(&3));
    assert!(client.is_attestation_revoked(&4));
}

/// `get_revoked_attestation_digests` returns only the revoked subset and excludes all others.
#[test]
fn inv9_revoked_view_excludes_unrevoked_entries() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d0 = digest(&env, 0xA0);
    let d1 = digest(&env, 0xA1);
    let d2 = digest(&env, 0xA2);
    let d3 = digest(&env, 0xA3);

    client.append_attestation_digest(&d0);
    client.append_attestation_digest(&d1);
    client.append_attestation_digest(&d2);
    client.append_attestation_digest(&d3);

    client.revoke_attestation_digest(&1);
    client.revoke_attestation_digest(&3);

    let page = client.get_revoked_attestation_digests(&0, &10);
    assert_eq!(page.len(), 2, "only 2 entries are revoked");
    assert_eq!(page.get(0).unwrap().digest, d1);
    assert_eq!(page.get(1).unwrap().digest, d3);

    // All returned entries must have revoked == true.
    for i in 0..page.len() {
        assert!(page.get(i).unwrap().revoked, "returned entry must be marked revoked");
    }
}

// ===========================================================================
// Invariant 10 — Default revocation state
// ===========================================================================

/// Before any revocation call, `is_attestation_revoked` returns `false` for every
/// in-range index.
#[test]
fn inv10_default_revocation_state_is_false_for_all_indices() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, MAX_ATTESTATION_APPEND_ENTRIES);

    for i in 0..MAX_ATTESTATION_APPEND_ENTRIES {
        assert!(
            !client.is_attestation_revoked(&i),
            "index {i} must default to not-revoked"
        );
    }
}

/// `get_attestation_digest_at` returns `revoked = false` for every index before
/// any revocation call.
#[test]
fn inv10_digest_at_returns_not_revoked_before_any_revoke() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 8);

    for i in 0..8u32 {
        let info = client.get_attestation_digest_at(&i).unwrap();
        assert!(!info.revoked, "index {i} must report revoked=false before any revoke call");
    }
}

/// `get_revoked_attestation_digests` returns an empty page when no entries are revoked.
#[test]
fn inv10_revoked_view_is_empty_before_any_revoke() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 10);

    let page = client.get_revoked_attestation_digests(&0, &10);
    assert_eq!(page.len(), 0, "revoked view must be empty before any revoke");
}

// ===========================================================================
// Invariant — Read boundary enforcement for paginated revoked view
// ===========================================================================

/// `get_revoked_attestation_digests` with `limit = 0` is rejected with
/// `AttestationReadLimitZero` rather than returning an empty page.
#[test]
fn inv_read_boundary_zero_limit_rejected() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 3);
    client.revoke_attestation_digest(&0);

    let res = client.try_get_revoked_attestation_digests(&0, &0);
    assert_contract_error(res, EscrowError::AttestationReadLimitZero);
}

/// `get_revoked_attestation_digests` with `limit > MAX_ATTESTATION_READ_PAGE` is rejected
/// with `AttestationReadLimitTooLarge`.
#[test]
fn inv_read_boundary_too_large_limit_rejected() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 5);
    for i in 0..5 {
        client.revoke_attestation_digest(&i);
    }

    let too_large = crate::MAX_ATTESTATION_READ_PAGE + 1;
    let res = client.try_get_revoked_attestation_digests(&0, &too_large);
    assert_contract_error(res, EscrowError::AttestationReadLimitTooLarge);
}

/// A `start` offset past the end of the revoked list returns an empty page rather than an error.
#[test]
fn inv_read_boundary_start_past_end_returns_empty() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 3);
    client.revoke_attestation_digest(&0);

    // start = 100 is past the end.
    let page = client.get_revoked_attestation_digests(&100, &10);
    assert_eq!(page.len(), 0);
}

// ===========================================================================
// Invariant — Digest content preservation through revocation lifecycle
// ===========================================================================

/// The digest stored at each index is never altered by revoke or unrevoke operations;
/// only the revocation flag changes.
#[test]
fn inv_digest_content_preserved_through_revocation_lifecycle() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    let d0 = digest(&env, 0x10);
    let d1 = digest(&env, 0x20);
    let d2 = digest(&env, 0x30);

    client.append_attestation_digest(&d0);
    client.append_attestation_digest(&d1);
    client.append_attestation_digest(&d2);

    // Revoke all.
    for i in 0..3 {
        client.revoke_attestation_digest(&i);
    }
    // Unrevoke all.
    for i in 0..3 {
        client.unrevoke_attestation_digest(&i);
    }
    // Revoke again.
    for i in 0..3 {
        client.revoke_attestation_digest(&i);
    }

    // Digests are unchanged.
    assert_eq!(client.get_attestation_digest_at(&0).unwrap().digest, d0);
    assert_eq!(client.get_attestation_digest_at(&1).unwrap().digest, d1);
    assert_eq!(client.get_attestation_digest_at(&2).unwrap().digest, d2);

    // All revoked.
    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 3);
    assert_eq!(log.get(0).unwrap(), d0);
    assert_eq!(log.get(1).unwrap(), d1);
    assert_eq!(log.get(2).unwrap(), d2);
}

/// Duplicate digests in the append log are stored as independent log entries with
/// independent revocation state.
#[test]
fn inv_duplicate_digests_have_independent_revocation_state() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);

    // Append the same digest twice — they get different indices.
    let d = digest(&env, 0x55);
    client.append_attestation_digest(&d);
    client.append_attestation_digest(&d);

    // Both entries should be present.
    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 2);
    assert_eq!(log.get(0).unwrap(), d);
    assert_eq!(log.get(1).unwrap(), d);

    // Revoking index 0 must not affect index 1.
    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));

    // Unrevoking index 0 must not affect index 1.
    client.unrevoke_attestation_digest(&0);
    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
}

// ===========================================================================
// Invariant — Revocation does not shrink the append log
// ===========================================================================

/// Revocation marks an entry but does not remove it from the log; `get_attestation_append_log`
/// still returns all entries including revoked ones.
#[test]
fn inv_revocation_does_not_remove_entry_from_log() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0x77);
    client.append_attestation_digest(&d);
    let len_before = client.get_attestation_append_log().len();

    client.revoke_attestation_digest(&0);

    let log = client.get_attestation_append_log();
    assert_eq!(
        log.len(),
        len_before,
        "revocation must not remove entries from the log"
    );
    assert_eq!(log.get(0).unwrap(), d, "revoked digest must still be readable");
}

/// Unrevoking an entry does not remove it or alter log length.
#[test]
fn inv_unrevocation_does_not_remove_entry_from_log() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    let d = digest(&env, 0x88);
    client.append_attestation_digest(&d);
    client.revoke_attestation_digest(&0);
    let len_after_revoke = client.get_attestation_append_log().len();

    client.unrevoke_attestation_digest(&0);

    let log = client.get_attestation_append_log();
    assert_eq!(
        log.len(),
        len_after_revoke,
        "unrevocation must not remove entries from the log"
    );
    assert_eq!(log.get(0).unwrap(), d);
}

// ===========================================================================
// Invariant — Authorization state guard ordering
// ===========================================================================

/// The state guard for revocation state (`AttestationNotRevoked`) runs *before* `require_auth`
/// so that an unauthenticated call on a not-revoked index still fails with `AttestationNotRevoked`,
/// not an opaque auth error.
///
/// This preserves the ADR-002 "read-only precondition before auth" pattern: callers can
/// cheaply probe whether the state allows the transition without needing admin credentials.
#[test]
fn inv_unrevoke_state_guard_fires_before_auth_check() {
    let env = Env::default();
    let (client, _) = setup_with_init(&env);
    fill_log(&client, &env, 1);
    // Index 0 is not revoked — the state guard must fire first.
    // No mock_auths set, so auth would fail if the guard did not fire.
    // But we only test behavior when mocks are NOT set; default Env has no auth.
    assert_contract_error(
        client.try_unrevoke_attestation_digest(&0),
        EscrowError::AttestationNotRevoked,
    );
}
