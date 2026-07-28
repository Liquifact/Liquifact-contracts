# Allowlist Authorization and Access Rules

This document describes the authorization rules governing the investor allowlist subsystem in the LiquiFact escrow contract. It is the authoritative reference for **who may call what**, in which contract states, the **exact guard ordering**, and the **operator-facing rejection codes**.

> **Scope:** allowlist toggle, per-investor membership, pagination reads, and the allowlist gate on funding entrypoints. For the data model and invariants, see [`allowlist.md`](allowlist.md). For error codes, see [`allowlist-errors.md`](allowlist-errors.md).

Source of truth: `escrow/src/lib.rs` — `set_allowlist_active`, `set_investor_allowlisted`, `set_investors_allowlisted`, `is_allowlist_active`, `is_investor_allowlisted`, `get_allowlisted_investors`, `get_allowlisted_investors_count`, `bump_ttl`, and `fund_impl`.

---

## Roles

| Role | Stored As | Allowlist Authority |
|------|-----------|---------------------|
| **Admin** | `InvoiceEscrow::admin` | May enable/disable the gate, add/remove investors, and batch-mutate the allowlist. |
| **Investor** | Caller address on `fund` / `fund_with_commitment` / `fund_batch` | May fund only when the gate is off, or when the gate is on and the investor has an active allowlist entry. |
| **Anyone** | — | May read allowlist state and call `bump_ttl` (permissionless). |

---

## Authorization Mechanisms

All authorization in the contract uses **Soroban `Address::require_auth()`**, which verifies that the caller's signature (or the contract's internal auth) is present in the transaction envelope.

| Mechanism | Description | Used By |
|-----------|-------------|---------|
| **Admin auth** | `load_escrow_require_admin` loads `DataKey::Escrow` and calls `escrow.admin.require_auth()`. | `set_allowlist_active`, `set_investor_allowlisted`, `set_investors_allowlisted` |
| **Investor auth** | `investor.require_auth()` inside `fund_impl`. | `fund`, `fund_with_commitment`, `fund_batch` (per-entry) |
| **None (read-only)** | No `require_auth`; no storage write. | `is_allowlist_active`, `is_investor_allowlisted`, `get_allowlisted_investors`, `get_allowlisted_investors_count` |
| **None (permissionless)** | No `require_auth`; extends TTL only. | `bump_ttl` |

> **Key fact:** failed `require_auth` surfaces as a **host authorization trap** (Soroban runtime panic), not a typed `EscrowError`. Admin-gated allowlist writes that fail auth never reach storage or emit events.

---

## Guard Ordering in `fund_impl`

The allowlist gate is one guard in a fixed sequence inside `fund_impl`. Every guard is evaluated in order; the first failure short-circuits the call.

| Step | Guard | Error on Failure |
|------|-------|------------------|
| 1 | `investor.require_auth()` | Host auth trap |
| 2 | `amount > 0` | `FundingAmountNotPositive` (100) |
| 3 | `amount >= min_contribution_floor` (if configured) | `FundingBelowMinContribution` (101) |
| 4 | `!paused_active(env)` | `PausedBlocksFunding` (210) |
| 5 | `!legal_hold_active(env)` | `LegalHoldBlocksFunding` (102) |
| 6 | `escrow.status == 0` (open) | `EscrowNotOpenForFunding` (103) |
| 7 | Funding deadline not passed (if configured) | `FundingDeadlinePassed` (164) |
| 8 | **Allowlist gate** — see below | `InvestorNotAllowlisted` (104) |

### Allowlist gate (step 8)

```rust
if Self::is_allowlist_active(env.clone()) {
    ensure(
        &env,
        Self::is_investor_allowlisted(env.clone(), investor.clone()),
        EscrowError::InvestorNotAllowlisted,
    );
}
```

- The gate is **only checked when `AllowlistActive == true`**. When the gate is off, the allowlist is completely bypassed — any address may fund regardless of whether it has an entry.
- `is_investor_allowlisted` uses `unwrap_or(false)` semantics: an absent persistent entry is treated identically to an explicit `false` value. Both result in rejection.
- The gate is evaluated **after** the status gate and deadline check, so a closed or past-deadline escrow is rejected before the allowlist is consulted.

---

## Access Control Matrix

### Admin entrypoints (mutating allowlist state)

| Entrypoint | Auth | Storage Mutated | Events Emitted | Errors |
|---|---|---|---|---|
| `set_allowlist_active(active)` | Admin (`load_escrow_require_admin`) | `AllowlistActive` (instance) | `AllowlistEnabledChanged` (`al_ena`) | Host auth trap if non-admin |
| `set_investor_allowlisted(investor, allowed)` | Admin (`load_escrow_require_admin`) | `InvestorAllowlisted(investor)` (persistent), `AllowlistIndex` (instance) | `InvestorAllowlistChanged` (`al_set`) | Host auth trap if non-admin |
| `set_investors_allowlisted(investors, allowed)` | Admin (`load_escrow_require_admin`, once) | `InvestorAllowlisted(addr)` per address (persistent), `AllowlistIndex` (instance) | `InvestorAllowlistChanged` per address (`al_set`), then `InvestorAllowlistBatchApplied` (`al_batch`) | Host auth trap if non-admin; `InvestorBatchEmpty` (70) if empty; `InvestorBatchTooLarge` (71) if > 32 |

### Funding entrypoints (allowlist gate applies)

| Entrypoint | Auth | Allowlist Gate | Errors |
|---|---|---|---|
| `fund(investor, amount)` | Investor (`investor.require_auth()`) | Checked when `AllowlistActive == true` | Host auth trap; `InvestorNotAllowlisted` (104); all other `fund_impl` guards |
| `fund_with_commitment(investor, amount, committed_lock_secs)` | Investor (`investor.require_auth()`) | Checked when `AllowlistActive == true` | Same as `fund` plus `TieredSecondDeposit` (108) on repeat deposit |
| `fund_batch(entries)` | Investor per entry (`investor.require_auth()` per entry inside `fund_impl`) | Checked per entry when `AllowlistActive == true` | Same as `fund` per entry; `FundingBatchEmpty` (82); `FundingBatchTooLarge` (83); `FundingBatchDuplicateInvestor` (84) |

### Read-only entrypoints (no auth)

| Entrypoint | Auth | Reads | Returns |
|---|---|---|---|
| `is_allowlist_active()` | None | `AllowlistActive` (instance), `unwrap_or(false)` | `bool` — current gate state |
| `is_investor_allowlisted(investor)` | None | `InvestorAllowlisted(investor)` (persistent), `unwrap_or(false)` | `bool` — `true` only if entry exists and is `true` |
| `get_allowlisted_investors(start, limit)` | None | `AllowlistIndex` (instance), then per-address re-check of `InvestorAllowlisted(addr)` (persistent) — revoked entries filtered out (INV-AL-05) | `Vec<Address>` — live allowlisted addresses for the requested page |
| `get_allowlisted_investors_count()` | None | Iterates `AllowlistIndex`, counts live entries | `u32` — number of live allowlisted addresses |
| `get_allowlist_page(start, limit)` | None | `AllowlistIndex`, `InvestorAllowlisted(addr)`, `Escrow` for `base_yield_bps`, optional `YieldTierTable` | `Vec<AllowlistEntry>` — `{ investor, tier }` pairs (not yet implemented; see note below) |

### Permissionless TTL management (no auth)

| Entrypoint | Auth | Storage Mutated | Errors |
|---|---|---|---|
| `bump_ttl(allowlisted)` | None (permissionless) | Instance storage TTL; per-address persistent TTLs on `InvestorAllowlisted`, `InvestorContribution`, `InvestorEffectiveYield`, `InvestorClaimNotBefore`, `InvestorClaimed` | `BumpTtlBatchEmpty`; `BumpTtlBatchTooLarge` |

> **Note:** `get_allowlist_page` is referenced in [`allowlist.md`](allowlist.md) but is not yet implemented in `escrow/src/lib.rs`. Once added, it will carry no auth (INV-AL-07) and return `Vec<AllowlistEntry>` where `AllowlistEntry = { investor: Address, tier: u32 }`.

---

## Transition Rules

### Gate toggle (`set_allowlist_active`)

| Transition | Effect on Funding | Effect on Allowlist Entries |
|---|---|---|
| `false → true` | Gate activates immediately; all future funding calls are gated on per-investor flags. | Existing entries are unchanged; no auto-population. |
| `true → false` | Gate deactivates immediately; all future funding calls bypass the allowlist check. | Existing entries are unchanged; they persist and can be re-activated by re-enabling the gate. |

The toggle does **not** erase per-address entries and adding entries does **not** turn the gate on. The two pieces