use super::*;

const USDC: Symbol = symbol_short!("USDC");

fn funded(env: &Env) -> LiquifactEscrowClient<'_> {
    env.mock_all_auths();
    let (client, _escrow_id, _sme) = init_and_fund_with_real_token(env, 1_000, "COLL001");
    client
}

#[test]
fn clear_after_funding_fails_until_terminal() {
    let env = Env::default();
    let client = funded(&env);
    client.record_sme_collateral_commitment(&USDC, &100);
    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::CollateralLockedAfterFunding,
    );
    client.withdraw();
    client.clear_sme_collateral_commitment();
    assert!(client.get_sme_collateral_commitment().is_none());
}

#[test]
fn legal_hold_blocks_clear_and_record_after_funding() {
    let env = Env::default();
    let client = funded(&env);
    client.record_sme_collateral_commitment(&USDC, &100);
    client.set_legal_hold(&true);
    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::LegalHoldBlocksCollateralChange,
    );
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&USDC, &200),
        EscrowError::LegalHoldBlocksCollateralChange,
    );
}

#[test]
fn decrease_and_asset_change_fail_increase_succeeds_after_funding() {
    let env = Env::default();
    let client = funded(&env);
    client.record_sme_collateral_commitment(&USDC, &100);
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&USDC, &99),
        EscrowError::CollateralDecreaseAfterFunding,
    );
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&symbol_short!("XLM"), &500),
        EscrowError::CollateralAssetChangeAfterFunding,
    );
    client.record_sme_collateral_commitment(&USDC, &100);
    client.record_sme_collateral_commitment(&USDC, &150);
    assert_eq!(client.get_sme_collateral_commitment().unwrap().amount, 150);
}

#[test]
fn batch_rules_after_funding() {
    let env = Env::default();
    let client = funded(&env);
    client.record_sme_collateral_commitment(&USDC, &100);

    let mut mixed = SorobanVec::new(&env);
    mixed.push_back((USDC, 200i128));
    mixed.push_back((symbol_short!("XLM"), 300i128));
    assert_contract_error(
        client.try_batch_record_collateral(&mixed),
        EscrowError::CollateralBatchMixedAssets,
    );

    let mut lower = SorobanVec::new(&env);
    lower.push_back((USDC, 50i128));
    assert_contract_error(
        client.try_batch_record_collateral(&lower),
        EscrowError::CollateralDecreaseAfterFunding,
    );

    let mut other = SorobanVec::new(&env);
    other.push_back((symbol_short!("XLM"), 500i128));
    assert_contract_error(
        client.try_batch_record_collateral(&other),
        EscrowError::CollateralAssetChangeAfterFunding,
    );

    let mut ok = SorobanVec::new(&env);
    ok.push_back((USDC, 120i128));
    ok.push_back((USDC, 180i128));
    client.batch_record_collateral(&ok);
    assert_eq!(client.get_sme_collateral_commitment().unwrap().amount, 180);
}

#[test]
fn before_funding_behaviour_unchanged() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.record_sme_collateral_commitment(&USDC, &100);
    client.record_sme_collateral_commitment(&symbol_short!("XLM"), &10);
    client.set_legal_hold(&true);
    client.record_sme_collateral_commitment(&USDC, &5);
    client.clear_sme_collateral_commitment();
    assert!(client.get_sme_collateral_commitment().is_none());
}
