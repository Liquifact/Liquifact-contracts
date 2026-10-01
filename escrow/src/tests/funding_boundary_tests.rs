use super::*;

use crate::EscrowError;

// ── fund amount boundaries ──────────────────────────────────────

#[test]
fn test_fund_negative_amount_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    assert_contract_error(
        client.try_fund(&investor, &(-1i128)),
        EscrowError::FundingAmountNotPositive,
    );
}

#[test]
fn test_fund_i128_min_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    assert_contract_error(
        client.try_fund(&investor, &i128::MIN),
        EscrowError::FundingAmountNotPositive,
    );
}

#[test]
fn test_fund_i128_max_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    let funded = client.fund(&investor, &i128::MAX);
    assert_eq!(funded.funded_amount, i128::MAX);
    assert_eq!(funded.status, 0);
}

#[test]
fn test_fund_exact_max_invoice_amount_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    let funded = client.fund(&investor, &crate::MAX_INVOICE_AMOUNT);
    assert_eq!(funded.funded_amount, crate::MAX_INVOICE_AMOUNT);
}

// ── fund_with_commitment amount boundaries ──────────────────────

#[test]
fn test_fund_with_commitment_negative_amount_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    assert_contract_error(
        client.try_fund_with_commitment(&investor, &(-1i128), &0u64),
        EscrowError::FundingAmountNotPositive,
    );
}

#[test]
fn test_fund_with_commitment_i128_max_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    let funded = client.fund_with_commitment(&investor, &i128::MAX, &0u64);
    assert_eq!(funded.funded_amount, i128::MAX);
}

// ── fund_batch size boundaries ─────────────────────────────────

#[test]
fn test_fund_batch_empty_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let entries: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingBatchEmpty,
    );
}

#[test]
fn test_fund_batch_too_large_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "BATCHBIG"),
        &sme,
        &1_000_000_000i128,
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
        &None::<i64>,
    );
    let mut entries: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    for _ in 0..(crate::MAX_FUND_BATCH + 1) {
        entries.push_back((Address::generate(&env), 1_000i128));
    }
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingBatchTooLarge,
    );
}

// ── fund_batch entry-level boundaries ──────────────────────────

#[test]
fn test_fund_batch_zero_entry_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let inv = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    let mut entries: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    entries.push_back((inv, 0i128));
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingAmountNotPositive,
    );
}

#[test]
fn test_fund_batch_negative_entry_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let inv = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);
    let mut entries: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    entries.push_back((inv, (-1i128)));
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingAmountNotPositive,
    );
}

#[test]
fn test_fund_batch_entry_below_floor_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "BATCHFLR"),
        &sme,
        &100_000i128,
        &800i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &Some(5_000i128),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let inv = Address::generate(&env);
    let mut entries: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    entries.push_back((inv, 1_000i128));
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingBelowMinContribution,
    );
}

// ── unfund amount boundaries ────────────────────────────────────

#[test]
fn test_unfund_zero_amount_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "UNFUNDZ"),
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
    client.fund(&investor, &10_000i128);
    assert_contract_error(
        client.try_unfund(&investor, &0i128),
        EscrowError::OverWithdrawal,
    );
}

#[test]
fn test_unfund_negative_amount_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "UNFUNDN"),
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
    client.fund(&investor, &10_000i128);
    assert_contract_error(
        client.try_unfund(&investor, &(-1i128)),
        EscrowError::OverWithdrawal,
    );
}

// ── refund_batch size boundaries ────────────────────────────────

#[test]
fn test_refund_batch_empty_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investors: SorobanVec<Address> = SorobanVec::new(&env);
    assert_contract_error(
        client.try_refund_batch(&investors),
        EscrowError::RefundBatchEmpty,
    );
}

#[test]
fn test_refund_batch_too_large_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let mut investors: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..(crate::MAX_REFUND_BATCH + 1) {
        investors.push_back(Address::generate(&env));
    }
    assert_contract_error(
        client.try_refund_batch(&investors),
        EscrowError::RefundBatchTooLarge,
    );
}

// ── init amount boundaries ──────────────────────────────────────

#[test]
fn test_init_negative_amount_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "INITNEG"),
            &sme,
            &(-1i128),
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
        ),
        EscrowError::AmountMustBePositive,
    );
}

#[test]
fn test_init_zero_amount_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "INITZERO"),
            &sme,
            &0i128,
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
        ),
        EscrowError::AmountMustBePositive,
    );
}

#[test]
fn test_init_exact_max_invoice_amount_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (_tok, _tre) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INITMAX"),
        &sme,
        &crate::MAX_INVOICE_AMOUNT,
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
}

#[test]
fn test_init_above_max_invoice_amount_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "INITOVR"),
            &sme,
            &(crate::MAX_INVOICE_AMOUNT + 1),
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
        ),
        EscrowError::AmountExceedsMax,
    );
}
