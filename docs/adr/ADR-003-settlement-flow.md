# ADR-003: Settlement Flow

**Status:** Accepted  
**Date:** 2026-03-28  
**Refs:** `escrow/src/lib.rs` — `init`, `settle`, `claim_investor_payout`, `fund_impl` (snapshot write), `get_funding_close_snapshot`; `docs/escrow-ledger-time.md`

---

## Context

After funding is complete the contract needs a deterministic path from "funded" to "investors paid" that is safe under legal hold, respects optional maturity gates, and gives indexers a stable pro-rata denominator.

## Decision

Settlement is a two-phase process:

**Phase 1 — SME settles (`settle`)**
1. Requires `status == 1` and SME auth.
2. If `maturity > 0`, requires `ledger.timestamp() >= maturity`.
3. Blocked while `LegalHold` is active.
4. Sets `status = 2` and emits `EscrowSettled`.

The maturity comparison is an inclusive integer-second boundary against
validator-observed ledger time. A call at exactly `maturity` succeeds; a call at
`maturity - 1` traps with `"Escrow has not yet reached maturity"` and leaves the
funded state unchanged.

`maturity == 0` is not a timestamp lock. It is the explicit "no maturity lock"
configuration, so a funded escrow can settle immediately once SME auth, status,
and legal-hold guards pass. To prevent operators and indexers from mistaking
zero for a deferred lock, `EscrowInitialized` and `EscrowSummary` surface
`has_maturity_lock = false` when `maturity == 0`.

**Phase 2 — Investor claims (`claim_investor_payout`)**
1. Requires `status == 2` and investor auth.
2. Blocked while `LegalHold` is active.
3. Checks `InvestorClaimNotBefore` (set by `fund_with_commitment`); rejects if ledger time is too early.
4. Marks `InvestorClaimed(investor) = true` (idempotency guard — panics on second call).
5. Emits `InvestorPayoutClaimed`.

**Pro-rata denominator — `FundingCloseSnapshot`**  
Written once, atomically, on the first transition to `status == 1` inside `fund_impl`. Contains `total_principal` (including overfunding), `funding_target`, ledger timestamp, and sequence. Immutable thereafter. Off-chain payout math uses `get_contribution(investor) / snapshot.total_principal`.

## Consequences

- Maturity is enforced as `now >= maturity` (inclusive boundary) against
  validator-observed ledger time; `maturity == 0` means no gate and is surfaced
  as `has_maturity_lock = false`.
- Overfunding is absorbed into `total_principal` so late investors are not disadvantaged in pro-rata math.
- The snapshot is written exactly once; a second fund call after `status == 1` cannot shift the denominator (escrow rejects it).
- Claim is a marker only — no token transfer happens inside the contract. The integration layer handles actual payout using the snapshot and contribution data.

## Rejected alternatives

- **Single-step settle-and-pay:** requires the contract to hold and transfer tokens for all investors atomically, which is expensive and introduces token custody risk.
- **Mutable snapshot:** would allow the denominator to shift after close, breaking pro-rata fairness.
- **Wall-clock oracle for maturity:** Stellar has no trusted time oracle; ledger timestamp is the only safe source.
