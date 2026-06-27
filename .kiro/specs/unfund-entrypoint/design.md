# Design: `unfund(investor, amount)` Entrypoint

## Overview

Add a new `unfund(investor: Address, amount: i128) -> InvoiceEscrow` entrypoint to the
LiquiFact escrow contract. It lets an investor partially or fully withdraw their
contribution while the escrow is still in the **open** state (`status == 0`), without
requiring the escrow to be cancelled first.

This is an **additive-only** change. No existing entrypoints, error codes, or storage keys
are modified.

---

## Motivation

`refund` only works in the cancelled state (status 4). An investor who changes their mind
before the escrow closes (or who contributed too much) has no on-chain recourse while status
is 0. `unfund` fills that gap without touching the cancel/refund path.

---

## Guard Ordering (follows ADR-002 canonical sequence)

All guards are read-only preconditions evaluated **before** any auth call or storage mutation.

| Step | Check | Error on failure |
|------|-------|-----------------|
| 1 | `escrow.status == 0` | `EscrowNotOpen` |
| 2 | `!legal_hold_active(&env)` | `LegalHoldActive` |
| 3 | `investor.require_auth()` | SDK auth rejection |
| 4 | `amount > 0` (use existing pattern) | Panic (same as `fund`) |
| 5 | `contribution >= amount` (checked_sub probe) | `OverWithdrawal` |

**Note on ordering:** status and legal-hold are both read-only pre-checks done before auth,
matching the existing pattern in `fund_impl`, `settle`, `withdraw`, and `cancel_funding`.

---

## State Mutations (all guarded by checked arithmetic)

```
contribution_after = contribution
    .checked_sub(amount)
    .unwrap_or_else(|| fail(&env, EscrowError::OverWithdrawal))

escrow.funded_amount = escrow.funded_amount
    .checked_sub(amount)
    .unwrap_or_else(|| fail(&env, EscrowError::OverWithdrawal))
```

After decrement:
- If `contribution_after == 0`:
  - Remove `DataKey::InvestorContribution(investor)` from persistent storage
    (via `set` to `0` — consistent with existing zero-out pattern in `refund`)
  - Decrement `UniqueFunderCount` using `saturating_sub(1)` — never goes below zero
- Else:
  - Persist `contribution_after` to `DataKey::InvestorContribution(investor)` (persistent)

Persist updated `escrow` to `DataKey::Escrow` (instance storage).

---

## Token Custody

The contract follows the same on-chain vs off-chain custody split used by `refund` and `withdraw`.

**On-chain custody** (`DataKey::FundingToken` is set and tokens are actually custodied in the
contract address):

```rust
let token_addr = Self::funding_token_or_fail(&env);
let this = env.current_contract_address();
external_calls::transfer_funding_token_with_balance_checks(
    &env, &token_addr, &this, &investor, amount,
);
```

**Off-chain accounting** (custody is off-chain, contract tracks accounting only):

```rust
// NOTE: On-chain custody is disabled for this escrow instance.
// The investor's contribution balance has been decremented in persistent
// storage and funded_amount has been updated. Token settlement is handled
// off-chain by the escrow operator. No on-chain transfer is performed here.
```

The existing `fund` entrypoint does **not** pull tokens on-chain (it is accounting-only).
The `refund` entrypoint **does** push tokens back (it always calls the transfer wrapper).
For `unfund`, the contract should mirror `refund`: always call the transfer wrapper so that
on-chain custody escrows return tokens immediately.

However, many existing tests run without a real SAC token. The implementation must therefore:
1. Always call `transfer_funding_token_with_balance_checks` (matching `refund` pattern).
2. Tests that need token transfer will use `init_and_fund_with_real_token` or mint tokens
   manually; tests that only verify accounting logic will use mocked-auth env where the SAC
   is a free address (transfer will fail), so those tests call the token path through
   `mock_all_auths` which handles it.

**Conclusion:** implement `unfund` to always call the transfer wrapper (same as `refund`),
and annotate the logic with the `// NOTE` comment for the off-chain dependency.

---

## New Event: `EscrowUnfunded`

```rust
#[contractevent]
pub struct EscrowUnfunded {
    #[topic]
    pub name: Symbol,                   // symbol_short!("unfunded")
    #[topic]
    pub invoice_id: Symbol,
    #[topic]
    pub investor: Address,
    pub amount: i128,                   // amount withdrawn this call
    pub remaining_contribution: i128,   // investor's balance after withdrawal
    pub new_funded_amount: i128,        // escrow.funded_amount after withdrawal
    pub timestamp: u64,                 // env.ledger().timestamp()
}
```

---

## New Error Variants (append-only — codes beyond existing max 164)

The current highest used error code is `164` (duplicated for `NoPendingAdmin = 163` and
`InsufficientContractBalance = 164`). New codes start at **165**.

| Variant | Code | When it fires |
|---------|------|---------------|
| `EscrowNotOpen` | 165 | `unfund` called when `status != 0`; unfunding is only valid in the open state |
| `OverWithdrawal` | 166 | Requested `amount` exceeds the investor's recorded contribution |
| `LegalHoldActive` | 167 | A compliance/legal hold is active; no fund movement is permitted |

### NatSpec doc comments (to be placed in the enum definition)

```rust
/// [`LiquifactEscrow::unfund`] called when [`InvoiceEscrow::status`] is not 0 (open).
/// Unfunding is only valid while the escrow is still accepting contributions.
EscrowNotOpen = 165,

/// [`LiquifactEscrow::unfund`] requested amount exceeds the investor's recorded contribution.
/// Never withdraw more than was contributed; checked via [`i128::checked_sub`].
OverWithdrawal = 166,

/// [`LiquifactEscrow::unfund`] blocked because a compliance/legal hold is active.
/// No fund movement is permitted until the hold is cleared by the admin.
LegalHoldActive = 167,
```

---

## Function Signature and Doc Comment

```rust
/// Withdraw `amount` of principal from this investor's contribution while the escrow
/// is still **open** (status = 0).
///
/// # Purpose
/// Lets investors reduce or fully exit their position before the escrow closes,
/// without requiring admin cancellation.
///
/// # Parameters
/// - `investor`: The investor address; must authorize this call.
/// - `amount`:   The amount to withdraw; must be positive and ≤ the investor's contribution.
///
/// # Guards (evaluated in order)
/// 1. `status == 0` — unfunding is forbidden in funded, settled, withdrawn, or cancelled states.
/// 2. No active legal hold — fund movement is blocked while a hold is in place.
/// 3. `investor.require_auth()` — only the investor may withdraw their own contribution.
/// 4. `amount > 0` — zero-amount withdrawals are rejected.
/// 5. `amount <= contribution` — over-withdrawal is rejected via `checked_sub`.
///
/// # State mutations
/// - Decrements `DataKey::InvestorContribution(investor)` by `amount` (persistent storage).
/// - If contribution reaches zero: clears the entry and decrements `UniqueFunderCount`
///   (floor: 0, never negative).
/// - Decrements `escrow.funded_amount` by `amount` (checked arithmetic).
///
/// # Token custody
/// Returns `amount` tokens to `investor` via the SEP-41 transfer wrapper when on-chain
/// custody is enabled. When custody is off-chain, accounting is updated only.
///
/// # Events
/// Emits [`EscrowUnfunded`] with `investor`, `amount`, `remaining_contribution`,
/// `new_funded_amount`, and `timestamp`.
///
/// # Errors
/// - [`EscrowError::EscrowNotOpen`] — status is not 0.
/// - [`EscrowError::LegalHoldActive`] — hold is active.
/// - [`EscrowError::OverWithdrawal`] — amount > contribution.
pub fn unfund(env: Env, investor: Address, amount: i128) -> InvoiceEscrow { ... }
```

---

## Interaction with Existing State

| Key | Effect |
|-----|--------|
| `DataKey::InvestorContribution(investor)` | Decremented by `amount`; removed (set to 0) if zero |
| `DataKey::UniqueFunderCount` | Decremented by 1 when contribution reaches 0; floor 0 |
| `DataKey::Escrow` (funded_amount) | Decremented by `amount` |
| `DataKey::FundingCloseSnapshot` | **Not touched** — snapshot only written at 0→1 transition |
| `DataKey::InvestorEffectiveYield` | **Not touched** — yield tier is immutable per investor |
| `DataKey::InvestorClaimNotBefore` | **Not touched** — claim lock is immutable once set |
| `DataKey::DistributedPrincipal` | **Not touched** — only used by `refund` and `withdraw` |
| `DataKey::InvestorRefunded` | **Not touched** — only used by `refund` |

**No status transition:** `unfund` never changes `escrow.status`. Partial unfund keeps status
at 0; full unfund (all investors exit) also stays at 0. Only `fund`, `cancel_funding`, `settle`,
and `withdraw` change status.

---

## Lifecycle Doc Update

`docs/escrow-lifecycle.md` additions:

1. **State diagram:** Add a self-loop on `open (0)` labeled `unfund(investor, amount) [investor]`.
2. **Valid transitions table:** Add a row:
   - From: `0` (open), To: `0` (open), Trigger: `unfund()`, Auth: investor, Notes: partial
     unfund keeps status; full unfund (contribution → 0) also stays open
3. **Legal hold table:** Add `unfund()` → Yes (blocked by legal hold)
4. **New section: Investor unfund path (status 0 — open):** mirrors the existing "Investor
   refund path" section with the unfund-specific invariants.
5. **On-chain vs off-chain custody note.**

---

## Test Coverage Plan

All tests go in `escrow/src/tests/funding.rs`.

| Test name | Verifies |
|-----------|---------|
| `test_unfund_partial` | Partial unfund: contribution and funded_amount decremented; UniqueFunderCount unchanged; EscrowUnfunded event |
| `test_unfund_full` | Full unfund: contribution removed (0), UniqueFunderCount decremented, funded_amount correct |
| `test_unfund_funder_count_floor` | UniqueFunderCount never goes below 0 even with adversarial storage state |
| `test_unfund_over_withdrawal` | amount > contribution → EscrowNotOpen (OverWithdrawal) error |
| `test_unfund_wrong_status_funded` | status 1 → EscrowNotOpen |
| `test_unfund_wrong_status_settled` | status 2 → EscrowNotOpen |
| `test_unfund_wrong_status_withdrawn` | status 3 → EscrowNotOpen |
| `test_unfund_wrong_status_cancelled` | status 4 → EscrowNotOpen |
| `test_unfund_legal_hold_blocked` | hold active → LegalHoldActive |
| `test_unfund_requires_investor_auth` | auth recorded for investor |
| `test_unfund_no_underflow` | checked_sub path exercised; never panics on exact boundary |
| `test_unfund_multiple_investors` | Two investors; one unfunds; other's contribution unchanged |
| `test_unfund_then_refund_after_cancel` | After partial unfund, cancel and refund remaining works |

---

## Security Notes

| Concern | Mitigation |
|---------|------------|
| Underflow on contribution | `checked_sub` → `OverWithdrawal` typed error; no `.unwrap()` |
| Underflow on funded_amount | `checked_sub` → `OverWithdrawal` typed error |
| UniqueFunderCount underflow | `saturating_sub(1)` — floor 0 |
| Unauthorized call | `investor.require_auth()` before any mutation |
| Hold guard bypass | Legal-hold check before auth (read-only; consistent with other entrypoints) |
| Status guard bypass | Status check before auth |
| Double withdrawal | Each call re-reads persistent contribution; zeroing removes re-entry surface |
| Token custody | `transfer_funding_token_with_balance_checks` enforces SEP-41 delta invariants |
| Non-standard tokens | Out of scope (same as all other entrypoints) |
