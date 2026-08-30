use super::*;
use crate::{
    AdminProposalCancelled, AdminProposedEvent, EscrowCloseSnapshot, FundingTargetUpdated,
};
use soroban_sdk::Event;

// Admin/governance operations: target changes, maturity changes, admin handover,
// legal hold, migration guards, and collateral metadata.

#[test]
fn test_update_maturity_emits_event() {
    use soroban_sdk::testutils::Events as _;
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let contract_id = client.address.clone();
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MAT_EVT"),
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
        &None,
        &None,
        &None,
    );
    client.update_maturity(&2000u64);
    assert_eq!(
        env.events().all().events().last().unwrap().clone(),
        crate::MaturityUpdatedEvent {
            name: symbol_short!("maturity"),
            invoice_id: client.get_escrow().invoice_id,
            old_maturity: 1000u64,
            new_maturity: 2000u64,
        }
        .to_xdr(&env, &contract_id)
    );
}

#[test]
#[should_panic]
fn test_update_maturity_unchanged_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV006c"),
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
        &None,
        &None,
        &None,
    );
    client.update_maturity(&2000u64);
}

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
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    let updated = client.update_maturity(&2000u64);
    assert_eq!(updated.maturity, 2000u64);
    assert_eq!(updated.status, 0);
}

#[test]
#[should_panic]
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
        &None,
        &None,
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    env.mock_auths(&[]);
    client.update_maturity(&2000u64);
}

#[test]
fn test_propose_admin_sets_pending_without_changing_admin() {
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    let pending = client.propose_admin(&new_admin);
    assert_eq!(pending, new_admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin));
    assert_eq!(client.get_escrow().admin, admin);
}

#[test]
fn test_accept_admin_promotes_pending_and_clears_pending() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let new_admin = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TACPT1"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.propose_admin(&new_admin);
    let updated = client.accept_admin();
    assert_eq!(updated.admin, new_admin);
    assert_eq!(client.get_escrow().admin, new_admin);
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
#[allow(deprecated)]
fn test_transfer_admin_deprecated_shim_only_proposes() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let new_admin = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TSHIM1"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let unchanged = client.transfer_admin(&new_admin);
    assert_eq!(unchanged.admin, admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin));
}

// --- Deprecated transfer_admin shim observability (issue #386) ---
//
// `transfer_admin` is a `#[deprecated]` shim that delegates to `propose_admin`.
// To make legacy one-step usage observable to indexers (and to drive the
// deprecation to completion), every successful `transfer_admin` call must
// publish **two** events in order: the existing `AdminProposedEvent` from the
// inner `propose_admin` delegation, followed by a dedicated
// `DeprecatedTransferAdminUsed` event. The canonical two-step entrypoint
// `propose_admin` must NOT emit `DeprecatedTransferAdminUsed`, so indexers
// can keep the two paths distinguishable.

/// `transfer_admin` must publish both events in this order:
/// `AdminProposedEvent` first (from the inner `propose_admin` delegation),
/// then `DeprecatedTransferAdminUsed`as the per-tx last event.
#[test]
#[allow(deprecated)]
fn test_transfer_admin_emits_proposal_and_deprecation_events_in_order() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let contract_id = client.address.clone();
    let new_admin = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    // Capture event count before the call so the assertion uses a delta and
    // stays robust against any future init-time event additions.
    let all_before = env.events().all();
    let events_before = all_before.events().len();

    client.transfer_admin(&new_admin);

    let all_events = env.events().all();
    let events = all_events.events();
    // Successful shim call publishes exactly 2 extra events: the inner
    // AdminProposedEvent plus the DeprecatedTransferAdminUsed.
    assert_eq!(
        events.len(),
        events_before + 2,
        "transfer_admin must publish AdminProposedEvent + DeprecatedTransferAdminUsed"
    );

    let proposal = AdminProposedEvent {
        name: symbol_short!("adm_prop"),
        invoice_id: client.get_escrow().invoice_id.clone(),
        current_admin: admin.clone(),
        pending_admin: new_admin.clone(),
    }
    .to_xdr(&env, &contract_id);
    assert_eq!(events.get(events_before).unwrap().clone(), proposal);

    let deprecation = crate::DeprecatedTransferAdminUsed {
        name: symbol_short!("depr_xfer"),
        invoice_id: client.get_escrow().invoice_id.clone(),
        proposed_address: new_admin.clone(),
    }
    .to_xdr(&env, &contract_id);
    assert_eq!(events.get(events_before + 1).unwrap().clone(), deprecation);
    // And the per-tx last event must be the deprecation event, not the proposal.
    assert_eq!(events.last().unwrap().clone(), deprecation);
}

/// `propose_admin` (the canonical two-step entrypoint) must NOT emit
/// `DeprecatedTransferAdminUsed` — that event is reserved for the
/// deprecated shim so indexers can distinguish the two paths.
#[test]
fn test_propose_admin_does_not_emit_deprecation_event() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let contract_id = client.address.clone();
    let new_admin = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    // Capture event count before the call so the assertion is delta-based.
    let all_before = env.events().all();
    let events_before = all_before.events().len();

    client.propose_admin(&new_admin);

    let all_events = env.events().all();
    let events = all_events.events();
    // propose_admin publishes exactly one extra event: its own AdminProposedEvent,
    // nothing else.
    assert_eq!(
        events.len(),
        events_before + 1,
        "propose_admin must publish only its own AdminProposedEvent"
    );

    // The single AdminProposedEvent should still match the canonical payload.
    let proposal = AdminProposedEvent {
        name: symbol_short!("adm_prop"),
        invoice_id: client.get_escrow().invoice_id.clone(),
        current_admin: admin.clone(),
        pending_admin: new_admin.clone(),
    }
    .to_xdr(&env, &contract_id);
    assert_eq!(events.last().unwrap().clone(), proposal);

    // Verify the deprecation event XDR is NOT in the recorded event list.
    let deprecation = crate::DeprecatedTransferAdminUsed {
        name: symbol_short!("depr_xfer"),
        invoice_id: client.get_escrow().invoice_id.clone(),
        proposed_address: new_admin.clone(),
    }
    .to_xdr(&env, &contract_id);
    assert!(
        !events.contains(&deprecation),
        "propose_admin must not emit DeprecatedTransferAdminUsed"
    );
}

/// The `proposed_address` carried by `DeprecatedTransferAdminUsed` must equal
/// the `new_admin` argument passed to `transfer_admin`, so indexers can
/// correlate the deprecation event with the `pending_admin` of the prior
/// `AdminProposedEvent` emitted in the same transaction.
#[test]
#[allow(deprecated)]
fn test_transfer_admin_deprecation_event_proposed_address_matches_call_arg() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let contract_id = client.address.clone();
    let new_admin = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    client.transfer_admin(&new_admin);

    let all_events = env.events().all();
    let events = all_events.events();
    assert_eq!(
        events.last().unwrap().clone(),
        crate::DeprecatedTransferAdminUsed {
            name: symbol_short!("depr_xfer"),
            invoice_id: client.get_escrow().invoice_id,
            proposed_address: new_admin,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// On the rejection path (`transfer_admin` called with the current admin),
/// `propose_admin` aborts with a typed error before any
/// `DeprecatedTransferAdminUsed` is published. Confirming no deprecation
/// event is emitted in the rejection path means failed calls cannot
/// pollute the deprecation-usage count.
#[test]
#[allow(deprecated)]
fn test_transfer_admin_does_not_emit_deprecation_event_on_rejection() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let contract_id = client.address.clone();
    default_init(&client, &env, &admin, &sme);

    // Capture event count before the rejected call so the assertion stays
    // robust against any future init-time event additions.
    let all_before = env.events().all();
    let events_before = all_before.events().len();

    // Same-address proposal: propose_admin aborts with `NewAdminSameAsCurrent`.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.transfer_admin(&admin);
    }));
    assert!(result.is_err(), "transfer_admin(current_admin) must reject");

    let all_events = env.events().all();
    let events = all_events.events();
    assert_eq!(
        events.len(),
        events_before,
        "rejected transfer_admin must publish no extra events"
    );

    let deprecation = crate::DeprecatedTransferAdminUsed {
        name: symbol_short!("depr_xfer"),
        invoice_id: client.get_escrow().invoice_id.clone(),
        proposed_address: admin.clone(),
    }
    .to_xdr(&env, &contract_id);
    assert!(
        !events.contains(&deprecation),
        "transfer_admin rejection must not emit DeprecatedTransferAdminUsed"
    );

    // And the AdminProposedEvent must not be present either (propose_admin
    // rejected the same-address proposal before reaching its publish call).
    let proposal = AdminProposedEvent {
        name: symbol_short!("adm_prop"),
        invoice_id: client.get_escrow().invoice_id.clone(),
        current_admin: admin.clone(),
        pending_admin: admin.clone(),
    }
    .to_xdr(&env, &contract_id);
    assert!(
        !events.contains(&proposal),
        "transfer_admin rejection must not emit AdminProposedEvent"
    );
}

#[test]
#[should_panic]
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    client.propose_admin(&admin);
}

#[test]
#[should_panic]
fn test_transfer_admin_uninitialized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let new_admin = Address::generate(&env);
    client.propose_admin(&new_admin);
}

#[test]
#[should_panic]
fn test_accept_admin_without_pending_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.accept_admin();
}

#[test]
#[should_panic]
fn test_accept_admin_requires_pending_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let new_admin = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    client.propose_admin(&new_admin);
    env.mock_auths(&[]);
    client.accept_admin();
}

#[test]
fn test_propose_admin_overwrites_prior_pending() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    client.propose_admin(&first);
    client.propose_admin(&second);

    assert_eq!(client.get_pending_admin(), Some(second.clone()));
    let updated = client.accept_admin();
    assert_eq!(updated.admin, second);
}

#[test]
fn test_propose_admin_emits_event() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let contract_id = client.address.clone();
    let new_admin = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    client.propose_admin(&new_admin);

    assert_eq!(
        env.events().all().events().last().unwrap().clone(),
        AdminProposedEvent {
            name: symbol_short!("adm_prop"),
            invoice_id: client.get_escrow().invoice_id,
            current_admin: admin,
            pending_admin: new_admin,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Assert `propose_admin` requires current-admin auth
#[test]
#[should_panic]
fn test_propose_admin_requires_current_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    let new_admin = Address::generate(&env);
    client.propose_admin(&new_admin);
}

/// Assert `propose_admin` rejects `NewAdminSameAsCurrent`
#[test]
#[should_panic]
fn test_propose_admin_same_address_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.propose_admin(&admin);
}

/// Assert `accept_admin` by wrong address panics
#[test]
#[should_panic]
fn test_accept_admin_by_wrong_address_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let new_admin = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    client.propose_admin(&new_admin);
    let wrong_admin = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &wrong_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.accept_admin();
}

/// End-to-end handover lifecycle: propose, accept, old admin lockout, new admin authority
#[test]
fn test_admin_handover_lifecycle() {
    use soroban_sdk::testutils::Events as _;
    let env = Env::default();
    env.mock_all_auths();
    let (client, old_admin, sme) = setup(&env);
    let new_admin = Address::generate(&env);
    default_init(&client, &env, &old_admin, &sme);

    // 1. Propose admin
    let pending = client.propose_admin(&new_admin);
    assert_eq!(pending, new_admin.clone());
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));

    // 2. Accept admin (verifying the events)
    let contract_id = client.address.clone();
    let updated = client.accept_admin();
    assert_eq!(updated.admin, new_admin.clone());
    assert_eq!(client.get_pending_admin(), None);

    // Verify AdminTransferredEvent
    assert_eq!(
        env.events().all().events().last().unwrap().clone(),
        crate::AdminTransferredEvent {
            name: symbol_short!("admin"),
            invoice_id: client.get_escrow().invoice_id,
            new_admin: new_admin.clone(),
        }
        .to_xdr(&env, &contract_id)
    );

    // 3. New admin can perform admin-gated actions
    let latest = client.update_funding_target(&20_000i128);
    assert_eq!(latest.funding_target, 20_000i128);

    // 4. Old admin can no longer perform admin-gated actions
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &old_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "update_funding_target",
            args: (30_000i128,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.update_funding_target(&30_000i128);
    }))
    .is_err());
}

#[test]
#[should_panic]
fn test_migrate_at_current_version_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.migrate(&SCHEMA_VERSION);
}

#[test]
#[should_panic]
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

// ── migrate() exhaustive typed-error contract tests ──────────────────────────
//
// migrate() is intentionally a no-op in the current release. Every path
// requires admin auth, validates the version, and terminates with one of three
// typed errors. These tests prove each branch fires correctly, that auth is
// checked before version reads, and that DataKey::Version is never mutated.
//
// See docs/OPERATOR_RUNBOOK.md §2 for the operator-side migration matrix.
// See escrow/src/lib.rs migrate() rustdoc for the per-error classification.

/// Unauthenticated callers must be rejected before any version check.
/// If auth were checked after the version guard, a mismatched `from_version`
/// could leak via `MigrationVersionMismatch` instead of the auth failure.
#[test]
fn test_migrate_rejects_non_admin_before_version_check() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    env.mock_auths(&[]);
    let result = client.try_migrate(&99u32);

    assert!(
        result.is_err(),
        "migrate should reject an unauthenticated call"
    );
    assert!(
        !matches!(
            result,
            Err(Err(soroban_sdk::InvokeError::Contract(code)))
                if code == EscrowError::MigrationVersionMismatch as u32
        ),
        "migrate must not reach version checks before admin auth (got MigrationVersionMismatch)"
    );
}

/// `migrate(SCHEMA_VERSION - 1)` after `init` (which stores
/// `SCHEMA_VERSION == 6`) must raise `MigrationVersionMismatch` because the
/// stored version (6) does not equal the claimed source version (5).
#[test]
fn test_migrate_version_mismatch_stored_neq_claimed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_contract_error(
        client.try_migrate(&(SCHEMA_VERSION - 1)),
        EscrowError::MigrationVersionMismatch,
    );
    assert_eq!(
        client.get_version(),
        SCHEMA_VERSION,
        "DataKey::Version must not change on MigrationVersionMismatch"
    );
}

/// Claiming a far-below `from_version` (0) against stored version 6 must
/// also raise `MigrationVersionMismatch`, not `NoMigrationPath`.
#[test]
fn test_migrate_far_below_stored_raises_mismatch() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_contract_error(
        client.try_migrate(&0u32),
        EscrowError::MigrationVersionMismatch,
    );
    assert_eq!(client.get_version(), SCHEMA_VERSION);
}

/// Calling `migrate` with `from_version == SCHEMA_VERSION` (boundary: the
/// contract is already at the latest schema) must raise
/// `AlreadyCurrentSchemaVersion`.
#[test]
fn test_migrate_at_schema_version_raises_already_current() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_contract_error(
        client.try_migrate(&SCHEMA_VERSION),
        EscrowError::AlreadyCurrentSchemaVersion,
    );
    assert_eq!(client.get_version(), SCHEMA_VERSION);
}

/// Any `from_version > SCHEMA_VERSION` claims a schema newer than the
/// contract knows about; this also maps to
/// `AlreadyCurrentSchemaVersion`.
#[test]
fn test_migrate_above_schema_version_raises_already_current() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    assert_contract_error(
        client.try_migrate(&(SCHEMA_VERSION + 1)),
        EscrowError::AlreadyCurrentSchemaVersion,
    );
    assert_eq!(client.get_version(), SCHEMA_VERSION);
}

/// When the stored version is below `SCHEMA_VERSION` and matches the claimed
/// `from_version`, the contract reaches the terminal `NoMigrationPath` branch.
#[test]
fn test_migrate_below_schema_version_matching_stored_raises_no_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    let older = SCHEMA_VERSION - 1;
    env.as_contract(&client.address, || {
        env.storage().instance().set(&DataKey::Version, &older);
    });

    assert_contract_error(client.try_migrate(&older), EscrowError::NoMigrationPath);
    assert_eq!(
        client.get_version(),
        older,
        "DataKey::Version must not change on NoMigrationPath"
    );
}

/// Every `from_version` in `[1, SCHEMA_VERSION - 1]` with a matching stored
/// version must raise `NoMigrationPath` — exhaustive coverage of the
/// "no implemented path" branch for all known historical versions.
#[test]
fn test_migrate_all_historical_versions_raise_no_path() {
    let env = Env::default();
    env.mock_all_auths();

    for &historical in &[1u32, 2, 3, 4, 5] {
        let (client, admin, sme) = setup(&env);
        default_init(&client, &env, &admin, &sme);
        env.as_contract(&client.address, || {
            env.storage().instance().set(&DataKey::Version, &historical);
        });

        assert_contract_error(
            client.try_migrate(&historical),
            EscrowError::NoMigrationPath,
        );
        assert_eq!(client.get_version(), historical);
    }
}

/// An uninitialized contract has `DataKey::Version` absent from storage,
/// which `.get(...).unwrap_or(0)` maps to `0`. Calling `migrate(0)` must
/// raise `NoMigrationPath`, not panic or silently succeed.
#[test]
fn test_migrate_from_zero_uninitialized_raises_no_path() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    assert_contract_error(client.try_migrate(&0u32), EscrowError::NoMigrationPath);

    let stored_after: u32 = env.as_contract(&client.address, || {
        env.storage().instance().get(&DataKey::Version).unwrap_or(0)
    });
    assert_eq!(
        stored_after, 0,
        "DataKey::Version must remain 0 (absent) on NoMigrationPath"
    );
}

/// Cross-branch immutability sweep: for representative values in every
/// error branch, confirm `DataKey::Version` is unchanged after the call.
#[test]
fn test_migrate_version_immutable_across_all_error_branches() {
    let env = Env::default();
    env.mock_all_auths();

    let cases: &[(u32, u32, EscrowError)] = &[
        (6, 5, EscrowError::MigrationVersionMismatch),
        (6, 6, EscrowError::AlreadyCurrentSchemaVersion),
        (6, 7, EscrowError::AlreadyCurrentSchemaVersion),
        (5, 5, EscrowError::NoMigrationPath),
        (0, 0, EscrowError::NoMigrationPath),
    ];

    for &(stored, claimed, expected) in cases {
        let (client, admin, sme) = setup(&env);

        if stored == 0 {
            // Uninitialized: just deploy; do not call init.
        } else {
            default_init(&client, &env, &admin, &sme);
            if stored != SCHEMA_VERSION {
                env.as_contract(&client.address, || {
                    env.storage().instance().set(&DataKey::Version, &stored);
                });
            }
        }

        let result = client.try_migrate(&claimed);
        assert_contract_error(result, expected);

        let actual_stored: u32 = env.as_contract(&client.address, || {
            env.storage()
                .instance()
                .get(&DataKey::Version)
                .unwrap_or(stored)
        });
        assert_eq!(
            actual_stored, stored,
            "DataKey::Version changed for stored={stored}, claimed={claimed}"
        );
    }
}

#[test]
fn test_read_model_summary_includes_optional_admin_fields() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let funding_token = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TSUM01"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &funding_token,
        &None,
        &treasury,
        &None,
        &Some(100i128),
        &Some(7u32),
        &Some(10_000i128),
        &None,
        &None,
        &None,
    );

    let summary = client.get_escrow_summary();

    assert_eq!(summary.escrow, client.get_escrow());
    assert_eq!(summary.legal_hold, client.get_legal_hold());
    assert_eq!(summary.funding_close_snapshot, EscrowCloseSnapshot::None);
    assert_eq!(summary.unique_funder_count, 0);
    assert!(!summary.is_allowlist_active);
    assert_eq!(summary.schema_version, client.get_version());
    assert_eq!(client.get_max_per_investor_cap(), Some(10_000i128));
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
        &None,
        &None,
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
#[should_panic]
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
        &None,
        &None,
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
        &None,
        &None,
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
        &None,
        &None,
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
#[should_panic]
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
        &None,
        &None,
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
    // No init, no set_legal_hold ├ö├ç├┤ DataKey::LegalHold is absent from storage.
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
        &None,
        &None,
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    env.mock_auths(&[]);
    client.update_funding_target(&10_000i128);
}

#[test]
#[should_panic]
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &5_000i128);
    client.update_funding_target(&10_000i128);
}

#[test]
#[should_panic]
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &4_000i128);
    client.update_funding_target(&3_000i128);
}

#[test]
#[should_panic]
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
        &None,
        &None,
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
        &None,
        &None,
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
#[should_panic]
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &5_000i128); // status ├ö├Ñ├å 1 (funded)
    client.settle(); // status ├ö├Ñ├å 2 (settled)
    client.update_funding_target(&6_000i128);
}

/// `update_funding_target` must be rejected when the escrow is in the **withdrawn**
/// state (status == 3); only the open state (0) is permitted.
#[test]
#[should_panic]
fn test_update_funding_target_fails_when_withdrawn() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _escrow_id, _sme) = init_and_fund_with_real_token(&env, 5_000i128, "WD001");
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &4_000i128); // funded_amount == 4_000, status still 0

    // new_target == funded_amount: boundary ├ö├ç├Â must not panic.
    let updated = client.update_funding_target(&4_000i128);
    assert_eq!(updated.funding_target, 4_000i128);
    assert_eq!(updated.funded_amount, 4_000i128);
    assert_eq!(updated.status, 0);
}

/// Passing a negative value must panic with "Target must be strictly positive".
#[test]
#[should_panic]
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    client.update_funding_target(&-1i128);
}
// --- update_maturity: open-only, ledger time semantics, MaturityUpdatedEvent ---

/// `update_maturity` must emit a `MaturityUpdatedEvent` with the correct
/// topic (`symbol_short!("maturity")`), `invoice_id`, `old_maturity`, and
/// `new_maturity` fields. Ledger timestamps are validator-observed integers;
/// the contract stores and compares them as raw `u64` seconds.
#[test]
fn test_update_maturity_event_fields() {
    use crate::MaturityUpdatedEvent;
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
        &soroban_sdk::String::from_str(&env, "MAT001"),
        &sme,
        &5_000i128,
        &800i64,
        &1000u64,
        &token,
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
    );

    client.update_maturity(&2000u64);

    assert_eq!(
        env.events().all(),
        std::vec![MaturityUpdatedEvent {
            name: symbol_short!("maturity"),
            invoice_id: client.get_escrow().invoice_id,
            old_maturity: 1000u64,
            new_maturity: 2000u64,
        }
        .to_xdr(&env, &contract_id)]
    );
}

/// `update_maturity` must be rejected when the escrow is in the **funded**
/// state (status == 1); only Open (0) is permitted.
#[test]
#[should_panic]
fn test_update_maturity_fails_when_funded() {
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
        &soroban_sdk::String::from_str(&env, "MAT002"),
        &sme,
        &5_000i128,
        &800i64,
        &1000u64,
        &token,
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
    );
    client.fund(&investor, &5_000i128); // status ├ö├Ñ├å 1 (funded)
    client.update_maturity(&2000u64);
}

/// `update_maturity` must be rejected when the escrow is **settled**
/// (status == 2); only Open (0) is permitted.
#[test]
#[should_panic]
fn test_update_maturity_fails_when_settled() {
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
        &soroban_sdk::String::from_str(&env, "MAT003"),
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
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &5_000i128); // status ├ö├Ñ├å 1
    client.settle(); // status ├ö├Ñ├å 2
    client.update_maturity(&2000u64);
}

/// `update_maturity` must be rejected when the escrow is **withdrawn**
/// (status == 3); only Open (0) is permitted.
#[test]
#[should_panic]
fn test_update_maturity_fails_when_withdrawn() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _escrow_id, _sme) = init_and_fund_with_real_token(&env, 5_000i128, "MAT004");
    client.withdraw(); // status → 3
    client.update_maturity(&2000u64);
}

/// Setting maturity to zero is valid ├ö├ç├Â it means no maturity gate.
/// The contract must accept zero as new_maturity in Open state.
#[test]
fn test_update_maturity_to_zero_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MAT005"),
        &sme,
        &5_000i128,
        &800i64,
        &1000u64,
        &token,
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
    );
    let updated = client.update_maturity(&0u64);
    assert_eq!(updated.maturity, 0u64);
    assert_eq!(updated.status, 0);
}

/// Ledger time semantics: `settle` uses `env.ledger().timestamp()`
/// (validator-observed seconds). Settle must pass exactly at maturity ├ö├ç├Â
/// confirming the boundary is `now >= maturity` (inclusive).
#[test]
fn test_settle_passes_exactly_at_maturity_ledger_time() {
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
        &soroban_sdk::String::from_str(&env, "MAT006"),
        &sme,
        &5_000i128,
        &800i64,
        &5000u64,
        &token,
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
    );
    client.fund(&investor, &5_000i128);

    // Advance ledger to exactly maturity ├ö├ç├Â must succeed
    env.ledger().with_mut(|l| l.timestamp = 5000);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

/// Ledger time semantics: settle must panic one second before maturity ├ö├ç├Â
/// confirming the `>=` boundary strictly excludes values below maturity.
#[test]
#[should_panic]
fn test_settle_fails_one_second_before_maturity() {
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
        &soroban_sdk::String::from_str(&env, "MAT007"),
        &sme,
        &5_000i128,
        &800i64,
        &5000u64,
        &token,
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
    );
    client.fund(&investor, &5_000i128);

    // One second before maturity ├ö├ç├Â must reject
    env.ledger().with_mut(|l| l.timestamp = 4999);
    client.settle();
}

/// A second `update_maturity` call in the same Open state must overwrite
/// the previous value correctly ├ö├ç├Â storage is atomic per call.
#[test]
fn test_update_maturity_twice_overwrites() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MAT008"),
        &sme,
        &5_000i128,
        &800i64,
        &1000u64,
        &token,
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
    );

    client.update_maturity(&2000u64);
    let updated = client.update_maturity(&3000u64);
    assert_eq!(updated.maturity, 3000u64);
    assert_eq!(client.get_escrow().maturity, 3000u64);
}

#[test]
fn test_update_maturity_edge_cases_success() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MAT_EDGE"),
        &sme,
        &5_000i128,
        &800i64,
        &1000u64,
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

    let updated1 = client.update_maturity(&2000u64);
    assert_eq!(updated1.maturity, 2000u64);

    let updated2 = client.update_maturity(&500u64);
    assert_eq!(updated2.maturity, 500u64);
}

// ── Authorization guard ordering audit (issue #265) ───────────────────────────
//
// Negative tests: each guarded entrypoint must trap when `require_auth` fails
// (Soroban host aborts the transaction). Canonical ordering is documented in
// `docs/escrow-security-checklist.md` Ôö¼┬║6 and ADR-002.

/// Helper to initialize and fund the escrow for authorization audit tests.
///
/// Returns a tuple containing:
/// - The `LiquifactEscrowClient` instance.
/// - The admin `Address`.
/// - The SME `Address`.
/// - The funding investor `Address`.
/// - A newly generated pending admin `Address`.
fn auth_audit_init_funded(
    env: &Env,
) -> (
    LiquifactEscrowClient<'_>,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let investor = Address::generate(env);
    let client = deploy(env);
    default_init(&client, env, &admin, &sme);
    client.fund(&investor, &TARGET);
    (client, admin, sme, investor, Address::generate(env))
}

#[test]
#[should_panic]
fn auth_audit_propose_admin_requires_current_admin() {
    let env = Env::default();
    let (client, _, _, _, _) = auth_audit_init_funded(&env);
    let new_admin = Address::generate(&env);
    env.mock_auths(&[]);
    client.propose_admin(&new_admin);
}

#[test]
#[should_panic]
fn auth_audit_accept_admin_requires_pending_admin() {
    let env = Env::default();
    let (client, _, _, _, pending_admin) = auth_audit_init_funded(&env);
    client.propose_admin(&pending_admin);
    env.mock_auths(&[]);
    client.accept_admin();
}

#[test]
#[should_panic]
fn auth_audit_fund_requires_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    env.mock_auths(&[]);
    client.fund(&investor, &TARGET);
}

#[test]
#[should_panic]
fn auth_audit_fund_with_commitment_requires_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    env.mock_auths(&[]);
    client.fund_with_commitment(&investor, &TARGET, &0u64);
}

#[test]
#[should_panic]
fn auth_audit_settle_requires_sme() {
    let env = Env::default();
    let (client, _, _, _, _) = auth_audit_init_funded(&env);
    env.mock_auths(&[]);
    client.settle();
}

#[test]
#[should_panic]
fn auth_audit_withdraw_requires_sme() {
    let env = Env::default();
    let (client, _, _, _, _) = auth_audit_init_funded(&env);
    env.mock_auths(&[]);
    client.withdraw();
}

#[test]
#[should_panic]
fn auth_audit_claim_investor_payout_requires_investor() {
    let env = Env::default();
    let (client, _, _, investor, _) = auth_audit_init_funded(&env);
    client.settle();
    env.mock_auths(&[]);
    client.claim_investor_payout(&investor);
}

#[test]
#[should_panic]
fn auth_audit_set_legal_hold_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.set_legal_hold(&true);
}

#[test]
#[should_panic]
fn auth_audit_bind_primary_attestation_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.bind_primary_attestation_hash(&soroban_sdk::BytesN::from_array(&env, &[0u8; 32]));
}

#[test]
#[should_panic]
fn auth_audit_append_attestation_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.append_attestation_digest(&soroban_sdk::BytesN::from_array(&env, &[0u8; 32]));
}

#[test]
#[should_panic]
fn auth_audit_set_allowlist_active_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.set_allowlist_active(&true);
}

#[test]
#[should_panic]
fn auth_audit_sweep_terminal_dust_requires_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);
    let escrow_id = deploy_id(&env);
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "AUTHSW"),
        &sme,
        &TARGET,
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
    );
    client.fund(&investor, &TARGET);
    client.settle();
    token.stellar.mint(&escrow_id, &100i128);
    env.mock_auths(&[]);
    client.sweep_terminal_dust(&100i128);
}

// --- Additional Negative-Auth Audit Tests ---

#[test]
#[should_panic]
fn auth_audit_init_requires_admin() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);
    env.mock_auths(&[]);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "AUTHINT"),
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
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn auth_audit_cancel_funding_requires_admin() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths();
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.cancel_funding();
}

#[test]
#[should_panic]
fn auth_audit_refund_requires_investor() {
    let env = Env::default();
    let (client, _, _, investor, _) = auth_audit_init_funded(&env);
    env.mock_all_auths();
    client.cancel_funding();
    env.mock_auths(&[]);
    client.refund(&investor);
}

#[test]
#[should_panic]
fn auth_audit_set_investors_allowlisted_requires_admin() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths();
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.set_investors_allowlisted(&soroban_sdk::Vec::new(&env), &true);
}

#[test]
#[should_panic]
fn auth_audit_update_funding_target_requires_admin() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths();
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.update_funding_target(&100_000i128);
}

#[test]
#[should_panic]
fn auth_audit_lower_max_unique_investors_requires_admin() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths();
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.lower_max_unique_investors(&1u32);
}

#[test]
#[should_panic]
fn auth_audit_update_maturity_requires_admin() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths();
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.update_maturity(&5000u64);
}

#[test]
#[should_panic]
fn auth_audit_rotate_beneficiary_requires_auth() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths();
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.rotate_beneficiary(&Address::generate(&env));
}

#[test]
#[should_panic]
fn auth_audit_revoke_attestation_digest_requires_admin() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths();
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.revoke_attestation_digest(&0u32);
}

#[test]
#[should_panic]
fn auth_audit_clear_legal_hold_requires_admin() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths();
    default_init(&client, &env, &admin, &sme);
    client.set_legal_hold(&true);
    env.mock_auths(&[]);
    client.clear_legal_hold();
}

#[test]
#[should_panic]
fn auth_audit_request_clear_legal_hold_requires_admin() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths();
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.request_clear_legal_hold();
}

#[test]
#[should_panic]
fn auth_audit_record_sme_collateral_commitment_requires_sme() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths();
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.record_sme_collateral_commitment(&symbol_short!("USDC"), &1000i128);
}

#[test]
#[should_panic]
fn auth_audit_partial_settle_requires_auth() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths();
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]);
    client.partial_settle(&sme);
}

#[test]
#[should_panic]
fn auth_audit_fund_batch_requires_investor() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.mock_all_auths();
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    env.mock_auths(&[]);
    client.fund_batch(&soroban_sdk::vec![&env, (investor.clone(), TARGET)]);
}

#[test]
#[should_panic]
fn auth_audit_sweep_terminal_dust_wrong_signer() {
    // Edge case: treasury vs admin on sweep
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);
    let escrow_id = deploy_id(&env);
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "WRSW"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &token.id,
        &None,
        &treasury, // Valid treasury
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    client.fund(&investor, &TARGET);
    client.settle();

    // Simulate admin trying to sweep instead of treasury
    use soroban_sdk::testutils::MockAuth;
    use soroban_sdk::{IntoVal, Vec as SorobanVec};
    env.mock_auths(&[MockAuth {
        address: &admin, // wrong signer (admin instead of treasury)
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "sweep_terminal_dust",
            args: SorobanVec::from_array(&env, [(100i128,).into_val(&env)]),
            sub_invokes: &[],
        },
    }]);

    client.sweep_terminal_dust(&100i128); // Panics because caller != treasury
}

// --- rotate_beneficiary tests ---

#[test]
fn test_rotate_beneficiary_success_dual_auth() {
    use soroban_sdk::testutils::Events as _;
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let new_sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    let updated = client.rotate_beneficiary(&new_sme);
    assert_eq!(updated.sme_address, new_sme);
    assert_eq!(client.get_escrow().sme_address, new_sme);
}

/*
#[test]
#[should_panic]
fn test_rotate_beneficiary_only_sme_auth_fails() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    use soroban_sdk::IntoVal;
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let new_sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[MockAuth {
        address: &sme,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "rotate_beneficiary",
            args: (&new_sme,).into_val(&env),
            sub_invokes: &[],
        },
    }]); // Only SME auth
    client.rotate_beneficiary(&new_sme);
}

#[test]
#[should_panic]
fn test_rotate_beneficiary_only_admin_auth_fails() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    use soroban_sdk::IntoVal;
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let new_sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "rotate_beneficiary",
            args: (&new_sme,).into_val(&env),
            sub_invokes: &[],
        },
    }]); // Only admin auth
    client.rotate_beneficiary(&new_sme);
}
*/

#[test]
#[should_panic]
fn test_rotate_beneficiary_no_auth_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let new_sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    env.mock_auths(&[]); // No auth
    client.rotate_beneficiary(&new_sme);
}

#[test]
#[should_panic]
fn test_rotate_beneficiary_new_same_as_current_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.rotate_beneficiary(&sme);
}

#[test]
#[should_panic]
fn test_rotate_beneficiary_in_settled_state_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let new_sme = Address::generate(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    client.fund(&investor, &TARGET);
    client.settle(); // status 2
    client.rotate_beneficiary(&new_sme);
}

#[test]
#[should_panic]
fn test_rotate_beneficiary_in_withdrawn_state_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let new_sme = Address::generate(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    client.fund(&investor, &TARGET);
    client.withdraw(); // status 3
    client.rotate_beneficiary(&new_sme);
}

#[test]
#[should_panic]
fn test_rotate_beneficiary_in_cancelled_state_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let new_sme = Address::generate(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    client.fund(&investor, &TARGET);
    client.cancel_funding(); // status 4
    client.rotate_beneficiary(&new_sme);
}

#[test]
#[should_panic]
fn test_rotate_beneficiary_with_legal_hold_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let new_sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_legal_hold(&true);
    client.rotate_beneficiary(&new_sme);
}

#[test]
fn test_rotate_beneficiary_in_funded_state_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let new_sme = Address::generate(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    client.fund(&investor, &TARGET); // status 1
    let updated = client.rotate_beneficiary(&new_sme);
    assert_eq!(updated.sme_address, new_sme);
}

#[test]
fn test_rotate_beneficiary_then_withdraw_goes_to_new_sme() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let new_sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);
    let escrow_id = deploy_id(&env);
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "WDTST"),
        &sme,
        &TARGET,
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
    );
    token.stellar.mint(&investor, &TARGET);
    token
        .stellar
        .approve(&investor, &escrow_id, &TARGET, &9999u32);
    client.fund(&investor, &TARGET);
    // Mint funded_amount into the escrow contract so withdraw() can transfer it.
    token.stellar.mint(&escrow_id, &TARGET);
    client.rotate_beneficiary(&new_sme);
    client.withdraw();
    assert_eq!(token.stellar.balance(&new_sme), TARGET);
}

#[test]
fn test_error_code_uniqueness() {
    let mut discriminants = std::collections::HashSet::new();
    let codes = [
        EscrowError::AmountMustBePositive as u32,
        EscrowError::YieldBpsOutOfRange as u32,
        EscrowError::EscrowAlreadyInitialized as u32,
        EscrowError::InvoiceIdInvalidLength as u32,
        EscrowError::InvoiceIdInvalidCharset as u32,
        EscrowError::MinContributionNotPositive as u32,
        EscrowError::MinContributionExceedsAmount as u32,
        EscrowError::MaxUniqueInvestorsNotPositive as u32,
        EscrowError::MaxPerInvestorNotPositive as u32,
        EscrowError::TierYieldOutOfRange as u32,
        EscrowError::TierYieldBelowBase as u32,
        EscrowError::TierLockNotIncreasing as u32,
        EscrowError::TierYieldNotNonDecreasing as u32,
        EscrowError::EscrowNotInitialized as u32,
        EscrowError::FundingTokenNotSet as u32,
        EscrowError::TreasuryNotSet as u32,
        EscrowError::LegalHoldBlocksTreasuryDustSweep as u32,
        EscrowError::SweepAmountNotPositive as u32,
        EscrowError::SweepAmountExceedsMax as u32,
        EscrowError::DustSweepNotTerminal as u32,
        EscrowError::NoFundingTokenBalanceToSweep as u32,
        EscrowError::EffectiveSweepAmountZero as u32,
        EscrowError::TransferAmountNotPositive as u32,
        EscrowError::InsufficientTokenBalanceBeforeTransfer as u32,
        EscrowError::SenderBalanceUnderflow as u32,
        EscrowError::RecipientBalanceUnderflow as u32,
        EscrowError::SenderBalanceDeltaMismatch as u32,
        EscrowError::RecipientBalanceDeltaMismatch as u32,
        EscrowError::SweepExceedsLiabilityFloor as u32,
        EscrowError::PrimaryAttestationAlreadyBound as u32,
        EscrowError::AttestationAppendLogCapacityReached as u32,
        EscrowError::CollateralAmountNotPositive as u32,
        EscrowError::CollateralAssetEmpty as u32,
        EscrowError::CollateralTimestampBackwards as u32,
        EscrowError::InvestorBatchEmpty as u32,
        EscrowError::InvestorBatchTooLarge as u32,
        EscrowError::FundingBatchEmpty as u32,
        EscrowError::FundingBatchTooLarge as u32,
        EscrowError::TargetNotPositive as u32,
        EscrowError::TargetUpdateNotOpen as u32,
        EscrowError::TargetBelowFundedAmount as u32,
        EscrowError::CapLowerNotOpen as u32,
        EscrowError::NoInvestorCapConfigured as u32,
        EscrowError::NewCapNotLower as u32,
        EscrowError::NewCapBelowCurrentFunderCount as u32,
        EscrowError::MaturityUpdateNotOpen as u32,
        EscrowError::NewAdminSameAsCurrent as u32,
        EscrowError::MigrationVersionMismatch as u32,
        EscrowError::AlreadyCurrentSchemaVersion as u32,
        EscrowError::NoMigrationPath as u32,
        EscrowError::FundingAmountNotPositive as u32,
        EscrowError::FundingBelowMinContribution as u32,
        EscrowError::LegalHoldBlocksFunding as u32,
        EscrowError::EscrowNotOpenForFunding as u32,
        EscrowError::InvestorNotAllowlisted as u32,
        EscrowError::InvestorContributionOverflow as u32,
        EscrowError::InvestorContributionExceedsCap as u32,
        EscrowError::UniqueInvestorCapReached as u32,
        EscrowError::TieredSecondDeposit as u32,
        EscrowError::InvestorClaimTimeOverflow as u32,
        EscrowError::FundedAmountOverflow as u32,
        EscrowError::CommitmentLockExceedsMaturity as u32,
        EscrowError::LegalHoldBlocksSettlement as u32,
        EscrowError::SettlementNotFunded as u32,
        EscrowError::MaturityNotReached as u32,
        EscrowError::LegalHoldBlocksWithdrawal as u32,
        EscrowError::WithdrawalNotFunded as u32,
        EscrowError::LegalHoldBlocksInvestorClaims as u32,
        EscrowError::NoContributionToClaim as u32,
        EscrowError::InvestorClaimNotSettled as u32,
        EscrowError::InvestorCommitmentLockNotExpired as u32,
        EscrowError::ComputePayoutArithmeticOverflow as u32,
        EscrowError::LegalHoldBlocksCancelFunding as u32,
        EscrowError::CancelFundingNotOpen as u32,
        EscrowError::RefundNotCancelled as u32,
        EscrowError::NoContributionToRefund as u32,
        EscrowError::LegalHoldClearRequestMissing as u32,
        EscrowError::LegalHoldClearNotReady as u32,
        EscrowError::LegalHoldClearDelayOverflow as u32,
        EscrowError::FundingDeadlinePassed as u32,
        EscrowError::LegalHoldBlocksBeneficiaryRotation as u32,
        EscrowError::RotationNotOpen as u32,
        EscrowError::NewSmeSameAsCurrent as u32,
        EscrowError::NoPendingAdmin as u32,
        EscrowError::InsufficientContractBalance as u32,
    ];
    for code in codes.iter() {
        assert!(
            discriminants.insert(*code),
            "Duplicate discriminant: {}",
            code
        );
    }
}
