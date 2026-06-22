# ADR-005: Optional Tiered Yield and Commitment Locks

**Status:** Accepted  
**Date:** 2026-03-28  
**Refs:** `escrow/src/lib.rs` — `validate_yield_tiers_table`, `effective_yield_for_commitment`, `fund_with_commitment`, `DataKey::YieldTierTable`, `DataKey::InvestorEffectiveYield`, `DataKey::InvestorClaimNotBefore`

---

## Context

Some invoice products offer higher yield to investors who commit to a longer lock period. The tier table must be fair, immutable after deploy, and not allow an investor to game their rate after their first deposit.

## Decision

`init` accepts an optional `Vec<YieldTier>` stored under `DataKey::YieldTierTable`. Each tier has `min_lock_secs` and `yield_bps`. Validation at init enforces:

- `min_lock_secs` strictly increasing across tiers.
- `yield_bps` non-decreasing and each tier `>= base yield_bps`.
- Each tier `yield_bps` in `0..=10_000`.

**First deposit** — investor calls `fund_with_commitment(investor, amount, committed_lock_secs)`:
- Selects the best matching tier where `committed_lock_secs >= tier.min_lock_secs`.
- Stores result under `DataKey::InvestorEffectiveYield(investor)`.
- If `committed_lock_secs > 0`, stores `ledger.timestamp() + committed_lock_secs` under `DataKey::InvestorClaimNotBefore(investor)`.
- Emits `EscrowFunded` containing `tier_lock_secs` (the matched threshold, or 0 if base yield).
- Panics if the investor already has a contribution (prevents re-selection).

**Follow-on deposits** — investor must use `fund()`, which reads the already-stored effective yield and does not allow re-selection.

## Consequences

- Tier selection is immutable after the first leg; an investor cannot upgrade their tier by calling `fund_with_commitment` again.
- `claim_investor_payout` enforces `InvestorClaimNotBefore` against ledger time.
- If no tier table is set, `fund_with_commitment` with `committed_lock_secs == 0` behaves identically to `fund`.
- Yield values are integer basis points only; fractional coupon math belongs off-chain.

## Worked Examples

Assume an escrow is initialized with `yield_bps = 500` (5%) and this tier table:

| Tier | `min_lock_secs` | `yield_bps` | Meaning |
|---:|---:|---:|---|
| 1 | `2_592_000` | `650` | 30-day commitment earns 6.5% |
| 2 | `7_776_000` | `800` | 90-day commitment earns 8.0% |
| 3 | `15_552_000` | `950` | 180-day commitment earns 9.5% |

### Tier-table validation

The table is accepted because:

- every tier yield is in `0..=10_000`;
- every tier yield is `>= base yield_bps` (`500`);
- `min_lock_secs` strictly increases (`30d < 90d < 180d`);
- `yield_bps` is non-decreasing (`650 <= 800 <= 950`).

Rejected examples:

| Invalid table fragment | Rejection |
|---|---|
| `[{ min_lock_secs: 2_592_000, yield_bps: 400 }]` | `TierYieldBelowBase` because `400 < base 500` |
| `[{ min_lock_secs: 2_592_000, yield_bps: 650 }, { min_lock_secs: 2_592_000, yield_bps: 800 }]` | `TierLockNotIncreasing` because lock thresholds must be strictly increasing |
| `[{ min_lock_secs: 2_592_000, yield_bps: 800 }, { min_lock_secs: 7_776_000, yield_bps: 700 }]` | `TierYieldNotNonDecreasing` because later tiers must not reduce yield |

### First deposit selects the effective yield

At ledger timestamp `1_710_000_000`, an investor calls:

```text
fund_with_commitment(investor, 1_000_000_000, 7_776_000)
```

The commitment is 90 days, so `effective_yield_for_commitment` selects tier 2:

| Stored key | Stored value |
|---|---:|
| `InvestorEffectiveYield(investor)` | `800` |
| `InvestorClaimNotBefore(investor)` | `1_717_776_000` |

The emitted `EscrowFunded` event includes:

| Field | Value |
|---|---:|
| `investor_effective_yield_bps` | `800` |
| `tier_lock_secs` | `7_776_000` |

### Commitments between thresholds

If the same tier table receives `committed_lock_secs = 5_184_000` (60 days), the
best matching threshold is the 30-day tier, not the 90-day tier:

| Input commitment | Matched `tier_lock_secs` | Effective yield |
|---:|---:|---:|
| `0` | `0` | `500` |
| `2_592_000` | `2_592_000` | `650` |
| `5_184_000` | `2_592_000` | `650` |
| `7_776_000` | `7_776_000` | `800` |
| `15_552_000` | `15_552_000` | `950` |

### Follow-on principal must use `fund`

After a first `fund_with_commitment`, a second `fund_with_commitment` from the
same investor is rejected with `TieredSecondDeposit`. The investor may add
principal only through `fund`, which preserves the stored effective yield and
claim lock.

Example:

1. First call: `fund_with_commitment(investor, 1_000_000_000, 7_776_000)` stores
   `InvestorEffectiveYield = 800` and `InvestorClaimNotBefore = 1_717_776_000`.
2. Follow-on call: `fund(investor, 250_000_000)` increases contribution but
   keeps the same `InvestorEffectiveYield` and `InvestorClaimNotBefore`.
3. Invalid call: `fund_with_commitment(investor, 250_000_000, 15_552_000)` is
   rejected; the investor cannot upgrade to the 180-day tier after the first leg.

### Claim lock enforcement

`claim_investor_payout` compares the current ledger timestamp against
`InvestorClaimNotBefore`.

| Ledger timestamp | Result |
|---:|---|
| `1_717_775_999` | rejected with `InvestorCommitmentLockNotExpired` |
| `1_717_776_000` | allowed if the escrow is settled and the investor has contribution |

The comparison is inclusive: `now >= InvestorClaimNotBefore` is sufficient.

## Rejected alternatives

- **Mutable tier selection:** allows gaming; immutability after first deposit is the fairness guarantee.
- **On-chain coupon calculation:** requires token custody and floating-point math; both are out of scope for this contract version.

## Test coverage

The state-machine rules above are verified in `escrow/src/tests/funding.rs`:

| Test | Rule verified |
|---|---|
| `test_fund_with_commitment_twice_panics` | Second `fund_with_commitment` from same investor panics |
| `test_fund_then_fund_with_commitment_panics` | `fund → fund_with_commitment` (inverse) panics |
| `test_fund_first_then_commitment_second_panics` | Same inverse rule, with tier table present |
| `test_second_fund_with_commitment_panics_without_tier_table` | Second `fund_with_commitment` panics on base-only escrow |
| `test_tiered_yield_and_follow_on_fund` | Follow-on `fund()` succeeds and preserves tier yield |
| `test_commitment_claim_lock_preserved_after_follow_on_fund` | Follow-on `fund()` preserves `InvestorClaimNotBefore` |
| `test_commitment_invariant_across_multiple_follow_on_funds` | Invariant holds across 3 consecutive follow-on `fund()` calls |
| `test_fund_with_commitment_zero_lock_behaves_as_fund` | `committed_lock_secs == 0` → base yield, `InvestorClaimNotBefore == 0` |
| `test_commitment_zero_lock_follow_on_fund_no_claim_gate` | Follow-on `fund()` after zero-lock preserves both zero guards |
| `test_fund_first_deposit_sets_base_yield_and_no_claim_gate` | Plain `fund()` first deposit → base yield, no claim gate |
