# Escrow Simulation: Stellar CLI Recipes

> **Scope:** Local / standalone validator simulation only. These recipes are **not** production
> deployment instructions. Production deployments require maintainer-controlled secrets and
> governance procedures described in the repository README.
>
> **CLI version:** Stellar CLI v22+. Flag names and XDR encoding may differ on older releases.
> Always cross-check the [Stellar Soroban CLI docs](https://developers.stellar.org/docs/tools/soroban-cli/stellar-cli)
> for your installed version.

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Start a Local Standalone Validator](#2-start-a-local-standalone-validator)
3. [Create Identities](#3-create-identities)
4. [Build the WASM](#4-build-the-wasm)
5. [Deploy the Contract](#5-deploy-the-contract)
6. [Deploy a Test Token](#6-deploy-a-test-token)
7. [init — Initialize an Escrow](#7-init--initialize-an-escrow)
8. [fund — Investor Funding](#8-fund--investor-funding)
9. [fund_with_commitment — Tiered Yield Funding](#9-fund_with_commitment--tiered-yield-funding)
10. [settle — SME Settlement](#10-settle--sme-settlement)
11. [withdraw — SME Liquidity Pull](#11-withdraw--sme-liquidity-pull)
12. [claim_investor_payout — Investor Claim](#12-claim_investor_payout--investor-claim)
13. [Read-Only Getters](#13-read-only-getters)
14. [Admin Operations](#14-admin-operations)
15. [Auth Flags Reference](#15-auth-flags-reference)
16. [Security Notes](#16-security-notes)

---

## 1. Prerequisites

| Requirement | Version / Notes |
|---|---|
| Stellar CLI | v22+ (`stellar --version`) |
| Rust toolchain | stable, 1.70+ |
| `wasm32v1-none` target | `rustup target add wasm32v1-none` |
| Docker (optional) | for `stellar container start` standalone node |

Install the Stellar CLI:

```bash
cargo install --locked stellar-cli --features opt
```

Verify:

```bash
stellar --version
# stellar 22.x.x
```

---

## 2. Start a Local Standalone Validator

The local standalone node runs in a Docker container via the `stellar container` subcommand
(not `stellar network start`):

```bash
stellar container start local
```

This starts a local Soroban-enabled validator accessible at `http://localhost:8000`. The network
passphrase for the local standalone is `Standalone Network ; February 2017`.

Add it as a named network for convenience:

```bash
stellar network add \
  --rpc-url http://localhost:8000/soroban/rpc \
  --network-passphrase "Standalone Network ; February 2017" \
  local
```

Confirm connectivity:

```bash
stellar network ls
```

Stop the container when done:

```bash
stellar container stop local
```

---

## 3. Create Identities

Create four named identities for the simulation. Each maps to a Stellar keypair stored locally.

```bash
# Admin — governs holds, maturity updates, admin transfer
stellar keys generate admin --network local --fund

# SME (invoice issuer) — calls settle() and withdraw()
stellar keys generate sme --network local --fund

# Two investors
stellar keys generate investor1 --network local --fund
stellar keys generate investor2 --network local --fund

# Treasury — receives terminal dust sweeps
stellar keys generate treasury --network local --fund
```

Retrieve addresses for use in later commands:

```bash
ADMIN=$(stellar keys address admin)
SME=$(stellar keys address sme)
INVESTOR1=$(stellar keys address investor1)
INVESTOR2=$(stellar keys address investor2)
TREASURY=$(stellar keys address treasury)

echo "ADMIN:     $ADMIN"
echo "SME:       $SME"
echo "INVESTOR1: $INVESTOR1"
echo "INVESTOR2: $INVESTOR2"
echo "TREASURY:  $TREASURY"
```

> **⚠ Maintainer secret note:** In production, `admin` and `treasury` correspond to
> `LIQUIFACT_ADMIN_ADDRESS` and the treasury multisig. Their secret keys (`SOURCE_SECRET`) must
> never be committed to version control or logged. The local identities above are simulation-only
> throwaway keys.

---

## 4. Build the WASM

```bash
rustup target add wasm32v1-none

cargo build --target wasm32v1-none --release
```

The compiled artifact is at:

```
target/wasm32v1-none/release/liquifact_escrow.wasm
```

Optionally lint before deploying:

```bash
cargo clippy -p escrow -- -D warnings
```

---

## 5. Deploy the Contract

```bash
CONTRACT_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/liquifact_escrow.wasm \
  --source admin \
  --network local)

echo "CONTRACT_ID: $CONTRACT_ID"
```

Save `CONTRACT_ID` — every subsequent `invoke` command references it.

Sample output:

```
CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4
```

---

## 6. Deploy a Test Token

The escrow binds one SEP-41 funding token at `init`. For local simulation, deploy the Stellar
native asset wrapper or a test token contract. The simplest approach uses the built-in
`stellar contract asset deploy` for the native XLM asset:

```bash
TOKEN_ID=$(stellar contract asset deploy \
  --asset native \
  --source admin \
  --network local)

echo "TOKEN_ID: $TOKEN_ID"
```

For a custom test token (e.g. USDC simulation), deploy a Soroban token contract separately and
mint to investor addresses before calling `fund`. The escrow only requires a standard SEP-41
interface (`transfer`, `balance`).

> **Token safety:** Only standard SEP-41 tokens are supported. Fee-on-transfer, rebasing, or
> hook tokens are **out of scope** and will cause `external_calls::transfer_funding_token_with_balance_checks`
> to panic on balance assertion. See [`escrow/src/external_calls.rs`](../escrow/src/external_calls.rs).

---

## 7. `init` — Initialize an Escrow

`init` is the first call on a freshly deployed contract. It is **one-time** — calling it again
panics with `"Escrow already initialized"`.

**Auth required:** `admin` (`admin.require_auth()`)

### Argument reference

| Argument | Type | Description |
|---|---|---|
| `admin` | `Address` | Governance address; controls holds, maturity, admin transfer |
| `invoice_id` | `String` | ASCII alphanumeric + `_`, max 32 chars (e.g. `INV001`) |
| `sme_address` | `Address` | Invoice issuer; authorized to call `settle` and `withdraw` |
| `amount` | `i128` | Funding target in token base units (7 decimals → `10_000_0000000` = 10,000 units) |
| `yield_bps` | `i64` | Base annualized yield in basis points (800 = 8%); range 0–10,000 |
| `maturity` | `u64` | Unix timestamp after which `settle` is allowed; `0` = no maturity gate |
| `funding_token` | `Address` | SEP-41 token contract address bound to this escrow (immutable) |
| `registry` | `Option<Address>` | Optional registry hint for indexers; not an on-chain authority |
| `treasury` | `Address` | Receives terminal dust sweeps; immutable after init |
| `yield_tiers` | `Option<Vec<YieldTier>>` | Optional tiered yield ladder; `null` to omit |
| `min_contribution` | `Option<i128>` | Minimum per-call funding floor; `null` for no floor |
| `max_unique_investors` | `Option<u32>` | Cap on distinct investor addresses; `null` for default 128 |

### Minimal init (no optional features)

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source admin \
  --network local \
  -- init \
  --admin "$ADMIN" \
  --invoice_id "INV001" \
  --sme_address "$SME" \
  --amount 10000_0000000 \
  --yield_bps 800 \
  --maturity 0 \
  --funding_token "$TOKEN_ID" \
  --registry null \
  --treasury "$TREASURY" \
  --yield_tiers null \
  --min_contribution null \
  --max_unique_investors null
```

### Init with maturity gate (Unix timestamp)

```bash
# Set maturity to a future timestamp (e.g. 2026-12-31 00:00:00 UTC = 1767139200)
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source admin \
  --network local \
  -- init \
  --admin "$ADMIN" \
  --invoice_id "INV002" \
  --sme_address "$SME" \
  --amount 50000_0000000 \
  --yield_bps 1000 \
  --maturity 1767139200 \
  --funding_token "$TOKEN_ID" \
  --registry null \
  --treasury "$TREASURY" \
  --yield_tiers null \
  --min_contribution null \
  --max_unique_investors null
```

### Init with tiered yield

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source admin \
  --network local \
  -- init \
  --admin "$ADMIN" \
  --invoice_id "INV003" \
  --sme_address "$SME" \
  --amount 10000_0000000 \
  --yield_bps 800 \
  --maturity 0 \
  --funding_token "$TOKEN_ID" \
  --registry null \
  --treasury "$TREASURY" \
  --yield_tiers '[{"min_lock_secs":2592000,"yield_bps":1000},{"min_lock_secs":7776000,"yield_bps":1200}]' \
  --min_contribution null \
  --max_unique_investors null
```

> Tiers must have strictly increasing `min_lock_secs` and non-decreasing `yield_bps` ≥ base
> `yield_bps`. The table is **immutable** after `init`.

### Init with min contribution floor and investor cap

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source admin \
  --network local \
  -- init \
  --admin "$ADMIN" \
  --invoice_id "INV004" \
  --sme_address "$SME" \
  --amount 10000_0000000 \
  --yield_bps 800 \
  --maturity 0 \
  --funding_token "$TOKEN_ID" \
  --registry null \
  --treasury "$TREASURY" \
  --yield_tiers null \
  --min_contribution 100_0000000 \
  --max_unique_investors 10
```

Sample output:

```json
{
  "invoice_id": "INV001",
  "admin": "GADMIN...",
  "sme_address": "GSME...",
  "amount": "10000_0000000",
  "funding_target": "10000_0000000",
  "funded_amount": "0",
  "yield_bps": "800",
  "maturity": "0",
  "status": "0"
}
```

---

## 8. `fund` — Investor Funding

Records investor principal. Moves status from `0` (open) → `1` (funded) when `funded_amount`
reaches `funding_target`. Captures a `FundingCloseSnapshot` on the first transition to funded.

**Auth required:** `investor` (`investor.require_auth()`)

> `fund` is the correct method for **all deposits after the first** from a given investor address.
> The first deposit may also use `fund` (base yield) or `fund_with_commitment` (tiered yield).

### Single investor funds the full target

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source investor1 \
  --network local \
  -- fund \
  --investor "$INVESTOR1" \
  --amount 10000_0000000
```

Sample output:

```json
{
  "invoice_id": "INV001",
  "funded_amount": "10000_0000000",
  "status": "1"
}
```

### Two investors split the target

```bash
# Investor 1 funds half
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source investor1 \
  --network local \
  -- fund \
  --investor "$INVESTOR1" \
  --amount 5000_0000000

# Investor 2 funds the remaining half — triggers status → 1 (funded)
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source investor2 \
  --network local \
  -- fund \
  --investor "$INVESTOR2" \
  --amount 5000_0000000
```

### Investor adds a follow-on deposit (same address, already funded once)

```bash
# Must use fund(), not fund_with_commitment(), for subsequent deposits
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source investor1 \
  --network local \
  -- fund \
  --investor "$INVESTOR1" \
  --amount 1000_0000000
```

> **Investor cap:** The contract enforces a hard cap of 128 distinct investor addresses per
> escrow (or the `max_unique_investors` value set at `init`). The 129th distinct address is
> rejected. Re-funding an existing investor at the cap is still allowed.

---

## 9. `fund_with_commitment` — Tiered Yield Funding

First deposit only per investor. Selects an effective yield from the tier ladder based on
`committed_lock_secs`. Sets `InvestorClaimNotBefore` when `committed_lock_secs > 0`.

**Auth required:** `investor` (`investor.require_auth()`)

```bash
# Commit to 30-day lock (2592000 seconds) — selects matching tier yield
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source investor1 \
  --network local \
  -- fund_with_commitment \
  --investor "$INVESTOR1" \
  --amount 5000_0000000 \
  --committed_lock_secs 2592000
```

> After calling `fund_with_commitment`, any additional principal from the same investor address
> **must** use `fund()`. Calling `fund_with_commitment` again for the same investor panics with
> `"Additional principal after a tiered first deposit must use fund()"`.

---

## 10. `settle` — SME Settlement

Marks a funded escrow as settled (status `1` → `2`). Blocked by legal hold and by maturity gate
if `maturity > 0` and `ledger.timestamp < maturity`.

**Auth required:** `sme_address` (`sme_address.require_auth()`)

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source sme \
  --network local \
  -- settle
```

Sample output:

```json
{
  "invoice_id": "INV001",
  "funded_amount": "10000_0000000",
  "yield_bps": "800",
  "maturity": "0",
  "status": "2"
}
```

> **Maturity gate:** If `maturity` was set at `init` and the ledger timestamp has not yet
> reached it, `settle` panics with `"Escrow has not yet reached maturity"`. On a local
> standalone validator you can advance ledger time by submitting transactions or using
> `stellar ledger bump` (check your CLI version for availability).

---

## 11. `withdraw` — SME Liquidity Pull

Alternative terminal path: SME pulls funded liquidity (status `1` → `3` / withdrawn) without
going through `settle`. Blocked by legal hold.

**Auth required:** `sme_address` (`sme_address.require_auth()`)

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source sme \
  --network local \
  -- withdraw
```

Sample output:

```json
{
  "invoice_id": "INV001",
  "funded_amount": "10000_0000000",
  "status": "3"
}
```

> `settle` and `withdraw` are mutually exclusive paths from status `1`. Once either is called,
> the escrow is in a terminal state.

---

## 12. `claim_investor_payout` — Investor Claim

Records that an investor has claimed their payout after settlement (status must be `2`). This is
an **accounting marker** — actual token transfer is handled by the integration layer, not this
contract. Blocked by legal hold and by `InvestorClaimNotBefore` if a commitment lock is active.

**Auth required:** `investor` (`investor.require_auth()`)

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source investor1 \
  --network local \
  -- claim_investor_payout \
  --investor "$INVESTOR1"
```

Sample output:

```
null
```

> The call emits an `InvestorPayoutClaimed` event. Verify with:
>
> ```bash
> stellar events --id "$CONTRACT_ID" --network local
> ```

---

## 13. Read-Only Getters

These calls do not require `--source` auth and do not modify state.

### `get_escrow` — Full escrow state

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network local \
  -- get_escrow
```

Sample output:

```json
{
  "invoice_id": "INV001",
  "admin": "GADMIN...",
  "sme_address": "GSME...",
  "amount": "10000_0000000",
  "funding_target": "10000_0000000",
  "funded_amount": "10000_0000000",
  "yield_bps": "800",
  "maturity": "0",
  "status": "2"
}
```

Status values: `0` = open, `1` = funded, `2` = settled, `3` = withdrawn.

### `get_unique_funder_count` — Distinct investor count

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network local \
  -- get_unique_funder_count
```

Sample output: `"2"`

### `get_contribution` — Principal for one investor

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network local \
  -- get_contribution \
  --investor "$INVESTOR1"
```

Sample output: `"5000_0000000"`

### `max_investors` — Investor cap

The hard cap is `128` distinct investors per escrow (or the `max_unique_investors` value set at
`init`). Read the configured cap:

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network local \
  -- get_max_unique_investors_cap
```

Returns `null` when no explicit cap was set at `init` (default 128 applies in contract logic).

### `get_funding_close_snapshot` — Pro-rata denominator

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network local \
  -- get_funding_close_snapshot
```

Sample output (after funding closes):

```json
{
  "total_principal": "10000_0000000",
  "funding_target": "10000_0000000",
  "closed_at_ledger_timestamp": "1745539200",
  "closed_at_ledger_sequence": "42"
}
```

Returns `null` before the escrow reaches funded status.

### `get_investor_yield_bps` — Effective yield for one investor

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network local \
  -- get_investor_yield_bps \
  --investor "$INVESTOR1"
```

Sample output: `"800"` (base yield) or a higher tier value if `fund_with_commitment` was used.

### `get_investor_claim_not_before` — Commitment lock timestamp

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network local \
  -- get_investor_claim_not_before \
  --investor "$INVESTOR1"
```

Returns `"0"` if no lock was set.

### `is_investor_claimed` — Claim marker

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network local \
  -- is_investor_claimed \
  --investor "$INVESTOR1"
```

Returns `"true"` or `"false"`.

### `get_funding_token` / `get_treasury` / `get_registry_ref` / `get_version`

```bash
stellar contract invoke --id "$CONTRACT_ID" --network local -- get_funding_token
stellar contract invoke --id "$CONTRACT_ID" --network local -- get_treasury
stellar contract invoke --id "$CONTRACT_ID" --network local -- get_registry_ref
stellar contract invoke --id "$CONTRACT_ID" --network local -- get_version
stellar contract invoke --id "$CONTRACT_ID" --network local -- get_legal_hold
stellar contract invoke --id "$CONTRACT_ID" --network local -- get_min_contribution_floor
```

`get_registry_ref` returns `null` when no registry was provided at `init`. The registry address
is a **discoverability hint only** — not an on-chain authority.

### `get_sme_collateral_commitment`

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network local \
  -- get_sme_collateral_commitment
```

Returns `null` until `record_sme_collateral_commitment` has been called. Sample output:

```json
{
  "asset": "USDC",
  "amount": "5000_0000000",
  "recorded_at": "1745539200"
}
```

### `get_primary_attestation_hash`

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network local \
  -- get_primary_attestation_hash
```

Returns `null` until `bind_primary_attestation_hash` has been called.

### `get_attestation_append_log`

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network local \
  -- get_attestation_append_log
```

Returns an array of 32-byte digests (up to `MAX_ATTESTATION_APPEND_ENTRIES` = 32 entries).
Returns an empty array `[]` if no digests have been appended.

---

## 14. Admin Operations

### `set_legal_hold` / `clear_legal_hold`

**Auth required:** `admin`

```bash
# Enable hold — blocks settle, withdraw, claim_investor_payout, fund, sweep_terminal_dust
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source admin \
  --network local \
  -- set_legal_hold \
  --active true

# Clear hold
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source admin \
  --network local \
  -- clear_legal_hold
```

### `update_maturity`

**Auth required:** `admin`. Only allowed in status `0` (open).

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source admin \
  --network local \
  -- update_maturity \
  --new_maturity 1767139200
```

### `transfer_admin`

**Auth required:** current `admin`.

```bash
NEW_ADMIN="GNEWADMIN..."

stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source admin \
  --network local \
  -- transfer_admin \
  --new_admin "$NEW_ADMIN"
```

### `sweep_terminal_dust`

**Auth required:** `treasury`. Only in terminal states (status `2` or `3`). Capped at
`MAX_DUST_SWEEP_AMOUNT` (100,000,000 base units) per call.

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source treasury \
  --network local \
  -- sweep_terminal_dust \
  --amount 1000000
```

### `bind_primary_attestation_hash`

**Auth required:** `admin`. Single-set — panics if already bound.

```bash
# 32-byte hex digest (e.g. SHA-256 of a legal document bundle)
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source admin \
  --network local \
  -- bind_primary_attestation_hash \
  --digest "aabbccdd...32bytes...hex"
```

### `append_attestation_digest`

**Auth required:** `admin`. Append-only audit log; bounded at 32 entries
(`MAX_ATTESTATION_APPEND_ENTRIES`). Does not replace the primary hash.

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source admin \
  --network local \
  -- append_attestation_digest \
  --digest "aabbccdd...32bytes...hex"
```

### `update_funding_target`

**Auth required:** `admin`. Only allowed in status `0` (open). New target must be positive and
≥ `funded_amount` at the time of the call.

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source admin \
  --network local \
  -- update_funding_target \
  --new_target 20000_0000000
```

### `migrate`

**Auth required:** none (panics immediately unless a migration path is implemented). Used to
advance the stored schema version. The current contract panics for all `from_version` values
because no migration path is implemented — this entrypoint is a forward-compatibility hook for
future schema upgrades.

The current `SCHEMA_VERSION` is `5`. A fresh deployment stores version `5`. Two panic cases:

```bash
# Panics: "Already at current schema version" (from_version matches stored AND equals SCHEMA_VERSION)
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source admin \
  --network local \
  -- migrate \
  --from_version 5

# Panics: "from_version does not match stored version" (mismatch with what is stored)
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source admin \
  --network local \
  -- migrate \
  --from_version 4
```

> Do not call `migrate` in simulation unless you have extended the implementation with a
> concrete migration path. It is documented here for completeness.

### `record_sme_collateral_commitment`

**Auth required:** `sme_address`. Record-only — does not move tokens.

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source sme \
  --network local \
  -- record_sme_collateral_commitment \
  --asset "USDC" \
  --amount 5000_0000000
```

---

## 15. Auth Flags Reference

### How `require_auth()` maps to CLI flags

In Stellar CLI v22+, the `--source` flag specifies the keypair that signs the transaction. When
a contract entrypoint calls `addr.require_auth()`, the CLI must present a valid authorization
for that address. For the common case where `--source` is the authorizing address, no extra
`--auth` flag is needed — the CLI automatically includes the source account's authorization.

| Entrypoint | `require_auth()` caller | `--source` to use |
|---|---|---|
| `init` | `admin` | `admin` |
| `fund` | `investor` | `investor1` / `investor2` |
| `fund_with_commitment` | `investor` | `investor1` / `investor2` |
| `settle` | `sme_address` | `sme` |
| `withdraw` | `sme_address` | `sme` |
| `claim_investor_payout` | `investor` | `investor1` / `investor2` |
| `sweep_terminal_dust` | `treasury` | `treasury` |
| `set_legal_hold` / `clear_legal_hold` | `admin` | `admin` |
| `update_maturity` | `admin` | `admin` |
| `transfer_admin` | `admin` (current) | `admin` |
| `bind_primary_attestation_hash` | `admin` | `admin` |
| `append_attestation_digest` | `admin` | `admin` |
| `update_funding_target` | `admin` | `admin` |
| `migrate` | none (panics; no path implemented) | `admin` (by convention) |
| `record_sme_collateral_commitment` | `sme_address` | `sme` |

### Multi-party authorization (`--auth`)

When the transaction source differs from the authorizing address (e.g. a relayer pays fees but
the investor authorizes the `fund` call), use `--auth` to attach a pre-signed authorization
entry. Refer to the [Stellar CLI auth docs](https://developers.stellar.org/docs/tools/soroban-cli/stellar-cli)
for the `--auth` flag format, which encodes a `SorobanAuthorizationEntry` in XDR.

For local simulation, using `--source <identity>` where the identity matches the required
`require_auth()` address is the simplest approach and covers all recipes in this document.

### Role summary

| Role | Address | Capabilities |
|---|---|---|
| **Admin** | `LIQUIFACT_ADMIN_ADDRESS` | `init`, legal hold, maturity update, admin transfer, attestation binding |
| **SME** | set at `init` | `settle`, `withdraw`, `record_sme_collateral_commitment` |
| **Investor** | any funded address | `fund`, `fund_with_commitment`, `claim_investor_payout` |
| **Treasury** | set at `init` (immutable) | `sweep_terminal_dust` |

---

## 16. Security Notes

### Maintainer secrets

The following environment variables are required for production deployments and must **never**
be committed to version control, logged, or shared:

| Variable | Purpose | Required for |
|---|---|---|
| `SOURCE_SECRET` | Deployer / admin Stellar secret key (`S...`) | `stellar contract deploy`, `init`, all admin calls |
| `LIQUIFACT_ADMIN_ADDRESS` | Initial admin address (`G...`) | `--admin` arg in `init` |

In production, `admin` should be a multisig or governed contract address, not a single hot key.
See the repository README for the release runbook.

### Token transfer behavior is out of scope

Token transfers (principal custody, payout disbursement, yield payment) are handled by the
**integration layer**, not by this escrow contract. The escrow stores numeric state only.

The `external_calls::transfer_funding_token_with_balance_checks` function in
[`escrow/src/external_calls.rs`](../escrow/src/external_calls.rs) enforces pre/post balance
equality for SEP-41 tokens used in `sweep_terminal_dust`. Fee-on-transfer, rebasing, and hook
tokens are **explicitly out of scope** and will cause a panic on balance assertion.

### These recipes are for local simulation only

- All addresses above are placeholder simulation identities.
- Do not use `stellar container start local` credentials against Testnet or Mainnet.
- The local standalone validator has no real economic value; keys generated with `--fund` are
  funded from a local friendbot and have no Mainnet equivalent.
- For Testnet simulation, replace `--network local` with `--network testnet` and fund accounts
  via the [Stellar Testnet Friendbot](https://friendbot.stellar.org).

### State transition safety

| Status | Value | Allowed next transitions |
|---|---|---|
| Open | `0` | → `1` (funded, via `fund`) |
| Funded | `1` | → `2` (settled, via `settle`) or `3` (withdrawn, via `withdraw`) |
| Settled | `2` | terminal — `claim_investor_payout`, `sweep_terminal_dust` only |
| Withdrawn | `3` | terminal — `sweep_terminal_dust` only |

Transitions are forward-only. Legal hold blocks `settle`, `withdraw`, `claim_investor_payout`,
`fund` (new funding), and `sweep_terminal_dust`.

### Investor cap

The contract enforces a maximum of **128 distinct investor addresses** per escrow instance (or
the `max_unique_investors` value set at `init`). This is a storage-growth guard against
denial-of-storage attacks. Invoices requiring more than 128 backers should be split across
multiple escrow instances or handled via a higher-level allocation flow.

### Ledger time

`settle` and `claim_investor_payout` compare against `Env::ledger().timestamp()` (validator-
observed ledger time), not wall-clock time. Maturity and commitment lock boundaries are
`>=` / `<` integer second comparisons. Simulated and live network timestamps may differ.

### Registry ref is not an authority

The optional `registry` address stored at `init` is a **discoverability hint for indexers only**.
It is not verified or called by this contract and must not be used as proof of registry state
without independent verification.
