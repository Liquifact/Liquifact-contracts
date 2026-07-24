#[cfg(test)]
use super::{
    default_init, deploy, deploy_with_id, free_addresses, install_stellar_asset_token, setup,
    EscrowError, MAX_CLAIMABLE_PAYOUT_BATCH,
};
use soroban_sdk::{Address, Env, String, Vec as SorobanVec};

// Basic happy-path and edge-case tests for get_claimable_payouts

#[test]
fn test_get_claimable_payouts_happy_path_and_ordering() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "BATCH001"),
        &sme,
        &600i128,
        &800i64,
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
    );

    // Three investors: A=100, B=200, C=300 => total 600
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);

    token.stellar.mint(&inv_a, &100i128);
    token.stellar.mint(&inv_b, &200i128);
    token.stellar.mint(&inv_c, &300i128);

    client.fund(&inv_a, &100i128);
    client.fund(&inv_b, &200i128);
    client.fund(&inv_c, &300i128);

    client.settle();

    // Individual views
    let a = client.get_claimable_payout(&inv_a);
    let b = client.get_claimable_payout(&inv_b);
    let c = client.get_claimable_payout(&inv_c);

    let mut q = SorobanVec::new(&env);
    q.push_back(inv_a.clone());
    q.push_back(inv_b.clone());
    q.push_back(inv_c.clone());

    let res = client.get_claimable_payouts(q);
    assert_eq!(res.len(), 3);
    assert_eq!(res.get(0).unwrap(), a);
    assert_eq!(res.get(1).unwrap(), b);
    assert_eq!(res.get(2).unwrap(), c);
}

#[test]
fn test_unknown_and_claimed_and_duplicate_and_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "BATCH002"),
        &sme,
        &300i128,
        &800i64,
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
    );

    let inv_x = Address::generate(&env);
    let inv_y = Address::generate(&env);
    let inv_z = Address::generate(&env);

    // Only fund X with full target so others are unknown
    token.stellar.mint(&inv_x, &300i128);
    client.fund(&inv_x, &300i128);
    client.settle();

    // Claim for inv_x to mark as claimed
    client.claim_investor_payout(&inv_x);
    assert!(client.is_investor_claimed(&inv_x));

    // Duplicate addresses and unknown address (inv_y)
    let mut v = SorobanVec::new(&env);
    v.push_back(inv_x.clone());
    v.push_back(inv_y.clone());
    v.push_back(inv_x.clone());

    let out = client.get_claimable_payouts(v);
    // inv_x is claimed -> zero, inv_y unknown -> zero, duplicate preserves zero
    assert_eq!(out.len(), 3);
    assert_eq!(out.get(0).unwrap(), 0i128);
    assert_eq!(out.get(1).unwrap(), 0i128);
    assert_eq!(out.get(2).unwrap(), 0i128);

    // Empty vector
    let empty: SorobanVec<Address> = SorobanVec::new(&env);
    let empty_out = client.get_claimable_payouts(empty);
    assert_eq!(empty_out.len(), 0);
}

#[test]
fn test_max_batch_and_exceeding_batch() {
    let env = Env::default();
    let (client, _admin, _sme) = setup(&env);

    // Build MAX_CLAIMABLE_PAYOUT_BATCH addresses (none funded)
    let mut v = SorobanVec::new(&env);
    for _ in 0..MAX_CLAIMABLE_PAYOUT_BATCH {
        v.push_back(Address::generate(&env));
    }
    let ok = client.get_claimable_payouts(v.clone());
    assert_eq!(ok.len() as u32, MAX_CLAIMABLE_PAYOUT_BATCH);

    // Exceeding batch should return typed error
    let mut v2 = SorobanVec::new(&env);
    for _ in 0..(MAX_CLAIMABLE_PAYOUT_BATCH + 1) {
        v2.push_back(Address::generate(&env));
    }
    let res = client.try_get_claimable_payouts(v2);
    super::assert_contract_error(res, EscrowError::ClaimablePayoutReadBatchTooLarge);
}

#[test]
fn test_mixed_batch_matches_individual_calls() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "BATCH003"),
        &sme,
        &300i128,
        &800i64,
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
    );

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);

    // Fund only A and B
    token.stellar.mint(&inv_a, &100i128);
    token.stellar.mint(&inv_b, &200i128);
    client.fund(&inv_a, &100i128);
    client.fund(&inv_b, &200i128);
    client.settle();

    // Claim B so it's zero
    client.claim_investor_payout(&inv_b);

    let mut v = SorobanVec::new(&env);
    v.push_back(inv_a.clone()); // known
    v.push_back(inv_c.clone()); // unknown
    v.push_back(inv_b.clone()); // claimed
    v.push_back(inv_a.clone()); // duplicate known

    let batch_out = client.get_claimable_payouts(v.clone());

    // Compare each position to single-call behavior
    for i in 0..v.len() {
        let addr = v.get(i).unwrap();
        let single = client.get_claimable_payout(&addr);
        assert_eq!(batch_out.get(i).unwrap(), single);
    }
}
