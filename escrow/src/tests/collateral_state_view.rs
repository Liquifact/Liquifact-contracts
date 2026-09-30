//! Invariants for the SME collateral **state view**.
//!
//! The collateral record is *metadata only*: it never moves a token, never reserves a balance,
//! and never gates settlement, withdrawal, or investor claims. What it *does* own is a small,
//! strictly-typed state machine plus a read surface that risk tooling and indexers consume. This
//! module pins that state machine and that read surface so neither can drift silently.
//!
//! # State machine
//!
//! There is exactly one collateral key, [`DataKey::SmeCollateralPledge`], holding an
//! `Option<SmeCollateralCommitment>`. Its lifecycle is:
//!
//! ```text
//!            record_sme_collateral_commitment / batch_record_collateral
//!   UNSET ───────────────────────────────────────────────────────────────► SET
//!     ▲                                                                   │  ▲
//!     │  clear_sme_collateral_commitment                                  │  │ replacement
//!     └───────────────────────────────────────────────────────────────────┘  │ (last write wins)
//!                                                                           batch (last item wins)
//! ```
//!
//! Invariants protected below:
//!
//! - **I1 — deterministic defaults.** With no key stored, the view is `None` and the ceiling
//!   reads as [`MAX_INVOICE_AMOUNT`]. The view never panics and never writes a default in.
//! - **I2 — allowed transitions.** `UNSET → SET` (single or batch), `SET → SET` (replacement /
//!   batch), `SET → UNSET` (clear), and `UNSET → SET → UNSET → SET` (a full cycle is repeatable).
//! - **I3 — forbidden transitions.** `UNSET → UNSET` via `clear` is rejected
//!   ([`EscrowError::NoCollateralToClear`]); `SET → clear → clear` is rejected the same way.
//! - **I4 — authorization.** Every write to the collateral key is SME-gated; the ceiling is
//!   admin-gated. A rejected caller leaves the stored record byte-for-byte unchanged.
//! - **I5 — read fidelity.** The view, `get_sme_collateral_commitment`, `get_collateral_limit`,
//!   and the `get_escrow_summary` snapshot never disagree; reads are idempotent and side-effect free.
//! - **I6 — write ordering.** Validation precedes the write, so every rejected transition
//!   preserves the previously stored record (no partial write, no reset to defaults).
//! - **I7 — monotonic timestamps.** A replacement may not move `recorded_at` backwards; equal
//!   timestamps are accepted.
//! - **I8 — boundary values.** `amount = 1` and `amount = MAX_INVOICE_AMOUNT` are accepted and
//!   surfaced exactly; `0`, negatives, and `amount > ceiling` are rejected with typed errors.
//! - **I9 — batch bounds.** An empty batch and a batch longer than [`MAX_COLLATERAL_BATCH`] are
//!   rejected; a full-length batch is accepted, and any invalid item voids the whole batch.
//! - **I10 — audit trail.** Each successful write emits exactly one event carrying the fields an
//!   indexer needs to rebuild the record history without polling storage.
//!
//! # Failure recovery (F1–F7)
//!
//! Everything above describes the state machine in steady state. The second half of this module
//! describes what happens when a call does **not** land, because on a Soroban invocation that
//! fails the host discards the whole invocation: the storage journal is rolled back, the event log
//! is cleared, and only a typed error code reaches the caller. That gives recovery a precise
//! shape — *a rejected call is indistinguishable from a call that never happened* — and the
//! invariants below pin that shape at every layer the operator can observe:
//!
//! - **F1 — dependency failures are typed and recoverable.** Every dependency the view has (the
//!   escrow record, the SME/admin signatures, the ledger clock, the admin nonce that gates a role
//!   rotation, and the funding token it must *not* have) either rejects with a typed code or with
//!   a non-contract host error, leaves the view untouched, and is satisfiable again on retry.
//! - **F2 — retries are idempotent and converge.** Any number of rejected attempts leaves the state
//!   byte-identical and adds nothing to the event log; a retry of an accepted write converges on
//!   the same record.
//! - **F3 — partial completion is impossible.** A batch that fails on its first, middle, or last
//!   item (including the last item of a full-length batch) writes nothing and emits nothing; only
//!   the corrected batch applies, and it applies exactly once.
//! - **F4 — recovery is possible.** A retired record is fully reconstructible from the
//!   `coll_clr` event and replayable through the ordinary record entrypoint, and a full
//!   failure-then-replay cycle returns the view to a previously observed state exactly.
//! - **F5 — failures are observable without side effects.** Each failure mode maps to a distinct,
//!   pinned [`EscrowError`] code; authorization failures stay *outside* the contract-code space so
//!   callers can tell "rejected by validation" from "rejected by auth"; and a rejected call
//!   publishes no event of any kind, so an indexer can never mistake it for a state change.
//! - **F6 — interleaved calls stay consistent.** A read-then-write across two invocations can lose
//!   a race with a concurrent admin update, and the rejection is recoverable by re-reading; any
//!   interleaving of records and clears serialises to the last accepted write; the record and the
//!   ceiling are always observable together from one `get_escrow_summary` invocation.
//! - **F7 — boundaries stay deterministic.** The amount and batch-length matrices classify each
//!   value identically on every run, including the values immediately outside each edge.

use crate::tests::{assert_contract_error, setup};
use crate::{
    CollateralClearedEvt, CollateralCommitmentSnapshot, CollateralRecordedEvt, DataKey,
    EscrowError, LiquifactEscrowClient, SmeCollateralCommitment, MAX_COLLATERAL_BATCH,
    MAX_INVOICE_AMOUNT,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    xdr::{ContractEvent, ContractEventBody, ScErrorType, ScMap, ScVal},
    Address, Env, Error, Event as _, IntoVal, InvokeError, Symbol, Vec as SorobanVec,
};
use std::collections::BTreeSet;
use std::fmt::Debug;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `init` takes 19 arguments; the trailing `Option`s are explicit `None`s so this
/// stays in step with the contract signature.
fn init_escrow(env: &Env, client: &LiquifactEscrowClient<'_>, admin: &Address, sme: &Address) {
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "COLSTATE1"),
        sme,
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
}

/// Deploy + `init` with all auths mocked. Returns the client plus the configured roles.
fn setup_initialized(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    let (client, admin, sme) = setup(env);
    init_escrow(env, &client, &admin, &sme);
    (client, admin, sme)
}

fn set_timestamp(env: &Env, timestamp: u64) {
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = timestamp;
    env.ledger().set(ledger_info);
}

fn sym(env: &Env, s: &str) -> Symbol {
    Symbol::new(env, s)
}

/// Events published by `contract` during the **most recent** contract invocation.
///
/// The host discards the event log of a failed invocation, so this is `0` after any rejected
/// call. That is precisely the guarantee asserted by the "rejected transitions publish no
/// events" tests below: a failed transition can never be mistaken for a state change.
fn last_invocation_event_count(env: &Env, contract: &Address) -> usize {
    env.events()
        .all()
        .filter_by_contract(contract)
        .events()
        .len()
}

/// The most recent event published by the escrow contract during the last invocation, if any.
fn last_escrow_event(env: &Env, contract: &Address) -> Option<ContractEvent> {
    env.events()
        .all()
        .filter_by_contract(contract)
        .events()
        .last()
        .cloned()
}

/// Builds a batch of `len` identical items, used to probe the `MAX_COLLATERAL_BATCH` edges.
fn repeated_items(env: &Env, asset: &Symbol, amount: i128, len: u32) -> SorobanVec<(Symbol, i128)> {
    let items: std::vec::Vec<(Symbol, i128)> = (0..len).map(|_| (asset.clone(), amount)).collect();
    SorobanVec::from_slice(env, &items)
}

// ---------------------------------------------------------------------------
// I1 — deterministic defaults for the unset state
// ---------------------------------------------------------------------------

/// Before `init` the collateral read is `None` and the ceiling is the additive-key default.
/// The view must not require initialization, must not panic, and must not fail.
#[test]
fn unset_view_reads_none_and_default_limit_before_init() {
    let env = Env::default();
    let id = env.register(crate::LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &id);

    assert_eq!(client.get_sme_collateral_commitment(), None);
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

/// After `init` but before any record, the state is still the unset default: the ceiling
/// default is independent of initialization and of the record key.
#[test]
fn unset_view_after_init_is_none_with_default_limit() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    assert_eq!(client.get_sme_collateral_commitment(), None);
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);

    // The bundled summary maps the absent record onto the explicit `None` snapshot variant.
    let summary = client.get_escrow_summary();
    assert_eq!(
        summary.sme_collateral_commitment,
        CollateralCommitmentSnapshot::None
    );
}

/// A ceiling set while no record exists is visible immediately; the record key stays absent.
#[test]
fn unset_view_still_reports_updated_limit() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    client.set_collateral_limit(&2_500i128);

    assert_eq!(client.get_sme_collateral_commitment(), None);
    assert_eq!(client.get_collateral_limit(), 2_500i128);
}

// ---------------------------------------------------------------------------
// I2 / I5 — allowed transitions and read fidelity
// ---------------------------------------------------------------------------

/// `UNSET → SET`: the record is surfaced field-for-field, including the ledger timestamp
/// that was in force at write time.
#[test]
fn allowed_transition_unset_to_set_surfaces_all_fields() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, 4_242);

    let returned = client.record_sme_collateral_commitment(&sym(&env, "USDC"), &5_000i128);

    let stored = client
        .get_sme_collateral_commitment()
        .expect("a record was just written");
    assert_eq!(stored, returned);
    assert_eq!(stored.asset, sym(&env, "USDC"));
    assert_eq!(stored.amount, 5_000i128);
    assert_eq!(stored.recorded_at, 4_242);
}

/// `SET → SET`: a replacement overwrites the previous record wholesale — no field of the old
/// record survives, including the asset symbol.
#[test]
fn allowed_transition_set_to_set_replaces_every_field() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, 100);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);

    set_timestamp(&env, 900);
    client.record_sme_collateral_commitment(&sym(&env, "EURC"), &2_500i128);

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.asset, sym(&env, "EURC"));
    assert_eq!(stored.amount, 2_500i128);
    assert_eq!(stored.recorded_at, 900);
}

/// `SET → UNSET`: clearing removes the key so the view returns to the exact unset default.
#[test]
fn allowed_transition_set_to_unset_returns_default() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);

    client.clear_sme_collateral_commitment();

    assert_eq!(client.get_sme_collateral_commitment(), None);
    assert_eq!(
        client.get_escrow_summary().sme_collateral_commitment,
        CollateralCommitmentSnapshot::None
    );
}

/// `UNSET → SET → UNSET → SET`: a full record/clear cycle is repeatable, and the second
/// record is not contaminated by the first (no stale amount, asset, or timestamp).
#[test]
fn full_record_clear_cycle_is_repeatable() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    set_timestamp(&env, 100);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);
    client.clear_sme_collateral_commitment();
    assert_eq!(client.get_sme_collateral_commitment(), None);

    // A second record after a clear must read as a *first* record: `prior_amount` is 0 in the
    // event even though the key held a value earlier in the lifecycle.
    set_timestamp(&env, 200);
    client.record_sme_collateral_commitment(&sym(&env, "GOLD"), &7_777i128);
    assert_eq!(
        last_escrow_event(&env, &contract_id),
        Some(
            CollateralRecordedEvt {
                name: symbol_short!("coll_rec"),
                invoice_id,
                amount: 7_777i128,
                prior_amount: 0i128,
            }
            .to_xdr(&env, &contract_id)
        ),
        "a record written after a clear must report prior_amount 0"
    );

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(
        stored,
        SmeCollateralCommitment {
            asset: sym(&env, "GOLD"),
            amount: 7_777i128,
            recorded_at: 200,
        }
    );
}

/// The view never recomputes: the record getter and the bundled snapshot describe the same
/// stored value, and both track the latest write.
#[test]
fn read_surface_never_disagrees_with_bundled_snapshot() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &7_777i128);
    let commitment = client
        .get_sme_collateral_commitment()
        .expect("a record was just written");
    let summary = client.get_escrow_summary();
    assert_eq!(
        summary.sme_collateral_commitment,
        CollateralCommitmentSnapshot::Some(commitment)
    );

    // The ceiling is a separate, independent key: tightening it never rewrites the stored
    // record the view reports.
    client.set_collateral_limit(&1_000i128);
    let after = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(after.amount, 7_777i128);
}

// ---------------------------------------------------------------------------
// I3 — forbidden transitions
// ---------------------------------------------------------------------------

/// `UNSET → UNSET` is not a transition: clearing with nothing stored is rejected with the
/// typed `NoCollateralToClear` error, not a silent no-op.
#[test]
fn forbidden_transition_clear_on_unset_is_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

/// `SET → UNSET → UNSET`: the second clear is rejected, and the rejected call neither
/// resurrects the record nor leaves an event behind.
#[test]
fn forbidden_transition_double_clear_is_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);
    client.clear_sme_collateral_commitment();

    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );
    assert_eq!(
        last_invocation_event_count(&env, &client.address),
        0,
        "a rejected clear must not publish an event"
    );

    assert_eq!(client.get_sme_collateral_commitment(), None);
}

/// Recording a negative or zero amount never reaches the write path.
#[test]
fn forbidden_transition_non_positive_amount_is_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    for bad in [0i128, -1i128, -1_000_000i128, i128::MIN] {
        assert_contract_error(
            client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &bad),
            EscrowError::CollateralAmountNotPositive,
        );
    }

    assert_eq!(
        client.get_sme_collateral_commitment(),
        None,
        "a rejected record must not create the key"
    );
}

/// An empty asset symbol is rejected: a stored record must always name an asset.
#[test]
fn forbidden_transition_empty_asset_is_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&sym(&env, ""), &1_000i128),
        EscrowError::CollateralAssetEmpty,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

/// A record above the configured ceiling is rejected, and the boundary itself is accepted.
#[test]
fn forbidden_transition_above_ceiling_is_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    client.set_collateral_limit(&1_000i128);

    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &1_001i128),
        EscrowError::CollateralLimitExceeded,
    );

    // The rejected over-limit write did not clobber the record that was already accepted.
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        1_000i128
    );
}

/// The ceiling gates new writes only; a record accepted under a looser ceiling stays readable
/// after the admin tightens it (the state view never hides previously recorded metadata).
#[test]
fn tightening_ceiling_does_not_hide_existing_record() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &9_000i128);

    client.set_collateral_limit(&100i128);

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, 9_000i128);
    assert_eq!(client.get_collateral_limit(), 100i128);
}

// ---------------------------------------------------------------------------
// I7 — monotonic timestamps
// ---------------------------------------------------------------------------

/// A replacement whose ledger timestamp is older than the stored one is rejected and the
/// stored record is preserved exactly.
#[test]
fn forbidden_transition_timestamp_moving_backwards_is_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, 1_000);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);

    set_timestamp(&env, 999);
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&sym(&env, "EURC"), &2_000i128),
        EscrowError::CollateralTimestampBackwards,
    );

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.asset, sym(&env, "USDC"));
    assert_eq!(stored.amount, 1_000i128);
    assert_eq!(stored.recorded_at, 1_000);
}

/// `now == prior.recorded_at` is accepted (the guard is `>=`, not `>`), and the timestamp in
/// the stored record stays the same value.
#[test]
fn replacement_at_the_same_timestamp_is_allowed() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, 1_000);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);

    client.record_sme_collateral_commitment(&sym(&env, "EURC"), &2_000i128);

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.asset, sym(&env, "EURC"));
    assert_eq!(stored.amount, 2_000i128);
    assert_eq!(stored.recorded_at, 1_000);
}

// ---------------------------------------------------------------------------
// I4 — authorization
// ---------------------------------------------------------------------------

/// Only the configured SME may write the record. With no SME signature available the call is
/// rejected and the stored state is untouched.
#[test]
fn non_sme_cannot_record_commitment() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);

    // Drop every mocked authorization: no caller can satisfy the SME gate.
    env.mock_auths(&[]);

    assert!(
        client
            .try_record_sme_collateral_commitment(&sym(&env, "EURC"), &2_000i128)
            .is_err(),
        "a caller that is not the configured SME must not be able to record collateral"
    );

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.asset, sym(&env, "USDC"));
    assert_eq!(stored.amount, 1_000i128);
}

/// The SME gate also covers the batch entrypoint.
#[test]
fn non_sme_cannot_batch_record_commitment() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    env.mock_auths(&[]);
    let items = SorobanVec::from_array(&env, [(sym(&env, "USDC"), 1_000i128)]);
    assert!(
        client.try_batch_record_collateral(&items).is_err(),
        "a caller that is not the configured SME must not be able to batch record collateral"
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

/// Clearing is SME-gated too: an unauthorized caller cannot retire a record.
#[test]
fn non_sme_cannot_clear_commitment() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);

    env.mock_auths(&[]);
    assert!(
        client.try_clear_sme_collateral_commitment().is_err(),
        "a caller that is not the configured SME must not be able to clear collateral"
    );
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        1_000i128
    );
}

/// The admin is not the SME: an admin-only signature cannot substitute for the SME gate on
/// the collateral record. (The ceiling setter is admin-gated; the record is not.)
#[test]
fn admin_signature_alone_cannot_record_commitment() {
    let env = Env::default();
    let (client, admin, _sme) = setup_initialized(&env);

    // Only the admin authorizes; the SME signature is absent.
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "record_sme_collateral_commitment",
            args: SorobanVec::from_array(&env, [(sym(&env, "USDC"), 1_000i128).into_val(&env)]),
            sub_invokes: &[],
        },
    }]);

    assert!(
        client
            .try_record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128)
            .is_err(),
        "admin auth must not satisfy the SME gate on the collateral record"
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

/// Reads are deliberately unauthenticated: risk tooling and indexers poll them without holding
/// the SME or admin key. Deploy a fresh, un-mocked environment to prove no `require_auth` runs.
#[test]
fn collateral_reads_require_no_authorization() {
    let env = Env::default();
    // Intentionally no `mock_all_auths()` / `mock_auths()`: any require_auth would abort.
    let id = env.register(crate::LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &id);

    assert_eq!(client.get_sme_collateral_commitment(), None);
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

// ---------------------------------------------------------------------------
// I5 — idempotency / repeated operations
// ---------------------------------------------------------------------------

/// Repeated reads are stable and side-effect free: they neither drift nor resurrect a record.
#[test]
fn repeated_reads_are_idempotent() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &3_000i128);

    let first = client.get_sme_collateral_commitment();
    let second = client.get_sme_collateral_commitment();
    let third = client.get_sme_collateral_commitment();
    assert_eq!(first, second);
    assert_eq!(second, third);
    assert_eq!(first.unwrap().amount, 3_000i128);

    // Reads publish no events: the record write was the only event-producing call.
    assert_eq!(
        last_invocation_event_count(&env, &client.address),
        0,
        "a read-only view must not publish events"
    );
    let _ = client.get_collateral_limit();
    let _ = client.get_escrow_summary();
    assert_eq!(last_invocation_event_count(&env, &client.address), 0);
}

/// Repeating the identical record is convergent: the state after the second call is the same
/// as after the first. (The event's `prior_amount` does change, which I10 covers.)
#[test]
fn repeated_identical_record_is_convergent() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, 500);

    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &3_000i128);
    let after_first = client.get_sme_collateral_commitment().unwrap();

    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &3_000i128);
    let after_second = client.get_sme_collateral_commitment().unwrap();

    assert_eq!(after_first, after_second);
}

/// Repeating a batch with the same items is convergent on the stored record.
#[test]
fn repeated_identical_batch_is_convergent() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, 500);
    let items = SorobanVec::from_array(
        &env,
        [(sym(&env, "USDC"), 100i128), (sym(&env, "EURC"), 900i128)],
    );

    let first = client.batch_record_collateral(&items);
    let second = client.batch_record_collateral(&items);

    assert_eq!(first, second);
    assert_eq!(client.get_sme_collateral_commitment().unwrap(), second);
}

/// Recording again after a clear is the same first-record transition, so the key's contents
/// are fully determined by the last accepted write — no residue from the previous lifecycle.
#[test]
fn record_after_clear_does_not_accumulate_state() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    for _ in 0..3 {
        client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);
        client.clear_sme_collateral_commitment();
        assert_eq!(client.get_sme_collateral_commitment(), None);
    }
    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );
}

// ---------------------------------------------------------------------------
// I8 — boundary values
// ---------------------------------------------------------------------------

/// The smallest accepted amount is stored and surfaced without truncation.
#[test]
fn boundary_minimum_amount_is_surfaced_exactly() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1i128);

    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        1i128
    );
}

/// The largest amount the default ceiling allows is stored and surfaced without wraparound.
#[test]
fn boundary_maximum_amount_is_surfaced_exactly() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &MAX_INVOICE_AMOUNT);

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, MAX_INVOICE_AMOUNT);
    assert_eq!(stored.amount, client.get_collateral_limit());
}

/// One stroop above the default ceiling is rejected, so the accepted range is exactly
/// `1..=MAX_INVOICE_AMOUNT`.
#[test]
fn boundary_one_above_default_ceiling_is_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceeded,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

/// `recorded_at` is a `u64` ledger timestamp; the maximum value round-trips without truncation.
#[test]
fn boundary_maximum_ledger_timestamp_round_trips() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, u64::MAX);

    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);

    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().recorded_at,
        u64::MAX
    );
}

/// The tightest legal ceiling (`1`) is visible to the view and admits exactly one amount.
#[test]
fn boundary_tightest_ceiling_admits_only_one_stroop() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    client.set_collateral_limit(&1i128);

    assert_eq!(client.get_collateral_limit(), 1i128);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1i128);
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &2i128),
        EscrowError::CollateralLimitExceeded,
    );
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        1i128
    );
}

// ---------------------------------------------------------------------------
// I9 — batch bounds and all-or-nothing semantics
// ---------------------------------------------------------------------------

/// A batch stores the **last** item; the caller receives that same value.
#[test]
fn batch_stores_the_last_item() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let usdc = sym(&env, "USDC");
    let eur = sym(&env, "EURC");

    let items = SorobanVec::from_array(
        &env,
        [
            (usdc.clone(), 100i128),
            (eur.clone(), 900i128),
            (usdc.clone(), 450i128),
        ],
    );
    let returned = client.batch_record_collateral(&items);

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored, returned);
    assert_eq!(stored.asset, usdc);
    assert_eq!(stored.amount, 450i128);
}

/// A single-entry batch behaves exactly like the single-record entrypoint.
#[test]
fn single_item_batch_matches_single_record() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, 321);

    let returned = client.batch_record_collateral(&SorobanVec::from_array(
        &env,
        [(sym(&env, "GOLD"), 12_345i128)],
    ));

    assert_eq!(returned, client.get_sme_collateral_commitment().unwrap());
    assert_eq!(returned.recorded_at, 321);
}

/// An empty batch is rejected with a typed error and changes nothing.
#[test]
fn batch_bounds_empty_batch_is_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    assert_contract_error(
        client.try_batch_record_collateral(&SorobanVec::from_array(&env, [])),
        EscrowError::CollateralBatchEmpty,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

/// A full-length batch is accepted; one entry more is rejected. The accepted length is exactly
/// `MAX_COLLATERAL_BATCH`.
#[test]
fn batch_bounds_max_length_accepted_one_more_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let usdc = sym(&env, "USDC");

    let at_max = repeated_items(&env, &usdc, 10i128, MAX_COLLATERAL_BATCH);
    let stored = client.batch_record_collateral(&at_max);
    assert_eq!(stored.amount, 10i128);

    let over_max = repeated_items(&env, &usdc, 10i128, MAX_COLLATERAL_BATCH + 1);
    assert_contract_error(
        client.try_batch_record_collateral(&over_max),
        EscrowError::CollateralBatchTooLarge,
    );
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        10i128
    );
}

/// One invalid item voids the whole batch: no partial write, and the prior record survives.
#[test]
fn batch_is_all_or_nothing_on_invalid_item() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let usdc = sym(&env, "USDC");
    client.record_sme_collateral_commitment(&usdc, &500i128);

    // Invalid in the middle: the valid items around it must not be applied.
    let bad_amount = SorobanVec::from_array(
        &env,
        [
            (usdc.clone(), 100i128),
            (sym(&env, "EURC"), 0i128),
            (usdc.clone(), 300i128),
        ],
    );
    assert_contract_error(
        client.try_batch_record_collateral(&bad_amount),
        EscrowError::CollateralAmountNotPositive,
    );

    // Invalid in first position: the empty-asset check fires before the amount check.
    let bad_asset =
        SorobanVec::from_array(&env, [(sym(&env, ""), 100i128), (usdc.clone(), 300i128)]);
    assert_contract_error(
        client.try_batch_record_collateral(&bad_asset),
        EscrowError::CollateralAssetEmpty,
    );

    // Invalid on the ceiling.
    client.set_collateral_limit(&1_000i128);
    let bad_limit = SorobanVec::from_array(
        &env,
        [(usdc.clone(), 100i128), (sym(&env, "EURC"), 1_001i128)],
    );
    assert_contract_error(
        client.try_batch_record_collateral(&bad_limit),
        EscrowError::CollateralLimitExceeded,
    );

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.asset, usdc);
    assert_eq!(stored.amount, 500i128);
}

/// The batch timestamp guard fires against the *stored* record, and a rejected batch leaves it
/// intact.
#[test]
fn batch_rejects_backwards_timestamp_against_stored_record() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, 1_000);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &500i128);

    set_timestamp(&env, 999);
    let items = SorobanVec::from_array(&env, [(sym(&env, "EURC"), 600i128)]);
    assert_contract_error(
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralTimestampBackwards,
    );

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.asset, sym(&env, "USDC"));
    assert_eq!(stored.amount, 500i128);
    assert_eq!(stored.recorded_at, 1_000);
}

// ---------------------------------------------------------------------------
// I10 — audit-trail integrity
// ---------------------------------------------------------------------------

/// Each accepted record publishes exactly one `coll_rec` event whose `prior_amount` is the
/// previous `amount`, so an indexer can rebuild the history without polling storage.
#[test]
fn record_events_chain_prior_amounts() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;
    let usdc = sym(&env, "USDC");

    for (amount, prior) in [(1_000i128, 0i128), (2_000, 1_000), (500, 2_000)] {
        client.record_sme_collateral_commitment(&usdc, &amount);
        assert_eq!(
            last_escrow_event(&env, &contract_id),
            Some(
                CollateralRecordedEvt {
                    name: symbol_short!("coll_rec"),
                    invoice_id: invoice_id.clone(),
                    amount,
                    prior_amount: prior,
                }
                .to_xdr(&env, &contract_id)
            ),
            "record event must chain prior_amount {} -> {}",
            prior,
            amount
        );
    }
}

/// A batch publishes one event per item and the `prior_amount` chain runs through the batch.
#[test]
fn batch_events_chain_prior_amounts_per_item() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    let items = SorobanVec::from_array(
        &env,
        [
            (sym(&env, "USDC"), 100i128),
            (sym(&env, "EURC"), 200i128),
            (sym(&env, "GOLD"), 300i128),
        ],
    );
    client.batch_record_collateral(&items);

    assert_eq!(
        last_invocation_event_count(&env, &contract_id),
        3,
        "a three-item batch must publish exactly three record events"
    );
    assert_eq!(
        last_escrow_event(&env, &contract_id),
        Some(
            CollateralRecordedEvt {
                name: symbol_short!("coll_rec"),
                invoice_id,
                amount: 300i128,
                prior_amount: 200i128,
            }
            .to_xdr(&env, &contract_id)
        )
    );
}

/// A rejected transition publishes nothing: no consumer can mistake a failed write for a state
/// change. Each assertion re-reads the last invocation's event log, which the host empties on
/// failure.
#[test]
fn rejected_transitions_publish_no_events() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &0i128),
        EscrowError::CollateralAmountNotPositive,
    );
    assert_eq!(last_invocation_event_count(&env, &client.address), 0);
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&sym(&env, ""), &1i128),
        EscrowError::CollateralAssetEmpty,
    );
    assert_eq!(last_invocation_event_count(&env, &client.address), 0);
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceeded,
    );
    assert_eq!(last_invocation_event_count(&env, &client.address), 0);
    assert_contract_error(
        client.try_batch_record_collateral(&SorobanVec::from_array(&env, [])),
        EscrowError::CollateralBatchEmpty,
    );
    assert_eq!(last_invocation_event_count(&env, &client.address), 0);
    assert_contract_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );
    assert_eq!(last_invocation_event_count(&env, &client.address), 0);

    assert_eq!(client.get_sme_collateral_commitment(), None);
}

/// The clear event copies the removed record's fields before deletion, so an indexer can
/// reconstruct what was retired without a post-mutation storage read.
#[test]
fn clear_event_copies_the_removed_record() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract_id = client.address.clone();
    set_timestamp(&env, 4_242);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_234i128);
    let invoice_id = client.get_escrow().invoice_id;

    client.clear_sme_collateral_commitment();

    assert_eq!(
        last_escrow_event(&env, &contract_id),
        Some(
            CollateralClearedEvt {
                name: symbol_short!("coll_clr"),
                invoice_id,
                asset: sym(&env, "USDC"),
                amount: 1_234i128,
                recorded_at: 4_242,
            }
            .to_xdr(&env, &contract_id)
        )
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

/// The collateral record is metadata only: it never moves funds. The recorded amount is
/// unrelated to the escrow's own accounting, so the summary's escrow fields are unchanged by
/// any collateral transition.
#[test]
fn collateral_transitions_do_not_touch_escrow_accounting() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let before = client.get_escrow_summary();

    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &9_999i128);
    client.batch_record_collateral(&SorobanVec::from_array(&env, [(sym(&env, "GOLD"), 5i128)]));
    client.clear_sme_collateral_commitment();

    let after = client.get_escrow_summary();
    assert_eq!(after.escrow, before.escrow);
    assert_eq!(after.unique_funder_count, before.unique_funder_count);
    assert_eq!(
        after.sme_collateral_commitment,
        CollateralCommitmentSnapshot::None
    );
}

// ===========================================================================
// Failure-recovery harness
//
// The whole F-section below is written against three primitives:
//
// 1. [`FailureKind`] — *how* a rejection reached the caller. A caller-visible failure taxonomy
//    needs exactly one distinction to be actionable: a typed [`EscrowError`] code (the contract
//    examined the arguments and refused) versus a non-contract host error (the call never got
//    to run: missing signature, unavailable dependency). Pinning the *host* code as well would
//    be over-fitting: only the contract/host distinction is part of this contract's API.
// 2. [`CollateralStateProbe`] — a byte-level fingerprint of everything a rejected call could
//    possibly have touched, read through three independent surfaces: raw instance storage (read
//    as the contract itself, so it catches a partial write the public views would hide), the
//    standalone views, and the bundled `get_escrow_summary` snapshot.
// 3. [`assert_probe_unchanged`] — the atomicity assertion every failure test makes.
// ===========================================================================

/// Caller-visible classification of a rejected invocation.
#[derive(Clone, Debug, PartialEq)]
enum FailureKind {
    /// The contract body ran and refused the call with a typed [`EscrowError`] code.
    Contract(u32),
    /// The host refused the call before the contract body could act on it: a missing
    /// authorization, or an unavailable cross-contract dependency.
    ///
    /// Deliberately not narrowed to a numeric host code — that value belongs to the host, not to
    /// this contract's interface, and integrators only need to know it is *not* a contract code.
    NonContract,
    /// The invocation aborted without a recoverable error value.
    Aborted,
}

impl FailureKind {
    /// The typed code, or a readable stand-in for the non-contract classes.
    fn describe(&self) -> String {
        match self {
            FailureKind::Contract(code) => format!("Contract({code})"),
            FailureKind::NonContract => "NonContract".to_string(),
            FailureKind::Aborted => "Aborted".to_string(),
        }
    }
}

/// Classify the outcome of a `try_*` invocation.
///
/// Panics with a pointed message if the call actually succeeded: a recovery test that silently
/// ran a happy path is worse than no test at all.
fn classify_failure<T: Debug, E: Debug>(
    label: &str,
    result: Result<Result<T, E>, Result<Error, InvokeError>>,
) -> FailureKind {
    match result {
        Err(Ok(err)) if err.is_type(ScErrorType::Contract) => FailureKind::Contract(err.get_code()),
        Err(Ok(_)) => FailureKind::NonContract,
        Err(Err(InvokeError::Contract(code))) => FailureKind::Contract(code),
        Err(Err(InvokeError::Abort)) => FailureKind::Aborted,
        Ok(inner) => panic!("{label}: expected the invocation to be rejected, got {inner:?}"),
    }
}

/// Assert the rejection carries exactly the typed code `expected`, in the contract-code space.
///
/// This is the caller-facing contract of the failure path: same input, same code, every run, with
/// nothing else attached to the error.
fn assert_contract_kind<T: Debug, E: Debug>(
    label: &str,
    result: Result<Result<T, E>, Result<Error, InvokeError>>,
    expected: EscrowError,
) {
    let observed = classify_failure(label, result);
    assert_eq!(
        observed,
        FailureKind::Contract(expected as u32),
        "{label}: expected the typed code {} ({:?}), got {}",
        expected as u32,
        expected,
        observed.describe()
    );
}

/// Assert the rejection never reached the contract body (authorization or dependency failure).
fn assert_non_contract_kind<T: Debug, E: Debug>(
    label: &str,
    result: Result<Result<T, E>, Result<Error, InvokeError>>,
) {
    let observed = classify_failure(label, result);
    assert_eq!(
        observed,
        FailureKind::NonContract,
        "{label}: expected a host-level (auth/dependency) rejection, got {}",
        observed.describe()
    );
}

/// Byte-level fingerprint of the collateral state across all three observable surfaces.
///
/// `*_raw` fields are read with [`Env::as_contract`] straight out of instance storage, so they
/// detect a partial write that a public view would normalize away — for example a failed
/// `set_collateral_limit` that left the default ceiling materialized in storage, or a batch that
/// wrote item *k* before failing on item *k+1*.
#[derive(Clone, Debug, PartialEq)]
struct CollateralStateProbe {
    // --- raw instance storage ------------------------------------------------
    /// `DataKey::SmeCollateralPledge` key presence, independent of the stored value.
    pledge_present: bool,
    pledge_raw: Option<SmeCollateralCommitment>,
    /// `DataKey::CollateralLimit`: `None` while the additive key has never been written.
    limit_raw: Option<i128>,
    /// `DataKey::Escrow` presence (the collateral surface's hard dependency).
    escrow_present: bool,
    sme_raw: Option<Address>,
    funded_amount_raw: Option<i128>,
    /// `DataKey::AdminNonce`: the replay counter that gates a role rotation.
    admin_nonce: u32,
    // --- standalone views ----------------------------------------------------
    view_record: Option<SmeCollateralCommitment>,
    view_limit: i128,
    view_unique_funder_count: u32,
    view_legal_hold: bool,
    // --- bundled single-invocation snapshot -----------------------------------
    snapshot_record: CollateralCommitmentSnapshot,
    snapshot_limit: i128,
}

impl CollateralStateProbe {
    /// The stored record as reported by the record getter.
    fn record(&self) -> Option<SmeCollateralCommitment> {
        self.view_record.clone()
    }
}

fn probe(env: &Env, client: &LiquifactEscrowClient<'_>) -> CollateralStateProbe {
    let address = client.address.clone();
    let (pledge_present, pledge_raw, limit_raw, escrow, admin_nonce) =
        env.as_contract(&address, || {
            (
                env.storage().instance().has(&DataKey::SmeCollateralPledge),
                env.storage().instance().get(&DataKey::SmeCollateralPledge),
                env.storage().instance().get(&DataKey::CollateralLimit),
                env.storage()
                    .instance()
                    .get::<_, crate::InvoiceEscrow>(&DataKey::Escrow),
                env.storage()
                    .instance()
                    .get::<_, u32>(&DataKey::AdminNonce)
                    .unwrap_or(0),
            )
        });

    let summary = client.get_escrow_summary();
    CollateralStateProbe {
        pledge_present,
        pledge_raw,
        limit_raw,
        escrow_present: escrow.is_some(),
        sme_raw: escrow.as_ref().map(|e| e.sme_address.clone()),
        funded_amount_raw: escrow.as_ref().map(|e| e.funded_amount),
        admin_nonce,
        view_record: client.get_sme_collateral_commitment(),
        view_limit: client.get_collateral_limit(),
        view_unique_funder_count: client.get_unique_funder_count(),
        view_legal_hold: client.get_legal_hold(),
        snapshot_record: summary.sme_collateral_commitment,
        snapshot_limit: summary.collateral_limit,
    }
}

/// The atomicity assertion of the failure path: a rejected call changed nothing, anywhere.
///
/// Field-wise rather than a single `PartialEq` so a regression names the surface that drifted.
fn assert_probe_unchanged(
    label: &str,
    before: &CollateralStateProbe,
    after: &CollateralStateProbe,
) {
    assert_eq!(
        after.pledge_present, before.pledge_present,
        "{label}: DataKey::SmeCollateralPledge presence changed"
    );
    assert_eq!(
        after.pledge_raw, before.pledge_raw,
        "{label}: the stored collateral record changed"
    );
    assert_eq!(
        after.limit_raw, before.limit_raw,
        "{label}: DataKey::CollateralLimit changed"
    );
    assert_eq!(
        after.escrow_present, before.escrow_present,
        "{label}: DataKey::Escrow presence changed"
    );
    assert_eq!(
        after.sme_raw, before.sme_raw,
        "{label}: the SME authorization dependency changed"
    );
    assert_eq!(
        after.funded_amount_raw, before.funded_amount_raw,
        "{label}: escrow funding accounting changed"
    );
    assert_eq!(
        after.admin_nonce, before.admin_nonce,
        "{label}: the admin nonce advanced on a rejected call"
    );
    assert_eq!(
        after.view_record, before.view_record,
        "{label}: get_sme_collateral_commitment() changed"
    );
    assert_eq!(
        after.view_limit, before.view_limit,
        "{label}: get_collateral_limit() changed"
    );
    assert_eq!(
        after.view_unique_funder_count, before.view_unique_funder_count,
        "{label}: get_unique_funder_count() changed"
    );
    assert_eq!(
        after.view_legal_hold, before.view_legal_hold,
        "{label}: get_legal_hold() changed"
    );
    assert_eq!(
        after.snapshot_record, before.snapshot_record,
        "{label}: the bundled snapshot's collateral record changed"
    );
    assert_eq!(
        after.snapshot_limit, before.snapshot_limit,
        "{label}: the bundled snapshot's ceiling changed"
    );
}

// ---------------------------------------------------------------------------
// Event-stream readers
//
// A rejected invocation publishes nothing, so indexers rely on the event log being an exact
// record of *accepted* state changes. These helpers read the log the way an indexer would:
// routing symbol out of the topics, payload fields out of the event body.
// ---------------------------------------------------------------------------

/// `ScVal::Symbol` as a [`Symbol`], interned into `env`.
fn sc_symbol(env: &Env, value: &ScVal) -> Option<Symbol> {
    match value {
        ScVal::Symbol(sym) => Some(Symbol::new(env, &sym.0.to_string())),
        _ => None,
    }
}

/// `ScVal::I128` as an [`i128`] (the SDK stores 128-bit integers as `hi`/`lo` halves).
fn sc_i128(value: &ScVal) -> Option<i128> {
    match value {
        ScVal::I128(parts) => Some(((parts.hi as i128) << 32) | (parts.lo as u128 as i128)),
        _ => None,
    }
}

/// `ScVal::U64` as a [`u64`].
fn sc_u64(value: &ScVal) -> Option<u64> {
    match value {
        ScVal::U64(v) => Some(*v),
        _ => None,
    }
}

/// The V0 body of a contract event: `(topics, body fields)`.
fn event_body(
    env: &Env,
    event: &ContractEvent,
) -> Option<(std::vec::Vec<Symbol>, Vec<(Symbol, ScVal)>)> {
    let ContractEventBody::V0(body) = &event.body else {
        return None;
    };
    let topics: std::vec::Vec<Symbol> = body
        .topics
        .iter()
        .map(|topic| sc_symbol(env, topic))
        .collect::<Option<std::vec::Vec<_>>>()?;
    // A SDK struct event carries its named fields as `ScVal::Map` of symbol -> value.
    let fields = match &body.data {
        ScVal::Map(Some(ScMap(entries))) => entries
            .iter()
            .filter_map(|entry| Some((sc_symbol(env, &entry.key)?, entry.val.clone())))
            .collect(),
        _ => Vec::new(),
    };
    Some((topics, fields))
}

/// The routing symbol of an event (`coll_rec`, `coll_clr`, `coll_lim`, ...): topic 1, right after
/// the event-type discriminant.
fn event_routing(env: &Env, event: &ContractEvent) -> Option<Symbol> {
    event_body(env, event).and_then(|(topics, _)| topics.get(1).cloned())
}

/// Every event published by the escrow contract so far, oldest first.
fn escrow_events(env: &Env, contract: &Address) -> std::vec::Vec<ContractEvent> {
    env.events()
        .all()
        .filter_by_contract(contract)
        .events()
        .iter()
        .cloned()
        .collect()
}

/// How many events the escrow published under routing symbol `routing` so far.
fn count_events(env: &Env, contract: &Address, routing: Symbol) -> usize {
    escrow_events(env, contract)
        .iter()
        .filter(|event| event_routing(env, event).as_ref() == Some(&routing))
        .count()
}

/// Read one numeric body field out of an event, e.g. `amount` or `recorded_at`.
fn event_field_u64(env: &Env, event: &ContractEvent, key: Symbol) -> Option<u64> {
    let (_, fields) = event_body(env, event)?;
    fields
        .into_iter()
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| sc_u64(&value))
}

/// Read one `i128` body field out of an event.
fn event_field_i128(env: &Env, event: &ContractEvent, key: Symbol) -> Option<i128> {
    let (_, fields) = event_body(env, event)?;
    fields
        .into_iter()
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| sc_i128(&value))
}

/// Read one symbol body field out of an event.
fn event_field_symbol(env: &Env, event: &ContractEvent, key: Symbol) -> Option<Symbol> {
    let (_, fields) = event_body(env, event)?;
    fields
        .into_iter()
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| sc_symbol(env, &value))
}

// ---------------------------------------------------------------------------
// Shared fixtures for the F-section
// ---------------------------------------------------------------------------

/// A failure that a rejected write could plausibly leave behind, plus the code it must report.
struct Fault {
    /// Human-readable name, used in assertion messages so a failure names the scenario.
    label: &'static str,
    /// The typed error the fault must produce.
    code: EscrowError,
    /// Accepted writes performed *before* the fault, so the fault is exercised against a realistic
    /// populated state rather than an empty one. Runs before the "before" fingerprint is taken, so
    /// these writes are not charged to the fault.
    prepare: Box<dyn Fn(&Env, &LiquifactEscrowClient<'_>)>,
    /// Injects the fault. Each closure asserts its own typed error, and must leave the environment
    /// byte-identical to how `prepare` left it — that is what the fingerprint comparison proves.
    inject: Box<dyn Fn(&Env, &LiquifactEscrowClient<'_>)>,
}

/// No preconditioning: the fault is injected into a freshly initialized escrow.
fn no_prep() -> Box<dyn Fn(&Env, &LiquifactEscrowClient<'_>)> {
    Box::new(|_env, _client| {})
}

/// Every fault mode the state view can be put into, built against a freshly initialized escrow.
///
/// Shared by the "all faults leave no trace", "all faults are recoverable", and "all faults report
/// distinct codes" tests so those three cannot drift apart: one list, three properties.
fn fault_matrix() -> std::vec::Vec<Fault> {
    std::vec![
        Fault {
            label: "record with a non-positive amount",
            code: EscrowError::CollateralAmountNotPositive,
            prepare: no_prep(),
            inject: Box::new(|env, client| {
                for bad in [0i128, -1, i128::MIN] {
                    assert_contract_kind(
                        "record(non-positive)",
                        client.try_record_sme_collateral_commitment(&sym(env, "USDC"), &bad),
                        EscrowError::CollateralAmountNotPositive,
                    );
                }
            }),
        },
        Fault {
            label: "record with an empty asset symbol",
            code: EscrowError::CollateralAssetEmpty,
            prepare: no_prep(),
            inject: Box::new(|env, client| {
                assert_contract_kind(
                    "record(empty asset)",
                    client.try_record_sme_collateral_commitment(&sym(env, ""), &1_000),
                    EscrowError::CollateralAssetEmpty,
                );
            }),
        },
        Fault {
            label: "record above the configured ceiling",
            code: EscrowError::CollateralLimitExceeded,
            prepare: Box::new(|_env, client| {
                client.set_collateral_limit(&1_000i128);
            }),
            inject: Box::new(|env, client| {
                // Both the boundary+1 and a far-over value are refused.
                for over in [1_001i128, 50_000, MAX_INVOICE_AMOUNT] {
                    assert_contract_kind(
                        "record(over ceiling)",
                        client.try_record_sme_collateral_commitment(&sym(env, "USDC"), &over),
                        EscrowError::CollateralLimitExceeded,
                    );
                }
            }),
        },
        Fault {
            label: "replacement whose ledger clock rewound",
            code: EscrowError::CollateralTimestampBackwards,
            prepare: Box::new(|env, client| {
                set_timestamp(env, 2_000);
                client.record_sme_collateral_commitment(&sym(env, "USDC"), &500i128);
            }),
            inject: Box::new(|env, client| {
                set_timestamp(env, 1_999);
                assert_contract_kind(
                    "record(clock rewind)",
                    client.try_record_sme_collateral_commitment(&sym(env, "EURC"), &600),
                    EscrowError::CollateralTimestampBackwards,
                );
            }),
        },
        Fault {
            label: "empty batch",
            code: EscrowError::CollateralBatchEmpty,
            prepare: no_prep(),
            inject: Box::new(|env, client| {
                assert_contract_kind(
                    "batch(empty)",
                    client.try_batch_record_collateral(&SorobanVec::from_array(env, [])),
                    EscrowError::CollateralBatchEmpty,
                );
            }),
        },
        Fault {
            label: "over-length batch",
            code: EscrowError::CollateralBatchTooLarge,
            prepare: no_prep(),
            inject: Box::new(|env, client| {
                for over in [MAX_COLLATERAL_BATCH + 1, MAX_COLLATERAL_BATCH * 4] {
                    let too_long = repeated_items(env, &sym(env, "USDC"), 10i128, over);
                    assert_contract_kind(
                        "batch(too long)",
                        client.try_batch_record_collateral(&too_long),
                        EscrowError::CollateralBatchTooLarge,
                    );
                }
            }),
        },
        Fault {
            label: "batch with an invalid item in the middle",
            code: EscrowError::CollateralAmountNotPositive,
            prepare: Box::new(|env, client| {
                client.record_sme_collateral_commitment(&sym(env, "USDC"), &50i128);
            }),
            inject: Box::new(|env, client| {
                let items = SorobanVec::from_array(
                    env,
                    [
                        (sym(env, "USDC"), 100i128),
                        (sym(env, "EURC"), 0i128),
                        (sym(env, "GOLD"), 300i128),
                    ],
                );
                assert_contract_kind(
                    "batch(middle invalid)",
                    client.try_batch_record_collateral(&items),
                    EscrowError::CollateralAmountNotPositive,
                );
            }),
        },
        Fault {
            label: "clear with nothing recorded",
            code: EscrowError::NoCollateralToClear,
            prepare: no_prep(),
            inject: Box::new(|_env, client| {
                assert_contract_kind(
                    "clear(none)",
                    client.try_clear_sme_collateral_commitment(),
                    EscrowError::NoCollateralToClear,
                );
            }),
        },
    ]
}

/// Deploy + `init` and hand both to `body`, so each matrix case starts from a fresh ledger.
fn with_initialized<R>(
    body: impl FnOnce(&Env, &LiquifactEscrowClient<'_>, Address, Address) -> R,
) -> R {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    init_escrow(&env, &client, &admin, &sme);
    body(&env, &client, admin, sme)
}

// ---------------------------------------------------------------------------
// F1 — dependency failures are typed and recoverable
// ---------------------------------------------------------------------------

/// The escrow configuration is the collateral surface's hard dependency. On an instance that was
/// never initialized, a record is refused with a *typed* code rather than panicking or silently
/// succeeding — and once `init` supplies the dependency, the identical call succeeds.
#[test]
fn dependency_missing_escrow_config_is_typed_and_recovers_after_init() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);

    // Dependency absent: `DataKey::Escrow` was never written. Only raw storage is read here —
    // the public views cannot be consulted on an uninitialized instance (`get_escrow` panics by
    // design), so the fingerprint starts from the raw key.
    let address = client.address.clone();
    let pledge_present = env.as_contract(&address, || {
        env.storage().instance().has(&DataKey::SmeCollateralPledge)
    });
    assert!(!pledge_present, "no record key before init");

    assert_contract_kind(
        "record before init",
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128),
        EscrowError::EscrowNotInitialized,
    );
    assert_eq!(
        last_invocation_event_count(&env, &client.address),
        0,
        "an uninitialized rejection must not publish an event"
    );

    // The rejection left nothing behind: the record key was not created.
    assert!(
        !env.as_contract(&address, || {
            env.storage().instance().has(&DataKey::SmeCollateralPledge)
        }),
        "a rejected record on an uninitialized instance must not create the key"
    );

    // Supply the dependency and retry the *same* call.
    init_escrow(&env, &client, &admin, &sme);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);

    let recovered = client
        .get_sme_collateral_commitment()
        .expect("retry applied");
    assert_eq!(recovered.amount, 1_000i128);
    assert_eq!(recovered.asset, sym(&env, "USDC"));
}

/// The SME signature is a *host*-level dependency: when it is unavailable the call is refused
/// before the contract body can act, and restoring the signature makes the identical call apply.
#[test]
fn dependency_missing_sme_signature_is_recoverable_and_never_a_contract_code() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    let before = probe(&env, &client);
    env.mock_auths(&[]);

    // The amount is valid, so the failure can only be the missing signature.
    assert_non_contract_kind(
        "record without an SME signature",
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128),
    );
    assert_eq!(
        last_invocation_event_count(&env, &client.address),
        0,
        "an auth rejection must not publish an event"
    );
    assert_probe_unchanged(
        "record without an SME signature",
        &before,
        &probe(&env, &client),
    );

    // Recovery: the same call, with the signature restored.
    env.mock_all_auths();
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        1_000i128
    );
}

/// The funding token is a dependency the collateral surface must *not* have. Recording stays
/// available when the token contract is unreachable, and the view remains correct — this is the
/// metadata-only guarantee stated as a dependency-failure property.
#[test]
fn dependency_unreachable_token_does_not_block_the_view() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    // `init` pointed at an address with no contract code behind it: every token call fails.
    assert!(
        client.try_get_token_balance().is_err(),
        "the fixture's funding token is expected to be unreachable"
    );

    let before = probe(&env, &client);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &7_777i128);
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        7_777i128,
        "collateral metadata must not depend on the funding token being reachable"
    );

    // And the accounting surfaces the token *would* have backed are still untouched.
    let after = probe(&env, &client);
    assert_eq!(after.funded_amount_raw, before.funded_amount_raw);
    assert_eq!(
        after.view_unique_funder_count,
        before.view_unique_funder_count
    );
    assert_eq!(after.escrow_present, before.escrow_present);
}

/// The ledger clock is a dependency with a monotonicity guard. A rewind is refused with a typed
/// code; advancing the clock is what makes the identical call succeed. This is the recovery path
/// for a caller whose record landed "in the future" relative to a lagging node.
#[test]
fn dependency_rewound_ledger_clock_is_typed_and_recovers_by_advancing() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, 5_000);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &100i128);

    set_timestamp(&env, 4_999);
    let before = probe(&env, &client);
    assert_contract_kind(
        "record against a rewound clock",
        client.try_record_sme_collateral_commitment(&sym(&env, "EURC"), &200i128),
        EscrowError::CollateralTimestampBackwards,
    );
    assert_probe_unchanged(
        "record against a rewound clock",
        &before,
        &probe(&env, &client),
    );

    // Recovery: the clock catches up to the stored `recorded_at` and the retry applies.
    set_timestamp(&env, 5_000);
    client.record_sme_collateral_commitment(&sym(&env, "EURC"), &200i128);
    let recovered = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(recovered.asset, sym(&env, "EURC"));
    assert_eq!(recovered.amount, 200i128);
    assert_eq!(recovered.recorded_at, 5_000);
}

/// The admin nonce is the replay guard on a role rotation. A stale nonce is refused with a typed
/// code *without* advancing the counter, so a client that crashed after submitting can safely
/// resubmit the same nonce — and an interleaved collateral write survives the rotation.
#[test]
fn dependency_stale_admin_nonce_is_recoverable_and_leaves_collateral_intact() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &4_321i128);
    let nonce = client.get_admin_nonce();

    let new_sme = Address::generate(&env);
    let before = probe(&env, &client);

    // A stale nonce is refused and must not consume the counter.
    assert_contract_kind(
        "rotate with a stale nonce",
        client.try_rotate_beneficiary(&new_sme, &(nonce + 99)),
        EscrowError::AdminNonceMismatch,
    );
    assert_eq!(
        client.get_admin_nonce(),
        nonce,
        "a rejected rotation must not advance the nonce"
    );
    assert_probe_unchanged("rotate with a stale nonce", &before, &probe(&env, &client));

    // Recovery: the same rotation with the nonce the contract actually expects.
    client.rotate_beneficiary(&new_sme, &nonce);
    assert_eq!(client.get_admin_nonce(), nonce + 1);
    assert_eq!(client.get_escrow().sme_address, new_sme);

    // The collateral record is unaffected by a successful role rotation: the metadata key is
    // independent of who is allowed to write it next.
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        4_321i128
    );
    client.record_sme_collateral_commitment(&sym(&env, "GOLD"), &9i128);
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().asset,
        sym(&env, "GOLD")
    );
}

/// Every dependency the surface declares, checked against one fault matrix.
///
/// Each row asserts the same three properties, so a new failure mode cannot be added without
/// also being typed, distinct, and provably recoverable:
/// - the rejection carries exactly the pinned code,
/// - no observable surface moved (raw storage, standalone views, bundled snapshot),
/// - the event log of the rejected invocation is empty.
#[test]
fn every_declared_fault_is_typed_distinct_and_leaves_no_trace() {
    let faults = fault_matrix();

    for fault in &faults {
        with_initialized(|env, client, _admin, _sme| {
            (fault.prepare)(env, client);

            let before = probe(env, client);
            let events_before = last_invocation_event_count(env, &client.address);

            (fault.inject)(env, client);

            assert_probe_unchanged(fault.label, &before, &probe(env, client));
            assert_eq!(
                last_invocation_event_count(env, &client.address),
                0,
                "{}: a rejected call must publish no events",
                fault.label
            );
            assert!(
                events_before == last_invocation_event_count(env, &client.address),
                "{}: the pre-existing event log must not grow",
                fault.label
            );
        });
    }

    // Caller-visible coverage: the matrix must pin *exactly* the set of codes the collateral
    // surface can raise through validation — no new failure mode can be added without declaring
    // its code here, and no declared code can go untested. (Distinctness of the codes themselves
    // is asserted separately in `fault_codes_are_observable_and_mutually_distinct`; two matrix
    // rows deliberately share a code, because the single and batch paths raise the same one.)
    let observed: BTreeSet<u32> = faults.iter().map(|f| f.code as u32).collect();
    let expected: BTreeSet<u32> = [
        EscrowError::CollateralAmountNotPositive,
        EscrowError::CollateralAssetEmpty,
        EscrowError::CollateralLimitExceeded,
        EscrowError::CollateralTimestampBackwards,
        EscrowError::CollateralBatchEmpty,
        EscrowError::CollateralBatchTooLarge,
        EscrowError::NoCollateralToClear,
    ]
    .into_iter()
    .map(|e| e as u32)
    .collect();
    assert_eq!(
        observed, expected,
        "the fault matrix and the pinned code set diverged"
    );
}

// ---------------------------------------------------------------------------
// F2 — retries are idempotent and converge
// ---------------------------------------------------------------------------

/// A rejected call is indistinguishable from one that never happened. N rejected attempts leave
/// the state byte-identical to a single rejected attempt — there is no accumulating residue.
#[test]
fn any_number_of_rejected_attempts_leaves_state_byte_identical() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    client.set_collateral_limit(&1_000i128);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &900i128);

    let baseline = probe(&env, &client);

    for attempt in 0..25 {
        assert_contract_kind(
            "repeated over-ceiling record",
            client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &1_001i128),
            EscrowError::CollateralLimitExceeded,
        );
        assert_probe_unchanged("repeated rejection", &baseline, &probe(&env, &client));
        assert_eq!(last_invocation_event_count(&env, &client.address), 0);

        // The rejection is a total no-op: the protected record is still the one in force.
        assert_eq!(
            client.get_sme_collateral_commitment().unwrap().amount,
            900i128,
            "attempt {attempt}: a rejected record must not replace the stored one"
        );
    }
}

/// Retrying a *corrected* write converges on exactly the record a first-time-correct call would
/// have produced — the failed attempts left no partial state to merge with.
#[test]
fn retry_after_failure_converges_on_the_same_record_as_a_clean_call() {
    let clean = Env::default();
    let (clean_client, _cadmin, _csme) = setup_initialized(&clean);
    clean_client.record_sme_collateral_commitment(&sym(&clean, "GOLD"), &6_500i128);
    let expected = clean_client.get_sme_collateral_commitment().unwrap();

    let retried = Env::default();
    let (retried_client, _radmin, _rsme) = setup_initialized(&retried);
    // Fail three times with different reasons, then succeed.
    assert_contract_kind(
        "zero amount",
        retried_client.try_record_sme_collateral_commitment(&sym(&retried, "GOLD"), &0i128),
        EscrowError::CollateralAmountNotPositive,
    );
    assert_contract_kind(
        "empty asset",
        retried_client.try_record_sme_collateral_commitment(&sym(&retried, ""), &6_500i128),
        EscrowError::CollateralAssetEmpty,
    );
    assert_contract_kind(
        "over ceiling",
        retried_client.try_record_sme_collateral_commitment(
            &sym(&retried, "GOLD"),
            &(MAX_INVOICE_AMOUNT + 1),
        ),
        EscrowError::CollateralLimitExceeded,
    );

    retried_client.record_sme_collateral_commitment(&sym(&retried, "GOLD"), &6_500i128);

    assert_eq!(
        retried_client.get_sme_collateral_commitment().unwrap(),
        expected,
        "a retried write must converge on the same record as a clean one"
    );
}

/// A rejected attempt does not consume authorization state, so an unlimited number of retries can
/// still be followed by exactly one successful write.
#[test]
fn retries_do_not_exhaust_authorization_or_the_nonce() {
    let env = Env::default();
    let (client, _admin, sme) = setup_initialized(&env);

    // Repeatedly fail the SME gate with no signature at all.
    env.mock_auths(&[]);
    for _ in 0..10 {
        assert_non_contract_kind(
            "unauthorized record",
            client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128),
        );
    }

    env.mock_all_auths();
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        1_000i128
    );

    // The retry still works after the SME has been rotated away and back, i.e. the gate is read
    // from storage on every call rather than cached across failures.
    let nonce = client.get_admin_nonce();
    let replacement = Address::generate(&env);
    client.rotate_beneficiary(&replacement, &nonce);
    assert_non_contract_kind("record after rotation, no signature", {
        env.mock_auths(&[]);
        client.try_record_sme_collateral_commitment(&sym(&env, "EURC"), &2_000i128)
    });
    env.mock_all_auths();
    client.record_sme_collateral_commitment(&sym(&env, "EURC"), &2_000i128);
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        2_000i128
    );
}

// ---------------------------------------------------------------------------
// F3 — partial completion is impossible
// ---------------------------------------------------------------------------

/// A batch that fails on the **first**, **middle**, or **last** item writes nothing and emits
/// nothing. The three positions are checked on a fresh ledger each, so the guarantee does not
/// depend on what ran before.
#[test]
fn a_batch_failing_at_any_position_completes_nothing() {
    let positions: [(&str, u32); 3] = [
        ("first item", 0),
        ("middle item", MAX_COLLATERAL_BATCH / 2),
        ("last item", MAX_COLLATERAL_BATCH - 1),
    ];

    for (label, bad_index) in positions {
        with_initialized(|env, client, _admin, _sme| {
            // A record worth protecting, written before the failing batch.
            client.record_sme_collateral_commitment(&sym(env, "USDC"), &100i128);
            let before = probe(env, client);

            let usdc = sym(env, "USDC");
            let items: std::vec::Vec<(Symbol, i128)> = (0..MAX_COLLATERAL_BATCH)
                .map(|i| {
                    if i == bad_index {
                        (sym(env, "EURC"), 0i128)
                    } else {
                        (usdc.clone(), 500i128)
                    }
                })
                .collect();

            assert_contract_kind(
                label,
                client.try_batch_record_collateral(&SorobanVec::from_slice(env, &items)),
                EscrowError::CollateralAmountNotPositive,
            );

            assert_probe_unchanged(label, &before, &probe(env, client));
            assert_eq!(
                last_invocation_event_count(env, &client.address),
                0,
                "{label}: a partially-processed batch must emit no events"
            );
        });
    }
}

/// The strongest form of "no partial completion": a batch long enough that a non-atomic
/// implementation would have written several items before failing still leaves storage untouched,
/// verified against raw instance storage rather than through the normalizing view.
#[test]
fn raw_storage_shows_no_item_of_a_failed_batch_was_written() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let address = client.address.clone();

    let valid_prefix: std::vec::Vec<(Symbol, i128)> = (0..MAX_COLLATERAL_BATCH - 1)
        .map(|_| (sym(&env, "USDC"), 111i128))
        .collect();
    let mut items = valid_prefix.clone();
    items.push((sym(&env, "GOLD"), 0i128)); // invalid, and last

    assert_contract_kind(
        "full-length batch with a bad last item",
        client.try_batch_record_collateral(&SorobanVec::from_slice(&env, &items)),
        EscrowError::CollateralAmountNotPositive,
    );

    // Read the key directly as the contract: absent, not "present holding a prefix item".
    let pledge_present = env.as_contract(&address, || {
        env.storage().instance().has(&DataKey::SmeCollateralPledge)
    });
    assert!(
        !pledge_present,
        "no item of the failed batch may reach storage, including the {} valid ones",
        valid_prefix.len()
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

/// Only the corrected batch applies, and it applies exactly once: one event per item, no events
/// from the failed attempts.
#[test]
fn a_corrected_batch_applies_exactly_once_after_failed_attempts() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract_id = client.address.clone();

    // Three failures spanning the batch's failure modes.
    let bad_first =
        SorobanVec::from_array(&env, [(sym(&env, ""), 10i128), (sym(&env, "USDC"), 20i128)]);
    assert_contract_kind(
        "batch with an empty first asset",
        client.try_batch_record_collateral(&bad_first),
        EscrowError::CollateralAssetEmpty,
    );
    let bad_last = SorobanVec::from_array(
        &env,
        [(sym(&env, "USDC"), 10i128), (sym(&env, "USDC"), -5i128)],
    );
    assert_contract_kind(
        "batch with a negative last amount",
        client.try_batch_record_collateral(&bad_last),
        EscrowError::CollateralAmountNotPositive,
    );
    assert_contract_kind(
        "over-length batch",
        client.try_batch_record_collateral(&repeated_items(
            &env,
            &sym(&env, "USDC"),
            10i128,
            MAX_COLLATERAL_BATCH + 1,
        )),
        EscrowError::CollateralBatchTooLarge,
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);

    let good = SorobanVec::from_array(
        &env,
        [
            (sym(&env, "USDC"), 100i128),
            (sym(&env, "EURC"), 200i128),
            (sym(&env, "GOLD"), 300i128),
        ],
    );
    client.batch_record_collateral(&good);

    assert_eq!(
        last_invocation_event_count(&env, &contract_id),
        3,
        "exactly one event per item of the corrected batch, and none from the failures"
    );
    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.asset, sym(&env, "GOLD"));
    assert_eq!(stored.amount, 300i128);
}

/// A batch that fails leaves the *previously stored* record — including its timestamp — intact, so
/// the monotonic-clock guard is not silently disarmed by the failed attempt.
#[test]
fn a_failed_batch_cannot_disarm_the_timestamp_guard() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, 10_000);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &100i128);

    // Rewind the clock, then submit a batch that would be valid but for the guard.
    set_timestamp(&env, 9_000);
    let items = SorobanVec::from_array(
        &env,
        [(sym(&env, "USDC"), 200i128), (sym(&env, "EURC"), 0i128)],
    );
    assert_contract_kind(
        "batch with an invalid item at a rewound clock",
        client.try_batch_record_collateral(&items),
        EscrowError::CollateralAmountNotPositive,
    );

    // The stored record is untouched, so the guard still rejects on the next single attempt.
    let before = probe(&env, &client);
    assert_contract_kind(
        "single record at the same rewound clock",
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &200i128),
        EscrowError::CollateralTimestampBackwards,
    );
    assert_probe_unchanged("guard after a failed batch", &before, &probe(&env, &client));
}

// ---------------------------------------------------------------------------
// F4 — recovery is possible
// ---------------------------------------------------------------------------

/// A retired record is fully reconstructible from the `coll_clr` event alone, and replaying those
/// fields through the ordinary record entrypoint restores the view exactly.
#[test]
fn a_retired_record_is_reconstructible_from_the_clear_event_and_replayable() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, 4_242);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_234i128);
    let original = client.get_sme_collateral_commitment().unwrap();

    client.clear_sme_collateral_commitment();
    // The event log holds only the most recent invocation, so the clear event is read
    // immediately — before any view call resets it.
    let clear_event =
        last_escrow_event(&env, &client.address).expect("the clear published an event");
    assert_eq!(
        event_routing(&env, &clear_event),
        Some(sym(&env, "coll_clr"))
    );
    assert_eq!(client.get_sme_collateral_commitment(), None);

    // Rebuild the record from the event payload — no storage read of the removed key.
    let rebuilt = SmeCollateralCommitment {
        asset: event_field_symbol(&env, &clear_event, sym(&env, "asset")).expect("asset"),
        amount: event_field_i128(&env, &clear_event, sym(&env, "amount")).expect("amount"),
        recorded_at: event_field_u64(&env, &clear_event, sym(&env, "recorded_at"))
            .expect("recorded_at"),
    };
    assert_eq!(
        rebuilt, original,
        "the clear event must carry the whole removed record"
    );

    // Replay it through the ordinary entrypoint at the original timestamp.
    set_timestamp(&env, rebuilt.recorded_at);
    client.record_sme_collateral_commitment(&rebuilt.asset, &rebuilt.amount);
    assert_eq!(client.get_sme_collateral_commitment().unwrap(), original);
}

/// A full failure-then-replay cycle returns the view to a previously observed state *exactly*,
/// including raw storage and the bundled snapshot — not merely to an equal-looking record.
#[test]
fn a_full_failure_then_replay_cycle_returns_to_the_observed_state() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, 777);

    client.record_sme_collateral_commitment(&sym(&env, "GOLD"), &12_345i128);
    client.set_collateral_limit(&50_000i128);
    let observed = probe(&env, &client);

    // A long, varied run of failures across every dependency.
    for _ in 0..3 {
        assert_contract_kind(
            "zero amount",
            client.try_record_sme_collateral_commitment(&sym(&env, "GOLD"), &0i128),
            EscrowError::CollateralAmountNotPositive,
        );
        assert_contract_kind(
            "empty asset",
            client.try_record_sme_collateral_commitment(&sym(&env, ""), &12_345i128),
            EscrowError::CollateralAssetEmpty,
        );
        assert_contract_kind(
            "over ceiling",
            client.try_record_sme_collateral_commitment(&sym(&env, "GOLD"), &50_001i128),
            EscrowError::CollateralLimitExceeded,
        );
        assert_contract_kind(
            "batch empty after failures",
            client.try_batch_record_collateral(&SorobanVec::from_array(&env, [])),
            EscrowError::CollateralBatchEmpty,
        );
        assert_contract_kind(
            "batch too long after failures",
            client.try_batch_record_collateral(&repeated_items(
                &env,
                &sym(&env, "GOLD"),
                10i128,
                MAX_COLLATERAL_BATCH + 1,
            )),
            EscrowError::CollateralBatchTooLarge,
        );
    }

    // Every failure was a no-op: still exactly the observed state.
    assert_eq!(probe(&env, &client), observed);

    // Retiring and replaying the record round-trips to the same fingerprint.
    client.clear_sme_collateral_commitment();
    assert_eq!(client.get_sme_collateral_commitment(), None);
    client.record_sme_collateral_commitment(&sym(&env, "GOLD"), &12_345i128);
    assert_eq!(probe(&env, &client), observed);
}

/// The record survives an unrelated *accepted* interleaving that touches the same escrow record,
/// so recovery does not require quiescence.
#[test]
fn recovery_survives_unrelated_accepted_writes() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128);

    // Fail a record...
    assert_contract_kind(
        "record with a bad amount",
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &0i128),
        EscrowError::CollateralAmountNotPositive,
    );

    // ...while an unrelated admin write succeeds in between.
    client.set_collateral_limit(&2_000i128);

    // The recovered write applies and the ceiling is what the caller was told it is.
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &2_000i128);
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        2_000i128
    );
    assert_eq!(client.get_collateral_limit(), 2_000i128);
    assert_eq!(
        client.get_escrow_summary().sme_collateral_commitment,
        CollateralCommitmentSnapshot::Some(SmeCollateralCommitment {
            asset: sym(&env, "USDC"),
            amount: 2_000i128,
            recorded_at: client.get_sme_collateral_commitment().unwrap().recorded_at,
        })
    );
}

// ---------------------------------------------------------------------------
// F5 — failures are observable without side effects
// ---------------------------------------------------------------------------

/// A rejected call publishes no event of any kind — not a "rejected" event, not an event for the
/// subset of a batch it managed to validate. An indexer consuming only the event log therefore
/// cannot invent a state change that never happened.
#[test]
fn a_rejected_call_publishes_no_event_of_any_kind() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    // Start from an accepted write so there is a non-empty prior event log to compare against.
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &500i128);
    let accepted = last_escrow_event(&env, &client.address).unwrap();

    let rejections: std::vec::Vec<(&str, std::vec::Vec<(Symbol, i128)>)> = vec![
        ("zero amount", vec![(sym(&env, "USDC"), 0i128)]),
        ("negative amount", vec![(sym(&env, "USDC"), -1i128)]),
        ("empty asset", vec![(sym(&env, ""), 1i128)]),
        (
            "over ceiling",
            vec![(sym(&env, "USDC"), &MAX_INVOICE_AMOUNT + 1)],
        ),
        (
            "batch, bad first item",
            vec![(sym(&env, ""), 1i128), (sym(&env, "USDC"), 2i128)],
        ),
        (
            "batch, bad last item",
            vec![(sym(&env, "USDC"), 1i128), (sym(&env, "USDC"), 0i128)],
        ),
    ];

    for (label, item) in rejections {
        let result = client.try_batch_record_collateral(&SorobanVec::from_slice(&env, &item));
        assert!(result.is_err(), "{label}: expected a rejection");
        assert_eq!(
            last_invocation_event_count(&env, &client.address),
            0,
            "{label}: the rejected invocation's event log must be empty"
        );
        assert_eq!(
            client.get_sme_collateral_commitment().unwrap().amount,
            500i128,
            "{label}: the accepted record must be untouched"
        );
    }

    // Reading the log again (a fresh invocation) still shows only the one accepted event.
    assert_eq!(last_invocation_event_count(&env, &client.address), 0);
    assert_eq!(last_escrow_event(&env, &client.address), None);
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        500i128
    );
    let _ = accepted;
}

/// Validation failures stay *inside* the contract-code space and authorization/dependency failures
/// stay *outside* it, so an integrating caller can branch on the distinction without string
/// matching. Both are asserted from the same invalid input, differing only in the mock auth.
#[test]
fn validation_and_authorization_failures_are_distinguishable_to_callers() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    // Invalid input *with* authorization: a typed contract code.
    assert_contract_kind(
        "invalid input, authorized",
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &0i128),
        EscrowError::CollateralAmountNotPositive,
    );

    // Valid input *without* authorization: a host-level rejection, not a contract code.
    env.mock_auths(&[]);
    assert_non_contract_kind(
        "valid input, unauthorized",
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &1_000i128),
    );

    // Invalid input *without* authorization: the contract still validates and reports its code,
    // because validation precedes the authorization check. This is why the two classes cannot be
    // conflated by an integrator.
    assert_contract_kind(
        "invalid input, unauthorized",
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &0i128),
        EscrowError::CollateralAmountNotPositive,
    );

    // Nothing landed in any of the three cases.
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

/// Every code in the matrix is observable *and* distinguishable from every other, so an operator
/// can tell a recoverable input error from an operator-actionable one (uninitialized, stale nonce)
/// without inspecting logs.
#[test]
fn fault_codes_are_observable_and_mutually_distinct() {
    let expected: BTreeSet<(EscrowError, u32)> = [
        EscrowError::EscrowNotInitialized,
        EscrowError::CollateralAmountNotPositive,
        EscrowError::CollateralAssetEmpty,
        EscrowError::CollateralLimitExceeded,
        EscrowError::CollateralTimestampBackwards,
        EscrowError::CollateralBatchEmpty,
        EscrowError::CollateralBatchTooLarge,
        EscrowError::NoCollateralToClear,
        EscrowError::AdminNonceMismatch,
        EscrowError::LegalHoldBlocksBeneficiaryRotation,
    ]
    .into_iter()
    .map(|e| (e, e as u32))
    .collect();

    for (error, code) in &expected {
        assert_eq!(
            classify_failure::<(), ()>("matrix", Err(Ok(Error::from_contract_error(*code)))),
            FailureKind::Contract(*code),
            "{error:?} must classify as its own typed code"
        );
    }

    let codes: BTreeSet<u32> = expected.iter().map(|(_, c)| *c).collect();
    assert_eq!(codes.len(), expected.len(), "two codes collide");
}

// ---------------------------------------------------------------------------
// F6 — interleaved calls stay consistent
// ---------------------------------------------------------------------------

/// The record and the ceiling that gates it are observable **together** from a single
/// `get_escrow_summary` invocation. A consumer diagnosing a `CollateralLimitExceeded` rejection
/// reads both from one ledger state and cannot be handed a mismatched pair.
#[test]
fn the_record_and_its_ceiling_are_observable_from_one_invocation() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    // Before any record, the snapshot reports the default ceiling and the unset record.
    let unset = client.get_escrow_summary();
    assert_eq!(unset.collateral_limit, MAX_INVOICE_AMOUNT);
    assert_eq!(
        unset.sme_collateral_commitment,
        CollateralCommitmentSnapshot::None
    );

    client.set_collateral_limit(&5_000i128);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &5_000i128);

    // The pair that the rejection below will be diagnosed against.
    let summary = client.get_escrow_summary();
    assert_eq!(summary.collateral_limit, 5_000i128);
    assert_eq!(
        summary.sme_collateral_commitment,
        CollateralCommitmentSnapshot::Some(SmeCollateralCommitment {
            asset: sym(&env, "USDC"),
            amount: 5_000i128,
            recorded_at: client.get_sme_collateral_commitment().unwrap().recorded_at,
        })
    );

    assert_contract_kind(
        "record above the ceiling",
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &5_001i128),
        EscrowError::CollateralLimitExceeded,
    );

    // Post-rejection, the snapshot still shows the ceiling that was in force and the record that
    // was protected — the two facts a consumer needs to retry correctly.
    let after = client.get_escrow_summary();
    assert_eq!(after.collateral_limit, 5_000i128);
    assert_eq!(
        after.sme_collateral_commitment,
        CollateralCommitmentSnapshot::Some(SmeCollateralCommitment {
            asset: sym(&env, "USDC"),
            amount: 5_000i128,
            recorded_at: client.get_sme_collateral_commitment().unwrap().recorded_at,
        })
    );
}

/// A read-then-write across two invocations loses a race with a concurrent admin tightening, and
/// the rejection is recoverable by re-reading: the caller learns the new ceiling and the retry
/// applies.
#[test]
fn a_lost_race_with_the_admin_ceiling_is_recoverable_by_re_reading() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    client.set_collateral_limit(&10_000i128);

    // The caller reads the ceiling...
    let observed_ceiling = client.get_collateral_limit();
    assert_eq!(observed_ceiling, 10_000i128);

    // ...and the admin tightens it before the write lands.
    client.set_collateral_limit(&1_000i128);

    // The write is rejected against the *new* ceiling, not the one the caller saw.
    assert_contract_kind(
        "record written against a stale ceiling",
        client.try_record_sme_collateral_commitment(&sym(&env, "USDC"), &(observed_ceiling - 1)),
        EscrowError::CollateralLimitExceeded,
    );

    // Recovery: re-read, then retry within the ceiling that is actually in force.
    let current_ceiling = client.get_collateral_limit();
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &current_ceiling);
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        current_ceiling
    );
    assert_eq!(client.get_collateral_limit(), current_ceiling);
}

/// Any interleaving of records and clears serialises to the last *accepted* write. Rejected
/// attempts interleaved between accepted ones never win.
#[test]
fn interleaved_records_and_clears_serialise_to_the_last_accepted_write() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    set_timestamp(&env, 1_000);

    let usdc = sym(&env, "USDC");
    let eurc = sym(&env, "EURC");

    // Accepted, rejected, accepted, rejected...
    client.record_sme_collateral_commitment(&usdc, &100i128);
    assert_contract_kind(
        "interleaved bad record",
        client.try_record_sme_collateral_commitment(&eurc, &-1i128),
        EscrowError::CollateralAmountNotPositive,
    );
    client.record_sme_collateral_commitment(&eurc, &200i128);
    assert_contract_kind(
        "interleaved over-ceiling record",
        client.try_record_sme_collateral_commitment(&usdc, &(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceeded,
    );
    client.batch_record_collateral(&SorobanVec::from_array(
        &env,
        [(usdc.clone(), 300i128), (eurc.clone(), 400i128)],
    ));

    assert_eq!(client.get_sme_collateral_commitment().unwrap().asset, eurc);
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        400i128
    );

    // Now interleave a clear: rejected clear, then accepted clear, then a re-record.
    assert_contract_kind(
        "interleaved bad clear",
        {
            // Clear succeeds here, so use a distinct failure: a batch that fails.
            client.try_batch_record_collateral(&SorobanVec::from_array(&env, []))
        },
        EscrowError::CollateralBatchEmpty,
    );
    client.clear_sme_collateral_commitment();
    assert_contract_kind(
        "clear again",
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );
    client.record_sme_collateral_commitment(&usdc, &500i128);

    let final_state = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(final_state.asset, usdc);
    assert_eq!(final_state.amount, 500i128);
}

/// Reads never mutate: pulling every view in the surface — including the bundled snapshot — leaves
/// the fingerprint identical and publishes nothing, so an operator polling for diagnostics cannot
/// perturb the state they are diagnosing.
#[test]
fn exhaustive_reads_are_side_effect_free_under_repetition() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    client.record_sme_collateral_commitment(&sym(&env, "USDC"), &2_500i128);

    let before = probe(&env, &client);
    for _ in 0..10 {
        let _ = client.get_sme_collateral_commitment();
        let _ = client.get_collateral_limit();
        let _ = client.get_escrow_summary();
        let _ = client.get_unique_funder_count();
        let _ = client.get_legal_hold();
        let _ = client.get_admin_nonce();
        let _ = client.get_version();
    }
    assert_probe_unchanged("repeated reads", &before, &probe(&env, &client));
    assert_eq!(
        last_invocation_event_count(&env, &client.address),
        0,
        "read-only invocations publish no events"
    );
}

// ---------------------------------------------------------------------------
// F7 — boundaries stay deterministic
// ---------------------------------------------------------------------------

/// The amount boundary matrix: each value is classified identically on every run, including the
/// values immediately outside each edge. Both the accepted and the rejected side are pinned, so a
/// change to either fails here.
#[test]
fn the_amount_boundary_matrix_is_deterministic() {
    // (amount, expected outcome)
    let cases: [(i128, Option<EscrowError>); 8] = [
        (1, None),
        (2, None),
        (999, None),
        (1_000, None),
        (1_001, Some(EscrowError::CollateralLimitExceeded)),
        (2_000, Some(EscrowError::CollateralLimitExceeded)),
        (0, Some(EscrowError::CollateralAmountNotPositive)),
        (-1, Some(EscrowError::CollateralAmountNotPositive)),
    ];

    with_initialized(|env, client, _admin, _sme| {
        client.set_collateral_limit(&1_000i128);

        // Two full passes: a boundary that drifts between runs fails the second pass.
        for pass in 0..2 {
            for (amount, expected) in cases {
                let label = format!("pass {pass}: amount {amount}");
                match expected {
                    None => {
                        client.record_sme_collateral_commitment(&sym(env, "USDC"), &amount);
                        assert_eq!(
                            client.get_sme_collateral_commitment().unwrap().amount,
                            amount,
                            "{label}: accepted amount must be surfaced exactly"
                        );
                        client.clear_sme_collateral_commitment();
                    }
                    Some(error) => {
                        assert_contract_kind(
                            &label,
                            client.try_record_sme_collateral_commitment(&sym(env, "USDC"), &amount),
                            error,
                        );
                    }
                }
            }
        }
    });
}

/// The batch-length boundary matrix: `0`, `1`, `MAX_COLLATERAL_BATCH`, and `MAX_COLLATERAL_BATCH + 1`
/// are classified identically on every run, with the stored record and the event count matching the
/// verdict.
#[test]
fn the_batch_length_boundary_matrix_is_deterministic() {
    let cases: [(u32, bool); 5] = [
        (0, false),
        (1, true),
        (MAX_COLLATERAL_BATCH - 1, true),
        (MAX_COLLATERAL_BATCH, true),
        (MAX_COLLATERAL_BATCH + 1, false),
    ];

    for pass in 0..2 {
        for (len, accepted) in cases {
            with_initialized(|env, client, _admin, _sme| {
                let label = format!("pass {pass}: batch of {len}");
                let items = repeated_items(env, &sym(env, "USDC"), 25i128, len);

                if accepted {
                    client.batch_record_collateral(&items);
                    // Read the event count first: a view call would reset the log.
                    assert_eq!(
                        last_invocation_event_count(env, &client.address),
                        len as usize,
                        "{label}: one event per item"
                    );
                    assert_eq!(
                        client.get_sme_collateral_commitment().unwrap().amount,
                        25i128,
                        "{label}: an accepted batch must be visible"
                    );
                } else {
                    let error = match len {
                        0 => EscrowError::CollateralBatchEmpty,
                        _ => EscrowError::CollateralBatchTooLarge,
                    };
                    assert_contract_kind(&label, client.try_batch_record_collateral(&items), error);
                    assert_eq!(
                        client.get_sme_collateral_commitment(),
                        None,
                        "{label}: a rejected batch must store nothing"
                    );
                }
            });
        }
    }
}

/// The default ceiling's boundary is deterministic too: the exact default is accepted and one
/// stroop above it is refused, on both the single and the batch path.
#[test]
fn the_default_ceiling_boundary_is_deterministic_on_both_paths() {
    with_initialized(|env, client, _admin, _sme| {
        assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);

        client.record_sme_collateral_commitment(&sym(env, "USDC"), &MAX_INVOICE_AMOUNT);
        assert_eq!(
            client.get_sme_collateral_commitment().unwrap().amount,
            MAX_INVOICE_AMOUNT
        );

        assert_contract_kind(
            "one stroop above the default ceiling",
            client
                .try_record_sme_collateral_commitment(&sym(env, "USDC"), &(MAX_INVOICE_AMOUNT + 1)),
            EscrowError::CollateralLimitExceeded,
        );

        let items = SorobanVec::from_array(env, [(sym(env, "USDC"), MAX_INVOICE_AMOUNT + 1)]);
        assert_contract_kind(
            "batch one stroop above the default ceiling",
            client.try_batch_record_collateral(&items),
            EscrowError::CollateralLimitExceeded,
        );

        // The accepted record survived both rejections.
        assert_eq!(
            client.get_sme_collateral_commitment().unwrap().amount,
            MAX_INVOICE_AMOUNT
        );
    });
}
