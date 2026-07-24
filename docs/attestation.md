# Attestation Model

This document describes the attestation system in the Liquifact escrow contract, including its data model, invariants, and usage patterns.

## Overview

The attestation system allows administrators to bind cryptographic proofs (hashes) to escrow contracts. It supports two types of attestations:

1. **Primary Attestation** - A single, immutable hash representing the primary attestation (e.g., invoice hash)
2. **Append Log** - An ordered log of up to 32 additional attestation digests for versioned updates

## Data Model

### Primary Attestation

```rust
pub fn get_primary_attestation_hash(env: Env) -> Option<BytesN<32>>
```

- **Storage**: `DataKey::PrimaryAttestationHash` (instance storage)
- **Type**: `Option<BytesN<32>>` - either `None` (unset) or `Some(hash)`
- **Mutability**: Write-once - can only be set once via `bind_primary_attestation_hash()`

### Append Log

```rust
pub fn get_attestation_append_log(env: Env) -> Vec<BytesN<32>>
```

- **Storage**: `DataKey::AttestationAppendLog` (instance storage)
- **Type**: `Vec<BytesN<32>>` - ordered list of digests
- **Capacity**: Maximum 32 entries (`MAX_ATTESTATION_APPEND_ENTRIES`)
- **Mutability**: Append-only - entries can be added but not removed

### Attestation State View

```rust
pub struct AttestationState {
    pub primary_hash: Option<BytesN<32>>,
    pub append_log: Vec<BytesN<32>>,
    pub append_log_len: u32,
    pub remaining_capacity: u32,
}

pub fn get_attestation_state(env: Env) -> AttestationState
```

A read-only view that returns the complete attestation state in a single call.

## Entrypoints

### Primary Attestation

| Function | Description | Auth |
|----------|-------------|------|
| `bind_primary_attestation_hash(digest)` | Bind the primary attestation hash | Admin only |
| `get_primary_attestation_hash()` | Read the primary hash | Public |

### Append Log

| Function | Description | Auth |
|----------|-------------|------|
| `append_attestation_digest(digest)` | Add a digest to the log | Admin only |
| `get_attestation_append_log()` | Read the full log | Public |
| `get_attestation_digest_at(index)` | Read digest at specific index | Public |
| `revoke_attestation_digest(index)` | Mark a digest as revoked | Admin only |
| `unrevoke_attestation_digest(index)` | Unmark a revoked digest | Admin only |
| `is_attestation_revoked(index)` | Check if digest is revoked | Public |

### State View

| Function | Description | Auth |
|----------|-------------|------|
| `get_attestation_state()` | Get complete attestation state | Public |

## Invariants

### Primary Attestation Invariants

1. **Write-Once**: The primary hash can only be set once. Attempting to bind a second time returns `EscrowError::PrimaryAttestationAlreadyBound`.

2. **Immutability**: Once set, the primary hash cannot be changed or removed.

3. **Admin-Only**: Only the admin can bind the primary attestation.

### Append Log Invariants

1. **Capacity Bound**: The log can hold at most 32 entries. Attempting to append beyond this returns `EscrowError::AttestationAppendLogCapacityReached`.

2. **Append-Only**: Entries cannot be removed from the log. Revocation only marks entries as revoked but does not remove them.

3. **Ordered**: Entries maintain insertion order. Index 0 is the first entry, index 31 is the last possible entry.

4. **Revocation is Reversible**: Revoked entries can be unrevoked. The `is_attestation_revoked()` function returns the current revocation status.

5. **Admin-Only Writes**: All write operations (`append`, `revoke`, `unrevoke`) require admin authentication.

### State View Invariants

1. **Consistency**: `append_log_len` always equals `append_log.len()`.

2. **Capacity Calculation**: `remaining_capacity` always equals `MAX_ATTESTATION_APPEND_ENTRIES - append_log_len`.

3. **Read-Only**: The state view does not modify any storage.

## Usage Patterns

### Pattern 1: Single Invoice Attestation

For a simple escrow with a single invoice document:

```rust
// Bind the invoice hash as primary attestation
let invoice_hash = sha256(invoice_document);
client.bind_primary_attestation_hash(&invoice_hash);

// Later, verify the attestation
let stored_hash = client.get_primary_attestation_hash();
assert_eq!(stored_hash, Some(invoice_hash));
```

### Pattern 2: Versioned Attestations

For escrows requiring multiple attestations over time:

```rust
// Bind primary attestation
client.bind_primary_attestation_hash(&primary_hash);

// Append versioned updates
client.append_attestation_digest(&v1_hash);
client.append_attestation_digest(&v2_hash);
client.append_attestation_digest(&v3_hash);

// Get complete state
let state = client.get_attestation_state();
assert_eq!(state.primary_hash, Some(primary_hash));
assert_eq!(state.append_log_len, 3);
assert_eq!(state.remaining_capacity, 29);
```

### Pattern 3: Revocable Attestations

For attestations that may need to be revoked:

```rust
// Append attestations
client.append_attestation_digest(&hash1);
client.append_attestation_digest(&hash2);

// Revoke the first attestation
client.revoke_attestation_digest(&0);
assert!(client.is_attestation_revoked(&0));
assert!(!client.is_attestation_revoked(&1));

// Later, unrevoke if needed
client.unrevoke_attestation_digest(&0);
assert!(!client.is_attestation_revoked(&0));
```

## Error Handling

### Primary Attestation Errors

| Error | Condition |
|-------|-----------|
| `PrimaryAttestationAlreadyBound` | Attempting to bind when already set |
| `NotAuthorized` | Non-admin caller |

### Append Log Errors

| Error | Condition |
|-------|-----------|
| `AttestationAppendLogCapacityReached` | Log is full (32 entries) |
| `AttestationIndexOutOfRange` | Index >= log length |
| `AttestationAlreadyRevoked` | Revoking an already-revoked entry |
| `AttestationNotRevoked` | Unrevoking a non-revoked entry |
| `NotAuthorized` | Non-admin caller |

## Security Considerations

1. **Admin Authority**: All write operations require admin authentication. Ensure admin keys are properly secured.

2. **Hash Integrity**: The contract does not validate the content of hashes - it only stores them. Callers must ensure hashes are computed correctly (e.g., using SHA-256).

3. **Revocation Semantics**: Revocation marks an entry as revoked but does not remove it. Indexers and off-chain systems should check `is_attestation_revoked()` before considering an attestation valid.

4. **Capacity Planning**: With a maximum of 32 append entries, plan attestation usage carefully. For high-frequency updates, consider using the primary attestation with external versioning.

## Testing

The attestation system is thoroughly tested in `escrow/src/tests/attestations.rs`. Key test categories:

- Primary attestation binding and immutability
- Append log capacity and ordering
- Revocation and unrevocation
- State view consistency
- Error conditions and edge cases

Run tests with:
```bash
cargo test --package escrow --test attestations
```

## Migration Notes

The attestation system was introduced in contract version 6. Older escrow contracts do not have attestation support. When upgrading:

1. Ensure the contract is initialized with version >= 6
2. Existing escrows can use attestation features immediately after upgrade
3. No data migration is required - attestation storage is separate from core escrow data
