# SME Collateral Commitment Metadata

`record_sme_collateral_commitment(asset, amount)` in [`escrow/src/lib.rs`](../escrow/src/lib.rs) is a **metadata-only** Soroban escrow entrypoint. It allows the configured SME address to report collateral metadata for off-chain risk review, but it does **not** move, reserve, escrow, freeze, or verify any asset on-chain.

The companion entrypoint [`clear_sme_collateral_commitment`] allows the SME to retire a previously recorded commitment, removing it from storage. Like the record operation, clearing is metadata-only — no tokens are moved.

> [!CAUTION]
> **These entrypoints are metadata-only.** They write/remove metadata in contract instance storage and emit Soroban events. They do **not** act as enforced liens, asset custody mechanisms, or on-chain encumbrances. Historical records are not proof of locked assets and must never be treated as such.

---

## Entrypoints

### `record_sme_collateral_commitment(env, asset, amount) -> SmeCollateralCommitment`

Records or replaces an off-chain collateral pledge against the escrow's invoice.

- **Auth**: SME address (`sme_address` from the escrow record), enforced via `load_escrow_require_sme`.
- **Storage**: writes [`DataKey::SmeCollateralPledge`] (instance storage). Replaces any prior record atomically.
- **Event**: emits [`CollateralRecordedEvt`] with `invoice_id`, `amount`, and `prior_amount`.
- **Token movement**: none. This function performs no SEP-41 token operations.

### `get_sme_collateral_commitment(env) -> Option<SmeCollateralCommitment>`

Returns the current commitment, or `None` if none has been recorded.

- **Auth**: none required (read-only).

### `clear_sme_collateral_commitment(env)`

Retires a previously recorded commitment, removing it from storage.

- **Auth**: SME address (`sme_address` from the escrow record), enforced via `load_escrow_require_sme`.
- **Storage**: removes [`DataKey::SmeCollateralPledge`] (instance storage). Returns [`EscrowError::NoCollateralToClear`] if no commitment exists.
- **Events**: emits [`CollateralClearedEvt`] and [`CollateralCommitmentCleared`] carrying the cleared commitment's `asset`, `amount`, and `recorded_at`.
- **Token movement**: none.
- **Guard ordering** (ADR-002):
  1. Read-only existence check — returns `NoCollateralToClear` immediately if absent.
  2. `require_auth` on the SME address.
  3. Remove storage entry and emit events.

---

## On-chain Behavior — `record_sme_collateral_commitment`

### 1. Authorization (SME-only)

Only the configured SME address (`InvoiceEscrow::sme_address`) is authorized to call this entrypoint. The contract enforces this by calling `sme_address.require_auth()` via the internal helper `load_escrow_require_sme`.

```rust
fn load_escrow_require_sme(env: &Env) -> InvoiceEscrow {
    let escrow: InvoiceEscrow = env.storage().instance()
        .get(&DataKey::Escrow)
        .unwrap_or_else(|| fail(env, EscrowError::EscrowNotInitialized));
    escrow.sme_address.require_auth();
    escrow
}
```

### 2. Validation Rules

The contract validates inputs and state before recording:

| Rule | Condition | Error (code) |
|------|-----------|-------------|
| **Positive Amount** | `amount > 0` | [`EscrowError::CollateralAmountNotPositive`] (60) |
| **Non-empty Asset Symbol** | `asset != Symbol::new(&env, "")` | [`EscrowError::CollateralAssetEmpty`] (61) |
| **Monotonic Timestamp on Replacement** | `now >= prior_commitment.recorded_at` | [`EscrowError::CollateralTimestampBackwards`] (62) |

The monotonic-timestamp check acts as a defense-in-depth against stale out-of-order writes. When the SME replaces an existing commitment, the current ledger timestamp (`Env::ledger().timestamp()`) must not be earlier than the previously recorded timestamp.

### 3. Storage

The contract writes the metadata record under [`DataKey::SmeCollateralPledge`] in **instance** storage. This completely replaces any previously recorded commitment. No prior record is preserved — the replacement is atomic and the old data is overwritten.

The recorded data is represented by the [`SmeCollateralCommitment`] struct:

```rust
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SmeCollateralCommitment {
    pub asset: Symbol,      // Off-chain asset symbol (non-empty)
    pub amount: i128,       // Reported collateral amount (positive)
    pub recorded_at: u64,   // Soroban ledger timestamp at write time
}
```

| Field | Type | Description |
|-------|------|-------------|
| `asset` | `Symbol` | The off-chain asset identifier (e.g. `"USDC"`, `"GOLD"`). Must be non-empty. |
| `amount` | `i128` | The SME-reported collateral amount in the asset's native units. Must be positive. |
| `recorded_at` | `u64` | The Soroban ledger timestamp (seconds since Unix epoch) when this record was written. Used only for indexing and monotonicity validation. |

To retrieve the current record, external callers can use [`LiquifactEscrow::get_sme_collateral_commitment`]:

```rust
pub fn get_sme_collateral_commitment(env: Env) -> Option<SmeCollateralCommitment> {
    env.storage().instance().get(&DataKey::SmeCollateralPledge)
}
```

Returns `None` when no commitment has ever been recorded (or after [`clear_sme_collateral_commitment`] has been called).

The commitment is also included in [`EscrowSummary`] via `CollateralCommitmentSnapshot`, allowing indexers to fetch it alongside other escrow state in a single call.

### 4. Event Emission — `CollateralRecordedEvt`

The contract emits a [`CollateralRecordedEvt`] event upon successful execution:

```rust
#[contractevent]
pub struct CollateralRecordedEvt {
    #[topic]
    pub name: Symbol,           // Hardcoded: symbol_short!("coll_rec")
    pub invoice_id: Symbol,     // Invoice identifier of the escrow
    pub amount: i128,           // Newly recorded collateral amount
    pub prior_amount: i128,     // Previously recorded amount (0 if first record)
}
```

| Field | Type | Description |
|-------|------|-------------|
| `name` | `Symbol` | Hardcoded event name `coll_rec`. |
| `invoice_id` | `Symbol` | The invoice ID associated with the current escrow. |
| `amount` | `i128` | The newly recorded positive collateral amount. |
| `prior_amount` | `i128` | The previously recorded amount, or `0` if no prior commitment existed. |

The `prior_amount` field provides clear **replacement semantics** for off-chain indexers: a value of `0` indicates the first record, while a non-zero value signals that a prior record was overwritten. Indexers can detect replacement by observing consecutive `CollateralRecordedEvt` events for the same `invoice_id`.

The event **intentionally omits** token contract, custodian, and transfer-receipt fields so consumers do not treat it as an on-chain encumbrance.

### 5. Event Emission — `CollateralClearedEvt` and `CollateralCommitmentCleared`

[`clear_sme_collateral_commitment`] emits two events carrying the cleared commitment's data:

```rust
#[contractevent]
pub struct CollateralClearedEvt {
    #[topic]
    pub name: Symbol,           // Hardcoded: symbol_short!("coll_clr")
    pub invoice_id: Symbol,
    pub asset: Symbol,          // Carried from the pledge at time of removal
    pub amount: i128,           // Carried from the pledge at time of removal
    pub recorded_at: u64,       // Original pledge ledger timestamp
}

#[contractevent]
pub struct CollateralCommitmentCleared {
    #[topic]
    pub name: Symbol,           // Hardcoded: symbol_short!("coll_clr")
    pub invoice_id: Symbol,
    pub asset: Symbol,
    pub amount: i128,
    pub recorded_at: u64,
}
```

Both events are emitted on every successful clear. The `asset`, `amount`, and `recorded_at` fields are taken from the commitment at the time of removal for auditability.

> For the full event schema reference, see [`docs/EVENT_SCHEMA.md`](EVENT_SCHEMA.md).

### 6. Replacement Semantics

When the SME calls `record_sme_collateral_commitment` and a prior commitment already exists:

1. The prior commitment's `amount` is captured as `prior_amount` in the emitted event.
2. The new commitment **atomically replaces** the old one under `DataKey::SmeCollateralPledge`.
3. The `recorded_at` timestamp must be monotonically non-decreasing (`now >= prior.recorded_at`).
4. The prior record is **not archived** — only the current value is retrievable via `get_sme_collateral_commitment`.

This means:
- The **first** call emits `CollateralRecordedEvt { amount: X, prior_amount: 0 }`.
- A **replacement** call emits `CollateralRecordedEvt { amount: Y, prior_amount: X }`.
- There is no limit on how many times the commitment can be replaced.
- To remove the record entirely, use [`clear_sme_collateral_commitment`].

---

## Limitations & Contrast with On-chain Custody Flows

> [!WARNING]
> These entrypoints are **metadata-only**. They write/remove metadata in contract instance storage and emit Soroban events. They do **not** act as enforced liens or asset custody mechanisms.

To prevent integration risks, integrators must understand how this metadata-only flow contrasts with on-chain asset custody:

| Capability | `record_sme_collateral_commitment` / `clear_sme_collateral_commitment` | On-chain custody flows (e.g., `fund`, `withdraw`, `sweep_terminal_dust`) |
|---|---|---|
| **Token Transfers** | ❌ Does not transfer any tokens to/from the escrow contract | ✅ Uses SEP-41 token transfers with pre/post balance checks |
| **Reserve / Freeze Balances** | ❌ Does not freeze or lock any on-chain balances | ✅ `fund` credits principal; `withdraw` debits it |
| **Custody Verification** | ❌ Does not verify the SME owns or holds the referenced asset | ✅ `fund` verifies investor has tokens and credits on-chain balance |
| **Enforcement / Blocking** | ❌ Has no effect on any contract flow (settle, withdraw, claim, refund, etc.) | ✅ Legal hold blocks risk-bearing transitions; tier locks gate claims |
| **Asset Validation** | ❌ `asset` is any arbitrary Symbol string with no on-chain verification | ✅ Funding token is a bound SEP-41 contract validated at init |
| **On-chain Encumbrance** | ❌ Creates no on-chain lien, encumbrance, or asset-control relationship | ✅ `fund` creates a funded-amount liability; claims are gated by lock times |
| **Balance Accounting** | ❌ Does not read or write any token balance | ✅ All token flows are validated with pre/post-balance equality checks |

Specifically:

- **No Token Transfers:** Calling these functions does not transfer any tokens from the SME to the escrow contract, nor does it interact with any token contracts.
- **No Reserve Balances:** They do not freeze or lock any on-chain balances.
- **No Custody Verification:** The escrow contract does not verify that the SME actually owns, holds, or has custody of the referenced asset.
- **No Enforcement or Blocking:** Recording or clearing a collateral commitment does not block, gate, or restrict any other contract flows. Specifically, it has no effect on settlement, SME withdrawal, investor claims, investor refunds, compliance holds, treasury dust sweeps, beneficiary rotation, or admin operations.
- **No status dependency for clear:** `clear_sme_collateral_commitment` can be called regardless of escrow status (open / funded / settled), allowing clean-up after settlement or cancellation.

Future versions of the platform that enforce asset movement or custody must introduce **distinct API endpoints** with explicit token-transfer logic. Historical records of this self-reported metadata are not proof of custody and must **never** be treated as proof of locked assets.

---

## Invariant: No Token Balance Change

The following invariant is guaranteed by the contract and should be verified by integrators and auditors:

> **Invariant ESC-COL-001 (No Token Movement):** Calling `record_sme_collateral_commitment` or `clear_sme_collateral_commitment` never changes the escrow contract's funding-token balance. These functions write/remove only `DataKey::SmeCollateralPledge` (instance storage) and emit events. They perform no SEP-41 token operations.

This invariant is test-anchored in `escrow/src/tests/coverage.rs` (see `test_sme_collateral_no_token_balance_change`), which verifies that the contract's token balance remains identical before and after the call.

---

## Test Coverage

The scenarios below are covered in the collateral test suite in
[`escrow/src/tests/coverage.rs`](../escrow/src/tests/coverage.rs):

| Test | Scenario |
|------|----------|
| `test_sme_collateral_no_token_balance_change` | No token balances change on the escrow contract after recording (both initial and replacement). Anchors invariant ESC-COL-001. |
| `test_collateral_first_record_returns_correct_fields_and_prior_amount_is_zero` | First record returns the correct asset/amount/timestamp; `get_sme_collateral_commitment` reflects it. |
| `test_collateral_first_record_event_prior_amount_is_zero` | `CollateralRecordedEvt` emitted by the first record has `prior_amount = 0`. |
| `test_collateral_replacement_overwrites_stored_value_and_emits_prior_amount` | Replacement overwrites storage; event carries the previous record's amount as `prior_amount`. |
| `test_collateral_backwards_timestamp_rejected` | Replacing with a ledger timestamp earlier than `recorded_at` is rejected with `CollateralTimestampBackwards`; original record is preserved. |
| `test_collateral_same_timestamp_replacement_is_allowed` | Equal timestamps (`now >= prior.recorded_at`) are accepted (monotonic, not strictly increasing). |
| `test_collateral_zero_amount_rejected` | Zero amount is rejected with `CollateralAmountNotPositive`. |
| `test_collateral_negative_amount_rejected` | Negative amount is rejected with `CollateralAmountNotPositive`. |
| `test_collateral_empty_asset_rejected` | Empty asset symbol is rejected with `CollateralAssetEmpty`. |
| `test_collateral_non_sme_caller_rejected` | A caller that is not the SME address is rejected (auth failure). |

Additional collateral scenarios are also exercised in:
- [`escrow/src/tests/admin.rs`](../escrow/src/tests/admin.rs) — collateral record in admin-flow baselines.
- [`escrow/src/tests/integration.rs`](../escrow/src/tests/integration.rs) — `test_collateral_record_event_payload_is_metadata_only` and `test_collateral_replacement_event_contains_prior_amount` for full event-payload verification.

---

## Off-chain Risk-Team Handling

Risk teams and off-chain services must treat the recorded data as **self-reported metadata** and verify its validity independently.

### Recommended verification procedures

1. **Verify Signer Context:** Confirm the transaction was signed by the correct SME address linked to the invoice (`InvoiceEscrow::sme_address`).
2. **Resolve Asset Symbol:** Ensure the reported `asset` symbol maps to the correct physical asset or token contract. The contract performs no on-chain validation of the symbol.
3. **Verify Custody Separately:** Confirm custody accounts, statements, and security perfection outside the blockchain. The escrow contract makes no assertion about the SME's ownership.
4. **Reconcile Independently:** Implement any asset-control or settlement actions in separate off-chain systems or dedicated contracts, completely detached from this metadata escrow record.
5. **Clear Labeling:** Label all indexed database fields as `reported_collateral_metadata` rather than implying locked balances or enforceable claims. Never use `collateral_locked`, `collateral_pledged`, or similar language that implies on-chain enforcement.
6. **Monitor Replacements:** Track `prior_amount` → `amount` transitions in `CollateralRecordedEvt` events to detect collateral amount changes over time.
7. **Track Clears:** Monitor `CollateralClearedEvt` events to detect when the SME retires a commitment.

### Event indexing guidance

When indexing `CollateralRecordedEvt` events:

```text
Topic 0 (fixed): collateral_recorded_evt
Topic 1:         name = "coll_rec"
Data:
  invoice_id: Symbol     — join key for escrow state
  amount:     i128       — current reported amount
  prior_amount: i128     — 0 for first record; previous amount for replacements
```

When indexing `CollateralClearedEvt` / `CollateralCommitmentCleared` events:

```text
Topic 0 (fixed): collateral_cleared_evt / collateral_commitment_cleared
Topic 1:         name = "coll_clr"
Data:
  invoice_id: Symbol     — join key for escrow state
  asset:      Symbol     — asset symbol from the cleared commitment
  amount:     i128       — amount from the cleared commitment
  recorded_at: u64       — original ledger timestamp from the cleared commitment
```

Indexers should:
- Use `invoice_id` as the join key to associate collateral records with escrow state.
- Treat `prior_amount == 0` as the initial record for a given invoice.
- Treat consecutive `CollateralRecordedEvt` events with non-zero `prior_amount` as replacement updates.
- Treat a `CollateralClearedEvt` after a `CollateralRecordedEvt` as record retirement; the escrow may later receive a new first record (`prior_amount = 0`).
- Never infer token custody, lien perfection, or asset encumbrance from event presence alone.

---

## Error Codes

| Code | Variant | Entrypoint | Trigger |
|------|---------|-----------|---------|
| 60 | `CollateralAmountNotPositive` | `record_sme_collateral_commitment` | `amount <= 0` |
| 61 | `CollateralAssetEmpty` | `record_sme_collateral_commitment` | asset symbol empty |
| 62 | `CollateralTimestampBackwards` | `record_sme_collateral_commitment` | new timestamp < stored timestamp |
| TBD | `NoCollateralToClear` | `clear_sme_collateral_commitment` | no pledge exists to clear |

---

## Example Flow

```
SME calls record_sme_collateral_commitment("GOLD", 5_000)
  → DataKey::SmeCollateralPledge stored with { asset: "GOLD", amount: 5000, recorded_at: <ts> }
  → CollateralRecordedEvt { name: "coll_rec", invoice_id: "INV001", amount: 5000, prior_amount: 0 }

SME updates: record_sme_collateral_commitment("GOLD", 7_000)
  → DataKey::SmeCollateralPledge overwritten with { amount: 7000, recorded_at: <ts2> }
  → CollateralRecordedEvt { amount: 7000, prior_amount: 5000 }

[invoice settled off-chain; pledge released]

SME calls clear_sme_collateral_commitment()
  → DataKey::SmeCollateralPledge removed
  → CollateralClearedEvt { name: "coll_clr", invoice_id: "INV001", asset: "GOLD", amount: 7000, recorded_at: <ts2> }
  → CollateralCommitmentCleared emitted with same payload
```

---

## Cross-references

- **Rustdoc:** See [`LiquifactEscrow::record_sme_collateral_commitment`] and [`LiquifactEscrow::clear_sme_collateral_commitment`] in [`escrow/src/lib.rs`](../escrow/src/lib.rs).
- **Struct definition:** [`SmeCollateralCommitment`] struct with `asset`, `amount`, and `recorded_at` fields.
- **Event schema:** [`CollateralRecordedEvt`], [`CollateralClearedEvt`], and [`CollateralCommitmentCleared`] in [`docs/EVENT_SCHEMA.md`](EVENT_SCHEMA.md).
- **Error codes:** Codes 60–62 and `NoCollateralToClear` in [`docs/escrow-error-messages.md`](escrow-error-messages.md).
- **Storage model:** [`DataKey::SmeCollateralPledge`] in [`docs/escrow-data-model.md`](escrow-data-model.md).
- **Read API:** [`get_sme_collateral_commitment()`] in [`docs/escrow-read-api.md`](escrow-read-api.md).
- **Security checklist:** Section 5.8 in [`docs/escrow-security-checklist.md`](escrow-security-checklist.md).
- **Audit handoff:** Section 6.3 in [`docs/audit-handoff-escrow.md`](audit-handoff-escrow.md).
- **Indexer guidance:** [`docs/escrow-indexer.md`](escrow-indexer.md).
- **CLI simulation:** [`docs/escrow-sim-stellar-cli.md`](escrow-sim-stellar-cli.md).
- **Glossary:** [`docs/glossary.md`](glossary.md) — definitions for `SmeCollateralCommitment` and related terms.

---

## Security Notes

- The contract writes/removes metadata only — there is **no token-transfer code path** reachable from these entrypoints.
- The `recorded_at` monotonicity check ensures replacement timestamps do not regress, providing defense-in-depth against stale replay attacks.
- The `asset` symbol is stored as an arbitrary `Symbol` with no on-chain resolution. Integrators must map it to real-world assets off-chain.
- A compromised SME key could write arbitrary collateral amounts or clear records, but since the record is metadata-only and does not gate any contract flow, the blast radius is limited to off-chain reporting inaccuracies.
- These records must **never** be used as the sole input for automated liquidation, margin calls, or asset-freeze decisions.
- **No double-clear risk:** the existence check in `clear_sme_collateral_commitment` ensures a second clear call returns `NoCollateralToClear` rather than silently succeeding.
