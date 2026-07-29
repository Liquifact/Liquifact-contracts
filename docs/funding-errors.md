# Funding Error Codes Reference

**Document Version:** 1.0  
**Contract Version:** Schema 6  
**Last Updated:** 2026-07-27

This document catalogs all typed [`EscrowError`](../escrow/src/lib.rs) codes emitted by funding-related entrypoints in the LiquiFact escrow contract. Integration clients must branch on these numeric codes (not panic strings) for robust error handling.

---

## Table of Contents

1. [Funding Entrypoints](#funding-entrypoints)
2. [Error Codes by Category](#error-codes-by-category)
3. [Detailed Error Reference](#detailed-error-reference)
4. [Common Error Patterns](#common-error-patterns)
5. [Client Integration Guide](#client-integration-guide)

---

## Funding Entrypoints

| Entrypoint | Description | Typical Use |
|------------|-------------|-------------|
| `fund` | Deposit principal from an investor | Standard funding flow for returning investors |
| `fund_with_commitment` | First deposit with optional lock period | Initial funding with tiered yield selection |
| `fund_batch` | Batch-record multiple investor deposits | Gas-efficient multi-investor funding |
| `unfund` | Withdraw principal before escrow closes | Investor exits open escrow |

All entrypoints share a common error surface with entrypoint-specific additions.

---

## Error Codes by Category

### Amount & Bounds Validation (100-111)

| Code | Error Variant | When Fired | Severity |
|------|---------------|------------|----------|
| 100 | `FundingAmountNotPositive` | `amount <= 0` | ❌ Client Error |
| 101 | `FundingBelowMinContribution` | `amount < min_contribution_floor` | ❌ Client Error |
| 105 | `InvestorContributionOverflow` | `prev + amount` overflows `i128` | ⚠️ Arithmetic Guard |
| 106 | `InvestorContributionExceedsCap` | `new_contribution > max_per_investor_cap` | ❌ Client Error |
| 107 | `UniqueInvestorCapReached` | New investor but `funder_count >= max_unique_investors_cap` | ❌ Client Error |
| 108 | `TieredSecondDeposit` | `fund_with_commitment` called when `prev > 0` | ❌ Client Error |
| 109 | `InvestorClaimTimeOverflow` | `now + committed_lock_secs` overflows `u64` | ⚠️ Arithmetic Guard |
| 110 | `FundedAmountOverflow` | `funded_amount + amount` overflows `i128` | ⚠️ Arithmetic Guard |
| 111 | `CommitmentLockExceedsMaturity` | `claim_not_before > maturity` | ❌ Client Error |

### State & Status Guards (102-104, 164, 210, 220-222)

| Code | Error Variant | When Fired | Severity |
|------|---------------|------------|----------|
| 102 | `LegalHoldBlocksFunding` | Compliance hold active | 🔒 Hold Active |
| 103 | `EscrowNotOpenForFunding` | `status != 0` (not open) | ❌ Client Error |
| 104 | `InvestorNotAllowlisted` | Allowlist active but investor not on list | ❌ Client Error |
| 164 | `FundingDeadlinePassed` | `now > funding_deadline` | ❌ Client Error |
| 210 | `PausedBlocksFunding` | Operational pause active | ⏸️ Pause Active |
| 220 | `UnfundEscrowNotOpen` | `unfund` called when `status != 0` | ❌ Client Error |
| 222 | `UnfundLegalHoldActive` | `unfund` blocked by legal hold | 🔒 Hold Active |

### Batch Operations (82-84)

| Code | Error Variant | When Fired | Severity |
|------|---------------|------------|----------|
| 82 | `FundingBatchEmpty` | `entries.len() == 0` | ❌ Client Error |
| 83 | `FundingBatchTooLarge` | `entries.len() > MAX_FUND_BATCH` (50) | ❌ Client Error |
| 84 | `FundingBatchDuplicateInvestor` | Duplicate address in batch | ❌ Client Error |

### Unfund-Specific (221)

| Code | Error Variant | When Fired | Severity |
|------|---------------|------------|----------|
| 221 | `OverWithdrawal` | `amount > investor_contribution` | ❌ Client Error |

---

## Detailed Error Reference

### Code 100: `FundingAmountNotPositive`

**Entrypoints:** `fund`, `fund_with_commitment`, `fund_batch` (per-entry)

**Trigger:** `amount <= 0` passed to any funding entrypoint.

**Why Rejected:** Zero or negative principal is meaningless — investors must deposit a positive amount.

**Client Fix:** Pass `amount > 0`.

**Example:**
```rust
// ❌ Will fail with code 100
client.fund(&investor, &0);
client.fund(&investor, &-1000);

// ✅ Correct
client.fund(&investor, &1_000_000);
```

---

### Code 101: `FundingBelowMinContribution`

**Entrypoints:** `fund`, `fund_with_commitment`, `fund_batch` (per-entry)

**Trigger:** `amount < min_contribution_floor` when floor is configured (> 0).

**Why Rejected:** Admin set a minimum deposit size to prevent dust spam or excessive per-call overhead.

**Client Fix:** Increase `amount` to meet or exceed the floor.

**Query:** `get_min_contribution_floor()` returns current floor (0 = no floor).

**Example:**
```rust
let floor = client.get_min_contribution_floor();
if floor > 0 && amount < floor {
    return Err("Deposit below minimum floor");
}
```

---

### Code 102: `LegalHoldBlocksFunding`

**Entrypoints:** `fund`, `fund_with_commitment`, `fund_batch`, `unfund`

**Trigger:** `DataKey::LegalHold == true`.

**Why Rejected:** Compliance/governance freeze — all fund movements blocked.

**Client Fix:** Wait for admin to call `clear_legal_hold()`. No investor action can bypass.

**Query:** `get_legal_hold()` returns `true` when hold active.

**Recovery:** Only current `admin` can clear via `clear_legal_hold()` or `set_legal_hold(false)`.

---

### Code 103: `EscrowNotOpenForFunding`

**Entrypoints:** `fund`, `fund_with_commitment`, `fund_batch`

**Trigger:** `escrow.status != 0` (not open).

**Why Rejected:** Funding only accepted while escrow is in **open** status. Once funded (status 1), escrow moves to settlement phase.

**Client Fix:** Check `get_escrow().status == 0` before funding.

**Status Lifecycle:**
```
0 (open) ──fund──▶ 1 (funded) ──settle──▶ 2 (settled)
                                  └─withdraw─▶ 3 (withdrawn)
0 (open) ──cancel_funding──▶ 4 (cancelled)
```

---

### Code 104: `InvestorNotAllowlisted`

**Entrypoints:** `fund`, `fund_with_commitment`, `fund_batch` (per-entry)

**Trigger:** Allowlist gate active (`is_allowlist_active() == true`) and investor not on list.

**Why Rejected:** Admin enabled KYC/compliance gating — only pre-approved addresses may fund.

**Client Fix:** Ask admin to call `set_investor_allowlisted(investor, true)`.

**Query:**
```rust
if client.is_allowlist_active() {
    if !client.is_investor_allowlisted(&investor) {
        return Err("Investor not allowlisted");
    }
}
```

**Recovery:** Admin adds investor via `set_investor_allowlisted` or disables gate via `set_allowlist_active(false)`.

---

### Code 105: `InvestorContributionOverflow`

**Entrypoints:** `fund`, `fund_with_commitment`, `fund_batch` (per-entry)

**Trigger:** `prev + amount` overflows `i128`.

**Why Rejected:** Arithmetic overflow guard — sum exceeds `i128::MAX`.

**Client Fix:** Reduce deposit size. Real-world amounts should never approach `2^127 - 1`.

**Boundary:** `i128::MAX = 9_223_372_036_854_775_807` base units

---

### Code 106: `InvestorContributionExceedsCap`

**Entrypoints:** `fund`, `fund_with_commitment`, `fund_batch` (per-entry)

**Trigger:** `new_contribution > max_per_investor_cap` when cap is configured.

**Why Rejected:** Admin set a ceiling on how much one address can commit (risk diversification).

**Client Fix:** Reduce deposit to stay within cap or ask admin to raise cap.

**Query:** `get_max_per_investor_cap()` returns `Some(cap)` or `None` (unlimited).

**Example:**
```rust
if let Some(cap) = client.get_max_per_investor_cap() {
    let current = client.get_investor_contribution(&investor);
    let available = cap.saturating_sub(current);
    ensure!(amount <= available, "Would exceed per-investor cap");
}
```

---

### Code 107: `UniqueInvestorCapReached`

**Entrypoints:** `fund`, `fund_with_commitment`, `fund_batch` (for new investors only)

**Trigger:** New investor (`prev == 0`) attempts to fund when `unique_funder_count >= max_unique_investors_cap`.

**Why Rejected:** Admin capped distinct addresses to bound storage footprint.

**Client Fix:** Wait for admin to raise cap via `raise_max_unique_investors` or use existing investor address.

**Query:** `get_remaining_investor_slots()` returns available slots.

**Note:** Existing investors (`prev > 0`) can add more principal without consuming a new slot.

---

### Code 108: `TieredSecondDeposit`

**Entrypoints:** `fund_with_commitment` only

**Trigger:** `fund_with_commitment` called when investor already has `prev > 0`.

**Why Rejected:** Tier selection and claim lock are **immutable** after first deposit. Calling `fund_with_commitment` again would allow re-selecting yield tier, violating fairness.

**Client Fix:** Use `fund()` (not `fund_with_commitment`) for all follow-on deposits.

**Pattern:**
```rust
// ✅ First deposit: choose tier
client.fund_with_commitment(&investor, &50_000, &100);

// ✅ Additional deposits: use fund()
client.fund(&investor, &30_000);

// ❌ Will fail with code 108
client.fund_with_commitment(&investor, &10_000, &200);
```

---

### Code 109: `InvestorClaimTimeOverflow`

**Entrypoints:** `fund_with_commitment`

**Trigger:** `now + committed_lock_secs` overflows `u64`.

**Why Rejected:** Claim lock timestamp must fit in `u64` ledger timestamp.

**Client Fix:** Reduce `committed_lock_secs`. Real locks should be << `u64::MAX` seconds.

**Boundary:** `u64::MAX = 18_446_744_073_709_551_615` seconds (~585 billion years)

---

### Code 110: `FundedAmountOverflow`

**Entrypoints:** `fund`, `fund_with_commitment`, `fund_batch`

**Trigger:** `escrow.funded_amount + amount` overflows `i128`.

**Why Rejected:** Cumulative funded principal across all investors exceeds `i128::MAX`.

**Client Fix:** Reject deposit. Real escrows should never approach this boundary.

**Boundary:** See contract docs for overflow-free coupon math constraints.

---

### Code 111: `CommitmentLockExceedsMaturity`

**Entrypoints:** `fund_with_commitment`

**Trigger:** `now + committed_lock_secs > escrow.maturity` when both `committed_lock_secs > 0` and `maturity > 0`.

**Why Rejected:** Investor's claim lock would expire **after** escrow maturity. Settlement could occur but investor cannot claim payout until lock expires, violating maturity contract.

**Client Fix:** Reduce `committed_lock_secs` so `claim_not_before <= maturity`, or use `committed_lock_secs = 0`.

**Example:**
```rust
let escrow = client.get_escrow();
let now = env.ledger().timestamp();
if escrow.maturity > 0 {
    let max_lock = escrow.maturity.saturating_sub(now);
    ensure!(committed_lock_secs <= max_lock, "Lock exceeds maturity");
}
```

---

### Code 164: `FundingDeadlinePassed`

**Entrypoints:** `fund`, `fund_with_commitment`, `fund_batch`

**Trigger:** Funding deadline configured and `env.ledger().timestamp() > deadline`.

**Why Rejected:** Admin set time window for funding; deposits after deadline rejected.

**Client Fix:** Wait for admin to extend deadline via `extend_funding_deadline`, or fund before deadline.

**Query:**
```rust
if let Some(deadline) = client.get_funding_deadline() {
    if env.ledger().timestamp() > deadline {
        return Err("Funding deadline passed");
    }
}
```

---

### Code 210: `PausedBlocksFunding`

**Entrypoints:** `fund`, `fund_with_commitment`, `fund_batch`, `unfund`

**Trigger:** Operational pause active (`is_paused() == true`).

**Why Rejected:** Admin activated incident-response circuit breaker to halt fund movement temporarily.

**Client Fix:** Wait for admin to call `set_paused(false)`. Orthogonal to legal hold.

**Query:** `is_paused()` returns `true` when pause active.

**Difference from Legal Hold:**
- **Pause:** Lightweight operational control, no delay to clear, single-call toggle
- **Legal Hold:** Compliance gate, may have mandatory delay, stronger semantics

---

### Code 220: `UnfundEscrowNotOpen`

**Entrypoints:** `unfund` only

**Trigger:** `unfund` called when `escrow.status != 0`.

**Why Rejected:** Unfunding only valid while escrow accepts contributions (open). Once funded, use `refund` if escrow is cancelled.

**Client Fix:** Check `get_escrow().status == 0` before calling `unfund`.

**Alternative:** If escrow cancelled (status 4), use `refund` instead.

---

### Code 221: `OverWithdrawal`

**Entrypoints:** `unfund` only

**Trigger:** `amount > investor_contribution`.

**Why Rejected:** Cannot withdraw more than deposited.

**Client Fix:** Query `get_investor_contribution(investor)` and ensure `amount <= contribution`.

**Example:**
```rust
let contribution = client.get_investor_contribution(&investor);
ensure!(amount <= contribution, "Cannot unfund more than contributed");
```

---

### Code 222: `UnfundLegalHoldActive`

**Entrypoints:** `unfund` only

**Trigger:** Legal hold active during `unfund` call.

**Why Rejected:** Same as code 102 — legal hold blocks all fund movements including unfunding.

**Client Fix:** Wait for admin to clear hold.

---

### Code 82: `FundingBatchEmpty`

**Entrypoints:** `fund_batch` only

**Trigger:** `entries.len() == 0`.

**Why Rejected:** Batch with zero entries is meaningless.

**Client Fix:** Pass at least one `(investor, amount)` entry.

---

### Code 83: `FundingBatchTooLarge`

**Entrypoints:** `fund_batch` only

**Trigger:** `entries.len() > MAX_FUND_BATCH` (50).

**Why Rejected:** Batch size capped to bound per-call CPU/storage work.

**Client Fix:** Split into chunks of ≤ 50 entries each.

**Constant:** `MAX_FUND_BATCH = 50`

---

### Code 84: `FundingBatchDuplicateInvestor`

**Entrypoints:** `fund_batch` only

**Trigger:** Two or more entries have same `investor` address.

**Why Rejected:** Duplicate addresses indicate malformed input.

**Client Fix:** Deduplicate or combine amounts for same investor into one entry.

**Atomicity:** If any entry violates this, **entire batch** rejected before any state mutation.

---

## Common Error Patterns

### 1. Pre-flight Validation Pattern

```rust
// 1. Check escrow is open
let escrow = client.get_escrow();
ensure!(escrow.status == 0, "Escrow not open");

// 2. Check no holds
ensure!(!client.get_legal_hold(), "Legal hold active");
ensure!(!client.is_paused(), "Operational pause active");

// 3. Check deadline
if let Some(deadline) = client.get_funding_deadline() {
    ensure!(env.ledger().timestamp() <= deadline, "Deadline passed");
}

// 4. Check allowlist
if client.is_allowlist_active() {
    ensure!(client.is_investor_allowlisted(&investor), "Not allowlisted");
}

// 5. Check floor
let floor = client.get_min_contribution_floor();
ensure!(amount >= floor, "Below minimum");

// 6. Check per-investor cap
if let Some(cap) = client.get_max_per_investor_cap() {
    let current = client.get_investor_contribution(&investor);
    ensure!(current + amount <= cap, "Exceeds cap");
}

// 7. Check unique investor slots (new investors only)
if client.get_investor_contribution(&investor) == 0 {
    ensure!(client.get_remaining_investor_slots() > 0, "No slots");
}

// ✅ All checks passed
client.fund(&investor, &amount);
```

### 2. Batch Funding Pattern

```rust
// Validate and deduplicate
let mut entries = Vec::new(&env);
let mut seen = Set::new();

for (investor, amount) in raw_entries {
    ensure!(amount > 0, "Amount must be positive");
    ensure!(!seen.contains(&investor), "Duplicate investor");
    seen.insert(investor.clone());
    entries.push_back((investor, amount));
}

ensure!(entries.len() <= 50, "Batch too large");

client.fund_batch(&entries);
```

### 3. Tiered Funding Pattern

```rust
// First deposit: use fund_with_commitment
client.fund_with_commitment(&investor, &50_000, &100);

// Follow-on: use fund() not fund_with_commitment
client.fund(&investor, &30_000);
```

### 4. Unfund Pattern

```rust
let escrow = client.get_escrow();
ensure!(escrow.status == 0, "Escrow not open");
ensure!(!client.get_legal_hold(), "Legal hold active");
ensure!(!client.is_paused(), "Pause active");

let contribution = client.get_investor_contribution(&investor);
ensure!(amount <= contribution, "Amount exceeds contribution");

client.unfund(&investor, &amount);
```

---

## Client Integration Guide

### Error Handling Best Practices

**1. Branch on numeric codes:**
```rust
match result {
    Err(Err(InvokeError::Contract(100))) => "Amount must be positive",
    Err(Err(InvokeError::Contract(103))) => "Escrow closed for funding",
    Err(Err(InvokeError::Contract(107))) => "No investor slots available",
    // ... handle other codes
}
```

**2. Pre-flight checks reduce rejections:**
Query state before funding to catch issues client-side.

**3. Retry strategies:**
- **Temporary (102, 210):** Retry after hold/pause clears
- **Permanent (103, 108, 164):** Do not retry without user action
- **Client errors (100, 101, 106):** Fix input and retry

**4. User feedback messages:**
- 102/222: "Compliance hold active — contact support"
- 103: "Funding closed — escrow already funded"
- 104: "Address not allowlisted — complete KYC"
- 107: "Escrow full — no investor slots"
- 108: "Use fund() for additional deposits"
- 111: "Commitment lock too long for maturity"
- 164: "Funding deadline passed"
- 210: "Escrow paused — try again soon"

---

## Cross-References

### Entrypoint Implementations
- `fund` (escrow/src/lib.rs line ~4953)
- `fund_with_commitment` (escrow/src/lib.rs line ~4984)
- `fund_batch` (escrow/src/lib.rs line ~5015)
- `unfund` (escrow/src/lib.rs line ~6779)

### Related Documentation
- [Allowlist Model](escrow-allowlist.md)
- [Investor Caps](escrow-investor-caps.md)
- [Legal Hold](escrow-legal-hold.md)
- [Pause Auth](pause-auth.md)
- [Tiered Yield ADR](adr/ADR-005-tiered-yield.md)
- [All Error Messages](escrow-error-messages.md)

### Test Coverage
- Funding tests (escrow/src/tests/funding.rs)
- Cap validation (escrow/src/tests/cap_validation.rs)
- Legal hold (escrow/src/tests/legal_hold.rs)
- Pause tests (escrow/src/tests/pause.rs)

---

**Document maintained by:** LiquiFact Core Contributors  
**Community:** https://discord.gg/JrGPH4V3
