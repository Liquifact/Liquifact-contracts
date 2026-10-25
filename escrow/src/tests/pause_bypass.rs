//! Regression tests for the pause max-duration bypass (issue #1549).
//!
//! An active pause must not be extendable by re-pausing it: re-pausing keeps the
//! original activation timestamp, may only widen the scope, and a fresh pause may
//! only start once the configured cooldown after the previous expiry has elapsed.
//!
//! Reuses the shared helpers from [`crate::tests`]: [`super::setup`],
//! [`super::install_stellar_asset_token`], [`super::assert_contract_error`] and
//! [`super::TARGET`].

use super::{assert_contract_error, install_stellar_asset_token, setup, TARGET};
use crate::{
    EscrowError, LiquifactEscrowClient, PauseReason, PauseScope, PausedChanged,
    MAX_PAUSE_MAX_DURATION_SECS,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, Event,
};

/// One hour — the max duration used by the bypass tests.
const ONE_HOUR: u64 = 3_600;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Move the ledger clock to `ts`.
fn advance(env: &Env, ts: u64) {
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = ts;
    env.ledger().set(ledger_info);
}

/// Deploy an escrow backed by a real Stellar asset token, left in the **open**
/// (unfunded) state so that `fund` can be exercised while a pause is active.
///
/// Returns `(client, admin, sme, investor, token)`.
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

    (client, admin, sme, investor, sac.id)
}

// ── 1. The bypass itself ──────────────────────────────────────────────────────

/// The core regression: re-pausing an active pause must not extend it.
///
/// Max duration is 1 hour and the pause is re-activated at 59 minutes. On `main`
/// the re-pause reset `PausedAt`, so the entrypoints stayed blocked well past the
/// configured 1 hour cap. Here the original activation timestamp survives, the
/// pause expires at `1000 + 3600`, and funding is unblocked afterwards.
#[test]
fn re_pause_does_not_extend_max_duration() {
    let env = Env::default();
    let (client, _admin, _sme, investor, _token) = init_open_with_token(&env, "PBY001");

    client.set_pause_max_duration(&ONE_HOUR);

    // Pause at t = 1000 -> expires at t = 4600.
    advance(&env, 1000);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    assert!(client.is_paused());

    // While the pause is effective, funding is blocked.
    assert_contract_error(
        client.try_fund(&investor, &TARGET),
        EscrowError::PausedBlocksFunding,
    );

    // Re-pause at 59 minutes (t = 4599) — the pause is still active, so this must
    // NOT reset the max-duration clock.
    advance(&env, 4599);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    assert!(client.is_paused());
    assert_eq!(client.get_pause_state().unwrap().activated_at, 1000);

    // At 1 hour the pause has expired.
    advance(&env, 4600);
    assert!(!client.is_paused());

    // At 1h30m the entrypoints must be usable again. On `main` the re-pause above
    // pushed the expiry out to 4599 + 3600, so funding was still blocked here.
    advance(&env, 5400);
    assert!(!client.is_paused());
    client.fund(&investor, &TARGET);
}

// ── 2. Scope changes are explicit ────────────────────────────────────────────

/// Widening the scope of an active pause keeps the original activation timestamp
/// and emits `PausedChanged` carrying the new scope.
#[test]
fn widening_keeps_original_timestamp_and_emits_event() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _token) = init_open_with_token(&env, "PBY002");
    let contract_id = client.address.clone();

    client.set_pause_max_duration(&ONE_HOUR);
    advance(&env, 1000);
    client.set_paused(&true, &PauseScope::Funding, &PauseReason::Maintenance);

    let state = client.get_pause_state().unwrap();
    assert_eq!(state.scope, PauseScope::Funding);
    assert_eq!(state.activated_at, 1000);

    // Widen Funding -> All while the pause is still active.
    advance(&env, 1500);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);

    // The widening is announced with the new scope. The events of the *last*
    // invocation are inspected, so they are read before any other call.
    let events = env.events().all();
    let last = events.events().last().unwrap().clone();
    assert_eq!(
        last,
        PausedChanged {
            name: symbol_short!("paused"),
            invoice_id: client.get_escrow().invoice_id,
            active: 1,
            scope: PauseScope::All,
            reason: PauseReason::Incident,
        }
        .to_xdr(&env, &contract_id)
    );

    let state = client.get_pause_state().unwrap();
    assert_eq!(state.scope, PauseScope::All);
    assert_eq!(state.reason, PauseReason::Incident);
    // The original activation timestamp survives the widening.
    assert_eq!(state.activated_at, 1000);

    // Expiry is still measured from the original activation.
    advance(&env, 4600);
    assert!(!client.is_paused());
}

/// Narrowing the scope of an active pause is rejected; the active pause is left
/// untouched.
#[test]
fn narrowing_fails_with_pause_already_active() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _token) = init_open_with_token(&env, "PBY003");

    client.set_pause_max_duration(&ONE_HOUR);
    advance(&env, 1000);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);

    // All -> Funding is a narrowing: rejected.
    advance(&env, 1500);
    assert_contract_error(
        client.try_set_paused(&true, &PauseScope::Funding, &PauseReason::Maintenance),
        EscrowError::PauseAlreadyActive,
    );

    // The active pause is unchanged.
    let state = client.get_pause_state().unwrap();
    assert_eq!(state.scope, PauseScope::All);
    assert_eq!(state.reason, PauseReason::Incident);
    assert_eq!(state.activated_at, 1000);
    assert!(client.is_paused());
}

/// A different single-family scope is not a widening either: `Funding` cannot be
/// swapped for `Withdrawal` while a pause is active.
#[test]
fn switching_between_family_scopes_fails_with_pause_already_active() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _token) = init_open_with_token(&env, "PBY004");

    client.set_pause_max_duration(&ONE_HOUR);
    advance(&env, 1000);
    client.set_paused(&true, &PauseScope::Funding, &PauseReason::Incident);

    advance(&env, 1500);
    assert_contract_error(
        client.try_set_paused(&true, &PauseScope::Withdrawal, &PauseReason::Incident),
        EscrowError::PauseAlreadyActive,
    );

    let state = client.get_pause_state().unwrap();
    assert_eq!(state.scope, PauseScope::Funding);
    assert_eq!(state.activated_at, 1000);
}

/// Re-pausing with the *same* scope is an idempotent no-op that keeps the original
/// activation timestamp (it is not a narrowing).
#[test]
fn same_scope_re_pause_keeps_original_timestamp() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _token) = init_open_with_token(&env, "PBY005");

    client.set_pause_max_duration(&ONE_HOUR);
    advance(&env, 1000);
    client.set_paused(&true, &PauseScope::Withdrawal, &PauseReason::Maintenance);

    // Re-pause at t = 2000 — still within the 1 h max duration.
    advance(&env, 2000);
    client.set_paused(&true, &PauseScope::Withdrawal, &PauseReason::Maintenance);

    let state = client.get_pause_state().unwrap();
    assert_eq!(state.scope, PauseScope::Withdrawal);
    assert_eq!(state.activated_at, 1000);
    assert!(client.is_paused());
}

// ── 3. Cooldown after expiry ──────────────────────────────────────────────────

/// A new pause may only start once `PausedAt + max_duration + cooldown` is reached.
#[test]
fn cooldown_boundaries_are_enforced() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _token) = init_open_with_token(&env, "PBY006");

    client.set_pause_max_duration(&ONE_HOUR);
    client.set_pause_cooldown(&ONE_HOUR);
    assert_eq!(client.get_pause_cooldown(), ONE_HOUR);

    // Pause at t = 1000 -> expires at t = 4600.
    advance(&env, 1000);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    assert!(client.is_paused());

    // The pause auto-expires at 4600 (the stored flag is cleared lazily).
    advance(&env, 4600);
    assert!(!client.is_paused());

    // expiry + cooldown - 1 = 8199 -> still inside the cooldown.
    advance(&env, 8199);
    assert_contract_error(
        client.try_set_paused(&true, &PauseScope::All, &PauseReason::Incident),
        EscrowError::PauseCooldownActive,
    );

    // expiry + cooldown = 8200 -> the cooldown has elapsed, activation succeeds
    // with a fresh clock.
    advance(&env, 8200);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    assert!(client.is_paused());
    let state = client.get_pause_state().unwrap();
    assert_eq!(state.activated_at, 8200);
}

/// The stale `Paused` flag and `PauseState` of an expired pause are cleared when the
/// expiry is noticed during `set_paused`.
#[test]
fn expired_pause_state_is_cleared_on_next_activation() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _token) = init_open_with_token(&env, "PBY007");

    client.set_pause_max_duration(&ONE_HOUR);
    advance(&env, 1000);
    client.set_paused(&true, &PauseScope::Funding, &PauseReason::Incident);
    assert_eq!(client.get_pause_state().unwrap().scope, PauseScope::Funding);

    // The pause expires at 4600; the typed state is no longer effective.
    advance(&env, 4600);
    assert!(!client.is_paused());
    assert!(client.get_pause_state().is_none());

    // The next activation starts from a clean slate: a narrow scope is accepted
    // because no pause is active any more.
    advance(&env, 4601);
    client.set_paused(&true, &PauseScope::Funding, &PauseReason::Security);
    let state = client.get_pause_state().unwrap();
    assert_eq!(state.scope, PauseScope::Funding);
    assert_eq!(state.activated_at, 4601);
}

/// With the default cooldown (`0`) the behaviour is unchanged: a new pause may start
/// as soon as the previous one expired.
#[test]
fn zero_cooldown_allows_immediate_reactivation() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _token) = init_open_with_token(&env, "PBY008");

    assert_eq!(client.get_pause_cooldown(), 0);
    client.set_pause_max_duration(&ONE_HOUR);

    advance(&env, 1000);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);

    // One second past the expiry the pause can be re-activated.
    advance(&env, 4601);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    assert_eq!(client.get_pause_state().unwrap().activated_at, 4601);
}

// ── 4. Unlimited max duration (legacy behaviour) ──────────────────────────────

/// With `max_duration == 0` (unlimited) the pause never auto-expires, exactly as
/// before. The only difference is the widen/narrow rule, which is asserted here.
#[test]
fn unlimited_max_duration_matches_legacy_behaviour() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _token) = init_open_with_token(&env, "PBY009");

    // Default: no max duration -> the pause never expires on its own.
    assert_eq!(client.get_pause_max_duration(), 0);

    advance(&env, 1000);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    assert!(client.is_paused());

    // The new rule: narrowing is rejected even when the duration is unlimited.
    assert_contract_error(
        client.try_set_paused(&true, &PauseScope::Funding, &PauseReason::Maintenance),
        EscrowError::PauseAlreadyActive,
    );

    // Widening is allowed and keeps the original activation timestamp.
    advance(&env, 5_000);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    assert_eq!(client.get_pause_state().unwrap().activated_at, 1000);

    // The pause is still effective far beyond any max duration.
    advance(&env, 100_000);
    assert!(client.is_paused());

    // An explicit un-pause still clears it.
    client.set_paused(&false, &PauseScope::All, &PauseReason::Incident);
    assert!(!client.is_paused());
    assert!(client.get_pause_state().is_none());

    // Re-activating after an explicit un-pause starts a fresh clock (no cooldown:
    // the previous pause was cleared, not expired).
    advance(&env, 100_001);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    assert_eq!(client.get_pause_state().unwrap().activated_at, 100_001);
}

// ── 5. Cooldown configuration ─────────────────────────────────────────────────

/// `set_pause_cooldown` accepts `0..=MAX_PAUSE_MAX_DURATION_SECS`.
#[test]
fn set_pause_cooldown_bounds() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _token) = init_open_with_token(&env, "PBY010");

    // Above the ceiling -> rejected.
    assert_contract_error(
        client.try_set_pause_cooldown(&(MAX_PAUSE_MAX_DURATION_SECS + 1)),
        EscrowError::PauseCooldownOutOfRange,
    );

    // Exactly at the ceiling -> accepted.
    client.set_pause_cooldown(&MAX_PAUSE_MAX_DURATION_SECS);
    assert_eq!(client.get_pause_cooldown(), MAX_PAUSE_MAX_DURATION_SECS);

    // Zero disables the cooldown.
    client.set_pause_cooldown(&0);
    assert_eq!(client.get_pause_cooldown(), 0);
}

/// A non-admin cannot change the cooldown.
#[test]
#[should_panic]
fn set_pause_cooldown_by_non_admin_panics() {
    let env = Env::default();
    let (client, _admin, _sme, _investor, _token) = init_open_with_token(&env, "PBY011");
    env.mock_auths(&[]);
    client.set_pause_cooldown(&ONE_HOUR);
}
