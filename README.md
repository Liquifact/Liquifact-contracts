# LiquiFact Escrow Contract

Soroban smart contracts for **LiquiFact** — invoice-backed liquidity on Stellar. This repo contains a single **escrow** contract that accounts for investor funding until SME settlement.

---

## Lifecycle

| Step | Method | Summary |
|------|--------|---------|
| 1 | `init` | Admin authorizes; validates amounts, yield, maturity; records funding-round metadata and `funding_opened_at`. |
| 2 | `fund` | Investor authorizes; **caps** deposits at the remaining `funding_target`; reports `excess_amount` for off-chain refunds. |
| 3 | `settle` | SME authorizes; moves from **funded** to **settled**. |
| Optional | `cancel` | Admin only, **open** and **zero** `funded_amount`; status → **cancelled**. |
| | `update_maturity` | Admin only while **open**; re-validates maturity bounds. |
| | `migrate` | Storage upgrade (currently **v1 → v2**). |

**Status codes**

| Value | Meaning |
|-------|---------|
| `0` | Open (accepting funding) |
| `1` | Funded (`funded_amount == funding_target`) |
| `2` | Settled |
| `3` | Cancelled (before any funds accepted) |

---

## Initialization validation

[`validate_init_params`](escrow/src/lib.rs) (and `init`) enforce:

- **`amount`**: must be `> 0`.
- **`funding_target`**: must be `> 0` and `≤ amount` (allows a lower fundraising goal than nominal invoice face).
- **`yield_bps`**: must be in `0..=10_000` (basis points, max 100%).
- **`maturity`**: must be **strictly after** the current ledger timestamp and at most **10 years** ahead (see `MAX_MATURITY_DELTA_SECS` in code).

Invalid inputs return [`EscrowError`](escrow/src/lib.rs) instead of persisting state.

---

## Funding cap and remainder

For each `fund` call:

- `need = funding_target - funded_amount`
- `amount_accepted = min(offer, need)`
- `excess_amount = offer - amount_accepted`

Only `amount_accepted` is added to `funded_amount`. The contract does not custody assets in this repository; integrators **must** return `excess_amount` to the investor when combining this accounting with token transfers.

When `funded_amount` reaches `funding_target`, status becomes **funded** and `funding_closed_at` is set to the current ledger timestamp.

---

## Funding-round metadata (schema v2)

[`InvoiceEscrow`](escrow/src/lib.rs) includes:

- **`funding_round_id`**: short `Symbol` label (e.g. indexer / analytics key).
- **`funding_opened_at`**: set at `init` to ledger time.
- **`funding_closed_at`**: set when the escrow becomes **funded**, else `0`.

`SCHEMA_VERSION` is **2**. Migrations from v1 fill `funding_round_id` with `MIGRAT` and best-effort `funding_closed_at` for already-funded legacy rows.

---

## Authorization (current)

| Function | `require_auth` |
|----------|----------------|
| `init` | `admin` |
| `fund` | `investor` |
| `settle` | `sme_address` |
| `cancel` | `admin` |
| `update_maturity` | `admin` |
| `migrate` | *none* (gate in production) |

---

## Threat model (summary)

| Risk | Mitigation in code |
|------|---------------------|
| Re-init overwriting state | `init` fails if `escrow` key already exists. |
| Overfunding / inflated `funded_amount` | Capped at `funding_target`; overflow uses checked arithmetic. |
| Absurd yield or maturity | Bounded at init and on maturity updates. |
| Cancellation after money in | `cancel` requires `funded_amount == 0`. |
| Unauthorized migration | **Must** add `admin.require_auth()` before mainnet. |

Off-chain invoice proof, KYC, and token movements remain out of scope for this crate.

---

## Repository layout

```text
liquifact-contracts/
├── Cargo.toml              # Workspace
├── README.md
├── escrow/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          # Contract
│       └── test.rs         # Unit + proptest
├── docs/                   # OpenAPI & tooling (optional)
└── .github/workflows/ci.yml
```

---

## Commands

| Command | Description |
|---------|-------------|
| `cargo build` | Build the contract |
| `cargo test` | Tests (with `testutils` where needed) |
| `cargo fmt --all -- --check` | CI formatting |
| `cargo llvm-cov --features testutils --fail-under-lines 95 --summary-only` | Coverage gate (matches CI) |

---

## Schema versioning

| Version | Description |
|---------|-------------|
| 1 | Legacy `InvoiceEscrowV1` (see `lib.rs`). |
| 2 | Current `InvoiceEscrow` + funding metadata fields. |

After layout changes: bump `SCHEMA_VERSION`, extend `migrate`, and add tests.

---

## Contributing

1. Fork and branch from `main`.
2. `cargo fmt`, `cargo build`, `cargo test`.
3. Meet **≥ 95%** line coverage with `cargo llvm-cov` as in CI.
4. Open a PR with a clear description and security-relevant notes where applicable.
