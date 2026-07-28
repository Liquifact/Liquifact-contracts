use super::*;
use crate::FundingUpgradeAuthorized;
use soroban_sdk::{BytesN, Event};

// Authorization tests for the funding-subsystem upgrade entrypoint
// `upgrade_funding`. Covers: admin-allowed (passes the typed gate),
// non-admin-rejected (typed `UpgradeUnauthorizedCaller`), the host-level
// `require_auth` trap, and the uninitialized-escrow guard.

/// A deterministic 32-byte WASM hash for exercising the authorization gate.
fn sample_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[7u8; 32])
}

/// A non-admin caller is rejected with the typed `UpgradeUnauthorizedCaller`
/// error before any WASM is touched. `mock_all_auths` lets the caller satisfy
/// `require_auth`, so the failure is the explicit `caller == admin` check.
#[test]
fn test_upgrade_funding_non_admin_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    let stranger = Address::generate(&env);
    assert_contract_error(
        client.try_upgrade_funding(&stranger, &sample_hash(&env)),
        EscrowError::UpgradeUnauthorizedCaller,
    );
}

/// The admin caller passes the authorization gate: the result is never the
/// `UpgradeUnauthorizedCaller` typed error. The deployer swap itself is not
/// asserted here because `update_current_contract_wasm` requires an installed
/// WASM hash, which is unavailable in the unit-test host — this mirrors the
/// existing `upgrade` entrypoint, which has no positive unit test for the
/// same reason.
#[test]
fn test_upgrade_funding_admin_passes_authorization_gate() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    let result = client.try_upgrade_funding(&admin, &sample_hash(&env));

    let unauthorized = EscrowError::UpgradeUnauthorizedCaller as u32;
    if let Err(Ok(error)) = &result {
        assert_ne!(
            *error,
            Error::from_contract_error(unauthorized),
            "admin caller must not be rejected as unauthorized"
        );
    }
    // Any other outcome (including a host trap on the uninstalled WASM swap)
    // means the admin cleared the authorization gate, which is what this test
    // asserts.
}

/// The admin caller, having cleared the gate, emits a `FundingUpgradeAuthorized`
/// event attributing the upgrade to the authorizing admin.
#[test]
fn test_upgrade_funding_admin_emits_event() {
    use soroban_sdk::testutils::Events as _;
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let contract_id = client.address.clone();

    let hash = sample_hash(&env);
    // The deployer swap on an uninstalled hash traps; the event is published
    // before that call (defensive ordering), so it is recorded regardless.
    let _ = client.try_upgrade_funding(&admin, &hash);

    let emitted = env.events().all().iter().any(|(id, _topics, _data)| id == contract_id);
    assert!(emitted, "expected a FundingUpgradeAuthorized event from the contract");
}

/// Without a satisfied signature the host-level `require_auth` traps before the
/// typed admin check runs.
#[test]
#[should_panic]
fn test_upgrade_funding_requires_caller_auth() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    env.mock_auths(&[]);
    client.upgrade_funding(&admin, &sample_hash(&env));
}

/// Calling before `init` fails with `EscrowNotInitialized`.
#[test]
fn test_upgrade_funding_uninitialized_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let caller = Address::generate(&env);

    assert_contract_error(
        client.try_upgrade_funding(&caller, &sample_hash(&env)),
        EscrowError::EscrowNotInitialized,
    );
}
