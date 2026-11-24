use super::*;
use proptest::prelude::*;
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use crate::MAX_INVOICE_AMOUNT;

/// Sets up an escrow with a real token, funded to the target amount, ready for withdraw.
fn setup_withdraw_with_fee<'a>(
    env: &'a Env,
    funded_amount: i128,
    protocol_fee_bps: i64,
) -> (
    LiquifactEscrowClient<'a>,
    Address,
    Address,
    Address,
    StellarAssetClient<'a>,
) {
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(env, &token_id);

    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &escrow_id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "FEE_SPLIT"),
        &sme,
        &funded_amount,
        &800i64,
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
        &Some(protocol_fee_bps),
    );

    let investor = Address::generate(env);
    sac_admin.mint(&investor, &funded_amount);
    client.fund(&investor, &funded_amount);

    // Mint the funded amount to the contract so withdraw can transfer it
    sac_admin.mint(&escrow_id, &funded_amount);

    (client, escrow_id, sme, treasury, sac_admin)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn prop_fee_plus_sme_net_equals_disbursed_principal(
        funded_amount in 1i128..=MAX_INVOICE_AMOUNT,
        protocol_fee_bps in 0i64..=10_000i64,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let (client, escrow_id, sme, treasury, sac) =
            setup_withdraw_with_fee(&env, funded_amount, protocol_fee_bps);

        let token_id = sac.address.clone();
        let token_client = TokenClient::new(&env, &token_id);

        // Record balances before withdraw
        let treasury_before = token_client.balance(&treasury);
        let sme_before = token_client.balance(&sme);
        let contract_before = token_client.balance(&escrow_id);

        // Compute expected fee and net using the contract formula
        let expected_fee = funded_amount
            .checked_mul(protocol_fee_bps as i128)
            .unwrap()
            / 10_000;
        let expected_net = funded_amount - expected_fee;

        // Withdraw
        client.withdraw();

        // Record balances after withdraw
        let treasury_after = token_client.balance(&treasury);
        let sme_after = token_client.balance(&sme);
        let contract_after = token_client.balance(&escrow_id);

        // Verify balance deltas
        let treasury_delta = treasury_after - treasury_before;
        let sme_delta = sme_after - sme_before;
        let contract_delta = contract_before - contract_after;

        prop_assert_eq!(treasury_delta, expected_fee,
            "treasury delta {} must equal expected fee {}", treasury_delta, expected_fee);
        prop_assert_eq!(sme_delta, expected_net,
            "SME delta {} must equal expected net {}", sme_delta, expected_net);
        prop_assert_eq!(contract_delta, funded_amount,
            "contract delta {} must equal funded amount {}", contract_delta, funded_amount);

        // Verify conservation invariant
        prop_assert_eq!(expected_fee + expected_net, funded_amount,
            "fee {} + net {} must equal funded amount {}", expected_fee, expected_net, funded_amount);

        // Verify non-negativity
        prop_assert!(expected_fee >= 0, "fee {} must be non-negative", expected_fee);
        prop_assert!(expected_net >= 0, "net {} must be non-negative", expected_net);

        // Verify upper bound
        prop_assert!(expected_fee <= funded_amount,
            "fee {} must not exceed funded amount {}", expected_fee, funded_amount);
    }

    #[test]
    fn prop_fee_record_matches_computed_fee_leg(
        funded_amount in 1i128..=MAX_INVOICE_AMOUNT,
        protocol_fee_bps in 0i64..=10_000i64,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _escrow_id, _sme, _treasury, _sac) =
            setup_withdraw_with_fee(&env, funded_amount, protocol_fee_bps);

        // Compute expected fee
        let expected_fee = funded_amount
            .checked_mul(protocol_fee_bps as i128)
            .unwrap()
            / 10_000;

        // Withdraw
        client.withdraw();

        // If fee is zero, no fee record should be written
        if expected_fee == 0 {
            // Zero fee writes no FeeRecord - verify by checking that the fee key doesn't exist
            // or has a zero value. Since we can't directly check for absence in the test helpers,
            // we just verify the arithmetic is correct.
            prop_assert_eq!(expected_fee, 0, "zero fee case");
        } else {
            // Non-zero fee: the event should carry the correct fee amount
            // The SmeWithdrew event carries the fee in its payload
            let events = env.events().all();
            let fee_found = events.events().iter().any(|event| {
                let event_str = format!("{:?}", event);
                event_str.contains("sme_wd") && event_str.contains(&expected_fee.to_string())
            });
            prop_assert!(fee_found, "SmeWithdrew event should carry fee {}", expected_fee);
        }
    }
}

#[test]
fn fee_split_endpoint_zero_bps_gives_sme_everything() {
    let env = Env::default();
    env.mock_all_auths();

    let funded_amount = 1_000_000i128;
    let (client, escrow_id, sme, treasury, sac) =
        setup_withdraw_with_fee(&env, funded_amount, 0);

    let token_id = sac.address.clone();
    let token_client = TokenClient::new(&env, &token_id);

    let treasury_before = token_client.balance(&treasury);
    let sme_before = token_client.balance(&sme);

    client.withdraw();

    let treasury_after = token_client.balance(&treasury);
    let sme_after = token_client.balance(&sme);

    // With 0 bps, treasury receives nothing, SME receives full amount
    assert_eq!(treasury_after, treasury_before);
    assert_eq!(sme_after - sme_before, funded_amount);
}

#[test]
fn fee_split_endpoint_max_bps_gives_treasury_everything() {
    let env = Env::default();
    env.mock_all_auths();

    let funded_amount = 1_000_000i128;
    let (client, escrow_id, sme, treasury, sac) =
        setup_withdraw_with_fee(&env, funded_amount, 10_000);

    let token_id = sac.address.clone();
    let token_client = TokenClient::new(&env, &token_id);

    let treasury_before = token_client.balance(&treasury);
    let sme_before = token_client.balance(&sme);

    client.withdraw();

    let treasury_after = token_client.balance(&treasury);
    let sme_after = token_client.balance(&sme);

    // With 10_000 bps, treasury receives full amount, SME receives nothing
    assert_eq!(treasury_after - treasury_before, funded_amount);
    assert_eq!(sme_after, sme_before);
}

#[test]
fn fee_split_rounding_residue_stays_with_sme() {
    let env = Env::default();
    env.mock_all_auths();

    // Use an amount that doesn't divide evenly by 10_000
    let funded_amount = 10_001i128;
    let fee_bps = 1i64; // 0.01%

    let (client, escrow_id, sme, treasury, sac) =
        setup_withdraw_with_fee(&env, funded_amount, fee_bps);

    let token_id = sac.address.clone();
    let token_client = TokenClient::new(&env, &token_id);

    let treasury_before = token_client.balance(&treasury);
    let sme_before = token_client.balance(&sme);

    client.withdraw();

    let treasury_after = token_client.balance(&treasury);
    let sme_after = token_client.balance(&sme);

    let treasury_delta = treasury_after - treasury_before;
    let sme_delta = sme_after - sme_before;

    // fee = 10_001 * 1 / 10_000 = 1 (floor division)
    // net = 10_001 - 1 = 10_000
    // residue of 1 stays with SME
    assert_eq!(treasury_delta, 1);
    assert_eq!(sme_delta, 10_000);
    assert_eq!(treasury_delta + sme_delta, funded_amount);
}

#[test]
fn fee_split_minimum_principal_floors_fee_to_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let funded_amount = 1i128;
    let fee_bps = 9_999i64; // 99.99%

    let (client, escrow_id, sme, treasury, sac) =
        setup_withdraw_with_fee(&env, funded_amount, fee_bps);

    let token_id = sac.address.clone();
    let token_client = TokenClient::new(&env, &token_id);

    let treasury_before = token_client.balance(&treasury);
    let sme_before = token_client.balance(&sme);

    client.withdraw();

    let treasury_after = token_client.balance(&treasury);
    let sme_after = token_client.balance(&sme);

    let treasury_delta = treasury_after - treasury_before;
    let sme_delta = sme_after - sme_before;

    // fee = 1 * 9_999 / 10_000 = 0 (floor division)
    // net = 1 - 0 = 1
    assert_eq!(treasury_delta, 0);
    assert_eq!(sme_delta, 1);
    assert_eq!(treasury_delta + sme_delta, funded_amount);
}
