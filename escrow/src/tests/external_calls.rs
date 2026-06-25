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
#[should_panic]
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
#[should_panic]
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
#[should_panic]
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

// ── Liability floor tests for sweep_terminal_dust ────────────────────────────

fn setup_cancelled_with_token<'a>(
    env: &'a Env,
    client: &LiquifactEscrowClient<'a>,
    admin: &Address,
    sme: &Address,
    investor: &Address,
    fund_amount: i128,
) -> (crate::tests::StellarTestToken<'a>, Address) {
    let token = install_stellar_asset_token(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "FLOOR01"),
        sme,
        &(fund_amount * 2),
        &0i64,
        &0u64,
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
    // Mint tokens into the contract to simulate on-chain custody
    token.stellar.mint(&client.address, &fund_amount);
    client.fund(investor, &fund_amount);
    client.cancel_funding();
    (token, treasury)
}

#[test]
fn sweep_liability_floor_allows_true_dust_after_all_refunded() {
    // After all investors are refunded, outstanding = 0, so any dust can be swept.
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let fund_amount = 1_000i128;
    let (token, treasury) =
        setup_cancelled_with_token(&env, &client, &admin, &sme, &investor, fund_amount);

    // Mint 1 extra unit of dust on top of the principal
    token.stellar.mint(&client.address, &1i128);

    // Refund the investor — this increments DistributedPrincipal by fund_amount
    client.refund(&investor);

    // Now outstanding = funded_amount - distributed = 1000 - 1000 = 0
    // balance = 1 (the dust), sweep_amt = 1, floor check: 1 - 1 >= 0 ✓
    let swept = client.sweep_terminal_dust(&1i128);
    assert_eq!(swept, 1i128);
    assert_eq!(token.token.balance(&treasury), 1i128);
    assert_eq!(client.get_distributed_principal(), fund_amount);
}

#[test]
#[should_panic]
fn sweep_liability_floor_blocks_sweep_when_investor_not_yet_refunded() {
    // No refunds yet: outstanding = funded_amount, balance = funded_amount.
    // Any sweep would dip below the floor.
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let fund_amount = 1_000i128;
    let (token, _treasury) =
        setup_cancelled_with_token(&env, &client, &admin, &sme, &investor, fund_amount);

    // balance == outstanding == 1000; sweep of even 1 unit violates the floor
    client.sweep_terminal_dust(&1i128);
}

#[test]
fn sweep_liability_floor_allows_sweep_of_excess_above_outstanding() {
    // Two investors fund 500 each. One is refunded. 500 outstanding remains.
    // Contract has 1001 tokens (500 refunded, 500 outstanding, 1 dust).
    // Sweep of 1 is allowed; sweep of 501 is not.
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FLOOR02"),
        &sme,
        &2_000i128,
        &0i64,
        &0u64,
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

    // Mint 1001 into contract: 500 for A, 500 for B, 1 dust
    token.stellar.mint(&client.address, &1_001i128);
    client.fund(&investor_a, &500i128);
    client.fund(&investor_b, &500i128);
    client.cancel_funding();

    // Refund investor_a → distributed = 500, outstanding = 500
    client.refund(&investor_a);
    assert_eq!(client.get_distributed_principal(), 500i128);

    // balance = 501 (500 for B + 1 dust), outstanding = 500
    // sweep of 1: 501 - 1 = 500 >= 500 ✓
    let swept = client.sweep_terminal_dust(&1i128);
    assert_eq!(swept, 1i128);
    assert_eq!(token.token.balance(&treasury), 1i128);
}

#[test]
#[should_panic]
fn sweep_liability_floor_blocks_sweep_that_would_eat_into_outstanding() {
    // Same setup as above but try to sweep 2 (which would leave 499 < 500 outstanding).
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FLOOR03"),
        &sme,
        &2_000i128,
        &0i64,
        &0u64,
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

    token.stellar.mint(&client.address, &1_001i128);
    client.fund(&investor_a, &500i128);
    client.fund(&investor_b, &500i128);
    client.cancel_funding();
    client.refund(&investor_a);

    // balance = 501, outstanding = 500; sweep of 2 → 501 - 2 = 499 < 500 ✗
    client.sweep_terminal_dust(&2i128);
}

#[test]
fn sweep_liability_floor_zero_funded_amount_allows_sweep() {
    // Edge case: escrow cancelled before any funding. funded_amount = 0,
    // distributed = 0, outstanding = 0. Any dust can be swept.
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FLOOR04"),
        &sme,
        &1_000i128,
        &0i64,
        &0u64,
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
    client.cancel_funding();

    // Stray airdrop of 50 tokens
    token.stellar.mint(&client.address, &50i128);

    let swept = client.sweep_terminal_dust(&50i128);
    assert_eq!(swept, 50i128);
    assert_eq!(token.token.balance(&treasury), 50i128);
}

#[test]
fn distributed_principal_accumulates_across_multiple_refunds() {
    // Three investors; refund them one by one and verify the counter.
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FLOOR05"),
        &sme,
        &1_800i128,
        &0i64,
        &0u64,
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

    token.stellar.mint(&client.address, &900i128);
    client.fund(&inv_a, &300i128);
    client.fund(&inv_b, &300i128);
    client.fund(&inv_c, &300i128);
    client.cancel_funding();

    assert_eq!(client.get_distributed_principal(), 0i128);

    client.refund(&inv_a);
    assert_eq!(client.get_distributed_principal(), 300i128);

    client.refund(&inv_b);
    assert_eq!(client.get_distributed_principal(), 600i128);

    client.refund(&inv_c);
    assert_eq!(client.get_distributed_principal(), 900i128);

    // All refunded — outstanding = 0, any dust can be swept
    token.stellar.mint(&client.address, &5i128);
    let swept = client.sweep_terminal_dust(&5i128);
    assert_eq!(swept, 5i128);
}
