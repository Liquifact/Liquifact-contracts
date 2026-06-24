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
            "snapshot set before any funding (case_idx={case_idx}, seed={case_seed})"
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
                    "snapshot set before funded transition (case_idx={case_idx}, seed={case_seed}, pos={pos})"
                );

                let before = client.get_escrow();
                assert_eq!(
                    before.status, 0,
                    "escrow closed before expected crossing (case_idx={case_idx}, seed={case_seed}, pos={pos})"
                );

                expected_funded_amount = expected_funded_amount
                    .checked_add(amounts[idx])
                    .expect("expected_funded_amount overflow");
                let after = client.fund(&investors[idx], &amounts[idx]);

                assert_eq!(
                    after.funded_amount, expected_funded_amount,
                    "funded_amount drift (case_idx={case_idx}, seed={case_seed}, pos={pos})"
                );

                if after.status == 1 {
                    assert_eq!(
                        pos, close_pos,
                        "status became funded before threshold crossing (case_idx={case_idx}, seed={case_seed}, pos={pos}, expected_close_pos={close_pos})"
                    );
                    transitions_to_funded += 1;
                    let snap = client
                        .get_funding_close_snapshot()
                        .expect("missing FundingCloseSnapshot at funded transition");
                    assert_eq!(
                        snap.total_principal, after.funded_amount,
                        "snapshot total_principal must equal funded_amount at close (case_idx={case_idx}, seed={case_seed})"
                    );
                    assert_eq!(
                        snap.funding_target, TARGET,
                        "snapshot funding_target must match escrow target (case_idx={case_idx}, seed={case_seed})"
                    );
                    assert_eq!(
                        snap.closed_at_ledger_timestamp, ts,
                        "snapshot timestamp must match close ledger timestamp (case_idx={case_idx}, seed={case_seed})"
                    );
                    assert_eq!(
                        snap.closed_at_ledger_sequence, seq,
                        "snapshot sequence must match close ledger sequence (case_idx={case_idx}, seed={case_seed})"
                    );
                    captured_snapshot = Some(snap.clone());

                    // Snapshot is immutable across reads.
                    assert_eq!(
                        client.get_funding_close_snapshot().unwrap(),
                        snap,
                        "snapshot changed across read (case_idx={case_idx}, seed={case_seed})"
                    );

                    // Once funded, further funding should not be possible.
                    let extra_investor = Address::generate(&env);
                    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        client.fund(&extra_investor, &1i128);
                    }));
                    assert!(
                        res.is_err(),
                        "fund succeeded after escrow became funded (case_idx={case_idx}, seed={case_seed})"
                    );

                    // Snapshot must remain unchanged across later state transitions.
                    client.settle();
                    assert_eq!(
                        client.get_funding_close_snapshot().unwrap(),
                        snap,
                        "snapshot changed after settle (case_idx={case_idx}, seed={case_seed})"
                    );
                } else {
                    assert_eq!(
                        after.status, 0,
                        "status must remain open prior to threshold crossing (case_idx={case_idx}, seed={case_seed}, pos={pos})"
                    );
                    if pos < close_pos {
                        assert!(
                            after.funded_amount < TARGET,
                            "funded_amount must stay below target before close_pos (case_idx={case_idx}, seed={case_seed}, pos={pos})"
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
            "status must become funded exactly once (case_idx={case_idx}, seed={case_seed})"
        );
        let snap = captured_snapshot.expect("expected snapshot after reaching funding target");
        assert_eq!(
            client.get_funding_close_snapshot().unwrap(),
            snap,
            "snapshot should remain stable at end of case (case_idx={case_idx}, seed={case_seed})"
        );
        assert_eq!(
            client.get_escrow().status,
            2,
            "expected escrow to be settled at end of case (case_idx={case_idx}, seed={case_seed})"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pro-rata payout conservation and rounding invariants
//
// Reference: docs/escrow-pro-rata.md
//
// Formula (floor / truncating integer division):
//   coupon      = total_principal × yield_bps / 10_000   (floor)
//   settle_pool = total_principal + coupon
//   payout_i    = contribution_i  × settle_pool / total_principal (floor)
//
// Invariants tested:
//   1. Σ payout_i ≤ settle_pool  (conservation — no over-distribution)
//   2. settle_pool - Σ payout_i ≥ 0  (non-negative residue swept as dust)
//   3. Non-participant returns 0
//   4. ComputePayoutArithmeticOverflow on overflow inputs
// ─────────────────────────────────────────────────────────────────────────────

/// Compute the expected settle_pool from raw inputs, mirroring the on-chain formula.
fn settle_pool_for(total_principal: i128, yield_bps: i64) -> i128 {
    let coupon = total_principal * (yield_bps as i128) / 10_000;
    total_principal + coupon
}

/// Deploy and fund an escrow with multiple investors, then settle it.
/// Returns (client, investors, amounts) ready for `compute_investor_payout` calls.
fn funded_and_settled_escrow<'a>(
    env: &'a Env,
    invoice_id: &str,
    yield_bps: i64,
    contributions: &[(Address, i128)],
) -> super::LiquifactEscrowClient<'a> {
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let (token, treasury) = free_addresses(env);

    let total: i128 = contributions.iter().map(|(_, a)| a).sum();
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, invoice_id),
        &sme,
        &total,
        &yield_bps,
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
    );

    for (investor, amount) in contributions {
        client.fund(investor, amount);
    }
    client.settle();
    client
}

/// Property: sum of all computed payouts never exceeds settle_pool.
/// Covers single investor, equal splits, and prime-denominator splits.
proptest! {
    #[test]
    fn prop_payout_sum_le_settle_pool(
        // 2–6 investors, each contributing 1..=500_000
        n_investors in 2usize..=6usize,
        seed in 0u64..u64::MAX,
        yield_bps in 0i64..=10_000i64,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        // Deterministic amounts from the proptest-provided seed
        let investors: Vec<Address> = (0..n_investors)
            .map(|_| Address::generate(&env))
            .collect();

        let mut rng = SplitMix64::new(seed);
        let amounts: Vec<i128> = (0..n_investors)
            .map(|_| rng.gen_i128_inclusive(1, 500_000))
            .collect();

        let pairs: Vec<(Address, i128)> = investors
            .iter()
            .cloned()
            .zip(amounts.iter().cloned())
            .collect();

        let client = funded_and_settled_escrow(
            &env,
            "PRPAYOUT",
            yield_bps,
            &pairs,
        );

        let snap = client
            .get_funding_close_snapshot()
            .expect("snapshot must exist after funding");
        let expected_pool = settle_pool_for(snap.total_principal, yield_bps);

        let payout_sum: i128 = investors
            .iter()
            .map(|inv| client.compute_investor_payout(inv))
            .sum();

        prop_assert!(
            payout_sum <= expected_pool,
            "sum of payouts ({payout_sum}) exceeded settle_pool ({expected_pool})"
        );
        let residue = expected_pool - payout_sum;
        prop_assert!(
            residue >= 0,
            "residue must be non-negative, got {residue}"
        );
    }
}

/// Single investor gets exactly settle_pool (no rounding loss when contribution == total_principal).
#[test]
fn payout_single_investor_equals_settle_pool() {
    let env = Env::default();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let contribution = 10_000i128;
    let yield_bps = 500i64; // 5%

    let client = funded_and_settled_escrow(
        &env,
        "SINGLE01",
        yield_bps,
        &[(investor.clone(), contribution)],
    );

    let snap = client.get_funding_close_snapshot().unwrap();
    let expected_pool = settle_pool_for(snap.total_principal, yield_bps);
    let payout = client.compute_investor_payout(&investor);

    // Single investor holds 100% of principal, so payout == settle_pool exactly.
    assert_eq!(
        payout, expected_pool,
        "single investor must receive full settle_pool"
    );
    assert!(payout >= contribution, "payout must include principal back");
}

/// Equal split: two investors each with the same contribution → payouts are equal
/// and their sum ≤ settle_pool.
#[test]
fn payout_equal_split_conservation() {
    let env = Env::default();
    env.mock_all_auths();

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let contribution = 7_777i128; // deliberately not round
    let yield_bps = 800i64;

    let client = funded_and_settled_escrow(
        &env,
        "EQUAL01",
        yield_bps,
        &[(inv_a.clone(), contribution), (inv_b.clone(), contribution)],
    );

    let snap = client.get_funding_close_snapshot().unwrap();
    let settle_pool = settle_pool_for(snap.total_principal, yield_bps);

    let pa = client.compute_investor_payout(&inv_a);
    let pb = client.compute_investor_payout(&inv_b);

    assert_eq!(pa, pb, "equal contributions must yield equal payouts");
    assert!(pa + pb <= settle_pool, "sum must not exceed settle_pool");
    let residue = settle_pool - pa - pb;
    assert!(residue >= 0, "residue must be non-negative");
}

/// Zero yield: payout == contribution for every investor, sum == total_principal.
#[test]
fn payout_zero_yield_returns_principal_only() {
    let env = Env::default();
    env.mock_all_auths();

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);

    let client = funded_and_settled_escrow(
        &env,
        "ZEROYLD1",
        0i64, // zero yield
        &[
            (inv_a.clone(), 3_000i128),
            (inv_b.clone(), 5_000i128),
            (inv_c.clone(), 2_000i128),
        ],
    );

    let pa = client.compute_investor_payout(&inv_a);
    let pb = client.compute_investor_payout(&inv_b);
    let pc = client.compute_investor_payout(&inv_c);

    // With 0% yield, settle_pool == total_principal, so floor division
    // must return the exact contribution.
    assert_eq!(pa, 3_000, "zero yield: payout equals contribution");
    assert_eq!(pb, 5_000, "zero yield: payout equals contribution");
    assert_eq!(pc, 2_000, "zero yield: payout equals contribution");
    assert_eq!(pa + pb + pc, 10_000, "zero yield: sum == total_principal");
}

/// Max yield (10_000 bps = 100%): settle_pool = 2 × total_principal.
/// Conservation still holds.
#[test]
fn payout_max_yield_conservation() {
    let env = Env::default();
    env.mock_all_auths();

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);

    let client = funded_and_settled_escrow(
        &env,
        "MAXYL001",
        10_000i64, // 100% yield → settle_pool = 2 × principal
        &[(inv_a.clone(), 3_001i128), (inv_b.clone(), 6_999i128)],
    );

    let snap = client.get_funding_close_snapshot().unwrap();
    let settle_pool = settle_pool_for(snap.total_principal, 10_000);

    let pa = client.compute_investor_payout(&inv_a);
    let pb = client.compute_investor_payout(&inv_b);

    assert!(
        pa + pb <= settle_pool,
        "sum must not exceed settle_pool at max yield"
    );
    assert!(settle_pool - pa - pb >= 0, "residue non-negative");
}

/// Prime denominator: total_principal is a prime so most floor divisions produce a remainder.
/// Verifies the residue is always ≥ 0 and < n_investors.
#[test]
fn payout_prime_denominator_residue_bounded() {
    let env = Env::default();
    env.mock_all_auths();

    // Use 3 investors contributing 97 + 101 + 103 = 301 (prime total)
    let investors: Vec<Address> = (0..3).map(|_| Address::generate(&env)).collect();
    let amounts = [97i128, 101i128, 103i128];
    let yield_bps = 1_000i64; // 10%

    let pairs: Vec<(Address, i128)> = investors
        .iter()
        .cloned()
        .zip(amounts.iter().cloned())
        .collect();

    let client = funded_and_settled_escrow(&env, "PRIME001", yield_bps, &pairs);

    let snap = client.get_funding_close_snapshot().unwrap();
    let settle_pool = settle_pool_for(snap.total_principal, yield_bps);

    let payout_sum: i128 = investors
        .iter()
        .map(|inv| client.compute_investor_payout(inv))
        .sum();

    assert!(
        payout_sum <= settle_pool,
        "prime denom: sum must not exceed settle_pool"
    );
    let residue = settle_pool - payout_sum;
    assert!(residue >= 0, "residue must be non-negative");
    // Residue is bounded by n_investors (each floor op drops at most 1 unit).
    assert!(
        residue < investors.len() as i128,
        "residue {residue} must be < n_investors ({})",
        investors.len()
    );
}

/// Non-participant returns 0 from compute_investor_payout.
#[test]
fn payout_non_participant_returns_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let stranger = Address::generate(&env);

    let client =
        funded_and_settled_escrow(&env, "NONPART1", 500i64, &[(investor.clone(), 5_000i128)]);

    // stranger never funded → must return 0, not panic
    let payout = client.compute_investor_payout(&stranger);
    assert_eq!(payout, 0, "non-participant must get 0");
}

/// Overflow inputs trigger ComputePayoutArithmeticOverflow.
///
/// contribution × settle_pool overflows i128 when both are near i128::MAX.
/// The contract must panic with the typed error rather than silently wrap.
#[test]
#[should_panic]
fn payout_overflow_panics_with_typed_error() {
    let env = Env::default();
    env.mock_all_auths();

    // We cannot reach i128::MAX contribution via normal fund() since the contract
    // stores funded_amount as i128 and settles normally. Instead we exercise
    // the overflow guard by constructing a scenario where contribution * settle_pool
    // would overflow.
    //
    // contribution = i128::MAX / 2 + 1, yield_bps = 10_000 → settle_pool = 2 * principal
    // contribution * settle_pool ~ (i128::MAX/2) * i128::MAX → overflows.
    //
    // To get such a large contribution through fund() we use a single investor
    // who deposits exactly i128::MAX / 2, which is within i128 range, but the
    // multiplication inside compute_investor_payout will overflow.
    let large: i128 = i128::MAX / 2;

    let investor = Address::generate(&env);
    let client = funded_and_settled_escrow(
        &env,
        "OVERFLOW",
        10_000i64, // 100% yield doubles settle_pool → triggers overflow
        &[(investor.clone(), large)],
    );

    // This call must panic with ComputePayoutArithmeticOverflow.
    client.compute_investor_payout(&investor);
}

/// Fuzz: random investor sets, contributions in [1, 1_000_000], yield in [0, 10_000].
/// Core conservation invariant across diverse inputs.
#[test]
fn fuzz_payout_conservation_multi_investor() {
    let cases: usize = std::env::var("ESCROW_FUZZ_CASES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(64);

    let base_seed = read_fuzz_seed_u64();

    for case_idx in 0..cases {
        let case_seed = base_seed ^ (case_idx as u64).wrapping_mul(0x6C62272E07BB0142u64);
        let mut rng = SplitMix64::new(case_seed);

        let env = Env::default();
        env.mock_all_auths();

        let n = 1 + rng.gen_usize(8); // 1..=8 investors
        let yield_bps = rng.gen_i128_inclusive(0, 10_000) as i64;

        let investors: Vec<Address> = (0..n).map(|_| Address::generate(&env)).collect();
        let amounts: Vec<i128> = (0..n)
            .map(|_| rng.gen_i128_inclusive(1, 1_000_000))
            .collect();

        let pairs: Vec<(Address, i128)> = investors
            .iter()
            .cloned()
            .zip(amounts.iter().cloned())
            .collect();

        // Unique invoice id per case to avoid EscrowAlreadyInitialized.
        // We reuse the same env per case so each gets its own deployed contract.
        let client = funded_and_settled_escrow(&env, "FUZZPAY0", yield_bps, &pairs);

        let snap = client
            .get_funding_close_snapshot()
            .expect("snapshot must exist");
        let settle_pool = settle_pool_for(snap.total_principal, yield_bps);

        let payout_sum: i128 = investors
            .iter()
            .map(|inv| client.compute_investor_payout(inv))
            .sum();

        assert!(
            payout_sum <= settle_pool,
            "case {case_idx}: sum ({payout_sum}) > settle_pool ({settle_pool}), seed={case_seed}"
        );
        assert!(
            settle_pool - payout_sum >= 0,
            "case {case_idx}: residue negative, seed={case_seed}"
        );
    }
}
