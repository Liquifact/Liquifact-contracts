# Escrow Attestations: KYC/KYB Operational Flows

> **Validation boundaries:** This document defines the accepted, rejected, duplicate, and
> boundary-case inputs for every attestation entrypoint. See the "Validation boundaries"
> section below for the authoritative matrix.

This document describes how the attestation entrypoints on the LiquiFact escrow contract
are used in KYC (Know Your Customer) and KYB (Know Your Business) compliance workflows.

See [`docs/attestation-invariants.md`](attestation-invariants.md) for the formal invariants and enforcement rules.

## What this is — and what it is not

Both entrypoints store a **32-byte digest** (e.g. SHA-256 of an IPFS CID or a document bundle
hash) on-chain. This is a **chain anchor**: a tamper-evident pointer that lets any observer
confirm that a specific document set existed at a specific ledger sequence.

**This is not a ZK claim.** The contract does not:
- verify the contents of the referenced document
- prove any property about the document (e.g. "this person passed KYC")
- execute any on-chain logic based on the attestation value
- interact with an oracle, identity registry, or ZK verifier

The on-chain record is a hash. Off-chain verifiers must retrieve the referenced document
independently and recompute the hash to confirm the anchor matches.

---

## Validation boundaries

This section is the single source of truth for what each attestation entrypoint accepts,
rejects, and how it behaves at the edges. All checks are deterministic: the same input
against the same state always produces the same result, and no validation path mutates
state or emits events before it has fully passed.

### `bind_primary_attestation_hash(digest: BytesN<32>)`

| Input class | Behavior |
|---|---|
| Valid: first call, any 32-byte digest, admin auth | Stores digest, emits `PrimaryAttestationBound` |
| Invalid: caller is not `InvoiceEscrow::admin` | Auth failure; no state change |
| Duplicate: second call with same digest | Rejected with `PrimaryAttestationAlreadyBound` (50) |
| Duplicate: second call with different digest | Rejected with `PrimaryAttestationAlreadyBound` (50) |
| Boundary: `digest == [0u8; 32]` | Accepted — the contract does not interpret digest content |
| Boundary: `digest == [0xff; 32]` | Accepted — same reasoning |

The digest is an opaque 32-byte anchor. There is no "zero digest" sentinel; a zero-filled
digest is a valid anchor and must not be special-cased by callers or indexers.

### `append_attestation_digest(digest: BytesN<32>)`

| Input class | Behavior |
|---|---|
| Valid: `log.len() < MAX_ATTESTATION_APPEND_ENTRIES`, admin auth | Appends at index `log.len()`, emits `AttestationDigestAppended` |
| Invalid: caller is not admin | Auth failure; no state change |
| Invalid: `log.len() == MAX_ATTESTATION_APPEND_ENTRIES` | Rejected with `AttestationAppendLogCapacityReached` (51) |
| Duplicate: same digest already present | Accepted — the log is an ordered audit trail, not a set |
| Boundary: append at index `MAX - 1` (32nd entry) | Accepted; log becomes full |
| Boundary: append at index `MAX` (33rd entry) | Rejected with `AttestationAppendLogCapacityReached` (51) |

### `append_attestation_digests(digests: Vec<BytesN<32>>)`

| Input class | Behavior |
|---|---|
| Valid: `1 <= len <= MAX_ATTESTATION_APPEND_BATCH` and `log.len() + len <= MAX_ATTESTATION_APPEND_ENTRIES` | Appends all entries atomically, one event per entry |
| Invalid: `len == 0` | Rejected with `AttestationAppendBatchEmpty` (57) |
| Invalid: `len > MAX_ATTESTATION_APPEND_BATCH` | Rejected with `AttestationAppendBatchTooLarge` (58) |
| Invalid: `log.len() + len > MAX_ATTESTATION_APPEND_ENTRIES` | Rejected with `AttestationAppendLogCapacityReached` (51); no partial write |
| Invalid: caller is not admin | Auth failure; no state change |
| Duplicate: repeated digests within the batch | Accepted — same policy as single append |
| Boundary: `len == 1` | Accepted (minimum valid batch) |
| Boundary: `len == MAX_ATTESTATION_APPEND_BATCH` | Accepted (maximum valid batch) |
| Boundary: batch fills log exactly to `MAX_ATTESTATION_APPEND_ENTRIES` | Accepted; next append fails with (51) |

The capacity check is a pre-flight: it runs before any mutation. A batch that would only
partially fit is rejected in full, so callers never observe a partial append.

### `revoke_attestation_digest(index: u32)`

| Input class | Behavior |
|---|---|
| Valid: `index < log.len()` and not already revoked, admin auth | Writes revocation marker, emits `AttestationDigestRevoked` |
| Invalid: `index >= log.len()` | Rejected with `AttestationIndexOutOfRange` (52) |
| Invalid: index already revoked | Rejected with `AttestationAlreadyRevoked` (53) |
| Invalid: caller is not admin | Auth failure; no state change |
| Duplicate: second revoke of same index | Rejected with `AttestationAlreadyRevoked` (53) |
| Boundary: `index == 0` on non-empty log | Accepted |
| Boundary: `index == log.len() - 1` | Accepted |
| Boundary: `index == log.len()` | Rejected with `AttestationIndexOutOfRange` (52) |
| Boundary: `index == u32::MAX` | Rejected with `AttestationIndexOutOfRange` (52) |

### `revoke_attestation_digests(indices: Vec<u32>)`

| Input class | Behavior |
|---|---|
| Valid: `1 <= len <= MAX_ATTESTATION_REVOKE_BATCH`, all indices in range and not revoked | Revokes all indices atomically, one event per index |
| Invalid: `len == 0` | Rejected with `AttestationBatchEmpty` (54) |
| Invalid: `len > MAX_ATTESTATION_REVOKE_BATCH` | Rejected with `AttestationBatchTooLarge` (55) |
| Invalid: any `index >= log.len()` | Rejected with `AttestationIndexOutOfRange` (52); full rollback |
| Invalid: any index already revoked | Rejected with `AttestationAlreadyRevoked` (53); full rollback |
| Invalid: caller is not admin | Auth failure; no state change |
| Duplicate: same index appears twice in the batch | Rejected with `AttestationAlreadyRevoked` (53) on the second occurrence; full rollback |
| Boundary: `len == 1` | Accepted |
| Boundary: `len == MAX_ATTESTATION_REVOKE_BATCH` | Accepted |

The batch is **not** pre-deduplicated. A duplicate index is treated as a second revocation
attempt and fails, rolling back the entire batch. Callers must deduplicate before submitting.

### `unrevoke_attestation_digest(index: u32)`

| Input class | Behavior |
|---|---|
| Valid: `index < log.len()` and currently revoked, admin auth | Clears revocation marker, emits `AttestationDigestUnrevoked` |
| Invalid: `index >= log.len()` | Rejected with `AttestationIndexOutOfRange` (52) |
| Invalid: index not currently revoked | Rejected with `AttestationNotRevoked` (56) |
| Invalid: caller is not admin | Auth failure; no state change |
| Duplicate: second unrevoke of same index | Rejected with `AttestationNotRevoked` (56) |
| Boundary: `index == 0` on non-empty log | Accepted if revoked |
| Boundary: `index == log.len() - 1` | Accepted if revoked |
| Boundary: `index == log.len()` | Rejected with `AttestationIndexOutOfRange` (52) |

### `get_revoked_attestation_digests(start: u32, limit: u32)`

| Input class | Behavior |
|---|---|
| Valid: `1 <= limit <= MAX_ATTESTATION_READ_PAGE` | Returns up to `limit` entries starting at `start` |
| Invalid: `limit == 0` | Rejected with `AttestationReadLimitZero` (57) |
| Invalid: `limit > MAX_ATTESTATION_READ_PAGE` | Rejected with `AttestationReadLimitTooLarge` (58) |
| Boundary: `start == log.len()` | Returns empty page (not an error) |
| Boundary: `start > log.len()` | Returns empty page (not an error) |
| Boundary: `start == u32::MAX` | Returns empty page (not an error) |
| Boundary: `limit == 1` | Accepted (minimum valid page) |
| Boundary: `limit == MAX_ATTESTATION_READ_PAGE` | Accepted (maximum valid page) |

Valid limits are applied exactly and are never silently clamped. A caller that requests
`limit == MAX_ATTESTATION_READ_PAGE` receives up to that many entries; a caller that
requests `limit == MAX_ATTESTATION_READ_PAGE + 1` receives an error, not a truncated page.

### `get_attestation_log_stats() -> (u32, u32)`

Pure read view. No validation boundaries beyond the implicit invariant
`used + remaining == MAX_ATTESTATION_APPEND_ENTRIES`. `remaining` is `0` when the log is
full; the next append will fail with `AttestationAppendLogCapacityReached` (51).

### Cross-cutting invariants

- **No partial writes.** Every entrypoint either fully applies its effect or leaves state
  and events untouched. Batches roll back on any per-element failure.
- **Auth is enforced after range/state checks** (ADR-002) so typed errors are surfaced
  deterministically even to unauthenticated callers.
- **Digest content is opaque.** No entrypoint inspects, normalizes, or rejects a digest
  based on its bytes. Zero-filled and all-ones digests are valid.
- **Duplicates are a policy choice, not a bug.** Append paths accept duplicates; revoke
  paths reject them. This asymmetry is intentional and documented above.
- **Concurrency.** All entrypoints are single-transaction; the Soroban host serializes
  execution per contract instance, so concurrent submissions cannot interleave state
  transitions. The first transaction to land determines the outcome; later ones observe
  the updated state and fail with the appropriate typed error.

---

## Entrypoints

### `bind_primary_attestation_hash(digest: BytesN<32>)`

| Property | Value |
|---|---|
| Auth | `InvoiceEscrow::admin` |
| Write policy | **Single-set** — panics if already bound |
| Storage key | `DataKey::PrimaryAttestationHash` |
| Event | `PrimaryAttestationBound { invoice_id, digest }` |

Binds the canonical compliance document digest for this escrow instance. Intended for the
initial KYC/KYB bundle that covers the SME and the invoice at origination.

**Frontrunning note:** whichever transaction lands first wins. Observers must read on-chain
state (or parse events) after ledger finality — there is no replay lock or commit-reveal scheme.
In practice, the admin key is governance-controlled, so frontrunning is only a concern if the
admin key is compromised.

### `append_attestation_digest(digest: BytesN<32>)`

| Property | Value |
|---|---|
| Auth | `InvoiceEscrow::admin` |
| Write policy | **Append-only**, bounded by `MAX_ATTESTATION_APPEND_ENTRIES` (32) |
| Storage key | `DataKey::AttestationAppendLog` |
| Event | `AttestationDigestAppended { invoice_id, index, digest }` |

Appends a digest to a bounded audit log. Intended for incremental compliance updates: re-KYC
cycles, updated KYB documents, AML screening refreshes, or legal hold evidence bundles.

The log is an ordered sequence, not a set — duplicate digests are allowed (e.g. re-confirming
an unchanged document at a new ledger timestamp via the event).

Duplicate digests are accepted by design: the log is an audit trail, and re-confirming an
unchanged document at a new ledger sequence is a legitimate operational event.

The 33rd append panics with `"attestation append log capacity reached"`. If more than 32
incremental anchors are needed, deploy a new escrow instance or extend the log off-chain using
the event stream.

### `get_attestation_log_stats() -> (u32, u32)`

Returns the current append-log usage and remaining capacity as `(used, remaining)`.
This is a pure read view for integrators that want to warn before the log fills. The returned
values satisfy `used + remaining == MAX_ATTESTATION_APPEND_ENTRIES`, and `remaining` drops to
`0` once the log is full and the next append would fail with
`AttestationAppendLogCapacityReached`.

### `get_revoked_attestation_digests(start: u32, limit: u32)`

Returns a page of revoked append-log entries. `start` is zero-based and may point at or beyond
the end of the log, in which case the result is empty. `limit` must be in
`1..=MAX_ATTESTATION_READ_PAGE` (20); zero returns `AttestationReadLimitZero` (57), and a value
above the maximum returns `AttestationReadLimitTooLarge` (58). Valid limits are applied exactly
and are never silently clamped.

### `revoke_attestation_digest(index: u32)`

| Property | Value |
|---|---|
| Auth | `InvoiceEscrow::admin` |
| Write policy | **Single-write per index** — returns `AttestationAlreadyRevoked` (53) if index is already revoked |
| Storage key | `DataKey::AttestationRevoked(u32)` |
| Event | `AttestationDigestRevoked { invoice_id, index }` |

Marks a previously appended digest as superseded without deleting or altering the append log
entry. The original digest remains auditable; indexers use the revocation marker to label the
entry as replaced or invalidated.

Intended for corrective compliance flows: a KYC/KYB bundle was updated and the old anchor must
be flagged as superseded while the full history stays on-chain.

**Typed errors** (not panic strings) are returned in all error cases:

| Condition | Error code | `EscrowError` variant |
|---|---|---|
| `index >= log.len()` | 52 | `AttestationIndexOutOfRange` |
| index already revoked | 53 | `AttestationAlreadyRevoked` |

#### SDK numeric error handling

SDKs must branch on `ContractError(code)`, not on panic strings. Panic strings are unstable
across contract versions; numeric codes are append-only and stable. Example:

```typescript
try {
  await contract.revoke_attestation_digest({ index: 5 });
} catch (e) {
  if (e.code === 52) { /* index out of range */ }
  if (e.code === 53) { /* already revoked */   }
}
```

See the [Validation boundaries](#validation-boundaries) section for the full accepted /
rejected / duplicate / boundary matrix, including the `u32::MAX` and zero-digest cases.

#### Why panic strings were removed

Prior to this change, `revoke_attestation_digest` used `assert!` with human-readable strings.
This was inconsistent with the rest of the attestation API (codes 50–51) and with the broader
`EscrowError` contract. Raw panic strings cannot be caught by type in SDKs or indexers and may
change without notice. Stable numeric codes allow SDK consumers to branch deterministically on
`ContractError(52)` / `ContractError(53)` without parsing message text.

### `append_attestation_digests(digests: Vec<BytesN<32>>)`

| Property | Value |
|---|---|
| Auth | `InvoiceEscrow::admin` |
| Write policy | **Batch append**, all-or-nothing |
| Batch bounds | Non-empty; max [`MAX_ATTESTATION_APPEND_BATCH`] (32) entries |
| Capacity check | Pre-flight: `current_log_len + batch_len <= MAX_ATTESTATION_APPEND_ENTRIES` |
| Storage key | `DataKey::AttestationAppendLog` |
| Event | One `AttestationDigestAppended { invoice_id, index, digest }` per entry |

Atomically appends multiple digests in a single call, saving per-call transaction fees for
operators that need to anchor several document hashes at the same ledger. All-or-nothing:
if any validation guard fails, no state is mutated and no events are emitted.

The pre-flight capacity check runs before any mutation — even a partially-fitting batch is
rejected entirely, guaranteeing callers never observe a partial append. Indices are assigned
sequentially starting from `log.len()` at call time, identical to repeated single-entry calls.

**Typed errors:**

| Condition | Error code | `EscrowError` variant |
|---|---|---|
| `digests.len() == 0` | 57 | `AttestationAppendBatchEmpty` |
| `digests.len() > MAX_ATTESTATION_APPEND_BATCH` | 58 | `AttestationAppendBatchTooLarge` |
| `current_log_len + digests.len() > MAX_ATTESTATION_APPEND_ENTRIES` | 51 | `AttestationAppendLogCapacityReached` |

```typescript
// Example: anchor three document hashes in one call
await contract.append_attestation_digests({
  digests: [sha256(bundle_a), sha256(bundle_b), sha256(bundle_c)]
});
```

### `revoke_attestation_digests(indices: Vec<u32>)`

| Property | Value |
|---|---|
| Auth | `InvoiceEscrow::admin` |
| Batch bounds | Non-empty; max [`MAX_ATTESTATION_REVOKE_BATCH`] (32) entries |
| Per-index policy | Same as single-revoke: range check then revocation check |
| Atomicity | Full batch rolls back on any per-index failure |
| Duplicate policy | **Not pre-deduplicated** — second occurrence of the same index fails with `AttestationAlreadyRevoked` (53) |
| Storage key | `DataKey::AttestationRevoked(u32)` per index |
| Event | One `AttestationDigestRevoked { invoice_id, index }` per newly revoked index |

Callers must deduplicate `indices` before submission. A duplicate index is treated as a
second revocation attempt and rolls back the entire batch with `AttestationAlreadyRevoked` (53).

Atomically revoke multiple attestation-digest indices in a single transaction. Each index
undergoes the same validation as the single-index `revoke_attestation_digest`:

| Condition | Error code | `EscrowError` variant |
|---|---|---|
| `indices.len() == 0` | 54 | `AttestationBatchEmpty` |
| `indices.len() > MAX_ATTESTATION_REVOKE_BATCH` | 55 | `AttestationBatchTooLarge` |
| `index >= log.len()` | 52 | `AttestationIndexOutOfRange` |
| index already revoked | 53 | `AttestationAlreadyRevoked` |

If **any** per-index validation fails, the entire batch is rolled back — no partial
revocation occurs. Off-chain indexers can safely consume the `att_rev` event stream
knowing that a successful batch emitted exactly one event per intended index.

```typescript
// Example: revoke indices 0, 2, and 4 in one call
await contract.revoke_attestation_digests({ indices: [0, 2, 4] });
```

### `unrevoke_attestation_digest(index: u32)`

| Property | Value |
|---|---|
| Auth | `InvoiceEscrow::admin` |
| Write policy | Clears `DataKey::AttestationRevoked(index)` — errors if not currently revoked |
| Storage key | `DataKey::AttestationRevoked(u32)` (removed) |
| Event | `AttestationDigestUnrevoked { invoice_id, index }` |
| Typed errors | `AttestationIndexOutOfRange` (52), `AttestationNotRevoked` (56) |

Clears the revocation marker set by `revoke_attestation_digest`. Use this to correct a
fat-finger revocation before indexers process the erroneous tombstone.

**Guard ordering (ADR-002):** range check → revocation-state check → `require_auth` →
storage mutation. This means range and state errors are surfaced even to unauthenticated
callers, consistent with the existing revoke path.

The append log entry and its digest are unaffected. After a successful unrevoke,
`is_attestation_revoked(index)` returns `false` and the entry is once again treated as
active by indexers.

**Errors** with `AttestationIndexOutOfRange` (52) if `index >= log.len()`, or
`AttestationNotRevoked` (56) if the index is not currently revoked.

---

## KYC/KYB operational flows

### Flow 1 — SME onboarding (KYB at origination)

```
Off-chain                              On-chain
─────────────────────────────────────────────────────────────────────
1. Compliance team collects KYB docs
   (company registration, UBO list,
   bank statements, AML screening).

2. Bundle is hashed:
   digest = SHA-256(canonical_bundle)

3. Bundle uploaded to IPFS or
   internal document store.
                                       4. Admin calls:
                                          bind_primary_attestation_hash(digest)
                                          → PrimaryAttestationBound event emitted
                                          → DataKey::PrimaryAttestationHash set (immutable)

5. Indexer reads PrimaryAttestationBound.
   Off-chain verifier fetches bundle,
   recomputes SHA-256, confirms match.
```

The primary hash is the canonical anchor for the escrow. It cannot be replaced — if the
origination bundle is superseded, use the append log (Flow 2).

---

### Flow 2 — Periodic re-KYC / KYB refresh (append log)

```
Off-chain                              On-chain
─────────────────────────────────────────────────────────────────────
1. Annual re-KYC cycle: compliance
   team collects updated docs.

2. New bundle hashed:
   digest = SHA-256(updated_bundle_v2)

3. Bundle stored with version tag.
                                       4. Admin calls:
                                          append_attestation_digest(digest)
                                          → AttestationDigestAppended { index: 0, digest }

   (Repeat for each refresh cycle,
    up to index 31.)
```

Each append is timestamped by the ledger sequence in the event. Off-chain systems can build a
full compliance timeline by replaying `AttestationDigestAppended` events in order.

---

### Flow 3 — Investor KYC (off-chain, referenced by append log)

Investor KYC is **not stored per-investor** in this contract. The escrow tracks investor
addresses and principal amounts; it does not custody identity documents.

The recommended pattern:

```
Off-chain                              On-chain
─────────────────────────────────────────────────────────────────────
1. Compliance platform runs KYC for
   each investor address.

2. Platform produces a Merkle root
   over (address, kyc_status, expiry)
   tuples for all approved investors.

3. Root hashed:
   digest = SHA-256(merkle_root || timestamp)
                                       4. Admin calls:
                                          append_attestation_digest(digest)
                                          → AttestationDigestAppended { index: N, digest }

5. Investor submits Merkle proof
   off-chain to compliance platform.
   Platform verifies proof against
   the on-chain anchor.
```

This keeps investor PII off-chain while providing a tamper-evident on-chain commitment that
a specific set of addresses was approved at a specific time.

---

### Flow 4 — Legal hold with evidence anchor

When a legal hold is set (`set_legal_hold(true)`), the admin may optionally anchor the
evidence bundle that triggered the hold:

```
Off-chain                              On-chain
─────────────────────────────────────────────────────────────────────
1. Legal team assembles hold evidence
   (court order, regulator notice, etc.)

2. digest = SHA-256(evidence_bundle)
                                       3. Admin calls:
                                          set_legal_hold(true)
                                          append_attestation_digest(digest)

4. Evidence bundle stored in legal
   document management system.
   On-chain digest provides audit trail.
```

Clearing the hold follows the same pattern in reverse: anchor the clearance document, then
call `clear_legal_hold()`.

---

### Flow 5 — Correction / supersession (revoke)

When a previously anchored KYC/KYB bundle is corrected (e.g. a document was re-uploaded
with a corrected date), the old digest must be flagged as superseded:

```
Off-chain                              On-chain
─────────────────────────────────────────────────────────────────────
1. Compliance team identifies that
   the bundle referenced by index N
   contains an error.

2. Corrected bundle is hashed:
   digest = SHA-256(corrected_bundle)

3. Corrected bundle stored in
   document management system.
                                       4. Admin calls:
                                          append_attestation_digest(digest)
                                          → AttestationDigestAppended { index: M, digest }

                                       5. Admin calls:
                                          revoke_attestation_digest(N)
                                          → AttestationDigestRevoked { index: N }

6. Indexer sees AttestationDigestRevoked
   for index N, labels entry N as
   superseded. Off-chain verifier checks
   the new anchor at index M.
```

The original digest at index N remains in the append log for auditability. Indexers
consume `AttestationDigestRevoked` events to compute the effective (non-revoked) chain.

### Flow 6 — Correcting an erroneous revocation (unrevoke)

If an admin fat-fingered the index on a `revoke_attestation_digest` call and revoked the
wrong entry, `unrevoke_attestation_digest` clears the marker before indexers propagate the
tombstone:

```
Off-chain                              On-chain
─────────────────────────────────────────────────────────────────────
1. Admin realises index N was revoked
   by mistake (correct target was M).
                                       2. Admin calls:
                                          unrevoke_attestation_digest(N)
                                          → AttestationDigestUnrevoked { index: N }

                                       3. Admin calls (if still needed):
                                          revoke_attestation_digest(M)
                                          → AttestationDigestRevoked { index: M }

4. Indexer sees AttestationDigestUnrevoked
   for index N and removes the
   superseded label. Entry N is active
   again; entry M is now the tombstone.
```

The append log is never mutated. The unrevoke only removes the `DataKey::AttestationRevoked(N)`
storage key; the digest at index N is unchanged.

## Security notes

- **Admin key custody:** all attestation entrypoints require `InvoiceEscrow::admin` auth. Production
  deployments should use a multisig or governed contract as admin so no single key can bind
  an arbitrary digest. See [ADR-002](adr/ADR-002-auth-boundaries.md).

- **No on-chain verification:** the contract stores bytes. It does not fetch the referenced
  document, verify a signature, or enforce any property of the digest content. Verification
  is entirely off-chain.

- **Collision resistance:** SHA-256 is assumed collision-resistant for operational purposes.
  If a weaker hash is used off-chain, the on-chain anchor provides no stronger guarantee.

- **Append log is not a set:** duplicate digests are accepted. Off-chain consumers should
  deduplicate by digest value if uniqueness matters for their use case.

- **Zero and all-ones digests are valid anchors:** the contract treats the 32-byte digest as
  opaque. There is no sentinel value; callers must not assume `[0u8; 32]` means "unset".
  Use `get_primary_attestation_hash() -> Option<BytesN<32>>` to distinguish unset from set.

- **Capacity:** `MAX_ATTESTATION_APPEND_ENTRIES = 32`. This is a storage-growth guardrail,
  not a compliance limit. If 32 entries are insufficient, the operational playbook should
  define a rotation policy (e.g. new escrow instance per compliance period).

- **Revocation does not delete history:** `revoke_attestation_digest` writes a `true` marker
  under a separate key; the original append log entry persists unchanged. This ensures the
  audit trail remains complete even after a correction.

- **Double-revocation guard:** each index may be revoked at most once. A second call for the
  same index returns `AttestationAlreadyRevoked` (53). Off-chain indexers can safely assume
  that once `AttestationDigestRevoked` is observed, it is final unless an
  `AttestationDigestUnrevoked` event follows.

- **Out-of-range rejection:** revoking a non-existent index returns
  `AttestationIndexOutOfRange` (52). The admin must read `get_attestation_append_log` to
  determine valid indices. `index == log.len()` and `index == u32::MAX` are both out of range.

- **Deterministic errors, not panic strings:** all attestation entrypoints return typed
  `EscrowError` codes. SDKs and indexers must branch on `ContractError(code)`, never on
  message text. Codes are append-only and stable across contract versions.

- **Unrevoke is admin-only:** `unrevoke_attestation_digest` is gated by `require_auth` on
  `InvoiceEscrow::admin`. ADR-002 guard ordering is preserved: range and state checks run
  before auth so typed errors (`AttestationIndexOutOfRange` = 52, `AttestationNotRevoked` = 56)
  are surfaced cleanly.

- **Unrevoke is idempotent in round-trips:** revoke → unrevoke → revoke is valid. Each
  transition is guarded, so double-unrevoke is rejected with `AttestationNotRevoked`.

- **Token economics:** attestation entrypoints do not interact with token balances, funding
  state, or settlement flows. They are metadata-only. See
  [`external_calls.rs`](../escrow/src/external_calls.rs) for token transfer boundaries.

- **Out of scope:** ZK proofs, on-chain identity verification, cross-contract KYC registry
  lookups, and automated compliance enforcement are all out of scope for this contract version.

---

## Test coverage

Attestation behavior is covered in [`escrow/src/tests/attestations.rs`](../escrow/src/tests/attestations.rs):

### Write-once invariant (`bind_primary_attestation_hash`)

| Test | What it proves |
|---|---|
| `test_bind_primary_hash_stores_and_reads` | Happy path: bind succeeds, getter returns digest |
| `test_get_primary_hash_none_before_bind` | Getter returns `None` before any bind |
| `test_bind_primary_hash_same_digest_panics` | Second bind (same digest) panics |
| `test_bind_primary_hash_different_digest_panics` | Second bind (different digest) panics |
| `test_bind_primary_hash_non_admin_panics` | Non-admin bind is rejected |
| `test_bind_primary_hash_typed_error` | `try_bind_primary_attestation_hash` returns typed error code 50 (`PrimaryAttestationAlreadyBound`) on second call |
| `test_bind_primary_hash_emits_event` | Emits `PrimaryAttestationBound` with correct `invoice_id` and `digest` |
| `test_bind_primary_hash_zero_digest_accepted` | Boundary: `[0u8; 32]` is accepted as a valid anchor |

### Bounded append log (`append_attestation_digest`)

| Test | What it proves |
|---|---|
| `test_append_log_empty_before_first_append` | Log is empty before first append |
| `test_append_single_entry_stored` | Single append stored at index 0 |
| `test_append_multiple_entries_ordered` | Insertion order preserved |
| `test_append_exactly_max_entries_succeeds` | 32nd entry succeeds (boundary inclusive) |
| `test_append_beyond_max_panics` | 33rd entry panics |
| `test_append_beyond_max_typed_error` | `try_append_attestation_digest` returns typed error code 51 (`AttestationAppendLogCapacityReached`) on 33rd call |
| `test_append_duplicate_digest_allowed` | Duplicate digests accepted (log is audit trail, not a set) |
| `test_append_non_admin_panics` | Non-admin append is rejected |
| `test_append_emits_event_with_correct_index` | Emits `AttestationDigestAppended` with correct `index` (0-based) and `digest` for each call |
| `test_append_zero_digest_accepted` | Boundary: `[0u8; 32]` is accepted and stored verbatim |
| `test_append_at_capacity_typed_error` | Boundary: append when `log.len() == MAX` returns (51) without mutating state |

### Independence

| Test | What it proves |
|---|---|
| `test_primary_bind_does_not_affect_append_log` | Primary bind leaves log empty |
| `test_append_does_not_affect_primary_hash` | Append leaves primary hash `None` |
| `test_primary_and_append_coexist` | Both can be set independently |

### Revocation tombstone (`revoke_attestation_digest` / `revoke_attestation_digests`)

| Test | What it proves |
|---|---|
| `test_revoke_single_entry` | Happy path: revoke index 0, `is_attestation_revoked` returns `true` |
| `test_revoke_later_index_does_not_affect_earlier` | Revoke index 1 leaves index 0 unaffected |
| `test_revoke_all_entries` | All entries can be revoked sequentially |
| `test_double_revoke_panics` | Second revocation of same index panics with `"attestation already revoked at index"` |
| `test_revoke_out_of_range_panics` | Revoke on empty log panics with `"attestation index out of range"` |
| `test_revoke_at_log_len_panics` | Revoke at index `log.len()` panics (0-indexed boundary) |
| `test_is_revoked_empty_log` | `is_attestation_revoked` returns `false` for any index on empty log |
| `test_revoke_non_admin_panics` | Non-admin revoke is rejected |
| `test_revoke_preserves_log_entry` | Append log contents unchanged after revocation |
| `test_revoke_does_not_affect_primary_hash` | Revocation leaves primary hash intact |
| `test_unrevoke_restores_state` | Revoke then unrevoke: `is_attestation_revoked` returns `false` |
| `test_unrevoke_preserves_log_entry` | Append log entry unchanged after unrevoke |
| `test_unrevoke_not_revoked_panics` | Unrevoke of non-revoked index is rejected |
| `test_unrevoke_out_of_range_panics` | Unrevoke on empty log is rejected |
| `test_double_unrevoke_panics` | Second unrevoke rejected after first succeeds |
| `test_unrevoke_non_admin_panics` | Non-admin unrevoke is rejected |
| `test_revoke_unrevoke_revoke_round_trip` | Round-trip revoke → unrevoke → revoke succeeds |
| `test_unrevoke_does_not_affect_other_indices` | Unrevoke of index 0 leaves index 1 revoked |
| `test_revoke_index_u32_max_typed_error` | Boundary: `index == u32::MAX` returns (52) |
| `test_unrevoke_index_u32_max_typed_error` | Boundary: `index == u32::MAX` returns (52) |

### Batch append (`append_attestation_digests`)

| Test | What it proves |
|---|---|
| `test_batch_append_happy_path` | Happy path: 3 digests appended atomically, all readable in order |
| `test_batch_append_starts_at_existing_log_length` | Indices offset correctly when log is pre-filled |
| `test_batch_append_single_element_succeeds` | Minimum valid batch size (1 entry) succeeds |
| `test_batch_append_max_size_succeeds` | Batch of exactly `MAX_ATTESTATION_APPEND_BATCH` succeeds |
| `test_batch_append_empty_returns_typed_error` | Empty batch returns `AttestationAppendBatchEmpty` (57) |
| `test_batch_append_over_limit_returns_typed_error` | `MAX + 1` entries returns `AttestationAppendBatchTooLarge` (58) |
| `test_batch_append_over_capacity_rejected_atomically` | Batch that would overflow the log is rejected with `AttestationAppendLogCapacityReached` (51); no partial write |
| `test_batch_append_fills_log_exactly_to_capacity` | Filling exactly to `MAX_ATTESTATION_APPEND_ENTRIES` succeeds; next single append fails |
| `test_batch_append_duplicate_digests_allowed` | Duplicate digests within a batch are accepted (audit trail, not a set) |
| `test_batch_append_non_admin_returns_error` | Non-admin caller is rejected; log is unmodified |
| `test_batch_append_emits_events_with_correct_indices` | Exactly one `att_app` event per entry with correct sequential index |
| `test_batch_append_events_offset_by_existing_log_length` | Event indices correctly offset when log is pre-filled |
| `test_batch_append_interleaved_with_single_appends` | Mixing single and batch appends preserves full ordered audit trail |
| `test_batch_append_entries_are_revocable` | Batch-appended entries are independently revocable after insertion |
| `test_batch_append_failed_call_leaves_log_unchanged` | Failed batch (over-limit or over-capacity) leaves log in its prior state |
| `test_batch_append_exact_fit_boundary` | Boundary: batch that exactly fills the log succeeds; next append fails with (51) |
| `test_batch_append_duplicate_digests_within_batch` | Duplicate digests inside one batch are accepted |

### Batch revocation (`revoke_attestation_digests`)

| Test | What it proves |
|---|---|
| `test_batch_revoke_happy_path` | Happy path: revoke indices 0, 2, 4 atomically |
| `test_batch_revoke_all_entries` | All entries can be revoked in one batch |
| `test_batch_revoke_empty_panics` | Empty batch returns `AttestationBatchEmpty` (54) |
| `test_batch_revoke_oversized_panics` | Batch > `MAX_ATTESTATION_REVOKE_BATCH` returns `AttestationBatchTooLarge` (55) |
| `test_batch_revoke_max_size_succeeds` | Batch at boundary (`MAX_ATTESTATION_REVOKE_BATCH`) succeeds |
| `test_batch_revoke_out_of_range_panics` | Out-of-range index in batch returns `AttestationIndexOutOfRange` (52) |
| `test_batch_revoke_already_revoked_panics` | Already-revoked index in batch returns `AttestationAlreadyRevoked` (53) |
| `test_batch_revoke_duplicate_index_panics` | Duplicate index in batch: second occurrence hits `AttestationAlreadyRevoked` (53), entire batch rolls back |
| `test_batch_revoke_non_admin_panics` | Non-admin batch revoke is rejected |
| `test_batch_revoke_preserves_log_entries` | Append log contents unchanged after batch revocation |
| `test_batch_revoke_emits_events` | Exactly one `att_rev` event per revoked index |
| `test_batch_revoke_atomic_rollback` | Mid-batch failure rolls back all prior revocations |
| `test_batch_revoke_duplicate_index_rolls_back` | Duplicate index in batch returns (53) and rolls back all prior revocations |
| `test_batch_revoke_u32_max_typed_error` | Boundary: `u32::MAX` in batch returns (52) and rolls back |

### Read view boundaries (`get_revoked_attestation_digests`)

| Test | What it proves |
|---|---|
| `test_read_limit_zero_typed_error` | `limit == 0` returns `AttestationReadLimitZero` (57) |
| `test_read_limit_too_large_typed_error` | `limit > MAX_ATTESTATION_READ_PAGE` returns `AttestationReadLimitTooLarge` (58) |
| `test_read_limit_exact_max_accepted` | Boundary: `limit == MAX_ATTESTATION_READ_PAGE` is applied exactly |
| `test_read_start_at_log_len_returns_empty` | Boundary: `start == log.len()` returns empty page |
| `test_read_start_beyond_log_len_returns_empty` | Boundary: `start > log.len()` returns empty page |
| `test_read_start_u32_max_returns_empty` | Boundary: `start == u32::MAX` returns empty page |

### Stats view (`get_attestation_log_stats`)

| Test | What it proves |
|---|---|
| `test_stats_empty_log` | `(0, MAX)` on a fresh instance |
| `test_stats_partial_log` | `(used, MAX - used)` after partial fill |
| `test_stats_full_log` | `(MAX, 0)` when the log is full |
| `test_stats_invariant_used_plus_remaining` | `used + remaining == MAX_ATTESTATION_APPEND_ENTRIES` at every fill level |

### Cross-cutting regression tests

| Test | What it proves |
|---|---|
| `test_no_partial_write_on_batch_append_failure` | Failed batch append leaves log length and contents unchanged |
| `test_no_partial_write_on_batch_revoke_failure` | Failed batch revoke leaves all revocation markers unchanged |
| `test_typed_errors_are_stable_codes` | Every documented error path returns the documented numeric code |
| `test_auth_checked_after_range_and_state` | Range/state errors are surfaced before auth failure (ADR-002) |
| `test_duplicate_append_allowed_duplicate_revoke_rejected` | Documents the intentional asymmetry between append and revoke duplicate policies |