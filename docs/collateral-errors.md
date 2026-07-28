# Liquifact Escrow Collateral Error Codes

This document details the typed Soroban contract errors associated with the **SME Collateral Commitment** feature in the `Liquifact` escrow contract. Client SDKs and integration libraries can use these codes to branch programmatically on specific failure states.

For a complete list of all escrow error codes, see [`docs/escrow-error-messages.md`](escrow-error-messages.md). For details on how the collateral system operates, see [`docs/escrow-sme-collateral.md`](escrow-sme-collateral.md).

---

## Stability and Category Mapping

Collateral-specific errors are grouped under the **SME Collateral** range (codes `60–62`) and the general **Funding deadline / balance / state** range (code `169`).

All errors in this contract are defined in [`escrow/src/lib.rs`](../escrow/src/lib.rs) inside the `EscrowError` enum. They are stable, append-only, and represented as `u32` values.

---

## Collateral Error Codes Reference

### 1. `EscrowNotInitialized` (Code `20`)
* **Variant:** `EscrowError::EscrowNotInitialized`
* **Entrypoints:** 
  * `record_sme_collateral_commitment`
  * `clear_sme_collateral_commitment`
* **Trigger Condition:** The contract instance storage does not contain the core escrow configuration (`DataKey::Escrow`). This check is performed via `load_escrow_require_sme`.
* **How to Avoid:** Ensure the escrow contract instance has been initialized by calling `init` before attempting any collateral-related operations.

---

### 2. `CollateralAmountNotPositive` (Code `60`)
* **Variant:** `EscrowError::CollateralAmountNotPositive`
* **Entrypoint:** `record_sme_collateral_commitment`
* **Trigger Condition:** The `amount` parameter passed to the function is less than or equal to zero (`amount <= 0`).
* **How to Avoid:** Provide a strictly positive metadata amount (e.g. `amount > 0`).

---

### 3. `CollateralAssetEmpty` (Code `61`)
* **Variant:** `EscrowError::CollateralAssetEmpty`
* **Entrypoint:** `record_sme_collateral_commitment`
* **Trigger Condition:** The `asset` Symbol parameter passed to the function is empty (i.e. `Symbol::new(&env, "")`).
* **How to Avoid:** Provide a non-empty symbol representing the asset identifier (e.g. `USDC`).

---

### 4. `CollateralTimestampBackwards` (Code `62`)
* **Variant:** `EscrowError::CollateralTimestampBackwards`
* **Entrypoint:** `record_sme_collateral_commitment`
* **Trigger Condition:** A prior commitment pledge exists in storage, and the current ledger timestamp (`env.ledger().timestamp()`) is strictly less than the stored commitment's `recorded_at` timestamp.
* **How to Avoid:** When updating/replacing an existing collateral commitment, ensure that the transaction is submitted with a monotonic ledger time that is greater than or equal to the timestamp of the prior commitment.

---

### 5. `NoCollateralToClear` (Code `169`)
* **Variant:** `EscrowError::NoCollateralToClear`
* **Entrypoint:** `clear_sme_collateral_commitment`
* **Trigger Condition:** The function attempts to clear the SME collateral commitment but no commitment pledge (`DataKey::SmeCollateralPledge`) is currently stored in the contract instance.
* **How to Avoid:** Check the status of the collateral commitment using `get_sme_collateral_commitment` before attempting to clear it. Do not invoke the clear entrypoint if no commitment exists.

---

## Authorization Failures vs Typed Errors

All collateral-mutating entrypoints require authorization from the configured SME address (`InvoiceEscrow::sme_address`):

- `record_sme_collateral_commitment` enforces authorization by invoking `sme_address.require_auth()`.
- `clear_sme_collateral_commitment` enforces authorization by invoking `sme_address.require_auth()`.

If an unauthorized address invokes these functions, Soroban's native authentication framework will reject the invocation with a standard host authorization failure rather than returning a custom `EscrowError` code.
