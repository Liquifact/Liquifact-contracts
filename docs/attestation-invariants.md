# Escrow Attestation Invariants

**Status:** Accepted  
**Refs:** [`escrow/src/lib.rs`](../escrow/src/lib.rs), [`escrow/src/tests/attestations.rs`](../escrow/src/tests/attestations.rs), [`docs/escrow-attestations.md`](escrow-attestations.md), [`docs/escrow-error-messages.md`](escrow-error-messages.md)

---

## Overview

The LiquiFact escrow contract provides compliance chain-anchoring capabilities through 32-byte digest attestations (e.g. SHA-256 hashes of off-chain KYC/KYB documents, IPFS CIDs, or legal bundles). Attestations act as tamper-evident metadata pointers anchored to specific ledger sequences.

This document specifies the core state and behavioral **invariants** governing attestation storage, access control, bounds, revocations, and entrypoint execution. All production contract modifications must preserve these invariants.

---

## Summary of Attestation Invariants

| ID | Invariant Name | Short Description | Primary Enforcement | Error Code / Behavior |
|---|---|---|---|---|
| **INV-ATT-1** | Admin Authorization Boundary | All state-mutating attestation entrypoints require `InvoiceEscrow::admin` auth. | `load_escrow_require_admin` / `admin.require_auth()` | Stellar Auth Rejection |
| **INV-ATT-2** | Primary Attestation Single-Set Immutability | `PrimaryAttestationHash` is write-once; cannot be overwritten, cleared, or rebound. | `bind_primary_attestation_hash` | `PrimaryAttestationAlreadyBound` (50) |
| **INV-ATT-3** | Append Log Bounded Capacity | `AttestationAppendLog` is capped at `MAX_ATTESTATION_APPEND_ENTRIES` (32 digests). | `append_attestation_digest` | `AttestationAppendLogCapacityReached` (51) |
| **INV-ATT-4** | Append Log Positional Stability | Entries in `AttestationAppendLog` maintain 0-based index position and value indefinitely. | Append-only logic (`push_back`) | Immutable log sequence |
| **INV-ATT-5** | Revocation Index Range Boundary | Revocation and unrevocation operations must target a valid index (`index < log.len()`). | `require_attestation_index_in_range` | `AttestationIndexOutOfRange` (52) |
| **INV-ATT-6** | Single Revocation Idempotency | Revoking an index requires it to be currently unrevoked; duplicate revokes fail. | `revoke_attestation_digest` | `AttestationAlreadyRevoked` (53) |
| **INV-ATT-7** | Batch Revocation Atomicity & Bounds | Batch revocation must be non-empty, capped at 32 entries, and fail atomically on error. | `revoke_attestation_digests` | Codes 52, 53, 54 (`BatchEmpty`), 55 (`BatchTooLarge`) |
| **INV-ATT-8** | Unrevocation Precondition | Unrevoking an index requires it to be currently marked as revoked. | `unrevoke_attestation_digest` | `AttestationNotRevoked` (56) |
| **INV-ATT-9** | Non-Financial Isolation | Attestation state does not affect escrow financial logic, token transfers, or settlement. | Decoupled storage & logic | Zero financial side-effects |
| **INV-ATT-10** | Event Transparency | Every attestation storage write publishes a structured event (`att_bind`, `att_app`, `att_rev`, `att_unrev`). | Event publishing | Contract Event Stream |

---

## Detailed Invariant Specifications

### INV-ATT-1: Admin Authorization Boundary

State-mutating attestation entrypoints—`bind_primary_attestation_hash`, `append_attestation_digest`, `revoke_attestation_digest`, `revoke_attestation_digests`, and `unrevoke_attestation_digest`—must be invoked by or signed with the authority of the escrow instance's `InvoiceEscrow::admin`.

- **Enforcement Location:** 
  - `bind_primary_attestation_hash` and `append_attestation_digest` call `Self::load_escrow_require_admin(&env)`.
  - `revoke_attestation_digest`, `revoke_attestation_digests`, and `unrevoke_attestation_digest` verify `escrow.admin.require_auth()`.
- **Read-Only Exception:** Read functions (`get_primary_attestation_hash`, `get_attestation_append_log`, `get_attestation_digest_at`, `is_attestation_revoked`, `get_revoked_attestation_digests`, `get_escrow_summary`) require no authorization.

---

### INV-ATT-2: Primary Attestation Single-Set Immutability

The primary compliance anchor (`DataKey::PrimaryAttestationHash`) represents the baseline origination document for the invoice/SME. It is strictly write-once.

- **Rule:** If `DataKey::PrimaryAttestationHash` is already set in instance storage, any call to `bind_primary_attestation_hash` MUST fail immediately with `EscrowError::PrimaryAttestationAlreadyBound` (code 50).
- **Enforcement Location:** [`escrow/src/lib.rs`](../escrow/src/lib.rs) inside `bind_primary_attestation_hash`.
- **Lifecycle Guarantee:** No entrypoint exists to update, overwrite, or delete `DataKey::PrimaryAttestationHash`.

---

### INV-ATT-3: Append Log Bounded Capacity

The attestation append log (`DataKey::AttestationAppendLog`) stores an ordered Soroban `Vec<BytesN<32>>` of incremental compliance updates.

- **Rule:** The total count of appended digests MUST NOT exceed `MAX_ATTESTATION_APPEND_ENTRIES` (32 entries).
- **Enforcement Location:** [`escrow/src/lib.rs`](../escrow/src/lib.rs) inside `append_attestation_digest`.
- **Behavior:** Pushing a 33rd entry fails with `EscrowError::AttestationAppendLogCapacityReached` (code 51). Integrators can monitor capacity via `get_attestation_append_log().len()`.

---

### INV-ATT-4: Append Log Positional Stability

Elements in `DataKey::AttestationAppendLog` form an append-only log.

- **Rule:** Once written at index $i$, entry $i$ ($0 \le i < \text{log.len()}$) retains its position and exact 32-byte value. Elements are never shifted, reordered, or removed.
- **Revocation Model:** Revoking an attestation does not modify `AttestationAppendLog`. Instead, an independent flag `DataKey::AttestationRevoked(index)` is set to `true`.

---

### INV-ATT-5: Revocation Index Range Boundary

Revocation and unrevocation operate on 0-based indices into `AttestationAppendLog`.

- **Rule:** Any call to `revoke_attestation_digest`, `revoke_attestation_digests`, or `unrevoke_attestation_digest` with index $i \ge \text{log.len()}$ MUST fail with `EscrowError::AttestationIndexOutOfRange` (code 52).
- **Enforcement Location:** Consolidated helper `require_attestation_index_in_range` in [`escrow/src/lib.rs`](../escrow/src/lib.rs).

---

### INV-ATT-6: Single Revocation Idempotency

Revocation flags an attestation index as superseded.

- **Rule:** An index $i$ can only be revoked if `DataKey::AttestationRevoked(i)` is not currently set.
- **Enforcement Location:** [`escrow/src/lib.rs`](../escrow/src/lib.rs) in `revoke_attestation_digest` and `revoke_attestation_digests`.
- **Behavior:** Attempting to revoke an already-revoked index fails with `EscrowError::AttestationAlreadyRevoked` (code 53).

---

### INV-ATT-7: Batch Revocation Atomicity & Bounds

`revoke_attestation_digests` allows batching multiple index revocations into a single transaction.

- **Bounds:** 
  - `indices.len() == 0` fails with `EscrowError::AttestationBatchEmpty` (code 54).
  - `indices.len() > MAX_ATTESTATION_REVOKE_BATCH` (32) fails with `EscrowError::AttestationBatchTooLarge` (code 55).
- **Atomicity:** All indices are validated in loop order. If ANY index fails range checks or is already revoked (including duplicate indices within the input batch), the transaction reverts with no partial state updates.

---

### INV-ATT-8: Unrevocation Precondition

`unrevoke_attestation_digest` enables reversing a mistakenly applied revocation flag.

- **Rule:** An index $i$ can only be unrevoked if `DataKey::AttestationRevoked(i)` is currently set.
- **Enforcement Location:** [`escrow/src/lib.rs`](../escrow/src/lib.rs) in `unrevoke_attestation_digest`.
- **Behavior:** Attempting to unrevoke an unrevoked index fails with `EscrowError::AttestationNotRevoked` (code 56). Upon success, `DataKey::AttestationRevoked(i)` is removed from storage.

---

### INV-ATT-9: Non-Financial Isolation

Attestation records are metadata pointers for off-chain verifiers and indexers.

- **Rule:** Attestation state (`PrimaryAttestationHash`, `AttestationAppendLog`, `AttestationRevoked`) MUST NEVER alter financial operations, status transitions, token transfer amounts, legal hold enforcement, or payout calculations.
- **Enforcement:** No state-changing entrypoint for financial flows (`fund`, `settle`, `withdraw`, `refund`, `claim_investor_payout`, `sweep_terminal_dust`) inspects attestation storage.

---

### INV-ATT-10: Event Transparency

All attestation state modifications publish structured Soroban contract events for off-chain indexing.

- **Event Mapping:**
  - `bind_primary_attestation_hash` $\rightarrow$ `PrimaryAttestationBound` (`att_bind`)
  - `append_attestation_digest` $\rightarrow$ `AttestationDigestAppended` (`att_app`)
  - `revoke_attestation_digest` / `revoke_attestation_digests` $\rightarrow$ `AttestationDigestRevoked` (`att_rev`)
  - `unrevoke_attestation_digest` $\rightarrow$ `AttestationDigestUnrevoked` (`att_unrev`)

---

## Entrypoint Cross-Reference Matrix

| Entrypoint | Type | Required Auth | Storage Keys Read / Written | Validated Error Codes | Event Emitted |
|---|---|---|---|---|---|
| `bind_primary_attestation_hash` | Mutating | `admin` | W: `PrimaryAttestationHash` | `PrimaryAttestationAlreadyBound` (50) | `PrimaryAttestationBound` |
| `append_attestation_digest` | Mutating | `admin` | R/W: `AttestationAppendLog` | `AttestationAppendLogCapacityReached` (51) | `AttestationDigestAppended` |
| `revoke_attestation_digest` | Mutating | `admin` | R: `AttestationAppendLog`<br>W: `AttestationRevoked(i)` | `AttestationIndexOutOfRange` (52)<br>`AttestationAlreadyRevoked` (53) | `AttestationDigestRevoked` |
| `revoke_attestation_digests` | Mutating | `admin` | R: `AttestationAppendLog`<br>W: `AttestationRevoked(i)` | `AttestationIndexOutOfRange` (52)<br>`AttestationAlreadyRevoked` (53)<br>`AttestationBatchEmpty` (54)<br>`AttestationBatchTooLarge` (55) | `AttestationDigestRevoked` (per index) |
| `unrevoke_attestation_digest` | Mutating | `admin` | R: `AttestationAppendLog`<br>Del: `AttestationRevoked(i)` | `AttestationIndexOutOfRange` (52)<br>`AttestationNotRevoked` (56) | `AttestationDigestUnrevoked` |
| `get_primary_attestation_hash` | Read-only | None | R: `PrimaryAttestationHash` | None | None |
| `get_attestation_append_log` | Read-only | None | R: `AttestationAppendLog` | None | None |
| `get_attestation_digest_at` | Read-only | None | R: `AttestationAppendLog`<br>R: `AttestationRevoked(i)` | None | None |
| `is_attestation_revoked` | Read-only | None | R: `AttestationRevoked(i)` | None | None |
| `get_revoked_attestation_digests` | Read-only | None | R: `AttestationAppendLog`<br>R: `AttestationRevoked(i)` | Bounded by `MAX_ATTESTATION_READ_PAGE` (20) | None |

---

## Data Model & Storage Layout

All attestation keys are stored in **Instance Storage**:

```rust
pub enum DataKey {
    /// Single-set primary digest. Written once via bind_primary_attestation_hash.
    PrimaryAttestationHash,
    /// Bounded append-only log of digests (max 32).
    AttestationAppendLog,
    /// Per-index revocation boolean marker.
    AttestationRevoked(u32),
}
```

---

## Test Verification Mapping

The invariants specified above are exhaustively verified in [`escrow/src/tests/attestations.rs`](../escrow/src/tests/attestations.rs):

| Invariant ID | Key Test Functions in `attestations.rs` |
|---|---|
| **INV-ATT-1** | `test_bind_primary_hash_non_admin_fails`, `test_append_digest_non_admin_fails`, `test_revoke_non_admin_fails`, `test_unrevoke_non_admin_fails` |
| **INV-ATT-2** | `test_bind_primary_hash_same_digest_fails`, `test_bind_primary_hash_different_digest_fails` |
| **INV-ATT-3** | `test_append_log_capacity_cap_exact_32`, `test_append_log_33rd_fails` |
| **INV-ATT-4** | `test_append_log_maintains_order_and_index` |
| **INV-ATT-5** | `test_revoke_out_of_bounds_fails`, `test_unrevoke_out_of_bounds_fails` |
| **INV-ATT-6** | `test_revoke_already_revoked_fails`, `test_revoke_double_call_fails` |
| **INV-ATT-7** | `test_revoke_digests_batch_happy_path`, `test_revoke_digests_batch_empty_fails`, `test_revoke_digests_batch_too_large_fails`, `test_revoke_digests_batch_duplicate_fails` |
| **INV-ATT-8** | `test_unrevoke_attestation_digest_reverses_revocation`, `test_unrevoke_not_revoked_fails` |
| **INV-ATT-9** | `test_attestations_do_not_interfere_with_funding_or_settlement` |
| **INV-ATT-10** | `test_bind_primary_hash_stores_and_reads`, `test_append_digest_emits_event`, `test_revoke_emits_event`, `test_unrevoke_emits_event` |
