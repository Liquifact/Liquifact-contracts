//! Deterministic failure-recovery tests for the investor allowlist subsystem.
//!
//! # Coverage
//!
//! | Scenario family | What we verify |
//! |---|---|
//! | Defaults | Gate and per-address status are `false` before any mutation |
//! | Enable / disable toggle | Storage written, events emitted, nonce consumed |
//! | Per-address add / remove | Persistent entry written, events emitted, idempotent remove |
//! | Auth enforcement | Every state-changing call fails without admin auth |
//! | Fund gate | `fund` and `fund_with_commitment` pass / fail deterministically |
//! | Toggle mid-funding | Disable unblocks; re-enable re-blocks |
//! | Revocation mid-funding | Prior contribution does NOT exempt investor; typed error returned |
//! | Multi-investor independence | One investor's status does not affect another |
//! | Batch set/revoke | Correct per-investor events emitted; batch bounded; auth required |
//! | Pagination arithmetic | `start ≈ u32::MAX`, `limit ≈ u32::MAX`, ceiling clamping, no overflow |
//! | Count accuracy | Increments, decrements, idempotent re-add, removal |
//! | Full pagination scan | All N entries recoverable across pages with no duplicates |

use soroban_sdk::testutils::Events as _;
use soroban_sdk::Vec as SorobanVec;
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Error, InvokeError};
use std::fmt::Debug;

use super::{
    AllowlistEnabledChanged, DataKey, EscrowError, InvestorAllowlistChanged, LiquifactEscrow,
    LiquifactEscrowClient, MAX_INVESTOR_ALLOWLIST_BATCH, MAX_INVESTOR_READ_BATCH,
};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

/// Initialise a fresh escrow instance.
///
/// Passes all 19 required arguments to `init`, matching the current contract
/// signature exactly.  Returns `(admin, sme)` so callers can use the admin
/// for subsequent privileged operations.
fn init_escrow(env: &Env, client: &LiquifactEscrowClient, invoice_id: &str) -> (Address, Address) {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, invoice_id),
        &sme,
        &100_000i128,
        &800i64,
        &0u64,       // maturity
        &token,
        &None,       // registry
        &treasury,
        &None,       // yield_tiers
        &None,       // min_contribution
        &None,       // max_unique_investors
        &None,       // max_per_investor
        &None,       // legal_hold_clear_delay
        &None,       // maturity_max_horizon
        &None,       // funding_deadline
        &None,       // allowlist_active
        &None::<i64>, // protocol_fee_bps
        &None::<u32>, // token_decimals
    );
    (admin, sme)
}

/// Convenience wrapper: deploy + init in one call.
fn deploy_and_init(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    let client = deploy(env);
    let (admin, sme) = init_escrow(env, &client, "ALINV001");
    (client, admin, sme)
}

/// Assert that a `try_*` client call returns the expected typed contract error.
///
/// Handles both SDK encoding forms (`Err(Ok(Error))` and
/// `Err(Err(InvokeError::Contract(code)))`) so the assertion stays valid
/// across Soroban SDK minor version differences.
fn assert_typed_error<T, E>(
    result: Result<Result<T, E>, Result<Error, InvokeError>>,
    expected: EscrowError,
) where
    T: Debug,
    E: Debug,
{
    let code = expected as u32;
    match result {
        Err(Ok(err)) => assert_eq!(err, Error::from_contract_error(code)),
        Err(Err(InvokeError::Contract(c))) => assert_eq!(c, code),
        other => panic!("expected ContractError({code}), got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 1. Defaults — gate and per-address status before any mutation
// ---------------------------------------------------------------------------

/// The allowlist gate is disabled by default; no admin call required to fund.
#[test]
fn test_allowlist_disabled_by_default() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    assert!(!client.is_allowlist_active());
}

/// Any investor address is not allowlisted until an explicit `set_investor_allowlisted` call.
#[test]
fn test_is_allowlisted_false_by_default() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let stranger = Address::generate(&env);
    assert!(!client.is_investor_allowlisted(&stranger));
}

// ---------------------------------------------------------------------------
// 2. Enable / disable toggle
// ---------------------------------------------------------------------------

/// `set_allowlist_active(true)` writes to instance storage and emits the
/// `AllowlistEnabledChanged` event with `active = 1`.
#[test]
fn test_enable_allowlist_persists_and_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, "ALINV002");
    let invoice_id = client.get_escrow().invoice_id;
    let contract_id = client.address.clone();

    client.set_allowlist_active(&true, &0u32);

    // Verify storage.
    env.as_contract(&contract_id, || {
        assert_eq!(
            env.storage()
                .instance()
                .get::<DataKey, bool>(&DataKey::AllowlistActive),
            Some(true)
        );
    });

    // Verify event.
    let all = env.events().all();
    let events = all.events();
    assert_eq!(
        events.last().unwrap().clone(),
        AllowlistEnabledChanged {
            name: symbol_short!("al_ena"),
            invoice_id,
            active: 1,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// `set_allowlist_active(false)` writes `false` to instance storage and emits
/// `AllowlistEnabledChanged` with `active = 0`.
#[test]
fn test_disable_allowlist_persists_and_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, "ALINV003");
    let invoice_id = client.get_escrow().invoice_id;
    let contract_id = client.address.clone();

    client.set_allowlist_active(&true, &0u32);
    client.set_allowlist_active(&false, &1u32);

    // Verify storage.
    env.as_contract(&contract_id, || {
        assert_eq!(
            env.storage()
                .instance()
                .get::<DataKey, bool>(&DataKey::AllowlistActive),
            Some(false)
        );
    });

    // Verify the last event is the disable event.
    let all = env.events().all();
    let events = all.events();
    assert_eq!(
        events.last().unwrap().clone(),
        AllowlistEnabledChanged {
            name: symbol_short!("al_ena"),
            invoice_id,
            active: 0,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Enabling then reading the gate state is consistent.
#[test]
fn test_enable_and_disable_is_readable() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    client.set_allowlist_active(&true, &0u32);
    assert!(client.is_allowlist_active());

    client.set_allowlist_active(&false, &1u32);
    assert!(!client.is_allowlist_active());
}

// ---------------------------------------------------------------------------
// 3. Auth enforcement — toggle
// ---------------------------------------------------------------------------

/// `set_allowlist_active` must fail when no admin auth is provided.
#[test]
#[should_panic]
fn test_enable_allowlist_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    env.mock_auths(&[]);
    client.set_allowlist_active(&true, &0u32);
}

/// Disabling the gate also requires admin auth.
#[test]
#[should_panic]
fn test_disable_allowlist_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    client.set_allowlist_active(&true, &0u32);
    env.mock_auths(&[]);
    client.set_allowlist_active(&false, &1u32);
}

// ---------------------------------------------------------------------------
// 4. Per-address add / remove
// ---------------------------------------------------------------------------

/// `set_investor_allowlisted(addr, true)` writes to persistent storage and
/// emits `InvestorAllowlistChanged` with `allowed = 1`.
#[test]
fn test_add_to_allowlist_persists_and_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, "ALINV004");
    let invoice_id = client.get_escrow().invoice_id;
    let contract_id = client.address.clone();
    let investor = Address::generate(&env);

    client.set_investor_allowlisted(&investor, &true, &0u32);

    // Verify persistent storage.
    env.as_contract(&contract_id, || {
        assert_eq!(
            env.storage()
                .persistent()
                .get::<DataKey, bool>(&DataKey::InvestorAllowlisted(investor.clone())),
            Some(true)
        );
    });

    // Verify event.
    let all = env.events().all();
    let events = all.events();
    assert_eq!(
        events.last().unwrap().clone(),
        InvestorAllowlistChanged {
            name: symbol_short!("al_set"),
            invoice_id,
            investor,
            allowed: 1,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// `set_investor_allowlisted(addr, false)` writes `false` to persistent
/// storage and emits `InvestorAllowlistChanged` with `allowed = 0`.
#[test]
fn test_remove_from_allowlist_persists_and_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, "ALINV005");
    let invoice_id = client.get_escrow().invoice_id;
    let contract_id = client.address.clone();
    let investor = Address::generate(&env);

    client.set_investor_allowlisted(&investor, &true, &0u32);
    client.set_investor_allowlisted(&investor, &false, &1u32);

    // Verify persistent storage.
    env.as_contract(&contract_id, || {
        assert_eq!(
            env.storage()
                .persistent()
                .get::<DataKey, bool>(&DataKey::InvestorAllowlisted(investor.clone())),
            Some(false)
        );
    });

    // Verify last event.
    let all = env.events().all();
    let events = all.events();
    assert_eq!(
        events.last().unwrap().clone(),
        InvestorAllowlistChanged {
            name: symbol_short!("al_set"),
            invoice_id,
            investor,
            allowed: 0,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Removing an address that was never added succeeds without error (idempotent).
#[test]
fn test_remove_non_existent_address_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let stranger = Address::generate(&env);
    // Must not panic.
    client.set_investor_allowlisted(&stranger, &false, &0u32);
    assert!(!client.is_investor_allowlisted(&stranger));
}

/// Re-allowlisting an already-allowlisted investor is idempotent.
#[test]
fn test_re_allowlist_same_investor_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    client.set_investor_allowlisted(&investor, &true, &0u32);
    assert_eq!(client.get_allowlisted_investors_count(), 1);

    // Setting true again must not bump the count.
    client.set_investor_allowlisted(&investor, &true, &1u32);
    assert_eq!(
        client.get_allowlisted_investors_count(),
        1,
        "idempotent re-add must not increment the index"
    );
    assert!(client.is_investor_allowlisted(&investor));
}

// ---------------------------------------------------------------------------
// 5. Auth enforcement — per-address
// ---------------------------------------------------------------------------

/// `set_investor_allowlisted` must fail without admin auth.
#[test]
#[should_panic]
fn test_add_to_allowlist_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);
    env.mock_auths(&[]);
    client.set_investor_allowlisted(&investor, &true, &0u32);
}

/// Removing an investor also requires admin auth.
#[test]
#[should_panic]
fn test_remove_from_allowlist_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);
    client.set_investor_allowlisted(&investor, &true, &0u32);
    env.mock_auths(&[]);
    client.set_investor_allowlisted(&investor, &false, &1u32);
}

// ---------------------------------------------------------------------------
// 6. Fund gate — basic allow/deny matrix
// ---------------------------------------------------------------------------

/// Gate inactive, no entry → `fund` succeeds.
#[test]
fn test_fund_allowed_when_allowlist_disabled() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);
    let escrow = client.fund(&investor, &5_000i128);
    assert_eq!(escrow.funded_amount, 5_000i128);
}

/// Gate inactive, no entry → `fund_with_commitment` succeeds.
#[test]
fn test_fund_with_commitment_allowed_when_allowlist_disabled() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);
    let escrow = client.fund_with_commitment(&investor, &5_000i128, &0u64);
    assert_eq!(escrow.funded_amount, 5_000i128);
}

/// Gate active, investor allowlisted → `fund` succeeds.
#[test]
fn test_fund_allowed_when_on_allowlist() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true, &0u32);
    client.set_investor_allowlisted(&investor, &true, &1u32);

    let escrow = client.fund(&investor, &5_000i128);
    assert_eq!(escrow.funded_amount, 5_000i128);
}

/// Gate active, investor NOT allowlisted → `fund` returns `InvestorNotAllowlisted` (104).
#[test]
fn test_fund_blocked_when_not_on_allowlist_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true, &0u32);

    assert_typed_error(
        client.try_fund(&investor, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );
}

/// Gate active, investor explicitly denied → `fund` returns `InvestorNotAllowlisted` (104).
#[test]
fn test_fund_blocked_when_explicitly_denied_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true, &0u32);
    client.set_investor_allowlisted(&investor, &false, &1u32);

    assert_typed_error(
        client.try_fund(&investor, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );
}

/// Gate active, investor allowlisted → `fund_with_commitment` succeeds.
#[test]
fn test_fund_with_commitment_allowed_when_on_allowlist() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true, &0u32);
    client.set_investor_allowlisted(&investor, &true, &1u32);

    let escrow = client.fund_with_commitment(&investor, &5_000i128, &0u64);
    assert_eq!(escrow.funded_amount, 5_000i128);
}

/// Gate active, investor absent → `fund_with_commitment` returns `InvestorNotAllowlisted`.
#[test]
fn test_fund_with_commitment_blocked_when_not_on_allowlist_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true, &0u32);

    assert_typed_error(
        client.try_fund_with_commitment(&investor, &1_000i128, &0u64),
        EscrowError::InvestorNotAllowlisted,
    );
}

/// Gate active, investor explicitly denied → `fund_with_commitment` returns `InvestorNotAllowlisted`.
#[test]
fn test_fund_with_commitment_blocked_when_explicitly_denied_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true, &0u32);
    client.set_investor_allowlisted(&investor, &false, &1u32);

    assert_typed_error(
        client.try_fund_with_commitment(&investor, &1_000i128, &0u64),
        EscrowError::InvestorNotAllowlisted,
    );
}

// ---------------------------------------------------------------------------
// 7. Toggle mid-funding — disable unblocks; re-enable re-blocks
// ---------------------------------------------------------------------------

/// Disabling the gate after enabling it lets any address fund without an entry.
#[test]
fn test_fund_allowed_after_disable_even_without_entry() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true, &0u32);
    client.set_allowlist_active(&false, &1u32);

    let escrow = client.fund(&investor, &3_000i128);
    assert_eq!(escrow.funded_amount, 3_000i128);
}

/// Re-enabling the gate after a successful fund blocks subsequent deposits for
/// un-enrolled investors.  A prior contribution does NOT grant a permanent pass.
#[test]
fn test_gate_reenable_blocks_investor_without_entry() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    // Gate off → first deposit succeeds.
    let after_first = client.fund(&investor, &1_000i128);
    assert_eq!(after_first.funded_amount, 1_000i128);

    // Enable gate without allowlisting this investor.
    client.set_allowlist_active(&true, &0u32);

    // Second deposit must be rejected even though investor has a prior contribution.
    assert_typed_error(
        client.try_fund(&investor, &500i128),
        EscrowError::InvestorNotAllowlisted,
    );
    // Prior contribution must remain unchanged.
    assert_eq!(client.get_contribution(&investor), 1_000i128);
}

/// Entries persist across a disable → re-enable cycle so enrolled investors do not
/// need to be re-added.
#[test]
fn test_entries_persist_across_disable_reenable() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true, &0u32);
    client.set_investor_allowlisted(&investor, &true, &1u32);
    client.set_allowlist_active(&false, &2u32);

    // Entry must still be visible while gate is disabled.
    assert!(client.is_investor_allowlisted(&investor));

    // Re-enable → investor should fund without re-adding.
    client.set_allowlist_active(&true, &3u32);
    let escrow = client.fund(&investor, &2_000i128);
    assert_eq!(escrow.funded_amount, 2_000i128);
}

/// A removed investor is blocked again after the gate is re-enabled.
#[test]
fn test_removed_investor_blocked_after_reenable() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    client.set_allowlist_active(&true, &0u32);
    client.set_investor_allowlisted(&investor, &true, &1u32);
    client.set_investor_allowlisted(&investor, &false, &2u32);

    assert_typed_error(
        client.try_fund(&investor, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );
}

// ---------------------------------------------------------------------------
// 8. Security: revocation mid-funding
// ---------------------------------------------------------------------------

/// Revoking an investor AFTER their first deposit blocks subsequent deposits.
///
/// Invariant: the gate re-evaluates the current allowlist status on every call.
/// A historical contribution confers no bypass.
#[test]
fn test_revocation_mid_funding_blocks_next_deposit_fund() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    // Step 1: allowlist active, investor allowed — first deposit succeeds.
    client.set_allowlist_active(&true, &0u32);
    client.set_investor_allowlisted(&investor, &true, &1u32);
    let after_first = client.fund(&investor, &3_000i128);
    assert_eq!(after_first.funded_amount, 3_000i128);
    assert_eq!(client.get_contribution(&investor), 3_000i128);

    // Step 2: admin revokes the investor.
    client.set_investor_allowlisted(&investor, &false, &2u32);
    assert!(!client.is_investor_allowlisted(&investor));

    // Step 3: second deposit must be rejected.
    assert_typed_error(
        client.try_fund(&investor, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );

    // Contribution must not have changed.
    assert_eq!(client.get_contribution(&investor), 3_000i128);
}

/// Revoking before the investor's first `fund_with_commitment` call blocks it.
#[test]
fn test_revocation_before_first_fwc_deposit_blocks_it() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    // Allowlist then immediately revoke before any deposit.
    client.set_allowlist_active(&true, &0u32);
    client.set_investor_allowlisted(&investor, &true, &1u32);
    client.set_investor_allowlisted(&investor, &false, &2u32);

    assert_typed_error(
        client.try_fund_with_commitment(&investor, &5_000i128, &0u64),
        EscrowError::InvestorNotAllowlisted,
    );
    // No contribution recorded.
    assert_eq!(client.get_contribution(&investor), 0i128);
}

// ---------------------------------------------------------------------------
// 9. Multi-investor independence
// ---------------------------------------------------------------------------

/// Allowlisting investor A does not affect investor B or C.  Each address is
/// independently gated.
#[test]
fn test_multiple_investors_independent_allowlist_entries() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);

    client.set_allowlist_active(&true, &0u32);
    client.set_investor_allowlisted(&a, &true, &1u32);
    client.set_investor_allowlisted(&b, &true, &2u32);

    assert!(client.is_investor_allowlisted(&a));
    assert!(client.is_investor_allowlisted(&b));
    assert!(!client.is_investor_allowlisted(&c));

    let after_a = client.fund(&a, &2_000i128);
    assert_eq!(after_a.funded_amount, 2_000i128);
    let after_b = client.fund(&b, &3_000i128);
    assert_eq!(after_b.funded_amount, 5_000i128);

    // C is rejected.
    assert_typed_error(
        client.try_fund(&c, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );

    // A and B contributions are unchanged after C's rejected attempt.
    assert_eq!(client.get_contribution(&a), 2_000i128);
    assert_eq!(client.get_contribution(&b), 3_000i128);
    assert_eq!(client.get_contribution(&c), 0i128);
}

// ---------------------------------------------------------------------------
// 10. Batch set / revoke
// ---------------------------------------------------------------------------

/// Batch-adding three investors writes persistent entries for each and makes
/// all three visible via `is_investor_allowlisted`.
#[test]
fn test_batch_add_sets_all_entries() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);

    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    v.push_back(a.clone());
    v.push_back(b.clone());
    v.push_back(c.clone());

    client.set_investors_allowlisted(&v, &true, &0u32);

    assert!(client.is_investor_allowlisted(&a));
    assert!(client.is_investor_allowlisted(&b));
    assert!(client.is_investor_allowlisted(&c));
}

/// Batch-revoking previously-allowlisted investors removes all entries.
#[test]
fn test_batch_revoke_removes_all_entries() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);

    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    v.push_back(a.clone());
    v.push_back(b.clone());
    v.push_back(c.clone());

    client.set_investors_allowlisted(&v, &true, &0u32);
    client.set_investors_allowlisted(&v, &false, &1u32);

    assert!(!client.is_investor_allowlisted(&a));
    assert!(!client.is_investor_allowlisted(&b));
    assert!(!client.is_investor_allowlisted(&c));
}

/// Batch of 1 emits exactly one `al_set` event — same payload as a single-investor call.
#[test]
fn test_batch_single_entry_emits_single_event() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, "ALINV010");
    let invoice_id = client.get_escrow().invoice_id;
    let contract_id = client.address.clone();
    let investor = Address::generate(&env);

    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    v.push_back(investor.clone());

    let events_before = env.events().all().events().len();
    client.set_investors_allowlisted(&v, &true, &0u32);
    let all = env.events().all();
    let events = all.events();

    // Exactly one new event.
    assert_eq!(events.len(), events_before + 1);
    assert_eq!(
        events.last().unwrap().clone(),
        InvestorAllowlistChanged {
            name: symbol_short!("al_set"),
            invoice_id,
            investor,
            allowed: 1,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Batch of N emits exactly N `al_set` events — one per investor, in order.
#[test]
fn test_batch_n_entries_emits_n_events_in_order() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, "ALINV011");
    let invoice_id = client.get_escrow().invoice_id;
    let contract_id = client.address.clone();

    let n: usize = 3;
    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    let mut addrs = std::vec::Vec::new();
    for _ in 0..n {
        let a = Address::generate(&env);
        addrs.push(a.clone());
        v.push_back(a);
    }

    let events_before = env.events().all().events().len();
    client.set_investors_allowlisted(&v, &true, &0u32);
    let all = env.events().all();
    let events = all.events();

    assert_eq!(events.len(), events_before + n);
    for (i, addr) in addrs.iter().enumerate() {
        assert_eq!(
            events.get((events_before + i) as u32).unwrap().clone(),
            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: invoice_id.clone(),
                investor: addr.clone(),
                allowed: 1,
            }
            .to_xdr(&env, &contract_id)
        );
    }
}

/// Batch of exactly `MAX_INVESTOR_ALLOWLIST_BATCH` succeeds (boundary).
#[test]
fn test_batch_exact_max_size_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let max = MAX_INVESTOR_ALLOWLIST_BATCH as usize;
    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..max {
        v.push_back(Address::generate(&env));
    }

    // Must not panic.
    client.set_investors_allowlisted(&v, &true, &0u32);
    assert_eq!(
        client.get_allowlisted_investors_count(),
        MAX_INVESTOR_ALLOWLIST_BATCH
    );
}

/// Batch of `MAX + 1` is rejected with `InvestorBatchTooLarge` (71).
#[test]
fn test_batch_one_over_max_returns_investor_batch_too_large() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let too_many = MAX_INVESTOR_ALLOWLIST_BATCH as usize + 1;
    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..too_many {
        v.push_back(Address::generate(&env));
    }

    assert_typed_error(
        client.try_set_investors_allowlisted(&v, &true, &0u32),
        EscrowError::InvestorBatchTooLarge,
    );
    // No partial writes.
    assert_eq!(client.get_allowlisted_investors_count(), 0);
}

/// Empty batch is rejected with `InvestorBatchEmpty` (70).
#[test]
fn test_batch_empty_returns_investor_batch_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let v: SorobanVec<Address> = SorobanVec::new(&env);

    assert_typed_error(
        client.try_set_investors_allowlisted(&v, &true, &0u32),
        EscrowError::InvestorBatchEmpty,
    );
}

/// Batch requires admin auth.
#[test]
#[should_panic]
fn test_batch_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    v.push_back(Address::generate(&env));
    env.mock_auths(&[]);
    client.set_investors_allowlisted(&v, &true, &0u32);
}

/// Batch-revoking `MAX` entries brings count from `MAX` to `0` atomically.
#[test]
fn test_batch_revoke_max_entries_count_reaches_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let max = MAX_INVESTOR_ALLOWLIST_BATCH as usize;
    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..max {
        v.push_back(Address::generate(&env));
    }

    client.set_investors_allowlisted(&v, &true, &0u32);
    assert_eq!(client.get_allowlisted_investors_count(), MAX_INVESTOR_ALLOWLIST_BATCH);

    client.set_investors_allowlisted(&v, &false, &1u32);
    assert_eq!(
        client.get_allowlisted_investors_count(),
        0,
        "batch revoke of all MAX entries must set count to 0"
    );
}

/// Batch + gate: allowlisted addresses can fund; non-members are rejected.
#[test]
fn test_batch_allowlist_then_gate_active_correct_access() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let outsider = Address::generate(&env);

    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    v.push_back(a.clone());
    v.push_back(b.clone());

    client.set_investors_allowlisted(&v, &true, &0u32);
    client.set_allowlist_active(&true, &1u32);

    let after_a = client.fund(&a, &2_000i128);
    assert_eq!(after_a.funded_amount, 2_000i128);
    let after_b = client.fund(&b, &3_000i128);
    assert_eq!(after_b.funded_amount, 5_000i128);

    assert_typed_error(
        client.try_fund(&outsider, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );
}

// ---------------------------------------------------------------------------
// 11. Count accuracy
// ---------------------------------------------------------------------------

/// Count is 0 before any allowlisting.
#[test]
fn test_count_zero_before_any_allowlisting() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    assert_eq!(client.get_allowlisted_investors_count(), 0);
}

/// Count increments correctly for each distinct address added.
#[test]
fn test_count_increments_per_distinct_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    for i in 1u32..=5 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true, &(i - 1));
        assert_eq!(
            client.get_allowlisted_investors_count(),
            i,
            "count must equal number of distinct allowlisted addresses after {i} adds"
        );
    }
}

/// Count decrements when an investor is revoked.
#[test]
fn test_count_decrements_on_revoke() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);

    client.set_investor_allowlisted(&a, &true, &0u32);
    client.set_investor_allowlisted(&b, &true, &1u32);
    client.set_investor_allowlisted(&c, &true, &2u32);
    assert_eq!(client.get_allowlisted_investors_count(), 3);

    client.set_investor_allowlisted(&b, &false, &3u32);
    assert_eq!(client.get_allowlisted_investors_count(), 2);

    client.set_investor_allowlisted(&a, &false, &4u32);
    assert_eq!(client.get_allowlisted_investors_count(), 1);

    client.set_investor_allowlisted(&c, &false, &5u32);
    assert_eq!(client.get_allowlisted_investors_count(), 0);
}

/// Re-allowlisting an address that is already allowlisted must not double-count.
#[test]
fn test_count_re_allowlist_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let inv = Address::generate(&env);
    client.set_investor_allowlisted(&inv, &true, &0u32);
    assert_eq!(client.get_allowlisted_investors_count(), 1);

    // Duplicate set-true must not bump count.
    client.set_investor_allowlisted(&inv, &true, &1u32);
    assert_eq!(
        client.get_allowlisted_investors_count(),
        1,
        "idempotent re-add must not increment the count"
    );
}

// ---------------------------------------------------------------------------
// 12. Pagination arithmetic — overflow / saturation safety
// ---------------------------------------------------------------------------

/// `start = u32::MAX`, empty index → returns empty without overflow.
#[test]
fn test_pagination_max_start_empty_index_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let result = client.get_allowlisted_investors(&u32::MAX, &50u32);
    assert_eq!(result.len(), 0, "must return empty when start >= len (0)");
}

/// `start = u32::MAX`, one investor in index → start >= len = 1, returns empty.
#[test]
fn test_pagination_max_start_one_investor_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let inv = Address::generate(&env);
    client.set_investor_allowlisted(&inv, &true, &0u32);

    let result = client.get_allowlisted_investors(&u32::MAX, &1u32);
    assert_eq!(result.len(), 0, "start past end; must return empty");
}

/// `start = u32::MAX - 1`, limit = 50, one investor → start >= 1, returns empty without panic.
#[test]
fn test_pagination_near_max_start_returns_empty_without_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let inv = Address::generate(&env);
    client.set_investor_allowlisted(&inv, &true, &0u32);

    let result = client.get_allowlisted_investors(&(u32::MAX - 1), &50u32);
    assert_eq!(result.len(), 0, "start past len; must return empty");
}

/// `start = u32::MAX`, `limit = u32::MAX` → both saturated, must return empty without panic.
#[test]
fn test_pagination_max_start_max_limit_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let result = client.get_allowlisted_investors(&u32::MAX, &u32::MAX);
    assert_eq!(result.len(), 0, "both at u32::MAX must not overflow");
}

/// `start = 0`, `limit = u32::MAX` with 3 investors → limit clamped to 50; all 3 returned.
#[test]
fn test_pagination_zero_start_max_limit_returns_all_clamped() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    for _ in 0..3 {
        client.set_investor_allowlisted(&Address::generate(&env), &true, &0u32);
    }

    let result = client.get_allowlisted_investors(&0u32, &u32::MAX);
    assert_eq!(result.len(), 3, "all 3 investors returned when limit clamped");
}

/// With 5 investors, `start = 4`, `limit = u32::MAX` → window [4, 5); returns 1 item.
#[test]
fn test_pagination_last_valid_start_returns_one_item() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let mut addrs: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..5 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true, &0u32);
        addrs.push_back(addr);
    }

    let result = client.get_allowlisted_investors(&4u32, &u32::MAX);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get(0).unwrap(), addrs.get(4).unwrap());
}

/// `limit = 51` (one above the 50-ceiling) → `actual_limit` is clamped to 50.
/// With 60 investors, exactly 50 are returned.
#[test]
fn test_pagination_limit_above_ceiling_clamped_to_50() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    for _ in 0..60 {
        client.set_investor_allowlisted(&Address::generate(&env), &true, &0u32);
    }

    let result = client.get_allowlisted_investors(&0u32, &51u32);
    assert_eq!(result.len(), MAX_INVESTOR_READ_BATCH, "limit 51 must be clamped to 50");

    let result2 = client.get_allowlisted_investors(&0u32, &50u32);
    assert_eq!(result2.len(), MAX_INVESTOR_READ_BATCH, "limit exactly 50 must return 50");
}

/// With 5 investors, `start = 3`, `limit = u32::MAX` → returns items [3] and [4] (2 items).
#[test]
fn test_pagination_large_limit_from_mid_page_returns_tail() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let mut addrs: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..5 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true, &0u32);
        addrs.push_back(addr);
    }

    let result = client.get_allowlisted_investors(&3u32, &u32::MAX);
    assert_eq!(result.len(), 2, "tail of 2 items returned without overflow");
    assert_eq!(result.get(0).unwrap(), addrs.get(3).unwrap());
    assert_eq!(result.get(1).unwrap(), addrs.get(4).unwrap());
}

// ---------------------------------------------------------------------------
// 13. Full pagination scan — no duplicates, exact total
// ---------------------------------------------------------------------------

/// Paginate through all 13 investors (3 pages of 5 + 1 tail page of 3).
/// The union of all pages must be exactly the inserted set — no duplicates, no
/// missing entries.
#[test]
fn test_full_pagination_scan_no_overflow_or_duplicates() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let n: usize = 13;
    let mut inserted = std::collections::HashSet::new();
    for _ in 0..n {
        let addr = Address::generate(&env);
        inserted.insert(addr.to_string());
        client.set_investor_allowlisted(&addr, &true, &0u32);
    }

    let page_size: u32 = 5;
    let mut collected = std::collections::HashSet::new();
    let mut start: u32 = 0;
    loop {
        let page = client.get_allowlisted_investors(&start, &page_size);
        let page_len = page.len();
        if page_len == 0 {
            break;
        }
        for i in 0..page_len {
            let addr_str = format!("{}", page.get(i).unwrap());
            assert!(
                collected.insert(addr_str.clone()),
                "duplicate address {addr_str} in paginated results"
            );
        }
        start = start.saturating_add(page_size);
        if page_len < page_size {
            break;
        }
    }

    assert_eq!(collected.len(), n, "total paginated count must equal {n}");
    assert_eq!(
        collected, inserted,
        "paginated set must match exactly the inserted set"
    );
}

// ---------------------------------------------------------------------------
// 14. Nonce replay protection
// ---------------------------------------------------------------------------

/// Replaying the same nonce on `set_allowlist_active` must fail.
#[test]
#[should_panic]
fn test_set_allowlist_active_nonce_replay_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    client.set_allowlist_active(&true, &0u32);
    // Same nonce again → must panic.
    client.set_allowlist_active(&false, &0u32);
}

/// Replaying the same nonce on `set_investor_allowlisted` must fail.
#[test]
#[should_panic]
fn test_set_investor_allowlisted_nonce_replay_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);

    client.set_investor_allowlisted(&investor, &true, &0u32);
    // Same nonce again → must panic.
    client.set_investor_allowlisted(&investor, &false, &0u32);
}

/// Replaying the same nonce on `set_investors_allowlisted` must fail.
#[test]
#[should_panic]
fn test_set_investors_allowlisted_nonce_replay_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let investor = Address::generate(&env);
    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    v.push_back(investor.clone());

    client.set_investors_allowlisted(&v, &true, &0u32);
    // Same nonce again → must panic.
    client.set_investors_allowlisted(&v, &false, &0u32);
}

// ---------------------------------------------------------------------------
// 15. Batch event payload equivalence
// ---------------------------------------------------------------------------

/// The per-investor events emitted by `set_investors_allowlisted` are identical in
/// shape to those emitted by individual `set_investor_allowlisted` calls — the
/// batch is a pure efficiency alias, not a separate semantic path.
#[test]
fn test_batch_event_payload_matches_single_call_event_payload() {
    let env = Env::default();
    env.mock_all_auths();

    // Client 1: two single-call sets.
    let client1 = deploy(&env);
    init_escrow(&env, &client1, "ALINV020");
    let invoice_id1 = client1.get_escrow().invoice_id;
    let contract_id1 = client1.address.clone();

    let a = Address::generate(&env);
    let b = Address::generate(&env);

    client1.set_investor_allowlisted(&a, &true, &0u32);
    let single_a = env.events().all().events().last().unwrap().clone();

    client1.set_investor_allowlisted(&b, &true, &1u32);
    let single_b = env.events().all().events().last().unwrap().clone();

    // Client 2: batch set of the same addresses.
    let client2 = deploy(&env);
    init_escrow(&env, &client2, "ALINV021");
    let invoice_id2 = client2.get_escrow().invoice_id;
    let contract_id2 = client2.address.clone();

    let mut v: SorobanVec<Address> = SorobanVec::new(&env);
    v.push_back(a.clone());
    v.push_back(b.clone());

    let events_before = env.events().all().events().len();
    client2.set_investors_allowlisted(&v, &true, &0u32);
    let all2 = env.events().all();
    let events2 = all2.events();
    let batch_a = events2.get(events_before as u32).unwrap().clone();
    let batch_b = events2.get(events_before as u32 + 1).unwrap().clone();

    // The expected event XDRs: single-call path and batch path must produce the same shape.
    let expected_a = InvestorAllowlistChanged {
        name: symbol_short!("al_set"),
        invoice_id: invoice_id1.clone(),
        investor: a.clone(),
        allowed: 1,
    }
    .to_xdr(&env, &contract_id1);

    let expected_a_batch = InvestorAllowlistChanged {
        name: symbol_short!("al_set"),
        invoice_id: invoice_id2.clone(),
        investor: a.clone(),
        allowed: 1,
    }
    .to_xdr(&env, &contract_id2);

    // Structure must be identical across call sites (different escrow ids produce
    // structurally identical events; we verify the shape matches the schema).
    assert_eq!(single_a, expected_a);
    assert_eq!(batch_a, expected_a_batch);

    let expected_b = InvestorAllowlistChanged {
        name: symbol_short!("al_set"),
        invoice_id: invoice_id1,
        investor: b.clone(),
        allowed: 1,
    }
    .to_xdr(&env, &contract_id1);

    let expected_b_batch = InvestorAllowlistChanged {
        name: symbol_short!("al_set"),
        invoice_id: invoice_id2,
        investor: b.clone(),
        allowed: 1,
    }
    .to_xdr(&env, &contract_id2);

    assert_eq!(single_b, expected_b);
    assert_eq!(batch_b, expected_b_batch);
}

// ---------------------------------------------------------------------------
// 16. Storage layer verification
// ---------------------------------------------------------------------------

/// AllowlistActive is stored in **instance** storage.
#[test]
fn test_allowlist_active_stored_in_instance_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, "ALINV030");
    let contract_id = client.address.clone();

    client.set_allowlist_active(&true, &0u32);

    env.as_contract(&contract_id, || {
        assert_eq!(
            env.storage()
                .instance()
                .get::<DataKey, bool>(&DataKey::AllowlistActive),
            Some(true),
            "AllowlistActive must live in instance storage"
        );
    });
}

/// InvestorAllowlisted entries are stored in **persistent** storage, not instance.
#[test]
fn test_investor_allowlisted_stored_in_persistent_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client, "ALINV031");
    let contract_id = client.address.clone();
    let investor = Address::generate(&env);

    client.set_investor_allowlisted(&investor, &true, &0u32);

    env.as_contract(&contract_id, || {
        // Must be in persistent.
        assert_eq!(
            env.storage()
                .persistent()
                .get::<DataKey, bool>(&DataKey::InvestorAllowlisted(investor.clone())),
            Some(true),
            "InvestorAllowlisted must live in persistent storage"
        );
        // Must NOT be in instance.
        assert!(
            !env.storage()
                .instance()
                .has(&DataKey::InvestorAllowlisted(investor.clone())),
            "InvestorAllowlisted must not be in instance storage"
        );
    });
}

// ---------------------------------------------------------------------------
// 17. InvestorNotAllowlisted error code is stable (regression guard)
// ---------------------------------------------------------------------------

/// `InvestorNotAllowlisted` must have the stable numeric code 104.
///
/// This is a regression guard: SDKs and integrations branch on the numeric
/// code.  Changing it would silently break downstream error handling.
#[test]
fn test_investor_not_allowlisted_error_code_is_104() {
    assert_eq!(
        EscrowError::InvestorNotAllowlisted as u32,
        104,
        "InvestorNotAllowlisted error code must remain 104"
    );
}

/// `InvestorBatchEmpty` must have the stable numeric code 70.
#[test]
fn test_investor_batch_empty_error_code_is_70() {
    assert_eq!(EscrowError::InvestorBatchEmpty as u32, 70);
}

/// `InvestorBatchTooLarge` must have the stable numeric code 71.
#[test]
fn test_investor_batch_too_large_error_code_is_71() {
    assert_eq!(EscrowError::InvestorBatchTooLarge as u32, 71);
}
