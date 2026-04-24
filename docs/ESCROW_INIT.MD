# LiquifactEscrow — `init` Invariants

> **Scope:** Soroban `escrow` crate only.  
> No EVM/Solidity semantics apply. All examples use Rust/Soroban conventions.

---

## Table of Contents

- [LiquifactEscrow — `init` Invariants](#liquifactescrow--init-invariants)
  - [Table of Contents](#table-of-contents)
  - [Overview](#overview)
  - [Function signature](#function-signature)
  - [Invariant reference table](#invariant-reference-table)
  - [Invariant details](#invariant-details)
    - [INV-1 Positive amount](#inv-1-positive-amount)
    - [INV-2 yield\_bps range](#inv-2-yield_bps-range)
    - [INV-3/4 Tier table size](#inv-34-tier-table-size)
    - [INV-5 Tier table monotonicity](#inv-5-tier-table-monotonicity)
    - [INV-6 Per-tier bps range](#inv-6-per-tier-bps-range)
    - [INV-7 floor ≤ target](#inv-7-floor--target)
    - [INV-8 target ≤ cap](#inv-8-target--cap)
    - [INV-9 cap\_bps range](#inv-9-cap_bps-range)
    - [INV-10 Valid addresses](#inv-10-valid-addresses)
    - [INV-11 One-shot guarantee](#inv-11-one-shot-guarantee)
  - [Error codes](#error-codes)
  - [Validation order](#validation-order)
  - [Cap semantics](#cap-semantics)
  - [Token economics boundary](#token-economics-boundary)
  - [Security notes](#security-notes)
  - [Test coverage](#test-coverage)
    - [Test inventory](#test-inventory)

---

## Overview

`LiquifactEscrow::init` is the single initialisation entry-point for the
escrow contract. It locks funds for a `depositor`/`recipient` pair and
records the yield parameters that govern how interest is calculated during
the escrow period.

Because `init` writes the only mutable state that cannot be changed after
deployment, correctness here is load-bearing for the entire contract.

---

## Function signature

```rust
pub fn init(env: Env, params: EscrowParams) -> Result<(), EscrowError>
```

`EscrowParams` bundles all parameters to keep the call-site clean and to
allow atomic construction in tests.

---

## Invariant reference table

| ID | Field(s) | Rule | Error |
|----|----------|------|-------|
| INV-1 | `amount` | `> 0` (strictly positive) | `InvalidAmount` |
| INV-2 | `yield_bps` | `∈ [0, 10_000]` | `InvalidYieldBps` |
| INV-3 | `tiers` | Empty vec is valid (flat rate applies) | — |
| INV-4 | `tiers` | `len ≤ MAX_TIERS` (20) | `TierTableTooLarge` |
| INV-5 | `tiers[i].min_amount` | Strictly increasing across entries | `TierTableNotMonotonic` |
| INV-6 | `tiers[i].bps` | `∈ [0, 10_000]` for every tier | `InvalidTierBps` |
| INV-7 | `floor_bps`, `target_bps` | `floor_bps ≤ target_bps` | `FloorExceedsTarget` |
| INV-8 | `target_bps`, `cap_bps` | `target_bps ≤ cap_bps` | `TargetExceedsCap` |
| INV-9 | `cap_bps` | `∈ [0, 10_000]` | `CapOutOfRange` |
| INV-10 | `depositor`, `recipient` | Valid Soroban `Address` (non-zero, auth-capable) | `InvalidAddress` |
| INV-11 | storage | `init` may only be called once per deployment | `AlreadyInitialized` |

---

## Invariant details

### INV-1 Positive amount

```rust
if p.amount <= 0 {
    return Err(EscrowError::InvalidAmount);
}
```

`amount` is `i128` to match Soroban's token-balance type.  Zero is rejected
because an escrow with no funds locked is meaningless and would allow
zero-cost initialisation spam.  Negative values are rejected for the same
reason and because they cannot represent a real balance.

**Boundary:** `amount = 1` is the minimum accepted value.

---

### INV-2 yield_bps range

```rust
if p.yield_bps > MAX_BPS {   // MAX_BPS = 10_000
    return Err(EscrowError::InvalidYieldBps);
}
```

Basis points are bounded to `[0, 10_000]` (0 % – 100 %).  A flat yield
of 0 bps is valid and means the depositor earns no interest.  Values above
10 000 have no meaningful financial interpretation in this protocol and are
rejected to prevent misconfiguration.

---

### INV-3/4 Tier table size

An empty `tiers` vec is **valid**; the flat `yield_bps` applies in that
case.  Passing a non-empty `tiers` vec triggers size and content checks.

The upper bound `MAX_TIERS = 20` caps the ledger footprint of a single
`EscrowState` entry and bounds the CPU cost of the linear validation loop.

---

### INV-5 Tier table monotonicity

```rust
if tier.min_amount <= prev_min {
    return Err(EscrowError::TierTableNotMonotonic);
}
```

`min_amount` values must be **strictly increasing** (no equal elements).
This is required so that the highest-matching tier can be selected with a
single linear scan — duplicate entries would create ambiguity about which
tier applies at a given balance.

Both ascending-but-duplicate and descending orders are rejected.

---

### INV-6 Per-tier bps range

Each `YieldTier.bps` must be in `[0, 10_000]`.  The same reasoning as
INV-2 applies.  An invalid bps in any tier — not just the first — causes
`InvalidTierBps`.

---

### INV-7 floor ≤ target

```
floor_bps ≤ target_bps
```

`floor_bps` is the guaranteed minimum yield the protocol commits to.
`target_bps` is the intended operating yield.  The floor must never exceed
the target; a floor above the target would mean the guaranteed payout
exceeds the goal, which is economically incoherent.

**Equal values (`floor == target`) are accepted.** This models a fixed-rate
escrow with no upside.

---

### INV-8 target ≤ cap

```
target_bps ≤ cap_bps
```

The cap is the maximum yield the protocol will ever pay regardless of tier
table outputs.  A target above the cap would mean the goal is unreachable,
which signals misconfiguration.

**Equal values (`target == cap`) are accepted.** This disables upside
sharing — every outcome is clamped to the target.

---

### INV-9 cap_bps range

```rust
if p.cap_bps > MAX_BPS {
    return Err(EscrowError::CapOutOfRange);
}
```

Checked **before** INV-7 and INV-8 so that the relational checks always
operate on values that are known to be within `[0, 10_000]`.

---

### INV-10 Valid addresses

`depositor` and `recipient` must be valid Soroban `Address` values.
The `require_auth()` call on `depositor` at the start of `init` ensures
that:

1. The depositor explicitly consents to locking funds.
2. The zero address (which cannot sign) is implicitly rejected by the
   host layer before any Rust code runs.

`recipient` is not auth-gated at init time but must be a valid address
that can later be authorised when claiming.

---

### INV-11 One-shot guarantee

```rust
if env.storage().persistent().has(&symbol_short!(INIT_KEY)) {
    return Err(EscrowError::AlreadyInitialized);
}
```

The `INIT` flag is written to persistent storage after a successful `init`.
Any subsequent call returns `AlreadyInitialized` without modifying state.

This prevents a race condition where a second caller could overwrite the
depositor or recipient addresses after funds have been transferred in.

---

## Error codes

| Variant | `u32` | Description |
|---------|-------|-------------|
| `InvalidAmount` | 1 | `amount ≤ 0` |
| `InvalidYieldBps` | 2 | `yield_bps > 10_000` |
| `EmptyTierTable` | 3 | Reserved; currently unused (empty table is valid) |
| `TierTableTooLarge` | 4 | `tiers.len() > MAX_TIERS` |
| `TierTableNotMonotonic` | 5 | `min_amount` not strictly increasing |
| `InvalidTierBps` | 6 | A tier's `bps > 10_000` |
| `FloorExceedsTarget` | 7 | `floor_bps > target_bps` |
| `TargetExceedsCap` | 8 | `target_bps > cap_bps` |
| `CapOutOfRange` | 9 | `cap_bps > 10_000` |
| `InvalidAddress` | 10 | Reserved; enforced via host auth |
| `AlreadyInitialized` | 11 | `init` called more than once |

Error codes are stable across contract upgrades; **do not renumber**.

---

## Validation order

Validation runs in this fixed order, returning the **first** error found:

```
1. AlreadyInitialized   (one-shot guard)
2. depositor.require_auth()
3. InvalidAmount
4. InvalidYieldBps
5. CapOutOfRange        ← before relational checks so domain is clean
6. FloorExceedsTarget
7. TargetExceedsCap
8. validate_yield_tiers_table
   a. TierTableTooLarge
   b. For each tier: InvalidTierBps, TierTableNotMonotonic
```

This order is **load-bearing for tests** — `test_error_priority_*` cases
rely on it.  Do not reorder without updating those tests.

---

## Cap semantics

The `cap_bps` field sets a **hard ceiling** on the yield the protocol will
disburse:

```
effective_yield = min(tier_or_flat_bps, cap_bps)
effective_yield = max(effective_yield, floor_bps)
```

The floor–target–cap triple defines three operating zones:

| Zone | Condition | Meaning |
|------|-----------|---------|
| Below floor | effective < floor | Protocol tops up to floor (guaranteed minimum) |
| Normal | floor ≤ effective ≤ cap | Market rate applies |
| Capped | effective > cap | Yield is clamped at cap |

Setting `cap_bps == target_bps == floor_bps` creates a fixed-rate escrow
with no variable component.

> **Out of scope:** The actual yield accrual computation and token transfers
> are handled in `external_calls.rs` and are not validated by `init`.

---

## Token economics boundary

`init` does **not** perform any token transfer.  It only:

1. Validates parameters.
2. Writes `EscrowState` to persistent storage.
3. Emits an `escrow/init` event.

The caller is responsible for transferring `amount` tokens to the contract
address before or after `init` (per the protocol's deposit flow in
`external_calls.rs`).  This separation keeps the escrow contract
stateless with respect to token balances and avoids re-entrancy concerns.

---

## Security notes

- **No admin key.** Once initialised, `depositor` and `recipient` are
  immutable.  No privileged address can alter them.
- **Auth is explicit.** Soroban requires `require_auth()` to be called
  explicitly; unlike EVM, `msg.sender` does not implicitly authorise.
- **Overflow.** All arithmetic in validation uses direct comparisons on
  `u32` and `i128`; no addition or multiplication is performed, so
  overflow is not a concern in this function.
- **Reentrancy.** Soroban's execution model does not permit re-entrancy
  within a single transaction, so no reentrancy guard is needed.
- **Storage cost.** `MAX_TIERS = 20` bounds the size of the persisted
  `EscrowState` entry.  Callers that attempt to pass >20 tiers receive
  `TierTableTooLarge` before any storage write occurs.

---

## Test coverage

Tests live in `escrow/src/test/init.rs`.

Run coverage locally:

```sh
cargo llvm-cov --package escrow --html
# report opens at target/llvm-cov/html/index.html
```

CI gate: **≥ 95 % line coverage** on `escrow/src/lib.rs` (enforced via
`cargo llvm-cov --fail-under-lines 95`).

### Test inventory

| Test name | Invariant(s) covered |
|-----------|----------------------|
| `test_init_minimal_valid_no_tiers` | Happy path, no tiers |
| `test_init_with_tiers_valid` | Happy path, 3 tiers |
| `test_init_max_tiers_valid` | INV-4 boundary |
| `test_init_yield_bps_zero_valid` | INV-2 boundary |
| `test_init_all_bps_at_max_valid` | INV-2/7/8/9 boundaries |
| `test_get_state_round_trips_params` | State persistence |
| `test_inv1_amount_zero_rejected` | INV-1 |
| `test_inv1_amount_negative_rejected` | INV-1 |
| `test_inv1_amount_i128_min_rejected` | INV-1 edge |
| `test_inv1_amount_one_accepted` | INV-1 boundary |
| `test_inv2_yield_bps_above_max_rejected` | INV-2 |
| `test_inv2_yield_bps_at_max_accepted` | INV-2 boundary |
| `test_inv9_cap_above_max_rejected` | INV-9 |
| `test_inv9_cap_at_max_accepted` | INV-9 boundary |
| `test_inv7_floor_exceeds_target_rejected` | INV-7 |
| `test_inv7_floor_equals_target_accepted` | INV-7 boundary |
| `test_inv8_target_exceeds_cap_rejected` | INV-8 |
| `test_inv8_target_equals_cap_accepted` | INV-8 boundary |
| `test_inv4_tier_table_too_large_rejected` | INV-4 |
| `test_inv5_duplicate_min_amount_rejected` | INV-5 |
| `test_inv5_descending_min_amount_rejected` | INV-5 |
| `test_inv5_single_tier_accepted` | INV-5 boundary |
| `test_inv6_tier_bps_above_max_rejected` | INV-6 |
| `test_inv6_tier_bps_at_max_accepted` | INV-6 boundary |
| `test_inv6_tier_bps_zero_accepted` | INV-6 boundary |
| `test_inv11_double_init_rejected` | INV-11 |
| `test_edge_amount_i128_max_accepted` | INV-1 edge |
| `test_edge_all_bps_zero_accepted` | INV-2/7/8/9 edge |
| `test_edge_tier_table_exactly_max_tiers` | INV-4 boundary |
| `test_edge_tier_table_max_plus_one_rejected` | INV-4 boundary |
| `test_inv6_invalid_bps_in_second_tier` | INV-6 (non-first tier) |
| `test_error_priority_amount_before_yield_bps` | Error ordering |
| `test_error_priority_cap_before_floor_target` | Error ordering |
