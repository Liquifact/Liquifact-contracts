use super::{LiquifactEscrow, LiquifactEscrowClient, SCHEMA_VERSION};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn init_default(env: &Env, client: &LiquifactEscrowClient<'_>) -> (Address, Address) {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000i128,
        &800i64,
        &1000u64,
    );
    (admin, sme)
}

#[test]
fn test_init_sets_version_and_persists() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let (admin, sme) = init_default(&env, &client);

    let escrow = client.get_escrow();
    assert_eq!(escrow.invoice_id, symbol_short!("INV001"));
    assert_eq!(escrow.admin, admin);
    assert_eq!(escrow.sme_address, sme);
    assert_eq!(escrow.amount, 10_000i128);
    assert_eq!(escrow.funding_target, 10_000i128);
    assert_eq!(escrow.funded_amount, 0i128);
    assert_eq!(escrow.settled_amount, 0i128);
    assert_eq!(escrow.yield_bps, 800i64);
    assert_eq!(escrow.maturity, 1000u64);
    assert_eq!(escrow.status, 0u32);
    assert_eq!(escrow.version, SCHEMA_VERSION);

    assert_eq!(client.get_version(), SCHEMA_VERSION);
}

#[test]
#[should_panic(expected = "Escrow already initialized")]
fn test_reinit_is_rejected_and_cannot_overwrite_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let (_admin, _sme) = init_default(&env, &client);

    let attacker_admin = Address::generate(&env);
    let attacker_sme = Address::generate(&env);
    client.init(
        &attacker_admin,
        &symbol_short!("ATTACK"),
        &attacker_sme,
        &9999i128,
        &999i64,
        &9999u64,
    );
}

#[test]
#[should_panic(expected = "Escrow not initialized")]
fn test_get_escrow_uninitialized_panics() {
    let env = Env::default();
    let client = deploy(&env);
    client.get_escrow();
}

#[test]
fn test_fund_updates_amount_and_status() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_default(&env, &client);

    let investor = Address::generate(&env);
    let after_1 = client.fund(&investor, &5_000i128);
    assert_eq!(after_1.funded_amount, 5_000i128);
    assert_eq!(after_1.status, 0u32);

    let after_2 = client.fund(&investor, &5_000i128);
    assert_eq!(after_2.funded_amount, 10_000i128);
    assert_eq!(after_2.status, 1u32);
}

#[test]
#[should_panic(expected = "Funding amount must be positive")]
fn test_fund_with_zero_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_default(&env, &client);

    let investor = Address::generate(&env);
    client.fund(&investor, &0i128);
}

#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_after_funded_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_default(&env, &client);

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000i128);
    client.fund(&investor, &1i128);
}

#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_before_funded_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_default(&env, &client);
    client.settle();
}

#[test]
fn test_settle_transitions_to_settled() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let (_admin, _sme) = init_default(&env, &client);

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000i128);

    let settled = client.settle();
    assert_eq!(settled.status, 2u32);
    assert_eq!(client.get_escrow().status, 2u32);
}

#[test]
#[should_panic]
fn test_init_requires_admin_auth() {
    let env = Env::default();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    client.init(
        &admin,
        &symbol_short!("INVNA"),
        &sme,
        &10_000i128,
        &800i64,
        &1000u64,
    );
}

#[test]
#[should_panic]
fn test_fund_requires_investor_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_default(&env, &client);

    let investor = Address::generate(&env);
    env.mock_auths(&[]);
    client.fund(&investor, &1i128);
}

#[test]
#[should_panic]
fn test_settle_requires_sme_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_default(&env, &client);

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000i128);

    env.mock_auths(&[]);
    client.settle();
}

#[test]
fn test_update_maturity_by_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let (_admin, _sme) = init_default(&env, &client);

    let updated = client.update_maturity(&2000u64);
    assert_eq!(updated.maturity, 2000u64);
    assert_eq!(client.get_escrow().maturity, 2000u64);
}

#[test]
#[should_panic(expected = "Maturity can only be updated in Open state")]
fn test_update_maturity_fails_if_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_default(&env, &client);

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000i128);

    client.update_maturity(&2000u64);
}

#[test]
fn test_withdraw_by_sme() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let (_admin, _sme) = init_default(&env, &client);

    let investor = Address::generate(&env);
    client.fund(&investor, &10_000i128);

    let withdrawn_amount = client.withdraw();
    assert_eq!(withdrawn_amount, 10_000i128);

    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 3u32);
    assert_eq!(escrow.funded_amount, 0i128);
}
