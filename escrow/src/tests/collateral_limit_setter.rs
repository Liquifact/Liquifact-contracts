// Compatibility contracts for the admin-only `set_collateral_limit` setter.
//
// These tests pin the public surface other callers and off-chain indexers depend on:
//
// - the additive-key default (`MAX_INVOICE_AMOUNT` when `DataKey::CollateralLimit` is absent),
//   which is what keeps escrow instances that predate the setter accepting exactly the
//   commitments they accepted before;
// - the accepted range `1..=MAX_INVOICE_AMOUNT`, asserted at both edges and immediately
//   outside them, so a future bound change shows up as a boundary failure rather than a
//   silent behavior change;
// - authorization (admin only) and the fact that a rejected call mutates nothing;
// - the event payload shape, since indexers branch on `old_limit` / `new_limit`;
// - the interaction with `record_sme_collateral_commitment` / `batch_record_collateral`,
//   which is where the ceiling actually takes effect.

use crate::tests::{assert_contract_error, setup};
use crate::{CollateralLimitUpdated, EscrowError, LiquifactEscrowClient, MAX_INVOICE_AMOUNT};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _},
    Address, Env, Event, IntoVal, Symbol, Vec as SorobanVec,
};

/// Initializes a minimal escrow. `init` takes 19 arguments; the trailing `Option`s are
/// passed as explicit `None`s so this stays in step with the contract signature.
fn init_escrow(env: &Env, client: &LiquifactEscrowClient, admin: &Address, sme: &Address) {
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "COLLIM01"),
        sme,
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
        &None::<u32>,
    );
}

fn setup_initialized(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    let (client, admin, sme) = setup(env);
    init_escrow(env, &client, &admin, &sme);
    (client, admin, sme)
}

// --- Compatibility: default value and public shape ---

/// The additive-key default must stay `MAX_INVOICE_AMOUNT` on an instance that never
/// called the setter. This is the ADR-007 guarantee that predates the key.
#[test]
fn default_limit_is_max_invoice_amount_before_any_set() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

/// Tightening the ceiling must not reject a commitment that the default ceiling accepted,
/// proving the stored value (not just the getter) is what enforces the bound.
#[test]
fn legacy_default_still_accepts_amount_up_to_max_invoice_amount() {
    let env = Env::default();
    let (client, _admin, sme) = setup_initialized(&env);

    let asset = Symbol::new(&env, "USDC");
    client.record_sme_collateral_commitment(&asset, &MAX_INVOICE_AMOUNT);

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, MAX_INVOICE_AMOUNT);
    assert_eq!(stored.asset, asset);
    let _ = sme;
}

/// Lowering the ceiling is deliberately permitted: it is an admin risk-tightening lever,
/// not a monotone ratchet. Existing commitments are not retroactively invalidated.
#[test]
fn limit_can_be_lowered_and_raised_repeatedly() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    for limit in [5_000i128, 1i128, 250_000i128, 7i128, MAX_INVOICE_AMOUNT] {
        client.set_collateral_limit(&limit);
        assert_eq!(client.get_collateral_limit(), limit);
    }
}

// --- Success paths ---

#[test]
fn admin_sets_collateral_limit_in_bounds() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    client.set_collateral_limit(&5_000i128);
    assert_eq!(client.get_collateral_limit(), 5_000i128);

    // A second, lower in-bounds update also succeeds.
    client.set_collateral_limit(&1i128);
    assert_eq!(client.get_collateral_limit(), 1i128);
}

#[test]
fn set_collateral_limit_emits_event_with_old_and_new_limit() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract_id = client.address.clone();

    client.set_collateral_limit(&2_000i128);

    let all_events = env.events().all();
    assert_eq!(
        all_events.events().last().unwrap().clone(),
        CollateralLimitUpdated {
            name: symbol_short!("coll_lim"),
            invoice_id: client.get_escrow().invoice_id,
            old_limit: MAX_INVOICE_AMOUNT,
            new_limit: 2_000i128,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Consecutive updates must chain: each event's `old_limit` is the previous `new_limit`.
/// Indexers that reconstruct the ceiling history depend on this.
#[test]
fn consecutive_updates_chain_old_and_new_limits() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    client.set_collateral_limit(&1_000i128);
    client.set_collateral_limit(&4_000i128);
    assert_eq!(client.get_collateral_limit(), 4_000i128);
}

// --- Boundary conditions ---

#[test]
fn boundary_exactly_at_minimum_is_accepted() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    client.set_collateral_limit(&1i128);
    assert_eq!(client.get_collateral_limit(), 1i128);
}

#[test]
fn boundary_exactly_at_max_is_accepted() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    client.set_collateral_limit(&MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

#[test]
fn boundary_just_below_max_is_accepted() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    let just_below_max = MAX_INVOICE_AMOUNT - 1;
    client.set_collateral_limit(&just_below_max);
    assert_eq!(client.get_collateral_limit(), just_below_max);
}

// --- Malformed / out-of-range input ---

#[test]
fn set_collateral_limit_rejects_non_positive() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    for bad in [0i128, -1i128, -100_000i128, i128::MIN] {
        assert_contract_error(
            client.try_set_collateral_limit(&bad),
            EscrowError::CollateralLimitNotPositive,
        );
    }

    // Rejected calls must not change the stored limit.
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

#[test]
fn set_collateral_limit_rejects_exceeding_max_invoice_amount() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    for bad in [MAX_INVOICE_AMOUNT + 1, i128::MAX] {
        assert_contract_error(
            client.try_set_collateral_limit(&bad),
            EscrowError::CollateralLimitExceedsMax,
        );
    }

    // The maximum allowed value itself is accepted.
    client.set_collateral_limit(&MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

/// A rejected update must leave a previously stored limit untouched, not reset it to the
/// default. Guards against writing state before validating.
#[test]
fn rejected_update_preserves_previously_stored_limit() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    client.set_collateral_limit(&4_242i128);
    assert_contract_error(
        client.try_set_collateral_limit(&0i128),
        EscrowError::CollateralLimitNotPositive,
    );
    assert_contract_error(
        client.try_set_collateral_limit(&(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceedsMax,
    );
    assert_eq!(client.get_collateral_limit(), 4_242i128);
}

// --- Authorization ---

#[test]
fn non_admin_cannot_set_collateral_limit() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let non_admin = Address::generate(&env);

    // Only the non-admin authorizes; the escrow admin's signature is absent, so
    // `load_escrow_require_admin` must fail.
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "set_collateral_limit",
            args: SorobanVec::from_array(&env, [1_000i128.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);

    // Authorization failures surface as the host's auth error, not a typed
    // `EscrowError`, so assert that the call failed without pinning a code.
    assert!(
        client.try_set_collateral_limit(&1_000i128).is_err(),
        "a caller that is not the escrow admin must not be able to set the limit"
    );

    // The limit must remain unchanged after the rejected call.
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

// --- Interaction with the recording entrypoints (where the ceiling bites) ---

#[test]
fn record_sme_collateral_commitment_enforces_configured_limit() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    client.set_collateral_limit(&1_000i128);

    // Exactly at the limit succeeds.
    let asset = Symbol::new(&env, "USDC");
    client.record_sme_collateral_commitment(&asset, &1_000i128);

    // One stroop above the limit is rejected with the typed error.
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &1_001i128),
        EscrowError::CollateralLimitExceeded,
    );
}

/// A commitment accepted under a looser ceiling stays readable after the admin tightens the
/// limit; the ceiling gates new writes only and is not retroactive.
#[test]
fn tightening_limit_does_not_invalidate_existing_commitment() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let asset = Symbol::new(&env, "USDC");

    client.set_collateral_limit(&10_000i128);
    client.record_sme_collateral_commitment(&asset, &9_000i128);

    client.set_collateral_limit(&100i128);

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, 9_000i128);
}

/// The ceiling is checked before any write, so a rejected commitment leaves the previously
/// stored one in place.
#[test]
fn rejected_commitment_preserves_previous_commitment() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let asset = Symbol::new(&env, "USDC");

    client.set_collateral_limit(&1_000i128);
    client.record_sme_collateral_commitment(&asset, &500i128);

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &1_500i128),
        EscrowError::CollateralLimitExceeded,
    );

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, 500i128);
}

/// `batch_record_collateral` enforces the same ceiling as the single entrypoint, and does so
/// during pre-validation so one over-limit item rejects the whole batch atomically.
#[test]
fn batch_record_collateral_enforces_configured_limit_atomically() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let usdc = Symbol::new(&env, "USDC");
    let eur = Symbol::new(&env, "EUR");

    client.set_collateral_limit(&1_000i128);

    // Every item within the ceiling succeeds and the last item wins.
    let ok = SorobanVec::from_array(
        &env,
        [
            (usdc.clone(), 100i128),
            (eur.clone(), 1_000i128),
            (usdc.clone(), 400i128),
        ],
    );
    let stored = client.batch_record_collateral(&ok);
    assert_eq!(stored.amount, 400i128);
    assert_eq!(stored.asset, usdc);

    // A single over-limit item rejects the entire batch and leaves the prior state intact.
    let bad = SorobanVec::from_array(&env, [(usdc.clone(), 100i128), (eur, 1_001i128)]);
    assert_contract_error(
        client.try_batch_record_collateral(&bad),
        EscrowError::CollateralLimitExceeded,
    );
    let after = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(after.amount, 400i128);
    assert_eq!(after.asset, usdc);
}
