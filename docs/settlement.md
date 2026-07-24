# Settlement model and invariants

This document describes the settlement lifecycle implemented by the escrow contract in [escrow/src/lib.rs](../escrow/src/lib.rs) and the behavior encoded by the settlement tests in [escrow/src/tests/settlement.rs](../escrow/src/tests/settlement.rs).

The settlement model is intentionally simple:

- an escrow starts in `open` state,
- funding closes when the target is reached or when `partial_settle` is called,
- once the escrow is `funded`, it can become `settled` only if the current ledger satisfies the maturity and hold gates,
- after settlement, payout claims can be computed and recorded for investors.

---

## 1. State model

The contract uses the `InvoiceEscrow.status` field as the primary lifecycle state:

| Status | Meaning |
|---|---|
| `0` | `open` |
| `1` | `funded` |
| `2` | `settled` |
| `3` | `withdrawn` |
| `4` | `cancelled` |

The settlement path is centered on the transition `open -> funded -> settled`.

### Core settlement data

The settlement behavior is driven by these fields and structures:

- `InvoiceEscrow.funded_amount`: the amount credited to the escrow at funding close.
- `InvoiceEscrow.yield_bps`: the base coupon rate applied to the settlement pool.
- `InvoiceEscrow.maturity`: an optional maturity timestamp. A zero value means there is no maturity lock.
- `InvoiceEscrow.status`: the lifecycle state that gates settlement.
- `FundingCloseSnapshot`: a write-once snapshot captured when the escrow first reaches `status == 1`.
- `SettlementReadiness`: a read-only bundle that summarizes whether settlement would succeed on the current ledger.
- `SettledAt`: an additive storage key recording the ledger timestamp of the successful `settle` call.

The relevant types live in [escrow/src/lib.rs](../escrow/src/lib.rs) and are consumed by the settlement tests in [escrow/src/tests/settlement.rs](../escrow/src/tests/settlement.rs).

---

## 2. Settlement invariants

The implementation enforces the following invariants.

### 2.1 Settlement requires a funded escrow

`settle` only succeeds when `escrow.status == 1`.

If the escrow is still `open`, or if it has already been `settled` or `withdrawn`, settlement is rejected with the typed error `SettlementNotFunded`.

### 2.2 Legal hold blocks settlement

A compliance/legal hold is checked before settlement is allowed. If the hold is active, the entrypoint fails with `LegalHoldBlocksSettlement`.

This is the highest-priority blocker in the readiness gate: even if the escrow is funded and maturity has passed, `ready_now` remains false while the hold is active.

### 2.3 Operational pause blocks settlement

The contract also has an operational pause guard. Settlement is rejected while pause is active with `PausedBlocksSettlement`.

This is orthogonal to the legal hold and is checked before the SME authorization path.

### 2.4 Maturity is enforced only when a maturity lock exists

If `maturity > 0`, settlement requires the current ledger timestamp to be at least the configured maturity.

If `maturity == 0`, the escrow has no maturity lock and settlement is not gated by time.

This is reflected in `SettlementReadiness.maturity_reached`:

- `maturity == 0` means the maturity gate is vacuously satisfied,
- otherwise `maturity_reached` is true only when `now >= maturity`.

### 2.5 Settlement is a one-way transition

Once `settle` succeeds, the escrow transitions to status `2` (`settled`). There is no reverse transition from `settled` back to `funded` or `open` inside the settlement path.

### 2.6 The funding-close snapshot is captured once and reused for settlement math

When the escrow first becomes funded, the contract writes a `FundingCloseSnapshot` containing:

- `total_principal`,
- `funding_target`,
- `closed_at_ledger_timestamp`,
- `closed_at_ledger_sequence`.

That snapshot is the canonical denominator for settlement payout calculations. The contract uses it for both aggregate settlement-pool math and per-investor payout math.

### 2.7 Settlement pool math is deterministic and floor-rounded

The settlement pool is computed from the funding-close snapshot using the base yield configured on the escrow:

```text
coupon       = floor(total_principal × yield_bps / 10_000)
settle_pool  = total_principal + coupon
```

This arithmetic is the same one used by `compute_investor_payout` for individual investor claims. The contract uses checked integer multiplication and division so overflows fail with `ComputePayoutArithmeticOverflow` instead of silently returning a wrong value.

### 2.8 Readiness mirrors the settlement gate

`get_settlement_readiness` returns a `SettlementReadiness` object whose `ready_now` field is derived from the same gate as `settle` and `partial_settle`.

In other words:

- `ready_now == true` is a reliable predictor that `settle` will succeed on the current ledger,
- `ready_now == false` means settlement is blocked by a hold, non-funded status, or an unmet maturity gate.

---

## 3. Entrypoints that participate in settlement

### `partial_settle`

`partial_settle` is an early funding-close entrypoint. It is available to the SME or admin and closes funding early while the escrow is still `open`.

Effectively it:

- requires the caller to be the SME or the admin,
- rejects the call while a legal hold is active,
- transitions the escrow from `open` to `funded`,
- writes a `FundingCloseSnapshot` if one did not already exist.

This makes the escrow settleable even when the full funding target was not reached.

### `settle`

`settle` is the main settlement entrypoint.

It requires:

- the escrow to be `funded`,
- no active legal hold,
- no active pause,
- the SME authorization path,
- maturity to be satisfied if a maturity lock exists.

On success it:

- transitions the escrow status from `1` to `2`,
- writes `SettledAt`,
- publishes an `EscrowSettled` event with the settlement pool and settlement timestamp.

### `is_settleable`

A read-only helper that returns whether `settle` would currently succeed.

### `get_settlement_readiness`

A bundled read-only view that reports:

- `is_settleable`,
- `legal_hold_active`,
- `maturity_reached`,
- `ready_now`.

This is the most convenient off-chain view when an integrator needs a single answer about whether settlement is currently possible.

### `get_settlement_pool`

A read-only view of the aggregate settlement pool owed by the SME. It returns `0` before funding because the settlement snapshot is not yet available.

### `compute_investor_payout`

A per-investor view that computes the payout owed to a specific investor after settlement. It uses the same settlement-pool math as `settle`, but scales the amount by the investor’s contribution relative to the funding-close snapshot total principal.

### `claim_investor_payout` and `claim_payouts_batch`

These entrypoints are settlement-adjacent: they consume the settlement model by allowing a funded-and-settled escrow to expose payout claims for investors.

---

## 4. Worked example

Suppose an escrow is initialized with:

- `funded_amount = 1_000_000_000`
- `yield_bps = 500` (5%)
- `maturity = 0` (no maturity lock)

The contract computes the coupon as:

```text
coupon = floor(1_000_000_000 × 500 / 10_000)
       = 50_000_000
```

The settlement pool is therefore:

```text
settle_pool = 1_000_000_000 + 50_000_000
            = 1_050_000_000
```

If a single investor contributed the full principal, that investor’s payout is the full `settle_pool`.

If two investors contributed proportionally, each investor’s payout is scaled from the same pool according to their contribution share. That is what `compute_investor_payout` enforces.

---

## 5. Auditor notes

For audits and off-chain indexing, the most important facts are:

1. Settlement is gated by the combination of funded status, maturity, and hold/pause conditions.
2. The settlement pool is derived from the funding-close snapshot and the base `yield_bps`.
3. The funding-close snapshot is written once when the escrow first becomes funded.
4. `get_settlement_readiness` is the canonical readiness view and should be treated as the authoritative off-chain approximation of the `settle` gate.
