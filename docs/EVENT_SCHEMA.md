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

> **Source-truth note:** This document is secondary to the storage schema
> documentation in `README.md`. The current branch contains event-model drift,
> so any event statement that depends on unsettled storage or API shape should be
> read as provisional until the contract source is normalized.

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

### 1. `escrow_ii` — Escrow Created

Emitted by `init()`. Marks the beginning of an invoice escrow lifecycle.

| Field | Value |
|---|---|
| Topic[0] | `"escrow_ii"` (`symbol_short!("escrow_ii")`) |
| Payload type | `EscrowInitialized` |

#### Payload: `EscrowInitialized`

| Field | Rust type | Description |
|---|---|---|
| `escrow` | `InvoiceEscrow` | Full escrow snapshot at init |
| `funding_token` | `Address` | Bound SEP-41 token; equals `DataKey::FundingToken` |
| `treasury` | `Address` | Bound treasury; equals `DataKey::Treasury` |
| `registry` | `Option<Address>` | Optional registry hint; equals `DataKey::RegistryRef` (`None` when unset) |

**Compatibility:** Additive fields on `EscrowInitialized` — old indexers may ignore
`funding_token`, `treasury`, and `registry`; new indexers can bootstrap bound references
from this single event without follow-up `get_funding_token` / `get_treasury` polls.

#### Nested `InvoiceEscrow` fields (within `escrow`)

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

The live source currently contains additional schema drift around `admin`,
`settled_amount`, and version bookkeeping. Reviewers should use the README
storage section for the canonical persisted-layout discussion.

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
  "status"        : 0,
  "funding_token" : "CTOKEN...",
  "treasury"      : "GTREAS...",
  "registry"      : null
}
```

---

### 1a. `inv_cap` — Investor Cap Lowered

Emitted by `lower_max_unique_investors()` when admin tightens the distinct-investor limit
while the escrow is **open** (status `0`). Added in Issue #255.

| Field | Value |
|---|---|
| Topic[0] | `"inv_cap"` (`symbol_short!("inv_cap")`) |
| Topic[1] | `invoice_id` (Symbol) |
| Payload type | `MaxUniqueInvestorsCapLowered` |

#### Payload: `MaxUniqueInvestorsCapLowered`

| Field | Rust type | Description |
|---|---|---|
| `name` | `Symbol` | Always `"inv_cap"` |
| `invoice_id` | `Symbol` | Invoice whose cap was lowered |
| `old_cap` | `u32` | Previous `MaxUniqueInvestorsCap` value |
| `new_cap` | `u32` | New (lower) cap value now stored |

#### Example (JSON representation after XDR decode)

```json
{
  "event"     : "inv_cap",
  "invoice_id": "INV1023",
  "old_cap"   : 10,
  "new_cap"   : 5
}
```

---

### 2. `funded` — Investor Contribution Recorded

Emitted by `fund()` and `fund_with_commitment()` on **every successful call**, regardless of
whether the target was just met. Use `status == 1` in the payload to detect the moment
the escrow became fully funded.

| Field | Value |
|---|---|
| Topic[0] | `"funded"` (`symbol_short!("funded")`) |
| Topic[1] | `invoice_id` (Symbol) |
| Topic[2] | `investor` (Address) |
| Payload type | `EscrowFunded` |

#### Payload: `FundedPayload` (intended view)

| Field | Rust type | Description |
|---|---|---|
| `invoice_id` | `Symbol` | Invoice this contribution belongs to |
| `investor` | `Address` | Wallet that called `fund()` |
| `amount` | `i128` | Amount contributed in **this** call |
| `funded_amount` | `i128` | Cumulative total **after** this call |
| `status` | `u32` | `0` = still open · `1` = target just met |

Current source drift also includes an `is_paid` field on the event struct that
is not reflected consistently across the rest of the contract.

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

Emitted by `settle()` once, when the SME finalizes the escrow after maturity (status → 2).
Contains everything needed to compute investor payouts without re-reading contract storage.

| Field | Value |
|---|---|
| Topic[0] | `"escrow_sd"` (`symbol_short!("escrow_sd")`) |
| Topic[1] | `invoice_id` (Symbol) |
| Payload type | `EscrowSettled` |

#### Payload: `SettledPayload` (intended view)

| Field | Rust type | Description |
|---|---|---|
| `invoice_id` | `Symbol` | Invoice that has been settled |
| `funded_amount` | `i128` | Total principal held at settlement |
| `yield_bps` | `i64` | Annualized yield rate for payout calculation |
| `maturity` | `u64` | Original maturity timestamp (used to compute accrued interest) |

The current branch's settlement implementation is internally inconsistent, so
this event description should not be treated as a stronger source of truth than
the contract file itself.

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
| `3` | **withdrawn** | Explicitly written by `withdraw`, though not documented consistently elsewhere |

---

## Topic Filter Cheat Sheet

Use these filters with `getEvents` (Stellar RPC) or Mercury subscriptions.

> **Important:** Filter by `Topic[0]` (the event name Symbol) for each event type.
> The contract does **not** use a shared namespace topic — each event struct has its
> own `symbol_short!` name as the first topic.

| Event | Topic[0] symbol | Human label |
|---|---|---|
| Escrow created | `escrow_ii` | `EscrowInitialized` |
| Investor funded | `funded` | `EscrowFunded` |
| Escrow settled | `escrow_sd` | `EscrowSettled` |
| SME withdrew | `sme_wd` | `SmeWithdrew` |
| Investor claimed | `inv_claim` | `InvestorPayoutClaimed` |
| Dust swept | `dust_sw` | `TreasuryDustSwept` |
| Legal hold changed | `legalhld` | `LegalHoldChanged` |
| Funding target updated | `fund_tgt` | `FundingTargetUpdated` |
| Maturity updated | `maturity` | `MaturityUpdatedEvent` |
| Admin transferred | `admin` | `AdminTransferredEvent` |
| Collateral recorded | `coll_rec` | `CollateralRecordedEvt` |
| Primary attestation bound | `att_bind` | `PrimaryAttestationBound` |
| Attestation log appended | `att_app` | `AttestationDigestAppended` |
| Investor cap lowered | `inv_cap` | `MaxUniqueInvestorsCapLowered` |
| Allowlist enabled/disabled | `al_ena` | `AllowlistEnabledChanged` |
| Investor allowlist changed | `al_set` | `InvestorAllowlistChanged` |

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
| 2026-05-27 | v0.2 | **Issue #251** — Extended `EscrowInitialized` payload with `funding_token`, `treasury`, `registry` fields (additive; backward-compatible) |
| 2026-05-27 | v0.2 | **Issue #255** — Added `MaxUniqueInvestorsCapLowered` (`inv_cap`) event for `lower_max_unique_investors` |
| 2026-05-27 | v0.2 | **Doc fix** — Corrected topic cheat sheet: replaced stale `"escrow"/"initd"` with actual per-event `symbol_short!` names; added complete event catalogue |
