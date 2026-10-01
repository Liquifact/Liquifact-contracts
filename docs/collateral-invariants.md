
# Collateral Invariants

This document enumerates the invariants that must always hold for the **SME collateral commitment** metadata in the LiquiFact escrow contract.

> **Compatibility contract:** The entry points, storage key, error codes, and event payloads described below are part of the public interface. Any change to their names, signatures, error semantics, or payload shape must ship with a tested migration path and an updated version of this document.

---

## Validation Boundaries

## Overview

The escrow contract allows the SME (Small‑Medium Enterprise) to record optional collateral information via the entrypoint:

The following entry points are the stable public surface for this feature. Their names, argument order, and return types must not change without a compatibility plan.

- `record_sme_collateral_commitment`
- `clear_sme_collateral_commitment`
- `get_sme_collateral_commitment`

The recorded data is stored in the instance storage key `DataKey::SmeCollateralPledge` and emitted in the event `CollateralRecordedEvt`.  It is **metadata‑only** and does **not** move tokens, lock assets, or affect any settlement or withdrawal logic.

The storage key `DataKey::SmeCollateralPledge` and the event symbol `CollateralRecordedEvt` are part of the compatibility contract: off‑chain indexers and downstream consumers depend on them. Renaming or moving either requires a coordinated migration.

---

## State Model

The collateral subsystem owns exactly one piece of instance storage: `DataKey::SmeCollateralPledge`, which holds an optional `SmeCollateralPledge { amount, asset, recorded_at }`. The pledge is either **absent** (`None`) or **present** (`Some`). All transitions are performed by `record_sme_collateral_commitment` (absent→present, present→present) and `clear_sme_collateral_commitment` (present→absent). No other entry point may read or write this key, so the stored value is the single source of truth for the collateral state.

## Invariants

| # | Invariant | Description | Enforced By |
|---|-----------|-------------|--------------|
| 1 | **Positive Amount** | `amount` must be strictly greater than zero. | `record_sme_collateral_commitment` (`EscrowError::CollateralAmountNotPositive`) |
| 2 | **Non‑empty Asset Symbol** | `asset` must be a non‑empty `Symbol`. | `record_sme_collateral_commitment` (`EscrowError::CollateralAssetEmpty`) |
| 3 | **Monotonic Timestamp** | When replacing an existing pledge, the new `recorded_at` timestamp must not be earlier than the previous one. | `record_sme_collateral_commitment` (`EscrowError::CollateralTimestampBackwards`) |
| 4 | **Metadata‑only Semantics** | Recording collateral does **not** transfer tokens, reserve balances, or block any contract flows (settle, withdraw, claim, refund, etc.). | Documentation & contract design – enforced by the fact that the function only writes to storage and emits an event; no token calls are made. |
| 5 | **Clear Only When Present** | `clear_sme_collateral_commitment` can only be called if a pledge exists. | `clear_sme_collateral_commitment` (`EscrowError::NoCollateralToClear`) |
| 6 | **Single Source of Truth** | The stored `SmeCollateralPledge` is the authoritative record; reading via `get_sme_collateral_commitment` returns the latest pledge or `None`. | Getter function `get_sme_collateral_commitment` and storage key `DataKey::SmeCollateralPledge`. |
| 7 | **Event Payload Consistency** | `CollateralRecordedEvt` always contains the prior amount (if any) and the new amount, enabling off‑chain indexers to track changes. | Emitted in `record_sme_collateral_commitment`; tests verify payload (`tests/integration.rs`). |

---

## State-Transition & Authorization Invariants

The following invariants are owned by this subsystem and must be preserved by any change to the entry points above:

- **Authorization**: `record_sme_collateral_commitment` and `clear_sme_collateral_commitment` require the caller to be the SME authorized for the escrow. Unauthorized callers must fail with the existing auth error before any storage write, so a rejected call cannot mutate the pledge.
- **Atomicity**: Each entry point performs at most one storage write and one event emission. A failed validation (invariants 1–3, 5) must leave the previously stored pledge unchanged; there is no partial update.
- **Determinism**: For identical inputs and identical prior state, the resulting state and emitted event are identical. No randomness, time-of-call dependence, or external reads are used beyond the caller-supplied `recorded_at`.
- **Idempotence of reads**: `get_sme_collateral_commitment` is a pure read; it never mutates state and always returns the current pledge or `None`.
- **Replay resistance**: Because `recorded_at` must be monotonic (invariant 3), replaying an older record call is rejected rather than silently downgrading the stored pledge.
- **Concurrency**: Soroban executes contract entry points serially per ledger, so no interleaving can occur between the validation and the storage write within a single call. Retries of a rejected call are safe because they fail validation before writing.

## Enforcement Locations

- **Function** `record_sme_collateral_commitment` – lines 3037‑3060 in `escrow/src/lib.rs`.
- **Error Codes** – `EscrowError::CollateralAmountNotPositive` (60), `EscrowError::CollateralAssetEmpty` (61), `EscrowError::CollateralTimestampBackwards` (62).
- **Clear Function** `clear_sme_collateral_commitment` – validates presence and emits `CollateralClearedEvt` (error 169 if missing).
- **Getter** `get_sme_collateral_commitment` – safe read‑only accessor.
- **Tests** – see `escrow/src/tests/admin.rs`, `integration.rs`, and `coverage.rs` for invariant checks.

---

## Duplicate & Invalid Input Handling

## Related Entry Points

| Entry Point | Purpose | Relevant Invariant Checks |
|-------------|---------|---------------------------|
| `record_sme_collateral_commitment` | Store/replace collateral metadata. | 1‑3, 4, 7 |
| `clear_sme_collateral_commitment` | Remove existing pledge. | 5 |
| `get_sme_collateral_commitment` | Retrieve current pledge. | 6 |

---

## Failure Modes & Observability

All rejections surface as typed `EscrowError` variants (`CollateralAmountNotPositive`, `CollateralAssetEmpty`, `CollateralTimestampBackwards`, `NoCollateralToClear`) and abort the call without writing storage, so callers and indexers can distinguish validation failures from success. Successful transitions emit `CollateralRecordedEvt` (with prior and new amounts) or `CollateralClearedEvt`; these events are the primary observability signal for off-chain indexers. No sensitive data is included in errors or events beyond the amounts and asset symbol already supplied by the caller.

## Security & Design Notes

- The SME collateral commitment is **off‑chain risk review metadata** only.  Consumers must treat it as advisory information; it provides no on‑chain guarantees of custody or lien.
- Because it does not affect token balances, the contract does not perform any token‑transfer safety checks for this path.
- The monotonic timestamp invariant prevents replay attacks that could otherwise downgrade a previously recorded higher‑value pledge.

Failures on this path must be diagnosable from the emitted error code and event without exposing sensitive SME data. Logs and events must not include off‑chain risk details beyond the amount, asset symbol, and timestamps already defined in the payload.

---

*Document last updated: 2026‑07‑26*
