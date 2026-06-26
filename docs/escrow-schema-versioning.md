# Escrow Schema Versioning and Upgrade Contract

This document consolidates the rules for `SCHEMA_VERSION`, `DataKey::Version`, `migrate(from_version)`, and `upgrade(new_wasm_hash)`.

The short rule: additive storage keys are usually safe without migration; changing existing stored layouts is breaking and requires either a tested migration path or a full redeploy.

## Version Sources

| Item | Meaning |
| --- | --- |
| `SCHEMA_VERSION` | Compile-time target schema in `escrow/src/lib.rs`. |
| `DataKey::Version` | Stored schema version written at `init` and read by `get_version()`. |
| `get_version()` | Returns the stored `DataKey::Version`, or `0` when absent before init. |
| `migrate(from_version)` | Admin-gated version check and future migration hook. Currently all paths fail with typed errors. |
| `upgrade(new_wasm_hash)` | Admin-gated same-address WASM replacement. It does not change storage or bump `DataKey::Version`. |

`init` writes `DataKey::Version = SCHEMA_VERSION`. A same-address WASM upgrade changes executable code only; it does not rewrite storage by itself.

## Current Version

`SCHEMA_VERSION = 6`.

Version 6 moved per-investor keys to persistent storage so investor-count growth does not enlarge the contract instance entry. There is no enumerable on-chain path to copy old per-investor instance keys into persistent storage, so v6 requires redeploy for affected old instances.

See [`docs/adr/ADR-007-storage-key-evolution.md`](adr/ADR-007-storage-key-evolution.md) for the accepted storage-key policy.

## Additive-Only Changes

Additive changes are compatible when all are true:

- New state uses a new `DataKey` variant.
- Reads use `.get(...).unwrap_or(default)` or equivalent defaulting.
- Existing `DataKey` variants are not renamed, removed, or reordered.
- Existing stored struct layouts and XDR shapes do not change.
- Existing entrypoints keep their previous semantics when the new key is absent.

Additive-only changes normally do not require a `SCHEMA_VERSION` bump and do not require `migrate()`.

## Breaking Changes

Treat these as breaking:

- Adding a required field to an existing stored `#[contracttype]` struct.
- Removing, renaming, or reordering a `DataKey` variant.
- Changing the Rust/XDR type stored under an existing key.
- Renumbering or removing an `EscrowError` discriminant.
- Moving storage between instance and persistent storage when the old key set cannot be enumerated.

Breaking changes require one of:

- A tested `migrate(from_version)` branch that rewrites old storage into the new layout and writes `DataKey::Version` last.
- A documented full redeploy path when migration is impossible or unsafe.

## `migrate(from_version)`

`migrate` is admin-gated before version checks. Keep that ordering for every future migration branch.

Current behavior:

| Condition | Error |
| --- | --- |
| Stored version does not equal `from_version` | `MigrationVersionMismatch` |
| `from_version >= SCHEMA_VERSION` | `AlreadyCurrentSchemaVersion` |
| `from_version < SCHEMA_VERSION` and no migration branch exists | `NoMigrationPath` |

No storage writes occur in the current implementation. Do not call `migrate` as a post-upgrade bookkeeping step unless a concrete migration branch has been added.

## Adding a Real Migration Path

Checklist for contributors:

1. Bump `SCHEMA_VERSION`.
2. Add a branch in `migrate` for the exact old version.
3. Keep admin authorization before all version reads and writes.
4. Require `stored == from_version`.
5. Decode old values safely.
6. Transform values using checked arithmetic where amounts or counters are involved.
7. Write transformed state before version update.
8. Write `DataKey::Version` last.
9. Return the new version.
10. Add tests for success, mismatch, already-current, no-path, and version immutability.

Example shape:

```rust
if from_version == 6 && SCHEMA_VERSION == 7 {
    // read old keys
    // write new keys or rewritten structs
    env.storage().instance().set(&DataKey::Version, &7u32);
    return 7;
}
```

## `upgrade(new_wasm_hash)`

`upgrade` replaces the current contract WASM for the same contract id. It requires admin auth and emits an upgrade event, but it does not:

- Change `DataKey::Version`.
- Rewrite `InvoiceEscrow`.
- Rewrite per-investor keys.
- Validate that the new WASM is storage-compatible.

Before using `upgrade`, operators must verify that the new WASM is additive-only or that a migration plan is ready. If the change requires redeploy, do not use `upgrade` as a substitute for migration.

## Persistent Compatibility Rules

`DataKey` and `EscrowError` are part of the persistence and integration contract:

- Append new `DataKey` variants; do not delete or rename existing variants.
- Append new `EscrowError` variants; do not reuse numeric codes.
- Keep stored `#[contracttype]` structs backward-readable unless a migration branch exists.
- Document mandatory redeploy when old storage cannot be enumerated or decoded.

## Test Coverage

Existing tests in `escrow/src/tests/admin.rs` cover the current migrate contract:

- Auth-first ordering.
- `MigrationVersionMismatch`.
- `AlreadyCurrentSchemaVersion`.
- `NoMigrationPath`.
- Historical versions below `SCHEMA_VERSION`.
- Uninitialized version `0`.
- `DataKey::Version` immutability across error branches.

When a success migration branch is added, add a success-path test that writes old-version storage, calls `migrate(old_version)`, reads the new layout, and asserts `get_version() == SCHEMA_VERSION`.

## Operator Checklist

Before merging a storage-affecting PR:

- Identify every new or changed storage key.
- Classify the change as additive, migratable, or redeploy-only.
- Confirm absent-key defaults for additive changes.
- Confirm no `EscrowError` codes were reused.
- Confirm README, ADRs, and release notes describe the compatibility path.
- Run the full Rust test suite and any storage-growth tests relevant to new per-address keys.
