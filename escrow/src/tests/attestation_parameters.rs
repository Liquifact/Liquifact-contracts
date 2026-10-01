//! Boundary tests for attestation parameters and their deterministic behavior.

use crate::tests::{assert_contract_error, default_init, setup};
use crate::{
    EscrowError, LiquifactEscrowClient, MAX_ATTESTATION_APPEND_ENTRIES, MAX_ATTESTATION_READ_PAGE,
    MAX_ATTESTATION_REVOKE_BATCH,
};
use soroban_sdk::{BytesN, Env, Vec};

fn initialized_client(env: &Env) -> LiquifactEscrowClient<'_> {
    let (client, admin, sme) = setup(env);
    default_init(&client, env, &admin, &sme);
    client
}

fn digest(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

#[test]
fn duplicate_digest_is_preserved_through_append_capacity_boundary() {
    let env = Env::default();
    let client = initialized_client(&env);
    let repeated = digest(&env, 0xA5);

    for _ in 0..MAX_ATTESTATION_APPEND_ENTRIES {
        client.append_attestation_digest(&repeated);
    }

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(log.get(0).unwrap(), repeated);
    assert_eq!(
        log.get(MAX_ATTESTATION_APPEND_ENTRIES - 1).unwrap(),
        repeated
    );
    assert_contract_error(
        client.try_append_attestation_digest(&repeated),
        EscrowError::AttestationAppendLogCapacityReached,
    );
    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES
    );
}

#[test]
fn attestation_page_clamps_large_limit_and_handles_extreme_start() {
    let env = Env::default();
    let client = initialized_client(&env);
    let mut indices = Vec::new(&env);

    for index in 0..MAX_ATTESTATION_APPEND_ENTRIES {
        client.append_attestation_digest(&digest(&env, index as u8));
        indices.push_back(index);
    }
    client.revoke_attestation_digests(&indices);

    let first_page = client.get_attestation_digests(&0, &(MAX_ATTESTATION_READ_PAGE + 1));
    assert_eq!(first_page.len(), MAX_ATTESTATION_READ_PAGE);
    for index in 0..MAX_ATTESTATION_READ_PAGE {
        let entry = first_page.get(index).unwrap();
        assert_eq!(entry.digest, digest(&env, index as u8));
        assert!(entry.revoked);
    }

    let final_page = client.get_attestation_digests(&MAX_ATTESTATION_READ_PAGE, &u32::MAX);
    assert_eq!(
        final_page.len(),
        MAX_ATTESTATION_APPEND_ENTRIES - MAX_ATTESTATION_READ_PAGE
    );
    assert_eq!(client.get_attestation_digests(&u32::MAX, &1).len(), 0);
    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES
    );
}

#[test]
fn revoked_attestation_page_enforces_limit_boundaries() {
    let env = Env::default();
    let client = initialized_client(&env);
    let mut indices = Vec::new(&env);

    for index in 0..MAX_ATTESTATION_REVOKE_BATCH {
        client.append_attestation_digest(&digest(&env, index as u8));
        indices.push_back(index);
    }
    client.revoke_attestation_digests(&indices);

    assert_contract_error(
        client.try_get_revoked_attestation_digests(&0, &0),
        EscrowError::AttestationReadLimitZero,
    );
    assert_contract_error(
        client.try_get_revoked_attestation_digests(&0, &(MAX_ATTESTATION_READ_PAGE + 1)),
        EscrowError::AttestationReadLimitTooLarge,
    );

    let first_page = client.get_revoked_attestation_digests(&0, &MAX_ATTESTATION_READ_PAGE);
    assert_eq!(first_page.len(), MAX_ATTESTATION_READ_PAGE);
    for index in 0..MAX_ATTESTATION_READ_PAGE {
        assert_eq!(
            first_page.get(index).unwrap().digest,
            digest(&env, index as u8)
        );
    }

    let final_page = client
        .get_revoked_attestation_digests(&MAX_ATTESTATION_READ_PAGE, &MAX_ATTESTATION_READ_PAGE);
    assert_eq!(
        final_page.len(),
        MAX_ATTESTATION_REVOKE_BATCH - MAX_ATTESTATION_READ_PAGE
    );
    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_REVOKE_BATCH
    );
}

#[test]
fn serialized_appends_competing_for_final_slot_do_not_overwrite() {
    let env = Env::default();
    let client = initialized_client(&env);

    for index in 0..(MAX_ATTESTATION_APPEND_ENTRIES - 1) {
        client.append_attestation_digest(&digest(&env, index as u8));
    }

    let first = digest(&env, 0xA1);
    let competing = digest(&env, 0xB2);
    client.append_attestation_digest(&first);

    assert_contract_error(
        client.try_append_attestation_digest(&competing),
        EscrowError::AttestationAppendLogCapacityReached,
    );

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(log.get(MAX_ATTESTATION_APPEND_ENTRIES - 1).unwrap(), first);
}

#[test]
fn overlapping_revoke_batch_rolls_back_when_competing_revoke_won() {
    let env = Env::default();
    let client = initialized_client(&env);
    client.append_attestation_digest(&digest(&env, 0x01));
    client.append_attestation_digest(&digest(&env, 0x02));

    client.revoke_attestation_digest(&1);

    let mut competing_batch = Vec::new(&env);
    competing_batch.push_back(0);
    competing_batch.push_back(1);
    assert_contract_error(
        client.try_revoke_attestation_digests(&competing_batch),
        EscrowError::AttestationAlreadyRevoked,
    );

    assert!(!client.is_attestation_revoked(&0));
    assert!(client.is_attestation_revoked(&1));
}
