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
  `FundingCloseSnapshot` is recorded exactly once at the crossing entry
- Remaining entries continue to be processed even after the transition
- The snapshot's `total_principal` reflects `funded_amount` at the exact entry that crossed
  the threshold, not the final batch total

**Example:**
```rust
let entries = vec![
    (investor_a, 30_000i128),
    (investor_b, 55_000i128), // crosses funding_target = 80_000 → snapshot written here
    (investor_c, 10_000i128), // processed post-transition; contribution recorded
];
let result = fund_batch(entries); // All three processed; status = 1
```

**Test coverage** (see `escrow/src/tests/funding.rs`):

| Scenario | Test |
|----------|------|
| N-entry batch == N sequential `fund` calls (funded_amount, contributions, UniqueFunderCount) | `test_fund_batch_equivalence_funded_amount_contributions_and_unique_count` |
| Equivalence holds when batch crosses target | `test_fund_batch_equivalence_when_batch_crosses_target` |
| Snapshot written once, immutable, crossing-entry total captured | `test_fund_batch_mid_batch_transition_snapshot_written_exactly_once` |
| First entry crosses target; snapshot immutable | `test_fund_batch_first_entry_crosses_target_snapshot_immutable` |
| Snapshot captures correct ledger timestamp/sequence | `test_fund_batch_snapshot_captures_ledger_time` |
| Entries after funded transition are processed | `test_fund_batch_entries_after_transition_are_processed` |
| `FundingBatchEmpty` typed error | `test_fund_batch_empty_yields_typed_error` |
| `FundingBatchTooLarge` typed error | `test_fund_batch_too_large_yields_typed_error` |
| Exactly MAX_FUND_BATCH (50) entries succeeds | `test_fund_batch_exactly_max_batch_size_succeeds_and_counts_all_investors` |
| Zero-amount entry → `FundingAmountNotPositive` | `test_fund_batch_zero_amount_entry_yields_typed_error` |
| Below min-contribution floor → `FundingBelowMinContribution` | `test_fund_batch_below_min_contribution_floor_yields_typed_error` |
| Per-investor cap enforced per entry | `test_fund_batch_per_investor_cap_enforced_per_entry_typed_error` |
| Same investor twice accumulates; cap still enforced | `test_fund_batch_same_investor_accumulates_and_cap_enforced` |
| Max unique investors cap enforced inside batch | `test_fund_batch_unique_investor_cap_enforced_inside_batch` |
| Legal hold blocks batch | `test_fund_batch_blocked_by_legal_hold` |
| Allowlist gate blocks non-allowlisted entry | `test_fund_batch_blocked_by_allowlist_gate` |
| All allowlisted entries succeed | `test_fund_batch_succeeds_when_all_entries_allowlisted` |
| Batch rejected when escrow already funded | `test_fund_batch_rejected_after_escrow_already_funded` |
| Unique count increments once per address | `test_fund_batch_unique_count_incremented_once_per_investor` |
| Sequential batches don't double-count existing investors | `test_fund_batch_sequential_batches_unique_count_does_not_double_count` |
| Over-funding single entry | `test_fund_batch_overfunding_single_entry` |
| Over-funding across two entries; snapshot correct | `test_fund_batch_overfunding_across_two_entries_snapshot_correct` |
| Per-investor `require_auth` recorded for each entry | `test_fund_batch_investor_auth_recorded_for_each_entry` |
| Event count == entry count | `test_fund_batch_event_count_matches_entry_count` |

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
   **Only status 0 (open) is cancellable**; funded (1), settled (2), withdrawn (3), and
   already-cancelled (4) escrows reject with `CancelFundingNotOpen` (code 141). See
   `test_cancel_funding_transition_matrix_and_refund_unlock` in
   [`escrow/src/tests/integration.rs`](../escrow/src/tests/integration.rs) for the full matrix.
2. Each investor calls `refund(investor)` — transfers exactly `DataKey::InvestorContribution`
   back to the investor via `external_calls::transfer_funding_token_with_balance_checks`.
3. `InvestorContribution` is zeroed after transfer (checks-effects-interactions pattern).
4. `DataKey::DistributedPrincipal` is incremented by the refunded amount. This feeds the `sweep_terminal_dust` liability floor.
5. `DataKey::InvestorRefunded` is set to `true` — `is_investor_refunded()` returns `true`.
6. A second `refund()` call panics with `"no contribution to refund"` (contribution is 0).

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
