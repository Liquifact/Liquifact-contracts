use super::super::{
    CollateralCommitmentSnapshot, CollateralConfig, LiquifactEscrow, LiquifactEscrowClient,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn init_escrow(env: &Env, client: &LiquifactEscrowClient) -> (Address, Address) {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "COLCFG01"),
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
        &None::<i64>,
    );
    (admin, sme)
}

#[test]
fn test_collateral_config_defaults_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let config = client.get_collateral_config();
    assert_eq!(config.collateral_limit, super::super::MAX_INVOICE_AMOUNT);
    assert_eq!(config.sme_commitment, CollateralCommitmentSnapshot::None);
}

#[test]
fn test_collateral_config_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let (admin, sme) = init_escrow(&env, &client);

    let config = client.get_collateral_config();
    assert_eq!(config.collateral_limit, super::super::MAX_INVOICE_AMOUNT);
    assert_eq!(config.sme_commitment, CollateralCommitmentSnapshot::None);
}

#[test]
fn test_collateral_config_matches_individual_getters() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let (admin, sme) = init_escrow(&env, &client);

    let config = client.get_collateral_config();
    let limit = client.get_collateral_limit();
    let commitment = client.get_sme_collateral_commitment();

    assert_eq!(config.collateral_limit, limit);
    match commitment {
        Some(c) => assert_eq!(config.sme_commitment, CollateralCommitmentSnapshot::Some(c)),
        None => assert_eq!(config.sme_commitment, CollateralCommitmentSnapshot::None),
    }
}
