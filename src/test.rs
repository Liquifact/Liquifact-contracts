#ed(test)
mod tests {
    use crate::*;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        Address, BytesN, Env,
    };
    use std::thread;

    fn setup_test(env: &Env) -> (YieldTierContractClient<'_>, Address) {
        let contract_id = env.register(YieldTierContract, ());
        let client = YieldTierContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        (client, admin)
    }

    fn setup_initialized_test(env: &Env) -> (YieldTierContractClient<'_>, Address) {
        let (client, admin) = setup_test(env);
        client.init(&admin);
        (client, admin)
    }

    // ── 1. Basic Functionality & Default States ──────────────────────────────

    fn assert_contract_error<T: std::fmt::Debug>(
        result: Result<T, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
        expected: Error,
    ) {
        let expected_code = expected as u32;
        match result {
            Err(Ok(error)) => assert_eq!(
                error,
                soroban_sdk::Error::from_contract_error(expected_code)
            ),
            Err(Err(soroban_sdk::InvokeError::Contract(code))) => {
                assert_eq!(code, expected_code)
            }
            other => panic!("expected contract error {expected_code}, got {other:?}"),
        }
    }

    #[test]
    fn test_init_rejects_duplicate_without_changing_admin() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let replacement_admin = Address::generate(&env);
        client.init(&admin);
        let events_before = env.events().all();

        assert_contract_error(
            client.try_init(&replacement_admin),
            Error::AlreadyInitialized,
        );
        assert_eq!(
            env.storage().instance().get::<_, Address>(&ADMIN_KEY),
            Some(admin)
        );
        assert_eq!(env.events().all(), events_before);
    }

    #[test]
    fn test_admin_operations_reject_uninitialized_contract() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);
        let new_wasm = BytesN::from_array(&env, &[1; 32]);

        assert_eq!(
            client.try_upgrade(&new_wasm),
            Ok(Err(Error::NotInitialized))
        );
        assert_eq!(
            client.try_set_yield_tier(&YieldTierState::Tier1),
            Ok(Err(Error::NotInitialized))
        );
        assert_eq!(client.get_yield_tier(), YieldTierState::Unset);
        assert!(env.events().all().is_empty());
    }

    #[test]
    fn test_all_yield_tier_values_and_repeated_submissions_are_deterministic() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.init(&admin);

        for tier in [
            YieldTierState::Unset,
            YieldTierState::Tier1,
            YieldTierState::Tier2,
            YieldTierState::Tier3,
        ] {
            client.set_yield_tier(&tier);
            client.set_yield_tier(&tier);
            assert_eq!(client.get_yield_tier(), tier);
        }

        assert_eq!(env.events().all().len(), 8);
    }

    #[test]
    fn test_get_yield_tier_returns_default_when_unset() {
        let env = Env::default();
        let (client, _admin) = setup_test(&env);
        let state = client.get_yield_tier();
        assert_eq!(state, YieldTierState::Unset);
        assert_eq!(client.get_version(), 0);
    }

    #[test]
    fn test_get_yield_tier_returns_stored_state() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);
        assert_eq!(client.get_version(), 1);

        client.set_yield_tier(&YieldTierState::Tier3);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier3);
        assert_eq!(client.get_version(), 2);
    }

    // ── 2. Authorization & Initializer Guards ────────────────────────────────

    #[test]
    fn test_upgrade_admin_allowed() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        let new_wasm = BytesN::from_array(&env, &[1; 32]);
        let res = client.upgrade(&new_wasm);
        assert_eq!(res, ());

        let binding = env.events().all();
        assert_eq!(binding.events().len(), 1);

        assert_eq!(client.get_version(), 1);
    }

    #[test]
    fn test_upgrade_non_admin_rejected() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);

        let new_wasm = BytesN::from_array(&env, &[1; 32]);
        let result = client.try_upgrade(&new_wasm);
        assert!(result.is_err());
        assert_eq!(client.get_version(), 0);
    }

    #[test]
    fn test_upgrade_before_init_rejected() {
        let env = Env::default();
        let (client, _admin) = setup_test(&env);
        env.mock_all_auths();

        let new_wasm = BytesN::from_array(&env, &[1; 32]);
        let result = client.try_upgrade(&new_wasm);
        assert_eq!(result, Err(Ok(Error::NotInitialized)));
    }

    #[test]
    fn test_upgrade_before_init_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let new_wasm = BytesN::from_array(&env, &[1; 32]);
        assert!(client.try_upgrade(&new_wasm).is_err());
    }

    #[test]
    fn test_upgrade_repeated_calls_are_deterministic() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let new_wasm = BytesN::from_array(&env, &[2; 32]);
        for _ in 0.3 {
            client.upgrade(&new_wasm);
        }
        assert_eq(
            env.events().all().last().unwrap(),
            (
                contract_id,
                (symbol_short!("upgrade"),).into_val(&env),
                (new_wasm.clone(),).into_val(&env),
            )
        );
    }

    /// --------------------------------------------------------------------------
    /// set_yield_tier authorization and state transitions
    /// --------------------------------------------------------------------------

    #[test]
    fn test_set_yield_tier_admin_authorized() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        client.set_yield_tier(&YieldTierState::Tier1);
        let binding = env.events().all();
        assert_eq!(binding.events().len(), 1);

        assert_eq!(client.get_yield_tier(), YieldTierState::Tier1);
        assert_eq!(client.get_version(), 1);
    }

    #[test]
    fn test_set_yield_tier_non_admin_rejected() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);

        let result = client.try_set_yield_tier(&YieldTierState::Tier1);
        assert!(result.is_err());
        assert_eq!(client.get_yield_tier(), YieldTierState::Unset);
        assert_eq!(client.get_version(), 0);
    }

    #[test]
    fn test_set_yield_tier_before_init_rejected() {
        let env = Env::default();
        let (client, _admin) = setup_test(&env);
        env.mock_all_auths();

        let result = client.try_set_yield_tier(&YieldTierState::Tier1);
        assert_eq!(result, Err(Ok(Error::NotInitialized)));
        assert_eq!(client.get_yield_tier(), YieldTierState::Unset);
    }

    #[test]
    fn test_get_admin_before_and_after_init() {
        let env = Env::default();
        let (client, admin) = setup_test(&env);
        assert_eq!(client.try_get_admin(), Err(Ok(Error::NotInitialized)));

        client.init(&admin);
        assert_eq!(client.get_admin(), admin);
    }

    // ── 3. Racing Initializations & State Preservation ───────────────────────

    #[test]
    fn test_racing_init_rejects_duplicate_and_preserves_admin() {
        let env = Env::default();
        let (client, admin) = setup_test(&env);
        let racing_attacker = Address::generate(&env);

        // First initialization succeeds
        client.init(&admin);
        assert_eq!(client.get_admin(), admin);

        // Concurrent/racing re-initialization fails deterministically
        let res = client.try_init(&racing_attacker);
        assert!(res.is_err());

        // Invariant check: original admin is preserved intact
        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_version(), 0);
    }

    // ── 4. Concurrency Hardening: Monotonic Sequencing & Retries ─────────────

    #[test]
    fn test_duplicate_writes_are_deterministic_and_idempotent() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        // Repeated writes with the exact same tier state
        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);
        assert_eq!(client.get_version(), 1);

        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);
        assert_eq!(client.get_version(), 2);
    }

    #[test]
    fn test_concurrent_writes_are_serialized_with_monotonic_version() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        let sequence = [
            YieldTierState::Tier1,
            YieldTierState::Tier2,
            YieldTierState::Tier3,
            YieldTierState::Tier1,
            YieldTierState::Unset,
        ];

        for (idx, tier) in sequence.iter().enumerate() {
            client.set_yield_tier(tier);
            let binding = env.events().all();
            assert_eq!(binding.events().len(), 1);

            assert_eq!(client.get_yield_tier(), *tier);
            assert_eq!(client.get_version(), (idx + 1) as u32);
        }
    }

    #[test]
    fn test_optimistic_concurrency_control_success_and_stale_rejection() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        assert_eq!(client.get_version(), 0);

        // Writer 1 reads version 0 and writes Tier1 successfully
        let res1 = client.set_tier_with_version(&YieldTierState::Tier1, &0);
        assert_eq!(res1, ());
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier1);
        assert_eq!(client.get_version(), 1);

        // Racing Writer 2 also based on stale version 0 attempts to write Tier2
        let res2 = client.try_set_tier_with_version(&YieldTierState::Tier2, &0);
        assert_eq!(res2, Err(Ok(Error::StaleVersion)));

        // Invariant: Tier1 remains intact, version has not incremented on failure
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier1);
        assert_eq!(client.get_version(), 1);

        // Writer 2 refreshes to current version (1) and retries successfully
        let res3 = client.set_tier_with_version(&YieldTierState::Tier2, &1);
        assert_eq!(res3, ());
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);
        assert_eq!(client.get_version(), 2);
    }

    // ── 5. Failure Recovery & Error Isolation ────────────────────────────────

    #[test]
    fn test_failed_unauthorized_call_does_not_mutate_state_or_leak_events() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);

        // Failed unauthorized mutation
        let res = client.try_set_yield_tier(&YieldTierState::Tier3);
        assert!(res.is_err());

        // No spurious events leaked from failed invocation
        let binding_before = env.events().all();
        assert_eq!(binding_before.events().len(), 0);

        // State and version remain unchanged
        assert_eq!(client.get_yield_tier(), YieldTierState::Unset);
        assert_eq!(client.get_version(), 0);

        // Subsequent authorized retry succeeds cleanly
        env.mock_all_auths();
        client.set_yield_tier(&YieldTierState::Tier3);

        let binding_after = env.events().all();
        assert_eq!(binding_after.events().len(), 1);

        assert_eq!(client.get_yield_tier(), YieldTierState::Tier3);
        assert_eq!(client.get_version(), 1);
    }

    #[test]
    fn test_failed_upgrade_does_not_corrupt_yield_tier_state() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(client.get_version(), 1);

        // Clear mock auths to test unauthenticated upgrade failure
        env.set_auths(&[]);
        let dummy_wasm = BytesN::from_array(&env, &[9; 32]);
        let res = client.try_upgrade(&dummy_wasm);
        assert!(res.is_err());

        // Prior state intact
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);
        assert_eq!(client.get_version(), 1);
    }

    // ── 6. Timing & Ledger Sequence Boundaries ───────────────────────────────

    #[test]
    fn test_concurrency_across_ledger_sequence_and_timestamp_advance() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        // Write on initial ledger
        client.set_yield_tier(&YieldTierState::Tier1);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier1);
        assert_eq!(client.get_version(), 1);

        // Advance ledger sequence by 10,000 blocks and timestamp by 7 days
        let mut ledger_info = env.ledger().get();
        ledger_info.sequence_number += 10_000;
        ledger_info.timestamp += 7 * 86_400;
        env.ledger().set(ledger_info);

        // State remains completely intact across time boundary
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier1);
        assert_eq!(client.get_version(), 1);

        // Subsequent operation succeeds deterministically
        client.set_yield_tier(&YieldTierState::Tier3);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier3);
        assert_eq!(client.get_version(), 2);
    }

    // ── 7. Multi-threaded Parallel Environment Isolation ─────────────────────

    #[test]
    fn test_multithreaded_concurrent_independent_deployments() {
        let num_threads = 8;
        let mut handles = std::vec::Vec::new();

        for thread_idx in 0..num_threads {
            let handle = thread::spawn(move || {
                let env = Env::default();
                env.mock_all_auths();
                let (client, admin) = setup_test(&env);

                client.init(&admin);
                assert_eq!(client.get_admin(), admin);

                let target_tier = match thread_idx % 3 {
                    0 => YieldTierState::Tier1,
                    1 => YieldTierState::Tier2,
                    _ => YieldTierState::Tier3,
                };

                client.set_yield_tier(&target_tier);
                assert_eq!(client.get_yield_tier(), target_tier);
                assert_eq!(client.get_version(), 1);

                // Duplicate idempotent write
                client.set_yield_tier(&target_tier);
                assert_eq!(client.get_yield_tier(), target_tier);
                assert_eq!(client.get_version(), 2);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().expect("Worker thread panicked");
        }
    }

    // --------------------------------------------------------------------
    // Deterministic failure recovery tests
    //
    // Invariants under test:
    //   1. A failed write must not mutate persisted state.
    //   2. A failed write must not emit a success event.
    //   3. Retrying after a failure must be deterministic and idempotent
    //      with respect to the final state and the emitted event sequence.
    //   4. Concurrent/repeated calls must not produce an inconsistent result.
    // --------------------------------------------------------------------

    // Failure before init: any admin-guarded write must be rejected and must
    // leave the contract in the Unset state with no events emitted.
    #[test]
    fn test_failure_before_init_leaves_state_unset() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        // No init has run, so the admin guard must reject the write.
        let result = client.try_set_yield_tier(&YieldTierState::Tier1);
        assert!(result.is_error());

        // State must remain Unset and no events must have been emitted.
        assert_eq(client.get_yield_tier(), YieldTierState::Unset);
        assert_eq(env.events().all().len(), 0);
    }

    // Failure before init on upgrade: same invariants as above.
    #[test]
    fn test_failure_upgrade_before_init_leaves_state_unset() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let new_wasm = BytesN::from_array(&env, &[2; 32]);
        let result = client.try_upgrade(&new_wasm);
        assert!(result.is_error());

        assert_eq(client.get_yield_tier(), YieldTierState::Unset);
        assert_eq(env.events().all().len(), 0);
    }

    // Retry after a failed write: the second attempt must succeed deterministically
    // and produce exactly one tier_set event with the final value.
    #[test]
    fn test_retry_after_failure_is_deterministic() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // First attempt without auth mocking -> must fail and not mutate state.
        let failed = client.try_set_yield_tier(&YieldTierState::Tier2);
        assert!(failed.is_error());
        assert_eq(client.get_yield_tier(), YieldTierState::Unset);
        assert_eq(env.events().all().len(), 0);

        // Retry with auth mocked -> must succeed and emit exactly one event.
        env.mock_all_auths();
        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq(client.get_yield_tier(), YieldTierState::Tier2);

        let events = env.events().all();
        assert_eq(events.len(), 1);
        assert_eq(
            events.last().unwrap(),
            (
                contract_id,
                (symbol_short!("tier_set"),).into_val(&env),
                (YieldTierState::Tier2,).into_val(&env),
            )
        );
    }

    // Retry after a failed upgrade: the second attempt must succeed and the
    // event must reflect the actually applied wasm hash.
    #[test]
    fn test_retry_upgrade_after_failure_is_deterministic() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let new_wasm = BytesN::from_array(&env, &[3; 32]);

        // First attempt without auth mocking -> must fail and emit nothing.
        let failed = client.try_upgrade(&new_wasm);
        assert!(failed.is_error());
        assert_eq(env.events().all().len(), 0);

        // Retry with auth mocked -> must succeed and emit exactly one event.
        env.mock_all_auths();
        client.upgrade(&new_wasm);

        let events = env.events().all();
        assert_eq(events.len(), 1);
        assert_eq(
            events.last().unwrap(),
            (
                contract_id,
                (symbol_short!("upgrade"),).into_val(&env),
                (new_wasm.clone(),).into_val(&env),
            )
        );
    }

    // Partial completion guard: a failed write must not leave behind any
    // intermediate state. After a failure the contract must behave exactly like
    // a freshly initialized contract with Unset tier.
    #[test]
    fn test_partial_completion_does_not_corrupt_state() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // Failed write without auth.
        let failed = client.try_set_yield_tier(&YieldTierState::Tier3);
        assert!(failed.is_error());

        // State must be identical to a fresh contract: Unset, no events.
        assert_eq(client.get_yield_tier(), YieldTierState::Unset);
        assert_eq(env.events().all().len(), 0);

        // A later successful write must produce the expected single event.
        env.mock_all_auths();
        client.set_yield_tier(&YieldTierState::Tier3);
        assert_eq(client.get_yield_tier(), YieldTierState::Tier3);
        assert_eq(env.events().all().lang(), 1);
    }

    // Concurrent/repeated execution: multiple successful writes must be
    // deterministic and events must be emitted in causal order with the
    // correct final value.
    #[test]
    fn test_repeated_writes_are_deterministic() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        client.set_yield_tier(&YieldTierState::Tier1);
        client.set_yield_tier(&YieldTierState::Tier2);
        client.set_yield_tier(&YieldTierState::Tier3);

        // Final state must reflect the last write.
        assert_eq(client.get_yield_tier(), YieldTierState::Tier3);

        // Exactly three events, in order, with the correct values.
        let events = env.events().all();
        assert_eq(events.len(.), 3);
        assert_eq(
            events.get(0).unwrap(),
            (
                contract_id.clone(),
                (symbol_short!("tier_set"),).into_val(&env),
                (YieldTierState::Tier1,).into_val(&env),
            )
        );
        assert_eq(
            events.get(1).unwrap(),
            (
                contract_id.clone(),
                (symbol_short!("tier_set"),).into_val(&env),
                (YieldTierState::Tier2,).into_val(&env),
            )
        );
        assert_eq(
            events.get(2).unwrap(),
            (
                contract_id.clone(),
                (symbol_short!("tier_set"),).into_val(&env),
                (YieldTierState::Tier3,).into_val(&env),
            )
        );
    }

    // Boundary case: writing the same tier twice is allowed and must be
    // deterministic -- the final state and the number of events are well
    // defined.
    #[test]
    fn test_set_yield_tier_non_admin_rejected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // non-admin will fail auth without mock_all_auths
        let result = client.try_set_yield_tier(&YieldTierState::Tier1);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_yield_tier_rejects_unset_state() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let result = client.try_set_yield_tier(&YieldTierState::Unset);
        assert!(matches!(result, Ok(Err(Error::InvalidYieldTier))));
    }

    #[test]
    fn test_set_yield_tier_repeated_value_is_idempotent() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        client.set_yield_tier(&YieldTierState::Tier2);
        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);
    }

    #[test]
    fn test_set_yield_tier_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        client.set_yield_tier(&YieldTierState::Tier2);
        client.set_yield_tier(&YieldTierState::Tier2);

        assert_eq(client.get_yield_tier(), YieldTierState::Tier2);
        assert_eq(env.events().all().len(), 2);
    }

    // Boundary case: a failed write must not affect the admin authorization
    // for later calls. After a failure, an authorized admin can still write.
    #[test]
    fn test_failure_preserves_admin_authorization() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // Failed write without auth.
        let failed = client.try_set_yield_tier(&YieldTierState::Tier1);
        assert!(failed.is_error());

        // Authorized admin can still write after the failure.
        env.mock_all_auths();
        client.set_yield_tier(&YieldTierState::Tier1);
        assert_eq(client.get_yield_tier(), YieldTierState::Tier1);
    }

    // Regression: a failed write must not consume or alter the admin slot.
    // The contract must still accept the original admin's authorized writes.
    #[test]
    fn test_failure_does_not_consume_admin_slot() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // Multiple failed writes in a row.
        for _ in 0..3 {
            let failed = client.try_set_yield_tier(&YieldTierState::Tier3);
            assert!(failed.is_error());
        }

        // State unchanged and no events emitted.
        assert_eq(client.get_yield_tier(), YieldTierState::Unset);
        assert_eq(env.events().all().len(), 0);

        // Admin still authorized.
        env.mock_all_auths();
        client.set_yield_tier(&YieldTierState::Tier3);
        assert_eq(client.get_yield_tier(), YieldTierState::Tier3);
    }

    // Regission: a failed upgrade must not corrupt the yield tier state.
    #[test]
    fn test_failed_upgrade_does_not_corrupt_tier_state() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);
        client.set_yield_tier(&YieldTierState::Tier2);

        // Failed upgrade without auth.
        let new_wasm = BytesN::from_array(&env, &[9; 32]);
        let failed = client.try_upgrade(&new_wasm);
        assert!(failed.is_error());

        // Tier state must be unchanged.
        asser_eq(client.get_yield_tier(), YieldTierState::Tier2);
    }

    // Regression: events from a failed attempt must not leak into the event
    // stream of a subsequent successful attempt.
    #[test]
    fn test_failure_events_do_not_leak() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // Failed write.
        let failed = client.try_set_yield_tier(&YieldTierState::Tier1);
        assert!(failed.is_error());
        assert_eq(env.events().all().len(), 0);

        // Successful write after the failure.
        env.mock_all_auths();
        client.set_yield_tier(&YieldTierState::Tier1);

        // Exactly one event, and it must be the successful write.
        let events = env.events().all();
        assert_eq(events.len(), 1);
        assert_eq(
            events.last().unwrap(),
            (
                contract_id,
                (symbol_short!("tier_set"),).into_val(&env),
                (YieldTierState::Tier1,).into_val(&env),
            )
        );
    }

    // Boundary: the contract must remain recoverable across multiple ledger
    // advances. A failure on one ledger must not affect the ability to write
    // on a later ledger.
    #[test]
    fn test_recovery_across_ledger_advance() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // Failed write on the current ledger.
        let failed = client.try_set_yield_tier(&YieldTierState::Tier1);
        assert!(failed.is_error());

        // Advance the ledger and retry with auth.
        env.ledger().set_sequence(100);
        env.mock_all_auths();
        client.set_yield_tier(&YieldTierState::Tier1);

        assert_eq(client.get_yield_tier(), YieldTierState::Tier1);
        assert_eq(env.events().all().len(), 1);
    }
}
