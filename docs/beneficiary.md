# SME Beneficiary Model and Invariants

This document describes the design, storage layout, behavioral invariants, and entrypoint interaction model of the SME beneficiary in the LiquiFact escrow contract.

## Data Model & Storage

The beneficiary is represented in the contract state as the `sme_address` (Address) field within the `InvoiceEscrow` struct.

### Storage Layout

The `InvoiceEscrow` struct is persisted in contract instance storage under `DataKey::Escrow`:

```rust
pub struct InvoiceEscrow {
    pub invoice_id: Symbol,
    pub admin: Address,
    pub sme_address: Address, // Current SME beneficiary address
    pub amount: i128,
    pub funding_target: i128,
    pub funded_amount: i128,
    pub yield_bps: i64,
    pub maturity: u64,
    pub status: u32,
}
```

## Behavioral Invariants

The contract maintains the following invariants regarding the beneficiary:

### 1. Identity & Rotation Governance
- **Dual Authorization Requirement:** The `sme_address` cannot be modified unilaterally by either the `admin` or the `sme_address` alone. Any update requires a valid signature from both the current admin and the current SME beneficiary.
- **Reject No-Op Rotations:** Rotation to the same address currently configured as the beneficiary is rejected with error code 162 (`NewSmeSameAsCurrent`).
- **Maturity & State Gates:** Rotation is only permitted before the invoice escrow is settled, withdrawn, or cancelled. The `status` must be either `0` (open) or `1` (funded). Otherwise, the call is rejected with error code 161 (`RotationNotOpen`).
- **Legal Hold Lockout:** If a compliance/legal hold is active, any attempt to rotate the beneficiary is blocked immediately with error code 160 (`LegalHoldBlocksBeneficiaryRotation`).

### 2. Disbursement Destination
- **Disbursement Routing:** The `withdraw` entrypoint sends the net funded amount (gross funded amount minus the protocol fee) exclusively to the stored `sme_address` at the time of the withdrawal. Once status is transitioned to `3` (withdrawn), the funds are dispersed and the state is terminal.

### 3. Gated Entrypoints
Only the configured `sme_address` is authorized to invoke or sign the following operations:
- `withdraw` (SME pull principal/yield)
- `record_sme_collateral_commitment` (Report metadata for off-chain risk)
- `clear_sme_collateral_commitment` (Clear reported collateral metadata)
- `partial_settle` (Authorized by either the SME or the admin)

## Worked Example

### 1. Initialization
An escrow is initialized with a target amount of `100_000_000` XLM.
- **Admin:** `G_ADMIN`
- **SME Beneficiary:** `G_SME_A`
- **Status:** `0` (open)

The contract stores `sme_address = G_SME_A`.

### 2. Rotation
The SME wants to rotate the beneficiary to a new wallet, `G_SME_B`.
- The rotation transaction must include signatures from both `G_SME_A` and `G_ADMIN`.
- The admin invokes `rotate_beneficiary(new_sme_address = G_SME_B)`.
- The contract verifies the dual authorization and updates `sme_address` to `G_SME_B`.
- The contract emits:
  1. `BeneficiaryRotated` event carrying:
     - `prior_sme`: `G_SME_A`
     - `new_sme`: `G_SME_B`
  2. `BenChange` event (topic name: `ben_chg`) carrying:
     - `prior_sme`: `G_SME_A`
     - `new_sme`: `G_SME_B`
     - `amount`: `100_000_000`

### 3. Withdrawal
Once funding target is met and the status changes to `1` (funded), the beneficiary withdraws.
- `G_SME_B` calls `withdraw()`.
- The contract checks `sme_address.require_auth()` which validates `G_SME_B`'s signature.
- The contract disperses the funds to `G_SME_B`.
