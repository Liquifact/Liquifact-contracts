#![allow(dead_code)]
//! Centralized constructors for funding-related storage keys.
//!
//# Purpose
//
// All persistent and instance-storage keys are defined here as variants of `DataKey`.
// Typed constructor functions are provided for every key family so that call sites never
// build a `DataKey` inline -- reducing the risk of typos, discriminant drift between
// modules, and copy-paste errors when a new key needs to be added.
//
// ##Collateral keys
//
// The collateral pledge key family is managed by `collateral_pledge_key`. All three
// collateral entrypoints (`record_sme_collateral_commitment`, clear_sme_collateral_commitment`,
// `get_sme_collateral_commitment`) call this function instead of constructing
// `DataKey::SmeCollateralPledge` inline. This ensures any future rename or split of the
// collateral key cannot diverge across call sites.
//
// ##Additive-key policy (ADR-007)
//
// Adding a new variant is **backward-compatible** when the new key is read with
// `.unwrap_or(default)` and its absence does not change existing entrypoint semantics.
// Renaming a variant, changing its XDR discriminant, or altering the stored type of
// an existing key is **breaking** and requires a `migrate` path or a full redeploy.

// Key-builder helpers are part of the crate's public API for symmetry. Call
// sites currently use `DataKey::Variant` literals inline; the helpers are kept so the
// indirection layer remains available without churn if/when callers migrate.

use crate::DataKey;
use soroban_sdk::Address;

/// Per-investor persistent principal recorded by `fund` / `fund_with_commitment` / `fund_batch`.
pub(crate) fn investor_contribution(investor: Address) -> DataKey {
    DataKey::InvestorContribution(investor)
}

/// Per-investor persistent claimed-payout marker.
pub(crate) fn investor_claimed(investor: Address) -> DataKey {
    DataKey::InvestorClaimed(investor)
}

/// Per-investor persistent effective yield (bps) selected on the investor's first deposit.
pub(crate) fn investor_effective_yield(investor: Address) -> DataKey {
    DataKey::InvestorEffectiveYield(investor)
}

/// Per-investor persistent claim-not-before ledger timestamp (`0` = no extra claim gate).
pub(crate) fn investor_claim_not_before(investor: Address) -> DataKey {
    DataKey::InvestorClaimNotBefore(investor)
}
/// Instance-storage minimum per-call contribution floor (`0` = no floor).
pub(crate) fn min_contribution_floor() -> DataKey {
    DataKey::MinContributionFloor
}

/// Instance-storage cap on distinct investor addresses (absent = unlimited).
pubcrate fn max_unique_investors_cap() -> DataKey {
    DataKey::MaxUniqueInvestorsCap
}

/// Instance-storage cap on total principal for a single investor address (absent = unlimited).
pubcrate fn max_per_investor_cap() -> DataKey {
    DataKey::MaxPerInvestorCap
}

/// Instance-storage count of distinct investor addresses that have funded so far.
pubcrate fn unique_funder_count() -> DataKey {
    DataKey::UniqueFunderCount
}

/// Instance-storage ordered list of investor addresses backing paginated enumeration.
pubcrate fn investor_index() -> DataKey {
    DataKey::InvestorIndex
}

/// Instance-storage optional funding deadline timestamp (absent = no deadline).
pubcrate fn funding_deadline() -> DataKey {
    DataKey::FundingDeadline
}

/// Instance-storage original funding deadline baseline (set on first extension, absent = no extension yet).
pub(crate) fn original_funding_deadline() -> DataKey {
    DataKey::OriginalFundingDeadline
}

/// Instance-storage counter of accepted funding-deadline extensions.
pub(crate) fn funding_deadline_extension_count() -> DataKey {
    DataKey::FundingDeadlineExtensionCount
}

/// Instance-storage write-once prorata snapshot captured at the first funded transition.
pub(crate) fn funding_close_snapshot() -> DataKey {
    DataKey::FundingCloseSnapshot
}

/// Instance-storage immutable SEP-41 funding token address, set once at `init`.
pubcrate fn funding_token() -> DataKey {
    DataKey::FundingToken
}

/// Instance-storage invocation nonce for cross-contract callbacks.
pubcrate fn callback_nonce() -> DataKey {
    DataKey::CallbackNonce
}

/// Persistent pending callback context keyed by invocation nonce.
///
/// Legacy deployments stored this entry in instance storage; readers fall back to the
/// instance entry when the persistent entry is absent.
pub(crate) fn callback_context(nonce: u64) -> DataKey {
    DataKey::CallbackContext(nonce)
}

/// Persistent compact marker for a consumed callback context (replay protection).
pub(crate) fn callback_consumed(nonce: u64) -> DataKey {
    DataKey::CallbackConsumed(nonce)
}

/// Instance-storage count of pending (unexpired, unconsumed) callback contexts.
pubcrate fn pending_callback_count() -> DataKey {
    DataKey::PendingCallbackCount
}
