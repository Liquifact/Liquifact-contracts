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

use crate::tests::{assert_contract_error, setup};
use crate::{
    CollateralClearedEvt, CollateralCommitmentSnapshot, CollateralRecordedEvt, EscrowError,
    LiquifactEscrowClient, MAX_COLLATERAL_BATCH, MAX_INVOICE_AMOUNT, SmeCollateralCommitment,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    xdr::ContractEvent,
    Address, Env, Event as _, IntoVal, Symbol, Vec as SorobanVec,
};

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
    let items: std::vec::Vec<(Symbol, i128)> = (0..len)
        .map(|_| (asset.clone(), amount))
        .collect();
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
        client.try_record_sme_collateral_commitment(&sym(&env, "EURC"), &2_000i128).is_err(),
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

    assert_eq!(client.get_sme_collateral_commitment().unwrap().amount, 1i128);
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
    assert_eq!(client.get_sme_collateral_commitment().unwrap().amount, 1i128);
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
        [(usdc.clone(), 100i128), (eur.clone(), 900i128), (usdc.clone(), 450i128)],
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
    assert_eq!(client.get_sme_collateral_commitment().unwrap().amount, 10i128);
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
    let bad_asset = SorobanVec::from_array(
        &env,
        [(sym(&env, ""), 100i128), (usdc.clone(), 300i128)],
    );
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
    assert_eq!(after.sme_collateral_commitment, CollateralCommitmentSnapshot::None);
}
