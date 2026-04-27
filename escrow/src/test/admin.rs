use super::*;
use crate::{AdminTransferredEvent, FundingTargetUpdated};
use soroban_sdk::Event;

// Admin/governance operations: target changes, maturity changes, admin transfer,
// legal hold, migration guards, and collateral metadata.

#[test]
fn test_update_maturity_success() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV006b"),
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
        &soroban_sdk::String::from_str(&env, "INV007"),
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
        &soroban_sdk::String::from_str(&env, "INV009"),
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
        &soroban_sdk::String::from_str(&env, "T001"),
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
        &soroban_sdk::String::from_str(&env, "T002"),
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

// --- AdminTransferredEvent validation tests ---

/// Verify that `transfer_admin` emits an `AdminTransferredEvent` with correct
/// `old_admin` and `new_admin` fields for indexer consumption.
#[test]
fn test_transfer_admin_event_emitted_with_correct_payload() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let contract_id = client.address.clone();
    let new_admin = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "EVT_ADMIN_001"),
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

    client.transfer_admin(&new_admin);

    let all_events = env.events().all();
    let events = all_events.events();
    assert_eq!(events.len(), 1, "Expected exactly one event");

    let expected_event = AdminTransferredEvent {
        name: symbol_short!("admin"),
        invoice_id: client.get_escrow().invoice_id,
        old_admin: admin,
        new_admin: new_admin.clone(),
    };

    let actual_event_xdr = events.get(0).unwrap();
    let expected_event_xdr = expected_event.to_xdr(&env, &contract_id);

    assert_eq!(
        *actual_event_xdr, expected_event_xdr,
        "Event payload must match expected old_admin and new_admin"
    );
}

/// Verify no event is emitted when transfer fails due to unauthorized caller.
#[test]
fn test_transfer_admin_unauthorized_no_event_emitted() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let _unauthorized = Address::generate(&env);
    let new_admin = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "EVT_ADMIN_002"),
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

    // Capture event count before failed transfer
    let events_before = env.events().all().events().len();

    // Attempt unauthorized transfer (should panic)
    env.mock_auths(&[]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.transfer_admin(&new_admin);
    }));

    assert!(result.is_err(), "Unauthorized transfer should panic");

    // Verify no new events were emitted
    let events_after = env.events().all().events().len();
    assert_eq!(
        events_before, events_after,
        "No event should be emitted on unauthorized transfer"
    );
}

/// Verify no event is emitted when transfer fails due to no-op (same admin).
#[test]
fn test_transfer_admin_no_op_no_event_emitted() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "EVT_ADMIN_003"),
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

    // Capture event count before no-op transfer
    let events_before = env.events().all().events().len();

    // Attempt no-op transfer (should panic)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.transfer_admin(&admin);
    }));

    assert!(result.is_err(), "No-op transfer should panic");

    // Verify no new events were emitted
    let events_after = env.events().all().events().len();
    assert_eq!(
        events_before, events_after,
        "No event should be emitted on no-op transfer"
    );
}

// --- Sequential transfer tests ---

/// Verify multiple sequential transfers work correctly with consistent events.
#[test]
fn test_transfer_admin_sequential_transfers() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let contract_id = client.address.clone();

    let admin_2 = Address::generate(&env);
    let admin_3 = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SEQ_ADMIN_001"),
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

    // First transfer: admin -> admin_2
    client.transfer_admin(&admin_2);
    assert_eq!(client.get_escrow().admin, admin_2);

    // Second transfer: admin_2 -> admin_3
    client.transfer_admin(&admin_3);
    assert_eq!(client.get_escrow().admin, admin_3);

    // Verify events were emitted for both transfers
    let all_events = env.events().all();
    let events = all_events.events();
    assert_eq!(
        events.len(),
        2,
        "Expected exactly 2 AdminTransferredEvent events"
    );

    // Verify first event payload
    let event_1 = AdminTransferredEvent {
        name: symbol_short!("admin"),
        invoice_id: client.get_escrow().invoice_id,
        old_admin: admin.clone(),
        new_admin: admin_2.clone(),
    };
    assert_eq!(
        *events.get(0).unwrap(),
        event_1.to_xdr(&env, &contract_id),
        "First event must have admin -> admin_2"
    );

    // Verify second event payload
    let event_2 = AdminTransferredEvent {
        name: symbol_short!("admin"),
        invoice_id: client.get_escrow().invoice_id,
        old_admin: admin_2,
        new_admin: admin_3.clone(),
    };
    assert_eq!(
        *events.get(1).unwrap(),
        event_2.to_xdr(&env, &contract_id),
        "Second event must have admin_2 -> admin_3"
    );
}

/// Verify latest admin is always enforced after multiple transfers.
#[test]
fn test_transfer_admin_latest_admin_enforced() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    let admin_2 = Address::generate(&env);
    let admin_3 = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SEQ_ADMIN_002"),
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

    // Transfer to admin_2
    client.transfer_admin(&admin_2);
    assert_eq!(client.get_escrow().admin, admin_2);

    // Transfer to admin_3
    client.transfer_admin(&admin_3);
    assert_eq!(client.get_escrow().admin, admin_3);

    // Verify old admin (admin_2) cannot transfer anymore
    env.mock_auths(&[]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.transfer_admin(&admin);
    }));
    assert!(
        result.is_err(),
        "Previous admin should not be able to transfer after rotation"
    );

    // Verify only current admin (admin_3) can transfer
    let admin_4 = Address::generate(&env);
    client.transfer_admin(&admin_4);
    assert_eq!(client.get_escrow().admin, admin_4);
}

// --- Authorization enforcement tests ---

/// Verify only current admin can transfer (unauthorized caller fails).
#[test]
#[should_panic]
fn test_transfer_admin_unauthorized_caller_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let unauthorized = Address::generate(&env);
    let new_admin = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "AUTH_ADMIN_001"),
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

    // Attempt transfer with unauthorized caller
    env.mock_auths(&[]);
    client.transfer_admin(&new_admin);
}

/// Verify state remains unchanged after failed transfer.
#[test]
fn test_transfer_admin_state_unchanged_on_failure() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let _unauthorized = Address::generate(&env);
    let new_admin = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "STATE_ADMIN_001"),
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

    // Capture initial state
    let initial_escrow = client.get_escrow();
    assert_eq!(initial_escrow.admin, admin);

    // Attempt unauthorized transfer (should panic)
    env.mock_auths(&[]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.transfer_admin(&new_admin);
    }));
    assert!(result.is_err(), "Unauthorized transfer should panic");

    // Verify state is unchanged
    let final_escrow = client.get_escrow();
    assert_eq!(
        final_escrow.admin, admin,
        "Admin should remain unchanged after failed transfer"
    );
    assert_eq!(
        final_escrow.invoice_id, initial_escrow.invoice_id,
        "Other state fields should remain unchanged"
    );
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
#[should_panic]
fn test_migrate_no_path_branch() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    // Simulate an older version 4 already in storage.
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Version, &4u32);
    });
    // migrate(4) should hit the "No migration path" branch.
    client.migrate(&4u32);
}

#[test]
#[should_panic]
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
        &soroban_sdk::String::from_str(&env, "COL001"),
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
        &soroban_sdk::String::from_str(&env, "COL002"),
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
        &soroban_sdk::String::from_str(&env, "COL003"),
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
        &soroban_sdk::String::from_str(&env, "LH001"),
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
        &soroban_sdk::String::from_str(&env, "LH002"),
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
        &soroban_sdk::String::from_str(&env, "INV001"),
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
        &soroban_sdk::String::from_str(&env, "INV001"),
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
        &soroban_sdk::String::from_str(&env, "INV001"),
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
        &soroban_sdk::String::from_str(&env, "INV001"),
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
        &soroban_sdk::String::from_str(&env, "INV001"),
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
        &soroban_sdk::String::from_str(&env, "EVT001"),
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
        &soroban_sdk::String::from_str(&env, "SETL001"),
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
        &soroban_sdk::String::from_str(&env, "WD001"),
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
        &soroban_sdk::String::from_str(&env, "BOUND001"),
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
        &soroban_sdk::String::from_str(&env, "NEG001"),
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
