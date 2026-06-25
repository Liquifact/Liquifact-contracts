# Investor Allowlist Model

## Overview

The LiquiFact escrow contract provides an optional investor allowlist gate that controls which addresses may fund an invoice escrow. The allowlist consists of two independent components:

1. **Toggle state** (`DataKey::AllowlistActive`) — stored in instance storage, controls whether the gate is enforced
2. **Per-address entries** (`DataKey::InvestorAllowlisted(Address)`) — stored in persistent storage, indicates whether a specific address is allowlisted

This document describes the interaction between these components, the storage model, TTL behavior, and the fund-gate enforcement rules.

## Storage Architecture

### Instance Storage: Toggle State

The allowlist toggle state is stored in **instance storage** under `DataKey::AllowlistActive`:

- **Type:** `bool`
- **Location:** Instance storage (shared TTL with the contract instance)
- **Default:** `false` (disabled) when absent
- **Mutability:** Admin-only via [`LiquifactEscrow::set_allowlist_active`]

Instance storage is loaded in full on every contract invocation and has a shared TTL with the contract instance. The toggle state is small (1 byte) and does not significantly impact instance storage footprint.

### Persistent Storage: Per-Address Entries

Per-address allowlist entries are stored in **persistent storage** under `DataKey::InvestorAllowlisted(Address)`:

- **Type:** `bool`
- **Location:** Persistent storage (independent per-address TTL)
- **Default:** `false` when absent (default-to-deny semantics)
- **Mutability:** Admin-only via [`LiquifactEscrow::set_investor_allowlisted`] or [`LiquifactEscrow::set_investors_allowlisted`]

Persistent storage entries have independent TTLs per address and are not loaded on every contract invocation. This design allows the allowlist to scale to many investors without growing the instance storage footprint.

### TTL Extension

When an allowlist entry is written or updated via `set_investor_allowlisted` or `set_investors_allowlisted`, the contract extends the persistent storage TTL by [`PERSISTENT_TTL_MIN_EXTENSION_LEDGERS`] (≈1 hour at 1 ledger/sec). This reduces the risk of silent allowlist disablement due to entry archival.

**Important:** TTL extension is **not** automatic on read operations. Operators must periodically write to allowlist entries (e.g., via a no-op `set_investor_allowlisted(addr, true)` for existing allowlisted addresses) to keep entries alive over long time horizons.

## Fund-Gate Enforcement

### Gate Logic

The fund-gate is enforced in `fund_impl` (the internal implementation shared by `fund` and `fund_with_commitment`) with the following logic:

```rust
if Self::is_allowlist_active(env.clone()) {
    ensure(
        &env,
        Self::is_investor_allowlisted(env.clone(), investor.clone()),
        EscrowError::InvestorNotAllowlisted,
    );
}
```

The gate is **only checked when the allowlist is active**. When inactive, any address may fund regardless of allowlist entries.

### Gate Behavior Matrix

| Allowlist Active | Entry Present | Entry Value | Funding Allowed |
|------------------|---------------|-------------|-----------------|
| `false` | Any | Any | ✅ Yes (gate bypassed) |
| `true` | Yes | `true` | ✅ Yes |
| `true` | Yes | `false` | ❌ No (rejected with `InvestorNotAllowlisted`) |
| `true` | No | N/A | ❌ No (default-to-deny, rejected with `InvestorNotAllowlisted`) |

### Default-to-Deny Semantics

When the allowlist is active, **absent entries are treated as `false`**. This default-to-deny behavior ensures that:

- Archived/evicted entries (due to TTL expiration) do not silently allow funding
- Addresses never added to the allowlist are blocked by default
- Explicit removal (`set_investor_allowlisted(addr, false)`) and absence are functionally equivalent

This is implemented via the `unwrap_or(false)` pattern in `is_investor_allowlisted`:

```rust
pub fn is_investor_allowlisted(env: Env, investor: Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::InvestorAllowlisted(investor))
        .unwrap_or(false)  // ← default-to-deny
}
```

## Active/Inactive Toggle Interaction

### Toggle Independence

The toggle state and per-address entries are **independent**:

- Disabling the toggle does **not** delete per-address entries
- Enabling the toggle does **not** auto-populate entries
- Entries persist across enable/disable cycles

This design allows operators to:
- Pre-populate the allowlist before enabling the gate
- Temporarily disable the gate for emergency funding without losing the allowlist
- Re-enable the gate with the same entries intact

### Toggle During Funding

The toggle can be changed at any time (including while the escrow is open for funding). This enables use cases such as:

- **Phase-based funding:** Start with allowlist disabled for early adopters, then enable for KYC'd investors
- **Emergency override:** Temporarily disable to allow urgent funding from non-allowlisted addresses
- **Gradual rollout:** Add addresses to the allowlist while the gate is disabled, then enable when ready

**Important:** Changing the toggle state affects **future** funding attempts only. It does not retroactively validate or invalidate existing contributions.

## Batch Operations

### Batch Bound

The batch operation [`LiquifactEscrow::set_investors_allowlisted`] is bounded by [`MAX_INVESTOR_ALLOWLIST_BATCH`] (32 entries) to keep storage and CPU work per call bounded. This prevents:

- Excessive storage writes in a single transaction
- Event emission spam (one event per address)
- CPU timeout risk on large batches

### Equivalence to Single Calls

The batch operation is **semantically equivalent” to calling [`LiquifactEscrow::set_investor_allowlisted`] individually for each address in the batch:

- Each address receives its own persistent storage write
- Each address receives its own TTL bump
- Each address emits its own [`InvestorAllowlistChanged`] event
- Admin authorization is required once for the entire batch

**Invariant:** The end state and emitted events after a batch call are identical to the same operations performed via single calls.

### Batch Use Cases

Batch operations are useful for:

- **Initial allowlist setup:** Adding KYC'd investors in bulk before funding opens
- **Bulk removal:** Removing a group of investors after compliance review
- **Periodic refresh:** Re-writing entries to extend TTLs for many addresses at once

## API Reference

### Admin Functions

#### `set_allowlist_active(env: Env, active: bool)`

Enable or disable the allowlist gate.

- **Authorization:** Admin only
- **Storage:** Writes to instance storage (`DataKey::AllowlistActive`)
- **Events:** Emits [`AllowlistEnabledChanged`]

#### `set_investor_allowlisted(env: Env, investor: Address, allowed: bool)`

Set whether a specific address is allowlisted.

- **Authorization:** Admin only
- **Storage:** Writes to persistent storage (`DataKey::InvestorAllowlisted(investor)`)
- **TTL:** Extends persistent TTL by [`PERSISTENT_TTL_MIN_EXTENSION_LEDGERS`]
- **Events:** Emits [`InvestorAllowlistChanged`]

#### `set_investors_allowlisted(env: Env, investors: Vec<Address>, allowed: bool)`

Batch set allowlist status for multiple addresses.

- **Authorization:** Admin only (once for the entire batch)
- **Storage:** Writes to persistent storage for each address
- **TTL:** Extends persistent TTL for each entry
- **Events:** Emits one [`InvestorAllowlistChanged`] per address
- **Bounds:** Rejects empty vectors or vectors > [`MAX_INVESTOR_ALLOWLIST_BATCH`] (32)
- **Errors:**
  - [`EscrowError::InvestorBatchEmpty`] (70) — empty vector
  - [`EscrowError::InvestorBatchTooLarge`] (71) — exceeds batch bound

### Read Functions

#### `is_allowlist_active(env: Env) -> bool`

Read whether the allowlist gate is active.

- **Authorization:** None (read-only)
- **Returns:** `true` if active, `false` if inactive
- **Default:** `false` when key is absent

#### `is_investor_allowlisted(env: Env, investor: Address) -> bool`

Check whether a specific address is allowlisted.

- **Authorization:** None (read-only)
- **Returns:** `true` if entry is `true`, `false` if entry is `false` or absent
- **Default:** `false` when key is absent (default-to-deny)

## Security Considerations

### TTL Management

Persistent storage entries can be archived if their TTL expires. To prevent silent allowlist disablement:

1. **Monitor TTL:** Track the age of allowlist entries off-chain
2. **Periodic refresh:** Call `set_investor_allowlisted(addr, true)` for active entries to extend TTL
3. **Gate validation:** When the gate is active, absent entries default to deny, so archival safely blocks funding

### Admin Key Security

The allowlist is controlled by the escrow admin. To prevent single-point-of-failure:

- Use a multisig or DAO-controlled admin address
- Implement governance processes for allowlist changes
- Document allowlist policies and change procedures

### Gate Bypass Risk

The allowlist gate is **only enforced in the contract**. To prevent bypass:

- Ensure all funding flows through the contract's `fund` or `fund_with_commitment` entrypoints
- Do not provide alternative funding paths that skip the gate
- Monitor for unexpected funding from non-allowlisted addresses when the gate is active

## Testing

Comprehensive tests for the allowlist model are located in `escrow/src/test_allowlist_tests.rs`. Key test coverage includes:

- Default states (disabled toggle, absent entries default to false)
- Enable/disable toggle behavior
- Add/remove per-address entries
- Fund gating when active vs inactive
- Batch operations and equivalence to single calls
- Batch bounds (empty, at bound, exceeds bound)
- Archived entry behavior (simulated via storage removal)
- Toggle during funding phase
- Multiple toggle cycles
- Entry persistence across enable/disable

Run tests with:

```bash
cargo test -p liquifact_escrow test_allowlist
```

## Related Documentation

- [Escrow Data Model](escrow-data-model.md) — Full storage schema and key reference
- [Escrow Error Messages](escrow-error-messages.md) — Typed error codes including `InvestorNotAllowlisted` (104)
- [ADR-007: Storage Key Evolution](adr/ADR-007-storage-key-evolution.md) — Persistent vs instance storage rationale
- [Escrow Gas and Storage Notes](escrow-gas-storage-notes.md) — Storage TTL and archival behavior
