// Tests for the admin-only `set_attestation_limit` / `get_attestation_limit` setters:
// default value, in-bounds set, out-of-bounds rejection, non-admin rejection, event emission,
// and enforcement by `append_attestation_digest` / `append_attestation_digests`.

use crate::tests::{assert_contract_error, setup};
use crate::{
    AttestationLimitUpdated, EscrowError, DEFAULT_ATTESTATION_LIMIT, MAX_ATTESTATION_LIMIT,
    MIN_ATTESTATION_LIMIT,
};
use soroban_sdk::{
    symbol_short, testutils::Address as _, testutils::Events as _, Address, BytesN, Env, IntoVal,
    Vec as SorobanVec,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn init_escrow(env: &Env, client: &crate::LiquifactEscrowClient, admin: &Address, sme: &Address) {
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "ATTLIM01"),
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

fn digest(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

// ---------------------------------------------------------------------------
// Default
// ---------------------------------------------------------------------------

/// Before any `set_attestation_limit` call the getter returns `DEFAULT_ATTESTATION_LIMIT`.
#[test]
fn default_attestation_limit_before_init() {
    let env = Env::default();
    let (client, _admin, _sme) = setup(&env);
    assert_eq!(client.get_attestation_limit(), DEFAULT_ATTESTATION_LIMIT);
}

/// Default holds even after init (key is absent until explicitly set).
#[test]
fn default_attestation_limit_after_init() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    assert_eq!(client.get_attestation_limit(), DEFAULT_ATTESTATION_LIMIT);
}

// ---------------------------------------------------------------------------
// In-bounds set
// ---------------------------------------------------------------------------

#[test]
fn admin_sets_attestation_limit_min() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_attestation_limit(&MIN_ATTESTATION_LIMIT);
    assert_eq!(client.get_attestation_limit(), MIN_ATTESTATION_LIMIT);
}

#[test]
fn admin_sets_attestation_limit_max() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_attestation_limit(&MAX_ATTESTATION_LIMIT);
    assert_eq!(client.get_attestation_limit(), MAX_ATTESTATION_LIMIT);
}

#[test]
fn admin_sets_attestation_limit_mid_value() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    let mid = (MIN_ATTESTATION_LIMIT + MAX_ATTESTATION_LIMIT) / 2;
    client.set_attestation_limit(&mid);
    assert_eq!(client.get_attestation_limit(), mid);
}

/// A second in-bounds update overwrites the first.
#[test]
fn admin_can_update_attestation_limit_multiple_times() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_attestation_limit(&5);
    assert_eq!(client.get_attestation_limit(), 5);

    client.set_attestation_limit(&10);
    assert_eq!(client.get_attestation_limit(), 10);

    client.set_attestation_limit(&MIN_ATTESTATION_LIMIT);
    assert_eq!(client.get_attestation_limit(), MIN_ATTESTATION_LIMIT);
}

// ---------------------------------------------------------------------------
// Event emission
// ---------------------------------------------------------------------------

#[test]
fn set_attestation_limit_emits_event_with_old_and_new_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let contract_id = client.address.clone();

    client.set_attestation_limit(&5);

    let all_events = env.events().all();
    assert_eq!(
        all_events.events().last().unwrap().clone(),
        AttestationLimitUpdated {
            name: symbol_short!("att_lim"),
            invoice_id: client.get_escrow().invoice_id,
            old_limit: DEFAULT_ATTESTATION_LIMIT,
            new_limit: 5,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Second call emits old_limit = previously configured value, not the default.
#[test]
fn set_attestation_limit_event_old_limit_tracks_previous_config() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let contract_id = client.address.clone();

    client.set_attestation_limit(&10);
    client.set_attestation_limit(&5);

    let all_events = env.events().all();
    assert_eq!(
        all_events.events().last().unwrap().clone(),
        AttestationLimitUpdated {
            name: symbol_short!("att_lim"),
            invoice_id: client.get_escrow().invoice_id,
            old_limit: 10,
            new_limit: 5,
        }
        .to_xdr(&env, &contract_id)
    );
}

// ---------------------------------------------------------------------------
// Out-of-bounds rejection
// ---------------------------------------------------------------------------

#[test]
fn set_attestation_limit_rejects_zero() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    assert_contract_error(
        client.try_set_attestation_limit(&0),
        EscrowError::AttestationLimitOutOfRange,
    );
    // Limit unchanged
    assert_eq!(client.get_attestation_limit(), DEFAULT_ATTESTATION_LIMIT);
}

#[test]
fn set_attestation_limit_rejects_above_max() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    assert_contract_error(
        client.try_set_attestation_limit(&(MAX_ATTESTATION_LIMIT + 1)),
        EscrowError::AttestationLimitOutOfRange,
    );
    assert_eq!(client.get_attestation_limit(), DEFAULT_ATTESTATION_LIMIT);
}

#[test]
fn set_attestation_limit_rejects_u32_max() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    assert_contract_error(
        client.try_set_attestation_limit(&u32::MAX),
        EscrowError::AttestationLimitOutOfRange,
    );
}

// ---------------------------------------------------------------------------
// Non-admin rejection
// ---------------------------------------------------------------------------

#[test]
fn non_admin_cannot_set_attestation_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let non_admin = Address::generate(&env);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "set_attestation_limit",
            args: SorobanVec::from_array(&env, [5u32.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_set_attestation_limit(&5u32);
    assert!(result.is_err());
    // Limit unchanged
    assert_eq!(client.get_attestation_limit(), DEFAULT_ATTESTATION_LIMIT);
}

// ---------------------------------------------------------------------------
// Enforcement by append_attestation_digest
// ---------------------------------------------------------------------------

/// When limit is lowered to 1, the second append is rejected.
#[test]
fn append_attestation_digest_respects_configured_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_attestation_limit(&1);
    assert_eq!(client.get_attestation_limit(), 1);

    // First append: succeeds.
    client.append_attestation_digest(&digest(&env, 0xAA));
    assert_eq!(client.get_attestation_append_log().len(), 1);

    // Second append: exceeds limit.
    assert_contract_error(
        client.try_append_attestation_digest(&digest(&env, 0xBB)),
        EscrowError::AttestationAppendLogCapacityReached,
    );
    assert_eq!(client.get_attestation_append_log().len(), 1);
}

/// Default limit (32) still allows all 32 appends.
#[test]
fn append_attestation_digest_default_limit_allows_full_log() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    for i in 0..DEFAULT_ATTESTATION_LIMIT {
        client.append_attestation_digest(&digest(&env, i as u8));
    }
    assert_eq!(client.get_attestation_append_log().len(), DEFAULT_ATTESTATION_LIMIT);

    // One more is rejected.
    assert_contract_error(
        client.try_append_attestation_digest(&digest(&env, 0xFF)),
        EscrowError::AttestationAppendLogCapacityReached,
    );
}

// ---------------------------------------------------------------------------
// Enforcement by append_attestation_digests (batch)
// ---------------------------------------------------------------------------

/// Batch append is also gated by the configured limit.
#[test]
fn append_attestation_digests_batch_respects_configured_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_attestation_limit(&2);

    // Batch of 2 fits.
    let batch = SorobanVec::from_array(&env, [digest(&env, 1), digest(&env, 2)]);
    client.append_attestation_digests(&batch);
    assert_eq!(client.get_attestation_append_log().len(), 2);

    // One more (single) is over the limit.
    assert_contract_error(
        client.try_append_attestation_digest(&digest(&env, 3)),
        EscrowError::AttestationAppendLogCapacityReached,
    );
}

/// Raising the limit after appends allows more entries.
#[test]
fn raising_limit_allows_additional_appends() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    // Start at limit 2, fill it.
    client.set_attestation_limit(&2);
    client.append_attestation_digest(&digest(&env, 1));
    client.append_attestation_digest(&digest(&env, 2));

    // Full — next append rejected.
    assert_contract_error(
        client.try_append_attestation_digest(&digest(&env, 3)),
        EscrowError::AttestationAppendLogCapacityReached,
    );

    // Raise limit to 4; now two more are allowed.
    client.set_attestation_limit(&4);
    client.append_attestation_digest(&digest(&env, 3));
    client.append_attestation_digest(&digest(&env, 4));
    assert_eq!(client.get_attestation_append_log().len(), 4);
}

// ---------------------------------------------------------------------------
// Fuzz (proptest)
// ---------------------------------------------------------------------------

use proptest::prelude::*;
proptest! {
    #[test]
    fn fuzz_set_attestation_limit(limit in 0u32..=64u32) {
        let env = Env::default();
        let (client, admin, sme) = setup(&env);
        init_escrow(&env, &client, &admin, &sme);

        let result = client.try_set_attestation_limit(&limit);
        if limit >= MIN_ATTESTATION_LIMIT && limit <= MAX_ATTESTATION_LIMIT {
            assert!(result.is_ok(), "expected ok for limit={limit}");
            assert_eq!(client.get_attestation_limit(), limit);
        } else {
            assert_contract_error(result, EscrowError::AttestationLimitOutOfRange);
        }
    }
}
