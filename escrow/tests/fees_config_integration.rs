use liquifact_escrow::{LiquifactEscrowClient, LiquifactEscrow};
use soroban_sdk::{Env, Address, String};
use soroban_sdk::testutils::Address as _;

#[test]
fn integration_get_fees_config_before_and_after_init() {
    let env = Env::default();
    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &id);

    // Before init, fee defaults to 0
    let config = client.get_fees_config();
    assert_eq!(config.protocol_fee_bps, 0);

    // Init with a fee and verify the view reflects it
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "FEES_INTEG"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &token,
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
        &Some(250i64),
    );

    let config2 = client.get_fees_config();
    assert_eq!(config2.protocol_fee_bps, 250);
}
