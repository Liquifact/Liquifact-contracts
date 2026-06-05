# Beneficiary Rotation System

The LiquiFact escrow contract provides a governed on-chain SME beneficiary rotation via the
`rotate_beneficiary` entrypoint.

This operation updates the stored `sme_address` atomically so the new SME immediately becomes the
sole authority for `withdraw`, `settle`, and `record_sme_collateral_commitment`.

## Overview

- `rotate_beneficiary` requires dual consent from both the current SME and the admin.
- Rotation is allowed only in non-terminal escrow states: `0` (open) or `1` (funded).
- Rotation is blocked while a legal hold is active.
- Rotation to the current SME address is rejected.
- A `BeneficiaryRotated` event is emitted on success.

## Entrypoint

### `rotate_beneficiary`

```rust
pub fn rotate_beneficiary(env: Env, new_sme_address: Address) -> InvoiceEscrow
```

#### Requirements

- `new_sme_address` must differ from the current `sme_address`.
- Escrow status must be `0` (open) or `1` (funded).
- `escrow.sme_address.require_auth()` must succeed.
- `escrow.admin.require_auth()` must succeed.
- `LegalHold` must not be active.

#### Effects

- Updates `InvoiceEscrow::sme_address` to `new_sme_address`.
- Persists the updated escrow state.
- Emits `BeneficiaryRotated` with both prior and new SME addresses.
- The new SME immediately gains authority for SME-gated actions.

#### Errors

- `LegalHoldBlocksBeneficiaryRotation` when legal hold is active.
- `RotationNotOpen` when the escrow has reached a terminal state.
- `NewSmeSameAsCurrent` when the new SME address equals the current SME.

## Authorization Model

`rotate_beneficiary` enforces dual authorization:

1. Current SME authorization via `escrow.sme_address.require_auth()`.
2. Current admin authorization via `escrow.admin.require_auth()`.

This ensures neither party can rotate the SME beneficiary alone.

## State & Security Notes

- Rotation is prohibited in terminal states (`settled`, `withdrawn`, `cancelled`).
- After rotation, the new SME controls all SME-authorized entrypoints.
- Collateral metadata ownership transfers with the new SME because
  `record_sme_collateral_commitment` is also authenticated against the stored `sme_address`.

## Event

`BeneficiaryRotated` is emitted after a successful rotation.

- `name`: `ben_rot`
- `invoice_id`: invoice identifier
- `prior_sme`: previous SME address
- `new_sme`: updated SME address

## Example

```ignore
let updated = client.rotate_beneficiary(&new_sme_address);
assert_eq!(updated.sme_address, new_sme_address);
```

## Notes for Integrators

- There is no proposal/accept timelock flow in the contract implementation.
- Off-chain systems should treat `sme_address` as updatable via this governed entrypoint.
- Legal hold must be cleared before rotation can succeed.
