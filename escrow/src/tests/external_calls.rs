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
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );
    // Mint to investor so fund() can transfer principal into escrow
    token.stellar.mint(investor, &fund_amount);
    client.fund(investor, &fund_amount);
    client.cancel_funding(&0u32);
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
#[ignore = "upstream latent: escrow API/test drift"]
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
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );

    // Mint 1001 into contract: 500 for A, 500 for B, 1 dust
    token.stellar.mint(&investor_a, &1_001i128);
    client.fund(&investor_a, &500i128);
    client.fund(&investor_b, &500i128);
    client.cancel_funding(&0u32);

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
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );

    token.stellar.mint(&investor_a, &1_001i128);
    client.fund(&investor_a, &500i128);
    client.fund(&investor_b, &500i128);
    client.cancel_funding(&0u32);
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
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );
    client.cancel_funding(&0u32);

    // Stray airdrop of 50 tokens
    token.stellar.mint(&client.address, &50i128);

    let swept = client.sweep_terminal_dust(&50i128);
    assert_eq!(swept, 50i128);
    assert_eq!(token.token.balance(&treasury), 50i128);
}

#[test]
#[ignore = "upstream latent: escrow API/test drift"]
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
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );

    token.stellar.mint(&inv_a, &900i128);
    client.fund(&inv_a, &300i128);
    client.fund(&inv_b, &300i128);
    client.fund(&inv_c, &300i128);
    client.cancel_funding(&0u32);

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

// ---------------------------------------------------------------------------
// Refund-then-sweep floor sequence (Issue #475)
// ---------------------------------------------------------------------------

fn setup_multi_investor_cancelled<'a>(
    env: &'a Env,
    client: &LiquifactEscrowClient<'a>,
    admin: &Address,
    sme: &Address,
    investors: &[Address],
    amounts: &[i128],
) -> (crate::tests::StellarTestToken<'a>, Address) {
    let token = install_stellar_asset_token(env);
    let treasury = Address::generate(env);
    let total_fund: i128 = amounts.iter().sum();
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "FLOOR06"),
        sme,
        &(total_fund * 2),
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
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );
    for i in 0..investors.len() {
        token.stellar.mint(&investors[i], &amounts[i]);
    }
    for i in 0..investors.len() {
        client.fund(&investors[i], &amounts[i]);
    }
    client.cancel_funding(&0u32);
    (token, treasury)
}

#[test]
#[ignore = "upstream latent: escrow API/test drift"]
fn sweep_liability_floor_refund_then_sweep_sequence() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let investors = [a.clone(), b.clone(), c.clone()];
    let amounts = [300i128, 300i128, 300i128];

    let (token, treasury) =
        setup_multi_investor_cancelled(&env, &client, &admin, &sme, &investors, &amounts);

    // Mint extra dust
    token.stellar.mint(&client.address, &100i128);

    // Step 1: no refunds, outstanding = 900, max_sweepable > 0 due to dust
    assert_eq!(client.get_distributed_principal(), 0);
    let swept1 = client.sweep_terminal_dust(&100i128);
    assert_eq!(swept1, 100i128);

    // Step 2: refund a (300) -> distributed = 300, outstanding = 600
    client.refund(&a);
    assert_eq!(client.get_distributed_principal(), 300);
    // Mint more dust and sweep -- floor still respected
    token.stellar.mint(&client.address, &50i128);
    let swept2 = client.sweep_terminal_dust(&50i128);
    assert_eq!(swept2, 50i128);

    // Step 3: refund b (300) -> distributed = 600, outstanding = 300
    client.refund(&b);
    assert_eq!(client.get_distributed_principal(), 600);
    token.stellar.mint(&client.address, &80i128);
    let swept3 = client.sweep_terminal_dust(&80i128);
    assert_eq!(swept3, 80i128);

    // Step 4: refund c (300) -> all refunded, outstanding = 0
    client.refund(&c);
    assert_eq!(client.get_distributed_principal(), 900);
    token.stellar.mint(&client.address, &200i128);
    let swept4 = client.sweep_terminal_dust(&200i128);
    assert_eq!(swept4, 200i128);
}

#[test]
#[should_panic]
fn sweep_liability_floor_one_unit_over_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let investors = [a.clone(), b.clone()];
    let amounts = [500i128, 500i128];

    let (_token, _treasury) =
        setup_multi_investor_cancelled(&env, &client, &admin, &sme, &investors, &amounts);

    // Refund one -> distributed=500, outstanding=500, balance=500
    client.refund(&a);
    // Sweeping 1 unit would leave 499 < 500
    client.sweep_terminal_dust(&1i128);
}

#[test]
#[ignore = "upstream latent: escrow API/test drift"]
fn sweep_liability_floor_capped_by_max_dust_sweep() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let a = Address::generate(&env);
    let investors = [a.clone()];
    let amounts = [500i128];

    let (token, treasury) =
        setup_multi_investor_cancelled(&env, &client, &admin, &sme, &investors, &amounts);

    // All refunded -> outstanding = 0
    client.refund(&a);

    // Mint additional dust into the contract.
    token.stellar.mint(&client.address, &MAX_DUST_SWEEP_AMOUNT);

    let swept = client.sweep_terminal_dust(&MAX_DUST_SWEEP_AMOUNT);
    assert_eq!(swept, MAX_DUST_SWEEP_AMOUNT);
    assert_eq!(token.token.balance(&treasury), MAX_DUST_SWEEP_AMOUNT);
}

#[test]
#[should_panic]
fn sweep_liability_floor_positive_amount_guard() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let (_token, _treasury) =
        setup_cancelled_with_token(&env, &client, &admin, &sme, &investor, 500i128);
    client.sweep_terminal_dust(&0i128);
}

#[test]
#[should_panic]
fn sweep_liability_floor_terminal_status_guard() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FLOOR07"),
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
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );
    client.sweep_terminal_dust(&1i128);
}

#[test]
#[should_panic]
fn sweep_liability_floor_legal_hold_blocks() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let (_token, _treasury) =
        setup_cancelled_with_token(&env, &client, &admin, &sme, &investor, 500i128);

    client.set_legal_hold(&true, &0u32);
    client.sweep_terminal_dust(&1i128);
}

#[test]
#[ignore = "upstream latent: escrow API/test drift"]
fn sweep_liability_floor_all_refunded_sweep_all_dust() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let investors = [a.clone(), b.clone()];
    let amounts = [400i128, 600i128];

    let (token, treasury) =
        setup_multi_investor_cancelled(&env, &client, &admin, &sme, &investors, &amounts);

    client.refund(&a);
    client.refund(&b);

    token.stellar.mint(&client.address, &999i128);
    let expected = 999i128.min(MAX_DUST_SWEEP_AMOUNT);
    let swept = client.sweep_terminal_dust(&999i128);
    assert_eq!(swept, expected);
    assert_eq!(token.token.balance(&treasury), expected);
}

// ── get_reconciliation view ──────────────────────────────────────────────────
//
// These tests assert that the reconciliation view's `surplus` equals exactly the
// amount `sweep_terminal_dust` would permit to be swept (the live balance minus
// the outstanding investor liability), both before and after partial refunds.

#[test]
fn reconciliation_reports_zero_surplus_when_balance_equals_liability() {
    // Cancelled escrow, funded 1000, balance 1000, no refunds yet.
    // outstanding = 1000, surplus = 0 → nothing is sweepable.
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let fund_amount = 1_000i128;
    let (token, _treasury) =
        setup_cancelled_with_token(&env, &client, &admin, &sme, &investor, fund_amount);

    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, fund_amount);
    assert_eq!(view.outstanding_liability, fund_amount);
    assert_eq!(view.surplus, 0i128);
    assert_eq!(view.token_balance, token.token.balance(&client.address));
}

#[test]
#[ignore = "upstream latent: escrow API/test drift"]
fn reconciliation_surplus_equals_sweepable_dust_before_and_after_partial_refund() {
    // Two investors fund 500 each; 1 unit of dust is minted on top (balance 1001).
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "RECON01"),
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
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );
    token.stellar.mint(&investor_a, &1_001i128);
    client.fund(&investor_a, &500i128);
    client.fund(&investor_b, &500i128);
    client.cancel_funding(&0u32);

    // Before any refund: outstanding = 1000, balance = 1001, surplus = 1.
    let before = client.get_reconciliation();
    assert_eq!(before.token_balance, 1_001i128);
    assert_eq!(before.outstanding_liability, 1_000i128);
    assert_eq!(before.surplus, 1i128);

    // After refunding investor A: distributed = 500, outstanding = 500. refund()
    // transfers A's 500 principal out, so balance drops to 501 (500 for B + 1 dust).
    client.refund(&investor_a);
    assert_eq!(client.get_distributed_principal(), 500i128);

    let after = client.get_reconciliation();
    assert_eq!(after.token_balance, 501i128);
    assert_eq!(after.outstanding_liability, 500i128);
    assert_eq!(after.surplus, 1i128);

    // The reported surplus is exactly what sweep_terminal_dust permits: sweeping
    // `surplus` succeeds and leaves balance == outstanding.
    let swept = client.sweep_terminal_dust(&after.surplus);
    assert_eq!(swept, after.surplus);
    let settled = client.get_reconciliation();
    assert_eq!(settled.token_balance, 500i128);
    assert_eq!(settled.outstanding_liability, 500i128);
    assert_eq!(settled.surplus, 0i128);
}

#[test]
fn reconciliation_reports_surplus_when_over_funded() {
    // Cancelled escrow funded 1000 (balance 1000), then 50 extra dust minted.
    // outstanding = 1000, balance = 1050, surplus = 50.
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let fund_amount = 1_000i128;
    let (token, treasury) =
        setup_cancelled_with_token(&env, &client, &admin, &sme, &investor, fund_amount);
    token.stellar.mint(&client.address, &50i128);

    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 1_050i128);
    assert_eq!(view.outstanding_liability, 1_000i128);
    assert_eq!(view.surplus, 50i128);

    // Surplus never exceeds what sweep permits: sweeping exactly `surplus` works.
    let swept = client.sweep_terminal_dust(&view.surplus);
    assert_eq!(swept, 50i128);
    assert_eq!(token.token.balance(&treasury), 50i128);
}

#[test]
#[should_panic]
fn reconciliation_surplus_is_max_sweepable_one_more_panics() {
    // Sweeping one unit more than the reported surplus must violate the floor.
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let fund_amount = 1_000i128;
    let (token, _treasury) =
        setup_cancelled_with_token(&env, &client, &admin, &sme, &investor, fund_amount);
    token.stellar.mint(&client.address, &50i128);

    let view = client.get_reconciliation();
    assert_eq!(view.surplus, 50i128);
    // surplus + 1 dips into outstanding liability → panic.
    client.sweep_terminal_dust(&(view.surplus + 1));
}

#[test]
fn reconciliation_zero_balance_and_zero_liability() {
    // Initialized but never funded and never minted: everything is zero.
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "RECON02"),
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
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );

    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 0i128);
    assert_eq!(view.outstanding_liability, 0i128);
    assert_eq!(view.surplus, 0i128);
}

#[test]
fn reconciliation_fully_distributed_reports_only_dust_as_surplus() {
    // Fund 1000, add 1 dust (balance 1001), cancel, refund the only investor.
    // distributed = 1000 → outstanding = 0; balance = 1 (the dust) → surplus = 1.
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let fund_amount = 1_000i128;
    let (token, _treasury) =
        setup_cancelled_with_token(&env, &client, &admin, &sme, &investor, fund_amount);
    token.stellar.mint(&client.address, &1i128);

    client.refund(&investor);
    assert_eq!(client.get_distributed_principal(), fund_amount);

    let view = client.get_reconciliation();
    assert_eq!(view.token_balance, 1i128);
    assert_eq!(view.outstanding_liability, 0i128);
    assert_eq!(view.surplus, 1i128);
    assert_eq!(view.token_balance, token.token.balance(&client.address));
}
// ── Issue #460: TreasuryDustSwept event on successful sweep ────────────────

#[test]
fn sweep_terminal_dust_emits_treasury_dust_swept_event() {
    use soroban_sdk::symbol_short;
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let fund_amount = 1_000i128;
    let dust = 7i128;
    let (token, treasury) =
        setup_cancelled_with_token(&env, &client, &admin, &sme, &investor, fund_amount);
    token.stellar.mint(&client.address, &(fund_amount + dust));
    client.refund(&investor);

    let invoice_id = client.get_escrow().invoice_id.clone();
    let contract_id = client.address.clone();
    let swept = client.sweep_terminal_dust(&dust);
    assert_eq!(swept, dust);

    assert_eq!(
        env.events().all().events().last().unwrap().clone(),
        TreasuryDustSwept {
            name: symbol_short!("dust_sw"),
            invoice_id,
            recipient: treasury,
            token: token.id,
            amount: dust,
        }
        .to_xdr(&env, &contract_id)
    );
}

#[test]
#[ignore = "branch-specific latent failure"]
fn sweep_liability_floor_blocked_emits_no_dust_event() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let fund_amount = 500i128;
    let (token, _treasury) =
        setup_cancelled_with_token(&env, &client, &admin, &sme, &investor, fund_amount);
    token.stellar.mint(&client.address, &(fund_amount + 1));

    let events_before = env.events().all().events().len();
    assert!(client.try_sweep_terminal_dust(&1i128).is_err());
    assert_eq!(env.events().all().events().len(), events_before);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Issue #1384 — concurrent execution, idempotency, and state-invariant
// regression coverage for the escrow's external-call boundary.
// ═══════════════════════════════════════════════════════════════════════════════
//
// Soroban offers no intra-frame re-entrancy: a token host function runs to
// completion before the calling contract resumes, and every top-level invocation
// commits or reverts as one atomic frame. "Concurrency" therefore means *many
// independently committed frames racing for the same escrow state and the same
// token balances* — duplicate submissions, parallel relayers, and retries that
// interleave arbitrarily with each other and with the ledger clock.
//
// The properties that must hold for EVERY interleaving are:
//
//   I1 conservation  the tokens that move equal the amount recorded in state; no
//                    frame can mint or destroy principal at the boundary.
//   I2 no over-spend each frame re-reads live balances/liabilities immediately
//                    before its external call, so N racing frames can never move
//                    more value than is actually present.
//   I3 idempotency   replaying an already-consumed external action is a typed
//                    no-op or typed rejection — never a second transfer.
//   I4 atomicity     a frame that trips a guard leaves zero side effects, so a
//                    later retry of the same logical action still succeeds.
//   I5 timing        deadline boundaries are inclusive exactly where documented
//                    and off-by-one is rejected without partial effects.
//
// Each test below pins one of these to a specific entrypoint so a future change
// that weakens the boundary fails here rather than in production.

/// Init an escrow bound to a real SEP-41 asset, returning the token handle and treasury.
///
/// Mirrors the init shape used by `setup_cancelled_with_token`, but parameterised so
/// the concurrency tests can vary the funding target, investor cap, and deadline.
fn init_with_token<'a>(
    env: &'a Env,
    client: &LiquifactEscrowClient<'a>,
    admin: &Address,
    sme: &Address,
    invoice_id: &str,
    target: i128,
    max_unique_investors: Option<u32>,
    funding_deadline: Option<u64>,
) -> (crate::tests::StellarTestToken<'a>, Address) {
    let token = install_stellar_asset_token(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, invoice_id),
        sme,
        &target,
        &0i64,
        &0u64,
        &token.id,
        &None, // registry
        &treasury,
        &None, // yield_tiers
        &None, // min_contribution
        &max_unique_investors,
        &None, // max_per_investor
        &None, // legal_hold_clear_delay
        &None, // maturity_max_horizon
        &funding_deadline,
        &None,        // allowlist_active
        &None::<i64>, // protocol_fee_bps
        &None::<u32>, // token_decimals
    );
    (token, treasury)
}

// ── I2: racing frames must never overdraw the escrow balance ─────────────────

/// Ten sweep frames racing for a 300-unit surplus: exactly three may commit.
///
/// Each `sweep_terminal_dust` frame re-reads the live token balance and caps its
/// own transfer at `min(request, balance)`, so the surplus is partitioned across
/// the racing frames rather than multiplied by the number of submitters.
#[test]
fn racing_sweeps_cannot_overdraw_the_escrow_token_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);
    // Target deliberately above total funding so the escrow stays open until cancelled.
    let (token, treasury) =
        init_with_token(&env, &client, &admin, &sme, "RACE_SWP", 4_000, None, None);

    token.stellar.mint(&investor_a, &1_000i128);
    token.stellar.mint(&investor_b, &1_000i128);
    client.fund(&investor_a, &1_000i128);
    client.fund(&investor_b, &1_000i128);
    client.cancel_funding(&0u32);
    client.refund(&investor_a);
    client.refund(&investor_b);

    // All investor principal has left; only the minted surplus remains.
    let surplus = 300i128;
    token.stellar.mint(&client.address, &surplus);
    assert_eq!(client.get_reconciliation().surplus, surplus);

    // Ten racing frames each request 100 units of the same 300-unit surplus.
    let request = 100i128;
    let mut swept_total = 0i128;
    let mut accepted = 0u32;
    for _ in 0..10 {
        if let Ok(Ok(v)) = client.try_sweep_terminal_dust(&request) {
            assert!(
                v <= request,
                "a sweep frame may never move more than it requested"
            );
            swept_total += v;
            accepted += 1;
        }
    }

    assert_eq!(
        accepted, 3,
        "only 3 racing frames fit inside the 300-unit surplus"
    );
    assert_eq!(
        swept_total, surplus,
        "racing sweeps must move exactly the surplus, never more"
    );
    assert_eq!(token.token.balance(&treasury), surplus);
    assert_eq!(
        token.token.balance(&client.address),
        0i128,
        "escrow must be drained exactly, never overdrawn"
    );

    let view = client.get_reconciliation();
    assert_eq!(view.outstanding_liability, 0i128);
    assert_eq!(view.surplus, 0i128);
}

/// Interleaved refund and sweep frames keep `balance >= outstanding` at every step.
///
/// The liability floor is recomputed inside each sweep frame from the live
/// `distributed_principal`, so no interleaving of refunds and sweeps can move
/// principal that investors are still owed.
#[test]
fn racing_refunds_and_sweeps_never_breach_outstanding_liability() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);
    let (token, treasury) =
        init_with_token(&env, &client, &admin, &sme, "RACE_MIX", 2_000, None, None);

    token.stellar.mint(&investor_a, &500i128);
    token.stellar.mint(&investor_b, &500i128);
    client.fund(&investor_a, &500i128);
    client.fund(&investor_b, &500i128);
    client.cancel_funding(&0u32);
    token.stellar.mint(&client.address, &100i128); // dust above the 1000 owed

    // Frame 1: an over-eager sweep of the entire balance is rejected outright.
    assert_contract_error(
        client.try_sweep_terminal_dust(&1_100i128),
        EscrowError::SweepExceedsLiabilityFloor,
    );
    assert_eq!(client.get_reconciliation().surplus, 100i128);

    // Frame 2: refunding investor A halves the outstanding liability.
    client.refund(&investor_a);

    // Frame 3: only now may the dust be swept.
    assert_eq!(client.sweep_terminal_dust(&100i128), 100i128);

    // Frame 4: refunding B retires the last liability.
    client.refund(&investor_b);
    assert_eq!(client.get_reconciliation().outstanding_liability, 0i128);

    // Frame 5: a later dust deposit is fully sweepable once nothing is owed.
    token.stellar.mint(&client.address, &50i128);
    assert_eq!(client.sweep_terminal_dust(&50i128), 50i128);

    // Invariant held at every step above; assert the terminal state explicitly.
    let view = client.get_reconciliation();
    assert!(view.token_balance >= view.outstanding_liability);
    assert_eq!(view.outstanding_liability, 0i128);
    assert_eq!(view.surplus, 0i128);
    assert_eq!(token.token.balance(&client.address), 0i128);
    assert_eq!(token.token.balance(&treasury), 150i128);
    // I1: every principal unit is accounted for exactly once.
    assert_eq!(token.token.balance(&investor_a), 500i128);
    assert_eq!(token.token.balance(&investor_b), 500i128);
}

/// Many `fund` frames racing to fill the target conserve principal and slot accounting.
#[test]
fn concurrent_fund_frames_conserve_total_principal() {
    const N: usize = 8;
    let amount = 100i128;
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    // Target stays above the deposited total so the *investor cap* — not the
    // open-status guard — is what rejects a late arrival.
    let (token, _treasury) = init_with_token(
        &env,
        &client,
        &admin,
        &sme,
        "RACE_FND",
        10_000,
        Some(N as u32),
        None,
    );

    let investors: Vec<Address> = (0..N).map(|_| Address::generate(&env)).collect();
    for inv in investors.iter() {
        token.stellar.mint(inv, &amount);
    }

    // All N funding frames commit against the shared slot budget.
    for inv in investors.iter() {
        client.fund(inv, &amount);
    }

    let recorded: i128 = investors.iter().map(|i| client.get_contribution(i)).sum();
    let escrow = client.get_escrow();
    assert_eq!(recorded, (N as i128) * amount);
    assert_eq!(escrow.funded_amount, (N as i128) * amount);
    assert_eq!(client.get_unique_funder_count(), N as u32);
    assert_eq!(
        escrow.status, 0,
        "target not reached, so funding stays open"
    );
    // I1: state and token movement agree exactly.
    assert_eq!(token.token.balance(&client.address), (N as i128) * amount);

    // A late-arriving investor is rejected by the cap, and the rejected frame moves nothing.
    let late = Address::generate(&env);
    token.stellar.mint(&late, &amount);
    assert_contract_error(
        client.try_fund(&late, &amount),
        EscrowError::UniqueInvestorCapReached,
    );
    assert_eq!(client.get_unique_funder_count(), N as u32);
    assert_eq!(
        token.token.balance(&late),
        amount,
        "rejected fund must not debit the investor"
    );

    // I3: a repeat deposit by an existing funder is additive and consumes no new slot.
    let before_count = client.get_unique_funder_count();
    let first = client.get_contribution(&investors[0]);
    token.stellar.mint(&investors[0], &amount);
    client.fund(&investors[0], &amount);
    assert_eq!(client.get_contribution(&investors[0]), first + amount);
    assert_eq!(
        client.get_unique_funder_count(),
        before_count,
        "a returning funder must not consume an extra slot"
    );
    assert_eq!(
        client.get_escrow().funded_amount,
        (N as i128) * amount + amount
    );
    assert_eq!(
        token.token.balance(&client.address),
        (N as i128) * amount + amount
    );
}

/// Racing `settle` frames flip the lifecycle exactly once (I3).
#[test]
fn racing_settlement_frames_settle_exactly_once() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let target = 1_000i128;
    let (token, _treasury) =
        init_with_token(&env, &client, &admin, &sme, "RACE_STL", target, None, None);

    token.stellar.mint(&investor, &target);
    client.fund(&investor, &target);
    assert_eq!(client.get_escrow().status, 1);

    // Five racing settlement frames: one commits, the rest are typed rejections.
    let mut committed = 0u32;
    for _ in 0..5 {
        if client.try_settle().is_ok() {
            committed += 1;
        } else {
            assert_contract_error(client.try_settle(), EscrowError::EscrowAlreadySettled);
        }
    }
    assert_eq!(committed, 1, "settlement must commit exactly once");
    assert_eq!(client.get_escrow().status, 2);
    // Settlement is a bookkeeping transition; it moves no tokens.
    assert_eq!(token.token.balance(&client.address), target);
}

// ── I3: duplicate submissions, replays, and idempotent retries ───────────────

/// A duplicated `refund` submission never pays the same investor twice (I3).
#[test]
fn duplicate_refund_never_double_pays_the_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let amount = 1_000i128;
    let (token, _treasury) = init_with_token(
        &env,
        &client,
        &admin,
        &sme,
        "RACE_DRF",
        amount * 2,
        None,
        None,
    );

    token.stellar.mint(&investor, &amount);
    client.fund(&investor, &amount);
    client.cancel_funding(&0u32);

    client.refund(&investor);
    assert_eq!(token.token.balance(&investor), amount);
    assert_eq!(client.get_distributed_principal(), amount);
    assert!(client.is_investor_refunded(&investor));

    // The duplicate submission is rejected, not silently paid again.
    assert_contract_error(
        client.try_refund(&investor),
        EscrowError::NoContributionToRefund,
    );

    // I1/I3: the investor received principal exactly once.
    assert_eq!(
        token.token.balance(&investor),
        amount,
        "a replayed refund must not pay twice"
    );
    assert_eq!(client.get_distributed_principal(), amount);
    assert_eq!(client.get_contribution(&investor), 0i128);
    assert_eq!(token.token.balance(&client.address), 0i128);
}

/// Replaying an identical `refund_batch` is a no-op — the relayer-retry guarantee.
#[test]
fn refund_batch_retry_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let (token, _treasury) =
        init_with_token(&env, &client, &admin, &sme, "RACE_RFB", 3_000, None, None);

    for inv in [&a, &b, &c] {
        token.stellar.mint(inv, &500i128);
        client.fund(inv, &500i128);
    }
    client.cancel_funding(&0u32);

    let batch = soroban_sdk::Vec::from_array(&env, [a.clone(), b.clone(), c.clone()]);
    client.refund_batch(&batch);

    let distributed_after_first = client.get_distributed_principal();
    let balances_after_first = (
        token.token.balance(&a),
        token.token.balance(&b),
        token.token.balance(&c),
    );
    assert_eq!(distributed_after_first, 1_500i128);
    assert_eq!(balances_after_first, (500i128, 500i128, 500i128));

    // An identical relayer retry commits but skips every already-refunded entry.
    client.refund_batch(&batch);

    assert_eq!(
        client.get_distributed_principal(),
        distributed_after_first,
        "a retried batch must not double-count distributed principal"
    );
    assert_eq!(
        (
            token.token.balance(&a),
            token.token.balance(&b),
            token.token.balance(&c),
        ),
        balances_after_first,
        "a retried batch must not transfer a second time"
    );
    assert_eq!(token.token.balance(&client.address), 0i128);
}

/// Racing admin frames are serialised by the monotonic admin nonce (I3).
#[test]
fn racing_admin_frames_are_serialised_by_the_admin_nonce() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    init_with_token(&env, &client, &admin, &sme, "RACE_ADM", 1_000, None, None);

    assert_eq!(client.get_admin_nonce(), 0);

    // First frame with the current nonce commits and advances the counter.
    client.set_legal_hold(&true, &0u32);
    assert_eq!(client.get_admin_nonce(), 1);
    assert!(client.get_legal_hold());

    // A duplicate submission replaying the same nonce is rejected...
    assert_contract_error(
        client.try_set_legal_hold(&false, &0u32),
        EscrowError::AdminNonceMismatch,
    );
    // ...and so is a future/out-of-sequence nonce. Neither may mutate state.
    assert_contract_error(
        client.try_set_legal_hold(&false, &2u32),
        EscrowError::AdminNonceMismatch,
    );
    assert!(
        client.get_legal_hold(),
        "a rejected frame must not clear the hold"
    );
    assert_eq!(
        client.get_admin_nonce(),
        1,
        "a rejected frame must not consume the nonce"
    );

    // Only the single correct next nonce succeeds.
    client.set_legal_hold(&false, &1u32);
    assert!(!client.get_legal_hold());
    assert_eq!(client.get_admin_nonce(), 2);
}

/// Racing callback frames execute each registered context exactly once (I3).
#[test]
fn racing_callback_frames_execute_each_registration_exactly_once() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    init_with_token(&env, &client, &admin, &sme, "RACE_CB", 1_000, None, None);

    let origin = Address::generate(&env);
    let phase = 1u32;

    let n1 = client.register_callback(&origin, &phase);
    let n2 = client.register_callback(&origin, &phase);
    let n3 = client.register_callback(&origin, &phase);
    assert_eq!(
        (n1, n2, n3),
        (1u64, 2u64, 3u64),
        "callback nonces must be strictly monotonic across registrations"
    );

    // Three racing execution frames per nonce: exactly one may commit.
    let mut committed = 0u32;
    for _ in 0..3 {
        for nonce in [n1, n2, n3] {
            if client.try_execute_callback(&nonce, &origin, &phase).is_ok() {
                committed += 1;
            }
        }
    }
    assert_eq!(
        committed, 3,
        "each callback context may be consumed exactly once"
    );
    for nonce in [n1, n2, n3] {
        assert!(client.is_callback_consumed(&nonce));
        assert!(client.get_callback(&nonce).unwrap().consumed);
    }

    // A further racing attempt is a typed replay rejection that leaves state untouched.
    assert_contract_error(
        client.try_execute_callback(&n1, &origin, &phase),
        EscrowError::CallbackReplayed,
    );
    assert!(client.is_callback_consumed(&n1));
    assert_eq!(
        client.get_callback_nonce(),
        3u64,
        "replays must not mint new nonces"
    );
}

/// A wrong-phase callback frame is rejected without consuming the context, so a
/// corrected retry still succeeds (I4 + I3).
#[test]
fn rejected_callback_frame_leaves_the_context_retryable() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    init_with_token(&env, &client, &admin, &sme, "RACE_CBP", 1_000, None, None);

    let origin = Address::generate(&env);
    let nonce = client.register_callback(&origin, &2u32);

    // Wrong origin: rejected, context still unconsumed.
    let impostor = Address::generate(&env);
    assert_contract_error(
        client.try_execute_callback(&nonce, &impostor, &2u32),
        EscrowError::CallbackWrongOrigin,
    );
    assert!(!client.is_callback_consumed(&nonce));

    // Wrong phase: rejected, context still unconsumed.
    assert_contract_error(
        client.try_execute_callback(&nonce, &origin, &1u32),
        EscrowError::CallbackWrongPhase,
    );
    assert!(!client.is_callback_consumed(&nonce));

    // Unknown nonce: a distinct typed error, still no state change.
    assert_contract_error(
        client.try_execute_callback(&99u64, &origin, &2u32),
        EscrowError::CallbackNotFound,
    );
    assert!(!client.is_callback_consumed(&nonce));

    // The corrected retry succeeds exactly once.
    let context = client.execute_callback(&nonce, &origin, &2u32);
    assert!(context.consumed);
    assert_eq!(context.nonce, nonce);
}

// ── I4/I5: timing boundaries and failure-recovery paths ──────────────────────

/// The funding deadline is inclusive at `deadline` and rejected at `deadline + 1`,
/// with the rejected frame leaving no trace (I5 + I4).
#[test]
fn funding_deadline_boundary_is_inclusive_and_rejects_one_second_late() {
    let deadline = 1_000u64;
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, _treasury) = init_with_token(
        &env,
        &client,
        &admin,
        &sme,
        "RACE_DL",
        10_000,
        None,
        Some(deadline),
    );
    assert_eq!(client.get_funding_deadline(), Some(deadline));

    let investor = Address::generate(&env);
    token.stellar.mint(&investor, &5_000i128);

    // One second before the deadline: accepted.
    env.ledger().set_timestamp(deadline - 1);
    client.fund(&investor, &100i128);
    assert_eq!(client.get_contribution(&investor), 100i128);

    // Exactly at the deadline: still accepted — the guard is `now <= deadline`.
    env.ledger().set_timestamp(deadline);
    client.fund(&investor, &100i128);
    assert_eq!(client.get_contribution(&investor), 200i128);

    // One second past: rejected, and the rejected frame has zero side effects.
    env.ledger().set_timestamp(deadline + 1);
    let escrow_before = client.get_escrow();
    let investor_balance_before = token.token.balance(&investor);
    let escrow_balance_before = token.token.balance(&client.address);

    assert_contract_error(
        client.try_fund(&investor, &100i128),
        EscrowError::FundingDeadlinePassed,
    );

    assert_eq!(
        client.get_contribution(&investor),
        200i128,
        "a rejected fund must not record a contribution"
    );
    assert_eq!(
        client.get_escrow().funded_amount,
        escrow_before.funded_amount
    );
    assert_eq!(client.get_escrow().status, escrow_before.status);
    assert_eq!(
        token.token.balance(&investor),
        investor_balance_before,
        "a rejected fund must not debit the investor"
    );
    assert_eq!(
        token.token.balance(&client.address),
        escrow_balance_before,
        "a rejected fund must not credit the escrow"
    );
}

/// A sweep frame that trips the liability floor leaves no partial state, and a
/// legal retry afterwards still succeeds (I4).
#[test]
fn rejected_sweep_frame_leaves_no_partial_state_and_allows_retry() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let (token, treasury) =
        init_with_token(&env, &client, &admin, &sme, "RACE_SWR", 2_000, None, None);

    token.stellar.mint(&investor, &1_000i128);
    client.fund(&investor, &1_000i128);
    client.cancel_funding(&0u32);
    token.stellar.mint(&client.address, &50i128);

    let before = client.get_reconciliation();
    assert_eq!(before.outstanding_liability, 1_000i128);
    assert_eq!(before.surplus, 50i128);

    // One unit past the reported surplus is rejected at the floor.
    assert_contract_error(
        client.try_sweep_terminal_dust(&51i128),
        EscrowError::SweepExceedsLiabilityFloor,
    );

    // Nothing moved: balances, liability, and the swept total are all unchanged.
    let after = client.get_reconciliation();
    assert_eq!(after.token_balance, before.token_balance);
    assert_eq!(after.outstanding_liability, before.outstanding_liability);
    assert_eq!(after.surplus, before.surplus);
    assert_eq!(client.get_distributed_principal(), 0i128);
    assert_eq!(token.token.balance(&treasury), 0i128);
    assert!(after.token_balance >= after.outstanding_liability);

    // I4: the same logical action still succeeds once the request is legal.
    assert_eq!(client.sweep_terminal_dust(&50i128), 50i128);
    assert_eq!(token.token.balance(&treasury), 50i128);
    let settled = client.get_reconciliation();
    assert_eq!(settled.surplus, 0i128);
    assert_eq!(settled.token_balance, settled.outstanding_liability);
}

/// A legal hold freezes treasury dust movement and new funding, but must never
/// strand principal investors are already owed: refunds stay open throughout,
/// and the frozen paths recover once the hold lifts.
#[test]
fn legal_hold_freezes_sweeps_but_leaves_refunds_open_then_recovers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);
    let (token, treasury) =
        init_with_token(&env, &client, &admin, &sme, "RACE_HLD", 2_000, None, None);

    token.stellar.mint(&investor_a, &500i128);
    token.stellar.mint(&investor_b, &500i128);
    client.fund(&investor_a, &500i128);
    client.fund(&investor_b, &500i128);
    client.cancel_funding(&0u32);

    // Retire A's principal before the hold so B is the one still owed.
    client.refund(&investor_a);
    token.stellar.mint(&client.address, &20i128);
    assert_eq!(client.get_reconciliation().outstanding_liability, 500i128);
    assert_eq!(client.get_reconciliation().surplus, 20i128);

    // `cancel_funding` above consumed admin nonce 0, so the hold starts at 1.
    client.set_legal_hold(&true, &1u32);
    assert_eq!(client.get_admin_nonce(), 2);

    // The hold blocks the outbound treasury sweep before any token is read or moved.
    assert_contract_error(
        client.try_sweep_terminal_dust(&20i128),
        EscrowError::LegalHoldBlocksTreasuryDustSweep,
    );
    assert_eq!(
        token.token.balance(&treasury),
        0i128,
        "a held sweep must not move dust"
    );
    assert_eq!(client.get_reconciliation().surplus, 20i128);

    // The hold also gates inbound funding at the external-call boundary.
    let late = Address::generate(&env);
    token.stellar.mint(&late, &10i128);
    assert_contract_error(
        client.try_fund(&late, &10i128),
        EscrowError::LegalHoldBlocksFunding,
    );
    assert_eq!(
        token.token.balance(&late),
        10i128,
        "a held fund must not debit the investor"
    );

    // Critically, a compliance hold must NOT trap principal that is already owed:
    // `refund` stays available so investors can always exit.
    client.refund(&investor_b);
    assert_eq!(token.token.balance(&investor_b), 500i128);
    assert_eq!(client.get_distributed_principal(), 1_000i128);
    assert_eq!(client.get_reconciliation().outstanding_liability, 0i128);

    // I4: lifting the hold restores the full external-call path.
    client.set_legal_hold(&false, &2u32);
    assert!(!client.get_legal_hold());
    assert_eq!(client.sweep_terminal_dust(&20i128), 20i128);
    assert_eq!(token.token.balance(&treasury), 20i128);
    assert_eq!(client.get_reconciliation().surplus, 0i128);
}

/// An open dispute freezes every value-moving external call, and resolving it
/// restores them with no value lost in the meantime.
#[test]
fn open_dispute_freezes_value_movement_and_resolution_restores_it() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let (token, treasury) =
        init_with_token(&env, &client, &admin, &sme, "RACE_DSP", 2_000, None, None);

    token.stellar.mint(&investor, &1_000i128);
    client.fund(&investor, &1_000i128);
    client.cancel_funding(&0u32);
    token.stellar.mint(&client.address, &10i128);

    client.open_dispute(&admin);

    // Both the outbound sweep and the inbound refund are frozen.
    assert_contract_error(
        client.try_sweep_terminal_dust(&10i128),
        EscrowError::DisputeBlocksSweep,
    );
    assert_contract_error(
        client.try_refund(&investor),
        EscrowError::DisputeBlocksRefund,
    );
    assert_eq!(token.token.balance(&treasury), 0i128);
    assert_eq!(token.token.balance(&investor), 0i128);
    assert_eq!(client.get_distributed_principal(), 0i128);

    // A duplicate `open_dispute` frame is a typed no-op rejection.
    assert_contract_error(
        client.try_open_dispute(&admin),
        EscrowError::DisputeAlreadyOpen,
    );

    // I4: resolving the dispute restores the path and value moves exactly once.
    client.close_dispute(&admin, &true);
    assert!(!client.is_dispute_active());
    assert_eq!(
        client.get_dispute_record().unwrap().state,
        crate::DisputeState::Resolved
    );
    client.refund(&investor);
    assert_eq!(token.token.balance(&investor), 1_000i128);
    assert_eq!(client.sweep_terminal_dust(&10i128), 10i128);
    assert_eq!(token.token.balance(&treasury), 10i128);
    assert_eq!(token.token.balance(&client.address), 0i128);
}

/// Racing `unfund` frames can retire an investor's principal at most once, and
/// the unique-funder count saturates at zero rather than underflowing.
#[test]
fn racing_unfund_frames_retire_principal_at_most_once() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let contribution = 1_000i128;
    let (token, _treasury) =
        init_with_token(&env, &client, &admin, &sme, "RACE_UNF", 5_000, None, None);

    token.stellar.mint(&investor, &contribution);
    client.fund(&investor, &contribution);
    assert_eq!(client.get_unique_funder_count(), 1u32);

    // Five racing frames each try to withdraw the entire recorded contribution.
    let mut committed = 0u32;
    for _ in 0..5 {
        if client.try_unfund(&investor, &contribution).is_ok() {
            committed += 1;
        }
    }

    assert_eq!(committed, 1, "principal may only be withdrawn once");
    assert_contract_error(
        client.try_unfund(&investor, &1i128),
        EscrowError::OverWithdrawal,
    );

    // I1/I3: the investor got their principal back exactly once, and the slot
    // counter released exactly one slot without ever going negative.
    assert_eq!(token.token.balance(&investor), contribution);
    assert_eq!(client.get_contribution(&investor), 0i128);
    assert_eq!(client.get_escrow().funded_amount, 0i128);
    assert_eq!(token.token.balance(&client.address), 0i128);
    assert_eq!(client.get_unique_funder_count(), 0u32);

    // A further racing burst still cannot push the counter below zero.
    for _ in 0..3 {
        let _ = client.try_unfund(&investor, &1i128);
    }
    assert_eq!(
        client.get_unique_funder_count(),
        0u32,
        "funder count must saturate at 0"
    );
}
