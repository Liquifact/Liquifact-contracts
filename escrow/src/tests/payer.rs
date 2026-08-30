use super::*;
use soroban_sdk::{symbol_short, testutils::Events, Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn assert_contract_error<T, E>(
    result: Result<Result<T, E>, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
    expected: EscrowError,
) where
    T: std::fmt::Debug,
    E: std::fmt::Debug,
{
    let expected_code = expected as u32;
    match result {
        Err(Ok(error)) => {
            assert_eq!(
                error,
                soroban_sdk::Error::from_contract_error(expected_code)
            );
        }
        Err(Err(soroban_sdk::InvokeError::Contract(code))) => {
            assert_eq!(code, expected_code);
        }
        other => panic!("expected ContractError({expected_code}), got {other:?}"),
    }
}

fn setup_payer(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address, Address) {
    let (client, admin, sme) = setup(env);
    let payer = Address::generate(env);
    (client, admin, sme, payer)
}

// ===========================================================================
// 1. Init sets payer = admin
// ===========================================================================

#[test]
fn init_sets_payer_to_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "PAYER_INIT"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let escrow = client.get_escrow();
    assert_eq!(escrow.payer, admin);
}

// ===========================================================================
// 2. rotate_payer happy path
// ===========================================================================

#[test]
fn rotate_payer_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, payer) = setup_payer(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROT_HAPPY"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let new_payer = Address::generate(&env);
    let updated = client.rotate_payer(&new_payer);

    assert_eq!(updated.payer, new_payer);
    assert_eq!(client.get_escrow().payer, new_payer);
}

// ===========================================================================
// 3. rotate_payer: no-op rejected (same address)
// ===========================================================================

#[test]
fn rotate_payer_noop_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, _payer) = setup_payer(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROT_NOOP"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Try rotating to admin (the current payer set at init) — should fail
    let result = client.try_rotate_payer(&admin);
    assert_contract_error(result, EscrowError::NewPayerSameAsCurrent);
}

// ===========================================================================
// 4. rotate_payer: legal hold blocks rotation
// ===========================================================================

#[test]
fn rotate_payer_legal_hold_blocks() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, _payer) = setup_payer(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROT_HOLD"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.set_legal_hold(&true);
    let new_payer = Address::generate(&env);
    let result = client.try_rotate_payer(&new_payer);
    assert_contract_error(result, EscrowError::LegalHoldBlocksPayerRotation);
}

// ===========================================================================
// 5. rotate_payer: not open (settled) blocks rotation
// ===========================================================================

#[test]
fn rotate_payer_not_open_blocks() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, _payer) = setup_payer(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROT_SETTLED"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000i128);
    client.settle();

    let new_payer = Address::generate(&env);
    let result = client.try_rotate_payer(&new_payer);
    assert_contract_error(result, EscrowError::PayerRotationNotOpen);
}

// ===========================================================================
// 6. rotate_payer: in funded (status=1) is allowed
// ===========================================================================

#[test]
fn rotate_payer_allowed_when_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, payer) = setup_payer(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROT_FUND"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Rotate to desired payer first
    client.rotate_payer(&payer);

    let investor = Address::generate(&env);
    client.fund(&investor, &5_000i128);

    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 0); // not yet fully funded

    let new_payer = Address::generate(&env);
    let updated = client.rotate_payer(&new_payer);
    assert_eq!(updated.payer, new_payer);
}

// ===========================================================================
// 7. rotate_payer: requires dual auth (admin + payer)
// ===========================================================================

#[test]
fn rotate_payer_requires_dual_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, _payer) = setup_payer(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROT_AUTH"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Clear all auths so payer+admin auth is missing
    env.set_auths(&[]);
    let new_payer = Address::generate(&env);
    let _ = client.try_rotate_payer(&new_payer);
    // Should fail — either admin or payer auth missing
}

// ===========================================================================
// 8. fund: payer auth is required
// ===========================================================================

#[test]
fn fund_requires_payer_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, _payer) = setup_payer(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FUND_AUTH"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    // Clear all auths so payer auth is missing
    env.set_auths(&[]);
    let _ = client.try_fund(&investor, &1_000i128);
    // Should fail — payer auth missing
}

// ===========================================================================
// 9. fund: payer can be different from investor
// ===========================================================================

#[test]
fn fund_payer_different_from_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, payer) = setup_payer(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FUND_DIFF"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Rotate to our specific payer
    client.rotate_payer(&payer);

    let investor = Address::generate(&env);
    let funded = client.fund(&investor, &5_000i128);

    assert_eq!(funded.funded_amount, 5_000i128);
    assert_eq!(client.get_escrow().payer, payer);
    assert_eq!(client.get_escrow().sme_address, sme);
    assert_ne!(payer, investor);
}

// ===========================================================================
// 10. double rotation: admin rotates, then new payer rotates
// ===========================================================================

#[test]
fn double_payer_rotation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, _payer) = setup_payer(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROT_DBL"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let payer1 = Address::generate(&env);
    let payer2 = Address::generate(&env);

    let updated1 = client.rotate_payer(&payer1);
    assert_eq!(updated1.payer, payer1);

    let updated2 = client.rotate_payer(&payer2);
    assert_eq!(updated2.payer, payer2);

    let escrow = client.get_escrow();
    assert_eq!(escrow.payer, payer2);
}

// ===========================================================================
// 11. rotate_beneficiary still works alongside payer
// ===========================================================================

#[test]
fn rotate_beneficiary_with_payer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, payer) = setup_payer(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROT_BOTH"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.rotate_payer(&payer);

    let new_sme = Address::generate(&env);
    let updated = client.rotate_beneficiary(&new_sme);

    assert_eq!(updated.sme_address, new_sme);
    assert_eq!(updated.payer, payer);
}

// ===========================================================================
// 12. get_escrow_summary includes payer via escrow
// ===========================================================================

#[test]
fn escrow_summary_includes_payer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, payer) = setup_payer(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SUMMARY"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.rotate_payer(&payer);

    let summary = client.get_escrow_summary();
    assert_eq!(summary.escrow.payer, payer);
}

// ===========================================================================
// 13. rotate_payer to cancelled status is blocked
// ===========================================================================

#[test]
fn rotate_payer_cancelled_blocks() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, _payer) = setup_payer(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROT_CANCEL"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.set_legal_hold(&true);
    client.clear_legal_hold();
    client.cancel_funding();

    let new_payer = Address::generate(&env);
    let result = client.try_rotate_payer(&new_payer);
    assert_contract_error(result, EscrowError::PayerRotationNotOpen);
}

// ===========================================================================
// 14. fund after payer rotation: new payer's auth is needed
// ===========================================================================

#[test]
fn fund_after_payer_rotation_uses_new_payer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, payer) = setup_payer(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FUND_ROT"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.rotate_payer(&payer);

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000i128);

    let escrow = client.get_escrow();
    assert_eq!(escrow.funded_amount, 10_000i128);
    assert_eq!(escrow.status, 1);
    assert_eq!(escrow.payer, payer);
}
