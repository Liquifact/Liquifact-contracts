# LiquiFact Operator Runbook: Redeploy vs. On-Chain Upgrade

> **Scope:** Stellar / Soroban only. This runbook does not apply to EVM or
> Solidity deployments. CLI examples use the `stellar` CLI; verify flag syntax
> against your installed version via `stellar --version`.

---

## 1. Decision tree — redeploy vs. on-chain WASM upgrade

On Stellar/Soroban, "upgrading" a contract means uploading new WASM bytecode
and calling the contract's own upgrade entrypoint (if it exposes one) **or**
deploying a new contract instance entirely.

```
Is InvoiceEscrow struct layout or any stored contracttype XDR shape changed?
│
├─ YES → REDEPLOY (new contract address, new init)
│         └─ Reason: Soroban deserializes stored XDR using the types embedded
│                    in the *current* WASM. A layout change causes deserialization
│                    panic on every read of old data under new WASM.
│
└─ NO  → Are you adding only new optional DataKey variants read with .unwrap_or()?
          │
          ├─ YES → WASM UPGRADE IN PLACE is safe.
          │         └─ Steps: upload new WASM → call upgrade entrypoint (if present)
          │                   → call migrate() only if you want to bump DataKey::Version.
          │                   migrate() currently panics — extend it first.
          │
          └─ NO  → Review change carefully.
                    If you rename/remove an existing DataKey variant → REDEPLOY.
                    If you only add new behavior with no stored-state change → WASM UPGRADE.
```

> **Key Soroban difference from EVM:** there is no `delegatecall`-style proxy
> pattern. Upgrading WASM replaces the bytecode for the *same* contract
> address, but all stored data remains in place and is decoded against the
> **new** WASM's types. A struct layout change is therefore a breaking storage
> change.

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

### Implementing a real migration in `migrate()`

```rust
// Example: adding a new required field `fee_bps: i64` to InvoiceEscrow.
// This requires a redeploy (new struct XDR) but illustrates the pattern
// for a same-instance migration when only a primitive flag changes.

pub fn migrate(env: Env, from_version: u32) -> u32 {
    let stored: u32 = env.storage().instance().get(&DataKey::Version).unwrap_or(0);
    assert!(stored == from_version, "from_version does not match stored version");
    if from_version >= SCHEMA_VERSION {
        panic!("Already at current schema version");
    }

    // Example path: 4 → 5 (additive new key, no struct change)
    if from_version == 4 {
        // Initialize new optional key with a safe default.
        env.storage().instance().set(&DataKey::MinContributionFloor, &0i128);
        env.storage().instance().set(&DataKey::Version, &5u32);
        return 5;
    }

    panic!(
        "No migration path from version {} — extend migrate or redeploy",
        from_version
    );
}
```

**Current state (v5):** `migrate()` panics on **all** paths. No migration
work is implemented. Operators must redeploy if `InvoiceEscrow` layout changes.

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

# Verify stored version matches SCHEMA_VERSION (should return 5)
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK \
  -- get_version
```

---

## 4. WASM upgrade in place (additive-only changes)

Use this path only when no `#[contracttype]` stored struct layout has changed.

```bash
# Step 1: Upload new WASM (get new hash)
stellar contract upload \
  --wasm target/wasm32v1-none/release/liquifact_escrow.wasm \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK

# Step 2: Upgrade the existing contract instance's WASM
stellar contract upgrade \
  --id <EXISTING_CONTRACT_ID> \
  --wasm-hash <NEW_WASM_HASH> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK

# Step 3 (optional): Call migrate() only if you implemented a migration path.
# In the current release, migrate() panics — do NOT call it unless extended.
# stellar contract invoke --id <CONTRACT_ID> ... -- migrate --from_version 4
```

> **Soroban note:** `stellar contract upgrade` replaces the WASM for the
> contract at the given ID. The stored instance data is preserved. Old XDR is
> decoded against the **new** WASM types on the next read.

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

There is **no on-chain downgrade** path on Soroban. If a WASM upgrade
introduces a bug:

```
Option A (safest): Re-upload previous WASM, call stellar contract upgrade
                   back to old hash. Works only if stored data is still
                   compatible with old WASM types.

Option B (layout-broken): Redeploy from old WASM hash (already uploaded).
                           Migrate investor positions off-chain.

Option C (emergency): Activate legal hold on the broken contract to block
                      payouts and settlement. Communicate status to investors.
                      Proceed with Option A or B after root cause is confirmed.
```

```bash
# Option A — revert WASM (requires previous hash)
stellar contract upgrade \
  --id <CONTRACT_ID> \
  --wasm-hash <PREVIOUS_WASM_HASH> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK

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

2. **Perform** the WASM upload and (if applicable) `stellar contract upgrade`.

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
on Soroban). Upgrade authority flows through `stellar contract upgrade`
(protocol-level, requires the deployer footprint) and is not exposed as an
on-chain entrypoint in the current release.

### Admin key hygiene

- Use a multisig wallet or a governed contract as `admin` at all times.
- Never use a single-signer hot wallet as `admin` in production.
- `transfer_admin` requires the current admin's authorization — test the
  rotation on Testnet before executing on Mainnet.

### `migrate()` is not a no-op

Calling `migrate()` with a mismatched `from_version` **panics and aborts the
transaction**. This is intentional — it prevents operators from accidentally
skipping version validation. Do not script automated `migrate()` calls without
first implementing the migration path.

---

## 9. Version compatibility matrix

| WASM version (SCHEMA_VERSION) | Can read data from | Notes |
|-------------------------------|-------------------|-------|
| 5 | 5 | Same version — fully compatible |
| 5 | 4 | Only if no struct layout changed; new optional keys absent → defaults |
| 5 | ≤3 | Only if InvoiceEscrow XDR shape identical; new optional keys absent → defaults |
| 4 reading 5 data | ❌ | Unknown keys ignored; may panic if struct layout changed |

---

## 10. Glossary

| Term | Meaning in this context |
|------|------------------------|
| WASM upload | `stellar contract upload` — publishes bytecode to network; returns a hash |
| WASM upgrade | `stellar contract upgrade` — replaces bytecode for an existing contract ID |
| Redeploy | Deploy a **new** contract instance; old instance is not migrated automatically |
| `DataKey::Version` | On-chain stored schema version set by `init` and updated by `migrate` |
| `SCHEMA_VERSION` | Compile-time constant in WASM; the target version for `init` and migration |
| Legal hold | Admin-set flag that blocks settlement, withdrawal, and investor claims |
| SEP-41 | [Stellar token interface standard](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md) |
