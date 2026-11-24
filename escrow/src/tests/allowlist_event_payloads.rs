//! Tests for allowlist event topic symbols and payload fields.
//!
//! Covers:
//! - `AllowlistEnabledChanged` payload fields from [`LiquifactEscrow::set_allowlist_active`].
//! - `InvestorAllowlistChanged` payload fields from [`LiquifactEscrow::set_investor_allowlisted`].
//! - `InvestorAllowlistChanged` per-entry events from [`LiquifactEscrow::set_investors_allowlisted`].
//! - No-op behavior: setting already-allowlisted to true or non-allowlisted to false emits no event.

use super::*;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::Event;

// ── helpers ──────────────────────────────────────────────────────────────────

fn init_for_allowlist(env: &Env, client: &LiquifactEscrowClient) -> Address {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let (tok, tre) = free_addresses(env);
    client.init(
        &admin,
        &String::from_str(env, "ALLOW01"),
        &sme,
        &100_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
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
    admin
}

// ── AllowlistEnabledChanged event tests ──────────────────────────────────────

/// Verify that `set_allowlist_active` emits `AllowlistEnabledChanged` with the
/// correct payload fields.
#[test]
fn test_allowlist_enabled_changed_event_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let admin = init_for_allowlist(&env, &client);
    let invoice_id = client.get_escrow().invoice_id;

    let _ = env.events().all(); // drain init events
    client.set_allowlist_active(&true);

    let all_events = env.events().all();

    assert_eq!(
        all_events,
        std::vec![AllowlistEnabledChanged {
            name: symbol_short!("al_ena"),
            invoice_id: invoice_id.clone(),
            active: 1,
        }
        .to_xdr(&env, &contract_id)],
        "AllowlistEnabledChanged payload must match expected fields"
    );
}

/// Verify that disabling the allowlist emits `AllowlistEnabledChanged` with `active: 0`.
#[test]
fn test_allowlist_enabled_changed_event_disabled() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let admin = init_for_allowlist(&env, &client);
    let invoice_id = client.get_escrow().invoice_id;

    // Enable first
    client.set_allowlist_active(&true);
    let _ = env.events().all(); // drain enable event

    // Disable
    client.set_allowlist_active(&false);

    let all_events = env.events().all();

    assert_eq!(
        all_events,
        std::vec![AllowlistEnabledChanged {
            name: symbol_short!("al_ena"),
            invoice_id,
            active: 0,
        }
        .to_xdr(&env, &contract_id)],
        "AllowlistEnabledChanged with active=false must report 0"
    );
}

// ── InvestorAllowlistChanged event tests ─────────────────────────────────────

/// Verify that `set_investor_allowlisted` emits `InvestorAllowlistChanged` with the
/// correct payload fields when adding an investor.
#[test]
fn test_investor_allowlist_changed_event_payload_add() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let admin = init_for_allowlist(&env, &client);
    let investor = Address::generate(&env);
    let invoice_id = client.get_escrow().invoice_id;

    let _ = env.events().all(); // drain init events
    client.set_investor_allowlisted(&investor, &true);

    let all_events = env.events().all();

    assert_eq!(
        all_events,
        std::vec![InvestorAllowlistChanged {
            name: symbol_short!("al_set"),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            allowed: 1,
        }
        .to_xdr(&env, &contract_id)],
        "InvestorAllowlistChanged payload must match expected fields"
    );
}

/// Verify that removing an investor emits `InvestorAllowlistChanged` with `allowed: 0`.
#[test]
fn test_investor_allowlist_changed_event_remove() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let admin = init_for_allowlist(&env, &client);
    let investor = Address::generate(&env);
    let invoice_id = client.get_escrow().invoice_id;

    // Add first
    client.set_investor_allowlisted(&investor, &true);
    let _ = env.events().all(); // drain add event

    // Remove
    client.set_investor_allowlisted(&investor, &false);

    let all_events = env.events().all();

    assert_eq!(
        all_events,
        std::vec![InvestorAllowlistChanged {
            name: symbol_short!("al_set"),
            invoice_id,
            investor,
            allowed: 0,
        }
        .to_xdr(&env, &contract_id)],
        "InvestorAllowlistChanged with allowed=false must report 0"
    );
}

/// Verify that setting an already-allowlisted investor to true is a no-op and emits no event.
#[test]
fn test_set_investor_allowlisted_noop_already_allowlisted() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let admin = init_for_allowlist(&env, &client);
    let investor = Address::generate(&env);

    // Add investor
    client.set_investor_allowlisted(&investor, &true);
    let _ = env.events().all(); // drain add event

    // Set to true again (no-op) - should not emit event
    client.set_investor_allowlisted(&investor, &true);
    let events_after = env.events().all();

    assert_eq!(
        events_after.events().len(),
        0,
        "setting already-allowlisted investor to true must not emit an event"
    );
}

/// Verify that setting a non-allowlisted investor to false is a no-op and emits no event.
#[test]
fn test_set_investor_allowlisted_noop_non_allowlisted() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let admin = init_for_allowlist(&env, &client);
    let investor = Address::generate(&env);

    let _ = env.events().all(); // drain init events

    // Set non-allowlisted investor to false (no-op) - should not emit event
    client.set_investor_allowlisted(&investor, &false);
    let events_after = env.events().all();

    assert_eq!(
        events_after.events().len(),
        0,
        "setting non-allowlisted investor to false must not emit an event"
    );
}

// ── set_investors_allowlisted per-entry event tests ───────────────────────────

/// Verify that `set_investors_allowlisted` emits one `InvestorAllowlistChanged` event
/// per batch entry, each with the correct per-entry `investor` and `allowed` fields.
#[test]
fn test_set_investors_allowlisted_per_entry_events() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);

    let admin = init_for_allowlist(&env, &client);
    let invoice_id = client.get_escrow().invoice_id;

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let inv3 = Address::generate(&env);

    let mut investors = SorobanVec::new(&env);
    investors.push_back(inv1.clone());
    investors.push_back(inv2.clone());
    investors.push_back(inv3.clone());

    let _ = env.events().all(); // drain init events
    client.set_investors_allowlisted(&investors, &true);

    let all_events = env.events().all();

    // Three InvestorAllowlistChanged events, one per entry.
    let events_for_inv1: std::vec::Vec<_> = all_events
        .events()
        .iter()
        .filter(|e| {
            let expected = InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: inv1.clone(),
                allowed: 1,
            };
            **e == expected.to_xdr(&env, &contract_id)
        })
        .collect();
    assert_eq!(
        events_for_inv1.len(),
        1,
        "exactly one InvestorAllowlistChanged for inv1"
    );

    let events_for_inv2: std::vec::Vec<_> = all_events
        .events()
        .iter()
        .filter(|e| {
            let expected = InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: inv2.clone(),
                allowed: 1,
            };
            **e == expected.to_xdr(&env, &contract_id)
        })
        .collect();
    assert_eq!(
        events_for_inv2.len(),
        1,
        "exactly one InvestorAllowlistChanged for inv2"
    );

    let events_for_inv3: std::vec::Vec<_> = all_events
        .events()
        .iter()
        .filter(|e| {
            let expected = InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: inv3.clone(),
                allowed: 1,
            };
            **e == expected.to_xdr(&env, &contract_id)
        })
        .collect();
    assert_eq!(
        events_for_inv3.len(),
        1,
        "exactly one InvestorAllowlistChanged for inv3"
    );

    // Total: exactly 3 InvestorAllowlistChanged events.
    assert_eq!(all_events.events().len(), 3);
}
