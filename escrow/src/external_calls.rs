/// Hardened wrappers around cross-contract calls used by this escrow.
///
/// This crate only performs **token** transfers on the address stored under
/// [`crate::DataKey::FundingToken`] after initialization. That address must be a **standard**
/// [SEP-41](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md)-style
/// token with no fee-on-transfer or balance-deficit behavior: post-transfer balance **deltas** must
/// match the requested `amount` exactly on both sides.

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
//! ## Additional state invariants enforced
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
//! ## Out-of-scope token economics
//!
//! Malicious, rebasing, or "hook" tokens are **explicitly out of scope** and will cause safe-failure
//! panics at the balance-check boundary. If such tokens bypass these checks, they must be excluded
//! by governance allowlists and integration review. Fee-on-transfer tokens are not supported.
//!
//! ## Check order
//!
//! Both wrappers enforce checks in this sequence:
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

use crate::{ensure, fail, EscrowError};
use soroban_sdk::{token::TokenClient, Address, Env, MuxedAddress};

/// Transfer `amount` of `token_addr` from `from` (typically this escrow contract) to `treasury`,
/// then verify SEP-41-style conservation: sender decreases and recipient increases by exactly
/// `amount`.
///
/// ## Check order
///
/// 1. Assert `from != treasury` — prevents self-transfer that would spoof balance-delta accounting.
/// 2. Assert `amount > 0` — positivity guard before any balance read.
/// 3. Assert `sender_before >= amount` — pre-flight sufficiency check.
/// 4. Execute SEP-41 `transfer`.
/// 5. Assert `spent == amount` — sender-side conservation.
/// 6. Assert `received == amount` — recipient-side conservation.
///
/// # Errors
///
/// Emits typed [`EscrowError`] codes:
/// - [`EscrowError::TransferSameSenderRecipient`] if `from == treasury`.
/// - [`EscrowError::TransferAmountNotPositive`] if `amount <= 0`.
/// - [`EscrowError::InsufficientTokenBalanceBeforeTransfer`] if sender lacks funds.
/// - [`EscrowError::SenderBalanceUnderflow`] if sender balance delta underflows.
/// - [`EscrowError::SenderBalanceDeltaMismatch`] if sender delta ≠ `amount`.
/// - [`EscrowError::RecipientBalanceUnderflow`] if recipient balance delta underflows.
/// - [`EscrowError::RecipientBalanceDeltaMismatch`] if recipient delta ≠ `amount`.
pub fn transfer_funding_token_with_balance_checks(
    env: &Env,
    token_addr: &Address,
    from: &Address,
    treasury: &Address,
    amount: i128,
) {
    // INVARIANT 1: distinct sender and recipient — self-transfer spoofs conservation.
    ensure(
        env,
        from != treasury,
        EscrowError::TransferSameSenderRecipient,
    );
    // INVARIANT 2: amount must be strictly positive.
    ensure(env, amount > 0, EscrowError::TransferAmountNotPositive);

    let token = TokenClient::new(env, token_addr);
    let from_before = token.balance(from);
    let treasury_before = token.balance(treasury);

    // INVARIANT 3: sender must hold at least `amount` before the transfer.
    ensure(
        env,
        from_before >= amount,
        EscrowError::InsufficientTokenBalanceBeforeTransfer,
    );

    token.transfer(from, &MuxedAddress::from(treasury.clone()), &amount);

    let from_after = token.balance(from);
    let treasury_after = token.balance(treasury);

    // INVARIANT 4 + 5: sender delta must equal `amount` exactly (no rebasing or hooks).
    let spent = from_before
        .checked_sub(from_after)
        .unwrap_or_else(|| fail(env, EscrowError::SenderBalanceUnderflow));
    ensure(
        env,
        spent == amount,
        EscrowError::SenderBalanceDeltaMismatch,
    );

    // INVARIANT 4 + 5: recipient delta must equal `amount` exactly (no fee-on-transfer).
    let received = treasury_after
        .checked_sub(treasury_before)
        .unwrap_or_else(|| fail(env, EscrowError::RecipientBalanceUnderflow));
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
/// ## Check order
///
/// 1. Assert `investor != to` — prevents self-transfer that would spoof balance-delta accounting.
/// 2. Assert `amount > 0` — positivity guard before any balance read.
/// 3. Assert `sender_before >= amount` — pre-flight sufficiency check.
/// 4. Execute SEP-41 `transfer`.
/// 5. Assert `spent == amount` — sender-side conservation.
/// 6. Assert `received == amount` — recipient-side conservation.
///
/// # Errors
///
/// Emits typed [`EscrowError`] codes:
/// - [`EscrowError::InboundTransferSameSenderRecipient`] if `investor == to`.
/// - [`EscrowError::InboundTransferAmountNotPositive`] if `amount <= 0`.
/// - [`EscrowError::InboundInsufficientTokenBalanceBeforeTransfer`] if investor lacks funds.
/// - [`EscrowError::InboundSenderBalanceUnderflow`] if investor balance delta underflows.
/// - [`EscrowError::InboundSenderBalanceDeltaMismatch`] if investor delta ≠ `amount`.
/// - [`EscrowError::InboundRecipientBalanceUnderflow`] if recipient balance delta underflows.
/// - [`EscrowError::InboundRecipientBalanceDeltaMismatch`] if recipient delta ≠ `amount`.
pub fn transfer_funding_token_inbound_with_balance_checks(
    env: &Env,
    token_addr: &Address,
    investor: &Address,
    to: &Address,
    amount: i128,
) {
    // INVARIANT 1: distinct sender and recipient — self-transfer spoofs conservation.
    ensure(
        env,
        investor != to,
        EscrowError::InboundTransferSameSenderRecipient,
    );
    // INVARIANT 2: amount must be strictly positive.
    ensure(
        env,
        amount > 0,
        EscrowError::InboundTransferAmountNotPositive,
    );

    let token = TokenClient::new(env, token_addr);
    let investor_before = token.balance(investor);
    let contract_before = token.balance(to);

    // INVARIANT 3: investor must hold at least `amount` before the transfer.
    ensure(
        env,
        investor_before >= amount,
        EscrowError::InboundInsufficientTokenBalanceBeforeTransfer,
    );

    token.transfer(investor, &MuxedAddress::from(to.clone()), &amount);

    let investor_after = token.balance(investor);
    let contract_after = token.balance(to);

    // INVARIANT 4 + 5: sender delta must equal `amount` exactly.
    let spent = investor_before
        .checked_sub(investor_after)
        .unwrap_or_else(|| fail(env, EscrowError::InboundSenderBalanceUnderflow));
    ensure(
        env,
        spent == amount,
        EscrowError::InboundSenderBalanceDeltaMismatch,
    );

    // INVARIANT 4 + 5: recipient delta must equal `amount` exactly.
    let received = contract_after
        .checked_sub(contract_before)
        .unwrap_or_else(|| fail(env, EscrowError::InboundRecipientBalanceUnderflow));
    ensure(
        env,
        received == amount,
        EscrowError::InboundRecipientBalanceDeltaMismatch,
    );
}

/// Alias: `transfer_funding_token_inbound_with_balance_checks` under the inbound-leg name.
/// Used in tests that phrase the operation as "transfer into escrow".
pub use transfer_funding_token_inbound_with_balance_checks as transfer_into_escrow_with_balance_checks;
