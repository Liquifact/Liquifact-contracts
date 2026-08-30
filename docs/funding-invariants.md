# Funding Invariants

This document enumerates every invariant that the funding subsystem must maintain, and identifies
where each is enforced in the codebase.

---

> **⚠ Accuracy note:** The `unfund` entrypoint (referenced in invariants #1, #3, #4, #8, #12,
> #18, and #24) is **planned but not yet implemented** in the current codebase. Its typed error
> codes (`UnfundEscrowNotOpen` = 220, `OverWithdrawal` = 221, `UnfundLegalHoldActive` = 222)
> are defined in `EscrowError` but the corresponding `pub fn unfund(…)` does not yet exist in
> `escrow/src/lib.rs`. The behaviour described below is the **intended design**; verify against
> the implementation when `unfund` lands.  The tests in `escrow/src/tests/funding.rs` and
> `escrow/src/tests/arithmetic_overflow.rs` exercise the planned behaviour.

## Overview

"Funding" spans all state from the first `fund()` call through the terminal `0→1` status flip,
and covers the accounting rules that downstream paths (`settle`, `withdraw`,
`claim_investor_payout`, `refund`, `sweep_terminal_dust`) rely on. Violating any invariant here
can produce insolvency, double-pays, or incorrect pro-rata distributions.

See also:
- [`escrow-lifecycle.md`](escrow-lifecycle.md) — state machine and transition rules
- [`escrow-pro-rata.md`](escrow-pro-rata.md) — payout formula and rounding policy
- [`escrow-numeric-model.md`](escrow-numeric-model.md) — i128 arithmetic bounds
- [`escrow-investor-caps.md`](escrow-investor-caps.md) — cap semantics and Sybil limitations

---

## 1. Amount Positivity

**Invariant:** Every deposit amount must be strictly positive.

```
amount > 0
```

**Where enforced:**
- `fund_impl` in `escrow/src/lib.rs` — `ensure(&env, amount > 0, EscrowError::FundingAmountNotPositive)` (code 100)
- `fund_batch` pre-validation loop — checks every entry before any state mutation
- `unfund` *(planned)* — amount ≤ 0 falls through the `amount <= contribution` guard and then the explicit zero-amount guard, both emitting `EscrowError::OverWithdrawal` (code 221)

---

## 2. Minimum Contribution Floor

**Invariant:** When `DataKey::MinContributionFloor` is set and positive, every individual deposit
(including follow-on deposits from an existing investor) must be ≥ the floor.

```
if floor > 0: amount >= floor
```

**Where enforced:**
- `fund_impl` in `escrow/src/lib.rs` — `ensure(&env, amount >= floor, EscrowError::FundingBelowMinContribution)` (code 101)
- `fund_batch` pre-validation loop — checks every entry for floor compliance before any `fund_impl` call

**Related entrypoints:**
- `lower_min_contribution_floor` — admin-only; only accepts a strictly smaller positive value;
  only valid in status 0 (`EscrowError::FloorLowerNotOpen` = 173)

---

## 3. funded_amount Conservation (Principal Accounting)

**Invariant:** At all times while the escrow is open (status 0 or 1), `funded_amount` equals the
sum of every investor's stored contribution.

```
escrow.funded_amount = Σ get_contribution(addr) over all addr
```

**Where enforced:**
- `fund_impl` — increments `escrow.funded_amount` by `amount` with `checked_add`, and
  increments `DataKey::InvestorContribution(investor)` by the same `amount`. Both are written
  atomically before the token transfer.
- `unfund` *(planned)* — decrements both `escrow.funded_amount` and `DataKey::InvestorContribution(investor)`
  by `amount` using `checked_sub`, with the contribution guard ensuring `amount ≤ contribution`.
- Property test: `prop_funding_accounting_invariants_issue_325` in
  `escrow/src/tests/properties.rs` verifies this over randomised sequences of fund calls.

**Overflow guards:**
- `funded_amount` uses `checked_add`; overflow emits `EscrowError::FundedAmountOverflow` (code 110)
- Per-investor contribution uses `checked_add`; overflow emits
  `EscrowError::InvestorContributionOverflow` (code 105)

---

## 4. funded_amount Monotonicity

**Invariant:** `funded_amount` never decreases while deposits are being accepted, and never
decreases after the escrow transitions to funded (status 1).

- During funding (status 0): `funded_amount` increases on every successful `fund` call.
- `unfund` *(planned)* is the only path that decreases `funded_amount`, and it is only valid in status 0.
- After status reaches 1, no call can decrease `funded_amount`.

**Where enforced:**
- `fund_impl` — `checked_add` always yields a value ≥ prev; negative amounts are rejected first.
- `unfund` *(planned)* — requires `status == 0` (`EscrowError::UnfundEscrowNotOpen` = 220).
- Property test: `prop_funded_amount_non_decreasing` in `escrow/src/tests/properties.rs`.

---

## 5. init Amount Upper Bound (MAX_INVOICE_AMOUNT)

**Invariant:** The funding target set at init must not exceed `MAX_INVOICE_AMOUNT`.

```
MAX_INVOICE_AMOUNT = (1i128 << 63) - 1 = 9_223_372_036_854_775_807
```

This bound prevents overflow in `compute_investor_payout`. The tightest constraint comes from
step (3) of the payout formula:

```
contribution × settle_pool / total_principal
```

With `contribution = total_principal` and `yield_bps = 10_000`, the product is
`2 × total_principal²`, which must fit in `i128`. Solving gives `total_principal ≤ 2⁶³ − 1`.

**Where enforced:**
- `init` in `escrow/src/lib.rs` — `ensure(&env, amount <= MAX_INVOICE_AMOUNT, EscrowError::AmountExceedsMax)` (code 14)

---

## 6. Per-investor Cap

**Invariant:** When `DataKey::MaxPerInvestorCap` is configured, an investor's cumulative
contribution (including the new deposit) must not exceed the cap.

```
if max_per_investor is Some(cap): prev_contribution + amount <= cap
```

This is enforced across all deposit paths including follow-on deposits by the same investor.

**Where enforced:**
- `fund_impl` — `ensure(&env, new_contribution <= cap, EscrowError::InvestorContributionExceedsCap)` (code 106)

**Related entrypoints:**
- `raise_max_per_investor` — admin-only; only accepts strictly larger value;
  requires the cap to have been configured at init (`EscrowError::MaxPerInvestorCapNotConfigured` = 24)

---

## 7. Unique Investor Cap

**Invariant:** When `DataKey::MaxUniqueInvestorsCap` is configured, the number of distinct
investor addresses with a non-zero contribution must not exceed the cap.

```
if max_unique_investors is Some(cap): UniqueFunderCount < cap (for first-time investors)
```

Existing investors (prev contribution > 0) are never blocked by this check, even after the cap
is fully consumed.

**Where enforced:**
- `fund_impl` — cap check runs only when `prev == 0`:
  `ensure(&env, cur_funder_count < cap, EscrowError::UniqueInvestorCapReached)` (code 107)

**Related entrypoints:**
- `lower_max_unique_investors` — admin-only; new cap must be strictly less than current and ≥
  current `UniqueFunderCount` (`EscrowError::NewCapBelowCurrentFunderCount` = 78)
- `raise_max_unique_investors` — admin-only; new cap must be strictly greater than current

---

## 8. UniqueFunderCount Accuracy

**Invariant:** `DataKey::UniqueFunderCount` equals the number of distinct addresses with a
non-zero `DataKey::InvestorContribution`.

```
UniqueFunderCount = |{ addr : get_contribution(addr) > 0 }|
```

**Where enforced:**
- `fund_impl` — increments the counter **only** when `prev == 0` (first deposit from this
  address). The read is hoisted to avoid a redundant storage read.
- `unfund` *(planned)* — decrements the counter (with `saturating_sub` to prevent underflow) when
  `remaining_contribution == 0`.
- Property test: `prop_funding_accounting_invariants_issue_325` in
  `escrow/src/tests/properties.rs` asserts the count matches the set of non-zero contributors
  after every step.

---

## 9. Allowlist Gate

**Invariant:** When `DataKey::AllowlistActive` is `true`, only addresses present in
`DataKey::InvestorAllowlisted` may fund.

**Where enforced:**
- `fund_impl` — `ensure(&env, Self::is_investor_allowlisted(...), EscrowError::InvestorNotAllowlisted)` (code 104)
  Checked after the escrow is read and legal-hold / status guards pass.

---

## 10. Funding Deadline

**Invariant:** When `DataKey::FundingDeadline` is set, no new deposit is accepted after the
deadline ledger timestamp.

```
if FundingDeadline is Some(d): env.ledger().timestamp() <= d
```

**Where enforced:**
- `fund_impl` — `ensure(&env, env.ledger().timestamp() <= deadline, EscrowError::FundingDeadlinePassed)` (code 164)

**Related entrypoints:**
- `extend_funding_deadline` — admin-only; strictly extends an existing deadline; only valid
  in status 0; new deadline must be < maturity when maturity is set
  (`EscrowError::FundingDeadlineAtOrAfterMaturity` = 218)

---

## 11. Status Gate: Funding Only in Open State

**Invariant:** Deposits are only accepted when `escrow.status == 0` (open).

**Where enforced:**
- `fund_impl` — `require_funding_open(&env, escrow.status)` →
  `guard_status_eq(env, status, 0, EscrowError::EscrowNotOpenForFunding)` (code 103)
- `fund_batch` — each entry is routed through `fund_impl`, which carries the same gate.
- `partial_settle` — transitions status 0 → 1 directly; has its own `guard_status_eq` for
  status 0 (`EscrowError::PartialSettleNotOpen` = 202)

---

## 12. Legal Hold Gate

**Invariant:** No deposit is accepted while a compliance hold is active.

**Where enforced:**
- `fund_impl` — `guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksFunding)` (code 102)
- `unfund` *(planned)* — `ensure(&env, !Self::legal_hold_active(&env), EscrowError::UnfundLegalHoldActive)` (code 222)

---

## 13. Operational Pause Gate

**Invariant:** No deposit is accepted while the operational pause is active.

**Where enforced:**
- `fund_impl` — `ensure(&env, !Self::paused_active(&env), EscrowError::PausedBlocksFunding)` (code 210)
  Checked before `require_auth` to fail fast without recording an auth attempt.

---

## 14. Status Transition: 0 → 1 (Funded)

**Invariant:** The escrow transitions from status 0 (open) to status 1 (funded) **exactly once**,
at the first point where `funded_amount >= funding_target`.

```
status 0 → 1 iff funded_amount >= funding_target (first occurrence)
```

**Where enforced:**
- `fund_impl` — transition check occurs after `funded_amount` is updated:
  `if escrow.status == 0 && escrow.funded_amount >= escrow.funding_target { escrow.status = 1; ... }`
- `update_funding_target` — if lowering the target to `funded_amount` causes the condition to
  become true, the same transition logic runs inline.
- `partial_settle` — forces the transition regardless of `funded_amount vs funding_target`;
  only valid in status 0.
- Property test: `prop_status_only_increases` and `prop_funding_accounting_invariants_issue_325`
  in `escrow/src/tests/properties.rs` assert exactly one transition to funded.

**On-chain signal (issue #913):** Every `0 → 1` transition emits a `FundingStateChanged` event
(topic `fund_st_ch`) immediately after storage is committed and `EscrowFunded` / `EscrowPartialSettle`
/ `FundingTargetUpdated` is published. The event carries `from_status`, `to_status`,
`funded_amount`, `funding_target`, `ledger_timestamp`, and a `trigger` symbol so indexers can
react without buffering per-deposit events. See `docs/EVENT_SCHEMA.md` § `FundingStateChanged`.

---

## 15. FundingCloseSnapshot Immutability

**Invariant:** `DataKey::FundingCloseSnapshot` is written **exactly once**, at the moment the
escrow first reaches status 1. It captures `total_principal = funded_amount` at that exact call
and must never be overwritten.

```
FundingCloseSnapshot.total_principal = funded_amount at first funded transition
Once written: FundingCloseSnapshot is immutable
```

**Where enforced:**
- `fund_impl` — `if !env.storage().instance().has(&DataKey::FundingCloseSnapshot) { ... set ... }`.
  The `has` guard prevents a second write.
- `update_funding_target` — uses the same `has` guard before writing.
- `partial_settle` — uses the same `has` guard.
- Property test: `fuzz_multi_investor_fund_ordering_snapshot_once_only` in
  `escrow/src/tests/properties.rs` verifies across 64 random funding orderings that the snapshot
  is written once and never changes.
- Property test: `snapshot_denominator_consistent_across_all_payout_reads` in
  `escrow/src/tests/properties.rs`.

---

## 16. Tiered Yield: First-Deposit-Only Selection

**Invariant:** An investor's effective yield tier and commitment lock are fixed on the first
deposit and cannot be changed by any subsequent deposit.

- `fund_with_commitment` is only valid when `prev_contribution == 0`.
- Follow-on principal from the same investor must use `fund()`, which reads the already-stored
  `InvestorEffectiveYield` and `InvestorClaimNotBefore` without modifying them.

**Where enforced:**
- `fund_impl` (tiered path) — `ensure(&env, prev == 0, EscrowError::TieredSecondDeposit)` (code 108)
- `fund_impl` (simple path, returning investor) — reads stored yield for event; does not write
  `InvestorEffectiveYield` or `InvestorClaimNotBefore`.

---

## 17. Commitment Lock ≤ Maturity

**Invariant:** When both a commitment lock and a maturity timestamp are configured, the
claim-not-before timestamp for an investor must not exceed the maturity timestamp.

```
if committed_lock_secs > 0 && maturity > 0:
    now + committed_lock_secs <= maturity
```

**Where enforced:**
- `fund_impl` (tiered path) —
  `ensure(&env, claim_nb <= escrow.maturity, EscrowError::CommitmentLockExceedsMaturity)` (code 111)

**Overflow guard:**
- The `now + committed_lock_secs` addition uses `checked_add`; overflow emits
  `EscrowError::InvestorClaimTimeOverflow` (code 109).

---

## 18. SEP-41 Balance-Delta Conservation

**Invariant:** Every token transfer must result in the sender's balance decreasing by exactly
`amount` and the recipient's balance increasing by exactly `amount`.

```
sender_post = sender_pre - amount      (exact)
recipient_post = recipient_pre + amount  (exact)
```

**Where enforced:**
- `external_calls::transfer_into_escrow_with_balance_checks` — inbound (investor → escrow):
  asserts `received == amount` and `spent >= 0`. Called by `fund_impl`.
- `external_calls::transfer_funding_token_with_balance_checks` — outbound (escrow → recipient):
  asserts `spent == amount` and `received == amount`. Called by `withdraw`, `refund`,
  `claim_investor_payout`, `unfund` *(planned)*, and `sweep_terminal_dust`.
- Typed errors: `SenderBalanceDeltaMismatch` (40), `RecipientBalanceDeltaMismatch` (41),
  `SenderBalanceUnderflow` (38), `RecipientBalanceUnderflow` (39).

Fee-on-transfer and rebasing tokens are explicitly out of scope; they will trip these
balance-delta checks and produce a typed error.

---

## 19. fund_batch Atomicity and Duplicate Rejection

**Invariant:** A `fund_batch` call is all-or-nothing with respect to two pre-conditions:

1. **Positivity and floor:** all entries must pass amount > 0 and `amount >= floor` checks before
   any `fund_impl` call mutates state.
2. **Duplicate addresses:** all investor addresses in the batch must be unique; the entire batch
   is rejected before any state mutation if a duplicate is found.

**Where enforced:**
- `fund_batch` pre-validation loop — checks positivity and floor for all `n` entries first.
- `fund_batch` duplicate-address loop — O(n²) pairwise comparison (bounded by `MAX_FUND_BATCH = 50`);
  emits `EscrowError::FundingBatchDuplicateInvestor` (code 84) on the first duplicate found.
- Stateful per-entry guards (cap checks, overflow) are still enforced inside `fund_impl` against
  running accumulated state.

---

## 20. Protocol Fee Conservation

**Invariant:** At `withdraw`, the funded principal is split into an SME payout and a treasury fee
such that no principal is created or destroyed.

```
fee        = funded_amount × protocol_fee_bps / 10_000  (floor)
sme_payout = funded_amount - fee                          (checked_sub)
sme_payout + fee == funded_amount                         (always)
```

Floor rounding means any sub-`10_000`-unit residue stays with the SME. With
`protocol_fee_bps == 0`, no treasury transfer occurs and the SME receives the full
`funded_amount`.

**Where enforced:**
- `withdraw` in `escrow/src/lib.rs` — `checked_mul` and `checked_div` for fee;
  `checked_sub` for net. Overflow emits `EscrowError::WithdrawFeeArithmeticOverflow` (216);
  underflow emits `EscrowError::WithdrawNetArithmeticUnderflow` (217).
- Contract balance sufficiency checked before state mutation:
  `ensure(&env, contract_balance >= amount, EscrowError::InsufficientContractBalance)` (165).

---

## 21. Pro-Rata Aggregate Payout Bound

**Invariant:** The sum of all `compute_investor_payout` values must not exceed the settlement
pool.

```
settle_pool = total_principal + floor(total_principal × yield_bps / 10_000)
Σ payout_i ≤ settle_pool
residue = settle_pool - Σ payout_i ≥ 0
```

For uniform yield, `0 ≤ residue < n_investors` (each floor division drops at most 1 unit).
The residue is swept by `sweep_terminal_dust` after all investors have claimed.

**Where enforced:**
- `compute_investor_payout` uses `checked_mul` / `checked_div` with
  `EscrowError::ComputePayoutArithmeticOverflow` (129) on overflow.
- `claim_investor_payout` — rejects a zero payout with `EscrowError::PayoutZero` (170) before
  transferring.
- Property tests in `escrow/src/tests/properties.rs`:
  - `prop_payout_sum_le_settle_pool` (uniform yield, 2–6 investors)
  - `prop_aggregate_payout_le_settle_pool_tiered` (mixed/tiered yield, snapshot denominator
    consistency)
  - `fuzz_payout_conservation_multi_investor` (64-case fuzz, 1–8 investors, full yield range)

---

## 22. Refund Conservation (Cancelled Escrows)

**Invariant:** In status 4 (cancelled), the total principal returned via `refund` calls must
never exceed `funded_amount`, and no investor can be refunded more than their own contribution.

```
Σ refunded_i ≤ funded_amount
refunded_i ≤ contribution_i
```

`DistributedPrincipal` tracks the running total and must satisfy:

```
DistributedPrincipal ≤ funded_amount
```

When every investor has refunded: `DistributedPrincipal == funded_amount`.

**Where enforced:**
- `refund_impl` — zeroes `InvestorContribution` **before** the token transfer
  (checks-effects-interactions pattern); the zero prevents a second refund.
- `refund_impl` — increments `DistributedPrincipal` with `saturating_add`.
- `sweep_terminal_dust` — enforces the liability floor:
  `balance - sweep_amt >= funded_amount - distributed_principal`
  (`EscrowError::SweepExceedsLiabilityFloor` = 42).

---

## 23. Status Never Regresses

**Invariant:** `escrow.status` is strictly forward. Once a status value is reached, it can only
increase or stay the same; it can never decrease.

Valid transitions only:

| From | To | Trigger |
|------|----|---------|
| 0 | 1 | `fund_impl`, `update_funding_target`, `partial_settle` |
| 0 | 4 | `cancel_funding` |
| 1 | 2 | `settle` |
| 1 | 3 | `withdraw` |

All other transitions produce typed errors (e.g. `EscrowError::EscrowNotOpenForFunding`,
`EscrowError::WithdrawalNotFunded`, `EscrowError::CancelFundingNotOpen`).

**Where enforced:**
- Every state-changing entrypoint uses `guard_status_eq` or `require_funding_open` before
  mutating `escrow.status`.
- Property tests: `prop_status_only_increases`, `prop_no_regression_from_funded_status`,
  `prop_no_regression_after_withdraw` in `escrow/src/tests/properties.rs`.

---

## 24. Remaining Investor Slots Conservation

**Invariant:** When a unique-investor cap is configured, remaining slots must be non-negative
and satisfy:

```
remaining = cap - UniqueFunderCount ≥ 0
```

Repeat deposits by the same investor must not decrement remaining slots.

**Where enforced:**
- `get_remaining_investor_slots` computes this on the fly from stored `cap` and `UniqueFunderCount`.
- `unfund` *(planned)* uses `saturating_sub` when decrementing `UniqueFunderCount`, preventing underflow.
- Property tests: `prop_remaining_slots_conservation_non_underflow`,
  `slots_repeat_deposit_does_not_decrement_remaining`,
  `slots_lower_cap_mid_sequence_invariant` in `escrow/src/tests/properties.rs`.

---

## Entrypoint Cross-Reference

| Invariant | Primary entrypoints |
|-----------|---------------------|
| Amount positivity (#1) | `fund`, `fund_with_commitment`, `fund_batch` |
| Min contribution floor (#2) | `fund`, `fund_with_commitment`, `fund_batch`, `lower_min_contribution_floor` |
| Conservation (#3) | `fund`, `fund_with_commitment`, `fund_batch`, `unfund` *(planned)* |
| Monotonicity (#4) | `fund`, `fund_with_commitment`, `fund_batch`, `unfund` *(planned)* |
| MAX_INVOICE_AMOUNT (#5) | `init` |
| Per-investor cap (#6) | `fund`, `fund_with_commitment`, `fund_batch`, `raise_max_per_investor` |
| Unique investor cap (#7) | `fund`, `fund_with_commitment`, `fund_batch`, `lower_max_unique_investors`, `raise_max_unique_investors` |
| UniqueFunderCount accuracy (#8) | `fund`, `fund_with_commitment`, `fund_batch`, `unfund` *(planned)* |
| Allowlist gate (#9) | `fund`, `fund_with_commitment`, `fund_batch` |
| Funding deadline (#10) | `fund`, `fund_with_commitment`, `fund_batch`, `extend_funding_deadline` |
| Status gate (#11) | `fund`, `fund_with_commitment`, `fund_batch`, `partial_settle` |
| Legal hold gate (#12) | `fund`, `fund_with_commitment`, `fund_batch`, `unfund` *(planned)* |
| Pause gate (#13) | `fund`, `fund_with_commitment`, `fund_batch` |
| Status 0→1 transition (#14) | `fund_impl`, `update_funding_target`, `partial_settle` |
| Snapshot immutability (#15) | `fund_impl`, `update_funding_target`, `partial_settle` |
| Tier first-deposit-only (#16) | `fund_with_commitment`, `fund` |
| Commitment lock ≤ maturity (#17) | `fund_with_commitment` |
| SEP-41 balance-delta (#18) | `fund`, `fund_with_commitment`, `fund_batch`, `withdraw`, `refund`, `claim_investor_payout`, `unfund` *(planned)*, `sweep_terminal_dust` |
| Batch atomicity / dedup (#19) | `fund_batch` |
| Protocol fee conservation (#20) | `withdraw` |
| Pro-rata aggregate bound (#21) | `compute_investor_payout`, `claim_investor_payout` |
| Refund conservation (#22) | `refund`, `refund_batch`, `sweep_terminal_dust` |
| Status never regresses (#23) | all state-mutating entrypoints |
| Remaining slots conservation (#24) | `fund`, `fund_with_commitment`, `fund_batch`, `unfund` *(planned)*, `lower_max_unique_investors`, `raise_max_unique_investors` |
