//! # LiquifactEscrow — Soroban Escrow Contract
//!
//! This crate implements a yield-bearing escrow for the Stellar/Soroban platform.
//! Funds are locked at `init` time and can accumulate protocol yield according to
//! a tiered basis-point schedule before being claimed or released.
//!
//! ## Invariants enforced at `init`
//!
//! | # | Invariant | Error |
//! |---|-----------|-------|
//! | 1 | `amount > 0` | `InvalidAmount` |
//! | 2 | `yield_bps ∈ [0, 10_000]` | `InvalidYieldBps` |
//! | 3 | Tier table empty vec is valid (flat rate applies) | — |
//! | 4 | Tier table has ≤ `MAX_TIERS` entries | `TierTableTooLarge` |
//! | 5 | Tier `min_amount` values are strictly increasing | `TierTableNotMonotonic` |
//! | 6 | Every tier `bps ∈ [0, 10_000]` | `InvalidTierBps` |
//! | 7 | `floor_bps ≤ target_bps` | `FloorExceedsTarget` |
//! | 8 | `target_bps ≤ cap_bps` | `TargetExceedsCap` |
//! | 9 | `cap_bps ≤ 10_000` | `CapOutOfRange` |
//! | 10 | `depositor` and `recipient` are valid addresses | host-enforced |
//!
//! ## Token economics
//!
//! Yield accrual and token transfers are intentionally out of scope for this
//! crate; see `external_calls.rs` for the token interface boundary.
//!
//! ## Security assumptions
//!
//! - `init` is one-shot: a second call returns `AlreadyInitialized`.
//! - All validation uses direct comparisons — no arithmetic that can overflow.
//! - No EVM/Solidity semantics apply; auth is explicit via `require_auth()`.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Vec,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of yield tier entries in the tier table.
/// Kept small to bound ledger-entry size and CPU cost of validation.
pub const MAX_TIERS: u32 = 20;

/// Hard upper bound on basis-point values (100 %).
pub const MAX_BPS: u32 = 10_000;

// ---------------------------------------------------------------------------
// Error enum
// ---------------------------------------------------------------------------

/// All errors that `init` (and helpers it calls) can return.
///
/// `#[contracterror]` (not `#[contracttype]`) is required so Soroban generates
/// the `From<soroban_sdk::Error>` impl that `#[contractimpl]` requires when
/// a function returns `Result<_, EscrowError>`.
///
/// Each variant maps to a unique `u32` for a stable ABI across upgrades.
/// **Do not renumber variants.**
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum EscrowError {
    /// `amount` passed to `init` was zero or negative (i128 ≤ 0).
    InvalidAmount = 1,
    /// Top-level `yield_bps` is outside `[0, MAX_BPS]`.
    InvalidYieldBps = 2,
    /// Reserved — an empty tier table is currently valid (flat rate applies).
    EmptyTierTable = 3,
    /// Tier table exceeded `MAX_TIERS` entries.
    TierTableTooLarge = 4,
    /// Tier `min_amount` values are not strictly ascending.
    TierTableNotMonotonic = 5,
    /// A tier's `bps` value is outside `[0, MAX_BPS]`.
    InvalidTierBps = 6,
    /// `floor_bps` is strictly greater than `target_bps`.
    FloorExceedsTarget = 7,
    /// `target_bps` is strictly greater than `cap_bps`.
    TargetExceedsCap = 8,
    /// `cap_bps` is outside `[0, MAX_BPS]`.
    CapOutOfRange = 9,
    /// `depositor` or `recipient` is an invalid address.
    InvalidAddress = 10,
    /// `init` was called a second time on an already-initialized contract.
    AlreadyInitialized = 11,
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// One entry in the yield-tier table.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YieldTier {
    /// Minimum escrowed balance (stroops) for this tier to activate.
    /// Must be strictly greater than the previous entry.
    pub min_amount: i128,
    /// Annualised yield in basis points. `[0, 10_000]`.
    pub bps: u32,
}

/// All parameters supplied to `init`.
#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowParams {
    pub depositor: Address,
    pub recipient: Address,
    pub amount: i128,
    pub yield_bps: u32,
    pub floor_bps: u32,
    pub target_bps: u32,
    pub cap_bps: u32,
    pub tiers: Vec<YieldTier>,
}

/// Persisted escrow state after successful `init`.
#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowState {
    pub depositor: Address,
    pub recipient: Address,
    pub amount: i128,
    pub yield_bps: u32,
    pub floor_bps: u32,
    pub target_bps: u32,
    pub cap_bps: u32,
    pub tiers: Vec<YieldTier>,
    /// Ledger sequence number at initialisation; used for yield accrual.
    pub init_ledger: u32,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct LiquifactEscrow;

#[contractimpl]
impl LiquifactEscrow {
    /// Initialise the escrow (one-shot).
    ///
    /// `params.depositor` must authorise this call.
    /// Returns the first [`EscrowError`] that fires in invariant-table order.
    pub fn init(env: Env, params: EscrowParams) -> Result<(), EscrowError> {
        // Guard: one-shot — symbol_short! requires a string literal, not a const.
        if env.storage().persistent().has(&symbol_short!("INIT")) {
            return Err(EscrowError::AlreadyInitialized);
        }

        params.depositor.require_auth();

        Self::validate_params(&env, &params)?;

        let state = EscrowState {
            depositor: params.depositor.clone(),
            recipient: params.recipient.clone(),
            amount: params.amount,
            yield_bps: params.yield_bps,
            floor_bps: params.floor_bps,
            target_bps: params.target_bps,
            cap_bps: params.cap_bps,
            tiers: params.tiers,
            init_ledger: env.ledger().sequence(),
        };

        env.storage()
            .persistent()
            .set(&symbol_short!("STAT"), &state);
        env.storage()
            .persistent()
            .set(&symbol_short!("INIT"), &true);

        // Emit event: topics = (escrow, init), data = (depositor, recipient, amount)
        // Using the low-level publish API; #[contractevent] macro alternative is
        // available if the repo's SDK version enforces it.
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("init")),
            (params.depositor, params.recipient, state.amount),
        );

        Ok(())
    }

    /// Read the current escrow state. Panics if not yet initialised.
    pub fn get_state(env: Env) -> EscrowState {
        env.storage()
            .persistent()
            .get(&symbol_short!("STAT"))
            .unwrap()
    }

    /// Validate all `EscrowParams` invariants before writing any state.
    ///
    /// Public so tests can call it directly without going through the full
    /// `init` flow (no auth or storage side-effects required).
    pub fn validate_params(_env: &Env, p: &EscrowParams) -> Result<(), EscrowError> {
        if p.amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }
        if p.yield_bps > MAX_BPS {
            return Err(EscrowError::InvalidYieldBps);
        }
        if p.cap_bps > MAX_BPS {
            return Err(EscrowError::CapOutOfRange);
        }
        if p.floor_bps > p.target_bps {
            return Err(EscrowError::FloorExceedsTarget);
        }
        if p.target_bps > p.cap_bps {
            return Err(EscrowError::TargetExceedsCap);
        }
        Self::validate_yield_tiers_table(&p.tiers)
    }

    /// Validate the yield tier table.
    ///
    /// Empty table → valid (flat rate applies).
    /// Non-empty → ≤ MAX_TIERS entries, each bps in range, min_amount strictly ascending.
    pub fn validate_yield_tiers_table(tiers: &Vec<YieldTier>) -> Result<(), EscrowError> {
        let len = tiers.len();
        if len == 0 {
            return Ok(());
        }
        if len > MAX_TIERS {
            return Err(EscrowError::TierTableTooLarge);
        }

        let mut prev_min: i128 = i128::MIN;
        for i in 0..len {
            let tier = tiers.get(i).unwrap();
            if tier.bps > MAX_BPS {
                return Err(EscrowError::InvalidTierBps);
            }
            if tier.min_amount <= prev_min {
                return Err(EscrowError::TierTableNotMonotonic);
            }
            prev_min = tier.min_amount;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
// The test module lives in src/test/init.rs and is compiled only when the
// "testutils" feature is enabled (required for soroban_sdk testutils + the
// generated *Client type).
#[cfg(any(test, feature = "testutils"))]
pub mod test;
