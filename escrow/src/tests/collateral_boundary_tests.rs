//! Boundary and compatibility contracts for the SME collateral surface.
//!
//! [`collateral_state_view`](super::collateral_state_view) pins the *state machine* of the
//! collateral key: which transitions are legal, what each one publishes, and how the read surface
//! stays in agreement. This module covers the complementary axis — the **edges of the input
//! domain** and the **compatibility surface that existing callers and indexers depend on**.
//!
//! # Compatibility contracts (C1–C6)
//!
//! On Soroban a contract is a wire protocol, not just a Rust API. Anything an integrator can
//! observe from outside the contract is a compatibility contract, and changing it breaks existing
//! callers even when the Rust signature still compiles. The collateral surface exposes six:
//!
//! - **C1 — error codes are frozen.** Callers branch on the numeric code, not on a panic string.
//!   Every collateral error is pinned to its exact `u32`, the codes are proven pairwise distinct,
//!   and each pinned code is proven reachable through the public entrypoints.
//! - **C2 — event routing symbols and payload schemas are frozen.** Indexers route on the `name`
//!   topic and read named body fields. `coll_rec`, `coll_lim`, and `coll_clr` are pinned together
//!   with their topic counts, their payload keys, and the fact that `invoice_id` is a *topic* for
//!   the limit and clear events but a *body field* for the record event.
//! - **C3 — public constants are frozen.** `MAX_COLLATERAL_BATCH` and `MAX_INVOICE_AMOUNT` are
//!   part of the contract: a client that pre-sizes a batch vector or clamps an amount locally must
//!   see the same bounds the contract enforces at the edges.
//! - **C4 — the additive-key default is frozen.** An instance that never called
//!   `set_collateral_limit` leaves the key absent, reads the ceiling as `MAX_INVOICE_AMOUNT`, and
//!   accepts exactly the commitments it accepted before the setter existed. Writing the ceiling
//!   to the default value is observationally transparent through every read surface.
//! - **C5 — guard ordering is frozen.** `record_sme_collateral_commitment` and
//!   `batch_record_collateral` validate the payload *before* they authorize;
//!   `clear_sme_collateral_commitment` checks existence *before* it authorizes; and
//!   `set_collateral_limit` authorizes *before* it validates. A caller therefore knows which code
//!   a malformed payload yields, and a legacy caller that never had a ceiling key still gets a
//!   typed validation error rather than a panic.
//! - **C6 — authorization failures stay outside the contract-code space.** A caller that is not
//!   the configured SME is rejected with a host-level auth error, never a typed `EscrowError`, so
//!   a legacy caller can always tell "fix the payload" from "fix the signature".
//!
//! # Boundary conditions (B1–B8)
//!
//! - **B1 — amount floor.** `amount = 1` is accepted; `0` and every negative value (down to
//!   `i128::MIN`) are rejected with `CollateralAmountNotPositive`.
//! - **B2 — amount ceiling.** With the default ceiling, `amount = MAX_INVOICE_AMOUNT` is accepted
//!   and one stroop more is rejected; with a configured ceiling, `amount == limit` is accepted and
//!   `limit + 1` is rejected. A tightened ceiling stays non-retroactive.
//! - **B3 — batch length.** Length `1` and length `MAX_COLLATERAL_BATCH` are accepted; length `0`
//!   and `MAX_COLLATERAL_BATCH + 1` are rejected.
//! - **B4 — batch atomicity.** An invalid item voids the whole batch from any position, including
//!   the first and the last slot of a full-length batch.
//! - **B5 — empty payload.** An empty asset symbol is rejected, alone and inside a batch; an empty
//!   batch is rejected as empty *before* its (absent) items are examined.
//! - **B6 — timestamp edges.** `recorded_at = 0` round-trips, `u64::MAX` round-trips, and a
//!   replacement at the same timestamp is accepted while a strictly older one is rejected.
//! - **B7 — rejected calls are inert.** Every rejection above leaves the record, the ceiling, and
//!   the raw key presence byte-identical, and publishes no event of any kind.
//! - **B8 — determinism.** The amount/ceiling and batch-length matrices are classified identically
//!   on every sweep, so the suite is neither order-dependent nor timing-dependent.
//!
//! # Reading the event log
//!
//! [`Env::events`] exposes the events of the **most recent** contract invocation only, and a failed
//! invocation publishes none. Every event assertion below therefore captures the log immediately
//! after the call under test, before any other invocation (including the read-only
//! `get_escrow_summary` inside [`probe`]) replaces that view.

use crate::tests::{deploy, setup};
use crate::{
    CollateralClearedEvt, CollateralCommitmentSnapshot, CollateralLimitUpdated,
    CollateralRecordedEvt, DataKey, EscrowError, LiquifactEscrowClient, SmeCollateralCommitment,
    MAX_COLLATERAL_BATCH, MAX_INVOICE_AMOUNT,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    xdr::{ContractEvent, ContractEventBody, ScErrorType, ScMap, ScVal},
    Address, Env, Error, Event as _, IntoVal, InvokeError, Symbol, Vec as SorobanVec,
};
use std::fmt::Debug;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `init` takes 19 arguments; the trailing `Option`s are explicit `None`s so this stays in step
/// with the contract signature.
fn init_escrow(env: &Env, client: &LiquifactEscrowClient<'_>, admin: &Address, sme: &Address) {
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "COLBND01"),
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

/// Builds a batch of `len` identical items, used to probe the `MAX_COLLATERAL_BATCH` edges.
fn repeated_items(env: &Env, asset: &Symbol, amount: i128, len: u32) -> SorobanVec<(Symbol, i128)> {
    let items: std::vec::Vec<(Symbol, i128)> = (0..len).map(|_| (asset.clone(), amount)).collect();
    SorobanVec::from_slice(env, &items)
}

// --- event inspection (an indexer reads routing symbol + payload keys) -----------

fn sc_symbol(env: &Env, value: &ScVal) -> Option<Symbol> {
    match value {
        ScVal::Symbol(sym) => Some(Symbol::new(env, &sym.0.to_string())),
        _ => None,
    }
}

/// `ScVal::I128` as an [`i128`]. The SDK stores 128-bit integers as a 64-bit `hi`/`lo` pair;
/// the sign bit is cleared so the decode is total for every positive value on the wire.
fn sc_i128(value: &ScVal) -> Option<i128> {
    match value {
        ScVal::I128(parts) => {
            let magnitude = ((parts.hi as u128) << 64) | (parts.lo as u128);
            Some((magnitude & (u128::MAX >> 1)) as i128)
        }
        _ => None,
    }
}

fn sc_u64(value: &ScVal) -> Option<u64> {
    match value {
        ScVal::U64(v) => Some(*v),
        _ => None,
    }
}

/// The V0 body of a contract event: `(topics, body fields)`.
type EventBodyView = Option<(std::vec::Vec<Symbol>, Vec<(Symbol, ScVal)>)>;

fn event_body(env: &Env, event: &ContractEvent) -> EventBodyView {
    let ContractEventBody::V0(body) = &event.body;
    let topics: std::vec::Vec<Symbol> = body
        .topics
        .iter()
        .map(|topic| sc_symbol(env, topic))
        .collect::<Option<std::vec::Vec<_>>>()?;
    let fields = match &body.data {
        ScVal::Map(Some(ScMap(entries))) => entries
            .iter()
            .filter_map(|entry| Some((sc_symbol(env, &entry.key)?, entry.val.clone())))
            .collect(),
        _ => Vec::new(),
    };
    Some((topics, fields))
}

/// The routing symbol of an event (`coll_rec`, `coll_clr`, `coll_lim`): topic 1, right after the
/// event-type discriminant.
fn event_routing(env: &Env, event: &ContractEvent) -> Option<Symbol> {
    event_body(env, event).and_then(|(topics, _)| topics.get(1).cloned())
}

/// Events published by `contract` during the **most recent** contract invocation.
///
/// A successful invocation replaces this view, and a failed one leaves it empty — which is exactly
/// what makes "this call published nothing" observable.
fn last_invocation_events(env: &Env, contract: &Address) -> std::vec::Vec<ContractEvent> {
    env.events()
        .all()
        .filter_by_contract(contract)
        .events()
        .to_vec()
}

/// How many events the most recent invocation published under routing symbol `routing`.
fn count_routing_events(env: &Env, contract: &Address, routing: Symbol) -> usize {
    last_invocation_events(env, contract)
        .iter()
        .filter(|event| event_routing(env, event).as_ref() == Some(&routing))
        .count()
}

fn event_field_i128(env: &Env, event: &ContractEvent, key: Symbol) -> Option<i128> {
    let (_, fields) = event_body(env, event)?;
    fields
        .into_iter()
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| sc_i128(&value))
}

fn event_field_u64(env: &Env, event: &ContractEvent, key: Symbol) -> Option<u64> {
    let (_, fields) = event_body(env, event)?;
    fields
        .into_iter()
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| sc_u64(&value))
}

fn event_field_symbol(env: &Env, event: &ContractEvent, key: Symbol) -> Option<Symbol> {
    let (_, fields) = event_body(env, event)?;
    fields
        .into_iter()
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| sc_symbol(env, &value))
}

/// Sorted payload keys of an event, used to pin the wire schema of each collateral event.
fn event_field_keys(env: &Env, event: &ContractEvent) -> std::vec::Vec<Symbol> {
    let (_, fields) = event_body(env, event).unwrap();
    let mut keys: std::vec::Vec<Symbol> = fields.into_iter().map(|(name, _)| name).collect();
    keys.sort_by_key(|symbol| symbol.to_string());
    keys
}

/// The topic list of an event: `[event_type, routing, ..topic fields]`.
fn event_topics(env: &Env, event: &ContractEvent) -> std::vec::Vec<Symbol> {
    event_body(env, event).unwrap().0
}

// --- failure classification ---------------------------------------------------

/// How a `try_*` invocation failed, as observed by a caller that only has the wire protocol.
#[derive(Debug, PartialEq)]
enum FailureKind {
    /// Rejected by the contract with a typed `EscrowError` code.
    Contract(u32),
    /// Rejected by the host (authorization, or a dependency that is simply absent).
    NonContract,
    /// Aborted without a recoverable error value.
    Aborted,
}

impl FailureKind {
    fn describe(&self) -> String {
        match self {
            FailureKind::Contract(code) => format!("Contract({code})"),
            FailureKind::NonContract => "NonContract".to_string(),
            FailureKind::Aborted => "Aborted".to_string(),
        }
    }
}

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

/// Assert the rejection carries exactly the typed code `expected`, in the contract-code space,
/// and published no event of any kind.
fn assert_rejected<T: Debug, E: Debug>(
    env: &Env,
    contract: &Address,
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
    let published = last_invocation_events(env, contract);
    assert!(
        published.is_empty(),
        "{label}: a rejected call must publish no event, got {}",
        published.len()
    );
}

/// Assert the rejection never reached the contract body (authorization failure).
fn assert_rejected_without_contract_code<T: Debug, E: Debug>(
    env: &Env,
    contract: &Address,
    label: &str,
    result: Result<Result<T, E>, Result<Error, InvokeError>>,
) {
    let observed = classify_failure(label, result);
    assert_eq!(
        observed,
        FailureKind::NonContract,
        "{label}: expected a host-level (auth) rejection, got {}",
        observed.describe()
    );
    let published = last_invocation_events(env, contract);
    assert!(
        published.is_empty(),
        "{label}: a rejected call must publish no event, got {}",
        published.len()
    );
}

// --- state fingerprint ---------------------------------------------------------

/// Everything an integrator can observe about the collateral state, read the way a caller would.
#[derive(Clone, Debug, PartialEq)]
struct BoundaryProbe {
    /// `DataKey::SmeCollateralPledge` presence, independent of the stored value.
    pledge_present: bool,
    pledge_raw: Option<SmeCollateralCommitment>,
    /// `DataKey::CollateralLimit`: `None` while the additive key has never been written.
    limit_raw: Option<i128>,
    view_record: Option<SmeCollateralCommitment>,
    view_limit: i128,
    snapshot_record: CollateralCommitmentSnapshot,
    snapshot_limit: i128,
}

fn probe(env: &Env, client: &LiquifactEscrowClient<'_>) -> BoundaryProbe {
    let address = client.address.clone();
    let (pledge_present, pledge_raw, limit_raw) = env.as_contract(&address, || {
        (
            env.storage().instance().has(&DataKey::SmeCollateralPledge),
            env.storage().instance().get(&DataKey::SmeCollateralPledge),
            env.storage().instance().get(&DataKey::CollateralLimit),
        )
    });
    let summary = client.get_escrow_summary();
    BoundaryProbe {
        pledge_present,
        pledge_raw,
        limit_raw,
        view_record: client.get_sme_collateral_commitment(),
        view_limit: client.get_collateral_limit(),
        snapshot_record: summary.sme_collateral_commitment,
        snapshot_limit: summary.collateral_limit,
    }
}

/// Assert nothing changed anywhere an integrator can look.
fn assert_probe_unchanged(label: &str, before: &BoundaryProbe, after: &BoundaryProbe) {
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
        after.view_record, before.view_record,
        "{label}: get_sme_collateral_commitment changed"
    );
    assert_eq!(
        after.view_limit, before.view_limit,
        "{label}: get_collateral_limit changed"
    );
    assert_eq!(
        after.snapshot_record, before.snapshot_record,
        "{label}: the summary snapshot's collateral record changed"
    );
    assert_eq!(
        after.snapshot_limit, before.snapshot_limit,
        "{label}: the summary snapshot's ceiling changed"
    );
}

// ---------------------------------------------------------------------------
// C1 — error codes are frozen
// ---------------------------------------------------------------------------

/// Every collateral error keeps its exact numeric code.
///
/// A client SDK that switches on `60`, `61`, … must keep working. Renumbering any of these is a
/// breaking protocol change even though the Rust enum still compiles, which is why the codes are
/// asserted literally instead of being derived.
#[test]
fn collateral_error_codes_are_frozen() {
    assert_eq!(EscrowError::CollateralAmountNotPositive as u32, 60);
    assert_eq!(EscrowError::CollateralAssetEmpty as u32, 61);
    assert_eq!(EscrowError::CollateralTimestampBackwards as u32, 62);
    assert_eq!(EscrowError::CollateralBatchEmpty as u32, 63);
    assert_eq!(EscrowError::CollateralBatchTooLarge as u32, 64);
    assert_eq!(EscrowError::CollateralLimitNotPositive as u32, 65);
    assert_eq!(EscrowError::CollateralLimitExceeded as u32, 66);
    assert_eq!(EscrowError::CollateralLimitExceedsMax as u32, 67);
    assert_eq!(EscrowError::NoCollateralToClear as u32, 169);
}

/// The frozen codes are pairwise distinct, so a new variant cannot silently alias an existing one.
#[test]
fn collateral_error_codes_are_pairwise_distinct() {
    let codes = [
        EscrowError::CollateralAmountNotPositive as u32,
        EscrowError::CollateralAssetEmpty as u32,
        EscrowError::CollateralTimestampBackwards as u32,
        EscrowError::CollateralBatchEmpty as u32,
        EscrowError::CollateralBatchTooLarge as u32,
        EscrowError::CollateralLimitNotPositive as u32,
        EscrowError::CollateralLimitExceeded as u32,
        EscrowError::CollateralLimitExceedsMax as u32,
        EscrowError::NoCollateralToClear as u32,
    ];
    let mut sorted = codes;
    sorted.sort_unstable();
    for pair in sorted.windows(2) {
        assert_ne!(
            pair[0], pair[1],
            "duplicate collateral error code in the frozen set"
        );
    }
}

/// Every frozen code is reachable through the public entrypoints, so each pinned number is a
/// number a caller actually observes — not a dead enum variant.
#[test]
fn every_frozen_collateral_code_is_reachable_from_the_public_surface() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");
    let empty = sym(&env, "");

    // 60 — non-positive amount.
    assert_rejected(
        &env,
        &contract,
        "record with a zero amount",
        client.try_record_sme_collateral_commitment(&asset, &0i128),
        EscrowError::CollateralAmountNotPositive,
    );
    // 61 — empty asset symbol.
    assert_rejected(
        &env,
        &contract,
        "record with an empty asset",
        client.try_record_sme_collateral_commitment(&empty, &10i128),
        EscrowError::CollateralAssetEmpty,
    );
    // 63 — empty batch.
    assert_rejected(
        &env,
        &contract,
        "empty batch",
        client.try_batch_record_collateral(&SorobanVec::new(&env)),
        EscrowError::CollateralBatchEmpty,
    );
    // 64 — oversized batch.
    assert_rejected(
        &env,
        &contract,
        "oversized batch",
        client.try_batch_record_collateral(&repeated_items(
            &env,
            &asset,
            1i128,
            MAX_COLLATERAL_BATCH + 1,
        )),
        EscrowError::CollateralBatchTooLarge,
    );
    // 66 — amount above the ceiling.
    assert_rejected(
        &env,
        &contract,
        "record above the default ceiling",
        client.try_record_sme_collateral_commitment(&asset, &(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceeded,
    );
    // 65 / 67 — setter bounds.
    assert_rejected(
        &env,
        &contract,
        "set a zero ceiling",
        client.try_set_collateral_limit(&0i128),
        EscrowError::CollateralLimitNotPositive,
    );
    assert_rejected(
        &env,
        &contract,
        "set a ceiling above the max",
        client.try_set_collateral_limit(&(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceedsMax,
    );
    // 169 — clearing a record that was never set.
    assert_rejected(
        &env,
        &contract,
        "clear an unset record",
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );

    // 62 — a backwards timestamp, which needs a stored record and a rewound clock.
    set_timestamp(&env, 1_000);
    client.record_sme_collateral_commitment(&asset, &10i128);
    set_timestamp(&env, 999);
    assert_rejected(
        &env,
        &contract,
        "record with a rewound clock",
        client.try_record_sme_collateral_commitment(&asset, &20i128),
        EscrowError::CollateralTimestampBackwards,
    );
}

// ---------------------------------------------------------------------------
// C2 — event routing symbols and payload schemas are frozen
// ---------------------------------------------------------------------------

/// The `coll_rec` event: routing symbol at topic 1, `invoice_id` carried as a *body* field, and
/// exactly the three documented payload keys.
#[test]
fn record_event_schema_is_frozen() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");
    let invoice_id = client.get_escrow().invoice_id;

    client.record_sme_collateral_commitment(&asset, &500i128);

    let published = last_invocation_events(&env, &contract);
    assert_eq!(
        published.len(),
        1,
        "a record must publish exactly one event"
    );
    let event = &published[0];

    let topics = event_topics(&env, event);
    assert_eq!(
        topics.len(),
        2,
        "coll_rec carries only the routing symbol as a topic"
    );
    assert_eq!(event_routing(&env, event), Some(symbol_short!("coll_rec")));
    assert_eq!(
        event_field_keys(&env, event),
        std::vec![
            Symbol::new(&env, "amount"),
            Symbol::new(&env, "invoice_id"),
            Symbol::new(&env, "prior_amount"),
        ]
    );
    assert_eq!(
        event_field_i128(&env, event, Symbol::new(&env, "amount")),
        Some(500i128)
    );
    assert_eq!(
        event_field_i128(&env, event, Symbol::new(&env, "prior_amount")),
        Some(0i128),
        "the first record of a lifetime must report a prior amount of 0"
    );
    assert_eq!(
        event_field_symbol(&env, event, Symbol::new(&env, "invoice_id")),
        Some(invoice_id)
    );
}

/// The `coll_lim` event: routing symbol plus `invoice_id` as topics, and the two-sided limit
/// payload that lets an indexer reconstruct the ceiling's history.
#[test]
fn limit_event_schema_is_frozen() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id;

    client.set_collateral_limit(&1_000i128);

    let published = last_invocation_events(&env, &contract);
    assert_eq!(
        published.len(),
        1,
        "a successful setter call must publish exactly one event"
    );
    let event = &published[0];

    let topics = event_topics(&env, event);
    assert_eq!(
        topics.len(),
        3,
        "coll_lim carries the routing symbol and invoice_id as topics"
    );
    assert_eq!(event_routing(&env, event), Some(symbol_short!("coll_lim")));
    assert_eq!(topics[2], invoice_id);
    assert_eq!(
        event_field_keys(&env, event),
        std::vec![
            Symbol::new(&env, "new_limit"),
            Symbol::new(&env, "old_limit"),
        ]
    );
    assert_eq!(
        event_field_i128(&env, event, Symbol::new(&env, "old_limit")),
        Some(MAX_INVOICE_AMOUNT),
        "a legacy instance reports the additive-key default as the old limit"
    );
    assert_eq!(
        event_field_i128(&env, event, Symbol::new(&env, "new_limit")),
        Some(1_000i128)
    );
}

/// The `coll_clr` event: the removal-side counterpart, carrying the record it retired so an
/// indexer can replay a retirement without polling storage after the mutation.
#[test]
fn clear_event_schema_is_frozen() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");
    let invoice_id = client.get_escrow().invoice_id;

    set_timestamp(&env, 4_242);
    client.record_sme_collateral_commitment(&asset, &500i128);
    client.clear_sme_collateral_commitment();

    let published = last_invocation_events(&env, &contract);
    assert_eq!(published.len(), 1, "a clear must publish exactly one event");
    let event = &published[0];

    let topics = event_topics(&env, event);
    assert_eq!(
        topics.len(),
        3,
        "coll_clr carries the routing symbol and invoice_id as topics"
    );
    assert_eq!(event_routing(&env, event), Some(symbol_short!("coll_clr")));
    assert_eq!(topics[2], invoice_id);
    assert_eq!(
        event_field_keys(&env, event),
        std::vec![
            Symbol::new(&env, "amount"),
            Symbol::new(&env, "asset"),
            Symbol::new(&env, "recorded_at"),
        ]
    );
    assert_eq!(
        event_field_i128(&env, event, Symbol::new(&env, "amount")),
        Some(500i128)
    );
    assert_eq!(
        event_field_symbol(&env, event, Symbol::new(&env, "asset")),
        Some(asset)
    );
    assert_eq!(
        event_field_u64(&env, event, Symbol::new(&env, "recorded_at")),
        Some(4_242u64),
        "coll_clr must copy the original record's timestamp, not the clear time"
    );
}

/// The SDK event structs themselves are part of the public surface: a caller that deserializes
/// the published XDR into these types must get an exact match.
#[test]
fn collateral_events_deserialize_into_the_published_structs() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");
    let invoice_id = client.get_escrow().invoice_id;

    client.set_collateral_limit(&2_500i128);
    let limit_published = env.events().all().events().last().cloned();
    client.record_sme_collateral_commitment(&asset, &2_500i128);
    let record_published = env.events().all().events().last().cloned();
    client.clear_sme_collateral_commitment();
    let clear_published = env.events().all().events().last().cloned();

    assert_eq!(
        limit_published,
        Some(
            CollateralLimitUpdated {
                name: symbol_short!("coll_lim"),
                invoice_id: invoice_id.clone(),
                old_limit: MAX_INVOICE_AMOUNT,
                new_limit: 2_500i128,
            }
            .to_xdr(&env, &contract)
        ),
        "coll_lim must serialize exactly as CollateralLimitUpdated"
    );
    assert_eq!(
        record_published,
        Some(
            CollateralRecordedEvt {
                name: symbol_short!("coll_rec"),
                invoice_id: invoice_id.clone(),
                amount: 2_500i128,
                prior_amount: 0i128,
            }
            .to_xdr(&env, &contract)
        ),
        "coll_rec must serialize exactly as CollateralRecordedEvt"
    );
    assert_eq!(
        clear_published,
        Some(
            CollateralClearedEvt {
                name: symbol_short!("coll_clr"),
                invoice_id,
                asset,
                amount: 2_500i128,
                recorded_at: 0u64,
            }
            .to_xdr(&env, &contract)
        ),
        "coll_clr must serialize exactly as CollateralClearedEvt"
    );
}

/// A batch of `n` accepted items publishes exactly `n` `coll_rec` events, so an indexer replaying
/// the log reconstructs the same history the contract stored.
#[test]
fn accepted_batch_publishes_one_record_event_per_item() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let usdc = sym(&env, "USDC");
    let eur = sym(&env, "EUR");

    let items = SorobanVec::from_array(
        &env,
        [
            (usdc.clone(), 100i128),
            (eur.clone(), 200i128),
            (usdc.clone(), 300i128),
        ],
    );
    let stored = client.batch_record_collateral(&items);

    assert_eq!(stored.amount, 300i128);
    assert_eq!(stored.asset, usdc);
    let published = last_invocation_events(&env, &contract);
    assert_eq!(
        published.len(),
        3,
        "each accepted batch item must publish one coll_rec event"
    );
    assert_eq!(
        count_routing_events(&env, &contract, symbol_short!("coll_rec")),
        3
    );
    // `prior_amount` chains item to item, so the log alone tells the batch's own history.
    let keys = Symbol::new(&env, "prior_amount");
    let chain: std::vec::Vec<i128> = published
        .iter()
        .map(|event| event_field_i128(&env, event, keys.clone()).unwrap())
        .collect();
    assert_eq!(chain, std::vec![0i128, 100i128, 200i128]);
}

// ---------------------------------------------------------------------------
// C3 — public constants are frozen
// ---------------------------------------------------------------------------

/// `MAX_COLLATERAL_BATCH` and `MAX_INVOICE_AMOUNT` are part of the contract: a client that
/// pre-sizes a batch vector or clamps an amount locally must see the same bounds the contract
/// enforces at the edges.
#[test]
fn collateral_public_constants_are_frozen() {
    assert_eq!(MAX_COLLATERAL_BATCH, 50u32);
    assert_eq!(MAX_INVOICE_AMOUNT, i128::MAX / 10_000);

    // The two constants are the bounds the contract actually enforces, so drive them through the
    // contract rather than trusting the declarations: a client that pre-sizes a batch to the cap
    // must be able to submit it, and a client that clamps an amount to the cap must be accepted.
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");

    let at_cap = repeated_items(&env, &asset, 1i128, MAX_COLLATERAL_BATCH);
    assert_eq!(at_cap.len(), MAX_COLLATERAL_BATCH);
    assert!(client.try_batch_record_collateral(&at_cap).is_ok());

    let at_ceiling = client.try_record_sme_collateral_commitment(&asset, &MAX_INVOICE_AMOUNT);
    let published = last_invocation_events(&env, &contract);
    assert!(
        at_ceiling.is_ok(),
        "the default ceiling must admit the historical maximum commitment"
    );
    assert_eq!(
        published.len(),
        1,
        "an accepted record must publish its event"
    );
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

// ---------------------------------------------------------------------------
// C4 — the additive-key default is frozen for legacy instances
// ---------------------------------------------------------------------------

/// An instance that never called the setter leaves the key absent, reads the default ceiling, and
/// accepts exactly the commitment it accepted before the setter existed.
#[test]
fn legacy_instance_without_the_ceiling_key_keeps_the_default() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);

    let before = probe(&env, &client);
    assert_eq!(
        before.limit_raw, None,
        "the additive key must stay absent until the setter is called"
    );
    assert_eq!(before.view_limit, MAX_INVOICE_AMOUNT);
    assert_eq!(before.snapshot_limit, MAX_INVOICE_AMOUNT);
    assert_eq!(before.view_record, None);
    assert_eq!(before.snapshot_record, CollateralCommitmentSnapshot::None);

    let asset = sym(&env, "USDC");
    let stored = client.record_sme_collateral_commitment(&asset, &MAX_INVOICE_AMOUNT);
    assert_eq!(stored.amount, MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

/// A legacy instance also accepts a full-length batch, because the batch bound is independent of
/// the ceiling and neither key was ever written.
#[test]
fn legacy_instance_accepts_a_full_length_batch() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");

    let stored =
        client.batch_record_collateral(&repeated_items(&env, &asset, 1i128, MAX_COLLATERAL_BATCH));
    assert_eq!(stored.amount, 1i128);
    assert_eq!(stored.recorded_at, 0);
    assert_eq!(
        count_routing_events(&env, &contract, symbol_short!("coll_rec")),
        MAX_COLLATERAL_BATCH as usize
    );
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

/// Writing the ceiling to its default value is observationally transparent through every read
/// surface, so a legacy reader cannot tell the key exists.
#[test]
fn setting_the_ceiling_to_the_default_value_is_observationally_transparent() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let before = probe(&env, &client);

    client.set_collateral_limit(&MAX_INVOICE_AMOUNT);
    let after = probe(&env, &client);

    assert_eq!(after.view_limit, before.view_limit);
    assert_eq!(after.snapshot_limit, before.snapshot_limit);
    assert_eq!(
        after.limit_raw,
        Some(MAX_INVOICE_AMOUNT),
        "the setter must materialize the key it was given"
    );
}

// ---------------------------------------------------------------------------
// C5 — guard ordering is frozen
// ---------------------------------------------------------------------------

/// `record_sme_collateral_commitment` validates the payload **before** it loads the escrow and
/// requires the SME signature, so a malformed payload yields a typed validation code even on an
/// instance that was never initialized.
#[test]
fn record_validates_before_authorization_and_initialization() {
    let env = Env::default();
    let client = deploy(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");
    let empty = sym(&env, "");

    assert_rejected(
        &env,
        &contract,
        "uninitialized: zero amount",
        client.try_record_sme_collateral_commitment(&asset, &0i128),
        EscrowError::CollateralAmountNotPositive,
    );
    assert_rejected(
        &env,
        &contract,
        "uninitialized: empty asset",
        client.try_record_sme_collateral_commitment(&empty, &1i128),
        EscrowError::CollateralAssetEmpty,
    );
    assert_rejected(
        &env,
        &contract,
        "uninitialized: above ceiling",
        client.try_record_sme_collateral_commitment(&asset, &(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceeded,
    );
    // A *well-formed* payload on the same uninitialized instance gets as far as the escrow load.
    assert_rejected(
        &env,
        &contract,
        "uninitialized: well-formed payload",
        client.try_record_sme_collateral_commitment(&asset, &1i128),
        EscrowError::EscrowNotInitialized,
    );
}

/// `batch_record_collateral` follows the same rule, after its two size guards: an empty vector
/// and an over-long vector are both rejected before any item is inspected.
#[test]
fn batch_validates_size_before_items_and_authorization() {
    let env = Env::default();
    let client = deploy(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");
    let empty = sym(&env, "");

    assert_rejected(
        &env,
        &contract,
        "uninitialized: empty batch",
        client.try_batch_record_collateral(&SorobanVec::new(&env)),
        EscrowError::CollateralBatchEmpty,
    );
    assert_rejected(
        &env,
        &contract,
        "uninitialized: oversized batch of zero amounts",
        client.try_batch_record_collateral(&repeated_items(
            &env,
            &asset,
            0i128,
            MAX_COLLATERAL_BATCH + 1,
        )),
        EscrowError::CollateralBatchTooLarge,
    );
    // Size is checked before item validity: an over-long batch of wholly invalid items still
    // reports the size, so the reported code never depends on how far the loop would have got.
    assert_rejected(
        &env,
        &contract,
        "uninitialized: oversized batch of empty assets",
        client.try_batch_record_collateral(&repeated_items(
            &env,
            &empty,
            i128::MIN,
            MAX_COLLATERAL_BATCH + 1,
        )),
        EscrowError::CollateralBatchTooLarge,
    );
    // A well-formed, correctly-sized batch reaches the escrow load.
    assert_rejected(
        &env,
        &contract,
        "uninitialized: well-formed single-item batch",
        client.try_batch_record_collateral(&repeated_items(&env, &asset, 1i128, 1)),
        EscrowError::EscrowNotInitialized,
    );
}

/// `clear_sme_collateral_commitment` checks existence **before** authorization (ADR-002), so a
/// legacy caller clearing a record that was never set gets the typed `NoCollateralToClear` rather
/// than an auth failure.
#[test]
fn clear_checks_existence_before_authorization() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();

    assert_rejected(
        &env,
        &contract,
        "clear with nothing recorded",
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );
}

/// `set_collateral_limit` is the mirror image: it authorizes **before** it validates, so an
/// out-of-range limit from a non-admin caller is an auth failure, not a code that would let an
/// unauthorized caller probe the accepted range.
#[test]
fn setter_authorizes_before_validating() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let outsider = Address::generate(&env);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &outsider,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract,
            fn_name: "set_collateral_limit",
            args: SorobanVec::from_array(&env, [0i128.into_val(&env)]),
            sub_invokes: &[],
        },
    }]);

    assert_rejected_without_contract_code(
        &env,
        &contract,
        "set_collateral_limit with a malformed limit from a non-admin",
        client.try_set_collateral_limit(&0i128),
    );
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

/// Within a single call the payload guards are ordered `amount > 0`, then non-empty asset, then
/// the ceiling. An input that violates several at once always reports the first.
#[test]
fn payload_guard_precedence_is_deterministic() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");
    let empty = sym(&env, "");
    client.set_collateral_limit(&1_000i128);
    let before = probe(&env, &client);

    for sweep in 0..3 {
        // Zero amount + empty asset + above ceiling: the amount guard is reported.
        assert_rejected(
            &env,
            &contract,
            &format!("sweep {sweep}: zero amount and empty asset"),
            client.try_record_sme_collateral_commitment(&empty, &0i128),
            EscrowError::CollateralAmountNotPositive,
        );
        // Empty asset + above ceiling: the asset guard is reported.
        assert_rejected(
            &env,
            &contract,
            &format!("sweep {sweep}: empty asset above ceiling"),
            client.try_record_sme_collateral_commitment(&empty, &i128::MAX),
            EscrowError::CollateralAssetEmpty,
        );
        // Non-positive amount + above ceiling: the amount guard is reported.
        assert_rejected(
            &env,
            &contract,
            &format!("sweep {sweep}: negative amount above ceiling"),
            client.try_record_sme_collateral_commitment(&asset, &(-5i128)),
            EscrowError::CollateralAmountNotPositive,
        );
    }

    assert_probe_unchanged("precedence sweep", &before, &probe(&env, &client));
}

// ---------------------------------------------------------------------------
// C6 — authorization failures stay outside the contract-code space
// ---------------------------------------------------------------------------

/// A caller that is not the configured SME is rejected with a host-level auth error, never a typed
/// `EscrowError`, and leaves the record untouched. This is what lets a legacy caller distinguish
/// "fix the payload" from "fix the signature".
#[test]
fn non_sme_callers_get_auth_failures_not_contract_codes() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");

    client.record_sme_collateral_commitment(&asset, &10i128);
    let before = probe(&env, &client);

    let outsider = Address::generate(&env);
    for (label, fn_name) in [
        ("record", "record_sme_collateral_commitment"),
        ("batch_record_collateral", "batch_record_collateral"),
        (
            "clear_sme_collateral_commitment",
            "clear_sme_collateral_commitment",
        ),
    ] {
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &outsider,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract,
                fn_name,
                args: SorobanVec::new(&env),
                sub_invokes: &[],
            },
        }]);

        match label {
            "record" => assert_rejected_without_contract_code(
                &env,
                &contract,
                "record by a non-SME caller",
                client.try_record_sme_collateral_commitment(&asset, &1i128),
            ),
            "batch_record_collateral" => assert_rejected_without_contract_code(
                &env,
                &contract,
                "batch by a non-SME caller",
                client.try_batch_record_collateral(&SorobanVec::from_array(
                    &env,
                    [(asset.clone(), 1i128)],
                )),
            ),
            _ => assert_rejected_without_contract_code(
                &env,
                &contract,
                "clear by a non-SME caller",
                client.try_clear_sme_collateral_commitment(),
            ),
        }
        env.mock_all_auths();
    }

    assert_probe_unchanged("non-SME write attempts", &before, &probe(&env, &client));
}

// ---------------------------------------------------------------------------
// B1 — amount floor
// ---------------------------------------------------------------------------

/// `amount = 1` is the smallest accepted commitment and round-trips exactly.
#[test]
fn smallest_positive_amount_is_accepted() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let asset = sym(&env, "USDC");

    let stored = client.record_sme_collateral_commitment(&asset, &1i128);
    assert_eq!(stored.amount, 1i128);
    assert_eq!(stored.recorded_at, 0);
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        1i128
    );
}

/// Every non-positive amount, down to `i128::MIN`, is rejected with the same code and mutates
/// nothing — on both the single and the batch entrypoint.
#[test]
fn non_positive_amount_matrix_is_typed_and_inert() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");

    // Seed a record so "mutates nothing" is observable.
    client.record_sme_collateral_commitment(&asset, &42i128);
    let before = probe(&env, &client);

    for bad in [0i128, -1i128, -10_000i128, i128::MIN] {
        assert_rejected(
            &env,
            &contract,
            &format!("record with amount {bad}"),
            client.try_record_sme_collateral_commitment(&asset, &bad),
            EscrowError::CollateralAmountNotPositive,
        );
        assert_probe_unchanged(
            &format!("record with amount {bad}"),
            &before,
            &probe(&env, &client),
        );

        // The batch entrypoint reports the same code for the same value, wherever the bad item
        // sits in the vector.
        for (label, items) in [
            (
                "leading",
                SorobanVec::from_array(&env, [(asset.clone(), bad), (asset.clone(), 10i128)]),
            ),
            (
                "trailing",
                SorobanVec::from_array(&env, [(asset.clone(), 10i128), (asset.clone(), bad)]),
            ),
        ] {
            assert_rejected(
                &env,
                &contract,
                &format!("batch with a {label} amount of {bad}"),
                client.try_batch_record_collateral(&items),
                EscrowError::CollateralAmountNotPositive,
            );
            assert_probe_unchanged(
                &format!("batch with a {label} amount of {bad}"),
                &before,
                &probe(&env, &client),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// B2 — amount ceiling
// ---------------------------------------------------------------------------

/// With the default ceiling, the historical maximum is accepted; one stroop above is rejected.
#[test]
fn default_ceiling_edge_is_at_max_invoice_amount() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");

    assert_rejected(
        &env,
        &contract,
        "one stroop above the default ceiling",
        client.try_record_sme_collateral_commitment(&asset, &(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceeded,
    );
    let stored = client.record_sme_collateral_commitment(&asset, &MAX_INVOICE_AMOUNT);
    assert_eq!(stored.amount, MAX_INVOICE_AMOUNT);
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        MAX_INVOICE_AMOUNT
    );
}

/// With a configured ceiling, `amount == limit` is accepted and `limit + 1` is rejected — at both
/// the tightest and the widest ceiling the contract can express.
#[test]
fn configured_ceiling_edges_are_inclusive() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");

    // Tightest accepted ceiling: 1.
    client.set_collateral_limit(&1i128);
    assert_rejected(
        &env,
        &contract,
        "one stroop above the tightest ceiling",
        client.try_record_sme_collateral_commitment(&asset, &2i128),
        EscrowError::CollateralLimitExceeded,
    );
    let stored = client.record_sme_collateral_commitment(&asset, &1i128);
    assert_eq!(stored.amount, 1i128);

    // Widest accepted ceiling.
    client.set_collateral_limit(&MAX_INVOICE_AMOUNT);
    assert_rejected(
        &env,
        &contract,
        "one stroop above the widest ceiling",
        client.try_record_sme_collateral_commitment(&asset, &(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceeded,
    );
    let stored = client.record_sme_collateral_commitment(&asset, &MAX_INVOICE_AMOUNT);
    assert_eq!(stored.amount, MAX_INVOICE_AMOUNT);
}

/// A ceiling tightened below an existing commitment is non-retroactive: the stored record stays
/// readable, and only new writes are gated.
#[test]
fn tightened_ceiling_preserves_an_existing_over_limit_record() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");

    client.set_collateral_limit(&10_000i128);
    client.record_sme_collateral_commitment(&asset, &9_000i128);

    client.set_collateral_limit(&100i128);
    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, 9_000i128);
    assert_rejected(
        &env,
        &contract,
        "replacement above a tightened ceiling",
        client.try_record_sme_collateral_commitment(&asset, &101i128),
        EscrowError::CollateralLimitExceeded,
    );
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        9_000i128,
        "a rejected replacement must leave the over-limit record in place"
    );
}

// ---------------------------------------------------------------------------
// B3 — batch length edges
// ---------------------------------------------------------------------------

/// Batch length `1` is the smallest accepted batch, and behaves like the single entrypoint.
#[test]
fn single_item_batch_is_accepted() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");

    let stored =
        client.batch_record_collateral(&SorobanVec::from_array(&env, [(asset.clone(), 7i128)]));
    assert_eq!(stored.amount, 7i128);
    assert_eq!(stored.asset, asset);
    assert_eq!(
        count_routing_events(&env, &contract, symbol_short!("coll_rec")),
        1
    );
    assert_eq!(client.get_sme_collateral_commitment().unwrap(), stored);
}

/// Batch length `MAX_COLLATERAL_BATCH` is accepted; `MAX_COLLATERAL_BATCH + 1` is rejected and
/// publishes nothing.
#[test]
fn batch_length_edges_are_inclusive() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");

    let stored =
        client.batch_record_collateral(&repeated_items(&env, &asset, 1i128, MAX_COLLATERAL_BATCH));
    assert_eq!(stored.amount, 1i128);
    assert_eq!(
        count_routing_events(&env, &contract, symbol_short!("coll_rec")),
        MAX_COLLATERAL_BATCH as usize
    );

    let before = probe(&env, &client);
    assert_rejected(
        &env,
        &contract,
        "one item past the batch cap",
        client.try_batch_record_collateral(&repeated_items(
            &env,
            &asset,
            1i128,
            MAX_COLLATERAL_BATCH + 1,
        )),
        EscrowError::CollateralBatchTooLarge,
    );
    assert_probe_unchanged("oversized batch", &before, &probe(&env, &client));
}

/// An empty batch is rejected as empty, and the rejection is inert.
#[test]
fn empty_batch_is_rejected_and_inert() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");
    client.record_sme_collateral_commitment(&asset, &42i128);
    let before = probe(&env, &client);

    assert_rejected(
        &env,
        &contract,
        "empty batch",
        client.try_batch_record_collateral(&SorobanVec::new(&env)),
        EscrowError::CollateralBatchEmpty,
    );
    assert_probe_unchanged("empty batch", &before, &probe(&env, &client));
}

// ---------------------------------------------------------------------------
// B4 — batch atomicity at the edges
// ---------------------------------------------------------------------------

/// An invalid item voids the whole batch from any position, including the first and the last slot
/// of a full-length batch. Nothing is written and nothing is published.
#[test]
fn invalid_item_voids_the_batch_at_any_position() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");
    let empty = sym(&env, "");

    client.set_collateral_limit(&1_000i128);
    client.record_sme_collateral_commitment(&asset, &42i128);
    // The seed record is the one accepted write, and it published exactly one event. Count it
    // before any other invocation replaces the "last invocation" view.
    let mut accepted_events = count_routing_events(&env, &contract, symbol_short!("coll_rec"));
    assert_eq!(accepted_events, 1);
    let before = probe(&env, &client);

    // First slot, middle slot, and last slot of a full-length batch.
    for position in [0u32, MAX_COLLATERAL_BATCH / 2, MAX_COLLATERAL_BATCH - 1] {
        let items = repeated_items(&env, &asset, 10i128, MAX_COLLATERAL_BATCH);
        let mut raw: std::vec::Vec<(Symbol, i128)> = (0..MAX_COLLATERAL_BATCH)
            .map(|i| items.get(i).unwrap())
            .collect();
        raw[position as usize] = (empty.clone(), 10i128);
        let patched = SorobanVec::from_slice(&env, &raw);

        assert_rejected(
            &env,
            &contract,
            &format!("full batch with an empty asset at position {position}"),
            client.try_batch_record_collateral(&patched),
            EscrowError::CollateralAssetEmpty,
        );
        assert_probe_unchanged(
            &format!("full batch with an empty asset at position {position}"),
            &before,
            &probe(&env, &client),
        );
        accepted_events += count_routing_events(&env, &contract, symbol_short!("coll_rec"));
        assert_eq!(
            accepted_events, 1,
            "a failed full-length batch must not add to the audit trail"
        );
    }

    // A one-item batch with an over-ceiling amount is rejected as a whole.
    assert_rejected(
        &env,
        &contract,
        "over-ceiling single-item batch",
        client.try_batch_record_collateral(&SorobanVec::from_array(
            &env,
            [(asset.clone(), 1_001i128)],
        )),
        EscrowError::CollateralLimitExceeded,
    );
    assert_probe_unchanged(
        "over-ceiling single-item batch",
        &before,
        &probe(&env, &client),
    );
}

// ---------------------------------------------------------------------------
// B5 — empty payload
// ---------------------------------------------------------------------------

/// An empty asset symbol is rejected on its own, alongside every accepted amount class, and the
/// stored record is untouched.
#[test]
fn empty_asset_symbol_is_rejected_for_every_amount_class() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");
    let empty = sym(&env, "");

    client.set_collateral_limit(&1_000i128);
    client.record_sme_collateral_commitment(&asset, &42i128);
    let before = probe(&env, &client);

    for amount in [1i128, 500i128, 1_000i128, MAX_INVOICE_AMOUNT] {
        assert_rejected(
            &env,
            &contract,
            &format!("record with an empty asset and amount {amount}"),
            client.try_record_sme_collateral_commitment(&empty, &amount),
            EscrowError::CollateralAssetEmpty,
        );
        assert_probe_unchanged(
            &format!("record with an empty asset and amount {amount}"),
            &before,
            &probe(&env, &client),
        );
    }
}

/// A cleared record is `None` again, and the key is removed rather than left materialized, so a
/// repeated clear/record cycle returns the surface to exactly its starting shape — an indexer
/// replaying `coll_rec` / `coll_clr` always sees a consistent lifecycle.
#[test]
fn clear_then_record_cycle_returns_to_the_starting_shape() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");

    let start = probe(&env, &client);
    assert_eq!(start.view_record, None);
    assert!(
        !start.pledge_present,
        "an unset escrow must not materialize the key"
    );

    let mut recorded_events = 0usize;
    let mut cleared_events = 0usize;
    for _ in 0..2 {
        client.record_sme_collateral_commitment(&asset, &900i128);
        recorded_events += count_routing_events(&env, &contract, symbol_short!("coll_rec"));
        let set = probe(&env, &client);
        assert!(set.pledge_present);
        assert_eq!(set.view_record.unwrap().amount, 900i128);

        client.clear_sme_collateral_commitment();
        cleared_events += count_routing_events(&env, &contract, symbol_short!("coll_clr"));
        let cleared = probe(&env, &client);
        assert_eq!(cleared.view_record, None);
        assert_eq!(cleared.snapshot_record, CollateralCommitmentSnapshot::None);
        assert!(!cleared.pledge_present);
    }

    assert_eq!(
        recorded_events, 2,
        "each recorded cycle must publish one coll_rec event"
    );
    assert_eq!(
        cleared_events, 2,
        "each clear must publish one coll_clr event"
    );

    let end = probe(&env, &client);
    assert_eq!(end.view_limit, start.view_limit);
    assert_eq!(end.limit_raw, start.limit_raw);
}

// ---------------------------------------------------------------------------
// B6 — timestamp edges
// ---------------------------------------------------------------------------

/// `recorded_at = 0` (the ledger default) round-trips exactly, and a replacement at the same
/// timestamp is accepted — a legacy caller that records twice within one ledger cannot be blocked
/// by the monotonicity guard.
#[test]
fn timestamp_floor_and_equal_timestamp_are_accepted() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let asset = sym(&env, "USDC");

    assert_eq!(env.ledger().timestamp(), 0);
    let first = client.record_sme_collateral_commitment(&asset, &10i128);
    assert_eq!(first.recorded_at, 0);

    let second = client.record_sme_collateral_commitment(&asset, &20i128);
    assert_eq!(second.recorded_at, 0);
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        20i128
    );
}

/// A replacement one second in the past is rejected on both entrypoints, and the newer record
/// survives untouched.
#[test]
fn timestamp_one_second_backwards_is_rejected() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");

    set_timestamp(&env, 1_000);
    client.record_sme_collateral_commitment(&asset, &10i128);
    let before = probe(&env, &client);

    for rewound in [999u64, 0u64] {
        set_timestamp(&env, rewound);
        assert_rejected(
            &env,
            &contract,
            &format!("record with timestamp {rewound}"),
            client.try_record_sme_collateral_commitment(&asset, &20i128),
            EscrowError::CollateralTimestampBackwards,
        );
        assert_probe_unchanged(
            &format!("record with timestamp {rewound}"),
            &before,
            &probe(&env, &client),
        );
    }

    // The same guard covers the batch entrypoint.
    set_timestamp(&env, 500);
    assert_rejected(
        &env,
        &contract,
        "batch with a rewound clock",
        client
            .try_batch_record_collateral(&SorobanVec::from_array(&env, [(asset.clone(), 20i128)])),
        EscrowError::CollateralTimestampBackwards,
    );
    assert_probe_unchanged("batch with a rewound clock", &before, &probe(&env, &client));
}

/// The largest representable timestamp round-trips through the record without truncation.
#[test]
fn maximum_ledger_timestamp_round_trips() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let asset = sym(&env, "USDC");

    set_timestamp(&env, u64::MAX);
    let stored = client.record_sme_collateral_commitment(&asset, &10i128);
    assert_eq!(stored.recorded_at, u64::MAX);
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().recorded_at,
        u64::MAX
    );
}

// ---------------------------------------------------------------------------
// B7 / B8 — rejected calls are inert, and the matrices are deterministic
// ---------------------------------------------------------------------------

/// Walks the whole amount/ceiling matrix twice on the same instance and asserts that every
/// position is classified identically, with the same typed code, and that the accepted positions
/// leave the record reproducible.
#[test]
fn the_boundary_matrix_is_deterministic_across_repeated_sweeps() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");
    client.set_collateral_limit(&1_000i128);

    // (amount, expected code). `None` means accepted.
    let matrix: std::vec::Vec<(i128, Option<EscrowError>)> = std::vec![
        (i128::MIN, Some(EscrowError::CollateralAmountNotPositive)),
        (-1i128, Some(EscrowError::CollateralAmountNotPositive)),
        (0i128, Some(EscrowError::CollateralAmountNotPositive)),
        (1i128, None),
        (2i128, None),
        (999i128, None),
        (1_000i128, None),
        (1_001i128, Some(EscrowError::CollateralLimitExceeded)),
        (i128::MAX, Some(EscrowError::CollateralLimitExceeded)),
    ];

    for (sweep, (amount, expected)) in matrix.iter().enumerate().cycle().take(2 * matrix.len()) {
        let label = format!("sweep {sweep}, amount {amount}");
        let before = probe(&env, &client);
        let result = client.try_record_sme_collateral_commitment(&asset, amount);
        match expected {
            Some(code) => {
                assert_rejected(&env, &contract, &label, result, *code);
                assert_probe_unchanged(&label, &before, &probe(&env, &client));
            }
            None => assert!(
                result.is_ok(),
                "{label}: {amount} is inside the ceiling and must be accepted"
            ),
        }
    }

    // The final accepted value is the same on both sweeps.
    assert_eq!(
        client.get_sme_collateral_commitment().unwrap().amount,
        1_000i128
    );
}

/// The batch length matrix is classified identically on every sweep, at 0, 1, 2, the cap, and one
/// past it.
#[test]
fn the_batch_length_matrix_is_deterministic() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");

    let matrix: std::vec::Vec<(u32, Option<EscrowError>)> = std::vec![
        (0, Some(EscrowError::CollateralBatchEmpty)),
        (1, None),
        (2, None),
        (MAX_COLLATERAL_BATCH, None),
        (
            MAX_COLLATERAL_BATCH + 1,
            Some(EscrowError::CollateralBatchTooLarge)
        ),
    ];

    for sweep in 0..2 {
        for (len, expected) in &matrix {
            let label = format!("sweep {sweep}, batch length {len}");
            let result =
                client.try_batch_record_collateral(&repeated_items(&env, &asset, 1i128, *len));
            match expected {
                Some(code) => {
                    assert_rejected(&env, &contract, &label, result, *code);
                }
                None => assert!(result.is_ok(), "{label}: length {len} must be accepted"),
            }
        }
    }
}

/// An arbitrarily long run of rejected calls leaves the observable state byte-identical and adds
/// nothing to the audit trail: recovery is "retry with a valid payload", and nothing else.
#[test]
fn any_number_of_rejected_attempts_leaves_state_and_audit_trail_unchanged() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_initialized(&env);
    let contract = client.address.clone();
    let asset = sym(&env, "USDC");
    let empty = sym(&env, "");

    client.set_collateral_limit(&1_000i128);
    client.record_sme_collateral_commitment(&asset, &500i128);
    let before = probe(&env, &client);

    for round in 0..5i128 {
        let non_positive = -round - 1;
        let over_ceiling = 1_000i128 + round + 1;
        assert_rejected(
            &env,
            &contract,
            &format!("round {round}: non-positive amount"),
            client.try_record_sme_collateral_commitment(&asset, &non_positive),
            EscrowError::CollateralAmountNotPositive,
        );
        assert_rejected(
            &env,
            &contract,
            &format!("round {round}: empty asset"),
            client.try_record_sme_collateral_commitment(&empty, &500i128),
            EscrowError::CollateralAssetEmpty,
        );
        assert_rejected(
            &env,
            &contract,
            &format!("round {round}: above ceiling"),
            client.try_record_sme_collateral_commitment(&asset, &over_ceiling),
            EscrowError::CollateralLimitExceeded,
        );
        assert_rejected(
            &env,
            &contract,
            &format!("round {round}: empty batch"),
            client.try_batch_record_collateral(&SorobanVec::new(&env)),
            EscrowError::CollateralBatchEmpty,
        );
        assert_rejected(
            &env,
            &contract,
            &format!("round {round}: oversized batch"),
            client.try_batch_record_collateral(&repeated_items(
                &env,
                &asset,
                1i128,
                MAX_COLLATERAL_BATCH + 1,
            )),
            EscrowError::CollateralBatchTooLarge,
        );
        assert_rejected(
            &env,
            &contract,
            &format!("round {round}: zero ceiling"),
            client.try_set_collateral_limit(&0i128),
            EscrowError::CollateralLimitNotPositive,
        );
        assert_rejected(
            &env,
            &contract,
            &format!("round {round}: ceiling above max"),
            client.try_set_collateral_limit(&(MAX_INVOICE_AMOUNT + 1)),
            EscrowError::CollateralLimitExceedsMax,
        );
        assert_probe_unchanged(
            &format!("rejection storm, round {round}"),
            &before,
            &probe(&env, &client),
        );
    }
}
