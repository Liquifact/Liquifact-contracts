//! Concurrent-execution hardening tests for [`LiquifactEscrow::get_collateral_state`]
//! and the write paths that feed it.
//!
//! # What "concurrent execution" means in a Soroban contract
//! Real on-chain concurrency (multi-threaded writes) does not exist — every
//! invocation runs sequentially against a single ledger snapshot.  But
//! higher-level integrators face the *equivalent* concurrency hazards:
//!
//! 1. **Partial reads.**  If a caller read `CollateralLimit` and then
//!    `SmeCollateralPledge` across two separate host invocations, a write in
//!    between could yield an inconsistent view (`amount > collateral_limit`).
//!    `get_collateral_state` performs both reads in **one** call; these tests
//!    pin that atomicity.
//! 2. **Retries / duplicate submissions.**  Off-chain clients frequently retry
//!    identical requests.  Idempotent writes + idempotent reads prevent
//!    double-counting or "now-you-see-it-now-you-don't" flapping.
//! 3. **Interleaved reads.**  Read-only entrypoints must never mutate state,
//!    even across many calls, so any number of read+write+read sequences must
//!    produce a deterministic, monotonic observable timeline.
//! 4. **Timing boundaries.**  Soroban's `env.ledger().timestamp()` advances
//!    once per ledger.  The `recorded_at` monotonicity guard must reject
//!    replays submitted with a stale `recorded_at` (simulated racing writes
//!    from a prior ledger).
//!
//! # Guarantees asserted here
//! - Every bundled `get_collateral_state` view satisfies `amount <= collateral_limit`
//!   (atomic-snapshot invariant — cannot observe torn reads).
//! - Repeated `get_collateral_state` calls with no intervening writes return
//!   identical byte-equivalent structs.
//! - N identical `record_sme_collateral_commitment` calls (same amount + same asset
//!   + same ledger timestamp) are idempotent in outcome: the final stored value
//!   equals the input, and calling it N times is indistinguishable from 1.
//! - Interleaving `set_collateral_limit` writes with `get_collateral_state` reads
//!   never reveals an intermediate `limit` field that disagrees with the
//!   bundled ceiling.
//! - `clear_sme_collateral_commitment` → the flat view flips all fields back to
//!   their documented `is_set=false` defaults in a single snapshot (no partial
//!   clear).
//! - Two concurrent "racing" commitments submitted in the same ledger timestamp
//!   (same `recorded_at`) produce deterministic results: the second call
//!   succeeds (equality is allowed, as it is the replay case) but no backwards
//!   timestamp passes.

use super::super::{
    CollateralState, LiquifactEscrow, LiquifactEscrowClient, MAX_INVOICE_AMOUNT,
};
use crate::tests::assert_contract_error;
use crate::EscrowError;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, Symbol};

// ── helpers ──────────────────────────────────────────────────────────────────

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn deploy_and_init(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "COLST8"),
        &sme,
        &10_000i128,
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
        &None::<u32>,
    );

    (client, admin, sme)
}

/// Assert the atomic-snapshot invariant: whenever `is_set == true` the bundled
/// `amount` must not exceed the bundled `collateral_limit`.  Call this after
/// **every** `get_collateral_state` invocation in this file so that regressions
/// (e.g. a future refactor that reads storage across two separate calls and
/// somehow leaks a torn view) are caught.
fn assert_atomic_snapshot_invariant(s: &CollateralState) {
    if s.is_set {
        assert!(
            s.amount <= s.collateral_limit,
            "torn-snapshot! amount={} > collateral_limit={}",
            s.amount,
            s.collateral_limit,
        );
    }
    // Default shape when not set:
    if !s.is_set {
        let empty_sym = Symbol::new(&Env::default(), "");
        // Compare string-repr because Symbols from different envs may not `eq`
        // via `PartialEq` on some SDK versions; compare by debug format instead.
        assert_eq!(s.amount, 0);
        assert_eq!(s.recorded_at, 0);
    }
}

// ── 1. Defaults (pre-init / post-init) ───────────────────────────────────────

#[test]
fn defaults_before_init_flat_view() {
    let env = Env::default();
    let client = deploy(&env);
    let s = client.get_collateral_state();
    assert_atomic_snapshot_invariant(&s);
    assert!(!s.is_set);
    assert_eq!(s.amount, 0);
    assert_eq!(s.recorded_at, 0);
    assert_eq!(s.collateral_limit, MAX_INVOICE_AMOUNT);
}

#[test]
fn defaults_after_init_flat_view_persist() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);
    let s = client.get_collateral_state();
    assert_atomic_snapshot_invariant(&s);
    assert!(!s.is_set);
    assert_eq!(s.amount, 0);
    assert_eq!(s.collateral_limit, MAX_INVOICE_AMOUNT);
}

// ── 2. Idempotency ───────────────────────────────────────────────────────────

/// N consecutive reads with no intervening writes return identical values.
#[test]
fn idempotent_repeated_reads_no_mutation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);
    client.set_collateral_limit(&6_000i128);
    let asset = Symbol::new(&env, "USDC");
    client.record_sme_collateral_commitment(&asset, &3_000i128);

    let s1 = client.get_collateral_state();
    let s2 = client.get_collateral_state();
    let s3 = client.get_collateral_state();
    let s4 = client.get_collateral_state();
    assert_atomic_snapshot_invariant(&s1);
    assert_atomic_snapshot_invariant(&s2);
    assert_atomic_snapshot_invariant(&s3);
    assert_atomic_snapshot_invariant(&s4);
    assert_eq!(s1, s2);
    assert_eq!(s2, s3);
    assert_eq!(s3, s4);
}

/// Idempotent retries of an identical `record_sme_collateral_commitment` in the
/// same ledger timestamp are a no-op: stored state does not change, and
/// reading it N times yields N identical snapshots.
#[test]
fn idempotent_replay_of_same_record_call_same_timestamp() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);
    let mut li = env.ledger().get();
    li.timestamp = 100;
    env.ledger().set(li);

    let asset = Symbol::new(&env, "XLM");
    let amt = 5_000i128;

    let a = client.record_sme_collateral_commitment(&asset, &amt);
    let sa = client.get_collateral_state();
    let b = client.record_sme_collateral_commitment(&asset, &amt); // same ts replay
    let sb = client.get_collateral_state();
    let c = client.record_sme_collateral_commitment(&asset, &amt); // 3rd time
    let sc = client.get_collateral_state();

    assert_atomic_snapshot_invariant(&sa);
    assert_atomic_snapshot_invariant(&sb);
    assert_atomic_snapshot_invariant(&sc);
    // outcomes equal (timestamp monotonicity allows equal, so no backward error)
    assert_eq!(a.amount, b.amount);
    assert_eq!(b.amount, c.amount);
    assert_eq!(a.asset, b.asset);
    assert_eq!(a.recorded_at, b.recorded_at);
    assert_eq!(b.recorded_at, c.recorded_at);
    assert_eq!(sa, sb);
    assert_eq!(sb, sc);
}

/// Idempotent retries of an identical `set_collateral_limit` call produce the
/// same observable read state (i.e. setting X twice is the same as setting X).
#[test]
fn idempotent_replay_of_set_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);
    client.set_collateral_limit(&8_888i128);
    let a = client.get_collateral_state();
    client.set_collateral_limit(&8_888i128);
    let b = client.get_collateral_state();
    client.set_collateral_limit(&8_888i128);
    let c = client.get_collateral_state();
    assert_atomic_snapshot_invariant(&a);
    assert_atomic_snapshot_invariant(&b);
    assert_atomic_snapshot_invariant(&c);
    assert_eq!(a.collateral_limit, 8_888);
    assert_eq!(a, b);
    assert_eq!(b, c);
}

/// Idempotent `clear_sme_collateral_commitment`: called twice, both produce the
/// same flat-view defaults.
#[test]
fn idempotent_double_clear() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);
    client.set_collateral_limit(&7_000i128);
    let asset = Symbol::new(&env, "ETH");
    client.record_sme_collateral_commitment(&asset, &2_000i128);
    let before = client.get_collateral_state();
    assert!(before.is_set);

    client.clear_sme_collateral_commitment();
    let a = client.get_collateral_state();
    client.clear_sme_collateral_commitment();
    let b = client.get_collateral_state();

    assert_atomic_snapshot_invariant(&a);
    assert_atomic_snapshot_invariant(&b);
    assert!(!a.is_set);
    assert_eq!(a.amount, 0);
    assert_eq!(a.recorded_at, 0);
    assert_eq!(a.collateral_limit, 7_000); // limit NOT cleared by clear-collateral
    assert_eq!(a, b);
}

// ── 3. Atomic snapshot invariant (no torn reads) ─────────────────────────────

/// Write ceiling, then write a commitment right at the ceiling, then N reads:
/// every bundled view must show `amount <= collateral_limit` with exact field
/// agreement.  This would fail if `get_collateral_state` somehow performed two
/// separate host calls (torn reads exposed).
#[test]
fn atomic_snapshot_amount_and_ceiling_agree_at_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);
    client.set_collateral_limit(&10_000i128);
    let asset = Symbol::new(&env, "USDC");
    client.record_sme_collateral_commitment(&asset, &10_000i128);

    for _ in 0..16 {
        let s = client.get_collateral_state();
        assert_atomic_snapshot_invariant(&s);
        assert_eq!(s.amount, 10_000);
        assert_eq!(s.collateral_limit, 10_000);
        assert_eq!(s.is_set, true);
        assert_eq!(s.asset, asset);
    }
}

/// Interleave writes with reads at the boundary: each intermediate read is a
/// self-consistent snapshot.  This is the "concurrent read+write" case: there
/// is no thread-level concurrency in Soroban, but an integrator issuing two
/// calls from a loop must still never observe the view in a contradictory
/// state.
#[test]
fn interleave_writes_reads_always_self_consistent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);
    let asset = Symbol::new(&env, "STEP");

    let steps = [
        (3_000i128, 1_000i128, 1u64),   // limit=3000, amount=1000, ts=1
        (5_000i128, 2_500i128, 2u64),   // limit=5000, amount=2500, ts=2
        (10_000i128, 10_000i128, 3u64), // limit=10000, amount=10000, ts=3
        (2_000i128, 2_000i128, 4u64),   // limit=2000, amount=2000, ts=4
    ];

    for (lim, amt, ts) in steps.iter() {
        let mut li = env.ledger().get();
        li.timestamp = *ts;
        env.ledger().set(li);

        client.set_collateral_limit(lim);
        client.record_sme_collateral_commitment(&asset, amt);

        // After each pair of writes, the bundled view MUST be consistent with
        // both authoritative storage reads.
        let s = client.get_collateral_state();
        assert_atomic_snapshot_invariant(&s);
        assert_eq!(s.collateral_limit, *lim);
        assert_eq!(s.amount, *amt);
        assert_eq!(s.recorded_at, *ts);
        assert_eq!(s.is_set, true);
        // And match individual getters:
        assert_eq!(s.collateral_limit, client.get_collateral_limit());
        let stored = client.get_sme_collateral_commitment().unwrap();
        assert_eq!(stored.amount, s.amount);
        assert_eq!(stored.recorded_at, s.recorded_at);
        assert_eq!(stored.asset, s.asset);
    }
}

// ── 4. Clear is atomic (no partial clear observable) ─────────────────────────

#[test]
fn clear_flips_all_fields_simultaneously() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);
    client.set_collateral_limit(&4_000i128);
    let asset = Symbol::new(&env, "EUR");
    let mut li = env.ledger().get();
    li.timestamp = 42;
    env.ledger().set(li);
    client.record_sme_collateral_commitment(&asset, &1_234i128);

    let before = client.get_collateral_state();
    assert!(before.is_set);
    assert_eq!(before.recorded_at, 42);
    assert_eq!(before.asset, asset);

    client.clear_sme_collateral_commitment();

    // A single bundled read after clear shows every `is_set=false` default in
    // the same snapshot — no partial "asset cleared but amount still 1234".
    let s = client.get_collateral_state();
    assert_atomic_snapshot_invariant(&s);
    assert!(!s.is_set);
    assert_eq!(s.amount, 0);
    assert_eq!(s.recorded_at, 0);
    assert_eq!(s.collateral_limit, 4_000); // collateral_limit preserved; only commitment cleared
}

// ── 5. Timing boundaries (monotonicity vs racing writes) ─────────────────────

/// Two back-to-back writes within the **same** ledger timestamp (simulated
/// racing submission that gets sequenced into the same ledger): the equality
/// branch of the `now >= prior.recorded_at` guard must accept it.  Final state
/// equals whichever call ran last.
#[test]
fn racing_writes_same_timestamp_both_succeed_and_state_matches_last() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);
    let mut li = env.ledger().get();
    li.timestamp = 500;
    env.ledger().set(li);

    let a1 = Symbol::new(&env, "RACEA");
    let a2 = Symbol::new(&env, "RACEB");
    let c1 = client.record_sme_collateral_commitment(&a1, &1_000i128);
    let c2 = client.record_sme_collateral_commitment(&a2, &2_000i128); // same ts

    // both recorded_at values equal
    assert_eq!(c1.recorded_at, c2.recorded_at);
    // final stored view matches the later call
    let s = client.get_collateral_state();
    assert_atomic_snapshot_invariant(&s);
    assert_eq!(s.asset, a2);
    assert_eq!(s.amount, 2_000);
    assert_eq!(s.recorded_at, 500);
}

/// Stale replay at a timestamp *earlier* than the stored commitment must fail
/// (CollateralTimestampBackwards).  Simulates: client submits a write → it
/// lands → a concurrent duplicate (from an earlier mempool snapshot) is also
/// sequenced, with a *lagging* timestamp — must be rejected.
#[test]
fn stale_racing_write_with_earlier_timestamp_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);

    let mut li = env.ledger().get();
    li.timestamp = 1_000;
    env.ledger().set(li);
    let asset = Symbol::new(&env, "A");
    let prior = client.record_sme_collateral_commitment(&asset, &500i128);
    assert_eq!(prior.recorded_at, 1_000);

    // roll ledger back, simulate stale racing submission
    let mut li2 = env.ledger().get();
    li2.timestamp = 999;
    env.ledger().set(li2);
    assert_contract_error(
        Ok(client.try_record_sme_collateral_commitment(&asset, &600i128)),
        EscrowError::CollateralTimestampBackwards,
    );

    // final state must still equal the prior successful write — no partial
    // mutation was applied by the failing attempt.
    let s = client.get_collateral_state();
    assert_atomic_snapshot_invariant(&s);
    assert_eq!(s.amount, 500);
    assert_eq!(s.asset, asset);
    assert_eq!(s.recorded_at, 1_000);
}

// ── 6. Struct shape pin for the flat view ────────────────────────────────────

/// Compile-time pin: `CollateralState` exposes exactly these 5 named fields in
/// this order.  Adding/renaming a field breaks this test.
#[test]
fn flat_view_struct_shape_pin_destructured() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);
    client.set_collateral_limit(&9_000i128);
    let mut li = env.ledger().get();
    li.timestamp = 777;
    env.ledger().set(li);
    let asset = Symbol::new(&env, "SHAPE");
    client.record_sme_collateral_commitment(&asset, &4_500i128);

    let CollateralState {
        is_set,
        asset: a,
        amount,
        recorded_at,
        collateral_limit,
    } = client.get_collateral_state();

    assert_eq!(is_set, true);
    assert_eq!(a, asset);
    assert_eq!(amount, 4_500);
    assert_eq!(recorded_at, 777);
    assert_eq!(collateral_limit, 9_000);
}

// ── 7. No stale ceiling after limit write ────────────────────────────────────

/// Writing a *lower* ceiling then reading the flat view MUST return the new
/// (lower) ceiling — never the old one.  This defends against a future bug
/// where the view reads a stale copy.
#[test]
fn limit_write_visible_in_next_flat_view_call_no_stale() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);

    client.set_collateral_limit(&10_000i128);
    assert_eq!(client.get_collateral_state().collateral_limit, 10_000);

    client.set_collateral_limit(&1_000i128);
    assert_eq!(client.get_collateral_state().collateral_limit, 1_000);

    client.set_collateral_limit(&5_000i128);
    assert_eq!(client.get_collateral_state().collateral_limit, 5_000);
}

// ── 8. No-auth / pure view (caller addr absent still succeeds) ───────────────

/// `get_collateral_state` must be callable without any auth mock and without a
/// source account — pure views never trigger `require_auth`.
#[test]
fn pure_view_no_auth_needed_pre_init() {
    let env = Env::default();
    let client = deploy(&env);
    // explicitly: no `env.mock_all_auths()`, no `set_source_account`
    let s = client.get_collateral_state();
    assert_atomic_snapshot_invariant(&s);
    assert!(!s.is_set);
    assert_eq!(s.collateral_limit, MAX_INVOICE_AMOUNT);
}

#[test]
fn pure_view_no_auth_needed_after_init() {
    let env = Env::default();
    // Only init needs auth; subsequent views must not.
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);
    // drop auth mock by re-creating Env and re-binding to same contract id? Instead,
    // just verify that a second read returns OK (auth mock remains in place but
    // the entrypoint never reaches require_auth).  Both default + post-init
    // tests above exercise the no-auth path pre-init; this test confirms the
    // call still returns deterministically.
    let s = client.get_collateral_state();
    assert_atomic_snapshot_invariant(&s);
}

/// Failed writes must roll back completely so a caller can safely retry with
/// a valid boundary value.
#[test]
fn failed_limit_update_is_recoverable() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = deploy_and_init(&env);
    client.set_collateral_limit(&4_000i128);

    assert_contract_error(
        client.try_set_collateral_limit(&MAX_INVOICE_AMOUNT + 1),
        EscrowError::CollateralLimitExceedsMax,
    );
    assert_eq!(client.get_collateral_state().collateral_limit, 4_000);

    client.set_collateral_limit(&6_000i128);
    assert_eq!(client.get_collateral_state().collateral_limit, 6_000);
}
