# LiquiFact Escrow — Event Schema Reference

> **Audience**: Backend indexers, analytics engineers, and integration partners
> who consume Stellar ledger meta from Horizon or RPC to reconstruct contract
> history without polling contract storage.

---

## Overview

Every state-changing function in the `LiquifactEscrow` contract emits a
[Soroban contract event](https://developers.stellar.org/docs/smart-contracts/events).
Events are written into the transaction's ledger meta and can be read via:

- **Horizon** — `GET /transactions/{hash}/effects` or event streaming
- **Stellar RPC** — `getEvents` with `contractId` + topic filters
- **Mercury / indexer frameworks** — native Soroban event subscription

Each event has:
- A **two-element topic tuple**: `(namespace: Symbol, action: Symbol)`
- A **typed data payload** encoded as a Soroban XDR `ScVal`

---

## Versioning Strategy

| Scenario | Action |
|---|---|
| **Additive field added** to an existing payload | No version bump — old indexers ignore unknown fields |
| **Field renamed or removed** (breaking change) | Bump `Topic[0]` from `"escrow"` → `"escrow_v2"` |
| **New event action** added | No version bump — indexers filter by `Topic[1]` |
| **Payload type changed** entirely | New `Topic[0]` namespace |

> Indexers **must** filter by both `Topic[0]` (namespace) and `Topic[1]`
> (action) to be stable across future contract upgrades.

---

## Events

### 1. `escrow.initialized` — Escrow Created

Emitted by `init()`. Marks the beginning of an invoice escrow lifecycle.

| Field | Value |
|---|---|
| `Topic[0]` | `"escrow"` |
| `Topic[1]` | `"initd"` *(≤ 8 chars — Soroban `symbol_short` limit)* |
| Payload type | `InvoiceEscrow` |

#### Payload: `InvoiceEscrow`

| Field | Rust type | Description |
|---|---|---|
| `invoice_id` | `Symbol` | Unique invoice ID (e.g. `"INV1023"`) |
| `sme_address` | `Address` | SME wallet receiving the stablecoin |
| `amount` | `i128` | Face value in smallest token unit |
| `funding_target` | `i128` | Target to reach before SME is paid (= `amount` initially) |
| `funded_amount` | `i128` | Always `0` at init |
| `yield_bps` | `i64` | Annualized yield in basis points (e.g. `800` = 8 %) |
| `maturity` | `u64` | Unix timestamp (seconds) for invoice maturity |
| `status` | `u32` | Always `0` (open) at init |

#### Example (JSON representation after XDR decode)

```json
{
  "event"         : "escrow.initialized",
  "invoice_id"    : "INV1023",
  "sme_address"   : "GBSME...",
  "amount"        : 100000000000,
  "funding_target": 100000000000,
  "funded_amount" : 0,
  "yield_bps"     : 800,
  "maturity"      : 1750000000,
  "status"        : 0
}
```

---

### 2. `escrow.funded` — Investor Contribution Recorded

Emitted by `fund()` on **every successful call**, regardless of whether the
target was just met. Use `status == 1` in the payload to detect the moment
the escrow became fully funded.

| Field | Value |
|---|---|
| `Topic[0]` | `"escrow"` |
| `Topic[1]` | `"funded"` |
| Payload type | `FundedPayload` |

#### Payload: `FundedPayload`

| Field | Rust type | Description |
|---|---|---|
| `invoice_id` | `Symbol` | Invoice this contribution belongs to |
| `investor` | `Address` | Wallet that called `fund()` |
| `amount` | `i128` | Amount contributed in **this** call |
| `funded_amount` | `i128` | Cumulative total **after** this call |
| `status` | `u32` | `0` = still open · `1` = target just met |

> **Analytics tip**: Sum `amount` per `invoice_id` across all `funded` events
> to reconstruct the full investor contribution table without reading state.

#### Example

```json
{
  "event"        : "escrow.funded",
  "invoice_id"   : "INV1023",
  "investor"     : "GBINV...",
  "amount"       : 50000000000,
  "funded_amount": 100000000000,
  "status"       : 1
}
```

---

### 3. `escrow.settled` — Invoice Settled by Buyer

Emitted by `settle()` once, when the buyer has paid and the contract marks
the escrow as settled (status 2). Contains everything needed to compute
investor payouts without re-reading contract storage.

| Field | Value |
|---|---|
| `Topic[0]` | `"escrow"` |
| `Topic[1]` | `"settled"` |
| Payload type | `SettledPayload` |

#### Payload: `SettledPayload`

| Field | Rust type | Description |
|---|---|---|
| `invoice_id` | `Symbol` | Invoice that has been settled |
| `funded_amount` | `i128` | Total principal held at settlement |
| `yield_bps` | `i64` | Annualized yield rate for payout calculation |
| `maturity` | `u64` | Original maturity timestamp (used to compute accrued interest) |

> **Payout formula** (off-chain, backend responsibility):
> ```
> gross_yield = funded_amount × (yield_bps / 10_000) × (days_held / 365)
> investor_payout = funded_amount + gross_yield
> ```

#### Example

```json
{
  "event"         : "escrow.settled",
  "invoice_id"    : "INV1023",
  "funded_amount" : 100000000000,
  "yield_bps"     : 800,
  "maturity"      : 1750000000
}
```

---

## Status Code Reference

| Value | Name | Description |
|---|---|---|
| `0` | **open** | Escrow initialized; accepting investor funding |
| `1` | **funded** | Target met; SME can be paid; awaiting buyer settlement |
| `2` | **settled** | Buyer paid; investors can redeem principal + yield |

---

## Topic Filter Cheat Sheet

Use these filters with `getEvents` (Stellar RPC) or Mercury subscriptions:

```json
{
  "contractId": "<CONTRACT_ADDRESS>",
  "topics": [
    ["AAAADwAAAAZlc2Nyb3c=", "AAAADwAAAAVpbml0ZA=="]
  ]
}
```

| Event | Topic[0] (base64 XDR) | Topic[1] (base64 XDR) | Human label |
|---|---|---|---|
| `initialized` | `AAAADwAAAAZlc2Nyb3c=` | `AAAADwAAAAVpbml0ZA==` | `"escrow"` / `"initd"` |
| `funded` | `AAAADwAAAAZlc2Nyb3c=` | `AAAADwAAAAZmdW5kZWQ=` | `"escrow"` / `"funded"` |
| `settled` | `AAAADwAAAAZlc2Nyb3c=` | `AAAADwAAAAdzZXR0bGVk` | `"escrow"` / `"settled"` |

---

## Security Notes

- **No sensitive data in events**: Escrow events intentionally omit off-chain
  identifiers (e.g. buyer email, KYC data). They only expose on-chain addresses
  and amounts already visible in the transaction itself.
- **Events are append-only**: Once emitted, events cannot be mutated or deleted.
  Indexers can treat them as an immutable audit log.
- **Re-org safety**: On a Stellar re-org (rare), events from rolled-back
  transactions are also rolled back. Indexers should confirm ledger closedness
  (via `ledgerVersion` in the ledger meta) before treating events as final.
- **Input validation**: The contract asserts valid state transitions before
  emitting events, so an emitted event always represents a successfully
  committed state change.

---

## Changelog

| Date | Version | Change |
|---|---|---|
| 2026-03-23 | v0.1 | Initial schema — `initialized`, `funded`, `settled` events defined |
