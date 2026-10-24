//! Attestor authorization, append uniqueness, historical attribution, and event compatibility.
use super::*;
use crate::{AttestationAuthored, AttestorChanged};
use soroban_sdk::{
    testutils::{MockAuth, MockAuthInvoke},
    BytesN, IntoVal,
};

fn initialized(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    let (client, admin, sme) = setup(env);
    default_init(&client, env, &admin, &sme);
    (client, admin, Address::generate(env))
}

fn digest(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

fn authorize(
    env: &Env,
    client: &LiquifactEscrowClient<'_>,
    signer: &Address,
    name: &str,
    args: SorobanVec<Val>,
) {
    env.mock_auths(&[MockAuth {
        address: signer,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: name,
            args,
            sub_invokes: &[],
        },
    }]);
}

#[test]
fn admin_controls_role_and_rotation_preserves_authors() {
    let env = Env::default();
    let (client, admin, attestor) = initialized(&env);
    let d = digest(&env, 1);
    assert_contract_error(
        client.try_append_attestation_digest_as(&attestor, &d),
        EscrowError::NotAttestor,
    );
    client.append_attestation_digest(&d);
    client.set_attestor(&Some(attestor.clone()));
    client.append_attestation_digest_as(&attestor, &digest(&env, 2));
    let next = Address::generate(&env);
    client.set_attestor(&Some(next.clone()));
    assert_contract_error(
        client.try_append_attestation_digest_as(&attestor, &digest(&env, 3)),
        EscrowError::NotAttestor,
    );
    client.append_attestation_digest_as(&next, &digest(&env, 3));
    client.set_attestor(&None);
    assert_contract_error(
        client.try_append_attestation_digest_as(&next, &digest(&env, 4)),
        EscrowError::NotAttestor,
    );
    client.append_attestation_digest(&digest(&env, 4));
    assert_eq!(client.get_attestation_author(&0), Some(admin.clone()));
    assert_eq!(client.get_attestation_author(&1), Some(attestor));
    assert_eq!(client.get_attestation_author(&2), Some(next));
    assert_eq!(client.get_attestation_author(&3), Some(admin));
    assert_eq!(client.get_attestation_author(&4), None);
    assert_eq!(client.get_attestation_author(&u32::MAX), None);
}

#[test]
fn attestor_identity_and_signature_are_both_required() {
    let env = Env::default();
    let (client, admin, attestor) = initialized(&env);
    client.set_attestor(&Some(attestor.clone()));
    let d = digest(&env, 1);
    // Even a valid admin signature does not make the admin the designated attestor.
    assert_contract_error(
        client.try_append_attestation_digest_as(&admin, &d),
        EscrowError::NotAttestor,
    );
    env.mock_auths(&[]);
    assert!(client
        .try_append_attestation_digest_as(&attestor, &d)
        .is_err());
    authorize(
        &env,
        &client,
        &admin,
        "append_attestation_digest_as",
        (attestor.clone(), d.clone()).into_val(&env),
    );
    assert!(client
        .try_append_attestation_digest_as(&attestor, &d)
        .is_err());
    assert!(client.get_attestation_append_log().is_empty());
    assert_eq!(client.get_attestation_author(&0), None);
    authorize(
        &env,
        &client,
        &attestor,
        "append_attestation_digest_as",
        (attestor.clone(), d.clone()).into_val(&env),
    );
    client.append_attestation_digest_as(&attestor, &d);
    assert_eq!(env.auths()[0].0, attestor);
}

#[test]
fn attestor_cannot_set_clear_bind_revoke_unrevoke_or_append_as_admin() {
    let env = Env::default();
    let (client, admin, attestor) = initialized(&env);
    client.set_attestor(&Some(attestor.clone()));
    let d = digest(&env, 1);
    client.append_attestation_digest_as(&attestor, &d);
    for role in [None, Some(admin.clone())] {
        env.mock_auths(&[]);
        assert!(client.try_set_attestor(&role).is_err());
        authorize(
            &env,
            &client,
            &attestor,
            "set_attestor",
            (role.clone(),).into_val(&env),
        );
        assert!(client.try_set_attestor(&role).is_err());
    }
    authorize(
        &env,
        &client,
        &attestor,
        "append_attestation_digest",
        (digest(&env, 2),).into_val(&env),
    );
    assert!(client
        .try_append_attestation_digest(&digest(&env, 2))
        .is_err());
    authorize(
        &env,
        &client,
        &attestor,
        "bind_primary_attestation_hash",
        (digest(&env, 3),).into_val(&env),
    );
    assert!(client
        .try_bind_primary_attestation_hash(&digest(&env, 3))
        .is_err());
    authorize(
        &env,
        &client,
        &attestor,
        "revoke_attestation_digest",
        (0u32,).into_val(&env),
    );
    assert!(client.try_revoke_attestation_digest(&0).is_err());
    authorize(
        &env,
        &client,
        &admin,
        "revoke_attestation_digest",
        (0u32,).into_val(&env),
    );
    client.revoke_attestation_digest(&0);
    authorize(
        &env,
        &client,
        &attestor,
        "unrevoke_attestation_digest",
        (0u32,).into_val(&env),
    );
    assert!(client.try_unrevoke_attestation_digest(&0).is_err());
    assert!(client.get_attestation_digest_at(&0).unwrap().revoked);
    assert_eq!(client.get_attestation_author(&0), Some(attestor.clone()));
    authorize(
        &env,
        &client,
        &admin,
        "unrevoke_attestation_digest",
        (0u32,).into_val(&env),
    );
    client.unrevoke_attestation_digest(&0);
    assert!(!client.get_attestation_digest_at(&0).unwrap().revoked);
    // Failed role changes left the original attestor in place.
    authorize(
        &env,
        &client,
        &attestor,
        "append_attestation_digest_as",
        (attestor.clone(), digest(&env, 2)).into_val(&env),
    );
    client.append_attestation_digest_as(&attestor, &digest(&env, 2));
}

#[test]
fn both_paths_reject_primary_cross_path_and_revoked_duplicates_atomically() {
    let env = Env::default();
    let (client, _, attestor) = initialized(&env);
    client.set_attestor(&Some(attestor.clone()));
    client.bind_primary_attestation_hash(&digest(&env, 0));
    client.append_attestation_digest(&digest(&env, 1));
    client.append_attestation_digest_as(&attestor, &digest(&env, 2));
    client.revoke_attestation_digest(&1);
    let before = client.get_attestation_append_log();
    for byte in 0..=2 {
        let d = digest(&env, byte);
        assert_contract_error(
            client.try_append_attestation_digest(&d),
            EscrowError::AttestationDigestDuplicate,
        );
        assert_contract_error(
            client.try_append_attestation_digest_as(&attestor, &d),
            EscrowError::AttestationDigestDuplicate,
        );
        assert_eq!(client.get_attestation_append_log(), before);
        assert_eq!(client.get_attestation_author(&2), None);
    }
    assert!(client.get_attestation_digest_at(&1).unwrap().revoked);
    assert_eq!(client.get_attestation_author(&1), Some(attestor));
}

#[test]
fn capacity_is_shared_and_full_log_still_rejects_replays() {
    let env = Env::default();
    let (client, _, attestor) = initialized(&env);
    client.set_attestor(&Some(attestor.clone()));
    for i in 0..MAX_ATTESTATION_APPEND_ENTRIES {
        if i % 2 == 0 {
            client.append_attestation_digest(&digest(&env, i as u8));
        } else {
            client.append_attestation_digest_as(&attestor, &digest(&env, i as u8));
        }
    }
    let before = client.get_attestation_append_log();
    assert_contract_error(
        client.try_append_attestation_digest(&digest(&env, 32)),
        EscrowError::AttestationAppendLogCapacityReached,
    );
    assert_contract_error(
        client.try_append_attestation_digest_as(&attestor, &digest(&env, 32)),
        EscrowError::AttestationAppendLogCapacityReached,
    );
    assert_contract_error(
        client.try_append_attestation_digest_as(&attestor, &digest(&env, 0)),
        EscrowError::AttestationDigestDuplicate,
    );
    assert_eq!(client.get_attestation_append_log(), before);
    assert_eq!(client.get_attestation_author(&32), None);
}

#[test]
fn legacy_entries_have_no_author_and_still_reject_duplicates() {
    let env = Env::default();
    let (client, admin, _) = initialized(&env);
    let d = digest(&env, 1);
    env.as_contract(&client.address, || {
        env.storage().instance().set(
            &DataKey::AttestationAppendLog,
            &soroban_sdk::vec![&env, d.clone()],
        );
    });
    assert_eq!(client.get_attestation_author(&0), None);
    assert_contract_error(
        client.try_append_attestation_digest(&d),
        EscrowError::AttestationDigestDuplicate,
    );
    client.append_attestation_digest(&digest(&env, 2));
    assert_eq!(client.get_attestation_author(&0), None);
    assert_eq!(client.get_attestation_author(&1), Some(admin.clone()));
    env.as_contract(&client.address, || {
        assert_eq!(
            env.storage()
                .persistent()
                .get::<_, Address>(&DataKey::AttestationAuthor(1)),
            Some(admin)
        );
        assert!(!env.storage().instance().has(&DataKey::AttestationAuthor(1)));
    });
}

#[test]
fn role_and_append_events_preserve_existing_payload_and_add_attribution() {
    let env = Env::default();
    let (client, admin, attestor) = initialized(&env);
    let invoice_id = client.get_escrow().invoice_id;
    for role in [Some(attestor.clone()), None, Some(attestor.clone())] {
        client.set_attestor(&role);
        assert_eq!(
            env.events().all().events().last().unwrap().clone(),
            AttestorChanged {
                name: symbol_short!("att_role"),
                invoice_id: invoice_id.clone(),
                attestor: role,
            }
            .to_xdr(&env, &client.address)
        );
    }
    for (index, author) in [(0u32, admin), (1u32, attestor.clone())] {
        let d = digest(&env, index as u8);
        if index == 0 {
            client.append_attestation_digest(&d);
        } else {
            client.append_attestation_digest_as(&attestor, &d);
        }
        let snapshot = env.events().all();
        let events = snapshot.events();
        assert_eq!(
            events.get(events.len() - 2).unwrap().clone(),
            AttestationAuthored {
                name: symbol_short!("att_by"),
                invoice_id: invoice_id.clone(),
                index,
                author,
            }
            .to_xdr(&env, &client.address)
        );
        assert_eq!(
            events.last().unwrap().clone(),
            AttestationDigestAppended {
                name: symbol_short!("att_app"),
                invoice_id: invoice_id.clone(),
                index,
                digest: d,
            }
            .to_xdr(&env, &client.address)
        );
    }
}

#[test]
fn role_entrypoints_require_initialized_escrow() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    let attestor = Address::generate(&env);
    assert_contract_error(
        client.try_set_attestor(&Some(attestor.clone())),
        EscrowError::EscrowNotInitialized,
    );
    assert_contract_error(
        client.try_append_attestation_digest_as(&attestor, &digest(&env, 1)),
        EscrowError::EscrowNotInitialized,
    );
    assert_eq!(client.get_attestation_author(&0), None);
}

#[test]
fn attestor_append_works_after_real_token_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = init_and_fund_with_real_token(&env, 1000, "ATT_FUNDED");
    let attestor = Address::generate(&env);
    client.set_attestor(&Some(attestor.clone()));
    client.append_attestation_digest_as(&attestor, &digest(&env, 1));
    assert_eq!(client.get_attestation_author(&0), Some(attestor));
}
