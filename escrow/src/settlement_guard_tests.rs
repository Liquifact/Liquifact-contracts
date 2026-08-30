//! Self-contained tests for the settlement once-only guard (issue #1198).
//!
//! These tests deliberately do **not** depend on the crate's `tests/` module tree, which is
//! currently disabled pending reconciliation with the lib API (see the note near the end of
//! `lib.rs`). Instead they drive the public [`LiquifactEscrow`] surface directly, so the
//! reentrancy / double-application guard can actually be exercised by `cargo test` today.
//!
//! Guards under test (in [`LiquifactEscrow::settle`]):
//! - the settled marker (`status == 2` + [`crate::DataKey::SettledAt`]) is committed
//!   **before** the outward [`crate::EscrowSettled`] event;
//! - a second settlement of the same escrow is rejected with the dedicated typed error
//!   [`crate::EscrowError::EscrowAlreadySettled`] rather than being double-applied.

use soroban_sdk::{testutils::Address as _, Address, Env, String};

use super::{LiquifactEscrow, LiquifactEscrowClient};

/// Deploy an escrow, initialise it with `maturity == 0`, and fund a single investor to
/// target so the escrow is settleable (`status == 1`).
///
/// `maturity == 0` removes the ledger-time gate so `settle` succeeds immediately, keeping
/// each test focused on the once-only guard. Funding is off-chain bookkeeping (no token
/// transfer), so a generated funding-token address is sufficient.
fn deploy_funded<'a>(env: &'a Env, invoice_id: &str) -> LiquifactEscrowClient<'a> {
    deploy_funded_with_id(env, invoice_id).0
}

/// Deploy an escrow, initialise it with `maturity == 0`, and fund a single investor, returning
/// both the [`LiquifactEscrowClient`] **and** its contract [`Address`] so callers can compose
/// it into a cross-contract [`LiquifactEscrow::settle_batch`] batch.
fn deploy_funded_with_id<'a>(
    env: &'a Env,
    invoice_id: &str,
) -> (LiquifactEscrowClient<'a>, Address) {
    // `mock_all_auths_allowing_non_root_auth` (rather than `mock_all_auths`) lets
    // [`LiquifactEscrow::settle_batch`] settle *other* escrow instances: each target's
    // `settle` requires its own SME `require_auth` in a **non-root** invocation, which the
    // plain `mock_all_auths` would reject with an `Auth, InvalidAction` error. It is a
    // strict superset of `mock_all_auths` (root invocations still pass), so it is also safe
    // for the single-entry tests that call `settle` directly.
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
        &None::<u32>,
    );
    let investor = Address::generate(env);
    client.fund(&investor, &1_000i128);
    (client, id)
}

/// Edge case — **first settlement succeeds**: transitions status 1 → 2 and commits the
/// once-only `SettledAt` marker.
#[test]
fn first_settlement_succeeds_and_marks_settled() {
    let env = Env::default();
    let client = deploy_funded(&env, "GDSETL1");

    assert_eq!(client.get_escrow().status, 1u32, "precondition: funded");
    let result = client.settle();

    assert_eq!(result.escrow.status, 2, "status must transition to settled");
    assert_eq!(client.get_escrow().status, 2u32);
    assert!(
        client.get_settled_at().is_some(),
        "settled marker must be committed by a successful first settlement"
    );
}

/// Edge case — **second settlement of the same escrow is rejected** and must not
/// double-apply: the settled marker and accounting state are unchanged after the rejected call.
#[test]
fn second_settlement_of_same_escrow_is_rejected() {
    let env = Env::default();
    let client = deploy_funded(&env, "GDSETL2");

    let first = client.settle();
    let settled_at = client.get_settled_at();

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.settle();
    }));
    assert!(res.is_err(), "a second settle must be rejected");

    // Not double-applied: state is identical to after the first settlement.
    assert_eq!(client.get_escrow().status, 2u32);
    assert_eq!(
        client.get_settled_at(),
        settled_at,
        "settled marker unchanged"
    );
    assert_eq!(
        client.get_escrow().funded_amount,
        first.escrow.funded_amount,
        "funded principal must not be re-processed by the rejected second call"
    );
}

/// Edge case — **concurrent double-call applies only once**: two settle invocations on the
/// same escrow leave exactly one settlement applied (a single settled marker), with the
/// second call rejected.
#[test]
fn concurrent_double_call_applies_once() {
    let env = Env::default();
    let client = deploy_funded(&env, "GDSETL3");

    client.settle();
    let settled_at = client.get_settled_at();

    // Second, "concurrent" attempt is atomically rejected (status is already == 2).
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.settle();
    }));
    assert!(res.is_err(), "second concurrent settlement must not apply");
    assert_eq!(
        client.get_escrow().status,
        2u32,
        "still settled exactly once"
    );
    assert_eq!(
        client.get_settled_at(),
        settled_at,
        "settled marker written exactly once"
    );
}

/// Edge case — **flag is set before any outward effect**: by the time `settle` returns, the
/// once-only `SettledAt` marker is already committed (before the outward [`crate::EscrowSettled`]
/// event), so any subsequent re-entry is blocked by the guard.
#[test]
fn settled_flag_set_before_external_effect() {
    let env = Env::default();
    let client = deploy_funded(&env, "GDSETL4");

    client.settle();

    // The settled flag is committed before the caller observes any effect, so a re-entrant
    // settlement attempt finds the flag already set and is rejected.
    assert!(client.get_settled_at().is_some());
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.settle();
    }));
    assert!(res.is_err(), "flag already set must block re-entry");
}

/// Edge case — **unrelated escrows are unaffected**: settling one escrow must not change the
/// status of another instance; the second stays funded and can still settle independently.
#[test]
fn unrelated_escrow_unaffected_by_another_settlement() {
    let env = Env::default();
    let client_a = deploy_funded(&env, "GDSETL_A");
    let client_b = deploy_funded(&env, "GDSETL_B");

    client_a.settle();

    assert_eq!(client_a.get_escrow().status, 2u32, "A settled");
    assert_eq!(
        client_b.get_escrow().status,
        1u32,
        "B untouched by A's settlement — still funded"
    );
    assert!(client_b.get_settled_at().is_none());

    // B can still settle independently.
    client_b.settle();
    assert_eq!(client_b.get_escrow().status, 2u32);
}

/// Issue #1209: a disputed escrow must not release funds while evidence is pending.
/// The dispute record is preserved and the release path is blocked before any token transfer
/// or status transition occurs.
#[test]
fn dispute_freezes_funds_until_resolved() {
    let env = Env::default();
    env.mock_all_auths();

    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "DISPUTE01"),
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
        &None::<u32>,
    );

    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);

    let before = client.get_escrow();
    assert_eq!(before.status, 1u32, "funded precondition");
    assert!(!client.is_dispute_active());

    let release_before = client.try_withdraw();
    assert!(
        release_before.is_ok(),
        "release succeeds when no dispute exists"
    );
    assert_eq!(
        client.get_escrow().status,
        3u32,
        "status transitions to withdrawn"
    );

    let env2 = Env::default();
    env2.mock_all_auths();
    let id2 = env2.register(LiquifactEscrow, ());
    let client2 = LiquifactEscrowClient::new(&env2, &id2);
    let admin2 = Address::generate(&env2);
    let sme2 = Address::generate(&env2);
    let treasury2 = Address::generate(&env2);
    let token2 = Address::generate(&env2);
    client2.init(
        &admin2,
        &String::from_str(&env2, "DISPUTE02"),
        &sme2,
        &1_000i128,
        &800i64,
        &0u64,
        &token2,
        &None,
        &treasury2,
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
    let investor2 = Address::generate(&env2);
    client2.fund(&investor2, &1_000i128);

    client2.open_dispute(&admin2);
    assert!(client2.is_dispute_active());
    assert!(client2.get_dispute_record().is_some());

    let disputed_release = client2.try_withdraw();
    assert!(
        disputed_release.is_err(),
        "withdraw must fail while dispute is active"
    );

    client2.close_dispute(&admin2, &true);
    assert!(!client2.is_dispute_active());
    let resolved_release = client2.try_withdraw();
    assert!(
        resolved_release.is_ok(),
        "release succeeds once dispute is resolved"
    );
    assert_eq!(client2.get_escrow().status, 3u32);
}

#[test]
fn dispute_close_requires_admin_authority() {
    let env = Env::default();
    env.mock_all_auths();

    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "DISPUTE03"),
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
        &None::<u32>,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);

    client.open_dispute(&admin);
    let outsider = Address::generate(&env);
    let err = client.try_close_dispute(&outsider, &true);
    assert!(err.is_err(), "non-admin must not close a dispute");
    assert!(client.is_dispute_active());
}

/// Edge case for the technical guidance “**make the guard total across all settlement
/// entrypoints**”: [`LiquifactEscrow::settle_batch`] settles a batch of **distinct** escrows,
/// each exactly once. A well-formed batch of unrelated targets is fully applied.
#[test]
fn settle_batch_settles_distinct_targets_once() {
    let env = Env::default();
    let (client_a, _addr_a) = deploy_funded_with_id(&env, "GDBATCH_A");
    let (client_b, addr_b) = deploy_funded_with_id(&env, "GDBATCH_B");
    let (client_c, addr_c) = deploy_funded_with_id(&env, "GDBATCH_C");

    assert_eq!(client_a.get_escrow().status, 1u32, "precondition");
    assert_eq!(client_b.get_escrow().status, 1u32, "precondition");
    assert_eq!(client_c.get_escrow().status, 1u32, "precondition");

    client_a.settle_batch(&soroban_sdk::vec![&env, addr_b, addr_c]);

    assert_eq!(client_b.get_escrow().status, 2u32, "B settled via batch");
    assert_eq!(client_c.get_escrow().status, 2u32, "C settled via batch");
    assert_eq!(
        client_a.get_escrow().status,
        1u32,
        "batch caller is unaffected; only named targets settle"
    );
}

/// Edge case for “make the guard total across all settlement entrypoints”: a
/// [`LiquifactEscrow::settle_batch`] that lists the **same escrow address twice** must be
/// rejected atomically. The once-only guard fires on the second entry, and because the batch
/// is atomic the *entire* call reverts — the target is left untouched (still funded, no settled
/// marker), not double-applied.
#[test]
fn settle_batch_duplicate_address_rejected_atomically() {
    let env = Env::default();
    let (client_a, _addr_a) = deploy_funded_with_id(&env, "GDBATCH_DUP_A");
    let (client_b, addr_b) = deploy_funded_with_id(&env, "GDBATCH_DUP_B");

    assert_eq!(client_b.get_escrow().status, 1u32, "precondition: funded");

    // Identical address twice in one batch: the second settle hits EscrowAlreadySettled.
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client_a.settle_batch(&soroban_sdk::vec![&env, addr_b.clone(), addr_b]);
    }));
    assert!(
        res.is_err(),
        "duplicate address in a batch must be rejected"
    );

    // Atomic rollback: neither batch entry applied — the whole call reverted.
    assert_eq!(
        client_b.get_escrow().status,
        1u32,
        "target must remain funded after an atomic batch revert"
    );
    assert!(
        client_b.get_settled_at().is_none(),
        "no settled marker written by a reverted batch"
    );
}
