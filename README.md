# LiquiFact Escrow Contracts

Soroban smart contracts for LiquiFact, the invoice liquidity network on Stellar.
This repository contains the `escrow` contract that holds investor funds for
tokenized invoices until settlement.

---

## Prerequisites

- Rust 1.70+ (stable)
- `wasm32v1-none` target for WASM builds: `rustup target add wasm32v1-none`
- Soroban / Stellar CLI (optional — for deployment and contract interaction)

For local development and CI, Rust alone is sufficient.

---

## Quick start

```bash
cargo build
cargo test
```

---

## Schema version changelog (`DataKey::Version`)

The `SCHEMA_VERSION` constant in `escrow/src/lib.rs` is stored on-chain under
`DataKey::Version` at [`init`] and is the authoritative version for upgrade
decisions. All production instances should have this value match the deployed
WASM.

| Version | Description | Upgrade path |
|---------|-------------|--------------|
| 1 | Initial schema (`InvoiceEscrow` v1, basic funding / settle) | N/A |
| 2 | Added per-investor yield keys (`InvestorEffectiveYield`, `InvestorClaimNotBefore`) | Additive keys — no `migrate` call required for read compatibility |
| 3 | Added `FundingCloseSnapshot`, `MinContributionFloor`, `MaxUniqueInvestorsCap`, `UniqueFunderCount` | Additive keys — old instances return `None` / `0` defaults |
| 4 | Added attestation API (`PrimaryAttestationHash`, `AttestationAppendLog`) | Additive keys — no `migrate` call required |
| 5 | Added `YieldTierTable` (`fund_with_commitment`), `RegistryRef`, `Treasury`; tightened `InvoiceEscrow` layout | **Redeploy required** if `InvoiceEscrow` struct layout differs from stored XDR |

> **Current:** `SCHEMA_VERSION = 5`

---

## Storage-only upgrade policy (additive fields)

**Compatible without redeploy** when you only:

- Add **new** `DataKey` variants and/or new `#[contracttype]` structs stored
  under **new** keys.
- Read new keys with `.get(...).unwrap_or(default)` so missing keys behave as
  "unset" on old deployments.

**Requires new deployment or explicit migration** when you:

- Change the layout or XDR shape of an existing stored type (e.g. add a
  required field to `InvoiceEscrow` without a migration that rewrites
  `DataKey::Escrow`).
- Rename or change the XDR shape of an existing `DataKey` variant used in
  production.

### `migrate` entrypoint — explicit panic semantics

`LiquifactEscrow::migrate(from_version)` **panics in all current cases**.
There is **no silent migration path** from any prior version to version 5.
Callers must not assume it will do bookkeeping work:

| Condition | Panic message |
|-----------|---------------|
| `stored != from_version` | `"from_version does not match stored version"` |
| `from_version >= SCHEMA_VERSION` | `"Already at current schema version"` |
| Any `from_version < SCHEMA_VERSION` | `"No migration path from version {N} — extend migrate or redeploy"` |

To add a real migration path (e.g. rewrite `DataKey::Escrow` after a struct
field change), implement the transformation inside `migrate` before the final
`panic!` and update `DataKey::Version`.

### `DataKey` naming convention

| Rule | Example |
|------|---------|
| PascalCase enum variant | `DataKey::FundingToken` |
| Per-address variants use tuple form | `DataKey::InvestorContribution(Address)` |
| New variants must be additive (no rename of existing) | — |

### Compatibility test plan (short)

1. Deploy version _N_; exercise `init`, `fund`, `settle`.
2. Deploy version _N+1_ with only new optional keys; repeat flows; assert old
   instances still readable.
3. If `InvoiceEscrow` changes, add a migration test **or** document mandatory
   redeploy.

See [`docs/OPERATOR_RUNBOOK.md`](docs/OPERATOR_RUNBOOK.md) for the full
redeploy-vs-upgrade decision tree and Stellar/Soroban CLI examples.

---

## Release runbook: build, deploy, verify

**Who may deploy production:** only addresses and keys owned by LiquiFact
governance (multisig / custody). Treat contract admin and deployer secrets as
**highly sensitive**.

See [`docs/OPERATOR_RUNBOOK.md`](docs/OPERATOR_RUNBOOK.md) for the step-by-step
runbook including pre-flight checklists, rollback protocol, and legal hold
coordination.

### Environment variables (example)

| Variable | Purpose |
|----------|---------|
| `STELLAR_NETWORK` | e.g. `testnet` / `mainnet` / custom network passphrase |
| `SOROBAN_RPC_URL` | Soroban RPC endpoint |
| `SOURCE_SECRET` | Funding / deployer Stellar secret key (`S...`) |
| `LIQUIFACT_ADMIN_ADDRESS` | Initial admin intended to control holds and funding target |

Exact CLI flags change between Soroban releases; always cross-check the
[Stellar Soroban docs](https://developers.stellar.org/docs/tools/soroban-cli/stellar-cli)
for your installed `stellar` CLI version.

### Build WASM

```bash
rustup target add wasm32v1-none
cargo build --target wasm32v1-none --release -p liquifact_escrow
# Artifact (typical):
# target/wasm32v1-none/release/liquifact_escrow.wasm
```

### Lint

```bash
# Escrow crate only (mirrors CI)
cargo clippy -p liquifact_escrow -- -D warnings

# Entire workspace
cargo clippy --all-targets -- -D warnings
```

---

## Escrow contract — public entrypoints

| Entrypoint | Description |
|------------|-------------|
| `init` | Create an invoice escrow; binds funding token, treasury, optional registry. |
| `fund` | Record investor principal; marks escrow funded when target is met. |
| `fund_with_commitment` | First deposit with optional lock period; selects tiered yield. |
| `settle` | Mark a funded escrow as settled (SME auth required; maturity enforced). |
| `withdraw` | SME pulls funded liquidity (accounting record). |
| `claim_investor_payout` | Investor records a payout claim after settlement. |
| `sweep_terminal_dust` | Treasury sweeps rounding residue from a terminal escrow. |
| `migrate` | Schema version gate — **panics on all paths** in the current release. |
| `set_legal_hold` | Admin activates/clears compliance hold. |
| `bind_primary_attestation_hash` | Admin sets a single-write 32-byte digest. |
| `append_attestation_digest` | Admin appends to bounded audit log. |
| `record_sme_collateral_commitment` | SME records collateral pledge (metadata only). |
| `get_escrow` | Read current escrow state. |
| `get_version` | Read stored `DataKey::Version`. |

---

## Storage guardrails

The escrow stores per-investor contribution entries inside the contract
instance. That map is intentionally bounded.

- Supported investor cardinality: configured via `max_unique_investors` at
  `init` (optional cap); no hard-coded global max since investor cardinality
  is escrow-specific.
- Attestation append log: bounded at `MAX_ATTESTATION_APPEND_ENTRIES = 32`.
- Dust sweep: capped at `MAX_DUST_SWEEP_AMOUNT = 100_000_000` base units per
  call.

---

## Test organization

Escrow tests are organized by feature area under
[`escrow/src/test/`](escrow/src/test):

| File | Coverage area |
|------|--------------|
| `init.rs` | Initialization, invoice-id validation, getters, init-shaped baselines |
| `funding.rs` | Funding, contribution accounting, snapshots, tier selection |
| `settlement.rs` | Settlement, withdrawal, investor claims, maturity boundaries, dust sweep |
| `admin.rs` | Admin-governed state changes, legal hold, migration guards, collateral metadata |
| `integration.rs` | External token-wrapper assumptions, metadata-only integration checks |
| `properties.rs` | Proptest-based invariants |

Shared helpers live in [`escrow/src/test.rs`](escrow/src/test.rs). Each test
creates its own fresh `Env` so feature modules do not rely on hidden
cross-test state.

---

## Architecture Decision Records

Core design decisions are captured in [`docs/adr/`](docs/adr/):

| ADR | Decision |
|-----|---------|
| [ADR-001](docs/adr/ADR-001-state-model.md) | Escrow state model (`status` 0–3, forward-only transitions) |
| [ADR-002](docs/adr/ADR-002-auth-boundaries.md) | Authorization boundaries per role (admin, SME, investor, treasury) |
| [ADR-003](docs/adr/ADR-003-settlement-flow.md) | Two-phase settlement flow and funding-close snapshot |
| [ADR-004](docs/adr/ADR-004-legal-hold.md) | Legal / compliance hold mechanism |
| [ADR-005](docs/adr/ADR-005-tiered-yield.md) | Optional tiered yield and per-investor commitment locks |
| [ADR-006](docs/adr/ADR-006-dust-sweep-and-token-safety.md) | Treasury dust sweep and SEP-41 token safety wrapper |

---

## Token integration security checklist

See [`docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`](docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md)
for supported token assumptions, explicit unsupported token warnings, and the
integration-layer responsibilities required when this contract interacts with
external token contracts.

---

## SME collateral metadata

See [`docs/escrow-sme-collateral.md`](docs/escrow-sme-collateral.md) for the risk-team handling rules for `record_sme_collateral_commitment` and `CollateralRecordedEvt`. The record is SME-reported metadata only; it is not proof of custody, token movement, or an enforceable on-chain claim.

## Security notes

- **Auth:** state-changing entrypoints use `require_auth()` for the
  appropriate role (admin, SME, investor, **treasury** for dust sweep).
- **Legal hold:** governance-controlled; misuse risk is mitigated by using a
  multisig `admin` and operational policy (see
  [`docs/OPERATOR_RUNBOOK.md`](docs/OPERATOR_RUNBOOK.md)).
- **Collateral record:** SME-reported metadata only; not proof of custody,
  token movement, reserved balance, or an enforceable on-chain claim.
- **Token integration:** fee-on-transfer, rebasing, and hook tokens are
  **explicitly out of scope**. Post-transfer balance-equality checks in
  [`external_calls`](escrow/src/external_calls.rs) will `panic!` (safe
  failure) on non-compliant tokens.
- **Overflow:** `fund` uses `checked_add` on `funded_amount`.
- **Dust sweep:** gated on terminal escrow status, per-call cap
  (`MAX_DUST_SWEEP_AMOUNT`), actual balance, legal hold, and treasury auth;
  only the configured SEP-41 token is transferred with post-transfer balance
  equality checks.
- **Tiered yield / claim locks:** first-deposit discipline prevents changing
  an investor's tier after their initial leg; claim timestamps are ledger-based.
- **Funding snapshot:** single-write immutability avoids shifting pro-rata
  denominators after close.
- **Registry ref:** stored for discoverability only; must not be used as
  authority without verifying the registry contract independently.
- **migrate:** panics on all paths in the current release — no silent
  migration work is performed.

### Contract type clone/derive safety

- `DataKey` keeps `Clone` because key wrappers are reused for storage
  get/set paths.
- `InvoiceEscrow` and `SmeCollateralCommitment` intentionally do **not**
  derive `Clone`; this prevents accidental full-state duplication in hot paths.
- `InvoiceEscrow` and `SmeCollateralCommitment` derive `PartialEq` for
  deterministic state assertions in tests and `Debug` for failure diagnostics.
- `init` publishes `EscrowInitialized` from stored state instead of cloning
  the in-memory escrow snapshot, reducing avoidable copy overhead.

---

## CI

Run these before opening a PR:

```bash
cargo fmt --all -- --check
cargo clippy -p liquifact_escrow -- -D warnings
cargo build
cargo test
cargo llvm-cov --features testutils --fail-under-lines 95 --summary-only -p liquifact_escrow
```

### Cargo.lock process notes

- Keep `Cargo.lock` committed and reviewed for every dependency change.
- For routine updates, use a dedicated dependency branch and include lockfile diff context in PR.
- For emergency advisory bumps, prioritize minimal version movement and full regression checks.
- After any lockfile update, re-run the full CI command set above before merge.
- Dependency policy, cadence, and emergency workflow are documented in
  [`docs/escrow-dependency-policy.md`](docs/escrow-dependency-policy.md).

---

## Contributing

MIT
