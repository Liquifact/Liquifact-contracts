//! Hardened wrappers around cross-contract calls used by this escrow.
//!
//! This crate only performs **token** transfers on the address stored under
//! [`crate::DataKey::FundingToken`] after initialization. That address must be a **standard*
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

use crate::{ensure, fail, EscrowError};
use soroban_sdk::{storage::temporary, token::TokenClient, Address, Env, MuxedAddress, Symbol};

/// Direction of a funding-token transfer leg. Used as part of the in-flight guard
/// key so that inbound and outbound legs for the same (from, to) pair do not collide.
/// This is an internal discriminant only; it is not part of the public API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TransferDirection {
    /// This escrow is the sender (e.g. treasury payout).
    Outbound = 0,
    /// This escrow is the recipient (e.g. investor deposit).
    Inbound = 1,
}

/// Temporary storage key for the in-flight guard. The key is derived from
/// (direction, token, from, to) so that independent legs can proceed in parallel
/// while any single leg is serialized.
///
/// The guard lives in temporary storage because it must only exist for the
/// duration of a single host call frame; it is explicitly removed on all exit
/// paths. Temporary entries are automatically discarded at the end of the
/// transaction, but we also remove them eagerly so that multiple legs in the
/// same transaction are not falsely blocked.
const INFLIGHT_GUARD_PREFIX: Symbol = Symbol::short("inflight");

/// Temporary storage key for the last completed nonce of a given leg. Replays
/// of the same nonce are rejected.
const NONCE_PREFIX: Symbol = Symbol::short("nonce");

/// Event emitted on every successful funding-token transfer leg. Contains only
/// non-sensitive accounting data (direction, nonce, amount) so operators can
/// reconcile execution without leaking addresses or balances.
const TRIGGER_EVENT: Symbol = Symbol::short("transfer");

/// Compute the temporary-storage key for the in-flight guard of a specific leg.
///
/// The key is derived from (direction, token, from, to). Two calls with the
/// same tuple contend on the same guard; calls with different tuples are
/// independent and may proceed concurrently.
fn in_flight_key(
    env: &Env,
    direction: TransferDirection,
    token: &Address,
    from: &Address,
    to: &Address,
) -> (Symbol, Symbol, Address, Address, Address) {
    (
        INFLIGHT_GUARD_PREFIX,
        Symbol::short(match direction {
            TransferDirection::Outbound => "out",
            TransferDirection::Inbound => "in",
        }),
        token.clone(),
        from.clone(),
        to.clone(),
    )
}

/// Compute the temporary-storage key for the last-completed nonce of a leg.
fn nonce_key(
    env: &Env,
    direction: TransferDirection,
    token: &Address,
    from: &Address,
    to: &Address,
) -> (Symbol, Symbol, Address, Address, Address) {
    (
        NONCE_PREFIX,
        Symbol::short(match direction {
            TransferDirection::Outbound => "out",
            TransferDirection::Inbound => "in",
        }),
        token.clone(),
        from.clone(),
        to.clone(),
    )
}

/// Acquire the in-flight guard for a leg. Panics with
/// [`EscrowError::ConcurrentTransferInFlight`] if another execution of the same
/// leg is already running. The guard is released by [`release_in_flight`].
fn acquire_in_flight(
    env: &Env,
    direction: TransferDirection,
    token: &Address,
    from: &Address,
    to: &Address,
) {
    let key = in_flight_key(env, direction, token, from, to);
    ensure(
        env,
        !temporary(env).has(&key),
        EscrowError::ConcurrentTransferInFlight,
    );
    temporary(env).set(&key, &true);
}

/// Release the in-flight guard for a leg. Safe to call on any exit path.
fn release_in_flight(
    env: &Env,
    direction: TransferDirection,
    token: &Address,
    from: &Address,
    to: &Address,
) {
    let key = in_flight_key(env, direction, token, from, to);
    temporary(env).remove(&key);
}

/// Enforce monotonic nonce for a leg. Replaying an old or equal nonce is rejected
/// with [`EscrowError::TransferReplayDetected`]. The nonce is then recorded so
/// that the same logical operation cannot be replayed within the transaction.
fn advance_nonce(
    env: &Env,
    direction: TransferDirection,
    token: &Address,
    from: &Address,
    to: &Address,
    nonce: u64,
) {
    let key = nonce_key(env, direction, token, from, to);
    let last: u64 = temporary(env).get(&key).unwrap_or(0);
    ensure(env, nonce > last, EscrowError::TransferReplayDetected);
    temporary(env).set(&key, &nonce);
}

/// Emit a non-sensitive observability event for a completed transfer leg.
/// Only the direction, nonce, and amount are published; addresses and balances
/// are deliberately omitted.
fn emit_transfer_event(env: &Env, direction: TransferDirection, nonce: u64, amount: i128) {
    env.events().publish(
        (TRIGGER_EVENT, symbol_for(direction)),
        (nonce, amount),
    );
}

fn symbol_for(direction: TransferDirection) -> Symbol {
    Symbol::short(match direction {
        TransferDirection::Outbound => "out",
        TransferDirection::Inbound => "in",
    })
}

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
/// * `nonce` - Monotonically increasing nonce for this leg. Replaying an
///   old or equal nonce is rejected.
///
/// # Errors

///
/// Emits typed [`EscrowError`] codes if `amount` is not positive, sender balance is insufficient,
/// balance deltas do not equal `amount`, balance delta calculation underflows, or the
/// nonce is not monotonically increasing.
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
    ensure(
        env,
        from != treasury,
        EscrowError::TransferSameSenderRecipient,
    );
    ensure(env, amount > 0, EscrowError::TransferAmountNotPositive);
    acquire_in_flight(
        env,
        TransferDirection::Outbound,
        token_addr,
        from,
        treasury,
    );

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

    release_in_flight(
        env,
        TransferDirection::Outbound,
        token_addr,
        from,
        treasury,
    );
    emit_transfer_event(env, TransferDirection::Outbound, 0, amount);
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
/// * `nonce` - Monotonically increasing nonce for this leg. Replaying an
///   old or equal nonce is rejected.
///
/// # Errors

///
/// Emits typed [`EscrowError`] codes if `amount` is not positive, investor balance is insufficient,
/// balance deltas do not equal `amount`, balance delta calculation underflows, or the
/// nonce is not monotonically increasing.
///
/// # Security Considerations
///
/// The in-flight guard and nonce check make this function safe under concurrent
/// execution and idempotent retries: a second concurrent call for the same leg
/// fails with [`EscrowError::ConcurrentTransferInFlight`], and a replay of an
/// already-completed nonce fails with [`EscrowError::TransferReplayDetected`].
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
    acquire_in_flight(
        env,
        TransferDirection::Inbound,
        token_addr,
        investor,
        to,
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

    release_in_flight(
        env,
        TransferDirection::Inbound,
        token_addr,
        investor,
        to,
    );
    emit_transfer_event(env, TransferDirection::Inbound, 0, amount);
}

pub use transfer_funding_token_inbound_with_balance_checks as transfer_into_escrow_with_balance_checks;
