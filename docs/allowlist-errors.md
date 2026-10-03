# Allowlist Error Codes

The escrow contract's investor allowlist subsystem uses three typed Soroban error codes from
[`EscrowError`](../escrow/src/lib.rs). This document lists each code, the exact conditions that
trigger it, which entrypoints can emit it, and how integrators can avoid it.

All codes are **append-only and stable** — SDKs must branch on the numeric
`ContractError(code)`, not on panic-string text.

## Constants

| Constant | Value | Description |
| --- | ---: | --- |
| `MAX_INVESTOR_ALLOWLIST_BATCH` | 32 | Maximum addresses per `set_investors_allowlisted` call |

## Error Reference

| Code | Variant | Entrypoint(s) | When it fires | How to avoid it |
| ---: | --- | --- | --- | --- |
| 70 | `InvestorBatchEmpty` | `set_investors_allowlisted` | The `investors` vector has length 0 — no addresses were passed. | Always pass at least one address. If your address list may be empty, guard the call client-side before invoking the entrypoint. |
| 71 | `InvestorBatchTooLarge` | `set_investors_allowlisted` | The `investors` vector exceeds `MAX_INVESTOR_ALLOWLIST_BATCH` (32). | Split large lists into chunks of 32 or fewer and make multiple calls. Each call is independent and atomically committed. |
| 104 | `InvestorNotAllowlisted` | `fund`, `fund_with_commitment`, `fund_batch` | The allowlist gate is active (`set_allowlist_active(true)` was called) **and** the investor address either has no `DataKey::InvestorAllowlisted` entry or that entry is `false`. | Add the investor to the allowlist via `set_investor_allowlisted` or `set_investors_allowlisted` **before** they attempt to fund. If the investor was previously allowlisted and later revoked, re-add them before their next deposit. |

## Entrypoint Cross-Reference

### Admin entrypoints (no error — always succeed when called by the escrow admin)

| Entrypoint | Purpose | Errors |
| --- | --- | --- |
| `set_allowlist_active(active: bool)` | Enable or disable the allowlist gate. When enabled, only allowlisted addresses may fund. | None — always succeeds under admin auth. |
| `set_investor_allowlisted(investor, allowed)` | Add or remove a single investor from the allowlist. Idempotent: re-adding an already-allowlisted address is a no-op that still emits an event. | None — always succeeds under admin auth. |

### Admin entrypoints (may emit batch-bound errors)

| Entrypoint | Purpose | Errors |
| --- | --- | --- |
| `set_investors_allowlisted(investors, allowed)` | Batch add or remove investors. Semantically identical to calling `set_investor_allowlisted` individually for each address, but requires admin auth once. | `InvestorBatchEmpty` (70) if the vector is empty; `InvestorBatchTooLarge` (71) if the vector exceeds 32 elements. |

### Read-only entrypoints (no error — pure queries)

| Entrypoint | Purpose |
| --- | --- |
| `is_allowlist_active()` | Returns `true` if the allowlist gate is enabled. Defaults to `false` when never configured. |
| `is_investor_allowlisted(investor)` | Returns `true` if the investor has an explicit allowlist entry set to `true`. Defaults to `false` when no entry exists. |
| `get_allowlisted_investors(start, limit)` | Returns a paginated list of currently-allowlisted addresses. Filters by live status so revoked addresses never appear. Page size capped at 50. |
| `get_allowlisted_investors_count()` | Returns the total number of currently-allowlisted addresses. |

### Funding entrypoints (may emit `InvestorNotAllowlisted`)

| Entrypoint | Behaviour |
| --- | --- |
| `fund(investor, amount)` | Single-investor deposit. Calls the internal `fund_impl` gate, which checks the allowlist when active. Emits `InvestorNotAllowlisted` (104) if the address is not on the allowlist. |
| `fund_with_commitment(investor, amount, lock_secs)` | Single-investor deposit with a commitment lock. Same allowlist check as `fund`. |
| `fund_batch(entries)` | Multi-investor batch deposit. Validates all entries up front (positivity, min-contribution floor, duplicate addresses), then calls `fund_impl` per entry. Each entry individually hits the allowlist gate — a single non-allowlisted address in the batch fails the entire call atomically. |

### TTL management (no error — storage hygiene)

| Entrypoint | Purpose |
| --- | --- |
| `bump_ttl(allowlisted)` | Extends the persistent-storage TTL for the provided allowlisted addresses. Prevents silent expiry of `InvestorAllowlisted` entries. Called off-chain by custodians. No errors. |

## Lifecycle Example

```
1. Admin calls set_allowlist_active(true)          — gate is now on
2. Admin calls set_investor_allowlisted(Alice, true) — Alice is now on the list
3. Alice calls fund(alice, 1000)                     — succeeds
4. Admin calls set_investor_allowlisted(Alice, false) — Alice is revoked
5. Alice calls fund(alice, 500)                      — reverts with InvestorNotAllowlisted (104)
6. Admin calls set_investors_allowlisted([Alice, Bob], true) — both re-added
7. Alice calls fund(alice, 500)                      — succeeds again
```

## Stability Policy

Error codes 70, 71, and 104 are append-only and will never be renumbered or reassigned. New
allowlist-related failures will receive new codes at the end of the admin-validation range (70+)
or funding range (100+). See [`docs/escrow-error-messages.md`](escrow-error-messages.md) for the
full code table and range-group convention.
