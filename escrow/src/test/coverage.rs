use crate::{
    test::{free_addresses, setup},
    DataKey, YieldTier,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Vec as SorobanVec,
};

#[test]
#[should_panic(expected = "from_version does not match stored version")]
fn test_migrate_wrong_version() {
    let env = Env::default();
    let (client, _admin, _sme) = setup(&env);
    client.migrate(&1);
}

#[test]
#[should_panic(expected = "Already at current schema version")]
fn test_migrate_already_current() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.migrate(&5);
}

#[test]
#[should_panic(expected = "No migration path from version 0 - extend migrate or redeploy")]
fn test_migrate_no_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    env.as_contract(&client.address, || {
        env.storage().instance().set(&DataKey::Version, &0u32);
    });

    client.migrate(&0);
}

#[test]
fn test_admin_and_maturity_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    let updated = client.update_maturity(&200);
    assert_eq!(updated.maturity, 200);

    let new_admin = Address::generate(&env);
    let updated = client.transfer_admin(&new_admin);
    assert_eq!(updated.admin, new_admin);
}

#[test]
#[should_panic(expected = "Maturity can only be updated in Open state")]
fn test_update_maturity_not_open() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    client.fund(&investor, &100);
    client.update_maturity(&200);
}

#[test]
#[should_panic(expected = "New admin must differ from current admin")]
fn test_transfer_admin_same_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.transfer_admin(&admin);
}

#[test]
#[should_panic(expected = "Legal hold blocks new funding while active")]
fn test_fund_during_legal_hold() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.set_legal_hold(&true);
    let investor = Address::generate(&env);
    client.fund(&investor, &10);
}

#[test]
#[should_panic(expected = "funding amount below min_contribution floor")]
fn test_fund_below_floor() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &Some(50),
        &None,
    );

    let investor = Address::generate(&env);
    client.fund(&investor, &10);
}

#[test]
#[should_panic(expected = "Escrow must be settled before investor claim")]
fn test_claim_not_settled() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    client.fund(&investor, &10);
    client.claim_investor_payout(&investor);
}

#[test]
#[should_panic(expected = "Investor commitment lock not expired (ledger timestamp)")]
fn test_claim_lock_not_expired() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    client.fund_with_commitment(&investor, &100, &3600);

    env.ledger().with_mut(|li| li.timestamp = 101);
    client.settle();

    client.claim_investor_payout(&investor);
}

#[test]
fn test_all_getters() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);
    let registry = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &Some(registry.clone()),
        &treasury,
        &None,
        &Some(10),
        &Some(5),
    );

    assert_eq!(client.get_funding_token(), funding_token);
    assert_eq!(client.get_treasury(), treasury);
    assert_eq!(client.get_registry_ref(), Some(registry));
    assert_eq!(client.get_version(), 5);
    assert!(!client.get_legal_hold());
    assert_eq!(client.get_min_contribution_floor(), 10);
    assert_eq!(client.get_max_unique_investors_cap(), Some(5));
    assert_eq!(client.get_unique_funder_count(), 0);
    assert!(client.get_primary_attestation_hash().is_none());
    assert_eq!(client.get_attestation_append_log().len(), 0);
}

#[test]
fn test_attestations_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    let hash1 = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    let hash2 = soroban_sdk::BytesN::from_array(&env, &[2u8; 32]);

    client.bind_primary_attestation_hash(&hash1);
    assert_eq!(client.get_primary_attestation_hash(), Some(hash1.clone()));

    client.append_attestation_digest(&hash2);
    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 1);
    assert_eq!(log.get(0).unwrap(), hash2);
}

#[test]
#[should_panic(expected = "primary attestation already bound")]
fn test_bind_primary_attestation_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    let hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    client.bind_primary_attestation_hash(&hash);
    client.bind_primary_attestation_hash(&hash);
}

#[test]
fn test_unique_investors_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "CAP"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &Some(2),
    );

    client.fund(&Address::generate(&env), &10);
    client.fund(&Address::generate(&env), &10);
    assert_eq!(client.get_unique_funder_count(), 2);
}

#[test]
#[should_panic(expected = "unique investor cap reached")]
fn test_unique_investors_cap_exceeded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "CAP"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &Some(1),
    );

    client.fund(&Address::generate(&env), &10);
    client.fund(&Address::generate(&env), &10);
}

#[test]
fn test_sweep_terminal_dust_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let token = crate::test::install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &100);
    env.ledger().with_mut(|li| li.timestamp = 200);
    client.settle();

    token.stellar.mint(&client.address, &50);

    let swept = client.sweep_terminal_dust(&50);
    assert_eq!(swept, 50);
    assert_eq!(token.token.balance(&treasury), 50);
}

#[test]
#[should_panic(expected = "dust sweep only in terminal states (settled or withdrawn)")]
fn test_sweep_not_terminal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.sweep_terminal_dust(&10);
}

#[test]
#[should_panic(expected = "no funding token balance to sweep")]
fn test_sweep_no_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let token = crate::test::install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &100);
    env.ledger().with_mut(|li| li.timestamp = 200);
    client.settle();

    client.sweep_terminal_dust(&10);
}

#[test]
fn test_withdraw_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "W"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &100);
    assert_eq!(client.get_escrow().status, 1);

    let updated = client.withdraw();
    assert_eq!(updated.status, 3);
}

#[test]
#[should_panic(expected = "Escrow has not yet reached maturity")]
fn test_settle_too_early() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &20000,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &100);
    client.settle();
}

#[test]
fn test_update_funding_target_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    let updated = client.update_funding_target(&200);
    assert_eq!(updated.funding_target, 200);
}

#[test]
#[should_panic(expected = "Target cannot be less than already funded amount")]
fn test_update_funding_target_too_low() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &50);
    client.update_funding_target(&40);
}

#[test]
fn test_sme_collateral_commitment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    let asset = soroban_sdk::Symbol::new(&env, "GOLD");
    let commitment = client.record_sme_collateral_commitment(&asset, &5000);
    assert_eq!(commitment.amount, 5000);
    assert_eq!(commitment.asset, asset);

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, 5000);
}

#[test]
fn test_clear_legal_hold_convenience() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.set_legal_hold(&true);
    assert!(client.get_legal_hold());
    client.clear_legal_hold();
    assert!(!client.get_legal_hold());
}

#[test]
fn test_claim_not_before_getter() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    client.fund_with_commitment(&investor, &50, &1000);
    let nbf = client.get_investor_claim_not_before(&investor);
    assert!(nbf > 0);
}

#[test]
fn test_init_with_tiers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 500,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 600,
    });

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &1000,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
    );
    assert_eq!(client.get_escrow().yield_bps, 100); // Default yield
}

#[test]
#[should_panic(expected = "sweep amount exceeds MAX_DUST_SWEEP_AMOUNT")]
fn test_sweep_too_much() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &100);
    env.ledger().with_mut(|li| li.timestamp = 200);
    client.settle();

    client.sweep_terminal_dust(&(crate::MAX_DUST_SWEEP_AMOUNT + 1));
}

#[test]
#[should_panic(expected = "Escrow must be funded before withdrawal")]
fn test_withdraw_not_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.withdraw();
}

#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_not_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.settle();
}

#[test]
fn test_fund_with_zero_commitment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    client.fund_with_commitment(&investor, &50, &0);
    assert_eq!(client.get_investor_claim_not_before(&investor), 0);
}

#[test]
#[should_panic(expected = "Target must be strictly positive")]
fn test_update_target_invalid() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.update_funding_target(&0);
}

#[test]
#[should_panic(expected = "yield_bps must be between 0 and 10_000")]
fn test_init_yield_out_of_range() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10001,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic(expected = "min_contribution must be positive when configured")]
fn test_init_min_contribution_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &Some(0),
        &None,
    );
}

#[test]
#[should_panic(expected = "tiers must have strictly increasing min_lock_secs")]
fn test_init_tiers_unsorted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 500,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 600,
    });
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
    );
}

#[test]
#[should_panic(expected = "tiers must have non-decreasing yield_bps")]
fn test_init_tiers_not_increasing_yield() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 600,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 500,
    });
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
    );
}

#[test]
#[should_panic(expected = "tier yield_bps must be >= base yield_bps")]
fn test_init_tiers_lower_than_base() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 50,
    });
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
    );
}

#[test]
fn test_get_yield_bps_empty_tiers_branch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    // Inject empty tiers directly to trigger the branch in get_yield_bps_for_commitment
    env.as_contract(&client.address, || {
        let empty_tiers: SorobanVec<YieldTier> = SorobanVec::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::YieldTierTable, &empty_tiers);
    });

    let investor = Address::generate(&env);
    // This will trigger line 489 in lib.rs
    client.fund_with_commitment(&investor, &10, &0);
}

#[test]
#[should_panic(expected = "tier yield_bps must be 0..=10_000")]
fn test_init_tier_yield_out_of_range() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 10001,
    });
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
    );
}
