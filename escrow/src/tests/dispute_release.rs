use super::*;

fn funded_client() -> (Env, LiquifactEscrowClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "DISPUTE001"),
        &sme,
        &1000i128,
        &100i64,
        &0u64,
        &token,
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
        &None::<u32>,
    );
    let investor = Address::generate(&env);
    client.fund(&investor, &900i128);
    (env, client, admin, sme)
}

#[test]
fn release_before_dispute_succeeds() {
    let (_, client, _, _) = funded_client();
    let before = client.get_escrow();
    assert_eq!(before.status, 0);

    let released = client.withdraw();
    assert_eq!(released.status, 3);
    assert!(!client.is_dispute_active());
}

#[test]
fn release_during_dispute_is_blocked() {
    let (env, client, admin, _) = funded_client();
    client.open_dispute(&admin);
    assert!(client.is_dispute_active());

    let result = client.try_withdraw();
    assert_contract_error(result, EscrowError::DisputeBlocksWithdrawal);
}

#[test]
fn dispute_opened_during_release_flow_blocks_release() {
    let (_, client, admin, _) = funded_client();
    client.open_dispute(&admin);
    let result = client.try_withdraw();
    assert_contract_error(result, EscrowError::DisputeBlocksWithdrawal);
}

#[test]
fn dispute_resolved_then_release_succeeds() {
    let (_, client, admin, _) = funded_client();
    client.open_dispute(&admin);
    client.close_dispute(&admin, &true);

    let released = client.withdraw();
    assert_eq!(released.status, 3);
    assert!(!client.is_dispute_active());
}

#[test]
fn unauthorized_dispute_close_panics() {
    let (env, client, _, _) = funded_client();
    let outsider = Address::generate(&env);
    let err = client.try_close_dispute(&outsider, &true);
    assert_contract_error(err, EscrowError::Unauthorized);
}
