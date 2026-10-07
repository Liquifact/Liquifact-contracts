//! Deterministic fee-split properties around the withdrawal boundary.

use crate::tests::{install_stellar_asset_token, setup};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn init_and_fund(
    env: &Env,
    fee_bps: i64,
    amount: i128,
) -> (
    crate::LiquifactEscrowClient<'_>,
    Address,
    Address,
    Address,
    i128,
) {
    let (client, admin, sme) = setup(env);
    let token = install_stellar_asset_token(env);
    let treasury = Address::generate(env);
    let investor = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "FEEPROP"),
        &sme,
        &amount,
        &0i64,
        &0u64,
        &token.id,
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
        &None,
        &Some(fee_bps),
        &None::<u32>,
    );
    token.stellar.mint(&investor, &amount);
    client.fund(&investor, &amount);
    (client, token.id, sme, treasury, amount)
}

#[test]
fn fee_split_conserves_gross_principal_at_rounding_boundaries() {
    let amount = 100_003i128;
    for fee_bps in [0i64, 1, 9_999, 10_000] {
        let env = Env::default();
        let (client, token_id, sme, treasury, gross) = init_and_fund(&env, fee_bps, amount);
        let token = soroban_sdk::token::TokenClient::new(&env, &token_id);
        let sme_before = token.balance(&sme);
        let treasury_before = token.balance(&treasury);

        client.withdraw();

        let expected_fee = gross * i128::from(fee_bps) / 10_000;
        let expected_net = gross - expected_fee;
        let fee_delta = token.balance(&treasury) - treasury_before;
        let net_delta = token.balance(&sme) - sme_before;
        assert_eq!(fee_delta, expected_fee, "fee split at {fee_bps} bps");
        assert_eq!(net_delta, expected_net, "net split at {fee_bps} bps");
        assert_eq!(fee_delta + net_delta, gross);
        assert_eq!(client.get_escrow().status, 3);
    }
}

#[test]
fn invalid_fee_configuration_is_rejected_before_funding() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);
    let result = client.try_init(
        &admin,
        &soroban_sdk::String::from_str(&env, "BADFEE"),
        &sme,
        &1_000i128,
        &0i64,
        &0u64,
        &token.id,
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &Some(10_001i64),
        &None::<u32>,
    );
    assert!(result.is_err());
}
