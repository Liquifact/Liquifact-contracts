# escrow-schema-versioning.md

## Overview

The escrow contract stores a **schema version** constant (`SCHEMA_VERSION`) that is written to storage under `DataKey::Version` during `init`. This version is the single source of truth for upgrade decisions and is exposed via `LiquifactEscrow::get_version`.

### Additive‑only rules (storage‑only upgrades)
- **Never rename or delete** an existing `DataKey` variant.
- **Never renumber** `EscrowError` discriminants; error codes are append‑only.
- Adding new `DataKey` variants or new contract‑type structs is safe **if** they are read with `.get(...).unwrap_or(default)` so older deployments treat missing keys as unset.
- Changing the layout or XDR shape of an existing stored type (e.g., adding a required field to `InvoiceEscrow`) **requires** either a migration path in `migrate` **or** a full redeploy.

### Migration flow (`LiquifactEscrow::migrate`)
The `migrate(from_version)` entrypoint validates the stored version and returns typed errors:
| Condition | Typed error (code) |
|-----------|-------------------|
| `stored != from_version` | `MigrationVersionMismatch` (90) |
| `from_version >= SCHEMA_VERSION` | `AlreadyCurrentSchemaVersion` (91) |
| `from_version < SCHEMA_VERSION` with no path | `NoMigrationPath` (92) |

When a new schema change that cannot be handled additively is introduced, implement the transformation inside `migrate` **before** returning the appropriate error and bump `DataKey::Version`.

### Admin entrypoint `upgrade(new_wasm_hash)`
`upgrade` is an admin‑only call that deploys a new contract WASM. After upgrading, operators must:
1. Verify the stored `SCHEMA_VERSION` matches the new contract.
2. If the version increased and a migration path is required, call `migrate` with the previous version.
3. If only additive storage changes were made, no migration call is needed.

### Contributor checklist for storage changes
- [ ] Add new `DataKey` variants only; never rename/delete existing ones.
- [ ] Ensure all new reads use `.unwrap_or(default)` for backward compatibility.
- [ ] If modifying an existing stored struct, either:
    - Add a migration path in `migrate` and bump `SCHEMA_VERSION`, **or**
    - Document that a redeploy is required.
- [ ] Never change `EscrowError` discriminant values; append new errors.
- [ ] Update `docs/escrow-schema-versioning.md` with any new version details.
- [ ] Add or update migration tests asserting the correct typed errors.

---

For a full overview see the [README](../README.md) schema version section.
