//! Tests for issue #1571: `settle_batch` per-escrow result codes.
//!
//! Acceptance criteria covered:
//! 1. `[eligible, not-matured, already-settled, eligible]` → `[0, MaturityNotReached, EscrowAlreadySettled, 0]`,
//!    with both eligible escrows settled (status == 2).
//! 2. Duplicate address rejection (`SettlementBatchDuplicate`).
//! 3. Self-reference rejection (`SettlementBatchSelfReference`).
//! 4. Empty batch rejection (`SettlementBatchEmpty`).
//! 5. Oversized batch rejection (`SettlementBatchTooLarge`).
//! 6. `SettleBatchProcessed` event emitted with correct `ok_count` / `fail_count`.

use super::{
    assert_contract_error, deploy_id, EscrowError, LiquifactEscrow, LiquifactEscrowClient,
    SettleBatchProcessed, MAX_SETTLE_BATCH,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, Event, String,
};

// ---------------------------------------------------------------------------
// Error codes from EscrowError used in result vectors.
// ---------------------------------------------------------------------------
const MATURITY_NOT_REACHED: u32 = 122;
const ESCROW_ALREADY_SETTLED: u32 = 236;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Deploy and initialise a funded escrow. `maturity` controls the time lock:
/// - `0`  → no maturity lock; `settle` succeeds immediately.
/// - any other value → `settle` fails until `ledger.timestamp >= maturity`.
fn deploy_funded_escrow<'a>(
    env: &'a Env,
    invoice_id: &str,
    maturity: u64,
) -> (LiquifactEscrowClient<'a>, Address) {
    let id = deploy_id(env);
    let client = LiquifactEscrowClient::new(env, &id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &1_000i128,
        &800i64,
        &maturity,
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
        &None::<i64>,
    );

    // Fund to target so status transitions to 1 (funded).
    client.fund(&Address::generate(env), &1_000i128);

    (client, id)
}

// ---------------------------------------------------------------------------
// AC-1: mixed eligible / ineligible results
// ---------------------------------------------------------------------------

/// `[eligible, not-matured, already-settled, eligible]`
/// → codes `[0, MaturityNotReached, EscrowAlreadySettled, 0]`
/// and status of entries 0 and 3 must be 2 (settled).
#[test]
fn settle_batch_returns_per_entry_codes() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    // Set ledger time so maturity=0 escrows are settleable and future-maturity are not.
    let mut info = env.ledger().get();
    info.timestamp = 1_000;
    env.ledger().set(info);

    // Caller escrow (batch runner).
    let caller_id = deploy_id(&env);
    let caller = LiquifactEscrowClient::new(&env, &caller_id);

    // Entry 0: eligible (maturity = 0, funded).
    let (client_a, addr_a) = deploy_funded_escrow(&env, "SB_A", 0);
    // Entry 1: not yet matured.
    let (_client_b, addr_b) = deploy_funded_escrow(&env, "SB_B", 9_999);
    // Entry 2: already settled — settle it before the batch.
    let (client_c, addr_c) = deploy_funded_escrow(&env, "SB_C", 0);
    client_c.settle();
    assert_eq!(
        client_c.get_escrow().status,
        2u32,
        "precondition: C settled"
    );
    // Entry 3: eligible (maturity = 0, funded).
    let (client_d, addr_d) = deploy_funded_escrow(&env, "SB_D", 0);

    let results = caller.settle_batch(&soroban_sdk::vec![
        &env,
        addr_a.clone(),
        addr_b.clone(),
        addr_c.clone(),
        addr_d.clone(),
    ]);

    assert_eq!(results.len(), 4, "one result per input");
    assert_eq!(results.get(0).unwrap(), 0u32, "A: eligible → 0");
    assert_eq!(
        results.get(1).unwrap(),
        MATURITY_NOT_REACHED,
        "B: not matured"
    );
    assert_eq!(
        results.get(2).unwrap(),
        ESCROW_ALREADY_SETTLED,
        "C: already settled"
    );
    assert_eq!(results.get(3).unwrap(), 0u32, "D: eligible → 0");

    // Status assertions: only the eligible entries must be settled.
    assert_eq!(client_a.get_escrow().status, 2u32, "A settled");
    assert_eq!(client_d.get_escrow().status, 2u32, "D settled");
}

// ---------------------------------------------------------------------------
// AC-2: duplicate rejection
// ---------------------------------------------------------------------------

/// A batch containing the same address twice is rejected atomically with
/// `SettlementBatchDuplicate` before any `try_settle` is called.
#[test]
fn settle_batch_duplicate_address_reverts() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let caller_id = deploy_id(&env);
    let caller = LiquifactEscrowClient::new(&env, &caller_id);
    let (_c, addr_a) = deploy_funded_escrow(&env, "SBD_A", 0);

    let result = caller.try_settle_batch(&soroban_sdk::vec![&env, addr_a.clone(), addr_a.clone(),]);
    assert_contract_error(result, EscrowError::SettlementBatchDuplicate);
}

// ---------------------------------------------------------------------------
// AC-3: self-reference rejection
// ---------------------------------------------------------------------------

/// A batch containing the calling contract's own address is rejected atomically
/// with `SettlementBatchSelfReference`.
#[test]
fn settle_batch_self_reference_reverts() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let caller_id = deploy_id(&env);
    let caller = LiquifactEscrowClient::new(&env, &caller_id);
    let (_c, addr_a) = deploy_funded_escrow(&env, "SBS_A", 0);

    let result = caller.try_settle_batch(&soroban_sdk::vec![&env, addr_a, caller_id.clone(),]);
    assert_contract_error(result, EscrowError::SettlementBatchSelfReference);
}

/// Self-reference as the only entry is also rejected.
#[test]
fn settle_batch_self_reference_only_entry_reverts() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let caller_id = deploy_id(&env);
    let caller = LiquifactEscrowClient::new(&env, &caller_id);

    let result = caller.try_settle_batch(&soroban_sdk::vec![&env, caller_id.clone()]);
    assert_contract_error(result, EscrowError::SettlementBatchSelfReference);
}

// ---------------------------------------------------------------------------
// AC-4 / AC-5: empty and oversized batch rejections
// ---------------------------------------------------------------------------

#[test]
fn settle_batch_empty_reverts() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let caller_id = deploy_id(&env);
    let caller = LiquifactEscrowClient::new(&env, &caller_id);

    let result = caller.try_settle_batch(&soroban_sdk::vec![&env]);
    assert_contract_error(result, EscrowError::SettlementBatchEmpty);
}

#[test]
fn settle_batch_oversized_reverts() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let caller_id = deploy_id(&env);
    let caller = LiquifactEscrowClient::new(&env, &caller_id);

    // Build MAX_SETTLE_BATCH + 1 distinct addresses; the size guard fires first.
    let mut addrs = soroban_sdk::vec![&env];
    for _ in 0..(MAX_SETTLE_BATCH + 1) {
        addrs.push_back(Address::generate(&env));
    }

    let result = caller.try_settle_batch(&addrs);
    assert_contract_error(result, EscrowError::SettlementBatchTooLarge);
}

// ---------------------------------------------------------------------------
// AC-6: SettleBatchProcessed event
// ---------------------------------------------------------------------------

/// A fully-successful batch emits `SettleBatchProcessed { ok_count: 2, fail_count: 0 }`
/// as the last event from the caller contract.
#[test]
fn settle_batch_emits_settle_batch_processed_all_ok() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let caller_id = deploy_id(&env);
    let caller = LiquifactEscrowClient::new(&env, &caller_id);

    let (_ca, addr_a) = deploy_funded_escrow(&env, "EVT_A", 0);
    let (_cb, addr_b) = deploy_funded_escrow(&env, "EVT_B", 0);

    caller.settle_batch(&soroban_sdk::vec![&env, addr_a, addr_b]);

    let all = env.events().all();
    let last = all
        .events()
        .last()
        .expect("expected at least one event")
        .clone();
    assert_eq!(
        last,
        SettleBatchProcessed {
            name: symbol_short!("stl_batch"),
            ok_count: 2,
            fail_count: 0,
        }
        .to_xdr(&env, &caller_id),
        "last event must be SettleBatchProcessed {{ok_count:2, fail_count:0}}"
    );
}

/// A mixed batch emits `SettleBatchProcessed { ok_count: 1, fail_count: 1 }`.
#[test]
fn settle_batch_emits_settle_batch_processed_mixed() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let mut info = env.ledger().get();
    info.timestamp = 1_000;
    env.ledger().set(info);

    let caller_id = deploy_id(&env);
    let caller = LiquifactEscrowClient::new(&env, &caller_id);

    let (_ca, addr_a) = deploy_funded_escrow(&env, "MEVT_A", 0); // eligible
    let (_cb, addr_b) = deploy_funded_escrow(&env, "MEVT_B", 9_999); // not matured

    let results = caller.settle_batch(&soroban_sdk::vec![&env, addr_a, addr_b]);

    assert_eq!(results.get(0).unwrap(), 0u32, "A ok");
    assert_eq!(results.get(1).unwrap(), MATURITY_NOT_REACHED, "B fail");

    let all = env.events().all();
    let last = all
        .events()
        .last()
        .expect("expected at least one event")
        .clone();
    assert_eq!(
        last,
        SettleBatchProcessed {
            name: symbol_short!("stl_batch"),
            ok_count: 1,
            fail_count: 1,
        }
        .to_xdr(&env, &caller_id),
        "last event must be SettleBatchProcessed {{ok_count:1, fail_count:1}}"
    );
}

// ---------------------------------------------------------------------------
// Additional edge cases
// ---------------------------------------------------------------------------

/// A single-entry batch with an eligible escrow settles it and returns `[0]`.
#[test]
fn settle_batch_single_eligible_entry() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let caller_id = deploy_id(&env);
    let caller = LiquifactEscrowClient::new(&env, &caller_id);
    let (client_a, addr_a) = deploy_funded_escrow(&env, "SE_A", 0);

    let results = caller.settle_batch(&soroban_sdk::vec![&env, addr_a]);

    assert_eq!(results.len(), 1);
    assert_eq!(results.get(0).unwrap(), 0u32);
    assert_eq!(client_a.get_escrow().status, 2u32, "A settled");
}

/// A batch where all entries are ineligible still returns a result vector
/// without reverting, and `SettleBatchProcessed` has `ok_count: 0, fail_count: 2`.
#[test]
fn settle_batch_all_fail_does_not_revert() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let mut info = env.ledger().get();
    info.timestamp = 500;
    env.ledger().set(info);

    let caller_id = deploy_id(&env);
    let caller = LiquifactEscrowClient::new(&env, &caller_id);

    let (_ca, addr_a) = deploy_funded_escrow(&env, "AF_A", 9_999);
    let (_cb, addr_b) = deploy_funded_escrow(&env, "AF_B", 9_999);

    let results = caller.settle_batch(&soroban_sdk::vec![&env, addr_a, addr_b]);

    assert_eq!(results.len(), 2);
    assert_eq!(results.get(0).unwrap(), MATURITY_NOT_REACHED, "A fail");
    assert_eq!(results.get(1).unwrap(), MATURITY_NOT_REACHED, "B fail");

    let all = env.events().all();
    let last = all
        .events()
        .last()
        .expect("expected at least one event")
        .clone();
    assert_eq!(
        last,
        SettleBatchProcessed {
            name: symbol_short!("stl_batch"),
            ok_count: 0,
            fail_count: 2,
        }
        .to_xdr(&env, &caller_id),
        "last event must be SettleBatchProcessed {{ok_count:0, fail_count:2}}"
    );
}
