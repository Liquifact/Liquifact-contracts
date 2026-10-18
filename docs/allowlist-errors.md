# Allowlist Error Codes

> **Validation boundaries.** This document is the normative reference for the
> accepted, rejected, duplicate, and boundary-case inputs of the investor
> allowlist subsystem. The invariants below are enforced in
> `escrow/src/lib.rs` and exercised by
> `escrow/src/tests/allowlist_event_payloads.rs`. Any change to a bound or
> rejection condition is a breaking change and must update this document and
> its focused tests in the same PR.

The escrow contract's investor allowlist subsystem uses three typed Soroban error codes from
[`EscrowError`](../escrow/src/lib.rs). This document lists each code, the exact conditions that
trigger it, which entrypoints can emit it, and how integrators can avoid it.

All codes are **append-only and stable** — SDKs must branch on the numeric
`ContractError(code)`, not on panic-string text.

## Constants

| Constant | Value | Description |
| --- | ---: | --- |
| `MAX_INVESTOR_ALLOWLIST_BATCH` | 32 | Maximum addresses per `set_investors_allowlisted` call |

## Validation Boundaries

The following table defines the exact boundary for each allowlist input. Values
outside the accepted range are rejected deterministically with the listed error
code; no partial state is written.

| Input | Accepted | Rejected | Boundary case | Error |
| --- | --- | --- | --- | --- |
| `investors.len()` in `set_investors_allowlisted` | `1..=MAX_INVESTOR_ALLOWLIST_BATCH` (1–32) | `0` and `> 32` | `len == 1` accepted; `len == 32` accepted; `len == 33` rejected | `InvestorBatchEmpty` (70) for `0`; `InvestorBatchTooLarge` (71) for `> 32` |
| Duplicate addresses within one batch | First occurrence applied, subsequent occurrences are idempotent no-ops | — (duplicates are not an error) | A batch of all-identical addresses is accepted and results in a single effective write | None |
| `allowed` flag | `true` or `false` | — | Re-setting an existing entry to the same value is a no-op that still emits an event | None |
| `investor` address in `set_investor_allowlisted` | Any valid `Address` | — | Re-adding an already-allowlisted address is idempotent | None |
| `investor` in `fund` / `fund_with_commitment` / `fund_batch` | Allowlisted when gate is active | Not allowlisted (no entry or entry `false`) when gate is active | Gate inactive ⇒ check is skipped entirely | `InvestorNotAllowlisted` (104) |
| `get_allowlisted_investors(start, limit)` | `limit` capped at 50 | `limit > 50` is clamped, not an error | `limit == 0` returns an empty page | None |

### Invariants

1. **Atomicity.** A rejected batch writes no allowlist entries; a rejected
   funding call transfers no tokens and mutates no balances.
2. **Idempotence.** Applying the same `set_investor_allowlisted(investor, allowed)`
   call twice yields the same stored state as applying it once. Events are still
   emitted on the second call so off-chain indexers observe the intent.
3. **Determinism.** Given identical inputs and prior state, the accept/reject
   decision and the resulting state are identical across runs and nodes.
4. **Gate monotonicity.** `set_allowlist_active` never mutates the allowlist
   itself; toggling the gate only changes whether the gate is consulted.
5. **No silent expiry.** `InvestorAllowlisted` entries are persistent storage;
   `bump_ttl` must be called by custodians to prevent archival. An archived
   entry reads as absent and therefore as not-allowlisted.

### Failure-mode handling

- **Partial failure.** `set_investors_allowlisted` validates the whole vector
  before writing any entry, so a batch that fails validation leaves prior state
  untouched. `fund_batch` validates all entries up front and is atomic: one
  non-allowlisted address fails the entire call.
- **Retries.** All admin entrypoints are idempotent, so a retried call after a
  timeout cannot double-apply or corrupt state. Funding calls are not
  idempotent by design; callers must not retry a funding call whose result is
  unknown without first reading on-chain state.
- **Concurrent execution.** Soroban executes a contract invocation
  single-threaded per ledger, so two concurrent invocations are serialized by
  the ledger. The last writer wins for `set_investor_allowlisted`; no
  interleaving can produce a state that violates the invariants above.

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

## Observability

Failures are diagnosable without exposing sensitive data:

- Rejections surface as typed `ContractError(code)` values (70, 71, 104) that
  SDKs branch on numerically; no panic strings are part of the stable surface.
- Allowlist mutations emit events carrying the investor address and the new
  `allowed` flag, so indexers can reconstruct the allowlist from the event log
  alone. Event payload shapes are covered by
  `escrow/src/tests/allowlist_event_payloads.rs`.
- No error path logs or returns amounts, balances, or other investor-private
  data beyond the address already present in the triggering call.

## Test Coverage

Focused tests for accepted input, rejected input, duplicate submissions, and
boundary values live in `escrow/src/tests/allowlist_event_payloads.rs`. Any
change to a bound in the table above must add or update a test in that module.

## Stability Policy

Error codes 70, 71, and 104 are append-only and will never be renumbered or reassigned. New
allowlist-related failures will receive new codes at the end of the admin-validation range (70+)
or funding range (100+). See [`docs/escrow-error-messages.md`](escrow-error-messages.md) for the
full code table and range-group convention.
