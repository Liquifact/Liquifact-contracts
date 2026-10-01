use crate::{
    tests::{assert_contract_error, init_and_fund_with_real_token, TARGET},
    EscrowError, FinalRelease, PartialRelease,
};
use soroban_sdk::token::TokenClient;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    vec, Address, Env, IntoVal, String,
};

#[test]
fn test_release_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = init_and_fund_with_real_token(&env, TARGET, "INV001");
    assert_contract_error(
        client.try_release(&0),
        EscrowError::ReleaseAmountNotPositive,
    );
}

#[test]
fn test_release_exact_remaining() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, escrow_id, sme) = init_and_fund_with_real_token(&env, TARGET, "INV001");

    let token = client.funding_token();
    let token_client = TokenClient::new(&env, &token);

    let init_sme_balance = token_client.balance(&sme);
    let init_escrow_balance = token_client.balance(&escrow_id);

    client.release(&TARGET);

    let final_sme_balance = token_client.balance(&sme);
    assert_eq!(final_sme_balance, init_sme_balance + TARGET);
    assert_eq!(
        token_client.balance(&escrow_id),
        init_escrow_balance - TARGET
    );

    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 3); // Withdrawn
}

#[test]
fn test_release_above_remaining() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = init_and_fund_with_real_token(&env, TARGET, "INV001");

    assert_contract_error(
        client.try_release(&(TARGET + 1)),
        EscrowError::ReleaseExceedsRemaining,
    );
}

#[test]
fn test_two_partial_releases_race() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, escrow_id, sme) = init_and_fund_with_real_token(&env, TARGET, "INV001");

    let p1 = TARGET / 2;
    client.release(&p1);

    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 1); // Still funded

    let p2 = TARGET - p1;
    client.release(&p2);

    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 3); // Withdrawn
}

#[test]
fn test_final_release_repeated() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = init_and_fund_with_real_token(&env, TARGET, "INV001");

    client.release(&TARGET);

    // Escrow is now status 3, so release should fail with ReleaseNotFunded
    assert_contract_error(client.try_release(&TARGET), EscrowError::ReleaseNotFunded);
}

#[test]
fn test_release_unauthorized() {
    let env = Env::default();
    let (client, _, _) = init_and_fund_with_real_token(&env, TARGET, "INV001");

    // without mock_all_auths, this should fail with auth error.
    let res = client.try_release(&TARGET);
    assert!(res.is_err());
}
