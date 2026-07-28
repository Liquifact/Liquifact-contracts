//! Centralised storage-key definitions for the LiquiFact escrow contract.
//!
//! # Purpose
//!
//! All persistent and instance-storage keys are defined in [`DataKey`] (lib.rs).
//! Typed constructor functions are provided here for every key family so that call sites never
//! build a [`DataKey`] inline — reducing the risk of typos, discriminant drift between
//! modules, and copy-paste errors when a new key needs to be added.
//!
//! ## Collateral keys
//!
//! The collateral pledge key family is managed by [`collateral_pledge_key`]. All three
//! collateral entrypoints (`record_sme_collateral_commitment`, `clear_sme_collateral_commitment`,
//! `get_sme_collateral_commitment`) call this function instead of constructing
//! `DataKey::SmeCollateralPledge` inline. This ensures any future rename or split of the
//! collateral key cannot silently diverge across call sites.
//!
//! ## Additive-key policy (ADR-007)
//!
//! Adding a new variant to [`DataKey`] is **backward-compatible** when the new key is read with
//! `.unwrap_or(default)` and its absence does not change existing entrypoint semantics.
//! Renaming a variant, changing its XDR discriminant, or altering the stored type of an
//! existing key is **breaking** and requires a `migrate` path or a full redeploy.

use soroban_sdk::Address;

use crate::DataKey;

// ---------------------------------------------------------------------------
// Collateral key constructors
// ---------------------------------------------------------------------------

/// Return the canonical storage key for the SME collateral pledge.
///
/// All three collateral entrypoints — `record_sme_collateral_commitment`,
/// `clear_sme_collateral_commitment`, and `get_sme_collateral_commitment` — must call this
/// function instead of constructing `DataKey::SmeCollateralPledge` inline. This single
/// construction point guarantees that a future rename or variant-split cannot silently
/// diverge across call sites.
///
/// # Storage tier
///
/// The returned key lives in **instance** storage (shared TTL with the contract instance).
/// Callers are responsible for using `env.storage().instance()`.
///
/// # Example
///
/// ```ignore
/// use crate::keys::collateral_pledge_key;
///
/// let key = collateral_pledge_key();
/// env.storage().instance().set(&key, &commitment);
/// ```
#[inline(always)]
pub fn collateral_pledge_key() -> DataKey {
    DataKey::SmeCollateralPledge
}

/// Return the canonical storage key for the settlement records log.
///
/// The settlement records log is an append-only [`Vec<SettlementRecord>`] stored in instance
/// storage. It is written by [`LiquifactEscrow::settle`] and read by
/// [`LiquifactEscrow::get_settlement_records`].
///
/// # Storage tier
///
/// Instance storage (shared TTL with the contract instance).
#[inline(always)]
pub fn settlement_records_key() -> DataKey {
    DataKey::SettlementRecords
}
