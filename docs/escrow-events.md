# Escrow Contract Events

This document provides a reference for indexers and block explorers to consume events emitted by the Liquifact Escrow contract.

## 📡 Event Structure

All events follow the Soroban `contractevent` format. Key fields like `invoice_id` and `investor` are marked as **topics** to enable efficient filtering by indexers.

### Common Topics
- **Topic 0**: Contract ID (provided by Soroban host).
- **Topic 1**: Event Name (Symbol, e.g., `funded`, `escrow_sd`).
- **Topic 2**: `invoice_id` (Symbol) — present in most events.
- **Topic 3**: `investor` (Address) — present in funding and claim events.

---

## 📋 Event Catalog

### `EscrowFunded`
Emitted when an investor deposits principal.

**Topics:**
1. `funded` (Symbol)
2. `invoice_id` (Symbol)
3. `investor` (Address)

**Data Payload:**
- `amount` (i128)
- `funded_amount` (i128)
- `status` (u32)
- `investor_effective_yield_bps` (i64)

**Example (JSON Decoded):**
```json
{
  "topics": ["funded", "INV_001", "G...INVESTOR"],
  "data": {
    "amount": "1000000000",
    "funded_amount": "5000000000",
    "status": 0,
    "investor_effective_yield_bps": 500
  }
}
```

### `EscrowSettled`
Emitted when the SME finalizes the escrow after maturity.

**Topics:**
1. `escrow_sd` (Symbol)
2. `invoice_id` (Symbol)

**Data Payload:**
- `funded_amount` (i128)
- `yield_bps` (i64)
- `maturity` (u64)

**Example (JSON Decoded):**
```json
{
  "topics": ["escrow_sd", "INV_001"],
  "data": {
    "funded_amount": "10000000000",
    "yield_bps": 500,
    "maturity": 1714184400
  }
}
```

### `InvestorPayoutClaimed`
Emitted when an investor records their payout claim.

**Topics:**
1. `inv_claim` (Symbol)
2. `invoice_id` (Symbol)
3. `investor` (Address)

**Example (JSON Decoded):**
```json
{
  "topics": ["inv_claim", "INV_001", "G...INVESTOR"],
  "data": null
}
```

### `InvestorAllowlistChanged`
Emitted when an admin adds or removes an investor from the allowlist. This event is
emitted per-address even when the change is performed via the batch entrypoint
`set_investors_allowlisted`, so indexers receive one `InvestorAllowlistChanged` event
for each address in the batch.

**Topics:**
1. `al_set` (Symbol)
2. `invoice_id` (Symbol)
3. `investor` (Address)

**Data Payload:**
- `allowed` (u32): `1` for allowed, `0` for blocked.

**Notes:**
- Batch mutations via `set_investors_allowlisted` emit one `al_set` event per affected
  investor to preserve parity with individual `set_investor_allowlisted` calls.

### `LegalHoldChanged`
Emitted when an admin toggles the compliance hold.

**Topics:**
1. `legalhld` (Symbol)
2. `invoice_id` (Symbol)

**Data Payload:**
- `active` (u32): `1` for enabled, `0` for cleared.

---

## 🛠️ Indexing Recommendations

### Filtering by Invoice
To track all activity for a specific invoice, indexers should filter for events where **Topic 2** matches the `invoice_id`.

### Filtering by Investor
To track an investor's portfolio, filter for events where **Topic 3** matches the investor's `Address`. This applies to `EscrowFunded` and `InvestorPayoutClaimed`.

### Decoding payloads
Payloads are XDR-encoded. Use the `liquifact_escrow` WASM/interface or the `Stellar SDK` to decode the `data` field into the corresponding Rust structs.
