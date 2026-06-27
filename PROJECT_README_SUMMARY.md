# LiquiFact Escrow Contracts (liquifact_escrow)

Soroban smart contract for **LiquiFact**: holds investor funds for a tokenized invoice until settlement, supports investor payout claims after settlement, and includes governance-controlled compliance and attestation features.

This repo is a Cargo workspace containing a single contract crate:
- `escrow/` → the Soroban contract `liquifact_escrow`.

## What the contract does

At a high level, each escrow instance represents one invoice funding round and supports a lifecycle:

1. **Initialize (`init`)**
   - Configures:
     - `admin` (governance-controlled)
     - `sme_address` (beneficiary receiving liquidity on settlement)
     - invoice identifier (`invoice_id`)
     - funding parameters (`amount`, `yield_bps`, `maturity`)
     - bound contracts/addresses: `funding_token` (SEP-41) + `treasury`
     - optional features: registry hint, tiered yield ladder, contribution floor, investor caps, allowlist gating, legal-hold clear delay, funding deadline

2. **Fund (`fund`, `fund_with_commitment`, `fund_batch`)**
   - Investors add principal while the escrow is **open**.
   - Supports:
     - per-investor maximum cap (`max_per_investor`)
     - optional global cap on distinct funders (`max_unique_investors`)
     - optional minimum contribution per call (`min_contribution`)
     - optional investor allowlist gate
     - optional tiered yield/commitment lock discipline via first-deposit-only configuration
   - When `funded_amount >= funding_target`, escrow transitions to **funded** and writes a **write-once pro-rata snapshot** (`FundingCloseSnapshot`).

3. **Settle (`partial_settle`, `settle`)**
   - SME finalizes the deal after funding is reached.
   - If configured, settlement requires ledger time to be past `maturity`.
   - Legal hold blocks settlement.

4. **Withdraw (`withdraw`)**
   - SME pulls the funded liquidity when the escrow is in the correct state.
   - Legal hold blocks withdrawal.

5. **Investor payout claims (`claim_investor_payout`)**
   - After settlement, investors claim that they are eligible for a payout.
   - Claim gating includes optional commitment lock expiration (ledger timestamp based).
   - The per-investor “claimed” marker is idempotent and prevents event re-emission.

6. **Cancel + Refund (`cancel_funding`, `refund`)**
   - Admin can cancel only when open.
   - Investors can refund their principal in the cancelled state.

7. **Treasury dust sweep (`sweep_terminal_dust`)**
   - Treasury can sweep bounded “residue” tokens from this contract only in terminal states (settled/withdrawn/cancelled).
   - For cancelled escrows, a liability floor ensures sweeps cannot pull below outstanding investor obligations.

## Key security and design properties

### Roles and authorization
- `admin`: controls governance actions (legal hold, allowlist configuration, cap lowering, admin handover, etc.).
- `sme_address`: authorizes settlement/withdraw flows; also beneficiary rotation.
- `investor`: authorizes funding and payout claim.
- `treasury`: authorizes dust sweep.

### Legal / compliance hold
- An admin can activate `LegalHold` to block settlement, SME withdrawal, investor claims, etc.
- Clearing requires the current admin authorization.
- No built-in break-glass bypass—production deployments should use a governed/multisig admin.

### Token integration assumptions
- Cross-contract token movement is performed only through a funding token address set at `init`.
- Transfers are enforced with **strict SEP-41-style balance delta checks**:
  - sender balance decreases by exactly `amount`
  - recipient (treasury) balance increases by exactly `amount`
- Fee-on-transfer, rebasing, and hook-modifying tokens are treated as out-of-scope and should fail at the balance-check boundary.

### Deterministic pro-rata payout math
- The contract provides `compute_investor_payout(env, investor)` implementing the authoritative pro-rata formula documented in `docs/escrow-pro-rata.md`.
- A write-once `FundingCloseSnapshot` anchors denominators for fairness and determinism.

### Storage schema versioning & migrations
- `SCHEMA_VERSION` is stored on-chain under `DataKey::Version`.
- Current schema version: **6**.
- `migrate` is intentionally **not implemented** in the current release; it aborts with typed errors in all current paths.

## Contract entrypoints (major)

- Lifecycle & funding:
  - `init`
  - `fund`
  - `fund_with_commitment`
  - `fund_batch`
  - `partial_settle`
  - `settle`

- SME and investor flows:
  - `withdraw`
  - `claim_investor_payout`

- Admin/governance & compliance:
  - `set_legal_hold`, `clear_legal_hold`
  - `request_clear_legal_hold`
  - `set_allowlist_active`, `set_investor_allowlisted`, `set_investors_allowlisted`
  - `lower_max_unique_investors`
  - `rotate_beneficiary`
  - `propose_admin`, `accept_admin`
  - `cancel_funding`

- Token maintenance:
  - `sweep_terminal_dust`

- Attestation / audit logging:
  - `bind_primary_attestation_hash`
  - `append_attestation_digest`
  - `revoke_attestation_digest`

- Read APIs:
  - `get_escrow`, `get_escrow_summary`
  - `get_contribution`
  - `get_funding_close_snapshot`
  - `get_version`
  - `get_legal_hold`, `get_*` helpers
  - `compute_investor_payout`

## Development / testing

This is a Rust + Soroban contract crate.

Typical commands:
```bash
cargo build
cargo test
```

For WASM builds:
```bash
rustup target add wasm32v1-none
cargo build --target wasm32v1-none --release -p liquifact_escrow
```

## Docs in this repo

The `docs/` folder contains operator and audit-oriented documentation, including:
- security checklists and the auth model (`docs/escrow-security-checklist.md`, `docs/adr/ADR-002-auth-boundaries.md`)
- pro-rata payout math and rounding (`docs/escrow-pro-rata.md`)
- legal hold details (`docs/escrow-legal-hold.md`)
- token integration assumptions (`docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`)
- operator deployment runbooks (`docs/OPERATOR_RUNBOOK.md`)
- EVM/Soroban mental model and storage/TTL notes

## Licensing

The repository is MIT licensed.

