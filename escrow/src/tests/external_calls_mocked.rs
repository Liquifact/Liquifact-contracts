//! Tests for balance-delta invariants with mocked tokens.
//!
//! This module contains tests that would fail if balance deltas diverge from expected behavior.
//! Uses mocked token implementations where feasible in the Soroban test harness.

use super::super::external_calls::transfer_funding_token_with_balance_checks;
use super::*;
use soroban_sdk::{contract, contractimpl, token::TokenInterface, Address, Env, MuxedAddress};
// ---------------------------------------------------------------------------
// Mock: fee-on-transfer token
// Steals 1% on every transfer — recipient gets less than sender sent.
// Registered as a real Soroban contract so TokenClient can dispatch to it.
// ---------------------------------------------------------------------------

#[contract]
pub struct FeeOnTransferToken;

#[contractimpl]
impl TokenInterface for FeeOnTransferToken {
    fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        let fee = amount / 100; // steal 1%
        let credited = amount - fee; // recipient gets less

        let to_addr = to.address();

        let from_bal = Self::balance(env.clone(), from.clone());
        env.storage().persistent().set(&from, &(from_bal - amount)); // full debit

        let to_bal = Self::balance(env.clone(), to_addr.clone());
        env.storage()
            .persistent()
            .set(&to_addr, &(to_bal + credited)); // under-credit
    }

    fn allowance(_env: Env, _from: Address, _spender: Address) -> i128 {
        0
    }
    fn approve(_env: Env, _from: Address, _spender: Address, _amount: i128, _exp: u32) {}
    fn transfer_from(_env: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {
        unimplemented!()
    }
    fn burn(_env: Env, _from: Address, _amount: i128) {
        unimplemented!()
    }
    fn burn_from(_env: Env, _spender: Address, _from: Address, _amount: i128) {
        unimplemented!()
    }
    fn decimals(_env: Env) -> u32 {
        7
    }
    fn name(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "FeeToken")
    }
    fn symbol(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "FEE")
    }
}

/// Mint tokens directly into the fee token's storage (bypasses transfer).
fn mint_fee_token(env: &Env, contract_id: &Address, to: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let current: i128 = env.storage().persistent().get(to).unwrap_or(0);
        env.storage().persistent().set(to, &(current + amount));
    });
}

// ---------------------------------------------------------------------------
// Tests: fee-on-transfer rejection (the main goal of this issue)
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "recipient balance delta must equal transfer amount")]
fn test_fee_on_transfer_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let fee_token_id = env.register(FeeOnTransferToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_fee_token(&env, &fee_token_id, &holder, 1000i128);

    // Panics: recipient gets 990 but function expects exactly 1000
    transfer_funding_token_with_balance_checks(&env, &fee_token_id, &holder, &treasury, 1000i128);
}

// ---------------------------------------------------------------------------
// Tests: positive-amount guard
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "transfer amount must be positive")]
fn test_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 0);
}

#[test]
#[should_panic(expected = "transfer amount must be positive")]
fn test_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, -1i128);
}

// ---------------------------------------------------------------------------
// Tests: insufficient balance guard
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "insufficient token balance before transfer")]
fn test_insufficient_balance_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // Mint only 500 but try to transfer 1000
    token.stellar.mint(&holder, &500i128);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 1000i128);
}

// ---------------------------------------------------------------------------
// Tests: compliant token (control cases — these should all pass)
// ---------------------------------------------------------------------------

#[test]
fn test_compliant_token_passes() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    let amount = 1000i128;
    token.stellar.mint(&holder, &amount);

    let holder_before = token.token.balance(&holder);
    let treasury_before = token.token.balance(&treasury);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, amount);

    let holder_after = token.token.balance(&holder);
    let treasury_after = token.token.balance(&treasury);

    let total_before = holder_before + treasury_before;
    let total_after = holder_after + treasury_after;

    assert_eq!(total_before, total_after, "total supply must be conserved");
    assert_eq!(holder_before - holder_after, amount);
    assert_eq!(treasury_after - treasury_before, amount);
}

#[test]
fn test_minimum_amount_passes() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    token.stellar.mint(&holder, &1i128);

    let holder_before = token.token.balance(&holder);
    let treasury_before = token.token.balance(&treasury);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 1i128);

    assert_eq!(holder_before - token.token.balance(&holder), 1i128);
    assert_eq!(token.token.balance(&treasury) - treasury_before, 1i128);
}

#[test]
fn test_large_transfer_no_overflow() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    let large_amount = i128::MAX / 100;
    token.stellar.mint(&holder, &large_amount);

    let holder_before = token.token.balance(&holder);
    let treasury_before = token.token.balance(&treasury);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, large_amount);

    assert_eq!(holder_before - token.token.balance(&holder), large_amount);
    assert_eq!(
        token.token.balance(&treasury) - treasury_before,
        large_amount
    );
}

#[test]
fn test_multiple_sequential_transfers() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury1 = Address::generate(&env);
    let treasury2 = Address::generate(&env);

    token.stellar.mint(&holder, &3000i128);

    let transfer_amount = 1000i128;

    let holder_before1 = token.token.balance(&holder);
    let t1_before = token.token.balance(&treasury1);
    transfer_funding_token_with_balance_checks(
        &env,
        &token.id,
        &holder,
        &treasury1,
        transfer_amount,
    );
    assert_eq!(
        holder_before1 - token.token.balance(&holder),
        transfer_amount
    );
    assert_eq!(token.token.balance(&treasury1) - t1_before, transfer_amount);

    let holder_before2 = token.token.balance(&holder);
    let t2_before = token.token.balance(&treasury2);
    transfer_funding_token_with_balance_checks(
        &env,
        &token.id,
        &holder,
        &treasury2,
        transfer_amount,
    );
    assert_eq!(
        holder_before2 - token.token.balance(&holder),
        transfer_amount
    );
    assert_eq!(token.token.balance(&treasury2) - t2_before, transfer_amount);

    assert_eq!(token.token.balance(&holder), 1000i128);
    assert_eq!(token.token.balance(&treasury1), transfer_amount);
    assert_eq!(token.token.balance(&treasury2), transfer_amount);
}

#[test]
fn test_sender_ends_at_zero_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    let amount = 1000i128;
    token.stellar.mint(&holder, &amount);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, amount);

    assert_eq!(token.token.balance(&holder), 0i128);
    assert_eq!(token.token.balance(&treasury), amount);
}
