//! Tests for the dedicated emergency-**pauser** role (issue #1548).
//!
//! The pauser is an optional address that may activate a scoped pause immediately, without
//! waiting on a governed multisig admin. It is deliberately one-way: it can pause but never
//! unpause, and it cannot change any pause configuration.
//!
//! Reuses the shared helpers from [`crate::tests`]: [`super::setup`],
//! [`super::install_stellar_asset_token`], [`super::assert_contract_error`] and
//! [`super::TARGET`].

use super::{assert_contract_error, install_stellar_asset_token, setup, TARGET};
use crate::{EscrowError, LiquifactEscrowClient, PauseReason, PauseScope, PausedChanged};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _, MockAuth, MockAuthInvoke},
    Address, Env, Event,
};

/// One hour — the max duration used by the pauser expiry test.
const ONE_HOUR: u64 = 3_600;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Move the ledger clock to `ts`.
fn advance(env: &Env, ts: u64) {
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = ts;
    env.ledger().set(ledger_info);
}

/// Deploy an escrow backed by a real Stellar asset token, left in the **open** (unfunded)
/// state so that `fund` can be exercised while a pause is active.
///
/// Returns `(client, admin, sme, treasury, investor)`.
fn init_open_with_token<'a>(
    env: &'a Env,
    id: &str,
) -> (
    LiquifactEscrowClient<'a>,
    Address,
    Address,
    Address,
    Address,
) {
    let (client, admin, sme) = setup(env);
    let sac = install_stellar_asset_token(env);
    let treasury = Address::generate(env);
    let investor = Address::generate(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, id),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &sac.id,
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
    sac.stellar.mint(&investor, &TARGET);

    (client, admin, sme, treasury, investor)
}

/// Authorize **only** `address` for the given contract function.
fn authorize_only(env: &Env, client: &LiquifactEscrowClient, address: &Address, fn_name: &str) {
    env.mock_auths(&[MockAuth {
        address,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name,
            args: soroban_sdk::Vec::<soroban_sdk::Val>::new(env),
            sub_invokes: &[],
        },
    }]);
}

// ── 1. The core pauser flow ───────────────────────────────────────────────────

/// The pauser activates the `Funding` scope, funding is blocked with
/// `PausedBlocksFunding`, and the admin un-pauses.
#[test]
fn pauser_pauses_funding_scope_and_admin_unpauses() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, investor) = init_open_with_token(&env, "PAU001");
    let pauser = Address::generate(&env);

    client.set_pauser(&Some(pauser.clone()));
    assert_eq!(client.get_pauser(), Some(pauser.clone()));

    // The pauser activates the Funding scope.
    client.pause_as_pauser(&pauser, &PauseScope::Funding, &PauseReason::Incident);
    assert!(client.is_paused());
    let state = client.get_pause_state().unwrap();
    assert_eq!(state.scope, PauseScope::Funding);
    assert_eq!(state.reason, PauseReason::Incident);

    // Funding is blocked while the pause is active.
    assert_contract_error(
        client.try_fund(&investor, &TARGET),
        EscrowError::PausedBlocksFunding,
    );

    // Only the admin can lift it.
    client.set_paused(&false, &PauseScope::Funding, &PauseReason::Incident);
    assert!(!client.is_paused());

    // Funding works again.
    client.fund(&investor, &TARGET);
}

/// The pauser may widen the scope of an active pause, but never narrow it.
#[test]
fn pauser_may_widen_but_not_narrow_the_scope() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU002");
    let pauser = Address::generate(&env);
    client.set_pauser(&Some(pauser.clone()));

    client.pause_as_pauser(&pauser, &PauseScope::Funding, &PauseReason::Incident);
    assert_eq!(client.get_pause_state().unwrap().scope, PauseScope::Funding);

    // Widening Funding -> All is allowed.
    client.pause_as_pauser(&pauser, &PauseScope::All, &PauseReason::Incident);
    let state = client.get_pause_state().unwrap();
    assert_eq!(state.scope, PauseScope::All);
    // The original activation timestamp survives the widening.
    assert_eq!(state.activated_at, 0);

    // Narrowing All -> Withdrawal is rejected.
    assert_contract_error(
        client.try_pause_as_pauser(&pauser, &PauseScope::Withdrawal, &PauseReason::Incident),
        EscrowError::PauseAlreadyActive,
    );
    assert_eq!(client.get_pause_state().unwrap().scope, PauseScope::All);
}

// ── 2. The pauser cannot un-pause or reconfigure ──────────────────────────────

/// The pauser cannot un-pause: `set_paused(false, ..)` requires the admin's auth.
#[test]
#[should_panic]
fn pauser_cannot_unpause() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU003");
    let pauser = Address::generate(&env);
    client.set_pauser(&Some(pauser.clone()));
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);

    // Authorize ONLY the pauser — the admin-only un-pause must fail.
    authorize_only(&env, &client, &pauser, "set_paused");
    client.set_paused(&false, &PauseScope::All, &PauseReason::Incident);
}

/// The pauser cannot change the pause configuration (max duration, cooldown, rate limit).
#[test]
#[should_panic]
fn pauser_cannot_change_pause_configuration() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU004");
    let pauser = Address::generate(&env);
    client.set_pauser(&Some(pauser.clone()));

    // Authorize ONLY the pauser — the admin-only configuration call must fail.
    authorize_only(&env, &client, &pauser, "set_pause_max_duration");
    client.set_pause_max_duration(&ONE_HOUR);
}

/// The pauser cannot appoint or revoke a pauser.
#[test]
#[should_panic]
fn pauser_cannot_set_pauser() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU005");
    let pauser = Address::generate(&env);
    client.set_pauser(&Some(pauser.clone()));

    authorize_only(&env, &client, &pauser, "set_pauser");
    client.set_pauser(&None);
}

// ── 3. Only the stored pauser may pause ───────────────────────────────────────

/// A non-pauser calling `pause_as_pauser` fails with `NotPauser`.
#[test]
fn non_pauser_cannot_pause_as_pauser() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU006");
    let pauser = Address::generate(&env);
    let outsider = Address::generate(&env);
    client.set_pauser(&Some(pauser.clone()));

    assert_contract_error(
        client.try_pause_as_pauser(&outsider, &PauseScope::All, &PauseReason::Incident),
        EscrowError::NotPauser,
    );

    // The stored pauser still works.
    client.pause_as_pauser(&pauser, &PauseScope::All, &PauseReason::Incident);
    assert!(client.is_paused());
}

/// With no pauser configured at all, `pause_as_pauser` fails with `NotPauser`.
#[test]
fn no_pauser_configured_fails_with_not_pauser() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU007");
    let outsider = Address::generate(&env);

    assert_eq!(client.get_pauser(), None);
    assert_contract_error(
        client.try_pause_as_pauser(&outsider, &PauseScope::All, &PauseReason::Incident),
        EscrowError::NotPauser,
    );
}

/// Clearing the pauser revokes it immediately.
#[test]
fn clearing_pauser_revokes_it() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU008");
    let pauser = Address::generate(&env);

    client.set_pauser(&Some(pauser.clone()));
    assert_eq!(client.get_pauser(), Some(pauser.clone()));

    client.set_pauser(&None);
    assert_eq!(client.get_pauser(), None);

    assert_contract_error(
        client.try_pause_as_pauser(&pauser, &PauseScope::All, &PauseReason::Incident),
        EscrowError::NotPauser,
    );
}

// ── 4. Role conflicts ─────────────────────────────────────────────────────────

/// The pauser may not be the SME or the treasury.
#[test]
fn pauser_may_not_be_sme_or_treasury() {
    let env = Env::default();
    let (client, _admin, sme, treasury, _investor) = init_open_with_token(&env, "PAU009");

    assert_contract_error(
        client.try_set_pauser(&Some(sme.clone())),
        EscrowError::PauserRoleConflict,
    );
    assert_contract_error(
        client.try_set_pauser(&Some(treasury.clone())),
        EscrowError::PauserRoleConflict,
    );

    // Neither conflict changed the stored value.
    assert_eq!(client.get_pauser(), None);

    // A dedicated address is accepted.
    let pauser = Address::generate(&env);
    client.set_pauser(&Some(pauser.clone()));
    assert_eq!(client.get_pauser(), Some(pauser));
}

/// Only the admin may appoint a pauser.
#[test]
#[should_panic]
fn set_pauser_by_non_admin_panics() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU010");
    env.mock_auths(&[]);
    client.set_pauser(&Some(Address::generate(&env)));
}

// ── 5. The rate limit is shared across both entrypoints ──────────────────────

/// Admin and pauser toggles draw from the **same** rolling window: the combined count
/// is what the limit applies to.
#[test]
fn rate_limit_is_shared_across_admin_and_pauser() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU011");
    let pauser = Address::generate(&env);
    client.set_pauser(&Some(pauser.clone()));

    // One toggle per window.
    client.set_pause_rate_limit(&1u32, &60u64);

    // The admin consumes the single slot.
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);

    // The pauser's toggle is rejected by the same window.
    assert_contract_error(
        client.try_pause_as_pauser(&pauser, &PauseScope::All, &PauseReason::Incident),
        EscrowError::PauseToggleRateLimitExceeded,
    );
}

/// The same holds in the other direction: the pauser's toggle consumes the slot.
#[test]
fn rate_limit_is_shared_across_pauser_and_admin() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU012");
    let pauser = Address::generate(&env);
    client.set_pauser(&Some(pauser.clone()));

    client.set_pause_rate_limit(&1u32, &60u64);

    // The pauser consumes the single slot.
    client.pause_as_pauser(&pauser, &PauseScope::All, &PauseReason::Incident);

    // The admin's toggle is rejected by the same window.
    assert_contract_error(
        client.try_set_paused(&false, &PauseScope::All, &PauseReason::Incident),
        EscrowError::PauseToggleRateLimitExceeded,
    );
    // The pause is still in place.
    assert!(client.is_paused());
}

// ── 6. Event payloads are identical ───────────────────────────────────────────

/// An admin-initiated pause and a pauser-initiated pause produce byte-identical
/// `PausedChanged` events.
#[test]
fn event_payloads_are_identical_between_admin_and_pauser() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU013");
    let pauser = Address::generate(&env);
    client.set_pauser(&Some(pauser.clone()));

    // Admin-initiated pause.
    client.set_paused(&true, &PauseScope::Funding, &PauseReason::Incident);
    let admin_event = env.events().all().events().last().unwrap().clone();

    // Reset, then pauser-initiated pause with the same scope/reason.
    client.set_paused(&false, &PauseScope::Funding, &PauseReason::Incident);
    client.pause_as_pauser(&pauser, &PauseScope::Funding, &PauseReason::Incident);
    let pauser_event = env.events().all().events().last().unwrap().clone();

    assert_eq!(admin_event, pauser_event);

    // ... and both carry the expected payload.
    let expected = PausedChanged {
        name: symbol_short!("paused"),
        invoice_id: client.get_escrow().invoice_id,
        active: 1,
        scope: PauseScope::Funding,
        reason: PauseReason::Incident,
    }
    .to_xdr(&env, &client.address);
    assert_eq!(admin_event, expected);
    assert_eq!(pauser_event, expected);
}

/// `set_pauser` emits `PauserChanged` carrying the new pauser.
#[test]
fn set_pauser_emits_pauser_changed_event() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU014");
    let pauser = Address::generate(&env);

    client.set_pauser(&Some(pauser.clone()));
    let last = env.events().all().events().last().unwrap().clone();
    assert_eq!(
        last,
        crate::PauserChanged {
            name: symbol_short!("pauser"),
            invoice_id: client.get_escrow().invoice_id,
            pauser: Some(pauser.clone()),
        }
        .to_xdr(&env, &client.address)
    );

    // Revoking emits the same event with `None`.
    client.set_pauser(&None);
    let last = env.events().all().events().last().unwrap().clone();
    assert_eq!(
        last,
        crate::PauserChanged {
            name: symbol_short!("pauser"),
            invoice_id: client.get_escrow().invoice_id,
            pauser: None,
        }
        .to_xdr(&env, &client.address)
    );
}

// ── 7. The pauser cannot extend a pause ──────────────────────────────────────

/// The pauser goes through the same activation path as the admin, so a pauser re-pause
/// cannot push the max-duration clock out.
#[test]
fn pauser_cannot_extend_the_max_duration() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU015");
    let pauser = Address::generate(&env);
    client.set_pauser(&Some(pauser.clone()));
    client.set_pause_max_duration(&ONE_HOUR);

    // The admin activates the pause at t = 1000 -> expires at t = 4600.
    advance(&env, 1000);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);

    // The pauser re-pauses at t = 4599, one second before expiry.
    advance(&env, 4599);
    client.pause_as_pauser(&pauser, &PauseScope::All, &PauseReason::Incident);
    assert!(client.is_paused());

    // The activation timestamp is unchanged, so the clock still expires at 4600.
    assert_eq!(client.get_pause_state().unwrap().activated_at, 1000);
    advance(&env, 4600);
    assert!(!client.is_paused());
}

/// The cooldown after an auto-expiry also applies to pauser-initiated pauses.
#[test]
fn pauser_pause_respects_the_cooldown() {
    let env = Env::default();
    let (client, _admin, _sme, _treasury, _investor) = init_open_with_token(&env, "PAU016");
    let pauser = Address::generate(&env);
    client.set_pauser(&Some(pauser.clone()));
    client.set_pause_max_duration(&ONE_HOUR);
    client.set_pause_cooldown(&ONE_HOUR);

    // The admin activates the pause at t = 1000 -> expires at t = 4600.
    advance(&env, 1000);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);

    // The pause auto-expires at 4600.
    advance(&env, 4600);
    assert!(!client.is_paused());

    // Inside the cooldown the pauser cannot re-activate it.
    advance(&env, 8199);
    assert_contract_error(
        client.try_pause_as_pauser(&pauser, &PauseScope::All, &PauseReason::Incident),
        EscrowError::PauseCooldownActive,
    );

    // Once the cooldown elapsed, the pauser may pause again.
    advance(&env, 8200);
    client.pause_as_pauser(&pauser, &PauseScope::All, &PauseReason::Incident);
    assert!(client.is_paused());
    assert_eq!(client.get_pause_state().unwrap().activated_at, 8200);
}
