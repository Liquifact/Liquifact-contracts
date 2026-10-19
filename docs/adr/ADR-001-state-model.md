# ADR-001: Escrow State Model

**Status:** Accepted  
**Date:** 2026-03-28  
**Refs:** `escrow/src/lib.rs` — `InvoiceEscrow`, `DataKey::Escrow`, `fund_impl`, `settle`, `withdraw`, `repay`

---

## Context

The escrow needs a clear, auditable lifecycle so that state-changing entrypoints can enforce valid transitions and indexers can reconstruct history from events alone.

## Decision

Use a single `u32` status field on `InvoiceEscrow` with five values:

| Value | Name | Meaning |
|-------|------|---------|
| `0` | open | Accepting investor funding |
| `1` | funded | `funded_amount >= funding_target`; SME may withdraw or settle |
| `2` | settled | SME called `settle`, or a withdrawn escrow was fully repaid via `repay`; investors may claim payout |
| `3` | withdrawn | SME called `withdraw`; the SME may repay on-chain, and full repayment transitions to settled |
| `4` | cancelled | Admin called `cancel_funding`; terminal, investors may refund |

Transitions are forward (`0 → 1 → 2`, `0 → 1 → 3`, `0 → 4`) with one deliberate terminal-repair edge: a fully repaid withdrawn escrow moves `3 → 2`. No other entrypoint moves status backward. The full escrow snapshot is stored under `DataKey::Escrow` and rewritten atomically on every state change.

## Consequences

- Any entrypoint that reads `status` gets a consistent view within a single host function call (Soroban single-writer model).
- `settle` and `withdraw` both require `status == 1`, so they are mutually exclusive entrypoints. `withdraw` moves to `3`; `settle` can never run from `3`, but `repay` can complete the `3 → 2` edge once the settlement pool is fully repaid.
- `fund` is blocked once `status != 0`, preventing post-funded contributions.
- `cancel_funding` is blocked once `status != 0`, preventing cancellation of funded escrows.
- `refund` is only permitted when `status == 4`, allowing principal recovery for cancelled escrows.
- Property test `prop_status_only_increases` enforces monotonicity on the funding/settlement path. The only backward status edge is the deliberate `3 → 2` repayment path, which is covered by dedicated repayment tests.

## Rejected alternatives

- **String/enum status stored as Symbol:** harder to compare in assertions and costs more storage bytes.
- **Separate boolean flags (`is_funded`, `is_settled`):** allows invalid combinations (both true); integer status is unambiguous.
