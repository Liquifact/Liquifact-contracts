# Escrow Data Model

Every value persisted by the LiquiFact escrow contract, its storage tier, value type,
default-on-absence behavior, and the schema version that introduced it.

For the evolution policy see [ADR-007](adr/ADR-007-storage-key-evolution.md).

---

## Storage tiers

| Tier | API | Scope | TTL |
|------|-----|-------|-----|
| **Instance** | `env.storage().instance()` | Loaded in full on every invocation | Shared with the contract instance; extend via `bump_ttl` |
| **Persistent** | `env.storage().persistent()` | Independent per-key entry | Per-key; must be extended independently |

All `DataKey` variants use **instance** storage except `InvestorAllowlisted(Address)`, which uses
**persistent** storage. This split means allowlist entries can expire independently of the rest of
the contract state — operators must extend both tiers together (see §5.4 of
`docs/escrow-security-checklist.md`).

---

## `DataKey` complete reference

`DataKey` is a `#[contracttype]` enum. Each variant is XDR-encoded as a distinct key.
`Clone` is derived so keys can be reused across get/set calls without moving ownership.

### Schema version changelog

| Version | Keys added | Upgrade path |
|---------|-----------|-------------|
| 1 | `Escrow`, `Version`, `InvestorContribution`, `LegalHold`, `SmeCollateralPledge`, `InvestorClaimed`, `FundingToken` | N/A |
| 2 | `InvestorEffectiveYield`, `InvestorClaimNotBefore` | Additive — old instances return defaults |
| 3 | `FundingCloseSnapshot`, `MinContributionFloor`, `MaxUniqueInvestorsCap`, `MaxPerInvestorCap`, `PendingAdmin`, `UniqueFunderCount` | Additive — old instances return defaults |
| 4 | `PrimaryAttestationHash`, `AttestationAppendLog` | Additive — old instances return defaults |
| 5 | `Treasury`, `RegistryRef`, `YieldTierTable`, `AllowlistActive`, `InvestorAllowlisted`, `InvestorRefunded` | Redeploy required if `InvoiceEscrow` XDR changed |
| 5† | `DistributedPrincipal` | Additive — old instances return `0` |

† Added after the v5 tag; backward-compatible additive key.

---

### All keys

| Variant | Storage | Value type | Default when absent | Schema version | Mutable? |
|---------|---------|-----------|---------------------|---------------|---------|
| `Escrow` | instance | `InvoiceEscrow` | — (panics if absent) | 1 | Yes — rewritten on every state transition |
| `Version` | instance | `u32` | `0` | 1 | Only via `migrate` |
| `InvestorContribution(Address)` | instance | `i128` | `0` | 1 | Yes — incremented per deposit; zeroed on `refund` |
| `LegalHold` | instance | `bool` | `false` | 1 | Yes — toggled by admin |
| `SmeCollateralPledge` | instance | `SmeCollateralCommitment` | absent | 1 | Yes — replaceable by SME |
| `InvestorClaimed(Address)` | instance | `bool` | `false` | 1 | Write-once `true`; second claim is a no-op |
| `FundingToken` | instance | `Address` | — (panics if absent) | 1 | No — immutable after init |
| `Treasury` | instance | `Address` | — (panics if absent) | 5 | No — immutable after init |
| `RegistryRef` | instance | `Address` | `None` | 5 | No — hint only, not an authority |
| `YieldTierTable` | instance | `Vec<YieldTier>` | absent (base yield applies) | 5 | No — immutable after init |
| `FundingCloseSnapshot` | instance | `FundingCloseSnapshot` | absent | 3 | Write-once on first `status → 1` |
| `InvestorEffectiveYield(Address)` | instance | `i64` (bps) | `InvoiceEscrow::yield_bps` | 2 | Write-once on first deposit |
| `InvestorClaimNotBefore(Address)` | instance | `u64` (ledger timestamp) | `0` (no gate) | 2 | Write-once on first deposit |
| `MinContributionFloor` | instance | `i128` | `0` (no floor) | 3 | No — written as `0` even when unconfigured |
| `MaxUniqueInvestorsCap` | instance | `u32` | absent (unlimited) | 3 | Lowerable via `lower_max_unique_investors` |
| `MaxPerInvestorCap` | instance | `i128` | absent (unlimited) | 3 | No — immutable after init |
| `PendingAdmin` | instance | `Address` | absent (no handover) | 3 | Set by `propose_admin`; cleared by `accept_admin` |
| `UniqueFunderCount` | instance | `u32` | `0` | 3 | Yes — incremented once per new investor |
| `PrimaryAttestationHash` | instance | `BytesN<32>` | absent | 4 | Write-once; panics on second call |
| `AttestationAppendLog` | instance | `Vec<BytesN<32>>` | absent (empty) | 4 | Append-only; bounded at 32 entries |
| `AllowlistActive` | instance | `bool` | `false` | 5 | Yes — toggled by admin |
| `InvestorAllowlisted(Address)` | **persistent** | `bool` | `false` | 5 | Yes — set by admin |
| `InvestorRefunded(Address)` | instance | `bool` | `false` | 5 | Write-once `true`; prevents double-refund |
| `DistributedPrincipal` | instance | `i128` | `0` | 5† | Yes — incremented by `refund` |

---

### Per-address keys

These variants carry an `Address` discriminator — each investor address gets an independent slot.

**Instance storage (per-investor):**
- `InvestorContribution(Address)` — principal credited; zeroed when `refund` runs
- `InvestorClaimed(Address)` — settlement claim marker (event only, no token transfer)
- `InvestorEffectiveYield(Address)` — yield bps locked at first deposit
- `InvestorClaimNotBefore(Address)` — claim lock timestamp from `fund_with_commitment`
- `InvestorRefunded(Address)` — double-refund guard in cancelled escrows

**Persistent storage (per-investor):**
- `InvestorAllowlisted(Address)` — funding gate when `AllowlistActive` is true

The persistent/instance split means `InvestorAllowlisted` entries have a different TTL lifecycle
from all other keys. If instance storage expires and `AllowlistActive` defaults to `false`, the
allowlist gate silently disables even if persistent entries remain. Always extend both tiers
together via `bump_ttl`.

---

## Stored struct reference

### `InvoiceEscrow` — `DataKey::Escrow`

```rust
pub struct InvoiceEscrow {
    pub invoice_id: Symbol,       // ASCII [A-Za-z0-9_], max 32 chars
    pub admin: Address,
    pub sme_address: Address,
    pub amount: i128,             // original invoice face value (> 0)
    pub funding_target: i128,     // updatable while status == 0
    pub funded_amount: i128,      // running total; checked_add on each fund call
    pub yield_bps: i64,           // base annualised yield, 0..=10_000
    pub maturity: u64,            // ledger timestamp; 0 = no maturity gate
    pub status: u32,              // see state machine below
}
```

**Status values:**

| Value | Name | Terminal? | Sweep allowed? |
|-------|------|-----------|---------------|
| 0 | open | No | No |
| 1 | funded | No | No |
| 2 | settled | Yes | Yes |
| 3 | withdrawn | Yes | Yes |
| 4 | cancelled | Yes | Yes (liability floor applies) |

Transitions are strictly forward: `0→1→2`, `0→1→3`, `0→4`. No entrypoint decrements status.

### `FundingCloseSnapshot` — `DataKey::FundingCloseSnapshot`

```rust
pub struct FundingCloseSnapshot {
    pub total_principal: i128,           // funded_amount at status → 1 (includes over-funding)
    pub funding_target: i128,
    pub closed_at_ledger_timestamp: u64,
    pub closed_at_ledger_sequence: u32,
}
```

Written once inside `fund_impl` on the first `status → 1` transition. Immutable thereafter.
Pro-rata share: `get_contribution(addr) / snapshot.total_principal`.

### `SmeCollateralCommitment` — `DataKey::SmeCollateralPledge`

```rust
pub struct SmeCollateralCommitment {
    pub asset: Symbol,
    pub amount: i128,
    pub recorded_at: u64,
}
```

Record-only metadata. Does not custody tokens, create a lien, or trigger liquidation.

### `YieldTier` — element of `DataKey::YieldTierTable`

```rust
pub struct YieldTier {
    pub min_lock_secs: u64,   // strictly increasing across tiers
    pub yield_bps: i64,       // non-decreasing, each >= base yield_bps
}
```

Validated at `init`. Immutable after init. Used by `fund_with_commitment` to select effective yield.

---

## Additive-key policy (ADR-007)

A new `DataKey` variant is **backward-compatible** when:
1. Read with `.get(...).unwrap_or(default)` — absent on old deployments returns the default.
2. Does not change the XDR shape of any existing variant or stored struct.
3. Does not alter existing entrypoint semantics when absent.

A change is **breaking** (requires migration or redeploy) when:
- An existing stored struct gains a required field.
- An existing `DataKey` variant is renamed or its XDR discriminant changes.
- An existing key's stored type changes.

`migrate` validates `stored_version == from_version` before any write. No migration paths are
currently implemented. Adding an optional key with `unwrap_or` does not require a version bump.

---

## Security notes

- **`DistributedPrincipal`** is incremented before the token transfer in `refund` (checks-effects-interactions). `sweep_terminal_dust` uses it to enforce the liability floor in cancelled escrows: `balance - sweep_amt >= funded_amount - distributed_principal`.
- **`InvestorRefunded`** is a double-spend guard. `InvestorContribution` is zeroed before the transfer; a second `refund` call fails at the `amount > 0` check.
- **`InvestorClaimed`** is an event marker only — no token transfer occurs. Off-chain systems must implement their own idempotency.
- **`AllowlistActive` / `InvestorAllowlisted` TTL mismatch** — see §5.4 of `docs/escrow-security-checklist.md`.
- **Storage growth** — per-address instance keys grow linearly with investor count. `MaxUniqueInvestorsCap` bounds this. Any new per-address key must re-evaluate the instance storage footprint.
