use super::*;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Set up two investors A and B, each funding `amount` into a fresh escrow.
/// Returns `(client, admin, investor_a, investor_b, token_id, escrow_id)`.
fn setup_two_investors(
    env: &Env,
    amount: i128,
) -> (
    LiquifactEscrowClient<'_>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();

    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token_id = sac.address();
    let sac_admin = soroban_sdk::token::StellarAssetClient::new(env, &token_id);

    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &escrow_id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "INV-FREEZE"),
        &sme,
        &(amount * 2),
        &800i64,
        &0u64,
        &token_id,
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

    let investor_a = Address::generate(env);
    let investor_b = Address::generate(env);

    sac_admin.mint(&investor_a, &amount);
    sac_admin.mint(&investor_b, &amount);

    client.fund(&investor_a, &amount);
    client.fund(&investor_b, &amount);

    (client, admin, investor_a, investor_b, token_id, escrow_id)
}

// ── set_investor_frozen / get_investor_freeze ────────────────────────────────

#[test]
fn freeze_stores_and_view_returns_correct_values() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, investor_a, _investor_b, _token_id, _escrow_id) =
        setup_two_investors(&env, 1_000_000);

    // initially not frozen
    let (frozen, code) = client.get_investor_freeze(&investor_a);
    assert!(!frozen);
    assert_eq!(code, 0);

    // freeze with reason code 42
    client.set_investor_frozen(&investor_a, &true, &42u32);
    let (frozen, code) = client.get_investor_freeze(&investor_a);
    assert!(frozen);
    assert_eq!(code, 42);

    // unfreeze
    client.set_investor_frozen(&investor_a, &false, &0u32);
    let (frozen, code) = client.get_investor_freeze(&investor_a);
    assert!(!frozen);
    assert_eq!(code, 0);
}

#[test]
fn freeze_no_op_fails_with_unchanged_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, investor_a, _investor_b, _token_id, _escrow_id) =
        setup_two_investors(&env, 1_000_000);

    // freeze once, then try to freeze again — should fail
    client.set_investor_frozen(&investor_a, &true, &1u32);
    let result = client.try_set_investor_frozen(&investor_a, &true, &1u32);
    assert_contract_error(result, EscrowError::InvestorFreezeUnchanged);
}

#[test]
fn non_admin_cannot_freeze() {
    let env = Env::default();
    let (client, _admin, investor_a, investor_b, _token_id, _escrow_id) =
        setup_two_investors(&env, 1_000_000);

    // only mock auths for investor_b (not the admin)
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &investor_b,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "set_investor_frozen",
            args: (investor_a.clone(), true, 0u32).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_set_investor_frozen(&investor_a, &true, &0u32);
    // Should fail with auth error (not admin)
    assert!(result.is_err());
}

// ── freeze event payload ─────────────────────────────────────────────────────

#[test]
fn freeze_emits_correct_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, investor_a, _investor_b, _token_id, _escrow_id) =
        setup_two_investors(&env, 1_000_000);

    client.set_investor_frozen(&investor_a, &true, &99u32);

    let events = env.events().all();
    // find the inv_frz event
    let freeze_event = events.iter().find(|(_, topics, _)| {
        if let soroban_sdk::Val::from(topics) = topics {
            // topics is a Vec<Val>; check first topic is the symbol
            true
        } else {
            false
        }
    });

    // Simpler: check that the last event data contains frozen=1 and reason_code=99
    // The contractevent macro encodes non-topic fields as the data Val.
    let all: soroban_sdk::Vec<(
        soroban_sdk::Address,
        soroban_sdk::Vec<soroban_sdk::Val>,
        soroban_sdk::Val,
    )> = env.events().all();
    let mut found = false;
    for i in 0..all.len() {
        let (_contract, topics, data) = all.get(i).unwrap();
        // topics[0] should be symbol_short!("inv_frz")
        if topics.len() >= 1 {
            let t0: Result<soroban_sdk::Symbol, _> = topics.get(0).unwrap().try_into_val(&env);
            if let Ok(sym) = t0 {
                if sym == soroban_sdk::symbol_short!("inv_frz") {
                    // decode data as (frozen: u32, reason_code: u32)
                    let (frozen_val, reason_val): (u32, u32) = data.try_into_val(&env).unwrap();
                    assert_eq!(frozen_val, 1u32);
                    assert_eq!(reason_val, 99u32);
                    found = true;
                }
            }
        }
    }
    assert!(found, "inv_frz event not found");
}

// ── claim_investor_payout blocked for frozen ──────────────────────────────────

#[test]
fn frozen_investor_cannot_claim_unfrozen_can() {
    let env = Env::default();
    env.mock_all_auths();

    let amount: i128 = 1_000_000;
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_id);
    let token = soroban_sdk::token::TokenClient::new(&env, &token_id);

    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV-CLAIM-FREEZE"),
        &sme,
        &(amount * 2),
        &800i64,
        &0u64,
        &token_id,
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

    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);

    sac_admin.mint(&investor_a, &amount);
    sac_admin.mint(&investor_b, &amount);
    client.fund(&investor_a, &amount);
    client.fund(&investor_b, &amount);

    // settle
    client.settle(&sme);
    // mint payout tokens to the escrow so transfers can succeed
    let total_payout = amount * 2 + (amount * 2 * 800 / 10_000);
    sac_admin.mint(&escrow_id, &total_payout);

    // freeze investor_a
    client.set_investor_frozen(&investor_a, &true, &1u32);

    // investor_a's claim fails
    let res = client.try_claim_investor_payout(&investor_a);
    assert_contract_error(res, EscrowError::InvestorFrozen);

    // record investor_b's balance before claim
    let b_before = token.balance(&investor_b);

    // investor_b's claim succeeds
    client.claim_investor_payout(&investor_b);
    let b_after = token.balance(&investor_b);
    assert!(b_after > b_before, "investor_b should have received payout");

    // unfreeze investor_a, then their claim succeeds
    client.set_investor_frozen(&investor_a, &false, &0u32);
    let a_before = token.balance(&investor_a);
    client.claim_investor_payout(&investor_a);
    let a_after = token.balance(&investor_a);
    assert!(
        a_after > a_before,
        "investor_a should receive payout after unfreeze"
    );
}

// ── fund blocked for frozen ──────────────────────────────────────────────────

#[test]
fn frozen_investor_cannot_fund_unfrozen_can() {
    let env = Env::default();
    env.mock_all_auths();

    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_id);

    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV-FUND-FREEZE"),
        &sme,
        &10_000_000i128,
        &800i64,
        &0u64,
        &token_id,
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

    let investor_a = Address::generate(&env);
    sac_admin.mint(&investor_a, &5_000_000i128);

    // freeze before any funding
    client.set_investor_frozen(&investor_a, &true, &7u32);

    let res = client.try_fund(&investor_a, &1_000_000i128);
    assert_contract_error(res, EscrowError::InvestorFrozen);

    // unfreeze → funding works
    client.set_investor_frozen(&investor_a, &false, &0u32);
    client.fund(&investor_a, &1_000_000i128);
    let (frozen, _) = client.get_investor_freeze(&investor_a);
    assert!(!frozen);
}

// ── unfund blocked for frozen ────────────────────────────────────────────────

#[test]
fn frozen_investor_cannot_unfund_unfrozen_can() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, investor_a, _investor_b, _token_id, _escrow_id) =
        setup_two_investors(&env, 1_000_000);

    client.set_investor_frozen(&investor_a, &true, &3u32);

    let res = client.try_unfund(&investor_a, &500_000i128);
    assert_contract_error(res, EscrowError::InvestorFrozen);

    client.set_investor_frozen(&investor_a, &false, &0u32);
    // should succeed after unfreeze
    client.unfund(&investor_a, &500_000i128);
}

// ── refund blocked for frozen ────────────────────────────────────────────────

#[test]
fn frozen_investor_cannot_refund_unfrozen_can() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, investor_a, _investor_b, _token_id, escrow_id) =
        setup_two_investors(&env, 1_000_000);

    // cancel the escrow
    client.cancel(&client.get_escrow().admin);

    client.set_investor_frozen(&investor_a, &true, &5u32);

    let res = client.try_refund(&investor_a);
    assert_contract_error(res, EscrowError::InvestorFrozen);

    // unfreeze → refund works
    client.set_investor_frozen(&investor_a, &false, &0u32);
    client.refund(&investor_a);
}

// ── refund_batch skips frozen ────────────────────────────────────────────────

#[test]
fn refund_batch_skips_frozen_refunds_unfrozen() {
    let env = Env::default();
    env.mock_all_auths();

    let amount: i128 = 1_000_000;
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_id);
    let token = soroban_sdk::token::TokenClient::new(&env, &token_id);

    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "BATCH-FREEZE"),
        &sme,
        &(amount * 2),
        &800i64,
        &0u64,
        &token_id,
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

    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);

    sac_admin.mint(&investor_a, &amount);
    sac_admin.mint(&investor_b, &amount);
    client.fund(&investor_a, &amount);
    client.fund(&investor_b, &amount);

    // cancel escrow
    client.cancel(&admin);

    // freeze investor_a
    client.set_investor_frozen(&investor_a, &true, &9u32);

    let b_before = token.balance(&investor_b);
    let a_before = token.balance(&investor_a);

    let mut batch = soroban_sdk::Vec::new(&env);
    batch.push_back(investor_a.clone());
    batch.push_back(investor_b.clone());

    // batch should succeed (investor_a is silently skipped)
    client.refund_batch(&batch);

    // investor_b was refunded
    let b_after = token.balance(&investor_b);
    assert!(b_after > b_before, "investor_b should be refunded");

    // investor_a was NOT refunded (still frozen)
    let a_after = token.balance(&investor_a);
    assert_eq!(
        a_after, a_before,
        "investor_a should NOT be refunded while frozen"
    );
}
