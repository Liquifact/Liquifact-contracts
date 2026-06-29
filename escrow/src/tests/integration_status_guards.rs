// Tests for the status guard functionality
use super::*;
use crate::EscrowError;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup_env_and_client() -> (Env, LiquifactEscrowClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let client = LiquifactEscrowClient::new(&env, &env.register_contract(None, LiquifactEscrow));
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let tok = Address::generate(&env);
    let tre = Address::generate(&env);

    // Use the 14-argument init signature
    client.init(
        &admin,
        &String::from_str(&env, "TEST_INV"),
        &sme,
        &10_000i128,
        &500i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    (env, client, admin, sme)
}

#[test]
#[should_panic(expected = "Target update requires Open status")]
fn test_update_funding_target_wrong_status() {
    let (_env, client, admin, _sme) = setup_env_and_client();
    // Move to cancelled status
    client.cancel_funding(&admin);
    // Now try to update funding target -> Should panic with TargetUpdateNotOpen
    client.update_funding_target(&admin, &5_000i128);
}

#[test]
#[should_panic(expected = "Cancel funding requires Open status")]
fn test_cancel_funding_wrong_status() {
    let (_env, client, admin, _sme) = setup_env_and_client();
    // Move to cancelled status
    client.cancel_funding(&admin);
    // Try to cancel again -> CancelFundingNotOpen
    client.cancel_funding(&admin);
}

#[test]
#[should_panic(expected = "Refund requires Cancelled status")]
fn test_refund_wrong_status() {
    let (env, client, _admin, _sme) = setup_env_and_client();
    let investor = Address::generate(&env);
    // Escrow is Open, not Cancelled -> RefundNotCancelled
    client.refund(&investor);
}
