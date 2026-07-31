use super::super::{LiquifactEscrow, LiquifactEscrowClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn init_escrow(env: &Env, client: &LiquifactEscrowClient, fee_bps: i64) -> (Address, Address) {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "FEES01"),
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
        &Some(fee_bps),
    );
    (admin, sme)
}

#[test]
fn test_fees_config_defaults_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let config = client.get_fees_config();
    assert_eq!(config.protocol_fee_bps, 0);
}

#[test]
fn test_fees_config_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let (_admin, _sme) = init_escrow(&env, &client, 250);

    let config = client.get_fees_config();
    assert_eq!(config.protocol_fee_bps, 250);
}

#[test]
fn test_fees_config_matches_getter() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let (_admin, _sme) = init_escrow(&env, &client, 1250);

    let config = client.get_fees_config();
    let fee_bps = client.get_protocol_fee_bps();

    assert_eq!(config.protocol_fee_bps, fee_bps);
}
