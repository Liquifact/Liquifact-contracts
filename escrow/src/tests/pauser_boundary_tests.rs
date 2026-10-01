//! Boundary and fuzz-style tests for the operational pauser subsystem.
//!
//! These tests validate numeric and length boundaries for pauser operations
//! — `set_pause_max_duration`, `set_pause_rate_limit`, and `set_paused` —
//! asserting the typed [`EscrowError`] variants for inputs at min, max,
//! zero, and over-limit positions.
//!
//! Mirrors the `collateral_boundary_tests.rs` precedent (PR #1131) and the
//! `MIN_PAUSE_*_SECS` / `MAX_PAUSE_*_SECS` contract constants.

use super::super::{
    EscrowError, LiquifactEscrow, LiquifactEscrowClient, PauseReason, PauseScope,
    MAX_PAUSE_MAX_DURATION_SECS, MAX_PAUSE_TOGGLE_LIMIT, MAX_PAUSE_TOGGLE_WINDOW_SECS,
    MIN_PAUSE_MAX_DURATION_SECS, MIN_PAUSE_TOGGLE_LIMIT, MIN_PAUSE_TOGGLE_WINDOW_SECS,
};
use crate::tests::assert_contract_error;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

fn setup_escrow(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    // Mirror of collateral_boundary_tests::setup_escrow; kept local for test isolation.
    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "PAUBND01"),
        &sme,
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
    );

    (client, admin, sme)
}

// ── set_pause_max_duration boundary tests ────────────────────────────────────

#[test]
fn test_pause_max_duration_at_min_seconds_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Exactly MIN — boundary, must succeed.
    client.set_pause_max_duration(&MIN_PAUSE_MAX_DURATION_SECS);
    assert_eq!(client.get_pause_max_duration(), MIN_PAUSE_MAX_DURATION_SECS);
}

#[test]
fn test_pause_max_duration_at_min_minus_one_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // One below MIN — rejected with typed error.
    assert_contract_error(
        client.try_set_pause_max_duration(&(MIN_PAUSE_MAX_DURATION_SECS - 1)),
        EscrowError::PauseMaxDurationOutOfRange,
    );

    // State must remain at the default (no auto-expiry).
    assert_eq!(client.get_pause_max_duration(), 0u64);
}

#[test]
fn test_pause_max_duration_at_max_seconds_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Exactly MAX — boundary, must succeed.
    client.set_pause_max_duration(&MAX_PAUSE_MAX_DURATION_SECS);
    assert_eq!(client.get_pause_max_duration(), MAX_PAUSE_MAX_DURATION_SECS);
}

#[test]
fn test_pause_max_duration_at_max_plus_one_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // One above MAX — rejected with typed error.
    assert_contract_error(
        client.try_set_pause_max_duration(&(MAX_PAUSE_MAX_DURATION_SECS + 1)),
        EscrowError::PauseMaxDurationOutOfRange,
    );

    // State must remain at the default (no auto-expiry).
    assert_eq!(client.get_pause_max_duration(), 0u64);
}

#[test]
fn test_pause_max_duration_at_zero_disables_auto_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // 0 is the special-case "disable auto-expiry" sentinel — must succeed.
    client.set_pause_max_duration(&0u64);
    assert_eq!(client.get_pause_max_duration(), 0u64);
}

#[test]
fn test_pause_max_duration_at_u64_max_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Far over MAX — fuzz-style over-limit input.
    assert_contract_error(
        client.try_set_pause_max_duration(&u64::MAX),
        EscrowError::PauseMaxDurationOutOfRange,
    );

    // State must remain at the default (no auto-expiry).
    assert_eq!(client.get_pause_max_duration(), 0u64);
}

// ── set_pause_rate_limit boundary tests ──────────────────────────────────────

#[test]
fn test_pause_rate_limit_at_min_limit_min_window_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Both at MIN — lower-corner valid configuration.
    let (limit, window) =
        client.set_pause_rate_limit(&MIN_PAUSE_TOGGLE_LIMIT, &MIN_PAUSE_TOGGLE_WINDOW_SECS);
    assert_eq!(limit, MIN_PAUSE_TOGGLE_LIMIT);
    assert_eq!(window, MIN_PAUSE_TOGGLE_WINDOW_SECS);

    let (got_limit, got_window) = client.get_pause_rate_limit();
    assert_eq!(got_limit, MIN_PAUSE_TOGGLE_LIMIT);
    assert_eq!(got_window, MIN_PAUSE_TOGGLE_WINDOW_SECS);
}

#[test]
fn test_pause_rate_limit_at_max_limit_max_window_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Both at MAX — upper-corner valid configuration.
    let (limit, window) =
        client.set_pause_rate_limit(&MAX_PAUSE_TOGGLE_LIMIT, &MAX_PAUSE_TOGGLE_WINDOW_SECS);
    assert_eq!(limit, MAX_PAUSE_TOGGLE_LIMIT);
    assert_eq!(window, MAX_PAUSE_TOGGLE_WINDOW_SECS);
}

#[test]
fn test_pause_rate_limit_both_zero_disables_rate_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Both zero is the special-case "disable rate limit" sentinel.
    let (limit, window) = client.set_pause_rate_limit(&0u32, &0u64);
    assert_eq!(limit, 0u32);
    assert_eq!(window, 0u64);
}

#[test]
fn test_pause_rate_limit_limit_zero_window_nonzero_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // limit=0 with window>0 is an invalid combination (XOR required).
    assert_contract_error(
        client.try_set_pause_rate_limit(&0u32, &MIN_PAUSE_TOGGLE_WINDOW_SECS),
        EscrowError::PauseRateLimitInvalidCombination,
    );

    // Rate limit must remain unset.
    let (limit, window) = client.get_pause_rate_limit();
    assert_eq!(limit, 0u32);
    assert_eq!(window, 0u64);
}

#[test]
fn test_pause_rate_limit_window_zero_limit_nonzero_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // window=0 with limit>0 is an invalid combination (XOR required).
    assert_contract_error(
        client.try_set_pause_rate_limit(&MIN_PAUSE_TOGGLE_LIMIT, &0u64),
        EscrowError::PauseRateLimitInvalidCombination,
    );

    // Rate limit must remain unset.
    let (limit, window) = client.get_pause_rate_limit();
    assert_eq!(limit, 0u32);
    assert_eq!(window, 0u64);
}

#[test]
fn test_pause_rate_limit_above_max_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // limit = MAX + 1 — fuzz over-limit.
    assert_contract_error(
        client
            .try_set_pause_rate_limit(&(MAX_PAUSE_TOGGLE_LIMIT + 1), &MIN_PAUSE_TOGGLE_WINDOW_SECS),
        EscrowError::PauseToggleLimitOutOfRange,
    );

    // Rate limit must remain unset.
    let (limit, window) = client.get_pause_rate_limit();
    assert_eq!(limit, 0u32);
    assert_eq!(window, 0u64);
}

#[test]
fn test_pause_rate_limit_below_min_window_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // window = MIN - 1 — fuzz under-limit.
    assert_contract_error(
        client
            .try_set_pause_rate_limit(&MIN_PAUSE_TOGGLE_LIMIT, &(MIN_PAUSE_TOGGLE_WINDOW_SECS - 1)),
        EscrowError::PauseToggleWindowOutOfRange,
    );

    // Rate limit must remain unset.
    let (limit, window) = client.get_pause_rate_limit();
    assert_eq!(limit, 0u32);
    assert_eq!(window, 0u64);
}

#[test]
fn test_pause_rate_limit_above_max_window_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // window = MAX + 1 — fuzz over-limit.
    assert_contract_error(
        client
            .try_set_pause_rate_limit(&MIN_PAUSE_TOGGLE_LIMIT, &(MAX_PAUSE_TOGGLE_WINDOW_SECS + 1)),
        EscrowError::PauseToggleWindowOutOfRange,
    );

    // Rate limit must remain unset.
    let (limit, window) = client.get_pause_rate_limit();
    assert_eq!(limit, 0u32);
    assert_eq!(window, 0u64);
}

#[test]
fn test_pause_rate_limit_u32_max_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Fuzz-style extreme over-limit for limit field.
    assert_contract_error(
        client.try_set_pause_rate_limit(&u32::MAX, &MIN_PAUSE_TOGGLE_WINDOW_SECS),
        EscrowError::PauseToggleLimitOutOfRange,
    );

    // Rate limit must remain unset.
    let (limit, window) = client.get_pause_rate_limit();
    assert_eq!(limit, 0u32);
    assert_eq!(window, 0u64);
}

// ── set_paused + rate-limit boundary tests ───────────────────────────────────

#[test]
fn test_set_paused_within_rate_limit_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Configure rate limit: 2 toggles per window.
    client.set_pause_rate_limit(&2u32, &60u64);

    // First two toggles within the window must succeed.
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    assert!(client.is_paused());
    client.set_paused(&false, &PauseScope::All, &PauseReason::Incident);
    assert!(!client.is_paused());
}

#[test]
fn test_set_paused_third_toggle_within_window_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Configure rate limit: 2 toggles per window.
    client.set_pause_rate_limit(&2u32, &60u64);

    // Burn the quota.
    // Counts every call, including no-op true→true transitions.
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    client.set_paused(&false, &PauseScope::All, &PauseReason::Incident);

    // Third toggle within the same window must be rejected.
    assert_contract_error(
        client.try_set_paused(&true, &PauseScope::All, &PauseReason::Incident),
        EscrowError::PauseToggleRateLimitExceeded,
    );

    // is_paused must still report the post-2nd-toggle state (false).
    assert!(!client.is_paused());
}

#[test]
fn test_pause_window_expiry_resets_counter() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Configure a tight 60-second window.
    client.set_pause_rate_limit(&1u32, &60u64);

    // Burn the quota.
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);

    // Still within the window — toggle rejected.
    assert_contract_error(
        client.try_set_paused(&false, &PauseScope::All, &PauseReason::Incident),
        EscrowError::PauseToggleRateLimitExceeded,
    );

    // Advance the ledger past the window expiry (now + 61 seconds).
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp += 61;
    env.ledger().set(ledger_info);

    // After window expiry, the counter must reset and the next toggle succeeds.
    client.set_paused(&false, &PauseScope::All, &PauseReason::Incident);
    assert!(!client.is_paused());
}

#[test]
fn test_pause_rate_limit_invalid_reconfigure_preserves_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Configure a valid rate limit and burn the quota.
    client.set_pause_rate_limit(&1u32, &60u64);
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);

    // Attempt an invalid reconfigure (limit > MAX).
    assert_contract_error(
        client.try_set_pause_rate_limit(&(MAX_PAUSE_TOGGLE_LIMIT + 1), &60u64),
        EscrowError::PauseToggleLimitOutOfRange,
    );

    // Invalid reconfigure must be a no-op: prior (limit, window) preserved
    // AND the next toggle is still rejected.
    let (limit, window) = client.get_pause_rate_limit();
    assert_eq!(limit, 1u32);
    assert_eq!(window, 60u64);
    assert_contract_error(
        client.try_set_paused(&false, &PauseScope::All, &PauseReason::Incident),
        EscrowError::PauseToggleRateLimitExceeded,
    );
}

#[test]
fn test_set_paused_rate_limit_one_per_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Tighter bound: only 1 toggle per window.
    client.set_pause_rate_limit(&MIN_PAUSE_TOGGLE_LIMIT, &MIN_PAUSE_TOGGLE_WINDOW_SECS);

    // First toggle consumes the quota.
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);

    // Second toggle must be rejected.
    assert_contract_error(
        client.try_set_paused(&false, &PauseScope::All, &PauseReason::Incident),
        EscrowError::PauseToggleRateLimitExceeded,
    );
}

// ── get_pause_* default-value tests ──────────────────────────────────────────

#[test]
fn test_get_pause_max_duration_default_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Before any set_pause_max_duration call, the default is 0 (no auto-expiry).
    assert_eq!(client.get_pause_max_duration(), 0u64);
}

#[test]
fn test_get_pause_rate_limit_default_zero_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Before any set_pause_rate_limit call, the default is (0, 0) (rate limit disabled).
    let (limit, window) = client.get_pause_rate_limit();
    assert_eq!(limit, 0u32);
    assert_eq!(window, 0u64);
}

// ── Mixed-corner and reset-on-reconfigure coverage ───────────────────────────

#[test]
fn test_pause_rate_limit_min_limit_max_window_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Diagonal: limit at MIN, window at MAX. Both fields are validated
    // independently — a future bug that ANDs the bounds must not slip through.
    let (limit, window) =
        client.set_pause_rate_limit(&MIN_PAUSE_TOGGLE_LIMIT, &MAX_PAUSE_TOGGLE_WINDOW_SECS);
    assert_eq!(limit, MIN_PAUSE_TOGGLE_LIMIT);
    assert_eq!(window, MAX_PAUSE_TOGGLE_WINDOW_SECS);
}

#[test]
fn test_pause_rate_limit_max_limit_min_window_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Inverse diagonal: limit at MAX, window at MIN.
    let (limit, window) =
        client.set_pause_rate_limit(&MAX_PAUSE_TOGGLE_LIMIT, &MIN_PAUSE_TOGGLE_WINDOW_SECS);
    assert_eq!(limit, MAX_PAUSE_TOGGLE_LIMIT);
    assert_eq!(window, MIN_PAUSE_TOGGLE_WINDOW_SECS);
}

#[test]
fn test_pause_rate_limit_reconfigure_resets_toggle_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_escrow(&env);

    // Configure a tight rate limit: 2 toggles per window.
    client.set_pause_rate_limit(&2u32, &MIN_PAUSE_TOGGLE_WINDOW_SECS);

    // Burn the quota: two toggles succeed, third is rejected.
    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    client.set_paused(&false, &PauseScope::All, &PauseReason::Incident);
    assert_contract_error(
        client.try_set_paused(&true, &PauseScope::All, &PauseReason::Incident),
        EscrowError::PauseToggleRateLimitExceeded,
    );

    // Reconfiguring the rate limit must reset the toggle window so the next
    // toggle is allowed again.
    client.set_pause_rate_limit(&MIN_PAUSE_TOGGLE_LIMIT, &MIN_PAUSE_TOGGLE_WINDOW_SECS);

    client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    assert!(client.is_paused());
}
