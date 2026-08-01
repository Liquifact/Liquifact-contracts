# `set_protocol_fee_bps` — Admin Fee Setter

This document describes the `set_protocol_fee_bps` entrypoint added by
[issue #1094](https://github.com/Liquifact/Liquifact-contracts/issues/1094),
which allows an admin to update the protocol fee rate within a pre-configured
operator ceiling.

---

## Motivation

`init` writes `protocol_fee_bps` once and the original design had no update path.
Issue #1094 adds a post-init setter so operators can adjust the fee rate without
redeploying the contract, while keeping the ceiling (`set_fees_limit`) under
separate admin control.

---

## Entrypoints

### `set_fees_limit(limit: i64)`

| Field | Value |
|-------|-------|
| **Auth** | Admin (`admin.require_auth()`) |
| **Parameter** | `limit`: max allowed `protocol_fee_bps` value, in basis points |
| **Validation** | Must satisfy `0 ≤ limit ≤ 10_000` |
| **Rejection** | Outside range → typed error |
| **Effect** | Stores `limit` under `DataKey::FeesLimit`; subsequent `set_protocol_fee_bps` calls are validated against this ceiling |

### `get_fees_limit() → i64`

Read-only. Returns the stored fees limit, or `0` if never set.

### `set_protocol_fee_bps(fee_bps: i64)`

| Field | Value |
|-------|-------|
| **Auth** | Admin (`admin.require_auth()`) |
| **Parameter** | `fee_bps`: new protocol fee rate in basis points |
| **Validation** | Must satisfy `0 ≤ fee_bps ≤ get_fees_limit()` |
| **Rejection** | Outside range → `EscrowError::ProtocolFeeBpsOutOfRange` (215) |
| **Effect** | Overwrites `DataKey::ProtocolFeeBps`; emits `ProtocolFeeUpdated` |

### `get_protocol_fee_bps() → i64`

Read-only. Returns the stored protocol fee rate, or `0` for legacy/unset instances.

---

## Guard ordering for `set_protocol_fee_bps`

1. Escrow must be initialized — `get_admin` present
2. `admin.require_auth()` — Soroban host auth check
3. `ProtocolFeeBpsOutOfRange` (215) — `fee_bps < 0` or `fee_bps > get_fees_limit()`
4. Storage write: `DataKey::ProtocolFeeBps ← fee_bps`
5. Event emission: `ProtocolFeeUpdated { old_fee_bps, new_fee_bps }`

---

## Event: `ProtocolFeeUpdated`

Emitted by `set_protocol_fee_bps` on every successful call.

| Field | Type | Meaning |
|-------|------|---------|
| `name` (topic) | `Symbol` | `"fee_upd"` |
| `invoice_id` (topic) | `Symbol` | Escrow invoice identifier |
| `old_fee_bps` | `i64` | Fee rate before this update |
| `new_fee_bps` | `i64` | Fee rate after this update |

---

## Two-knob model

```
Operator ceiling (set_fees_limit)   ≥   Active fee rate (set_protocol_fee_bps)
         │                                           │
         │ admin sets once                          │ admin updates as needed
         │ (governance-level)                        │ (operational-level)
         └────────────────────────────────────────────────┘
```

`set_fees_limit` bounds the maximum fee the admin can ever impose; lowering
it is a one-way governance decision (unless raised again). `set_protocol_fee_bps`
can be adjusted freely within that ceiling.

---

## Conservation invariant

The fee split computed at `withdraw` time uses the **stored** `DataKey::ProtocolFeeBps`
value at that moment, not the value at init. The conservation invariant
`fee + sme_net == funded_amount` continues to hold regardless of how many times
the fee rate was updated between `init` and `withdraw`.

---

## Cross-references

- [`docs/fees-auth.md`](fees-auth.md) — authorization rules (reflects pre-setter design; this doc is additive)
- [`docs/fees-errors.md`](fees-errors.md) — error codes including `ProtocolFeeBpsOutOfRange` (215)
- [`docs/fees-states.md`](fees-states.md) — state machine overview
- [`docs/escrow-fee-split-conservation.md`](escrow-fee-split-conservation.md) — conservation invariant proofs
