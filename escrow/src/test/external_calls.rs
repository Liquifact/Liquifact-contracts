use super::super::external_calls::transfer_funding_token_with_balance_checks;
use super::*;
use soroban_sdk::{Address, Env, MuxedAddress};

#[test]
fn test_balance_delta_invariants_with_standard_token() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // Test with a single clean transfer to verify balance delta invariants
    let amount = 1000i128;

    // Ensure clean state
    let holder_balance = token.token.balance(&holder);
    if holder_balance > 0 {
        token.token.transfer(
            &holder,
            MuxedAddress::from(treasury.clone()),
            &holder_balance,
        );
    }

    // Mint fresh amount
    token.stellar.mint(&holder, &amount);

    let holder_before = token.token.balance(&holder);
    let treasury_before = token.token.balance(&treasury);

    // Verify initial state
    assert_eq!(holder_before, amount);
    assert_eq!(treasury_before, 0i128);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, amount);

    let holder_after = token.token.balance(&holder);
    let treasury_after = token.token.balance(&treasury);

    // Verify exact balance deltas - this is the core invariant test
    let spent = holder_before - holder_after;
    let received = treasury_after - treasury_before;

    assert_eq!(
        spent, amount,
        "Sender balance delta must equal transfer amount"
    );
    assert_eq!(
        received, amount,
        "Recipient balance delta must equal transfer amount"
    );
    assert_eq!(
        holder_after, 0i128,
        "Sender should have zero balance after transfer"
    );
    assert_eq!(
        treasury_after, amount,
        "Recipient should have exact transfer amount"
    );
}

#[test]
#[should_panic(expected = "transfer amount must be positive")]
fn test_panics_with_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    token.stellar.mint(&holder, &1000i128);

    // This should panic due to zero amount
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 0i128);
}

#[test]
#[should_panic(expected = "transfer amount must be positive")]
fn test_panics_with_negative_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    token.stellar.mint(&holder, &1000i128);

    // This should panic due to negative amount
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, -100i128);
}

#[test]
fn test_muxed_address_compatibility() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    let amount = 500i128;
    token.stellar.mint(&holder, &amount);

    // Verify that MuxedAddress conversion works correctly
    let muxed_treasury = MuxedAddress::from(treasury.clone());
    assert_eq!(muxed_treasury.address(), treasury);

    // Transfer should work with MuxedAddress internally
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, amount);

    assert_eq!(token.token.balance(&holder), 0i128);
    assert_eq!(token.token.balance(&treasury), amount);
}

#[test]
#[should_panic(expected = "insufficient token balance before transfer")]
fn test_balance_underflow_detection() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // Don't mint any tokens to holder (balance = 0)

    // This should panic at the insufficient balance check
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 100i128);
}

#[test]
fn test_multiple_transfers_cumulative_balance_deltas() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    let initial_amount = 1000i128;
    token.stellar.mint(&holder, &initial_amount);

    let transfer_amounts = [100i128, 200i128, 300i128];
    let mut total_transferred = 0i128;

    for amount in transfer_amounts.iter() {
        let holder_before = token.token.balance(&holder);
        let treasury_before = token.token.balance(&treasury);

        transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, *amount);

        let holder_after = token.token.balance(&holder);
        let treasury_after = token.token.balance(&treasury);

        // Verify exact balance deltas for each transfer
        assert_eq!(holder_before - holder_after, *amount);
        assert_eq!(treasury_after - treasury_before, *amount);

        total_transferred += amount;
    }

    // Verify final state
    assert_eq!(
        token.token.balance(&holder),
        initial_amount - total_transferred
    );
    assert_eq!(token.token.balance(&treasury), total_transferred);
}

#[test]
fn test_edge_case_maximum_amount_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // Test with a large amount (but not i128::MAX to avoid overflow issues)
    let large_amount = i128::MAX / 1000; // Safe large amount
    token.stellar.mint(&holder, &large_amount);

    let holder_before = token.token.balance(&holder);
    let treasury_before = token.token.balance(&treasury);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, large_amount);

    let holder_after = token.token.balance(&holder);
    let treasury_after = token.token.balance(&treasury);

    // Verify exact balance deltas even with large amounts
    assert_eq!(holder_before - holder_after, large_amount);
    assert_eq!(treasury_after - treasury_before, large_amount);
    assert_eq!(holder_after, 0i128);
    assert_eq!(treasury_after, large_amount);
}
