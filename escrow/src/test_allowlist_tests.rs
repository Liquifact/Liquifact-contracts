use super::{
    AllowlistEnabledChanged, DataKey, InvestorAllowlistChanged, LiquifactEscrow,
    LiquifactEscrowClient,
};
use soroban_sdk::Vec as SorobanVec;
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Event};

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn init(env: &Env, client: &LiquifactEscrowClient) -> (Address, Address) {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "ALINV001"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    (admin, sme)
}

// --- defaults ---

#[test]
fn test_allowlist_disabled_by_default() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    assert!(!client.is_allowlist_active());
}

#[test]
fn test_is_allowlisted_false_by_default() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let stranger = Address::generate(&env);
    assert!(!client.is_investor_allowlisted(&stranger));
}

// --- enable / disable ---

#[test]
fn test_enable_and_disable_allowlist() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let invoice_id = client.get_escrow().invoice_id;
    let contract_id = client.address.clone();

    client.set_allowlist_active(&true);
    let enabled_events = env.events().all();
    env.as_contract(&contract_id, || {
        assert!(
            env.storage()
                .instance()
                .get::<DataKey, bool>(&DataKey::AllowlistActive)
                == Some(true)
        );
    });

    client.set_allowlist_active(&false);
    let disabled_events = env.events().all();
    env.as_contract(&contract_id, || {
        assert!(
            env.storage()
                .instance()
                .get::<DataKey, bool>(&DataKey::AllowlistActive)
                == Some(false)
        );
    });

    assert_eq!(
        enabled_events,
        std::vec![AllowlistEnabledChanged {
            name: symbol_short!("al_ena"),
            invoice_id: invoice_id.clone(),
            active: 1,
        }
        .to_xdr(&env, &contract_id)]
    );
    assert_eq!(
        disabled_events,
        std::vec![AllowlistEnabledChanged {
            name: symbol_short!("al_ena"),
            invoice_id,
            active: 0,
        }
        .to_xdr(&env, &contract_id)]
    );
}

#[test]
#[should_panic]
fn test_enable_allowlist_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    env.mock_auths(&[]);
    client.set_allowlist_active(&true);
}

#[test]
#[should_panic]
fn test_disable_allowlist_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    client.set_allowlist_active(&true);
    env.mock_auths(&[]);
    client.set_allowlist_active(&false);
}

// --- add / remove ---

#[test]
fn test_add_and_remove_from_allowlist() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let invoice_id = client.get_escrow().invoice_id;
    let contract_id = client.address.clone();
    let investor = Address::generate(&env);

    client.set_investor_allowlisted(&investor, &true);
    let added_events = env.events().all();
    env.as_contract(&contract_id, || {
        assert!(
            env.storage()
                .persistent()
                .get::<DataKey, bool>(&DataKey::InvestorAllowlisted(investor.clone()))
                == Some(true)
        );
    });

    client.set_investor_allowlisted(&investor, &false);
    let removed_events = env.events().all();
    env.as_contract(&contract_id, || {
        assert!(
            env.storage()
                .persistent()
                .get::<DataKey, bool>(&DataKey::InvestorAllowlisted(investor.clone()))
                == Some(false)
        );
    });

    assert_eq!(
        added_events,
        std::vec![InvestorAllowlistChanged {
            name: symbol_short!("al_set"),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            allowed: 1,
        }
        .to_xdr(&env, &contract_id)]
    );
    assert_eq!(
        removed_events,
        std::vec![InvestorAllowlistChanged {
            name: symbol_short!("al_set"),
            invoice_id,
            investor: investor.clone(),
            allowed: 0,
        }
        .to_xdr(&env, &contract_id)]
    );
}

#[test]
#[should_panic]
fn test_add_to_allowlist_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let investor = Address::generate(&env);
    env.mock_auths(&[]);
    client.set_investor_allowlisted(&investor, &true);
}

#[test]
#[should_panic]
fn test_remove_from_allowlist_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let investor = Address::generate(&env);
    client.set_investor_allowlisted(&investor, &true);
    env.mock_auths(&[]);
    client.set_investor_allowlisted(&investor, &false);
}

#[test]
fn test_remove_non_existent_address_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let stranger = Address::generate(&env);
    // Should not panic.
    client.set_investor_allowlisted(&stranger, &false);
    assert!(!client.is_investor_allowlisted(&stranger));
}

// --- fund gating ---

#[test]
fn test_fund_allowed_when_allowlist_disabled() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let investor = Address::generate(&env);
    // Allowlist off — anyone can fund.
    let escrow = client.fund(&investor, &5_000i128);
    assert_eq!(escrow.funded_amount, 5_000i128);
}

#[test]
fn test_fund_with_commitment_allowed_when_allowlist_disabled() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let investor = Address::generate(&env);
    // Allowlist off — anyone can fund with commitment.
    let escrow = client.fund_with_commitment(&investor, &5_000i128, &0u64);
    assert_eq!(escrow.funded_amount, 5_000i128);
}

#[test]
fn test_fund_allowed_when_on_allowlist() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &true);

    let escrow = client.fund(&investor, &5_000i128);
    assert_eq!(escrow.funded_amount, 5_000i128);
}

#[test]
#[should_panic]
fn test_fund_blocked_when_not_on_allowlist() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true);
    client.fund(&investor, &1_000i128);
}

#[test]
#[should_panic]
fn test_fund_with_commitment_blocked_when_not_on_allowlist() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true);
    client.fund_with_commitment(&investor, &1_000i128, &0u64);
}

#[test]
fn test_fund_with_commitment_allowed_when_on_allowlist() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &true);

    let escrow = client.fund_with_commitment(&investor, &5_000i128, &0u64);
    assert_eq!(escrow.funded_amount, 5_000i128);
}

#[test]
fn test_fund_allowed_after_disable_even_without_entry() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true);
    client.set_allowlist_active(&false);

    // Gate is off — investor not in list but can still fund.
    let escrow = client.fund(&investor, &3_000i128);
    assert_eq!(escrow.funded_amount, 3_000i128);
}

#[test]
fn test_entries_persist_across_disable_reenable() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &true);
    client.set_allowlist_active(&false);
    // Entry still there even while disabled.
    assert!(client.is_investor_allowlisted(&investor));
    // Re-enable — investor can still fund without re-adding.
    client.set_allowlist_active(&true);
    let escrow = client.fund(&investor, &2_000i128);
    assert_eq!(escrow.funded_amount, 2_000i128);
}

#[test]
#[should_panic]
fn test_removed_investor_blocked_after_reenable() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &true);
    client.set_investor_allowlisted(&investor, &false);

    client.fund(&investor, &1_000i128);
}

#[test]
fn test_multiple_investors_independent_allowlist_entries() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);

    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&a, &true);
    client.set_investor_allowlisted(&b, &true);

    assert!(client.is_investor_allowlisted(&a));
    assert!(client.is_investor_allowlisted(&b));
    assert!(!client.is_investor_allowlisted(&c));

    client.fund(&a, &3_000i128);
    client.fund(&b, &3_000i128);

    let blocked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.fund(&c, &1_000i128);
    }));
    assert!(blocked.is_err());
}

#[test]
fn test_batch_add_and_remove_from_allowlist() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);

    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    v.push_back(a.clone());
    v.push_back(b.clone());
    v.push_back(c.clone());

    client.set_investors_allowlisted(&v, &true);

    assert!(client.is_investor_allowlisted(&a));
    assert!(client.is_investor_allowlisted(&b));
    assert!(client.is_investor_allowlisted(&c));

    client.set_investors_allowlisted(&v, &false);

    assert!(!client.is_investor_allowlisted(&a));
    assert!(!client.is_investor_allowlisted(&b));
    assert!(!client.is_investor_allowlisted(&c));
}

#[test]
#[should_panic]
fn test_batch_rejects_empty_vector() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let v: SorobanVec<Address> = SorobanVec::new(&env);
    client.set_investors_allowlisted(&v, &true);
}

#[test]
#[should_panic]
fn test_batch_rejects_too_large_vector() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    let cap = super::MAX_INVESTOR_ALLOWLIST_BATCH as usize;
    for _ in 0..(cap + 1) {
        v.push_back(Address::generate(&env));
    }

    client.set_investors_allowlisted(&v, &true);
}

#[test]
#[should_panic]
fn test_batch_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let a = Address::generate(&env);
    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    v.push_back(a.clone());

    env.mock_auths(&[]);
    client.set_investors_allowlisted(&v, &true);
}
