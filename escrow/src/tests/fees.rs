use super::*;
use proptest::prelude::*;

fn setup_perf_fee_env<'a>(
    env: &'a Env,
    target: i128,
    yield_bps: i64,
) -> (
    LiquifactEscrowClient<'a>,
    Address,
    Address,
    Address,
    Address,
    StellarAssetClient<'a>,
) {
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token_id = sac.address();
    let sac_client = StellarAssetClient::new(env, &token_id);

    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &escrow_id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "INV_PERF_1"),
        &sme,
        &target,
        &yield_bps,
        &0u64,
        &token_id,
        &None,
        &treasury,
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

    (client, admin, sme, treasury, escrow_id, sac_client)
}

#[test]
fn test_performance_fee_default_is_zero() {
    let env = Env::default();
    let (client, _, _, _, _, _) = setup_perf_fee_env(&env, 1_000_000, 800);
    assert_eq!(client.get_performance_fee_bps(), 0);
}

#[test]
fn test_set_performance_fee_bps_success_and_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _, _, _, _) = setup_perf_fee_env(&env, 1_000_000, 800);

    let res = client.set_performance_fee_bps(&1_000);
    assert_eq!(res, 1_000);
    assert_eq!(client.get_performance_fee_bps(), 1_000);

    // Verify PerformanceFeeUpdated event
    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if let Ok(fee_event) = PerformanceFeeUpdated::try_from(event) {
            if fee_event.new_fee_bps == 1_000 && fee_event.old_fee_bps == 0 {
                found = true;
                break;
            }
        }
    }
    assert!(found, "PerformanceFeeUpdated event was not found");
}

#[test]
fn test_set_performance_fee_bps_out_of_range() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _) = setup_perf_fee_env(&env, 1_000_000, 800);

    // Boundary: 5_000 is allowed
    assert_eq!(client.set_performance_fee_bps(&5_000), 5_000);

    // 5_001 is out of range
    assert_contract_error(
        client.try_set_performance_fee_bps(&5_001),
        EscrowError::PerformanceFeeBpsOutOfRange,
    );
}

#[test]
fn test_set_performance_fee_locked_after_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, escrow_id, sac) = setup_perf_fee_env(&env, 1_000_000, 800);

    let investor = Address::generate(&env);
    sac.mint(&investor, &500_000);
    client.fund(&investor, &500_000);

    // After partial funding (funded_amount > 0), setter must fail
    assert_contract_error(
        client.try_set_performance_fee_bps(&1_000),
        EscrowError::StatusNotZero,
    );
}

#[test]
fn test_conservation_and_balance_deltas() {
    let env = Env::default();
    env.mock_all_auths();
    let target = 1_000_000i128;
    let yield_bps = 1_000i64; // 10% yield
    let perf_fee_bps = 2_000u32; // 20% performance fee on coupon
    let (client, _, _, treasury, escrow_id, sac) = setup_perf_fee_env(&env, target, yield_bps);

    client.set_performance_fee_bps(&perf_fee_bps);

    let investor = Address::generate(&env);
    sac.mint(&investor, &target);
    client.fund(&investor, &target);

    // Settle pool requires target + yield: 1_000_000 + 100_000 = 1_100_000
    // Mint 100_000 yield to contract
    sac.mint(&escrow_id, &100_000);
    client.settle();

    let gross_payout = client.compute_investor_payout(&investor);
    assert_eq!(gross_payout, 1_100_000);

    // Preview
    let (preview_net, preview_fee) = client.preview_claim_split(&investor);
    let claimable = client.get_claimable_payout(&investor);
    assert_eq!(claimable, preview_net);
    assert_eq!(preview_net + preview_fee, gross_payout);

    // Coupon = 100_000. Fee = 100_000 * 2_000 / 10_000 = 20_000. Net = 1_080_000.
    assert_eq!(preview_fee, 20_000);
    assert_eq!(preview_net, 1_080_000);

    let token_id = sac.address();
    let token_client = TokenClient::new(&env, &token_id);

    let inv_balance_before = token_client.balance(&investor);
    let treasury_balance_before = token_client.balance(&treasury);
    let contract_balance_before = token_client.balance(&escrow_id);

    client.claim_investor_payout(&investor);

    let inv_balance_after = token_client.balance(&investor);
    let treasury_balance_after = token_client.balance(&treasury);
    let contract_balance_after = token_client.balance(&escrow_id);

    assert_eq!(inv_balance_after - inv_balance_before, preview_net);
    assert_eq!(
        treasury_balance_after - treasury_balance_before,
        preview_fee
    );
    assert_eq!(
        contract_balance_before - contract_balance_after,
        gross_payout
    );
    assert_eq!(preview_net + preview_fee, gross_payout);

    // Verify PerformanceFeeCollected event emitted
    let events = env.events().all();
    let mut fee_event_found = false;
    for event in events.iter() {
        if let Ok(collected) = PerformanceFeeCollected::try_from(event) {
            if collected.investor == investor && collected.fee == preview_fee {
                fee_event_found = true;
                break;
            }
        }
    }
    assert!(fee_event_found, "PerformanceFeeCollected event not found");
}

#[test]
fn test_fee_never_touches_principal_zero_yield() {
    let env = Env::default();
    env.mock_all_auths();
    let target = 1_000_000i128;
    let yield_bps = 0i64; // 0% yield
    let perf_fee_bps = 5_000u32; // 50% performance fee
    let (client, _, _, treasury, escrow_id, sac) = setup_perf_fee_env(&env, target, yield_bps);

    client.set_performance_fee_bps(&perf_fee_bps);

    let investor = Address::generate(&env);
    sac.mint(&investor, &target);
    client.fund(&investor, &target);
    client.settle();

    let gross_payout = client.compute_investor_payout(&investor);
    assert_eq!(gross_payout, target);

    let (net, fee) = client.preview_claim_split(&investor);
    assert_eq!(fee, 0);
    assert_eq!(net, target);
    assert_eq!(client.get_claimable_payout(&investor), target);

    let token_id = sac.address();
    let token_client = TokenClient::new(&env, &token_id);
    let treasury_before = token_client.balance(&treasury);

    client.claim_investor_payout(&investor);

    let treasury_after = token_client.balance(&treasury);
    assert_eq!(treasury_after, treasury_before); // Treasury received nothing
    assert_eq!(token_client.balance(&investor), target); // Investor received full principal
}

#[test]
fn test_multiple_investors_split() {
    let env = Env::default();
    env.mock_all_auths();
    let target = 2_000_000i128;
    let yield_bps = 1_000i64; // 10% yield
    let perf_fee_bps = 2_500u32; // 25% performance fee
    let (client, _, _, treasury, escrow_id, sac) = setup_perf_fee_env(&env, target, yield_bps);

    client.set_performance_fee_bps(&perf_fee_bps);

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    sac.mint(&inv1, &1_000_000);
    sac.mint(&inv2, &1_000_000);

    client.fund(&inv1, &1_000_000);
    client.fund(&inv2, &1_000_000);

    sac.mint(&escrow_id, &200_000); // 10% of 2_000_000
    client.settle();

    let (net1, fee1) = client.preview_claim_split(&inv1);
    let (net2, fee2) = client.preview_claim_split(&inv2);

    // Each investor: 1_000_000 principal, 100_000 coupon.
    // Fee = 100_000 * 25% = 25_000. Net = 1_075_000.
    assert_eq!(net1, 1_075_000);
    assert_eq!(fee1, 25_000);
    assert_eq!(net2, 1_075_000);
    assert_eq!(fee2, 25_000);

    client.claim_investor_payout(&inv1);
    client.claim_investor_payout(&inv2);

    let token_id = sac.address();
    let token_client = TokenClient::new(&env, &token_id);
    assert_eq!(token_client.balance(&treasury), 50_000);
    assert_eq!(token_client.balance(&inv1), 1_075_000);
    assert_eq!(token_client.balance(&inv2), 1_075_000);
    assert_eq!(token_client.balance(&escrow_id), 0);
}

proptest! {
    #[test]
    fn prop_fee_le_coupon_and_net_ge_contribution(
        contribution in 1_000i128..10_000_000_000i128,
        yield_rate_bps in 0i64..10_000i64,
        fee_bps in 0u32..=5_000u32,
    ) {
        // Pure arithmetic proptest matching contract formula
        let coupon_gross = contribution
            .checked_mul(yield_rate_bps as i128)
            .unwrap()
            / 10_000;
        let payout = contribution + coupon_gross;

        let coupon = payout.checked_sub(contribution).unwrap().max(0);
        let fee = coupon
            .checked_mul(fee_bps as i128)
            .unwrap()
            / 10_000;
        let net = payout.checked_sub(fee).unwrap();

        prop_assert!(fee <= coupon, "fee {} must be <= coupon {}", fee, coupon);
        prop_assert!(net >= contribution, "net {} must be >= contribution {}", net, contribution);
        prop_assert_eq!(net + fee, payout, "net {} + fee {} must equal payout {}", net, fee, payout);
    }
}
