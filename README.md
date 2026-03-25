# LiquiFact Escrow Contract – Threat Model & Security Notes

Soroban smart contracts for **LiquiFact** — the global invoice liquidity network on Stellar.
This repo contains the **escrow** contract that holds investor funds for tokenized invoices until settlement.

---

## Threat Model

### 1. Unauthorized Access

**Risk:**
- Anyone can call `fund` or `settle`

**Impact:**
- Malicious settlement
- Fake funding events

**Mitigation (Current):**
- None (mock auth used in tests)

**Recommended Controls:**
- Require auth:
  - `fund`: investor must authorize
  - `settle`: only trusted role (e.g. admin/oracle)

---

### 2. Arithmetic Risks (Overflow / Underflow)

**Risk:**
- `funded_amount += amount` may overflow `i128`

---

### 3. Replay / Double Execution

```bash
git clone <this-repo-url>
cd liquifact-contracts
cargo build
cargo test
```

---

### 5. Invalid Input / Economic Attacks

**Risks:**
- Negative funding
- Zero funding
- Invalid maturity

| Command                | Description                                                |
|------------------------|------------------------------------------------------------|
| `cargo build`          | Build all contracts                                        |
| `cargo test`           | Run unit tests and property-based tests (using `proptest`) |
| `cargo fmt`            | Format code                                                |
| `cargo fmt -- --check` | Check formatting (used in CI)                              |

---

### 6. Time-based Attacks

```text
liquifact-contracts/
├── Cargo.toml              # Workspace definition
├── docs/
│   └── EVENT_SCHEMA.md    # Indexer-friendly event schema reference
├── escrow/
│   ├── Cargo.toml          # Escrow contract crate
│   └── src/
│       ├── lib.rs       # LiquiFact escrow contract (init, fund, settle, migrate)
│       └── test.rs      # Unit tests
├── docs/
│   ├── openapi.yaml     # OpenAPI 3.1 specification
│   ├── package.json     # Test runner deps (AJV, js-yaml)
│   └── tests/
│       └── openapi.test.js  # Schema conformance tests (51 cases)
└── .github/workflows/
    └── ci.yml              # CI: fmt, build, test
```

Records an investor contribution. Transitions to `status = 1` when
`funded_amount >= funding_target`.

> **Production note:** Must be called atomically with a SEP-41 token `transfer`
> from `investor` to the contract address. This version records accounting only.

**Parameters**

| Parameter   | Constraints                                  |
|-------------|----------------------------------------------|
| `_investor` | Investor's Stellar address (for audit trail) |
| `amount`    | > 0 recommended; partial funding is allowed  |

**Returns** — Updated `InvoiceEscrow`.

**Failure conditions**

| Condition                 | Behaviour                               |
|---------------------------|-----------------------------------------|
| `status != 0`             | Panics: `"Escrow not open for funding"` |
| `init` not called         | Panics: `"Escrow not initialized"`      |
| `funded_amount` overflows | Rust panics (debug) / wraps (release)   |

**State transitions**

- **init** — Create an invoice escrow (invoice id, SME address, admin address, amount, yield bps, maturity).
- **get_escrow** — Read current escrow state.
- **get_version** — Return the stored schema version number.
- **fund** — Record investor funding; status becomes "funded" when target is met.
- **settle** — Mark escrow as settled (buyer paid; investors receive principal + yield).
- **migrate** — Upgrade storage from an older schema version to the current one (see below).

---

## Contract migration strategy

### Overview

The escrow contract stores its state as a single [`InvoiceEscrow`](escrow/src/lib.rs) struct under the instance storage key `"escrow"`, alongside a `"version"` key that holds the current schema version (`u32`).

Any change to the struct layout (adding, removing, or retyping a field) is a **breaking schema change** and requires a version bump and a migration path.

### Version history

| Version | Description |
|---------|-------------|
| 1       | Initial schema — `invoice_id`, `sme_address`, `amount`, `funding_target`, `funded_amount`, `yield_bps`, `maturity`, `status`, `version` |

### How versioning works

- `SCHEMA_VERSION` in `lib.rs` is the source of truth for the current schema.
- Every `init` call writes `SCHEMA_VERSION` into both the struct's `version` field and the `"version"` storage key.
- `get_version()` lets off-chain tooling (indexers, upgrade scripts) read the stored version before deciding whether to call `migrate`.

### Adding a new schema version (step-by-step)

1. **Bump `SCHEMA_VERSION`** in `lib.rs` (e.g. `1` to `2`).
2. **Keep the old struct** — add a `legacy` module (or a type alias like `InvoiceEscrowV1`) so the old bytes can still be deserialized.
3. **Add a migration arm** in `LiquifactEscrow::migrate`:
   ```rust
   if from_version == 1 {
       let old: InvoiceEscrowV1 = env.storage().instance()
           .get(&symbol_short!("escrow")).unwrap();
       let new = InvoiceEscrow {
           // spread old fields, default new ones
           new_field: default_value,
           version: 2,
           ..old.into()
       };
       env.storage().instance().set(&symbol_short!("escrow"), &new);
       env.storage().instance().set(&symbol_short!("version"), &2u32);
   }
   ```
4. **Write a test** in `test.rs` that manually writes the old struct bytes into storage and asserts the migrated state is correct.
5. **Gate `migrate` behind admin auth** before deploying to production (see security notes below).

### Deployment upgrade flow

```
1. Deploy new WASM (bump SCHEMA_VERSION, add migration arm)
2. Call get_version()  ->  confirm stored version == N
3. Call migrate(N)     ->  storage upgraded to N+1
4. Call get_version()  ->  confirm stored version == N+1
5. Resume normal operations
```

The contract rejects `migrate` calls that:
- Pass a `from_version` that does not match the stored version (prevents accidental double-migration).
- Pass a `from_version >= SCHEMA_VERSION` (already up to date).

### Security notes

- **Re-initialization guard** — `init` panics if the escrow is already initialized, preventing state overwrite.
- **`migrate` must be admin-gated in production** — the current implementation is open for testability. Before mainnet deployment, add `admin_address.require_auth()` at the top of `migrate` so only the contract deployer can trigger upgrades.
- **No silent data loss** — migration arms must explicitly handle every field. Defaulting a field to zero/false is intentional and must be documented in the version history table above.
- **Immutable history** — old migration arms should never be removed; they ensure any instance at any historical version can be brought forward step-by-step.

---

## Security & Authorization

Currently, the contract methods (`init`, `fund`, `settle`) **do not enforce authorization** via `require_auth()`. They rely solely on state-machine guards (e.g. checking if `status == 0` before funding).

> **Warning:** This represents an authentication gap. Any caller can trigger these functions. Negative tests have been added to track this gap and ensure proper exceptions are thrown when the contract is in an invalid state.

---


## Funding Constraints
- **Minimum Funding:** All funding amounts must be strictly greater than zero ($> 0$). 
- **Initialization:** Escrow creation will fail if the target amount is not positive.
- **Integer Safety:** Uses `checked_add` to prevent overflow during funded amount accounting.

---

## Security Assumptions

- Soroban runtime guarantees:
- Deterministic execution
- Storage integrity
- Token transfers handled externally
- Off-chain systems validate invoice authenticity

---

---

## Invariants

- `funded_amount <= funding_target` (soft enforced)
- `status transitions`: 0 → 1 → 2
- Cannot settle before funded
| Step | Command | Fails if… |
|------|---------|-----------|
| Format | `cargo fmt --all -- --check` | any file is not formatted |
| Build | `cargo build` | compilation error |
| Tests | `cargo test` | any test fails |
| Coverage | `cargo llvm-cov --features testutils --fail-under-lines 95` | line coverage < 95 % |

### Coverage gate

The pipeline uses [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) (installed via `taiki-e/install-action`) to measure line coverage and hard-fail the job when it drops below **95 %**.

To run the coverage check locally:

```bash
# Install once
cargo install cargo-llvm-cov

# Run (requires llvm-tools-preview component)
rustup component add llvm-tools-preview
cargo llvm-cov --features testutils --fail-under-lines 95 --summary-only
```

Keep formatting, tests, and coverage passing before opening a PR.

---

## Contributing

1. **Fork** the repo and clone your fork.
2. **Create a branch** from `main`: `git checkout -b feature/your-feature` or `fix/your-fix`.
3. **Setup**: ensure Rust stable is installed; run `cargo build` and `cargo test`.
4. **Make changes**:
   - Follow existing patterns in `escrow/src/lib.rs`.
   - Add or update tests in `escrow/src/test.rs`.
   - Format with `cargo fmt`.
5. **Verify locally**:
   - `cargo fmt --all -- --check`
   - `cargo build`
   - `cargo test --features testutils`
6. **Commit** with clear messages (e.g. `feat(escrow): X`, `test(escrow): Y`).
7. **Push** to your fork and open a **Pull Request** to `main`.
8. Wait for CI and address review feedback.

We welcome new contracts (e.g. settlement, tokenization helpers), tests, and docs that align with LiquiFact's invoice financing flow.

---

## Future Improvements

- Multi-escrow support
- Role-based access control
- Token integration
- Event emission
- Formal verification
