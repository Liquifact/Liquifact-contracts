//! Hardened wrappers around cross-contract calls used by this escrow.
//!
//! This crate only performs **token** transfers on the address stored under
//! [`crate::DataKey::FundingToken`] after initialization. That address must be a **standard**
//! [SEP-41](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md)-style
//! token with no fee-on-transfer or balance-deficit behavior: post-transfer balance **deltas** must
//! match the requested `amount` exactly on both sides.

//! ## Balance-delta invariants
//!
//! All transfers enforce strict pre/post balance checks to ensure mathematical conservation of value:
//! - **Sender**: balance must decrease by exactly `amount`
//! - **Recipient**: balance must increase by exactly `amount`
//! - **Muxed mapping**: recipient address is wrapped in [`MuxedAddress`] for Stellar compatibility
//! - **Safe failure**: any deviation causes immediate panic with descriptive error message
//!
//! The invariants are enforced through atomic balance verification:
//! 1. Capture pre-transfer balances for both parties
//! 2. Execute the transfer using standard SEP-41 interface
//! 3. Capture post-transfer balances and calculate exact deltas
//! 4. Assert mathematical equality: `sender_delta == recipient_delta == amount`
//!
//! ## Test reality and verification
//!
//! The test suite validates these invariants through:
//! - Standard token transfers with exact delta verification
//! - Edge cases including zero/negative amounts and insufficient balance
//! - Multiple transfer scenarios to ensure cumulative consistency
//! - Mocked token scenarios (where feasible) to detect divergence
//!
//! ## Out-of-scope token economics
//!
//! Malicious, rebasing, or "hook" tokens are **explicitly out of scope** and will cause safe-failure
//! panics at the balance-check boundary. If such tokens bypass these checks, they must be excluded
//! by governance allowlists and integration review. Fee-on-transfer tokens are not supported.
//!
//! Specifically excluded:
//! - Tokens with transfer fees (fee-on-transfer)
//! - Rebasing tokens that change total supply
//! - Tokens with hooks or callbacks that modify balances
//! - Tokens with non-standard balance accounting
//!
//! ## Governance allowlists
//!
//! Integration review and governance allowlists are the primary defense mechanisms against
//! out-of-scope token economics. The balance-delta checks serve as a technical safety net,
//! but proper token selection through governance processes remains essential.
//!
//! # Soroban execution and "reentrancy"
//!
//! Unlike many EVM environments, Soroban does not allow the classic pattern of an external call
//! immediately re-entering the same contract mid-host-function in an interleaved way: the token
//! host function runs to completion before this contract resumes. **Still** treat the token as
//! adversarial for **correctness of balances**: always record pre/post balances around transfers so
//! integration bugs and non-compliant tokens are caught at the host boundary.
//!
//! ## Reviewer timeline (host-call boundary)
//!
//! `transfer_funding_token_with_balance_checks` follows this sequence:
//! 1. Read sender/recipient balances before transfer.
//! 2. Invoke SEP-41 `transfer` on the configured token contract.
//! 3. Soroban host executes that token call to completion, then returns.
//! 4. Read sender/recipient balances after transfer.
//! 5. Assert exact conservation (`spent == amount` and `received == amount`).
//!
//! Security takeaway: this is not relying on "non-reentrancy" as a magic property. It enforces
//! post-call accounting invariants at the external-call boundary where token behavior is observed.

use crate::{ensure, fail, EscrowError};
use soroban_sdk::{token::TokenClient, Address, Env, MuxedAddress};

/// Transfer `amount` of `token_addr` from `from` (typically this escrow contract) to `treasury`,
/// then verify SEP-41-style conservation: sender decreases and recipient increases by exactly
/// `amount`.
///
/// This function performs strict balance-delta verification through atomic balance checks:
/// 1. Records pre-transfer balances for both sender and recipient
/// 2. Executes transfer using [`MuxedAddress::from`] for Stellar compatibility
/// 3. Records post-transfer balances and calculates exact deltas
/// 4. Asserts mathematical equality: `sender_delta == recipient_delta == amount`
///
/// The invariants enforced ensure mathematical conservation of value and detect:
/// - Fee-on-transfer tokens (sender delta > amount)
/// - Rebasing/malicious tokens (recipient delta != amount)
/// - Balance manipulation or integration bugs
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `token_addr` - Address of the SEP-41 token contract
/// * `from` - Address transferring from (usually this escrow contract)
/// * `treasury` - Address receiving the tokens
/// * `amount` - Amount to transfer (must be positive)
///
/// # Errors
///
/// Emits typed [`EscrowError`] codes if `amount` is not positive, sender balance is insufficient,
/// balance deltas do not equal `amount`, or balance delta calculation underflows.
///
/// # Security Considerations
///
/// This function assumes the token contract follows standard SEP-41 semantics without
/// fee-on-transfer, rebasing, or hook behaviors. Non-compliant tokens will cause this
/// function to fail with a typed error, serving as a safety boundary. Such tokens should be
/// excluded through governance allowlists and integration review processes.
pub fn transfer_funding_token_with_balance_checks(
    env: &Env,
    token_addr: &Address,
    from: &Address,
    treasury: &Address,
    amount: i128,
) {
    ensure(env, amount > 0, EscrowError::TransferAmountNotPositive);
    let token = TokenClient::new(env, token_addr);
    let from_before = token.balance(from);
    let treasury_before = token.balance(treasury);
    ensure(
        env,
        from_before >= amount,
        EscrowError::InsufficientTokenBalanceBeforeTransfer,
    );

    token.transfer(from, MuxedAddress::from(treasury.clone()), &amount);

    let from_after = token.balance(from);
    let treasury_after = token.balance(treasury);

    let spent = from_before
        .checked_sub(from_after)
        .unwrap_or_else(|| fail(env, EscrowError::SenderBalanceUnderflow));
    let received = treasury_after
        .checked_sub(treasury_before)
        .unwrap_or_else(|| fail(env, EscrowError::RecipientBalanceUnderflow));

    ensure(
        env,
        spent == amount,
        EscrowError::SenderBalanceDeltaMismatch,
    );
    ensure(
        env,
        received == amount,
        EscrowError::RecipientBalanceDeltaMismatch,
    );
}

/// Transfer `amount` of `token_addr` from `investor` to `to` (typically this escrow contract),
/// then verify SEP-41-style conservation: sender decreases and recipient increases by exactly
/// `amount`.
///
/// This function performs strict balance-delta verification through atomic balance checks:
/// 1. Records pre-transfer balances for both investor and contract
/// 2. Executes transfer using [`MuxedAddress::from`] for Stellar compatibility
/// 3. Records post-transfer balances and calculates exact deltas
/// 4. Asserts mathematical equality: `sender_delta == recipient_delta == amount`
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `token_addr` - Address of the SEP-41 token contract
/// * `investor` - Address transferring from (the investor)
/// * `to` - Address receiving the tokens (usually this escrow contract)
/// * `amount` - Amount to transfer (must be positive)
///
/// # Errors
///
/// Emits typed [`EscrowError`] codes if `amount` is not positive, investor balance is insufficient,
/// balance deltas do not equal `amount`, or balance delta calculation underflows.
pub fn transfer_funding_token_inbound_with_balance_checks(
    env: &Env,
    token_addr: &Address,
    investor: &Address,
    to: &Address,
    amount: i128,
) {
    ensure(
        env,
        amount > 0,
        EscrowError::InboundTransferAmountNotPositive,
    );
    let token = TokenClient::new(env, token_addr);
    let investor_before = token.balance(investor);
    let contract_before = token.balance(to);
    ensure(
        env,
        investor_before >= amount,
        EscrowError::InboundInsufficientTokenBalanceBeforeTransfer,
    );

    token.transfer(investor, MuxedAddress::from(to.clone()), &amount);

    let investor_after = token.balance(investor);
    let contract_after = token.balance(to);

    let spent = investor_before
        .checked_sub(investor_after)
        .unwrap_or_else(|| fail(env, EscrowError::InboundSenderBalanceUnderflow));
    let received = contract_after
        .checked_sub(contract_before)
        .unwrap_or_else(|| fail(env, EscrowError::InboundRecipientBalanceUnderflow));

    ensure(
        env,
        spent == amount,
        EscrowError::InboundSenderBalanceDeltaMismatch,
    );
    ensure(
        env,
        received == amount,
        EscrowError::InboundRecipientBalanceDeltaMismatch,
    );
}
