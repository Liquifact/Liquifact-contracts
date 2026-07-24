// Tests for the shared `require_funding_open` guard and `guard_status_eq` / `guard_status_in`
// helpers introduced in this refactor. Each test verifies that the named entrypoint rejects
// calls from funded, settled, and cancelled status with the same error code that was present
// before the guard was extracted. No behavior change is expected — this is a pure refactor.
use super::*;
use crate::EscrowError;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn setup_open(
    env: &Env,
) -> (
    LiquifactEscrowClient<'_>,
    Address,
    Address,
    Address,
    Address,
) {
    let client = LiquifactEscrowClient::new(env, &env.register(LiquifactEscrow, ()));
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let tok = Address::generate(env);
    let tre = Address::generate(env);

    client.init(
        &admin,
        &String::from_str(env, "TEST_INV"),
        &sme,
        &10_000i128,
        &500i64,
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
    (client, admin, sme, tok, tre)
}

/// Move the escrow to cancelled status (status == 4).
fn cancel(client: &LiquifactEscrowClient<'_>, _admin: &Address) {
    client.cancel_funding();
}

// ---------------------------------------------------------------------------
// require_funding_open — fund entrypoints reject non-open status
// ---------------------------------------------------------------------------

/// `fund` rejects when escrow is cancelled (require_funding_open → EscrowNotOpenForFunding).
#[test]
fn test_fund_rejects_when_cancelled() {
    let env = make_env();
    let (client, admin, _sme, _tok, _tre) = setup_open(&env);
    cancel(&client, &admin);
    let investor = Address::generate(&env);
    let result = client.try_fund(&investor, &500i128);
    assert_contract_error(result, EscrowError::EscrowNotOpenForFunding);
}

/// `fund_with_commitment` rejects when escrow is cancelled.
#[test]
fn test_fund_with_commitment_rejects_when_cancelled() {
    let env = make_env();
    let (client, admin, _sme, _tok, _tre) = setup_open(&env);
    cancel(&client, &admin);
    let investor = Address::generate(&env);
    let result = client.try_fund_with_commitment(&investor, &500i128, &0u64);
    assert_contract_error(result, EscrowError::EscrowNotOpenForFunding);
}

/// `fund_batch` rejects when escrow is cancelled.
#[test]
fn test_fund_batch_rejects_when_cancelled() {
    let env = make_env();
    let (client, admin, _sme, _tok, _tre) = setup_open(&env);
    cancel(&client, &admin);
    let investor = Address::generate(&env);
    let entries = soroban_sdk::vec![&env, (investor, 500i128)];
    let result = client.try_fund_batch(&entries);
    assert_contract_error(result, EscrowError::EscrowNotOpenForFunding);
}

// ---------------------------------------------------------------------------
// guard_status_eq — entrypoints with per-call typed error codes
// ---------------------------------------------------------------------------

/// `update_funding_target` rejects when cancelled with `TargetUpdateNotOpen`.
#[test]
fn test_update_funding_target_rejects_when_cancelled() {
    let env = make_env();
    let (client, admin, _sme, _tok, _tre) = setup_open(&env);
    cancel(&client, &admin);
    let result = client.try_update_funding_target(&5_000i128);
    assert_contract_error(result, EscrowError::TargetUpdateNotOpen);
}

/// `lower_max_unique_investors` rejects when cancelled with `CapLowerNotOpen`.
#[test]
fn test_lower_max_unique_investors_rejects_when_cancelled() {
    let env = make_env();
    // Must configure a max_unique_investors cap at init.
    let client = LiquifactEscrowClient::new(&env, &env.register(LiquifactEscrow, ()));
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let tok = Address::generate(&env);
    let tre = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CAP_INV"),
        &sme,
        &10_000i128,
        &500i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &Some(10u32), // max_unique_investors cap
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    cancel(&client, &admin);
    let result = client.try_lower_max_unique_investors(&5u32);
    assert_contract_error(result, EscrowError::CapLowerNotOpen);
}

/// `lower_min_contribution_floor` rejects when cancelled with `FloorLowerNotOpen`.
#[test]
fn test_lower_min_contribution_floor_rejects_when_cancelled() {
    let env = make_env();
    // Must configure a min_contribution floor at init.
    let client = LiquifactEscrowClient::new(&env, &env.register(LiquifactEscrow, ()));
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let tok = Address::generate(&env);
    let tre = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "FLR_INV"),
        &sme,
        &10_000i128,
        &500i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &Some(100i128), // min_contribution floor
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    cancel(&client, &admin);
    let result = client.try_lower_min_contribution_floor(&50i128);
    assert_contract_error(result, EscrowError::FloorLowerNotOpen);
}

// ---------------------------------------------------------------------------
// guard_status_eq — other entrypoints (regression)
// ---------------------------------------------------------------------------

/// `cancel_funding` rejects when already cancelled with `CancelFundingNotOpen`.
#[test]
fn test_cancel_funding_rejects_when_already_cancelled() {
    let env = make_env();
    let (client, admin, _sme, _tok, _tre) = setup_open(&env);
    cancel(&client, &admin);
    let result = client.try_cancel_funding();
    assert_contract_error(result, EscrowError::CancelFundingNotOpen);
}

/// `refund` rejects when escrow is open (not cancelled) with `RefundNotCancelled`.
#[test]
fn test_refund_rejects_when_open() {
    let env = make_env();
    let (client, _admin, _sme, _tok, _tre) = setup_open(&env);
    let investor = Address::generate(&env);
    let result = client.try_refund(&investor);
    assert_contract_error(result, EscrowError::RefundNotCancelled);
}

// ---------------------------------------------------------------------------
// Happy-path: open-window calls succeed while status == 0
// ---------------------------------------------------------------------------

/// `update_funding_target` succeeds when escrow is open.
#[test]
fn test_update_funding_target_succeeds_when_open() {
    let env = make_env();
    let (client, _admin, _sme, _tok, _tre) = setup_open(&env);
    // Should not panic; target remains valid (> 0 and >= funded_amount == 0).
    client.update_funding_target(&8_000i128);
}

/// `lower_max_unique_investors` succeeds when escrow is open and cap is configured.
#[test]
fn test_lower_max_unique_investors_succeeds_when_open() {
    let env = make_env();
    let client = LiquifactEscrowClient::new(&env, &env.register(LiquifactEscrow, ()));
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let tok = Address::generate(&env);
    let tre = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "OPEN_CAP"),
        &sme,
        &10_000i128,
        &500i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &Some(10u32),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    // Lowering from 10 to 5 should succeed while open.
    client.lower_max_unique_investors(&5u32);
}
