#allow(dead_code, unused_imports)]
/// Validation boundaries for the key constructors in `escrow/src/keys.rs`.
///
/// These tests pin the deterministic behavior of every key builder:
///  * valid input produces the expected `DataKey` variant,
///  * distinct inputs produce distinct keys (no collisions),
///  * duplicate inputs produce equal keys (stable identity),
///  * boundary values (zero, u64::MAX) round-trip without loss.
///
/// The key constructors are the only supported way to build a `DataKey`
/// for funding storage; these tests are the regression barrier against
/// discriminant drift or accidental renames.

use crate::keys::*;
use crate::DataKey;
use soroban_sdk::{Address, Env};

//// ---------------------------------------------------------------------------
/// Per-investor key family
//// --------------------------------------------------------------------------

#[test]
fn investor_contribution_matches_variant() {
    let env = Env::default();
    let investor = Address::generate(&env);
    assert_eq!(
        investor_contribution(investor.clone()),
        DataKey::InvestorContribution(investor)
    );
}

#[test]
fn investor_contribution_is_deterministic() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let a = investor_contribution(investor.clone());
    let b = investor_contribution(investor.clone());
    assert_eq!(a, b);
}

#[test]
fn investor_contribution_distinct_investors_differ() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    assert_ne!(
        investor_contribution(a),
        investor_contribution(b)
    );
}

#[test]
fn investor_effective_yield_matches_variant() {
    let env = Env::default();
    let investor = Address::generate(&env);
    assert_eq!(
        investor_effective_yield(investor.clone()),
        DataKey::InvestorEffectiveYield(investor)
    );
}

#[test]
fn investor_claim_not_before_matches_variant() {
    let env = Env::default();
    let investor = Address::generate(&env);
    assert_eq!(
        investor_claim_not_before(investor.clone()),
        DataKey::InvestorClaimNotBefore(investor)
    );
}

#[test]
fn investor_claimed_matches_variant() {
    let env = Env::default();
    let investor = Address::generate(&env);
    assert_eq!(
        investor_claimed(investor.clone()),
        DataKey::InvestorClaimed(investor)
    );
}

/// Investor key families must not collide across families for the same
/// address. This guards against a refactor that accidentally reuses a
/// discriminant for two different key families.
#[test]
fn investor_key_families_are_disjoint() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let keys = [
        investor_contribution(investor.clone()),
        investor_effective_yield(investor.clone()),
        investor_claim_not_before(investor.clone()),
        investor_claimed(investor.clone()),
    ];
    for i in 0..keys.len() {
        for j in (i + 1)..keys.len() {
            assert_ne!(keys[i], keys[j]);
        }
    }
}

//// ---------------------------------------------------------------------------
/// Singleton instance-key family
//// --------------------------------------------------------------------------

#[test]
fn singleton_keys_match_variants() {
    assert_eq!(min_contribution_floor(), DataKey::MinContributionFloor);
    assert_eq!(max_unique_investors_cap(), DataKey::MaxUniqueInvestorsCap);
    assert_eq!(max_per_investor_cap(), DataKey::MaxPerInvestorCap);
    assert_eq!(unique_funder_count(), DataKey::UniqueFunderCount);
    assert_eq!(investor_index(), DataKey::InvestorIndex);
    assert_eq!(funding_deadline(), DataKey::FundingDeadline);
    assert_eq!(funding_close_snapshot(), DataKey::FundingCloseSnapshot);
    assert_eq!(funding_token(), DataKey::FundingToken);
    assert_eq!(funding_token_scale(), DataKey::FundingTokenScale);
    assert_eq!(callback_nonce(), DataKey::CallbackNonce);
    assert_eq!(released_amount(), DataKey::ReleasedAmount);
}

#[test]
fn singleton_keys_are_distinct() {
    let keys = [
        min_contribution_floor(),
        max_unique_investors_cap(),
        max_per_investor_cap(),
        unique_funder_count(),
        investor_index(),
        funding_deadline(),
        funding_close_snapshot(),
        funding_token(),
        funding_token_scale(),
        callback_nonce(),
        released_amount(),
    ];
    for i in 0..keys.len() {
        for j in (i + 1)..keys.len() {
            assert_ne!(keys[i], keys[j]);
        }
    }
}

//// ---------------------------------------------------------------------------
/// Nonce-keyed callback context family
//// --------------------------------------------------------------------------

#[test]
fn callback_context_matches_variant() {
    assert_eq!(callback_context(0), DataKey::CallbackContext(0));
    assert_eq!(callback_context(1), DataKey::CallbackContext(1));
    assert_eq!(
        callback_context(u64::MAX - 1),
        DataKey::CallbackContext(u64::MAX - 1)
    );
    assert_eq!(
        callback_context(u64::MAX),
        DataKey::CallbackContext(u64::MAX)
    );
}

/// Boundary: the lowest and highest nonce values must not collide with
/// each other or with any other nonce in between.
#[test]
fn callback_context_boundaries_are_distinct() {
    assert_ne!(callback_context(0), callback_context(1));
    assert_ne!(callback_context(0), callback_context(u64::MAX));
    assert_ne!(
        callback_context(u64::MAX - 1),
        callback_context(u64::MAX)
    );
    assert_ne!(callback_context(1), callback_context(u64::MAX - 1));
}

/// Duplicate nonces must produce identical keys so a retried callback
/// with the same nonce addresses the same storage slot (idempotent).
#[test]
fn callback_context_duplicate_nonce_is_identical() {
    let nonce = 42_u64;
    assert_eq!(callback_context(nonce), callback_context(nonce));
}

//// ---------------------------------------------------------------------------
/// Cross-family collision guard
//// ---------------------------------------------------------------------------

/// A nonce-keyed callback context must never collide with a singleton key
/// or an investor-keyed key, even when the nonce is 0 or u64::MAX.
#[test]
fn callback_context_does_not_collide_with_singletons() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let nonces = [0_u64, 1_u64, u64::MAX - 1, u64::MAX];
    for nonce in nonces {
        let key = callback_context(nonce);
        assert_ne!(key, min_contribution_floor());
        assert_ne!(key, max_unique_investors_cap());
        assert_ne!(key, max_per_investor_cap());
        assert_ne!(key, unique_funder_count());
        assert_ne!(key, investor_index());
        assert_ne!(key, funding_deadline());
        assert_ne!(key, funding_close_snapshot());
        assert_ne!(key, funding_token());
        assert_ne!(key, funding_token_scale());
        assert_ne!(key, callback_nonce());
        assert_ne!(key, released_amount());
        assert_ne!(key, investor_contribution(investor.clone()));
        assert_ne!(key, investor_effective_yield(investor.clone()));
        assert_ne!(key, investor_claim_not_before(investor.clone()));
        assert_ne!(key, investor_claimed(investor.clone()));
    }
}

//// ---------------------------------------------------------------------------
/// Regression: the collateral pledge key must be constructed through the
/// dedicated helper so all three collateral entrypoints share one definition.
/// This test pins the constructor to the expected variant and ensures it is
/// distinct from every other key family.
#[test]
fn collateral_pledge_key_matches_variant() {
    let env = Env::default();
    let investor = Address::generate(&env);
    assert_eq!(
        collateral_pledge_key(investor.clone()),
        DataKey::SmeCollateralPledge(investor)
    );
}

#[test]
fn collateral_pledge_key_is_deterministic() {
    let env = Env::default();
    let investor = Address::generate(&env);
    assert_eq!(
        collateral_pledge_key(investor.clone()),
        collateral_pledge_key(investor.clone())
    );
}

#[test]
fn collateral_pledge_key_distinct_investors_differ() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    assert_ne!(collateral_pledge_key(a), collateral_pledge_key(b));
}

#[test]
fn collateral_pledge_key_does_not_collide_with_investor_keys() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let collateral = collateral_pledge_key(investor.clone());
    assert_ne!(collateral, investor_contribution(investor.clone()));
    assert_ne!(collateral, investor_effective_yield(investor.clone()));
    assert_ne!(collateral, investor_claim_not_before(investor.clone()));
    assert_ne!(collateral, investor_claimed(investor.clone()));
    assert_ne!(collateral, callback_context(0));
    assert_ne!(collateral, funding_token());
}
