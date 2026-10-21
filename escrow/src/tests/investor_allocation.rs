use super::*;

const T: i128 = 10_000;

fn setup_cap<'a>(
    env: &'a Env,
    cap: Option<i128>,
) -> (LiquifactEscrowClient<'a>, StellarAssetClient<'a>) {
    let (client, admin, sme) = setup(env);
    let tok = install_stellar_asset_token(env);
    client.init(
        &admin,
        &String::from_str(env, "ALLOC1"),
        &sme,
        &T,
        &800i64,
        &0u64,
        &tok.id,
        &None,
        &Address::generate(env),
        &None,
        &None,
        &None,
        &cap,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    (client, tok.stellar)
}

#[test]
fn anchor_allocation_overrides_global_cap() {
    let env = Env::default();
    let (client, sac) = setup_cap(&env, Some(100));
    let anchor = Address::generate(&env);
    let other = Address::generate(&env);
    sac.mint(&anchor, &1_000);
    sac.mint(&other, &1_000);

    client.set_investor_allocation(&anchor, &Some(400));
    assert_eq!(client.get_investor_effective_cap(&anchor), Some(400));
    assert_eq!(client.get_investor_effective_cap(&other), Some(100));

    client.fund(&anchor, &400);
    assert_eq!(client.get_contribution(&anchor), 400);
    assert_contract_error(
        client.try_fund(&anchor, &1),
        EscrowError::InvestorContributionExceedsCap,
    );

    client.fund(&other, &100);
    assert_contract_error(
        client.try_fund(&other, &1),
        EscrowError::InvestorContributionExceedsCap,
    );
    assert_eq!(client.get_contribution(&other), 100);
    assert_eq!(client.get_escrow().funded_amount, 500);
}

#[test]
fn allocation_validation_and_clear_falls_back() {
    let env = Env::default();
    let (client, sac) = setup_cap(&env, Some(100));
    let inv = Address::generate(&env);
    sac.mint(&inv, &1_000);

    assert_contract_error(
        client.try_set_investor_allocation(&inv, &Some(0)),
        EscrowError::InvestorAllocationInvalid,
    );
    client.set_investor_allocation(&inv, &Some(300));
    client.fund(&inv, &200);
    assert_contract_error(
        client.try_set_investor_allocation(&inv, &Some(199)),
        EscrowError::InvestorAllocationInvalid,
    );
    assert_eq!(client.get_investor_effective_cap(&inv), Some(300));

    client.set_investor_allocation(&inv, &None);
    assert_eq!(client.get_investor_effective_cap(&inv), Some(100));
    assert_contract_error(
        client.try_fund(&inv, &1),
        EscrowError::InvestorContributionExceedsCap,
    );
}

#[test]
fn no_cap_and_no_allocation_is_unlimited() {
    let env = Env::default();
    let (client, _) = setup_cap(&env, None);
    let inv = Address::generate(&env);
    assert_eq!(client.get_investor_effective_cap(&inv), None);
    client.set_investor_allocation(&inv, &Some(5));
    assert_eq!(client.get_investor_effective_cap(&inv), Some(5));
}

#[test]
fn batch_setter_is_atomic() {
    let env = Env::default();
    let (client, sac) = setup_cap(&env, Some(100));
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    sac.mint(&b, &1_000);
    client.fund(&b, &100);

    let mut bad: SorobanVec<(Address, Option<i128>)> = SorobanVec::new(&env);
    bad.push_back((a.clone(), Some(400)));
    bad.push_back((b.clone(), Some(50))); // below b's contribution
    assert_contract_error(
        client.try_set_investor_allocations(&bad),
        EscrowError::InvestorAllocationInvalid,
    );
    assert_eq!(client.get_investor_effective_cap(&a), Some(100));

    let mut good: SorobanVec<(Address, Option<i128>)> = SorobanVec::new(&env);
    good.push_back((a.clone(), Some(400)));
    good.push_back((b.clone(), Some(150)));
    client.set_investor_allocations(&good);
    assert_eq!(client.get_investor_effective_cap(&a), Some(400));
    assert_eq!(client.get_investor_effective_cap(&b), Some(150));
}

#[test]
fn fund_batch_respects_allocations_per_entry() {
    let env = Env::default();
    let (client, sac) = setup_cap(&env, Some(100));
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    sac.mint(&a, &1_000);
    sac.mint(&b, &1_000);
    client.set_investor_allocation(&a, &Some(400));

    let mut over: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    over.push_back((a.clone(), 400));
    over.push_back((b.clone(), 101));
    assert_contract_error(
        client.try_fund_batch(&over),
        EscrowError::InvestorContributionExceedsCap,
    );
    assert_eq!(client.get_contribution(&a), 0);

    let mut ok: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    ok.push_back((a.clone(), 400));
    ok.push_back((b.clone(), 100));
    client.fund_batch(&ok);
    assert_eq!(client.get_contribution(&a), 400);
    assert_eq!(client.get_contribution(&b), 100);
}

#[test]
fn allocation_requires_open_status() {
    let env = Env::default();
    let (client, sac) = setup_cap(&env, None);
    let inv = Address::generate(&env);
    sac.mint(&inv, &T);
    client.fund(&inv, &T);
    assert_contract_error(
        client.try_set_investor_allocation(&inv, &Some(T)),
        EscrowError::EscrowNotOpenForFunding,
    );
}
