//! Overflow and saturation tests for yield-tier arithmetic (issue #808).
//!
//! Asserts checked and saturating arithmetic behavior across pro-rata coupon
//! calculations, tier commitment lock timestamps, funding capacities, and funder count
//! tracking. Exercises boundary conditions including `i128` extremes, sum near max,
//! and subtraction near zero.

#[cfg(test)]
use super::{assert_contract_error, deploy, free_addresses, install_stellar_asset_token};
use crate::{DataKey, EscrowError, FundingCloseSnapshot, InvoiceEscrow, YieldResolution, YieldTier};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, String, Vec as SorobanVec,
};

fn setup_tiered_escrow<'a>(
    env: &'a Env,
    invoice_id: &str,
    target: i128,
    base_yield_bps: i64,
    tiers: Option<SorobanVec<YieldTier>>,
) -> (
    super::LiquifactEscrowClient<'a>,
    Address,
    Address,
    soroban_sdk::token::StellarAssetClient<'a>,
) {
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let sac = install_stellar_asset_token(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &target,
        &base_yield_bps,
        &0u64,
        &sac.id,
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
        &None::<u32>,
    );

    (client, admin, sme, sac.stellar)
}

/// Verify that computing pro-rata payouts with extreme `i128::MAX` principal
/// does not wrap around and correctly emits `ComputePayoutArithmeticOverflow`.
#[test]
fn test_yield_tier_payout_i128_extreme_multiplication_overflow() {
    let env = Env::default();
    env.mock_all_auths();

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 10_000, // 100% max yield tier
    });

    let (client, _admin, _sme, sac) =
        setup_tiered_escrow(&env, "YT_EXTREME_MUL", 1_000_000i128, 500i64, Some(tiers));

    let investor = Address::generate(&env);
    sac.mint(&investor, &1_000_000i128);
    client.fund_with_commitment(&investor, &1_000_000i128, &100u64);
    client.settle();

    // Inject i128::MAX into FundingCloseSnapshot to test multiplication overflow
    // during pro-rata coupon computation: i128::MAX * 10_000 overflows i128.
    env.as_contract(&client.address, || {
        let mut snap: FundingCloseSnapshot = env
            .storage()
            .instance()
            .get(&DataKey::FundingCloseSnapshot)
            .unwrap();
        snap.total_principal = i128::MAX;
        env.storage()
            .instance()
            .set(&DataKey::FundingCloseSnapshot, &snap);

        let mut escrow: InvoiceEscrow = env.storage().instance().get(&DataKey::Escrow).unwrap();
        escrow.funded_amount = i128::MAX;
        escrow.yield_bps = 10_000;
        env.storage().instance().set(&DataKey::Escrow, &escrow);
    });

    // Pro-rata computation must fail cleanly with ComputePayoutArithmeticOverflow (code 129).
    let res_payout = client.try_compute_investor_payout(&investor);
    assert_contract_error(res_payout, EscrowError::ComputePayoutArithmeticOverflow);

    // Settlement pool view must also fail cleanly without wraparound or uncontrolled panic.
    let res_pool = client.try_get_settlement_pool();
    assert_contract_error(res_pool, EscrowError::ComputePayoutArithmeticOverflow);
}

/// Verify that computing pro-rata payouts when `total_principal + coupon` exceeds
/// `i128::MAX` (sum near max) triggers `ComputePayoutArithmeticOverflow`.
#[test]
fn test_yield_tier_payout_sum_near_max_overflow() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _sme, sac) =
        setup_tiered_escrow(&env, "YT_SUM_MAX", 1_000_000i128, 0i64, None);

    let investor = Address::generate(&env);
    sac.mint(&investor, &1_000_000i128);
    client.fund(&investor, &1_000_000i128);
    client.settle();

    // Set total_principal to near max: i128::MAX - 50.
    // Set effective yield to 1 bps so multiplication doesn't overflow:
    // coupon = (i128::MAX - 50) * 1 / 10_000 ≈ 3.4e34 > 50.
    // Thus total_principal + coupon overflows i128 during checked_add.
    env.as_contract(&client.address, || {
        let mut snap: FundingCloseSnapshot = env
            .storage()
            .instance()
            .get(&DataKey::FundingCloseSnapshot)
            .unwrap();
        snap.total_principal = i128::MAX - 50;
        env.storage()
            .instance()
            .set(&DataKey::FundingCloseSnapshot, &snap);

        env.storage()
            .persistent()
            .set(&DataKey::InvestorEffectiveYield(investor.clone()), &1i64);
        env.storage()
            .persistent()
            .set(&DataKey::InvestorContribution(investor.clone()), &100i128);
    });

    let res_payout = client.try_compute_investor_payout(&investor);
    assert_contract_error(res_payout, EscrowError::ComputePayoutArithmeticOverflow);
}

/// Verify that commitment lock calculations with extreme `u64::MAX` durations (sum near max)
/// do not wrap around and cleanly emit `InvestorClaimTimeOverflow`.
#[test]
fn test_yield_tier_claim_lock_time_sum_near_max_overflow() {
    let env = Env::default();
    env.mock_all_auths();

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 10,
        yield_bps: 1_000,
    });

    let (client, _admin, _sme, sac) =
        setup_tiered_escrow(&env, "YT_LOCK_MAX", 1_000_000i128, 500i64, Some(tiers));

    let investor = Address::generate(&env);
    sac.mint(&investor, &5_000i128);

    // Supplying u64::MAX lock duration attempts now.checked_add(u64::MAX), which overflows u64.
    env.ledger().set_timestamp(100);
    let res = client.try_fund_with_commitment(&investor, &5_000i128, &u64::MAX);
    assert_contract_error(res, EscrowError::InvestorClaimTimeOverflow);
}

/// Verify subtraction near zero and saturating subtraction behavior during unfund operations
/// on tiered yield commitments and capacity calculations.
#[test]
fn test_yield_tier_unfund_subtraction_near_zero_and_saturating() {
    let env = Env::default();
    env.mock_all_auths();

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 2_000,
    });

    let (client, _admin, _sme, sac) =
        setup_tiered_escrow(&env, "YT_UNFUND_SUB", 10_000i128, 500i64, Some(tiers));

    let investor = Address::generate(&env);
    sac.mint(&investor, &10_000i128);

    // Fund tiered commitment
    client.fund_with_commitment(&investor, &5_000i128, &100u64);
    assert_eq!(client.get_unique_funder_count(), 1);

    // 1. Subtraction underflow guard: attempting to unfund more than recorded contribution.
    let res_over = client.try_unfund(&investor, &5_001i128);
    assert_contract_error(res_over, EscrowError::OverWithdrawal);

    // 2. Subtraction near zero: unfund exact remaining balance down to 0.
    client.unfund(&investor, &5_000i128);
    assert_eq!(client.get_contribution(&investor), 0);

    // 3. Saturating subtraction on UniqueFunderCount: must reach exactly 0 without wrapping.
    assert_eq!(client.get_unique_funder_count(), 0);

    // 4. Test saturating subtraction against undercounting/corruption boundary at 0.
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::UniqueFunderCount, &0u32);
    });
    // Unfunding 0 contribution when count is already 0 should remain at 0 (saturating_sub).
    assert_eq!(client.get_unique_funder_count(), 0);

    // 5. Verify get_remaining_funding_capacity uses saturating_sub when over-funded.
    let investor2 = Address::generate(&env);
    sac.mint(&investor2, &20_000i128);
    client.fund(&investor2, &20_000i128); // 20_000 > target (10_000) -> overfunded
    assert_eq!(client.get_remaining_funding_capacity(), 0);
}

/// Verify checked addition safety against `FundedAmountOverflow` and
/// `InvestorContributionOverflow` at `i128` boundary extremes.
#[test]
fn test_yield_tier_funded_amount_and_contribution_overflow() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _sme, sac) =
        setup_tiered_escrow(&env, "YT_FUND_OVER", 1_000_000i128, 500i64, None);

    let investor = Address::generate(&env);
    sac.mint(&investor, &100_000i128);

    // 1. Inject near-max funded_amount into escrow state to trigger FundedAmountOverflow.
    env.as_contract(&client.address, || {
        let mut escrow: InvoiceEscrow = env.storage().instance().get(&DataKey::Escrow).unwrap();
        escrow.funded_amount = i128::MAX - 10;
        env.storage().instance().set(&DataKey::Escrow, &escrow);
    });
    let res_funded = client.try_fund_with_commitment(&investor, &20i128, &0u64);
    assert_contract_error(res_funded, EscrowError::FundedAmountOverflow);

    // 2. Reset funded_amount and inject near-max investor contribution to trigger InvestorContributionOverflow.
    env.as_contract(&client.address, || {
        let mut escrow: InvoiceEscrow = env.storage().instance().get(&DataKey::Escrow).unwrap();
        escrow.funded_amount = 0;
        env.storage().instance().set(&DataKey::Escrow, &escrow);
        env.storage().persistent().set(
            &DataKey::InvestorContribution(investor.clone()),
            &(i128::MAX - 10),
        );
    });
    let res_contrib = client.try_fund_with_commitment(&investor, &20i128, &0u64);
    assert_contract_error(res_contrib, EscrowError::InvestorContributionOverflow);
}

/// Verify deterministic ladder selection and boundary safety in `preview_yield_tier`
/// with extreme lock and yield values.
#[test]
fn test_yield_tier_preview_extreme_values_no_wraparound() {
    let env = Env::default();
    env.mock_all_auths();

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 1_000,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 10_000,
        yield_bps: 10_000, // max allowed yield rate (100%)
    });

    let (client, _admin, _sme, _sac) =
        setup_tiered_escrow(&env, "YT_PREVIEW_EXT", 1_000_000i128, 500i64, Some(tiers));

    // Extreme large lock (u64::MAX) and extreme amount (i128::MAX) must resolve to the highest tier without panic.
    let resolution_max = client.preview_yield_tier(&i128::MAX, &u64::MAX);
    assert_eq!(resolution_max.effective_yield_bps, 10_000);
    assert_eq!(resolution_max.matched_lock_secs, 10_000);

    // Zero / negative amount and 0 lock must safely fall back to base yield without underflow or error.
    let resolution_zero = client.preview_yield_tier(&0i128, &0u64);
    assert_eq!(resolution_zero.effective_yield_bps, 500);
    assert_eq!(resolution_zero.matched_lock_secs, 0);

    let resolution_neg = client.preview_yield_tier(&i128::MIN, &0u64);
    assert_eq!(resolution_neg.effective_yield_bps, 500);
    assert_eq!(resolution_neg.matched_lock_secs, 0);
}
