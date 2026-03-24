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

| Command                | Description                   |
| ---------------------- | ----------------------------- |
| `cargo build`          | Build all contracts           |
| `cargo test`           | Run unit tests                |
| `cargo fmt`            | Format code                   |
| `cargo fmt -- --check` | Check formatting (used in CI) |

---

## Project structure

```
liquifact-contracts/
├── Cargo.toml           # Workspace definition
├── escrow/
│   ├── Cargo.toml       # Escrow contract crate
│   └── src/
│       ├── lib.rs       # LiquiFact escrow contract (init, fund, settle)
│       └── test.rs      # Unit tests
├── docs/
│   ├── openapi.yaml     # OpenAPI 3.1 specification
│   ├── package.json     # Test runner deps (AJV, js-yaml)
│   └── tests/
│       └── openapi.test.js  # Schema conformance tests (51 cases)
└── .github/workflows/
    └── ci.yml           # CI: fmt, build, test
```

### Escrow contract (high level)

- **init** — Create an invoice escrow (admin, invoice id, SME address, amount, yield bps, maturity, **metadata_hash**). Requires `admin` authorization.
- **get_escrow** — Read current escrow state (no auth required).
- **get_metadata_hash** — Return the immutable SHA-256 hash anchored at `init` (no auth required).
- **fund** — Record investor funding; status becomes “funded” when target is met. Requires `investor` authorization.
- **settle** — Mark escrow as settled (buyer paid; investors receive principal + yield). Requires `sme_address` authorization.

### Authorization model

All sensitive state transitions are protected by Soroban's native [`require_auth`](https://developers.stellar.org/docs/smart-contracts/example-contracts/auth) mechanism.

| Function            | Required Signer | Rationale                                                  |
| ------------------- | --------------- | ---------------------------------------------------------- |
| `init`              | `admin`         | Prevents unauthorized escrow creation or re-initialization |
| `fund`              | `investor`      | Each investor authorizes their own contribution            |
| `settle`            | `sme_address`   | Only the SME beneficiary may trigger settlement            |
| `get_escrow`        | —               | Read-only; no state mutation                               |
| `get_metadata_hash` | —               | Read-only; no state mutation                               |

`require_auth` integrates with Soroban's authorization framework: on-chain, the transaction must carry a valid signature (or sub-invocation auth) from the required address. In tests, `env.mock_all_auths()` satisfies all checks so happy-path logic can be verified independently of key management.

#### Security assumptions

- The `admin` address is trusted to create legitimate escrows. Rotate or use a multisig address in production.
- Re-initialization is blocked at the contract level (`"Escrow already initialized"` panic) regardless of who calls `init`.
- `settle` can only move status from `1 → 2`; calling it on an open or already-settled escrow panics.

---

### Invoice metadata hash anchoring

The `metadata_hash` field in `InvoiceEscrow` is a 32-byte immutable commitment to the off-chain invoice document (PDF, JSON, etc.) anchored on-chain at escrow creation.

#### How it works

1. **Off-chain**: compute `SHA-256(invoice_document_bytes)` — use the raw bytes of the canonical JSON body or PDF file.
2. **On-chain**: pass the 32-byte digest to `init` as `metadata_hash`. The contract stores it and never overwrites it.
3. **Verification**: at any point, any party can recompute the SHA-256 of the document they hold and compare it to the value returned by `get_metadata_hash()`. A mismatch proves tampering.

#### Algorithm choice

| Property              | Detail                                                         |
| --------------------- | -------------------------------------------------------------- |
| Digest size           | 32 bytes (`BytesN<32>`)                                        |
| Recommended algorithm | SHA-256 (output is exactly 32 bytes)                           |
| Collision resistance  | 2^128 birthday-bound — adequate for document integrity         |
| Contract validation   | None — bytes are stored opaquely; callers choose the algorithm |

SHA-256 is strongly recommended. Weaker algorithms (MD5, SHA-1) **must not** be used.

#### Security notes

- The contract cannot verify the pre-image; it is the caller's responsibility to use a collision-resistant hash and canonical document serialization.
- No setter exists: `metadata_hash` cannot be changed after `init`, making the commitment immutable for the life of the escrow.
- A malicious admin could submit a hash that does not correspond to the real invoice. Use off-chain governance (e.g. multi-party attestation) to validate the hash before trusting it.

---

## API documentation (OpenAPI)

The REST API surface is documented in [`docs/openapi.yaml`](docs/openapi.yaml) (OpenAPI 3.1).

### Endpoints

| Method | Path                            | Auth | Description                        |
| ------ | ------------------------------- | ---- | ---------------------------------- |
| `GET`  | `/v1/health`                    | —    | Liveness probe                     |
| `GET`  | `/v1/info`                      | —    | API name, version, network         |
| `GET`  | `/v1/invoices`                  | JWT  | List invoice summaries (paginated) |
| `GET`  | `/v1/invoices/{invoiceId}`      | JWT  | Full escrow detail for one invoice |
| `POST` | `/v1/escrow`                    | JWT  | Initialise a new invoice escrow    |
| `POST` | `/v1/escrow/{invoiceId}/fund`   | JWT  | Record investor funding            |
| `POST` | `/v1/escrow/{invoiceId}/settle` | JWT  | Settle a funded escrow             |

### Security

- All mutating and data endpoints require a `Bearer` JWT in the `Authorization` header.
- `/health` and `/info` are public (no auth required).
- Stellar addresses are validated as 56-char base32 (`[A-Z2-7]`) strings.
- Monetary amounts are always in stroops (smallest unit); `amount ≥ 1` is enforced.
- `yield_bps` is capped at `10000` (100 %) to prevent overflow.

### Running the schema conformance tests

```bash
cd docs
npm install
npm test
# tests 51 | pass 51 | fail 0
```

---

## CI/CD

GitHub Actions runs on every push and pull request to `main`:

| Step     | Command                                                     | Fails if…                 |
| -------- | ----------------------------------------------------------- | ------------------------- |
| Format   | `cargo fmt --all -- --check`                                | any file is not formatted |
| Build    | `cargo build`                                               | compilation error         |
| Tests    | `cargo test`                                                | any test fails            |
| Coverage | `cargo llvm-cov --features testutils --fail-under-lines 95` | line coverage < 95 %      |

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
   - `cargo test`
6. **Commit** with clear messages (e.g. `feat(escrow): X`, `test(escrow): Y`).
7. **Push** to your fork and open a **Pull Request** to `main`.
8. Wait for CI and address review feedback.

We welcome new contracts (e.g. settlement, tokenization helpers), tests, and docs that align with LiquiFact’s invoice financing flow.

---

## License

MIT (see root LiquiFact project for full license).
