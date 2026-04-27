use super::*;
use proptest::prelude::*;
use std::vec::Vec;

// Property tests stay isolated so deterministic unit-test grouping remains easy
// to review while fuzzier invariants keep their own namespace.

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
            &String::from_str(&env, "FUZZSNAP"),
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
