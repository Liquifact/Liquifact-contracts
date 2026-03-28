use super::{LiquifactEscrow, LiquifactEscrowClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn init_escrow(
    env: &Env,
    client: &LiquifactEscrowClient,
    admin: &Address,
    sme: &Address,
    amount: i128,
) {
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &String::from_str(env, "INV001"),
        sme,
        &amount,
        &800i64,
        &3000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
}

#[test]
fn test_update_funding_target_by_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    init_escrow(&env, &client, &admin, &sme, 5_000i128);
    let updated = client.update_funding_target(&10_000i128);
    assert_eq!(
        updated.funding_target, 10_000i128,
        "funding_target should be updated"
    );
    assert_eq!(
        updated.status, 0,
        "status must remain Open after target update"
    );
}

#[test]
#[should_panic]
fn test_update_funding_target_by_non_admin_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);
    init_escrow(&env, &client, &admin, &sme, 5_000i128);

    env.mock_auths(&[]);
    client.update_funding_target(&10_000i128);
}

#[test]
#[should_panic(expected = "Target can only be updated in Open state")]
fn test_update_funding_target_fails_when_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    init_escrow(&env, &client, &admin, &sme, 5_000i128);
    client.fund(&investor, &5_000i128);
    client.update_funding_target(&10_000i128);
}

#[test]
#[should_panic(expected = "Target cannot be less than already funded amount")]
fn test_update_funding_target_below_funded_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    init_escrow(&env, &client, &admin, &sme, 10_000i128);
    client.fund(&investor, &4_000i128);
    client.update_funding_target(&3_000i128);
}

#[test]
#[should_panic(expected = "Target must be strictly positive")]
fn test_update_funding_target_zero_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    init_escrow(&env, &client, &admin, &sme, 5_000i128);
    client.update_funding_target(&0i128);
}
