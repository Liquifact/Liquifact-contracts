# Collateral Model — LiquiFact Escrow

> **Canonical reference** for the SME collateral commitment subsystem in
> `escrow/src/lib.rs`. Supersedes the older `docs/escrow-sme-collateral.md`,
> which is retained only for cross-reference and now points here.

This document explains the collateral data model, every invariant of that
model, and every entrypoint that modifies or reads collateral state. It is kept
in lock-step with the implementation; the only authoritative source of behavior
is the Rust source — when this document and the code disagree, the code is
correct.

**Scope.** The collateral subsystem is a metadata-only pledge ledger. It does
**not** move tokens, freeze balances, custody assets, or create an enforceable
on-chain encumbrance. Off-chain risk teams must verify custody separately.

---

## 1. Overview

| Item | Value |
|---|---|
| Storage backend | `env.storage().instance()` |
| Storage key | `DataKey::SmeCollateralPledge` |
| Stored struct | `SmeCollateralCommitment` |
| Mutating entrypoints | `record_sme_collateral_commitment`, `clear_sme_collateral_commitment` |
| Read-only entrypoint | `get_sme_collateral_commitment` |
| Indirect read | `EscrowSummary.sme_collateral_commitment` (as `CollateralCommitmentSnapshot`) |
| Required auth | `InvoiceEscrow::sme_address` for every write |
| Schema version | Introduced additively under [ADR-007](adr/ADR-007-storage-key-evolution.md) (legacy instances behave as "no pledge recorded") |
| Token movement | **None.** No `TokenClient::transfer` is invoked anywhere in the collateral subsystem. |

The collateral key is **optional** in storage: it is absent until the SME first
records a pledge and after the SME clears one. Reads return `None` in both
cases; consumers cannot distinguish "never recorded" from "cleared" — neither
state implies on-chain evidence of custody.

---

## 2. Data Model

### 2.1 Stored struct (`escrow/src/lib.rs`)

```rust
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SmeCollateralCommitment {
    pub asset: Symbol,         // off-chain asset identifier; non-empty
    pub amount: i128,          // reported amount in the asset's own base units; strictly positive
    pub recorded_at: u64,      // env.ledger().timestamp() at the time of the successful record call
}
```

| Field | Type | Validation | Source |
|---|---|---|---|
| `asset` | `Symbol` | Must be non-empty (`!= Symbol::new("")`). | `record_sme_collateral_commitment` in `escrow/src/lib.rs` |
| `amount` | `i128` | Must be strictly positive (`> 0`). | `record_sme_collateral_commitment` in `escrow/src/lib.rs` |
| `recorded_at` | `u64` | Set to `env.ledger().timestamp()` of the current call; not user-supplied. | `record_sme_collateral_commitment` in `escrow/src/lib.rs` |

### 2.2 Storage key

```rust
pub enum DataKey {
    // …
    /// Optional SME collateral commitment metadata (record-only — not an on-chain
    /// asset lock). Absent when no commitment has been recorded. Replaceable by
    /// the SME.
    SmeCollateralPledge,
    // …
}
```

- **Scope:** single escrow instance. There is at most **one** `SmeCollateralPledge`
  per instance at any time; history prior to the latest record is preserved only
  via emitted events (`CollateralRecordedEvt.prior_amount`).
- **Presence on legacy instances:** absent on escrows predating the addition of
  the collateral key. Per [ADR-007](adr/ADR-007-storage-key-evolution.md) Rule 1,
  the key is read with the `Option<SmeCollateralCommitment>` pattern, so legacy
  instances resolve to `None` without panicking. **No `migrate` path is required**
  and none is implemented.

### 2.3 Read projection (`EscrowSummary`)

```rust
pub struct EscrowSummary {
    pub escrow: InvoiceEscrow,
    pub has_maturity_lock: bool,
    pub legal_hold: bool,
    pub funding_close_snapshot: EscrowCloseSnapshot,
    pub unique_funder_count: u32,
    pub is_allowlist_active: bool,
    pub schema_version: u32,
    pub sme_collateral_commitment: CollateralCommitmentSnapshot,
    pub has_primary_attestation: bool,
    pub attestation_log_length: u32,
}

pub enum CollateralCommitmentSnapshot {
    None,
    Some(SmeCollateralCommitment),
}
```

- `CollCommitmentSnapshot::None` when `DataKey::SmeCollateralPledge` is absent
  (never recorded, cleared, or pre-additive legacy instance).
- `Some(SmeCollateralCommitment)` when a record is presently stored.

---

## 3. Invariants

These invariants hold for **every** escrow instance during its lifetime. They
follow directly from `record_sme_collateral_commitment`,
`clear_sme_collateral_commitment`, and the storage key definition in
`escrow/src/lib.rs`. Every invariant is enforced by the entrypoint itself and
verified by at least one test in §7.

| # | Invariant | Enforcement site |
|---|---|---|
| I-1 | **Metadata only.** Recording or clearing a pledge never moves tokens, never reserves balances, never freezes or encumbers any contract address. No `TokenClient::transfer` is invoked from either collateral entrypoint; balance checks are absent. | Whole subsystem (search for `transfer` calls in the three collateral entrypoints — there are none). |
| I-2 | **Singleton storage.** At any time, an escrow holds at most one `SmeCollateralCommitment`. Previous pledges are not retained on-chain — they survive only in event history (`prior_amount` field). | `DataKey::SmeCollateralPledge` is a unit variant; `set(...)` overwrites; `remove(...)` deletes. |
| I-3 | **SME-only writes.** Both `record_sme_collateral_commitment` and `clear_sme_collateral_commitment` require `InvoiceEscrow::sme_address.require_auth()` via the shared helper `load_escrow_require_sme`. The admin **cannot** record or clear. | See [ADR-002](adr/ADR-002-auth-boundaries.md) decision table. |
| I-4 | **SME cannot rotate ownership.** A new SME address is set only via `rotate_beneficiary` (admin/SME two-party flow); after rotation, the new SME must sign future collateral entries. There is no on-chain link between prior and successor pledge records beyond the event sequence. | `rotate_beneficiary` and `load_escrow_require_sme`. |
| I-5 | **Positive amount.** A `record` call with `amount <= 0` is rejected with `EscrowError::CollateralAmountNotPositive` (`u32 = 60`). No record is written and no event is emitted on rejection. | `record_sme_collateral_commitment`. |
| I-6 | **Non-empty asset.** A `record` call with `asset == Symbol::new("")` is rejected with `EscrowError::CollateralAssetEmpty` (`u32 = 61`). The asset symbol is an opaque string and is **not** validated to refer to any on-chain token contract — `EscrowError::Token_*` is never raised here. | `record_sme_collateral_commitment`. |
| I-7 | **Monotonic ledger timestamp on replacement.** If a prior `SmeCollateralCommitment` is present, `env.ledger().timestamp() >= prior.recorded_at` is required. Equality (`==`) is allowed; a strictly smaller timestamp is rejected with `EscrowError::CollateralTimestampBackwards` (`u32 = 62`). | `record_sme_collateral_commitment`. |
| I-8 | **Clear releases storage.** After `clear_sme_collateral_commitment`, `DataKey::SmeCollateralPledge` is removed; `get_sme_collateral_commitment()` then returns `None`, and `EscrowSummary.sme_collateral_commitment` is `None`. | `clear_sme_collateral_commitment`. |
| I-9 | **No double-clear.** A second `clear_sme_collateral_commitment` call when no record exists fails with `EscrowError::NoCollateralToClear` (`u32 = 169`) before any auth is consumed and before any storage mutation. The failure on the second call is **deterministic and idempotent**. | `clear_sme_collateral_commitment`. |
| I-10 | **Status-independent.** Neither collateral mutating entrypoint inspects `InvoiceEscrow::status`. Recording and clearing are valid in every status (`0` open, `1` funded, `2` settled, `3` withdrawn, `4` cancelled). | Absence of `require_funding_open` / `guard_status_eq` / `guard_status_in` calls in the collateral entrypoints. |
| I-11 | **Hold/pause/allowlist-orthogonal.** Neither entrypoint reads `DataKey::LegalHold`, `DataKey::Paused`, `InvestorAllowlisted`, or any investor-side state. A legal hold, operational pause, or allowlist gate does **not** prevent the SME from updating collateral metadata. | Absence of `guard_not_legal_hold` and `paused_active` calls in the collateral entrypoints. |
| I-12 | **No effect on principal-bearing state.** Recording or clearing does not modify `InvoiceEscrow`, `InvestorContribution`, `FundingCloseSnapshot`, `SettledAt`, `DistributedPrincipal`, `UniqueFunderCount`, or any token balance. | Whole subsystem: no `instance().set(&DataKey::Escrow, …)`, no `InvestorContribution.set`, no `TokenClient` calls. |
| I-13 | **Single event on record.** `record` emits exactly **one** event on success: `CollateralRecordedEvt` with `name` topic `coll_rec` and `prior_amount` in data (`0` on first record, otherwise the prior stored `amount`). | `record_sme_collateral_commitment` (one `.publish(&env)` call). |
| I-14 | **Two events on clear.** `clear` emits exactly **two** events on success, both with `name` topic `coll_clr` and `invoice_id` topic, with **identical** data payloads (`asset`, `amount`, `recorded_at`): `CollateralClearedEvt` and `CollateralCommitmentCleared`. The duplication is intentional so that indexers which key on either struct can reconstruct the record-to-clear lifecycle without re-reading storage after the mutation. **Do not double-count statistics on `coll_clr`: collapse events with identical `(name, invoice_id, asset, amount, recorded_at)` to one logical clear.** | `clear_sme_collateral_commitment` (two `.publish(&env)` calls). |
| I-15 | **Canonical guard order on clear.** Per [ADR-002](adr/ADR-002-auth-boundaries.md), the clear path runs the **read-only existence check first** so that an absent pledge reports `NoCollateralToClear` instead of an auth failure. `require_auth` is then invoked, then the storage removal and event emission. | `clear_sme_collateral_commitment` order of operations. |
| I-16 | **Additive-key legacy safety.** Escrow instances predating the addition of `DataKey::SmeCollateralPledge` Read with `Option` return — never panic on absence. No `migrate` call is required (and none exists for this key per [ADR-007](adr/ADR-007-storage-key-evolution.md)). | `get_sme_collateral_commitment` reads via `.get(...)` returning `Option`. |

---

## 4. Entrypoints

### 4.1 `record_sme_collateral_commitment(env, asset: Symbol, amount: i128) -> SmeCollateralCommitment`

Write or replace the SME collateral commitment metadata.

| Attribute | Value |
|---|---|
| Source | `escrow/src/lib.rs` (`pub fn record_sme_collateral_commitment(...)`) |
| Auth | `InvoiceEscrow::sme_address.require_auth()` (via `load_escrow_require_sme`); the helper returns `EscrowNotInitialized` (`u32 = 20`) before `require_auth` if storage is uninitialised. |
| Mutates | `DataKey::SmeCollateralPledge` (`instance` storage). |
| Emits | Exactly one `CollateralRecordedEvt` on success; nothing on failure. |
| Returns | The newly written `SmeCollateralCommitment`. |
| Token movement | None. |

**Guard order** (steps that branch on inputs/storage):

1. `ensure(amount > 0)` → `CollateralAmountNotPositive` (`u32 = 60`).
2. `ensure(asset != Symbol::new(""))` → `CollateralAssetEmpty` (`u32 = 61`).
3. `load_escrow_require_sme(&env)` → `EscrowNotInitialized` (`u32 = 20`) when storage is missing; otherwise `sme_address.require_auth()` is invoked.
4. Read prior commitment from `DataKey::SmeCollateralPledge` (`Option`).
5. If prior exists: `ensure(now >= prior.recorded_at)` → `CollateralTimestampBackwards` (`u32 = 62`).
6. Compute `prior_amount = prior.as_ref().map(|c| c.amount).unwrap_or(0)`.
7. Build `commitment = SmeCollateralCommitment { asset, amount, recorded_at: now }` and overwrite `DataKey::SmeCollateralPledge`.
8. Emit `CollateralRecordedEvt { name: coll_rec, invoice_id, amount, prior_amount }`.
9. Return `commitment`.

**Errors raised.** `CollateralAmountNotPositive` (`60`), `CollateralAssetEmpty`
(`61`), `CollateralTimestampBackwards` (`62`), `EscrowNotInitialized` (`20`).

**Notes.**

- `recorded_at` is set to `env.ledger().timestamp()` of the current call — the
  caller cannot supply it. Off-chain indexers should treat the field as
  validator-observed ledger time.
- A replacement overwrites the prior record; the prior amount is preserved in
  the emitted `prior_amount` data field.
- This entrypoint is **idempotent overwrites**: success returns the new value;
  a second identical call with a monotonic-or-later ledger timestamp replaces
  the prior record and emits `prior_amount = prior.amount`.

### 4.2 `get_sme_collateral_commitment(env) -> Option<SmeCollateralCommitment>`

Read the current SME collateral commitment metadata from instance storage.

| Attribute | Value |
|---|---|
| Source | `escrow/src/lib.rs` |
| Auth | None (read-only). |
| Mutates | Nothing. |
| Emits | Nothing. |
| Returns | `Some(SmeCollateralCommitment)` when `DataKey::SmeCollateralPledge` is present; `None` otherwise (never recorded, cleared, or pre-additive legacy). |

**Errors raised.** None — a missing key returns `None`, not an error.

**Notes.** The view does not require `init` to have been called: it returns
`None` before `init` and for legacy instances that predate the key. This is
verified by `read_view_defaults_before_init` in
[`escrow/src/tests/coverage.rs`](../escrow/src/tests/coverage.rs).

### 4.3 `clear_sme_collateral_commitment(env)`

Retire the recorded SME collateral pledge.

| Attribute | Value |
|---|---|
| Source | `escrow/src/lib.rs` |
| Auth | `InvoiceEscrow::sme_address.require_auth()` (via `load_escrow_require_sme`). |
| Mutates | Removes `DataKey::SmeCollateralPledge` (`instance` storage). |
| Emits | Exactly two events on success — `CollateralClearedEvt` and `CollateralCommitmentCleared` — both with `name = coll_clr` and `invoice_id` topic; nothing on failure. |
| Returns | `()` (unit). |
| Token movement | None. |

**Guard order** (canonical ADR-002 sequence; the existence check is
deliberately placed before `require_auth` so an absent pledge reports a useful
error rather than an auth failure):

1. Read `DataKey::SmeCollateralPledge`; on absence, `fail(&env, EscrowError::NoCollateralToClear)` (`u32 = 169`). **No auth is consumed on this path.**
2. `load_escrow_require_sme(&env)` → `EscrowNotInitialized` (`20`) before `require_auth` if storage is uninitialised.
3. `env.storage().instance().remove(&DataKey::SmeCollateralPledge)`.
4. Emit `CollateralClearedEvt { name: coll_clr, invoice_id, asset, amount, recorded_at }`.
5. Emit `CollateralCommitmentCleared { name: coll_clr, invoice_id, asset, amount, recorded_at }`.

**Errors raised.** `NoCollateralToClear` (`169`), `EscrowNotInitialized` (`20`).

**Notes.**

- The two events emitted on success carry **identical** topic and data payloads.
  Indexers should fold these into one logical clear event to avoid double
  counting — see §5.
- `clear` reads the prior commitment into local variables **before** the
  `require_auth` call to keep the existence check first in the guard order.
  Consequently, the `NoCollateralToClear` error never reveals the SME's
  signature being rejected when storage is simply empty.

---

## 5. Events

All collateral events are emitted by `record_sme_collateral_commitment` and
`clear_sme_collateral_commitment` only. Topic layout follows the
Soroban `#[contractevent]` model — see
[`docs/EVENT_SCHEMA.md`](EVENT_SCHEMA.md).

### 5.1 `CollateralRecordedEvt`

| Location | `escrow/src/lib.rs` |
|---|---|
| Topics | `[fixed_sym, name = coll_rec]`<br>where `fixed_sym = "collateral_recorded_evt"` (generated from the struct name in snake-case). |
| Data | `invoice_id: Symbol`, `amount: i128`, `prior_amount: i128` |
| Emitted by | `record_sme_collateral_commitment` (exactly once on success). |

`prior_amount` is `0` for the first record on an instance and the prior
stored amount on each replacement.

### 5.2 `CollateralClearedEvt`

| Location | `escrow/src/lib.rs` |
|---|---|
| Topics | `[fixed_sym, name = coll_clr, invoice_id: Symbol]`<br>where `fixed_sym = "collateral_cleared_evt"`. |
| Data | `asset: Symbol`, `amount: i128`, `recorded_at: u64` |
| Emitted by | `clear_sme_collateral_commitment` (exactly once on success). |

### 5.3 `CollateralCommitmentCleared`

| Location | `escrow/src/lib.rs` |
|---|---|
| Topics | `[fixed_sym, name = coll_clr, invoice_id: Symbol]`<br>where `fixed_sym = "collateral_commitment_cleared"`. |
| Data | `asset: Symbol`, `amount: i128`, `recorded_at: u64` |
| Emitted by | `clear_sme_collateral_commitment` (exactly once on success). |

This event is the removal-side counterpart to `CollateralRecordedEvt` and
**supplements** `CollateralClearedEvt`. The `name` and `invoice_id` topics and
the data payload are identical by design so that an indexer which routes on
either `CollateralClearedEvt` or `CollateralCommitmentCleared` reconstructs the
same record-to-clear lifecycle. The two are emitted in the order
`CollateralClearedEvt` → `CollateralCommitmentCleared` in the same transaction.

**Deduplication guidance.**

A logical "clear" is one `(coll_clr, invoice_id, asset, amount, recorded_at)`
tuple, regardless of which of the two event structs delivers it. Production
indexers must collapse these to one logical clear per `coll_clr` event to avoid
double-counting analytics. Off-chain reconciliation should sum over the
`coll_rec`-prefixed stream for the same `(invoice_id, asset)` series to derive a
monotonic history of pledge revisions without ever calling storage after the
fact.

### 5.4 Out-of-scope events for collateral

No settlement, withdrawal, or claim event references the collateral metadata.
In particular, `SmeWithdrew`, `EscrowSettled`, `InvestorPayoutClaimed`, and
`FundingCancelled` never carry a `prior_amount` or `recorded_at` field — the
collateral commitment is independent of the principal flow.

---

## 6. Error Codes

Stable, append-only numeric codes from `EscrowError` in
`escrow/src/lib.rs`. Codes here reserved exclusively for the collateral
subsystem:

| Code | Variant | Trigger | Entrypoint |
|---:|---|---|---|
| 60 | `CollateralAmountNotPositive` | `amount <= 0` | `record_sme_collateral_commitment` |
| 61 | `CollateralAssetEmpty` | `asset == Symbol::new("")` | `record_sme_collateral_commitment` |
| 62 | `CollateralTimestampBackwards` | `now < prior.recorded_at` on replacement | `record_sme_collateral_commitment` |
| 169 | `NoCollateralToClear` | `DataKey::SmeCollateralPledge` is absent | `clear_sme_collateral_commitment` |

In addition, `EscrowNotInitialized` (`u32 = 20`) is raised by the underlying
`load_escrow_require_sme` helper if either mutating entrypoint is invoked
before `init` completes — see I-3 and I-15 for the auth sequence.

Client SDKs should branch on the numeric code rather than panic strings — see
[`docs/escrow-error-messages.md`](escrow-error-messages.md).

---

## 7. Worked Example

This walkthrough uses the test fixture values from
[`escrow/src/tests/coverage.rs`](../escrow/src/tests/coverage.rs) where
`PLEDGE = 50_000_000_000`. Every step in this example is traceable to a
named test in §8:

| Step | Anchored to test |
|---|---|
| 7.1 first record of `PLEDGE` | `test_overwrite_then_clear` (initial leg) |
| 7.2 replacement with `2 * PLEDGE` at a later timestamp | `test_overwrite_then_clear` (replacement leg) |
| 7.3 clear + second clear | `test_overwrite_then_clear` and `test_double_clear_rejected` |

The example uses `asset = "USDC"` for narrative clarity; the focused tests use
`asset = soroban_sdk::symbol_short!("USDC")`. Both produce the same `Symbol`
payload at the event layer.

### 7.1 Initialised escrow

The escrow was brought up via `init` with `invoice_id = "INV-COLLAT-01"`,
`admin`, `sme`, `funding_token`, `treasury`, and `yield_bps = 0`. The
collateral key is absent.

```text
record_sme_collateral_commitment("USDC", PLEDGE)            # PLEDGE = 50_000_000_000
  ├─ amt > 0?        ✓
  ├─ asset non-empty?✓
  ├─ load escrow     ✓
  ├─ prior is None   ✓  (first record → backward-timestamp guard skipped)
  ├─ now = 0         (Soroban test default ledger timestamp)
  ├─ write           DataKey::SmeCollateralPledge := { asset="USDC", amount=PLEDGE, recorded_at=0 }
  └─ emit            CollateralRecordedEvt {
                        topics: ["collateral_recorded_evt", "coll_rec"]
                        data:   { invoice_id="INV-COLLAT-01", amount=PLEDGE, prior_amount=0 }
                      }

get_sme_collateral_commitment()
  → Some({ asset="USDC", amount=PLEDGE, recorded_at=0 })
```

### 7.2 Pledge revision at a later ledger timestamp

The SME wishes to revise the pledge upward. The ledger timestamp is
advanced to a strictly later value; equality (`now == prior.recorded_at`)
is also allowed by the `>=` guard.

```text
record_sme_collateral_commitment("USDC", 2 * PLEDGE)        # 100_000_000_000
  ├─ amt > 0?        ✓
  ├─ asset non-empty?✓
  ├─ load escrow     ✓
  ├─ prior is Some   ✓
  ├─ now > prior.recorded_at     ⇒  now >= prior.recorded_at  ✓ (passes monotonic guard)
  ├─ prior_amount = PLEDGE       (50_000_000_000)
  ├─ write           DataKey::SmeCollateralPledge := { asset="USDC", amount=2*PLEDGE, recorded_at=now }
  └─ emit            CollateralRecordedEvt {
                        topics: ["collateral_recorded_evt", "coll_rec"]
                        data:   { invoice_id="INV-COLLAT-01", amount=2*PLEDGE, prior_amount=PLEDGE }
                      }

get_sme_collateral_commitment()
  → Some({ asset="USDC", amount=2*PLEDGE, recorded_at=now })    # revision persisted
```

> **Backwards-timestamp attempt.** Calling `record_sme_collateral_commitment`
> with the ledger timestamp **rewound** below `prior.recorded_at` is rejected
> with `CollateralTimestampBackwards` (`u32 = 62`). The prior record is left
> untouched. Synthetic rewinds of ledger time are exposed by the
> "Collateral group" sub-section of `typed_error_codes_cover_range_boundaries`
> in [`escrow/src/tests/coverage.rs`](../escrow/src/tests/coverage.rs), whose
> collateral fixture advances to `5000`, then **rewinds** to `100`, records at
> `200`, and asserts `CollateralTimestampBackwards` is raised. Do not read the
> `5000 → 100 → 200` chronology into the on-chain protocol — it is purely a
> test-harness technique that deliberately simulates clock skew.

### 7.3 Clearing after settlement

Once the off-chain pledge has been retired, the SME clears the on-chain
metadata. Two events are emitted in the same transaction, both with `name =
coll_clr` and identical data payloads.

```text
clear_sme_collateral_commitment()
  ├─ read DataKey::SmeCollateralPledge                       → present
  ├─ load_escrow_require_sme                                 → sme.require_auth() ok
  ├─ remove DataKey::SmeCollateralPledge
  ├─ emit CollateralClearedEvt {
  │     topics: ["collateral_cleared_evt", "coll_clr", "INV-COLLAT-01"]
  │     data:   { asset="USDC", amount=2*PLEDGE, recorded_at=now }
  │   }
  └─ emit CollateralCommitmentCleared {
        topics: ["collateral_commitment_cleared", "coll_clr", "INV-COLLAT-01"]
        data:   { asset="USDC", amount=2*PLEDGE, recorded_at=now }
      }

get_sme_collateral_commitment()
  → None

clear_sme_collateral_commitment()                           # second clear
  → fail: NoCollateralToClear (u32 = 169)                    # no auth consumed
```

After this point, `EscrowSummary.sme_collateral_commitment` is
`CollateralCommitmentSnapshot::None` and any subsequent
`record_sme_collateral_commitment` opens a fresh pledge-history series.

---

## 8. Test Coverage

Each invariant listed in §3 is exercised by at least one test in
[`escrow/src/tests/coverage.rs`](../escrow/src/tests/coverage.rs). Test names
are stable in the current build; tests marked `#[ignore]` have known upstream
drift and are scheduled for follow-up fixes (see the source comments).

| Test | Invariant(s) covered |
|---|---|
| `test_get_returns_none_before_record` | I-2 (singleton / absent is `None`); I-16 (additive-key legacy safety) |
| `test_sme_collateral_commitment` | I-3 (SME-authed write); I-5 (positive amount); I-6 (non-empty asset); I-13 (one event); access via `get_sme_collateral_commitment` round-trip |
| `test_sme_collateral_empty_asset_rejected` | I-6 — rejects `Symbol::new("")` with `CollateralAssetEmpty` (`61`) |
| `test_overwrite_then_clear` | I-2 (singleton overwrite); I-8 (clear releases storage); I-12 (no other state mutation); I-14 (clear emits events; subsequent `get` is `None`) |
| `test_sme_collateral_replacement_preserves_prior_amount` | I-7 (monotonic timestamp; replacement allowed); I-13 (`prior_amount` preserved in event) |
| `test_sme_collateral_stale_timestamp_rejected` (`#[ignore]`) | I-7 — currently ignored pending upstream test/API drift |
| `test_double_clear_rejected` | I-9 (subsequent clear returns `NoCollateralToClear`); I-15 (existence check before auth) |
| `typed_error_codes_cover_range_boundaries` ("Collateral group" sub-section) | I-5 (rejects `amount = 0`); I-7 (rejects stale timestamp on second record when ledger is rewound) |
| `read_view_defaults_before_init` | I-16 — `get_sme_collateral_commitment` returns `None` before `init` |

> **Note on I-11 (hold/pause/allowlist-orthogonal).** No focused end-to-end test
> exercises every held/paused/allowlisted combination against `record_*` /
> `clear_*`. The invariant is asserted by source-level absence: auditors and
> code reviewers can verify it by inspecting the
> `record_sme_collateral_commitment`, `clear_sme_collateral_commitment`, and
> `get_sme_collateral_commitment` bodies in `escrow/src/lib.rs` and confirming
> that none of `guard_not_legal_hold`, `paused_active`, or any allowlist read
> is invoked. Cross-cutting tests for `{read → record → clear}` interactions
> across the admin-only entrypoints live in
> [`escrow/src/tests/admin.rs`](../escrow/src/tests/admin.rs); collateral
> event-payload assertions live in
> [`escrow/src/tests/integration.rs`](../escrow/src/tests/integration.rs).

---

## 9. Off-Chain / Risk-Team Handling

The on-chain collateral commitment is **a pledge ledger, not a custody
system**. Off-chain verifiers must independently establish that the SME has
asset control, rights, or lien obligations matching the reported `asset` symbol
and `amount`.

### 9.1 Indexer guidance

- Treat each `CollateralRecordedEvt` as a **revision** of the ongoing pledge
  series; reconstruct history from the event log using `prior_amount` deltas.
- On `clear`, fold `CollateralClearedEvt` and `CollateralCommitmentCleared`
  into a single logical "pledge retired" record. They carry identical topics
  and data payloads by design (§5).
- Do **not** treat the recorded `amount` as a locked token balance. It is the
  SME's reported figure in the off-chain asset's own units.

### 9.2 Risk-team checklist

| Step | Action | Anchor |
|---|---|---|
| 1 | Pull the latest `CollateralRecordedEvt` for an `invoice_id` and read `recorded_at`, `asset`, `amount`. | §5.1 |
| 2 | Cross-reference the asset symbol against the off-chain pledge registry for the SME. | Off-chain |
| 3 | Reconcile `prior_amount` deltas to construct the pledge revision history. | §5.1 |
| 4 | When `clear` is observed, fold the two events and lift the retirement timestamp; do not assume continuing pledge coverage. | §5.2, §5.3 |

### 9.3 Security notes

- **Metadata only.** Neither `record_sme_collateral_commitment` nor
  `clear_sme_collateral_commitment` transfers or locks tokens. This is **not
  proof of custody** — the contract does not verify off-chain asset control.
- **SME-only writes.** Every mutating operation requires
  `sme_address.require_auth()`. A compromised admin key still cannot write
  collateral metadata.
- **No status dependency.** Collateral writes are allowed in any status,
  including during a legal hold or operational pause.
- **No double-clear risk.** The existence check on clear's entry ensures a
  second `clear` call returns `NoCollateralToClear` (`169`) without consuming
  auth — useful absence errors are not masked by auth failures (I-15).
- **Additive-key safety.** No `migrate` path is needed for the collateral
  key; legacy instances without the key behave as "no pledge recorded"
  ([ADR-007](adr/ADR-007-storage-key-evolution.md)). Do not migrate this key
  away from `instance` storage without first confirming the storage-growth
  impact.

---

## 10. Related Documents

| Document | Purpose |
|---|---|
| [`docs/escrow-sme-collateral.md`](escrow-sme-collateral.md) | Older single-page reference retained for cross-link; this document supersedes it. |
| [`docs/escrow-data-model.md`](escrow-data-model.md) | Full `DataKey` and stored-struct catalog including `SmeCollateralCommitment`. |
| [`docs/glossary.md`](glossary.md) | Cross-team glossary (entry: *Collateral commitment*). |
| [`docs/EVENT_SCHEMA.md`](EVENT_SCHEMA.md) | Authoritative event schema for indexers (entries: `CollateralRecordedEvt`, `CollateralClearedEvt`). |
| [`docs/escrow-events.md`](escrow-events.md) | Consumer-facing event overview. |
| [`docs/escrow-read-api.md`](escrow-read-api.md) | Catalog of read-only entrypoints including `get_sme_collateral_commitment`. |
| [`docs/adr/ADR-002-auth-boundaries.md`](adr/ADR-002-auth-boundaries.md) | Canonical guard order and role/entrypoint matrix. |
| [`docs/adr/ADR-007-storage-key-evolution.md`](adr/ADR-007-storage-key-evolution.md) | Additive-key policy and additive migration safety. |
| [`docs/escrow-security-checklist.md`](escrow-security-checklist.md) | Authorisation guard ordering checklist (§6 in particular). |
| [`escrow/src/lib.rs`](../escrow/src/lib.rs) | Authoritative source for the collateral subsystem. |
| [`escrow/src/tests/coverage.rs`](../escrow/src/tests/coverage.rs) | Focused collateral test suite. |
