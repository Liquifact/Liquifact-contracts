# Escrow Lifecycle — State Machine Reference

This document describes the `InvoiceEscrow.status` state machine, valid transitions,
forbidden regressions, and interaction rules between `withdraw` vs `settle` paths.

---

## Status values

| Value | Name | Meaning |
|-------|------|---------|
| `0` | `open` | Escrow is initialized; funding is active |
| `1` | `funded` | At least one investor reached or exceeded the funding target |
| `2` | `settled` | SME has finalized settlement after legal/financial review |
| `3` | `withdrawn` | SME has withdrawn liquidity (pull model, off-chain settlement) |

---

## State diagram

```text
                ┌─────────────┐
                │   (init)    │
                │  status = 0 │
                │    open      │
                └──────┬──────┘
                       │
                       │ fund(amount >= funding_target)
                       ▼
                ┌─────────────┐
                │  funded     │
                │ status = 1  │
                └──────┬──────┘
                       │
         ┌─────────────┼─────────────┐
         │             │             │
         │             │             │
         ▼             ▼             ▼
  ┌──────────┐  ┌──────────┐  ┌──────────┐
  │ settled  │  │ withdrawn│  │  (open)  │  ← more funding if target not met
  │ status=2 │  │ status=3 │
  └──────────┘  └──────────┘  └──────────┘
      (terminal)    (terminal)
```

---

## Valid transitions

| From | To | Trigger | Auth required |
|------|----|---------|--------------|
| `0` (open) | `1` (funded) | `fund()` or `fund_with_commitment()` when `funded_amount >= funding_target` | Investor auth |
| `1` (funded) | `2` (settled) | `settle()` | SME auth; legal hold must be inactive; if `maturity > 0`, ledger timestamp must be >= maturity |
| `1` (funded) | `3` (withdrawn) | `withdraw()` | SME auth; legal hold must be inactive |

---

## Forbidden transitions (must panic)

| From | To | Reason |
|------|----|--------|
| `0` (open) | `1` (funded) | Must reach funding target first |
| `0` (open) | `2` (settled) | Escrow must be funded first |
| `0` (open) | `3` (withdrawn) | Escrow must be funded first |
| `1` (funded) | `0` (open) | Status never regresses |
| `2` (settled) | `0` (open) | Status never regresses |
| `2` (settled) | `1` (funded) | Already past this state |
| `2` (settled) | `3` (withdrawn) | Settle and withdraw are mutually exclusive |
| `3` (withdrawn) | `0` (open) | Status never regresses |
| `3` (withdrawn) | `1` (funded) | Already past this state |
| `3` (withdrawn) | `2` (settled) | Already past this state |

---

## Mutual exclusivity: `withdraw` vs `settle`

`withdraw` and `settle` are **mutually exclusive** terminal paths. Both require:
- `status == 1` (funded)
- No active legal hold
- SME authentication

Once one path is taken, the other is unreachable:
- After `withdraw()` → status is `3`; `settle()` panics
- After `settle()` → status is `2`; `withdraw()` panics

---

## SME auth vs admin role

| Function | Role |
|----------|------|
| `settle()` | SME (off-chain settlement policy, not an EVM `onlyOwner` concept) |
| `withdraw()` | SME |
| `set_legal_hold()` | Admin only |
| `update_maturity()` | Admin only |
| `transfer_admin()` | Admin only |

The SME role represents the off-chain settlement policy authority. The admin role
handles on-chain configuration and compliance controls.

---

## Legal hold interaction

Legal hold blocks all risk-bearing operations regardless of status:

| Function | Blocked by legal hold |
|----------|----------------------|
| `fund()` | Yes |
| `settle()` | Yes |
| `withdraw()` | Yes |
| `claim_investor_payout()` | Yes |

Once legal hold is cleared, normal state transitions resume.

---

## Maturity gate

When `maturity > 0`:
- `settle()` requires `env.ledger().timestamp() >= escrow.maturity`
- When `maturity == 0`: `settle()` succeeds immediately (no time gate)

`withdraw()` does **not** check maturity; it is a pull model for SME liquidity.

---

## Security notes

- **Out of scope:** Non-standard token economics (rebasing, fee-on-transfer).
  See `escrow/src/external_calls.rs` and `docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`.
- **funded_amount** is a non-decreasing i128. Overflow is checked via `checked_add`.
- **Snapshot immutability:** `FundingCloseSnapshot` is written once at the
  `0 → 1` transition and must remain readable after `settle()` or `withdraw()`.