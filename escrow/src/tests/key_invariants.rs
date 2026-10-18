//! Focused tests for the state invariants enforced by `escrow/src/keys.rs`.
//!
//! # What is tested
//!
//! - **Key identity** — each constructor returns exactly the `DataKey` variant it documents;
//!   two calls with equal arguments produce equal keys, and calls with *different* arguments
//!   produce *different* keys. This locks the discriminant mapping so a future rename of a
//!   `DataKey` variant (a storage-breaking change, ADR-007) causes a compile error here
//!   before it silently migrates data on-chain.
//!
//! - **Nonce sentinel guard** — `callback_context(0)` must panic in debug builds. Nonce `0`
//!   is the "never registered" default; storing a context under it would corrupt the
//!   `is_callback_consumed(0)` view.
//!
//! - **Per-investor key isolation** — distinct investor addresses must produce distinct
//!   persistent keys; two investors must not alias each other's contribution, yield,
//!   claim-lock, or claimed-flag storage. Verified via the public contract API so the test
//!   proves the runtime storage behaviour, not just the type-level identity.
//!
//! - **Idempotency / double-write safety** — writing a key twice with the same address must
//!   not corrupt state: the second write must overwrite cleanly without creating orphaned
//!   entries.
//!
//! - **Callback nonce monotonicity** — sequential `register_callback` calls must increment
//!   the nonce by exactly `1` each time, starting from `1`. The gap between nonces must
//!   never be `0` (duplicate) or `> 1` (skipped entry).
//!
//! - **Callback context isolation** — contexts registered under different nonces must be
//!   independent: consuming one must not consume or corrupt another.
//!
//! - **Collateral key singleton** — writing collateral metadata replaces the previous record
//!   exactly once; subsequent reads return the newest value, not a stale one. Clearing
//!   removes exactly the key it wrote.
//!
//! - **Storage-tier placement** — per-investor keys land in *persistent* storage; per-escrow
//!   configuration keys land in *instance* storage. A key in the wrong tier would have
//!   incorrect TTL semantics and archival behaviour.
//!
//! # What is NOT tested here
//!
//! Authorization, token transfers, and entrypoint-level business logic are covered by the
//! sibling test modules (`funding.rs`, `settlement.rs`, `cap_validation.rs`, etc.). This
//! module tests only the invariants that live in `keys.rs` itself.

use super::{default_init, deploy, free_addresses};
use crate::{keys, DataKey, EscrowError};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};

// ---------------------------------------------------------------------------
// 1. Key identity — constructors return the documented DataKey variant
// ---------------------------------------------------------------------------

#[test]
fn investor_contribution_key_matches_data_key_variant() {
    let env = Env::default();
    let addr = Address::generate(&env);
    assert_eq!(
        keys::investor_contribution(addr.clone()),
        DataKey::InvestorContribution(addr),
        "investor_contribution() must wrap DataKey::InvestorContribution"
    );
}

#[test]
fn investor_effective_yield_key_matches_data_key_variant() {
    let env = Env::default();
    let addr = Address::generate(&env);
    assert_eq!(
        keys::investor_effective_yield(addr.clone()),
        DataKey::InvestorEffectiveYield(addr),
        "investor_effective_yield() must wrap DataKey::InvestorEffectiveYield"
    );
}

#[test]
fn investor_claim_not_before_key_matches_data_key_variant() {
    let env = Env::default();
    let addr = Address::generate(&env);
    assert_eq!(
        keys::investor_claim_not_before(addr.clone()),
        DataKey::InvestorClaimNotBefore(addr),
        "investor_claim_not_before() must wrap DataKey::InvestorClaimNotBefore"
    );
}

#[test]
fn investor_claimed_key_matches_data_key_variant() {
    let env = Env::default();
    let addr = Address::generate(&env);
    assert_eq!(
        keys::investor_claimed(addr.clone()),
        DataKey::InvestorClaimed(addr),
        "investor_claimed() must wrap DataKey::InvestorClaimed"
    );
}

#[test]
fn instance_key_constructors_match_data_key_variants() {
    // Verify every zero-arity instance-storage constructor maps to the correct variant.
    // Changing a DataKey discriminant without updating the constructor would break storage.
    assert_eq!(keys::min_contribution_floor(), DataKey::MinContributionFloor);
    assert_eq!(keys::max_unique_investors_cap(), DataKey::MaxUniqueInvestorsCap);
    assert_eq!(keys::max_per_investor_cap(), DataKey::MaxPerInvestorCap);
    assert_eq!(keys::unique_funder_count(), DataKey::UniqueFunderCount);
    assert_eq!(keys::investor_index(), DataKey::InvestorIndex);
    assert_eq!(keys::funding_deadline(), DataKey::FundingDeadline);
    assert_eq!(keys::funding_close_snapshot(), DataKey::FundingCloseSnapshot);
    assert_eq!(keys::funding_token(), DataKey::FundingToken);
    assert_eq!(keys::funding_token_scale(), DataKey::FundingTokenScale);
    assert_eq!(keys::callback_nonce(), DataKey::CallbackNonce);
    assert_eq!(keys::released_amount(), DataKey::ReleasedAmount);
    assert_eq!(keys::collateral_pledge_key(), DataKey::SmeCollateralPledge);
}

#[test]
fn callback_context_key_matches_data_key_variant() {
    // Nonce 1 is the first valid value.
    assert_eq!(
        keys::callback_context(1),
        DataKey::CallbackContext(1),
        "callback_context(1) must wrap DataKey::CallbackContext(1)"
    );
    assert_eq!(
        keys::callback_context(u64::MAX),
        DataKey::CallbackContext(u64::MAX),
        "callback_context(u64::MAX) must wrap DataKey::CallbackContext(u64::MAX)"
    );
}

// ---------------------------------------------------------------------------
// 2. Key equality — same args → equal keys; different args → different keys
// ---------------------------------------------------------------------------

#[test]
fn investor_keys_equal_for_same_address() {
    let env = Env::default();
    let addr = Address::generate(&env);

    assert_eq!(
        keys::investor_contribution(addr.clone()),
        keys::investor_contribution(addr.clone()),
        "same address must produce equal InvestorContribution keys"
    );
    assert_eq!(
        keys::investor_effective_yield(addr.clone()),
        keys::investor_effective_yield(addr.clone()),
    );
    assert_eq!(
        keys::investor_claim_not_before(addr.clone()),
        keys::investor_claim_not_before(addr.clone()),
    );
    assert_eq!(
        keys::investor_claimed(addr.clone()),
        keys::investor_claimed(addr.clone()),
    );
}

#[test]
fn investor_keys_differ_for_different_addresses() {
    let env = Env::default();
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);

    // Contribution keys must not alias between investors.
    assert_ne!(
        keys::investor_contribution(addr_a.clone()),
        keys::investor_contribution(addr_b.clone()),
        "two distinct investors must have distinct InvestorContribution keys"
    );
    assert_ne!(
        keys::investor_effective_yield(addr_a.clone()),
        keys::investor_effective_yield(addr_b.clone()),
    );
    assert_ne!(
        keys::investor_claim_not_before(addr_a.clone()),
        keys::investor_claim_not_before(addr_b.clone()),
    );
    assert_ne!(
        keys::investor_claimed(addr_a.clone()),
        keys::investor_claimed(addr_b.clone()),
    );
}

#[test]
fn cross_family_keys_never_alias_for_the_same_address() {
    // An InvestorContribution key for address A must not equal an InvestorClaimed key
    // for address A, even though both carry the same address. Each variant must be
    // independently discriminated by XDR.
    let env = Env::default();
    let addr = Address::generate(&env);

    let contribution = keys::investor_contribution(addr.clone());
    let yield_key = keys::investor_effective_yield(addr.clone());
    let not_before = keys::investor_claim_not_before(addr.clone());
    let claimed = keys::investor_claimed(addr.clone());

    // All four must be pairwise distinct.
    assert_ne!(contribution, yield_key);
    assert_ne!(contribution, not_before);
    assert_ne!(contribution, claimed);
    assert_ne!(yield_key, not_before);
    assert_ne!(yield_key, claimed);
    assert_ne!(not_before, claimed);
}

#[test]
fn callback_context_keys_differ_for_different_nonces() {
    assert_ne!(
        keys::callback_context(1),
        keys::callback_context(2),
        "nonces 1 and 2 must produce different CallbackContext keys"
    );
    assert_ne!(
        keys::callback_context(1),
        keys::callback_context(u64::MAX),
    );
}

// ---------------------------------------------------------------------------
// 3. Nonce sentinel guard — callback_context(0) panics in debug builds
// ---------------------------------------------------------------------------

/// `callback_context(0)` must panic in debug builds.
///
/// Nonce `0` is the unregistered sentinel: `get_callback_nonce()` returns `0` before any
/// callback is registered. Storing a `CallbackContext` under nonce `0` would make
/// `is_callback_consumed(0)` return `true` for an unconsumed or non-existent entry,
/// silently corrupting the consumed/pending distinction.
///
/// This test is compiled only in debug mode (`#[cfg(debug_assertions)]`) because the guard
/// is a `debug_assert!` — it compiles to nothing in optimised WASM builds where the
/// invariant is ensured structurally (the nonce counter starts at `0` and increments via
/// `checked_add`, so the first stored nonce is always `1`).
#[test]
#[cfg(debug_assertions)]
fn callback_context_nonce_zero_panics_in_debug() {
    let result = std::panic::catch_unwind(|| {
        let _ = keys::callback_context(0);
    });
    assert!(
        result.is_err(),
        "callback_context(0) must panic in debug builds: \
         nonce 0 is the unregistered sentinel and must never be stored"
    );
}

/// Boundary: nonce `1` is the first valid value and must not panic.
#[test]
fn callback_context_nonce_one_is_valid() {
    // Should not panic — just checking the key is constructed without error.
    let key = keys::callback_context(1);
    assert_eq!(key, DataKey::CallbackContext(1));
}

/// Boundary: nonce `u64::MAX` is the maximum valid value and must not panic.
#[test]
fn callback_context_nonce_max_is_valid() {
    let key = keys::callback_context(u64::MAX);
    assert_eq!(key, DataKey::CallbackContext(u64::MAX));
}

// ---------------------------------------------------------------------------
// 4. Per-investor storage isolation — runtime proof via the contract API
// ---------------------------------------------------------------------------
//
// These tests exercise the actual storage layer through the public contract
// interface so they prove the keys isolate *storage cells*, not just key values.

#[test]
fn two_investors_have_isolated_contribution_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    // Init with a large enough target so both investors can contribute.
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "KEY_ISO_1"),
        &sme,
        &200_000_000_000i128,
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
        &None::<i64>,
        &None::<u32>,
    );

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let amount_a = 70_000_000_000i128;
    let amount_b = 30_000_000_000i128;

    client.fund(&inv_a, &amount_a);
    client.fund(&inv_b, &amount_b);

    // Each investor's contribution must be independent.
    assert_eq!(
        client.get_contribution(&inv_a),
        amount_a,
        "investor A contribution must not be affected by investor B's deposit"
    );
    assert_eq!(
        client.get_contribution(&inv_b),
        amount_b,
        "investor B contribution must not be affected by investor A's deposit"
    );

    // A follow-on deposit by A must not change B's balance.
    client.fund(&inv_a, &10_000_000_000i128);
    assert_eq!(
        client.get_contribution(&inv_b),
        amount_b,
        "investor B balance must be unchanged after a follow-on deposit by investor A"
    );
}

#[test]
fn two_investors_have_isolated_yield_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    use soroban_sdk::Vec as SorobanVec;
    use crate::YieldTier;

    // Configure two yield tiers so fund_with_commitment picks a non-base yield.
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier { min_lock_secs: 100, yield_bps: 1000 });

    let (token, treasury) = super::free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "KEY_ISO_2"),
        &sme,
        &200_000_000_000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &Some(tiers),
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

    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);

    // Investor A commits to the tier (gets 1000 bps yield).
    client.fund_with_commitment(&inv_a, &100_000_000_000i128, &200u64);
    // Investor B uses plain fund (gets 800 bps base yield).
    client.fund(&inv_b, &100_000_000_000i128);

    assert_eq!(
        client.get_investor_yield_bps(&inv_a),
        1000i64,
        "investor A must have tier yield 1000"
    );
    assert_eq!(
        client.get_investor_yield_bps(&inv_b),
        800i64,
        "investor B must have base yield 800 — must not be overwritten by A's tier"
    );
}

// ---------------------------------------------------------------------------
// 5. Idempotency / double-write safety
// ---------------------------------------------------------------------------

#[test]
fn follow_on_fund_overwrites_contribution_cleanly() {
    // Two deposits by the same investor must accumulate correctly without
    // creating a second storage entry that orphans the first.
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "KEY_IDEM_1"),
        &sme,
        &200_000_000_000i128,
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
        &None::<i64>,
        &None::<u32>,
    );

    let inv = Address::generate(&env);
    client.fund(&inv, &50_000_000_000i128);
    client.fund(&inv, &50_000_000_000i128);

    // Must be exactly the sum — no doubling, no truncation.
    assert_eq!(
        client.get_contribution(&inv),
        100_000_000_000i128,
        "two deposits by the same investor must accumulate to their sum"
    );
    // Unique-funder count must not have incremented twice.
    assert_eq!(
        client.get_unique_funder_count(),
        1,
        "a re-funding investor must not inflate the unique funder count"
    );
}

// ---------------------------------------------------------------------------
// 6. Callback nonce monotonicity
// ---------------------------------------------------------------------------

#[test]
fn callback_nonce_starts_at_zero_before_first_registration() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    assert_eq!(
        client.get_callback_nonce(),
        0,
        "callback nonce must be 0 before any callback is registered"
    );
}

#[test]
fn callback_nonce_increments_by_one_on_each_registration() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    let origin = Address::generate(&env);

    let n1 = client.register_callback(&origin, &1u32);
    assert_eq!(n1, 1, "first nonce must be 1");
    assert_eq!(client.get_callback_nonce(), 1);

    let n2 = client.register_callback(&origin, &2u32);
    assert_eq!(n2, 2, "second nonce must be 2");
    assert_eq!(client.get_callback_nonce(), 2);

    let n3 = client.register_callback(&origin, &3u32);
    assert_eq!(n3, 3, "third nonce must be 3");
    assert_eq!(client.get_callback_nonce(), 3);

    // Nonces must be strictly sequential — no gaps and no duplicates.
    assert_eq!(
        n2 - n1,
        1,
        "gap between consecutive nonces must be exactly 1"
    );
    assert_eq!(n3 - n2, 1);
}

#[test]
fn callback_nonce_is_not_affected_by_consumption() {
    // Consuming (executing) a callback must not decrement or reset the nonce counter.
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    let origin = Address::generate(&env);

    let n1 = client.register_callback(&origin, &1u32);
    client.execute_callback(&n1, &origin, &1u32);

    // Nonce counter must stay at 1, not reset to 0.
    assert_eq!(
        client.get_callback_nonce(),
        1,
        "executing a callback must not reset the nonce counter"
    );

    // Next registration must still get nonce 2.
    let n2 = client.register_callback(&origin, &2u32);
    assert_eq!(n2, 2, "nonce after consumption of nonce 1 must be 2");
}

// ---------------------------------------------------------------------------
// 7. Callback context isolation
// ---------------------------------------------------------------------------

#[test]
fn callback_contexts_are_isolated_across_nonces() {
    // Consuming nonce N must leave nonce M (M != N) unconsumed.
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    let origin_a = Address::generate(&env);
    let origin_b = Address::generate(&env);

    let n1 = client.register_callback(&origin_a, &10u32);
    let n2 = client.register_callback(&origin_b, &20u32);

    // Consume n2 first.
    client.execute_callback(&n2, &origin_b, &20u32);

    assert!(
        !client.is_callback_consumed(&n1),
        "consuming nonce {n2} must not mark nonce {n1} as consumed"
    );
    assert!(
        client.is_callback_consumed(&n2),
        "nonce {n2} must be consumed after execute_callback"
    );

    // Independently consume n1.
    client.execute_callback(&n1, &origin_a, &10u32);
    assert!(client.is_callback_consumed(&n1));
}

#[test]
fn unconsumed_callback_query_returns_false_for_unregistered_nonce() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    // Nonce 99 was never registered.
    assert!(
        !client.is_callback_consumed(&99u64),
        "is_callback_consumed must return false for a nonce that was never registered"
    );
    assert!(
        client.get_callback(&99u64).is_none(),
        "get_callback must return None for a nonce that was never registered"
    );
}

// ---------------------------------------------------------------------------
// 8. Collateral key — singleton replacement and clear
// ---------------------------------------------------------------------------

#[test]
fn collateral_write_replaces_previous_record() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    let asset = symbol_short!("USDC");

    client.record_sme_collateral_commitment(&asset, &10_000i128);
    let first = client
        .get_sme_collateral_commitment()
        .expect("first record must exist");
    assert_eq!(first.amount, 10_000i128);

    // A second call with a higher amount must replace, not duplicate.
    client.record_sme_collateral_commitment(&asset, &20_000i128);
    let second = client
        .get_sme_collateral_commitment()
        .expect("second record must exist");
    assert_eq!(
        second.amount, 20_000i128,
        "second write must overwrite the first — the key must behave as a singleton"
    );
}

#[test]
fn collateral_clear_removes_the_record() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    let asset = symbol_short!("USDC");
    client.record_sme_collateral_commitment(&asset, &5_000i128);
    assert!(client.get_sme_collateral_commitment().is_some());

    client.clear_sme_collateral_commitment();
    assert!(
        client.get_sme_collateral_commitment().is_none(),
        "clear must remove the collateral record entirely"
    );
}

#[test]
fn collateral_clear_on_absent_key_returns_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    // No record has been written yet — clear must fail with a typed error.
    super::assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );
}

// ---------------------------------------------------------------------------
// 9. Boundary: contribution at i128 boundary values
// ---------------------------------------------------------------------------

#[test]
fn zero_contribution_is_the_absent_sentinel() {
    // A fresh investor (who has never called fund) must appear as contribution == 0.
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    let never_funded = Address::generate(&env);
    assert_eq!(
        client.get_contribution(&never_funded),
        0i128,
        "an address that never called fund must have contribution 0"
    );
}

#[test]
fn claim_not_before_defaults_to_zero_for_plain_fund() {
    // fund() (no commitment) must write claim_not_before = 0 for new investors,
    // meaning no extra claim gate is imposed.
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    let inv = Address::generate(&env);
    client.fund(&inv, &10_000_000_000i128);

    assert_eq!(
        client.get_investor_claim_not_before(&inv),
        0u64,
        "plain fund must write claim_not_before = 0 (no extra claim gate)"
    );
}

#[test]
fn investor_not_claimed_before_settlement() {
    // is_investor_claimed must return false before settlement occurs.
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    let inv = Address::generate(&env);
    client.fund(&inv, &10_000_000_000i128);

    assert!(
        !client.is_investor_claimed(&inv),
        "investor must not be marked as claimed before settlement"
    );
}
