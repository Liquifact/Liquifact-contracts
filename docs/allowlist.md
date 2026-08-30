# Allowlist Model and Invariants

> **Audience.** Auditors, integrators, and core contributors who need a
> spec-style description of the investor allowlist in the LiquiFact escrow
> contract (`escrow/src/lib.rs`). This document states the **data model**,
> **invariants**, **entrypoint contracts**, and a single **worked example**
> end-to-end. For day-to-day operations, see the companion guide
> [`escrow-allowlist.md`](escrow-allowlist.md).

## Overview

The allowlist is a **toggleable KYC/AML gate** that restricts which
addresses may contribute principal to an escrow. It is composed of two
independent pieces of state:

1. A **single boolean toggle** stored in instance storage that controls
   whether the gate is currently enforced for `fund` and
   `fund_with_commitment`.
2. An ordered **index of investor addresses** in instance storage plus a
   per-address boolean flag in persistent storage that records allowlist
   membership.

The two pieces are intentionally independent: toggling the gate does not
erase per-address entries, and adding addresses to the allowlist does not
turn the gate on. The canonical fund-path guard accepts absent per-address
entries as **deny** when the gate is on (`unwrap_or(false)` semantics),
so archived or never-added entries cannot be exploited to bypass the gate.

## Data Model

### Storage keys

| Key | Storage | Type | Multiplicity | Default |
| --- | --- | --- | --- | --- |
| `DataKey::AllowlistActive` | instance | `bool` | singleton | `false` (absent) |
| `DataKey::AllowlistIndex` | instance | `Vec<Address>` | singleton | `[]` (absent) |
| `DataKey::InvestorAllowlisted(Address)` | persistent | `bool` | one per investor | `false` (absent) |

`AllowlistActive` and `AllowlistIndex` live in **instance storage** so
they share the contract's TTL. Reads on the funding path are cheap.

Per-address flags live in **persistent storage** with independent TTLs per
address. This is intentional: it lets the allowlist scale to thousands of
addresses without bloating the instance footprint (see
[`ADR-007`](adr/ADR-007-storage-key-evolution.md)).

### Index invariants (`AllowlistIndex`)

The index is a `Vec<Address>` that:

- **Appends** an address on the first transition `false → true` of its
  `InvestorAllowlisted` flag.
- **Sweeps in place** (linear swap-remove) on the first transition
  `true → false`, by position match.
- **Never shrinks** on the read-side: the index grows monotonically by
  appends and only shrinks by explicit revoke sweeps. Revoked
  addresses therefore remain in the index and must be **filtered at
  read time** (INV-AL-05).

### TTL semantics

> **Key fact:** none of the admin allowlist writes (`set_allowlist_active`,
> `set_investor_allowlisted`, `set_investors_allowlisted`) extend the
> persistent TTL on `InvestorAllowlisted(addr)`. The **only** entrypoint
> that extends the per-investor persistent TTLs is the permissionless
> `bump_ttl` (see INV-AL-08). Operators that need to keep allowlist
> credentials alive across long maturities must schedule periodic
> `bump_ttl` calls (e.g. via a cron relayer).

| Operation | Effect on persistent TTL |
| --- | --- |
| `set_allowlist_active(active)` | none |
| `set_investor_allowlisted(addr, allowed)` | none (TTL is **not** auto-extended on write) |
| `set_investors_allowlisted(addrs, allowed)` | none (per-address, same constraint) |
| `is_investor_allowlisted(addr)` / funding-path read | none (read-only) |
| `bump_ttl(allowlisted)` | extends `InvestorAllowlisted(addr)` _and_ `InvestorContribution` / `InvestorEffectiveYield` / `InvestorClaimNotBefore` / `InvestorClaimed` for each supplied address; also extends instance storage TTL |

[`PERSISTENT_TTL_MIN_EXTENSION_LEDGERS`] is the constant the contract uses
when extending TTL (≈1h at 1 ledger/s); `bump_ttl` always extends by this
horizon. The contract **never shortens** TTL — `extend_ttl` semantics.

> Operations **lower_bound**: `set_investors_allowlisted` is bounded by
> [`MAX_INVESTOR_ALLOWLIST_BATCH`]`=32`; `bump_ttl` is bounded by
> [`MAX_BUMP_TTL_BATCH`]`=32`; pagination reads are bounded by
> [`MAX_INVESTOR_READ_BATCH`]`=50`.

## Invariants

These are the rules the protocol guarantees for the lifetime of an
initialized escrow instance. Use them as the contract against which any
proposed change to the allowlist must be checked.

### INV-AL-01 — Deny-by-default when the gate is on

When `AllowlistActive == true`, every call to `fund`, `fund_with_commitment`,
or `fund_batch` whose investor address is not in the allowlist is rejected
with `EscrowError::InvestorNotAllowlisted` (`104`). Both an absent
`InvestorAllowlisted(addr)` and an explicit `false` value are treated
identically. The gate check lives in the shared
`guard_investor_allowlisted` helper, which is the **only** entry point
that translates allowlist status into a typed error.

### INV-AL-02 — Gate-off is a total bypass

When `AllowlistActive == false` (including when the key has been archived
and the read returns `unwrap_or(false)`), the values of every
`InvestorAllowlisted(addr)` entry are **strictly ignored** by every
funding-path entrypoint. No entrypoint may consult `is_investor_allowlisted`
without first consulting `is_allowlist_active` (the canonical two-step
read pattern).

### INV-AL-03 — Batch allowance is atomic and double-emits

`set_investors_allowlisted` is **atomic**: it either commits every
address in the batch or reverts with a typed error. On success it emits:

1. One `InvestorAllowlistChanged` event per address (`name == "al_set"`),
   in input order, each carrying the same `allowed` flag.
2. Exactly one terminating `InvestorAllowlistBatchApplied` event
   (`name == "al_batch"`) whose `batch_size` equals the input length and
   whose `allowed` flag matches the per-address events.

An empty vector is rejected with `EscrowError::InvestorBatchEmpty` (`70`).
A vector longer than `MAX_INVESTOR_ALLOWLIST_BATCH` is rejected with
`EscrowError::InvestorBatchTooLarge` (`71`).

### INV-AL-04 — Admin writes do not auto-extend TTL

A successful admin write (`set_allowlist_active`,
`set_investor_allowlisted`, `set_investors_allowlisted`) **does not**
extend the persistent TTL of `InvestorAllowlisted(addr)`. Persistent
allowlist entries are therefore subject to normal TTL/archival eviction
like any other persistent key. Operators and integrators that rely on
long-lived allowlist membership across long maturities must maintain
TTL via the permissionless `bump_ttl` entrypoint (INV-AL-08) or by
periodic re-write through admin operations.

### INV-AL-04a — Revoke sweeps the index in place, preserving order

Calling `set_investor_allowlisted(addr, false)` on an address that was
previously allowlisted (`InvestorAllowlisted(addr) == true`) **sweeps
`AllowlistIndex` by position match**: a linear scan of the index
locates the matching address and removes it in place. Because Soroban's
`Vec<Address>::remove` is implemented as a swap-remove, the **last
element slides into the freed slot** while the relative order of the
remaining entries is otherwise preserved. Concretely, an index
`[A, B, C]` after `set_investor_allowlisted(A, false)` becomes `[C, B]`
(not `[B, C]`); an index `[A, B]` becomes `[B]`; an index `[A]`
becomes `[]`. Concurrent invariants:

- Subsequent reads on the post-revoke index (pagination, count) skip
  the revoked address per INV-AL-05 and return a `Vec` whose **live**
  subset, taken in index order, may have shifted relative to the
  pre-revoke ordering at and after the removed position.
- The persisted key `InvestorAllowlisted(addr) = false` remains in
  storage until the next write or archival; reads treat it as `false`,
  equivalent to absent.

### INV-AL-05 — Reads filter revoked entries

Pagination entrypoints (`get_allowlisted_investors`,
`get_allowlist_page`, `get_allowlisted_investors_count`) **never return a
revoked address**. They iterate the `AllowlistIndex` and, per address,
re-read `InvestorAllowlisted(addr)`. Revoked entries are skipped, so the
`Vec` returned may be shorter than the index length, and the
`get_allowlisted_investors_count` value may be less than
`AllowlistIndex.len()`.

### INV-AL-06 — Idempotent re-allowlist extends TTL without re-indexing

Calling `set_investor_allowlisted(addr, true)` when the address is
already allowlisted is a **no-op for the index** (the address is not
re-appended), but it still emits an `InvestorAllowlistChanged` event and
**extends the persistent TTL** by the standard horizon. This makes
periodic "refresh" writes safe and idempotent.

### INV-AL-07 — Read-only views carry no auth

`is_allowlist_active`, `is_investor_allowlisted`,
`get_allowlisted_investors`, `get_allowlisted_investors_count`, and
`get_allowlist_page` perform no `require_auth` and write nothing. They
are usable from any context.

### INV-AL-08 — `bump_ttl` is permissionless and bounded

`bump_ttl` requires no auth. It extends the persistent TTL of
`InvestorAllowlisted(addr)` and the other per-investor keys for each
address in its input, plus the contract instance TTL. The batch is
bounded by `MAX_BUMP_TTL_BATCH = 32` and rejected as empty with
`EscrowError::BumpTtlBatchEmpty`; over-size is rejected with
`EscrowError::BumpTtlBatchTooLarge`. It never shortens TTL.

### INV-AL-09 — Authorisation is admin-only on writes

Every entrypoint that mutates allowlist state — `set_allowlist_active`,
`set_investor_allowlisted`, and `set_investors_allowlisted` —
**requires admin auth** via the shared `load_escrow_require_admin`
helper, which loads `DataKey::Escrow` and invokes
`escrow.admin.require_auth()`. Non-admin callers fail the auth check
**before** any storage write or event emission; the failure surfaces as
a Soroban contract-call authorisation error (host panic), not a typed
`EscrowError`.

### INV-AL-10 — Page-bound reads never panic

`get_allowlist_page` returns an empty `Vec` (no panic) when:

- `start >= AllowlistIndex.len()`,
- `limit == 0`,
- the resulting page would otherwise exceed `MAX_INVESTOR_READ_BATCH`
  (`limit` is silently clamped down).

`get_allowlisted_investors` defers to the shared `paginate` helper and
returns `[]` for empty / out-of-range / over-size requests.

## Entrypoints

### Admin: `set_allowlist_active(env, active)`

- **Auth:** `load_escrow_require_admin(env)` — admin only (INV-AL-09).
- **Storage mutated:** `DataKey::AllowlistActive` (instance, singleton).
- **Events emitted:**
  `AllowlistEnabledChanged { name: "al_ena", invoice_id, active: 0|1 }`.
- **Errors:** none specific to the allowlist; auth failures raise
  `EscrowError::Unauthorized`.

### Admin: `set_investor_allowlisted(env, investor, allowed)`

- **Auth:** admin only (INV-AL-09).
- **Storage mutated:**
  - `DataKey::InvestorAllowlisted(investor)` (persistent, `bool`).
  - `DataKey::AllowlistIndex` (instance, `Vec<Address>`): append on
    `false → true`; in-place position-match removal on `true → false`
    (INV-AL-04a), preserving the order of remaining entries.
- **TTL:** none — the persistent TTL on `InvestorAllowlisted(investor)`
  is **not** extended by this call (INV-AL-04). Use `bump_ttl` to refresh.
- **Events emitted:**
  `InvestorAllowlistChanged { name: "al_set", invoice_id, investor, allowed: 0|1 }`.
- **Errors:** admin auth only.

### Admin: `set_investors_allowlisted(env, investors, allowed)`

- **Auth:** admin only (INV-AL-09); one admin auth check for the whole
  batch (no per-address recheck).
- **Storage mutated:** for each `investors[i]`:
  - `DataKey::InvestorAllowlisted(investors[i])` (persistent).
  - `DataKey::AllowlistIndex` (instance): batched appends and removals
    applied within the same call (INV-AL-04a).
- **TTL:** none — per-address persistent TTLs are **not** extended by
  this call (INV-AL-04). Use `bump_ttl` to refresh.
- **Events emitted:** one `InvestorAllowlistChanged { name: "al_set", … }`
  per address (in input order), then exactly one trailing
  `InvestorAllowlistBatchApplied { name: "al_batch", invoice_id, batch_size, allowed: 0|1 }`
  (INV-AL-03).
- **Errors:**
  - `EscrowError::InvestorBatchEmpty` (`70`) — `investors` is empty.
  - `EscrowError::InvestorBatchTooLarge` (`71`) — `investors.len() > MAX_INVESTOR_ALLOWLIST_BATCH`
    (32).

### Read: `is_allowlist_active(env) -> bool`

- **Auth:** none (INV-AL-07).
- **Reads:** `DataKey::AllowlistActive` (instance) with `.unwrap_or(false)`.
- **Returns:** current gate state.

### Read: `is_investor_allowlisted(env, investor) -> bool`

- **Auth:** none (INV-AL-07).
- **Reads:** `DataKey::InvestorAllowlisted(investor)` (persistent) with
  `.unwrap_or(false)` → **default-to-deny** when absent.
- **Returns:** `true` iff the entry is present and `true`.

### Read (paged): `get_allowlisted_investors(env, start, limit) -> Vec<Address>`

- **Auth:** none (INV-AL-07).
- **Reads:** `DataKey::AllowlistIndex` (instance), then a per-address
  re-check of `InvestorAllowlisted(addr)` (persistent) — revokes are
  filtered out (INV-AL-05).
- **Bounds:** `limit <= MAX_INVESTOR_READ_BATCH` (`50`) or clamped via the
  shared `paginate` helper; `start` past `len` ⇒ empty result.
- **Returns:** live allowlisted addresses for the requested page.

### Read (count): `get_allowlisted_investors_count(env) -> u32`

- **Auth:** none (INV-AL-07).
- **Reads:** iterates `AllowlistIndex` and counts live entries (INV-AL-05).
- **Returns:** number of **live** allowlisted addresses; may be `<
  AllowlistIndex.len()` if any index entries have been revoked.

### Read (paged + tier): `get_allowlist_page(env, start, limit) -> Vec<AllowlistEntry>`

- **Auth:** none (INV-AL-07).
- **Reads:** `AllowlistIndex` (instance), `InvestorAllowlisted(addr)`
  per address (persistent), `Escrow` (instance) for `base_yield_bps`,
  optional `YieldTierTable`.
- **Bounds:** `limit` clamped to `MAX_INVESTOR_READ_BATCH` (50);
  out-of-range `start` or zero `limit` ⇒ empty; never panics on edges
  (INV-AL-10).
- **Returns:** `Vec<AllowlistEntry>` where `AllowlistEntry = { investor: Address, tier: u32 }`.
  `tier == 0` means base yield or not yet funded; `tier >= 1` is the
  1-based yield tier index when `fund_with_commitment` was used on the
  first deposit leg.

### Permissionless: `bump_ttl(env, allowlisted)`

- **Auth:** none (INV-AL-08).
- **Storage mutated:** instance storage TTL; per-address persistent TTLs
  on `InvestorAllowlisted`, `InvestorContribution`,
  `InvestorEffectiveYield`, `InvestorClaimNotBefore`, and
  `InvestorClaimed`, for each supplied address. Each extension uses
  `PERSISTENT_TTL_MIN_EXTENSION_LEDGERS` (≈1h at 1 ledger/s) for the
  persistent key and `INSTANCE_TTL_MIN_EXTENSION_LEDGERS` for the
  instance key.
- **Events emitted:** none.
- **Errors:**
  - `EscrowError::BumpTtlBatchEmpty` — empty batch.
  - `EscrowError::BumpTtlBatchTooLarge` — `len() > MAX_BUMP_TTL_BATCH` (32).
  - Exact numeric codes for these variants are maintained in
    [`escrow-error-messages.md`](escrow-error-messages.md); precise
    codes are append-only and must not be relied on for branching.
- **Notes:** never shortens TTL (INV-AL-08). Useful for renewing
  allowlist credentials ahead of long maturities (INV-AL-04).

## Worked Example

A single end-to-end lifecycle that exercises invariants INV-AL-01
through INV-AL-08.

### Setup

- Three investor addresses: `A`, `B` (KYC-approved), `C` (not approved).
  (`A` will be revoked later; see Step 5.)
- Admin address: `ADMIN`.
- Status: escrow is `Open` for funding.

### Step 1 — Admin enables the gate

```
ADMIN → set_allowlist_active(true)
```

| Effect | Detail |
| --- | --- |
| Storage | `AllowlistActive = true` (instance) |
| Event | `AllowlistEnabledChanged { name: "al_ena", invoice_id, active: 1 }` |
| Invariant | **INV-AL-01 active.** Funding-path now gates on per-address flags. |

### Step 2 — Admin batch-allowlists A and B

```
ADMIN → set_investors_allowlisted([A, B], true)
```

| Effect | Detail |
| --- | --- |
| Storage | `InvestorAllowlisted(A)/= true`, `InvestorAllowlisted(B)/= true` (persistent); `AllowlistIndex = [A, B]` (instance). Per INV-AL-04, **no TTL auto-extension** occurred — TTL still sits at whatever it was before this call. |
| Events | Two `InvestorAllowlistChanged { name: "al_set", …, allowed: 1 }`, then one `InvestorAllowlistBatchApplied { name: "al_batch", batch_size: 2, allowed: 1 }`. |
| Invariant | **INV-AL-03** (double-emission shape); **INV-AL-04** (no auto-extension). |

### Step 3 — Investor A funds successfully

```
A → fund_with_commitment(invoice_id, amount, tier, lock_secs)
```

- `guard_investor_allowlisted(env, A)` reads `AllowlistActive == true` and
  `InvestorAllowlisted(A) == true` ⇒ no panic.
- Principal flows; `InvestorContribution(A)` is written.
- Invariant upheld: **INV-AL-01** (authorised execution).

### Step 4 — Investor C attempts to fund

```
C → fund(invoice_id, amount)
```

- `guard_investor_allowlisted(env, C)` reads `AllowlistActive == true` and
  `InvestorAllowlisted(C)` is **absent** ⇒ `unwrap_or(false)` ⇒ panic.
- Panics with `EscrowError::InvestorNotAllowlisted` (code `104`). No
  storage mutated, no event emitted.
- Invariant upheld: **INV-AL-01**.

### Step 5 — Admin revokes A

```
ADMIN → set_investor_allowlisted(A, false)
```

| Effect | Detail |
| --- | --- |
| Storage | `InvestorAllowlisted(A) = false`; `AllowlistIndex` is **swept in place**: A is located by position match and removed, leaving `[B]` (INV-AL-04a). The persisted key `InvestorAllowlisted(A) = false` remains in storage. |
| Event | `InvestorAllowlistChanged { name: "al_set", invoice_id: …, investor: A, allowed: 0 }`. |
| Invariant | **INV-AL-04a** (order preserved: B remains in the index in its original position relative to the new length). |

> Note: even though A's contribution is still recorded under
> `InvestorContribution(A)`, the allowlist gate is now active against A
> on the same `is_investor_allowlisted` check.

### Step 6 — Investor A attempts a second deposit

```
A → fund(invoice_id, amount)
```

- `guard_investor_allowlisted(env, A)` reads `InvestorAllowlisted(A) ==
  false` ⇒ panic with `EscrowError::InvestorNotAllowlisted` (104).
- **INV-AL-01** upheld across the revocation.

### Step 7 — Read-side confirmation (INV-AL-05, INV-AL-04a)

```
get_allowlisted_investors_count  →  1   (B only; A filtered out)
get_allowlisted_investors(0, 50)  →  [B]
get_allowlist_page(0, 50)         →  [{investor: B, tier: 0}]
```

(If the escrow has a yield tier table and B used tier selection on
their first fund leg, `tier` reflects their 1-based tier index.)

### Step 8 — Permissionless TTL bump

A third-party relayer calls:

```
ANY → bump_ttl([B])
```

- Storage: persistent TTLs on `InvestorAllowlisted(B)`,
  `InvestorContribution(B)`, `InvestorEffectiveYield(B)`,
  `InvestorClaimNotBefore(B)`, `InvestorClaimed(B)` each extended by
  `PERSISTENT_TTL_MIN_EXTENSION_LEDGERS`; instance storage TTL also
  extended by `INSTANCE_TTL_MIN_EXTENSION_LEDGERS`.
- No events. No auth. **INV-AL-08 upheld.** (This is also the only
  practical way to keep allowlist credentials alive across long
  maturities — see INV-AL-04.)

## Integration Notes for Clients

- **Read-only views are free.** Pagination entrypoints are usable from
  any context (INV-AL-07). For UI/reconciliation, prefer
  `get_allowlist_page` when tier context matters, otherwise
  `get_allowlisted_investors`.
- **Default-to-deny.** Never assume an absent allowlist entry is
  "neutral"; absent ⇒ `false` ⇒ blocked when the gate is on. This is
  enforced at the funding-path boundary (INV-AL-01).
- **Treat absent as zero, not as "not configured".** If your client UI
  reports a `false` for an address it hasn't been explicitly managing,
  it is correct: not allowlisted, regardless of whether the key exists.
- **Maintenance burden.** Persistent entries can be archived by the
  protocol's TTL rules; keep allowlisted entries alive via periodic
  `set_investor_allowlisted(addr, true)` refreshes (idempotent by
  INV-AL-06) or by `bump_ttl` (INV-AL-08).
- **Event auditing.** A complete change log for an investor over the
  escrow's lifetime is `[{ InvestorAllowlistChanged }, …]` followed by
  exactly one `InvestorAllowlistBatchApplied` per batch call. Indexers
  should treat the terminating batch event as the "end of batch" marker
  for that call.
- **TTL maintenance is the operator's job.** Admin writes do **not**
  refresh TTL (INV-AL-04). Schedule `bump_ttl` (or
  periodic re-`set_investor_allowlisted(addr, true)`) for every active
  allowlist member to avoid silent eviction, especially when the
  escrow's maturity is far in the future.
- **Pagination is stable up to swap-remove.** `AllowlistIndex` is
  modified in place on revoke (INV-AL-04a): revoked entries are
  removed by position match, with the final element sliding into the
  freed slot. Off-chain cursor pagination over this index should
  therefore handle the **last-element slide** at and after the
  removed position. Treat the index as "append-only with in-place
  swap-remove" — never assume the relative order of items **after**
  a revoke is identical to the pre-revoke ordering.
- **Bound your inputs.** Always pre-validate `investors.len()` against
  `MAX_INVESTOR_ALLOWLIST_BATCH` (32) before submitting batch writes,
  and against `MAX_BUMP_TTL_BATCH` (32) for `bump_ttl`. Read pages should
  be `limit <= MAX_INVESTOR_READ_BATCH` (50) to avoid silent clamping
  on the read side.

## Cross-references

- Operational guide: [`escrow-allowlist.md`](escrow-allowlist.md).
- Storage key evolution: [`ADR-007`](adr/ADR-007-storage-key-evolution.md).
- Gas & storage notes: [`escrow-gas-storage-notes.md`](escrow-gas-storage-notes.md).
- Pagination patterns & read API: [`escrow-read-api.md`](escrow-read-api.md).
- Error codes: [`escrow-error-messages.md`](escrow-error-messages.md) —
  errors used or referenced here are `InvestorNotAllowlisted (104)`,
  `InvestorBatchEmpty (70)`, `InvestorBatchTooLarge (71)`,
  `ContributionReadBatchTooLarge (203)`, plus the typed
  `BumpTtlBatchEmpty` / `BumpTtlBatchTooLarge` variants emitted by
  `bump_ttl`. Numeric codes for the `BumpTtlBatch*` variants are
  append-only and maintained in [`escrow-error-messages.md`]; do not
  branch on their absolute value.
- Auth boundaries: [`ADR-002`](adr/ADR-002-auth-boundaries.md) —
  admin-only writes; reads carry no auth.
- Event schema: [`EVENT_SCHEMA.md`](EVENT_SCHEMA.md) — the three
  allowlist topic shapes (`al_ena`, `al_set`, `al_batch`).
- Tests: `escrow/src/test_allowlist_tests.rs` for allowlist coverage and
  `escrow/src/tests/coverage_boost_tests.rs` for pagination.
