use super::{LiquifactEscrow, LiquifactEscrowClient, SCHEMA_VERSION};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};

fn setup_test(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address, Address) {
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let investor = Address::generate(env);
    (client, admin, sme, investor)
}

#[test]
fn test_init() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, _) = setup_test(&env);

    let amount = 10_000_0000000i128;
    let yield_bps = 800i64;
    let maturity = 1000u64;

    let escrow = client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &amount,
        &yield_bps,
        &maturity,
    );

    assert_eq!(escrow.invoice_id, symbol_short!("INV001"));
    assert_eq!(escrow.admin, admin);
    assert_eq!(escrow.sme_address, sme);
    assert_eq!(escrow.amount, amount);
    assert_eq!(escrow.status, 0);
    assert_eq!(escrow.version, SCHEMA_VERSION);
}

#[test]
#[should_panic(expected = "Escrow already initialized")]
fn test_double_init_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, _) = setup_test(&env);

    client.init(&admin, &symbol_short!("INV001"), &sme, &1000, &800, &1000);
    client.init(&admin, &symbol_short!("INV001"), &sme, &1000, &800, &1000);
}

#[test]
fn test_full_funding_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, investor) = setup_test(&env);

    client.init(&admin, &symbol_short!("INV001"), &sme, &1000, &800, &1000);

    // Partial funding
    let escrow = client.fund(&investor, &500);
    assert_eq!(escrow.funded_amount, 500);
    assert_eq!(escrow.status, 0);
    assert_eq!(client.get_contribution(&investor), 500);

    // Full funding
    let escrow = client.fund(&investor, &500);
    assert_eq!(escrow.funded_amount, 1000);
    assert_eq!(escrow.status, 1);
}

#[test]
fn test_withdraw_to_sme() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, investor) = setup_test(&env);

    client.init(&admin, &symbol_short!("INV001"), &sme, &1000, &800, &1000);
    client.fund(&investor, &1000);

    let withdrawn = client.withdraw();
    assert_eq!(withdrawn, 1000);

    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 3);
}

#[test]
fn test_settle_and_claim() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, investor) = setup_test(&env);

    client.init(&admin, &symbol_short!("INV001"), &sme, &1000, &800, &1000);
    client.fund(&investor, &1000);
    client.withdraw();

    // Settle with interest (8% of 1000 = 80)
    let interest = 80;
    client.settle(&(1000 + interest));

    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 2);
    assert_eq!(escrow.settled_amount, 1080);

    // Claim
    let payout = client.claim(&investor);
    assert_eq!(payout, 1080);
}

#[test]
#[should_panic(expected = "Payout already claimed")]
fn test_double_claim_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme, investor) = setup_test(&env);

    client.init(&admin, &symbol_short!("INV001"), &sme, &1000, &800, &1000);
    client.fund(&investor, &1000);
    client.settle(&1080);

    client.claim(&investor);
    client.claim(&investor);
}
