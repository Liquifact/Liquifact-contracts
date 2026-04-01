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
- Panics if the investor already has a contribution (prevents re-selection).

**Follow-on deposits** — investor must use `fund()`, which reads the already-stored effective yield and does not allow re-selection.

## Consequences

- Tier selection is immutable after the first leg; an investor cannot upgrade their tier by calling `fund_with_commitment` again.
- `claim_investor_payout` enforces `InvestorClaimNotBefore` against ledger time.
- If no tier table is set, `fund_with_commitment` with `committed_lock_secs == 0` behaves identically to `fund`.
- Yield values are integer basis points only; fractional coupon math belongs off-chain.

## Rejected alternatives

- **Mutable tier selection:** allows gaming; immutability after first deposit is the fairness guarantee.
- **On-chain coupon calculation:** requires token custody and floating-point math; both are out of scope for this contract version.
