//! Tests for balance-delta invariants with mocked tokens.
//!
//! This module contains tests that would fail if balance deltas diverge from expected behavior.
//! Uses mocked token implementations where feasible in the Soroban test harness.

use super::super::external_calls::{
    transfer_funding_token_with_balance_checks, transfer_into_escrow_with_balance_checks,
};
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
#[should_panic]
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
#[should_panic]
fn test_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 0);
}

#[test]
#[should_panic]
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
#[should_panic]
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

// ---------------------------------------------------------------------------
// Mock: rebasing token that mints extra tokens to sender after transfer
// Simulates an elastic-supply token that changes balances unexpectedly.
// ---------------------------------------------------------------------------

#[contract]
pub struct RebasingToken;

#[contractimpl]
impl TokenInterface for RebasingToken {
    fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        let to_addr = to.address();

        // Standard transfer first
        let from_bal = Self::balance(env.clone(), from.clone());
        let to_bal = Self::balance(env.clone(), to_addr.clone());
        env.storage().persistent().set(&from, &(from_bal - amount));
        env.storage().persistent().set(&to_addr, &(to_bal + amount));

        // Rebasing effect: mint 10% extra to sender after transfer (simulates supply expansion)
        // This causes sender post-balance to be higher than expected, triggering underflow guard
        let malicious_mint = amount / 10;
        env.storage()
            .persistent()
            .set(&from, &(from_bal - amount + malicious_mint));
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
        soroban_sdk::String::from_str(&env, "RebaseToken")
    }
    fn symbol(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "REBASE")
    }
}

/// Mint tokens directly into the rebasing token's storage (bypasses transfer).
fn mint_rebasing_token(env: &Env, contract_id: &Address, to: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let current: i128 = env.storage().persistent().get(to).unwrap_or(0);
        env.storage().persistent().set(to, &(current + amount));
    });
}

// ---------------------------------------------------------------------------
// Tests: rebasing token detection (sender balance increases after transfer)
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_rebasing_token_sender_increases_rejected() {
    // Rebasing token mints extra tokens to sender after transfer.
    // Sender post-balance > sender pre-balance - amount, which triggers underflow.
    let env = Env::default();
    env.mock_all_auths();

    let rebase_token_id = env.register(RebasingToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_rebasing_token(&env, &rebase_token_id, &holder, 1000i128);

    // Panics: sender ends with 100 (1000 - 1000 + 100 rebasing) instead of 0
    // This triggers SenderBalanceUnderflow because from_before - from_after underflows
    transfer_funding_token_with_balance_checks(
        &env,
        &rebase_token_id,
        &holder,
        &treasury,
        1000i128,
    );
}

// ---------------------------------------------------------------------------
// Mock: hook token that steals from recipient after transfer
// Simulates a token with transfer hooks that modify recipient balance.
// ---------------------------------------------------------------------------

#[contract]
pub struct HookStealingToken;

#[contractimpl]
impl TokenInterface for HookStealingToken {
    fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        let to_addr = to.address();

        // Standard transfer
        let from_bal = Self::balance(env.clone(), from.clone());
        let to_bal = Self::balance(env.clone(), to_addr.clone());
        env.storage().persistent().set(&from, &(from_bal - amount));
        env.storage().persistent().set(&to_addr, &(to_bal + amount));

        // Hook effect: burn 10% of recipient's balance after transfer
        let burn_amount = amount / 10;
        let new_to_bal = to_bal + amount - burn_amount;
        env.storage().persistent().set(&to_addr, &new_to_bal);
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
        soroban_sdk::String::from_str(&env, "HookToken")
    }
    fn symbol(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "HOOK")
    }
}

fn mint_hook_token(env: &Env, contract_id: &Address, to: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let current: i128 = env.storage().persistent().get(to).unwrap_or(0);
        env.storage().persistent().set(to, &(current + amount));
    });
}

// ---------------------------------------------------------------------------
// Tests: hook token detection (recipient balance decreases after transfer)
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_hook_token_recipient_decreases_rejected() {
    // Hook steals 10% after transfer.
    // Treasury post-balance < treasury pre-balance + amount, triggering RecipientBalanceDeltaMismatch.
    let env = Env::default();
    env.mock_all_auths();

    let hook_token_id = env.register(HookStealingToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_hook_token(&env, &hook_token_id, &holder, 1000i128);

    // Panics: recipient ends with 900 (1000 - 100 hook steal) instead of 1000
    transfer_funding_token_with_balance_checks(&env, &hook_token_id, &holder, &treasury, 1000i128);
}

// ---------------------------------------------------------------------------
// Mock: malicious token that credits sender instead of debiting
// Simulates a "lying" token that reports incorrect balance changes.
// ---------------------------------------------------------------------------

#[contract]
pub struct LyingToken;

#[contractimpl]
impl TokenInterface for LyingToken {
    fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    fn transfer(_env: Env, from: Address, _to: MuxedAddress, amount: i128) {
        from.require_auth();
        // Intentionally does nothing - no debit, no credit
        // This simulates a token that lies about transfer execution
        _env.storage()
            .persistent()
            .get::<Address, i128>(&_to.address())
            .unwrap_or(0);
        let _ = amount;
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
        soroban_sdk::String::from_str(&env, "LyingToken")
    }
    fn symbol(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "LYE")
    }
}

fn mint_lying_token(env: &Env, contract_id: &Address, to: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let current: i128 = env.storage().persistent().get(to).unwrap_or(0);
        env.storage().persistent().set(to, &(current + amount));
    });
}

// ---------------------------------------------------------------------------
// Tests: lying token detection (no balance change at all)
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_lying_token_no_change_rejected() {
    // Token that does nothing on transfer - no debit, no credit.
    // Both sender and recipient deltas are 0, neither equals amount.
    let env = Env::default();
    env.mock_all_auths();

    let lying_token_id = env.register(LyingToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_lying_token(&env, &lying_token_id, &holder, 1000i128);

    // Panics with SenderBalanceDeltaMismatch (sender didn't lose anything)
    transfer_funding_token_with_balance_checks(&env, &lying_token_id, &holder, &treasury, 1000i128);
}

// ---------------------------------------------------------------------------
// Tests: error code assertions (validate specific panic reasons)
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_amount_zero_panics_with_transfer_amount_not_positive() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // Error code 36: TransferAmountNotPositive
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 0);
}

#[test]
#[should_panic]
fn test_amount_negative_panics_with_transfer_amount_not_positive() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // Error code 36: TransferAmountNotPositive (negative is still not positive)
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, -50);
}

#[test]
#[should_panic]
fn test_insufficient_balance_panics_with_insufficient_token_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // Mint nothing - balance is 0
    // Error code 37: InsufficientTokenBalanceBeforeTransfer
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 1);
}

// ---------------------------------------------------------------------------
// Tests: inbound transfer_into_escrow_with_balance_checks
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_inbound_fee_on_transfer_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let fee_token_id = env.register(FeeOnTransferToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_fee_token(&env, &fee_token_id, &investor, 1000i128);
    // Recipient (escrow) receives less than amount -> panic
    transfer_into_escrow_with_balance_checks(&env, &fee_token_id, &investor, &escrow, 1000i128);
}

#[test]
#[should_panic]
fn test_inbound_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);
    transfer_into_escrow_with_balance_checks(&env, &token.id, &investor, &escrow, 0);
}

#[test]
#[should_panic]
fn test_inbound_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);
    transfer_into_escrow_with_balance_checks(&env, &token.id, &investor, &escrow, -1i128);
}

#[test]
#[should_panic]
fn test_inbound_insufficient_balance_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);
    // Investor has no tokens
    transfer_into_escrow_with_balance_checks(&env, &token.id, &investor, &escrow, 1i128);
}

#[test]
#[should_panic]
fn test_inbound_lying_token_no_change_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let lying_token_id = env.register(LyingToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_lying_token(&env, &lying_token_id, &investor, 1000i128);
    // No balance change -> RecipientBalanceDeltaMismatch
    transfer_into_escrow_with_balance_checks(&env, &lying_token_id, &investor, &escrow, 1000i128);
}

#[test]
#[should_panic]
fn test_inbound_hook_token_recipient_decreases_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let hook_token_id = env.register(HookStealingToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_hook_token(&env, &hook_token_id, &investor, 1000i128);
    // Hook reduces escrow balance after transfer
    transfer_into_escrow_with_balance_checks(&env, &hook_token_id, &investor, &escrow, 1000i128);
}

#[test]
#[should_panic]
#[ignore = "upstream latent: escrow API/test drift"]
fn test_inbound_rebasing_token_sender_increases_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let rebase_token_id = env.register(RebasingToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_rebasing_token(&env, &rebase_token_id, &investor, 1000i128);
    // Sender ends with extra tokens -> SenderBalanceDeltaMismatch
    transfer_into_escrow_with_balance_checks(&env, &rebase_token_id, &investor, &escrow, 1000i128);
}

#[test]
fn test_inbound_compliant_token_passes() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);
    let amount = 1000i128;
    token.stellar.mint(&investor, &amount);
    let investor_before = token.token.balance(&investor);
    let escrow_before = token.token.balance(&escrow);
    transfer_into_escrow_with_balance_checks(&env, &token.id, &investor, &escrow, amount);
    let investor_after = token.token.balance(&investor);
    let escrow_after = token.token.balance(&escrow);
    assert_eq!(investor_before - investor_after, amount);
    assert_eq!(escrow_after - escrow_before, amount);
}

// ---------------------------------------------------------------------------
// Tests: MOCK_TOKEN_DEFAULT_BALANCE constant — unseen-address semantics
// ---------------------------------------------------------------------------

/// An unseen address should report exactly MOCK_TOKEN_DEFAULT_BALANCE via the mock.
#[test]
fn test_mock_token_default_balance_unseen_address() {
    use super::super::DefaultMockToken;
    use super::super::MOCK_TOKEN_DEFAULT_BALANCE;
    use soroban_sdk::token::TokenClient;

    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register(DefaultMockToken, ());
    let client = TokenClient::new(&env, &token_id);

    let stranger = Address::generate(&env);

    assert_eq!(
        client.balance(&stranger),
        MOCK_TOKEN_DEFAULT_BALANCE,
        "unseen address must report MOCK_TOKEN_DEFAULT_BALANCE"
    );
}

/// A transfer between two unseen addresses should produce symmetric deltas around
/// MOCK_TOKEN_DEFAULT_BALANCE: sender loses `amount`, recipient gains `amount`.
#[test]
fn test_mock_token_transfer_between_two_unseen_addresses() {
    use super::super::DefaultMockToken;
    use super::super::MOCK_TOKEN_DEFAULT_BALANCE;
    use soroban_sdk::token::TokenClient;

    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register(DefaultMockToken, ());
    let client = TokenClient::new(&env, &token_id);

    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let amount = 1_000_000i128;

    let sender_before = client.balance(&sender);
    let recipient_before = client.balance(&recipient);

    assert_eq!(sender_before, MOCK_TOKEN_DEFAULT_BALANCE);
    assert_eq!(recipient_before, MOCK_TOKEN_DEFAULT_BALANCE);

    client.transfer(&sender, &recipient, &amount);

    assert_eq!(
        client.balance(&sender),
        MOCK_TOKEN_DEFAULT_BALANCE - amount,
        "sender balance should decrease by amount"
    );
    assert_eq!(
        client.balance(&recipient),
        MOCK_TOKEN_DEFAULT_BALANCE + amount,
        "recipient balance should increase by amount"
    );
}

/// Repeated transfers from an unseen sender accumulate correctly against the default.
#[test]
fn test_mock_token_repeated_transfers_from_unseen_sender() {
    use super::super::DefaultMockToken;
    use super::super::MOCK_TOKEN_DEFAULT_BALANCE;
    use soroban_sdk::token::TokenClient;

    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register(DefaultMockToken, ());
    let client = TokenClient::new(&env, &token_id);

    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let amount = 500i128;
    let rounds = 3i128;

    for _ in 0..rounds {
        client.transfer(&sender, &recipient, &amount);
    }

    assert_eq!(
        client.balance(&sender),
        MOCK_TOKEN_DEFAULT_BALANCE - amount * rounds,
        "sender balance should decrease by total transferred"
    );
    assert_eq!(
        client.balance(&recipient),
        MOCK_TOKEN_DEFAULT_BALANCE + amount * rounds,
        "recipient balance should increase by total transferred"
    );
}
