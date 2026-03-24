# LiquiFact Contracts

Soroban smart contracts for **LiquiFact** — the global invoice liquidity network on Stellar. This repo contains the **escrow** contract that holds investor funds for tokenized invoices until settlement.

Part of the LiquiFact stack: **frontend** (Next.js) | **backend** (Express) | **contracts** (this repo).

---

## Prerequisites

- **Rust** 1.70+ (stable)
- **Soroban CLI** (optional, for deployment): [Stellar Soroban docs](https://developers.stellar.org/docs/smart-contracts/getting-started/soroban-cli)

For CI and local checks you only need Rust and `cargo`.

---

## Setup

1. **Clone the repo**
   ```bash
   git clone <this-repo-url>
   cd liquifact-contracts
   ```
2. **Build**
   ```bash
   cargo build
   ```
3. **Run tests**
   ```bash
   cargo test
   ```

---

## Development

| Command                    | Description                       |
|----------------------------|-----------------------------------|
| `cargo build`              | Build all contracts               |
| `cargo test`               | Run unit tests                    |
| `cargo fmt`                | Format code                       |
| `cargo fmt -- --check`     | Check formatting (used in CI)     |

---

## Project structure

```
liquifact-contracts/
├── Cargo.toml           # Workspace definition
├── escrow/
│   ├── Cargo.toml       # Escrow contract crate
│   └── src/
│       ├── lib.rs       # LiquiFact escrow contract (init, fund, settle, version)
│       └── test.rs      # Unit tests (≥ 95 % coverage)
└── .github/workflows/
    └── ci.yml           # CI: fmt, build, test
```

### Escrow contract (high level)

| Method        | Description                                                                 |
|---------------|-----------------------------------------------------------------------------|
| `version`     | **Read-only.** Returns semantic version string (`"MAJOR.MINOR.PATCH"`).    |
| `init`        | Create an invoice escrow (invoice id, SME address, amount, yield bps, maturity). |
| `get_escrow`  | Read current escrow state.                                                  |
| `fund`        | Record investor funding; status becomes `Funded` when target is met.       |
| `settle`      | Mark escrow as settled (buyer paid; investors receive principal + yield).   |

---

## Contract Version Introspection (`version`)

### Overview

`EscrowContract::version(&env)` is a **pure, read-only** method that returns the
semantic version of the compiled contract WASM binary as a `SorobanString`.

```rust
let env = Env::default();
let version: SorobanString = EscrowContract::version(&env);
assert_eq!(version.to_string(), "1.0.0");
```

### Version semantics

| Segment | Meaning                                                              |
|---------|----------------------------------------------------------------------|
| MAJOR   | Breaking change to the public interface or on-chain storage layout   |
| MINOR   | Backwards-compatible new functionality                               |
| PATCH   | Backwards-compatible bug-fix or documentation change only            |

### Why this matters

- **Tooling & indexers** can call `version()` before any interaction and fail
  fast on an incompatible version range.
- **Migration scripts** must re-read the version after a WASM upgrade to detect
  storage-layout changes (MAJOR bump).
- **Monitoring** can alert when a newly deployed binary carries an unexpected
  version string.

### Security properties

| Property            | Detail                                                                    |
|---------------------|---------------------------------------------------------------------------|
| No state mutation   | Safe to call from any context; cannot trigger side-effects.               |
| No auth required    | Purely informational; any caller may invoke it.                           |
| Tamper-resistant    | Value is a compile-time constant embedded in the WASM binary; it cannot be changed without redeployment. |

### Upgrade workflow

1. Bump `CONTRACT_VERSION` in `escrow/src/lib.rs`.
2. Run `cargo fmt && cargo test` — all tests must pass.
3. Deploy the new WASM binary.
4. Tooling calls `version()` on the live contract to confirm the upgrade.

---

## CI/CD

GitHub Actions runs on every push and pull request to `main`:

- **Format** — `cargo fmt --all -- --check`
- **Build** — `cargo build`
- **Tests** — `cargo test`

Keep formatting and tests passing before opening a PR.

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
   - `cargo test`
6. **Commit** with clear messages (e.g. `feat(escrow): X`, `test(escrow): Y`).
7. **Push** to your fork and open a **Pull Request** to `main`.
8. Wait for CI and address review feedback.

We welcome new contracts (e.g. settlement, tokenization helpers), tests, and docs that align with LiquiFact's invoice financing flow.

---

## License

MIT (see root LiquiFact project for full license).