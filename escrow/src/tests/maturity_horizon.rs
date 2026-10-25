use super::*;
use crate::{
    DEFAULT_MATURITY_MAX_HORIZON_SECS, MAX_MATURITY_MAX_HORIZON_SECS, MIN_MATURITY_MAX_HORIZON_SECS,
};

fn init_horizon(env: &Env, horizon: u64, maturity: u64) -> LiquifactEscrowClient<'_> {
    let (client, admin, sme) = setup(env);
    env.ledger().set_timestamp(1_000);
    let token = install_stellar_asset_token(env);
    client.init(
        &admin,
        &String::from_str(env, "HORIZON"),
        &sme,
        &TARGET,
        &800,
        &maturity,
        &token.id,
        &None,
        &Address::generate(env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &Some(horizon),
        &None,
        &None,
        &None,
    );
    client
}

#[test]
fn zero_horizon_is_rejected() {
    let env = Env::default();
    let client = init_horizon(&env, DEFAULT_MATURITY_MAX_HORIZON_SECS, 0);
    assert_contract_error(
        client.try_update_maturity_max_horizon(&0),
        EscrowError::MaturityHorizonOutOfRange,
    );
    assert_eq!(
        client.get_maturity_max_horizon(),
        DEFAULT_MATURITY_MAX_HORIZON_SECS
    );
}

#[test]
fn both_setters_enforce_inclusive_boundaries() {
    for raise in [false, true] {
        for horizon in [
            MIN_MATURITY_MAX_HORIZON_SECS - 1,
            MIN_MATURITY_MAX_HORIZON_SECS,
            MAX_MATURITY_MAX_HORIZON_SECS,
            MAX_MATURITY_MAX_HORIZON_SECS + 1,
            u64::MAX,
        ] {
            let env = Env::default();
            // Init bounds intentionally remain unchanged; a legacy horizon of zero
            // lets the raise-only path exercise MIN without tripping HorizonNotRaised.
            let client = init_horizon(&env, 0, 0);
            let result = if raise {
                client.try_raise_maturity_max_horizon(&horizon)
            } else {
                client.try_update_maturity_max_horizon(&horizon)
            };
            if (MIN_MATURITY_MAX_HORIZON_SECS..=MAX_MATURITY_MAX_HORIZON_SECS).contains(&horizon) {
                assert_eq!(result, Ok(Ok(horizon)));
                assert_eq!(client.get_maturity_max_horizon(), horizon);
            } else {
                assert_contract_error(result, EscrowError::MaturityHorizonOutOfRange);
                assert!(env.events().all().events().is_empty());
                assert_eq!(client.get_maturity_max_horizon(), 0);
            }
        }
    }
}

#[test]
fn lowering_to_remaining_maturity_is_allowed_but_below_it_is_atomic() {
    let env = Env::default();
    let maturity = 1_000 + 2 * MIN_MATURITY_MAX_HORIZON_SECS;
    let client = init_horizon(&env, DEFAULT_MATURITY_MAX_HORIZON_SECS, maturity);
    // Use the current ledger, not the timestamp at initialization.
    env.ledger().set_timestamp(2_000);
    let remaining = maturity - 2_000;
    assert_contract_error(
        client.try_update_maturity_max_horizon(&(remaining - 1)),
        EscrowError::MaturityHorizonBelowMaturity,
    );
    assert!(env.events().all().events().is_empty());
    assert_eq!(
        client.get_maturity_max_horizon(),
        DEFAULT_MATURITY_MAX_HORIZON_SECS
    );
    assert_eq!(client.get_escrow().maturity, maturity);
    assert_eq!(client.update_maturity_max_horizon(&remaining), remaining);
    assert_eq!(client.get_escrow().maturity, maturity);
}

#[test]
fn unchanged_update_and_non_increasing_raise_are_rejected() {
    let env = Env::default();
    let client = init_horizon(&env, DEFAULT_MATURITY_MAX_HORIZON_SECS, 0);
    assert_contract_error(
        client.try_update_maturity_max_horizon(&DEFAULT_MATURITY_MAX_HORIZON_SECS),
        EscrowError::MaturityHorizonUnchanged,
    );
    assert!(env.events().all().events().is_empty());
    for horizon in [
        DEFAULT_MATURITY_MAX_HORIZON_SECS,
        MIN_MATURITY_MAX_HORIZON_SECS,
    ] {
        assert_contract_error(
            client.try_raise_maturity_max_horizon(&horizon),
            EscrowError::HorizonNotRaised,
        );
        assert!(env.events().all().events().is_empty());
    }
    assert_eq!(
        client.get_maturity_max_horizon(),
        DEFAULT_MATURITY_MAX_HORIZON_SECS
    );
}

#[test]
fn both_setters_reject_every_non_open_status() {
    for status in 1..=4 {
        let env = Env::default();
        env.mock_all_auths();
        let client = if status == 4 {
            let client = init_horizon(&env, DEFAULT_MATURITY_MAX_HORIZON_SECS, 0);
            client.cancel_funding();
            client
        } else {
            let (client, _, _) = init_and_fund_with_real_token(&env, TARGET, "HORIZON_STATUS");
            if status == 2 {
                client.settle();
            }
            if status == 3 {
                client.withdraw();
            }
            client
        };
        let before = client.get_escrow();
        assert_eq!(before.status, status);
        let horizon = DEFAULT_MATURITY_MAX_HORIZON_SECS + 1;
        assert_contract_error(
            client.try_update_maturity_max_horizon(&horizon),
            EscrowError::MaturityHorizonUpdateNotOpen,
        );
        assert!(env.events().all().events().is_empty());
        assert_contract_error(
            client.try_raise_maturity_max_horizon(&horizon),
            EscrowError::MaturityHorizonUpdateNotOpen,
        );
        assert!(env.events().all().events().is_empty());
        assert_eq!(client.get_escrow(), before);
        assert_eq!(
            client.get_maturity_max_horizon(),
            DEFAULT_MATURITY_MAX_HORIZON_SECS
        );
    }
}

#[test]
fn horizon_updates_preserve_existing_investor_commitment() {
    let env = Env::default();
    let client = init_horizon(&env, DEFAULT_MATURITY_MAX_HORIZON_SECS, 0);
    let investor = Address::generate(&env);
    StellarAssetClient::new(&env, &client.get_funding_token()).mint(&investor, &100);
    client.fund_with_commitment(&investor, &100, &3_600);
    let lock = client.get_investor_claim_not_before(&investor);
    assert_eq!(lock, 4_600);
    assert_eq!(client.get_escrow().status, 0);
    client.update_maturity_max_horizon(&MIN_MATURITY_MAX_HORIZON_SECS);
    client.raise_maturity_max_horizon(&MAX_MATURITY_MAX_HORIZON_SECS);
    assert_eq!(client.get_investor_claim_not_before(&investor), lock);
    assert_eq!(client.get_contribution(&investor), 100);
}

#[test]
fn horizon_updates_do_not_overflow_near_maximum_ledger_time() {
    let env = Env::default();
    let client = init_horizon(&env, DEFAULT_MATURITY_MAX_HORIZON_SECS, 0);
    env.ledger().set_timestamp(u64::MAX - 1);
    client.update_maturity(&u64::MAX);
    client.update_maturity_max_horizon(&MIN_MATURITY_MAX_HORIZON_SECS);
    assert_eq!(client.get_escrow().maturity, u64::MAX);
}

#[test]
fn both_setters_require_admin_authorization() {
    for raise in [false, true] {
        let env = Env::default();
        let client = init_horizon(&env, DEFAULT_MATURITY_MAX_HORIZON_SECS, 0);
        env.mock_auths(&[]);
        let result = if raise {
            client.try_raise_maturity_max_horizon(&MAX_MATURITY_MAX_HORIZON_SECS)
        } else {
            client.try_update_maturity_max_horizon(&MAX_MATURITY_MAX_HORIZON_SECS)
        };
        assert!(result.is_err());
        assert_eq!(
            client.get_maturity_max_horizon(),
            DEFAULT_MATURITY_MAX_HORIZON_SECS
        );
    }
}
