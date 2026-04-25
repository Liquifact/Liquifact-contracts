# Escrow Data Model

This document describes every value persisted by the LiquiFact escrow contract, how keys are
structured in Soroban instance storage, and the rules that govern adding new keys without breaking
existing deployments.

For the policy that governs schema evolution see [ADR-007](adr/ADR-007-storage-key-evolution.md).

---

## Storage backend

All keys use `env.storage().instance()`. Instance storage is scoped to a single contract address
and is loaded in full on every host-function invocation. There is no persistent or temporary storage
in this contract.

Consequence: the total serialised size of all instance entries must stay within Soroban's
contract-data entry limit. The investor-contribution map is the main growth vector; it is bounded
by `MaxUniqueInvestorsCap` (default 128 per the README guardrails).

---

## `DataKey` enum — complete reference

`DataKey` is a `#[contracttype]` enum. Each variant becomes a distinct XDR-encoded key in the
instance storage map. The enum derives `Clone` so key values can be reused across get/set calls in
the same execution path without moving ownership.

### Scalar keys (always present after `init`)

| Variant | Rust type stored | Set by | Mutable? |
|---------|-----------------|--------|----------|
| `Escrow` | `InvoiceEscrow` | `init`, every state-changing entrypoint | Yes — rewritten atomically on each transition |
| `Version` | `u32` | `init` | Only via `migrate` |
| `FundingToken` | `Address` | `init` | No — immutable after init |
| `Treasury` | `Address` | `init` | No — immutable after init |
| `MinContributionFloor` | `i128` | `init` | No |
| `UniqueFunderCount` | `u32` | `init` (0), incremented by `fund_impl` | Yes — incremented on each new investor |
| `LegalHold` | `bool` | `set_legal_hold` / `clear_legal_hold` | Yes — toggled by admin |

`MinContributionFloor` is written as `0` even when no floor is configured, so reads always succeed
with `unwrap_or(0)`.

### Optional scalar keys (present only when configured)

| Variant | Rust type stored | Set by | Notes |
|---------|-----------------|--------|-------|
| `RegistryRef` | `Address` | `init` (when `registry` arg is `Some`) | Hint only — not an on-chain authority |
| `YieldTierTable` | `Vec<YieldTier>` | `init` (when non-empty tiers supplied) | Immutable after init |
| `FundingCloseSnapshot` | `FundingCloseSnapshot` | `fund_impl` on first transition to `status == 1` | Immutable once written |
| `SmeCollateralPledge` | `SmeCollateralCommitment` | `record_sme_collateral_commitment` | Record-only; replaceable by SME |
| `MaxUniqueInvestorsCap` | `u32` | `init` (when `max_unique_investors` arg is `Some`) | Absent means unlimited |
| `PrimaryAttestationHash` | `BytesN<32>` | `bind_primary_attestation_hash` | Single-set; panics on second call |
| `AttestationAppendLog` | `Vec<BytesN<32>>` | `append_attestation_digest` | Bounded by `MAX_ATTESTATION_APPEND_ENTRIES` (32) |

### Per-address keys (one entry per investor address)

These variants carry an `Address` discriminator so each investor gets an independent storage slot.

| Variant | Rust type stored | Set by | Default when absent |
|---------|-----------------|--------|---------------------|
| `InvestorContribution(Address)` | `i128` | `fund_impl` | `0` |
| `InvestorClaimed(Address)` | `bool` | `claim_investor_payout` | `false` |
| `InvestorEffectiveYield(Address)` | `i64` (bps) | `fund_impl` on first deposit | `InvoiceEscrow::yield_bps` |
| `InvestorClaimNotBefore(Address)` | `u64` (ledger timestamp) | `fund_impl` on first deposit | `0` (no gate) |

All four per-address keys are written together on an investor's first `fund` or
`fund_with_commitment` call. Subsequent `fund` calls update only `InvestorContribution`.

---

## Stored struct reference

### `InvoiceEscrow` (stored at `DataKey::Escrow`)

```rust
pub struct InvoiceEscrow {
    pub invoice_id: Symbol,       // validated ASCII [A-Za-z0-9_], max 32 chars
    pub admin: Address,
    pub sme_address: Address,
    pub amount: i128,             // original invoice face value
    pub funding_target: i128,     // may be updated while status == 0
    pub funded_amount: i128,      // running total; checked_add on each fund call
    pub yield_bps: i64,           // base annualised yield, 0..=10_000
    pub maturity: u64,            // ledger timestamp; 0 = no maturity gate
    pub status: u32,              // 0=open 1=funded 2=settled 3=withdrawn
}
```

`status` transitions are strictly forward. See [ADR-001](adr/ADR-001-state-model.md).

### `FundingCloseSnapshot` (stored at `DataKey::FundingCloseSnapshot`)

```rust
pub struct FundingCloseSnapshot {
    pub total_principal: i128,           // funded_amount at the moment status became 1
    pub funding_target: i128,
    pub closed_at_ledger_timestamp: u64,
    pub closed_at_ledger_sequence: u32,
}
```

Written once, atomically, inside `fund_impl` on the first transition to `status == 1`. Immutable
thereafter. Off-chain pro-rata share: `get_contribution(addr) / snapshot.total_principal`.

### `SmeCollateralCommitment` (stored at `DataKey::SmeCollateralPledge`)

```rust
pub struct SmeCollateralCommitment {
    pub asset: Symbol,
    pub amount: i128,
    pub recorded_at: u64,   // ledger timestamp at record time
}
```

Record-only. Does not custody tokens or trigger liquidation.

### `YieldTier` (element of `Vec<YieldTier>` at `DataKey::YieldTierTable`)

```rust
pub struct YieldTier {
    pub min_lock_secs: u64,
    pub yield_bps: i64,
}
```

Validated at `init`: `min_lock_secs` strictly increasing, `yield_bps` non-decreasing and each
`>= base yield_bps`. See [ADR-005](adr/ADR-005-tiered-yield.md).

---

## Schema version

`DataKey::Version` stores a `u32` written as `SCHEMA_VERSION` (currently `5`) at `init`. The
`migrate` entrypoint validates `from_version == stored` before applying any migration path. No
migration paths are currently implemented; adding a new optional key does not require a version bump
(see additive-key policy below).

---

## Additive-key policy

A new `DataKey` variant is **backward-compatible** when:

1. It is read with `.get(...).unwrap_or(default)` so old deployments (where the key is absent)
   behave as "unset / default".
2. It does not change the XDR shape of any existing variant or stored struct.
3. It does not alter the semantics of existing entrypoints when absent.

A change is **breaking** (requires migration or redeploy) when:

- An existing stored struct gains a required field (e.g. a new non-optional field on
  `InvoiceEscrow`).
- An existing `DataKey` variant is renamed or its XDR discriminant changes.
- An existing key's stored type changes (e.g. `LegalHold` from `bool` to `u32`).

See [ADR-007](adr/ADR-007-storage-key-evolution.md) for the full decision record and compatibility
test plan.

---

## Security notes

- **Token economics:** `external_calls::transfer_funding_token_with_balance_checks` asserts exact
  pre/post balance deltas. Fee-on-transfer or rebasing tokens are out of scope and will cause a
  safe panic. See [ADR-006](adr/ADR-006-dust-sweep-and-token-safety.md).
- **Collateral record:** `SmeCollateralPledge` is metadata only. It is not proof of encumbrance
  until a future version explicitly enforces token transfers.
- **Registry hint:** `RegistryRef` must not be used as an authority without verifying registry
  behavior off-chain or in a dedicated integration.
- **Attestation digests:** `PrimaryAttestationHash` and `AttestationAppendLog` store raw byte
  digests. The contract does not verify what the digest commits to; that is an off-chain concern.
- **Storage growth:** per-address keys grow linearly with investor count. `MaxUniqueInvestorsCap`
  (default 128) bounds this. Any schema change that adds per-address keys must re-evaluate the
  storage footprint against Soroban's contract-data entry limits.
