# ADR-002: Authorization Boundaries

**Status:** Accepted  
**Date:** 2026-03-28  
**Refs:** `escrow/src/lib.rs` — `init`, `fund`, `settle`, `withdraw`, `claim_investor_payout`, `sweep_terminal_dust`, `set_legal_hold`, `transfer_admin`

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
| `update_funding_target`, `update_maturity`, `transfer_admin`, `migrate` | `admin` |
| `record_sme_collateral_commitment` | `sme_address` |

`admin` and `treasury` are stored immutably at `init` (except `admin` which rotates via `transfer_admin`). There is no superuser that can act as all roles simultaneously unless the same key is used for multiple roles — which is a deployment concern, not a contract concern.

## Consequences

- A compromised investor key cannot settle or sweep funds.
- A compromised SME key cannot change the admin or sweep dust.
- Treasury auth on `sweep_terminal_dust` means the admin cannot drain the contract as "dust" unless it is also the treasury.
- Legal hold can only be set/cleared by admin, so governance controls compliance freezes.

## Rejected alternatives

- **Admin can do everything:** creates a single point of failure; role separation limits blast radius.
- **No treasury auth on sweep:** would let anyone trigger dust transfers once terminal; treasury auth is a cheap extra gate.
