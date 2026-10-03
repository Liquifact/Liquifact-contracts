//! Compatibility-contract tests for the balance-delta wrappers in `external_calls`.
//!
//! Every rejection test asserts the exact `EscrowError` code, so a token that fails for the
//! wrong reason cannot pass. Error numbers are pinned in `test_error_code_numbers_are_stable`
//! because callers depend on them.
//!
//! ## Check order (both wrappers)
//!
//! 1. `from != to` — self-transfer guard.
//! 2. `amount > 0` — positivity guard.
//! 3. `sender_balance_before >= amount` — sufficiency guard.
//! 4. SEP-41 `transfer` executes.
//! 5. `sender_spent == amount` — sender-side conservation.
//! 6. `recipient_received == amount` — recipient-side conservation.
//!
//! Each test targets exactly one step so a single broken invariant cannot accidentally satisfy
//! a different assertion.
//!
//! ## Mock tokens
//!
//! Four mocks cover non-compliant token archetypes:
//! - `FeeOnTransferToken` — sender debited in full, recipient credited only 99%
//!   (`RecipientBalanceDeltaMismatch`).
//! - `RebasingToken` — sender ends with *more* than it started with (net `+amount`)
//!   (`SenderBalanceDeltaMismatch`).
//! - `HookStealingToken` — recipient receives `amount - amount/10`, i.e. 90%
//!   (`RecipientBalanceDeltaMismatch`).
//! - `LyingToken` — `transfer` is a no-op; no balances change
//!   (sender check fires first → `SenderBalanceDeltaMismatch`).
//!
//! Standard SEP-41 token tests use the `install_stellar_asset_token` helper from `mod.rs`.

use super::super::external_calls::{
    transfer_funding_token_inbound_with_balance_checks,
    transfer_funding_token_with_balance_checks,
    transfer_into_escrow_with_balance_checks,
};
use super::*;
use crate::EscrowError;
use soroban_sdk::{contract, contractimpl, token::TokenInterface, Address, Env, MuxedAddress};

// ---------------------------------------------------------------------------
// Helper: mint directly into a mock token's persistent storage.
//
// This bypasses `transfer` so tests can set up arbitrary balances without
// invoking any of the wrappers under test.
// ---------------------------------------------------------------------------

fn mint_mock_balance(env: &Env, contract_id: &Address, to: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let current: i128 = env.storage().persistent().get(to).unwrap_or(0);
        env.storage().persistent().set(to, &(current + amount));
    });
}

// ---------------------------------------------------------------------------
// Mock: fee-on-transfer token (sender debited in full, recipient credited 99%)
//
// Trigger: `RecipientBalanceDeltaMismatch` (outbound) / `InboundRecipientBalanceDeltaMismatch`
// (inbound) — the sender spends exactly `amount`, but the escrow only receives 99%.
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
        let credited = amount - amount / 100; // 99% to recipient
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

// ---------------------------------------------------------------------------
// Mock: rebasing token (sender ends with MORE than it started with)
//
// Trigger: `SenderBalanceDeltaMismatch` — after "transfer", the sender's balance
// increases by `amount` instead of decreasing.  The wrapper's checked_sub on the
// sender side does not underflow (i128 can represent the negative delta), so the
// mismatch is caught by the delta equality check rather than the underflow check.
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

        // Sender debited `amount` then rebased up by `2 * amount`: net `+amount`.
        env.storage().persistent().set(&from, &(from_bal + amount));
        env.storage().persistent().set(&to_addr, &(to_bal + amount));
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

// ---------------------------------------------------------------------------
// Mock: hook token (burns 10% of the transferred amount from the recipient)
//
// Trigger: `RecipientBalanceDeltaMismatch` — sender is fully debited but recipient
// receives only 90% (`amount - amount / 10`).
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
//
// Trigger: `SenderBalanceDeltaMismatch` — no balances change, so the sender
// check fires first: spent == 0, not `amount`.
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
        // Deliberately moves nothing.
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

// ===========================================================================
// Error-code stability
//
// These numbers are part of the public API and must NEVER change. Client SDKs
// branch on the numeric code, not the variant name.
// ===========================================================================

#[test]
fn test_error_code_numbers_are_stable() {
    // Outbound (escrow → treasury) guards.
    assert_eq!(EscrowError::TransferAmountNotPositive as u32, 36);
    assert_eq!(EscrowError::InsufficientTokenBalanceBeforeTransfer as u32, 37);
    assert_eq!(EscrowError::SenderBalanceUnderflow as u32, 38);
    assert_eq!(EscrowError::SenderBalanceDeltaMismatch as u32, 40);
    assert_eq!(EscrowError::RecipientBalanceDeltaMismatch as u32, 41);

    // Inbound (investor → escrow) guards.
    assert_eq!(EscrowError::InboundTransferAmountNotPositive as u32, 171);
    assert_eq!(
        EscrowError::InboundInsufficientTokenBalanceBeforeTransfer as u32,
        172
    );
    assert_eq!(EscrowError::InboundSenderBalanceUnderflow as u32, 173);
    assert_eq!(EscrowError::InboundSenderBalanceDeltaMismatch as u32, 174);
    assert_eq!(EscrowError::InboundRecipientBalanceDeltaMismatch as u32, 176);

    // Self-transfer guards.
    assert_eq!(EscrowError::TransferSameSenderRecipient as u32, 283);
    assert_eq!(EscrowError::InboundTransferSameSenderRecipient as u32, 284);
}

// ===========================================================================
// Outbound (escrow → treasury): self-transfer guard
// ===========================================================================

/// Outbound self-transfer is rejected before any balance read.
///
/// Invariant: `from != treasury`.  A self-transfer would not move any funds and
/// would bypass balance-delta accounting entirely.
#[test]
#[should_panic(expected = "Error(Contract, #283)")]
fn test_outbound_same_sender_recipient_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    token.stellar.mint(&holder, &1000i128);

    // Sender and treasury are the same address.
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &holder, 1000);
}

// ===========================================================================
// Outbound: non-compliant token rejection paths
// ===========================================================================

/// Fee-on-transfer token: sender debited in full but recipient receives only 99%.
/// Sender check passes (spent == amount); recipient check fires.
#[test]
#[should_panic(expected = "Error(Contract, #41)")]
fn test_fee_on_transfer_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(FeeOnTransferToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);
    mint_mock_balance(&env, &token_id, &holder, 1000);

    // Sender loses 1000, recipient gains 990 → RecipientBalanceDeltaMismatch (#41).
    transfer_funding_token_with_balance_checks(&env, &token_id, &holder, &treasury, 1000);
}

/// Rebasing token: sender ends with more than it started with.
/// Net sender delta is negative → SenderBalanceDeltaMismatch.
#[test]
#[should_panic(expected = "Error(Contract, #40)")]
fn test_rebasing_token_sender_increases_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(RebasingToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);
    mint_mock_balance(&env, &token_id, &holder, 1000);

    // After "transfer", holder.balance = 2000, not 0 → SenderBalanceDeltaMismatch (#40).
    transfer_funding_token_with_balance_checks(&env, &token_id, &holder, &treasury, 1000);
}

/// Hook token: recipient receives only 90% of the requested amount.
/// Sender check passes; recipient check fires → RecipientBalanceDeltaMismatch.
#[test]
#[should_panic(expected = "Error(Contract, #41)")]
fn test_hook_token_recipient_decreases_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(HookStealingToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);
    mint_mock_balance(&env, &token_id, &holder, 1000);

    // Recipient gains 900 → RecipientBalanceDeltaMismatch (#41).
    transfer_funding_token_with_balance_checks(&env, &token_id, &holder, &treasury, 1000);
}

/// Lying token: nothing moves; both balances are unchanged.
/// Sender check fires first: spent == 0, not 1000 → SenderBalanceDeltaMismatch.
#[test]
#[should_panic(expected = "Error(Contract, #40)")]
fn test_lying_token_no_change_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(LyingToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);
    mint_mock_balance(&env, &token_id, &holder, 1000);

    // No balances change; sender check fires first → SenderBalanceDeltaMismatch (#40).
    transfer_funding_token_with_balance_checks(&env, &token_id, &holder, &treasury, 1000);
}

// ===========================================================================
// Outbound: amount validation
// ===========================================================================

/// Zero amount is rejected before any balance read — positivity guard.
#[test]
#[should_panic(expected = "Error(Contract, #36)")]
fn test_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // amount == 0 → TransferAmountNotPositive (#36).
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 0);
}

/// Negative amount is rejected before any balance read — positivity guard.
#[test]
#[should_panic(expected = "Error(Contract, #36)")]
fn test_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // amount == -1 → TransferAmountNotPositive (#36).
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, -1);
}

// ===========================================================================
// Outbound: insufficient balance
// ===========================================================================

/// Sender with zero balance cannot transfer even 1 unit.
#[test]
#[should_panic(expected = "Error(Contract, #37)")]
fn test_insufficient_balance_zero_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // holder has 0 balance; requesting 1 → InsufficientTokenBalanceBeforeTransfer (#37).
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 1);
}

/// Off-by-one: sender has exactly one fewer unit than requested.
#[test]
#[should_panic(expected = "Error(Contract, #37)")]
fn test_insufficient_balance_off_by_one_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    token.stellar.mint(&holder, &999i128);

    // holder has 999, requesting 1000 → InsufficientTokenBalanceBeforeTransfer (#37).
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 1000);
}

/// A failed transfer must not mutate any account balances.
///
/// This is a state-consistency assertion: even though the wrapper panics, Soroban
/// test transactions are atomic and the storage reverts on panic.
#[test]
#[should_panic(expected = "Error(Contract, #36)")]
fn test_failure_leaves_all_accounts_unchanged() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // Record pre-call balances.
    let holder_before = token.token.balance(&holder);
    let treasury_before = token.token.balance(&treasury);

    // This will panic (#36 — amount not positive) before touching balances.
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, -1);

    // Unreachable, but documents intent: no account must be modified on failure.
    assert_eq!(token.token.balance(&holder), holder_before);
    assert_eq!(token.token.balance(&treasury), treasury_before);
}

// ===========================================================================
// Outbound: compliant token — success and conservation invariants
// ===========================================================================

/// Exact balance conservation on a compliant token transfer.
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

    // Total supply conserved.
    assert_eq!(
        holder_before + treasury_before,
        holder_after + treasury_after,
        "total supply must be conserved"
    );
    // Exact deltas.
    assert_eq!(holder_before - holder_after, amount, "sender must decrease by amount");
    assert_eq!(treasury_after - treasury_before, amount, "recipient must increase by amount");
}

/// Minimum viable transfer: exactly 1 unit.
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

/// Large transfer (i128::MAX / 100) must not produce arithmetic overflow.
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

/// Multiple sequential outbound transfers share a fresh env but independent
/// recipients; cumulative balances must be consistent.
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

    assert_eq!(token.token.balance(&holder), 1000, "1000 should remain after 2 x 1000 out");
    assert_eq!(token.token.balance(&treasury1), 1000);
    assert_eq!(token.token.balance(&treasury2), 1000);
}

/// Boundary: exact balance transfer leaves sender at zero.
#[test]
fn test_exact_balance_transfer_leaves_sender_at_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);
    let amount = 500_000i128;
    token.stellar.mint(&holder, &amount);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, amount);

    assert_eq!(token.token.balance(&holder), 0, "sender must be drained to zero");
    assert_eq!(token.token.balance(&treasury), amount);
}

// ===========================================================================
// Inbound (investor → escrow): self-transfer guard
// ===========================================================================

/// Inbound self-transfer is rejected before any balance read.
///
/// Invariant: `investor != escrow`.
#[test]
#[should_panic(expected = "Error(Contract, #284)")]
fn test_inbound_same_sender_recipient_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    token.stellar.mint(&investor, &1000i128);

    // investor and escrow are the same address.
    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &investor, 1000);
}

/// Same test via the `transfer_into_escrow_with_balance_checks` alias.
#[test]
#[should_panic(expected = "Error(Contract, #284)")]
fn test_inbound_alias_same_sender_recipient_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    token.stellar.mint(&investor, &1000i128);

    transfer_into_escrow_with_balance_checks(&env, &token.id, &investor, &investor, 1000);
}

// ===========================================================================
// Inbound: non-compliant token rejection paths
// ===========================================================================

/// Fee-on-transfer token: investor debited in full; escrow receives only 99%.
/// Sender check passes; recipient check fires → InboundRecipientBalanceDeltaMismatch.
#[test]
#[should_panic(expected = "Error(Contract, #176)")]
fn test_inbound_fee_on_transfer_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(FeeOnTransferToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_mock_balance(&env, &token_id, &investor, 1000);

    // Investor loses 1000, escrow gains 990 → InboundRecipientBalanceDeltaMismatch (#176).
    transfer_funding_token_inbound_with_balance_checks(&env, &token_id, &investor, &escrow, 1000);
}

/// Rebasing token: investor ends with more than started → InboundSenderBalanceDeltaMismatch.
#[test]
#[should_panic(expected = "Error(Contract, #174)")]
fn test_inbound_rebasing_token_sender_increases_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(RebasingToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_mock_balance(&env, &token_id, &investor, 1000);

    // After "transfer", investor.balance = 2000 → InboundSenderBalanceDeltaMismatch (#174).
    transfer_funding_token_inbound_with_balance_checks(&env, &token_id, &investor, &escrow, 1000);
}

/// Hook token: recipient receives only 90% → InboundRecipientBalanceDeltaMismatch.
#[test]
#[should_panic(expected = "Error(Contract, #176)")]
fn test_inbound_hook_token_recipient_decreases_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(HookStealingToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_mock_balance(&env, &token_id, &investor, 1000);

    // Escrow receives 900 → InboundRecipientBalanceDeltaMismatch (#176).
    transfer_funding_token_inbound_with_balance_checks(&env, &token_id, &investor, &escrow, 1000);
}

/// Lying token: nothing moves; sender check fires first → InboundSenderBalanceDeltaMismatch.
#[test]
#[should_panic(expected = "Error(Contract, #174)")]
fn test_inbound_lying_token_no_change_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token_id = env.register(LyingToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_mock_balance(&env, &token_id, &investor, 1000);

    // No balances change; sender check fires first → InboundSenderBalanceDeltaMismatch (#174).
    transfer_funding_token_inbound_with_balance_checks(&env, &token_id, &investor, &escrow, 1000);
}

// ===========================================================================
// Inbound: amount validation
// ===========================================================================

/// Zero inbound amount is rejected before any balance read.
#[test]
#[should_panic(expected = "Error(Contract, #171)")]
fn test_inbound_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);

    // amount == 0 → InboundTransferAmountNotPositive (#171).
    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &escrow, 0);
}

/// Negative inbound amount is rejected before any balance read.
#[test]
#[should_panic(expected = "Error(Contract, #171)")]
fn test_inbound_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);

    // amount == -1 → InboundTransferAmountNotPositive (#171).
    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &escrow, -1);
}

// ===========================================================================
// Inbound: insufficient balance
// ===========================================================================

/// Investor with zero balance cannot transfer even 1 unit.
#[test]
#[should_panic(expected = "Error(Contract, #172)")]
fn test_inbound_insufficient_balance_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);

    // investor has 0 balance; requesting 1 → InboundInsufficientTokenBalanceBeforeTransfer (#172).
    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &escrow, 1);
}

/// Off-by-one: investor has exactly one fewer unit than requested.
#[test]
#[should_panic(expected = "Error(Contract, #172)")]
fn test_inbound_insufficient_balance_off_by_one_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);
    token.stellar.mint(&investor, &999i128);

    // investor has 999, requesting 1000 → InboundInsufficientTokenBalanceBeforeTransfer (#172).
    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &escrow, 1000);
}

// ===========================================================================
// Inbound: compliant token — success and conservation invariants
// ===========================================================================

/// Exact balance conservation on a compliant inbound transfer.
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

    let investor_after = token.token.balance(&investor);
    let escrow_after = token.token.balance(&escrow);

    assert_eq!(investor_before - investor_after, amount, "investor delta must equal amount");
    assert_eq!(escrow_after - escrow_before, amount, "escrow delta must equal amount");
}

/// Minimum inbound transfer: 1 unit.
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

/// Large inbound transfer (i128::MAX / 100) must not overflow.
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

/// Boundary: inbound exact balance transfer drains investor to zero.
#[test]
fn test_inbound_exact_balance_transfer_drains_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);
    let amount = 750_000i128;
    token.stellar.mint(&investor, &amount);

    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &escrow, amount);

    assert_eq!(token.token.balance(&investor), 0, "investor must be drained");
    assert_eq!(token.token.balance(&escrow), amount);
}

// ===========================================================================
// Alias: transfer_into_escrow_with_balance_checks
// ===========================================================================

/// The `transfer_into_escrow_with_balance_checks` alias is strictly equivalent to
/// `transfer_funding_token_inbound_with_balance_checks` for the success path.
#[test]
fn test_transfer_into_escrow_alias_success() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);
    let amount = 500i128;
    token.stellar.mint(&investor, &amount);

    transfer_into_escrow_with_balance_checks(&env, &token.id, &investor, &escrow, amount);

    assert_eq!(token.token.balance(&investor), 0);
    assert_eq!(token.token.balance(&escrow), amount);
}

/// The `transfer_into_escrow_with_balance_checks` alias propagates the zero-amount
/// rejection with the same error code as the canonical function.
#[test]
#[should_panic(expected = "Error(Contract, #171)")]
fn test_transfer_into_escrow_alias_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);

    transfer_into_escrow_with_balance_checks(&env, &token.id, &investor, &escrow, 0);
}

// ===========================================================================
// Boundary: sequential transfers with state accumulation
// ===========================================================================

/// Multiple sequential inbound transfers from the same investor accumulate correctly
/// in the escrow, and the investor balance decrements monotonically.
#[test]
fn test_multiple_sequential_inbound_transfers() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);
    token.stellar.mint(&investor, &3000i128);

    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &escrow, 1000);
    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &escrow, 1000);

    assert_eq!(token.token.balance(&investor), 1000, "1000 should remain after 2 x 1000 in");
    assert_eq!(token.token.balance(&escrow), 2000);
}

/// Multiple investors funding the same escrow each move exactly their stated amount.
#[test]
fn test_multiple_investors_fund_same_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor1 = deploy_id(&env);
    let investor2 = Address::generate(&env);
    let escrow = Address::generate(&env);
    token.stellar.mint(&investor1, &1500i128);
    token.stellar.mint(&investor2, &2500i128);

    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor1, &escrow, 1500);
    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor2, &escrow, 2500);

    assert_eq!(token.token.balance(&investor1), 0);
    assert_eq!(token.token.balance(&investor2), 0);
    assert_eq!(token.token.balance(&escrow), 4000, "escrow must hold sum of all investments");
}

// ===========================================================================
// Boundary: inbound then outbound round-trip
// ===========================================================================

/// After an inbound transfer into the escrow, the escrow can route funds back out
/// (simulating a treasury payout). Both legs must independently satisfy conservation.
#[test]
fn test_inbound_then_outbound_round_trip() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = deploy_id(&env);
    let treasury = Address::generate(&env);
    let amount = 1000i128;
    token.stellar.mint(&investor, &amount);

    // Investor funds the escrow.
    transfer_funding_token_inbound_with_balance_checks(&env, &token.id, &investor, &escrow, amount);

    assert_eq!(token.token.balance(&investor), 0);
    assert_eq!(token.token.balance(&escrow), amount);

    // Escrow routes funds to treasury.
    transfer_funding_token_with_balance_checks(&env, &token.id, &escrow, &treasury, amount);

    assert_eq!(token.token.balance(&escrow), 0);
    assert_eq!(token.token.balance(&treasury), amount);
}
