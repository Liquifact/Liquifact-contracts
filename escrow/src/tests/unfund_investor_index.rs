use super::*;
use proptest::prelude::*;
use std::collections::BTreeSet;

/// Acceptance criterion 1:
/// The repro fails on main and passes on the branch:
/// fund(A, 100) -> unfund(A, 100) -> fund(A, 50) ->
/// returns one record (A, 50), and get_investors(0, 10).len() == get_unique_funder_count() == 1.
#[test]
pub(crate) fn test_unfund_and_refund_deduplication_repro() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);

    client.init(
        &admin,
        &String::from_str(&env, "INV_REPRO"),
        &sme,
        &10_000_000_000i128,
        &800i64,
        &0u64,
        &token.id,
        &None,
        &admin,
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

    let investor_a = Address::generate(&env);
    token.stellar.mint(&investor_a, &1_000_000i128);

    // 1. fund(A, 100)
    client.fund(&investor_a, &100i128);
    assert_eq!(client.get_unique_funder_count(), 1);
    assert_eq!(client.get_investors(&0, &10).len(), 1);
    assert_eq!(client.get_contribution(&investor_a), 100i128);

    // 2. unfund(A, 100)
    client.unfund(&investor_a, &100i128);
    assert_eq!(client.get_unique_funder_count(), 0);
    assert_eq!(client.get_investors(&0, &10).len(), 0);
    assert_eq!(client.get_funding_records(&0, &10).len(), 0);
    assert_eq!(client.get_contribution(&investor_a), 0i128);

    // 3. fund(A, 50)
    client.fund(&investor_a, &50i128);

    // Assert: exactly one record (A, 50)
    let records = client.get_funding_records(&0, &10);
    assert_eq!(records.len(), 1);
    assert_eq!(records.get(0).unwrap(), (investor_a.clone(), 50i128));

    // Assert: get_investors(0, 10).len() == get_unique_funder_count() == 1
    let investors = client.get_investors(&0, &10);
    assert_eq!(investors.len(), 1);
    assert_eq!(investors.get(0).unwrap(), investor_a);
    assert_eq!(client.get_unique_funder_count(), 1);
    assert_eq!(investors.len(), client.get_unique_funder_count());
    assert_eq!(client.get_escrow().funded_amount, 50i128);
}

/// Acceptance criterion 3:
/// A re-funding investor who uses fund_with_commitment after a full unfund
/// is accepted and gets the newly selected tier (stale yield cleared).
#[test]
pub(crate) fn test_refund_with_commitment_after_full_unfund_gets_new_tier() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 1000,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 500,
        yield_bps: 1400,
    });

    client.init(
        &admin,
        &String::from_str(&env, "TIER_REENTRY"),
        &sme,
        &100_000i128,
        &800i64,    // base yield
        &10_000u64, // maturity far in the future
        &token.id,
        &None,
        &admin,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let investor = Address::generate(&env);
    token.stellar.mint(&investor, &100_000i128);

    // Initial deposit with commitment matching Tier 1 (1000 bps)
    client.fund_with_commitment(&investor, &10_000i128, &150u64);
    assert_eq!(client.get_investor_yield_bps(&investor), 1000);
    assert_eq!(client.get_investor_claim_not_before(&investor), 150);

    // Full unfund
    client.unfund(&investor, &10_000i128);
    assert_eq!(client.get_contribution(&investor), 0);
    assert_eq!(client.get_unique_funder_count(), 0);
    assert_eq!(client.get_investors(&0, &10).len(), 0);

    // Stale entries cleared: yield falls back to base yield, claim lock is 0
    assert_eq!(client.get_investor_yield_bps(&investor), 800);
    assert_eq!(client.get_investor_claim_not_before(&investor), 0);

    // Re-funding with commitment matching Tier 2 (1400 bps) must succeed and apply the new tier
    client.fund_with_commitment(&investor, &8_000i128, &600u64);
    assert_eq!(client.get_investor_yield_bps(&investor), 1400);
    assert_eq!(client.get_investor_claim_not_before(&investor), 600);
    assert_eq!(client.get_contribution(&investor), 8_000);
    assert_eq!(client.get_unique_funder_count(), 1);
    assert_eq!(client.get_investors(&0, &10).len(), 1);
}

/// Acceptance criterion 4:
/// Pagination of the remaining investors after a removal from the middle is asserted,
/// so order is preserved.
#[test]
pub(crate) fn test_unfund_middle_investor_preserves_order_and_pagination() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);

    client.init(
        &admin,
        &String::from_str(&env, "PAGINATION"),
        &sme,
        &10_000_000_000i128,
        &800i64,
        &0u64,
        &token.id,
        &None,
        &admin,
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

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);

    token.stellar.mint(&inv_a, &1_000i128);
    token.stellar.mint(&inv_b, &1_000i128);
    token.stellar.mint(&inv_c, &1_000i128);

    client.fund(&inv_a, &100i128);
    client.fund(&inv_b, &200i128);
    client.fund(&inv_c, &300i128);

    assert_eq!(client.get_unique_funder_count(), 3);
    let initial_investors = client.get_investors(&0, &10);
    assert_eq!(initial_investors.len(), 3);
    assert_eq!(initial_investors.get(0).unwrap(), inv_a);
    assert_eq!(initial_investors.get(1).unwrap(), inv_b);
    assert_eq!(initial_investors.get(2).unwrap(), inv_c);

    // Remove middle investor B
    client.unfund(&inv_b, &200i128);

    assert_eq!(client.get_unique_funder_count(), 2);

    // Full list preserves relative order [A, C]
    let remaining = client.get_investors(&0, &10);
    assert_eq!(remaining.len(), 2);
    assert_eq!(remaining.get(0).unwrap(), inv_a);
    assert_eq!(remaining.get(1).unwrap(), inv_c);

    // Paginated reads across boundary
    let page0 = client.get_investors(&0, &1);
    assert_eq!(page0.len(), 1);
    assert_eq!(page0.get(0).unwrap(), inv_a);

    let page1 = client.get_investors(&1, &1);
    assert_eq!(page1.len(), 1);
    assert_eq!(page1.get(0).unwrap(), inv_c);

    let page_past = client.get_investors(&2, &1);
    assert_eq!(page_past.len(), 0);

    // Funding records pagination & order preservation
    let rec0 = client.get_funding_records(&0, &1);
    assert_eq!(rec0.len(), 1);
    assert_eq!(rec0.get(0).unwrap(), (inv_a.clone(), 100i128));

    let rec1 = client.get_funding_records(&1, &1);
    assert_eq!(rec1.len(), 1);
    assert_eq!(rec1.get(0).unwrap(), (inv_c.clone(), 300i128));

    let all_rec = client.get_funding_records(&0, &10);
    assert_eq!(all_rec.len(), 2);
    assert_eq!(all_rec.get(0).unwrap(), (inv_a.clone(), 100i128));
    assert_eq!(all_rec.get(1).unwrap(), (inv_c.clone(), 300i128));
}

/// Verify partial unfund does not decrement unique count or drop from index.
#[test]
pub(crate) fn test_partial_unfund_retains_index_and_unique_count() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);

    client.init(
        &admin,
        &String::from_str(&env, "PARTIAL"),
        &sme,
        &10_000_000_000i128,
        &800i64,
        &0u64,
        &token.id,
        &None,
        &admin,
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

    let inv = Address::generate(&env);
    token.stellar.mint(&inv, &1_000i128);

    client.fund(&inv, &100i128);
    assert_eq!(client.get_unique_funder_count(), 1);

    // Partial unfund: 40 out of 100
    client.unfund(&inv, &40i128);

    assert_eq!(client.get_unique_funder_count(), 1);
    assert_eq!(client.get_contribution(&inv), 60i128);

    let investors = client.get_investors(&0, &10);
    assert_eq!(investors.len(), 1);
    assert_eq!(investors.get(0).unwrap(), inv);

    let records = client.get_funding_records(&0, &10);
    assert_eq!(records.len(), 1);
    assert_eq!(records.get(0).unwrap(), (inv.clone(), 60i128));
}

proptest! {
    /// Acceptance criterion 2:
    /// A proptest over random sequences of fund / partial unfund / full unfund / re-fund
    /// for 1–8 investors asserts, after every step:
    /// - index uniqueness
    /// - index.len() == unique_count
    /// - Σ get_funding_records == funded_amount
    #[test]
    fn prop_investor_index_lifecycle_invariants(
        n_investors in 1usize..=8usize,
        steps in prop::collection::vec((0usize..8usize, 0u8..4u8, 10i128..10_000i128), 1..30),
    ) {
        let env = Env::default();
        let (client, admin, sme) = setup(&env);
        let token = install_stellar_asset_token(&env);

        let target = 1_000_000_000_000i128; // Large target so status stays 0
        client.init(
            &admin,
            &String::from_str(&env, "PROPTEST"),
            &sme,
            &target,
            &800i64,
            &0u64,
            &token.id,
            &None,
            &admin,
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

        let mut investors = std::vec::Vec::new();
        for _ in 0..n_investors {
            let inv = Address::generate(&env);
            token.stellar.mint(&inv, &100_000_000i128);
            investors.push(inv);
        }

        for (raw_idx, action_type, raw_amount) in steps {
            let inv_idx = raw_idx % n_investors;
            let inv = &investors[inv_idx];
            let current_contrib = client.get_contribution(inv);

            match action_type {
                0 => {
                    // Fund
                    client.fund(inv, &raw_amount);
                }
                1 => {
                    // Partial unfund
                    if current_contrib > 1 {
                        let unfund_amt = (raw_amount % (current_contrib - 1)) + 1;
                        client.unfund(inv, &unfund_amt);
                    } else {
                        client.fund(inv, &raw_amount);
                    }
                }
                2 => {
                    // Full unfund
                    if current_contrib > 0 {
                        client.unfund(inv, &current_contrib);
                    } else {
                        client.fund(inv, &raw_amount);
                    }
                }
                _ => {
                    // Re-fund
                    client.fund(inv, &raw_amount);
                }
            }

            // Assertions after every single step:
            let index = client.get_investors(&0, &100);
            let unique_count = client.get_unique_funder_count();

            // 1. Index uniqueness: no duplicates
            let mut seen = BTreeSet::new();
            for i in 0..index.len() {
                let addr = index.get(i).unwrap();
                prop_assert!(seen.insert(addr.clone()), "Duplicate investor address in index: {:?}", addr);
            }

            // 2. index.len() == unique_count
            prop_assert_eq!(index.len(), unique_count, "index.len() != unique_count");

            // 3. Σ get_funding_records == funded_amount
            let records = client.get_funding_records(&0, &100);
            prop_assert_eq!(records.len(), index.len(), "records.len() != index.len()");

            let mut sum_records: i128 = 0;
            for i in 0..records.len() {
                let (addr, amt) = records.get(i).unwrap();
                prop_assert!(amt > 0, "Indexed investor had contribution <= 0: {:?}", addr);
                prop_assert_eq!(client.get_contribution(&addr), amt, "record contribution != stored contribution");
                sum_records += amt;
            }

            let funded_amount = client.get_escrow().funded_amount;
            prop_assert_eq!(sum_records, funded_amount, "Σ get_funding_records != funded_amount");
        }
    }
}
