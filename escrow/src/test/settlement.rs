use super::*;

// Settlement, withdrawal, investor claims, maturity boundaries, and dust sweep flows.

#[test]
fn test_settle_after_full_funding() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV003"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &TARGET);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_before_funded_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV011"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.settle();
}

#[test]
fn test_settle_requires_sme_auth() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV006"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    client.settle();
    assert!(
        env.auths().iter().any(|(addr, _)| *addr == sme),
        "sme auth was not recorded for settle"
    );
}

#[test]
#[should_panic]
fn test_settle_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV008"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    env.mock_auths(&[]);
    client.settle();
}

#[test]
#[should_panic(expected = "Escrow has not yet reached maturity")]
fn test_settle_before_maturity_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV032"),
        &sme,
        &1_000i128,
        &500i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    client.settle();
}

#[test]
fn test_settle_after_maturity_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV033"),
        &sme,
        &1_000i128,
        &500i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    env.ledger().set_timestamp(1001);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
fn test_settle_at_exact_maturity_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV034"),
        &sme,
        &1_000i128,
        &500i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    env.ledger().set_timestamp(1000);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
fn test_settle_with_zero_maturity_succeeds_immediately() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV035"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
fn test_settle_at_timestamp_zero_before_maturity_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV036"),
        &sme,
        &1_000i128,
        &500i64,
        &500u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.settle();
    }));
    assert!(
        result.is_err(),
        "Expected panic when settling before maturity"
    );
}

#[test]
fn test_withdraw_funded_then_cannot_settle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "WD001"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &TARGET);
    let wd = client.withdraw();
    assert_eq!(wd.status, 3);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.settle();
    }))
    .is_err());
}

#[test]
fn test_claim_investor_twice_is_idempotent() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CL001"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    client.settle();

    // First claim - should succeed and set the claimed marker
    client.claim_investor_payout(&investor);

    assert!(client.is_investor_claimed(&investor));

    // Second claim - should be idempotent (no-op, does not panic)
    client.claim_investor_payout(&investor);
    assert!(client.is_investor_claimed(&investor));
}

#[test]
#[should_panic(expected = "Address has no contribution to claim")]
fn test_claim_by_non_investor_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let stranger = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "STR001"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    // Escrow settled but stranger never funded
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.settle();

    client.claim_investor_payout(&stranger);
}

#[test]
fn test_clashing_investors_have_independent_claims() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CLASH01"),
        &sme,
        &2_000i128,
        &400i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&inv_a, &1_000i128);
    client.fund(&inv_b, &1_000i128);
    client.settle();

    client.claim_investor_payout(&inv_a);
    assert!(client.is_investor_claimed(&inv_a));
    assert!(!client.is_investor_claimed(&inv_b));

    client.claim_investor_payout(&inv_b);
    assert!(client.is_investor_claimed(&inv_b));
}

#[test]
#[should_panic(expected = "Escrow must be settled before investor claim")]
fn test_claim_before_settle_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CL002"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &1_000i128);
    client.claim_investor_payout(&investor);
}

#[test]
#[should_panic(expected = "Investor commitment lock not expired")]
fn test_claim_blocked_until_commitment_ledger_time() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "LOCK001"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    client.fund_with_commitment(&inv, &1_000i128, &500u64);
    client.settle();
    client.claim_investor_payout(&inv);
}

#[test]
fn test_claim_succeeds_after_commitment_and_settle() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "LOCK002"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    client.fund_with_commitment(&inv, &1_000i128, &100u64);
    client.settle();
    env.ledger().set_timestamp(150);
    client.claim_investor_payout(&inv);
    assert!(client.is_investor_claimed(&inv));
}

#[test]
fn test_cost_baseline_settle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV103b"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &TARGET);
    env.ledger().set_timestamp(1001);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
fn test_cost_baseline_full_lifecycle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV110"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &TARGET);
    env.ledger().set_timestamp(1000);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
fn test_sweep_terminal_dust_after_settle_transfers_to_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (escrow_id, client) = deploy_with_id(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW001"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.settle();

    token.stellar.mint(&escrow_id, &5_000i128);
    let before_t = token.token.balance(&treasury);
    let swept = client.sweep_terminal_dust(&5_000i128);
    assert_eq!(swept, 5_000i128);
    assert_eq!(token.token.balance(&treasury), before_t + 5_000i128);
}

#[test]
fn test_sweep_terminal_dust_after_withdraw_and_ledger_tick() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (escrow_id, client) = deploy_with_id(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW002"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.withdraw();

    env.ledger()
        .set_sequence_number(env.ledger().sequence() + 10);

    token.stellar.mint(&escrow_id, &333i128);
    let swept = client.sweep_terminal_dust(&333i128);
    assert_eq!(swept, 333i128);
}

#[test]
#[should_panic(expected = "dust sweep only in terminal states")]
fn test_sweep_rejected_when_open() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (escrow_id, client) = deploy_with_id(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW003"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    token.stellar.mint(&escrow_id, &100i128);
    client.sweep_terminal_dust(&100i128);
}

#[test]
#[should_panic(expected = "Legal hold blocks treasury dust sweep")]
fn test_sweep_blocked_under_legal_hold() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (_escrow_id, client) = deploy_with_id(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW004"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.settle();
    client.set_legal_hold(&true);
    client.sweep_terminal_dust(&1i128);
}

#[test]
#[should_panic(expected = "sweep amount exceeds MAX_DUST_SWEEP_AMOUNT")]
fn test_sweep_rejects_amount_above_dust_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (_escrow_id, client) = deploy_with_id(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW005"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.settle();
    client.sweep_terminal_dust(&(MAX_DUST_SWEEP_AMOUNT + 1));
}

#[test]
fn test_sweep_caps_at_contract_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (escrow_id, client) = deploy_with_id(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW006"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.settle();

    token.stellar.mint(&escrow_id, &50i128);
    let swept = client.sweep_terminal_dust(&100i128);
    assert_eq!(swept, 50i128);
}

#[test]
fn test_sweep_requires_treasury_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (escrow_id, client) = deploy_with_id(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SW007"),
        &sme,
        &1_000i128,
        &100i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &1_000i128);
    client.settle();
    token.stellar.mint(&escrow_id, &10i128);

    env.mock_auths(&[]);
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.sweep_terminal_dust(&10i128);
    }));
    assert!(err.is_err(), "sweep without treasury auth must fail");
}

#[test]
fn test_differential_settle_maturity_minus_one_vs_exact() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    const M: u64 = 10_000;
    client.init(
        &admin,
        &String::from_str(&env, "DIFF001"),
        &sme,
        &1_000i128,
        &300i64,
        &M,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
    );
    client.fund(&inv, &1_000i128);
    env.ledger().set_timestamp(M - 1);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.settle();
    }))
    .is_err());
    env.ledger().set_timestamp(M);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}
