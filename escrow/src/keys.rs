//! Centralized constructors for funding-related storage keys.
//!
//! Funding logic (`fund`/`fund_with_commitment`/`fund_batch`/`fund_impl`, the cap and
//! funding-deadline admin setters, `update_funding_target`, `partial_settle`, `unfund`, and
//! `bump_ttl`) previously constructed the [`DataKey`] variants below inline at each call site.
//! Two call sites agreeing on a key's shape by convention (rather than by construction) is
//! exactly the drift risk this module removes: every caller now obtains a given key through one
//! function instead of re-typing `DataKey::Variant(...)`.
//!
//! This is a pure indirection layer. No key's on-chain shape or `DataKey` discriminant changes —
//! that contract still belongs solely to [`DataKey`] (see ADR-007) — so no migration or
//! `SCHEMA_VERSION` bump is needed (issue #912).

use crate::DataKey;
use soroban_sdk::Address;

/// Per-investor persistent principal recorded by `fund` / `fund_with_commitment` / `fund_batch`.
pub(crate) fn investor_contribution(investor: Address) -> DataKey {
    DataKey::InvestorContribution(investor)
}

/// Per-investor persistent effective yield (bps) selected on the investor's first deposit.
pub(crate) fn investor_effective_yield(investor: Address) -> DataKey {
    DataKey::InvestorEffectiveYield(investor)
}

/// Per-investor persistent claim-not-before ledger timestamp (`0` = no extra claim gate).
pub(crate) fn investor_claim_not_before(investor: Address) -> DataKey {
    DataKey::InvestorClaimNotBefore(investor)
}

/// Per-investor persistent claimed-payout marker.
pub(crate) fn investor_claimed(investor: Address) -> DataKey {
    DataKey::InvestorClaimed(investor)
}

/// Instance-storage minimum per-call contribution floor (`0` = no floor).
pub(crate) fn min_contribution_floor() -> DataKey {
    DataKey::MinContributionFloor
}

/// Instance-storage cap on distinct investor addresses (absent = unlimited).
pub(crate) fn max_unique_investors_cap() -> DataKey {
    DataKey::MaxUniqueInvestorsCap
}

/// Instance-storage cap on total principal for a single investor address (absent = unlimited).
pub(crate) fn max_per_investor_cap() -> DataKey {
    DataKey::MaxPerInvestorCap
}

/// Instance-storage count of distinct investor addresses that have funded so far.
pub(crate) fn unique_funder_count() -> DataKey {
    DataKey::UniqueFunderCount
}

/// Instance-storage ordered list of investor addresses backing paginated enumeration.
pub(crate) fn investor_index() -> DataKey {
    DataKey::InvestorIndex
}

/// Instance-storage optional funding deadline timestamp (absent = no deadline).
pub(crate) fn funding_deadline() -> DataKey {
    DataKey::FundingDeadline
}

/// Instance-storage write-once pro-rata snapshot captured at the first funded transition.
pub(crate) fn funding_close_snapshot() -> DataKey {
    DataKey::FundingCloseSnapshot
}

/// Instance-storage immutable SEP-41 funding token address, set once at `init`.
pub(crate) fn funding_token() -> DataKey {
    DataKey::FundingToken
}

