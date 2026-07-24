//! Yield-tier boundary tests for the LiquiFact escrow contract.
//!
//! Covers the accept/reject boundaries of the optional yield-tier table validated at
//[ `init`, the tier-selection logic in `fund_with_commitment` and `preview_yield_tier`,
//! and the rejection guards for second-tiered deposits and maturity-bound commitment locks.
//!
//! # State model recap
//! - Yield-tier table is **immutable** after `init` (stored in `DataKey::YieldTierTable`).
//! - `fund_with_commitment` performs **first-deposit-only** tier selection via
//!   `effective_yield_for_commitment`.
//! - `preview_yield_tier` exposes the same selection logic as a read-only view.
//! - A second call to `fund_with_commitment` (or `fund` after tiered deposit) fails with
//!   `TieredSecondDeposit` — the selection window is permanently closed after leg one.
//! - `committed_lock_secs` must not exceed escrow `maturity` when both are non-zero
//!   (`CommitmentLockExceedsMaturity`).

use super::{
    assert_contract_error, default_init, deploy, deploy_with_id, free_addresses,
    install_stellar_asset_token, setup, LiquifactEscrowClient, StellarTestToken, TARGET,
};
use crate::{EscrowError, InvoiceEscrow, LiquifactEscrow, YieldTier};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    token::StellarAssetClient,
    Address, Env, Event, String, Vec as SorobanVec,
};

const BASE_YIELD: i64 = 800;
const TIER_TABLE_INV_ID: &str = "TIER001";

// Helper: build a standard tier ladder for boundary tests.
fn three_tier_ladder(env: &Env) -> SorobanVec<YieldTier> {
    let mut tiers = SorobanVec::new(env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 1000,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 300,
        yield_bps: 1200,
    });
    tiers
}

// Helper: build a two-tier ladder for simpler boundary tests.
fn two_tier_ladder(env: &Env) -> SorobanVec<YieldTier> {
    let mut tiers = SorobanVec::new(env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 1000,
    });
    tiers
}

// Helper: initialize an escrow with the given tiers and return client.
// Uses mock token for simplicity - caller must ensure investor has tokens.
fn init_with_tiers<'a>(
    env: &'a Env,
    tiers: Option<SorobanVec<YieldTier>>,
    invoice_id: &str,
) -> LiquifactEscrowClient<'a> {
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let (token, treasury) = free_addresses(env);

    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &TARGET,
        &BASE_YIELD,
        &0u64,
        &token,
        &None,
        &treasury,
        &tiers,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    client
}

// Helper: initialize with real token (SAC) for tests that call fund_with_commitment.
// Returns (client, sac_admin) where sac_admin can mint tokens to investors.
fn init_with_tiers_and_sac<'a>(
    env: &'a Env,
    tiers: Option<SorobanVec<YieldTier>>,
    invoice_id: &str,
) -> (LiquifactEscrowClient<'a>, StellarAssetClient<'a>) {
    env.mock_all_auths();
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(env, &token_id);

    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &TARGET,
        &BASE_YIELD,
        &0u64,
        &token_id,
        &None,
        &treasury,
        &tiers,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    (client, sac_admin)
}

// Helper: mint tokens to investor and call fund_with_commitment.
fn mint_and_fund_with_commitment(
    client: &LiquifactEscrowClient<'_>,
    sac_admin: &StellarAssetClient<'_>,
    investor: &Address,
    amount: i128,
    lock: u64,
) {
    sac_admin.mint(investor, &amount);
    client.fund_with_commitment(investor, &amount, &lock);
}

// ──────────────────────────────────────────────────────────────────────────────
// Tier table validation at `init` — accept boundaries
// ──────────────────────────────────────────────────────────────────────────────

/// `yield_bps == 0` is valid when base yield is also 0.
#[test]
fn test_init_accepts_yield_bps_zero() {
    let env = Env::default();
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 0,
    });
    // Should NOT panic with base_yield = 0
    let env2 = Env::default();
    env2.mock_all_auths();
    let mut tiers2 = SorobanVec::new(&env2);
    tiers2.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 0,
    });
    let client = deploy(&env2);
    let admin = Address::generate(&env2);
    let sme = Address::generate(&env2);
    let (token, treasury) = free_addresses(&env2);
    client.init(
        &admin,
        &String::from_str(&env2, "T0"),
        &sme,
        &TARGET,
        &0i64, // base_yield = 0
        &0u64,
        &token,
        &None,
        &treasury,
        &Some(tiers2),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
}

/// `yield_bps == 10_000` is valid (within 0..=10_000 range).
#[test]
fn test_init_accepts_yield_bps_max() {
    let env = Env::default();
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 10_000,
    });
    // Should NOT panic
    init_with_tiers(&env, Some(tiers), "T10K");
}

/// `yield_bps == base_yield` is valid (tier equals base).
#[test]
fn test_init_accepts_yield_bps_equals_base() {
    let env = Env::default();
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: BASE_YIELD,
    });
    // Should NOT panic
    init_with_tiers(&env, Some(tiers), "TEQ");
}

/// `min_lock_secs` strictly increasing is valid.
#[test]
fn test_init_accepts_strictly_increasing_locks() {
    let env = Env::default();
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 1000,
    });
    // Should NOT panic
    init_with_tiers(&env, Some(tiers), "TINC");
}

/// `yield_bps` non-decreasing is valid (equal yields allowed).
#[test]
fn test_init_accepts_non_decreasing_yields() {
    let env = Env::default();
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 900,
    });
    // Should NOT panic
    init_with_tiers(&env, Some(tiers), "TNDEC");
}

/// Empty tier table is valid (no tiers configured).
#[test]
fn test_init_accepts_empty_tier_table() {
    let env = Env::default();
    let empty_tiers: SorobanVec<YieldTier> = SorobanVec::new(&env);
    // Should NOT panic
    init_with_tiers(&env, Some(empty_tiers), "TEMPTY");
}

/// `None` tier table is valid (no tiers configured).
#[test]
fn test_init_accepts_none_tier_table() {
    let env = Env::default();
    // Should NOT panic
    init_with_tiers(&env, None, "TNONE");
}

// ──────────────────────────────────────────────────────────────────────────────
// Tier table validation at `init` — reject boundaries (typed errors)
// ──────────────────────────────────────────────────────────────────────────────

/// `yield_bps == 10_001` is invalid — exceeds u16-ish range.
#[test]
fn test_init_rejects_yield_bps_above_max() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 10_001,
    });

    let result = client.try_init(
        &admin,
        &String::from_str(&env, "T10K1"),
        &sme,
        &TARGET,
        &BASE_YIELD,
        &0u64,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_contract_error(result, EscrowError::TierYieldOutOfRange);
}

/// `yield_bps < base_yield` is invalid.
#[test]
fn test_init_rejects_yield_bps_below_base() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: BASE_YIELD - 1,
    });

    let result = client.try_init(
        &admin,
        &String::from_str(&env, "TLOW"),
        &sme,
        &TARGET,
        &BASE_YIELD,
        &0u64,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_contract_error(result, EscrowError::TierYieldBelowBase);
}

/// `min_lock_secs` equal (not strictly increasing) is invalid.
#[test]
fn test_init_rejects_equal_locks() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 1000,
    });

    let result = client.try_init(
        &admin,
        &String::from_str(&env, "TEQLOCK"),
        &sme,
        &TARGET,
        &BASE_YIELD,
        &0u64,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_contract_error(result, EscrowError::TierLockNotIncreasing);
}

/// `min_lock_secs` decreasing is invalid.
#[test]
fn test_init_rejects_decreasing_locks() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 900,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 1000,
    });

    let result = client.try_init(
        &admin,
        &String::from_str(&env, "TDECLOCK"),
        &sme,
        &TARGET,
        &BASE_YIELD,
        &0u64,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_contract_error(result, EscrowError::TierLockNotIncreasing);
}

/// `yield_bps` decreasing across tiers is invalid.
#[test]
fn test_init_rejects_decreasing_yields() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 1000,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 900,
    });

    let result = client.try_init(
        &admin,
        &String::from_str(&env, "TDECYIELD"),
        &sme,
        &TARGET,
        &BASE_YIELD,
        &0u64,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_contract_error(result, EscrowError::TierYieldNotNonDecreasing);
}

// ──────────────────────────────────────────────────────────────────────────────
// `fund_with_commitment` tier selection — accept boundaries (use SAC)
// ──────────────────────────────────────────────────────────────────────────────

/// `lock == 0` falls back to base yield (no tier matched).
#[test]
fn test_fund_commitment_lock_zero_returns_base_yield() {
    let env = Env::default();
    let (client, sac_admin) = init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "LOCK0");

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &0u64);

    assert_eq!(client.get_investor_yield_bps(&inv), BASE_YIELD);
    assert_eq!(client.get_investor_claim_not_before(&inv), 0u64);
}

/// `lock < first_tier.min_lock_secs` falls back to base yield (but claim gate = now + lock).
#[test]
fn test_fund_commitment_below_first_tier_returns_base_yield() {
    let env = Env::default();
    let (client, sac_admin) = init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "BELOW");

    let inv = Address::generate(&env);
    let now = env.ledger().timestamp();
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &50u64); // 50 < 100

    assert_eq!(client.get_investor_yield_bps(&inv), BASE_YIELD);
    // claim gate uses the committed lock duration, not the matched tier
    assert_eq!(client.get_investor_claim_not_before(&inv), now + 50);
}

/// `lock == first_tier.min_lock_secs` matches first tier.
#[test]
fn test_fund_commitment_exactly_first_tier() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "EXACT1");

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &100u64); // exactly 100

    assert_eq!(client.get_investor_yield_bps(&inv), 900);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 100
    );
}

/// `lock` between first and second tier matches first tier.
#[test]
fn test_fund_commitment_between_tiers_matches_lower() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "BETWEEN");

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &150u64); // between 100 and 200

    assert_eq!(client.get_investor_yield_bps(&inv), 900);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 150
    );
}

/// `lock == second_tier.min_lock_secs` matches second tier.
#[test]
fn test_fund_commitment_exactly_second_tier() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "EXACT2");

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &200u64);

    assert_eq!(client.get_investor_yield_bps(&inv), 1000);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 200
    );
}

/// `lock` between second and third tier matches second tier.
#[test]
fn test_fund_commitment_between_second_third_matches_second() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "BETWEEN2");

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &250u64); // between 200 and 300

    assert_eq!(client.get_investor_yield_bps(&inv), 1000);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 250
    );
}

/// `lock == third_tier.min_lock_secs` matches third tier.
#[test]
fn test_fund_commitment_exactly_third_tier() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "EXACT3");

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &300u64);

    assert_eq!(client.get_investor_yield_bps(&inv), 1200);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 300
    );
}

/// `lock > highest_tier.min_lock_secs` matches highest tier.
#[test]
fn test_fund_commitment_above_highest_tier_matches_highest() {
    let env = Env::default();
    let (client, sac_admin) = init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "ABOVE");

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &500u64); // well above 300

    assert_eq!(client.get_investor_yield_bps(&inv), 1200);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 500
    );
}

/// No tier table configured — falls back to base yield.
#[test]
fn test_fund_commitment_no_tier_table_returns_base() {
    let env = Env::default();
    let (client, sac_admin) = init_with_tiers_and_sac(&env, None, "NOTIER");

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &1000u64); // large lock, no tiers

    assert_eq!(client.get_investor_yield_bps(&inv), BASE_YIELD);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 1000
    );
}

/// Empty tier table — falls back to base yield.
#[test]
fn test_fund_commitment_empty_tier_table_returns_base() {
    let env = Env::default();
    let empty_tiers: SorobanVec<YieldTier> = SorobanVec::new(&env);
    let (client, sac_admin) = init_with_tiers_and_sac(&env, Some(empty_tiers), "EMPTYTIER");

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &1000u64);

    assert_eq!(client.get_investor_yield_bps(&inv), BASE_YIELD);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 1000
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// `fund_with_commitment` — reject boundaries (second deposit)
// ──────────────────────────────────────────────────────────────────────────────
// `fund_with_commitment` — reject boundaries (second deposit)
// ──────────────────────────────────────────────────────────────────────────────

/// Second `fund_with_commitment` by same investor fails with `TieredSecondDeposit`.
#[test]
fn test_fund_commitment_twice_rejected() {
    let env = Env::default();
    let (client, sac_admin) = init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "TWICE");

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &100u64);

    let result = client.try_fund_with_commitment(&inv, &1_000i128, &200u64);
    assert_contract_error(result, EscrowError::TieredSecondDeposit);
}

/// `fund_with_commitment` then plain `fund` by same investor is ALLOWED (adds principal at locked tier rate).
#[test]
fn test_fund_commitment_then_fund_allowed() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "COMTHENFUND");

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &2_000i128); // enough for both deposits
    client.fund_with_commitment(&inv, &1_000i128, &100u64); // First: tiered at 100s (yield 900)

    // Second: plain fund - should succeed, adds at same tier rate
    client.fund(&inv, &1_000i128);

    assert_eq!(client.get_investor_yield_bps(&inv), 900); // Still tier 0 yield
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 100
    );
}

/// Plain `fund` then `fund_with_commitment` by same investor fails with `TieredSecondDeposit`.
#[test]
fn test_fund_then_commitment_rejected() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "FUNDTHENCOM");

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &2_000i128);
    client.fund(&inv, &1_000i128);

    let result = client.try_fund_with_commitment(&inv, &1_000i128, &100u64);
    assert_contract_error(result, EscrowError::TieredSecondDeposit);
}

/// Second `fund_with_commitment` with different lock still fails.
#[test]
fn test_fund_commitment_twice_different_lock_rejected() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "TWICEDIFF");

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &2_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &100u64);

    let result = client.try_fund_with_commitment(&inv, &1_000i128, &50u64);
    assert_contract_error(result, EscrowError::TieredSecondDeposit);
}

// ──────────────────────────────────────────────────────────────────────────────
// `fund_with_commitment` — CommitmentLockExceedsMaturity boundaries
// ──────────────────────────────────────────────────────────────────────────────

/// `claim_nb == maturity` is accepted (inclusive boundary).
#[test]
fn test_commitment_lock_equal_maturity_accepted() {
    let env = Env::default();
    let tiers = two_tier_ladder(&env);
    let maturity = 1000u64;
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &String::from_str(&env, "LOCKMAT"),
        &sme,
        &TARGET,
        &BASE_YIELD,
        &maturity,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let inv = Address::generate(&env);
    // commit 1000s lock, maturity is 1000s -> claim_nb = now + 1000 == maturity -> OK
    client.fund_with_commitment(&inv, &1_000i128, &maturity);

    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + maturity
    );
}

/// `claim_nb == maturity + 1` is rejected with `CommitmentLockExceedsMaturity`.
#[test]
fn test_commitment_lock_exceeds_maturity_rejected() {
    let env = Env::default();
    let tiers = two_tier_ladder(&env);
    let maturity = 1000u64;
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &String::from_str(&env, "LOCKMAT1"),
        &sme,
        &TARGET,
        &BASE_YIELD,
        &maturity,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let inv = Address::generate(&env);
    // commit 1001s lock, maturity is 1000s -> claim_nb = now + 1001 > maturity -> REJECT
    let result = client.try_fund_with_commitment(&inv, &1_000i128, &(maturity + 1));
    assert_contract_error(result, EscrowError::CommitmentLockExceedsMaturity);
}

/// `maturity == 0` (no maturity lock) accepts any lock duration.
#[test]
fn test_commitment_lock_no_maturity_accepts_any() {
    let env = Env::default();
    let tiers = two_tier_ladder(&env);
    let client = init_with_tiers(&env, Some(tiers), "NOMAT");

    let inv = Address::generate(&env);
    // Large lock, but maturity == 0 -> no bound
    client.fund_with_commitment(&inv, &1_000i128, &100_000u64);

    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 100_000
    );
}

/// `lock == 0` with maturity > 0 is accepted (no claim gate).
#[test]
fn test_commitment_lock_zero_with_maturity_accepted() {
    let env = Env::default();
    let tiers = two_tier_ladder(&env);
    let maturity = 1000u64;
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &String::from_str(&env, "LOCKZERO"),
        &sme,
        &TARGET,
        &BASE_YIELD,
        &maturity,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let inv = Address::generate(&env);
    client.fund_with_commitment(&inv, &1_000i128, &0u64);

    assert_eq!(client.get_investor_claim_not_before(&inv), 0u64);
}

// ──────────────────────────────────────────────────────────────────────────────
// `preview_yield_tier` — boundary equivalence with `fund_with_commitment`
// ──────────────────────────────────────────────────────────────────────────────

/// `preview_yield_tier` matches `fund_with_commitment` for lock=0 (base case).
#[test]
fn test_preview_matches_actual_lock_zero() {
    let env = Env::default();
    let (client, sac_admin) = init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "PV0");

    let (preview_bps, preview_lock) = client.preview_yield_tier(&1_000i128, &0u64);
    assert_eq!(preview_bps, BASE_YIELD);
    assert_eq!(preview_lock, 0);

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &0u64);
    assert_eq!(client.get_investor_yield_bps(&inv), preview_bps);
}

/// `preview_yield_tier` matches `fund_with_commitment` for lock below first tier.
#[test]
fn test_preview_matches_actual_below_first_tier() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "PVBELOW");

    let (preview_bps, preview_lock) = client.preview_yield_tier(&1_000i128, &50u64);
    assert_eq!(preview_bps, BASE_YIELD);
    assert_eq!(preview_lock, 0);

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &50u64);
    assert_eq!(client.get_investor_yield_bps(&inv), preview_bps);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 50
    );
}

/// `preview_yield_tier` matches `fund_with_commitment` at exact first tier boundary.
#[test]
fn test_preview_matches_actual_at_first_tier() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "PVEXACT1");

    let (preview_bps, preview_lock) = client.preview_yield_tier(&1_000i128, &100u64);
    assert_eq!(preview_bps, 900);
    assert_eq!(preview_lock, 100);

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &100u64);
    assert_eq!(client.get_investor_yield_bps(&inv), preview_bps);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 100
    );
}

/// `preview_yield_tier` matches `fund_with_commitment` between tiers.
#[test]
fn test_preview_matches_actual_between_tiers() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "PVBETWEEN");

    let (preview_bps, preview_lock) = client.preview_yield_tier(&1_000i128, &150u64);
    assert_eq!(preview_bps, 900);
    assert_eq!(preview_lock, 100);

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &150u64);
    assert_eq!(client.get_investor_yield_bps(&inv), preview_bps);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 150
    );
}

/// `preview_yield_tier` matches `fund_with_commitment` at exact second tier boundary.
#[test]
fn test_preview_matches_actual_at_second_tier() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "PVEXACT2");

    let (preview_bps, preview_lock) = client.preview_yield_tier(&1_000i128, &200u64);
    assert_eq!(preview_bps, 1000);
    assert_eq!(preview_lock, 200);

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &200u64);
    assert_eq!(client.get_investor_yield_bps(&inv), preview_bps);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 200
    );
}

/// `preview_yield_tier` matches `fund_with_commitment` at exact third tier boundary.
#[test]
fn test_preview_matches_actual_at_third_tier() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "PVEXACT3");

    let (preview_bps, preview_lock) = client.preview_yield_tier(&1_000i128, &300u64);
    assert_eq!(preview_bps, 1200);
    assert_eq!(preview_lock, 300);

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &300u64);
    assert_eq!(client.get_investor_yield_bps(&inv), preview_bps);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 300
    );
}

/// `preview_yield_tier` matches `fund_with_commitment` above highest tier.
#[test]
fn test_preview_matches_actual_above_highest_tier() {
    let env = Env::default();
    let (client, sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "PVABOVE");

    let (preview_bps, preview_lock) = client.preview_yield_tier(&1_000i128, &500u64);
    assert_eq!(preview_bps, 1200);
    assert_eq!(preview_lock, 300);

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &500u64);
    assert_eq!(client.get_investor_yield_bps(&inv), preview_bps);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 500
    );
}

/// `preview_yield_tier` with no tier table matches base yield.
#[test]
fn test_preview_no_tiers_matches_base() {
    let env = Env::default();
    let (client, sac_admin) = init_with_tiers_and_sac(&env, None, "PVNOTIER");

    let (preview_bps, preview_lock) = client.preview_yield_tier(&1_000i128, &1000u64);
    assert_eq!(preview_bps, BASE_YIELD);
    assert_eq!(preview_lock, 0);

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &1000u64);
    assert_eq!(client.get_investor_yield_bps(&inv), preview_bps);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 1000
    );
}

/// `preview_yield_tier` with empty tier table matches base yield.
#[test]
fn test_preview_empty_tiers_matches_base() {
    let env = Env::default();
    let empty_tiers: SorobanVec<YieldTier> = SorobanVec::new(&env);
    let (client, sac_admin) = init_with_tiers_and_sac(&env, Some(empty_tiers), "PVEMPTY");

    let (preview_bps, preview_lock) = client.preview_yield_tier(&1_000i128, &1000u64);
    assert_eq!(preview_bps, BASE_YIELD);
    assert_eq!(preview_lock, 0);

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &1000u64);
    assert_eq!(client.get_investor_yield_bps(&inv), preview_bps);
    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        env.ledger().timestamp() + 1000
    );
}

/// `preview_yield_tier` amount parameter is unused (lock-only selection).
#[test]
fn test_preview_amount_unused() {
    let env = Env::default();
    let (client, _sac_admin) =
        init_with_tiers_and_sac(&env, Some(three_tier_ladder(&env)), "PVAMT");

    let (bps1, lock1) = client.preview_yield_tier(&100i128, &100u64);
    let (bps2, lock2) = client.preview_yield_tier(&10_000_000i128, &100u64);
    assert_eq!(bps1, bps2);
    assert_eq!(lock1, lock2);
}

// ──────────────────────────────────────────────────────────────────────────────
// `get_yield_tiers` read view — boundary tests
// ──────────────────────────────────────────────────────────────────────────────

/// `get_yield_tiers` returns empty vec when no tiers configured.
#[test]
fn test_get_yield_tiers_empty_when_none() {
    let env = Env::default();
    let client = init_with_tiers(&env, None, "GTNONE");

    let tiers = client.get_yield_tiers();
    assert_eq!(tiers.len(), 0);
}

/// `get_yield_tiers` returns empty vec when empty table configured.
#[test]
fn test_get_yield_tiers_empty_when_empty_table() {
    let env = Env::default();
    let empty_tiers: SorobanVec<YieldTier> = SorobanVec::new(&env);
    let client = init_with_tiers(&env, Some(empty_tiers), "GTEMPTY");

    let tiers = client.get_yield_tiers();
    assert_eq!(tiers.len(), 0);
}

/// `get_yield_tiers` returns single tier correctly.
#[test]
fn test_get_yield_tiers_single_tier() {
    let env = Env::default();
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });
    let client = init_with_tiers(&env, Some(tiers), "GT1");

    let result = client.get_yield_tiers();
    assert_eq!(result.len(), 1);
    assert_eq!(result.get(0).unwrap().min_lock_secs, 100);
    assert_eq!(result.get(0).unwrap().yield_bps, 900);
}

/// `get_yield_tiers` preserves order of tiers.
#[test]
fn test_get_yield_tiers_preserves_order() {
    let env = Env::default();
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 1000,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 300,
        yield_bps: 1200,
    });
    let client = init_with_tiers(&env, Some(tiers), "GTORDER");

    let result = client.get_yield_tiers();
    assert_eq!(result.len(), 3);
    assert_eq!(result.get(0).unwrap().min_lock_secs, 100);
    assert_eq!(result.get(1).unwrap().min_lock_secs, 200);
    assert_eq!(result.get(2).unwrap().min_lock_secs, 300);
}

/// `get_yield_tiers` is a pure read (no state change).
#[test]
fn test_get_yield_tiers_pure_read_no_state_change() {
    let env = Env::default();
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900,
    });
    let client = init_with_tiers(&env, Some(tiers), "GTPURE");

    // Call twice — should not mutate state
    let t1 = client.get_yield_tiers();
    let t2 = client.get_yield_tiers();
    assert_eq!(t1, t2);
}

// ──────────────────────────────────────────────────────────────────────────────
// Event emission for tiered deposits
// ──────────────────────────────────────────────────────────────────────────────

/// `EscrowFunded` event carries correct tier yield and lock for tiered deposit.
#[test]
fn test_tiered_deposit_event_emits_correct_fields() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    let tiers = three_tier_ladder(&env);

    // Use SAC for the client so we can mint tokens to investor
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let sac_id = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &sac_id);

    client.init(
        &admin,
        &String::from_str(&env, "EVT_TIER"),
        &sme,
        &TARGET,
        &BASE_YIELD,
        &0u64,
        &sac_id,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &150u64); // matches tier 0 (100s, 900 bps)

    let all_events = env.events().all();
    let events = all_events.events();
    let funded_event = events.last().expect("EscrowFunded event expected").clone();

    // Verify tier fields in event
    let expected = crate::EscrowFunded {
        name: symbol_short!("funded"),
        invoice_id: symbol_short!("EVT_TIER"),
        investor: inv.clone(),
        amount: 1_000i128,
        funded_amount: 1_000i128,
        status: 0,
        investor_effective_yield_bps: 900,
        tier_lock_secs: 100,
    };
    assert_eq!(funded_event, expected.to_xdr(&env, &contract_id));
}

/// `EscrowFunded` event carries base yield and zero lock for non-tiered deposit with tiers configured.
#[test]
fn test_base_deposit_event_with_tiers_configured() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    let tiers = three_tier_ladder(&env);

    // Use SAC for the client
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let sac_id = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &sac_id);

    client.init(
        &admin,
        &String::from_str(&env, "EVT_BASE"),
        &sme,
        &TARGET,
        &BASE_YIELD,
        &0u64,
        &sac_id,
        &None,
        &treasury,
        &Some(three_tier_ladder(&env)),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &50u64); // below first tier -> base

    let all_events = env.events().all();
    let events = all_events.events();
    let funded_event = events.last().expect("EscrowFunded event expected").clone();

    let expected = crate::EscrowFunded {
        name: symbol_short!("funded"),
        invoice_id: symbol_short!("EVT_BASE"),
        investor: inv.clone(),
        amount: 1_000i128,
        funded_amount: 1_000i128,
        status: 0,
        investor_effective_yield_bps: BASE_YIELD,
        tier_lock_secs: 0,
    };
    assert_eq!(funded_event, expected.to_xdr(&env, &contract_id));
}

/// `EscrowFunded` event carries base yield and zero lock when no tiers configured.
#[test]
fn test_event_no_tiers_configured() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);

    client.init(
        &admin,
        &String::from_str(&env, "EVT_NTR"),
        &sme,
        &TARGET,
        &BASE_YIELD,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let inv = Address::generate(&env);
    // Need SAC for fund_with_commitment
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let sac_id = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &sac_id);
    sac_admin.mint(&inv, &1_000i128);
    client.fund_with_commitment(&inv, &1_000i128, &100u64); // lock ignored, no tiers

    let all_events = env.events().all();
    let events = all_events.events();
    let funded_event = events.last().expect("EscrowFunded event expected").clone();

    let expected = crate::EscrowFunded {
        name: symbol_short!("funded"),
        invoice_id: symbol_short!("EVT_NTR"),
        investor: inv.clone(),
        amount: 1_000i128,
        funded_amount: 1_000i128,
        status: 0,
        investor_effective_yield_bps: BASE_YIELD,
        tier_lock_secs: 0,
    };
    assert_eq!(funded_event, expected.to_xdr(&env, &contract_id));
}
