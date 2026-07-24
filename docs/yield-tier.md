# Yield-tier model and invariants

This note documents the optional tier ladder used by the escrow contract for commitment-based yield. The implementation lives in [escrow/src/lib.rs](../escrow/src/lib.rs), and the behavior is exercised by the funding and boundary tests under [escrow/src/tests](../escrow/src/tests).

## 1. What the model stores

The tier ladder is optional configuration supplied to [escrow/src/lib.rs](../escrow/src/lib.rs) during init. It is stored as a `Vec<YieldTier>` under the instance storage key `DataKey::YieldTierTable`.

Each entry is:

- `min_lock_secs`: the minimum commitment lock required to match that tier.
- `yield_bps`: the yield rate awarded when the investor matches that tier.

The contract also carries the base escrow yield in `InvoiceEscrow::yield_bps`. That base yield is the fallback when no tier matches.

## 2. Invariants enforced at init

The ladder is validated once, during init, and then treated as immutable for the life of the escrow instance.

The invariants are:

1. `yield_bps` must be in the range `0..=10_000`.
2. Every tier must have `yield_bps >= base_yield_bps`.
3. Tiers must be ordered by strictly increasing `min_lock_secs`.
4. Tiers must be ordered by non-decreasing `yield_bps`.
5. If no tiers are configured (or the table is empty), the escrow behaves as a base-yield-only contract.

These rules are enforced by the validation helpers in [escrow/src/lib.rs](../escrow/src/lib.rs), which reject malformed ladders with typed errors such as `TierYieldBelowBase`, `TierLockNotIncreasing`, and `TierYieldNotNonDecreasing`.

## 3. How tier selection works

Tier selection happens when an investor makes their first contribution using `fund_with_commitment`.

The selection rule is:

- If the commitment lock is `0`, or there is no tier table, the base yield applies.
- Otherwise, pick the highest-yield tier whose `min_lock_secs <= committed_lock_secs`.
- If no tier qualifies, the base yield applies.

The resolved rate is then stored per investor under `DataKey::InvestorEffectiveYield(investor)` so later funding calls reuse the same rate.

## 4. Entry points that touch the model

### Init

`init(...)` accepts an optional `yield_tiers` argument and validates it before persisting it to `DataKey::YieldTierTable`.

Relevant behavior:

- The table is written once at init.
- It is not rewritten by later entrypoints.
- The base yield remains the fallback for non-tiered investors.

### First deposit: `fund_with_commitment`

`fund_with_commitment(investor, amount, committed_lock_secs)` is the only entrypoint that can select a tier for a new investor.

Behavior:

- It rejects a second tiered first-deposit attempt from the same investor with `TieredSecondDeposit`.
- It writes the selected effective yield into `DataKey::InvestorEffectiveYield(investor)`.
- If the lock is non-zero, it also writes `DataKey::InvestorClaimNotBefore(investor)` as `ledger.timestamp() + committed_lock_secs`.

### Follow-on funding: `fund`

`fund(investor, amount)` does not select a new tier. It preserves the investor's already-resolved yield and claim gate from the first deposit.

### Read views

The model is exposed through several pure-read entrypoints:

- `get_yield_tiers()` returns the configured ladder in the same order that was validated at init.
- `preview_yield_tier(amount, lock)` returns the same `(effective_yield_bps, matched_lock_secs)` resolution that `fund_with_commitment` would use.
- `get_effective_yield_bps(investor)` resolves the investor's effective yield for payout math.
- `get_investor_yield_bps(investor)` returns the same resolved value for compatibility with older naming.

## 5. Worked example

Assume the escrow base yield is `800 bps` and the init-time ladder is:

| Tier | `min_lock_secs` | `yield_bps` |
| --- | ---: | ---: |
| 0 | 100 | 900 |
| 1 | 200 | 1_000 |
| 2 | 300 | 1_200 |

Then:

| Commitment lock | Selected tier | Effective yield | Claim gate |
| --- | --- | ---: | --- |
| `0` | none | `800` | none |
| `50` | none | `800` | none |
| `100` | tier 0 | `900` | `now + 100` |
| `250` | tier 1 | `1_000` | `now + 250` |
| `300` | tier 2 | `1_200` | `now + 300` |

That example is aligned with the resolution helper in [escrow/src/lib.rs](../escrow/src/lib.rs): the contract always selects the highest-yield tier whose threshold is met, and it never re-selects a new tier on a later `fund()` call.

## 6. Why the invariants matter

These invariants keep the tier ladder fair and deterministic:

- investors cannot upgrade to a higher tier by making a second deposit;
- the ladder cannot be silently reordered or weakened after init;
- the payout rate for an investor is stable once the first deposit resolves it;
- claim timing remains tied to the original first-deposit commitment lock.
