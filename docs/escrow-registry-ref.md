# Off-Chain Registry-Reference Pointer

This document describes the lifecycle, semantics, and security properties of the
optional off-chain registry hint stored under `DataKey::RegistryRef` in the
Liquifact Escrow contract.

---

## Purpose

The registry-reference pointer (`DataKey::RegistryRef`) is an `Option<Address>`
that an escrow admin may set to hint off-chain indexers and integrators toward
the registry contract that tracks this escrow. It has **no authority** over any
on-chain operation — it is a discoverability aid only.

---

## Non-Authority Guarantee

> **The registry pointer confers no control over escrow funds, settlement, or authorization.**

No entrypoint that moves tokens, changes escrow status, or enforces authorization
reads `DataKey::RegistryRef`. The pointer is:

- Not checked by `fund`, `fund_with_commitment`, `settle`, `partial_settle`,
  `withdraw`, `refund`, or `claim_investor_payout`.
- Not used in any allowlist, legal-hold, or attestation gate.
- Not a substitute for querying the registry contract directly to verify
  on-chain membership.

Integrators **must not** use the presence of a `Some(addr)` value as a security
boundary or as proof that this escrow is registered with the named contract.

---

## Pointer States

| State   | Storage                        | `get_registry_ref` | Meaning |
|---------|--------------------------------|--------------------|---------|
| Unbound | Key absent (`None` on read)    | `None`             | No off-chain registry is associated with this escrow. |
| Bound   | `DataKey::RegistryRef = addr`  | `Some(addr)`       | `addr` is an indexer hint. Verify membership with that contract if authoritative state is required. |

---

## Lifecycle

### 1. Initialization (optional bind)

`LiquifactEscrow::init` accepts `registry: Option<Address>` as a parameter.

- `None` → key is not written; `get_registry_ref` returns `None`.
- `Some(addr)` → `addr` is stored under `DataKey::RegistryRef`.

No `RegistryRefRebound` event is emitted at `init`; the initial value is
included in the `EscrowInitialized` snapshot event under the `registry` field.

### 2. Rebind

```
rebind_registry_ref(registry: Option<Address>)
```

- **Auth:** requires the signature of the current escrow admin.
- **Effect:** writes `Some(addr)` to `DataKey::RegistryRef`, or removes the key
  when passed `None`.
- **Event:** emits `RegistryRefRebound { name: "reg_rebind", invoice_id, registry }`.

The pointer may be rebound any number of times and at any escrow status.

### 3. Clear

```
clear_registry_ref()
```

Convenience wrapper that calls `rebind_registry_ref(None)`. Behavior and
emitted event are identical to passing `None` directly.

---

## Emitted Event: `RegistryRefRebound`

Every mutation via `rebind_registry_ref` (including `clear_registry_ref`) emits:

| Field        | Type              | Description |
|--------------|-------------------|-------------|
| `name`       | `Symbol`          | Topic. Always `"reg_rebind"`. |
| `invoice_id` | `Symbol`          | Topic. Identifies the escrow. |
| `registry`   | `Option<Address>` | Data. New pointer value; `None` on clear. |

Off-chain indexers should subscribe to the `reg_rebind` topic to re-sync their
cached pointer without polling the contract storage.

See [`docs/escrow-events.md`](escrow-events.md) for the full event catalog and
XDR decoding guidance.

---

## Integrator Guidance

### Interpreting `None` (unbound)

A `None` return from `get_registry_ref` means no registry hint has been set or
the pointer was cleared. Treat the escrow as "not registered" for display
purposes. Do not gate any integration flow on this state.

### Interpreting `Some(addr)` (bound)

`addr` is a hint pointing to an off-chain registry contract. To determine
whether this escrow is actually a member:

1. Call the registry contract at `addr` directly.
2. Do not rely solely on the presence of the pointer.

### Handling `RegistryRefRebound` events

On receipt of a `reg_rebind` event:

1. Update your locally cached pointer for this `invoice_id`.
2. If the new value is `None`, mark the escrow as unregistered in your index.
3. If the new value is `Some(addr)`, update your reference and optionally
   re-verify membership with the named registry.
4. Do **not** infer any change to funded amount, settlement status, or
   authorization from this event.

---

## Authorization Matrix

| Entrypoint            | Required auth          |
|-----------------------|------------------------|
| `rebind_registry_ref` | Current escrow admin   |
| `clear_registry_ref`  | Current escrow admin   |
| `get_registry_ref`    | None (read-only)       |

The admin identity is stored at `DataKey::Escrow` (`InvoiceEscrow::admin`).
Following an `accept_admin` handover, the new admin gains exclusive mutation
rights; the old admin is locked out.

---

## Security Notes

- The registry pointer is never consulted for settlement-critical decisions.
  A doc-backing test in [`escrow/src/tests/admin.rs`](../escrow/src/tests/admin.rs)
  (`test_registry_ref_does_not_affect_settlement_or_funding`) asserts this
  invariant: binding, rebinding, and clearing the pointer does not change
  `funded_amount` or any settlement outcome.
- The pointer is stored in instance storage with the escrow's TTL. It is not
  stored in persistent storage and will be absent (read as `None`) on an expired
  or archived contract instance.
- Admin-only mutation prevents unauthorized pointer squatting; the two-step
  `propose_admin` / `accept_admin` handover transfers pointer control to the
  incoming admin atomically.

---

## Cross-References

- Entrypoints: `escrow/src/lib.rs` — `get_registry_ref`, `rebind_registry_ref`, `clear_registry_ref`
- Events: [`docs/escrow-events.md`](escrow-events.md) — `RegistryRefRebound`
- Admin handover: [`docs/escrow-lifecycle.md`](escrow-lifecycle.md)
- Read API: [`docs/escrow-read-api.md`](escrow-read-api.md)
