# ADR-002: Authorization Boundaries

**Status:** Accepted  
**Date:** 2026-03-28  
**Refs:** `escrow/src/lib.rs` — `init`, `fund`, `settle`, `withdraw`, `claim_investor_payout`, `sweep_terminal_dust`, `set_legal_hold`, `propose_admin`, `accept_admin`, `propose_treasury`, `accept_treasury`, `cancel_treasury_rotation`

---

## Context

Multiple principals interact with the escrow (admin, SME, investors, treasury). Each entrypoint must enforce exactly the right `require_auth()` call so no role can act outside its boundary.

## Decision

| Entrypoint | Required signer |
|---|---|
| `init` | `admin` |
| `fund`, `fund_with_commitment` | `investor` (per-call) |
| `settle`, `withdraw` | `sme_address` |
| `claim_investor_payout` | `investor` |
| `sweep_terminal_dust` | current `treasury` (rotatable via two-step handover) |
| `set_legal_hold`, `clear_legal_hold` | `admin` |
| `update_funding_target`, `update_maturity`, `migrate` | `admin` |
| `propose_admin` | current `admin` |
| `accept_admin` | pending admin stored in `DataKey::PendingAdmin` |
| `propose_treasury` | current `admin` |
| `accept_treasury` | pending treasury stored in `DataKey::PendingTreasury` |
| `cancel_treasury_rotation` | current `admin` |
| `record_sme_collateral_commitment` | `sme_address` |
| `batch_record_collateral` | `sme_address` |

`admin` and `treasury` are stored at `init`. Both rotate only through two-step handovers. The current admin calls `propose_admin(new_admin)`, which stores `DataKey::PendingAdmin` without changing authority. The pending address must then call `accept_admin()`, which requires its own authorization, promotes it into `InvoiceEscrow::admin`, and clears `DataKey::PendingAdmin`. The deprecated `transfer_admin` shim must not be treated as an immediate transfer; it only creates the pending proposal.

The treasury rotates through a time-delayed two-step handover so fee routing cannot be switched instantly. The admin calls `propose_treasury(new_treasury)`, which stores `DataKey::PendingTreasury = (new_treasury, eta)` with `eta = now + TREASURY_ROTATION_DELAY_SECS` (`N00`). The proposal rejects the current treasury (`NewTreasurySameAsCurrent`) and the contract's own address (`TreasuryInvalid`). The pending treasury must then call `accept_treasury()`, which requires its own authorization and `now >= eta` (`TreasuryRotationNotReady`), swaps `DataKey::Treasury`, and clears `DataKey::PendingTreasury`. The admin can also `cancel_treasury_rotation()`; with nothing pending it fails with `NoPendingTreasury`. A new proposal overwrites any existing pending proposal and resets the delay.

There is no superuser that can act as all roles simultaneously unless the same key is used for multiple roles — which is a deployment concern, not a contract concern.

Admin proposals preserve role separation on-chain: the proposed admin may not equal the current
SME beneficiary, immutable treasury, or escrow contract address. `accept_admin` repeats this
check because `rotate_beneficiary` can change the SME between proposal and acceptance. Explicit
proposal windows are bounded to 1 hour through 30 days; omitted windows use the 7-day default.
The deprecated `transfer_admin` shim delegates to `propose_admin`, so it cannot bypass either
restriction.

## Consequences

- A compromised investor key cannot settle or sweep funds.
- A compromised SME key cannot change the admin or sweep dust.
- Treasury auth on `sweep_terminal_dust` means the admin cannot drain the contract as "dust" unless it is also the treasury.
- Legal hold can only be set/cleared by admin, so governance controls compliance freezes.
- Admin authority changes only after both the current admin and successor have authorized. A typo in `new_admin` can be corrected by a new `propose_admin` call and does not lock admin-gated paths.
- `DataKey::PendingAdmin` is cleared after successful acceptance so stale proposals cannot be reused.
- Treasury rotation cannot be switched instantly: the admin alone cannot redirect fees, and the new treasury must sign after `TREASURY_ROTATION_DELAY_SECS`. A compromised admin key cannot route fees to an attacker without the attacker controlling the proposed treasury and waiting out the delay, during which the proposal can be cancelled.
- `DataKey::PendingTreasury` is cleared after successful acceptance or cancellation so stale proposals cannot be reused.
- Treasury rotation is not gated by legal hold: rotation neither moves funds nor changes settlement outcomes, and blocking it would let a hold prevent recovery from a compromised treasury key.

## Rejected alternatives

- **Admin can do everything:** creates a single point of failure; role separation limits blast radius.
- **No treasury auth on sweep:** would let anyone trigger dust transfers once terminal; treasury auth is a cheap extra gate.
- **Immediate treasury rotation by admin:** would let a compromised admin redirect fees instantly; the time-delayed two-step handover requires the new treasury to sign and gives the admin a window to cancel.

## No-Op Guards

Certain administrative entrypoints enforce a no-op guard to reject updates that do not change the existing state, preventing noisy events and wasted storage writes:
- **`update_maturity`**: Rejects `new_maturity == old_maturity` (`MaturityUnchanged`).
- **`propose_admin`**: Rejects `new_admin == current_admin` (`NewAdminSameAsCurrent`) and
  candidates that collide with the SME, treasury, or escrow contract address
  (`AdminCandidateConflictsWithRole`).
- **`rotate_beneficiary`**: Rejects `new_sme == current_sme` (`NewSmeSameAsCurrent`).
- **`propose_treasury`**: Rejects `new_treasury == current_treasury` (`NewTreasurySameAsCurrent`) and `treasury == contract_address` (`TreasuryInvalid`).

## Shared gate helpers (issue #626)

The repeated pre-`require_auth` checks are centralised in private helpers so a new
risk-bearing entrypoint cannot accidentally omit or diverge from them:

| Helper | Signature | Replaces |
|---|---|---|
| `guard_not_legal_hold` | `fn(env, error)` | inline `ensure(!legal_hold_active, LegalHoldBlocks*)` in `sweep_terminal_dust`, `rotate_beneficiary`, `fund_impl`, `partial_settle`, `settle`, `withdraw`, `claim_investor_payout`, `cancel_funding` |
| `guard_status_eq` | `fn(env, status, expected, error)` | inline `ensure(status == expected, ...)` checks |
| `guard_status_in` | `fn(env, status, &[Allowed], error)` | inline `ensure(matches!(status, ...), ...)` membership checks |
| `is_terminal_status` | predicate | inline `(status == 2 || status == 3 || status == 4)`, used by `sweep_terminal_dust` |
| `is_pre_settlement_status` | predicate | inline `(status == 0 || status == 1)`, used by `rotate_beneficiary` |
| `require_funding_open` | `fn(env, status)` | inline `ensure(status == 0, EscrowNotOpenForFunding)`, used by `fund_impl` |

All helpers are `#[inline(always)]` `pub(crate) fn` (or `const`) at the top of `escrow/src/lib.rs`
so they are inlined at call sites with no measurable cost; they preserve ADR-002's
canonical sequence (read-only preconditions → `Address::require_auth()` → storage writes
and token transfers) and the exact `EscrowError` variant at every call site. The
predicates (`is_terminal_status`, `is_pre_settlement_status`) are deliberately
separable from the `ensure`-side guards so that view helpers and tests can reuse
the same status-set definition without hiding a panic.
