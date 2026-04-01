# ADR-004: Legal / Compliance Hold

**Status:** Accepted  
**Date:** 2026-03-28  
**Refs:** `escrow/src/lib.rs` — `set_legal_hold`, `clear_legal_hold`, `legal_hold_active`, `DataKey::LegalHold`

---

## Context

Regulatory or compliance events may require freezing an escrow mid-lifecycle without destroying state. The mechanism must be governance-controlled and must not be bypassable by any other role.

## Decision

A single boolean stored under `DataKey::LegalHold` (defaults to `false` when absent). Only `admin` can set or clear it via `set_legal_hold(active: bool)` / `clear_legal_hold()`.

When active, the following entrypoints panic immediately:

- `fund` / `fund_with_commitment`
- `settle`
- `withdraw`
- `claim_investor_payout`
- `sweep_terminal_dust`

Read-only entrypoints (`get_escrow`, `get_contribution`, etc.) are never blocked.

There is no timelock or automatic expiry — clearing always requires an explicit admin call. Production deployments should use a multisig or DAO as `admin` so holds cannot be used to strand funds without governance approval.

## Consequences

- A hold can be applied at any lifecycle stage, including open (blocks new funding) and funded (blocks settlement and claims).
- There is no "break glass" path outside of the admin key — operational recovery playbooks must live off-chain.
- `LegalHoldChanged` event is emitted on every set/clear so indexers can reconstruct hold history.

## Rejected alternatives

- **Timelock on hold:** adds complexity and a false sense of safety; governance should decide duration.
- **Separate hold roles (compliance officer vs admin):** out of scope for v1; can be added via `transfer_admin` to a multisig that includes a compliance key.
