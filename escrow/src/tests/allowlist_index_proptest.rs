//! Property-based tests for the dense allowlist index (`AllowlistIndex`,
//! `AllowlistSlot`, `AllowlistSlotOf`, `AllowlistCount`).
//!
//! Verifies that arbitrary sequences of single and batch allowlist mutations
//! (`set_investor_allowlisted`, `set_investors_allowlisted`), including
//! starting from a seeded legacy `AllowlistIndex` layout, keep the contract's
//! count, paginated views (`get_allowlisted_investors`), membership queries,
//! and slot mappings consistent with a reference model set.

use super::*;
use proptest::prelude::*;
use std::collections::BTreeSet;

const POOL_SIZE: usize = 12;

#[derive(Clone, Debug)]
enum AllowlistOp {
    Single { idx: usize, allowed: bool },
    Batch { indices: Vec<usize>, allowed: bool },
}

fn op_strategy() -> impl Strategy<Value = AllowlistOp> {
    prop_oneof![
        (0..POOL_SIZE, any::<bool>()).prop_map(|(idx, allowed)| AllowlistOp::Single {
            idx,
            allowed,
        }),
        (prop::collection::vec(0..POOL_SIZE, 1..=8), any::<bool>())
            .prop_map(|(indices, allowed)| AllowlistOp::Batch { indices, allowed }),
    ]
}

fn init_escrow(env: &Env, client: &LiquifactEscrowClient<'_>) {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "ALPROP01"),
        &sme,
        &100_000i128,
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
        &None,
        &None,
        &None::<i64>,
    );
}

fn collect_all_pages(
    client: &LiquifactEscrowClient<'_>,
    page_limit: u32,
) -> Result<Vec<Address>, TestCaseError> {
    let mut collected = Vec::new();
    let mut start = 0u32;
    let total = client.get_allowlisted_investors_count();

    loop {
        let page = client.get_allowlisted_investors(&start, &page_limit);
        if page.is_empty() {
            break;
        }
        prop_assert!(
            page.len() <= page_limit,
            "page len {} exceeded limit {}",
            page.len(),
            page_limit
        );
        for i in 0..page.len() {
            collected.push(page.get(i).unwrap());
        }
        start = start.saturating_add(page.len());
        // Guard against infinite loop if pagination ever stalled.
        prop_assert!(
            start <= total.saturating_add(page_limit),
            "pagination exceeded expected bound"
        );
    }

    // Out-of-range start must return empty.
    let past_end = client.get_allowlisted_investors(&total, &page_limit);
    prop_assert!(past_end.is_empty(), "past-end page must be empty");

    Ok(collected)
}

fn assert_matches_model(
    env: &Env,
    client: &LiquifactEscrowClient<'_>,
    pool: &[Address],
    model: &BTreeSet<usize>,
) -> Result<(), TestCaseError> {
    let expected_count = model.len() as u32;
    let actual_count = client.get_allowlisted_investors_count();
    prop_assert_eq!(
        actual_count,
        expected_count,
        "get_allowlisted_investors_count mismatch"
    );

    // Zero-limit page must always be empty.
    prop_assert!(client.get_allowlisted_investors(&0, &0).is_empty());

    // Check pagination across multiple page sizes (small, medium, full).
    for &limit in &[1u32, 3, 5, 32] {
        let paged = collect_all_pages(client, limit)?;
        prop_assert_eq!(
            paged.len(),
            model.len(),
            "paginated total length mismatch for limit={}",
            limit
        );

        let mut seen_indices = BTreeSet::new();
        for addr in &paged {
            let idx = pool
                .iter()
                .position(|candidate| candidate == addr)
                .expect("paginated address must belong to pool");
            prop_assert!(
                seen_indices.insert(idx),
                "duplicate address at pool index {} for limit={}",
                idx,
                limit
            );
        }
        prop_assert_eq!(
            &seen_indices,
            model,
            "paginated set mismatch for limit={}",
            limit
        );
    }

    // Check per-address membership flag for the entire pool.
    for (idx, addr) in pool.iter().enumerate() {
        prop_assert_eq!(
            client.is_investor_allowlisted(addr),
            model.contains(&idx),
            "is_investor_allowlisted mismatch for pool index {}",
            idx
        );
    }

    // Check dense slot storage invariants: slots 0..count round-trip with AllowlistSlotOf,
    // and slot `count` is absent.
    env.as_contract(&client.address, || {
        for slot in 0..actual_count {
            let addr: Option<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::AllowlistSlot(slot));
            assert!(addr.is_some(), "missing AllowlistSlot({})", slot);
            let addr = addr.unwrap();
            let slot_of: Option<u32> = env
                .storage()
                .persistent()
                .get(&DataKey::AllowlistSlotOf(addr));
            assert_eq!(
                slot_of,
                Some(slot),
                "AllowlistSlotOf round-trip mismatch for slot {}",
                slot
            );
        }
        assert!(
            !env.storage()
                .persistent()
                .has(&DataKey::AllowlistSlot(actual_count)),
            "stale AllowlistSlot({}) present beyond count",
            actual_count
        );
    });

    Ok(())
}

fn apply_op(
    env: &Env,
    client: &LiquifactEscrowClient<'_>,
    pool: &[Address],
    model: &mut BTreeSet<usize>,
    op: &AllowlistOp,
) {
    match op {
        AllowlistOp::Single { idx, allowed } => {
            client.set_investor_allowlisted(&pool[*idx], allowed);
            if *allowed {
                model.insert(*idx);
            } else {
                model.remove(idx);
            }
        }
        AllowlistOp::Batch { indices, allowed } => {
            let mut batch = SorobanVec::new(env);
            for &idx in indices {
                batch.push_back(pool[idx].clone());
                if *allowed {
                    model.insert(idx);
                } else {
                    model.remove(&idx);
                }
            }
            client.set_investors_allowlisted(&batch, allowed);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn prop_allowlist_index_matches_model_after_every_op(
        ops in prop::collection::vec(op_strategy(), 1..=35),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let client = deploy(&env);
        init_escrow(&env, &client);

        let pool: Vec<Address> = (0..POOL_SIZE).map(|_| Address::generate(&env)).collect();
        let mut model = BTreeSet::new();

        assert_matches_model(&env, &client, &pool, &model)?;

        for op in &ops {
            apply_op(&env, &client, &pool, &mut model, op);
            assert_matches_model(&env, &client, &pool, &model)?;
        }
    }

    #[test]
    fn prop_legacy_allowlist_index_migration_then_random_ops(
        legacy_entries in prop::collection::vec((0..POOL_SIZE, any::<bool>()), 0..=16),
        ops in prop::collection::vec(op_strategy(), 1..=25),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let client = deploy(&env);
        init_escrow(&env, &client);

        let pool: Vec<Address> = (0..POOL_SIZE).map(|_| Address::generate(&env)).collect();
        let mut model = BTreeSet::new();

        // Seed legacy layout directly in contract storage:
        // - DataKey::AllowlistIndex in instance storage (may contain duplicates or stale entries)
        // - DataKey::InvestorAllowlisted(addr) in persistent storage
        // - No AllowlistCount / AllowlistSlot / AllowlistSlotOf yet
        env.as_contract(&client.address, || {
            let mut legacy_vec: SorobanVec<Address> = SorobanVec::new(&env);
            for &(idx, allowed) in &legacy_entries {
                let addr = pool[idx].clone();
                legacy_vec.push_back(addr.clone());
                env.storage()
                    .persistent()
                    .set(&DataKey::InvestorAllowlisted(addr), &allowed);
                if allowed {
                    model.insert(idx);
                } else {
                    model.remove(&idx);
                }
            }
            env.storage()
                .instance()
                .set(&DataKey::AllowlistIndex, &legacy_vec);
        });

        // First read triggers migrate_allowlist_index_if_needed and must match model.
        assert_matches_model(&env, &client, &pool, &model)?;

        // Legacy key must be removed once migrated.
        env.as_contract(&client.address, || {
            assert!(
                !env.storage().instance().has(&DataKey::AllowlistIndex),
                "legacy AllowlistIndex should be removed after migration"
            );
        });

        // Subsequent random single/batch ops must preserve consistency.
        for op in &ops {
            apply_op(&env, &client, &pool, &mut model, op);
            assert_matches_model(&env, &client, &pool, &model)?;
        }
    }
}

/// Verifies that `migrate_allowlist_index_if_needed` handles a seeded legacy
/// `AllowlistIndex` containing active members, revoked (`false`) entries,
/// absent persistent entries, and duplicate addresses, as well as an empty
/// legacy `AllowlistIndex`.
#[test]
fn legacy_allowlist_index_migration_handles_stale_duplicates_and_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    // 1. Empty legacy AllowlistIndex is removed cleanly on read.
    env.as_contract(&client.address, || {
        let empty_legacy: SorobanVec<Address> = SorobanVec::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::AllowlistIndex, &empty_legacy);
    });
    assert_eq!(client.get_allowlisted_investors_count(), 0);
    env.as_contract(&client.address, || {
        assert!(!env.storage().instance().has(&DataKey::AllowlistIndex));
    });

    // 2. Populated legacy AllowlistIndex with active, stale (false), absent, and duplicate entries.
    let pool: Vec<Address> = (0..POOL_SIZE).map(|_| Address::generate(&env)).collect();
    let mut model = BTreeSet::new();

    env.as_contract(&client.address, || {
        let mut legacy_vec: SorobanVec<Address> = SorobanVec::new(&env);
        for (idx, addr) in pool.iter().enumerate() {
            legacy_vec.push_back(addr.clone());
            if idx % 3 == 0 {
                // Explicitly false in persistent storage -> must not receive a slot.
                env.storage()
                    .persistent()
                    .set(&DataKey::InvestorAllowlisted(addr.clone()), &false);
            } else if idx % 3 == 1 {
                // Absent from persistent storage -> default-to-deny, must not receive a slot.
            } else {
                // Active member -> must receive exactly one slot even if duplicated in legacy_vec.
                env.storage()
                    .persistent()
                    .set(&DataKey::InvestorAllowlisted(addr.clone()), &true);
                legacy_vec.push_back(addr.clone());
                model.insert(idx);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::AllowlistIndex, &legacy_vec);
    });

    assert_matches_model(&env, &client, &pool, &model).unwrap();

    env.as_contract(&client.address, || {
        assert!(
            !env.storage().instance().has(&DataKey::AllowlistIndex),
            "legacy AllowlistIndex should be removed after migration"
        );
    });
}
