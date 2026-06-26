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

## Token Custody during Funding

To ensure custody is real and on-chain token balances reconcile with `funded_amount`, the contract performs atomic token transfers during funding:

1. **Atomic Transfer:** Every successful call to `fund()`, `fund_with_commitment()`, or `fund_batch()` atomically pulls the specified token amount from the investor's balance to the escrow contract (`env.current_contract_address()`).
2. **Balance-Delta Verification:** The transfer utilizes `external_calls::transfer_funding_token_inbound_with_balance_checks` to read pre/post balances of the investor and the escrow contract. It asserts that:
   - The investor's balance decreased by exactly `amount`.
   - The contract's balance increased by exactly `amount`.
   - Any mismatch or insufficient balance reverts the entire transaction, ensuring no double-credit or state mutation on failure.
3. **Reconciliation Invariant:** The contract's token balance always matches or exceeds `funded_amount` (reclaimed using `refund()` or settled/withdrawn). This ensures that the terminal dust sweep math `balance - sweep_amt >= funded_amount - distributed_principal` remains sound and protected.

---

## Batch funding (`fund_batch`)

`fund_batch(entries: Vec<(Address, i128)>)` processes multiple investor contributions in a single call,
reducing transaction overhead for primary issuance workflows.

**Semantics:**
- Each entry `(investor_address, amount)` is processed sequentially
- Per-investor `require_auth()` is called for each entry
- All existing [`fund()`](funding.md) invariants (allowlist, caps, min contribution, overflow guards)
  are enforced per entry
- One `EscrowFunded` event is emitted per entry
- If any entry fails its invariants, the call returns an error **without corrupting prior entries**
  (Soroban's transaction atomicity ensures consistent state)

**Capacity:**
- Batch size must be `> 0` and `<= MAX_FUND_BATCH` (50 entries)
- Empty batch panics with `EscrowError::FundingBatchEmpty`
- Oversized batch panics with `EscrowError::FundingBatchTooLarge`

**Funded-target snapshot:**
- If any entry causes the escrow to transition to **funded** (status `0 → 1`),
  `FundingCloseSnapshot` is recorded exactly once
- Remaining entries are processed even after the transition

**Example:**
```rust
let entries = vec![
    (investor_a, 30_000i128),
    (investor_b, 55_000i128),
    (investor_c, 10_000i128),
];
let result = fund_batch(entries); // All three funded in one call
```

---

## Valid transitions

| From | To | Trigger | Auth required |
|------|----|---------|--------------|
| `0` (open) | `1` (funded) | `fund()`, `fund_with_commitment()`, or `fund_batch()` when `funded_amount >= funding_target` | Investor auth (per-investor for batch) |
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
| `update_funding_deadline()` | Admin only |
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

## Funding deadline update

`update_funding_deadline(new_deadline: Option<u64>)` allows the admin to set, extend, or clear
the optional funding deadline while the escrow is **open** (status == 0):

| Status | `update_funding_deadline` result |
|--------|----------------------------------|
| 0 — Open | ✅ Allowed |
| 1 — Funded | ❌ Panics: "Funding deadline can only be updated in Open state" |
| 2 — Settled | ❌ Panics: "Funding deadline can only be updated in Open state" |
| 3 — Withdrawn | ❌ Panics: "Funding deadline can only be updated in Open state" |
| 4 — Cancelled | ❌ Panics: "Funding deadline can only be updated in Open state" |

**Validation rules:**
- `Some(d)`: `d` must be strictly greater than the current ledger timestamp (same rule as `init`).
- `None`: removes the deadline entirely; funding becomes unrestricted by time.
- `is_funding_expired()` returns `false` when no deadline is set (key absent from storage).

**Events:** `FundingDeadlineUpdated` carries `invoice_id`, `prior_deadline`, and `new_deadline`.

This is consistent with `update_funding_target` and `update_maturity`: all three are admin-gated,
open-state-only setters that emit a typed event with the prior and new values.

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
