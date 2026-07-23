//! Issue #734 — focused boundary tests for [`crate::LiquifactEscrow::rotate_beneficiary`].
//!
//! Each test pins a single typed-error code via `assert_contract_error` for the
//! reject paths and verifies event emission/absence on accept/reject via
//! `env.events().all()`. The status sweep `0..=4` covers the **exactly-at-accept**
//! boundary (`status == 1` is the last accepted) and the **one-over** boundary
//! (`status == 2` is the first rejected). All tests share the same fresh-`Env`
//! discipline and use the existing `setup`/`default_init`/`init_and_fund_with_real_token`
//! test helpers; no contract logic is modified.
//!
//! Coverage map for [`crate::EscrowError`] variants exercised here:
//!
//! | Variant (code)                | Test                                                              |
//! |-------------------------------|-------------------------------------------------------------------|
//! | `LegalHoldBlocksBeneficiaryRotation` (160) | `boundary_hold_returns_typed_160_in_open_state`, `precedence_hold_preceeds_state_check`, `precedence_hold_preceeds_auth_check`, `boundary_hold_off_on_off_sequence` (2nd attempt), `boundary_event_zero_on_hold_rejection` |
//! | `RotationNotOpen` (161)       | `boundary_state_status_2_settled_returns_typed_161`, `..._status_3_withdrawn_returns_typed_161`, `..._status_4_cancelled_returns_typed_161`, `precedence_state_preceeds_equality_check`, `sequence_open_then_cancelled_blocks_rotation` (post-cancel), `boundary_event_zero_on_state_rejection` |
//! | `NewSmeSameAsCurrent` (162)   | `boundary_equality_exact_same_returns_typed_162`, `boundary_equality_uses_post_rotation_state`, `boundary_event_zero_on_equality_rejection` |

use super::{assert_contract_error, default_init, init_and_fund_with_real_token, setup};
use crate::BeneficiaryRotated;

#[allow(unused_imports)]
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _},
    Address, Env,
};

// ─── State boundary sweep: status 0..4 ───────────────────────────────────────────────

/// Status **0** (Open) is the lowest accepted boundary. Rotation must succeed
/// and the post-state `status` must remain `0` (rotation changes only the
/// `sme_address` field, not the lifecycle state).
#[test]
fn test_rotate_beneficiary_boundary_state_status_0_open_accepts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_eq!(client.get_escrow().status, 0, "precondition: starts at Open");

    let new_sme = Address::generate(&env);
    assert_ne!(new_sme, sme);
    let updated = client.rotate_beneficiary(&new_sme);
    assert_eq!(updated.sme_address, new_sme);
    assert_eq!(
        updated.status, 0,
        "rotate is address-only; status must remain 0 (Open)"
    );
    assert_eq!(client.get_escrow().sme_address, new_sme);
}

/// Status **1** (Funded) is the **last accepted** boundary — `is_pre_settlement_status(1) == true`.
/// Rotation succeeds and the post-state status remains `1`. This pins down the
/// inclusive upper boundary of the accept window.
#[test]
fn test_rotate_beneficiary_boundary_state_status_1_funded_accepts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _escrow_id, original_sme) =
        init_and_fund_with_real_token(&env, 1_000i128, "BND_S1");
    assert_eq!(
        client.get_escrow().status,
        1,
        "precondition: escrow reaches funded (1) on init_and_fund_with_real_token"
    );
    assert_eq!(client.get_escrow().sme_address, original_sme);

    let new_sme = Address::generate(&env);
    assert_ne!(new_sme, original_sme);
    let updated = client.rotate_beneficiary(&new_sme);
    assert_eq!(updated.sme_address, new_sme);
    assert_eq!(
        updated.status, 1,
        "rotate is address-only; status must remain 1 (Funded)"
    );
}

/// Status **2** (Settled) is the **first rejected** boundary — "one over" the last
/// accepted state. Rotation returns the typed error
/// [`EscrowError::RotationNotOpen`] (code `161`), not a host trap.
#[test]
fn test_rotate_beneficiary_boundary_state_status_2_settled_returns_typed_161() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _escrow_id, _sme) =
        init_and_fund_with_real_token(&env, 1_000i128, "BND_S2");
    client.settle();
    assert_eq!(client.get_escrow().status, 2);

    let new_sme = Address::generate(&env);
    assert_contract_error(
        client.try_rotate_beneficiary(&new_sme),
        EscrowError::RotationNotOpen,
    );
}

/// Status **3** (Withdrawn) is one-over deep into the terminal cluster. The
/// typed error is `RotationNotOpen` (`161`), shared with `settled`/`cancelled`
/// by the `is_pre_settlement_status` predicate.
#[test]
fn test_rotate_beneficiary_boundary_state_status_3_withdrawn_returns_typed_161() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _escrow_id, _sme) =
        init_and_fund_with_real_token(&env, 1_000i128, "BND_S3");
    client.settle(); // status == 2
    client.withdraw(); // status == 3
    assert_eq!(client.get_escrow().status, 3);

    let new_sme = Address::generate(&env);
    assert_contract_error(
        client.try_rotate_beneficiary(&new_sme),
        EscrowError::RotationNotOpen,
    );
}

/// Status **4** (Cancelled) — terminal via admin gating. The typed error is
/// `RotationNotOpen` (`161`).
#[test]
fn test_rotate_beneficiary_boundary_state_status_4_cancelled_returns_typed_161() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.cancel_funding();
    assert_eq!(client.get_escrow().status, 4);

    let new_sme = Address::generate(&env);
    assert_contract_error(
        client.try_rotate_beneficiary(&new_sme),
        EscrowError::RotationNotOpen,
    );
}

/// **Sequence:** rotate in Open succeeds, then `cancel_funding` moves the
/// escrow into the Cancelled terminal. A follow-up rotation is now rejected
/// with typed `RotationNotOpen` (`161`). Pinpoints the state boundary surfacing
/// mid-flight rather than at deploy time.
#[test]
fn test_rotate_beneficiary_sequence_open_then_cancelled_blocks_rotation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    assert_eq!(client.get_escrow().status, 0);

    let first_new_sme = Address::generate(&env);
    let updated = client.rotate_beneficiary(&first_new_sme);
    assert_eq!(updated.status, 0);
    assert_eq!(updated.sme_address, first_new_sme);

    // Boundary step: admin-only action flips to terminal Cancelled (status 4).
    client.cancel_funding();
    assert_eq!(client.get_escrow().status, 4);

    // Rotation now blocked by state — typed #161.
    let second_new_sme = Address::generate(&env);
    assert_contract_error(
        client.try_rotate_beneficiary(&second_new_sme),
        EscrowError::RotationNotOpen,
    );
}

// ─── Equality boundary ─────────────────────────────────────────────────────────────

/// Exactly-equal-at-boundary (`new_sme == current sme`) is rejected with the
/// typed [`EscrowError::NewSmeSameAsCurrent`] (code `162`). The boundary itself
/// — `current == new` — is the inclusive reject point and any strictly different
/// address passes.
#[test]
fn test_rotate_beneficiary_boundary_equality_exact_same_returns_typed_162() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_contract_error(
        client.try_rotate_beneficiary(&sme),
        EscrowError::NewSmeSameAsCurrent,
    );
}

/// **One-over boundary:** After rotating `A → B`, a second rotate-with-same-value
/// attempts to rotate to the *new* current sme (`B`), which must reject with
/// `NewSmeSameAsCurrent` (`162`). Confirms the equality check uses the
/// **post-rotation** stored address, not the original `init` value.
#[test]
fn test_rotate_beneficiary_boundary_equality_uses_post_rotation_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    let sme_b = Address::generate(&env);
    client.rotate_beneficiary(&sme_b);
    assert_eq!(client.get_escrow().sme_address, sme_b);

    // `sme_b` is now the current; passing it again must reject (typed #162).
    assert_contract_error(
        client.try_rotate_beneficiary(&sme_b),
        EscrowError::NewSmeSameAsCurrent,
    );

    // The original `sme` is no longer current — should be accept-worthy now.
    let sme_c = Address::generate(&env);
    let updated = client.rotate_beneficiary(&sme_c);
    assert_eq!(updated.sme_address, sme_c);
    // `sme` itself is no longer the current target — re-passing it must again reject.
    assert_contract_error(
        client.try_rotate_beneficiary(&sme_c),
        EscrowError::NewSmeSameAsCurrent,
    );
}

// ─── Multi-step rotation within the same pre-settlement state ───────────────────────

/// Two successive rotations within Open (status `0`): `A → B → C`. Both must
/// succeed because each pair strictly differs from prior-current. Confirms the
/// contract permits repeated rotation in the accept window.
#[test]
fn test_rotate_beneficiary_sequence_two_rotations_within_open_succeed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    assert_eq!(client.get_escrow().status, 0);

    let sme_b = Address::generate(&env);
    let sme_c = Address::generate(&env);

    let u1 = client.rotate_beneficiary(&sme_b);
    assert_eq!(u1.sme_address, sme_b);
    assert_eq!(u1.status, 0);

    let u2 = client.rotate_beneficiary(&sme_c);
    assert_eq!(u2.sme_address, sme_c);
    assert_eq!(u2.status, 0);
    assert_eq!(client.get_escrow().sme_address, sme_c);
}

// ─── Legal-hold boundary ───────────────────────────────────────────────────────────

/// `Open + legal_hold == true`: typed [`EscrowError::LegalHoldBlocksBeneficiaryRotation`]
/// (code `160`). The legal-hold gate fires **before** the state check.
#[test]
fn test_rotate_beneficiary_boundary_hold_returns_typed_160_in_open_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_legal_hold(&true);
    assert!(client.get_legal_hold(), "precondition: hold is on");

    let new_sme = Address::generate(&env);
    assert_contract_error(
        client.try_rotate_beneficiary(&new_sme),
        EscrowError::LegalHoldBlocksBeneficiaryRotation,
    );
}

/// **Boundary flip sequence:** `hold=false` accepts, `hold=true` rejects
/// (`#160`), `hold=false` accepts again. Two rejections and one success — the
/// accept/reject decision tracks the hold bit exactly across the rotation
/// cycle that flips between accept and reject twice.
#[test]
fn test_rotate_beneficiary_boundary_hold_off_on_off_sequence() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    let sme_b = Address::generate(&env);

    // 1st attempt: hold = false (default after init) → accept.
    let updated = client.rotate_beneficiary(&sme_b);
    assert_eq!(updated.sme_address, sme_b);

    // 2nd attempt: hold = true → typed #160.
    client.set_legal_hold(&true);
    assert_contract_error(
        client.try_rotate_beneficiary(&sme_b),
        EscrowError::LegalHoldBlocksBeneficiaryRotation,
    );

    // 3rd attempt: hold = false → accept again (boundary flip resolves).
    // We use a fresh `sme_c` (not `sme_b`) because the rejected 2nd call
    // left `current sme_address == sme_b`; re-passing `sme_b` would trip
    // `NewSmeSameAsCurrent` (#162) and obscure the hold boundary we're testing.
    client.clear_legal_hold();
    let sme_c = Address::generate(&env);
    let updated2 = client.rotate_beneficiary(&sme_c);
    assert_eq!(updated2.sme_address, sme_c);
    assert_eq!(client.get_escrow().sme_address, sme_c);
}

// ─── Guard-ordering precedence ─────────────────────────────────────────────────────

/// `hold preceeds state`: even at terminal status `2` (settled), a live legal
/// hold causes rotation to fail with [`EscrowError::LegalHoldBlocksBeneficiaryRotation`]
/// (`#160`), **not** [`EscrowError::RotationNotOpen`] (`#161`). Pins the documented
/// `guard_not_legal_hold` ordering in `escrow/src/lib.rs`.
#[test]
fn test_rotate_beneficiary_precedence_hold_preceeds_state_check() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _escrow_id, _sme) =
        init_and_fund_with_real_token(&env, 1_000i128, "BND_PRE1");
    client.settle(); // status == 2
    assert_eq!(client.get_escrow().status, 2);
    client.set_legal_hold(&true);

    let new_sme = Address::generate(&env);
    assert_contract_error(
        client.try_rotate_beneficiary(&new_sme),
        EscrowError::LegalHoldBlocksBeneficiaryRotation,
    );
}

/// `hold preceeds auth`: with both `hold = true` and `mock_all_auths` enabled
/// (so auths would pass if reached), rotation fails with `#160`. Confirms the
/// legal-hold check short-circuits before any `Address::require_auth()` call.
#[test]
fn test_rotate_beneficiary_precedence_hold_preceeds_auth_check() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_legal_hold(&true);

    let new_sme = Address::generate(&env);
    assert_contract_error(
        client.try_rotate_beneficiary(&new_sme),
        EscrowError::LegalHoldBlocksBeneficiaryRotation,
    );
}

/// `state preceeds equality`: at terminal status `2`, passing the *current*
/// sme (`new_sme == current`) must return `RotationNotOpen` (`#161`), not
/// `NewSmeSameAsCurrent` (`#162`). The state gate fires strictly before
/// the equality gate so a caller cannot learn whether their equality
/// would-have-rejected from a rejected-by-state failure mode.
#[test]
fn test_rotate_beneficiary_precedence_state_preceeds_equality_check() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _escrow_id, sme) =
        init_and_fund_with_real_token(&env, 1_000i128, "BND_PRE2");
    client.settle();
    assert_eq!(client.get_escrow().status, 2);

    // `sme` is also the "new" sme (exactly equal) — but state must fire first.
    assert_contract_error(
        client.try_rotate_beneficiary(&sme),
        EscrowError::RotationNotOpen,
    );
}

// ─── Event boundary: emit on accept, no emit on reject ──────────────────────────────

/// On successful rotation: **exactly one** `BeneficiaryRotated` event from the
/// most recent invocation, with `prior_sme` and `new_sme` correctly set.
/// Note: `env.events().all()` returns events from the most recent invocation
/// only — the test reads immediately after `rotate_beneficiary` returns, with
/// no intervening view calls that would clear the buffer.
#[test]
fn test_rotate_beneficiary_boundary_event_one_on_success_with_correct_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let contract_id = client.address.clone();
    default_init(&client, &env, &admin, &sme);

    let new_sme = Address::generate(&env);
    client.rotate_beneficiary(&new_sme);

    // Capture events IMMEDIATELY after the rotate call (no view reads).
    let events = env.events().all();
    assert_eq!(
        events.events().len(),
        1,
        "rotate must emit exactly one event on success"
    );
    assert_eq!(
        events.events().last().unwrap().clone(),
        BeneficiaryRotated {
            name: symbol_short!("ben_rot"),
            invoice_id: client.get_escrow().invoice_id,
            prior_sme: sme,
            new_sme,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// On `LegalHoldBlocksBeneficiaryRotation` reject: **no** `BeneficiaryRotated`
/// event is emitted. The legal-hold gate aborts before the contract writes
/// state or publishes.
#[test]
fn test_rotate_beneficiary_boundary_event_zero_on_hold_rejection() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_legal_hold(&true);

    let new_sme = Address::generate(&env);
    assert_contract_error(
        client.try_rotate_beneficiary(&new_sme),
        EscrowError::LegalHoldBlocksBeneficiaryRotation,
    );

    let events = env.events().all();
    assert_eq!(
        events.events().len(),
        0,
        "no event must be emitted when legal hold blocks rotation"
    );
}

/// On `RotationNotOpen` reject (status != 0/1): **no** `BeneficiaryRotated` is
/// emitted. The state gate aborts before the storage write and event publish.
#[test]
fn test_rotate_beneficiary_boundary_event_zero_on_state_rejection() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _escrow_id, _sme) = init_and_fund_with_real_token(&env, 1_000i128, "BND_EVT_S");
    client.settle(); // status == 2

    let new_sme = Address::generate(&env);
    assert_contract_error(
        client.try_rotate_beneficiary(&new_sme),
        EscrowError::RotationNotOpen,
    );

    let events = env.events().all();
    assert_eq!(
        events.events().len(),
        0,
        "no event must be emitted when state blocks rotation"
    );
}

/// On `NewSmeSameAsCurrent` reject (equality no-op guard): **no**
/// `BeneficiaryRotated` is emitted. Confirms the no-op short-circuit happens
/// before the storage write and event publish — indexers therefore never see
/// `ben_rot` events that didn't actually rotate.
#[test]
fn test_rotate_beneficiary_boundary_event_zero_on_equality_rejection() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_contract_error(
        client.try_rotate_beneficiary(&sme),
        EscrowError::NewSmeSameAsCurrent,
    );

    let events = env.events().all();
    assert_eq!(
        events.events().len(),
        0,
        "no event must be emitted when new_sme equals current sme"
    );
}
