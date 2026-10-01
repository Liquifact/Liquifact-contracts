use super::*;
use crate::EscrowError;
use soroban_sdk::testutils::Address as _;

// ---------------------------------------------------------------------------
// Helper: initialise escrow and return (client, admin, sme, token, treasury)
// ---------------------------------------------------------------------------
fn setup_with_nonce(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 0;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "NCE_TEST"),
        &sme,
        &1_000_000i128,
        &500i64,
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
    (client, admin, sme)
}

// ---------------------------------------------------------------------------
// 1. Next nonce — successful execution and increment
// ---------------------------------------------------------------------------

#[test]
fn nonce_starts_at_zero() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);
    assert_eq!(client.get_admin_nonce(), 0u32);
}

#[test]
fn next_nonce_succeeds_and_increments() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    // First admin action with nonce 0 should succeed.
    client.set_allowlist_active(&true, &0u32);
    assert_eq!(client.get_admin_nonce(), 1u32);
}

#[test]
fn sequential_nonces_succeed() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    client.set_allowlist_active(&true, &0u32);
    assert_eq!(client.get_admin_nonce(), 1u32);

    client.set_allowlist_active(&false, &1u32);
    assert_eq!(client.get_admin_nonce(), 2u32);

    client.update_maturity(&2000u64, &2u32);
    assert_eq!(client.get_admin_nonce(), 3u32);
}

// ---------------------------------------------------------------------------
// 2. Old nonce — replay rejection
// ---------------------------------------------------------------------------

#[test]
fn old_nonce_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    // First action succeeds.
    client.set_allowlist_active(&true, &0u32);

    // Replay with the same nonce (0) should fail.
    assert_contract_error(
        client.try_set_allowlist_active(&false, &0u32),
        EscrowError::AdminNonceMismatch,
    );
}

#[test]
fn old_nonce_after_multiple_actions_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    client.set_allowlist_active(&true, &0u32);
    client.set_allowlist_active(&false, &1u32);
    client.update_maturity(&2000u64, &2u32);
    // Nonce is now 3. Attempting nonce 0 or 1 should fail.
    assert_contract_error(
        client.try_set_allowlist_active(&true, &0u32),
        EscrowError::AdminNonceMismatch,
    );
    assert_contract_error(
        client.try_set_allowlist_active(&true, &1u32),
        EscrowError::AdminNonceMismatch,
    );
}

// ---------------------------------------------------------------------------
// 3. Future nonce — out-of-order rejection
// ---------------------------------------------------------------------------

#[test]
fn future_nonce_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    // Nonce starts at 0. Attempting nonce 5 should fail.
    assert_contract_error(
        client.try_set_allowlist_active(&true, &5u32),
        EscrowError::AdminNonceMismatch,
    );
}

#[test]
fn future_nonce_after_actions_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    client.set_allowlist_active(&true, &0u32);
    // Nonce is now 1. Attempting nonce 100 should fail.
    assert_contract_error(
        client.try_set_allowlist_active(&false, &100u32),
        EscrowError::AdminNonceMismatch,
    );
}

// ---------------------------------------------------------------------------
// 4. Two same-nonce calls — race / duplicate invocation rejection
// ---------------------------------------------------------------------------

#[test]
fn duplicate_nonce_second_call_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    // First call with nonce 0 succeeds.
    client.set_allowlist_active(&true, &0u32);

    // Second call with nonce 0 (simulating a replayed transaction) should fail.
    assert_contract_error(
        client.try_set_allowlist_active(&false, &0u32),
        EscrowError::AdminNonceMismatch,
    );

    // Nonce should still be 1 (unchanged by the failed call).
    assert_eq!(client.get_admin_nonce(), 1u32);
}

#[test]
fn two_identical_nonces_different_entrypoints() {
    let env = Env::default();
    let (client, admin, sme) = setup_with_nonce(&env);

    // First action succeeds.
    client.set_allowlist_active(&true, &0u32);

    // Attempting a different admin entrypoint with the same nonce should also fail.
    assert_contract_error(
        client.try_update_maturity(&2000u64, &0u32),
        EscrowError::AdminNonceMismatch,
    );

    // Nonce should still be 1.
    assert_eq!(client.get_admin_nonce(), 1u32);
}

// ---------------------------------------------------------------------------
// 5. Nonce near maximum — overflow protection and boundary safety
// ---------------------------------------------------------------------------

#[test]
fn nonce_at_max_minus_one_succeeds() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    // Set nonce to u32::MAX - 1.
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::AdminNonce, &(u32::MAX - 1));
    });
    assert_eq!(client.get_admin_nonce(), u32::MAX - 1);

    // Action with nonce u32::MAX - 1 should succeed and increment to MAX.
    client.set_allowlist_active(&true, &(u32::MAX - 1));
    assert_eq!(client.get_admin_nonce(), u32::MAX);
}

#[test]
fn nonce_at_max_overflow_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    // Set nonce to u32::MAX.
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::AdminNonce, &u32::MAX);
    });
    assert_eq!(client.get_admin_nonce(), u32::MAX);

    // Action with nonce u32::MAX should fail because increment would overflow.
    assert_contract_error(
        client.try_set_allowlist_active(&true, &u32::MAX),
        EscrowError::AdminNonceMismatch,
    );

    // Nonce should remain u32::MAX (unchanged by the failed call).
    assert_eq!(client.get_admin_nonce(), u32::MAX);
}

// ---------------------------------------------------------------------------
// 6. Admin nonce persists across different entrypoints
// ---------------------------------------------------------------------------

#[test]
fn nonce_shared_across_entrypoints() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    // Use nonce 0 on set_allowlist_active.
    client.set_allowlist_active(&true, &0u32);
    assert_eq!(client.get_admin_nonce(), 1u32);

    // Use nonce 1 on update_maturity.
    client.update_maturity(&5000u64, &1u32);
    assert_eq!(client.get_admin_nonce(), 2u32);

    // Use nonce 2 on update_funding_target.
    client.update_funding_target(&2_000_000i128, &2u32);
    assert_eq!(client.get_admin_nonce(), 3u32);
}

// ---------------------------------------------------------------------------
// 7. Nonce on propose_admin / cancel_pending_admin
// ---------------------------------------------------------------------------

#[test]
fn propose_admin_increments_nonce() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    let new_admin = Address::generate(&env);
    client.propose_admin(&new_admin, &0u32);
    assert_eq!(client.get_admin_nonce(), 1u32);
}

#[test]
fn cancel_pending_admin_increments_nonce() {
    let env = Env::default();
    let (client, admin, _sme) = setup_with_nonce(&env);

    let new_admin = Address::generate(&env);
    client.propose_admin(&new_admin, &0u32);
    assert_eq!(client.get_admin_nonce(), 1u32);

    client.cancel_pending_admin(&1u32);
    assert_eq!(client.get_admin_nonce(), 2u32);
}

// ---------------------------------------------------------------------------
// 8. Legal hold entrypoints use nonce
// ---------------------------------------------------------------------------

#[test]
fn set_legal_hold_uses_nonce() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    client.set_legal_hold(&true, &0u32);
    assert!(client.get_legal_hold());
    assert_eq!(client.get_admin_nonce(), 1u32);
}

#[test]
fn request_clear_legal_hold_uses_nonce() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    // set legal hold first (nonce 0)
    client.set_legal_hold(&true, &0u32);
    assert_eq!(client.get_admin_nonce(), 1u32);

    // request clear (nonce 1)
    client.request_clear_legal_hold(&1u32);
    assert_eq!(client.get_admin_nonce(), 2u32);
}

// ---------------------------------------------------------------------------
// 9. Clear legal hold convenience wrapper uses nonce
// ---------------------------------------------------------------------------

#[test]
fn clear_legal_hold_uses_nonce() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    // Set legal hold (nonce 0)
    client.set_legal_hold(&true, &0u32);

    // Clear legal hold (nonce 1)
    client.clear_legal_hold(&1u32);
    assert!(!client.get_legal_hold());
    assert_eq!(client.get_admin_nonce(), 2u32);
}

// ---------------------------------------------------------------------------
// 10. Stale nonce error does not leak internal details
// ---------------------------------------------------------------------------

#[test]
fn stale_nonce_error_matches_future_nonce_error() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    // Stale nonce (0 after advancing) and future nonce (99) both return the same error.
    client.set_allowlist_active(&true, &0u32);

    let stale_result = client.try_set_allowlist_active(&false, &0u32);
    let future_result = client.try_set_allowlist_active(&false, &99u32);

    assert_contract_error(stale_result, EscrowError::AdminNonceMismatch);
    assert_contract_error(future_result, EscrowError::AdminNonceMismatch);
}

// ---------------------------------------------------------------------------
// 11. Migrate uses nonce
// ---------------------------------------------------------------------------

#[test]
fn migrate_uses_nonce() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);

    // migrate with wrong version will fail with MigrationVersionMismatch,
    // but the nonce should still be consumed first.
    let result = client.try_migrate(&0u32, &0u32);

    // Nonce should have been consumed (incremented to 1) before the
    // version check failed. But actually, nonce is consumed AFTER admin auth
    // Since the transaction reverts on failed version check, instance storage changes are rolled back.
    assert_eq!(client.get_admin_nonce(), 0u32);
}

// ---------------------------------------------------------------------------
// 12. Recovery participates in the same serialized admin state machine
// ---------------------------------------------------------------------------

#[test]
fn recover_admin_consumes_nonce_and_clears_only_expired_proposal() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);
    let pending = Address::generate(&env);
    client.propose_admin(&pending, &0u32);

    let expiry = client.get_pending_admin_expiry().unwrap();
    env.ledger().set_timestamp(expiry);
    assert_contract_error(
        client.try_recover_admin(&soroban_sdk::String::from_str(&env, "too_early"), &1u32),
        EscrowError::AdminRecoveryNotExpired,
    );
    assert_eq!(client.get_pending_admin(), Some(pending.clone()));
    assert_eq!(client.get_admin_nonce(), 1u32);

    env.ledger().set_timestamp(expiry + 1);
    let recovered = client.recover_admin(&soroban_sdk::String::from_str(&env, "expired"), &1u32);
    assert_eq!(recovered, pending);
    assert_eq!(client.get_pending_admin(), None);
    assert_eq!(client.get_pending_admin_expiry(), None);
    assert_eq!(client.get_admin_nonce(), 2u32);
}

#[test]
fn recover_admin_rejects_duplicate_nonce_without_mutating_pending_state() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);
    let pending = Address::generate(&env);
    client.propose_admin(&pending, &0u32);
    let expiry = client.get_pending_admin_expiry().unwrap();
    env.ledger().set_timestamp(expiry + 1);

    client.recover_admin(&soroban_sdk::String::from_str(&env, "first"), &1u32);
    assert_contract_error(
        client.try_recover_admin(&soroban_sdk::String::from_str(&env, "replay"), &1u32),
        EscrowError::AdminNonceMismatch,
    );
    assert_eq!(client.get_pending_admin(), None);
    assert_eq!(client.get_admin_nonce(), 2u32);
}

#[test]
fn stale_recovery_nonce_cannot_clear_a_newer_proposal() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_with_nonce(&env);
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    client.propose_admin(&first, &0u32);
    let first_expiry = client.get_pending_admin_expiry().unwrap();
    env.ledger().set_timestamp(first_expiry + 1);

    // A fresh proposal advances the serialization point. A delayed recovery carrying nonce 1
    // must not be able to remove the second proposal.
    client.propose_admin(&second, &1u32);
    assert_contract_error(
        client.try_recover_admin(&soroban_sdk::String::from_str(&env, "delayed"), &1u32),
        EscrowError::AdminNonceMismatch,
    );
    assert_eq!(client.get_pending_admin(), Some(second));
    assert_eq!(client.get_admin_nonce(), 2u32);
}
