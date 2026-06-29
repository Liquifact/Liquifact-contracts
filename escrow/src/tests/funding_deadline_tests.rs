// funding_deadline_tests.rs

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Env, testutils::Ledger, Address, Symbol, String};

    fn setup_env_and_client() -> (EscrowClient, Env, Address, Address) {
        let env = Env::default();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let client = EscrowClient::new(&env);
        (client, env, admin, sme)
    }

    #[test]
    fn test_init_with_past_deadline_rejected() {
        let (client, env, admin, sme) = setup_env_and_client();
        env.ledger().set(Ledger::timestamp(1_000_000), Ledger::sequence_number(10));
        let past_deadline = env.ledger().timestamp() - 10;
        let result = client.init(
            &admin,
            &String::from_str(&env, "INV_DEADLINE_PAST"),
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
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &past_deadline,
        );
        assert_contract_error(result, EscrowError::FundingDeadlinePassed);
    }

    #[test]
    fn test_funding_before_and_after_deadline() {
        let (client, env, admin, sme) = setup_env_and_client();
        let now = env.ledger().timestamp();
        let future_deadline = now + 30;
        client.init(
            &admin,
            &String::from_str(&env, "INV_DEADLINE"),
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
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &future_deadline,
        ).unwrap();
        let investor = Address::generate(&env);
        let funded = client.fund(&investor, &TARGET);
        assert_eq!(funded.funded_amount, TARGET);
        env.ledger().set(Ledger::timestamp(future_deadline + 1), Ledger::sequence_number(20));
        let result = client.fund(&investor, &TARGET);
        assert_contract_error(result, EscrowError::FundingDeadlinePassed);
        let expired = client.is_funding_expired();
        assert!(expired);
    }
}
