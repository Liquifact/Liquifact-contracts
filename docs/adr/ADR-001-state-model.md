# ADR-001: Escrow State Model

**Status:** Accepted  
**Date:** 2026-03-28  
**Refs:** `escrow/src/lib.rs` — `InvoiceEscrow`, `DataKey::Escrow`, `fund_impl`, `settle`, `withdraw`

---

## Context

The escrow needs a clear, auditable lifecycle so that state-changing entrypoints can enforce valid transitions and indexers can reconstruct history from events alone.

## Decision

Use a single `u32` status field on `InvoiceEscrow` with four values:

| Value | Name | Meaning |
|-------|------|---------|
| `0` | open | Accepting investor funding |
| `1` | funded | `funded_amount >= funding_target`; SME may withdraw or settle |
| `2` | settled | SME called `settle`; investors may claim payout |
| `3` | withdrawn | SME called `withdraw`; terminal, no settlement possible |

Transitions are strictly forward (`0 → 1 → 2` or `0 → 1 → 3`). No entrypoint moves status backward. The full escrow snapshot is stored under `DataKey::Escrow` and rewritten atomically on every state change.

## Consequences

- Any entrypoint that reads `status` gets a consistent view within a single host function call (Soroban single-writer model).
- `settle` and `withdraw` both require `status == 1`, so they are mutually exclusive terminal paths.
- `fund` is blocked once `status != 0`, preventing post-funded contributions.
- Property test `prop_status_only_increases` enforces the monotonicity invariant across arbitrary fund amounts.

## Rejected alternatives

- **String/enum status stored as Symbol:** harder to compare in assertions and costs more storage bytes.
- **Separate boolean flags (`is_funded`, `is_settled`):** allows invalid combinations (both true); integer status is unambiguous.
