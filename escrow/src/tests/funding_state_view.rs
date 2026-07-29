//! Read-only funding-state view coverage (issue #688).
//!
//! Scope: assertions against `get_funding_state` only. No existing behavior is modified.

use super::{default_init, init_and_fund_with_real_token, setup, TARGET};
use crate::EscrowCloseSnapshot;
use soroban_sdk::Env;

#[test]
fn funding_state_defaults_before_init() {
    let env = Env::default();
    let (client, _admin, _sme) = setup(&env);

    let state = client.get_funding_state();

    assert_eq!(state.funding_target, 0);
    assert_eq!(state.funded_amount, 0);
    assert_eq!(state.remaining_to_target, 0);
    assert!(!state.target_reached);
    assert_eq!(state.unique_funder_count, 0);
    assert_eq!(state.funding_deadline, 0);
    assert!(!state.has_funding_deadline);
    assert!(!state.is_expired);
    assert_eq!(state.status, 0);
    assert_eq!(state.close_snapshot, EscrowCloseSnapshot::None);
}

#[test]
fn funding_state_after_init_is_open_and_unfunded() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    let state = client.get_funding_state();

    assert_eq!(state.funding_target, TARGET);
    assert_eq!(state.funded_amount, 0);
    assert_eq!(state.remaining_to_target, TARGET);
    assert!(!state.target_reached);
    assert_eq!(state.status, 0);
    assert!(!state.has_funding_deadline);
    assert_eq!(state.close_snapshot, EscrowCloseSnapshot::None);
}

#[test]
fn funding_state_agrees_with_standalone_getters() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _escrow_id, _sme) = init_and_fund_with_real_token(&env, TARGET, "INV688");

    let state = client.get_funding_state();
    let escrow = client.get_escrow();

    assert_eq!(state.unique_funder_count, client.get_unique_funder_count());
    assert_eq!(state.is_expired, client.is_funding_expired());
    assert_eq!(state.status, escrow.status);
    assert_eq!(state.funded_amount, escrow.funded_amount);
    assert_eq!(state.funding_target, escrow.funding_target);
}

#[test]
fn funding_state_reports_target_reached_and_snapshot_once_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _escrow_id, _sme) = init_and_fund_with_real_token(&env, TARGET, "INV689");

    let state = client.get_funding_state();

    assert_eq!(state.funded_amount, TARGET);
    assert_eq!(state.remaining_to_target, 0);
    assert!(state.target_reached);
    assert_eq!(state.status, 1);
    assert_ne!(state.close_snapshot, EscrowCloseSnapshot::None);
    assert_eq!(state.unique_funder_count, 1);
}
