// Tests for the admin-only `set_collateral_limit` setter: in-bounds set, out-of-bounds
// rejection, non-admin rejection, and its interaction with
// `record_sme_collateral_commitment`'s enforcement of the configured limit.

use crate::tests::{assert_contract_error, setup};
use crate::{CollateralLimitUpdated, EscrowError, MAX_INVOICE_AMOUNT};
use soroban_sdk::{
    symbol_short, testutils::Address as _, Address, Env, Event, IntoVal, Symbol, Vec as SorobanVec,
};

fn init_escrow(env: &Env, client: &crate::LiquifactEscrowClient, admin: &Address, sme: &Address) {
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
    );
}

#[test]
fn admin_sets_collateral_limit_in_bounds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);

    client.set_collateral_limit(&5_000i128);
    assert_eq!(client.get_collateral_limit(), 5_000i128);
    assert_eq!(client.get_collateral_config().collateral_limit, 5_000i128);

    // A second, lower in-bounds update also succeeds.
    client.set_collateral_limit(&1i128);
    assert_eq!(client.get_collateral_limit(), 1i128);
}

#[test]
fn set_collateral_limit_emits_event_with_old_and_new_limit() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
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

#[test]
fn set_collateral_limit_rejects_non_positive() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    assert_contract_error(
        client.try_set_collateral_limit(&0i128),
        EscrowError::CollateralLimitNotPositive,
    );
    assert_contract_error(
        client.try_set_collateral_limit(&-1i128),
        EscrowError::CollateralLimitNotPositive,
    );

    // Rejected calls must not change the stored limit.
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

#[test]
fn set_collateral_limit_rejects_exceeding_max_invoice_amount() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    assert_contract_error(
        client.try_set_collateral_limit(&(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceedsMax,
    );

    // The maximum allowed value itself is accepted.
    client.set_collateral_limit(&MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

#[test]
fn non_admin_cannot_set_collateral_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    let non_admin = Address::generate(&env);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "set_collateral_limit",
            args: SorobanVec::from_array(&env, [1_000i128.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_set_collateral_limit(&1_000i128);
    assert!(result.is_err());

    // The limit must remain unchanged after the rejected call.
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

#[test]
fn record_sme_collateral_commitment_enforces_configured_limit() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);

    client.set_collateral_limit(&1_000i128);

    // Exactly at the limit succeeds.
    let asset = Symbol::new(&env, "USDC");
    client.record_sme_collateral_commitment(&asset, &1_000i128);

    // Above the limit is rejected with the pre-existing typed error.
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &1_001i128),
        EscrowError::CollateralLimitExceeded,
    );
}
