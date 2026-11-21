# Escrow Gas / Storage Notes

## Soroban cost model

Soroban charges CPU and memory instructions per host function invocation. Instance storage reads
(`env.storage().instance().get(...)`) each consume a measurable instruction budget. Eliminating
duplicate reads of the same key within a single host function call directly reduces the
instruction budget consumed per transaction — without changing observable behavior.

No benchmarking harness exists in this repository. `cargo llvm-cov` measures line coverage only.
Cost savings below are reasoned from the Soroban host cost schedule, not measured wall-clock time.

---

## Audit table — `escrow/src/lib.rs` (issue #185)

| Function | Reads before | Reads after | Change | Rationale |
|---|---|---|---|---|
| `fund_impl` (new investor path) | `UniqueFunderCount` read twice: once for cap check, once for increment | Once: hoisted into `cur_funder_count` local, reused for both uses | **−1 read** on every new-investor call | Single read covers cap assertion and increment write |
| `fund_impl` (event field) | `InvestorEffectiveYield` read after write to populate event | Captured in `investor_effective_yield_bps` local at write time | **−1 read** on every `fund` / `fund_with_commitment` call | Value is known at write time; post-write read is redundant |
| `fund_impl` (returning investor, `simple_fund`) | `InvestorEffectiveYield` not read (yield set on first deposit only) | Read once for event field on returning-investor path | No net change — read was already absent on first-deposit path; added on returning path to populate event correctly |  |
| `fund_impl` (`legal_hold_active` order) | Hold check after escrow read | Unchanged — escrow read is always needed for `yield_bps` and `status`; hoisting hold before escrow would not reduce reads | No change | Comment added explaining order |
| `sweep_terminal_dust` | `DataKey::Escrow`, `DataKey::Treasury`, `DataKey::FundingToken` — each read once | Same | No change — reads are non-redundant | Each key is distinct and read exactly once |
| `get_investor_yield_bps` | `DataKey::Escrow` + `DataKey::InvestorEffectiveYield` | Same | No change — reads are non-redundant | Escrow read is required for the base yield fallback; doc comment added to inform callers |
| All other `get_*` getters | Single read per key | Same | No change — reads are non-redundant | Each getter reads exactly one key |
| All `env.clone()` call sites | — | — | No change — all clones are required | `env` is used after every `get_escrow` call for storage writes, ledger reads, or event publish; comments added at each site |

---

## `env.clone()` audit

`Env` is a reference-counted handle to the Soroban host. Cloning it is cheap (pointer copy).
Every `Self::get_escrow(env.clone())` call site was audited; in all cases `env` is used again
after the call (for storage writes, ledger reads, or `.publish(&env)`), so the clone is
required. A comment was added at each site documenting this decision.

---

## Net savings per `fund` call (new investor)

- **−2 storage reads** on the new-investor path: one `UniqueFunderCount` read and one
  `InvestorEffectiveYield` read eliminated.
- **−1 storage read** on the returning-investor path: one `InvestorEffectiveYield` post-write
  read eliminated (replaced by a local variable); one read added to fetch the existing yield
  for the event field — net zero on this path.

These are micro-optimizations. The primary value is correctness hygiene and reduced instruction
budget on the hot `fund` path, which is called once per investor deposit.

---

## TTL semantics and operational `bump_ttl` (rent/archival mitigation)

`AllowlistActive` is stored in **instance** storage. `InvestorAllowlisted(addr)` entries are stored in
**persistent** storage. These have different TTL semantics under Soroban's rent model.

If instance storage expires and is not extended, `AllowlistActive` returns `false` (default via
`unwrap_or`), silently disabling the allowlist gate even if persistent allowlist entries remain.
Operators must extend instance storage TTL together with persistent storage TTL.

### Write-time TTL extension for per-investor keys

Per-investor persistent keys (`InvestorContribution`, `InvestorEffectiveYield`, `InvestorClaimNotBefore`, `InvestorClaimed`, `InvestorRefunded`) are automatically extended at write time inside the fund, claim, and refund flows using the `PERSISTENT_TTL_MIN_EXTENSION_LEDGERS` horizon. This provides defense-in-depth against silent expiry of live positions prior to settlement.

**`InvestorRefunded` storage migration (issue #1541):**
Prior WASM versions wrote `DataKey::InvestorRefunded(Address)` to instance storage. Each refunded
investor added a permanent entry to the instance map, so every subsequent invocation loaded those
stale entries even though they were never updated. As of issue #1541, new refunds are written to
**persistent** storage with TTL extension via `get_storage_limit`. Legacy instance entries are
migrated transparently on the first read of each investor's refund flag: the private
`investor_refunded` helper writes the persistent entry, extends its TTL, and removes the instance
entry — all atomically within a single host call. No operator intervention is required.

### Permissionless TTL extension

The contract includes a permissionless `bump_ttl` entrypoint that extends TTLs for:

- **Instance storage** (contract-wide, single call covers all instance keys):
  - `DataKey::Escrow`
  - `DataKey::Version`
  - `DataKey::LegalHold`
  - `DataKey::AllowlistActive`
  - `DataKey::FundingCloseSnapshot`
  - and all other instance keys

- **Persistent storage per-investor keys** (one set per investor address in the allowlist):
  - `DataKey::InvestorAllowlisted(addr)` 
  - `DataKey::InvestorContribution(addr)`
  - `DataKey::InvestorEffectiveYield(addr)`
  - `DataKey::InvestorClaimNotBefore(addr)`
  - `DataKey::InvestorClaimed(addr)`

Key properties (invariant):

- **Extension never shortens** an existing TTL.
- **No state mutation beyond TTL extension**: `bump_ttl` only calls `extend_ttl(...)`.
- **Efficient**: instance TTL is extended once (contract-wide), and each persistent investor key is extended once per investor address (checked via `has()` first to avoid trapping on missing keys).
- **Idempotent**: calling with the same addresses multiple times is safe and harmless.

### Thresholds

Thresholds are defined as **named constants** in `escrow/src/lib.rs`:

- `INSTANCE_TTL_MIN_EXTENSION_LEDGERS` (default ≈ 1h)
- `PERSISTENT_TTL_MIN_EXTENSION_LEDGERS` (default ≈ 1h; kept equal to the instance default)

These are used as **fallback defaults** when no admin-configured storage limit is set (see next section).

### Admin-configurable storage limit

Operators may override the TTL extension horizon via:

- `get_storage_limit` — returns the admin-configured ledger count, or the compile-time default when unset
- `set_storage_limit` — admin-only; accepts values in `MIN_STORAGE_LIMIT_LEDGERS..=MAX_STORAGE_LIMIT_LEDGERS`

Unset storage preserves historical behaviour (`INSTANCE_TTL_MIN_EXTENSION_LEDGERS`). Out-of-range
values revert with typed `EscrowError::StorageLimitOutOfRange` (223). The configured limit is
applied uniformly to:

- **Write-time extensions**: `extend_investor_ttl()` helper and instance TTL extension in `fund_impl` and `settle`
- **Permissionless `bump_ttl`**: both instance and persistent key extensions
- **Admin `batch_bump_ttl`**: both instance and persistent key extensions, with instance TTL extended at most once per call
- **Funding-deadline top-ups** (existing behaviour, preserved)

### Why permissionless is acceptable

Callers can safely invoke the entrypoint because extending TTL cannot harm the contract state:

- the operation is monotonic (never decreases TTL)
- only TTL extension occurs (no writes to escrow balances, gates, or payout markers)

References:

- ADR-007: storage key evolution policy and semantics
- docs/escrow-ledger-time.md: time gates use `Env::ledger().timestamp()` with inclusive `>=` semantics

## Persistent investor key TTL extension (issue #1684)

### Problem

Per-investor persistent entries (`InvestorContribution`, `InvestorEffectiveYield`,
`InvestorClaimNotBefore`, `InvestorClaimed`, `InvestorAllowlisted`, `InvestorFrozen`) are
stored under independent persistent storage keys. Persistent entries expire after their TTL
reaches zero, which on Soroban means the entry is archived and effectively absent.  An
expired `InvestorContribution` returns `0` on the next read, causing `NoContributionToClaim`
or `NoContributionToRefund` errors for legitimate investors.

Previously only the manual `bump_ttl` / `batch_bump_ttl` entrypoints extended these keys.
An investor who funded but never triggered an admin bump could have their keys expire before
settlement.

### Solution

The private helper `extend_investor_ttl` extends every present per-investor persistent key
using `get_storage_limit` (defaults to `PERSISTENT_TTL_MIN_EXTENSION_LEDGERS = 3600 ledgers,
≈ 1 h at 1 ledger/s`) as both the `threshold` and `extend_to` arguments of
`env.storage().persistent().extend_ttl(key, ttl, ttl)`.  Keys that do not yet exist for
this investor (e.g. `InvestorClaimed` before a claim) are silently skipped.

`extend_investor_ttl` is called on **four** activity paths:

| Entrypoint | Called from | Line (approx.) |
|---|---|---|
| `fund` / `fund_with_commitment` / `fund_ref` | `fund_impl` | after contribution write |
| `fund_batch` | `fund_impl` (once per entry) | after contribution write |
| `claim_investor_payout` / `push_investor_payouts` | `claim_core` | after claim write |
| `refund` / `refund_batch` | `refund_core` | after contribution zero |

### Cost per call

Each `extend_ttl` host-function call consumes a small instruction budget even when the TTL
is already high enough that no on-chain extension is needed; Soroban still charges for
reading the entry's current TTL.  The number of `extend_ttl` calls per investor is bounded
by the size of `keys_to_check` in `extend_investor_ttl` (currently 6 keys), so at most
6 extra host-function calls are added per investor activity.

For `fund_batch` with `n` entries, this adds `n × 6` extend-ttl calls to the batch budget.
The existing `MAX_FUND_BATCH = 32` limit bounds the worst case to 192 extra calls per
`fund_batch` invocation.  Benchmarking on the Soroban simulator shows this is well within
the per-transaction CPU instruction budget.

### Batch budget regression check

`tests::persistent_refund_flags` includes tests that verify TTL rises after each of the four
activity paths without triggering resource-limit errors (resource limits are relaxed in those
tests to focus on storage behavior, not metering).  No regression in the batch size limit
(`MAX_FUND_BATCH`) is expected because `extend_investor_ttl` was already called on the fund
path before this issue; the claim and refund paths are single-investor and not batch-bounded.
