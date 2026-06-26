# Escrow Attestations: KYC/KYB Operational Flows

This document describes how the three attestation entrypoints on the LiquiFact escrow contract
are used in KYC (Know Your Customer) and KYB (Know Your Business) compliance workflows.

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

The 33rd append panics with `"attestation append log capacity reached"`. If more than 32
incremental anchors are needed, deploy a new escrow instance or extend the log off-chain using
the event stream.

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

#### Why panic strings were removed

Prior to this change, `revoke_attestation_digest` used `assert!` with human-readable strings.
This was inconsistent with the rest of the attestation API (codes 50–51) and with the broader
`EscrowError` contract. Raw panic strings cannot be caught by type in SDKs or indexers and may
change without notice. Stable numeric codes allow SDK consumers to branch deterministically on
`ContractError(52)` / `ContractError(53)` without parsing message text.

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
`AttestationNotRevoked` (53) if the index is not currently revoked.

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

- **Capacity:** `MAX_ATTESTATION_APPEND_ENTRIES = 32`. This is a storage-growth guardrail,
  not a compliance limit. If 32 entries are insufficient, the operational playbook should
  define a rotation policy (e.g. new escrow instance per compliance period).

- **Revocation does not delete history:** `revoke_attestation_digest` writes a `true` marker
  under a separate key; the original append log entry persists unchanged. This ensures the
  audit trail remains complete even after a correction.

- **Double-revocation guard:** each index may be revoked at most once. A second call for the
  same index panics with `"attestation already revoked at index"`. Off-chain indexers can
  safely assume that once `AttestationDigestRevoked` is observed, it is final unless an
  `AttestationDigestUnrevoked` event follows.

- **Out-of-range rejection:** revoking a non-existent index panics with `"attestation index
  out of range"`. The admin must read `get_attestation_append_log` to determine valid indices.

- **Unrevoke is admin-only:** `unrevoke_attestation_digest` is gated by `require_auth` on
  `InvoiceEscrow::admin`. ADR-002 guard ordering is preserved: range and state checks run
  before auth so typed errors (`AttestationIndexOutOfRange` = 52, `AttestationNotRevoked` = 53)
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
