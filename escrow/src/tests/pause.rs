use super::*;
use crate::{EscrowError, PausedChanged};
use soroban_sdk::{testutils::Events, token::StellarAssetClient, Event};

// ── Helpers ──────────────────────────────────────────────────────────────────

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
        &None,
        &None,
        &None::<i64>,
    );
    (token, treasury)
}

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
    let escrow_id = env.register(LiquifactEscrow, ());
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
        &None,
        &None,
        &None::<i64>,
    );
    sac_admin.mint(investor, &TARGET);
    client.fund(investor, &TARGET);
    (client, escrow_id)
}

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
        &None,
        &None,
        &None::<i64>,
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
fn fund_blocked_when_paused() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "PAU001");
    client.set_paused(&true);
    client.fund(&investor, &TARGET);
}

#[test]
fn fund_succeeds_after_unpause() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "PAU002");
    client.set_paused(&true);
    assert!(client.is_paused());
    client.set_paused(&false);
    assert!(!client.is_paused());
    let escrow = client.fund(&investor, &TARGET);
    assert_eq!(escrow.status, 1);
}

// ── 2. fund_with_commitment ─────────────────────────────────────────────────

#[test]
#[should_panic]
fn fund_with_commitment_blocked_when_paused() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "PAU003");
    client.set_paused(&true);
    client.fund_with_commitment(&investor, &TARGET, &0u64);
}

#[test]
fn fund_with_commitment_succeeds_after_unpause() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "PAU004");
    client.set_paused(&true);
    client.set_paused(&false);
    let escrow = client.fund_with_commitment(&investor, &TARGET, &0u64);
    assert_eq!(escrow.status, 1);
}

// ── 3. fund_batch ───────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn fund_batch_blocked_when_paused() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "PAU005");
    client.set_paused(&true);
    let entries = SorobanVec::from_array(&env, [(investor.clone(), TARGET)]);
    client.fund_batch(&entries);
}

#[test]
fn fund_batch_succeeds_after_unpause() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "PAU006");
    client.set_paused(&true);
    client.set_paused(&false);
    let entries = SorobanVec::from_array(&env, [(investor.clone(), TARGET)]);
    let escrow = client.fund_batch(&entries);
    assert_eq!(escrow.status, 1);
}

// ── 4. settle ────────────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn settle_blocked_when_paused() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "PAU007");
    client.set_paused(&true);
    client.settle();
}

#[test]
fn settle_succeeds_after_unpause() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "PAU008");
    client.set_paused(&true);
    client.set_paused(&false);
    let escrow = client.settle();
    assert_eq!(escrow.status, 2);
}

// ── 5. withdraw ──────────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn withdraw_blocked_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, _escrow_id) = init_funded_with_real_token(&env, &admin, &sme, &investor, "PAU009");
    client.set_paused(&true);
    client.withdraw();
}

#[test]
fn withdraw_succeeds_after_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, _escrow_id) = init_funded_with_real_token(&env, &admin, &sme, &investor, "PAU010");
    client.set_paused(&true);
    client.set_paused(&false);
    let escrow = client.withdraw();
    assert_eq!(escrow.status, 3);
}

// ── 6. claim_investor_payout ─────────────────────────────────────────────────

#[test]
#[should_panic]
fn claim_investor_payout_blocked_when_paused() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "PAU011");
    client.settle();
    client.set_paused(&true);
    client.claim_investor_payout(&investor);
}

#[test]
fn claim_investor_payout_succeeds_after_unpause() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "PAU012");
    client.settle();
    client.set_paused(&true);
    client.set_paused(&false);
    client.claim_investor_payout(&investor);
    assert!(client.is_investor_claimed(&investor));
}

// ── 7. Read-only views unaffected by pause ───────────────────────────────────

#[test]
fn read_views_unaffected_by_pause() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "PAU013");

    client.set_paused(&true);
    assert!(client.is_paused());

    // get_escrow
    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 1);

    // get_escrow_summary
    let summary = client.get_escrow_summary();
    assert_eq!(summary.escrow.status, 1);

    // get_remaining_funding_capacity
    let cap = client.get_remaining_funding_capacity();
    assert_eq!(cap, 0);

    // get_funding_token
    let _token = client.get_funding_token();

    // get_treasury
    let _treasury = client.get_treasury();

    // get_version
    assert_eq!(client.get_version(), SCHEMA_VERSION);

    // get_contribution
    let contribution = client.get_contribution(&investor);
    assert_eq!(contribution, TARGET);

    // get_legal_hold — orthogonal
    assert!(!client.get_legal_hold());

    // get_min_contribution_floor
    let _floor = client.get_min_contribution_floor();

    // get_unique_funder_count
    assert_eq!(client.get_unique_funder_count(), 1);

    // is_allowlist_active
    let _ = client.is_allowlist_active();
}

#[test]
fn read_views_unaffected_by_pause_on_open_escrow() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "PAU014");

    client.set_paused(&true);
    assert!(client.is_paused());

    // get_escrow on open escrow
    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 0);

    // get_escrow_summary
    let summary = client.get_escrow_summary();
    assert_eq!(summary.escrow.status, 0);

    // get_remaining_funding_capacity should equal TARGET
    let cap = client.get_remaining_funding_capacity();
    assert_eq!(cap, TARGET);
}

// ── 8. Admin gating ──────────────────────────────────────────────────────────

#[test]
fn set_paused_by_admin_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "PAU015");
    client.set_paused(&true);
    assert!(client.is_paused());
    client.set_paused(&false);
    assert!(!client.is_paused());
}

#[test]
#[should_panic]
fn set_paused_by_non_admin_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "PAU016");
    env.mock_auths(&[]);
    client.set_paused(&true);
}

// ── 9. Redundant no-op calls ─────────────────────────────────────────────────

#[test]
fn set_paused_true_when_already_true_is_noop() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "PAU017");
    client.set_paused(&true);
    assert!(client.is_paused());
    // Second call should succeed (no-op)
    client.set_paused(&true);
    assert!(client.is_paused());
}

#[test]
fn set_paused_false_when_already_false_is_noop() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "PAU018");
    assert!(!client.is_paused());
    // Default is false, calling set_paused(false) should succeed
    client.set_paused(&false);
    assert!(!client.is_paused());
}

// ── 10. Event emission ───────────────────────────────────────────────────────

#[test]
fn set_paused_emits_event() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let contract_id = client.address.clone();
    init_open(&client, &env, &admin, &sme, "PAU019");

    client.set_paused(&true);
    let events = env.events().all();
    let last = events.events().last().unwrap().clone();
    assert_eq!(
        last,
        PausedChanged {
            name: symbol_short!("paused"),
            invoice_id: client.get_escrow().invoice_id,
            active: 1,
        }
        .to_xdr(&env, &contract_id)
    );

    client.set_paused(&false);
    let events = env.events().all();
    let last = events.events().last().unwrap().clone();
    assert_eq!(
        last,
        PausedChanged {
            name: symbol_short!("paused"),
            invoice_id: client.get_escrow().invoice_id,
            active: 0,
        }
        .to_xdr(&env, &contract_id)
    );
}

// ── 11. Orthogonal to legal hold ─────────────────────────────────────────────

#[test]
fn pause_orthogonal_to_legal_hold() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "PAU020");

    // Pause doesn't affect legal hold
    client.set_paused(&true);
    assert!(!client.get_legal_hold());

    // Legal hold doesn't affect pause
    client.set_legal_hold(&true);
    assert!(client.is_paused());
    assert!(client.get_legal_hold());

    // Clearing pause leaves legal hold intact
    client.set_paused(&false);
    assert!(!client.is_paused());
    assert!(client.get_legal_hold());

    // Clearing legal hold leaves pause intact
    client.clear_legal_hold();
    assert!(!client.is_paused());
    assert!(!client.get_legal_hold());
}

// ── 12. Pause gate fires before status validation ─────────────────────────────

#[test]
#[should_panic]
fn pause_gate_fires_before_status_validation_fund() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "PAU021");
    // Escrow is funded (status=1), but paused should block before "not open for funding"
    client.set_paused(&true);
    client.fund(&investor, &TARGET);
}

#[test]
#[should_panic]
fn pause_gate_fires_before_legal_hold_fund() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "PAU022");
    client.set_paused(&true);
    client.set_legal_hold(&true);
    // Should panic with PausedBlocksFunding, not LegalHoldBlocksFunding
    client.fund(&investor, &TARGET);
}

#[test]
#[should_panic]
fn pause_gate_fires_before_legal_hold_settle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "PAU023");
    client.set_paused(&true);
    client.set_legal_hold(&true);
    // Should panic with PausedBlocksSettlement, not LegalHoldBlocksSettlement
    client.settle();
}

#[test]
#[should_panic]
fn pause_gate_fires_before_legal_hold_withdraw() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, _escrow_id) = init_funded_with_real_token(&env, &admin, &sme, &investor, "PAU024");
    client.set_paused(&true);
    client.set_legal_hold(&true);
    // Should panic with PausedBlocksWithdrawal, not LegalHoldBlocksWithdrawal
    client.withdraw();
}

#[test]
#[should_panic]
fn pause_gate_fires_before_legal_hold_claim() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "PAU025");
    client.settle();
    client.set_paused(&true);
    client.set_legal_hold(&true);
    // Should panic with PausedBlocksInvestorClaims, not LegalHoldBlocksInvestorClaims
    client.claim_investor_payout(&investor);
}

// ── 13. Typed error codes ─────────────────────────────────────────────────────

#[test]
fn fund_returns_typed_error_when_paused() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "PAU026");
    client.set_paused(&true);
    assert_contract_error(
        client.try_fund(&investor, &TARGET),
        EscrowError::PausedBlocksFunding,
    );
}

#[test]
fn settle_returns_typed_error_when_paused() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "PAU027");
    client.set_paused(&true);
    assert_contract_error(client.try_settle(), EscrowError::PausedBlocksSettlement);
}

#[test]
fn withdraw_returns_typed_error_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, _escrow_id) = init_funded_with_real_token(&env, &admin, &sme, &investor, "PAU028");
    client.set_paused(&true);
    assert_contract_error(client.try_withdraw(), EscrowError::PausedBlocksWithdrawal);
}

#[test]
fn claim_returns_typed_error_when_paused() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_funded(&client, &env, &admin, &sme, &investor, "PAU029");
    client.settle();
    client.set_paused(&true);
    assert_contract_error(
        client.try_claim_investor_payout(&investor),
        EscrowError::PausedBlocksInvestorClaims,
    );
}

#[test]
fn fund_with_commitment_returns_typed_error_when_paused() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "PAU031");
    client.set_paused(&true);
    assert_contract_error(
        client.try_fund_with_commitment(&investor, &TARGET, &0u64),
        EscrowError::PausedBlocksFunding,
    );
}

#[test]
fn fund_batch_returns_typed_error_when_paused() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "PAU032");
    client.set_paused(&true);
    let entries = SorobanVec::from_array(&env, [(investor.clone(), TARGET)]);
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::PausedBlocksFunding,
    );
}

#[test]
fn set_paused_by_non_admin_returns_typed_error() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_open(&client, &env, &admin, &sme, "PAU033");
    env.mock_auths(&[]);
    assert_contract_error(
        client.try_set_paused(&true),
        EscrowError::Unauthorized,
    );
}

// ── 14. Multiple pauses toggle correctly ──────────────────────────────────────

#[test]
fn pause_toggle_cycle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    init_open(&client, &env, &admin, &sme, "PAU030");

    assert!(!client.is_paused());
    client.set_paused(&true);
    assert!(client.is_paused());
    client.set_paused(&false);
    assert!(!client.is_paused());
    client.set_paused(&true);
    assert!(client.is_paused());
    client.set_paused(&false);
    assert!(!client.is_paused());

    // Funding should succeed after cycle
    let escrow = client.fund(&investor, &TARGET);
    assert_eq!(escrow.status, 1);
}
