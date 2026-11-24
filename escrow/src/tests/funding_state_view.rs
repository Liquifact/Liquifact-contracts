use super::*;
use crate::{CollateralCommitmentSnapshot, EscrowCloseSnapshot};

#[test]
fn test_funding_state_views_consistent_across_lifecycle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);
    let partial = TARGET / 4;

    // Before funding
    let summary = client.get_escrow_summary();
    let capacity = client.get_remaining_funding_capacity();
    assert_eq!(summary.escrow.status, 0);
    assert_eq!(summary.escrow.funded_amount, 0);
    assert_eq!(summary.unique_funder_count, 0);
    assert_eq!(summary.funding_close_snapshot, EscrowCloseSnapshot::None);
    assert_eq!(
        summary.sme_collateral_commitment,
        CollateralCommitmentSnapshot::None
    );
    assert_eq!(capacity, TARGET);
    assert_eq!(
        capacity,
        summary.escrow.funding_target - summary.escrow.funded_amount
    );

    // After partial funding
    client.fund(&investor_a, &partial);
    let summary = client.get_escrow_summary();
    let capacity = client.get_remaining_funding_capacity();
    assert_eq!(summary.escrow.status, 0);
    assert_eq!(summary.escrow.funded_amount, partial);
    assert_eq!(summary.unique_funder_count, 1);
    assert_eq!(summary.funding_close_snapshot, EscrowCloseSnapshot::None);
    assert_eq!(capacity, TARGET - partial);
    assert_eq!(
        capacity,
        summary.escrow.funding_target - summary.escrow.funded_amount
    );

    // At target - becomes funded, capacity 0
    client.fund(&investor_b, &(TARGET - partial));
    let summary = client.get_escrow_summary();
    let capacity = client.get_remaining_funding_capacity();
    assert_eq!(summary.escrow.status, 1);
    assert_eq!(summary.escrow.funded_amount, TARGET);
    assert_eq!(summary.unique_funder_count, 2);
    assert!(matches!(
        summary.funding_close_snapshot,
        EscrowCloseSnapshot::Some(_)
    ));
    assert_eq!(capacity, 0);
    assert_eq!(
        capacity,
        summary
            .escrow
            .funding_target
            .saturating_sub(summary.escrow.funded_amount)
    );
}

#[test]
fn test_remaining_investor_slots_uncapped_and_capped() {
    let env = Env::default();
    env.mock_all_auths();

    // Uncapped: always None
    let (uncapped, admin, sme) = setup(&env);
    default_init(&uncapped, &env, &admin, &sme);
    assert_eq!(uncapped.get_remaining_investor_slots(), None);
    uncapped.fund(&Address::generate(&env), &(TARGET / 10));
    assert_eq!(uncapped.get_remaining_investor_slots(), None);

    // Capped: decrements per unique investor
    let capped = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    capped.init(
        &admin,
        &String::from_str(&env, "FSV_CAP"),
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

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);

    assert_eq!(capped.get_remaining_investor_slots(), Some(2));
    assert_eq!(capped.get_unique_funder_count(), 0);

    capped.fund(&inv_a, &(TARGET / 4));
    assert_eq!(capped.get_remaining_investor_slots(), Some(1));
    assert_eq!(capped.get_unique_funder_count(), 1);
    assert_eq!(capped.get_escrow_summary().unique_funder_count, 1);

    // Repeat funding by same investor should not consume a slot.
    capped.fund(&inv_a, &(TARGET / 4));
    assert_eq!(capped.get_remaining_investor_slots(), Some(1));
    assert_eq!(capped.get_unique_funder_count(), 1);

    capped.fund(&inv_b, &(TARGET / 2));
    assert_eq!(capped.get_remaining_investor_slots(), Some(0));
    assert_eq!(capped.get_unique_funder_count(), 2);
    assert_eq!(capped.get_escrow_summary().unique_funder_count, 2);
    assert_eq!(capped.get_escrow_summary().escrow.status, 1);
}