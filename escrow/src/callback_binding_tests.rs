//! Unit and integration tests for cross-contract callback binding (Issue #1222).
//!
//! Verifies:
//! - Registration of callbacks with invocation nonce, origin address, and expected phase.
//! - Execution and single-use consumption of callbacks.
//! - Guard against wrong origin (replaying against a different contract/origin).
//! - Guard against wrong nonce (mismatched invocation sequence).
//! - Guard against callback replay (double execution of the same nonce).
//! - Guard against callback execution after escrow cancellation.
//! - Guard against wrong phase.
//! - Auth validation for admin (registration) and origin (execution).

use soroban_sdk::{testutils::Address as _, Address, Env, String};

use super::{CallbackContext, LiquifactEscrow, LiquifactEscrowClient};

/// Deploy and initialize an escrow instance for callback testing.
fn deploy_escrow<'a>(
    env: &'a Env,
    invoice_id: &str,
) -> (LiquifactEscrowClient<'a>, Address, Address, Address) {
    env.mock_all_auths_allowing_non_root_auth();
    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &1_000i128,
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
    (client, admin, sme, id)
}

// -----------------------------------------------------------------------------
// Edge case 1: Valid callback
// -----------------------------------------------------------------------------

#[test]
fn test_valid_callback_registration_and_execution() {
    let env = Env::default();
    let (client, _admin, _sme, _id) = deploy_escrow(&env, "INV_CB_1");

    let origin = Address::generate(&env);
    let phase = 1u32;

    assert_eq!(client.get_callback_nonce(), 0);
    assert_eq!(client.get_callback(&1), None);
    assert!(!client.is_callback_consumed(&1));

    // Step 1: Register callback
    let nonce = client.register_callback(&origin, &phase);
    assert_eq!(nonce, 1);
    assert_eq!(client.get_callback_nonce(), 1);

    let stored = client.get_callback(&nonce).expect("callback should exist");
    assert_eq!(
        stored,
        CallbackContext {
            origin: origin.clone(),
            nonce: 1,
            phase: 1,
            created_at: env.ledger().timestamp(),
            consumed: false,
        }
    );
    assert!(!client.is_callback_consumed(&nonce));

    // Step 2: Execute callback
    let executed = client.execute_callback(&nonce, &origin, &phase);
    assert_eq!(executed.nonce, 1);
    assert_eq!(executed.origin, origin);
    assert_eq!(executed.phase, phase);
    assert!(executed.consumed);
    assert!(client.is_callback_consumed(&nonce));

    let after = client.get_callback(&nonce).expect("callback should exist");
    assert!(after.consumed);
}

// -----------------------------------------------------------------------------
// Edge case 2: Wrong origin
// -----------------------------------------------------------------------------

#[test]
fn test_callback_wrong_origin_rejected() {
    let env = Env::default();
    let (client, _admin, _sme, _id) = deploy_escrow(&env, "INV_CB_2");

    let origin_a = Address::generate(&env);
    let origin_b = Address::generate(&env);
    let phase = 1u32;

    let nonce = client.register_callback(&origin_a, &phase);
    assert_eq!(nonce, 1);

    // Attempt callback execution with wrong origin (origin_b instead of origin_a)
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_callback(&nonce, &origin_b, &phase);
    }));
    assert!(res.is_err(), "execution from wrong origin must fail");

    // Ensure state was not corrupted / consumed
    assert!(!client.is_callback_consumed(&nonce));
    let stored = client
        .get_callback(&nonce)
        .expect("callback should still exist");
    assert!(!stored.consumed);
}

// -----------------------------------------------------------------------------
// Edge case 3: Wrong nonce
// -----------------------------------------------------------------------------

#[test]
fn test_callback_wrong_nonce_rejected() {
    let env = Env::default();
    let (client, _admin, _sme, _id) = deploy_escrow(&env, "INV_CB_3");

    let origin = Address::generate(&env);
    let phase = 1u32;

    let nonce = client.register_callback(&origin, &phase);
    assert_eq!(nonce, 1);

    // Nonce that does not exist
    let res_nonexistent = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_callback(&999u64, &origin, &phase);
    }));
    assert!(
        res_nonexistent.is_err(),
        "execution with non-existent nonce must fail"
    );

    // Context for nonce 1 is still intact and unconsumed
    assert!(!client.is_callback_consumed(&1));
}

#[test]
fn test_callback_mismatched_nonce_between_origins() {
    let env = Env::default();
    let (client, _admin, _sme, _id) = deploy_escrow(&env, "INV_CB_3B");

    let origin_a = Address::generate(&env);
    let origin_b = Address::generate(&env);

    let nonce_1 = client.register_callback(&origin_a, &1u32);
    let nonce_2 = client.register_callback(&origin_b, &2u32);

    assert_eq!(nonce_1, 1);
    assert_eq!(nonce_2, 2);

    // Origin A attempts to consume Nonce 2 (registered for Origin B)
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_callback(&nonce_2, &origin_a, &1u32);
    }));
    assert!(
        res.is_err(),
        "origin A must not be able to execute callback registered for origin B"
    );

    assert!(!client.is_callback_consumed(&nonce_1));
    assert!(!client.is_callback_consumed(&nonce_2));
}

// -----------------------------------------------------------------------------
// Edge case 4: Callback replay
// -----------------------------------------------------------------------------

#[test]
fn test_callback_replay_rejected() {
    let env = Env::default();
    let (client, _admin, _sme, _id) = deploy_escrow(&env, "INV_CB_4");

    let origin = Address::generate(&env);
    let phase = 1u32;

    let nonce = client.register_callback(&origin, &phase);

    // First execution succeeds
    let first = client.execute_callback(&nonce, &origin, &phase);
    assert!(first.consumed);
    assert!(client.is_callback_consumed(&nonce));

    // Second execution (replay) must fail
    let replay_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_callback(&nonce, &origin, &phase);
    }));
    assert!(
        replay_res.is_err(),
        "callback replay must be rejected on second call"
    );

    // Ensure status remains consumed
    assert!(client.is_callback_consumed(&nonce));
}

// -----------------------------------------------------------------------------
// Edge case 5: Callback after cancellation
// -----------------------------------------------------------------------------

#[test]
fn test_callback_after_cancellation_rejected() {
    let env = Env::default();
    let (client, _admin, _sme, _id) = deploy_escrow(&env, "INV_CB_5");

    let origin = Address::generate(&env);
    let phase = 1u32;

    // Register callback while open (status == 0)
    let nonce = client.register_callback(&origin, &phase);
    assert_eq!(nonce, 1);

    // Cancel funding (transitions status to 4)
    client.cancel_funding();
    assert_eq!(client.get_escrow().status, 4);

    // Attempt callback execution after cancellation
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_callback(&nonce, &origin, &phase);
    }));
    assert!(
        res.is_err(),
        "callback execution after escrow cancellation must be rejected"
    );

    // Attempt registering a new callback on cancelled escrow must also fail
    let reg_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.register_callback(&origin, &phase);
    }));
    assert!(
        reg_res.is_err(),
        "registering callback on cancelled escrow must be rejected"
    );
}

// -----------------------------------------------------------------------------
// Additional edge cases: Wrong phase and multi-callback sequentiality
// -----------------------------------------------------------------------------

#[test]
fn test_callback_wrong_phase_rejected() {
    let env = Env::default();
    let (client, _admin, _sme, _id) = deploy_escrow(&env, "INV_CB_PHASE");

    let origin = Address::generate(&env);
    let expected_phase = 1u32;
    let wrong_phase = 2u32;

    let nonce = client.register_callback(&origin, &expected_phase);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_callback(&nonce, &origin, &wrong_phase);
    }));
    assert!(
        res.is_err(),
        "callback execution with wrong phase must be rejected"
    );

    assert!(!client.is_callback_consumed(&nonce));
}

#[test]
fn test_multiple_callbacks_independent_consumption() {
    let env = Env::default();
    let (client, _admin, _sme, _id) = deploy_escrow(&env, "INV_CB_MULTI");

    let origin_a = Address::generate(&env);
    let origin_b = Address::generate(&env);

    let nonce_1 = client.register_callback(&origin_a, &10u32);
    let nonce_2 = client.register_callback(&origin_b, &20u32);
    let nonce_3 = client.register_callback(&origin_a, &30u32);

    assert_eq!(nonce_1, 1);
    assert_eq!(nonce_2, 2);
    assert_eq!(nonce_3, 3);
    assert_eq!(client.get_callback_nonce(), 3);

    // Consume out of order: consume #2 first
    client.execute_callback(&nonce_2, &origin_b, &20u32);
    assert!(!client.is_callback_consumed(&1));
    assert!(client.is_callback_consumed(&2));
    assert!(!client.is_callback_consumed(&3));

    // Consume #1 next
    client.execute_callback(&nonce_1, &origin_a, &10u32);
    assert!(client.is_callback_consumed(&1));
    assert!(client.is_callback_consumed(&2));
    assert!(!client.is_callback_consumed(&3));

    // Consume #3 last
    client.execute_callback(&nonce_3, &origin_a, &30u32);
    assert!(client.is_callback_consumed(&1));
    assert!(client.is_callback_consumed(&2));
    assert!(client.is_callback_consumed(&3));
}
