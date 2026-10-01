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
//! ## Additional state invariants enforced (Issue #1257)
//!
//! Beyond balance-delta checks, the following structural invariants are enforced **before** any
//! SEP-41 transfer executes, preventing a class of silent-integrity bugs:
//!
//! **INVARIANT 1 — Distinct sender and recipient:**
//! `from != to` (outbound) and `investor != to` (inbound). A self-transfer circumvents the
//! balance-delta conservation model: the same address would appear as both sides, so any
//! balance mutation could be "explained away" while no net movement actually occurs.
//! Enforced with [`EscrowError::TransferSameSenderRecipient`] /
//! [`EscrowError::InboundTransferSameSenderRecipient`].
//!
//! **INVARIANT 2 — Amount positivity:**
//! `amount > 0` is verified before any balance read, so the downstream `checked_sub` deltas
//! never degenerate to zero or negative. Enforced with [`EscrowError::TransferAmountNotPositive`]
//! / [`EscrowError::InboundTransferAmountNotPositive`].
//!
//! **INVARIANT 3 — Sufficient sender balance pre-transfer:**
//! `sender_before >= amount` is asserted against the actual SEP-41 balance so a transfer that
//! would fail inside the token host (and possibly leave state partially mutated on a
//! non-compliant token) is short-circuited here.
//!
//! **INVARIANT 4 — Deterministic underflow-free deltas:**
//! `spent = from_before.checked_sub(from_after)` and
//! `received = to_after.checked_sub(to_before)` must be `Some(...)`. An underflow indicates
//! the token's balance model is non-monotonic (rebasing / hooking) and such tokens are
//! explicitly out of scope.
//!
//! **INVARIANT 5 — Exact conservation:**
//! `spent == amount && received == amount`. Any delta mismatch is a SEP-41 deviation:
//! fee-on-transfer, rebasing, or an integration bug that miscounted balances. Fail hard.
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
//! 1. INVARIANT: assert sender != recipient (self-transfer guard).
//! 2. INVARIANT: assert amount > 0 (positivity guard).
//! 3. Read sender/recipient balances before transfer.
//! 4. INVARIANT: assert sender balance >= amount (sufficiency guard).
//! 5. Invoke SEP-41 `transfer` on the configured token contract.
//! 6. Soroban host executes that token call to completion, then returns.
//! 7. Read sender/recipient balances after transfer.
//! 8. Compute deltas via checked_sub — underflow ⇒ invariant violation.
//! 9. INVARIANT: assert exact conservation (`spent == amount` and `received == amount`).
//!
//! Security takeaway: this is not relying on "non-reentrancy" as a magic property. It enforces
//! post-call accounting invariants at the external-call boundary where token behavior is observed.
//!
//! ## Retries, partial failure and concurrency (Issue #1382)
//!
//! * **Atomic failure:** every guard fails through [`crate::fail`] (a typed contract error), which
//!   aborts the invocation. Soroban reverts all storage writes and token movements of an aborted
//!   invocation, so a rejected transfer never leaves a partially-applied state behind.
//! * **Deterministic retries:** the helpers keep no state of their own. A rejected call can be
//!   retried with corrected inputs and is evaluated exactly like a first attempt; identical inputs
//!   against identical balances always produce the identical result or error code.
//! * **Concurrency:** Soroban executes a transaction's invocations sequentially and the host rejects
//!   re-entry into a contract that is already on the call stack, so a token callback cannot
//!   interleave with an in-progress transfer leg. Per-leg in-flight/nonce bookkeeping would be
//!   unreachable here and is intentionally not kept (it also cannot run outside a contract context,
//!   where these helpers are unit-tested).
//! * **Diagnosability:** each guard has its own [`EscrowError`] code, so a failure identifies the
//!   exact violated invariant without exposing balances or addresses.

use crate::{ensure, fail, EscrowError};
use soroban_sdk::{token::TokenClient, Address, Env, MuxedAddress};

/// Transfer `amount` of `token_addr` from `from` (typically this escrow contract) to `treasury`,
/// then verify SEP-41-style conservation: sender decreases and recipient increases by exactly
/// `amount`.
///
/// Guards run in this order, and each fails with its own [`EscrowError`] before any later step:
///
/// | # | Check | Error |
/// |---|-------|-------|
/// | 1 | `from != treasury` | [`EscrowError::TransferSameSenderRecipient`] |
/// | 2 | `amount > 0` | [`EscrowError::TransferAmountNotPositive`] |
/// | 3 | sender balance `>= amount` (before the transfer) | [`EscrowError::InsufficientTokenBalanceBeforeTransfer`] |
/// | 4 | sender delta does not underflow | [`EscrowError::SenderBalanceUnderflow`] |
/// | 4 | recipient delta does not underflow | [`EscrowError::RecipientBalanceUnderflow`] |
/// | 5 | sender delta `== amount` | [`EscrowError::SenderBalanceDeltaMismatch`] |
/// | 5 | recipient delta `== amount` | [`EscrowError::RecipientBalanceDeltaMismatch`] |
///
/// Guards 1-3 run before the token is called, so no token state is touched on those paths.
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
    // INVARIANT 1: a self-transfer would make both balance deltas zero-sum on one address.
    ensure(
        env,
        from != treasury,
        EscrowError::TransferSameSenderRecipient,
    );
    // INVARIANT 2: positivity is checked before any balance read.
    ensure(env, amount > 0, EscrowError::TransferAmountNotPositive);

    let token = TokenClient::new(env, token_addr);
    let from_before = token.balance(from);
    let treasury_before = token.balance(treasury);
    // INVARIANT 3: short-circuit before the token call when the sender cannot cover it.
    ensure(
        env,
        from_before >= amount,
        EscrowError::InsufficientTokenBalanceBeforeTransfer,
    );

    token.transfer(from, MuxedAddress::from(treasury.clone()), &amount);

    let from_after = token.balance(from);
    let treasury_after = token.balance(treasury);

    // INVARIANT 4: deltas must not underflow (non-monotonic balance model).
    let spent = from_before
        .checked_sub(from_after)
        .unwrap_or_else(|| fail(env, EscrowError::SenderBalanceUnderflow));
    let received = treasury_after
        .checked_sub(treasury_before)
        .unwrap_or_else(|| fail(env, EscrowError::RecipientBalanceUnderflow));

    // INVARIANT 5: exact conservation on both sides.
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
/// Same guard order as [`transfer_funding_token_with_balance_checks`], with the inbound error
/// codes:
///
/// | # | Check | Error |
/// |---|-------|-------|
/// | 1 | `investor != to` | [`EscrowError::InboundTransferSameSenderRecipient`] |
/// | 2 | `amount > 0` | [`EscrowError::InboundTransferAmountNotPositive`] |
/// | 3 | investor balance `>= amount` (before the transfer) | [`EscrowError::InboundInsufficientTokenBalanceBeforeTransfer`] |
/// | 4 | investor delta does not underflow | [`EscrowError::InboundSenderBalanceUnderflow`] |
/// | 4 | recipient delta does not underflow | [`EscrowError::InboundRecipientBalanceUnderflow`] |
/// | 5 | investor delta `== amount` | [`EscrowError::InboundSenderBalanceDeltaMismatch`] |
/// | 5 | recipient delta `== amount` | [`EscrowError::InboundRecipientBalanceDeltaMismatch`] |
pub fn transfer_funding_token_inbound_with_balance_checks(
    env: &Env,
    token_addr: &Address,
    investor: &Address,
    to: &Address,
    amount: i128,
) {
    ensure(
        env,
        investor != to,
        EscrowError::InboundTransferSameSenderRecipient,
    );
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

pub use transfer_funding_token_inbound_with_balance_checks as transfer_into_escrow_with_balance_checks;
