with open('escrow/src/tests/settlement.rs', 'a', encoding='utf-8') as f:
    f.write('''
// ──────────────────────────────────────────────────────────────────────────────
// is_settleable coverage
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_is_settleable_created_never_funded() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    assert!(!client.is_settleable(), "open escrow is not settleable");
}

#[test]
fn test_is_settleable_funded_before_maturity() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);
    let maturity: u64 = 5_000;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MAT001"),
        &sme,
        &TARGET,
        &500i64,
        &maturity,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    fund_to_target(&client, &env);
    env.ledger().with_mut(|l| l.timestamp = maturity - 1);
    assert!(!client.is_settleable(), "funded but before maturity is not settleable");
}

#[test]
fn test_is_settleable_funded_exact_maturity() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);
    let maturity: u64 = 5_000;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MAT002"),
        &sme,
        &TARGET,
        &500i64,
        &maturity,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    fund_to_target(&client, &env);
    env.ledger().with_mut(|l| l.timestamp = maturity);
    assert!(client.is_settleable(), "funded at exact maturity is settleable");
}

#[test]
fn test_is_settleable_funded_after_maturity() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);
    let maturity: u64 = 5_000;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MAT003"),
        &sme,
        &TARGET,
        &500i64,
        &maturity,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    fund_to_target(&client, &env);
    env.ledger().with_mut(|l| l.timestamp = maturity + 100);
    assert!(client.is_settleable(), "funded after maturity is settleable");
}

#[test]
fn test_is_settleable_legal_hold_active() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);
    client.set_legal_hold(&true);
    assert!(!client.is_settleable(), "legal hold active is not settleable");
}

#[test]
fn test_is_settleable_already_settled() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);
    client.settle();
    assert!(!client.is_settleable(), "already settled is not settleable");
}

#[test]
fn test_is_settleable_normal_successful() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    fund_to_target(&client, &env);
    assert!(client.is_settleable(), "normal successful is settleable");
}
''')
