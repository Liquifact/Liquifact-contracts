use super::{
    AllowlistEnabledChanged, DataKey, InvestorAllowlistBatchApplied, InvestorAllowlistChanged,
    LiquifactEscrow, LiquifactEscrowClient,
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
    // Allowlist off ÔÇö anyone can fund.
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
    // Allowlist off ÔÇö anyone can fund with commitment.
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

    // Gate is off ÔÇö investor not in list but can still fund.
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
    // Re-enable ÔÇö investor can still fund without re-adding.
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

// ---------------------------------------------------------------------------
// Batch event tests (Issue #379)
// ---------------------------------------------------------------------------

// --- existing al_set events still emitted correctly (under-target behavior) ---

#[test]
fn test_single_investor_set_still_emits_al_set_only() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;
    let investor = Address::generate(&env);

    client.set_investor_allowlisted(&investor, &true);
    let events = env.events().all();

    // Exactly one event, and it is al_set — not al_batch.
    assert_eq!(events.events().len(), 1);
    assert_eq!(
        events,
        std::vec![InvestorAllowlistChanged {
            name: symbol_short!("al_set"),
            invoice_id,
            investor,
            allowed: 1,
        }
        .to_xdr(&env, &contract_id)]
    );
}

// --- single-element batch ---

#[test]
fn test_single_element_batch_emits_one_al_set_and_one_al_batch() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;
    let investor = Address::generate(&env);

    let mut v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    v.push_back(investor.clone());

    client.set_investors_allowlisted(&v, &true);
    let all = env.events().all();

    // 1 al_set + 1 al_batch = 2 events total.
    assert_eq!(
        all,
        std::vec![
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: investor.clone(),
                allowed: 1,
            }
            .to_xdr(&env, &contract_id),
            InvestorAllowlistBatchApplied {
                name: symbol_short!("al_batch"),
                invoice_id,
                batch_size: 1,
                allowed: 1,
            }
            .to_xdr(&env, &contract_id),
        ]
    );
}

// --- multi-address batch: N al_set events + exactly 1 al_batch ---

#[test]
fn test_multi_address_batch_emits_n_al_set_and_one_al_batch() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let mut v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    v.push_back(a.clone());
    v.push_back(b.clone());
    v.push_back(c.clone());

    client.set_investors_allowlisted(&v, &true);
    let all = env.events().all();

    // 3 al_set events + 1 al_batch = 4 events total.
    assert_eq!(
        all,
        std::vec![
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: a,
                allowed: 1,
            }
            .to_xdr(&env, &contract_id),
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: b,
                allowed: 1,
            }
            .to_xdr(&env, &contract_id),
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: c,
                allowed: 1,
            }
            .to_xdr(&env, &contract_id),
            InvestorAllowlistBatchApplied {
                name: symbol_short!("al_batch"),
                invoice_id,
                batch_size: 3,
                allowed: 1,
            }
            .to_xdr(&env, &contract_id),
        ]
    );
}

// --- batch metadata: invoice_id, batch_size, allowed ---

#[test]
fn test_batch_event_metadata_allow() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    let mut v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    v.push_back(Address::generate(&env));
    v.push_back(Address::generate(&env));

    client.set_investors_allowlisted(&v, &true);
    let all = env.events().all();
    // Last event is the batch event — compare the full sequence.
    let a0 = v.get(0).unwrap();
    let a1 = v.get(1).unwrap();
    assert_eq!(
        all,
        std::vec![
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: a0,
                allowed: 1,
            }
            .to_xdr(&env, &contract_id),
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: a1,
                allowed: 1,
            }
            .to_xdr(&env, &contract_id),
            InvestorAllowlistBatchApplied {
                name: symbol_short!("al_batch"),
                invoice_id,
                batch_size: 2,
                allowed: 1,
            }
            .to_xdr(&env, &contract_id),
        ]
    );
}

#[test]
fn test_batch_event_metadata_disallow() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    let mut v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    v.push_back(Address::generate(&env));
    v.push_back(Address::generate(&env));
    v.push_back(Address::generate(&env));

    client.set_investors_allowlisted(&v, &false);
    let all = env.events().all();
    let a0 = v.get(0).unwrap();
    let a1 = v.get(1).unwrap();
    let a2 = v.get(2).unwrap();
    assert_eq!(
        all,
        std::vec![
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: a0,
                allowed: 0,
            }
            .to_xdr(&env, &contract_id),
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: a1,
                allowed: 0,
            }
            .to_xdr(&env, &contract_id),
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: a2,
                allowed: 0,
            }
            .to_xdr(&env, &contract_id),
            InvestorAllowlistBatchApplied {
                name: symbol_short!("al_batch"),
                invoice_id,
                batch_size: 3,
                allowed: 0,
            }
            .to_xdr(&env, &contract_id),
        ]
    );
}

// --- allow path ---

#[test]
fn test_batch_allow_flag_true() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    let investor = Address::generate(&env);
    let mut v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    v.push_back(investor.clone());

    client.set_investors_allowlisted(&v, &true);
    let all = env.events().all();

    assert_eq!(
        all,
        std::vec![
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: investor.clone(),
                allowed: 1,
            }
            .to_xdr(&env, &contract_id),
            InvestorAllowlistBatchApplied {
                name: symbol_short!("al_batch"),
                invoice_id,
                batch_size: 1,
                allowed: 1,
            }
            .to_xdr(&env, &contract_id),
        ]
    );
    assert!(client.is_investor_allowlisted(&investor));
}

// --- disallow path ---

#[test]
fn test_batch_allow_flag_false() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    let investor = Address::generate(&env);
    let mut v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    v.push_back(investor.clone());

    // First allow, then disallow via batch.
    client.set_investors_allowlisted(&v, &true);

    client.set_investors_allowlisted(&v, &false);
    let all = env.events().all();

    assert_eq!(
        all,
        std::vec![
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: investor.clone(),
                allowed: 0,
            }
            .to_xdr(&env, &contract_id),
            InvestorAllowlistBatchApplied {
                name: symbol_short!("al_batch"),
                invoice_id,
                batch_size: 1,
                allowed: 0,
            }
            .to_xdr(&env, &contract_id),
        ]
    );
    assert!(!client.is_investor_allowlisted(&investor));
}

// --- large batch ---

#[test]
fn test_large_batch_per_investor_events_and_one_batch_event() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    let cap = super::MAX_INVESTOR_ALLOWLIST_BATCH as usize;
    let mut v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..cap {
        v.push_back(Address::generate(&env));
    }

    client.set_investors_allowlisted(&v, &true);
    let all = env.events().all();

    // cap al_set events + 1 al_batch event.
    let expected_total = cap + 1;
    assert_eq!(all.events().len(), expected_total);

    // Build expected event list: cap al_set events followed by one al_batch.
    let mut expected: std::vec::Vec<soroban_sdk::xdr::ContractEvent> =
        std::vec::Vec::with_capacity(expected_total);
    for i in 0..cap {
        expected.push(
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: v.get(i as u32).unwrap(),
                allowed: 1,
            }
            .to_xdr(&env, &contract_id),
        );
    }
    expected.push(
        InvestorAllowlistBatchApplied {
            name: symbol_short!("al_batch"),
            invoice_id,
            batch_size: cap as u32,
            allowed: 1,
        }
        .to_xdr(&env, &contract_id),
    );

    assert_eq!(all, expected);
}

// --- invariant preservation ---

#[test]
fn test_batch_produces_same_per_investor_events_as_individual_calls() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();

    let a = Address::generate(&env);
    let b = Address::generate(&env);

    // --- single-call path ---
    let client1 = deploy(&env);
    init(&env, &client1);
    let contract_id1 = client1.address.clone();
    let invoice_id = client1.get_escrow().invoice_id;

    client1.set_investor_allowlisted(&a, &true);
    let single_a_events = env.events().all();

    client1.set_investor_allowlisted(&b, &true);
    let single_b_events = env.events().all();

    // --- batch path (new contract, same invoice id) ---
    let client2 = deploy(&env);
    let admin2 = Address::generate(&env);
    let sme2 = Address::generate(&env);
    let token2 = Address::generate(&env);
    let treasury2 = Address::generate(&env);
    client2.init(
        &admin2,
        &soroban_sdk::String::from_str(&env, "ALINV001"),
        &sme2,
        &10_000i128,
        &800i64,
        &0u64,
        &token2,
        &None,
        &treasury2,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    let contract_id2 = client2.address.clone();

    let mut v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    v.push_back(a.clone());
    v.push_back(b.clone());
    client2.set_investors_allowlisted(&v, &true);
    let batch_events = env.events().all();

    // The per-investor events from the batch match the shape of single-call events.
    // Single-call path emits exactly the al_set event.
    assert_eq!(
        single_a_events,
        std::vec![InvestorAllowlistChanged {
            name: symbol_short!("al_set"),
            invoice_id: invoice_id.clone(),
            investor: a.clone(),
            allowed: 1,
        }
        .to_xdr(&env, &contract_id1)]
    );
    assert_eq!(
        single_b_events,
        std::vec![InvestorAllowlistChanged {
            name: symbol_short!("al_set"),
            invoice_id: invoice_id.clone(),
            investor: b.clone(),
            allowed: 1,
        }
        .to_xdr(&env, &contract_id1)]
    );

    // Batch path emits the same per-investor events (same structure, different contract id)
    // followed by one al_batch summary.
    assert_eq!(
        batch_events,
        std::vec![
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: a,
                allowed: 1,
            }
            .to_xdr(&env, &contract_id2),
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: b,
                allowed: 1,
            }
            .to_xdr(&env, &contract_id2),
            InvestorAllowlistBatchApplied {
                name: symbol_short!("al_batch"),
                invoice_id,
                batch_size: 2,
                allowed: 1,
            }
            .to_xdr(&env, &contract_id2),
        ]
    );
}
