//! Regression coverage for the base-yield update guardrails.
//!
//! update_yield_bps used to move the base yield without looking at the tier
//! ladder or at principal that had already been recorded. Two kinds of split
//! were possible inside a single escrow, with no on-chain signal of either:
//!
//! * an investor who funded before the update keeps the yield that was locked in
//!   at their first deposit, while a later investor is credited with the new base;
//! * a tier whose yield fell below the new base made locking capital earn less
//!   than not locking it, and effective_yield_for_commitment silently returned
//!   the base instead of the tier.
//!
//! The base yield is now locked once principal exists, and the tier ladder is
//! re-validated against the new base before the update is applied.

use super::*;

use crate::YieldBpsUpdatedEvent;

fn tier(min_lock_secs: u64, yield_bps: i64) -> YieldTier {
    YieldTier {
        min_lock_secs,
        yield_bps,
    }
}

/// Deploys an escrow with the given base yield and tier ladder. The funding
/// token is a plain address, so this is only for tests that never move funds.
fn init_escrow<'a>(
    env: &'a Env,
    base_yield_bps: i64,
    tiers: Option<SorobanVec<YieldTier>>,
) -> (Address, LiquifactEscrowClient<'a>) {
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let (token, treasury) = free_addresses(env);
    client.init(
        &admin,
        &String::from_str(env, "YTUPD"),
        &sme,
        &TARGET,
        &base_yield_bps,
        &0u64,
        &token,
        &None,
        &treasury,
        &tiers,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    (contract_id, client)
}

/// Deploys an escrow backed by a real SEP-41 asset so that funding and
/// unfunding can actually move tokens.
fn init_escrow_with_token<'a>(
    env: &'a Env,
    base_yield_bps: i64,
    tiers: Option<SorobanVec<YieldTier>>,
) -> (Address, LiquifactEscrowClient<'a>, StellarTestToken<'a>) {
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(env);
    let token = install_stellar_asset_token(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &String::from_str(env, "YTUPD"),
        &sme,
        &TARGET,
        &base_yield_bps,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &tiers,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    (contract_id, client, token)
}

#[test]
fn test_update_yield_bps_rejects_base_above_configured_tier() {
    let env = Env::default();
    let tiers = SorobanVec::from_array(&env, [tier(100, 1_000)]);
    let (_contract_id, client) = init_escrow(&env, 800, Some(tiers));

    // A base of 1_200 would push the 1_000 bps tier below the base yield.
    let result = client.try_update_yield_bps(&1_200i64);
    assert_contract_error(result, EscrowError::TierYieldBelowBase);

    // The rejected update must not have been applied.
    assert_eq!(client.get_escrow().yield_bps, 800);
    assert_eq!(client.get_yield_tiers().len(), 1);
}

#[test]
fn test_update_yield_bps_locked_once_principal_is_recorded() {
    let env = Env::default();
    let (_contract_id, client, token) = init_escrow_with_token(&env, 800, None);

    let investor = Address::generate(&env);
    token.stellar.mint(&investor, &1_000i128);
    client.fund(&investor, &1_000i128);
    assert_eq!(client.get_escrow().funded_amount, 1_000i128);

    let result = client.try_update_yield_bps(&900i64);
    assert_contract_error(result, EscrowError::YieldBpsLockedAfterFunding);

    assert_eq!(client.get_escrow().yield_bps, 800);
}

#[test]
fn test_update_yield_bps_succeeds_again_after_a_full_unfund() {
    let env = Env::default();
    let (_contract_id, client, token) = init_escrow_with_token(&env, 800, None);

    let investor = Address::generate(&env);
    token.stellar.mint(&investor, &1_000i128);
    client.fund(&investor, &1_000i128);

    let result = client.try_update_yield_bps(&900i64);
    assert_contract_error(result, EscrowError::YieldBpsLockedAfterFunding);

    // A full unfund returns the escrow to zero principal, which releases the lock.
    client.unfund(&investor, &1_000i128);
    assert_eq!(client.get_escrow().funded_amount, 0);

    let updated = client.update_yield_bps(&900i64);
    assert_eq!(updated.yield_bps, 900);
    assert_eq!(client.get_escrow().yield_bps, 900);
}

#[test]
fn test_update_yield_bps_lowering_base_succeeds_and_emits_event() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    let tiers = SorobanVec::from_array(&env, [tier(100, 1_200)]);
    let (contract_id, client) = init_escrow(&env, 1_000, Some(tiers));

    let updated = client.update_yield_bps(&800i64);
    assert_eq!(updated.yield_bps, 800);

    // The events have to be captured right after the update: a read-only contract
    // call in between resets the event buffer, so the capture comes first.
    let all_events = env.events().all();
    let expected = YieldBpsUpdatedEvent {
        name: symbol_short!("yld_upd"),
        invoice_id: client.get_escrow().invoice_id,
        old_yield_bps: 1_000i64,
        new_yield_bps: 800i64,
    }
    .to_xdr(&env, &contract_id);

    assert_eq!(all_events.events().last().unwrap().clone(), expected);

    // The tier table is immutable and still valid against the lowered base.
    let stored = client.get_yield_tiers();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored.get_unchecked(0).yield_bps, 1_200);
}
