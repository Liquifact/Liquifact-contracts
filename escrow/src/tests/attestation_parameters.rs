// Boundary tests for attestation parameter constants.
//
// Covers:
//   - append_attestation_digest: succeeds up to MAX_ATTESTATION_APPEND_ENTRIES,
//     fails on the next call with AttestationAppendLogCapacityReached.
//   - revoke_attestation_digests: succeeds with exactly MAX_ATTESTATION_REVOKE_BATCH
//     indices, fails with one more with AttestationBatchTooLarge.

use crate::tests::{assert_contract_error, setup};
use crate::{EscrowError, MAX_ATTESTATION_APPEND_ENTRIES, MAX_ATTESTATION_REVOKE_BATCH};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec as SorobanVec};

fn init_escrow(env: &Env, client: &crate::LiquifactEscrowClient, admin: &Address, sme: &Address) {
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "ATTEST_PARAM_01"),
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
    );
}

fn digest(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

// ---------------------------------------------------------------------------
// append_attestation_digest cap boundary
// ---------------------------------------------------------------------------

/// Appending exactly MAX_ATTESTATION_APPEND_ENTRIES digests succeeds.
/// The next append must fail with AttestationAppendLogCapacityReached.
#[test]
fn append_cap_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Fill the log to the cap.
    for i in 0..MAX_ATTESTATION_APPEND_ENTRIES {
        // Use a unique seed per entry; wrap to u8 range.
        client.append_attestation_digest(&digest(&env, (i % 256) as u8));
    }

    // One more must exceed the cap.
    let over = client.try_append_attestation_digest(&digest(&env, 0xFFu8));
    assert_contract_error(over, EscrowError::AttestationAppendLogCapacityReached);
}

/// Appending one fewer than the cap succeeds; the cap entry itself also succeeds.
#[test]
fn append_exactly_at_cap_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    for i in 0..MAX_ATTESTATION_APPEND_ENTRIES - 1 {
        client.append_attestation_digest(&digest(&env, i as u8));
    }
    // The cap-th entry must still succeed.
    client.append_attestation_digest(&digest(&env, 0xFEu8));
}

// ---------------------------------------------------------------------------
// revoke_attestation_digests batch boundary
// ---------------------------------------------------------------------------

/// revoke_attestation_digests with exactly MAX_ATTESTATION_REVOKE_BATCH indices succeeds.
/// One more index must fail with AttestationBatchTooLarge.
#[test]
fn revoke_batch_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // We need at least MAX_ATTESTATION_REVOKE_BATCH + 1 entries in the log.
    let needed = MAX_ATTESTATION_REVOKE_BATCH + 1;
    for i in 0..needed {
        client.append_attestation_digest(&digest(&env, i as u8));
    }

    // Build an index vec of exactly MAX_ATTESTATION_REVOKE_BATCH entries.
    let mut indices = SorobanVec::new(&env);
    for i in 0..MAX_ATTESTATION_REVOKE_BATCH {
        indices.push_back(i);
    }
    client.revoke_attestation_digests(&indices);

    // Reset: re-add digests so indices are valid for the over-limit call.
    // (A fresh escrow is simpler — redeploy.)
    let env2 = Env::default();
    env2.mock_all_auths();
    let (client2, admin2, sme2) = setup(&env2);
    init_escrow(&env2, &client2, &admin2, &sme2);
    for i in 0..(MAX_ATTESTATION_REVOKE_BATCH + 1) {
        client2.append_attestation_digest(&digest(&env2, i as u8));
    }

    let mut over_indices = SorobanVec::new(&env2);
    for i in 0..(MAX_ATTESTATION_REVOKE_BATCH + 1) {
        over_indices.push_back(i);
    }
    let over = client2.try_revoke_attestation_digests(&over_indices);
    assert_contract_error(over, EscrowError::AttestationBatchTooLarge);
}

/// revoke_attestation_digests with exactly one index below the batch cap succeeds.
#[test]
fn revoke_single_index_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.append_attestation_digest(&digest(&env, 1u8));

    let mut indices = SorobanVec::new(&env);
    indices.push_back(0u32);
    client.revoke_attestation_digests(&indices);
}
