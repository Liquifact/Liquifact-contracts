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
| `4` | `cancelled` | Admin cancelled the escrow before it was funded; investors may reclaim principal via `refund()` |

---

## State diagram

```text
                ┌─────────────┐
                │   (init)    │
                │  status = 0 │
                │    open      │
                └──────┬──────┘
                       │
         ┌─────────────┼──────────────────────┐
         │             │                      │
         │ fund(amount >= funding_target)      │ cancel_funding() [admin]
         ▼             │                      ▼
  ┌─────────────┐      │               ┌─────────────┐
  │  funded     │      │               │  cancelled  │
  │ status = 1  │      │               │  status = 4 │
  └──────┬──────┘      │               └──────┬──────┘
         │             │                      │
  ┌──────┼──────┐      │ (more funding        │ refund(investor) [investor]
  │      │      │      │  if target not met)  │ → returns InvestorContribution
  ▼      ▼      │      │                      ▼
┌────┐ ┌────┐   │      │               (principal returned)
│ 2  │ │ 3  │   └──────┘
│set │ │wd  │
└────┘ └────┘
(terminal)  (terminal)
```

---

## Valid transitions

| From | To | Trigger | Auth required |
|------|----|---------|--------------|
| `0` (open) | `1` (funded) | `fund()` or `fund_with_commitment()` when `funded_amount >= funding_target` | Investor auth |
| `0` (open) | `4` (cancelled) | `cancel_funding()` | Admin auth; legal hold must be inactive |
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
| `1` (funded) | `4` (cancelled) | `cancel_funding` only allowed in Open state |
| `2` (settled) | any | Status never regresses from terminal |
| `3` (withdrawn) | any | Status never regresses from terminal |
| `4` (cancelled) | any | Status never regresses from terminal |

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

## Investor refund path (status 4 — cancelled)

When an escrow is cancelled before reaching its funding target, investors may recover
their principal:

1. Admin calls `cancel_funding()` — transitions `status 0 → 4`. Blocked by legal hold.
2. Each investor calls `refund(investor)` — transfers exactly `DataKey::InvestorContribution`
   back to the investor via `external_calls::transfer_funding_token_with_balance_checks`.
3. `InvestorContribution` is zeroed after transfer (checks-effects-interactions pattern).
4. `DataKey::InvestorRefunded` is set to `true` — `is_investor_refunded()` returns `true`.
5. A second `refund()` call panics with `"no contribution to refund"` (contribution is 0).

### Invariants

- Total refunded ≤ `funded_amount` (each investor can only reclaim their own contribution).
- No double-refund: contribution is zeroed before the token transfer.
- Balance-delta checks enforced by `external_calls` wrapper (SEP-41 conservation).
- `refund()` is blocked in all states except `4` (cancelled).

### Events emitted

| Event | When |
|-------|------|
| `FundingCancelled` | `cancel_funding()` succeeds |
| `InvestorRefundedEvt` | `refund()` succeeds |

---

## SME auth vs admin role

| Function | Role |
|----------|------|
| `settle()` | SME |
| `withdraw()` | SME |
| `cancel_funding()` | Admin only |
| `set_legal_hold()` | Admin only |
| `update_maturity()` | Admin only |
| `propose_admin()` | Admin only |
| `accept_admin()` | Pending admin only |

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
| `cancel_funding()` | Yes |
| `sweep_terminal_dust()` | Yes |

Once legal hold is cleared, normal state transitions resume.

---

## Maturity gate

When `maturity > 0`:
- `settle()` requires `env.ledger().timestamp() >= escrow.maturity`
- When `maturity == 0`: `settle()` succeeds immediately (no time gate)

`withdraw()` does **not** check maturity; it is a pull model for SME liquidity.

---

## Terminal states and dust sweep

`sweep_terminal_dust()` is permitted in all three terminal states:

| Status | Terminal | Dust sweep allowed |
|--------|----------|--------------------|
| `2` (settled) | Yes | Yes |
| `3` (withdrawn) | Yes | Yes |
| `4` (cancelled) | Yes | Yes |

This allows the treasury to recover any rounding residue left after all investors
have been refunded.

---

## Security notes

- **Out of scope:** Non-standard token economics (rebasing, fee-on-transfer).
  See `escrow/src/external_calls.rs` and `docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`.
- **funded_amount** is a non-decreasing i128. Overflow is checked via `checked_add`.
- **Snapshot immutability:** `FundingCloseSnapshot` is written once at the
  `0 → 1` transition and must remain readable after `settle()` or `withdraw()`.
- **Refund double-spend prevention:** `InvestorContribution` is zeroed before the
  token transfer; a second `refund()` call finds contribution `0` and panics.
