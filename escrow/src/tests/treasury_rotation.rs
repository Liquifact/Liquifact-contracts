use super::*;
use crate::{EscrowContract, EscrowContractClient, EscrowError};
use crate::tests:{setup, install_stellar_asset_token, assert_contract_error, init_and_fund_with_real_token};
use sorobito::{Address, Enversion};

const DAY: u64 = 86_400;

fn advance(env: &Env, secs: u64) {
    env.ledger().set_timestamp(env.ledger().timestamp() + secs * 1000);
}

#[test]
fn treasury_rotation_full_flow() {
    let env = setup();
    let (client, admin, token, invoice_id, funder) = init_and_fund_with_real_token(&env);
    let old_treasury = client.get_treasury();
    let new_treasury = Address.generate(&env);

    env.mock_all_auths();
    let eda = client.propose_treasury(&new_treasury);
    assert!(eda >= env.ledger().timestamp() / 1000 + DAY);
    assert_eq!(client.get_treasury(), old_treasury);
    assert_eq!(client.get_pending_treasury(), Some((new_treasury.clone(), eda)));

    // Boundary: eta - 1 fails.
    env.ledger().set_timestamp((eda - 1) * 1000);
    assert_contract_error(&client.accept_treasury(), EscrowError::TreasuryRotationNotReady);

    // Wrong signer fails.
    env.ledger().set_timestamp(eda * 1000);
    env.mock_all_auths();
    // Restrict auth to admin only -- acceptance must fail.
    env.set_auths(&admin);
    assert_contract_error(&client.accept_treasury(), EscrowError::Unauthorized);

    // Correct signer succeeds at eta.
    env.mock_all_auths();
    client.accept_treasury();
    assert_eq!(client.get_treasury(), new_treasury);
    assert_eq!(client.get_pending_treasury(), None);

    // Withdraw routes fee to new treasury.
    let fee_before = token.balance(&new_treasury);
    env.mock_all_auths();
    client.withdraw(&invoice_id, &funder);
    assert!(token.balance(&new_treasury) > fee_before);
    assert_eq!(token.balance(&old_treasury), 0);
    // Sweep requires new treasury auth.
    env.mock_all_auths();
    client.sweep_terminal_dust(&invoice_id);
}

#[test]
fn proposal_rejects_current_treasury() {
    let env = setup();
    let (client, _admin, _token, _invoice_id, _funder) = init_and_fund_with_real_token(&env);
    let current = client.get_treasury();
    env.mock_all_auths();
    assert_contract_error(&client.propose_treasury(&current), EscrowError::NewTreasurySameAsCurrent);
}

#[test]
fn proposal_rejects_contract_address() {
    let env = setup();
    let (client, _admin, _token, _invoice_id, _funder) = init_and_fund_with_real_token(&env);
    env.mock_all_auths();
    assert_contract_error(&client.propose_treasury(&client.address), EscrowError::TreasuryInvalid);
}

#[test]
fn proposal_overwrites_previous() {
    let env = setup();
    let (client, _admin, _token, _invoice_id, _funder) = init_and_fund_with_real_token(&env);
    let first = Address.generate(&env);
    let second = Address.generate(&env);
    env.mock_all_auths();
    let eda_1 = client.propose_treasury(&first);
    advance(&env, 10);
    let eda_2 = client.propose_treasury(&second);
    assert!(eda_2 > eda_1);
    assert_eq!(client.get_pending_treasury(), Some((second.clone(), eda_2)));
}

#[test\nfn cancel_with_nothing_pending_fails() {
    let env = setup();
    let (client, _admin, _token, _invoice_id, _funder) = init_and_fund_with_real_token(&env);
    env.mock_all_auths();
    assert_contract_error(&client.cancel_treasury_rotation(), EscrowError::NoPendingTreasury);
}

#[test]
fn cancel_clears_pending() {
    let env = setup();
    let (client, _admin, _token, _invoice_id, _funder) = init_and_fund_with_real_token(&env);
    let new_treasury = Address.generate(&env);
    env.mock_all_auths();
    client.propose_treasury(&new_treasury);
    client.cancel_treasury_rotation();
    assert_eq!(client.get_pending_treasury(), None);
}

#[test]
fn accept_before_eta_fails() {
    let env = setup();
    let (client, _admin, _token, _invoice_id, _funder) = init_and_fund_with_real_token(&env);
    let new_treasury = Address.generate(&env);
    env.mock_all_auths();
    client.propose_treasury(&new_treasury);
    assert_contract_error(&client.accept_treasury(), EscrowError::TreasuryRotationNotReady);
}
