use liquifact_escrow::LiquifactEscrowClient;
use soroban_sdk::{Env, Symbol, String, Address};
use soroban_sdk::testutils::{Address as _, Ledger as _};

#[test]
fn test_collateral_setter_basic() {
    let env = Env::default();
    env.mock_all_auths();
    
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 0;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);
    
    let contract_id = env.register_contract(None, liquifact_escrow::LiquifactEscrow {});
    let client = LiquifactEscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = (Address::generate(&env), Address::generate(&env));
    
    client.init(
        &admin,
        &String::from_str(&env, "INV001"),
        &sme,
        &100_000_000_000i128,
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
    
    let asset = Symbol::new(&env, "USDC");
    let amount = 10000i128;
    let result = client.set_collateral_parameters(&asset, &amount);
    assert_eq!(result.asset, asset);
    assert_eq!(result.amount, amount);
    println!("✅ Collateral setter test passed!");
}

#[test]
fn test_collateral_setter_empty_asset() {
    let env = Env::default();
    env.mock_all_auths();
    
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 0;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);
    
    let contract_id = env.register_contract(None, liquifact_escrow::LiquifactEscrow {});
    let client = LiquifactEscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = (Address::generate(&env), Address::generate(&env));
    
    client.init(
        &admin,
        &String::from_str(&env, "INV001"),
        &sme,
        &100_000_000_000i128,
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
    
    let empty_asset = Symbol::new(&env, "");
    let amount = 10000i128;
    let result = client.try_set_collateral_parameters(&empty_asset, &amount);
    assert!(result.is_err());
    println!("✅ Empty asset correctly rejected!");
}

#[test]
fn test_collateral_setter_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 0;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);
    
    let contract_id = env.register_contract(None, liquifact_escrow::LiquifactEscrow {});
    let client = LiquifactEscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = (Address::generate(&env), Address::generate(&env));
    
    client.init(
        &admin,
        &String::from_str(&env, "INV001"),
        &sme,
        &100_000_000_000i128,
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
    
    let asset = Symbol::new(&env, "USDC");
    let result = client.try_set_collateral_parameters(&asset, &0i128);
    assert!(result.is_err());
    println!("✅ Zero amount correctly rejected!");
}

#[test]
fn test_collateral_setter_exceeds_max() {
    let env = Env::default();
    env.mock_all_auths();
    
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 0;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);
    
    let contract_id = env.register_contract(None, liquifact_escrow::LiquifactEscrow {});
    let client = LiquifactEscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = (Address::generate(&env), Address::generate(&env));
    
    client.init(
        &admin,
        &String::from_str(&env, "INV001"),
        &sme,
        &100_000_000_000i128,
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
    
    let asset = Symbol::new(&env, "USDC");
    let huge_amount = 2_000_000_000_000_000i128;
    let result = client.try_set_collateral_parameters(&asset, &huge_amount);
    assert!(result.is_err());
    println!("✅ Amount exceeds max correctly rejected!");
}