# ADR-001: Escrow State Model

**Status:** Accepted  
**Date:** 2026-03-28  
**Refs:** `escrow/src/lib.rs` - `InvoiceEscrow`, `DataKey::Escrow`,
`fund_impl`, `partial_settle`, `settle`, `withdraw`, `cancel_funding`, `refund`

---

## Context

The escrow needs a clear, auditable lifecycle so that state-changing entrypoints
can enforce valid transitions and indexers can reconstruct history from events
alone.

## Decision

Use a single `u32` status field on `InvoiceEscrow` with five values:

| Value | Name | Meaning |
|-------|------|---------|
| `0` | open | Accepting investor funding |
| `1` | funded | `funded_amount >= funding_target` or funding was closed early through `partial_settle`; SME may withdraw or settle |
| `2` | settled | SME called `settle`; investors may claim payout |
| `3` | withdrawn | SME called `withdraw`; terminal, no settlement possible |
| `4` | cancelled | Admin called `cancel_funding`; investors may recover principal via `refund` |

Transitions are strictly forward through the funding lifecycle:

- `0 -> 1` through `fund`, `fund_with_commitment`, or `partial_settle`.
- `1 -> 2` through `settle`.
- `1 -> 3` through `withdraw`.
- `0 -> 4` through `cancel_funding`.

`refund` is only available in `status == 4` and keeps the escrow cancelled while
returning recorded investor principal. No entrypoint moves status backward. The
full escrow snapshot is stored under `DataKey::Escrow` and rewritten atomically
on every state change.

See `docs/STATE_MACHINE_IMPLEMENTATION.md` for the transition table, Mermaid
diagram, guards, events, and cancelled-branch liability-floor notes.

## Consequences

- Any entrypoint that reads `status` gets a consistent view within a single host
  function call (Soroban single-writer model).
- `settle` and `withdraw` both require `status == 1`, so they are mutually
  exclusive terminal paths.
- `fund` is blocked once `status != 0`, preventing post-funded contributions.
- `cancel_funding` is blocked once `status != 0`, preventing cancellation after
  funding has closed.
- `refund` requires `status == 4`, so principal recovery is only available after
  explicit cancellation.
- Property test `prop_status_only_increases` enforces the monotonicity invariant
  across arbitrary fund amounts.

## Rejected alternatives

- **String/enum status stored as Symbol:** harder to compare in assertions and
  costs more storage bytes.
- **Separate boolean flags (`is_funded`, `is_settled`):** allows invalid
  combinations (both true); integer status is unambiguous.
