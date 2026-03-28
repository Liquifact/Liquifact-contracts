//! Hardened wrappers around cross-contract calls used by this escrow.
//!
//! # Trusted external contracts (trust list)
//!
//! This crate only performs **token** transfers on the address stored under
//! [`crate::DataKey::FundingToken`] after initialization. That address must be a **standard**
//! [SEP-41](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md)-style
//! token with no fee-on-transfer or balance-deficit behavior: post-transfer balance **deltas** must
//! match the requested `amount` exactly on both sides. Malicious, rebasing, or “hook” tokens are
//! **out of scope** and may cause assertions to fail (safe failure) or, if they bypass checks, must
//! be excluded by governance and integration review.
//!
//! # Soroban execution and “reentrancy”
//!
//! Unlike many EVM environments, Soroban does not allow the classic pattern of an external call
//! immediately re-entering the same contract mid-host-function in an interleaved way: the token
//! host function runs to completion before this contract resumes. **Still** treat the token as
//! adversarial for **correctness of balances**: always record pre/post balances around transfers so
//! integration bugs and non-compliant tokens are caught at the host boundary.

use soroban_sdk::{token::TokenClient, Address, Env, MuxedAddress};

/// Transfer `amount` of `token_addr` from `from` (typically this escrow contract) to `treasury`,
/// then verify SEP-41-style conservation: sender decreases and recipient increases by exactly
/// `amount`.
///
/// # Panics
///
/// If balances do not move as expected (wrong token, malicious implementation, or unsupported
/// token economics such as fees taken from transferred amount).
pub fn transfer_funding_token_with_balance_checks(
    env: &Env,
    token_addr: &Address,
    from: &Address,
    treasury: &Address,
    amount: i128,
) {
    assert!(amount > 0, "transfer amount must be positive");
    let token = TokenClient::new(env, token_addr);
    let from_before = token.balance(from);
    let treasury_before = token.balance(treasury);
    assert!(
        from_before >= amount,
        "insufficient token balance before transfer"
    );

    token.transfer(from, &MuxedAddress::from(treasury.clone()), &amount);

    let from_after = token.balance(from);
    let treasury_after = token.balance(treasury);

    let spent = from_before
        .checked_sub(from_after)
        .expect("balance underflow on sender");
    let received = treasury_after
        .checked_sub(treasury_before)
        .expect("balance underflow on recipient");

    assert_eq!(
        spent, amount,
        "sender balance delta must equal transfer amount (check fee-on-transfer / malicious token)"
    );
    assert_eq!(
        received, amount,
        "recipient balance delta must equal transfer amount (check fee-on-transfer / malicious token)"
    );
}
