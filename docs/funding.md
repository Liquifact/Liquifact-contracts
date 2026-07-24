# Funding Model

This document describes the funding data model, its invariants, and every entrypoint that
reads or writes funding state. It is the canonical reference for auditors, integrators, and
off-chain indexers that need to reason about investor capital flows.

---

## Overview

The LiquiFact escrow contract custodies investor principal (in a single SEP-41 **funding
token**) until the invoice reaches a terminal disposition. The funding lifecycle spans
three layers:

1. **Deposit** — tokens move from investor addresses into the escrow contract.
2. **Accounting** — per-investor contributions, aggregate funded amount, unique-funder
   count, and the immutable funding-close snapshot are maintained.
3. **Disbursement** — after a terminal state is reached, principal (and optional yield
   coupon) is returned to investors, withdrawn by the SME, or swept as dust to the treasury.

Every token transfer uses `external_calls::transfer_funding_token_with_balance_checks` (or
its inbound counterpart), which asserts exact pre/post balance deltas — fee-on-transfer and
rebasing tokens are out of scope.

---

## Core data structures

### `InvoiceEscrow` (stored at `DataKey::Escrow`)

```rust
pub struct InvoiceEscrow {
    pub invoice_id: Symbol,
    pub admin: Address,
    pub sme_address: Address,
    pub amount: i128,             // original invoice face value (immutable after init)
    pub funding_target: i128,     // may be lowered by admin while status == 0
    pub funded_amount: i128,      // running total; checked_add on each fund call
    pub yield_bps: i64,           // base annualised yield, 0..=10_000
    pub maturity: u64,            // ledger timestamp; 0 = no maturity gate
    pub status: u32,              // 0=open 1=funded 2=settled 3=withdrawn 4=cancelled
}
```

Source: `escrow/src/lib.rs:892–903`.

### `FundingCloseSnapshot` (stored at `DataKey::FundingCloseSnapshot`)

```rust
pub struct FundingCloseSnapshot {
    pub total_principal: i128,               // funded_amount at the moment status became 1
    pub funding_target: i128,
    pub closed_at_ledger_timestamp: u64,
    pub closed_at_ledger_sequence: u32,
}
```

Source: `escrow/src/lib.rs:948–955`.

### `ReconciliationView` (returned by `get_reconciliation`)

```rust
pub struct ReconciliationView {
    pub token_balance: i128,
    pub outstanding_liability: i128,
    pub surplus: i128,
}
```

Source: `escrow/src/lib.rs:5560–5571`.

---

## Storage keys that participate in funding

| Key | Storage tier | Mutated by | Default |
|-----|-------------|------------|---------|
| `DataKey::FundingToken` | instance | `init` | immutable after init |
| `DataKey::Treasury` | instance | `init` | immutable after init |
| `DataKey::Escrow.funded_amount` | instance | `fund_impl`, `unfund`, `withdraw`, `cancel_funding` | `0` |
| `DataKey::Escrow.funding_target` | instance | `init`, `update_funding_target` | `amount` |
| `DataKey::Escrow.status` | instance | `fund_impl`, `settle`, `withdraw`, `cancel_funding`, `partial_settle` | `0` |
| `DataKey::UniqueFunderCount` | instance | `fund_impl` (increment), `unfund` (decrement on zero-out) | `0` |
| `DataKey::InvestorContribution(addr)` | persistent | `fund_impl`, `unfund`, `refund` | `0` |
| `DataKey::InvestorEffectiveYield(addr)` | persistent | `fund_impl` (first deposit only) | base `yield_bps` |
| `DataKey::InvestorClaimNotBefore(addr)` | persistent | `fund_impl` (first deposit, tiered) | `0` |
| `DataKey::InvestorClaimed(addr)` | persistent | `claim_investor_payout` | `false` |
| `DataKey::InvestorRefunded(addr)` | instance | `refund` | `false` |
| `DataKey::FundingCloseSnapshot` | instance | `fund_impl` (on 0→1 transition), `update_funding_target` (when lowering crosses threshold), `partial_settle` | absent |
| `DataKey::DistributedPrincipal` | instance | `refund`, `withdraw` | `0` |
| `DataKey::MinContributionFloor` | instance | `init`, `lower_min_contribution_floor` | `0` |
| `DataKey::MaxUniqueInvestorsCap` | instance | `init` | absent (unlimited) |
| `DataKey::MaxPerInvestorCap` | instance | `init`, `raise_max_per_investor` | absent (unlimited) |
| `DataKey::FundingDeadline` | instance | `init`, `extend_funding_deadline` | absent |
| `DataKey::ProtocolFeeBps` | instance | `init` | `0` |
| `DataKey::InvestorIndex` | instance | `fund_impl` (first deposit) | empty `Vec` |
| `DataKey::LegalHold` | instance | `set_legal_hold` | `false` |
| `DataKey::Paused` | instance | `set_paused` | `false` |

---

## Entrypoints that touch funding state

### Deposit entrypoints

| Entrypoint | Auth | Description | Source |
|------------|------|-------------|--------|
| `fund(investor, amount)` | investor | Simple deposit; sets effective yield on first deposit. | `lib.rs:4871` |
| `fund_with_commitment(investor, amount, committed_lock_secs)` | investor | First-deposit-only tiered yield with optional claim lock. | `lib.rs:4882` |
| `fund_batch(entries: Vec<(Address, i128)>)` | per-entry investor | Batch wrapper; same invariants as `fund`, plus duplicate-address rejection and upfront positivity/floor checks. | `lib.rs:4913` |
| `unfund(investor, amount)` | investor | Partial/full withdrawal while open (status 0). Decrements `funded_amount` and contribution; decrements `UniqueFunderCount` on zero-out. | `lib.rs:5395` |

All four delegate to `fund_impl` (for deposit) or perform their own guards. The shared
implementation lives at `lib.rs:3981`.

### Disbursement entrypoints

| Entrypoint | Auth | Description | Source |
|------------|------|-------------|--------|
| `withdraw()` | SME | SME pulls `funded_amount` (minus protocol fee). Status 1→3. | `lib.rs:4348` |
| `settle()` | SME | Finalizes escrow. Status 1→2. No token transfer. | `lib.rs:4256` |
| `partial_settle(caller)` | SME or admin | Closes funding early for under-funded invoice. Status 0→1; writes `FundingCloseSnapshot`. | `lib.rs:4213` |
| `claim_investor_payout(investor)` | investor | Pro-rata payout after settlement. Computes gross payout, marks claimed, transfers. | `lib.rs:4478` |
| `refund(investor)` | investor | Returns principal after cancellation (status 4). Zeros contribution, increments `DistributedPrincipal`. | `lib.rs:5260` |
| `refund_batch(investors)` | per-investor | Batch wrapper; skips already-refunded entries. | `lib.rs:5344` |
| `sweep_terminal_dust(amount)` | treasury | Moves up to `MAX_DUST_SWEEP_AMOUNT` rounding residue to treasury in terminal states. Enforces liability floor in cancelled state. | `lib.rs:2121` |

### Configuration entrypoints that affect funding

| Entrypoint | Auth | Description | Source |
|------------|------|-------------|--------|
| `init(…)` | admin | Creates escrow; binds funding token, treasury, optional yield tiers, min contribution floor, max caps, protocol fee, funding deadline. | `lib.rs:1787` |
| `update_funding_target(new_target)` | admin | Lowers/raises target while open. Can trigger 0→1 transition when lowering crosses `funded_amount`. | `lib.rs:3457` |
| `lower_min_contribution_floor(new_floor)` | admin | Strictly lowers the floor while open. | `lib.rs:3606` |
| `lower_max_unique_investors(new_cap)` | admin | Strictly lowers the unique-investor cap while open. | `lib.rs:3514` |
| `raise_max_unique_investors(new_cap)` | admin | Strictly raises the unique-investor cap while open. | `lib.rs:3562` |
| `raise_max_per_investor(new_cap)` | admin | Strictly raises the per-investor cap while open. | `lib.rs:3655` |
| `extend_funding_deadline(new_deadline)` | admin | Pushes the funding deadline forward while open. | `lib.rs:4799` |
| `cancel_funding()` | admin | Transitions status 0→4; enables investor refunds. Blocked by legal hold. | `lib.rs:5228` |

### Read-only views

| View | Description | Source |
|------|-------------|--------|
| `get_escrow()` | Full `InvoiceEscrow` snapshot. | `lib.rs:1957` |
| `get_contribution(investor)` | Per-investor principal. | `lib.rs:2559` |
| `get_contributions(investors)` | Batch per-investor principal. | `lib.rs:2571` |
| `get_unique_funder_count()` | Number of distinct funders. | `lib.rs:2370` |
| `get_remaining_funding_capacity()` | `funding_target - funded_amount` (floored at 0). | `lib.rs:1967` |
| `get_remaining_investor_slots()` | `cap - unique_funder_count` (if capped). | `lib.rs:4867` |
| `get_funding_close_snapshot()` | Immutable snapshot captured at funding close. | `lib.rs:2624` |
| `get_settlement_pool()` | Total pool (`total_principal + coupon`) from base yield. | `lib.rs:4709` |
| `compute_investor_payout(investor)` | Gross pro-rata payout for an investor. | `lib.rs:4621` |
| `get_claimable_payout(investor)` | Net claimable payout (0 if any gate blocks). | `lib.rs:4558` |
| `get_reconciliation()` | Live balance, outstanding liability, surplus. | `lib.rs:5525` |
| `get_distributed_principal()` | Cumulative principal returned via `refund`/`withdraw`. | `lib.rs:5491` |
| `get_token_balance()` | Live SEP-41 balance held by the contract. | `lib.rs:1648` |
| `get_protocol_fee_bps()` | Immutable protocol fee (0..=10_000). | `lib.rs:2343` |
| `get_min_contribution_floor()` | Current floor. | `lib.rs:2331` |
| `get_max_unique_investors_cap()` | Current cap (None = unlimited). | `lib.rs:2354` |
| `get_max_per_investor_cap()` | Per-investor cap (None = unlimited). | `lib.rs:2362` |
| `get_funding_deadline()` | Optional deadline timestamp. | `lib.rs:2285` |
| `is_funding_expired()` | Whether `now > deadline`. | `lib.rs:2290` |
| `is_settleable()` | Whether `settle` would succeed now. | `lib.rs:2955` |
| `get_settlement_readiness()` | Bundled settlement-readiness check. | `lib.rs:2976` |
| `get_investors(start, limit)` | Paginated investor address list. | `lib.rs:2597` |
| `preview_yield_tier(amount, lock)` | Simulated tier resolution. | `lib.rs:2687` |
| `get_investor_yield_bps(investor)` | Investor's effective yield. | `lib.rs:2646` |
| `get_investor_claim_not_before(investor)` | Investor's claim lock timestamp. | `lib.rs:2654` |

---

## Invariants

### INV-1: Conservation at withdrawal

```
sme_payout + fee == funded_amount
```

At `withdraw()`, the funded amount is split: `fee = funded_amount * protocol_fee_bps / 10_000`
(floor, checked) to treasury, `net = funded_amount - fee` (checked) to SME. Floor rounding
means sub-10,000 residue stays with the SME — no principal is created or destroyed.

Source: `lib.rs:4364–4379`.

### INV-2: Pro-rata aggregate bound (uniform yield)

```
sum(payout_i) <= total_principal + floor(total_principal * yield_bps / 10_000)
```

Each individual payout uses floor integer division, so the aggregate can never exceed the
settle pool. The residue (`settle_pool - sum(payout_i)`) is always `< n_investors` and
is swept via `sweep_terminal_dust`.

Source: `escrow-pro-rata.md:91–104`.

### INV-3: Pro-rata aggregate bound (tiered/mixed yield)

Per-investor effective yields can differ (from `fund_with_commitment` tier selection), but
the same floor-division rounding guarantee holds per investor. The aggregate bound is:

```
sum(payout_i) <= total_principal * (1 + max_effective_yield_bps / 10_000)
```

Source: `escrow-pro-rata.md:108–121`.

### INV-4: Snapshot immutability

`FundingCloseSnapshot` is written **exactly once** per escrow — at the first transition to
`status == 1`. Once written, neither `total_principal` nor `funding_target` is ever
overwritten. All `compute_investor_payout` calls read the same denominator.

Source: `lib.rs:4131–4143`.

### INV-5: Liability floor (cancelled escrow sweep)

In cancelled (status 4) escrows, `sweep_terminal_dust` enforces:

```
balance - sweep_amt >= funded_amount - distributed_principal
```

`distributed_principal` is incremented atomically by `refund()` and `withdraw()`, making
the invariant computable on-chain without iterating over investor addresses.

Source: `lib.rs:2160–2174`.

### INV-6: Contribution monotonicity

`DataKey::InvestorContribution` is only updated by:
- `fund_impl`: `checked_add` (never decreases)
- `refund`: set to `0`
- `unfund`: `checked_sub` (never increases)

Contribution is always non-negative. A second `refund` finds contribution `0` and fails.

Source: `lib.rs:4037–4039`, `lib.rs:5284`, `lib.rs:5416–5418`.

### INV-7: UniqueFunderCount consistency

- Incremented exactly once per new investor address (when `prev == 0` in `fund_impl`).
- Decremented (floor 0 via `saturating_sub`) when `unfund` reduces contribution to zero.
- Never exceeds the number of addresses with non-zero `InvestorContribution`.

Source: `lib.rs:4065–4077`, `lib.rs:4148–4161`, `lib.rs:5437–5445`.

### INV-8: Status monotonicity

Status transitions are strictly forward: `0 → 1 → 2`, `0 → 1 → 3`, or `0 → 4`. No
regression from terminal states (2, 3, 4) is possible. `unfund` stays at status 0.

Source: `lib.rs:4131`, `lib.rs:4294`, `lib.rs:4400`, `lib.rs:5235`.

### INV-9: Double-claim prevention

`DataKey::InvestorClaimed(addr)` is written `true` **before** the token transfer in
`claim_investor_payout`. A second call returns early (no re-transfer). If the transfer
fails, the Soroban host rolls back all storage writes including the marker.

Source: `lib.rs:4508–4510`, `lib.rs:4517`.

### INV-10: Double-refund prevention

`DataKey::InvestorContribution` is zeroed and `DataKey::InvestorRefunded` is set to
`true` before the token transfer. A second `refund` finds contribution `0` and fails
with `NoContributionToRefund`.

Source: `lib.rs:5284–5288`.

### INV-11: Funding amount overflow safety

`funded_amount` uses `checked_add` on every `fund_impl` call and `checked_sub` on every
`unfund` call. `MAX_INVOICE_AMOUNT = 2^63 - 1` bounds the invoice face value so that
`compute_investor_payout` arithmetic (three-step multiplication chain) stays within
`i128` range for all valid `yield_bps ∈ [0, 10_000]`.

Source: `lib.rs:228`, `lib.rs:4126–4129`.

### INV-12: Commitment lock bounded by maturity

When `fund_with_commitment` is used with `committed_lock_secs > 0` and the escrow has a
non-zero maturity, the contract rejects the deposit if `now + committed_lock_secs >
maturity`. This prevents a settled escrow from holding an investor's payout claim hostage
beyond the point where principal is due.

Source: `lib.rs:4115–4121`.

### INV-13: Fee-on-transfer token exclusion

`external_calls::transfer_funding_token_with_balance_checks` reads pre/post balances and
asserts exact deltas. Fee-on-transfer or rebasing tokens cause a typed panic at the
balance-check boundary — they are explicitly out of scope.

Source: `external_calls.rs`.

### INV-14: Protocol fee conservation at withdraw

With `protocol_fee_bps = 0`, the full `funded_amount` goes to the SME and no treasury
transfer occurs (byte-for-byte identical to the pre-fee contract). With
`protocol_fee_bps = 10_000`, the full `funded_amount` goes to treasury and the SME
receives `0`. For all intermediate values, `sme_payout + fee == funded_amount` by
construction.

Source: `lib.rs:4364–4379`.

### INV-15: Batch atomicity

`fund_batch` validates positivity and min-contribution for every entry **before** any
`fund_impl` call performs a state mutation. Duplicate-address detection (`O(n^2)` scan,
bounded by `MAX_FUND_BATCH = 50`) rejects the entire batch atomically. Remaining per-entry
invariants (caps, overflow) are enforced inside `fund_impl` against running accumulated
state.

Source: `lib.rs:3931–3966`.

### INV-16: Over-funding snapshot captures threshold-crossing total

When a batch deposit crosses the funding target, `FundingCloseSnapshot.total_principal`
records `funded_amount` at the exact crossing entry — not the final batch total. Remaining
entries are processed post-transition. The snapshot is never overwritten.

Source: `lib.rs:4131–4143`.

### INV-17: Funding deadline enforcement

`fund_impl` checks `DataKey::FundingDeadline` and rejects deposits when
`ledger.timestamp() > deadline`. `extend_funding_deadline` requires the new deadline to be
strictly greater than the current deadline and strictly less than maturity (when configured).

Source: `lib.rs:4020–4026`, `lib.rs:4815–4831`.

### INV-18: Deferred funding target → funded transition

When the admin lowers `funding_target` via `update_funding_target` and the new target is
≤ `funded_amount`, the escrow transitions to status 1 and `FundingCloseSnapshot` is
written — mirroring the promotion logic in `fund_impl`.

Source: `lib.rs:3474–3488`.

---

## Worked example

### Scenario

An escrow is initialized with:
- `amount` = 100,000 USDC, `funding_target` = 100,000 USDC
- `yield_bps` = 500 (5% annualised)
- `protocol_fee_bps` = 200 (2%)
- Tier table: `[(min_lock_secs=0, yield=500), (min_lock_secs=86400, yield=700)]`
- Maturity at ledger timestamp 1,000,000

### Step 1: Deposit phase

| Investor | Amount | Entry | Effective yield | Claim lock |
|----------|--------|-------|-----------------|------------|
| Alice | 40,000 | `fund` | 500 bps | 0 (no lock) |
| Bob | 30,000 | `fund_with_commitment(30_000, 90000)` | 700 bps (tier matched) | now + 86400 |
| Carol | 35,000 | `fund` | 500 bps | 0 |

After Carol's deposit, `funded_amount = 105,000 >= 100,000`. Status transitions 0→1.
`FundingCloseSnapshot` is written:
```
total_principal = 105,000
funding_target  = 100,000
```

Storage state:
```
UniqueFunderCount = 3
InvestorContribution[Alice] = 40,000
InvestorContribution[Bob]   = 30,000
InvestorContribution[Carol]  = 35,000
InvestorEffectiveYield[Bob]  = 700  (others absent → base 500)
```

### Step 2: Settlement

SME calls `settle()` after maturity. `EscrowSettled` is emitted with:
```
settle_pool = 105,000 + floor(105,000 * 500 / 10,000)
            = 105,000 + 5,250
            = 110,250
```

Status transitions 1→2.

### Step 3: Investor claims

Alice calls `claim_investor_payout(Alice)`:
```
coupon       = 105,000 * 500 / 10,000 = 5,250
settle_pool  = 105,000 + 5,250 = 110,250
gross_payout = 40,000 * 110,250 / 105,000 = 42,000
```

Bob calls `claim_investor_payout(Bob)`:
```
coupon       = 105,000 * 700 / 10,000 = 7,350
settle_pool  = 105,000 + 7,350 = 112,350
gross_payout = 30,000 * 112,350 / 105,000 = 32,100
```

Carol calls `claim_investor_payout(Carol)`:
```
coupon       = 105,000 * 500 / 10,000 = 5,250
settle_pool  = 105,000 + 5,250 = 110,250
gross_payout = 35,000 * 110,250 / 105,000 = 36,750
```

Aggregate: `42,000 + 32,100 + 36,750 = 110,850`.

Note: Bob receives a higher yield per dollar than Alice/Carol because he committed to
the 700 bps tier. The aggregate payout (110,850) is ≤ the maximum possible pool
(105,000 * (1 + 700/10,000) = 112,350). Any rounding residue is swept by treasury.

### Step 4 (alternative): SME withdrawal path

Instead of settle + claims, the SME could call `withdraw()`:
```
fee    = 105,000 * 200 / 10,000 = 2,100
net    = 105,000 - 2,100 = 102,900
```

- Treasury receives 2,100 USDC
- SME receives 102,900 USDC
- Status transitions 1→3
- `DistributedPrincipal += 105,000`

### Step 5 (alternative): Cancellation path

If the admin calls `cancel_funding()` before the target is reached:
- Status transitions 0→4
- Each investor calls `refund(investor)` to recover their contribution
- `DistributedPrincipal` is incremented by each refund amount
- `sweep_terminal_dust` enforces the liability floor: balance must remain ≥ outstanding

---

## Security considerations

- **Custody verification**: Every deposit and withdrawal uses balance-delta checks.
  Non-standard tokens (fee-on-transfer, rebasing) are out of scope.
- **Snapshot stability**: The pro-rata denominator is fixed at funding close. Off-chain
  tools must call `compute_investor_payout` rather than re-implementing the formula.
- **Overflow safety**: All funding arithmetic uses `checked_*` operations. `MAX_INVOICE_AMOUNT`
  bounds the input space so `compute_investor_payout` stays within `i128` range.
- **Legal hold**: Blocks `fund`, `settle`, `withdraw`, `claim_investor_payout`,
  `cancel_funding`, `sweep_terminal_dust`, and `unfund` when active.
- **Operational pause**: Independent circuit breaker that blocks `fund`, `settle`,
  `withdraw`, and `claim_investor_payout`.
- **Auth ordering**: Every mutating entrypoint follows read-only preconditions →
  `require_auth()` → storage writes → token transfers (see ADR-002).

---

## Cross-references

| Document | Relation |
|----------|----------|
| [escrow-data-model.md](escrow-data-model.md) | `DataKey` enum, stored structs, additive-key policy |
| [escrow-pro-rata.md](escrow-pro-rata.md) | Payout math, rounding policy, aggregate invariants |
| [escrow-lifecycle.md](escrow-lifecycle.md) | State machine, valid transitions, mutual exclusivity |
| [escrow-cancellation-refunds.md](escrow-cancellation-refunds.md) | Cancellation lifecycle, refund mechanics |
| [escrow-investor-caps.md](escrow-investor-caps.md) | Unique-investor cap, per-investor cap |
| [escrow-numeric-model.md](escrow-numeric-model.md) | Protocol fee split math |
| [escrow-ledger-time.md](escrow-ledger-time.md) | Ledger time trust model, claim locks |
| [escrow-token-safety.md](escrow-token-safety.md) | Token safety, balance-delta verification |
| [escrow-gas-storage-notes.md](escrow-gas-storage-notes.md) | Storage tiers, TTL, gas considerations |
| [ADR-001](adr/ADR-001-state-model.md) | State model design decision |
| [ADR-002](adr/ADR-002-auth-boundaries.md) | Authorization guard ordering |
| [ADR-005](adr/ADR-005-tiered-yield.md) | Tiered yield design |
| [yield-tier.md](yield-tier.md) | Yield-tier model, invariants, and entrypoint behavior |
| [ADR-006](adr/ADR-006-dust-sweep-and-token-safety.md) | Dust sweep and token safety |
| [ADR-007](adr/ADR-007-storage-key-evolution.md) | Storage key evolution policy |
