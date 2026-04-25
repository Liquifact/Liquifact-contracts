# WASM Ops Runbook: LiquifactEscrow Deployment and Version Registry

## 2a. Overview

LiquifactEscrow is a Soroban smart contract deployed on the Stellar network. Each compiled WASM
binary embeds a `SCHEMA_VERSION` constant (currently `5`) that is written to on-chain storage
under `DataKey::Version` when the contract is initialized. The backend must track which deployed
contract addresses correspond to which WASM version so it can call the correct entrypoints and
interpret stored state correctly.

`src/config/escrowVersions.js` is the authoritative map of `semver → SCHEMA_VERSION`. It is
updated manually each time a new WASM is deployed with a bumped schema version.

---

## 2b. Environment Variables

Set these in your deployment environment (`.env` for local dev; deployment secrets for staging/prod).
**Never commit secret values to the repository.**

| Variable | Required | Description |
|---|---|---|
| `STELLAR_NETWORK` | Yes | `testnet`, `public`, or a custom Horizon network passphrase |
| `SOROBAN_RPC_URL` | Yes | Soroban RPC endpoint URL (e.g. `https://soroban-testnet.stellar.org`) |
| `ESCROW_CONTRACT_SEMVER` | Yes | Semver of the deployed WASM matching a key in `ESCROW_VERSIONS` (e.g. `0.1.0`) |
| `ESCROW_CONTRACT_ADDRESS` | No | Comma-separated list of deployed contract addresses to monitor (C... strkey format) |
| `SOURCE_SECRET` | Deployment only | Stellar secret key (S...) for the deployer/admin account — **never in repo** |

---

## 2c. Deployment Procedure

### Step 1 — Build the WASM

```bash
rustup target add wasm32v1-none
cargo build --target wasm32v1-none --release
```

Artifact: `target/wasm32v1-none/release/liquifact_escrow.wasm`

### Step 2 — Upload the WASM to Stellar

```bash
stellar contract upload \
  --wasm target/wasm32v1-none/release/liquifact_escrow.wasm \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK \
  --rpc-url $SOROBAN_RPC_URL
```

Note the returned `<WASM_HASH>`.

### Step 3 — Deploy a new contract instance

```bash
stellar contract deploy \
  --wasm-hash <WASM_HASH> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK \
  --rpc-url $SOROBAN_RPC_URL
```

Note the returned `<CONTRACT_ADDRESS>`.

### Step 4 — Initialize the contract

Call `init` with the required parameters. See `LiquifactEscrow::init` in
`escrow/src/lib.rs` for the full parameter list (admin, invoice_id, sme_address, amount,
yield_bps, maturity, funding_token, registry, treasury, yield_tiers, min_contribution,
max_unique_investors).

```bash
stellar contract invoke \
  --id <CONTRACT_ADDRESS> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK \
  --rpc-url $SOROBAN_RPC_URL \
  -- init \
  --admin <ADMIN_ADDRESS> \
  --invoice-id "INV001" \
  # ... remaining args
```

### Step 5 — Verify on-chain SCHEMA_VERSION

```bash
stellar contract invoke \
  --id <CONTRACT_ADDRESS> \
  --source $SOURCE_SECRET \
  --network $STELLAR_NETWORK \
  --rpc-url $SOROBAN_RPC_URL \
  -- get_version
```

Assert the returned integer equals `getExpectedSchemaVersion(ESCROW_CONTRACT_SEMVER)` from
`src/config/escrowVersions.js`. If they differ, do not proceed — the wrong WASM was deployed.

### Step 6 — Update backend deployment secrets

Set `ESCROW_CONTRACT_SEMVER` to the new semver and add the new `<CONTRACT_ADDRESS>` to
`ESCROW_CONTRACT_ADDRESS` (comma-separated). Also add the new semver entry to
`src/config/escrowVersions.js` and open a PR.

### Step 7 — Restart the backend service

Redeploy or restart the Express service so it picks up the updated env vars.

---

## 2d. Registry Refresh Job

A periodic job checks that every known contract address reports the expected `SCHEMA_VERSION`.
It is advisory only — it logs warnings but never auto-migrates or auto-redeploys.

```js
const { getExpectedSchemaVersion } = require('./src/config/escrowVersions');

/**
 * Check each known contract address for SCHEMA_VERSION drift.
 * @param {string[]} addresses - C... strkey contract addresses
 */
async function refreshEscrowVersionRegistry(addresses) {
  const expected = getExpectedSchemaVersion(process.env.ESCROW_CONTRACT_SEMVER);
  for (const addr of addresses) {
    const onChainVersion = await sorobanCall(addr, 'get_version', []);
    if (onChainVersion !== expected) {
      logger.warn(
        { addr, onChainVersion, expected },
        'SCHEMA_VERSION mismatch — review required'
      );
    }
  }
}
```

`sorobanCall` is a placeholder for your Soroban RPC client (e.g. `@stellar/stellar-sdk`
`SorobanRpc.Server`). The job does not take any corrective action; a human must decide whether
to migrate the contract or update the backend.

Suggested schedule: run on backend startup and every 10 minutes in production.

---

## 2e. Security Notes

- `SOURCE_SECRET` is a Stellar secret key (`S...`). Treat it as a root credential. Store it
  only in deployment secrets (CI/CD vault, AWS Secrets Manager, etc.) — never in `.env` files
  committed to the repository.
- All addresses read from `ESCROW_CONTRACT_ADDRESS` must be validated as Stellar contract
  strkeys (`C...`, 56 characters, base32) before being passed to any RPC call. Reject and log
  any malformed entry at startup.
- On-chain `SCHEMA_VERSION` values are advisory. They confirm the contract was initialized with
  the expected code path but do not prove the WASM hash matches the audited artifact. Governance
  must independently verify `wasm_hash` against the build artifact checksum.
- The backend must not use the registry as an authority for contract behavior. It is a
  discoverability and drift-detection tool only.

---

## 2f. Rollback Procedure

Soroban contracts are immutable once deployed; a faulty WASM cannot be patched in place. A new
deployment is always required for a code fix.

To roll back the **backend** to a previous contract version:

1. Revert `ESCROW_CONTRACT_SEMVER` in deployment secrets to the previous semver.
2. Revert `ESCROW_CONTRACT_ADDRESS` to the previous contract address list.
3. Restart the backend service.

No on-chain state is modified by a backend rollback. Old contract instances remain live and
readable. If the new contract instance received any `init` or `fund` calls before rollback,
those on-chain state changes are permanent — coordinate with governance before rolling back
if live funds are involved.
