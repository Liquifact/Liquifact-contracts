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
                │  status = 0 │◄──── unfund(investor, amount) [investor]
                │    open     │      (partial or full; status stays 0)
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
- Every investor address must be unique within the batch; a repeated address panics with
  `EscrowError::FundingBatchDuplicateInvestor` (code 84). The entire batch is rejected
  atomically before any state mutation.

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
| Adjacent duplicate → `FundingBatchDuplicateInvestor` (code 84) | `test_fund_batch_rejects_adjacent_duplicate` |
| Non-adjacent duplicate → `FundingBatchDuplicateInvestor` (code 84) | `test_fund_batch_rejects_non_adjacent_duplicate` |
| Single-element batch (no duplicates possible) succeeds | `test_fund_batch_single_element_succeeds` |
| All-unique batch succeeds | `test_fund_batch_all_unique_succeeds` |
| MAX_FUND_BATCH (50) unique entries succeed | `test_fund_batch_max_unique_batch_succeeds` |
| Duplicate batch leaves no partial state | `test_fund_batch_duplicate_leaves_no_partial_state` |

---

## Valid transitions

| From | To | Trigger | Auth required | Notes |
|------|----|---------|--------------|-------|
| `0` (open) | `1` (funded) | `fund()`, `fund_with_commitment()`, or `fund_batch()` when `funded_amount >= funding_target` | Investor auth (per-investor for batch) | |
| `0` (open) | `0` (open) | `unfund(investor, amount)` | Investor auth; legal hold must be inactive | Partial unfund: funded_amount decreases, status stays 0. Full unfund (contribution → 0): UniqueFunderCount decrements, status stays 0 |
| `0` (open) | `4` (cancelled) | `cancel_funding()` | Admin auth; legal hold must be inactive | |
| `1` (funded) | `2` (settled) | `settle()` | SME auth; legal hold must be inactive; if `maturity > 0`, ledger timestamp must be >= maturity | |
| `1` (funded) | `3` (withdrawn) | `withdraw()` | SME auth; legal hold must be inactive | |

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

## Funding deadline — optional open-window expiry

An optional `funding_deadline` (ledger timestamp, `u64`) can be set at `init` via the
`funding_deadline: Option<u64>` parameter. When present:

- **New `fund` / `fund_batch` / `fund_with_commitment` calls are rejected** after the ledger
  timestamp passes the deadline, with `EscrowError::FundingDeadlinePassed` (code 164).
- **`cancel_funding` is NOT blocked by the deadline.** The admin may cancel a stalled
  open escrow at any time, before or after the deadline.
- **Already-funded escrows (status 1) are unaffected.** The deadline gate applies only to
  the open (status 0) state; it cannot retroactively trap funded principal.
- `is_funding_expired()` returns `true` when `deadline` is set and `now > deadline`.
- `get_funding_deadline()` returns `Some(deadline)` or `None` if not configured.

### Boundary semantics

| `now` vs `deadline` | `fund` allowed | `is_funding_expired` |
|---------------------|---------------|----------------------|
| `now < deadline` | ✅ Yes | `false` |
| `now == deadline` | ✅ Yes (inclusive at boundary) | `false` |
| `now > deadline` | ❌ No (`FundingDeadlinePassed`) | `true` |
| No deadline set | ✅ Always | `false` always |

### Typical expiry + cancellation flow

```
1. init(funding_deadline = T)             → status 0, deadline stored
2. investor fund() before T               → status 0 (partial) or 1 (funded)
3. ledger advances past T
4. investor fund() attempt                → rejected: FundingDeadlinePassed (164)
5. admin cancel_funding()                 → status 4 (cancelled)
6. investor refund()                      → principal returned to investor
```

### Validation at init

- `funding_deadline` must be strictly greater than the current ledger timestamp at init
  time (`EscrowError::FundingDeadlinePassed` if deadline <= now).
- When `maturity > 0`, `funding_deadline` must be strictly less than `maturity`
  (`EscrowError::FundingDeadlineBeyondMaturity`).
- `funding_deadline = 0` / `None` means no deadline (open window).

### Security notes

- A passed deadline cannot retroactively cancel or drain an already-funded escrow.
- The deadline gate never blocks `cancel_funding`; admin always retains manual override.
- Ledger time is validator-observed; see `docs/escrow-ledger-time.md` for skew guidance.

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

## Investor unfund path (status 0 — open)

While an escrow is open, investors may reduce or fully exit their principal position
without requiring admin cancellation:

1. Investor calls `unfund(investor, amount)` — decrements `DataKey::InvestorContribution`
   and `InvoiceEscrow::funded_amount` by `amount`.
2. If contribution reaches zero: `DataKey::InvestorContribution` entry is zeroed,
   `DataKey::UniqueFunderCount` is decremented (floor: 0).
3. Status remains 0 (open) in all cases — `unfund` never transitions status.
4. Tokens are returned to the investor via `external_calls::transfer_funding_token_with_balance_checks`
   (SEP-41 balance-delta invariants enforced).
5. `EscrowUnfunded` is emitted with the investor, amount, remaining contribution,
   new funded_amount, and ledger timestamp.

### Invariants

- Investor can only unfund their own contribution; no third-party unfunding.
- `unfund` is blocked while a legal hold is active (`EscrowError::LegalHoldActive`, code 223).
- `unfund` is blocked in any state other than open (0) (`EscrowError::EscrowNotOpen`, code 221).
- `amount` must be ≤ `DataKey::InvestorContribution[investor]` (`EscrowError::OverWithdrawal`, code 222 otherwise).
- `funded_amount` never goes negative (checked arithmetic).
- `UniqueFunderCount` never goes negative (saturating_sub).

### Events emitted

| Event | When |
|-------|------|
| `EscrowUnfunded` | `unfund()` succeeds |

### On-chain vs off-chain custody

`unfund` mirrors `refund` — it always calls
`transfer_funding_token_with_balance_checks`. For on-chain custody escrows, tokens are
returned immediately. For off-chain custody setups, the contract's token balance must
cover outstanding unfund requests, or operators must integrate a separate settlement
layer. This behavior is the same as `refund`.

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
| `cancel_funding()` | Yes |
| `claim_investor_payout()` | Yes |
| `fund()` | Yes |
| `settle()` | Yes |
| `sweep_terminal_dust()` | Yes |
| `unfund()` | Yes |
| `withdraw()` | Yes |

Once legal hold is cleared, normal state transitions resume.

---

## Maturity gate

When `maturity > 0`:
- `settle()` requires `env.ledger().timestamp() >= escrow.maturity`
- When `maturity == 0`: `settle()` succeeds immediately (no time gate)

`withdraw()` does **not** check maturity; it is a pull model for SME liquidity.

## Funding deadline update

`extend_funding_deadline(new_deadline: u64)` allows the admin to **push the funding deadline forward**
while the escrow is **open** (status == 0). Shortening or clearing the deadline is not supported by
this entrypoint.

| Status | `extend_funding_deadline` result |
|--------|----------------------------------|
| 0 — Open | ✅ Allowed when `new_deadline > current` and `< maturity` (when maturity configured) |
| 1 — Funded | ❌ `FundingDeadlineUpdateNotOpen` |
| 2 — Settled | ❌ `FundingDeadlineUpdateNotOpen` |
| 3 — Withdrawn | ❌ `FundingDeadlineUpdateNotOpen` |
| 4 — Cancelled | ❌ `FundingDeadlineUpdateNotOpen` |

**Validation rules:**
- A funding deadline must already be configured (`FundingDeadlineNotSet` otherwise).
- `new_deadline` must be strictly greater than the stored deadline (`FundingDeadlineNotExtended`).
- When `maturity > 0`, `new_deadline` must be strictly less than maturity (`FundingDeadlineBeyondMaturity`).

**Events:** `FundingDeadlineExtended` carries `invoice_id`, `old_deadline`, and `new_deadline`. 

### General deadline setter

`update_funding_deadline(new_deadline: Option<u64>)` is the general-purpose admin setter for
the funding window, consistent with `update_funding_target()` and `update_maturity()`. It is a
superset of `extend_funding_deadline()` and additionally supports setting a deadline where none
was configured at `init`, moving an existing deadline backward while it stays in the future, and
clearing the deadline entirely by passing `None`.

| Status | `update_funding_deadline` result |
|--------|----------------------------------|
| 0 — Open | ✅ Allowed when `new_deadline` is `None`, or `Some(d)` with `d > now` and `d < maturity` (when maturity configured) |
| 1 — Funded | ❌ `FundingDeadlineUpdateNotOpen` |
| 2 — Settled | ❌ `FundingDeadlineUpdateNotOpen` |
| 3 — Withdrawn | ❌ `FundingDeadlineUpdateNotOpen` |
| 4 — Cancelled | ❌ `FundingDeadlineUpdateNotOpen` |

**Validation rules** (mirroring the `funding_deadline` branch of `init`):

- No prior deadline is required; `None` may be replaced with `Some(d)`.
- When `new_deadline` is `Some(d)`, `d` must be strictly greater than the current ledger
  timestamp (`FundingDeadlinePassed`).
- When `maturity > 0`, `d` must be strictly less than maturity (`FundingDeadlineBeyondMaturity`).
- Passing `None` removes the stored key, after which `is_funding_expired()` returns `false`.

**Events:** `FundingDeadlineUpdated` carries `invoice_id`, `old_deadline`, and `new_deadline`.
Either timestamp may be `None`: `old_deadline` is `None` when no deadline was previously set,
and `new_deadline` is `None` when the admin cleared it.

**Choosing between the two:** use `extend_funding_deadline()` when the intent is specifically a
forward-only extension and the stricter guarantees (deadline must exist, must not have elapsed,
must strictly increase) are wanted. Use `update_funding_deadline()` for general configuration.



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
