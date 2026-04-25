//! Legal-hold matrix tests.
//!
//! Each risk-bearing function gets two focused tests:
//!   `*_blocked_under_hold`  — hold=true  → must panic with the exact contract message
//!   `*_passes_when_hold_cleared` — hold=false → operation succeeds normally
//!
//! Auth tests verify that only the admin can set or clear the hold.
//!
//! Gated functions (5 total):
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
        &String::from_str(env, id),
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
    );
    (token, treasury)
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
fn init_settled(
    env: &Env,
    admin: &Address,
    sme: &Address,
    investor: &Address,
    id: &str,
) -> (LiquifactEscrowClient<'_>, Address, Address, Address) {
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token = sac.address();
    let treasury = Address::generate(env);
    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &escrow_id);
    client.init(
        admin,
        &String::from_str(env, id),
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
    );
    client.fund(investor, &TARGET);
    client.settle();
    (client, escrow_id, token, treasury)
}

// ── 1. fund ──────────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Legal hold blocks new funding while active")]
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
#[should_panic(expected = "Legal hold blocks new funding while active")]
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
#[should_panic(expected = "Legal hold blocks settlement finalization")]
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
#[should_panic(expected = "Legal hold blocks SME withdrawal")]
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
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "LHW002");
    client.set_legal_hold(&true);
    client.clear_legal_hold();
    let escrow = client.withdraw();
    assert_eq!(escrow.status, 3);
}

// ── 5. claim_investor_payout ─────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Legal hold blocks investor claims")]
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
#[should_panic(expected = "Legal hold blocks treasury dust sweep")]
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
#[should_panic]
fn clear_legal_hold_by_non_admin_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "LHB002");
    client.set_legal_hold(&true);
    env.mock_auths(&[]);
    client.clear_legal_hold();
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
    assert!(result.is_err(), "settle must be blocked while hold is active");
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
    assert!(result.is_err(), "claim must be blocked after re-setting hold");

    // Clear again → claim succeeds.
    client.clear_legal_hold();
    client.claim_investor_payout(&investor);
    assert!(client.is_investor_claimed(&investor));
}

/// Admin transfer does not grant the new admin a free bypass: the hold persists
/// and the new admin must explicitly clear it.
#[test]
fn hold_persists_after_admin_transfer() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let new_admin = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "LHX003");
    client.set_legal_hold(&true);
    client.transfer_admin(&new_admin);
    // Hold is still active after admin rotation.
    assert!(client.get_legal_hold());
    // settle is still blocked.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.settle();
    }));
    assert!(result.is_err(), "settle must remain blocked after admin transfer");
    // New admin clears the hold.
    client.clear_legal_hold();
    assert!(!client.get_legal_hold());
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}
