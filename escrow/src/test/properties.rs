use super::*;
use proptest::prelude::*;
use std::vec::Vec;

proptest! {
    #[test]
    fn prop_funded_amount_non_decreasing(
        amount1 in 1i128..50_000_000_000i128,
        amount2 in 1i128..50_000_000_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);
        let client = deploy(&env);

        let target = 200_000_000_000i128;
        client.init(
            &admin,
            &soroban_sdk::String::from_str(&env, "INVTST"),
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
        amount in 1i128..100_000_000_000i128,
        target in 1i128..100_000_000_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let investor = Address::generate(&env);
        let client = deploy(&env);

        let escrow = client.init(
            &admin,
            &soroban_sdk::String::from_str(&env, "INVSTA"),
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
}

// Issue #145: Status state machine property tests
// Valid transitions: 0->1 (fund reaches target), 1->2 (settle), 1->3 (withdraw)
// Forbidden: 1->0, 2->0, 3->0, 2->1, 3->1, 2->2, 3->3, 2->3, 3->2

#[test]
fn prop_status_transitions_open_to_funded_only() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let target: i128 = 100_000_000_000i128;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ST0"),
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

    let initial = client.get_escrow();
    assert_eq!(initial.status, 0, "status must start at 0");

    let after = client.fund(&investor, &target);
    assert_eq!(after.status, 1, "funded: status must be 1");
    assert!(
        after.status <= 1,
        "status must not exceed 1 before settle/withdraw"
    );
}

#[test]
fn prop_status_settle_transition() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let target: i128 = 100_000_000_000i128;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ST1"),
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

    client.fund(&investor, &target);

    let before_settle = client.get_escrow();
    assert_eq!(before_settle.status, 1, "status before settle must be 1");

    let after_settle = client.settle();
    assert_eq!(after_settle.status, 2, "settle must transition to 2");
}

#[test]
fn prop_status_withdraw_transition() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let target: i128 = 100_000_000_000i128;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "STW1"),
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

    client.fund(&investor, &target);

    let before_withdraw = client.get_escrow();
    assert_eq!(
        before_withdraw.status, 1,
        "status before withdraw must be 1"
    );
    let after_withdraw = client.withdraw();
    assert_eq!(after_withdraw.status, 3, "withdraw must transition to 3");
}

// Issue #145: Forbidden regression tests

#[test]
fn prop_no_regression_from_funded_status() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let target: i128 = 100_000_000_000i128;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "NREG1"),
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

    client.fund(&investor, &target);

    let funded = client.get_escrow();
    assert_eq!(funded.status, 1, "must be funded");

    let settled = client.settle();
    assert!(settled.status >= 1, "status must not decrease after settle");
    assert_ne!(settled.status, 0, "status must never regress to 0");
    assert_ne!(settled.status, 1, "after settle status must not be 1");
}

#[test]
fn prop_no_regression_after_withdraw() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let target: i128 = 100_000_000_000i128;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "NREG2"),
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

    client.fund(&investor, &target);
    let withdrawn = client.withdraw();

    assert_eq!(withdrawn.status, 3, "withdraw must set status to 3");
    assert!(withdrawn.status >= 1, "status must not decrease below 1");
}

// Issue #145: Terminal state isolation

#[test]
fn prop_settled_is_terminal_for_settle() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let target: i128 = 100_000_000_000i128;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TERM1"),
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

    client.fund(&investor, &target);
    client.settle();

    let settled = client.get_escrow();
    assert_eq!(settled.status, 2, "must be settled");
}

#[test]
fn prop_withdrawn_is_terminal_for_withdraw() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let target: i128 = 100_000_000_000i128;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TERM2"),
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

    client.fund(&investor, &target);
    client.withdraw();

    let withdrawn = client.get_escrow();
    assert_eq!(withdrawn.status, 3, "must be withdrawn");
}

#[test]
fn prop_status_invariant_all_states_valid_range() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let target: i128 = 200_000_000_000i128;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV1"),
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

    assert!(client.get_escrow().status == 0);

    let partial_amount = target / 2;
    client.fund(&investor, &partial_amount);

    let after_partial = client.get_escrow();
    assert!(
        after_partial.status <= 1,
        "partial funding: status must be 0 or 1"
    );
}

// Issue #144: funded_amount monotonicity tests

#[test]
fn prop_funded_amount_sum_of_contributions() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let target: i128 = 300_000_000_000i128;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MONO1"),
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

    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let inv3 = Address::generate(&env);

    let amt1: i128 = 50_000_000_000i128;
    let amt2: i128 = 100_000_000_000i128;
    let amt3: i128 = 50_000_000_000i128;

    let after1 = client.fund(&inv1, &amt1);
    assert_eq!(after1.funded_amount, amt1, "first contribution");

    let after2 = client.fund(&inv2, &amt2);
    assert_eq!(after2.funded_amount, amt1 + amt2, "sum of contributions");

    let after3 = client.fund(&inv3, &amt3);
    assert_eq!(
        after3.funded_amount,
        amt1 + amt2 + amt3,
        "total contributions"
    );
}

#[test]
fn prop_funded_amount_respects_funding_target() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let client = deploy(&env);

    let target: i128 = 100_000_000_000i128;
    let excess: i128 = 50_000_000_000i128;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MONO2"),
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

    let fund_amount = target + excess;
    let after = client.fund(&investor, &fund_amount);
    assert_eq!(
        after.funded_amount, fund_amount,
        "funded_amount records exact amount"
    );
    assert!(after.funded_amount > target, "overfunding recorded");
}

#[test]
fn prop_funded_amount_non_decreasing_across_multiple_funders() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let inv3 = Address::generate(&env);
    let client = deploy(&env);

    let target: i128 = 300_000_000_000i128;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MONO3"),
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

    let amt1: i128 = 50_000_000_000i128;
    let amt2: i128 = 100_000_000_000i128;
    let amt3: i128 = 50_000_000_000i128;

    let before1 = client.get_escrow().funded_amount;
    let after1 = client.fund(&inv1, &amt1);
    assert!(after1.funded_amount >= before1, "first fund non-decreasing");

    let before2 = after1.funded_amount;
    let after2 = client.fund(&inv2, &amt2);
    assert!(
        after2.funded_amount >= before2,
        "second fund non-decreasing"
    );

    let before3 = after2.funded_amount;
    let after3 = client.fund(&inv3, &amt3);
    assert!(after3.funded_amount >= before3, "third fund non-decreasing");

    assert_eq!(
        after3.funded_amount,
        before1 + amt1 + amt2 + amt3,
        "total equals sum"
    );
}

#[test]
fn prop_funded_amount_equals_contribution_sum_for_funded_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let target: i128 = 300_000_000_000i128;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MONO4"),
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

    let amounts: [i128; 3] = [50_000_000_000i128, 100_000_000_000i128, 50_000_000_000i128];
    let mut total_contributed: i128 = 0;

    for amount in amounts {
        let before = client.get_escrow().funded_amount;
        let after = client.fund(&Address::generate(&env), &amount);

        total_contributed += amount;

        assert_eq!(
            after.funded_amount, total_contributed,
            "funded_amount equals running sum"
        );
        assert!(
            after.funded_amount >= before,
            "funded_amount never decreases"
        );
    }

    let final_funded = client.get_escrow().funded_amount;
    assert_eq!(
        final_funded, total_contributed,
        "final funded_amount equals total contributions"
    );
}

#[derive(Clone, Copy)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    fn gen_usize(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        (self.next_u64() % (upper as u64)) as usize
    }

    fn gen_i128_inclusive(&mut self, lo: i128, hi: i128) -> i128 {
        assert!(lo <= hi, "invalid range");
        let span: u128 = (hi - lo) as u128 + 1;
        let draw: u128 = (self.next_u64() as u128) % span;
        lo + (draw as i128)
    }
}

fn shuffle_in_place<T>(rng: &mut SplitMix64, items: &mut [T]) {
    // Fisher-Yates in-place shuffle.
    for i in (1..items.len()).rev() {
        let j = rng.gen_usize(i + 1);
        items.swap(i, j);
    }
}

fn read_fuzz_seed_u64() -> u64 {
    // Repro: set `ESCROW_FUZZ_SEED` (decimal or hex like `0xdeadbeef`) and re-run this test.
    const DEFAULT: u64 = 0xE5D7_F00D_1760_0001;
    let Ok(raw) = std::env::var("ESCROW_FUZZ_SEED") else {
        return DEFAULT;
    };
    let raw = raw.trim();
    if let Some(hex) = raw.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).unwrap_or(DEFAULT)
    } else {
        raw.parse::<u64>().unwrap_or(DEFAULT)
    }
}

#[test]
fn fuzz_multi_investor_fund_ordering_snapshot_once_only() {
    // Keep runtime predictable in CI; allow local override when investigating.
    let cases: usize = std::env::var("ESCROW_FUZZ_CASES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(64);
    let base_seed = read_fuzz_seed_u64();

    for case_idx in 0..cases {
        let case_seed = base_seed ^ (case_idx as u64).wrapping_mul(0x9E3779B97F4A7C15u64);
        let mut rng = SplitMix64::new(case_seed);

        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let client = deploy(&env);

        let (token, treasury) = free_addresses(&env);
        client.init(
            &admin,
            &soroban_sdk::String::from_str(&env, "FUZZSNAP"),
            &sme,
            &TARGET,
            &800i64,
            &0u64,
            &token,
            &None,
            &treasury,
            &None,
            &None,
            &None,
        );

        // Randomize investor count/order and positive amounts. Keep the sequence small so
        // runtime stays within budget and shrinking isn't required to debug failures.
        let investor_count: usize = 2 + rng.gen_usize(10); // 2..=11
        let investors: Vec<Address> = (0..investor_count)
            .map(|_| Address::generate(&env))
            .collect();

        let max_each = (TARGET / 2).max(1);
        let mut amounts: Vec<i128> = (0..investor_count)
            .map(|_| rng.gen_i128_inclusive(1, max_each))
            .collect();

        // Guarantee we cross the target at least once (and often overfund a bit).
        let sum: i128 = amounts.iter().sum();
        if sum < TARGET {
            let top_up_idx = rng.gen_usize(investor_count);
            let needed = TARGET - sum;
            let extra = rng.gen_i128_inclusive(0, (TARGET / 4).max(1));
            amounts[top_up_idx] = amounts[top_up_idx]
                .checked_add(needed + extra)
                .expect("amount top-up overflow");
        }

        let mut order: Vec<usize> = (0..investor_count).collect();
        shuffle_in_place(&mut rng, &mut order);

        // Find the first call that crosses the funding target so we can assert that:
        // - status flips to funded exactly once
        // - FundingCloseSnapshot is written exactly once and never changes thereafter
        let mut cumulative = 0i128;
        let mut close_pos = None;
        for (pos, &idx) in order.iter().enumerate() {
            cumulative = cumulative
                .checked_add(amounts[idx])
                .expect("cumulative overflow");
            if cumulative >= TARGET {
                close_pos = Some(pos);
                break;
            }
        }
        let close_pos = close_pos.expect("expected funding to reach target");

        assert_eq!(
            client.get_funding_close_snapshot(),
            None,
            "snapshot set before any funding (case_idx={}, seed={})",
            case_idx,
            case_seed
        );

        let mut transitions_to_funded = 0u32;
        let mut expected_funded_amount = 0i128;
        let mut captured_snapshot = None;

        for (pos, &idx) in order.iter().enumerate() {
            let ts = 1_700_000_000u64 + (case_idx as u64) * 100 + (pos as u64);
            let seq = 10_000u32 + (case_idx as u32) * 100 + (pos as u32);
            env.ledger().set_timestamp(ts);
            env.ledger().set_sequence_number(seq);

            if captured_snapshot.is_none() {
                // Snapshot must not exist before the funded transition.
                assert_eq!(
                    client.get_funding_close_snapshot(),
                    None,
                    "snapshot set before funded transition (case_idx={}, seed={}, pos={})",
                    case_idx,
                    case_seed,
                    pos
                );

                let before = client.get_escrow();
                assert_eq!(
                    before.status, 0,
                    "escrow closed before expected crossing (case_idx={}, seed={}, pos={})",
                    case_idx, case_seed, pos
                );

                expected_funded_amount = expected_funded_amount
                    .checked_add(amounts[idx])
                    .expect("expected_funded_amount overflow");
                let after = client.fund(&investors[idx], &amounts[idx]);

                assert_eq!(
                    after.funded_amount, expected_funded_amount,
                    "funded_amount drift (case_idx={}, seed={}, pos={})",
                    case_idx, case_seed, pos
                );

                if after.status == 1 {
                    assert_eq!(
                        pos, close_pos,
                        "status became funded before threshold crossing (case_idx={}, seed={}, pos={}, expected_close_pos={})",
                        case_idx, case_seed, pos, close_pos
                    );
                    transitions_to_funded += 1;
                    let snap = client
                        .get_funding_close_snapshot()
                        .expect("missing FundingCloseSnapshot at funded transition");
                    assert_eq!(
                        snap.total_principal, after.funded_amount,
                        "snapshot total_principal must equal funded_amount at close (case_idx={}, seed={})",
                        case_idx, case_seed
                    );
                    assert_eq!(
                        snap.funding_target, TARGET,
                        "snapshot funding_target must match escrow target (case_idx={}, seed={})",
                        case_idx, case_seed
                    );
                    assert_eq!(
                        snap.closed_at_ledger_timestamp, ts,
                        "snapshot timestamp must match close ledger timestamp (case_idx={}, seed={})",
                        case_idx, case_seed
                    );
                    assert_eq!(
                        snap.closed_at_ledger_sequence, seq,
                        "snapshot sequence must match close ledger sequence (case_idx={}, seed={})",
                        case_idx, case_seed
                    );
                    captured_snapshot = Some(snap.clone());

                    // Snapshot is immutable across reads.
                    assert_eq!(
                        client.get_funding_close_snapshot().unwrap(),
                        snap,
                        "snapshot changed across read (case_idx={}, seed={})",
                        case_idx,
                        case_seed
                    );

                    // Once funded, further funding should not be possible.
                    let extra_investor = Address::generate(&env);
                    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        client.fund(&extra_investor, &1i128);
                    }));
                    assert!(
                        res.is_err(),
                        "fund succeeded after escrow became funded (case_idx={}, seed={})",
                        case_idx,
                        case_seed
                    );

                    // Snapshot must remain unchanged across later state transitions.
                    client.settle();
                    assert_eq!(
                        client.get_funding_close_snapshot().unwrap(),
                        snap,
                        "snapshot changed after settle (case_idx={}, seed={})",
                        case_idx,
                        case_seed
                    );
                } else {
                    assert_eq!(
                        after.status, 0,
                        "status must remain open prior to threshold crossing (case_idx={}, seed={}, pos={})",
                        case_idx, case_seed, pos
                    );
                    if pos < close_pos {
                        assert!(
                            after.funded_amount < TARGET,
                            "funded_amount must stay below target before close_pos (case_idx={}, seed={}, pos={})",
                            case_idx,
                            case_seed,
                            pos
                        );
                    }
                }
            }

            if captured_snapshot.is_some() {
                break;
            }
        }

        assert_eq!(
            transitions_to_funded, 1,
            "status must become funded exactly once (case_idx={}, seed={})",
            case_idx, case_seed
        );
        let snap = captured_snapshot.expect("expected snapshot after reaching funding target");
        assert_eq!(
            client.get_funding_close_snapshot().unwrap(),
            snap,
            "snapshot should remain stable at end of case (case_idx={}, seed={})",
            case_idx,
            case_seed
        );
        assert_eq!(
            client.get_escrow().status,
            2,
            "expected escrow to be settled at end of case (case_idx={}, seed={})",
            case_idx,
            case_seed
        );
    }
}
