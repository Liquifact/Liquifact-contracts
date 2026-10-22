// Tests for fund_with_reference and get_reference_owner.
//
// Acceptance criteria covered:
//  [AC1]  Duplicate reference: second call fails; balances and contribution unchanged.
//  [AC2]  Event order and payloads: EscrowFunded then ContributionReferenced.
//  [AC3]  All fund gates apply (pause, legal hold, allowlist — table-driven ≥ 3).
//  [AC4]  get_reference_owner returns the correct owner.

use crate::{
    ContributionReferenced, DataKey, EscrowError, EscrowFunded, LiquifactEscrow,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    token::StellarAssetClient,
    Address, BytesN, Env,
};

// ── shared setup ──────────────────────────────────────────────────────────────

fn deploy_real<'a>(
    env: &'a Env,
    target: i128,
    invoice_id: &str,
) -> (
    crate::LiquifactEscrowClient<'a>,
    Address, // escrow_id
    StellarAssetClient<'a>,
    Address, // admin
) {
    env.mock_all_auths();
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(env, &token_id);
    let escrow_id = env.register(LiquifactEscrow, ());
    let client = crate::LiquifactEscrowClient::new(env, &escrow_id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, invoice_id),
        &sme,
        &target,
        &500i64,
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
    (client, escrow_id, sac_admin, admin)
}

fn make_ref(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

// ── AC1: duplicate reference rejected ────────────────────────────────────────

#[test]
fn duplicate_reference_rejected_second_call_has_no_effect() {
    let env = Env::default();
    let target = 100_000i128;
    let amount = 10_000i128;
    let (client, _escrow_id, sac_admin, ..) = deploy_real(&env, target, "REF_DUP");

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    sac_admin.mint(&investor1, &(amount * 2));
    sac_admin.mint(&investor2, &amount);

    let reference = make_ref(&env, 0xAA);

    // First call succeeds.
    client.fund_with_reference(&investor1, &amount, &reference);
    let contrib_after_first = client.get_contribution(&investor1);
    assert_eq!(contrib_after_first, amount);

    // Second call with the same reference must fail.
    let result = client.try_fund_with_reference(&investor2, &amount, &reference);
    super::assert_contract_error(result, EscrowError::ContributionReferenceReused);

    // Contribution unchanged.
    assert_eq!(client.get_contribution(&investor1), amount);
    assert_eq!(client.get_contribution(&investor2), 0);
}

#[test]
fn same_investor_same_reference_also_rejected() {
    let env = Env::default();
    let target = 100_000i128;
    let amount = 5_000i128;
    let (client, _escrow_id, sac_admin, ..) = deploy_real(&env, target, "REF_DUP2");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &(amount * 3));

    let reference = make_ref(&env, 0x01);
    client.fund_with_reference(&investor, &amount, &reference);

    let result = client.try_fund_with_reference(&investor, &amount, &reference);
    super::assert_contract_error(result, EscrowError::ContributionReferenceReused);

    // Contribution is still just one deposit.
    assert_eq!(client.get_contribution(&investor), amount);
}

#[test]
fn different_references_both_succeed() {
    let env = Env::default();
    let target = 100_000i128;
    let amount = 5_000i128;
    let (client, _escrow_id, sac_admin, ..) = deploy_real(&env, target, "REF_DIFF");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &(amount * 3));

    client.fund_with_reference(&investor, &amount, &make_ref(&env, 0x01));
    client.fund_with_reference(&investor, &amount, &make_ref(&env, 0x02));

    assert_eq!(client.get_contribution(&investor), amount * 2);
}

// ── AC2: event order and payloads ─────────────────────────────────────────────

#[test]
fn events_order_escrow_funded_then_contribution_referenced() {
    let env = Env::default();
    let target = 100_000i128;
    let amount = 20_000i128;
    let (client, escrow_id, sac_admin, ..) = deploy_real(&env, target, "REF_EVT");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &amount);
    let reference = make_ref(&env, 0x42);

    client.fund_with_reference(&investor, &amount, &reference);

    let all_events = env.events().all();
    let events_list = all_events.events();
    assert!(
        events_list.len() >= 2,
        "expected at least 2 events (EscrowFunded + ContributionReferenced)"
    );

    // First event: EscrowFunded
    let first = events_list.first().unwrap();
    assert_eq!(
        *first,
        EscrowFunded {
            name: symbol_short!("funded"),
            invoice_id: soroban_sdk::symbol_short!("REF_EVT"),
            investor: investor.clone(),
            amount,
            funded_amount: amount,
            status: 0u32,
            investor_effective_yield_bps: 500i64,
            tier_lock_secs: 0u64,
        }
        .to_xdr(&env, &escrow_id)
    );

    // Second event: ContributionReferenced
    let second = events_list.get(1).unwrap();
    assert_eq!(
        *second,
        ContributionReferenced {
            name: symbol_short!("fund_ref"),
            investor: investor.clone(),
            invoice_id: soroban_sdk::symbol_short!("REF_EVT"),
            amount,
            reference: reference.clone(),
        }
        .to_xdr(&env, &escrow_id)
    );
}

// ── AC3: gate tests (pause, legal hold, allowlist) ────────────────────────────

/// Gate 1: ContractPaused blocks fund_with_reference.
#[test]
fn gate_paused_blocks_fund_with_reference() {
    let env = Env::default();
    let target = 100_000i128;
    let amount = 5_000i128;
    let (client, _escrow_id, sac_admin, ..) = deploy_real(&env, target, "REF_PAUSE");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &amount);

    client.set_paused(&true, &crate::PauseScope::Funding, &crate::PauseReason::Incident);

    let result = client.try_fund_with_reference(&investor, &amount, &make_ref(&env, 0x10));
    super::assert_contract_error(result, EscrowError::ContractPaused);
}

/// Gate 2: LegalHoldActive blocks fund_with_reference.
#[test]
fn gate_legal_hold_blocks_fund_with_reference() {
    let env = Env::default();
    let target = 100_000i128;
    let amount = 5_000i128;
    let (client, _escrow_id, sac_admin, ..) = deploy_real(&env, target, "REF_HOLD");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &amount);

    client.set_legal_hold(&true);

    let result = client.try_fund_with_reference(&investor, &amount, &make_ref(&env, 0x20));
    super::assert_contract_error(result, EscrowError::LegalHoldActive);
}

/// Gate 3: Allowlist blocks non-allowlisted investor.
#[test]
fn gate_allowlist_blocks_unlisted_investor() {
    let env = Env::default();
    let target = 100_000i128;
    let amount = 5_000i128;
    let (client, _escrow_id, sac_admin, ..) = deploy_real(&env, target, "REF_ALIST");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &amount);

    // Enable allowlist but do NOT add investor.
    client.set_allowlist_active(&true);

    let result = client.try_fund_with_reference(&investor, &amount, &make_ref(&env, 0x30));
    super::assert_contract_error(result, EscrowError::InvestorNotAllowlisted);
}

/// Gate 4: Zero amount rejected.
#[test]
fn gate_zero_amount_rejected() {
    let env = Env::default();
    let (client, ..) = deploy_real(&env, 100_000i128, "REF_ZERO");
    let investor = Address::generate(&env);
    let result = client.try_fund_with_reference(&investor, &0i128, &make_ref(&env, 0x40));
    super::assert_contract_error(result, EscrowError::FundingAmountNotPositive);
}

/// Gate 5: FundingClosed (status != 0) blocks fund_with_reference.
#[test]
fn gate_funding_closed_after_target_reached() {
    let env = Env::default();
    let target = 10_000i128;
    let (client, _escrow_id, sac_admin, ..) = deploy_real(&env, target, "REF_CLOSED");

    // Fill to target with a plain fund call.
    let funder = Address::generate(&env);
    sac_admin.mint(&funder, &target);
    client.fund(&funder, &target); // status → 1

    let investor2 = Address::generate(&env);
    sac_admin.mint(&investor2, &1_000i128);
    let result = client.try_fund_with_reference(&investor2, &1_000i128, &make_ref(&env, 0x50));
    super::assert_contract_error(result, EscrowError::FundingClosed);
}

// ── AC4: get_reference_owner ──────────────────────────────────────────────────

#[test]
fn get_reference_owner_returns_investor_after_fund() {
    let env = Env::default();
    let target = 100_000i128;
    let amount = 8_000i128;
    let (client, _escrow_id, sac_admin, ..) = deploy_real(&env, target, "REF_VIEW");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &amount);
    let reference = make_ref(&env, 0xBB);

    assert_eq!(client.get_reference_owner(&reference), None);

    client.fund_with_reference(&investor, &amount, &reference);

    assert_eq!(client.get_reference_owner(&reference), Some(investor));
}

#[test]
fn get_reference_owner_returns_none_for_unused_reference() {
    let env = Env::default();
    let (client, ..) = deploy_real(&env, 100_000i128, "REF_NONE");
    let reference = make_ref(&env, 0xFF);
    assert_eq!(client.get_reference_owner(&reference), None);
}

#[test]
fn two_references_map_to_correct_owners() {
    let env = Env::default();
    let target = 100_000i128;
    let amount = 5_000i128;
    let (client, _escrow_id, sac_admin, ..) = deploy_real(&env, target, "REF_TWO");

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    sac_admin.mint(&inv1, &amount);
    sac_admin.mint(&inv2, &amount);

    let ref1 = make_ref(&env, 0x11);
    let ref2 = make_ref(&env, 0x22);

    client.fund_with_reference(&inv1, &amount, &ref1);
    client.fund_with_reference(&inv2, &amount, &ref2);

    assert_eq!(client.get_reference_owner(&ref1), Some(inv1));
    assert_eq!(client.get_reference_owner(&ref2), Some(inv2));
}

// ── fund_with_reference happy path: contribution and state correct ────────────

#[test]
fn fund_with_reference_records_contribution_correctly() {
    let env = Env::default();
    let target = 100_000i128;
    let amount = 15_000i128;
    let (client, _escrow_id, sac_admin, ..) = deploy_real(&env, target, "REF_HAPPY");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &amount);
    let reference = make_ref(&env, 0x07);

    let escrow = client.fund_with_reference(&investor, &amount, &reference);
    assert_eq!(escrow.funded_amount, amount);
    assert_eq!(client.get_contribution(&investor), amount);
    assert_eq!(client.get_reference_owner(&reference), Some(investor));
}
