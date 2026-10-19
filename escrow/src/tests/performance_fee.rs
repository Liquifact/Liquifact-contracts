use super::*;
use crate::{PerformanceFeeCollected, PerformanceFeeUpdated};
use proptest::prelude::*;

/// Inits with a real token, optionally sets the fee, funds one investor fully, mints the coupon
/// into the escrow and settles. Returns (client, token, investor, treasury).
fn settled<'a>(
    env: &'a Env,
    yield_bps: i64,
    fee_bps: u32,
    amount: i128,
) -> (
    LiquifactEscrowClient<'a>,
    StellarTestToken<'a>,
    Address,
    Address,
) {
    let (client, admin, sme) = setup(env);
    let tok = install_stellar_asset_token(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "INV001"),
        &sme,
        &amount,
        &yield_bps,
        &100u64,
        &tok.id,
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
    if fee_bps > 0 {
        client.set_performance_fee_bps(&fee_bps);
    }
    let investor = Address::generate(env);
    tok.stellar.mint(&investor, &amount);
    client.fund(&investor, &amount);
    let coupon = amount * yield_bps as i128 / 10_000;
    tok.stellar.mint(&client.address, &(amount + coupon));
    env.ledger().set_timestamp(101);
    client.settle();
    (client, tok, investor, treasury)
}

#[test]
fn fee_split_conserves_and_moves_balances() {
    let env = Env::default();
    let (client, tok, investor, treasury) = settled(&env, 1000, 2000, 1_000_000);
    let payout = client.compute_investor_payout(&investor);
    assert_eq!(payout, 1_100_000);
    // coupon 100_000 * 20% = 20_000
    assert_eq!(client.preview_claim_split(&investor), (1_080_000, 20_000));
    assert_eq!(client.get_claimable_payout(&investor), 1_080_000);

    let c0 = tok.token.balance(&client.address);
    let i0 = tok.token.balance(&investor);
    let t0 = tok.token.balance(&treasury);
    client.claim_investor_payout(&investor);
    assert_eq!(tok.token.balance(&investor) - i0, 1_080_000);
    assert_eq!(tok.token.balance(&treasury) - t0, 20_000);
    assert_eq!(c0 - tok.token.balance(&client.address), payout);
    assert_eq!(client.get_claimable_payout(&investor), 0);

    let ev = PerformanceFeeCollected {
        name: symbol_short!("perf_fee"),
        investor: investor.clone(),
        invoice_id: symbol_short!("INV001"),
        fee: 20_000,
    };
    assert!(env
        .events()
        .all()
        .events()
        .contains(&ev.to_xdr(&env, &client.address)));
}

#[test]
fn zero_yield_charges_no_fee_on_principal() {
    let env = Env::default();
    let (client, tok, investor, treasury) = settled(&env, 0, 5000, 1_000_000);
    assert_eq!(client.preview_claim_split(&investor), (1_000_000, 0));
    client.claim_investor_payout(&investor);
    assert_eq!(tok.token.balance(&investor), 1_000_000);
    assert_eq!(tok.token.balance(&treasury), 0);
}

#[test]
fn default_fee_is_zero() {
    let env = Env::default();
    let (client, tok, investor, treasury) = settled(&env, 1000, 0, 1_000_000);
    assert_eq!(client.preview_claim_split(&investor), (1_100_000, 0));
    client.claim_investor_payout(&investor);
    assert_eq!(tok.token.balance(&treasury), 0);
}

#[test]
fn setter_range_event_and_lock() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    assert_contract_error(
        client.try_set_performance_fee_bps(&5_001),
        EscrowError::PerformanceFeeBpsOutOfRange,
    );
    assert_eq!(client.set_performance_fee_bps(&5_000), 5_000);
    let ev = PerformanceFeeUpdated {
        name: symbol_short!("perf_upd"),
        old_bps: 0,
        new_bps: 5_000,
    };
    assert!(env
        .events()
        .all()
        .events()
        .contains(&ev.to_xdr(&env, &client.address)));

    let investor = Address::generate(&env);
    client.fund(&investor, &1);
    assert_contract_error(
        client.try_set_performance_fee_bps(&100),
        EscrowError::PerformanceFeeLocked,
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]
    #[test]
    fn prop_fee_never_exceeds_coupon(
        amount in 1_000i128..1_000_000_000i128,
        yield_bps in 0i64..5_000,
        fee_bps in 0u32..=5_000,
    ) {
        let env = Env::default();
        let (client, _tok, investor, _t) = settled(&env, yield_bps, fee_bps, amount);
        let payout = client.compute_investor_payout(&investor);
        let (net, fee) = client.preview_claim_split(&investor);
        prop_assert_eq!(net + fee, payout);
        prop_assert!(fee >= 0 && fee <= payout - amount);
        prop_assert!(net >= amount);
    }
}
