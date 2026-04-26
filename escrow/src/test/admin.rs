use super::*;
use crate::FundingTargetUpdated;

// Admin/governance operations: target changes, maturity changes, admin transfer,
// legal hold, migration guards, and collateral metadata.

#[test]
fn test_update_maturity_success() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV006b"),
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
    let updated = client.update_maturity(&2000u64);
    assert_eq!(updated.maturity, 2000u64);
    assert_eq!(updated.status, 0);
}

#[test]
#[should_panic(expected = "Maturity can only be updated in Open state")]
fn test_update_maturity_wrong_state() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV007"),
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
    client.update_maturity(&2000u64);
}

#[test]
#[should_panic]
fn test_update_maturity_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV009"),
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
    env.mock_auths(&[]);
    client.update_maturity(&2000u64);
}

#[test]
fn test_transfer_admin_updates_admin() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let new_admin = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "T001"),
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
    let updated = client.transfer_admin(&new_admin);
    assert_eq!(updated.admin, new_admin);
    assert_eq!(client.get_escrow().admin, new_admin);
}

#[test]
#[should_panic(expected = "New admin must differ from current admin")]
fn test_transfer_admin_same_address_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "T002"),
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
    client.transfer_admin(&admin);
}

#[test]
#[should_panic(expected = "Escrow not initialized")]
fn test_transfer_admin_uninitialized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin);
}

#[test]
#[should_panic(expected = "Already at current schema version")]
fn test_migrate_at_current_version_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.migrate(&SCHEMA_VERSION);
}

#[test]
#[should_panic(expected = "from_version does not match stored version")]
fn test_migrate_wrong_from_version_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.migrate(&99u32);
}

#[test]
#[should_panic(expected = "No migration path from version 4 — extend migrate or redeploy")]
fn test_migrate_no_path_branch() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    // Simulate an older version 4 already in storage.
    env.storage().instance().set(&DataKey::Version, &4u32);
    // migrate(4) should hit the "No migration path" branch.
    client.migrate(&4u32);
}

#[test]
#[should_panic(expected = "No migration path from version 0 — extend migrate or redeploy")]
fn test_migrate_from_zero_uninitialized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    // Uninitialized storage returns version 0; migrate(0) hits the no-path branch.
    client.migrate(&0u32);
}

#[test]
fn test_record_collateral_stored_and_does_not_block_settle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "COL001"),
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
    let c = client.record_sme_collateral_commitment(&symbol_short!("USDC"), &5000i128);
    assert_eq!(c.amount, 5000i128);
    assert_eq!(c.asset, symbol_short!("USDC"));
    assert_eq!(client.get_sme_collateral_commitment(), Some(c));

    client.fund(&investor, &TARGET);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

#[test]
#[should_panic(expected = "Collateral amount must be positive")]
fn test_collateral_zero_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "COL002"),
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
    client.record_sme_collateral_commitment(&symbol_short!("XLM"), &0i128);
}

#[test]
#[should_panic]
fn test_collateral_requires_sme_auth() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "COL003"),
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
    env.mock_auths(&[]);
    client.record_sme_collateral_commitment(&symbol_short!("XLM"), &100i128);
}

#[test]
fn test_legal_hold_blocks_settle_withdraw_claim_and_fund() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "LH001"),
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
    client.set_legal_hold(&true);
    assert!(client.get_legal_hold());

    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.settle();
    }))
    .is_err());

    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.withdraw();
    }))
    .is_err());

    client.clear_legal_hold();
    assert!(!client.get_legal_hold());
    let settled = client.settle();
    assert_eq!(settled.status, 2);

    client.set_legal_hold(&true);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.claim_investor_payout(&investor);
    }))
    .is_err());

    client.clear_legal_hold();
    client.claim_investor_payout(&investor);
    assert!(client.is_investor_claimed(&investor));
}

#[test]
#[should_panic(expected = "Legal hold blocks new funding while active")]
fn test_legal_hold_blocks_new_funds_when_open() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "LH002"),
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
    client.set_legal_hold(&true);
    client.fund(&investor, &1i128);
}

/// Soroban instance storage returns `None` for a key that has never been written.
/// `legal_hold_active` maps that `None` to `false` via `unwrap_or(false)`, so a
/// fresh deploy must read `false` without any explicit `set_legal_hold` call.
#[test]
fn test_get_legal_hold_defaults_false_on_fresh_deploy() {
    let env = Env::default();
    // No init, no set_legal_hold – DataKey::LegalHold is absent from storage.
    let client = deploy(&env);
    assert!(!client.get_legal_hold());
}

#[test]
fn test_update_funding_target_by_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV001"),
        &sme,
        &5_000i128,
        &800i64,
        &3000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    let updated = client.update_funding_target(&10_000i128);
    assert_eq!(updated.funding_target, 10_000i128);
    assert_eq!(updated.status, 0);
}

#[test]
#[should_panic]
fn test_update_funding_target_by_non_admin_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV001"),
        &sme,
        &5_000i128,
        &800i64,
        &3000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    env.mock_auths(&[]);
    client.update_funding_target(&10_000i128);
}

#[test]
#[should_panic(expected = "Target can only be updated in Open state")]
fn test_update_funding_target_fails_when_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV001"),
        &sme,
        &5_000i128,
        &800i64,
        &3000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &5_000i128);
    client.update_funding_target(&10_000i128);
}

#[test]
#[should_panic(expected = "Target cannot be less than already funded amount")]
fn test_update_funding_target_below_funded_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV001"),
        &sme,
        &10_000i128,
        &800i64,
        &3000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &4_000i128);
    client.update_funding_target(&3_000i128);
}

#[test]
#[should_panic(expected = "Target must be strictly positive")]
fn test_update_funding_target_zero_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV001"),
        &sme,
        &5_000i128,
        &800i64,
        &3000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    client.update_funding_target(&0i128);
}

// --- FundingTargetUpdated event and rejection coverage ---

/// Verify that `update_funding_target` emits a `FundingTargetUpdated` event whose
/// topic is `symbol_short!("fund_tgt")` and whose data fields carry the correct
/// `invoice_id`, `old_target`, and `new_target` values.
#[test]
fn test_update_funding_target_event_fields() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);
    let contract_id = client.address.clone();

    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "EVT001"),
        &sme,
        &5_000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    client.update_funding_target(&9_000i128);

    assert_eq!(
        env.events().all(),
        std::vec![FundingTargetUpdated {
            name: symbol_short!("fund_tgt"),
            invoice_id: client.get_escrow().invoice_id,
            old_target: 5_000i128,
            new_target: 9_000i128,
        }
        .to_xdr(&env, &contract_id)]
    );
}

/// `update_funding_target` must be rejected when the escrow is in the **settled**
/// state (status == 2); only the open state (0) is permitted.
#[test]
#[should_panic(expected = "Target can only be updated in Open state")]
fn test_update_funding_target_fails_when_settled() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "SETL001"),
        &sme,
        &5_000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &5_000i128); // status → 1 (funded)
    client.settle(); // status → 2 (settled)
    client.update_funding_target(&6_000i128);
}

/// `update_funding_target` must be rejected when the escrow is in the **withdrawn**
/// state (status == 3); only the open state (0) is permitted.
#[test]
#[should_panic(expected = "Target can only be updated in Open state")]
fn test_update_funding_target_fails_when_withdrawn() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "WD001"),
        &sme,
        &5_000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &5_000i128); // status → 1 (funded)
    client.withdraw(); // status → 3 (withdrawn)
    client.update_funding_target(&6_000i128);
}

/// Setting the new target exactly equal to `funded_amount` is the boundary case
/// that must succeed: the invariant is `new_target >= funded_amount`, so equality
/// is allowed.
#[test]
fn test_update_funding_target_equal_to_funded_amount_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "BOUND001"),
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
    );
    client.fund(&investor, &4_000i128); // funded_amount == 4_000, status still 0

    // new_target == funded_amount: boundary — must not panic.
    let updated = client.update_funding_target(&4_000i128);
    assert_eq!(updated.funding_target, 4_000i128);
    assert_eq!(updated.funded_amount, 4_000i128);
    assert_eq!(updated.status, 0);
}

/// Passing a negative value must panic with "Target must be strictly positive".
#[test]
#[should_panic(expected = "Target must be strictly positive")]
fn test_update_funding_target_negative_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "NEG001"),
        &sme,
        &5_000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
    client.update_funding_target(&-1i128);
}
