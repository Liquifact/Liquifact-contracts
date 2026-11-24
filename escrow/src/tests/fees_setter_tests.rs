use super::*;

#[test]
fn test_performance_fee_default_is_zero() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    assert_eq!(client.get_performance_fee_bps(), 0);
}

#[test]
fn test_set_performance_fee_bps_success_and_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    let invoice_id = client.get_escrow().invoice_id;
    let res = client.set_performance_fee_bps(&1_000);

    // Verify PerformanceFeeUpdated event (read before any further invocation).
    let expected = PerformanceFeeUpdated {
        name: symbol_short!("perf_upd"),
        invoice_id,
        old_fee_bps: 0,
        new_fee_bps: 1_000,
    }
    .to_xdr(&env, &client.address);
    let found = env.events().all().events().iter().any(|e| *e == expected);
    assert!(found, "PerformanceFeeUpdated event was not found");

    assert_eq!(res, 1_000);
    assert_eq!(client.get_performance_fee_bps(), 1_000);
}

#[test]
fn test_set_performance_fee_bps_out_of_range() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    // Boundary: 5_000 is allowed
    assert_eq!(client.set_performance_fee_bps(&5_000), 5_000);

    // 5_001 is out of range
    assert_contract_error(
        client.try_set_performance_fee_bps(&5_001),
        EscrowError::PerformanceFeeBpsOutOfRange,
    );
}

#[test]
fn test_set_performance_fee_bps_rejects_non_admin() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    env.mock_auths(&[]);
    assert!(client.try_set_performance_fee_bps(&1_000).is_err());
}
