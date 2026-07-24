use super::{
    AllowlistEnabledChanged, DataKey, EscrowError, InvestorAllowlistChanged, LiquifactEscrow,
    LiquifactEscrowClient,
};
use soroban_sdk::Vec as SorobanVec;
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Error, Event, InvokeError};
use std::fmt::Debug;

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
        &None,
        &None,
        &None::<i64>,
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

// =============================================================================
// Funding-gate enforcement matrix
//
// The tests above cover setter behaviour and basic should_panic guards.
// This section adds **typed-error** assertions for every gate combination so
// that:
//   - Both `fund` and `fund_with_commitment` are verified to return the exact
//     error code `InvestorNotAllowlisted` (104) when the gate blocks them.
//   - Successful paths return the correct funded_amount.
//   - Revocation mid-funding is validated: a prior contribution does not
//     exempt an investor from the gate on their next deposit.
//   - Disabling the allowlist after enabling it lets any address fund,
//     regardless of whether they have an entry.
// =============================================================================

/// Assert that a `try_*` client call returns the expected typed contract error.
///
/// Works for both `Err(Ok(Error))` (normal SDK encoding) and
/// `Err(Err(InvokeError::Contract(code)))` (raw host encoding) to stay
/// resilient across Soroban SDK minor version differences.
fn assert_contract_error_gate<T, E>(
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
// Helper: initialise an escrow backed by a fresh address-stub token.
// Returns (admin, sme).  The caller has already called `env.mock_all_auths()`.
// ---------------------------------------------------------------------------
fn init_gate(env: &Env, client: &LiquifactEscrowClient) -> (Address, Address) {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "GATEINV01"),
        &sme,
        &100_000i128,
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
        &None::<i64>,
    );
    (admin, sme)
}

// ---------------------------------------------------------------------------
// Gate matrix: fund
// ---------------------------------------------------------------------------

/// Gate inactive, no entry → fund succeeds (gate bypassed).
#[test]
fn gate_fund_allowlist_inactive_no_entry_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);
    // Allowlist is off by default — no entry required.
    let escrow = client.fund(&investor, &1_000i128);
    assert_eq!(escrow.funded_amount, 1_000i128);
}

/// Gate inactive, entry explicitly set to false → fund still succeeds (gate bypassed).
#[test]
fn gate_fund_allowlist_inactive_entry_false_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);
    // Write an explicit `false` entry while gate is off — should have no effect.
    client.set_investor_allowlisted(&investor, &false);
    let escrow = client.fund(&investor, &2_000i128);
    assert_eq!(escrow.funded_amount, 2_000i128);
}

/// Gate active, investor allowlisted → fund succeeds.
#[test]
fn gate_fund_allowlist_active_investor_allowed_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);
    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &true);

    let escrow = client.fund(&investor, &5_000i128);
    assert_eq!(escrow.funded_amount, 5_000i128);
}

/// Gate active, investor NOT allowlisted (absent entry) → fund returns `InvestorNotAllowlisted`.
#[test]
fn gate_fund_allowlist_active_investor_absent_returns_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);
    client.set_allowlist_active(&true);
    // No call to set_investor_allowlisted — entry is absent → default-to-deny.

    assert_contract_error_gate(
        client.try_fund(&investor, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );
}

/// Gate active, investor explicitly set to false → fund returns `InvestorNotAllowlisted`.
#[test]
fn gate_fund_allowlist_active_investor_denied_returns_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);
    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &false);

    assert_contract_error_gate(
        client.try_fund(&investor, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );
}

// ---------------------------------------------------------------------------
// Gate matrix: fund_with_commitment
// ---------------------------------------------------------------------------

/// Gate inactive, no entry → fund_with_commitment succeeds.
#[test]
fn gate_fwc_allowlist_inactive_no_entry_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);
    let escrow = client.fund_with_commitment(&investor, &3_000i128, &0u64);
    assert_eq!(escrow.funded_amount, 3_000i128);
}

/// Gate active, investor allowlisted → fund_with_commitment succeeds.
#[test]
fn gate_fwc_allowlist_active_investor_allowed_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);
    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &true);

    let escrow = client.fund_with_commitment(&investor, &4_000i128, &0u64);
    assert_eq!(escrow.funded_amount, 4_000i128);
}

/// Gate active, investor NOT allowlisted (absent) → fund_with_commitment returns
/// `InvestorNotAllowlisted`.
#[test]
fn gate_fwc_allowlist_active_investor_absent_returns_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);
    client.set_allowlist_active(&true);

    assert_contract_error_gate(
        client.try_fund_with_commitment(&investor, &1_000i128, &0u64),
        EscrowError::InvestorNotAllowlisted,
    );
}

/// Gate active, investor explicitly denied → fund_with_commitment returns
/// `InvestorNotAllowlisted`.
#[test]
fn gate_fwc_allowlist_active_investor_denied_returns_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);
    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &false);

    assert_contract_error_gate(
        client.try_fund_with_commitment(&investor, &1_000i128, &0u64),
        EscrowError::InvestorNotAllowlisted,
    );
}

// ---------------------------------------------------------------------------
// Toggle mid-funding
// ---------------------------------------------------------------------------

/// Disabling the gate after enabling it allows a previously-blocked investor
/// to fund without any allowlist entry.
#[test]
fn gate_disable_mid_funding_unblocks_any_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);

    client.set_allowlist_active(&true);
    // Confirm investor is blocked while gate is active.
    assert_contract_error_gate(
        client.try_fund(&investor, &500i128),
        EscrowError::InvestorNotAllowlisted,
    );

    // Disable gate — investor should now succeed without an entry.
    client.set_allowlist_active(&false);
    let escrow = client.fund(&investor, &500i128);
    assert_eq!(escrow.funded_amount, 500i128);
}

/// Re-enabling the gate after disabling it blocks investors without entries again.
#[test]
fn gate_reenable_after_disable_blocks_unenrolled_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);

    // Gate off → fund succeeds.
    let escrow = client.fund(&investor, &1_000i128);
    assert_eq!(escrow.funded_amount, 1_000i128);

    // Enable gate without allowlisting this investor.
    client.set_allowlist_active(&true);

    // Second deposit blocked — prior contribution does not exempt the investor.
    assert_contract_error_gate(
        client.try_fund(&investor, &500i128),
        EscrowError::InvestorNotAllowlisted,
    );
}

// ---------------------------------------------------------------------------
// Revocation mid-funding
// ---------------------------------------------------------------------------

/// Security invariant: revoking an investor mid-funding prevents their next
/// deposit even though they have an existing contribution.
///
/// A prior contribution does NOT grant a bypass — the gate checks the current
/// allowlist status on every call, not historical access.
#[test]
fn gate_revoke_mid_funding_blocks_next_deposit_fund() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);

    // Step 1: allowlist active, investor allowed — first deposit succeeds.
    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &true);
    let after_first = client.fund(&investor, &3_000i128);
    assert_eq!(after_first.funded_amount, 3_000i128);
    assert_eq!(client.get_contribution(&investor), 3_000i128);

    // Step 2: admin revokes the investor.
    client.set_investor_allowlisted(&investor, &false);
    assert!(!client.is_investor_allowlisted(&investor));

    // Step 3: second deposit must be rejected with InvestorNotAllowlisted.
    assert_contract_error_gate(
        client.try_fund(&investor, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );

    // Contribution must not have changed.
    assert_eq!(client.get_contribution(&investor), 3_000i128);
}

/// Same revocation invariant for `fund_with_commitment`.  Because
/// `fund_with_commitment` is first-deposit-only, this test verifies that
/// a revoked investor is blocked before they can make even their first tiered
/// deposit after the gate is re-enabled.
#[test]
fn gate_revoke_before_first_fwc_deposit_blocks_it() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);

    // Allowlist investor, then revoke before they deposit.
    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &true);
    client.set_investor_allowlisted(&investor, &false);

    assert_contract_error_gate(
        client.try_fund_with_commitment(&investor, &5_000i128, &0u64),
        EscrowError::InvestorNotAllowlisted,
    );
    assert_eq!(client.get_contribution(&investor), 0i128);
}

// ---------------------------------------------------------------------------
// Multiple-investor independence
// ---------------------------------------------------------------------------

/// Allowlisting investor A does not affect investor B.  Both are independently
/// gated, and investor C (never added) is rejected.
#[test]
fn gate_multiple_investors_independent_gating() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);

    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&a, &true);
    client.set_investor_allowlisted(&b, &true);
    // c is never added.

    // A and B succeed.
    let after_a = client.fund(&a, &2_000i128);
    assert_eq!(after_a.funded_amount, 2_000i128);
    let after_b = client.fund(&b, &3_000i128);
    assert_eq!(after_b.funded_amount, 5_000i128);

    // C is rejected.
    assert_contract_error_gate(
        client.try_fund(&c, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );

    // Contributions of A and B are unchanged after C's rejection.
    assert_eq!(client.get_contribution(&a), 2_000i128);
    assert_eq!(client.get_contribution(&b), 3_000i128);
    assert_eq!(client.get_contribution(&c), 0i128);
}

// ---------------------------------------------------------------------------
// Batch allowlist + gate interaction
// ---------------------------------------------------------------------------

/// Batch-allowlisting investors and then enabling the gate lets every batch
/// member fund; addresses outside the batch are rejected.
#[test]
fn gate_batch_allowlist_then_gate_active_correct_access() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let outsider = Address::generate(&env);

    let mut batch: SorobanVec<Address> = SorobanVec::new(&env);
    batch.push_back(a.clone());
    batch.push_back(b.clone());

    client.set_allowlist_active(&true);
    client.set_investors_allowlisted(&batch, &true);

    // Batch members succeed.
    client.fund(&a, &1_000i128);
    client.fund(&b, &1_000i128);

    // Outsider is rejected.
    assert_contract_error_gate(
        client.try_fund(&outsider, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );
}

/// Batch-revoking investors while the gate is active immediately blocks them.
#[test]
fn gate_batch_revoke_blocks_all_revoked_members() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let a = Address::generate(&env);
    let b = Address::generate(&env);

    let mut batch: SorobanVec<Address> = SorobanVec::new(&env);
    batch.push_back(a.clone());
    batch.push_back(b.clone());

    client.set_allowlist_active(&true);
    client.set_investors_allowlisted(&batch, &true);

    // Fund once successfully.
    client.fund(&a, &1_000i128);
    client.fund(&b, &1_000i128);

    // Batch-revoke both.
    client.set_investors_allowlisted(&batch, &false);

    // Both are now blocked.
    assert_contract_error_gate(
        client.try_fund(&a, &500i128),
        EscrowError::InvestorNotAllowlisted,
    );
    assert_contract_error_gate(
        client.try_fund(&b, &500i128),
        EscrowError::InvestorNotAllowlisted,
    );

    // Contributions remain at the pre-revocation values.
    assert_eq!(client.get_contribution(&a), 1_000i128);
    assert_eq!(client.get_contribution(&b), 1_000i128);
}

// ---------------------------------------------------------------------------
// guard_investor_allowlisted behaviour — exercised through every funding
// entrypoint to verify the extracted helper produces identical rejections.
// ---------------------------------------------------------------------------

/// `fund_batch` with gate active: allowed investor succeeds, unallowlisted
/// investor in the same batch is rejected.  Verifies the guard is applied
/// per-entry inside `fund_impl`, which `fund_batch` delegates to.
#[test]
fn guard_helper_fund_batch_allowed_and_blocked_in_same_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let allowed = Address::generate(&env);
    let blocked = Address::generate(&env);

    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&allowed, &true);
    // `blocked` is never added → absent entry defaults to deny.

    let mut batch: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    batch.push_back((allowed.clone(), 1_000i128));
    batch.push_back((blocked.clone(), 2_000i128));

    // The batch must be rejected atomically — no partial state.
    assert_contract_error_gate(
        client.try_fund_batch(&batch),
        EscrowError::InvestorNotAllowlisted,
    );
    assert_eq!(client.get_contribution(&allowed), 0i128);
    assert_eq!(client.get_contribution(&blocked), 0i128);
}

/// `fund_batch` with gate active: all investors allowlisted → entire batch
/// succeeds.
#[test]
fn guard_helper_fund_batch_all_allowed_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let a = Address::generate(&env);
    let b = Address::generate(&env);

    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&a, &true);
    client.set_investor_allowlisted(&b, &true);

    let mut batch: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    batch.push_back((a.clone(), 3_000i128));
    batch.push_back((b.clone(), 4_000i128));

    let escrow = client.fund_batch(&batch);
    assert_eq!(escrow.funded_amount, 7_000i128);
    assert_eq!(client.get_contribution(&a), 3_000i128);
    assert_eq!(client.get_contribution(&b), 4_000i128);
}

/// `fund_batch` with gate active: none allowlisted → first entry rejected.
#[test]
fn guard_helper_fund_batch_all_blocked_rejects() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let a = Address::generate(&env);
    let b = Address::generate(&env);

    client.set_allowlist_active(&true);
    // Neither a nor b is added.

    let mut batch: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    batch.push_back((a.clone(), 1_000i128));
    batch.push_back((b.clone(), 1_000i128));

    assert_contract_error_gate(
        client.try_fund_batch(&batch),
        EscrowError::InvestorNotAllowlisted,
    );
}

/// `fund_batch` with gate inactive: mixed entries all succeed (gate bypassed).
#[test]
fn guard_helper_fund_batch_gate_inactive_all_succeed() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let a = Address::generate(&env);
    let b = Address::generate(&env);

    // Gate is off by default — no allowlist setup needed.
    let mut batch: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    batch.push_back((a.clone(), 5_000i128));
    batch.push_back((b.clone(), 6_000i128));

    let escrow = client.fund_batch(&batch);
    assert_eq!(escrow.funded_amount, 11_000i128);
}

/// Verify the helper produces the identical typed error code (104) through
/// each of the three funding entrypoints when the gate blocks.
#[test]
fn guard_helper_same_error_code_across_all_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);
    client.set_allowlist_active(&true);
    // Investor is absent from the allowlist.

    // 1. fund
    assert_contract_error_gate(
        client.try_fund(&investor, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );

    // 2. fund_with_commitment
    assert_contract_error_gate(
        client.try_fund_with_commitment(&investor, &1_000i128, &0u64),
        EscrowError::InvestorNotAllowlisted,
    );

    // 3. fund_batch (single entry)
    let mut batch: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    batch.push_back((investor.clone(), 1_000i128));
    assert_contract_error_gate(
        client.try_fund_batch(&batch),
        EscrowError::InvestorNotAllowlisted,
    );
}

/// Toggling the gate off between two `fund` calls from the same investor
/// verifies that the helper re-evaluates on every invocation (no cached state).
#[test]
fn guard_helper_re_evaluates_on_every_call() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);

    // Gate on, investor not on list → rejected.
    client.set_allowlist_active(&true);
    assert_contract_error_gate(
        client.try_fund(&investor, &1_000i128),
        EscrowError::InvestorNotAllowlisted,
    );

    // Add investor, gate still on → allowed.
    client.set_investor_allowlisted(&investor, &true);
    let escrow = client.fund(&investor, &1_000i128);
    assert_eq!(escrow.funded_amount, 1_000i128);

    // Revoke investor, gate still on → rejected again.
    client.set_investor_allowlisted(&investor, &false);
    assert_contract_error_gate(
        client.try_fund(&investor, &500i128),
        EscrowError::InvestorNotAllowlisted,
    );
}

/// Verify the guard helper rejects the explicitly-denied investor
/// (entry set to `false`) with the same typed error, through
/// `fund_with_commitment`.
#[test]
fn guard_helper_fwc_explicitly_denied_rejects() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);
    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &false);

    assert_contract_error_gate(
        client.try_fund_with_commitment(&investor, &2_000i128, &0u64),
        EscrowError::InvestorNotAllowlisted,
    );
    assert_eq!(client.get_contribution(&investor), 0i128);
}

/// Verify contribution is unchanged when the guard blocks in `fund_batch`.
#[test]
fn guard_helper_fund_batch_contribution_unchanged_on_rejection() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_gate(&env, &client);

    let investor = Address::generate(&env);
    client.set_allowlist_active(&true);

    let mut batch: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    batch.push_back((investor.clone(), 5_000i128));

    assert_contract_error_gate(
        client.try_fund_batch(&batch),
        EscrowError::InvestorNotAllowlisted,
    );
    assert_eq!(client.get_contribution(&investor), 0i128);
}
