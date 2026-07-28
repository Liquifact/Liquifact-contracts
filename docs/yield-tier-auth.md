# Yield-Tier Authorization and Access Rules

## Overview

This document is the authorization-focused companion to [ADR-005: Optional Tiered
Yield and Commitment Locks](adr/ADR-005-tiered-yield.md) (design/state rules) and
[ADR-002: Authorization Boundaries](adr/ADR-002-auth-boundaries.md) (contract-wide
signer map). It answers three questions specifically for the yield-tier feature:

1. **Who** may call each yield-tier entrypoint (which role must sign)?
2. **In which escrow/investor state** is the call accepted?
3. **What typed error** is raised, and why, when a rule is violated?

All entrypoints referenced below live in `escrow/src/lib.rs`.

## Roles

Yield-tier touches exactly two roles — no SME or treasury involvement:

| Role | Stored as | Yield-tier responsibility |
|---|---|---|
| **admin** | `InvoiceEscrow::admin` | Supplies the immutable tier table once, at `init`. No entrypoint updates it afterward. |
| **investor** | per-call `Address` parameter | Selects a tier (or opts out) on their own **first** deposit only, via their own signature. |

There is no admin override for an investor's selected tier, and no investor can act
on another investor's behalf — each `Address` parameter is the signer required for
that call.

## Entrypoint Authorization Matrix

| Entrypoint | Required signer | Enforced via | Escrow state required | Investor state required |
|---|---|---|---|---|
| `init` | `admin` | `admin.require_auth()` | Not yet initialized | n/a |
| `fund_with_commitment` | `investor` | `investor.require_auth()` (in `fund_impl`) | `status == 0` (open) | No prior contribution (`prev == 0`) |
| `fund` | `investor` | `investor.require_auth()` (in `fund_impl`) | `status == 0` (open) | Any (first deposit or follow-on) |
| `fund_batch` | each entry's `investor` | per-entry `investor.require_auth()` (in `fund_impl`) | `status == 0` (open) | Any, per entry |
| `claim_investor_payout` | `investor` | `investor.require_auth()` | `status == 2` (settled) | `contribution > 0`; `now >= InvestorClaimNotBefore` |
| `get_yield_tiers` | none (pure read) | — | any | n/a |
| `get_investor_yield_bps` | none (pure read) | — | any | n/a |
| `get_investor_claim_not_before` | none (pure read) | — | any | n/a |
| `preview_yield_tier` | none (pure read) | — | any | n/a (simulates a hypothetical deposit) |

`fund_with_commitment` and `fund` share one internal implementation, `fund_impl`,
which is where `investor.require_auth()` actually executes — first statement in the
function body, before any read of escrow state (per the read-only-preconditions →
`require_auth` → storage-writes ordering documented in ADR-002).

## Allowed Transitions

Tier selection follows a strict once-only rule per investor address:

```text
No contribution (prev == 0)
   │
   ├─ fund_with_commitment(investor, amount, lock_secs)   ─▶  tier selected (or base yield if lock_secs == 0
   │                                                            or no tier table); InvestorEffectiveYield and
   │                                                            InvestorClaimNotBefore are written.
   │
   └─ fund(investor, amount)                              ─▶  base yield locked in; InvestorClaimNotBefore = 0.

Has contribution (prev > 0)
   │
   ├─ fund(investor, amount)                              ─▶  adds principal at the already-stored
   │                                                            InvestorEffectiveYield; lock is unchanged.
   │
   └─ fund_with_commitment(investor, amount, lock_secs)    ─▶  REJECTED: TieredSecondDeposit (108).
```

Once `InvestorEffectiveYield(investor)` is written on the first deposit, it is
**immutable** for that investor: no entrypoint (admin or investor) can change it.
This is the fairness guarantee ADR-005 calls out — an investor cannot upgrade their
own rate after committing, and the admin cannot alter it after the fact either,
since no such entrypoint exists.

## Rejections

### At `init` (admin-signed) — tier table validation

Enforced by `validate_yield_tiers_table`, called before the tier table is stored:

| Rule violated | Error | Code |
|---|---|---|
| Base `yield_bps` not in `0..=10_000` | `YieldBpsOutOfRange` | 2 |
| Escrow already initialized | `EscrowAlreadyInitialized` | 3 |
| Tier `yield_bps` not in `0..=10_000` | `TierYieldOutOfRange` | 10 |
| Tier `yield_bps < base yield_bps` | `TierYieldBelowBase` | 11 |
| Tier `min_lock_secs` not strictly increasing vs. previous tier | `TierLockNotIncreasing` | 12 |
| Tier `yield_bps` decreases vs. previous tier | `TierYieldNotNonDecreasing` | 13 |

### At `fund` / `fund_with_commitment` / `fund_batch` (investor-signed)

Checked in this order inside `fund_impl`, after `investor.require_auth()` succeeds:

| Rule violated | Error | Code |
|---|---|---|
| `amount <= 0` | `FundingAmountNotPositive` | 100 |
| `amount < min_contribution` floor | `FundingBelowMinContribution` | 101 |
| Operational pause active | `PausedBlocksFunding` | 210 |
| Legal hold active | `LegalHoldBlocksFunding` | 102 |
| `escrow.status != 0` (not open) | `EscrowNotOpenForFunding` | 103 |
| `now > FundingDeadline` (if set) | `FundingDeadlinePassed` | 164 |
| Allowlist active and investor not allowlisted | `InvestorNotAllowlisted` | 104 |
| `prev + amount` overflows `i128` | `InvestorContributionOverflow` | 105 |
| `prev + amount > MaxPerInvestorCap` (if set) | `InvestorContributionExceedsCap` | 106 |
| New investor and `UniqueFunderCount == MaxUniqueInvestorsCap` (if set) | `UniqueInvestorCapReached` | 107 |
| **`fund_with_commitment` called with `prev > 0`** (tier re-selection attempt) | `TieredSecondDeposit` | 108 |
| `now + committed_lock_secs` overflows `u64` | `InvestorClaimTimeOverflow` | 109 |
| `escrow.funded_amount + amount` overflows `i128` | `FundedAmountOverflow` | 110 |
| Resulting claim lock (`now + committed_lock_secs`) exceeds `maturity` (when `maturity > 0`) | `CommitmentLockExceedsMaturity` | 111 |

`TieredSecondDeposit` is the tier-specific rejection: it is the only error in this
table that fires exclusively for `fund_with_commitment`, and only because the
investor already has principal recorded. Calling `fund` instead, with the same
prior state, succeeds and simply adds principal at the stored rate.

### At `claim_investor_payout` (investor-signed)

| Rule violated | Error | Code |
|---|---|---|
| Operational pause active | `PausedBlocksInvestorClaims` | 213 |
| Legal hold active | `LegalHoldBlocksInvestorClaims` | 125 |
| `contribution == 0` (never funded) | `NoContributionToClaim` | 126 |
| `escrow.status != 2` (not settled) | `InvestorClaimNotSettled` | 127 |
| `now < InvestorClaimNotBefore` (tier lock not elapsed) | `InvestorCommitmentLockNotExpired` | 128 |
| Computed payout is `0` | `PayoutZero` | 170 |

`InvestorCommitmentLockNotExpired` is the tier-specific claim gate: it only rejects
when the investor selected a tier with `min_lock_secs > 0` (or otherwise supplied
`committed_lock_secs > 0`) and the lock has not yet elapsed. Investors who used plain
`fund()`, or `fund_with_commitment` with `committed_lock_secs == 0`, have
`InvestorClaimNotBefore == 0` and are never blocked by this check.

## Worked Example

Configuration at `init` (admin-signed), three-tier table over a 500 bps base:

| Tier | `min_lock_secs` | `yield_bps` |
|------|-----------------|-------------|
| 0 | 30 | 700 |
| 1 | 60 | 900 |
| 2 | 90 | 1,200 |

`maturity` = 200 (seconds from ledger epoch 0, for this example).

**Step 1 — Investor A's first deposit, tiered.**
Investor A signs `fund_with_commitment(A, 100_000, 60)` at `ledger.timestamp() == 0`.

- `A.require_auth()` succeeds (A signed the call).
- `escrow.status == 0` — open, passes `require_funding_open`.
- `prev == 0` — no existing contribution, so tier selection is allowed.
- Tier lookup: highest tier with `min_lock_secs <= 60` is tier 1 → effective yield
  `900`, matched lock `60`.
- `claim_nb = 0 + 60 = 60`; `60 <= maturity(200)` — passes `CommitmentLockExceedsMaturity`.
- Result: `InvestorEffectiveYield(A) = 900`, `InvestorClaimNotBefore(A) = 60`.

**Step 2 — Investor A tries to re-select a tier.**
Investor A signs `fund_with_commitment(A, 50_000, 90)`.

- `A.require_auth()` succeeds — but auth success does not bypass state checks.
- `prev == 100_000 > 0` → **rejected**: `TieredSecondDeposit` (108). Investor A must
  use `fund()` for the additional principal; the 900 bps rate and the lock at 60
  are untouched by the rejected call.

**Step 3 — Investor A adds a follow-on deposit correctly.**
Investor A signs `fund(A, 50_000)`.

- `A.require_auth()` succeeds; `prev == 100_000 > 0` so this takes the `simple_fund`
  follow-on path, which does **not** re-derive the tier.
- Stored `InvestorEffectiveYield(A) = 900` is read and reused; `InvestorClaimNotBefore`
  is untouched. Contribution becomes `150_000` at 900 bps.

**Step 4 — Investor A claims before the lock elapses.**
At `ledger.timestamp() == 59`, after settlement (`status == 2`), Investor A signs
`claim_investor_payout(A)`.

- `A.require_auth()` succeeds; `contribution > 0`; `escrow.status == 2`.
- `now(59) < InvestorClaimNotBefore(60)` → **rejected**:
  `InvestorCommitmentLockNotExpired` (128).

**Step 5 — Investor A claims after the lock elapses.**
At `ledger.timestamp() == 60`, Investor A signs `claim_investor_payout(A)` again.

- All gates pass (`60 >= 60` is the inclusive boundary); payout is computed at the
  locked-in 900 bps and transferred to A.

**Step 6 — A different admin key tries to change Investor A's tier.**
There is no entrypoint that takes an admin signature and mutates
`InvestorEffectiveYield` or `InvestorClaimNotBefore` for an existing investor — the
call simply does not exist. This is the authorization boundary in practice: the
tier table is admin-authored once, at `init`, and every subsequent state that
depends on it (`InvestorEffectiveYield`, `InvestorClaimNotBefore`) is written only
by the investor's own `fund` / `fund_with_commitment` call, gated by that
investor's own signature.

## Related Documentation

- [ADR-005: Optional Tiered Yield and Commitment Locks](adr/ADR-005-tiered-yield.md) — design rationale, state rules, and read API.
- [ADR-002: Authorization Boundaries](adr/ADR-002-auth-boundaries.md) — contract-wide signer map and guard ordering.
- [Escrow Error Messages](escrow-error-messages.md) — full typed-error registry, including all codes referenced above.
- [Escrow Ledger Time](escrow-ledger-time.md) — `InvestorClaimNotBefore` / `not_before` semantics.
- [Escrow Read API](escrow-read-api.md) — `get_yield_tiers`, `get_investor_yield_bps`, `preview_yield_tier` reference.
