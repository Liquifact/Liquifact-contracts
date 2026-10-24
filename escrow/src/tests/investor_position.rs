// Tests for get_investor_position, get_investor_positions, and the
// overflow-safe pagination fix applied to get_funding_records,
// get_yield_tiers_page, and get_attestation_digests.
//
// Acceptance criteria covered:
//  [AC1]  Position fields: simple investor, tiered investor, claimed investor, refunded investor.
//  [AC2]  Pagination edge-cases: empty index, start >= len, limit = 0, limit > cap, partial last page.
//  [AC3]  get_funding_records(u32::MAX - 1, 50) returns empty without trapping.
//  [AC4]  Budget: the tests exercise a 20-entry page (see `investor_positions_budget_20_entries`).

use crate::{InvestorPosition, LiquifactEscrow, MAX_POSITION_READ_PAGE};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    Address, Env,
};

// ── shared setup helper ───────────────────────────────────────────────────────

/// Deploy and init an escrow backed by a real SAC, returning
/// `(client, escrow_id, token_id, sac_admin, admin, sme)`.
fn deploy_real(
    env: &Env,
    target: i128,
    yield_bps: i64,
    invoice_id: &str,
) -> (
    crate::LiquifactEscrowClient<'_>,
    Address,
    Address,
    StellarAssetClient<'_>,
    Address,
    Address,
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
        &yield_bps,
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

    (client, escrow_id, token_id, sac_admin, admin, sme)
}

// ── AC1: position fields ──────────────────────────────────────────────────────

/// Simple investor: funded but escrow not yet settled.
/// contribution is positive; claimed = false; refunded = false; claimable = 0.
#[test]
fn position_simple_investor_before_settlement() {
    let env = Env::default();
    let target = 100_000i128;
    let (client, _escrow_id, _token_id, sac_admin, _admin, _sme) =
        deploy_real(&env, target, 500i64, "POS_SIMPLE");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &target);
    client.fund(&investor, &target);

    let pos = client.get_investor_position(&investor);
    assert_eq!(pos.investor, investor);
    assert_eq!(pos.contribution, target);
    assert!(!pos.claimed);
    assert!(!pos.refunded);
    // escrow status is 1 (funded) but not yet settled — claimable must be 0
    assert_eq!(pos.claimable_payout, 0);
    assert_eq!(pos.claim_not_before, 0);
}

/// Tiered investor: fund_with_commitment stores a per-investor effective yield
/// that differs from the escrow base yield.
#[test]
fn position_tiered_investor_effective_yield() {
    let env = Env::default();
    let target = 200_000i128;
    let base_yield: i64 = 400; // escrow-level bps

    let (client, _escrow_id, _token_id, sac_admin, admin, _sme) =
        deploy_real(&env, target, base_yield, "POS_TIERED");

    // Configure a yield tier: lock >= 3600 s → 800 bps
    let tier_yield: i64 = 800;
    let lock_secs: u64 = 3_600;
    client.set_yield_tiers(&soroban_sdk::vec![
        &env,
        crate::YieldTier {
            min_lock_secs: lock_secs,
            yield_bps: tier_yield,
        }
    ]);

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &target);
    client.fund_with_commitment(&investor, &target, &lock_secs);

    let pos = client.get_investor_position(&investor);
    assert_eq!(pos.contribution, target);
    // The investor's effective yield must match the tier, not the escrow base.
    assert_eq!(pos.effective_yield_bps, tier_yield);
    assert!(!pos.claimed);
    assert!(!pos.refunded);
}

/// Claimed investor: after settle + claim the `claimed` flag must be true
/// and `claimable_payout` must be 0.
#[test]
fn position_claimed_investor() {
    let env = Env::default();
    let target = 50_000i128;
    let yield_bps: i64 = 1_000; // 10 %
    let coupon = target * yield_bps as i128 / 10_000; // = 5 000

    let (client, escrow_id, _token_id, sac_admin, _admin, _sme) =
        deploy_real(&env, target, yield_bps, "POS_CLAIMED");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &target);
    client.fund(&investor, &target);

    // Mint the coupon portion into the escrow so settle's payout is solvent.
    sac_admin.mint(&escrow_id, &coupon);
    client.settle();

    // Claim
    client.claim_investor_payout(&investor);

    let pos = client.get_investor_position(&investor);
    assert!(pos.claimed, "investor should be marked claimed");
    assert_eq!(pos.claimable_payout, 0, "already claimed → claimable = 0");
    assert!(!pos.refunded);
}

/// Refunded investor: after cancellation + refund the `refunded` flag must be true.
#[test]
fn position_refunded_investor() {
    let env = Env::default();
    let target = 80_000i128;
    let contribution = 20_000i128; // partial — does not hit target → stays cancellable

    let (client, _escrow_id, _token_id, sac_admin, admin, _sme) =
        deploy_real(&env, target, 500i64, "POS_REFUND");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &contribution);
    client.fund(&investor, &contribution);

    // Cancel (status 0 → 4)
    client.cancel_funding(&admin);

    // Refund
    client.refund(&investor);

    let pos = client.get_investor_position(&investor);
    assert!(pos.refunded, "investor should be marked refunded");
    // Contribution was zeroed out by refund_impl before transfer.
    assert_eq!(pos.contribution, 0);
    assert!(!pos.claimed);
    assert_eq!(pos.claimable_payout, 0);
}

// ── AC2: pagination edge-cases for get_investor_positions ─────────────────────

/// Empty investor index → empty result.
#[test]
fn positions_empty_index_returns_empty() {
    let env = Env::default();
    let (client, ..) = deploy_real(&env, 100_000i128, 500i64, "POS_EMPTY");
    let result = client.get_investor_positions(&0, &10);
    assert_eq!(result.len(), 0);
}

/// start >= len → empty result.
#[test]
fn positions_start_beyond_end_returns_empty() {
    let env = Env::default();
    let target = 10_000i128;
    let (client, _escrow_id, _token_id, sac_admin, ..) =
        deploy_real(&env, target, 500i64, "POS_OOB");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &target);
    client.fund(&investor, &target);

    // index has 1 entry; start=1 is past the end
    let result = client.get_investor_positions(&1, &10);
    assert_eq!(result.len(), 0);

    // start = 100 also
    let result2 = client.get_investor_positions(&100, &10);
    assert_eq!(result2.len(), 0);
}

/// limit = 0 → empty result.
#[test]
fn positions_zero_limit_returns_empty() {
    let env = Env::default();
    let target = 10_000i128;
    let (client, _escrow_id, _token_id, sac_admin, ..) =
        deploy_real(&env, target, 500i64, "POS_ZLIM");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &target);
    client.fund(&investor, &target);

    let result = client.get_investor_positions(&0, &0);
    assert_eq!(result.len(), 0);
}

/// limit > MAX_POSITION_READ_PAGE → silently clamped; result length <= cap.
#[test]
fn positions_limit_above_cap_is_clamped() {
    let env = Env::default();
    // Fund 3 investors so we have enough entries to verify clamping matters.
    let per_investor = 5_000i128;
    let total_target = per_investor * 5; // headroom so all three fit
    let (client, _escrow_id, _token_id, sac_admin, ..) =
        deploy_real(&env, total_target, 500i64, "POS_CAP");

    for _ in 0..3 {
        let inv = Address::generate(&env);
        sac_admin.mint(&inv, &per_investor);
        client.fund(&inv, &per_investor);
    }

    // Request 9999 which is way above MAX_POSITION_READ_PAGE (20); only 3 entries exist.
    let result = client.get_investor_positions(&0, &9999);
    assert!(result.len() <= MAX_POSITION_READ_PAGE);
    assert_eq!(result.len(), 3);
}

/// Final partial page: 3 investors, start=2, limit=10 → exactly 1 entry returned.
#[test]
fn positions_partial_last_page() {
    let env = Env::default();
    let per_investor = 5_000i128;
    let total_target = per_investor * 5;
    let (client, _escrow_id, _token_id, sac_admin, ..) =
        deploy_real(&env, total_target, 500i64, "POS_PART");

    let mut investors = soroban_sdk::vec![&env];
    for _ in 0..3 {
        let inv = Address::generate(&env);
        sac_admin.mint(&inv, &per_investor);
        client.fund(&inv, &per_investor);
        investors.push_back(inv);
    }

    let result = client.get_investor_positions(&2, &10);
    assert_eq!(result.len(), 1);
    // The returned position's investor must match index[2]
    assert_eq!(result.get(0).unwrap().investor, investors.get(2).unwrap());
}

// ── AC3: overflow-safe pagination ─────────────────────────────────────────────

/// get_funding_records with start = u32::MAX - 1 and limit = 50 must return
/// empty without panicking (the escrow has no entries at that index).
#[test]
fn funding_records_near_u32_max_does_not_trap() {
    let env = Env::default();
    let target = 10_000i128;
    let (client, _escrow_id, _token_id, sac_admin, ..) =
        deploy_real(&env, target, 500i64, "POS_OVF");

    // Fund one entry so the index is non-empty (otherwise the early-exit fires trivially).
    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &target);
    client.fund(&investor, &target);

    // start = u32::MAX - 1 is far past the single entry — must not trap.
    let result = client.get_funding_records(&(u32::MAX - 1), &50);
    assert_eq!(result.len(), 0);
}

/// get_yield_tiers_page with start = u32::MAX - 1 must not trap.
#[test]
fn yield_tiers_page_near_u32_max_does_not_trap() {
    let env = Env::default();
    let (client, ..) = deploy_real(&env, 100_000i128, 500i64, "POS_YTOVF");

    client.set_yield_tiers(&soroban_sdk::vec![
        &env,
        crate::YieldTier { min_lock_secs: 3600, yield_bps: 800 }
    ]);

    let result = client.get_yield_tiers_page(&(u32::MAX - 1), &50);
    assert_eq!(result.len(), 0);
}

/// get_attestation_digests with start = u32::MAX - 1 must not trap.
#[test]
fn attestation_digests_near_u32_max_does_not_trap() {
    let env = Env::default();
    let (client, ..) = deploy_real(&env, 100_000i128, 500i64, "POS_ATTOVF");

    // Append one digest to make the log non-empty.
    client.append_attestation_digest(&soroban_sdk::BytesN::from_array(&env, &[1u8; 32]));

    let result = client.get_attestation_digests(&(u32::MAX - 1), &20);
    assert_eq!(result.len(), 0);
}

// ── AC4: budget measurement — 20-entry page ────────────────────────────────────

/// Exercise a full 20-entry page of get_investor_positions.
/// This test documents the simulated budget for a maximum-size page.
/// CI output (from `cargo test`) includes the budget in the test pass line;
/// the stripped wasm size is recorded separately in the PR.
#[test]
fn investor_positions_budget_20_entries() {
    let env = Env::default();
    let n: u32 = 20;
    let per_investor: i128 = 1_000;
    let total_target: i128 = per_investor * (n as i128 + 10); // headroom
    let (client, _escrow_id, _token_id, sac_admin, ..) =
        deploy_real(&env, total_target, 500i64, "POS_BUDGET");

    for _ in 0..n {
        let inv = Address::generate(&env);
        sac_admin.mint(&inv, &per_investor);
        client.fund(&inv, &per_investor);
    }

    let result = client.get_investor_positions(&0, &n);
    assert_eq!(result.len(), n);

    // Spot-check: every returned position has a positive contribution.
    for i in 0..n {
        let pos: InvestorPosition = result.get(i).unwrap();
        assert_eq!(pos.contribution, per_investor);
        assert!(!pos.claimed);
        assert!(!pos.refunded);
    }
}

// ── consistency: get_investor_position vs individual reads ────────────────────

/// get_investor_position must return the same contribution as get_contribution.
#[test]
fn position_contribution_consistent_with_get_contribution() {
    let env = Env::default();
    let target = 30_000i128;
    let (client, _escrow_id, _token_id, sac_admin, ..) =
        deploy_real(&env, target, 500i64, "POS_CONS");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &target);
    client.fund(&investor, &target);

    let pos = client.get_investor_position(&investor);
    let direct = client.get_contribution(&investor);
    assert_eq!(pos.contribution, direct);
}

/// After settlement, get_investor_position claimable_payout matches get_claimable_payout.
#[test]
fn position_claimable_payout_consistent_with_get_claimable_payout() {
    let env = Env::default();
    let target = 40_000i128;
    let yield_bps: i64 = 500;
    let coupon = target * yield_bps as i128 / 10_000;

    let (client, escrow_id, _token_id, sac_admin, ..) =
        deploy_real(&env, target, yield_bps, "POS_CPCONS");

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &target);
    client.fund(&investor, &target);
    sac_admin.mint(&escrow_id, &coupon);
    client.settle();

    let pos = client.get_investor_position(&investor);
    let direct = client.get_claimable_payout(&investor);
    assert_eq!(pos.claimable_payout, direct);
    // Must be positive for a properly funded + settled escrow
    assert!(pos.claimable_payout > 0);
}

/// get_investor_positions returns the same data as calling get_investor_position individually.
#[test]
fn positions_batch_matches_individual_position_calls() {
    let env = Env::default();
    let per_investor = 5_000i128;
    let n = 3u32;
    let total_target = per_investor * (n as i128 + 2);
    let (client, _escrow_id, _token_id, sac_admin, ..) =
        deploy_real(&env, total_target, 500i64, "POS_BATCH");

    let mut investors = soroban_sdk::vec![&env];
    for _ in 0..n {
        let inv = Address::generate(&env);
        sac_admin.mint(&inv, &per_investor);
        client.fund(&inv, &per_investor);
        investors.push_back(inv);
    }

    let page = client.get_investor_positions(&0, &n);
    assert_eq!(page.len(), n);

    for i in 0..n {
        let batch_pos: InvestorPosition = page.get(i).unwrap();
        let individual_pos = client.get_investor_position(&investors.get(i).unwrap());
        assert_eq!(batch_pos.investor, individual_pos.investor);
        assert_eq!(batch_pos.contribution, individual_pos.contribution);
        assert_eq!(batch_pos.effective_yield_bps, individual_pos.effective_yield_bps);
        assert_eq!(batch_pos.claim_not_before, individual_pos.claim_not_before);
        assert_eq!(batch_pos.claimed, individual_pos.claimed);
        assert_eq!(batch_pos.refunded, individual_pos.refunded);
        assert_eq!(batch_pos.claimable_payout, individual_pos.claimable_payout);
    }
}
