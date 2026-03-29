# LiquiFact Escrow Contract

Soroban smart contracts for **LiquiFact** on Stellar. This repository contains the `escrow` crate: a single-instance invoice escrow with funding, optional SME collateral **records**, compliance **legal hold**, SME withdrawal, settlement, and per-investor accounting.

## Contract API (summary)

| Method | Purpose |
|--------|---------|
| `init` | Create escrow (admin auth). Sets `funding_target = amount`. Binds **`funding_token`**, **`treasury`**, optional **`registry`**, optional **`yield_tiers`**, optional **`min_contribution`** (per-call floor), optional **`max_unique_investors`** (cap on distinct funder addresses); validates **`invoice_id`** (length ≤ 32, charset `[A-Za-z0-9_]`). See [**`escrow/README.md`**](escrow/README.md) for formal invariant stubs and security checklist. |
| `get_escrow` / `get_version` / `get_legal_hold` | Read state. |
| `get_min_contribution_floor` / `get_max_unique_investors_cap` / `get_unique_funder_count` | Read optional per-call funding floor, optional unique-funder cap, and current distinct funder count. |
| `bind_primary_attestation_hash` / `get_primary_attestation_hash` | Admin **single-set** 32-byte digest for off-chain bundle binding. |
| `append_attestation_digest` / `get_attestation_append_log` | Admin **append-only** digest log (bounded length). |
| `get_funding_token` / `get_treasury` / `get_registry_ref` | Immutable funding asset, treasury for dust recovery, optional registry hint (`None` if unset at init). |
| `get_contribution` | Per-investor funded principal. |
| `update_funding_target` | Admin, open state only; target ≥ `funded_amount`. |
| `fund` | Investor auth; blocked while legal hold is active. First deposit fixes per-investor **effective yield** (base `yield_bps`) and clears claim lock unless set by `fund_with_commitment`. |
| `fund_with_commitment` | **First deposit only** for that investor when using a commitment window: sets **effective yield** from optional tier table and **`InvestorClaimNotBefore`** when `committed_lock_secs > 0`. Further amounts use `fund`. |
| `get_funding_close_snapshot` | [`Option`] of immutable close record when status first became **funded** (pro-rata denominator). |
| `get_investor_yield_bps` / `get_investor_claim_not_before` | Read per-investor tier outcome and claim lock. |
| `withdraw` | SME auth; funded → withdrawn; blocked under legal hold. |
| `settle` | SME auth; funded → settled (maturity gate if set); blocked under legal hold. |
| `claim_investor_payout` | Investor auth; after settle; blocked under legal hold. |
| `sweep_terminal_dust` | **Treasury auth only**; transfers capped [`MAX_DUST_SWEEP_AMOUNT`] of the bound token after **terminal** status (settled or withdrawn); blocked under legal hold. |
| `record_sme_collateral_commitment` | SME auth; **record-only** pledge (asset + amount + timestamp). |
| `get_sme_collateral_commitment` / `is_investor_claimed` | Reads. |
| `set_legal_hold` / `clear_legal_hold` | Admin governance (hold blocks risk-bearing transitions). |
| `update_maturity` | Admin, open state only. |
| `transfer_admin` | Admin rotation. |
| `migrate` | Version guardrails (see upgrade policy below). |

### Per-instance funding asset and registry (issues #113, #116)

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
- **`funding_token`** and **`treasury`** are stored under `DataKey::FundingToken` / `DataKey::Treasury` and are **immutable** after `init` (no setter).
- **`registry`** is optional: when provided, it is stored under `DataKey::RegistryRef`; if omitted, that key is absent and `get_registry_ref` returns `None`. The registry id is a **read-only hint for indexers** — it does **not** grant this escrow any privilege and must not be treated as an on-chain source of truth without calling the registry contract directly (avoid static “call loops” that assume mutual authority).

### Invoice id validation (issue #118)

Off-chain invoice slugs should match the same rules enforced in `init`: non-empty, length ≤ 32 (Soroban `Symbol` maximum), characters in `[A-Za-z0-9_]` only (SEP-style slugs; no spaces, punctuation, or Unicode).

### Cross-contract token safety (issue #108)

`sweep_terminal_dust` delegates the SEP-41 `transfer` to [`escrow/src/external_calls.rs`](escrow/src/external_calls.rs), which records **sender and recipient balances before/after** and requires exact `amount` deltas. Only [`DataKey::FundingToken`] is used for this path (trust list in module rustdoc). Soroban does not exhibit classic EVM-style synchronous reentrancy into this contract mid-transfer; token implementations are still treated as adversarial for **balance correctness**. Unsupported token economics (fee-on-transfer) should trip assertions.

### Ledger time boundaries (issue #106)

**Risk:**

- `funded_amount += amount` may overflow `i128`
There is no separate on-chain **funding deadline** or **grace-period** field beyond **maturity** on [`InvoiceEscrow`] and optional **per-investor claim locks** from `fund_with_commitment`. Tests exercise off-by-one behavior around maturity (`now >= maturity`) and exact **funded** transitions (`funded_amount >= funding_target`). Integrators should assume **ledger timestamp skew** across validators matches Stellar norms and test boundary predicates accordingly.

### Optional tiered yield (issue #110)

When `init` receives a non-empty `yield_tiers` vector, tiers must have strictly increasing `min_lock_secs`, non-decreasing `yield_bps`, and every tier `yield_bps >=` base `yield_bps`. `fund_with_commitment(investor, amount, committed_lock_secs)` selects the best matching tier on the **first** deposit only; **`tier selection is immutable after that investor’s first leg`** (additional principal uses `fund` at the stored effective yield). **Rounding:** yields are integer basis points only; currency rounding for coupon cash flows belongs off-chain.

### Funding-close snapshot (issue #117)

On the first transition to **funded**, the contract persists [`FundingCloseSnapshot`](escrow/src/lib.rs): `total_principal` equals `funded_amount` at that instant (so **overfunding is absorbed** in the snapshot total), `funding_target`, and ledger timestamp/sequence. The snapshot is **immutable**; deterministic pro-rata uses `get_contribution(investor) / total_principal` with rational math off-chain.

### Treasury dust sweep (issue #107)

**Risks:**

- Negative funding
- Zero funding
- Invalid maturity

| Command                | Description                                                |
| ---------------------- | ---------------------------------------------------------- |
| `cargo build`          | Build all contracts                                        |
| `cargo test`           | Run unit tests and property-based tests (using `proptest`) |
| `cargo fmt`            | Format code                                                |
| `cargo fmt -- --check` | Check formatting (used in CI)                              |
`sweep_terminal_dust(amount)` moves `min(amount, balance, MAX_DUST_SWEEP_AMOUNT)` of the **bound** funding token from the escrow contract to **`treasury`**, using the safety wrapper above. It is only callable in **status 2 (settled)** or **status 3 (withdrawn)** so **open** or **funded** escrows cannot be drained as “dust.” Legal hold blocks sweeps. **`fund` does not move tokens** in this version; if custodial flows are added later, token balances must stay reconciled with ledger fields so sweeps cannot pull user principal. Tokens sent to this contract in **other** assets are not touched by this hook.

### Optional SME collateral (record-only)

`record_sme_collateral_commitment` stores a [`SmeCollateralCommitment`](escrow/src/lib.rs) under `DataKey::SmeCollateralPledge`. It does **not** lock tokens on-chain or trigger liquidation. Indexers should treat it as a disclosure field for future enforcement hooks; **no false liquidation** is possible from this field alone because no asset movement or status transition depends on it.

### Legal / compliance hold

When `DataKey::LegalHold` is true, the contract rejects new `fund`, `settle`, SME `withdraw`, and `claim_investor_payout`. Only the stored **admin** may set or clear the hold. **Emergency policy:** there is no separate break-glass entrypoint; recovery is via governed `admin` (multisig / DAO). Document operational playbooks off-chain so holds cannot strand funds without governance.

### Storage keys (`DataKey`)

Public enum in [`escrow/src/lib.rs`](escrow/src/lib.rs): `Escrow`, `Version`, `InvestorContribution(Address)`, `LegalHold`, `SmeCollateralPledge`, `InvestorClaimed(Address)`, `FundingToken`, `Treasury`, `RegistryRef` (present only when set at init), optional `YieldTierTable`, `FundingCloseSnapshot`, `InvestorEffectiveYield(Address)`, `InvestorClaimNotBefore(Address)`, `MinContributionFloor`, `MaxUniqueInvestorsCap` (when capped at init), `UniqueFunderCount`, `PrimaryAttestationHash`, `AttestationAppendLog`. New optional keys should keep **additive** names and avoid reusing or repurposing existing variants.

| Parameter   | Constraints                                  |
| ----------- | -------------------------------------------- |
| `_investor` | Investor's Stellar address (for audit trail) |
| `amount`    | > 0 recommended; partial funding is allowed  |
---

## Storage-only upgrade policy (additive fields)

**Compatible without redeploy** when you only:

| Condition                 | Behaviour                               |
| ------------------------- | --------------------------------------- |
| `status != 0`             | Panics: `"Escrow not open for funding"` |
| `init` not called         | Panics: `"Escrow not initialized"`      |
| `funded_amount` overflows | Rust panics (debug) / wraps (release)   |
- Add **new** `DataKey` variants and/or new `#[contracttype]` structs stored under **new** keys.
- Read new keys with `.get(...).unwrap_or(default)` so missing keys behave as “unset” on old deployments.

**Requires new deployment or explicit migration** when you:

- Change layout or meaning of an existing stored type (e.g. new required field on `InvoiceEscrow` without a migration that rewrites `DataKey::Escrow`).
- Rename or change the XDR shape of an existing `DataKey` variant used in production.

**Compatibility test plan (short):**

1. Deploy version _N_; exercise `init`, `fund`, `settle`.
2. Deploy version _N+1_ with only new optional keys; repeat flows; assert old instances still readable.
3. If `InvoiceEscrow` changes, add a migration test or document mandatory redeploy.

| Category       | Tag        | What is covered                                                                                                                    |
| -------------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Happy path     | `[HAPPY]`  | Full lifecycle, field persistence, `get_escrow` consistency                                                                        |
| Auth           | `[AUTH]`   | `require_auth` recorded for admin / investor / SME; panics without auth                                                            |
| State          | `[STATE]`  | Double-init, fund-after-funded, fund-after-settled, settle-when-open, double-settle                                                |
| Uninitialized  | `[UNINIT]` | `get_escrow`, `fund`, `settle` all panic before `init`                                                                             |
| Boundary       | `[BOUND]`  | `amount=1`, `amount=i128::MAX`, `yield_bps=i64::MAX`, `maturity=0`, `maturity=u64::MAX`, overshoot funding, exact-boundary funding |
| Repeated calls | `[REPEAT]` | Multiple investors accumulate correctly; `get_escrow` is idempotent                                                                |
`migrate` today validates `from_version` against stored `DataKey::Version` and errors if no path is implemented.

### `DataKey` naming convention

Use PascalCase variant names matching persisted role (`LegalHold`, `SmeCollateralPledge`). Per-address maps use wrapper variants: `InvestorContribution(Address)`, `InvestorClaimed(Address)`.

---

## Release runbook: build, deploy, verify

**Who may deploy production:** only addresses and keys owned by LiquiFact governance (multisig / custody). Treat contract admin and deployer secrets as **highly sensitive**.

| Version | Description                                                                                                                             |
| ------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| 1       | Initial schema — `invoice_id`, `sme_address`, `amount`, `funding_target`, `funded_amount`, `yield_bps`, `maturity`, `status`, `version` |
### Environment variables (example)

| Variable | Purpose |
|----------|---------|
| `STELLAR_NETWORK` | e.g. `TESTNET` / `PUBLIC` / custom Horizon passphrase |
| `SOROBAN_RPC_URL` | Soroban RPC endpoint |
| `SOURCE_SECRET` | Funding / deployer Stellar secret key (S ...) |
| `LIQUIFACT_ADMIN_ADDRESS` | Initial admin intended to control holds and funding target |

Exact CLI flags change between Soroban releases; always cross-check [Stellar Soroban docs](https://developers.stellar.org/docs/tools/soroban-cli/stellar-cli) for your installed `stellar` / `soroban` CLI version.

### Build WASM

```bash
rustup target add wasm32v1-none
cargo build --target wasm32v1-none --release
# Artifact (typical):
# target/wasm32v1-none/release/liquifact_escrow.wasm
```

### Deploy (example flow)

```bash
stellar contract deploy \
  --wasm target/wasm32v1-none/release/liquifact_escrow.wasm \
  --source-account "$SOURCE_SECRET" \
  --network "$STELLAR_NETWORK" \
  --rpc-url "$SOROBAN_RPC_URL"
# Record emitted contract id as LIQUIFACT_ESCROW_CONTRACT_ID
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

Initialize on-chain with `init` via `stellar contract invoke` (pass `admin`, **`invoice_id` as string**, `sme_address`, amounts, `yield_bps`, `maturity`, **`funding_token`**, **`registry`** as optional address, **`treasury`**, **`yield_tiers`** as optional vector per your product).

### Verify artifact hash

```bash
shasum -a 256 target/wasm32v1-none/release/liquifact_escrow.wasm
```

Store the digest in release notes and inject the same WASM into verification tooling (block explorer, internal registry). After deployment, confirm the **on-chain contract code hash** matches the audited artifact for that release tag.

### Backend / config registration

- Persist `LIQUIFACT_ESCROW_CONTRACT_ID` (and network passphrase) in the backend’s secure config.
- Rollback: **cannot** undeploy a contract; rollback is *forward-only*: deploy a new contract id, point new traffic to it, and sunset the old id. Document state replication needs if invoices were already bound to the old id.

---

## Funding Constraints

- **Minimum Funding:** All funding amounts must be strictly greater than zero ($> 0$).
- **Initialization:** Escrow creation will fail if the target amount is not positive.
- **Integer Safety:** Uses `checked_add` to prevent overflow during funded amount accounting.

---
## Local development and CI

| Step | Command |
|------|---------|
| Format | `cargo fmt --all -- --check` |
| Build | `cargo build` |
| Test | `cargo test` |
| Coverage (≥ 95% lines in CI) | `cargo llvm-cov --features testutils --fail-under-lines 95 --summary-only` |

Install coverage tools:

```bash
cargo install cargo-llvm-cov
rustup component add llvm-tools-preview
```

---

## Token integration security checklist

See [`docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`](docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md) for the supported token assumptions, explicit unsupported token warnings, and the integration-layer responsibilities required when this escrow contract interacts with external token contracts.

- `funded_amount <= funding_target` (soft enforced)
- `status transitions`: 0 → 1 → 2
- Cannot settle before funded
  | Step | Command | Fails if… |
  |------|---------|-----------|
  | Format | `cargo fmt --all -- --check` | any file is not formatted |
  | Build | `cargo build` | compilation error |
  | Tests | `cargo test` | any test fails |
  | Coverage | `cargo llvm-cov --features testutils --fail-under-lines 95` | line coverage < 95 % |
## Security notes

- **Auth:** state-changing entrypoints use `require_auth()` for the appropriate role (admin, SME, investor, **treasury** for dust sweep).
- **Legal hold:** is governance-controlled; misuse risk is mitigated by using a multisig `admin` and operational policy.
- **Collateral record:** is not proof of encumbrance until a future version explicitly enforces token transfers.
- **Token integration:** external token transfers and token safety validation must live in the integration layer; this contract stores only numeric amount state and collateral metadata.
- **Overflow:** `fund` uses `checked_add` on `funded_amount`.
- **Dust sweep:** gated on **terminal** escrow status, per-call **cap** ([`MAX_DUST_SWEEP_AMOUNT`]), actual **balance**, **legal hold**, and **treasury** auth; only the **configured** SEP-41 token is transferred, with **post-transfer balance equality** checks in [`external_calls`](escrow/src/external_calls.rs). Wrong-asset or oversized balances still require operational discipline — the hook is not a general-purpose withdrawal for live liabilities.
- **Tiered yield / claim locks:** first-deposit discipline (`fund` vs `fund_with_commitment`) prevents changing an investor’s tier after their initial leg; claim timestamps are ledger-based.
- **Funding snapshot:** single-write immutability avoids shifting pro-rata denominators after close.
- **Registry ref:** stored for discoverability only; it must not be used as an authority without verifying behavior of the registry contract off-chain or in a dedicated integration.

### Contract type clone/derive safety

- `DataKey` keeps `Clone` because key wrappers are reused for storage get/set paths.
- `InvoiceEscrow` and `SmeCollateralCommitment` intentionally do **not** derive `Clone`; this prevents accidental full-state duplication in hot paths.
- `InvoiceEscrow` and `SmeCollateralCommitment` derive `PartialEq` for deterministic state assertions in tests and `Debug` for failure diagnostics.
- `init` publishes `EscrowInitialized` from stored state instead of cloning the in-memory escrow snapshot, reducing avoidable copy overhead.

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

## Practical Integration Examples

These examples demonstrate the core lifecycle of the LiquiFact Escrow contract. For a full executable setup, see [escrow/src/examples.rs](file:///Users/kanas/Desktop/opensource/Liquifact-contracts/escrow/src/examples.rs).

### 1. Initialize Escrow (Admin)

Platform admins create the escrow for a specific invoice and SME beneficiary.

```rust
let amount = 1_000_000i128; // Smallest unit
let yield_bps = 1000i64;    // 10.00%
let maturity = 1750000000;  // Unix timestamp

client.init(
    &admin,
    &symbol_short!("INV001"),
    &sme_address,
    &amount,
    &yield_bps,
    &maturity
);
```

### 2. Fund Escrow (Investor)

Investors contribute funds. Status flips to `1` (Funded) once the target is met.

```rust
client.fund(&investor_address, &500_000i128);
```

### 3. Settle Invoice (SME/Admin)

After the buyer pays (off-chain), the SME marks the escrow as settled to release funds.

```rust
// Total due includes the 10% yield
client.settle(&1_100_000i128);
```

### 4. Claim Payout (Investor)

Investors claim their principal and proportional yield.

```rust
let total_payout = client.claim(&investor_address);
// Returns 1,100,000 for a 1M contribution at 10%
```

---

## Technical Flow Diagram

```mermaid
sequence_flow
    Admin->>Contract: init()
    Investor->>Contract: fund()
    Note over Investor, Contract: Status: Open -> Funded
    SME->>Contract: withdraw()
    Note over SME, Contract: Status: Funded -> Withdrawn
    Buyer-->>SME: Payment (Off-chain)
    SME->>Contract: settle()
    Note over SME, Contract: Status: Withdrawn -> Settled
    Investor->>Contract: claim()
    Note over Investor, Contract: Payout: Principal + Yield
```

---

## Future Improvements

- Multi-escrow support
- Role-based access control
- Token integration
- Event emission
- Formal verification
1. Branch from `main`.
2. Run `cargo fmt`, `cargo test`, and the coverage command above before pushing.
3. Keep README and rustdoc aligned with `escrow/src/lib.rs` behavior.
