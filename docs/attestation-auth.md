# Attestation Authorization and Access Rules

Documents **who** may call each attestation entrypoint, **when**, and **what happens on
rejection** -- for auditors, integrators, and reviewers. Verified directly against
`escrow/src/lib.rs` (line references below); this is not a design proposal.

> **Scope:** "attestation" in this contract means on-chain 32-byte digest anchoring
> (e.g. SHA-256 of an IPFS CID or document bundle hash). It is purely metadata --
> no ZK verification, no oracle interaction, no token movement. See
> [`docs/escrow-attestations.md`](escrow-attestations.md) for operational KYC/KYB flows.

---

## Roles

| Role | Attestation capability |
|------|------------------------|
| **Admin** (`InvoiceEscrow::admin`) | Sole authority over all state-mutating attestation entrypoints: bind, append (single and batch), revoke (single and batch), and unrevoke. |
| **SME** (`InvoiceEscrow::sme_address`) | No attestation capability. |
| **Investor** | No attestation capability. |
| **Treasury** (`DataKey::Treasury`) | No attestation capability. |
| **Anyone** | All read-only attestation views are callable without authorization. |

**No separate attestation operator role exists.** The single `admin` key gates every
write. Production deployments should use a multisig or governed contract as admin so no
single key can bind an arbitrary digest. See
[`docs/adr/ADR-002-auth-boundaries.md`](adr/ADR-002-auth-boundaries.md).

---

## Authorization Mechanism

All mutating attestation entrypoints use **Soroban `Address::require_auth()`** on the
`admin` address, enforced through one of two patterns:

### Pattern 1 -- `load_escrow_require_admin` helper

```rust
fn load_escrow_require_admin(env: &Env) -> InvoiceEscrow {
    let escrow: InvoiceEscrow = env
        .storage()
        .instance()
        .get(&DataKey::Escrow)
        .unwrap_or_else(|| fail(env, EscrowError::EscrowNotInitialized));
    escrow.admin.require_auth();
    escrow
}
```

Used by: `bind_primary_attestation_hash` (line 3277), `append_attestation_digest`
(line 3308), `append_attestation_digests` (line 3372).

### Pattern 2 -- Direct `escrow.admin.require_auth()` after `get_escrow`

```rust
let escrow = Self::get_escrow(env.clone());
escrow.admin.require_auth();
```

Used by: `revoke_attestation_digest` (line 3834), `revoke_attestation_digests`
(line 3885), `unrevoke_attestation_digest` (line 3976).

Both patterns are equivalent in effect: load escrow, verify caller is admin, proceed.
Pattern 2 is used when additional read-only validation occurs before auth (e.g. batch
size checks in `revoke_attestation_digests` and `append_attestation_digests`).

---

## Access Control Matrix

### Mutating Entrypoints (admin-only)

| Entrypoint | Required Auth | Legal Hold Gate | Pause Gate | State Precondition | Atomicity |
|------------|---------------|-----------------|------------|-------------------|-----------|
| `bind_primary_attestation_hash` | `admin` | No | No | Uninitialized (write-once) | Single write |
| `append_attestation_digest` | `admin` | No | No | `log.len() < 32` | Single write |
| `append_attestation_digests` | `admin` | No | No | `log.len() + n <= 32` | All-or-nothing batch |
| `revoke_attestation_digest` | `admin` | No | No | `index < log.len()`, not yet revoked | Single write |
| `revoke_attestation_digests` | `admin` | No | No | All indices valid and unrevoked | Full batch rollback on any failure |
| `unrevoke_attestation_digest` | `admin` | No | No | `index < log.len()`, currently revoked | Single write |

**Key observation:** Attestation entrypoints are **never** gated by legal hold or
operational pause. Unlike financial entrypoints (`settle`, `withdraw`, `fund`), the
attestation path has no compliance-hold or circuit-breaker preconditions. This is
documented in [`docs/pause-auth.md`](pause-auth.md) (attestation entrypoints listed
in the "NOT Gated by Pause" table).

**Key observation:** Attestation has **no escrow status precondition**. Mutating
attestation entrypoints can be called at any status (0, 1, 2, 3, 4) or even before
init (will get `EscrowNotInitialized`). Attestation metadata operates independently
of the escrow lifecycle state machine.

### Read-Only Views (no auth required)

| Entrypoint | Returns | Error on bad input |
|------------|---------|-------------------|
| `get_primary_attestation_hash()` | `Option<BytesN<32>>` -- `Some(digest)` or `None` | None |
| `get_attestation_append_log()` | `Vec<BytesN<32>>` -- full log or empty vec | None |
| `get_attestation_digest_at(index: u32)` | `Option<AttestationDigestInfo>` -- digest + revoked flag, or `None` if out of range | None |
| `is_attestation_revoked(index: u32)` | `bool` -- `true` if revoked, `false` otherwise | None |
| `get_revoked_attestation_digests(start: u32, limit: u32)` | `Vec<AttestationDigestInfo>` -- paginated revoked entries | `AttestationReadLimitZero` (57) if `limit == 0`; `AttestationReadLimitTooLarge` (58) if `limit > 20` |
| `get_escrow_summary()` | Includes `has_primary_attestation: bool` and `attestation_log_length: u32` | Panics if escrow not initialized |

---

## Guard Ordering

Every mutating entrypoint evaluates guards in a fixed order. Attestation entrypoints
follow two distinct patterns depending on whether batch-bound checks precede auth.

### `bind_primary_attestation_hash` (line 3277)

```
1. Load escrow + admin auth    -> load_escrow_require_admin (panics if uninitialized or unauthorized)
2. Write-once check            -> PrimaryAttestationAlreadyBound (50) if key exists
3. Storage write               -> DataKey::PrimaryAttestationHash <- digest
4. Event emission              -> PrimaryAttestationBound { att_bind, invoice_id, digest }
```

### `append_attestation_digest` (line 3308)

```
1. Load escrow + admin auth    -> load_escrow_require_admin
2. Capacity check              -> AttestationAppendLogCapacityReached (51) if log.len() >= 32
3. Storage write               -> push_back(digest) to AttestationAppendLog
4. Event emission              -> AttestationDigestAppended { att_app, invoice_id, index, digest }
```

### `append_attestation_digests` (line 3372) -- batch

```
1. Batch-empty check           -> AttestationAppendBatchEmpty (57) if empty
2. Batch-size check            -> AttestationAppendBatchTooLarge (58) if > 32
3. Load escrow + admin auth    -> load_escrow_require_admin
4. Pre-flight capacity check   -> AttestationAppendLogCapacityReached (51) if log.len() + n > 32
5. Storage write               -> single write with all digests appended atomically
6. Event emission              -> one AttestationDigestAppended per digest
```

### `revoke_attestation_digest` (line 3834)

```
1. Load escrow                 -> get_escrow (panics if uninitialized)
2. Admin auth                  -> escrow.admin.require_auth()
3. Load attestation log        -> load_attestation_log
4. Range check                 -> AttestationIndexOutOfRange (52) if index >= log.len()
5. Revocation-state check      -> AttestationAlreadyRevoked (53) if already revoked
6. Storage write               -> DataKey::AttestationRevoked(index) <- true
7. Event emission              -> AttestationDigestRevoked { att_rev, invoice_id, index }
```

### `revoke_attestation_digests` (line 3885) -- batch

```
1. Batch-empty check           -> AttestationBatchEmpty (54)
2. Batch-size check            -> AttestationBatchTooLarge (55) if > 32
3. Load escrow                 -> get_escrow (panics if uninitialized)
4. Admin auth                  -> escrow.admin.require_auth()
5. Load attestation log        -> load_attestation_log
6. Per-index loop:             -> range check (52), revocation check (53),
                                  write, emit event
   Any failure rolls back the entire batch (no partial revocation).
```

### `unrevoke_attestation_digest` (line 3976) -- ADR-002 ordering

```
1. Load attestation log        -> load_attestation_log
2. Range check                 -> AttestationIndexOutOfRange (52)
3. Revocation-state check      -> AttestationNotRevoked (56) if not currently revoked
4. Load escrow                 -> get_escrow (panics if uninitialized)
5. Admin auth                  -> escrow.admin.require_auth()
6. Storage remove              -> remove DataKey::AttestationRevoked(index)
7. Event emission              -> AttestationDigestUnrevoked { att_unrev, invoice_id, index }
```

**ADR-002 compliance note:** `unrevoke_attestation_digest` intentionally runs range
and state checks **before** `require_auth`. This surfaces typed errors
(`AttestationIndexOutOfRange` = 52, `AttestationNotRevoked` = 56) even to
unauthenticated callers, consistent with the intent that read-only validation precedes
authorization in the guard sequence.

---

## Rejection Codes

| Code | Variant | Trigger | Entrypoint(s) |
|------|---------|---------|---------------|
| 50 | `PrimaryAttestationAlreadyBound` | `bind_primary_attestation_hash` called when `PrimaryAttestationHash` already exists | `bind_primary_attestation_hash` |
| 51 | `AttestationAppendLogCapacityReached` | Append log has reached `MAX_ATTESTATION_APPEND_ENTRIES` (32) | `append_attestation_digest`, `append_attestation_digests` |
| 52 | `AttestationIndexOutOfRange` | `index >= log.len()` | `revoke_attestation_digest`, `revoke_attestation_digests`, `unrevoke_attestation_digest` |
| 53 | `AttestationAlreadyRevoked` | Index is already revoked | `revoke_attestation_digest`, `revoke_attestation_digests` |
| 54 | `AttestationBatchEmpty` | Empty `indices` vector | `revoke_attestation_digests` |
| 55 | `AttestationBatchTooLarge` | `indices.len() > MAX_ATTESTATION_REVOKE_BATCH` (32) | `revoke_attestation_digests` |
| 56 | `AttestationNotRevoked` | Index is not currently revoked | `unrevoke_attestation_digest` |
| 57 | `AttestationAppendBatchEmpty` | Empty `digests` vector | `append_attestation_digests` |
| 58 | `AttestationAppendBatchTooLarge` | `digests.len() > MAX_ATTESTATION_APPEND_BATCH` (32) | `append_attestation_digests` |

**SDK numeric error handling:** SDKs must branch on `ContractError(code)`, not on
panic strings. Panic strings are unstable across contract versions; numeric codes are
append-only and stable.

```typescript
try {
  await contract.revoke_attestation_digest({ index: 5 });
} catch (e) {
  if (e.code === 52) { /* index out of range */ }
  if (e.code === 53) { /* already revoked */   }
}
```

---

## Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_ATTESTATION_APPEND_ENTRIES` | 32 | Maximum number of digests in the append log |
| `MAX_ATTESTATION_APPEND_BATCH` | 32 | Maximum digests per `append_attestation_digests` call |
| `MAX_ATTESTATION_REVOKE_BATCH` | 32 | Maximum indices per `revoke_attestation_digests` call |
| `MAX_ATTESTATION_READ_PAGE` | 20 | Maximum page size for `get_revoked_attestation_digests` |

---

## Key Observations

1. **Single admin key gates everything.** All five mutating entrypoints (plus batch
   variants) require the same `InvoiceEscrow::admin` auth. No separate attestation
   operator role exists.

2. **No legal-hold or pause gate on attestation.** Unlike financial entrypoints
   (`settle`, `withdraw`, `fund`), attestation entrypoints are never blocked by
   `LegalHold` or `Paused` flags.

3. **No state-precondition gate on attestation.** Attestation can be called at any
   escrow status (0, 1, 2, 3, 4) or even before init (will get `EscrowNotInitialized`).

4. **Attestation is purely metadata.** No financial entrypoint reads attestation
   storage. Attestation state never affects token transfers, settlement calculations,
   or payout amounts (INV-ATT-9).

5. **Guard ordering varies by entrypoint.** `unrevoke_attestation_digest` specifically
   follows ADR-002 ordering (range check -> state check -> auth -> mutation), while
   `revoke_attestation_digest` checks auth first. Both patterns are intentional.

6. **Duplicate digests are allowed.** The append log is an ordered audit trail, not a
   set. Duplicate digests are accepted and may represent re-confirmations of unchanged
   documents at new ledger timestamps.

7. **Revocation does not delete history.** The original digest persists in the append
   log; only the revocation marker (`DataKey::AttestationRevoked(index)`) changes.

8. **Batch operations are atomic.** Both `revoke_attestation_digests` and
   `append_attestation_digests` roll back entirely on any per-item failure. No partial
   state is observed.

9. **Primary hash is write-once.** Once bound via `bind_primary_attestation_hash`, it
   cannot be overwritten, cleared, or rebound. Use the append log for incremental
   updates.

---

## Worked Example

### Setup

An escrow is initialized:
- **Admin:** `G_ADMIN`
- **SME:** `G_SME`
- **Status:** `0` (open)

### Step 1 -- Bind primary attestation hash

The compliance team has finalized the KYB bundle for the SME. They hash it and anchor
it on-chain:

```rust
// Off-chain: digest = SHA-256(kyb_bundle)
// On-chain:
bind_primary_attestation_hash(digest)
```

Guard evaluation:
1. `load_escrow_require_admin` loads escrow and verifies `G_ADMIN`'s signature -> passes.
2. `PrimaryAttestationHash` key does not exist -> write-once check passes.
3. Storage write: `DataKey::PrimaryAttestationHash <- digest`.
4. Emits `PrimaryAttestationBound { att_bind, invoice_id, digest }`.

If a second bind is attempted (even by admin with a different digest):
- Step 2 fails with `PrimaryAttestationAlreadyBound` (50).

### Step 2 -- Append incremental compliance updates

A year later, re-KYC produces a new document bundle:

```rust
append_attestation_digest(digest_v2)
```

Guard evaluation:
1. `load_escrow_require_admin` -> passes (admin signature).
2. `log.len()` is 0, which is < 32 -> capacity check passes.
3. Storage write: `push_back(digest_v2)` at index 0.
4. Emits `AttestationDigestAppended { att_app, invoice_id, index: 0, digest: digest_v2 }`.

### Step 3 -- Batch append multiple digests

AML screening produces three quarterly updates:

```rust
append_attestation_digests(vec![aml_q1, aml_q2, aml_q3])
```

Guard evaluation:
1. Batch is non-empty -> passes.
2. Batch size is 3, which is <= 32 -> passes.
3. `load_escrow_require_admin` -> passes.
4. Pre-flight: `log.len() + 3 <= 32` -> passes (log has 1 entry, 1 + 3 = 4 <= 32).
5. Single atomic write with all three digests appended.
6. Three `AttestationDigestAppended` events emitted (indices 1, 2, 3).

### Step 4 -- Revoke a superseded digest

The original KYB bundle at index 0 is found to contain an error:

```rust
revoke_attestation_digest(0)
```

Guard evaluation:
1. `get_escrow` -> loads escrow.
2. `escrow.admin.require_auth()` -> verifies `G_ADMIN`'s signature.
3. `load_attestation_log` -> log has 4 entries.
4. Range check: `0 < 4` -> passes.
5. Revocation check: index 0 is not revoked -> passes.
6. Storage write: `DataKey::AttestationRevoked(0) <- true`.
7. Emits `AttestationDigestRevoked { att_rev, invoice_id, index: 0 }`.

If `revoke_attestation_digest(0)` is called again:
- Step 5 fails with `AttestationAlreadyRevoked` (53).

### Step 5 -- Correct erroneous revocation (unrevoke)

The admin realizes index 1 (not index 0) should have been revoked:

```rust
unrevoke_attestation_digest(0)
```

Guard evaluation:
1. `load_attestation_log` -> log has 4 entries.
2. Range check: `0 < 4` -> passes.
3. Revocation check: index 0 IS revoked -> passes.
4. `get_escrow` -> loads escrow.
5. `escrow.admin.require_auth()` -> verifies `G_ADMIN`'s signature.
6. Storage remove: `DataKey::AttestationRevoked(0)` deleted.
7. Emits `AttestationDigestUnrevoked { att_unrev, invoice_id, index: 0 }`.

Then the correct revocation:

```rust
revoke_attestation_digest(1)
// -> AttestationDigestRevoked { att_rev, invoice_id, index: 1 }
```

### Step 6 -- Read-only queries (no auth)

Anyone can read attestation state without authorization:

```rust
get_primary_attestation_hash()          // -> Some(digest)
get_attestation_append_log()            // -> [digest, digest_v2, aml_q1, aml_q2, aml_q3]
get_attestation_digest_at(0)            // -> Some(AttestationDigestInfo { digest, revoked: false })
get_attestation_digest_at(1)            // -> Some(AttestationDigestInfo { digest: aml_q1, revoked: true })
is_attestation_revoked(0)               // -> false (was unrevoke)
is_attestation_revoked(1)               // -> true (was revoked)
get_revoked_attestation_digests(0, 20)  // -> [AttestationDigestInfo at index 1]
```

---

## Event Schema

All attestation events use the same shape:

| Event struct | Topic symbol | Fields |
|-------------|-------------|--------|
| `PrimaryAttestationBound` | `att_bind` | `invoice_id`, `digest` |
| `AttestationDigestAppended` | `att_app` | `invoice_id`, `index`, `digest` |
| `AttestationDigestRevoked` | `att_rev` | `invoice_id`, `index` |
| `AttestationDigestUnrevoked` | `att_unrev` | `invoice_id`, `index` |

See [`docs/escrow-events.md`](escrow-events.md) for full event schemas.

---

## Cross-References

- [`docs/escrow-attestations.md`](escrow-attestations.md) -- Operational KYC/KYB flows, security notes, test coverage matrix.
- [`docs/attestation-invariants.md`](attestation-invariants.md) -- Formal invariant specifications (INV-ATT-1 through INV-ATT-10).
- [`docs/beneficiary-auth.md`](beneficiary-auth.md) -- Access control matrix showing admin gates attestation (line 259).
- [`docs/pause-auth.md`](pause-auth.md) -- Attestation entrypoints listed as NOT pause-gated (lines 218-220).
- [`docs/audit-handoff-escrow.md`](audit-handoff-escrow.md) -- Audit bundle with attestation invariants (ESC-ATT-001, ESC-ATT-002).
- [`docs/escrow-read-api.md`](escrow-read-api.md) -- Complete catalog of attestation read views.
- [`docs/escrow-events.md`](escrow-events.md) -- Event schemas for attestation events.
- [`docs/escrow-error-messages.md`](escrow-error-messages.md) -- Error code reference.
- [`docs/adr/ADR-002-auth-boundaries.md`](adr/ADR-002-auth-boundaries.md) -- Auth boundaries ADR (guard ordering rationale).
- [`escrow/src/lib.rs`](../escrow/src/lib.rs) -- Contract source: `bind_primary_attestation_hash` (3277), `append_attestation_digest` (3308), `append_attestation_digests` (3372), `revoke_attestation_digest` (3834), `revoke_attestation_digests` (3885), `unrevoke_attestation_digest` (3976).
- [`escrow/src/tests/attestations.rs`](../escrow/src/tests/attestations.rs) -- Test suite covering all attestation entrypoints.
- [`escrow/src/tests/auth_matrix.rs`](../escrow/src/tests/auth_matrix.rs) -- Negative authorization test matrix.
