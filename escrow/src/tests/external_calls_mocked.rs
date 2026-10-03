//! Compatibility-contract tests for the balance-delta wrappers in `external_calls`.
//!
//! Every rejection test asserts the exact `EscrowError` code, so a token that fails for the
//! wrong reason cannot pass. Error numbers are pinned in `test_error_code_numbers_are_stable`
//! because callers depend on them.
//!
//! Check order in both wrappers: amount > 0, sender balance >= amount, then after the
//! transfer the sender delta (`spent == amount`) before the recipient delta (`received == amount`).

use super::super::external_calls::{
    transfer_funding_token_inbound_with_balance_checks, transfer_funding_token_with_balance_checks,
};
use super::*;
use crate::EscrowError;
use soroban_sdk::{contract, contractimpl, token::TokenInterface, Address, Env, MuxedAddress};

/// Mint directly into a mock token's storage (bypasses `transfer`).
fn mint_mock_balance(env: &Env, contract_id: &Address, to: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let current: i128 = env.storage().persistent().get(to).unwrap_or(0);
        env.storage().persistent().set(to, &(current + amount));
    });
}

// ---------------------------------------------------------------------------
// Mock: fee-on-transfer token (sender debited in full, recipient credited 99%)
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
        let credited = amount - amount / 100;
        let to_addr = to.address();

        let from_bal = Self::balance(env.clone(), from.clone());
        env.storage().persistent().set(&from, &(from_bal - amount));

        let to_bal = Self::balance(env.clone(), to_addr.clone());
        env.storage()
            .persistent()
            .set(&to_addr, &(to_bal + credited));
    }

    fn allowance(_env: Env, _from: Address, _spender: Address) -> i128 {
        0
    }
    fn approve(_env: Env, _from: Address, _spender: Address, _amount: i128, _exp: u32) {}
    fn transfer_from(_env: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {
        unimplemented()
    }
    fn burn(_env: Env, _from: Address, _amount: i128) {
        unimplemented()
    }
    fn burn_from(_env: Env, _spender: Address, _from: Address, _amount: i128) {
        unimplemented()
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

// ---------------------------------------------------------------------------
// Mock: rebasing token (sender ends with MORE than it started with)
// Sender delta is negative. `checked_sub` does not fail on a negative result, so the wrapper
// reports SenderBalanceDeltaMismatch, not SenderBalanceUnderflow.
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

        let from_bal = Self::balance(env.clone(), from.clone());
        let to_bal = Self::balance(env.clone(), to_addr.clone());

        // Sender is debited `amount` then rebased up by `2 * amount`: net `from_bal + amount`.
        env.storage().persistent().set(&from, &(from_bal + amount));
        env.storage().persistent().set(&to_addr, &(to_bal + amount));
    }

    fn allowance(_env: Env, _from: Address, _spender: Address) -> i128 {
        0
    }
    fn approve(_env: Env, _from: Address, _spender: Address, _amount: i128, _exp: u32) {}
    fn transfer_from(_env: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {
        unimplemented()
    }
    fn burn(_env: Env, _from: Address, _amount: i128) {
        unimplemented()
    }
    fn burn_from(_env: Env, _spender: Address, _from: Address, _amount: i128) {
        unimplemented()
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

// ---------------------------------------------------------------------------
// Mock: hook token (burns 10% of the transferred amount from the recipient after transfer)
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

        let from_bal = Self::balance(env.clone(), from.clone());
        let to_bal = Self::balance(env.clone(), to_addr.clone());
        env.storage().persistent().set(&from, &(from_bal - amount));
        env.storage()
            .persistent()
            .set(&to_addr, &(to_bal + amount - amount / 10));
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

// ---------------------------------------------------------------------------
// Mock: lying token (transfer succeeds but moves nothing)
// ---------------------------------------------------------------------------

#[contract]
pub struct LyingToken;

#[contractimpl]
impl TokenInterface for LyingToken {
    fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    fn transfer(_env: Env, from: Address, _to: MuxedAddress, _amount: i128) {
        from.require_auth();
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

// ---------------------------------------------------------------------------
// Compatibility: public error numbers must not change
// ---------------------------------------------------------------------------

#[test]
fn test_error_code_numbers_are_stable() {
    assert_eq!(EscrowError::TransferAmountNotPositive as u32, 36);
    assert_eq!(EscrowError::InsufficientTokenBalanceBeforeTransfer as u32, 37);
    assert_eq!(EscrowError::SenderBalanceUnderflow as u32, 38);
    assert_eq!(EscrowError::SenderBalanceDeltaMismatch as u32, 40);
    assert_eq!(EscrowError::RecipientBalanceDeltaMismatch as u32, 41);
    assert_eq!(EscrowError::InboundTransferAmountNotPositive as u32, 171);
    assert_eq!(
        EscrowError::InboundInsufficientTokenBalanceBeforeTransfer as u32,
        172
    );
    assert_eq!(EscrowError::InboundSenderBalanceUnderflow as u32, 173);
    assert_eq!(EscrowError::InboundSenderBalanceDeltaMismatch as u32, 174);
    assert_eq!(EscrowError::InboundRecipientBalanceDeltaMismatch as u32, 176);
}

// ---------------------------------------------------------------------------
// Outbound (escrow -> treasury): rejection paths
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #41)")]
fn test_fee_on_transfer_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(FeeOnTransferToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);
    mint_mock_balance(&env, &token_id, &holder, 1000);

    // Sender loses 1000, recipient gains 990 -> RecipientBalanceDeltaMismatch.
    transfer_funding_token_with_balance_checks(&env, &token_id, &holder, &treasury, 1000);
}

#[test]
#[should_panic(expected = "Error(Contract, #40)")]
fn test_rebasing_token_sender_increases_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(RebasingToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);
    mint_mock_balance(&env, &token_id, &holder, 1000);

    // Sender delta is -1000, not +1000 -> SenderBalanceDeltaMismatch.
    transfer_funding_token_with_balance_checks(&env, &token_id, &holder, &treasury, 1000);
}

#[test]
#[should_panic(expected = "Error(Contract, #41)")]
fn test_hook_token_recipient_decreases_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(HookStealingToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);
    mint_mock_balance(&env, &token_id, &holder, 1000);

    // Recipient gains 900 -> RecipientBalanceDeltaMismatch.
    transfer_funding_token_with_balance_checks(&env, &token_id, &holder, &treasury, 1000);
}

#[test]
#[should_panic(expected = "Error(Contract, #40)")]
fn test_lying_token_no_change_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(LyingToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);
    mint_mock_balance(&env, &token_id, &holder, 1000);

    // Nothing moves; the sender check runs first -> SenderBalanceDeltaMismatch.
    transfer_funding_token_with_balance_checks(&env, &token_id, &holder, &treasury, 1000);
}

#[test]
#[should_panic(expected = "Error(Contract, #36)")]
fn test_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #36)")]
fn test_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);
    let other = Address::generate(&env);

    token.stellar.mint(&holder, &500i128);
    token.stellar.mint(&other, &250i128);

    let accounts = [&holder, &treasury, &other];
    let before = total_balance(&token, &accounts);

    let result = catch_transfer_failure(assert_unwind_safe(|| {
        transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 1000i128);
    }));
    assert!(result.is_error(), "over-spend must fail");

    let after = total_balance(&token, &accounts);
    assert_eq!(before, after, "total supply must be conserved on failure");
}

#[test]
fn test_failure_leaves_all_accounts_unchanged() {
    // Explicitly check that no account is debited or credited when the transfer fails.
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, -1);
}

#[test]
#[should_panic(expected = "Error(Contract, #37)")]
fn test_insufficient_balance_zero_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #37)")]
fn test_insufficient_balance_off_by_one_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    token.stellar.mint(&holder, &999i128);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 1000);
}

// ---------------------------------------------------------------------------
// Outbound: compliant token (control cases)
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

    assert_eq!(
        holder_before + treasury_before,
        holder_after + treasury_after,
        "total supply must be conserved"
    );
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

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 1);

    assert_eq!(token.token.balance(&holder), 0);
    assert_eq!(token.token.balance(&treasury), 1);
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

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, large_amount);

    assert_eq!(token.token.balance(&holder), 0);
    assert_eq!(token.token.balance(&treasury), large_amount);
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

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury1, 1000);
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury2, 1000);

    assert_eq!(token.token.balance(&holder), 1000);
    assert_eq!(token.token.balance(&treasury1), 1000);
    assert_eq!(token.token.balance(&treasury2), 1000);
}

// ---------------------------------------------------------------------------
// Inbound (investor -> escrow): rejection paths
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #176)")]
fn test_inbound_fee_on_transfer_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(FeeOnTransferToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_mock_balance(&env, &token_id, &investor, 1000);

    transfer_funding_token_inbound_with_balance_checks(&env, &token_id, &investor, &escrow, 1000);
}

#[test]
#[should_panic(expected = "Error(Contract, #174)")]
fn test_inbound_rebasing_token_sender_increases_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(RebasingToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_mock_balance(&env, &token_id, &investor, 1000);

    transfer_funding_token_inbound_with_balance_checks(&env, &token_id, &investor, &escrow, 1000);
}

#[test]
#[should_panic(expected = "Error(Contract, #176)")]
fn test_inbound_hook_token_recipient_decreases_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(HookStealingToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_mock_balance(&env, &token_id, &investor, 1000);

    transfer_funding_token_inbound_with_balance_checks(&env, &token_id, &investor, &escrow, 1000);
}

#[test]
#[should_panic(expected = "Error(Contract, #174)")]
fn test_inbound_lying_token_no_change_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(LyingToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_mock_balance(&env, &token_id, &investor, 1000);

    // Sender check runs before the recipient check -> InboundSenderBalanceDeltaMismatch.
    transfer_funding_token_inbound_with_balance_checks(&env, &token_id, &investor, &escrow, 1000);
}

#[test]
#[should_panic(expected = "Error(Contract, #171)")]
fn test_inbound_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);

    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &escrow, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #171)")]
fn test_inbound_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let escrow = Address::generate(&env);

    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &escrow, -1);
}

#[test]
#[should_panic(expected = "Error(Contract, #172)")]
fn test_inbound_insufficient_balance_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);

    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &escrow, 1);
}

// ---------------------------------------------------------------------------
// Inbound: compliant token (control cases)
// ---------------------------------------------------------------------------

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

    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &escrow, amount);

    assert_eq!(investor_before - token.token.balance(&investor), amount);
    assert_eq!(token.token.balance(&escrow) - escrow_before, amount);
}

#[test]
fn test_inbound_minimum_amount_passes() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);
    token.stellar.mint(&investor, &1i128);

    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &escrow, 1);

    assert_eq!(token.token.balance(&investor), 0);
    assert_eq!(token.token.balance(&escrow), 1);
}

#[test]
fn test_inbound_large_transfer_no_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);
    let large_amount = i128::MAX / 100;
    token.stellar.mint(&investor, &large_amount);

    transfer_funding_token_inbound_with_balance_checks(
        &env,
        &token.id,
        &investor,
        &escrow,
        large_amount,
    );

    assert_eq!(token.token.balance(&investor), 0);
    assert_eq!(token.token.balance(&escrow), large_amount);
}

// ---------------------------------------------------------------------------
// MOCK_TOKEN_DEFAULT_BALANCE: unseen-address semantics (unchanged behavior)
// ---------------------------------------------------------------------------

#[test]
fn test_recovery_after_partial_failure_is_consistent() {
    // After a partial failure, a subsequent successful transfer must operate on the
    // original state and move exactly the requested amount.
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(DefaultMockToken, ());
    let client = TokenClient::new(&env, &token_id);
    let stranger = Address::generate(&env);

    // Failed attempt to transfer more than available.
    let failed = catch_transfer_failure(assert_unwind_safe(|| {
        transfer_into_escrow_with_balance_checks(
            &env,
            &token.id,
            &holder,
            &escrow,
            2000i128,
        );
    }));
    assert!(failed.is_error(), "over-spend must fail");
    assert_eq!(token.token.balance(&holder), 1000i128);
    assert_eq!(token.token.balance(&escrow), 0i128);

    // Recovery: a successful transfer of the available amount must now succeed.
    transfer_into_escrow_with_balance_checks(&env, &token.id, &holder, &escrow, 1000i128);

    assert_eq!(token.token.balance(&holder), 0i128);
    assert_eq!(token.token.balance(&escrow), 1000i128);
}

#[test]
fn test_boundary_exact_balance_succeeds() {
    // Boundary: transferring exactly the available balance must succeed and leave
    // the sender at zero.
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(DefaultMockToken, ());
    let client = TokenClient::new(&env, &token_id);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let amount = 1_000_000i128;

    assert_eq!(client.balance(&sender), MOCK_TOKEN_DEFAULT_BALANCE);
    assert_eq!(client.balance(&recipient), MOCK_TOKEN_DEFAULT_BALANCE);

    client.transfer(&sender, &recipient, &amount);

    assert_eq!(client.balance(&sender), MOCK_TOKEN_DEFAULT_BALANCE - amount);
    assert_eq!(
        client.balance(&recipient),
        MOCK_TOKEN_DEFAULT_BALANCE + amount
    );
}

#[test]
fn test_boundary_one_over_balance_fails_deterministically() {
    // Boundary: transferring one unit more than the available balance must fail
    // without mutating any state.
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
        MOCK_TOKEN_DEFAULT_BALANCE - amount * rounds
    );
    assert_eq!(
        client.balance(&recipient),
        MOCK_TOKEN_DEFAULT_BALANCE + amount * rounds
    );
}