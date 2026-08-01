//! Centralised storage-key definitions for the LiquiFact escrow contract.
//!
//! # Purpose
//!
//! All persistent and instance-storage keys are defined here as variants of [`DataKey`].
//! Typed constructor functions are provided for every key family so that call sites never
//! build a [`DataKey`] inline — reducing the risk of typos, discriminant drift between
//! modules, and copy-paste errors when a new key needs to be added.
//!
//! ## Collateral keys
//!
//! The collateral pledge key family is managed by [`collateral_pledge_key`]. All three
//! collateral entrypoints (`record_sme_collateral_commitment`, `clear_sme_collateral_commitment`,
//! `get_sme_collateral_commitment`) call this function instead of constructing
//! `DataKey::SmeCollateralPledge` inline. This ensures any future rename or split of the
//! collateral key cannot diverge across call sites.
//!
//! ## Funding keys
//!
//! Every storage key touched by the funding entrypoints (`fund`, `fund_with_commitment`,
//! `fund_batch`) is constructed here: the per-investor principal and claim-lock timestamp
//! (keyed by [`Address`]), and the contract-wide funding token, contribution floor,
//! per-investor cap, and funding-close snapshot.
//!
//! ## Additive-key policy (ADR-007)
//!
//! Adding a new variant is **backward-compatible** when the new key is read with
//! `.unwrap_or(default)` and its absence does not change existing entrypoint semantics.
//! Renaming a variant, changing its XDR discriminant, or altering the stored type of an
//! existing key is **breaking** and requires a `migrate` path or a full redeploy.

use crate::DataKey;
use soroban_sdk::Address;

// ---------------------------------------------------------------------------
// Funding key constructors
// ---------------------------------------------------------------------------

use crate::DataKey;
use soroban_sdk::Address;

/// Per-investor persistent principal recorded by `fund` / `fund_with_commitment` / `fund_batch`.
pub(crate) fn investor_contribution(investor: Address) -> DataKey {
    DataKey::InvestorContribution(investor)
}

/// Per-investor claim-lock timestamp (`committed_lock_secs` follow-on) recorded by
/// `fund_with_commitment`.
pub(crate) fn investor_claim_not_before(investor: Address) -> DataKey {
    DataKey::InvestorClaimNotBefore(investor)
}

/// Instance-storage immutable SEP-41 funding token address, set once at `init`.
pub(crate) fn funding_token() -> DataKey {
    DataKey::FundingToken
}

/// Instance-storage minimum per-contribution floor (0 ⇒ no floor configured).
pub(crate) fn min_contribution_floor() -> DataKey {
    DataKey::MinContributionFloor
}

/// Instance-storage maximum cumulative principal a single investor may contribute
/// (`None` ⇒ unlimited).
pub(crate) fn max_per_investor_cap() -> DataKey {
    DataKey::MaxPerInvestorCap
}

/// Instance-storage snapshot recorded exactly once when funding first closes
/// (status 0 → 1).
pub(crate) fn funding_close_snapshot() -> DataKey {
    DataKey::FundingCloseSnapshot
}

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
#[inline(always)]
pub fn collateral_pledge_key() -> DataKey {
    DataKey::SmeCollateralPledge
}

// ---------------------------------------------------------------------------
// Yield-tier key constructors
// ---------------------------------------------------------------------------

/// Instance-storage key for the configured yield-tier table.
#[inline(always)]
pub const fn yield_tier_table_key() -> DataKey {
    DataKey::YieldTierTable
}

/// Persistent-storage key for an investor's selected yield.
#[inline(always)]
pub fn investor_effective_yield_key(investor: &Address) -> DataKey {
    DataKey::InvestorEffectiveYield(investor.clone())
}

/// Persistent-storage key for an investor's earliest claim timestamp.
#[inline(always)]
pub fn investor_claim_not_before_key(investor: &Address) -> DataKey {
    DataKey::InvestorClaimNotBefore(investor.clone())
}
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Per-investor persistent claimed-payout marker.
pub(crate) fn investor_claimed(investor: Address) -> DataKey {
    DataKey::InvestorClaimed(investor)
}

    // ── yield-tier keys ───────────────────────────────────────────────────

    #[test]
    fn yield_tier_table_key_preserves_existing_variant() {
        assert!(matches!(yield_tier_table_key(), DataKey::YieldTierTable));
    }

    #[test]
    fn investor_yield_keys_preserve_address_and_family() {
        use soroban_sdk::testutils::Address as _;
        use soroban_sdk::Env;

        let env = Env::default();
        let investor = Address::generate(&env);

        match investor_effective_yield_key(&investor) {
            DataKey::InvestorEffectiveYield(address) => assert_eq!(address, investor),
            _ => panic!("unexpected effective-yield key variant"),
        }
        match investor_claim_not_before_key(&investor) {
            DataKey::InvestorClaimNotBefore(address) => assert_eq!(address, investor),
            _ => panic!("unexpected claim-lock key variant"),
        }
    }

    #[test]
    fn investor_yield_keys_keep_investors_separate() {
        use soroban_sdk::testutils::Address as _;
        use soroban_sdk::Env;

        let env = Env::default();
        let first = Address::generate(&env);
        let second = Address::generate(&env);

        match investor_effective_yield_key(&first) {
            DataKey::InvestorEffectiveYield(address) => assert_ne!(address, second),
            _ => panic!("unexpected effective-yield key variant"),
        }
        match investor_claim_not_before_key(&second) {
            DataKey::InvestorClaimNotBefore(address) => assert_ne!(address, first),
            _ => panic!("unexpected claim-lock key variant"),
        }
    }
    // ── collateral_pledge_key ────────────────────────────────────────────────

    /// The constructor must return the `SmeCollateralPledge` variant — verified by a
    /// `matches!` guard so the test does not depend on a `PartialEq` derive that the
    /// `#[contracttype]` macro does not generate for `DataKey`.
    #[test]
    fn collateral_pledge_key_returns_sme_collateral_pledge_variant() {
        let key = collateral_pledge_key();
        assert!(
            matches!(key, DataKey::SmeCollateralPledge),
            "collateral_pledge_key() must return DataKey::SmeCollateralPledge"
        );
    }

    /// Calling the constructor twice must produce structurally identical keys — callers
    /// that cache or compare keys between entrypoints (e.g. an indexer that stores the
    /// discriminant) must see a stable, idempotent value.
    #[test]
    fn collateral_pledge_key_is_idempotent() {
        let k1 = collateral_pledge_key();
        let k2 = collateral_pledge_key();
        assert!(matches!(k1, DataKey::SmeCollateralPledge));
        assert!(matches!(k2, DataKey::SmeCollateralPledge));
    }

    // ── funding key constructors ────────────────────────────────────────────

    /// `investor_contribution` must return the `InvestorContribution` variant carrying
    /// the given address, matching how the tuple variant is looked up at call sites.
    #[test]
    fn investor_contribution_returns_investor_contribution_variant() {
        // DataKey does not derive PartialEq, so we can only assert the discriminant via
        // `matches!`; the wrapped Address equality is exercised at the storage-layer tests.
        let key = investor_contribution(Address::generate(&soroban_sdk::Env::default()));
        assert!(matches!(key, DataKey::InvestorContribution(_)));
    }

    /// `investor_claim_not_before` must return the `InvestorClaimNotBefore` variant.
    #[test]
    fn investor_claim_not_before_returns_expected_variant() {
        let key = investor_claim_not_before(Address::generate(&soroban_sdk::Env::default()));
        assert!(matches!(key, DataKey::InvestorClaimNotBefore(_)));
    }

    /// Unit-type funding keys must be constructible and distinguishable from one another.
    #[test]
    fn unit_type_funding_keys_are_distinct() {
        assert!(matches!(funding_token(), DataKey::FundingToken));
        assert!(matches!(
            min_contribution_floor(),
            DataKey::MinContributionFloor
        ));
        assert!(matches!(max_per_investor_cap(), DataKey::MaxPerInvestorCap));
        assert!(matches!(
            funding_close_snapshot(),
            DataKey::FundingCloseSnapshot
        ));
    }
}
