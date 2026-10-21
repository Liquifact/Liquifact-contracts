/// Tests for issue #1552 — hardened legal-hold state machine and unified event topics.
///
/// State machine: None → Active → ClearRequested → None
/// Every invalid transition has a typed-error test; every valid transition has an
/// event-sequence test that asserts the exact ordered list of events.
use super::*;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Deploy and init an escrow with zero clear delay (instant-clear path).
fn setup_hold_client(env: &Env) -> (LiquifactEscrowClient<'_>, Address) {
    env.mock_all_auths();
    let (client, admin, sme) = setup(env);
    let (token, treasury) = free_addresses(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "LH-SM"),
        &sme,
        &100_000_000i128,
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
    (client, admin)
}

/// Deploy and init an escrow with a nonzero clear delay (timed-clear path).
fn setup_hold_client_with_delay(
    env: &Env,
    delay_secs: u64,
) -> (LiquifactEscrowClient<'_>, Address) {
    env.mock_all_auths();
    let (client, admin, sme) = setup(env);
    let (token, treasury) = free_addresses(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "LH-DELAY"),
        &sme,
        &100_000_000i128,
        &800i64,
        &delay_secs,
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
    (client, admin)
}

/// Advance the ledger timestamp by `secs`.
fn advance_time(env: &Env, secs: u64) {
    let mut info = env.ledger().get();
    info.timestamp += secs;
    env.ledger().set(info);
}

// ── invalid transition: set_legal_hold(false) when no hold ───────────────────

#[test]
fn set_hold_false_when_inactive_is_no_op_no_event() {
    let env = Env::default();
    let (client, _admin) = setup_hold_client(&env);

    // No hold active — calling false is a no-op; no event emitted.
    client.set_legal_hold(&false);

    // No LegalHoldChanged event should have been emitted.
    let events = env.events().all();
    for i in 0..events.len() {
        let (_contract, topics, _data) = events.get(i).unwrap();
        if topics.len() >= 1 {
            let t0: Result<Symbol, _> = topics.get(0).unwrap().try_into_val(&env);
            if let Ok(sym) = t0 {
                assert_ne!(
                    sym,
                    symbol_short!("legalhld"),
                    "no LegalHoldChanged event should be emitted on no-op"
                );
            }
        }
    }
}

// ── invalid transition: set_legal_hold(true) when already active ─────────────

#[test]
fn set_hold_true_when_already_active_fails() {
    let env = Env::default();
    let (client, _admin) = setup_hold_client(&env);

    client.set_legal_hold(&true);

    let result = client.try_set_legal_hold(&true);
    assert_contract_error(result, EscrowError::LegalHoldAlreadyActive);
}

#[test]
fn set_hold_true_already_active_storage_unchanged() {
    let env = Env::default();
    let (client, _admin) = setup_hold_client(&env);

    client.set_legal_hold(&true);
    // Attempt to set again — should fail, storage stays the same.
    let _ = client.try_set_legal_hold(&true);

    // Hold is still active.
    assert!(client.legal_hold_active());
}

// ── invalid transition: request_clear when no hold ───────────────────────────

#[test]
fn request_clear_when_no_hold_fails() {
    let env = Env::default();
    let (client, _admin) = setup_hold_client(&env);

    let result = client.try_request_clear_legal_hold();
    assert_contract_error(result, EscrowError::LegalHoldNotActive);
}

#[test]
fn request_clear_when_no_hold_storage_unchanged() {
    let env = Env::default();
    let (client, _admin) = setup_hold_client(&env);

    // No hold — request_clear should fail and leave no LegalHoldClearableAt.
    let _ = client.try_request_clear_legal_hold();
    // Hold still inactive.
    assert!(!client.legal_hold_active());
}

// ── invalid transition: clear_after_delay when no hold ───────────────────────

#[test]
fn clear_after_delay_when_no_hold_fails() {
    let env = Env::default();
    let (client, _admin) = setup_hold_client(&env);

    let result = client.try_clear_legal_hold_after_delay();
    assert_contract_error(result, EscrowError::LegalHoldNotActive);
}

// ── topic unification: clear_after_delay emits "legalhld" ────────────────────

#[test]
fn clear_after_delay_emits_legalhld_topic() {
    let env = Env::default();
    let (client, _admin) = setup_hold_client(&env);

    // Set hold, request clear (zero delay → clearable immediately), then clear.
    client.set_legal_hold(&true);
    client.request_clear_legal_hold();
    client.clear_legal_hold_after_delay();

    // Find the LegalHoldChanged event emitted by clear_after_delay (active=0).
    let events = env.events().all();
    let mut found_clear_event = false;
    for i in 0..events.len() {
        let (_contract, topics, data) = events.get(i).unwrap();
        if topics.len() >= 1 {
            let t0: Result<Symbol, _> = topics.get(0).unwrap().try_into_val(&env);
            if let Ok(sym) = t0 {
                if sym == symbol_short!("legalhld") {
                    // Check it's the clear event (active=0).
                    let active: Result<u32, _> = data.try_into_val(&env);
                    if let Ok(a) = active {
                        if a == 0u32 {
                            found_clear_event = true;
                        }
                    }
                }
            }
        }
    }
    assert!(
        found_clear_event,
        "clear_legal_hold_after_delay must emit LegalHoldChanged with topic 'legalhld' and active=0"
    );
}

// ── full lifecycle event sequence ─────────────────────────────────────────────

#[test]
fn full_lifecycle_event_sequence_zero_delay() {
    let env = Env::default();
    let (client, _admin) = setup_hold_client(&env);

    // Lifecycle: activate → request clear → clear
    client.set_legal_hold(&true);
    client.request_clear_legal_hold();
    client.clear_legal_hold_after_delay();

    // Collect all events, filter to the legalhld / lh_req topics.
    let events = env.events().all();
    let mut sequence: soroban_sdk::Vec<Symbol> = soroban_sdk::Vec::new(&env);
    for i in 0..events.len() {
        let (_contract, topics, _data) = events.get(i).unwrap();
        if topics.len() >= 1 {
            let t0: Result<Symbol, _> = topics.get(0).unwrap().try_into_val(&env);
            if let Ok(sym) = t0 {
                if sym == symbol_short!("legalhld") || sym == symbol_short!("lh_req") {
                    sequence.push_back(sym);
                }
            }
        }
    }

    // Expected: legalhld (activate), lh_req (request), legalhld (clear).
    assert_eq!(sequence.len(), 3);
    assert_eq!(sequence.get(0).unwrap(), symbol_short!("legalhld"));
    assert_eq!(sequence.get(1).unwrap(), symbol_short!("lh_req"));
    assert_eq!(sequence.get(2).unwrap(), symbol_short!("legalhld"));
}

// ── timed lifecycle with delay ────────────────────────────────────────────────

#[test]
fn timed_lifecycle_clear_after_delay() {
    let env = Env::default();
    let delay = 3600u64;
    let (client, _admin) = setup_hold_client_with_delay(&env, delay);

    client.set_legal_hold(&true);
    client.request_clear_legal_hold();

    // Not ready yet — advance by less than the delay.
    advance_time(&env, delay - 1);
    let result = client.try_clear_legal_hold_after_delay();
    assert_contract_error(result, EscrowError::LegalHoldClearNotReady);

    // Advance past the delay.
    advance_time(&env, 2);
    client.clear_legal_hold_after_delay();
    assert!(!client.legal_hold_active());
}

// ── LegalHoldClearCancelled on cancel_clear_legal_hold ───────────────────────

#[test]
fn cancel_clear_emits_lh_cancel_event() {
    let env = Env::default();
    let delay = 1000u64;
    let (client, _admin) = setup_hold_client_with_delay(&env, delay);

    client.set_legal_hold(&true);
    client.request_clear_legal_hold();
    client.cancel_clear_legal_hold();

    // Find the lh_cancel event.
    let events = env.events().all();
    let mut found = false;
    for i in 0..events.len() {
        let (_contract, topics, _data) = events.get(i).unwrap();
        if topics.len() >= 1 {
            let t0: Result<Symbol, _> = topics.get(0).unwrap().try_into_val(&env);
            if let Ok(sym) = t0 {
                if sym == symbol_short!("lh_cancel") {
                    found = true;
                }
            }
        }
    }
    assert!(found, "cancel_clear_legal_hold must emit lh_cancel event");

    // Hold is still active after cancel.
    assert!(client.legal_hold_active());
}

// ── set_legal_hold(true) while clear pending: LegalHoldAlreadyActive ─────────

#[test]
fn set_hold_true_while_clear_pending_fails_already_active() {
    let env = Env::default();
    let delay = 1000u64;
    let (client, _admin) = setup_hold_client_with_delay(&env, delay);

    client.set_legal_hold(&true);
    client.request_clear_legal_hold();

    // Re-asserting while clear is pending must fail with LegalHoldAlreadyActive.
    let result = client.try_set_legal_hold(&true);
    assert_contract_error(result, EscrowError::LegalHoldAlreadyActive);
}

// ── clear without prior request fails ────────────────────────────────────────

#[test]
fn clear_without_request_fails() {
    let env = Env::default();
    let delay = 1000u64;
    let (client, _admin) = setup_hold_client_with_delay(&env, delay);

    client.set_legal_hold(&true);

    // Trying to clear without a prior request.
    let result = client.try_clear_legal_hold_after_delay();
    assert_contract_error(result, EscrowError::LegalHoldClearRequestMissing);

    // Hold still active, storage unchanged.
    assert!(client.legal_hold_active());
}

// ── both activation and clear emit identical "legalhld" topic ────────────────

#[test]
fn both_activate_and_clear_use_legalhld_topic() {
    let env = Env::default();
    let (client, _admin) = setup_hold_client(&env);

    client.set_legal_hold(&true);
    client.request_clear_legal_hold();
    client.clear_legal_hold_after_delay();

    let events = env.events().all();
    let mut legalhld_count = 0u32;
    for i in 0..events.len() {
        let (_contract, topics, _data) = events.get(i).unwrap();
        if topics.len() >= 1 {
            let t0: Result<Symbol, _> = topics.get(0).unwrap().try_into_val(&env);
            if let Ok(sym) = t0 {
                if sym == symbol_short!("legalhld") {
                    legalhld_count += 1;
                }
                // Verify "legal_h" is NEVER used.
                if sym == symbol_short!("legal_h") {
                    panic!("obsolete 'legal_h' topic found — must not be emitted");
                }
            }
        }
    }
    // Exactly two legalhld events: one for activate (active=1), one for clear (active=0).
    assert_eq!(legalhld_count, 2, "expected exactly 2 legalhld events");
}
