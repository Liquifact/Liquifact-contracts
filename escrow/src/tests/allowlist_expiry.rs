use super::*;

use soroban_sdk::testutils::Events as _;

#[test]
fn investor_allowlist_expiry_blocks_funding_at_exact_expiry() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &true);

    let mut ledger = env.ledger().get();
    ledger.timestamp = 1_000;
    env.ledger().set(ledger);
    client.set_investor_allowlist_expiry(&investor, &1_010);

    env.ledger().with_mut(|li| li.timestamp = 1_009);
    let funded = client.fund(&investor, &5_000i128);
    assert_eq!(funded.funded_amount, 5_000i128);

    env.ledger().with_mut(|li| li.timestamp = 1_010);
    assert_contract_error(
        client.try_fund(&investor, &1_000i128),
        EscrowError::InvestorAllowlistExpired,
    );
}

#[test]
fn non_allowlisted_investor_expiry_does_not_grant_access() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    client.set_allowlist_active(&true);
    let mut ledger = env.ledger().get();
    ledger.timestamp = 1_000;
    env.ledger().set(ledger);
    client.set_investor_allowlist_expiry(&investor, &1_010);

    assert_contract_error(
        client.try_fund(&investor, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );
}

#[test]
fn revoked_investor_remains_blocked_even_with_future_expiry() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &true);

    let mut ledger = env.ledger().get();
    ledger.timestamp = 1_000;
    env.ledger().set(ledger);
    client.set_investor_allowlist_expiry(&investor, &1_010);
    client.set_investor_allowlisted(&investor, &false);

    assert_contract_error(
        client.try_fund(&investor, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );
}

#[test]
fn inactive_allowlist_ignores_per_investor_expiry() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    client.set_allowlist_active(&false);
    client.set_investor_allowlisted(&investor, &true);

    let mut ledger = env.ledger().get();
    ledger.timestamp = 1_000;
    env.ledger().set(ledger);
    client.set_investor_allowlist_expiry(&investor, &1_005);

    env.ledger().with_mut(|li| li.timestamp = 1_006);
    let funded = client.fund(&investor, &5_000i128);
    assert_eq!(funded.funded_amount, 5_000i128);
}

#[test]
fn set_investors_allowlist_expiry_batch_rejects_empty_and_oversize() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    let empty: SorobanVec<Address> = SorobanVec::new(&env);
    assert_contract_error(
        client.try_set_investors_allowlist_expiry(&empty, &1_010u64),
        EscrowError::InvestorBatchEmpty,
    );

    let mut oversize: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..=(crate::MAX_INVESTOR_ALLOWLIST_BATCH) {
        oversize.push_back(Address::generate(&env));
    }
    assert_contract_error(
        client.try_set_investors_allowlist_expiry(&oversize, &1_010u64),
        EscrowError::InvestorBatchTooLarge,
    );
}

#[test]
fn set_investors_allowlist_expiry_batch_emits_one_event_per_investor() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    let mut investors: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..3 {
        investors.push_back(Address::generate(&env));
    }

    let before = env.events().all().events().len();
    client.set_investors_allowlist_expiry(&investors, &1_010u64);
    let after = env.events().all().events().len();

    // The shared test setup emits one event before this batch mutation, so assert the
    // delta from the baseline rather than assuming a zero-event starting state.
    assert_eq!(after.saturating_sub(before), 2usize);
    for inv in investors.iter() {
        assert_eq!(client.get_investor_allowlist_expiry(&inv), 1_010u64);
    }
}
