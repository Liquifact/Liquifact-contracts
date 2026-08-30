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
        &None::<u32>,
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

// =============================================================================
// Allowlist-limit feature tests
//
// Coverage:
//   • DEFAULT_ALLOWLIST_LIMIT returned by get_allowlist_limit before any set
//   • In-bounds set (at MIN, at MAX, somewhere in the middle)
//   • Over-bounds set rejected (> MAX_ALLOWLIST_LIMIT)  → AllowlistLimitOutOfRange
//   • Under-bounds set rejected (0, i.e. < MIN_ALLOWLIST_LIMIT) → AllowlistLimitOutOfRange
//   • Non-admin caller rejected
//   • set_investor_allowlisted enforces limit
//   • set_investors_allowlisted enforces limit across a batch
//   • Removing an investor frees a slot (re-adding succeeds after removal)
//   • Re-allowlisting an already-allowlisted investor is idempotent (no double-count)
//   • Limit can be lowered after being raised (existing overcount is NOT evicted;
//     new additions are blocked until enough removals bring count below the new limit)
// =============================================================================

use super::{DEFAULT_ALLOWLIST_LIMIT, MAX_ALLOWLIST_LIMIT, MIN_ALLOWLIST_LIMIT};

// ---------------------------------------------------------------------------
// get_allowlist_limit — defaults
// ---------------------------------------------------------------------------

/// Before any admin call the getter must return DEFAULT_ALLOWLIST_LIMIT.
#[test]
fn allowlist_limit_default_is_returned_before_any_set() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);
    assert_eq!(client.get_allowlist_limit(), DEFAULT_ALLOWLIST_LIMIT);
}

// ---------------------------------------------------------------------------
// set_allowlist_limit — in-bounds
// ---------------------------------------------------------------------------

/// Setting the limit to MIN_ALLOWLIST_LIMIT succeeds and is immediately readable.
#[test]
fn allowlist_limit_set_to_min_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    client.set_allowlist_limit(&MIN_ALLOWLIST_LIMIT);
    assert_eq!(client.get_allowlist_limit(), MIN_ALLOWLIST_LIMIT);
}

/// Setting the limit to MAX_ALLOWLIST_LIMIT succeeds and is immediately readable.
#[test]
fn allowlist_limit_set_to_max_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    client.set_allowlist_limit(&MAX_ALLOWLIST_LIMIT);
    assert_eq!(client.get_allowlist_limit(), MAX_ALLOWLIST_LIMIT);
}

/// Setting the limit to a middle value (e.g. 50) succeeds and is readable.
#[test]
fn allowlist_limit_set_to_midrange_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    client.set_allowlist_limit(&50u32);
    assert_eq!(client.get_allowlist_limit(), 50u32);
}

/// The limit can be updated multiple times; the last written value wins.
#[test]
fn allowlist_limit_can_be_updated_multiple_times() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    client.set_allowlist_limit(&100u32);
    assert_eq!(client.get_allowlist_limit(), 100u32);

    client.set_allowlist_limit(&5u32);
    assert_eq!(client.get_allowlist_limit(), 5u32);

    client.set_allowlist_limit(&MAX_ALLOWLIST_LIMIT);
    assert_eq!(client.get_allowlist_limit(), MAX_ALLOWLIST_LIMIT);
}

// ---------------------------------------------------------------------------
// set_allowlist_limit — out-of-range rejection
// ---------------------------------------------------------------------------

/// Setting the limit to 0 (below MIN) is rejected with AllowlistLimitOutOfRange.
#[test]
fn allowlist_limit_zero_rejected_with_typed_error() {
    // TODO: implement test body
}

#[test]
fn test_allowlist_metadata_defaults_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let metadata = client.get_allowlist_metadata();

    assert_eq!(metadata.schema_version, 0);
    assert!(!metadata.is_active);
    assert_eq!(metadata.allowlisted_count, 0);
}

#[test]
fn test_allowlist_metadata_reflects_initialized_values() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    assert_contract_error_gate(
        client.try_set_allowlist_limit(&0u32),
        EscrowError::AllowlistLimitOutOfRange,
    );
    // Unchanged.
    assert_eq!(client.get_allowlist_limit(), DEFAULT_ALLOWLIST_LIMIT);
}

/// Setting the limit to MAX + 1 is rejected with AllowlistLimitOutOfRange.
#[test]
fn allowlist_limit_above_max_rejected_with_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let too_large = MAX_ALLOWLIST_LIMIT + 1;
    assert_contract_error_gate(
        client.try_set_allowlist_limit(&too_large),
        EscrowError::AllowlistLimitOutOfRange,
    );
    // Unchanged.
    assert_eq!(client.get_allowlist_limit(), DEFAULT_ALLOWLIST_LIMIT);
}

/// u32::MAX is rejected with AllowlistLimitOutOfRange.
#[test]
fn allowlist_limit_u32_max_rejected_with_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    assert_contract_error_gate(
        client.try_set_allowlist_limit(&u32::MAX),
        EscrowError::AllowlistLimitOutOfRange,
    );
}

// ---------------------------------------------------------------------------
// set_allowlist_limit — authorization
// ---------------------------------------------------------------------------

/// A non-admin caller must not be able to set the allowlist limit.
#[test]
fn allowlist_limit_set_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    // Strip all mocked auths so the call is genuinely unauthorised.
    env.mock_auths(&[]);
    let result = client.try_set_allowlist_limit(&5u32);
    assert!(
        result.is_err(),
        "Expected auth failure but set_allowlist_limit succeeded without admin auth"
    );
}

// ---------------------------------------------------------------------------
// Capacity enforcement — single investor
// ---------------------------------------------------------------------------

/// When the limit is 1, only one investor can be allowlisted; a second addition
/// is rejected with AllowlistCapacityReached.
#[test]
fn allowlist_capacity_limit_1_blocks_second_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    client.set_allowlist_limit(&1u32);

    let a = Address::generate(&env);
    let b = Address::generate(&env);

    client.set_investor_allowlisted(&a, &true);
    assert!(client.is_investor_allowlisted(&a));

    assert_contract_error_gate(
        client.try_set_investor_allowlisted(&b, &true),
        EscrowError::AllowlistCapacityReached,
    );
    assert!(!client.is_investor_allowlisted(&b));
}

/// The allowlist index length equals the limit exactly; adding one more address
/// is rejected with AllowlistCapacityReached.
#[test]
fn allowlist_capacity_at_exact_limit_blocks_next_addition() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let limit = 3u32;
    client.set_allowlist_limit(&limit);

    for _ in 0..limit {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
    }

    let extra = Address::generate(&env);
    assert_contract_error_gate(
        client.try_set_investor_allowlisted(&extra, &true),
        EscrowError::AllowlistCapacityReached,
    );
}

/// Re-allowlisting an address that is already allowlisted is idempotent: it does
/// not consume an additional slot and succeeds even when the list is full.
#[test]
fn allowlist_readd_already_allowlisted_is_idempotent_and_does_not_count_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    client.set_allowlist_limit(&1u32);

    let a = Address::generate(&env);
    client.set_investor_allowlisted(&a, &true);

    // Re-adding should not fail, even though the list is at capacity.
    client.set_investor_allowlisted(&a, &true);
    assert!(client.is_investor_allowlisted(&a));
}

/// Removing an investor frees a slot; the next addition succeeds.
#[test]
fn allowlist_removal_frees_slot_for_new_entry() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    client.set_allowlist_limit(&1u32);

    let a = Address::generate(&env);
    let b = Address::generate(&env);

    client.set_investor_allowlisted(&a, &true);

    // Adding b fails — at capacity.
    assert_contract_error_gate(
        client.try_set_investor_allowlisted(&b, &true),
        EscrowError::AllowlistCapacityReached,
    );

    // Remove a — slot freed.
    client.set_investor_allowlisted(&a, &false);

    // Now b can be added.
    client.set_investor_allowlisted(&b, &true);
    assert!(client.is_investor_allowlisted(&b));
    assert!(!client.is_investor_allowlisted(&a));
}

// ---------------------------------------------------------------------------
// Capacity enforcement — batch
// ---------------------------------------------------------------------------

/// A batch that would push the allowlisted count past the limit is rejected
/// atomically with AllowlistCapacityReached.
#[test]
fn allowlist_batch_rejected_when_exceeds_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    // Limit = 2; pre-fill 1 slot; batch of 2 would take total to 3 → reject.
    client.set_allowlist_limit(&2u32);

    let existing = Address::generate(&env);
    client.set_investor_allowlisted(&existing, &true);

    let new_a = Address::generate(&env);
    let new_b = Address::generate(&env);
    let mut batch: SorobanVec<Address> = SorobanVec::new(&env);
    batch.push_back(new_a.clone());
    batch.push_back(new_b.clone());

    assert_contract_error_gate(
        client.try_set_investors_allowlisted(&batch, &true),
        EscrowError::AllowlistCapacityReached,
    );
}

/// A batch that exactly fills up to the limit succeeds.
#[test]
fn allowlist_batch_exactly_filling_limit_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let limit = 3u32;
    client.set_allowlist_limit(&limit);

    let mut batch: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..limit {
        batch.push_back(Address::generate(&env));
    }

    // Exactly at limit — should succeed.
    client.set_investors_allowlisted(&batch, &true);
    assert_eq!(client.get_allowlist_limit(), limit);
}

/// Batch-revoking investors does not count against the limit; after removal the
/// freed slots are available.
#[test]
fn allowlist_batch_revoke_frees_slots() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    client.set_allowlist_limit(&2u32);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);

    let mut batch_ab: SorobanVec<Address> = SorobanVec::new(&env);
    batch_ab.push_back(a.clone());
    batch_ab.push_back(b.clone());

    // Fill to limit.
    client.set_investors_allowlisted(&batch_ab, &true);

    // Revoke a — frees a slot.
    let mut batch_a: SorobanVec<Address> = SorobanVec::new(&env);
    batch_a.push_back(a.clone());
    client.set_investors_allowlisted(&batch_a, &false);

    // c can now be added.
    let mut batch_c: SorobanVec<Address> = SorobanVec::new(&env);
    batch_c.push_back(c.clone());
    client.set_investors_allowlisted(&batch_c, &true);

    assert!(!client.is_investor_allowlisted(&a));
    assert!(client.is_investor_allowlisted(&b));
    assert!(client.is_investor_allowlisted(&c));
}

// ---------------------------------------------------------------------------
// Lowering the limit does not evict existing entries
// ---------------------------------------------------------------------------

/// Lowering the limit below the current count does not remove existing entries —
/// but new additions are blocked.
#[test]
fn allowlist_lowering_limit_does_not_evict_but_blocks_new_additions() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    client.set_allowlist_limit(&3u32);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let d = Address::generate(&env);

    client.set_investor_allowlisted(&a, &true);
    client.set_investor_allowlisted(&b, &true);
    client.set_investor_allowlisted(&c, &true);

    // Lower the limit to 1.
    client.set_allowlist_limit(&1u32);

    // Existing entries are untouched.
    assert!(client.is_investor_allowlisted(&a));
    assert!(client.is_investor_allowlisted(&b));
    assert!(client.is_investor_allowlisted(&c));

    // New addition is blocked (3 entries >= new limit of 1).
    assert_contract_error_gate(
        client.try_set_investor_allowlisted(&d, &true),
        EscrowError::AllowlistCapacityReached,
    );
    let investor = Address::generate(&env);
    let removed = Address::generate(&env);

    client.set_allowlist_active(&true);
    client.set_investor_allowlisted(&investor, &true);
    client.set_investor_allowlisted(&removed, &true);
    client.set_investor_allowlisted(&removed, &false);

    let metadata = client.get_allowlist_metadata();

    assert_eq!(metadata.schema_version, crate::SCHEMA_VERSION);
    assert!(metadata.is_active);
    assert_eq!(metadata.allowlisted_count, 1);
}

// =============================================================================
// Allowlist arithmetic safety — overflow / saturation tests
//
// Background
// ----------
// `get_allowlisted_investors` computes the pagination window with
//
//   let actual_limit = limit.min(50);
//   let end = start.saturating_add(actual_limit).min(len);
//
// Prior to the fix the expression was `start + actual_limit` (plain u32 add),
// which panics in debug builds and wraps silently in release builds when
// `start` is near `u32::MAX`.  These tests pin the *correct* post-fix
// behaviour, and also cover the arithmetic in:
//
//   • `get_allowlisted_investors_count`  — counts over the AllowlistIndex Vec;
//     safe because Vec::len() returns u32 and the counter never exceeds len.
//   • `set_investors_allowlisted`        — batch size bound (u32 ≤ MAX=32);
//     the only arithmetic is the positivity/ceiling check, no addition.
//   • metadata aggregation              — `allowlisted_count` is a read-only
//     delegate to `get_allowlisted_investors_count`; same safety bound applies.
//
// Test nomenclature
// -----------------
//   overflow_*   — previously-crashing path; must now return an empty Vec
//   saturation_* — start very close to u32::MAX; must clamp correctly
//   count_*      — cover the increment loop in get_allowlisted_investors_count
//   batch_*      — cover arithmetic in set_investors_allowlisted batch path
// =============================================================================

// ── get_allowlisted_investors: extreme start values ──────────────────────────

/// `start = u32::MAX`, non-zero limit, empty index → returns empty (start >= len = 0).
///
/// The early-exit guard `if start >= len` fires before the addition, so no
/// overflow occurs even in the degenerate all-zero case.
#[test]
fn overflow_get_allowlisted_investors_max_start_empty_index() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let result = client.get_allowlisted_investors(&u32::MAX, &50);
    assert_eq!(result.len(), 0, "must return empty when start >= len (0)");
}

/// `start = u32::MAX`, limit = 1, one investor in index → `start >= len = 1`,
/// so the guard fires before any addition.  Must return empty.
#[test]
fn overflow_get_allowlisted_investors_max_start_one_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let inv = Address::generate(&env);
    client.set_investor_allowlisted(&inv, &true);

    // len = 1; start = u32::MAX >= 1 → early-exit fires, no addition.
    let result = client.get_allowlisted_investors(&u32::MAX, &1);
    assert_eq!(
        result.len(),
        0,
        "must return empty; start past end of index"
    );
}

/// `start = u32::MAX - 1`, limit = 50 (actual_limit also 50).
/// Saturating add: (u32::MAX - 1).saturating_add(50) = u32::MAX (saturated).
/// .min(len = 1) → end = 1, but start (u32::MAX - 1) >= 1, so early-exit fires.
/// Must return empty, not panic.
#[test]
fn saturation_get_allowlisted_investors_near_max_start() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let inv = Address::generate(&env);
    client.set_investor_allowlisted(&inv, &true);

    // start = u32::MAX - 1 ≥ len = 1 → early-exit.
    let result = client.get_allowlisted_investors(&(u32::MAX - 1), &50);
    assert_eq!(
        result.len(),
        0,
        "start past len; must return empty without overflow"
    );
}

/// `start = u32::MAX`, limit = u32::MAX → actual_limit = 50 (ceiling enforced).
/// Still start >= any realistic len, so must return empty.
#[test]
fn saturation_get_allowlisted_investors_max_start_max_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let result = client.get_allowlisted_investors(&u32::MAX, &u32::MAX);
    assert_eq!(
        result.len(),
        0,
        "both start and limit at u32::MAX must not overflow"
    );
}

/// `start = 0`, limit = u32::MAX → `actual_limit` is clamped to 50 (ceiling).
/// With 3 investors: end = 0.saturating_add(50).min(3) = 3.  Must return all 3.
#[test]
fn saturation_get_allowlisted_investors_zero_start_max_limit_clamped() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    for _ in 0..3 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
    }

    let result = client.get_allowlisted_investors(&0, &u32::MAX);
    assert_eq!(
        result.len(),
        3,
        "ceiling should clamp limit; all 3 investors returned"
    );
}

/// start close to but still within the valid range.
///
/// With 5 investors in the index, start = 4 (last valid), limit = u32::MAX → actual_limit = 50.
/// saturating_add(4, 50) = 54, min(5) = 5.  Loop runs `for i in 4..5` → returns item[4].
#[test]
fn saturation_get_allowlisted_investors_last_valid_start() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let mut addrs = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
        addrs.push_back(addr);
    }

    // start = 4, effective limit = 50 → window [4, min(54, 5)) = [4, 5)
    let result = client.get_allowlisted_investors(&4, &u32::MAX);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get(0).unwrap(), addrs.get(4).unwrap());
}

/// Ceiling enforcement: limit = 51 (just above the 50-ceiling) → actual_limit = 50.
/// With 60 investors, start = 0: window [0, 50).  Must return exactly 50.
#[test]
fn saturation_get_allowlisted_investors_limit_above_ceiling_clamped() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    // Add 60 investors
    for _ in 0..60 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
    }

    let result = client.get_allowlisted_investors(&0, &51);
    assert_eq!(
        result.len(),
        50,
        "limit above ceiling must be clamped to 50"
    );

    // Also test limit = 50 exactly — must also return 50
    let result2 = client.get_allowlisted_investors(&0, &50);
    assert_eq!(result2.len(), 50, "limit == ceiling must return 50");
}

// ── get_allowlisted_investors_count: count increment safety ──────────────────

/// count += 1 is safe for a small set; verify the counter is accurate for every
/// intermediate size from 0 to 5.
#[test]
fn count_get_allowlisted_investors_count_increments_correctly() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let count_before = client.get_allowlisted_investors_count();
    assert_eq!(count_before, 0, "count must be 0 before any allowlisting");

    let mut addrs = Vec::new();
    for i in 1u32..=5 {
        let addr = Address::generate(&env);
        addrs.push(addr.clone());
        client.set_investor_allowlisted(&addr, &true);
        assert_eq!(
            client.get_allowlisted_investors_count(),
            i,
            "count must equal number of distinct allowlisted addresses"
        );
    }
}

/// Revoking an investor reduces the count by exactly 1.
#[test]
fn count_get_allowlisted_investors_count_decrements_on_revoke() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);

    client.set_investor_allowlisted(&a, &true);
    client.set_investor_allowlisted(&b, &true);
    client.set_investor_allowlisted(&c, &true);
    assert_eq!(client.get_allowlisted_investors_count(), 3);

    client.set_investor_allowlisted(&b, &false);
    assert_eq!(
        client.get_allowlisted_investors_count(),
        2,
        "revoke must decrement count"
    );

    client.set_investor_allowlisted(&a, &false);
    assert_eq!(client.get_allowlisted_investors_count(), 1);

    client.set_investor_allowlisted(&c, &false);
    assert_eq!(
        client.get_allowlisted_investors_count(),
        0,
        "all revoked → count must be 0"
    );
}

/// Re-allowlisting an already-allowlisted investor must not double-count.
#[test]
fn count_re_allowlist_idempotent_count() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let inv = Address::generate(&env);
    client.set_investor_allowlisted(&inv, &true);
    assert_eq!(client.get_allowlisted_investors_count(), 1);

    // Setting the same address to true again: count must remain 1.
    // Note: the index only appends when was_allowlisted == false, so a double
    // set_true call is a no-op on the index.
    client.set_investor_allowlisted(&inv, &true);
    assert_eq!(
        client.get_allowlisted_investors_count(),
        1,
        "idempotent re-allowlisting must not bump the count"
    );
}

/// `get_allowlist_metadata` reflects the same count as
/// `get_allowlisted_investors_count` — both read through the same index.
#[test]
fn count_metadata_allowlisted_count_matches_direct_count() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    for _ in 0..4 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
    }

    let direct = client.get_allowlisted_investors_count();
    let via_metadata = client.get_allowlist_metadata().allowlisted_count;
    assert_eq!(
        direct, via_metadata,
        "metadata.allowlisted_count must equal direct count"
    );
    assert_eq!(direct, 4);
}

// ── set_investors_allowlisted: batch arithmetic ──────────────────────────────

/// Batch of exactly MAX_INVESTOR_ALLOWLIST_BATCH entries succeeds (boundary check).
#[test]
fn batch_allowlist_exact_max_batch_size_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let max = super::MAX_INVESTOR_ALLOWLIST_BATCH as usize;
    let mut v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..max {
        v.push_back(Address::generate(&env));
    }

    // Exactly MAX must succeed.
    client.set_investors_allowlisted(&v, &true);
    assert_eq!(
        client.get_allowlisted_investors_count(),
        super::MAX_INVESTOR_ALLOWLIST_BATCH,
        "all MAX batch members must be allowlisted"
    );
}

/// Batch of MAX + 1 must be rejected with InvestorBatchTooLarge (typed error 101).
#[test]
fn batch_allowlist_one_over_max_batch_returns_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let too_many = super::MAX_INVESTOR_ALLOWLIST_BATCH as usize + 1;
    let mut v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..too_many {
        v.push_back(Address::generate(&env));
    }

    assert_contract_error_gate(
        client.try_set_investors_allowlisted(&v, &true),
        EscrowError::InvestorBatchTooLarge,
    );
    // Count must remain 0 — no partial writes.
    assert_eq!(client.get_allowlisted_investors_count(), 0);
}

/// Empty batch must be rejected with InvestorBatchEmpty (typed error 100).
#[test]
fn batch_allowlist_empty_batch_returns_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);

    assert_contract_error_gate(
        client.try_set_investors_allowlisted(&v, &true),
        EscrowError::InvestorBatchEmpty,
    );
}

/// Batch revoke of MAX entries brings the count from MAX to 0 in a single call.
#[test]
fn batch_revoke_max_entries_count_reaches_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let max = super::MAX_INVESTOR_ALLOWLIST_BATCH as usize;
    let mut v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..max {
        v.push_back(Address::generate(&env));
    }

    client.set_investors_allowlisted(&v, &true);
    assert_eq!(
        client.get_allowlisted_investors_count(),
        super::MAX_INVESTOR_ALLOWLIST_BATCH
    );

    client.set_investors_allowlisted(&v, &false);
    assert_eq!(
        client.get_allowlisted_investors_count(),
        0,
        "batch revoke of all MAX entries must set count to 0"
    );
}

// ── Combined: pagination + arithmetic consistency ────────────────────────────

/// After adding N investors, paginating through all pages must yield exactly N
/// results in total, with no duplicates and no panic at the page boundary.
/// Uses 3 pages of size 5 over 13 investors (final page has only 3 items).
#[test]
fn pagination_full_scan_no_overflow_or_duplicates() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let n: usize = 13;
    let mut inserted = std::collections::HashSet::new();
    for _ in 0..n {
        let addr = Address::generate(&env);
        inserted.insert(addr.to_string());
        client.set_investor_allowlisted(&addr, &true);
    }

    // Paginate with page size 5 → 3 pages (5, 5, 3).
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
            let addr_str = page.get(i).unwrap().to_string();
            assert!(
                collected.insert(addr_str.clone()),
                "duplicate address {addr_str} found in paginated results"
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

/// After adding 5 investors, querying `start = 3` with `limit = u32::MAX`
/// should return exactly 2 items (items 3 and 4) — no panic from the large limit.
#[test]
fn saturation_large_limit_from_mid_page_returns_tail() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let mut addrs = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
        addrs.push_back(addr);
    }

    // start = 3, limit = u32::MAX → actual_limit = 50 → end = min(3+50, 5) = 5
    let result = client.get_allowlisted_investors(&3, &u32::MAX);
    assert_eq!(result.len(), 2, "tail of 2 items returned without overflow");
    assert_eq!(result.get(0).unwrap(), addrs.get(3).unwrap());
    assert_eq!(result.get(1).unwrap(), addrs.get(4).unwrap());
}
