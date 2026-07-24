use super::*;
use crate::EscrowError;

// ─────────────────────────────────────────────────────────────────────────────
// Focused boundary and rejection tests for fund, fund_with_commitment, and
// fund_batch entrypoints.  Every test asserts the *exact* typed EscrowError
// code (via `try_*` + `assert_contract_error`), covers both the accept and
// reject side of a boundary, and exercises one-over / one-under where
// applicable.
// ─────────────────────────────────────────────────────────────────────────────

// ---------------------------------------------------------------------------
// 1. fund() — Amount validation
// ---------------------------------------------------------------------------

#[test]
fn fund_negative_amount_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&investor, &(-1i128)),
        EscrowError::FundingAmountNotPositive,
    );
}

#[test]
fn fund_zero_amount_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&investor, &0i128),
        EscrowError::FundingAmountNotPositive,
    );
}

#[test]
fn fund_positive_one_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    let escrow = client.fund(&investor, &1i128);
    assert_eq!(escrow.funded_amount, 1i128);
    assert_eq!(escrow.status, 0);
}

// ---------------------------------------------------------------------------
// 2. fund() — Min contribution floor boundary
// ---------------------------------------------------------------------------

#[test]
fn fund_exactly_at_floor_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let floor = 1_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "FLRBD1"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(floor),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let investor = Address::generate(&env);
    let escrow = client.fund(&investor, &floor);
    assert_eq!(escrow.funded_amount, floor);
}

#[test]
fn fund_one_below_floor_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let floor = 1_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "FLRBD2"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(floor),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&investor, &(floor - 1)),
        EscrowError::FundingBelowMinContribution,
    );
}

#[test]
fn fund_one_above_floor_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let floor = 1_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "FLRBD3"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(floor),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let investor = Address::generate(&env);
    let escrow = client.fund(&investor, &(floor + 1));
    assert_eq!(escrow.funded_amount, floor + 1);
}

#[test]
fn fund_follow_on_below_floor_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let floor = 5_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "FLRBD4"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(floor),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &floor);
    // Follow-on below floor must be rejected.
    assert_contract_error(
        client.try_fund(&investor, &(floor - 1)),
        EscrowError::FundingBelowMinContribution,
    );
}

// ---------------------------------------------------------------------------
// 3. fund() — Status guards (typed errors)
// ---------------------------------------------------------------------------

#[test]
fn fund_rejects_after_funded_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    client.fund(&investor, &TARGET);
    assert_eq!(client.get_escrow().status, 1);
    let other = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&other, &1i128),
        EscrowError::EscrowNotOpenForFunding,
    );
}

#[test]
fn fund_rejects_after_settled_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    client.fund(&investor, &TARGET);
    client.settle();
    assert_eq!(client.get_escrow().status, 2);
    let other = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&other, &1i128),
        EscrowError::EscrowNotOpenForFunding,
    );
}

#[test]
fn fund_rejects_after_withdrawn_typed() {
    let env = Env::default();
    env.mock_all_auths();
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &token_id);
    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "STBD4"),
        &sme,
        &TARGET,
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
    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &TARGET);
    client.fund(&investor, &TARGET);
    sac_admin.mint(&escrow_id, &TARGET);
    client.withdraw();
    assert_eq!(client.get_escrow().status, 3);
    let other = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&other, &1i128),
        EscrowError::EscrowNotOpenForFunding,
    );
}

#[test]
fn fund_rejects_after_cancelled_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.cancel_funding();
    assert_eq!(client.get_escrow().status, 4);
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&investor, &1i128),
        EscrowError::EscrowNotOpenForFunding,
    );
}

// ---------------------------------------------------------------------------
// 4. fund() — Legal hold (typed error)
// ---------------------------------------------------------------------------

#[test]
fn fund_rejects_during_legal_hold_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_legal_hold(&true);
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&investor, &1i128),
        EscrowError::LegalHoldBlocksFunding,
    );
}

// ---------------------------------------------------------------------------
// 5. fund() — Operational pause (typed error)
// ---------------------------------------------------------------------------

#[test]
fn fund_rejects_when_paused_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_paused(&true);
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&investor, &1i128),
        EscrowError::PausedBlocksFunding,
    );
}

#[test]
fn fund_succeeds_after_unpause_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_paused(&true);
    client.set_paused(&false);
    let investor = Address::generate(&env);
    let escrow = client.fund(&investor, &1i128);
    assert_eq!(escrow.funded_amount, 1i128);
}

// ---------------------------------------------------------------------------
// 6. fund() — Allowlist gate (typed error)
// ---------------------------------------------------------------------------

#[test]
fn fund_rejects_when_allowlist_active_and_not_listed_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_allowlist_active(&true);
    let investor = Address::generate(&env);
    // Investor not explicitly allowlisted — default deny.
    assert_contract_error(
        client.try_fund(&investor, &1i128),
        EscrowError::InvestorNotAllowlisted,
    );
}

#[test]
fn fund_succeeds_when_allowlisted_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_allowlist_active(&true);
    let investor = Address::generate(&env);
    client.set_investor_allowlisted(&investor, &true);
    let escrow = client.fund(&investor, &1i128);
    assert_eq!(escrow.funded_amount, 1i128);
}

#[test]
fn fund_succeeds_when_allowlist_inactive_regardless_of_entry_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    // Allowlist inactive — even if investor is allowlisted, it does not matter.
    client.set_allowlist_active(&false);
    let investor = Address::generate(&env);
    // No set_investor_allowlisted call — should still succeed.
    let escrow = client.fund(&investor, &1i128);
    assert_eq!(escrow.funded_amount, 1i128);
}

// ---------------------------------------------------------------------------
// 7. fund() — Per-investor cap boundary
// ---------------------------------------------------------------------------

#[test]
fn fund_exactly_at_per_investor_cap_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let cap = 10_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "INVCAP1"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(cap),
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let investor = Address::generate(&env);
    let escrow = client.fund(&investor, &cap);
    assert_eq!(escrow.funded_amount, cap);
    assert_eq!(client.get_contribution(&investor), cap);
}

#[test]
fn fund_one_over_per_investor_cap_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let cap = 10_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "INVCAP2"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(cap),
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&investor, &(cap + 1)),
        EscrowError::InvestorContributionExceedsCap,
    );
}

#[test]
fn fund_follow_on_one_over_per_investor_cap_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let cap = 10_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "INVCAP3"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(cap),
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &(cap - 1));
    // Follow-on of 2 would make total cap+1.
    assert_contract_error(
        client.try_fund(&investor, &2i128),
        EscrowError::InvestorContributionExceedsCap,
    );
}

// ---------------------------------------------------------------------------
// 8. fund() — Unique investor cap boundary
// ---------------------------------------------------------------------------

#[test]
fn fund_exactly_at_unique_investor_cap_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "UNQCAP1"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(2u32),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    client.fund(&inv1, &(TARGET / 4));
    client.fund(&inv2, &(TARGET / 4));
    assert_eq!(client.get_unique_funder_count(), 2);
}

#[test]
fn fund_one_over_unique_investor_cap_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "UNQCAP2"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(2u32),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    client.fund(&inv1, &(TARGET / 4));
    client.fund(&inv2, &(TARGET / 4));
    let inv3 = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&inv3, &(TARGET / 4)),
        EscrowError::UniqueInvestorCapReached,
    );
}

#[test]
fn fund_existing_investor_not_counted_against_cap_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "UNQCAP3"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(1u32),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let inv = Address::generate(&env);
    client.fund(&inv, &(TARGET / 3));
    assert_eq!(client.get_unique_funder_count(), 1);
    // Follow-on from same investor must succeed.
    let escrow = client.fund(&inv, &(TARGET / 3));
    assert_eq!(escrow.funded_amount, TARGET * 2 / 3);
    assert_eq!(client.get_unique_funder_count(), 1);
}

// ---------------------------------------------------------------------------
// 9. fund() — Funded amount transition at exact boundary
// ---------------------------------------------------------------------------

#[test]
fn fund_exact_target_transitions_to_funded_status() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let small_target = 100i128;
    client.init(
        &admin,
        &String::from_str(&env, "TRGBD1"),
        &sme,
        &small_target,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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
    let investor = Address::generate(&env);
    let escrow = client.fund(&investor, &small_target);
    assert_eq!(escrow.status, 1);
    assert_eq!(escrow.funded_amount, small_target);
}

#[test]
fn fund_one_below_target_stays_open() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let small_target = 100i128;
    client.init(
        &admin,
        &String::from_str(&env, "TRGBD2"),
        &sme,
        &small_target,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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
    let investor = Address::generate(&env);
    let escrow = client.fund(&investor, &(small_target - 1));
    assert_eq!(escrow.status, 0);
    assert_eq!(escrow.funded_amount, small_target - 1);
}

#[test]
fn fund_one_over_target_transitions_to_funded_status() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let small_target = 100i128;
    client.init(
        &admin,
        &String::from_str(&env, "TRGBD3"),
        &sme,
        &small_target,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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
    let investor = Address::generate(&env);
    let escrow = client.fund(&investor, &(small_target + 1));
    assert_eq!(escrow.status, 1);
    assert_eq!(escrow.funded_amount, small_target + 1);
}

// ---------------------------------------------------------------------------
// 10. fund_with_commitment() — Boundary tests
// ---------------------------------------------------------------------------

#[test]
fn fund_with_commitment_negative_amount_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund_with_commitment(&investor, &(-1i128), &0u64),
        EscrowError::FundingAmountNotPositive,
    );
}

#[test]
fn fund_with_commitment_zero_amount_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund_with_commitment(&investor, &0i128, &0u64),
        EscrowError::FundingAmountNotPositive,
    );
}

#[test]
fn fund_with_commitment_after_first_deposit_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    // First deposit via fund_with_commitment.
    client.fund_with_commitment(&investor, &(TARGET / 2), &0u64);
    // Second deposit via fund_with_commitment must be rejected.
    assert_contract_error(
        client.try_fund_with_commitment(&investor, &(TARGET / 2), &0u64),
        EscrowError::TieredSecondDeposit,
    );
}

#[test]
fn fund_with_commitment_after_plain_fund_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    client.fund(&investor, &(TARGET / 2));
    assert_contract_error(
        client.try_fund_with_commitment(&investor, &(TARGET / 2), &0u64),
        EscrowError::TieredSecondDeposit,
    );
}

#[test]
fn fund_with_commitment_rejects_when_paused_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_paused(&true);
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund_with_commitment(&investor, &1i128, &0u64),
        EscrowError::PausedBlocksFunding,
    );
}

#[test]
fn fund_with_commitment_rejects_during_legal_hold_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_legal_hold(&true);
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund_with_commitment(&investor, &1i128, &0u64),
        EscrowError::LegalHoldBlocksFunding,
    );
}

#[test]
fn fund_with_commitment_rejects_after_funded_status_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    client.fund(&investor, &TARGET);
    assert_eq!(client.get_escrow().status, 1);
    let other = Address::generate(&env);
    assert_contract_error(
        client.try_fund_with_commitment(&other, &1i128, &0u64),
        EscrowError::EscrowNotOpenForFunding,
    );
}

#[test]
fn fund_with_commitment_rejects_when_not_allowlisted_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_allowlist_active(&true);
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund_with_commitment(&investor, &1i128, &0u64),
        EscrowError::InvestorNotAllowlisted,
    );
}

#[test]
fn fund_with_commitment_lock_exceeds_maturity_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let maturity = 1000u64;
    client.init(
        &admin,
        &String::from_str(&env, "LKBD1"),
        &sme,
        &10_000i128,
        &800i64,
        &maturity,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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
    let investor = Address::generate(&env);
    // Lock period that pushes claim_nb past maturity.
    assert_contract_error(
        client.try_fund_with_commitment(&investor, &1_000i128, &(maturity + 1)),
        EscrowError::CommitmentLockExceedsMaturity,
    );
}

#[test]
fn fund_with_commitment_lock_at_maturity_succeeds_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let maturity = 1000u64;
    client.init(
        &admin,
        &String::from_str(&env, "LKBD2"),
        &sme,
        &10_000i128,
        &800i64,
        &maturity,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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
    let investor = Address::generate(&env);
    // Lock period exactly at maturity — should succeed.
    let escrow = client.fund_with_commitment(&investor, &1_000i128, &maturity);
    assert_eq!(escrow.funded_amount, 1_000i128);
}

#[test]
fn fund_with_commitment_lock_one_under_maturity_succeeds_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let maturity = 1000u64;
    client.init(
        &admin,
        &String::from_str(&env, "LKBD3"),
        &sme,
        &10_000i128,
        &800i64,
        &maturity,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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
    let investor = Address::generate(&env);
    let escrow = client.fund_with_commitment(&investor, &1_000i128, &(maturity - 1));
    assert_eq!(escrow.funded_amount, 1_000i128);
}

// ---------------------------------------------------------------------------
// 11. fund_batch() — Pre-validation boundary tests
// ---------------------------------------------------------------------------

#[test]
fn fund_batch_empty_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let empty: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    assert_contract_error(
        client.try_fund_batch(&empty),
        EscrowError::FundingBatchEmpty,
    );
}

#[test]
fn fund_batch_oversized_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let mut entries = SorobanVec::new(&env);
    for _ in 0..=(MAX_FUND_BATCH as usize) {
        entries.push_back((Address::generate(&env), 1_000i128));
    }
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingBatchTooLarge,
    );
}

#[test]
fn fund_batch_zero_amount_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let inv = Address::generate(&env);
    let entries = soroban_sdk::vec![&env, (inv, 0i128)];
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingAmountNotPositive,
    );
}

#[test]
fn fund_batch_negative_amount_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let inv = Address::generate(&env);
    let entries = soroban_sdk::vec![&env, (inv, -5i128)];
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingAmountNotPositive,
    );
}

#[test]
fn fund_batch_below_floor_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let floor = 5_000i128;
    client.init(
        &admin,
        &String::from_str(&env, "BTFLR1"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(floor),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let inv = Address::generate(&env);
    let entries = soroban_sdk::vec![&env, (inv, floor - 1)];
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingBelowMinContribution,
    );
}

#[test]
fn fund_batch_duplicate_address_rejects_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let inv = Address::generate(&env);
    let entries = soroban_sdk::vec![&env, (inv.clone(), 1_000i128), (inv, 2_000i128)];
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingBatchDuplicateInvestor,
    );
}

#[test]
fn fund_batch_rejects_after_funded_status_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    client.fund(&investor, &TARGET);
    assert_eq!(client.get_escrow().status, 1);
    let inv = Address::generate(&env);
    let entries = soroban_sdk::vec![&env, (inv, 1i128)];
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::EscrowNotOpenForFunding,
    );
}

#[test]
fn fund_batch_rejects_during_legal_hold_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_legal_hold(&true);
    let inv = Address::generate(&env);
    let entries = soroban_sdk::vec![&env, (inv, 1i128)];
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::LegalHoldBlocksFunding,
    );
}

#[test]
fn fund_batch_rejects_when_paused_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.set_paused(&true);
    let inv = Address::generate(&env);
    let entries = soroban_sdk::vec![&env, (inv, 1i128)];
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::PausedBlocksFunding,
    );
}

#[test]
fn fund_batch_negative_in_second_entry_rejects_atomically_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let entries = soroban_sdk::vec![&env, (inv_a, 1_000i128), (inv_b, -5i128)];
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingAmountNotPositive,
    );
    // Verify no partial state mutation.
    assert_eq!(client.get_escrow().funded_amount, 0);
}

// ---------------------------------------------------------------------------
// 12. fund() — Event verification at boundary transition
// ---------------------------------------------------------------------------

#[test]
fn fund_emits_funded_event_on_transition_to_funded() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    let small_target = 100i128;
    client.init(
        &admin,
        &String::from_str(&env, "EVTBD1"),
        &sme,
        &small_target,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
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
    let inv = Address::generate(&env);
    let escrow = client.fund(&inv, &small_target);
    assert_eq!(escrow.status, 1);

    let events = env.events().all();
    let events_list = events.events();
    let last = events_list.last().expect("expected funded event");
    assert_eq!(
        *last,
        EscrowFunded {
            name: symbol_short!("funded"),
            invoice_id: symbol_short!("EVTBD1"),
            investor: inv,
            amount: small_target,
            funded_amount: small_target,
            status: 1,
            investor_effective_yield_bps: 800,
            tier_lock_secs: 0,
        }
        .to_xdr(&env, &contract_id)
    );
}

#[test]
fn fund_emits_funded_event_when_still_open() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "EVTBD2"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
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
    let inv = Address::generate(&env);
    let small_deposit = TARGET / 4;
    let escrow = client.fund(&inv, &small_deposit);
    assert_eq!(escrow.status, 0);

    let events = env.events().all();
    let events_list = events.events();
    let last = events_list.last().expect("expected funded event");
    assert_eq!(
        *last,
        EscrowFunded {
            name: symbol_short!("funded"),
            invoice_id: symbol_short!("EVTBD2"),
            investor: inv,
            amount: small_deposit,
            funded_amount: small_deposit,
            status: 0,
            investor_effective_yield_bps: 800,
            tier_lock_secs: 0,
        }
        .to_xdr(&env, &contract_id)
    );
}

// ---------------------------------------------------------------------------
// 13. fund() — Auth verification for all three entrypoints
// ---------------------------------------------------------------------------

#[test]
fn fund_requires_investor_auth_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    client.fund(&investor, &1i128);
    assert!(
        env.auths().iter().any(|(addr, _)| *addr == investor),
        "investor auth must be recorded"
    );
}

#[test]
fn fund_with_commitment_requires_investor_auth_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    client.fund_with_commitment(&investor, &1i128, &0u64);
    assert!(
        env.auths().iter().any(|(addr, _)| *addr == investor),
        "investor auth must be recorded for fund_with_commitment"
    );
}

#[test]
fn fund_batch_requires_per_investor_auth_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let entries = soroban_sdk::vec![&env, (inv_a.clone(), 1_000i128), (inv_b.clone(), 2_000i128)];
    client.fund_batch(&entries);
    let authed: std::vec::Vec<Address> = env.auths().iter().map(|(addr, _)| addr.clone()).collect();
    assert!(authed.contains(&inv_a), "inv_a auth must be recorded");
    assert!(authed.contains(&inv_b), "inv_b auth must be recorded");
}

// ---------------------------------------------------------------------------
// 14. fund() — Minimal target edge case (target = 1)
// ---------------------------------------------------------------------------

#[test]
fn fund_minimal_target_one_unit_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "MNMTGT"),
        &sme,
        &1i128,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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
    let investor = Address::generate(&env);
    let escrow = client.fund(&investor, &1i128);
    assert_eq!(escrow.status, 1);
    assert_eq!(escrow.funded_amount, 1i128);
}

// ---------------------------------------------------------------------------
// 15. fund_batch — Positive boundary: at MAX_FUND_BATCH exactly
// ---------------------------------------------------------------------------

#[test]
fn fund_batch_at_max_size_exactly_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    client.init(
        &admin,
        &String::from_str(&env, "MXBD1"),
        &sme,
        &(MAX_FUND_BATCH as i128 * 100),
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
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
    let mut entries = SorobanVec::new(&env);
    for _ in 0..(MAX_FUND_BATCH as usize) {
        entries.push_back((Address::generate(&env), 100i128));
    }
    let result = client.fund_batch(&entries);
    assert_eq!(
        result.funded_amount,
        MAX_FUND_BATCH as i128 * 100,
        "all entries must be recorded"
    );
}

// ---------------------------------------------------------------------------
// 16. fund() — Contribution accumulation does not clobber existing investor
// ---------------------------------------------------------------------------

#[test]
fn fund_follow_on_preserves_existing_contribution_typed() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let investor = Address::generate(&env);
    let first = 3_000i128;
    let second = 7_000i128;
    client.fund(&investor, &first);
    assert_eq!(client.get_contribution(&investor), first);
    client.fund(&investor, &second);
    assert_eq!(client.get_contribution(&investor), first + second);
    assert_eq!(client.get_escrow().funded_amount, first + second);
}

// ---------------------------------------------------------------------------
// 17. fund() — UniqueFunderCount invariants across boundary transitions
// ---------------------------------------------------------------------------

#[test]
fn fund_unique_funder_count_unchanged_after_funded_transition() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let small_target = 100i128;
    client.init(
        &admin,
        &String::from_str(&env, "UNQBD1"),
        &sme,
        &small_target,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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
    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    client.fund(&inv1, &(small_target / 2));
    assert_eq!(client.get_unique_funder_count(), 1);
    assert_eq!(client.get_escrow().status, 0);
    // This fund transitions to funded status.
    client.fund(&inv2, &(small_target / 2));
    assert_eq!(client.get_escrow().status, 1);
    assert_eq!(client.get_unique_funder_count(), 2);
}

#[test]
fn fund_overfunding_unique_funder_count_stays_correct() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let small_target = 100i128;
    client.init(
        &admin,
        &String::from_str(&env, "UNQBD2"),
        &sme,
        &small_target,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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
    // Single investor overfunds.
    let inv = Address::generate(&env);
    client.fund(&inv, &(small_target + 50));
    assert_eq!(client.get_escrow().status, 1);
    assert_eq!(client.get_unique_funder_count(), 1);
    // Follow-on from same investor after funded must be rejected — escrow is no longer open.
    assert_contract_error(
        client.try_fund(&inv, &10),
        EscrowError::EscrowNotOpenForFunding,
    );
}
