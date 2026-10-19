/// Tests for Issue #1536: optional strict funding cap.
///
/// Covers:
/// - Strict mode: target 1 000, funded 900 — a contribution of 101 fails,
///   one of 100 succeeds and flips to status 1; balances asserted on both paths.
/// - fund_batch in strict mode: third entry overshoots → whole batch reverts.
/// - Default (non-strict) behaviour unchanged: over-funding still passes.
/// - set_strict_funding_cap: auth, status gating, and its event.
/// - is_strict_funding_cap getter.
use crate::{EscrowError, LiquifactEscrow, StrictFundingCapChanged};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger as _},
    token::StellarAssetClient,
    Address, Env, Event, Vec as SorobanVec,
};

// Helper: init an escrow with a real token and mint to the escrow for balance checks.
fn setup_strict_cap_escrow<'a>(
    env: &'a Env,
    target: i128,
) -> (crate::LiquifactEscrowClient<'a>, Address, Address, Address) {
    env.mock_all_auths();
    let mut li = env.ledger().get();
    li.timestamp = 1_000_000;
    li.sequence_number = 1000;
    env.ledger().set(li);

    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token_id = sac.address();

    let escrow_id = env.register(LiquifactEscrow, ());
    let client = crate::LiquifactEscrowClient::new(env, &escrow_id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "SFC_TST"),
        &sme,
        &target,
        &800i64,
        &0u64,
        &token_id,
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

    (client, admin, sme, token_id)
}

fn mint_to(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token_id).mint(to, &amount);
}

// --- is_strict_funding_cap defaults to false ---

#[test]
fn test_is_strict_funding_cap_default_false() {
    let env = Env::default();
    let (client, _admin, _sme, _token) = setup_strict_cap_escrow(&env, 1_000i128);
    assert!(!client.is_strict_funding_cap());
}

// --- set_strict_funding_cap toggles and emits event ---

#[test]
fn test_set_strict_funding_cap_emits_event() {
    let env = Env::default();
    let (client, _admin, _sme, _token) = setup_strict_cap_escrow(&env, 1_000i128);
    let contract_id = client.address.clone();

    // Snapshot invoice_id before the action (get_escrow would clear event log)
    let invoice_id = client.get_escrow().invoice_id;

    client.set_strict_funding_cap(&true);

    // Snapshot events immediately after set_strict_funding_cap, BEFORE any further contract calls
    // (even read calls like is_strict_funding_cap() clear the event buffer).
    let all_events = env.events().all();
    let last_event = all_events.events().last().unwrap().clone();
    let expected = StrictFundingCapChanged {
        name: soroban_sdk::symbol_short!("sfc_chg"),
        invoice_id,
        strict: 1u32,
    }
    .to_xdr(&env, &contract_id);
    assert_eq!(last_event, expected);

    // Now verify the flag was actually set
    assert!(client.is_strict_funding_cap());
}

#[test]
fn test_set_strict_funding_cap_noop_emits_nothing() {
    let env = Env::default();
    let (client, _admin, _sme, _token) = setup_strict_cap_escrow(&env, 1_000i128);

    // Set false → false (no-op): the setter should emit no events.
    // env.events().all() returns only the events from the last invocation.
    client.set_strict_funding_cap(&false);
    let events = env.events().all();
    assert_eq!(
        events.events().len(),
        0,
        "no-op set_strict_funding_cap must emit no events"
    );
}

// --- set_strict_funding_cap: status gating (must be open) ---

#[test]
fn test_set_strict_funding_cap_after_funded_fails() {
    let env = Env::default();
    let target = 1_000i128;
    let (client, _admin, _sme, token_id) = setup_strict_cap_escrow(&env, target);

    // Fund the escrow to completion
    let investor = Address::generate(&env);
    mint_to(&env, &token_id, &investor, target);
    client.fund(&investor, &target);
    assert_eq!(client.get_escrow().status, 1);

    // Now try to set the strict cap — must fail (status != 0)
    let result = client.try_set_strict_funding_cap(&true);
    super::assert_contract_error(result, EscrowError::TargetUpdateNotOpen);
}

// --- set_strict_funding_cap: auth test ---

#[test]
#[should_panic]
fn test_set_strict_funding_cap_non_admin_panics() {
    let env = Env::default();
    let (client, _admin, _sme, _token) = setup_strict_cap_escrow(&env, 1_000i128);

    // Drop all mock auths — next call will fail auth check
    env.mock_auths(&[]);
    client.set_strict_funding_cap(&true);
}

// --- Strict mode: contribution exceeds remaining capacity ---
//
// target=1000, funded=900, strict=true → 101 fails, 100 succeeds → status 1

#[test]
fn test_strict_mode_overshoot_rejected() {
    let env = Env::default();
    let target = 1_000i128;
    let (client, _admin, _sme, token_id) = setup_strict_cap_escrow(&env, target);

    // Fund 900 (below target)
    let investor1 = Address::generate(&env);
    mint_to(&env, &token_id, &investor1, 900i128);
    client.fund(&investor1, &900i128);
    assert_eq!(client.get_escrow().funded_amount, 900i128);

    // Enable strict cap
    client.set_strict_funding_cap(&true);

    // A contribution of 101 must fail (100 remaining)
    let investor2 = Address::generate(&env);
    mint_to(&env, &token_id, &investor2, 101i128);
    let result = client.try_fund(&investor2, &101i128);
    super::assert_contract_error(result, EscrowError::ContributionExceedsRemainingCapacity);

    // funded_amount must be unchanged
    assert_eq!(client.get_escrow().funded_amount, 900i128);
    assert_eq!(client.get_escrow().status, 0);
}

#[test]
fn test_strict_mode_exact_remaining_succeeds_and_flips_status() {
    let env = Env::default();
    let target = 1_000i128;
    let (client, _admin, _sme, token_id) = setup_strict_cap_escrow(&env, target);

    // Fund 900
    let investor1 = Address::generate(&env);
    mint_to(&env, &token_id, &investor1, 900i128);
    client.fund(&investor1, &900i128);

    // Enable strict cap
    client.set_strict_funding_cap(&true);

    // Exactly 100 remaining → must succeed and flip to status 1
    let investor2 = Address::generate(&env);
    mint_to(&env, &token_id, &investor2, 100i128);
    let escrow = client.fund(&investor2, &100i128);

    assert_eq!(escrow.funded_amount, 1_000i128);
    assert_eq!(
        escrow.status, 1,
        "escrow must flip to funded after exact fill"
    );
}

// --- Default (non-strict) behaviour: over-funding still passes ---

#[test]
fn test_non_strict_overfunding_allowed() {
    let env = Env::default();
    let target = 1_000i128;
    let (client, _admin, _sme, token_id) = setup_strict_cap_escrow(&env, target);

    // strict cap is false by default
    assert!(!client.is_strict_funding_cap());

    // Investor funds 900, then another funds 200 (100 over target) — must succeed
    let investor1 = Address::generate(&env);
    mint_to(&env, &token_id, &investor1, 900i128);
    client.fund(&investor1, &900i128);

    let investor2 = Address::generate(&env);
    mint_to(&env, &token_id, &investor2, 200i128);
    let escrow = client.fund(&investor2, &200i128);

    assert_eq!(
        escrow.funded_amount, 1_100i128,
        "over-funding allowed in non-strict mode"
    );
    assert_eq!(escrow.status, 1);
}

// --- fund_batch in strict mode: third entry overshoots → whole batch reverts ---

#[test]
fn test_strict_mode_fund_batch_overshoot_reverts_whole_batch() {
    let env = Env::default();
    let target = 1_000i128;
    let (client, _admin, _sme, token_id) = setup_strict_cap_escrow(&env, target);

    // Fund 700 first (300 remaining)
    let seed_investor = Address::generate(&env);
    mint_to(&env, &token_id, &seed_investor, 700i128);
    client.fund(&seed_investor, &700i128);

    client.set_strict_funding_cap(&true);

    // Batch: 100 + 100 + 200 = 400 > 300 remaining → must revert entirely
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);
    mint_to(&env, &token_id, &inv_a, 100i128);
    mint_to(&env, &token_id, &inv_b, 100i128);
    mint_to(&env, &token_id, &inv_c, 200i128);

    let mut entries = SorobanVec::new(&env);
    entries.push_back((inv_a.clone(), 100i128));
    entries.push_back((inv_b.clone(), 100i128));
    entries.push_back((inv_c.clone(), 200i128));

    let result = client.try_fund_batch(&entries);
    super::assert_contract_error(result, EscrowError::ContributionExceedsRemainingCapacity);

    // No contribution or count changes
    assert_eq!(client.get_escrow().funded_amount, 700i128);
    assert_eq!(client.get_escrow().status, 0);
}

// --- fund_batch strict mode: batch that fits succeeds ---

#[test]
fn test_strict_mode_fund_batch_within_capacity_succeeds() {
    let env = Env::default();
    let target = 1_000i128;
    let (client, _admin, _sme, token_id) = setup_strict_cap_escrow(&env, target);

    // Fund 400 first (600 remaining)
    let seed_investor = Address::generate(&env);
    mint_to(&env, &token_id, &seed_investor, 400i128);
    client.fund(&seed_investor, &400i128);

    client.set_strict_funding_cap(&true);

    // Batch: 200 + 200 + 200 = 600 = remaining → must succeed
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);
    mint_to(&env, &token_id, &inv_a, 200i128);
    mint_to(&env, &token_id, &inv_b, 200i128);
    mint_to(&env, &token_id, &inv_c, 200i128);

    let mut entries = SorobanVec::new(&env);
    entries.push_back((inv_a.clone(), 200i128));
    entries.push_back((inv_b.clone(), 200i128));
    entries.push_back((inv_c.clone(), 200i128));

    let escrow = client.fund_batch(&entries);
    assert_eq!(escrow.funded_amount, 1_000i128);
    assert_eq!(escrow.status, 1);
}

// --- fund_with_commitment in strict mode ---

#[test]
fn test_strict_mode_fund_with_commitment_overshoot_rejected() {
    let env = Env::default();
    let target = 1_000i128;
    let (client, _admin, _sme, token_id) = setup_strict_cap_escrow(&env, target);

    // Fund 900 first
    let investor1 = Address::generate(&env);
    mint_to(&env, &token_id, &investor1, 900i128);
    client.fund(&investor1, &900i128);

    client.set_strict_funding_cap(&true);

    // fund_with_commitment with 101 (over remaining 100) → must fail
    let investor2 = Address::generate(&env);
    mint_to(&env, &token_id, &investor2, 101i128);
    let result = client.try_fund_with_commitment(&investor2, &101i128, &0u64);
    super::assert_contract_error(result, EscrowError::ContributionExceedsRemainingCapacity);

    assert_eq!(client.get_escrow().funded_amount, 900i128);
}

// --- disable strict mode allows over-funding again ---

#[test]
fn test_disable_strict_mode_allows_overfunding() {
    let env = Env::default();
    let target = 1_000i128;
    let (client, _admin, _sme, token_id) = setup_strict_cap_escrow(&env, target);

    let investor1 = Address::generate(&env);
    mint_to(&env, &token_id, &investor1, 900i128);
    client.fund(&investor1, &900i128);

    // Enable then disable strict cap
    client.set_strict_funding_cap(&true);
    client.set_strict_funding_cap(&false);
    assert!(!client.is_strict_funding_cap());

    // Over-funding must pass again
    let investor2 = Address::generate(&env);
    mint_to(&env, &token_id, &investor2, 200i128);
    let escrow = client.fund(&investor2, &200i128);
    assert_eq!(escrow.funded_amount, 1_100i128);
    assert_eq!(escrow.status, 1);
}
