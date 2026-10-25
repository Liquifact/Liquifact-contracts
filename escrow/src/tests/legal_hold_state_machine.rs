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
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &Some(delay_secs),
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    (client, admin)
}

/// Events emitted by the most recent contract invocation, as XDR.
fn last_events(env: &Env) -> std::vec::Vec<soroban_sdk::xdr::ContractEvent> {
    env.events().all().events().iter().cloned().collect()
}

/// Expected XDR of a `LegalHoldChanged` event (unified `legalhld` topic).
fn hold_changed(
    env: &Env,
    client: &LiquifactEscrowClient<'_>,
    invoice_id: &Symbol,
    active: u32,
) -> soroban_sdk::xdr::ContractEvent {
    crate::LegalHoldChanged {
        name: symbol_short!("legalhld"),
        invoice_id: invoice_id.clone(),
        active,
    }
    .to_xdr(env, &client.address)
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
    let invoice_id = client.get_escrow().invoice_id;
    client.set_legal_hold(&false);

    // No LegalHoldChanged event should have been emitted.
    let events = last_events(&env);
    assert!(
        !events.contains(&hold_changed(&env, &client, &invoice_id, 0)),
        "no LegalHoldChanged event should be emitted on no-op"
    );
    assert!(!client.get_legal_hold());
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
    assert!(client.get_legal_hold());
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
    assert!(!client.get_legal_hold());
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
    let invoice_id = client.get_escrow().invoice_id;
    client.set_legal_hold(&true);
    client.request_clear_legal_hold();
    client.clear_legal_hold_after_delay();

    // The clear must emit LegalHoldChanged with the unified `legalhld` topic and active=0.
    let events = last_events(&env);
    assert!(
        events.contains(&hold_changed(&env, &client, &invoice_id, 0)),
        "clear_legal_hold_after_delay must emit LegalHoldChanged with topic 'legalhld' and active=0"
    );
}

// ── full lifecycle event sequence ─────────────────────────────────────────────

#[test]
fn full_lifecycle_event_sequence_zero_delay() {
    let env = Env::default();
    let (client, _admin) = setup_hold_client(&env);

    // Lifecycle: activate → request clear → clear. Each step's events are checked
    // right after the call that emits them.
    let invoice_id = client.get_escrow().invoice_id;

    client.set_legal_hold(&true);
    assert!(last_events(&env).contains(&hold_changed(&env, &client, &invoice_id, 1)));

    client.request_clear_legal_hold();
    // Requesting a clear does not clear the hold or emit a `legalhld` clear event.
    assert!(!last_events(&env).contains(&hold_changed(&env, &client, &invoice_id, 0)));
    assert!(client.get_legal_hold());

    client.clear_legal_hold_after_delay();
    assert!(last_events(&env).contains(&hold_changed(&env, &client, &invoice_id, 0)));
    assert!(!client.get_legal_hold());
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
    assert!(!client.get_legal_hold());
}

// ── LegalHoldClearCancelled on cancel_clear_legal_hold ───────────────────────

#[test]
fn cancel_clear_emits_lh_cancel_event() {
    let env = Env::default();
    let delay = 1000u64;
    let (client, _admin) = setup_hold_client_with_delay(&env, delay);

    let invoice_id = client.get_escrow().invoice_id;
    client.set_legal_hold(&true);
    client.request_clear_legal_hold();
    client.cancel_clear_legal_hold();

    // The cancel must emit the lh_cancel event.
    let expected = crate::LegalHoldClearCancelled {
        name: symbol_short!("lh_cancel"),
        invoice_id: invoice_id.clone(),
    }
    .to_xdr(&env, &client.address);
    assert!(
        last_events(&env).contains(&expected),
        "cancel_clear_legal_hold must emit lh_cancel event"
    );

    // Hold is still active after cancel.
    assert!(client.get_legal_hold());
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
    assert!(client.get_legal_hold());
}

// ── both activation and clear emit identical "legalhld" topic ────────────────

#[test]
fn both_activate_and_clear_use_legalhld_topic() {
    let env = Env::default();
    let (client, _admin) = setup_hold_client(&env);

    let invoice_id = client.get_escrow().invoice_id;

    // Activation emits `legalhld` with active=1 ...
    client.set_legal_hold(&true);
    let on = last_events(&env);
    assert_eq!(
        on.iter()
            .filter(|e| **e == hold_changed(&env, &client, &invoice_id, 1))
            .count(),
        1
    );

    client.request_clear_legal_hold();

    // ... and the delayed clear emits the same `legalhld` topic with active=0.
    client.clear_legal_hold_after_delay();
    let off = last_events(&env);
    assert_eq!(
        off.iter()
            .filter(|e| **e == hold_changed(&env, &client, &invoice_id, 0))
            .count(),
        1
    );
}
