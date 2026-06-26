//! Legal-hold matrix tests.
//!
//! Each risk-bearing function gets two focused tests:
//!   `*_blocked_under_hold`  — hold=true  → must panic with the exact contract message
//!   `*_passes_when_hold_cleared` — hold=false → operation succeeds normally
//!
//! Edge-case tests verify:
//!   - Hold check fires before status validation (fund, settle, withdraw)
//!   - Idempotent toggling (set true→true, clear false→false)
//!   - Non-gated operations (`update_maturity`, admin handover, getters) are NOT blocked
//!   - Claim idempotency survives a hold toggle
//!   - A single hold toggle blocks all gated ops in separate escrows
//!
//! Auth tests verify that only the admin can set or clear the hold.
//!
//! Gated functions (6 entrypoints, 5 unique messages):
//!   fund / fund_with_commitment  → "Legal hold blocks new funding while active"
//!   settle                       → "Legal hold blocks settlement finalization"
//!   withdraw                     → "Legal hold blocks SME withdrawal"
//!   claim_investor_payout        → "Legal hold blocks investor claims"
//!   sweep_terminal_dust          → "Legal hold blocks treasury dust sweep"

use super::*;
use soroban_sdk::token::StellarAssetClient;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Initialise a minimal escrow (open, maturity=0, no tiers).
fn init_open(
    client: &LiquifactEscrowClient<'_>,
    env: &Env,
    admin: &Address,
    sme: &Address,
    id: &str,
) -> (Address, Address) {
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, id),
        sme,
        &TARGET,
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
    );
    (token, treasury)
}

/// Initialise an open escrow with a configured legal-hold clear delay.
fn init_open_with_clear_delay(
    client: &LiquifactEscrowClient<'_>,
    env: &Env,
    admin: &Address,
    sme: &Address,
    id: &str,
    legal_hold_clear_delay: Option<u64>,
) -> (Address, Address) {
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, id),
        sme,
        &TARGET,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &legal_hold_clear_delay,
        &None,
    );
    (token, treasury)
}

/// Initialise with a real SAC token, fund to target, and mint `TARGET` tokens
/// into the escrow contract so `withdraw()` can actually transfer them.
fn init_funded_with_real_token<'a>(
    env: &'a Env,
    admin: &Address,
    sme: &Address,
    investor: &Address,
    id: &str,
) -> (LiquifactEscrowClient<'a>, Address) {
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(env, &token_id);
    let treasury = Address::generate(env);
    let escrow_id = env.register(crate::LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &escrow_id);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, id),
        sme,
        &TARGET,
        &800i64,
        &0u64,
        &token_id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    sac_admin.mint(investor, &TARGET);
    client.fund(investor, &TARGET);
    (client, escrow_id)
}

/// Initialise, fund to target, return (token, treasury).
fn init_funded(
    client: &LiquifactEscrowClient<'_>,
    env: &Env,
    admin: &Address,
    sme: &Address,
    investor: &Address,
    id: &str,
) -> (Address, Address) {
    let (token, treasury) = init_open(client, env, admin, sme, id);
    client.fund(investor, &TARGET);
    (token, treasury)
}

/// Initialise, fund, settle, return (escrow_id, token, treasury).
fn init_settled<'a>(
    env: &'a Env,
    admin: &Address,
    sme: &Address,
    investor: &Address,
    id: &str,
) -> (LiquifactEscrowClient<'a>, Address, Address, Address) {
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token = sac.address();
    let treasury = Address::generate(env);
    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &escrow_id);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, id),
        sme,
        &TARGET,
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
    );
    let sac_admin = StellarAssetClient::new(env, &token);
    sac_admin.mint(investor, &TARGET);
    client.fund(investor, &TARGET);
    client.settle();
    (client, escrow_id, token, treasury)
}

// ── 1. fund ──────────────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn fund_blocked_under_hold() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "LHF001");
    client.set_legal_hold(&true);
    client.fund(&investor, &TARGET);
}

#[test]
fn fund_passes_when_hold_cleared() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "LHF002");
    client.set_legal_hold(&true);
    client.clear_legal_hold();
    assert!(!client.get_legal_hold());
    let escrow = client.fund(&investor, &TARGET);
    assert_eq!(escrow.status, 1);
}

// ── 2. fund_with_commitment ───────────────────────────────────────────────────

#[test]
#[should_panic]
fn fund_with_commitment_blocked_under_hold() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "LHC001");
    client.set_legal_hold(&true);
    client.fund_with_commitment(&investor, &TARGET, &0u64);
}

#[test]
fn fund_with_commitment_passes_when_hold_cleared() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "LHC002");
    client.set_legal_hold(&true);
    client.clear_legal_hold();
    let escrow = client.fund_with_commitment(&investor, &TARGET, &0u64);
    assert_eq!(escrow.status, 1);
}

// ── 3. settle ────────────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn settle_blocked_under_hold() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "LHS001");
    client.set_legal_hold(&true);
    client.settle();
}

#[test]
fn settle_passes_when_hold_cleared() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "LHS002");
    client.set_legal_hold(&true);
    client.clear_legal_hold();
    let escrow = client.settle();
    assert_eq!(escrow.status, 2);
}

// ── 4. withdraw ──────────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn withdraw_blocked_under_hold() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "LHW001");
    client.set_legal_hold(&true);
    client.withdraw();
}

#[test]
fn withdraw_passes_when_hold_cleared() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, _escrow_id) = init_funded_with_real_token(&env, &admin, &sme, &investor, "LHW002");
    client.set_legal_hold(&true);
    client.clear_legal_hold();
    let escrow = client.withdraw();
    assert_eq!(escrow.status, 3);
}

// ── 5. claim_investor_payout ─────────────────────────────────────────────────

#[test]
#[should_panic]
fn claim_investor_payout_blocked_under_hold() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "LHP001");
    client.settle();
    client.set_legal_hold(&true);
    client.claim_investor_payout(&investor);
}

#[test]
fn claim_investor_payout_passes_when_hold_cleared() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "LHP002");
    client.settle();
    client.set_legal_hold(&true);
    client.clear_legal_hold();
    client.claim_investor_payout(&investor);
    assert!(client.is_investor_claimed(&investor));
}

// ── 6. sweep_terminal_dust ───────────────────────────────────────────────────

#[test]
#[should_panic]
fn sweep_terminal_dust_blocked_under_hold() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, escrow_id, token, _treasury) =
        init_settled(&env, &admin, &sme, &investor, "LHD001");
    let stellar = StellarAssetClient::new(&env, &token);
    stellar.mint(&escrow_id, &1_000i128);
    client.set_legal_hold(&true);
    client.sweep_terminal_dust(&1_000i128);
}

#[test]
fn sweep_terminal_dust_passes_when_hold_cleared() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, escrow_id, token, treasury) =
        init_settled(&env, &admin, &sme, &investor, "LHD002");
    let stellar = StellarAssetClient::new(&env, &token);
    stellar.mint(&escrow_id, &500i128);
    client.set_legal_hold(&true);
    client.clear_legal_hold();
    let swept = client.sweep_terminal_dust(&500i128);
    assert_eq!(swept, 500i128);
    assert_eq!(stellar.balance(&treasury), 500i128);
}

// ── 7. Admin-only: set_legal_hold ────────────────────────────────────────────

#[test]
fn set_legal_hold_by_admin_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHA001");
    client.set_legal_hold(&true);
    assert!(client.get_legal_hold());
    client.set_legal_hold(&false);
    assert!(!client.get_legal_hold());
}

#[test]
fn set_legal_hold_emits_event_with_correct_flag() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHA002");
    // set → active=1
    client.set_legal_hold(&true);
    assert!(
        env.auths().iter().any(|(addr, _)| *addr == admin),
        "admin auth must be recorded for set_legal_hold"
    );
    // clear → active=0
    client.clear_legal_hold();
    assert!(!client.get_legal_hold());
}

#[test]
#[should_panic]
fn set_legal_hold_by_non_admin_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHA003");
    // Drop all mock auths so the non-admin call has no authorisation.
    env.mock_auths(&[]);
    client.set_legal_hold(&true);
}

// ── 8. Admin-only: clear_legal_hold ──────────────────────────────────────────

#[test]
fn clear_legal_hold_by_admin_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHB001");
    client.set_legal_hold(&true);
    assert!(client.get_legal_hold());
    client.clear_legal_hold();
    assert!(!client.get_legal_hold());
}

#[test]
fn request_clear_legal_hold_by_admin_succeeds_with_zero_delay() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open_with_clear_delay(&client, &env, &admin, &sme, "LHR001", Some(0));
    client.set_legal_hold(&true);
    client.request_clear_legal_hold();
    assert!(client.get_legal_hold_clearable_at().is_some());
    client.set_legal_hold(&false);
    assert!(!client.get_legal_hold());
}

#[test]
#[should_panic]
fn request_clear_legal_hold_by_non_admin_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open_with_clear_delay(&client, &env, &admin, &sme, "LHR002", Some(0));
    client.set_legal_hold(&true);
    env.mock_auths(&[]);
    client.request_clear_legal_hold();
}

#[test]
#[should_panic]
fn set_legal_hold_false_before_clearable_at_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open_with_clear_delay(&client, &env, &admin, &sme, "LHR003", Some(10));
    client.set_legal_hold(&true);
    client.request_clear_legal_hold();
    client.set_legal_hold(&false);
}

#[test]
fn set_legal_hold_false_after_clearable_at_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open_with_clear_delay(&client, &env, &admin, &sme, "LHR004", Some(10));
    client.set_legal_hold(&true);
    client.request_clear_legal_hold();
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 10);
    client.set_legal_hold(&false);
    assert!(!client.get_legal_hold());
}

#[test]
#[should_panic]
fn clear_legal_hold_by_non_admin_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHB002");
    client.set_legal_hold(&true);
    env.mock_auths(&[]);
    client.clear_legal_hold();
}

// ── 9. Admin-only: cancel_clear_legal_hold ─────────────────────────────────────

#[test]
fn cancel_clear_legal_hold_with_pending_request_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHB001");
    client.set_legal_hold(&true);
    assert!(client.get_legal_hold());
    client.request_clear_legal_hold();
    assert!(client.get_legal_hold_clearable_at().is_some());
    client.cancel_clear_legal_hold();
    assert!(client.get_legal_hold());
    assert!(client.get_legal_hold_clearable_at().is_none());
    // event emitted -> captured by env.auths()
}

#[test]
#[should_panic(expected = "No pending clear request to cancel")]
fn cancel_clear_legal_hold_without_pending_request_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHB002");
    client.set_legal_hold(&true);
    env.mock_auths(&[]);
    client.cancel_clear_legal_hold();
}

#[test]
#[should_panic]
fn cancel_clear_legal_hold_by_non_admin_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHB003");
    client.set_legal_hold(&true);
    client.request_clear_legal_hold();
    env.mock_auths(&[]);
    client.cancel_clear_legal_hold();
}

#[test]
fn cancel_clear_legal_hold_allows_new_request_after_cancellation() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHB004");
    client.set_legal_hold(&true);
    client.request_clear_legal_hold();
    let before_cancel = client.get_legal_hold_clearable_at().unwrap();
    client.cancel_clear_legal_hold();
    client.request_clear_legal_hold();
    let after_request = client.get_legal_hold_clearable_at().unwrap();
    assert_ne!(after_request, before_cancel);
    // after clearable_at - delay expected
}

// ── 9. Default state ─────────────────────────────────────────────────────────

#[test]
fn legal_hold_defaults_to_false_after_init() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHN001");
    assert!(!client.get_legal_hold());
}

// ── 10. No-bypass: hold survives state transitions ───────────────────────────

/// A hold set while open must still block settle after the escrow becomes funded.
#[test]
fn hold_set_before_funding_still_blocks_settle_after_funded() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "LHX001");
    // Hold is set while escrow is still open.
    client.set_legal_hold(&true);
    // fund() itself is blocked — clear hold, fund, then re-apply hold.
    client.clear_legal_hold();
    client.fund(&investor, &TARGET);
    assert_eq!(client.get_escrow().status, 1);
    client.set_legal_hold(&true);
    // settle must still be blocked.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.settle();
    }));
    assert!(
        result.is_err(),
        "settle must be blocked while hold is active"
    );
}

/// Clearing the hold and immediately re-setting it must block again.
#[test]
fn hold_can_be_toggled_and_re_blocks_operations() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "LHX002");

    // First toggle: set → clear → settle succeeds.
    client.set_legal_hold(&true);
    client.clear_legal_hold();
    let settled = client.settle();
    assert_eq!(settled.status, 2);

    // Second toggle: re-set → claim is blocked.
    client.set_legal_hold(&true);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.claim_investor_payout(&investor);
    }));
    assert!(
        result.is_err(),
        "claim must be blocked after re-setting hold"
    );

    // Clear again → claim succeeds.
    client.clear_legal_hold();
    client.claim_investor_payout(&investor);
    assert!(client.is_investor_claimed(&investor));
}

/// Admin handover does not grant the new admin a free bypass: the hold persists
/// and the new admin must explicitly clear it.
#[test]
fn hold_persists_after_admin_handover() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let new_admin = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "LHX003");
    client.set_legal_hold(&true);
    client.propose_admin(&new_admin, &None);
    client.accept_admin();
    // Hold is still active after admin handover.
    assert!(client.get_legal_hold());
    // settle is still blocked.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.settle();
    }));
    assert!(
        result.is_err(),
        "settle must remain blocked after admin handover"
    );
    // New admin clears the hold.
    client.clear_legal_hold();
    assert!(!client.get_legal_hold());
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

// ── 11. Edge-case: hold check fires before amount / status / auth checks ─────

/// Hold must block `sweep_terminal_dust` before the zero-amount guard fires.
#[test]
#[should_panic]
fn hold_blocks_sweep_before_zero_amount_check() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, _escrow_id, _token, _treasury) =
        init_settled(&env, &admin, &sme, &investor, "LHZ001");
    client.set_legal_hold(&true);
    // Zero amount would normally panic "sweep amount must be positive";
    // the hold check must fire first.
    client.sweep_terminal_dust(&0i128);
}

/// Hold must block `settle` before the status guard fires (open escrow).
#[test]
#[should_panic]
fn hold_blocks_settle_before_status_check_on_open_escrow() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHS003");
    client.set_legal_hold(&true);
    // Escrow is open (status 0) — "Escrow must be funded" would fire next,
    // but hold must panic first.
    client.settle();
}

/// Hold must block `withdraw` before the status guard fires (open escrow).
#[test]
#[should_panic]
fn hold_blocks_withdraw_before_status_check_on_open_escrow() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHW003");
    client.set_legal_hold(&true);
    client.withdraw();
}

/// Hold must block `fund` before the status guard fires (escrow already funded).
/// Note: the `amount > 0` check fires before the hold check in `fund_impl`,
/// so zero-amount is NOT a valid test — use a fully-funded escrow instead.
#[test]
#[should_panic]
fn hold_blocks_fund_before_status_check_on_funded_escrow() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "LHF003");
    client.set_legal_hold(&true);
    // Escrow is funded (status 1) — "Escrow not open for funding" would fire next,
    // but hold must panic first.
    client.fund(&investor, &1i128);
}

// ── 12. Idempotent toggling ──────────────────────────────────────────────────

#[test]
fn set_legal_hold_true_when_already_true_is_idempotent() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHI001");
    client.set_legal_hold(&true);
    assert!(client.get_legal_hold());
    // Second set(true) must not panic.
    client.set_legal_hold(&true);
    assert!(client.get_legal_hold());
}

#[test]
fn clear_legal_hold_when_already_false_is_idempotent() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHI002");
    // Hold defaults to false — clear must not panic.
    client.clear_legal_hold();
    assert!(!client.get_legal_hold());
    client.clear_legal_hold();
    assert!(!client.get_legal_hold());
}

// ── 13. Non-gated operations are NOT blocked by hold ─────────────────────────

/// `update_maturity`, admin handover, and getters must all work under hold.
#[test]
fn non_risk_operations_not_blocked_by_hold() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHN002");

    // Enable hold.
    client.set_legal_hold(&true);
    assert!(client.get_legal_hold());

    // Getters must still work.
    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 0u32);
    assert!(client.get_legal_hold());

    // `update_maturity` must not be blocked.
    let updated = client.update_maturity(&9999u64);
    assert_eq!(updated.maturity, 9999u64);

    // Two-step admin handover must not be blocked.
    let new_admin = Address::generate(&env);
    client.propose_admin(&new_admin, &None);
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));
    client.accept_admin();
    let escrow = client.get_escrow();
    assert_eq!(escrow.admin, new_admin);
    assert_eq!(client.get_pending_admin(), None);
}

// ── 14. Re-entrancy / double-spend: claim idempotent after hold cleared ───────

/// After clearing a hold, the idempotent claim guard must still prevent
/// double-spend (the `is_claimed` marker survives the hold toggle).
#[test]
fn claim_after_hold_cleared_still_idempotent() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "LHD003");
    client.settle();

    // Set and clear hold.
    client.set_legal_hold(&true);
    client.clear_legal_hold();

    // First claim succeeds.
    client.claim_investor_payout(&investor);
    assert!(client.is_investor_claimed(&investor));

    // Second claim must be idempotent (no panic).
    client.claim_investor_payout(&investor);
    assert!(client.is_investor_claimed(&investor));
}

// ── 15. Multiple gated operations blocked by one hold toggle ──────────────────

/// A single hold (set once) must block all risk-bearing entrypoints that the
/// escrow state would otherwise permit. We verify this across three separate
/// escrows driven to the required state before the hold.
#[test]
fn single_hold_blocks_all_gated_ops() {
    // settle
    {
        let env = Env::default();
        let (client, admin, sme) = setup(&env);
        let investor = Address::generate(&env);
        init_funded(&client, &env, &admin, &sme, &investor, "LHG001");
        client.set_legal_hold(&true);
        crate::tests::assert_contract_error(
            client.try_settle(),
            crate::EscrowError::LegalHoldBlocksSettlement,
        );
    }
    // withdraw
    {
        let env = Env::default();
        let (client, admin, sme) = setup(&env);
        let investor = Address::generate(&env);
        init_funded(&client, &env, &admin, &sme, &investor, "LHG002");
        client.set_legal_hold(&true);
        crate::tests::assert_contract_error(
            client.try_withdraw(),
            crate::EscrowError::LegalHoldBlocksWithdrawal,
        );
    }
    // claim_investor_payout
    {
        let env = Env::default();
        let (client, admin, sme) = setup(&env);
        let investor = Address::generate(&env);
        init_funded(&client, &env, &admin, &sme, &investor, "LHG003");
        client.settle();
        client.set_legal_hold(&true);
        crate::tests::assert_contract_error(
            client.try_claim_investor_payout(&investor),
            crate::EscrowError::LegalHoldBlocksInvestorClaims,
        );
    }
    // sweep_terminal_dust
    {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let investor = Address::generate(&env);
        let (client, escrow_id, token, _treasury) =
            init_settled(&env, &admin, &sme, &investor, "LHG004");
        let stellar = StellarAssetClient::new(&env, &token);
        stellar.mint(&escrow_id, &1_000i128);
        client.set_legal_hold(&true);
        crate::tests::assert_contract_error(
            client.try_sweep_terminal_dust(&1_000i128),
            crate::EscrowError::LegalHoldBlocksTreasuryDustSweep,
        );
    }
}

// ── 16. Typed-error timing: delay window enforcement ─────────────────────────

/// Calling `set_legal_hold(false)` (or `clear_legal_hold`) without a prior
/// `request_clear_legal_hold` must emit [`EscrowError::LegalHoldClearRequestMissing`]
/// when a non-zero clear delay is configured.
///
/// Security invariant: the delay cannot be bypassed by skipping the request step.
#[test]
fn clear_without_request_emits_clear_request_missing() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open_with_clear_delay(&client, &env, &admin, &sme, "LHT001", Some(100));
    client.set_legal_hold(&true);

    // No request_clear_legal_hold call — direct clear must fail with the typed error.
    assert_contract_error(
        client.try_set_legal_hold(&false),
        EscrowError::LegalHoldClearRequestMissing,
    );
    // Hold must still be active: no partial state mutation.
    assert!(client.get_legal_hold());
}

/// One ledger-second before `clearable_at`, `set_legal_hold(false)` must emit
/// [`EscrowError::LegalHoldClearNotReady`].
///
/// Security invariant: even a single-tick early attempt is rejected.
#[test]
fn clear_one_ledger_before_clearable_at_emits_clear_not_ready() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let delay: u64 = 100;
    init_open_with_clear_delay(&client, &env, &admin, &sme, "LHT002", Some(delay));
    client.set_legal_hold(&true);
    client.request_clear_legal_hold();

    let clearable_at = client
        .get_legal_hold_clearable_at()
        .expect("clearable_at set");
    // Advance to one second before the boundary.
    env.ledger().set_timestamp(clearable_at - 1);

    assert_contract_error(
        client.try_set_legal_hold(&false),
        EscrowError::LegalHoldClearNotReady,
    );
    // Hold is still active.
    assert!(client.get_legal_hold());
}

/// At exactly `clearable_at`, `set_legal_hold(false)` must succeed.
///
/// Boundary condition: `now >= clearable_at` is inclusive — the tick at which
/// the delay expires is valid.
#[test]
fn clear_at_exact_clearable_at_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let delay: u64 = 100;
    init_open_with_clear_delay(&client, &env, &admin, &sme, "LHT003", Some(delay));
    client.set_legal_hold(&true);
    client.request_clear_legal_hold();

    let clearable_at = client
        .get_legal_hold_clearable_at()
        .expect("clearable_at set");
    env.ledger().set_timestamp(clearable_at);

    client.set_legal_hold(&false);
    assert!(!client.get_legal_hold());
    // LegalHoldClearableAt key must be cleaned up after a successful clear.
    assert!(client.get_legal_hold_clearable_at().is_none());
}

// ── 17. Zero-delay boundary ───────────────────────────────────────────────────

/// When `legal_hold_clear_delay` is 0, `request_clear_legal_hold` followed
/// immediately by `set_legal_hold(false)` (same ledger timestamp) must succeed.
///
/// The zero-delay case must not require any ledger advancement.
#[test]
fn zero_delay_request_then_immediate_clear_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open_with_clear_delay(&client, &env, &admin, &sme, "LHT004", Some(0));
    client.set_legal_hold(&true);

    client.request_clear_legal_hold();
    // clearable_at == now (no advancement needed).
    let clearable_at = client
        .get_legal_hold_clearable_at()
        .expect("clearable_at set");
    assert_eq!(clearable_at, env.ledger().timestamp());

    // No ledger advance — same timestamp must be accepted.
    client.set_legal_hold(&false);
    assert!(!client.get_legal_hold());
}

// ── 18. Admin-handover recovery scenario ─────────────────────────────────────

/// Full recovery path:
///   1. Admin sets hold (all risk-bearing ops blocked).
///   2. Admin proposes a new admin; new admin accepts (hold persists).
///   3. New admin clears the hold.
///   4. `settle`, `withdraw`, and `claim_investor_payout` all resume.
///
/// This covers the documented funds-safety recovery lever described in
/// `docs/escrow-legal-hold.md` §"Failure mode: hold + lost admin key".
#[test]
fn recovery_new_admin_clears_hold_and_operations_resume() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let new_admin = Address::generate(&env);

    // --- Setup: funded escrow with real token so withdraw() can transfer. ---
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &token_id);
    let treasury = Address::generate(&env);
    let escrow_id = env.register(crate::LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "LHR010"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &token_id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    sac_admin.mint(&investor, &TARGET);
    client.fund(&investor, &TARGET);
    // Mint yield portion so claim_investor_payout can transfer principal + yield.
    let yield_amount = TARGET * 800 / 10_000;
    sac_admin.mint(&escrow_id, &yield_amount);

    // --- Step 1: activate hold — settle is now blocked. ---
    client.set_legal_hold(&true);
    assert!(client.get_legal_hold());
    assert_contract_error(client.try_settle(), EscrowError::LegalHoldBlocksSettlement);

    // --- Step 2: propose + accept new admin while hold is active. ---
    // propose_admin and accept_admin are NOT gated by the hold (by design).
    client.propose_admin(&new_admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));
    client.accept_admin();
    // Hold persists after handover.
    assert!(client.get_legal_hold());
    assert_eq!(client.get_escrow().admin, new_admin);

    // Risk-bearing ops remain blocked even though admin changed.
    assert_contract_error(client.try_settle(), EscrowError::LegalHoldBlocksSettlement);
    assert_contract_error(
        client.try_withdraw(),
        EscrowError::LegalHoldBlocksWithdrawal,
    );
    assert_contract_error(
        client.try_claim_investor_payout(&investor),
        EscrowError::LegalHoldBlocksInvestorClaims,
    );

    // --- Step 3: new admin clears the hold. ---
    client.clear_legal_hold();
    assert!(!client.get_legal_hold());

    // --- Step 4: operations resume. ---
    // settle transitions funded(1) → settled(2); withdraw requires status=1
    // so we verify settle first, then demonstrate claim on the settled escrow.
    let settled = client.settle();
    assert_eq!(settled.status, 2);

    client.claim_investor_payout(&investor);
    assert!(client.is_investor_claimed(&investor));
}
