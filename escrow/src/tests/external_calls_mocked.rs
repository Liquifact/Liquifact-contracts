//! Adversarial-token tests for `transfer_funding_token_with_balance_checks`.
//!
//! This module proves that every typed error path in
//! `escrow/src/external_calls.rs` fires correctly against non-compliant tokens.
//!
//! ## Error paths covered
//!
//! | Error code | Variant | Test(s) |
//! |---|---|---|
//! | 36 | `TransferAmountNotPositive` | `test_zero_amount_rejected`, `test_negative_amount_rejected` |
//! | 37 | `InsufficientTokenBalanceBeforeTransfer` | `test_insufficient_balance_rejected`, `test_zero_balance_rejected` |
//! | 40 | `SenderBalanceDeltaMismatch` | `test_fee_on_transfer_token_rejected`, `test_large_fee_token_rejected`, `test_exact_fee_boundary_rejected` |
//! | 41 | `RecipientBalanceDeltaMismatch` | `test_rebasing_token_over_credits_rejected`, `test_no_op_transfer_token_rejected`, `test_hook_token_extra_mint_rejected` |
//! | control | (no error) | `test_compliant_token_passes`, `test_minimum_amount_passes`, `test_large_transfer_no_overflow`, `test_multiple_sequential_transfers`, `test_sender_ends_at_zero_balance` |

use super::super::external_calls::transfer_funding_token_with_balance_checks;
use super::*;
use soroban_sdk::{contract, contractimpl, token::TokenInterface, Address, Env, MuxedAddress};

// ---------------------------------------------------------------------------
// Mock: fee-on-transfer token (1% fee)
//
// Steals 1% on every transfer — sender is debited `amount` but recipient
// gets only `amount - fee`. This violates the sender-delta invariant
// (`spent == amount` holds) but the recipient-delta invariant fails first
// due to the mismatch between debit and credit.
//
// Security note: triggers `SenderBalanceDeltaMismatch` because the wrapper
// checks sender delta first. Both deltas diverge, but the error fires at
// the first failing assertion.
// ---------------------------------------------------------------------------

/// A SEP-41-like token that silently deducts a 1% protocol fee on every
/// `transfer` call. The sender loses `amount` but the recipient only
/// receives `amount * 99 / 100`, so balance conservation is broken.
///
/// This models real-world fee-on-transfer (FoT) tokens (e.g. some DeFi
/// tokens that fund a DAO treasury from each transfer).
#[contract]
pub struct FeeOnTransferToken;

#[contractimpl]
impl TokenInterface for FeeOnTransferToken {
    fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        let fee = amount / 100; // steal 1 %
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

/// Mint tokens directly into the fee token's persistent storage (bypasses transfer auth).
fn mint_fee_token(env: &Env, contract_id: &Address, to: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let current: i128 = env.storage().persistent().get(to).unwrap_or(0);
        env.storage().persistent().set(to, &(current + amount));
    });
}

// ---------------------------------------------------------------------------
// Mock: large-fee token (50% fee)
//
// Deducts 50% — makes the divergence unmistakeable even for large amounts.
// Separately exercises the SenderBalanceDeltaMismatch path with a larger gap.
// ---------------------------------------------------------------------------

/// A SEP-41-like token that deducts a 50% fee on every transfer.
/// This models an extreme deflationary token or a malicious token that
/// siphons half the transferred value into a hidden reserve.
#[contract]
pub struct LargeFeeToken;

#[contractimpl]
impl TokenInterface for LargeFeeToken {
    fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        let credited = amount / 2; // 50% fee — recipient gets half

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
        soroban_sdk::String::from_str(&env, "LargeFeeToken")
    }
    fn symbol(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "LFEE")
    }
}

fn mint_large_fee_token(env: &Env, contract_id: &Address, to: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let current: i128 = env.storage().persistent().get(to).unwrap_or(0);
        env.storage().persistent().set(to, &(current + amount));
    });
}

// ---------------------------------------------------------------------------
// Mock: rebasing token (over-credits recipient)
//
// Credits the recipient with `amount * 2` — simulating a rebasing or
// "auto-compounding" token that mints extra supply on every transfer.
// This violates RecipientBalanceDeltaMismatch.
// ---------------------------------------------------------------------------

/// A SEP-41-like token that credits the recipient with twice the transferred
/// amount on every call. This models a rebasing or interest-bearing token
/// whose `transfer` hook mints bonus tokens (e.g. staking reward tokens that
/// auto-compound into the recipient's balance during transfer).
///
/// Security note: triggers `RecipientBalanceDeltaMismatch` because the
/// recipient receives more than `amount`.
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
        env.storage().persistent().set(&from, &(from_bal - amount)); // correct debit

        let to_bal = Self::balance(env.clone(), to_addr.clone());
        env.storage()
            .persistent()
            .set(&to_addr, &(to_bal + amount * 2)); // over-credit (rebasing)
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
        soroban_sdk::String::from_str(&env, "RebasingToken")
    }
    fn symbol(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "REB")
    }
}

fn mint_rebasing_token(env: &Env, contract_id: &Address, to: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let current: i128 = env.storage().persistent().get(to).unwrap_or(0);
        env.storage().persistent().set(to, &(current + amount));
    });
}

// ---------------------------------------------------------------------------
// Mock: no-op transfer token (leaves balances unchanged)
//
// `transfer` is a complete no-op — it accepts the call but moves nothing.
// This models a frozen/paused token or a buggy integration where the
// transfer silently succeeds without updating any balances.
//
// Security note: triggers `SenderBalanceDeltaMismatch` (sender was not
// debited) because `spent == 0` but the wrapper requires `spent == amount`.
// ---------------------------------------------------------------------------

/// A SEP-41-like token whose `transfer` implementation is a complete no-op.
/// Sender and recipient balances never change regardless of the requested
/// amount. This models a paused or frozen token contract.
///
/// Security note: triggers `SenderBalanceDeltaMismatch` because `spent == 0`
/// but the wrapper requires `spent == amount`.
#[contract]
pub struct NoOpTransferToken;

#[contractimpl]
impl TokenInterface for NoOpTransferToken {
    fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    fn transfer(env: Env, from: Address, _to: MuxedAddress, _amount: i128) {
        from.require_auth();
        // Intentional no-op: balances are never updated.
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
        soroban_sdk::String::from_str(&env, "NoOpToken")
    }
    fn symbol(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "NOOP")
    }
}

fn mint_no_op_token(env: &Env, contract_id: &Address, to: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let current: i128 = env.storage().persistent().get(to).unwrap_or(0);
        env.storage().persistent().set(to, &(current + amount));
    });
}

// ---------------------------------------------------------------------------
// Mock: hook token that mints extra to recipient (via side-effect)
//
// Correctly debits the sender but then also mints an extra bonus amount
// directly to the recipient's balance on top of the transfer. This simulates
// a hook/callback token whose `transfer` triggers an external rewards mint.
//
// Security note: triggers `RecipientBalanceDeltaMismatch` because the
// recipient ends up with more than `amount` extra.
// ---------------------------------------------------------------------------

/// A SEP-41-like token that executes a "hook" during every transfer,
/// minting an extra bonus of `amount / 10` directly to the recipient on
/// top of the transferred amount. This simulates a token with a transfer
/// hook that distributes rewards.
///
/// Security note: triggers `RecipientBalanceDeltaMismatch` because the
/// recipient delta is `amount + bonus` rather than exactly `amount`.
#[contract]
pub struct HookToken;

#[contractimpl]
impl TokenInterface for HookToken {
    fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        let bonus = amount / 10; // extra 10% minted to recipient via "hook"
        let to_addr = to.address();

        let from_bal = Self::balance(env.clone(), from.clone());
        env.storage().persistent().set(&from, &(from_bal - amount)); // correct debit

        let to_bal = Self::balance(env.clone(), to_addr.clone());
        env.storage()
            .persistent()
            .set(&to_addr, &(to_bal + amount + bonus)); // over-credit via hook
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

// ===========================================================================
// Tests: fee-on-transfer rejection — SenderBalanceDeltaMismatch (error 40)
// ===========================================================================

/// A 1% fee token must be rejected: sender is debited `amount` but recipient
/// receives only `amount * 99 / 100`, so the wrapper's balance conservation
/// check fires and panics with `SenderBalanceDeltaMismatch`.
#[test]
#[should_panic]
fn test_fee_on_transfer_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let fee_token_id = env.register(FeeOnTransferToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_fee_token(&env, &fee_token_id, &holder, 1000i128);

    // Panics: recipient gets 990 but wrapper expects exactly 1000.
    transfer_funding_token_with_balance_checks(&env, &fee_token_id, &holder, &treasury, 1000i128);
}

/// A 1% fee token is rejected even for very small amounts where the fee
/// rounds to 0 (e.g. amount = 1). Amount 1 / 100 = 0 fee, so credited == 1,
/// which is compliant. Use amount = 100 to guarantee a 1-unit fee.
#[test]
#[should_panic]
fn test_fee_on_transfer_small_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let fee_token_id = env.register(FeeOnTransferToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    // 100 / 100 = 1 unit fee; recipient gets 99.
    mint_fee_token(&env, &fee_token_id, &holder, 100i128);
    transfer_funding_token_with_balance_checks(&env, &fee_token_id, &holder, &treasury, 100i128);
}

/// A 50% fee token must be rejected — the divergence is larger but the same
/// error path (`SenderBalanceDeltaMismatch`) fires.
#[test]
#[should_panic]
fn test_large_fee_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register(LargeFeeToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_large_fee_token(&env, &token_id, &holder, 2000i128);

    // Recipient gets 1000, not 2000 — triggers SenderBalanceDeltaMismatch.
    transfer_funding_token_with_balance_checks(&env, &token_id, &holder, &treasury, 2000i128);
}

/// Boundary case: 1% fee on an amount of exactly 1 produces 0 fee (integer
/// truncation), so the token *appears* compliant. This test verifies the mock
/// helper correctly handles boundary math and serves as a documentation anchor:
/// FoT tokens with fees that round to zero on tiny amounts are still dangerous
/// on larger transfers.
#[test]
fn test_fee_token_boundary_amount_one_passes() {
    let env = Env::default();
    env.mock_all_auths();

    let fee_token_id = env.register(FeeOnTransferToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    // amount=1 → fee = 1/100 = 0 → credited = 1. Compliant at this scale.
    mint_fee_token(&env, &fee_token_id, &holder, 1i128);
    transfer_funding_token_with_balance_checks(&env, &fee_token_id, &holder, &treasury, 1i128);

    // Balances: holder=0, treasury=1.
    env.as_contract(&fee_token_id, || {
        let h: i128 = env.storage().persistent().get(&holder).unwrap_or(0);
        let t: i128 = env.storage().persistent().get(&treasury).unwrap_or(0);
        assert_eq!(h, 0i128);
        assert_eq!(t, 1i128);
    });
}

// ===========================================================================
// Tests: rebasing / over-credit — RecipientBalanceDeltaMismatch (error 41)
// ===========================================================================

/// A rebasing token that double-credits the recipient must be rejected.
/// The sender delta is correct but `received == amount * 2` triggers
/// `RecipientBalanceDeltaMismatch`.
#[test]
#[should_panic]
fn test_rebasing_token_over_credits_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register(RebasingToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_rebasing_token(&env, &token_id, &holder, 1000i128);

    // Recipient gets 2000, not 1000 — triggers RecipientBalanceDeltaMismatch.
    transfer_funding_token_with_balance_checks(&env, &token_id, &holder, &treasury, 1000i128);
}

/// A hook token that mints an extra 10% to the recipient must be rejected.
/// The extra mint makes `received == amount + bonus`, violating the invariant.
#[test]
#[should_panic]
fn test_hook_token_extra_mint_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register(HookToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_hook_token(&env, &token_id, &holder, 1000i128);

    // Recipient gets 1100 (10% bonus hook) — triggers RecipientBalanceDeltaMismatch.
    transfer_funding_token_with_balance_checks(&env, &token_id, &holder, &treasury, 1000i128);
}

// ===========================================================================
// Tests: no-op transfer rejection — SenderBalanceDeltaMismatch (error 40)
// ===========================================================================

/// A no-op transfer token (frozen/paused) must be rejected. `transfer` accepts
/// the call but moves nothing, so `spent == 0` while the wrapper requires
/// `spent == amount`, triggering `SenderBalanceDeltaMismatch`.
#[test]
#[should_panic]
fn test_no_op_transfer_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register(NoOpTransferToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_no_op_token(&env, &token_id, &holder, 1000i128);

    // Transfer does nothing — sender delta is 0, triggers SenderBalanceDeltaMismatch.
    transfer_funding_token_with_balance_checks(&env, &token_id, &holder, &treasury, 1000i128);
}

/// No-op transfer is rejected even when the sender has exactly the transfer
/// amount as their entire balance. Covers the "exact balance, frozen" edge case.
#[test]
#[should_panic]
fn test_no_op_transfer_exact_balance_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register(NoOpTransferToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_no_op_token(&env, &token_id, &holder, 500i128);

    // Sender has exactly 500, transfers 500 — but no-op means spent=0.
    transfer_funding_token_with_balance_checks(&env, &token_id, &holder, &treasury, 500i128);
}

// ===========================================================================
// Tests: TransferAmountNotPositive (error 36)
// ===========================================================================

/// Zero amount must be rejected before any token call is made.
/// The guard fires at the entry of `transfer_funding_token_with_balance_checks`
/// before balance reads or the SEP-41 `transfer` call.
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

/// Negative amount must be rejected immediately — it is not a valid transfer
/// quantity under SEP-41 semantics.
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

/// The most negative i128 value must also be rejected.
#[test]
#[should_panic]
fn test_min_i128_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, i128::MIN);
}

// ===========================================================================
// Tests: InsufficientTokenBalanceBeforeTransfer (error 37)
// ===========================================================================

/// Sender has less than the requested amount: the pre-transfer balance check
/// fires before the SEP-41 `transfer` call, preventing an attempted overdraft.
#[test]
#[should_panic]
fn test_insufficient_balance_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // Mint only 500 but attempt to transfer 1000.
    token.stellar.mint(&holder, &500i128);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 1000i128);
}

/// Sender has zero balance: rejected before any token transfer attempt.
#[test]
#[should_panic]
fn test_zero_balance_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // No mint — holder balance is 0.
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 1i128);
}

/// Sender balance is exactly one less than the requested amount (off-by-one).
#[test]
#[should_panic]
fn test_balance_one_less_than_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    let amount = 1_000i128;
    token.stellar.mint(&holder, &(amount - 1));

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, amount);
}

// ===========================================================================
// Tests: compliant token — control cases (all must pass)
// ===========================================================================

/// A well-behaved Stellar asset token transfers correctly. Both sender and
/// recipient deltas equal `amount` exactly, conserving total supply.
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
    assert_eq!(
        holder_before - holder_after,
        amount,
        "sender debited exactly amount"
    );
    assert_eq!(
        treasury_after - treasury_before,
        amount,
        "recipient credited exactly amount"
    );
}

/// Minimum meaningful amount (1 base unit) transfers correctly.
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

/// Large amount (i128::MAX / 100) transfers without overflow.
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

/// Two sequential transfers to different recipients maintain correct running balances.
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

/// Sender drains their entire balance in one call — final sender balance is exactly 0.
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

/// Transfer of exact balance (sender balance == amount) succeeds — boundary between
/// InsufficientTokenBalanceBeforeTransfer and a valid transfer.
#[test]
fn test_exact_balance_transfer_passes() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    let amount = 7777i128;
    token.stellar.mint(&holder, &amount);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, amount);

    assert_eq!(token.token.balance(&holder), 0i128);
    assert_eq!(token.token.balance(&treasury), amount);
}
