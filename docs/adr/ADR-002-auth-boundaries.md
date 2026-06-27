# ADR-002: Authorization Boundaries

**Status:** Accepted  
**Date:** 2026-03-28  
**Refs:** `escrow/src/lib.rs` — `init`, `fund`, `settle`, `withdraw`, `claim_investor_payout`, `sweep_terminal_dust`, `set_legal_hold`, `propose_admin`, `accept_admin`

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
| `sweep_terminal_dust` | `treasury` (immutable after init) |
| `set_legal_hold`, `clear_legal_hold` | `admin` |
| `update_funding_target`, `update_maturity`, `migrate` | `admin` |
| `propose_admin` | current `admin` |
| `accept_admin` | pending admin stored in `DataKey::PendingAdmin` |
| `record_sme_collateral_commitment` | `sme_address` |

`admin` and `treasury` are stored at `init`; `treasury` is immutable, while `admin` rotates only through a two-step handover. The current admin calls `propose_admin(new_admin)`, which stores `DataKey::PendingAdmin` without changing authority. The pending address must then call `accept_admin()`, which requires its own authorization, promotes it into `InvoiceEscrow::admin`, and clears `DataKey::PendingAdmin`. The deprecated `transfer_admin` shim must not be treated as an immediate transfer; it only creates the pending proposal.

There is no superuser that can act as all roles simultaneously unless the same key is used for multiple roles — which is a deployment concern, not a contract concern.

## Consequences

- A compromised investor key cannot settle or sweep funds.
- A compromised SME key cannot change the admin or sweep dust.
- Treasury auth on `sweep_terminal_dust` means the admin cannot drain the contract as "dust" unless it is also the treasury.
- Legal hold can only be set/cleared by admin, so governance controls compliance freezes.
- Admin authority changes only after both the current admin and successor have authorized. A typo in `new_admin` can be corrected by a new `propose_admin` call and does not lock admin-gated paths.
- `DataKey::PendingAdmin` is cleared after successful acceptance so stale proposals cannot be reused.

## Rejected alternatives

- **Admin can do everything:** creates a single point of failure; role separation limits blast radius.
- **No treasury auth on sweep:** would let anyone trigger dust transfers once terminal; treasury auth is a cheap extra gate.

## No-Op Guards

Certain administrative entrypoints enforce a no-op guard to reject updates that do not change the existing state, preventing noisy events and wasted storage writes:
- **`update_maturity`**: Rejects `new_maturity == old_maturity` (`MaturityUnchanged`).
- **`propose_admin`**: Rejects `new_admin == current_admin` (`NewAdminSameAsCurrent`).
- **`rotate_beneficiary`**: Rejects `new_sme == current_sme` (`NewSmeSameAsCurrent`).
