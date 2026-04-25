use super::*;
use soroban_sdk::{contract, contractimpl};

// External-call and token-integration assumptions that should stay separate
// from escrow state-machine assertions.

#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) -> bool {
        panic!("Token contract transfer should not be invoked by escrow metadata-only flows")
    }

    pub fn is_paused(_env: Env) -> bool {
        panic!("Token contract pause status should not be read by escrow metadata-only flows")
    }

    pub fn decimals(_env: Env) -> i32 {
        6
    }
}

#[test]
fn test_collateral_record_is_metadata_only_and_does_not_invoke_token_contract() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let _token_id = env.register(MockToken, ());
    let funding = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "COLTI001"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &funding,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    let commitment = client.record_sme_collateral_commitment(&symbol_short!("USDC"), &5_000i128);
    assert_eq!(commitment.asset, symbol_short!("USDC"));
    assert_eq!(commitment.amount, 5_000i128);
    assert!(client.get_sme_collateral_commitment().is_some());
}

#[test]
fn test_token_integration_assumptions_are_documented_in_readme() {
    let contents = include_str!("../../../docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md");
    assert!(
        contents.contains("fee-on-transfer"),
        "Expected unsupported token warning to be documented"
    );
    assert!(
        contents.contains("smallest units"),
        "Expected smallest-unit assumption to be documented"
    );
}

#[test]
fn test_external_transfer_wrapper_balance_deltas() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);
    token.stellar.mint(&holder, &777i128);
    external_calls::transfer_funding_token_with_balance_checks(
        &env, &token.id, &holder, &treasury, 777i128,
    );
    assert_eq!(token.token.balance(&holder), 0);
    assert_eq!(token.token.balance(&treasury), 777i128);
}

#[test]
#[should_panic(expected = "insufficient token balance before transfer")]
fn test_external_wrapper_panics_when_undercollateralized() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);
    token.stellar.mint(&holder, &1i128);
    external_calls::transfer_funding_token_with_balance_checks(
        &env, &token.id, &holder, &treasury, 10i128,
    );
}
