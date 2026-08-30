# Beneficiary Authorization and Access Rules

This document describes the authorization rules governing the **SME beneficiary** (`sme_address`) in the LiquiFact escrow contract: which roles exist, which entrypoints each role may call, in which contract states, and what rejection codes result from violations.

---

## Roles

| Role | Stored As | Description | Mutability |
|------|-----------|-------------|------------|
| **SME / Beneficiary** | `InvoiceEscrow::sme_address` | Receives the funded principal upon withdrawal; authorizes settlement, withdrawal, and collateral operations. | Changed via `rotate_beneficiary` (dual auth) |
| **Admin** | `InvoiceEscrow::admin` | On-chain configuration, compliance holds, lifecycle management (cancel, pause, legal hold). | Two-step handover via `propose_admin` / `accept_admin` |
| **Investor** | Per-investor `DataKey::InvestorContribution` | Funds the escrow; claims payouts or refunds. | Self-authorized per call |
| **Treasury** | `DataKey::Treasury` (immutable) | Receives protocol fees and terminal dust. | Immutable after `init` |

---

## Authorization Mechanisms

All authorization in the contract uses **Soroban `Address::require_auth()`**, which verifies that the caller's signature (or the contract's internal auth) is present in the transaction envelope.

| Mechanism | Description | Used By |
|-----------|-------------|---------|
| **Single-party auth** | One address signs the call. | `withdraw`, `settle`, `record_sme_collateral_commitment`, `clear_sme_collateral_commitment`, `cancel_funding`, `set_legal_hold`, `clear_legal_hold`, `propose_admin`, `accept_admin`, `fund` |
| **Dual auth** | Two distinct addresses must both sign the same transaction. | `rotate_beneficiary` |
| **Role-gated (SME OR Admin)** | The caller may be either the SME or the admin. | `partial_settle` |

---

## Access Control Matrix

### Beneficiary-Related Entrypoints

| Entrypoint | Authorized Role | Allowed States | Legal Hold Gate | Pause Gate | Auth Mechanism |
|------------|----------------|----------------|----------------|------------|----------------|
| `rotate_beneficiary` | **SME + Admin** (both) | Open (0), Funded (1) | Blocks with `LegalHoldBlocksBeneficiaryRotation` (160) | None | `sme_address.require_auth()` + `admin.require_auth()` |
| `withdraw` | **SME** | Funded (1) | Blocks with `LegalHoldBlocksWithdrawal` (123) | Blocks with `PausedBlocksWithdrawal` (212) | `load_escrow_require_sme` → `sme_address.require_auth()` |
| `settle` | **SME** | Funded (1) | Blocks with `LegalHoldBlocksSettlement` (120) | Blocks with `PausedBlocksSettlement` (211) | `load_escrow_require_sme` → `sme_address.require_auth()` |
| `partial_settle` | **SME OR Admin** | Open (0) | Blocks with `LegalHoldBlocksPartialSettle` (201) | None | `caller.require_auth()` + `ensure(caller == sme_address \|\| caller == admin)` |
| `record_sme_collateral_commitment` | **SME** | Any | None | None | `load_escrow_require_sme` → `sme_address.require_auth()` |
| `clear_sme_collateral_commitment` | **SME** | Any | None | None | `load_escrow_require_sme` → `sme_address.require_auth()` |

> **Note:** `load_escrow_require_sme` (defined at `escrow/src/lib.rs:2299`) reads the escrow from storage and calls `escrow.sme_address.require_auth()` in one step. It panics with `EscrowNotInitialized` if no escrow exists.

### All Other Entrypoints (for context)

| Entrypoint | Authorized Role | Allowed State(s) | Legal Hold Gate | Pause Gate |
|------------|----------------|-----------------|----------------|------------|
| `init` | **Admin** | Uninitialized | No | No |
| `fund` / `fund_with_commitment` / `fund_batch` | **Investor** (self) | Open (0) | Yes | Yes |
| `cancel_funding` | **Admin** | Open (0) | Yes | No |
| `claim_investor_payout` | **Investor** (self) | Settled (2) | Yes | Yes |
| `refund` | **Investor** (self) | Cancelled (4) | No | No |
| `sweep_terminal_dust` | **Treasury** | Settled (2), Withdrawn (3), Cancelled (4) | Yes | No |
| `set_legal_hold` / `clear_legal_hold` | **Admin** | Any | N/A | No |
| `pause` / `unpause` | **Admin** | Any | No | N/A |
| `propose_admin` | **Admin** (current) | Any | No | No |
| `accept_admin` | **Pending Admin** | Any | No | No |

---

## Guard Ordering

Every entrypoint evaluates guards in a fixed order. The general pattern is:

1. **Pause gate** (if applicable) — check before any authorization or state mutation.
2. **Legal-hold gate** (if applicable) — read-only storage check.
3. **Authorization** — `require_auth()` for the relevant role(s).
4. **State precondition** — `ensure(status == expected)`.
5. **Business logic** — additional preconditions (maturity, balance, no-op guards, etc.).
6. **State transition + storage write + event emission.**

### `rotate_beneficiary` guard order (src/lib.rs:2233)

```
1. Legal-hold gate              → guard_not_legal_hold(LegalHoldBlocksBeneficiaryRotation)
2. Read escrow from storage
3. State gate (pre-settlement)  → ensure(is_pre_settlement_status, RotationNotOpen)
4. No-op guard                  → ensure(new != current, NewSmeSameAsCurrent)
5. Dual authorization           → sme_address.require_auth() + admin.require_auth()
6. Storage write + event emit   → BeneficiaryRotated, BenChange
```

### `withdraw` guard order (src/lib.rs:4371)

```
1. Pause gate                   → ensure(!paused_active, PausedBlocksWithdrawal)
2. Legal-hold gate              → guard_not_legal_hold(LegalHoldBlocksWithdrawal)
3. SME authorization            → load_escrow_require_sme → sme_address.require_auth()
4. State gate (funded)          → guard_status_eq(status, 1, WithdrawalNotFunded)
5. Balance check                → ensure(contract_balance >= amount, InsufficientContractBalance)
6. State transition + transfers → status = 3; token transfer to sme_address
```

### `settle` guard order (src/lib.rs:4279)

```
1. Pause gate                   → ensure(!paused_active, PausedBlocksSettlement)
2. Legal-hold gate              → guard_not_legal_hold(LegalHoldBlocksSettlement)
3. SME authorization            → load_escrow_require_sme → sme_address.require_auth()
4. Once-only guard              → ensure(status != 2, EscrowAlreadySettled)  (rejected here if already settled)
5. State gate (funded)          → ensure(status == 1, SettlementNotFunded)
6. Maturity gate                → ensure(now >= maturity, MaturityNotReached) if maturity > 0
7. State transition             → status = 2
```

---

## Rejection Codes

Every beneficiary-related rejection in the contract uses typed `EscrowError` variants:

| Code | Variant | Trigger | Entrypoint(s) |
|------|---------|---------|---------------|
| 160 | `LegalHoldBlocksBeneficiaryRotation` | Legal hold is active | `rotate_beneficiary` |
| 161 | `RotationNotOpen` | Status is not 0 or 1 (pre-settlement) | `rotate_beneficiary` |
| 162 | `NewSmeSameAsCurrent` | `new_sme_address` equals the current beneficiary | `rotate_beneficiary` |
| 120 | `LegalHoldBlocksSettlement` | Legal hold is active | `settle` |
| 121 | `SettlementNotFunded` | Status is not 1 (funded) and not 2 (already settled) | `settle` |
| 236 | `EscrowAlreadySettled` | Status is 2 (already settled); settlement is once-only | `settle` |
| 123 | `LegalHoldBlocksWithdrawal` | Legal hold is active | `withdraw` |
| 124 | `WithdrawalNotFunded` | Status is not 1 (funded) | `withdraw` |
| 201 | `LegalHoldBlocksPartialSettle` | Legal hold is active | `partial_settle` |
| 200 | `PartialSettleUnauthorizedCaller` | Caller is neither SME nor admin | `partial_settle` |
| 202 | `PartialSettleNotOpen` | Status is not 0 (open) | `partial_settle` |
| 212 | `PausedBlocksWithdrawal` | Operational pause is active | `withdraw` |
| 211 | `PausedBlocksSettlement` | Operational pause is active | `settle` |

See [`docs/escrow-error-messages.md`](./escrow-error-messages.md) for the full error catalogue.

---

## State-Dependent Authorization

```
                    ┌─────────────────────────────────┐
                    │  Status 0: Open                  │
                    │  rotate_beneficiary: ✅ (dual)   │
                    │  partial_settle: ✅ (SME|Admin)  │
                    │  record/clear collateral: ✅     │
                    │  withdraw: ❌ (not funded)       │
                    │  settle: ❌ (not funded)         │
                    └─────────────────────────────────┘
                                    │
                                    │ fund (investor)
                                    ▼
                    ┌─────────────────────────────────┐
                    │  Status 1: Funded                │
                    │  rotate_beneficiary: ✅ (dual)   │
                    │  withdraw: ✅ (SME)              │
                    │  settle: ✅ (SME)                │
                    │  record/clear collateral: ✅     │
                    │  partial_settle: ❌ (not open)   │
                    └─────────────────────────────────┘
                                   / \
                                  /   \
                        settle   /     \  withdraw
                        (SME)   /       \  (SME)
                              ▼           ▼
              ┌──────────────────┐  ┌──────────────────┐
              │ Status 2:Settled │  │ Status 3:Withdrn │
              │ rotate_ben: ❌  │  │ rotate_ben: ❌  │
              │ withdraw: ❌    │  │ withdraw: ❌    │
              │ collateral: ✅ │  │ collateral: ✅ │
              └──────────────────┘  └──────────────────┘
```

**Legal hold** (when active) blocks the following beneficiary-related entrypoints regardless of state:
- `rotate_beneficiary` (code 160)
- `withdraw` (code 123)
- `settle` (code 120)
- `partial_settle` (code 201)

**Operational pause** blocks `withdraw` (code 212) and `settle` (code 211), but does **not** block `rotate_beneficiary` or collateral operations.

---

## Worked Example

### Setup
An escrow is initialized:
- **Admin:** `G_ADMIN`
- **SME Beneficiary:** `G_SME_A`
- **Amount:** `100_000_000` XLM
- **Status:** `0` (open)

The contract stores `sme_address = G_SME_A`.

### Step 1: Rotate beneficiary (requires dual auth)
The SME wishes to change the payout destination to `G_SME_B`.

A single transaction must carry signatures from **both** `G_SME_A` (the current beneficiary) and `G_ADMIN`. The caller invokes:

```rust
rotate_beneficiary(env, G_SME_B)
```

Guard evaluation:
1. Legal hold inactive → passes.
2. Status is 0 (open) → `is_pre_settlement_status` returns true → passes.
3. `G_SME_B != G_SME_A` → passes (no-op guard).
4. `G_SME_A.require_auth()` verifies current SME's signature → passes.
5. `G_ADMIN.require_auth()` verifies admin's signature → passes.
6. Storage updated: `sme_address = G_SME_B`.

Post-rotation state:
- `sme_address` = `G_SME_B`
- Status remains 0
- Events: `BeneficiaryRotated` + `BenChange`

### Step 2: Fund (investor auth)
Investors fund the escrow. Once `funded_amount >= funding_target`, status transitions to `1` (funded).

### Step 3: Attempt withdrawal without SME auth → rejected
`G_SME_B` calls `withdraw()`:
- Pause inactive → passes.
- Legal hold inactive → passes.
- `load_escrow_require_sme` calls `sme_address.require_auth()` → verifies `G_SME_B`'s signature → passes.
- Status is 1 (funded) → passes.
- Balance sufficient → passes.
- State transitions to 3 (withdrawn), funds transfer to `G_SME_B`.

If `G_SME_A` (the former beneficiary) attempted `withdraw()`:
- `sme_address.require_auth()` would verify `G_SME_A`'s signature against the stored `G_SME_B` → **fails** with auth error.

### Step 4: Attempt rotation after withdrawal → rejected
Any call to `rotate_beneficiary` in status 3:
- Legal hold check passes.
- `is_pre_settlement_status(3)` returns **false** → panics with `RotationNotOpen` (161).

---

## Authorization Helper Functions

### `load_escrow_require_sme` (src/lib.rs:2299)

```rust
fn load_escrow_require_sme(env: &Env) -> InvoiceEscrow {
    let escrow = env.storage().instance()
        .get(&DataKey::Escrow)
        .unwrap_or_else(|| fail(env, EscrowError::EscrowNotInitialized));
    escrow.sme_address.require_auth();
    escrow
}
```

Used by: `withdraw`, `settle`, `record_sme_collateral_commitment`, `clear_sme_collateral_commitment`.

### `load_escrow_require_admin` (src/lib.rs:2285)

```rust
fn load_escrow_require_admin(env: &Env) -> InvoiceEscrow {
    let escrow = env.storage().instance()
        .get(&DataKey::Escrow)
        .unwrap_or_else(|| fail(env, EscrowError::EscrowNotInitialized));
    escrow.admin.require_auth();
    escrow
}
```

Used by: `cancel_funding`, `set_legal_hold`, `clear_legal_hold`, `pause`, `unpause`, admin-gated attestation operations.

### `is_pre_settlement_status` (src/lib.rs:713)

```rust
pub(crate) fn is_pre_settlement_status(status: u32) -> bool {
    matches!(status, 0 | 1)
}
```

Gates `rotate_beneficiary`.

---

## Cross-References

- [`docs/beneficiary.md`](./beneficiary.md) — Design doc, data model, behavioral invariants.
- [`docs/ESCROW_BENEFICIARY_ROTATION.md`](./ESCROW_BENEFICIARY_ROTATION.md) — Operator reference for rotation flow, dual auth rationale, event schemas.
- [`docs/STATE_MACHINE_IMPLEMENTATION.md`](./STATE_MACHINE_IMPLEMENTATION.md) — State machine with full transition matrix and forbidden transitions.
- [`docs/escrow-lifecycle.md`](./escrow-lifecycle.md) — Lifecycle doc with SME vs Admin role table.
- [`docs/escrow-error-messages.md`](./escrow-error-messages.md) — Typed error codes reference.
- [`docs/adr/ADR-002-auth-boundaries.md`](./adr/ADR-002-auth-boundaries.md) — Auth boundaries ADR.
- [`escrow/src/lib.rs`](../escrow/src/lib.rs) — Contract source with auth checks at lines 2233, 4236, 4279, 4371, 2754, 3037.
