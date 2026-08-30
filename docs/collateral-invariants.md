# Collateral Invariants

This document enumerates the invariants that must always hold for the **SME collateral commitment** metadata in the LiquiFact escrow contract.

---

## Overview

The escrow contract allows the SME (Small‑Medium Enterprise) to record optional collateral information via the entrypoint:

- `record_sme_collateral_commitment`
- `clear_sme_collateral_commitment`
- `get_sme_collateral_commitment`

The recorded data is stored in the instance storage key `DataKey::SmeCollateralPledge` and emitted in the event `CollateralRecordedEvt`.  It is **metadata‑only** and does **not** move tokens, lock assets, or affect any settlement or withdrawal logic.

---

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

## Enforcement Locations

- **Function** `record_sme_collateral_commitment` – lines 3037‑3060 in `escrow/src/lib.rs`.
- **Error Codes** – `EscrowError::CollateralAmountNotPositive` (60), `EscrowError::CollateralAssetEmpty` (61), `EscrowError::CollateralTimestampBackwards` (62).
- **Clear Function** `clear_sme_collateral_commitment` – validates presence and emits `CollateralClearedEvt` (error 169 if missing).
- **Getter** `get_sme_collateral_commitment` – safe read‑only accessor.
- **Tests** – see `escrow/src/tests/admin.rs`, `integration.rs`, and `coverage.rs` for invariant checks.

---

## Related Entry Points

| Entry Point | Purpose | Relevant Invariant Checks |
|-------------|---------|---------------------------|
| `record_sme_collateral_commitment` | Store/replace collateral metadata. | 1‑3, 4, 7 |
| `clear_sme_collateral_commitment` | Remove existing pledge. | 5 |
| `get_sme_collateral_commitment` | Retrieve current pledge. | 6 |

---

## Security & Design Notes

- The SME collateral commitment is **off‑chain risk review metadata** only.  Consumers must treat it as advisory information; it provides no on‑chain guarantees of custody or lien.
- Because it does not affect token balances, the contract does not perform any token‑transfer safety checks for this path.
- The monotonic timestamp invariant prevents replay attacks that could otherwise downgrade a previously recorded higher‑value pledge.

---

*Document last updated: 2026‑07‑26*
