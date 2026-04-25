# ADR-007: Storage Key Evolution and Additive-Key Policy

**Status:** Accepted  
**Date:** 2026-04-25  
**Refs:** `escrow/src/lib.rs` — `DataKey`, `SCHEMA_VERSION`, `migrate`; `docs/escrow-data-model.md`

---

## Context

The escrow contract stores all state in Soroban instance storage under a `DataKey` enum. As the
protocol evolves, new features require new storage keys. Soroban does not provide automatic schema
migration: the contract author must decide which changes are safe to deploy in-place and which
require a coordinated migration or full redeploy.

The repository README documents a high-level policy ("Storage-only upgrade policy"). This ADR
formalises that policy, defines the compatibility boundary, and records the test plan.

## Decision

### Rule 1 — New optional keys are always backward-compatible

Adding a new `DataKey` variant is safe when:

1. The new key is read with `.get(...).unwrap_or(default)` so deployments that predate the key
   behave as "unset / default" without panicking.
2. The XDR shape of every existing variant and stored struct is unchanged.
3. The new key's absence does not alter the semantics of any existing entrypoint.

Such changes do **not** require a `SCHEMA_VERSION` bump or a `migrate` call.

### Rule 2 — Changing existing stored types is breaking

The following changes require either a `migrate` implementation or a full redeploy:

- Adding a non-optional field to an existing `#[contracttype]` struct (e.g. `InvoiceEscrow`).
- Renaming a `DataKey` variant or changing its XDR discriminant.
- Changing the stored Rust type of an existing key (e.g. `LegalHold: bool → u32`).

When a breaking change is needed, implement a `migrate(from_version, to_version)` path that reads
the old layout, rewrites under the new layout, and bumps `DataKey::Version`.

### Rule 3 — `SCHEMA_VERSION` tracks breaking changes only

`SCHEMA_VERSION` (currently `5`) is incremented only when a `migrate` path is added. Additive-only
releases leave the version unchanged.

### Rule 4 — Per-address key growth must be re-evaluated

Any new per-address `DataKey` variant (e.g. `DataKey::SomeFlag(Address)`) multiplies storage
consumption by the investor count. Before merging, verify the worst-case serialised size at
`MaxUniqueInvestorsCap` (128) stays within Soroban's contract-data entry limit. The existing
storage-growth regression tests in `escrow/src/test/` serve as the baseline.

## Compatibility test plan

1. Deploy version _N_; exercise `init`, `fund`, `settle`, `claim_investor_payout`.
2. Deploy version _N+1_ with only new optional keys; repeat the same flows; assert old instance
   keys are still readable and return expected defaults.
3. If `InvoiceEscrow` or another existing struct changes, add a migration test that:
   a. Writes the old layout directly via `env.storage().instance().set(...)`.
   b. Calls `migrate(N, N+1)`.
   c. Reads back the new layout and asserts correctness.
4. If no migration path is feasible, document mandatory redeploy in the release notes and bump
   `SCHEMA_VERSION`.

## Consequences

- Reviewers can approve additive-key PRs without requiring a migration test.
- Breaking changes are blocked from merging until a migration path or explicit redeploy note exists.
- `SCHEMA_VERSION` remains a reliable signal: a stored version lower than `SCHEMA_VERSION` means
  `migrate` must be called before using new features that depend on the new layout.
- Storage-growth tests act as regression guards; any PR that adds per-address keys must update or
  extend those tests.

## Rejected alternatives

- **Always bump version on any key addition:** creates unnecessary migration ceremony for purely
  additive changes and makes `migrate` a no-op most of the time.
- **Persistent or temporary storage for per-investor data:** would avoid the instance-size concern
  but changes the access pattern and TTL semantics; deferred to a future ADR if investor counts
  need to scale beyond 128.
