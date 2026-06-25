# LiquiFact Escrow (`liquifact_escrow`)

Soroban escrow for invoice funding, settlement, and investor claims. This README adds **formal invariant stubs** (machine-readable IDs plus math-style properties), **test traceability**, **attestation hashing**, **minimum contribution floors**, and **unique investor caps** (issues #102–#105).

## Deterministic Yield Calculation

The contract provides a dedicated helper function `calculate_principal_plus_yield(principal, yield_bps)` for computing payout amounts:

- **Formula**: `payout = principal + (principal × yield_bps) / 10_000`
- **Rounding**: Integer division truncates toward zero (floor for positive values), conservative for the contract
- **Overflow Protection**: Uses checked arithmetic with explicit panics on overflow
- **Validation**: Asserts principal ≥ 0 and yield_bps ∈ [0, 10_000]
- **Determinism**: Pure integer math ensures identical results across all platforms

### Usage Example

```rust
// 10,000 at 800 bps (8%) = 10,800
let payout = calculate_principal_plus_yield(10_000i128, 800i64);
assert_eq!(payout, 10_800i128);
```

### Security Properties

- No floating-point arithmetic (avoids precision issues)
- Input validation prevents invalid parameters
- Checked multiplication and addition prevent overflow
- Conservative rounding (truncation) protects contract solvency

## Formal invariant specification (stubs)

Intended for auditors, formal-methods tooling, and regression design. Properties are stated over escrow state unless noted. Status codes: `0=open`, `1=funded`, `2=settled`, `3=withdrawn`.

```yaml
schema_version: 5
invariants:
  - id: ESC-FUND-001
    name: funded_amount_monotone
    math: "forall funding txs in open status: funded_amount' = funded_amount + amount ∧ amount > 0"
    tests:
      - test::prop_funded_amount_non_decreasing
      - test::test_repeated_funding_accumulates_contribution

  - id: ESC-FUND-002
    name: funded_amount_upper_implicit
    math: "funded_amount = sum over investors of contribution(investor) while bookkeeping invariants hold"
    tests:
      - test::test_contributions_sum_equals_funded_amount
      - test::test_multiple_investors_tracked_independently

  - id: ESC-STA-001
    name: status_monotone
    math: "status never decreases; valid transitions 0→1→(2|3); 3 and 2 are terminal from 1"
    tests:
      - test::prop_status_only_increases
      - test::test_withdraw_funded_then_cannot_settle

  - id: ESC-CLM-001
    name: investor_claim_once
    math: "forall investor: InvestorClaimed(investor) set at most once after status=2"
    tests:
      - test::test_claim_investor_twice_panics
      - test::test_claim_succeeds_after_commitment_and_settle

  - id: ESC-ATT-001
    name: primary_attestation_single_set
    math: "PrimaryAttestationHash absent ∨ uniquely set; second bind_primary fails"
    tests:
      - test::test_bind_primary_attestation_single_set_and_get
      - test::test_bind_primary_attestation_twice_panics

  - id: ESC-ATT-002
    name: attestation_append_bounded
    math: "len(AttestationAppendLog) ≤ MAX_ATTESTATION_APPEND_ENTRIES"
    tests:
      - test::test_append_attestation_respects_max_length

  - id: ESC-MIN-001
    name: min_contribution_per_call
    math: "if min_floor > 0 then each fund amount ≥ min_floor"
    tests:
      - test::test_min_contribution_floor_rejects_below_and_accepts_equal
      - test::test_min_floor_applies_to_follow_on_fund

  - id: ESC-CAP-001
    name: unique_funder_cap
    math: "if cap = MaxUniqueInvestorsCap then #{investor : contribution(investor) > 0} ≤ cap"
    tests:
      - test::test_max_unique_investors_cap_enforced

  - id: ESC-INI-001
    name: single_initialization_guard
    math: "Initialized key set exactly once; subsequent init calls panic"
    tests:
      - test::test_double_init_panics
      - test::test_init_sets_initialized_flag
```

## New init parameters

`init(..., yield_tiers, min_contribution, max_unique_investors)`:

| Parameter | Type | Meaning |
|-----------|------|---------|
| `min_contribution` | `Option<i128>` | When `Some(x)`, requires every `fund` / `fund_with_commitment` amount `≥ x`, and `x ≤` initial `amount`. `None` disables the floor. |
| `max_unique_investors` | `Option<u32>` | When `Some(n)`, at most `n` distinct investor addresses may make a first deposit. `None` means unlimited. |

## Attestation API (off-chain bundle binding)

- **`bind_primary_attestation_hash(digest: BytesN<32>)`**: admin; **single-set** (immutable once stored).
- **`append_attestation_digest(digest)`**: admin; **append-only** log, capacity `MAX_ATTESTATION_APPEND_ENTRIES` (see `lib.rs`).
- **Frontrunning**: first finalized binding transaction wins for the primary slot; integrators should read on-chain state or events after finality.

## SME collateral commitment metadata

`record_sme_collateral_commitment` is SME-authenticated metadata only. It writes `SmeCollateralCommitment`, emits `CollateralRecordedEvt`, and does not move tokens, verify custody, reserve balances, or create an enforceable on-chain claim. Off-chain risk teams should follow [`docs/escrow-sme-collateral.md`](../docs/escrow-sme-collateral.md) before using the record in underwriting, monitoring, or reporting.

## Security review sign-off checklist (pre-deploy)

Use as a human gate; not a substitute for professional audit.

- [ ] `admin` is a multisig or governed contract (legal hold and attestation are admin-gated).
- [ ] Escrow has a **single-initialization guard** to prevent re-initialization after deployment.
- [ ] Funding token is standard SEP-41; fee-on-transfer tokens are out of scope (see module docs and `docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`).
- [ ] SME collateral records are labeled as reported metadata only and reviewed against [`docs/escrow-sme-collateral.md`](../docs/escrow-sme-collateral.md).
- [ ] `min_contribution` and `max_unique_investors` match the legal offering (floor vs. target; cap is per-address, not KYC’d entity).
- [ ] Attestation digests match the intended off-chain bundle (hash algorithm and canonical encoding documented off-chain).
- [ ] Maturity and claim-lock semantics use ledger time only (see `lib.rs` rustdoc).
- [ ] CI: `cargo fmt --all -- --check`, `cargo test`, `cargo llvm-cov --features testutils --fail-under-lines 95` pass.

## Developer UX: Build, Test, and Coverage Commands

### Prerequisites

- **Rust 1.70+ (stable)**: Required for Soroban contract development
- **Soroban CLI**: Optional for deployment, but recommended for local development
- **cargo-llvm-cov**: For coverage reporting (install via `cargo install cargo-llvm-cov`)
- **Docker**: Optional, for local Stellar network simulation

#### macOS/Linux Installation

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Add WASM target for Stellar/Soroban
rustup target add wasm32v1-none

# Install coverage tool
cargo install cargo-llvm-cov

# Install Soroban CLI (optional, for deployment)
cargo install soroban-cli

# Verify installation
rustc --version
cargo --version
rustup target list --installed | grep wasm32v1-none
```

#### Windows Notes

- Use the official Rust installer from https://rustup.rs/
- Commands are the same but use PowerShell or Command Prompt
- Docker Desktop recommended for local Stellar simulation

### Build Commands

#### Build WASM for Stellar/Soroban
```bash
# Build release WASM for deployment
cargo build --target wasm32v1-none --release

# Build for development/debug
cargo build --target wasm32v1-none

# Artifact location:
# target/wasm32v1-none/release/liquifact_escrow.wasm
```

#### Standard Rust Build
```bash
# Build the workspace
cargo build

# Build release version
cargo build --release

# Build specific package
cargo build -p liquifact_escrow
```

### Test Commands

#### Run All Tests
```bash
# Run all tests in workspace
cargo test

# Run escrow package tests specifically
cargo test -p liquifact_escrow

# Run tests with output
cargo test -p liquifact_escrow -- --nocapture

# Run specific test module
cargo test -p liquifact_escrow test::init
```

#### Run Tests by Feature Area
```bash
# Initialization tests
cargo test -p liquifact_escrow test::init::*

# Funding tests  
cargo test -p liquifact_escrow test::funding::*

# Settlement tests
cargo test -p liquifact_escrow test::settlement::*

# Admin tests
cargo test -p liquifact_escrow test::admin::*

# Property-based tests
cargo test -p liquifact_escrow test::properties::*
```

### Coverage Commands

#### Generate Coverage Report
```bash
# Full coverage with HTML report
cargo llvm-cov --features testutils --summary-only -p liquifact_escrow

# Coverage with minimum 95% threshold (CI standard)
cargo llvm-cov --features testutils --fail-under-lines 95 --summary-only -p liquifact_escrow

# Detailed HTML coverage report
cargo llvm-cov --features testutils --html -p liquifact_escrow

# Open HTML report in browser (after html command)
open target/llvm-cov/html/liquifact_escrow/index.html
```

#### Coverage by Test Module
```bash
# Coverage for specific test areas
cargo llvm-cov --features testutils --test test::init -p liquifact_escrow
cargo llvm-cov --features testutils --test test::funding -p liquifact_escrow
cargo llvm-cov --features testutils --test test::settlement -p liquifact_escrow
```

### Test Snapshots

#### Update Test Snapshots
```bash
# Update proptest regressions (if using proptest)
cargo test -p liquifact_escrow --features testutils -- --reset

# Re-run specific failing tests to update snapshots
cargo test -p liquifact_escrow test::prop_funded_amount_non_decreasing -- --exact
```

### Code Quality Commands

#### Formatting and Linting
```bash
# Format all code
cargo fmt --all

# Check formatting (CI requirement)
cargo fmt --all -- --check

# Run clippy linting
cargo clippy -p liquifact_escrow -- -D warnings

# Run clippy on entire workspace
cargo clippy --all-targets -- -D warnings
```

### Stellar CLI Integration

#### Environment Setup
```bash
# Set Stellar network (choose one)
export STELLAR_NETWORK="TESTNET"    # For testnet
export STELLAR_NETWORK="PUBLIC"    # For mainnet
export STELLAR_NETWORK="STANDALONE" # For local testing

# Set Soroban RPC URL
export SOROBAN_RPC_URL="https://soroban-testnet.stellar.org"

# Set deployer secret (for testing only)
export SOURCE_SECRET="S..."
```

#### Contract Deployment Commands
```bash
# Deploy contract (requires Soroban CLI)
soroban contract deploy \
  --wasm target/wasm32v1-none/release/liquifact_escrow.wasm \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK

# Invoke contract function
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK \
  -- function_name \
  --arg1 value1 \
  --arg2 value2
```

#### Local Stellar Network Simulation
```bash
# Start local Stellar network (requires Docker)
docker run -d -p 8000:8000 stellar/quickstart:latest

# Or use standalone mode for testing
cargo test -p liquifact_escrow --features testutils
```

### Development Workflow

#### Complete Development Cycle
```bash
# 1. Format and lint code
cargo fmt --all -- --check
cargo clippy -p liquifact_escrow -- -D warnings

# 2. Build and run tests
cargo build
cargo test -p liquifact_escrow

# 3. Check coverage
cargo llvm-cov --features testutils --fail-under-lines 95 --summary-only -p liquifact_escrow

# 4. Build WASM for deployment
cargo build --target wasm32v1-none --release
```

#### Quick Test Cycle
```bash
# Fast iteration: format, build, test
cargo fmt --all && cargo build && cargo test -p liquifact_escrow
```

### Troubleshooting

#### Common Issues
```bash
# If WASM target not found
rustup target add wasm32v1-none

# If tests fail with "testutils" feature missing
cargo test -p liquifact_escrow --features testutils

# If coverage tool not found
cargo install cargo-llvm-cov

# If Soroban CLI commands fail
cargo install soroban-cli
```

#### Performance Tips
```bash
# Run tests in parallel (default)
cargo test -p liquifact_escrow --release

# Run single-threaded for debugging
cargo test -p liquifact_escrow -- --test-threads=1

# Skip slow tests during development
cargo test -p liquifact_escrow -- --skip slow_test
```

### Security Assumptions (Token Economics)

This escrow contract makes explicit assumptions about external token contracts, documented in [`src/external_calls.rs`](src/external_calls.rs):

**Supported Tokens (In Scope):**
- Standard **SEP-41** tokens with no fee-on-transfer behavior
- Tokens where post-transfer balance deltas exactly match requested amounts
- Tokens that maintain balance conservation during transfers

**Unsupported Tokens (Out of Scope):**
- Fee-on-transfer tokens (taxes on transfers)
- Rebalancing or rebasing tokens
- Tokens with hooks that modify transfer amounts
- Malicious tokens that don't follow SEP-41 standards

**Safety Mechanisms:**
- Pre/post balance verification on all token transfers
- Assertions that fail safely on non-compliant tokens
- Treasury dust sweep only works with standard tokens

**Integration Responsibility:**
- Token contract verification must happen in the integration layer
- Governance should review and approve funding token choices
- Operational discipline required for balance reconciliation

### Platform Compatibility

#### macOS (Apple Silicon/Intel)
```bash
# All commands work natively
# Use Homebrew for dependencies if needed
brew install rust
```

#### Linux (Ubuntu/Debian/CentOS)
```bash
# Install system dependencies
sudo apt update && sudo apt install -y build-essential pkg-config libssl-dev

# All cargo commands work as documented
```

#### Windows
```bash
# Use PowerShell or Command Prompt
# Install Rust from https://rustup.rs/
# Consider WSL2 for better Linux compatibility
```

#### Docker Cross-Platform
```bash
# Use official Rust image for consistent builds
docker run --rm -v $(pwd):/workspace -w /workspace rust:latest cargo build
```

## CI / coverage

The GitHub Actions workflow runs format, build, tests, and **≥ 95% line coverage** via `cargo llvm-cov`.

Run these locally before pushing:

```bash
cargo fmt --all -- --check
cargo clippy -p liquifact_escrow -- -D warnings
cargo build --target wasm32v1-none --release -p liquifact_escrow
cargo test -p liquifact_escrow
cargo llvm-cov --features testutils --fail-under-lines 95 --summary-only -p liquifact_escrow
```

## Security review sign-off checklist (pre-deploy)

Use as a human gate; not a substitute for professional audit.

- [ ] `admin` is a multisig or governed contract (legal hold and attestation are admin-gated).
- [ ] Escrow has a **single-initialization guard** to prevent re-initialization after deployment.
- [ ] Funding token is standard SEP-41; fee-on-transfer tokens are out of scope (see module docs and `docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`).
- [ ] `min_contribution` and `max_unique_investors` match the legal offering (floor vs. target; cap is per-address, not KYC’d entity).
- [ ] Attestation digests match the intended off-chain bundle (hash algorithm and canonical encoding documented off-chain).
- [ ] Maturity and claim-lock semantics use ledger time only (see `lib.rs` rustdoc).
- [ ] `migrate` is understood to panic on all paths; redeploy policy is confirmed.
- [ ] CI: `cargo fmt --all -- --check`, `cargo test`, `cargo llvm-cov --features testutils --fail-under-lines 95` pass.

