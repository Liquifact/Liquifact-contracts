#allow(dead_code, unused_imports)]
/// Concurrency and idempotency tests for the key constructors in `escrow/src/keys.rs`.
///
/// These tests pin down the invariants that make concurrent execution around
/// the key builders deterministic:
///
/// 1. **Determinism ** — equal inputs always produce equal keys, and the
///    builders are pure (no hidden state, no env dependency).
/// 2. **Domain separation ** — different investors / nonces / key families
///    never collide.
/// 3. **Idempotency ** — repeated construction (e.g. retries) produces the
///    same key, so a retried write overwrites the same slot instead of
///    creating a duplicate entry.
/// 4. **Order independence ** — construction order does not affect the
///    resulting key set, so interleaved calls from concurrent logical
///    flows cannot diverge.
///
/// The tests exercise the public `crate::keys` builders directly and the
/// `unwrap_or`-style additive-key policy (ADR-007) by round-tripping keys
/// through storage in a fresh `Env`.

use crate::keys;
use crate::DataKey;
use crate::LiquifactEscrow;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger as _;
use soroban_sdk:{Address, Env};

/// Build a fresh env with a deterministic ledger so tests do not depend on
/// ambient test-harness state.
fn fresh_env() -> Env {
    let env = Env::default();
    let mut info = env.ledger().get();
    info.sequence_number = 100;
    info.timestamp = 0;
    env.ledger().set(info);
    env
}

/// Round-trip a key through instance storage to prove it is a valid,
/// deterministic storage key and that repeated writes are idempotent.
fn round_trip_instance(env: &Env, key: &DataKey, value: u32) -> u32 {
    env.storage().instance().set(key, &value);
    env.storage().instance().get(key).unwrap_or(0)
}

#[test]
fn investor_contribution_is_deterministic_and_pure() {
    let env = fresh_env();
    let investor = Address::generate(&env);

    // Same input => same key, regardless of how many times it is built.
    let k1 = keys::investor_contribution(investor.clone());
    let k2 = keys::investor_contribution(investor.clone());
    let k3 = keys::investor_contribution(investor.clone());
    assert_eq(k1, k2);
    assert_eq(k2, k3);

    // Round-trip through storage and rewrite the same slot twice.
    assert_eq(round_trip_instance(&env, &k1, 7), 7);
    assert_eq(round_trip_instance(&env, &k2, 7), 7);

    // The builder must not depend on the env at all.
    let env2 = fresh_env();
    let investor2 = Address::generate(&env2);
    let k3_different = keys::investor_contribution(investor2.clone());
    assert_ne(k1, k3_different);
    // Same address across envs still compares equal by its own value.
    assert_eq(
        keys::investor_contribution(investor.clone()),
        keys::investor_contribution(investor.clone()),
    );
}

#[test]
fn investor_key_families_are_distinct() {
    let env = fresh_env();
    let investor = Address::generate(&env);

    // Each per-investor family must map to a different discriminant so a
    // concurrent write to one family cannot clobber another.
    let contribution = keys::investor_contribution(investor.clone());
    let yield = keys::investor_effective_yield(investor.clone());
    let not_before = keys::investor_claim_not_before(investor.clone());
    let claimed = keys::investor_claimed(investor.clone());

    assert_ne(contribution, claimed);
    assert_ne(yield, claimed);
    assert_ne(not_before, claimed);
    assert_ne(contribution, yield);
    assert_ne(contribution, not_before);
    assert_ne(yield, not_before);
}

#[test]
fn instance_keys_are_stable_and_distinct() {
    let env = fresh_env();

    // Stability: repeated calls yield the same key (idempotent retries).
    assert_eq(keys::min_contribution_floor(), keys::min_contribution_floor());
    assert_eq(keys::max_unique_investors_cap(), keys::max_unique_investors_cap());
    assert_eq(keys::max_per_investor_cap(), keys::max_per_investor_cap());
    assert_eq(keys::unique_funder_count(), keys::unique_funder_count());
    assert_eq(keys::investor_index(), keys::investor_index());
    assert_eq(keys::funding_deadline(), keys::funding_deadline());
    assert_eq(
        keys::funding_close_snapshot(),
        keys::funding_close_snapshot(),
    );
    assert_eq(keys::funding_token(), keys::funding_token());
    assert_eq(
        keys::funding_token_scale(),
        keys::funding_token_scale(),
    );
    assert_eq(keys::callback_nonce(), keys::callback_nonce());
    assert_eq(keys::released_amount(), keys::released_amount());

    // Distinctness: every instance key family must be unique.
    let keys = [
        keys::min_contribution_floor(),
        keys::max_unique_investors_cap(),
        keys::max_per_investor_cap(),
        keys::unique_funder_count(),
        keys::investor_index(),
        keys::funding_deadline(),
        keys::funding_close_snapshot(),
        keys::funding_token(),
        keys::funding_token_scale(),
        keys::callback_nonce(),
        keys::released_amount(),
    ];
    for i in 0..keys.len() {
        for j in (i + 1)..keys.len() {
            assert_ne(keys[i].clone(), keys[j].clone());
        }
    }

    // Round-trip the callback context key to confirm it is a valid
    // instance storage key and that rewriting is idempotent.
    let nonce_key = keys::callback_context(42);
    assert_eq(round_trip_instance(&env, &nonce_key, 1), 1);
    assert_eq(round_trip_instance(&env, &nonce_key, 1), 1);
}

#[test]
fn callback_context_keys_are_nonce_scoped() {
    let env = fresh_env();

    // Distinct nonces must not collide, even when built in interleaved
    // orders (concurrent logical flows).
    let k0 = keys::callback_context(0);
    let k1 = keys::callback_context(1);
    let k2 = keys::callback_context(2);
    let k1_again = keys::callback_context(1);
    let k0_again = keys::callback_context(0);

    assert_eq(k1, k1_again);
    assert_eq(k0, k0_again);
    assert_ne(k0, k1);
    assert_ne(k1, k2);
    assert_ne(k0, k2);

    // Boundary: u64::MAX remains a distinct, stable key.
    let max = keys::callback_context(u64::MAX);
    assert_eq(max, keys::callback_context(u64::MAX));
    assert_ne(max, k2);
    assert_ne(max, k1);
    assert_ne(max, k0);

    // Round-trip the boundary nonce.
    assert_eq(round_trip_instance(&env, &max, 9), 9);
    assert_eq(round_trip_instance(&env, &max, 9), 9);
}

/// Regression: the collateral pledge key must be built through the
/// dedicated helper so call sites cannot drift. We assert the helper is
/// pure and that it differs from the per-investor family.
#[test]
fn collateral_pledge_key_is_stable_and_distinct() {
    let env = fresh_env();
    let investor = Address::generate(&env);

    let a = keys::collateral_pledge_key(investor.clone());
    let b = keys::collateral_pledge_key(investor.clone());
    assert_eq(a, b);
    assert_ne(a, keys::investor_contribution(investor.clone()));
    assert_ne(a, keys::investor_claimed(investor.clone()));

    // Rewriting the same slot is idempotent.
    assert_eq(round_trip_instance(&env, &a, 5), 5);
    assert_eq(round_trip_instance(&env, &b, 5), 5);
}

/// Regression: the additive-key policy (ADR-007) requires that an absent key
/// reads as the default without mutating state, and that a subsequent write
/// is observable. This guarantees that a concurrent reader that lost the
/// race to a writer still sees a consistent value.
#[test]
fn additive_key_read_default_then_write_is_consistent() {
    let env = fresh_env();
    let key = keys::funding_token_scale();

    // Absent key reads as the default and must not materialize state.
    assert(!env.storage().instance().has(&key));
    let absent: u32 = env.storage().instance().get(&key).unwrap_or(0);
    assert_eq(absent, 0);
    assert(!env.storage().instance().has(&key));

    // A write then a repeated write of the same value is idempotent.
    assert_eq(round_trip_instance(&env, &key, 7), 7);
    assert_eq(round_trip_instance(&env, &key, 7), 7);
    assert(env.storage().instance().has(&key));
}

/// Regression: a concurrent write to two different investors must not
/// clash on the same storage slot. We simulate the interleaving by
/// building both keys before writing and then verifying both values survive.
#[test]
fn interleaved_investor_writes_do_not_collide() {
    let env = fresh_env();
    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);

    // Both flows build their key before either writes.
    let key_a = keys::investor_contribution(investor_a.clone());
    let key_b = keys::investor_contribution(investor_b.clone());
    assert_ne(key_a.clone(), key_b.clone());

    // Interleaved writes.
    env.storage().instance().set(&key_a, 100);
    env.storage().instance().set(&key_b, 200);

    // Retried writes must not change the other investor's slot.
    env.storage().instance().set(&key_a, 100);
    env.storage().instance().set(&key_b, 200);

    let val_a: u32 = env.storage().instance().get(&key_a).unwrap_or(0);
    let val_b: u32 = env.storage().instance().get(&key_b).unwrap_or(0);
    assert_eq(val_a, 100);
    assert_eq(val_b, 200);
}

/// Regression: building a key must not mutate storage as a side effect.
/// This guarantees that a failed or retried flow that only built keys
/// leaves no partial state behind.
#[test]
fn key_builders_have_no_storage_side_effects() {
    let env = fresh_env();
    let investor = Address::generate(&env);

    let _ = keys::investor_contribution(investor.clone());
    let _ = keys::investor_effective_yield(investor.clone());
    let _ = keys::investor_claim_not_before(investor.clone());
    let _ = keys::investor_claimed(investor.clone());
    let _ = keys::min_contribution_floor();
    let _ = keys::max_unique_investors_cap();
    let _ = keys::max_per_investor_cap();
    let _ = keys::unique_funder_count();
    let _ = keys::investor_index();
    let _ = keys::funding_deadline();
    let _ = keys::funding_close_snapshot();
    let _ = keys::funding_token();
    let _ = keys::funding_token_scale();
    let _ = keys::callback_nonce();
    let _ = keys::callback_context(7);
    let _ = keys::released_amount();
    let _ = keys::collateral_pledge_key(investor.clone());

    // No key family should have been materialized in storage.
    assert(!env.storage().instance().has(&keys::min_contribution_floor()));
    assert(!env.storage().instance().has(&keys::funding_token()));
    assert(!env.storage().instance().has(&keys::callback_nonce()));
    assert(!env.storage().instance().has(&keys::callback_context(7)));
}

/// Regression: the contract type must remain registerable and the key
/// builders must be callable from a test that also exercises the contract
/// client, proving the public interface is preserved.
#[test]
fn key_builders_remain_compatible_with_contract_registration() {
    let env = fresh_env();
    let _id = env.register(LiquifactEscrow, ());

    // Builders are still callable and stable after registration.
    let investor = Address::generate(&env);
    assert_eq(
        keys::investor_contribution(investor.clone()),
        keys::investor_contribution(investor.clone()),
    );
    assert_eq(keys::funding_token(), keys::funding_token());
}
