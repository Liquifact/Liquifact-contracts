# ADR-005: Optional Tiered Yield and Commitment Locks

**Status:** Accepted  
**Date:** 2026-03-28  
**Refs:** `escrow/src/lib.rs` — `validate_yield_tiers_table`, `effective_yield_for_commitment`, `fund_with_commitment`, `get_effective_yield_bps`, `compute_investor_payout`, `DataKey::YieldTierTable`, `DataKey::InvestorEffectiveYield`, `DataKey::InvestorClaimNotBefore`

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
- When both a positive commitment lock and a positive escrow `maturity` are configured, the deposit is rejected if `now + committed_lock_secs > maturity` with `CommitmentLockExceedsMaturity`.
- Emits `EscrowFunded` containing `tier_lock_secs` (the matched threshold, or 0 if base yield).
- Panics if the investor already has a contribution (prevents re-selection).

**Follow-on deposits** — investor must use `fund()`, which reads the already-stored effective yield and does not allow re-selection.

**Reading the resolved rate** — `get_effective_yield_bps(investor)` exposes the resolved rate that `compute_investor_payout` applies: `InvestorEffectiveYield(investor)` when set (tiered first deposit), otherwise the escrow base `yield_bps`. This is a pure read with no auth and no mutation, and it uses the *exact* fallback expression in the payout math so integrators do not re-implement the `unwrap_or(base)` resolution. `get_investor_yield_bps` is a historical alias returning the same value; the distinction is documentation framing (stored per-investor slot vs. resolved tier-or-base rate), not behavior. See `docs/escrow-read-api.md`.

## Consequences

- Tier selection is immutable after the first leg; an investor cannot upgrade their tier by calling `fund_with_commitment` again.
- `claim_investor_payout` enforces `InvestorClaimNotBefore` against ledger time.
- If no tier table is set, `fund_with_commitment` with `committed_lock_secs == 0` behaves identically to `fund`.
- Yield values are integer basis points only; fractional coupon math belongs off-chain.

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
| `test_effective_yield_bps_tiered_returns_tier_yield` | `get_effective_yield_bps` returns the selected tier yield for a tiered investor |
| `test_effective_yield_bps_non_tiered_returns_base` | `get_effective_yield_bps` returns base yield for a plain `fund()` investor |
| `test_effective_yield_bps_unknown_investor_returns_base` | `get_effective_yield_bps` returns base yield for an address that never funded |
| `test_effective_yield_bps_zero_base_yield` | `get_effective_yield_bps` resolves to `0` when base yield is `0` |
| `test_effective_yield_bps_matches_payout_resolution` | `get_effective_yield_bps` matches the yield `compute_investor_payout` applies |

## Read API

The tier table is readable after `init` via `get_yield_tiers() -> Vec<YieldTier>`.

- Returns an empty `Vec` when no tiers were configured.
- Returned order matches the validated non-decreasing ordering enforced at `init`.
- Pure read — no auth required, no state mutation.
- See `docs/escrow-read-api.md` for the full getter reference.

## Worked examples

All numeric examples below use a three-tier table configured at `init`:

| Tier | `min_lock_secs` | `yield_bps` |
|------|-----------------|-------------|
| 0 | 30 | 700 |
| 1 | 60 | 900 |
| 2 | 90 | 1_200 |

Base yield (`yield_bps` at init) = **500 bps**.

### Tier selection at first deposit

An investor calls `fund_with_commitment(investor, amount, lock_secs)` on their **first** deposit only:

| `lock_secs` | Matched tier | Effective yield | `InvestorClaimNotBefore` |
|-------------|--------------|-----------------|---------------------------|
| 0 | (none) | 500 (base) | `0` (no claim gate) |
| 45 | tier 0 | 700 | `ledger.timestamp() + 45` |
| 60 | tier 1 | 900 | `ledger.timestamp() + 60` |
| 120 | tier 2 | 1_200 | `ledger.timestamp() + 120` |

The contract picks the **highest-yield tier** whose `min_lock_secs <= lock_secs`. If no tier qualifies, the base yield applies.

### Follow-on deposits

After the first deposit, the investor **must** use plain `fund()` for additional principal:

```text
fund_with_commitment(100_000, lock=60)  → effective yield 900, claim lock set
fund(50_000)                            → adds principal at 900 bps; lock unchanged
fund_with_commitment(...)               → TieredSecondDeposit (rejected)
```

### Validation rejections at `init`

| Misconfigured table | Error |
|---------------------|-------|
| Tier yield 400 when base is 500 | `TierYieldBelowBase` |
| Locks `[60, 30]` (not strictly increasing) | `TierLockNotIncreasing` |
| Yields `[900, 800]` (decreasing) | `TierYieldNotNonDecreasing` |

### Claim-time enforcement

`claim_investor_payout` requires `ledger.timestamp() >= InvestorClaimNotBefore(investor)` when the stored value is non-zero. The lock is anchored to the **first** `fund_with_commitment` call and is **not** reset by follow-on `fund()` calls.