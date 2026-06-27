# Beneficiary Rotation (SME) — dual authorization & downstream routing

The LiquiFact escrow contract supports a governed on-chain rotation of the **SME beneficiary** (the address that receives the escrow’s funded principal on `withdraw`).

This document is the authoritative, code-accurate reference for the `rotate_beneficiary` flow, including its **dual-authorization requirement**, its **exact guard ordering**, and its **operator-facing rejection codes**.

> **Downstream impact:** `rotate_beneficiary` changes the `sme_address` stored in contract state. Later SME-gated disbursement (`withdraw`) uses the *current* `sme_address`, so rotation determines where the funded principal is routed.

---

## Entry point

### `rotate_beneficiary`

```rust
pub fn rotate_beneficiary(env: Env, new_sme_address: Address) -> InvoiceEscrow
```

#### What it updates

- `InvoiceEscrow::sme_address` is atomically updated from the current SME to `new_sme_address`.

#### Authorization model (why both signatures are required)

This entrypoint enforces **dual authorization**:

1. **Outgoing SME** (`escrow.sme_address.require_auth()`)
2. **Current admin** (`escrow.admin.require_auth()`)

Both must sign in the same transaction. This prevents unilateral redirection of the withdrawal destination by:

- a compromised admin key alone (admin cannot rotate without the SME signing), and
- a compromised SME key alone (SME cannot rotate without the admin signing).

#### Exact guard ordering (code-accurate)

`rotate_beneficiary` evaluates guards in this order:

1. **Legal-hold gate (read-only)**
   - Condition: `!legal_hold_active`
   - If `LegalHold` is active, the call aborts immediately.

2. **State gate (allowed states only)**
   - Condition: `escrow.status == 0 || escrow.status == 1`
   - Meaning:
     - `0` = **open** (pre-settlement)
     - `1` = **funded** (still pre-settlement)

3. **No-op guard**
   - Condition: `new_sme_address != escrow.sme_address`
   - Rotating to the current address is rejected.

4. **Dual authorization**
   - `escrow.sme_address.require_auth()`
   - `escrow.admin.require_auth()`

5. **Storage write + event emission**
   - Persists the updated `sme_address` into `DataKey::Escrow`.
   - Emits `BeneficiaryRotated`.

---

## Allowed states

Rotation is only permitted in **pre-settlement** states:

- `status = 0` (**open**)
- `status = 1` (**funded**)

Rotation is rejected in terminal/post-settlement states:

- `status = 2` (**settled**)
- `status = 3` (**withdrawn**)
- `status = 4` (**cancelled**)

---

## Operator-facing rejection codes

These are the typed `EscrowError` variants emitted by `rotate_beneficiary`:

- **`LegalHoldBlocksBeneficiaryRotation` (160)**
  - Trigger: legal hold is active.
  - Meaning: compliance/legal hold blocks beneficiary rotation.

- **`RotationNotOpen` (161)**
  - Trigger: escrow is not in a pre-settlement state.
  - Meaning: `status` must be `0` (open) or `1` (funded).

- **`NewSmeSameAsCurrent` (162)**
  - Trigger: `new_sme_address == escrow.sme_address`.
  - Meaning: no-op rotations are rejected.

---

## Downstream effect on `withdraw`

`withdraw` is SME-gated and sends the funded principal to the **current** stored `sme_address`.

So after a successful rotation:

- `withdraw` will route disbursement to the **new** SME beneficiary.
- the new SME becomes the authority for subsequent SME-gated flows.

### Eventing for indexers

- `rotate_beneficiary` emits **`BeneficiaryRotated`** (with `prior_sme` and `new_sme`).
- After rotation, later SME disbursement emits **`SmeWithdrew`**.

Indexers should:

1. update their internal “active SME” mapping on `BeneficiaryRotated`, then
2. attribute a later `SmeWithdrew` to the SME that was current after the rotation.

---

## Event schema

### `BeneficiaryRotated`

Emitted after successful `rotate_beneficiary`.

Fields:

- `name`: `ben_rot`
- `invoice_id`: the escrow invoice id
- `prior_sme`: previous SME address
- `new_sme`: updated SME address

---

## Security notes (operator guidance)

- Rotation is intentionally **not** a proposal/accept flow. It is a single call requiring both the outgoing SME and admin signatures.
- Legal hold blocks beneficiary rotation before any authorization checks run.
- Rotation only affects the withdrawal destination (`sme_address`). It does not move tokens directly; token routing happens in `withdraw`.
- If you operate with multisig governance, ensure the admin key used for `rotate_beneficiary` signing cannot be invoked unilaterally without SME consent (and vice-versa), matching the intended dual-control policy.

