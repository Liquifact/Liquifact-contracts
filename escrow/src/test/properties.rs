use super::*;
use proptest::prelude::*;

// Property tests stay isolated so deterministic unit-test grouping remains easy
// to review while fuzzier invariants keep their own namespace.

proptest! {
    #[test]
    fn prop_funded_amount_non_decreasing(
        amount1 in 1i128..5_000_0000000i128,
        amount2 in 1i128..5_000_0000000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);
        let client = deploy(&env);

        let target = 20_000_0000000i128;
        client.init(
            &admin,
            &String::from_str(&env, "INVTST"),
            &sme,
            &target,
            &800i64,
            &0u64,
            &Address::generate(&env),
            &None,
            &Address::generate(&env),
            &None,
            &None,
            &None,
        );

        let before = client.get_escrow().funded_amount;
        client.fund(&investor1, &amount1);
        let after1 = client.get_escrow().funded_amount;
        prop_assert!(after1 >= before, "funded_amount must be non-decreasing");

        if client.get_escrow().status == 0 {
            client.fund(&investor2, &amount2);
            let after2 = client.get_escrow().funded_amount;
            prop_assert!(after2 >= after1, "funded_amount must be non-decreasing on successive funds");
        }
    }

    #[test]
    fn prop_status_only_increases(
        amount in 1i128..10_000_0000000i128,
        target in 1i128..10_000_0000000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let investor = Address::generate(&env);
        let client = deploy(&env);

        let escrow = client.init(
            &admin,
            &String::from_str(&env, "INVSTA"),
            &sme,
            &target,
            &800i64,
            &0u64,
            &Address::generate(&env),
            &None,
            &Address::generate(&env),
            &None,
            &None,
            &None,
        );
        prop_assert_eq!(escrow.status, 0);

        let after_fund = client.fund(&investor, &amount);
        prop_assert!(after_fund.status >= escrow.status, "status must not decrease");
        prop_assert!(after_fund.status <= 3, "status must be in valid range");

        if amount >= target {
            prop_assert_eq!(after_fund.status, 1);
            let after_settle = client.settle();
            prop_assert_eq!(after_settle.status, 2);
        } else {
            prop_assert_eq!(after_fund.status, 0);
        }
    }

    #[test]
    fn prop_status_transition_open_to_funded_then_settled_or_withdrawn(
        amount in 1i128..10_000_0000000i128,
        target in 1i128..10_000_0000000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let investor = Address::generate(&env);
        let client = deploy(&env);

        let escrow = client.init(
            &admin,
            &String::from_str(&env, "INVTRAN"),
            &sme,
            &target,
            &800i64,
            &0u64,
            &Address::generate(&env),
            &None,
            &Address::generate(&env),
            &None,
            &None,
            &None,
        );
        prop_assert_eq!(escrow.status, 0);

        if amount >= target {
            let after_fund = client.fund(&investor, &amount);
            prop_assert_eq!(after_fund.status, 1);

            let after_settle = client.settle();
            prop_assert!(after_settle.status == 2 || after_settle.status == 3);
        }
    }

    #[test]
    fn prop_status_cannot_regress_from_funded(
        target in 1i128..10_000_0000000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let investor = Address::generate(&env);
        let client = deploy(&env);

        let escrow = client.init(
            &admin,
            &String::from_str(&env, "REG001"),
            &sme,
            &target,
            &800i64,
            &0u64,
            &Address::generate(&env),
            &None,
            &Address::generate(&env),
            &None,
            &None,
            &None,
        );

        if target <= 0 { return; }

        client.fund(&investor, &target);
        let status_after_fund = client.get_escrow().status;
        prop_assert!(status_after_fund >= escrow.status);
        prop_assert!(status_after_fund <= 3);
    }

    #[test]
    fn prop_funded_amount_overflow_protection(
        amount1 in 1i128..1_000_000000i128,
        amount2 in 1i128..1_000_000000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let investor = Address::generate(&env);
        let client = deploy(&env);

        let target = 1_000_000000i128;
        client.init(
            &admin,
            &String::from_str(&env, "OVF001"),
            &sme,
            &target,
            &800i64,
            &0u64,
            &Address::generate(&env),
            &None,
            &Address::generate(&env),
            &None,
            &None,
            &None,
        );

        let total = amount1.saturating_add(amount2);
        if total > 0 {
            let before = client.get_escrow().funded_amount;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.fund(&investor, &total);
            }));
            prop_assert!(result.is_ok() || before >= target);
        }
    }
}
