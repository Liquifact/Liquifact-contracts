//! Property and unit tests for the protocol fee split conservation (ADR-006 / issue #663 / #1395).
//!
//! Enforces:
//! - Exact conservation: `fee + sme_net == funded_amount` across all valid `funded_amount`
//!   and `protocol_fee_bps` (`0..=10_000`), including endpoints.
//! - Non-negativity and upper bounds (`fee >= 0`, `sme_net >= 0`, `fee <= funded_amount`).
//! - Token balance deltas for treasury and SME matching computed legs after `withdraw()`.
//! - `SmeWithdrew` event payload matching the computed legs.
//! - `DistributedPrincipal` advancing by gross `funded_amount`.
//! - Deterministic behavior on endpoints (`0` bps and `10_000` bps), rounding residue direction,
//!   and minimum principal flooring.

use proptest::prelude::*;
use super::{
    deploy, install_stellar_asset_token, StellarTestToken,
};
use crate::{
    LiquifactEscrowClient, SmeWithdrew, MAX_INVOICE_AMOUNT,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, Event, String,
};

/// Mirror of the on-chain protocol-fee split at `withdraw`.
/// Returns `(fee, sme_net)` with `fee + sme_net == amount` for `0..=10_000` bps.
fn model_fee_split(amount: i128, fee_bps: i64) -> (i128, i128) {
    let fee = amount * (fee_bps as i128) / 10_000;
    (fee, amount - fee)
}

struct Sac<'a> {
    id: Address,
    token: soroban_sdk::token::TokenClient<'a>,
    stellar: soroban_sdk::token::StellarAssetClient<'a>,
}

fn install_sac(env: &Env) -> Sac<'_> {
    let test_token = install_stellar_asset_token(env);
    Sac {
        id: test_token.id,
        token: test_token.token,
        stellar: test_token.stellar,
    }
}

fn deploy_init_sac<'a>(
    env: &'a Env,
    invoice_id: &str,
    amount: i128,
    yield_bps: i64,
    fee_bps: Option<i64>,
) -> (LiquifactEscrowClient<'a>, Sac<'a>, Address, Address) {
    env.mock_all_auths();
    let sac = install_sac(env);
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &amount,
        &yield_bps,
        &0u64,
        &sac.id,
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
        &fee_bps,
        &None::<u32>,
    );
    (client, sac, sme, treasury)
}

fn mint_and_fund(
    client: &LiquifactEscrowClient<'_>,
    sac: &Sac<'_>,
    investor: &Address,
    amount: i128,
) {
    sac.stellar.mint(investor, &amount);
    client.fund(investor, &amount);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn prop_fee_plus_sme_net_equals_disbursed_principal(
        funded_amount in prop_oneof![
            1 => Just(1i128),
            1 => Just(MAX_INVOICE_AMOUNT),
            2 => 2i128..=100_000i128,
            2 => 100_001i128..=10_000_000i128,
            2 => 10_000_001i128..=MAX_INVOICE_AMOUNT,
        ],
        fee_bps in prop_oneof![
            1 => Just(0i64),
            1 => Just(10_000i64),
            3 => 1i64..=9_999i64,
        ],
    ) {
        let env = Env::default();
        let (client, sac, sme, treasury) =
            deploy_init_sac(&env, "FEECONS", funded_amount, 0i64, Some(fee_bps));

        let investor = Address::generate(&env);
        mint_and_fund(&client, &sac, &investor, funded_amount);
        prop_assert_eq!(
            client.get_escrow().status,
            1,
            "funding exactly the target must close the escrow as funded"
        );

        let treasury_before = sac.token.balance(&treasury);
        let sme_before = sac.token.balance(&sme);

        client.withdraw();

        let events = env.events().all();

        let (fee_exp, net_exp) = model_fee_split(funded_amount, fee_bps);
        prop_assert!(fee_exp >= 0, "fee must never be negative");
        prop_assert!(net_exp >= 0, "sme_net must never be negative");
        prop_assert!(fee_exp <= funded_amount, "fee must never exceed funded_amount");
        prop_assert_eq!(
            fee_exp + net_exp,
            funded_amount,
            "conservation: fee + sme_net must equal funded_amount exactly"
        );

        prop_assert_eq!(
            sac.token.balance(&treasury) - treasury_before,
            fee_exp,
            "treasury balance delta must equal the computed fee"
        );
        prop_assert_eq!(
            sac.token.balance(&sme) - sme_before,
            net_exp,
            "SME balance delta must equal the computed net payout"
        );

        let last_event = events.events().last().unwrap().clone();
        let expected = SmeWithdrew {
            name: symbol_short!("sme_wd"),
            invoice_id: client.get_escrow().invoice_id.clone(),
            amount: net_exp,
            recipient: sme.clone(),
            fee: fee_exp,
        }
        .to_xdr(&env, &client.address);
        prop_assert_eq!(last_event, expected, "SmeWithdrew event must match the computed split");

        prop_assert_eq!(
            client.get_distributed_principal(),
            funded_amount,
            "DistributedPrincipal must advance by the full gross funded_amount"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]
    #[test]
    fn prop_fee_record_matches_computed_fee_leg(
        funded_amount in 100i128..=1_000_000i128,
        fee_bps in 1i64..=5_000i64,
    ) {
        let env = Env::default();
        let (client, sac, _sme, treasury) =
            deploy_init_sac(&env, "FEEREC", funded_amount, 0i64, Some(fee_bps));

        let investor = Address::generate(&env);
        mint_and_fund(&client, &sac, &investor, funded_amount);

        let treasury_before = sac.token.balance(&treasury);
        client.withdraw();
        let treasury_after = sac.token.balance(&treasury);

        let (fee_exp, _) = model_fee_split(funded_amount, fee_bps);
        prop_assert_eq!(
            treasury_after - treasury_before,
            fee_exp,
            "treasury balance delta must match computed fee leg"
        );
    }
}

#[test]
fn fee_split_endpoint_zero_bps_gives_sme_everything() {
    let env = Env::default();
    let (client, sac, sme, treasury) =
        deploy_init_sac(&env, "ZEROBPS", 100_000i128, 0i64, Some(0i64));

    let investor = Address::generate(&env);
    mint_and_fund(&client, &sac, &investor, 100_000i128);

    let treasury_before = sac.token.balance(&treasury);
    let sme_before = sac.token.balance(&sme);

    client.withdraw();

    assert_eq!(sac.token.balance(&treasury) - treasury_before, 0);
    assert_eq!(sac.token.balance(&sme) - sme_before, 100_000i128);
}

#[test]
fn fee_split_endpoint_max_bps_gives_treasury_everything() {
    let env = Env::default();
    let (client, sac, sme, treasury) =
        deploy_init_sac(&env, "MAXBPS", 100_000i128, 0i64, Some(10_000i64));

    let investor = Address::generate(&env);
    mint_and_fund(&client, &sac, &investor, 100_000i128);

    let treasury_before = sac.token.balance(&treasury);
    let sme_before = sac.token.balance(&sme);

    client.withdraw();

    assert_eq!(sac.token.balance(&treasury) - treasury_before, 100_000i128);
    assert_eq!(sac.token.balance(&sme) - sme_before, 0);
}

#[test]
fn fee_split_rounding_residue_stays_with_sme() {
    // funded_amount = 3, fee_bps = 3333 (33.33%)
    // fee = 3 * 3333 / 10000 = 0
    // sme_net = 3 - 0 = 3
    let env = Env::default();
    let (client, sac, sme, treasury) =
        deploy_init_sac(&env, "ROUNDRES", 3i128, 0i64, Some(3333i64));

    let investor = Address::generate(&env);
    mint_and_fund(&client, &sac, &investor, 3i128);

    let treasury_before = sac.token.balance(&treasury);
    let sme_before = sac.token.balance(&sme);

    client.withdraw();

    assert_eq!(sac.token.balance(&treasury) - treasury_before, 0);
    assert_eq!(sac.token.balance(&sme) - sme_before, 3i128);
}

#[test]
fn fee_split_minimum_principal_floors_fee_to_zero() {
    // funded_amount = 1, fee_bps = 500 (5%)
    // fee = 1 * 500 / 10000 = 0
    // sme_net = 1
    let env = Env::default();
    let (client, sac, sme, treasury) =
        deploy_init_sac(&env, "MINPRIN", 1i128, 0i64, Some(500i64));

    let investor = Address::generate(&env);
    mint_and_fund(&client, &sac, &investor, 1i128);

    let treasury_before = sac.token.balance(&treasury);
    let sme_before = sac.token.balance(&sme);

    client.withdraw();

    assert_eq!(sac.token.balance(&treasury) - treasury_before, 0);
    assert_eq!(sac.token.balance(&sme) - sme_before, 1i128);
}
