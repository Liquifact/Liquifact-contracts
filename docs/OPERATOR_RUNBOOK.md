# LiquiFact Operator Runbook: Redeploy vs. On-Chain Upgrade

> **Scope:** Stellar / Soroban only. This runbook does not apply to EVM or
> Solidity deployments. CLI examples use the `stellar` CLI; verify flag syntax
> against your installed version via `stellar --version`.

---

## 1. Decision tree — redeploy vs. on-chain WASM upgrade

On Stellar/Soroban, "upgrading" a contract means preserving the contract ID and
stored ledger entries while switching the instance to new WASM bytecode. The
standard contract pattern is an admin-gated entrypoint that calls
`env.deployer().update_current_contract_wasm(new_wasm_hash)`. If the deployed
contract does not expose that upgrade entrypoint, operators must redeploy a new
contract instance instead.

```
Does the existing instance expose an admin-gated WASM upgrade entrypoint?
│
├─ NO  → REDEPLOY (new contract address, new init)
│
└─ YES → Did InvoiceEscrow or any stored #[contracttype] XDR shape change?
          │
          ├─ YES → REDEPLOY (new contract address, new init)
          │         └─ Reason: stored XDR must decode against the new WASM's
          │                    contract types; layout changes are breaking.
          │
          └─ NO  → Is the change only new DataKey variants read with defaults?
                    │
                    ├─ YES → WASM UPGRADE IN PLACE. Do not call migrate().
                    │
                    └─ NO  → Does the change require rewriting existing storage?
                              │
                              ├─ YES → Extend migrate() first, test it, then upgrade
                              │         and call migrate() with the stored version.
                              │
                              └─ NO  → WASM UPGRADE IN PLACE, after code review.
```

> **Key Soroban difference from EVM:** there is no `delegatecall`-style proxy
> pattern in this contract. A same-address upgrade preserves stored data and
> runs the new WASM against that data. A stored type layout change is therefore
> a breaking storage change unless an explicit, tested migration can decode the
> old data shape and rewrite it safely.

---

## 2. `SCHEMA_VERSION` lifecycle

`SCHEMA_VERSION` (defined in `escrow/src/lib.rs`) and `DataKey::Version` track
the storage schema independently of the WASM binary version.

| Action | `DataKey::Version` | `SCHEMA_VERSION` in WASM |
|--------|--------------------|--------------------------|
| Fresh `init` | Written to `SCHEMA_VERSION` | Same |
| Additive-only WASM upgrade | Unchanged (old value stays) | New WASM constant |
| Layout-breaking change + redeploy + new `init` | Written to new `SCHEMA_VERSION` | Same |
| Operator calls `migrate()` after extending it | Updated by `migrate` to new version | Same |

### When to bump `SCHEMA_VERSION`

Bump `SCHEMA_VERSION` when **any** of the following is true:

- You change the XDR shape of `InvoiceEscrow`, `SmeCollateralCommitment`,
  `FundingCloseSnapshot`, `YieldTier`, or any other `#[contracttype]` struct
  stored at an existing key.
- You remove or rename an existing `DataKey` variant that live instances use.
- You change the semantic meaning of an existing stored value in a backward-
  incompatible way.

Do **not** bump `SCHEMA_VERSION` for:

- Adding a new `DataKey` variant read with `.get(...).unwrap_or(default)`.
- Adding a new `#[contracttype]` stored at a new key.
- Behavioral changes that do not touch stored state.

### Changelog-based transition classification

This table is derived from the `SCHEMA_VERSION` changelog in
`escrow/src/lib.rs`. It is the operator classification for historical
transitions through version 5.

| Transition | Changelog source | Operator action |
|------------|------------------|-----------------|
| Fresh deploy → 1 | Initial schema (`InvoiceEscrow` v1, basic fund / settle) | Fresh `init`; no migration exists before v1. |
| 1 → 2 | Added `InvestorEffectiveYield`, `InvestorClaimNotBefore` | Additive. No `migrate()` call required when readers default missing values. |
| 2 → 3 | Added `FundingCloseSnapshot`, `MinContributionFloor`, `MaxUniqueInvestorsCap`, `UniqueFunderCount` | Additive. No `migrate()` call required when readers default missing values. |
| 3 → 4 | Added `PrimaryAttestationHash`, `AttestationAppendLog` | Additive. No `migrate()` call required. |
| 4 → 5 | Added `YieldTierTable`, `RegistryRef`, `Treasury`; tightened `InvoiceEscrow` layout | Conditional. Additive keys alone need no `migrate()`, but any `InvoiceEscrow` XDR/layout change is breaking and requires redeploy. |
| 5 → 6 | Per-investor keys moved to persistent storage | Breaking for existing instances. Redeploy required because prior per-investor instance keys are address-keyed and not enumerable by the contract. |

Operational rule: additive keys are safe only when old instances can read them
with explicit defaults. Changing, renaming, or removing an existing key or
changing an existing stored type's XDR shape is not additive.

### Implementing a real migration in `migrate()`

```rust
pub fn migrate(env: Env, from_version: u32) -> u32 {
    // Keep this guard first. Current code already requires admin auth before
    // version checks so every future storage rewrite is admin-gated.
    Self::get_escrow(env.clone()).admin.require_auth();

    let stored: u32 = env.storage().instance().get(&DataKey::Version).unwrap_or(0);
    ensure(
        &env,
        stored == from_version,
        EscrowError::MigrationVersionMismatch,
    );

    if from_version >= SCHEMA_VERSION {
        fail(&env, EscrowError::AlreadyCurrentSchemaVersion)
    }

    // Example pattern for a future same-instance migration:
    if from_version == 6 && SCHEMA_VERSION == 7 {
        // 1. Read only old-version data that the new WASM can still decode.
        // 2. Validate arithmetic with checked_* operations.
        // 3. Write new keys or rewritten values exactly once.
        // 4. Write DataKey::Version last.
        env.storage().instance().set(&DataKey::Version, &7u32);
        return 7;
    }

    fail(&env, EscrowError::NoMigrationPath)
}
```

Step-by-step implementation requirements for a real migration:

1. Bump `SCHEMA_VERSION` in `escrow/src/lib.rs`.
2. Add the migration branch above the terminal `EscrowError::NoMigrationPath`.
3. Keep `Self::get_escrow(env.clone()).admin.require_auth()` before all version
   checks and before every storage write.
4. Read `DataKey::Version` and require `stored == from_version`.
5. For each migrated value, prove the new WASM can decode the old stored value;
   otherwise redeploy instead of migrating in place.
6. Use checked arithmetic for transformed numeric values and keep writes
   bounded. The migration should be O(number of explicitly supplied or
   enumerable keys); do not design a migration that assumes contract storage can
   enumerate all investors.
7. Write all transformed state before writing `DataKey::Version`.
8. Write `DataKey::Version` last and return the new version.
9. Add unit tests for version mismatch, already-current version, unauthorized
   caller, the successful migration path, and repeated calls after success.
10. Update this runbook, the rustdoc changelog, and any affected read/API docs.

**Current state (v6):** `migrate()` fails with typed contract errors on **all**
paths. No migration work is implemented. The entrypoint is admin-gated before
version checks so any future storage-mutating migration path is authenticated by
construction. See [ADR-007](adr/ADR-007-storage-key-evolution.md) for the
storage-key evolution policy. Operators must redeploy if `InvoiceEscrow` layout
changes.

### Exhaustive test coverage for `migrate()`

The unit test suite in `escrow/src/tests/admin.rs` exercises every documented
error branch end-to-end:

- **Auth-first ordering** — `test_migrate_rejects_non_admin_before_version_check`
  asserts that a non-admin caller is rejected with an auth failure, never
  reaching the version guards.
- **`MigrationVersionMismatch`** — `test_migrate_version_mismatch_stored_neq_claimed`
  and `test_migrate_far_below_stored_raises_mismatch` cover the exact-mismatch
  guard and assert `DataKey::Version` is untouched.
- **`AlreadyCurrentSchemaVersion`** — `test_migrate_at_schema_version_raises_already_current`
  and `test_migrate_above_schema_version_raises_already_current` cover the
  boundary (`from_version == SCHEMA_VERSION`) and the above-boundary case.
- **`NoMigrationPath`** — `test_migrate_below_schema_version_matching_stored_raises_no_path`,
  `test_migrate_all_historical_versions_raise_no_path` (v1–v5), and
  `test_migrate_from_zero_uninitialized_raises_no_path` cover every
  below-`SCHEMA_VERSION` path with a matching stored version, including the
  absent-key default (`0`).
- **Version immutability** — `test_migrate_version_immutable_across_all_error_branches`
  sweep-covers representative values from each branch and asserts
  `DataKey::Version` is unchanged on every failed call.

### Current `migrate()` panic policy

This table must match the `migrate` rustdoc in `escrow/src/lib.rs`.

| Condition | Typed error |
|-----------|-------------|
| `stored_version != from_version` | `EscrowError::MigrationVersionMismatch` |
| `from_version >= SCHEMA_VERSION` | `EscrowError::AlreadyCurrentSchemaVersion` |
| Any `from_version < SCHEMA_VERSION` without an implemented migration branch | `EscrowError::NoMigrationPath` |

Because Soroban aborts the transaction on contract panic, these errors perform
no storage writes in the current release. Operators must not call `migrate()` as
a bookkeeping step after additive upgrades.

---

## 2.5. `upgrade()` vs `migrate()`: precise operator guide

### Division of labor

| What changed in the new WASM | Action | Notes |
|------------------------------|--------|-------|
| New `DataKey` variants (additive, read with defaults) | `upgrade()` only | No `migrate()` call; old instances return defaults |
| Bug fix / logic change (no stored type changes) | `upgrade()` only | Storage layout unchanged |
| Existing `#[contracttype]` struct gained a field | **Redeploy** | Stored XDR cannot be decoded by new WASM |
| Existing `DataKey` renamed, removed, or reordered | **Redeploy** | XDR discriminant changes corrupt existing state |
| Storage rewrite is feasible and `migrate()` extended | `upgrade()` then `migrate(stored_version)` | Implement migration branch first |

### Additive-key compatibility rules (ADR-007, Rule 1)

A new `DataKey` variant is safe to deploy in-place (without calling `migrate()`) when:

1. **Read with defaults**: every entrypoint that reads the new key uses
   `.get(&DataKey::NewVariant).unwrap_or(default)` so pre-existing instances
   behave as "unset / default" without panicking.
2. **No struct shape change**: the XDR encoding of every existing stored
   `#[contracttype]` struct (`InvoiceEscrow`, `FundingCloseSnapshot`,
   `YieldTier`, `SmeCollateralCommitment`) is unchanged.
3. **No existing variant changes**: no existing `DataKey` variant is renamed,
   removed, or reordered.

### DataKey XDR discriminant stability rule (critical)

The `DataKey` enum is serialized to XDR on-chain. In Soroban's `contracttype`
XDR encoding, **each variant is assigned an integer discriminant equal to its
position in the enum definition (0-indexed)**. This discriminant is stored
on-chain as the key identifier.

**Consequence:** if you reorder existing `DataKey` variants, their on-chain
discriminants change. A storage slot that was previously keyed as discriminant
5 (`LegalHold`) would become readable only under the new position's integer.
All existing data keyed under the old discriminant becomes unreachable under the
new WASM's type system — the contract would silently return defaults or decode
garbage for those keys.

**Rule: never reorder existing `DataKey` variants. Only append new variants at
the end of the enum.** This is the "additive-only" guarantee in ADR-007.

Reviewers must verify this rule on every PR that touches `DataKey`. The rule
applies to both instance and persistent storage keys.

### `migrate()` typed-error branches (current release)

All three branches abort the Soroban transaction with no storage writes:

| Condition | Error | Error code |
|-----------|-------|-----------|
| `stored_version != from_version` | `EscrowError::MigrationVersionMismatch` | 90 |
| `from_version >= SCHEMA_VERSION` | `EscrowError::AlreadyCurrentSchemaVersion` | 91 |
| `from_version < SCHEMA_VERSION`, no implemented branch | `EscrowError::NoMigrationPath` | 92 |

Execution order within `migrate()`:
1. `Self::load_escrow_require_admin(&env)` — admin auth gate (always first).
2. Read `DataKey::Version` from instance storage.
3. `ensure(stored == from_version)` → `MigrationVersionMismatch` if not equal.
4. `if from_version >= SCHEMA_VERSION` → `AlreadyCurrentSchemaVersion`.
5. `else` → `NoMigrationPath` (no implemented migration branch in this release).

**No storage writes occur in any current execution path.** Future extension:
add the migration logic above step 5, write transformed state, set
`DataKey::Version` to the new version, and return the new version.

### Step-by-step: additive-only WASM upgrade

Use this path when only new `DataKey` variants are added (no struct changes,
no variant reordering).

```
Pre-conditions:
  - New DataKey variants all use .unwrap_or(default) reads.
  - No existing DataKey variant was reordered or renamed.
  - No existing #[contracttype] struct field was added, removed, or changed.
  - Tests on Testnet pass.

Steps:
  1. cargo build --target wasm32v1-none --release -p liquifact_escrow
  2. stellar contract upload --wasm target/.../liquifact_escrow.wasm ...
     → captures NEW_WASM_HASH
  3. stellar contract invoke --id <ID> ... -- set_legal_hold --active true
     (blocks settlements/claims during the swap window)
  4. stellar contract invoke --id <ID> ... -- upgrade --new_wasm_hash <NEW_WASM_HASH>
  5. stellar contract invoke --id <ID> ... -- get_version
     (should still return the same value — upgrade() does not change DataKey::Version)
  6. stellar contract invoke --id <ID> ... -- get_escrow
     (verify all fields are intact)
  7. stellar contract invoke --id <ID> ... -- clear_legal_hold
  8. Run post-upgrade smoke tests (fund, get_investor_contribution, etc.)
  *** Do NOT call migrate() — no migration is needed for additive changes. ***
```

### Step-by-step: schema-breaking upgrade + migrate

Use this path only when an existing stored struct or `DataKey` semantic must
change AND you have extended `migrate()` with a concrete transformation.

```
Pre-conditions:
  - migrate() has been extended with a from_version → new_version branch.
  - The migration branch reads old data, transforms it, writes new data.
  - DataKey::Version is written LAST in the migration branch.
  - SCHEMA_VERSION has been bumped in escrow/src/lib.rs.
  - cargo test passes with the new migration branch.
  - Testnet mirror has been upgraded and migrate() called successfully.

Steps:
  1. cargo build --target wasm32v1-none --release -p liquifact_escrow
  2. stellar contract upload --wasm target/.../liquifact_escrow.wasm ...
     → captures NEW_WASM_HASH
  3. stellar contract invoke --id <ID> ... -- get_version
     → note STORED_VERSION (e.g., 6)
  4. stellar contract invoke --id <ID> ... -- set_legal_hold --active true
  5. stellar contract invoke --id <ID> ... -- upgrade --new_wasm_hash <NEW_WASM_HASH>
  6. stellar contract invoke --id <ID> ... -- migrate --from_version <STORED_VERSION>
     (migrate() validates stored == from_version, then applies the transformation)
  7. stellar contract invoke --id <ID> ... -- get_version
     (should now return new SCHEMA_VERSION)
  8. stellar contract invoke --id <ID> ... -- get_escrow
     (verify all fields are correct under the new schema)
  9. stellar contract invoke --id <ID> ... -- clear_legal_hold
  10. Run post-upgrade smoke tests.
```

> **Warning:** if `migrate()` is called before `upgrade()`, it will run against
> the old WASM's `SCHEMA_VERSION` constant and will error with
> `AlreadyCurrentSchemaVersion` (since stored version == old SCHEMA_VERSION ==
> old WASM's constant). Always upgrade first, then migrate.

---

## 3. Pre-flight checklist (testnet → mainnet)

Complete all items before promoting to Mainnet.

### Build & verify

```bash
# 1. Add WASM target
rustup target add wasm32v1-none

# 2. Build release WASM
cargo build --target wasm32v1-none --release -p liquifact_escrow

# 3. Format check
cargo fmt --all -- --check

# 4. Lint (zero warnings)
cargo clippy -p liquifact_escrow -- -D warnings

# 5. Full test suite
cargo test -p liquifact_escrow

# 6. Coverage gate (≥ 95% lines)
cargo llvm-cov \
  --features testutils \
  --fail-under-lines 95 \
  --summary-only \
  -p liquifact_escrow

# 7. Confirm WASM artifact exists
ls target/wasm32v1-none/release/liquifact_escrow.wasm
```

### Contract security checklist

- [ ] `admin` is a multisig or governed contract (not an EOA alone).
- [ ] `funding_token` is a standard SEP-41 token (no fee-on-transfer).
- [ ] `treasury` address is controlled by LiquiFact governance.
- [ ] `invoice_id` matches off-chain invoice slug (ASCII alphanumeric + `_`,
      max 32 chars).
- [ ] `maturity` is set in ledger timestamp seconds (not wall-clock oracle).
- [ ] `min_contribution` and `max_unique_investors` match legal offering
      documents.
- [ ] Legal hold (`set_legal_hold`) procedure is documented in ops playbook.
- [ ] Attestation digests and their canonical off-chain encoding are
      documented.
- [ ] CI passes: format, clippy, tests, coverage ≥ 95%.

### Testnet smoke test

```bash
export STELLAR_NETWORK=testnet
export SOROBAN_RPC_URL=https://soroban-testnet.stellar.org
export SOURCE_SECRET=S...          # deployer secret key
export LIQUIFACT_ADMIN_ADDRESS=G...

# Upload WASM
stellar contract upload \
  --wasm target/wasm32v1-none/release/liquifact_escrow.wasm \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK

# Deploy instance
stellar contract deploy \
  --wasm-hash <WASM_HASH_FROM_UPLOAD> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK

# Call init (example — adjust params to your invoice)
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK \
  -- init \
  --admin $LIQUIFACT_ADMIN_ADDRESS \
  --invoice_id INV001 \
  --sme_address G... \
  --amount 10000000000 \
  --yield_bps 800 \
  --maturity 0 \
  --funding_token C... \
  --registry null \
  --treasury G... \
  --yield_tiers null \
  --min_contribution null \
  --max_unique_investors null

# Verify stored version matches SCHEMA_VERSION (should return 6)
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK \
  -- get_version
```

---

## 4. WASM upgrade in place (additive-only changes)

Use this path only when no `#[contracttype]` stored struct layout has changed
and the currently deployed contract exposes an admin-gated upgrade entrypoint
that calls `env.deployer().update_current_contract_wasm(new_wasm_hash)`.

```bash
# Step 1: Upload new WASM (get new hash)
stellar contract upload \
  --wasm target/wasm32v1-none/release/liquifact_escrow.wasm \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK

# Step 2: Invoke the deployed contract's upgrade() entrypoint with admin credentials.
stellar contract invoke \
  --id <EXISTING_CONTRACT_ID> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK \
  -- upgrade --new_wasm_hash <NEW_WASM_HASH>

# Step 3 (optional): Call migrate() only if you implemented a migration path.
# In the current release, migrate() panics — do NOT call it unless extended.
# stellar contract invoke --id <CONTRACT_ID> ... -- migrate --from_version 4
```

> **Soroban note:** `env.deployer().update_current_contract_wasm` replaces the
> WASM for the contract at the current ID. The stored instance data is
> preserved. Old XDR is decoded against the **new** WASM types on the next read.

---

## 5. Redeploy (layout-breaking changes)

When `InvoiceEscrow` struct or any stored `#[contracttype]` changes XDR shape,
the only safe path is a fresh deploy.

```bash
# 1. Build and upload new WASM (as above).

# 2. Deploy new contract instance — this gets a new contract ID.
stellar contract deploy \
  --wasm-hash <NEW_WASM_HASH> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK
# → prints NEW_CONTRACT_ID

# 3. Call init on the new instance.
stellar contract invoke \
  --id <NEW_CONTRACT_ID> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK \
  -- init ...

# 4. Migrate off-chain state (investor records, indexer pointers) to new contract ID.
# 5. Retire old contract: set legal hold, then archive off-chain reference.
```

**The old contract instance is NOT deleted on-chain** — Soroban does not
support contract destruction. Operators must:

- Communicate the new contract ID to all integrators and indexers.
- Ensure no new funding flows reach the old contract (update integrator configs
  before announcing the migration).
- Keep legal hold active on the old contract if it has live principal.

---

## 6. Rollback protocol

There is **no automatic on-chain downgrade** path on Soroban. If a WASM upgrade
introduces a bug, recovery still requires an available admin-gated upgrade
entrypoint on the deployed contract:

```
Option A (safest): Re-upload previous WASM, invoke the contract's upgrade
                   entrypoint back to the old hash. Works only if stored data
                   is still compatible with old WASM types.

Option B (layout-broken): Redeploy from old WASM hash (already uploaded).
                           Migrate investor positions off-chain.

Option C (emergency): Activate legal hold on the broken contract to block
                      payouts and settlement. Communicate status to investors.
                      Proceed with Option A or B after root cause is confirmed.
```

```bash
# Option A — revert WASM through the contract's upgrade entrypoint
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK \
  -- upgrade --new_wasm_hash <PREVIOUS_WASM_HASH>

# Emergency hold (before investigating)
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK \
  -- set_legal_hold --active true
```

---

## 7. Legal hold coordination during upgrade windows

1. **Before** uploading new WASM: activate legal hold on any live escrow
   instance that will be upgraded, to block in-flight settlement or claims.

   ```bash
   stellar contract invoke --id <ID> ... -- set_legal_hold --active true
   ```

2. **Perform** the WASM upload and, if the contract supports it, invoke the
   admin-gated upgrade entrypoint.

3. **Verify** the upgraded contract: call `get_version`, `get_escrow`, and
   run smoke tests on Testnet mirror.

4. **Clear** legal hold once you are satisfied the upgrade is correct.

   ```bash
   stellar contract invoke --id <ID> ... -- clear_legal_hold
   ```

> **Important:** `clear_legal_hold` requires the **same `admin`** that set it.
> If admin was rotated during the upgrade, the new admin must call it. There is
> no bypass or timelock in the current contract — operational playbooks must
> ensure admin continuity.

---

## 8. Security notes for operators

### Token economics (out of scope)

`escrow/src/external_calls.rs` explicitly documents that **fee-on-transfer,
rebasing, and hook tokens are out of scope**. The post-transfer balance-equality
assertions will `panic!` (safe failure) if the token does not conform to
standard SEP-41 behavior. Governance must vet any token contract before it is
used as `funding_token` in an escrow instance.

### No EVM proxy patterns

This contract does not implement a proxy pattern (no `delegatecall` equivalent
on Soroban). Same-address upgrade authority flows through the `upgrade()`
entrypoint that requires admin authorization before calling
`env.deployer().update_current_contract_wasm`. See §4 for the complete
in-place upgrade procedure.

### Admin key hygiene

- Use a multisig wallet or a governed contract as `admin` at all times.
- Never use a single-signer hot wallet as `admin` in production.
- **Two-Step Rotation:** Admin rotation is strictly a two-step procedure to prevent locking out admin-gated functions due to typographical errors:
  1. **`propose_admin(new_admin, validity_window_secs)`**: Requires authorization from the current admin. It validates that `new_admin` is not the current admin (reverts with `NewAdminSameAsCurrent` / code 80 if they are identical). On success, it writes the successor to `DataKey::PendingAdmin` and the expiry timestamp to `DataKey::PendingAdminExpiry` and emits `AdminProposedEvent` (`adm_prop`).
  2. **`accept_admin()`**: Requires authorization from the proposed successor address. It verifies that a proposal exists (reverts with `NoPendingAdmin` / code 172 if `DataKey::PendingAdmin` is absent) and that the proposal has not expired (reverts with `AdminProposalExpired` / code 85 if `ledger.timestamp() > PendingAdminExpiry`). On success, it updates `InvoiceEscrow::admin` to the successor address, clears the pending keys from storage, and emits `AdminTransferredEvent` (`admin`).
- Dashboards and runbooks should call `get_pending_admin_remaining_secs()` to display the remaining proposal validity window. The view returns `Some(0)` exactly at expiry while `accept_admin` still accepts, and also after expiry when `accept_admin` rejects.
- Test both steps on Testnet before executing on Mainnet (see `test_admin_handover_lifecycle` and `test_post_handover_admin_can_clear_hold_set_by_old_admin` in `escrow/src/tests/admin.rs` for implementation reference).

#### Cancelling a pending admin proposal

If you proposed the wrong successor, or the handover is being abandoned before
`accept_admin` is called, use `cancel_pending_admin` to retract the nomination.
Until cancelled or expired, the proposed address can call `accept_admin`;
leaving the pending key live is a standing key-rotation risk.

```bash
# Cancel an unaccepted handover — requires current admin authorization.
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK \
  -- cancel_pending_admin
```

| State | Effect |
|-------|--------|
| `DataKey::PendingAdmin` present | Removed; `accept_admin` will now fail with `NoPendingAdmin` (code 163). |
| `DataKey::PendingAdmin` absent | Panics with `NoPendingAdmin` (code 163) — nothing to cancel. |

The current `InvoiceEscrow::admin` is **unchanged**. The operator may call
`propose_admin` again after a cancel to nominate a different successor. The
`AdminProposalCancelled` event (`adm_can`) carries `invoice_id` and the
`cancelled_pending` address for indexer auditing.


### `migrate()` is not a no-op

Calling `migrate()` with a mismatched `from_version` **panics and aborts the
transaction**. This is intentional — it prevents operators from accidentally
skipping version validation. Do not script automated `migrate()` calls without
first implementing the migration path.

### Deprecated transfer_admin shim and two-step handover

The `transfer_admin` entrypoint is exposed solely as a `#[deprecated]` shim that delegates to [`propose_admin`](#admin-key-hygiene). 

Calling `transfer_admin` **does not** perform an immediate or one-step handover of admin authority. It only initiates Step 1 of the rotation process by setting a pending proposal in contract storage. The proposed successor address must still explicitly call `accept_admin` to assume active admin authority.

#### Handover Observability
Because the shim delegates to `propose_admin`, calling it emits exactly one event:
- **`AdminProposedEvent`** (topic: `adm_prop`) carrying the `invoice_id`, `current_admin`, and `pending_admin`.

No other events (such as `DeprecatedTransferAdminUsed`) are defined or emitted by the contract code. Operators auditing or indexers parsing for deprecated shim usage should monitor `AdminProposedEvent` and check if the initiating transaction invoked `transfer_admin` instead of the canonical `propose_admin` entrypoint.

#### Recovery Path: Overriding Stuck Holds via Key Rotation
The admin is the only role authorized to call `clear_legal_hold` or `request_clear_legal_hold`. If the active admin's private key is lost or compromised while a legal hold is active, funds will remain frozen on-chain because all risk-bearing operations (funding, settle, withdraw, claim) are blocked by the hold.

However, the admin rotation entrypoints (`propose_admin`, `accept_admin`, `cancel_pending_admin`) are **not** gated by the legal hold. This design ensures that operators can execute a recovery handover even while the contract is frozen:

1. **Propose Successor:** The current admin (or the governance multisig/DAO) calls `propose_admin(new_admin, validity_window_secs)` to nominate the recovery key.
2. **Accept Handover:** The nominated address calls `accept_admin()`. This immediately promotions the new address to `InvoiceEscrow::admin` and clears the pending proposal. The old admin key is immediately locked out.
3. **Clear Hold:** The newly promoted admin calls `clear_legal_hold()` (or initiates the timelocked clear via `request_clear_legal_hold`), which successfully unfrezes the escrow contract.

This recovery flow ensures that admin key rotation is always available as an emergency lever to restore operations under a compliance lock. See [ADR-002](adr/ADR-002-auth-boundaries.md) for the authorization boundary definitions.

---

## 9. Version compatibility matrix

| WASM version (SCHEMA_VERSION) | Can read data from | Notes |
|-------------------------------|-------------------|-------|
| 6 | 6 | Same version — fully compatible |
| 6 | 5 | Requires redeploy for per-investor key relocation; no in-place migration path |
| 6 | ≤4 | Only with an explicit migration path or redeploy; new optional keys absent → defaults when compatible |
| ≤5 reading 6 data | ❌ | Older WASM reads per-investor accounting from instance storage |

---

## 10. Glossary

| Term | Meaning in this context |
|------|------------------------|
| WASM upload | `stellar contract upload` — publishes bytecode to network; returns a hash |
| WASM upgrade | Admin-gated contract entrypoint calling `env.deployer().update_current_contract_wasm` for an existing contract ID |
| Redeploy | Deploy a **new** contract instance; old instance is not migrated automatically |
| `DataKey::Version` | On-chain stored schema version set by `init` and updated by `migrate` |
| `SCHEMA_VERSION` | Compile-time constant in WASM; the target version for `init` and migration |
| Legal hold | Admin-set flag that blocks settlement, withdrawal, and investor claims |
| SEP-41 | [Stellar token interface standard](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md) |
