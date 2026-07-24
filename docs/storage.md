# Storage Model

This document describes the storage architecture of the `liquifact_escrow` contract (`escrow/src/lib.rs`).
It is based entirely on the current implementation: every `DataKey` variant, storage backend,
invariant, lifecycle step, and entrypoint cross-reference is grounded in the code, not assumed.
Reviewers and auditors should treat this file as the authoritative storage-layer description
of the contract, paired with [`docs/escrow-data-model.md`](escrow-data-model.md) (data shape) and
[`docs/adr/ADR-007-storage-key-evolution.md`](adr/ADR-007-storage-key-evolution.md) (key evolution rules).

## Source of truth

- **Storage code**: [`escrow/src/lib.rs`](../escrow/src/lib.rs)
- **Storage enum**: [`DataKey`](#storage-data-model) defined at lines 755–1052
- **`InvoiceEscrow` snapshot**: lines 1099–1118 (rewritten atomically on every transition)
- **Helper constants**: `SCHEMA_VERSION`, `MAX_*` bounds, `INSTANCE_TTL_MIN_EXTENSION_LEDGERS`,
  `PERSISTENT_TTL_MIN_EXTENSION_LEDGERS` at lines 119–224
- **TTL extension helper**: `bump_ttl()` at line 4999
- **State machine**: [`docs/escrow-lifecycle.md`](escrow-lifecycle.md)
- **Schema/version policy**: [`docs/escrow-schema-versioning.md`](escrow-schema-versioning.md)

---

# Storage Model

## Storage backends

The contract uses two Soroban storage tiers, both via `env.storage()`:

| Backend | `env` API | TTL behavior | Backing size budget | Used for |
|---------|-----------|--------------|---------------------|----------|
| **Instance** | `env.storage().instance()` | Shared TTL with the contract instance; extended via `bump_ttl` | A single contract-data entry; aggregate serialised size must stay within per-entry limits | Global config (`Escrow`, `Version`, `FundingToken`, `Treasury`, `LegalHold`, etc.) and structured snapshots (`FundingCloseSnapshot`, `YieldTierTable`) |
| **Persistent** | `env.storage().persistent()` | Per-key TTL (each address has an independent rent/alive window) | One entry per `DataKey::*(Address)` variant, so investor cardinality no longer crowds the instance entry | Per-investor data (contributions, yield, claim locks, claim markers, refunds, allowlist) |

There is no use of `env.storage().temporary()` in the current implementation. Every written or
read key lives in either **instance** or **persistent** storage as documented below.

The split between the two backends is preserved by [`ADR-007`](adr/ADR-007-storage-key-evolution.md)
and the additive-key policy in [`docs/escrow-data-model.md`](escrow-data-model.md). Relocating a
key between backends is **not** enumerable on-chain (investor addresses cannot be iterated from
storage) — see the `SCHEMA_VERSION = 6` row of the changelog for the rationale.

## Storage ownership and TTL extension

TTL is managed by:

- **Write-time extension**: `fund_impl` and `bump_ttl` extend persistent per-investor keys
  (`InvestorContribution`, `InvestorEffectiveYield`, `InvestorClaimNotBefore`,
  `InvestorClaimed`, `InvestorAllowlisted`) using `PERSISTENT_TTL_MIN_EXTENSION_LEDGERS`
  (≈1 h at 1 ledger/sec; lines ~212–220 in `lib.rs`).
- **Permissionless `bump_ttl`** ([`bump_ttl` entrypoint](../escrow/src/lib.rs) line 4999):
  extends instance keys that gate settlement/claim readiness (`Escrow`, `Version`, `LegalHold`,
  `AllowlistActive`, `FundingCloseSnapshot`) plus caller-supplied per-investor persistent keys.
  **Invariant**: the operation is monotonic — TTL is never shortened — and it performs no
  storage state mutation other than TTL extension.

## Storage keys — overview

`DataKey` is a `#[contracttype]` enum whose variants are encoded to XDR with fixed integer
discriminants equal to their 0-indexed position in the enum definition
([`ADR-007`](adr/ADR-007-storage-key-evolution.md), Rule 6). This means **variants must never be
renamed, removed, or reordered**; only appending new variants at the end is safe in-place.

The full enumerated list, storage backend, stored type, and mutability are given in the
[**Storage Data Model**](#storage-data-model) section below.

---

# Storage Data Model

This section enumerates every `DataKey` variant in declaration order, the storage backend it
lives in, the stored Rust type, the entrypoint that writes it, the entrypoints that read it,
and the absence default. The `Set by` column references the entrypoint that first writes the
key; subsequent writes are listed in the entrypoint cross-reference
[below](#entrypoints).

## Instance-storage entries

| `DataKey` variant | Stored type | Set by | Mutable after first write? | Absence default | Notes |
|-------------------|-------------|--------|----------------------------|-----------------|-------|
| `Escrow` | `InvoiceEscrow` | `init`; every state-mutating entrypoint | **Yes** — rewritten atomically on each transition | `EscrowNotInitialized` typed error | Central authoritative state. The full snapshot is replaced wholesale on each transition. |
| `Version` | `u32` | `init` | **No** (only via the (currently failing) `migrate` path) | `0` | Read via `get_version`. Current value: `SCHEMA_VERSION` = 6. |
| `LegalHold` | `bool` | `set_legal_hold` / `clear_legal_hold` | **Yes** — toggled by admin | `false` | Single-call admin gate; read by `guard_not_legal_hold` and every risk-bearing entrypoint. |
| `LegalHoldClearableAt` | `u64` | `request_clear_legal_hold` | **Yes** — overwritten on each request | absent ⇒ no clear pending | Two-phase hold-clear timestamp. |
| `LegalHoldClearDelay` | `u64` | `init` (when `legal_hold_clear_delay` supplied) | **No** | `0` (delay disabled) | Window required between `request_clear_legal_hold` and effective `set_legal_hold(false)`. |
| `SmeCollateralPledge` | `SmeCollateralCommitment` | `record_sme_collateral_commitment` | **Yes** — replaceable by SME | absent ⇒ no pledge | Record-only metadata — not an on-chain asset lock. |
| `FundingToken` | `Address` | `init` | **No** — immutable after init | `FundingTokenNotSet` typed error | SEP-41 funding-asset binding. |
| `Treasury` | `Address` | `init` | **No** — immutable after init | `TreasuryNotSet` typed error | Recipient of dust sweep and protocol fee at `withdraw`. |
| `RegistryRef` | `Address` | `init` (when `registry` arg is `Some(_)`) | **Yes** — removable via `clear_registry_ref` | absent ⇒ `None` | Hint only — not an on-chain authority. Read via `get_registry_ref`. |
| `YieldTierTable` | `Vec<YieldTier>` | `init` (when non-empty tiers supplied) | **No** — immutable after init | absent ⇒ no tiering | Per-investor tier selection happens on **first** deposit via `fund_with_commitment`. |
| `FundingCloseSnapshot` | `FundingCloseSnapshot` | `fund_impl` on first transition to `status == 1` | **No** — single-write | absent until funded | Immutable pro-rata denominator. |
| `MinContributionFloor` | `i128` | `init` (also written as `0` when not configured) | **Yes** — `lower_min_contribution_floor` lowers it | `0` (no floor) | Minimum per-call funding amount. |
| `MaxUniqueInvestorsCap` | `u32` | `init` (when `max_unique_investors` arg is `Some(_)`) | **Yes** — `lower_*` / `raise_*` mutate it | absent ⇒ unlimited | Distinct-investor count bound. |
| `MaxPerInvestorCap` | `i128` | `init` (when `max_per_investor` arg is `Some(_)`) | **Yes** — only raisable via `raise_max_per_investor` | absent ⇒ unlimited | Per-investor cap; raise-only. |
| `PendingAdmin` | `Address` | `propose_admin` | **Yes** — replaced on supersede, removed on accept/cancel | absent ⇒ no pending handover | Two-phase admin transfer. |
| `PendingAdminExpiry` | `u64` | `propose_admin` | **Yes** — replaced/removed with `PendingAdmin` | absent ⇒ no pending handover | Validity window in seconds from now. |
| `UniqueFunderCount` | `u32` | `init` (as `0`), `fund_impl` (incremented on new investor), `unfund` (decremented when contribution hits `0`, floor `0`) | **Yes** — increments on first deposit; decrements on last unfund | `0` | Tracks `|{addr : contribution(addr) > 0}|`. |
| `PrimaryAttestationHash` | `BytesN<32>` | `bind_primary_attestation_hash` | **No** — single-set; panics on second call | absent ⇒ no primary bound | Admin-only off-chain bundle attestation. |
| `AttestationAppendLog` | `Vec<BytesN<32>>` | `append_attestation_digest` | **Append-only** at the tail | absent ⇒ empty log | Bounded by `MAX_ATTESTATION_APPEND_ENTRIES` (32). |
| `AttestationRevoked(u32)` | `bool` | `revoke_attestation_digest` (writes `true`); `unrevoke_attestation_digest` (writes `false`) | **Yes** — flag flipped by revoke/unrevoke | absent ⇒ not revoked | Per-index revocation marker — preserves the original digest. |
| `AllowlistActive` | `bool` | `set_allowlist_active` | **Yes** — toggled by admin | `false` | Master allowlist gate. |
| `AllowlistIndex` | `Vec<Address>` | `set_investor_allowlisted` / `set_investors_allowlisted` | **Yes** — appended on add; not compacted on remove | absent ⇒ empty index | Required for enumeration; allowlist entry may still be `true` even when not present in the index (operator responsibility). |
| `DistributedPrincipal` | `i128` | `refund` / `refund_batch` | **Yes** — incremented on each successful refund | absent ⇒ `0` | Feeds the `sweep_terminal_dust` liability floor. |
| `MaturityMaxHorizon` | `u64` | `init` (when supplied) | **Yes** — `raise_maturity_max_horizon` raises it | `DEFAULT_MATURITY_MAX_HORIZON_SECS` | Longest maturity permitted from the current ledger time. |
| `FundingDeadline` | `u64` | `init` (when `funding_deadline` supplied) | **Yes** — `extend_funding_deadline` strictly pushes it forward | absent ⇒ no deadline | Optional funding cutoff; checked inside `fund_impl`. |
| `InvestorIndex` | `Vec<Address>` | `fund_impl` (append on first deposit) | **Yes** — append-only | absent ⇒ empty list | Used by `get_investors` for pagination. |
| `SettledAt` | `u64` | `settle` on transition to `status == 2` | **No** — single-write | absent ⇒ not yet settled | Read via `get_settled_at`. |
| `Paused` | `bool` | `set_paused` | **Yes** — toggled by admin | `false` | Lightweight operational pause (orthogonal to the legal hold). |
| `ProtocolFeeBps` | `i64` | `init` | **No** (init defines base) but **lowerable** via `lower_protocol_fee_bps` while open | `0` (no fee) | Immutable-from-init point of view; only managed by `lower_*` once running. |

Instance keys that **must always** be present post-`init` are: `Escrow`, `Version`,
`FundingToken`, `Treasury`, `UniqueFunderCount` (= `0`), `AllowlistActive` (= `false`),
`Paused` (= `false`), `LegalHold` (= `false`), `LegalHoldClearDelay` (= `0` unless configured),
`MinContributionFloor` (= `0` unless configured), `ProtocolFeeBps` (= `0` unless configured).
All other instance keys are written lazily and read with `unwrap_or(default)`.

## Persistent-storage entries (per investor)

Persistent keys are indexed by `Address` so each investor has an independent TTL. They are
extended at write time and again by `bump_ttl` (see lines ~5017–5057 in `escrow/src/lib.rs`).

| `DataKey` variant | Stored type | Set by | Mutable after first write? | Absence default | Notes |
|-------------------|-------------|--------|----------------------------|-----------------|-------|
| `InvestorContribution(Address)` | `i128` | `fund_impl` (and `fund_batch`); decremented by `unfund`; zeroed by `refund` | **Yes** — accumulates; only `unfund`/`refund` ever subtract | `0` | One entry per investor address. |
| `InvestorEffectiveYield(Address)` | `i64` (bps) | `fund_impl` on **first** deposit (via `fund_with_commitment` or tiered `fund`) | **No** — set once; preserves fairness | `InvoiceEscrow::yield_bps` | Tier selection is immutable after the first deposit leg. |
| `InvestorClaimNotBefore(Address)` | `u64` (ledger timestamp) | `fund_impl` on **first** deposit (when commitment lock is taken) | **No** — set once | `0` (no extra claim gate) | Combined with escrow `maturity` by `claim_investor_payout`. |
| `InvestorClaimed(Address)` | `bool` | `claim_investor_payout` | **No** — single-write | `false` | Idempotent `claim_investor_payout` returns early if `true`. |
| `InvestorAllowlisted(Address)` | `bool` | `set_investor_allowlisted` / `set_investors_allowlisted` | **Yes** — admin toggle | `false` | Read on every `fund`/`fund_with_commitment` while `AllowlistActive` is `true`. |

> **Note on `InvestorRefunded`.** Although `InvestorRefunded(Address)` is per-investor by name,
> the implementation stores it in **instance** storage
> (`env.storage().instance().set(&DataKey::InvestorRefunded(...), &true)` inside `refund_impl`,
> `escrow/src/lib.rs`). It is therefore a single `(bool, Address)` slot in the instance entry and
> does **not** have an independent TTL like the four explicitly-persistent investor keys
> (`InvestorContribution`, `InvestorEffectiveYield`, `InvestorClaimNotBefore`, `InvestorClaimed`).
> The DataKey rustdoc and ADR-007 list only those four (plus `InvestorAllowlisted`) as
> **Persistent** — `InvestorRefunded` is deliberately kept in instance storage.

## Stored struct reference

### `InvoiceEscrow` (at `DataKey::Escrow`)

```rust
pub struct InvoiceEscrow {
    pub invoice_id: Symbol,       // ASCII [A-Za-z0-9_], max 32 chars (MAX_INVOICE_ID_STRING_LEN)
    pub admin: Address,           // current admin authority
    pub sme_address: Address,     // payout destination at withdraw; rotatable via rotate_beneficiary
    pub amount: i128,             // original invoice face value (target hint)
    pub funding_target: i128,     // may be updated while status == 0
    pub funded_amount: i128,      // running total; checked_add on each fund call
    pub yield_bps: i64,           // base annualised yield, 0..=10_000
    pub maturity: u64,            // ledger timestamp; 0 = no maturity gate
    pub status: u32,              // 0=open 1=funded 2=settled 3=withdrawn 4=cancelled
}
```

### `FundingCloseSnapshot` (at `DataKey::FundingCloseSnapshot`)

```rust
pub struct FundingCloseSnapshot {
    pub total_principal: i128,             // funded_amount at close (including over-funding)
    pub funding_target: i128,
    pub closed_at_ledger_timestamp: u64,
    pub closed_at_ledger_sequence: u32,
}
```

### `SmeCollateralCommitment` (at `DataKey::SmeCollateralPledge`)

```rust
pub struct SmeCollateralCommitment {
    pub asset: Symbol,    // off-chain asset symbol
    pub amount: i128,     // reported amount (must be positive)
    pub recorded_at: u64, // Soroban ledger timestamp
}
```

### `YieldTier` (element of `Vec<YieldTier>` at `DataKey::YieldTierTable`)

```rust
pub struct YieldTier {
    pub min_lock_secs: u64,  // strictly increasing across the ladder
    pub yield_bps: i64,      // non-decreasing across the ladder; >= base yield at init time
}
```

### Composite read returns

`EscrowSummary`, `SettlementReadiness`, `InvestmentView`, and `ReconciliationView` are
**not** stored — they are assembled at read time from multiple keys to keep a single host
invocation cost for indexers.

## Private typed accessors (storage helpers)

Two private helpers centralise storage reads for immutable addresses and guarantee consistent
typed errors at every call site:

| Helper | Key read | Error on absence |
|--------|----------|-----------------|
| `funding_token_or_fail(&env)` | `DataKey::FundingToken` | [`EscrowError::FundingTokenNotSet`](../escrow/src/lib.rs) (code 21) |
| `treasury_or_fail(&env)` | `DataKey::Treasury` | [`EscrowError::TreasuryNotSet`](../escrow/src/lib.rs) (code 22) |

Both helpers panic with the typed error listed above when the key is missing. The public
getters `get_funding_token` and `get_treasury` delegate to them; internal callers
(`sweep_terminal_dust`, `refund`, etc.) also use them instead of inlining
`.get().unwrap_or_else(|| fail(...))`.

---

# Storage Invariants

The invariants below are **only** the ones actually enforced by typed errors and assertions in
the code. Every invariant names the enforcement point (`ensure(…)`, `guard_*`, or
`is_terminal_status`/`is_pre_settlement_status`).

## Structural invariants

- **INV-1 — Single initialization.** `DataKey::Escrow` may be present **at most once**. A
  second `init()` call panics with [`EscrowError::EscrowAlreadyInitialized`](../escrow/src/lib.rs)
  (code 3). The check is performed by
  `ensure(!env.storage().instance().has(&DataKey::Escrow), EscrowError::EscrowAlreadyInitialized)`.

- **INV-2 — Escrow must exist before mutation.** Every state-mutating entrypoint reads
  `DataKey::Escrow` first and panics with
  [`EscrowError::EscrowNotInitialized`](../escrow/src/lib.rs) (code 20) if absent.
  `load_escrow_require_admin` and `load_escrow_require_sme` centralise this read.

- **INV-3 — Funding token / treasury always set after successful init.** `init()` writes
  `DataKey::FundingToken` and `DataKey::Treasury` before completing. Every later entrypoint
  reads them via `funding_token_or_fail` / `treasury_or_fail`; absence implies an `init` was
  aborted/corrupted and panics with codes 21 / 22.

- **INV-4 — `DataKey` enum variant ordering is append-only.** Variants are encoded with a
  discriminant equal to their declaration index. Renaming, reordering, or removing a variant
  changes its on-chain discriminant and breaks existing storage. ADR-007 Rule 6 enforces this.
  Storage-key evolution is otherwise constrained by [`ADR-007`](adr/ADR-007-storage-key-evolution.md).

## Status-state invariants

- **INV-5 — `status` is monotonically non-decreasing.** Valid transitions:
  `0 → 1`, `0 → 4`, `1 → 2`, `1 → 3`. Statuses 2/3/4 are terminal. Regression is impossible
  because the new `status` is computed as `escrow.status + 1` (or set to specific terminal
  values) before being written back into `DataKey::Escrow`. See
  [`ADR-001`](adr/ADR-001-state-model.md) and [`docs/escrow-lifecycle.md`](escrow-lifecycle.md).

- **INV-6 — `fund` / `fund_with_commitment` / `fund_batch` require `status == 0`.** Enforced
  by `require_funding_open` (helper at lines ~520–560) which delegates to
  `guard_status_eq(status, 0, EscrowError::EscrowNotOpenForFunding)`.

- **INV-7 — `unfund` requires `status == 0`.** Enforced inside `unfund` with
  [`EscrowError::EscrowNotOpen`](../escrow/src/lib.rs) (code 221).

- **INV-8 — `cancel_funding` requires `status == 0`.** Enforced with
  [`EscrowError::CancelFundingNotOpen`](../escrow/src/lib.rs) (code 141). `refund` requires
  `status == 4` via [`EscrowError::RefundNotCancelled`](../escrow/src/lib.rs) (code 142).

- **INV-9 — `settle` requires `status == 1` and maturity gate passed.** With `maturity > 0`,
  `ledger.timestamp() >= maturity` is required (else `MaturityNotReached`, code 122).
  `partial_settle` requires `status == 0` (`PartialSettleNotOpen`, code 202).

- **INV-10 — `withdraw` requires `status == 1`.** Withdraw is only valid from funded state
  (`WithdrawalNotFunded`, code 124).

- **INV-11 — `claim_investor_payout` requires `status == 2` and `InvestorClaimNotBefore <= now`.**
  Enforced by [`EscrowError::InvestorClaimNotSettled`](../escrow/src/lib.rs) (code 127) and
  [`EscrowError::InvestorCommitmentLockNotExpired`](../escrow/src/lib.rs) (code 128).

## Funding invariants

- **INV-12 — `funded_amount` accumulates without overflow.** Every `fund_impl`/`fund_batch`
  leg uses `i128::checked_add` and panics with
  [`EscrowError::FundedAmountOverflow`](../escrow/src/lib.rs) (code 110) on overflow.

- **INV-13 — Per-investor contribution accumulates without overflow.** Enforced by
  [`EscrowError::InvestorContributionOverflow`](../escrow/src/lib.rs) (code 105).

- **INV-14 — `funded_amount ≥ sum over investors of InvestorContribution(investor)`.**
  `unfund` and `refund` update both `funded_amount` and the matching `InvestorContribution`
  atomically inside the same `ensure`-checked transaction. `withdraw` does **not** mutate any
  `InvestorContribution` — the SME pulls the contract balance, and the per-investor records
  remain in place for audit and post-`withdraw` reads. `claim_investor_payout` transfers the
  pro-rata share out via the SEP-41 balance-checked wrapper; whether the per-investor
  contribution is zeroed on a successful claim is determined inside the function body and
  not asserted by this invariant — read the dedicated entrypoint row in
  [Entrypoints](#entrypoints) for the canonical write-set.

- **INV-15 — Min contribution floor enforced.** When `MinContributionFloor > 0`, every fund
  amount `≥ floor` (else `FundingBelowMinContribution`, code 101). Floor lowering is
  admin-gated and bound to `status == 0`.

- **INV-16 — `max_per_investor` raises-only.** Raise via `raise_max_per_investor`; lowering is
  not exposed (`NewAdmin same as current` / `MaxPerInvestorCapNotRaised` per spec). Cap
  enforced against `InvestorContribution(addr) + amount` on every deposit.

- **INV-17 — Unique-investor cap enforced.** `UniqueFunderCount` increments exactly once per
  new investor address; `fund_impl` rejects when the new count would exceed the stored cap
  (`UniqueInvestorCapReached`, code 107).

- **INV-18 — `fund_with_commitment` first-deposit-only.** After `InvestorContribution > 0`,
  calling `fund_with_commitment` returns `TieredSecondDeposit` (code 108); further principal
  from that address must go through `fund()`. Tier selection is immutable.

- **INV-19 — Commitment lock fits before maturity.** `fund_with_commitment` rejects, with
  [`EscrowError::CommitmentLockExceedsMaturity`](../escrow/src/lib.rs) (code 111), any deposit
  whose per-investor `claim_not_before` timestamp would not fit inside the escrow's `maturity`
  window — i.e. an investor cannot bind principal to a claim-lock that the escrow is unable
  to honour. The exact comparison operator (strict `>` vs `>=`) is enforced inside
  `fund_with_commitment`'s body and should be verified against the deployed source.

## Snapshot / payout invariants

- **INV-20 — `FundingCloseSnapshot` is single-write.** Written exactly once at the first
  `status` transition `0 → 1` (including in batch middle entries). Subsequent writes are
  blocked by a `!env.storage().instance().has(&DataKey::FundingCloseSnapshot)` check. Field
  `total_principal` equals `funded_amount` at the moment of the crossing deposit; `total_principal`
  is therefore the pro-rata denominator.

- **INV-21 — `SettledAt` is single-write.** Written once when `settle` flips status to `2`;
  reading returns `None` for legacy instances and `Some(timestamp)` afterwards.

- **INV-22 — `InvestorClaimed` is single-write.** Set to `true` after a successful payout
  transfer; a subsequent `claim_investor_payout` returns silently without re-emitting the
  transfer or event.

- **INV-23 — Protocol-fee conservation.** At `withdraw`,
  `fee + sme_payout == funded_amount` by construction (floor division, two `checked_*`
  operations). The contract `fee` goes to `Treasury` and `sme_payout` to `sme_address`; no
  principal is created or destroyed by the split.

- **INV-24 — Refund-side liability tracking.** `DistributedPrincipal` tracks principal
  already returned via `refund`/`refund_batch`. `sweep_terminal_dust` enforces
  `balance − sweep_amt ≥ funded_amount − DistributedPrincipal`
  ([`EscrowError::SweepExceedsLiabilityFloor`](../escrow/src/lib.rs), code 42).

## Attestation invariants

- **INV-25 — `PrimaryAttestationHash` is single-set.** Panics with
  [`EscrowError::PrimaryAttestationAlreadyBound`](../escrow/src/lib.rs) (code 50) on a second
  bind.

- **INV-26 — `AttestationAppendLog` bounded.** Append length capped at
  `MAX_ATTESTATION_APPEND_ENTRIES` (32); a 33rd append returns
  [`EscrowError::AttestationAppendLogCapacityReached`](../escrow/src/lib.rs) (code 51).
  Revocation does NOT consume a slot; the original digest stays at the same index, marked
  `AttestationRevoked(index) = true`.

- **INV-27 — Attestation revocation trees are idempotent.** Indices must be `in range`
  (`AttestationIndexOutOfRange`, code 52), not already revoked (`AttestationAlreadyRevoked`,
  code 53), and the batch must satisfy `AttestationBatchEmpty` / `AttestationBatchTooLarge`
  bounds.

## Authorization / governance invariants

- **INV-28 — Single-init admin guard.** Subsequent `init` calls panic with
  `EscrowAlreadyInitialized` (INV-1). Only `init` writes `DataKey::Version` to `SCHEMA_VERSION`.

- **INV-29 — Admin handover is two-phase.** `propose_admin` writes both `PendingAdmin` and
  `PendingAdminExpiry`; `accept_admin` validates that:
  1. the caller matches `PendingAdmin` (auth required),
  2. `now < PendingAdminExpiry` (else `AdminProposalExpired`, code 85),
  3. `now != admin` and `new_pending != current_pending` (else `NewAdminSameAsCurrent` / `PendingAdminUnchanged`).

- **INV-30 — Pause flag is independent of legal hold.** Either flag independently blocks the
  four risk-bearing entrypoints (`fund`, `settle`, `withdraw`, `claim_investor_payout`).
  Clearing one does not affect the other. The pause is a single-call admin switch without a
  two-phase delay.

- **INV-31 — Legal-hold clearing is two-phase.** `set_legal_hold(false)` after a
  `request_clear_legal_hold` is enforced as a no-op when
  `LegalHoldClearableAt` is unset (`LegalHoldClearRequestMissing`, code 150) or the delay
  has not elapsed (`LegalHoldClearNotReady`, code 151).

- **INV-32 — Beneficiary rotation only in pre-settlement.** `rotate_beneficiary` checks
  `is_pre_settlement_status(status)` and panics with `RotationNotOpen` (code 161) or
  `LegalHoldBlocksBeneficiaryRotation` (code 160).

## TTL invariants

- **INV-33 — `bump_ttl` is monotonic.** TTL is only ever extended (never shortened). The
  operation touches only TTLs of the supplied keys; no state mutations occur.

- **INV-34 — Per-investor persistent keys are extended on write.** `fund_impl` and
  `claim_investor_payout` extend the relevant persistent keys
  (`InvestorContribution`/`InvestorEffectiveYield`/`InvestorClaimNotBefore`/`InvestorClaimed`)
  using `PERSISTENT_TTL_MIN_EXTENSION_LEDGERS` (≈1h at 1 ledger/sec).

- **INV-35 — Instance-storage TTL extended at settle and bump_ttl.** `settle` extends the
  instance entry to ensure post-settlement reads remain rent-free. `bump_ttl` extends
  `DataKey::Escrow`, `Version`, `LegalHold`, `AllowlistActive`, `FundingCloseSnapshot`,
  plus caller-supplied per-investor keys.

- **INV-36 — Per-investor entries are persistent and never removed.** Once written,
  per-investor persistent keys (`InvestorContribution`, `InvestorEffectiveYield`,
  `InvestorClaimNotBefore`, `InvestorClaimed`) and the per-investor instance key
  `InvestorRefunded` are **never deleted** by any entrypoint. Refunds, unfunds, and claims
  mutate the stored value (zeroing the principal, setting the bool to `true`) but the storage
  slot remains. The only path to remove a slot from contract storage is TTL expiry; `bump_ttl`
  (INV-33) can keep entries alive across long-running escrows."

---

# Storage Lifecycle

The lifecycle below traces the **actual** state transitions of the storage tier per
implementation path. Each phase lists the keys touched, the entrypoints that drive it, and
the storage backend involved.

## Phase 1 — Initialization

Performed by [`LiquifactEscrow::init`](../escrow/src/lib.rs) (line 1791).

1. **Guard — escrow must be empty.** `ensure(!instance().has(DataKey::Escrow), EscrowAlreadyInitialized)` (INV-1).
2. **Validate inputs.** Yield basis points, amounts, `invoice_id` charset/length, tier ladder
   monotonicity, optional `legal_hold_clear_delay`, `protocol_fee_bps`,
   `funding_deadline ≤ maturity`, etc.
3. **Write the auxiliary instance keys first** (`DataKey::FundingToken`, `DataKey::Treasury`,
   `DataKey::Version` = `SCHEMA_VERSION` = 6, optional `DataKey::RegistryRef`,
   `DataKey::YieldTierTable`, `DataKey::LegalHoldClearDelay`, `DataKey::MaxUniqueInvestorsCap`,
   `DataKey::MaxPerInvestorCap`, `DataKey::MaturityMaxHorizon`, `DataKey::FundingDeadline`,
   `DataKey::MinContributionFloor` (default `0`), `DataKey::ProtocolFeeBps` (default `0`),
   `DataKey::UniqueFunderCount` = `0`, `DataKey::AllowlistActive` = `false`,
   `DataKey::Paused` = `false`, `DataKey::LegalHold` = `false`). Optional keys are
   written only when the corresponding `init` argument is `Some(_)` (or when the explicit
   zero default is required).
4. **Construct and persist the `InvoiceEscrow` snapshot** as the final write
   (`DataKey::Escrow` ← validated `InvoiceEscrow { status = 0 }`). Writing `Escrow` last
   means that any partial-init failure leaves no stale `DataKey::Escrow` behind; the
   `EscrowAlreadyInitialized` guard (INV-1) therefore never trips on a half-initialized state.
5. **Emit `EscrowInitialized`** and **extend instance TTL** so the freshly initialized state
   is rent-stable for at least `INSTANCE_TTL_MIN_EXTENSION_LEDGERS` ahead.

After init:
- **No persistent keys** are written unless an investor funds.
- The escrow is in `status == 0` (Open).

## Phase 2 — Mutation

### Funding path

`fund`, `fund_with_commitment`, `unfund`, `fund_batch` (and the cancel/refund path) live here.

- **Read**: `DataKey::Escrow` (always), `MinContributionFloor`, `MaxUniqueInvestorsCap`,
  `MaxPerInvestorCap`, `FundingDeadline`, `LegalHold`, `Paused`, `AllowlistActive`,
  `InvestorAllowlisted(addr)`, `InvestorContribution(addr)`, `InvestorEffectiveYield(addr)`,
  `InvestorClaimNotBefore(addr)`, `YieldTierTable`, `UniqueFunderCount`, `MaturityMaxHorizon`,
  `FundingCloseSnapshot` (only on status flip).
- **Write (new investor)**:
  - `InvestorContribution(addr)` (persistent)
  - `InvestorEffectiveYield(addr)` (persistent) — first deposit only
  - `InvestorClaimNotBefore(addr)` (persistent) — first deposit only
  - `InvestorIndex` (append) (instance)
  - `UniqueFunderCount += 1` (instance)
  - `Escrow.funded_amount` via rewrite of `DataKey::Escrow` (instance)
- **Write (returning investor, plain fund)**:
  - `InvestorContribution(addr) += amount` (persistent)
  - `Escrow.funded_amount += amount` (instance)
- **Write (status flip `0 → 1`)**: `DataKey::FundingCloseSnapshot` once (instance, immutable).
- **Write (unfund)**:
  - `InvestorContribution(addr) -= amount` (persistent); zero when contribution reaches `0`
  - `Escrow.funded_amount -= amount` (instance)
  - `UniqueFunderCount -= 1` if `contribution → 0` (saturating floor at `0`)
- **Write (`cancel_funding` + `refund` path)**:
  - `Escrow.status = 4` (instance) — written by `cancel_funding`
  - `InvestorContribution(addr) → 0` after transfer (persistent) — written by `refund`/`refund_batch`
  - `InvestorRefunded(addr) = true` (**instance**, written once per refund; see the note in
    the [Persistent-storage](#persistent-storage-entries-per-investor) section)
  - `DistributedPrincipal += amount` (instance) — feeds the dust-sweep liability floor
  - `Escrow.funded_amount -= amount` (instance)

Each path extends the persistent TTL of every key written (INV-34).

### Configuration / governance path

`update_funding_target`, `update_maturity`, `lower/raise_max_unique_investors`,
`lower_min_contribution_floor`, `lower_protocol_fee_bps`, `raise_max_per_investor`, `set_paused`,
`set_legal_hold`, `request_clear_legal_hold`, `clear_legal_hold`, `rotate_beneficiary`,
`rebind_registry_ref`, `clear_registry_ref`, `propose_admin`, `accept_admin`,
`cancel_pending_admin`, `set_allowlist_active`, `set_investor_allowlisted(s)`,
`update_maturity_max_horizon`, `raise_maturity_max_horizon`, `extend_funding_deadline`.

These entrypoints touch only instance keys. They never write persistent keys and never delete
existing keys except `PendingAdmin` (cleared on accept/cancel; see below) and `RegistryRef`
(removed by `clear_registry_ref`).

### Settlement path

`settle`, `withdraw`, `claim_investor_payout`, `partial_settle`.

- **`settle`**: writes `Escrow.status = 2` (instance), writes `SettledAt` (instance, single-write,
  INV-21). May transition before writing depletion/allowlist side-effects. Pro-rata math
  computes the pool but does not move tokens.
- **`withdraw`** (line 4396): writes `Escrow.status = 3` (instance), updates `DistributedPrincipal`
  with the gross `funded_amount`, and emits `SmeWithdrew`. The funded principal is split between
  `Treasury` (fee portion, `funded_amount * ProtocolFeeBps / 10_000`) and `sme_address` (net).
  Both transfers go through `external_calls::transfer_funding_token_with_balance_checks`. A
  pre-transfer balance check rejects with `InsufficientContractBalance` (165) if the contract
  holds less than `funded_amount` of the funding token. The per-investor keys
  (`InvestorContribution`, `InvestorClaimed`, etc.) are **never** mutated by `withdraw` — the
  SME takes the contract balance, the contribution records are retained for audit.
- **`claim_investor_payout`**: writes `InvestorClaimed(addr) = true` (persistent, single-write).
  Internal pro-rata math uses `FundingCloseSnapshot.total_principal` and
  `InvestorContribution(addr)`. Idempotent — second call short-circuits (INV-22).

## Phase 3 — Read paths

Pure read functions never mutate storage and never extend TTL by themselves.

- **General getters** (`get_escrow`, `get_version`, `get_funding_token`, `get_treasury`,
  `get_pending_admin`, `get_pending_admin_expiry`, `get_pending_admin_remaining_secs`,
  `has_maturity_lock`, `get_legal_hold`, `get_legal_hold_clear_delay`,
  `get_legal_hold_clearable_at`, `get_min_contribution_floor`, `get_protocol_fee_bps`,
  `get_max_unique_investors_cap`, `get_max_per_investor_cap`, `get_unique_funder_count`,
  `is_allowlist_active`, `is_investor_allowlisted`, `is_paused`, `get_remaining_funding_capacity`,
  `get_funding_deadline`, `is_funding_expired`, `is_investor_refunded`,
  `get_distributed_principal`, `get_maturity_max_horizon`).
- **Attestation reads** (`get_primary_attestation_hash`, `get_attestation_append_log`,
  `get_attestation_digest_at`, `is_attestation_revoked`, `get_revoked_attestation_digests`).
- **Investor reads** (`get_contribution`, `get_contributions`, `get_investors`,
  `get_investor_yield_bps`, `get_investor_claim_not_before`, `is_investor_claimed`).
- **Settlement reads** (`is_settleable`, `get_settlement_readiness`, `get_settlement_pool`,
  `get_settled_at`, `get_funding_close_snapshot`, `compute_investor_payout`,
  `get_claimable_payout`).
- **Composite reads** (`get_escrow_summary`, `get_reconciliation`).
- **Allowlist reads** (`get_allowlisted_investors`, `get_allowlisted_investors_count`).
- **Yield reads** (`get_yield_tiers`, `preview_yield_tier`).
- **Collateral reads** (`get_sme_collateral_commitment`).

All getters read with `unwrap_or(default)` for optional keys (INV-3 implicit).

## Phase 4 — Deletion

The contract performs these explicit deletions:

| Operation | Keys deleted |
|-----------|--------------|
| `accept_admin` | `PendingAdmin`, `PendingAdminExpiry` |
| `cancel_pending_admin` | `PendingAdmin`, `PendingAdminExpiry` |
| `clear_registry_ref` | `RegistryRef` |
| `undo`/tier re-selection | not currently exposed |

No other entrypoint deletes a key. Once written, the following keys are **never** deleted for
the lifetime of the escrow instance:

- `Escrow`, `Version`, `FundingToken`, `Treasury`, `YieldTierTable`, `MaturityMaxHorizon`,
  `ProtocolFeeBps`, `UniqueFunderCount`, `LegalHoldClearDelay`,
  every `FundingCloseSnapshot` (single-write), every `SettledAt` (single-write),
  every `InvestorClaimed` (single-write), every `InvestorEffectiveYield` /
  `InvestorClaimNotBefore` (set on first deposit and never cleared),
  every `InvestorRefunded` (single-write),
  every `PrimaryAttestationHash` (single-set).

INV-20 (`FundingCloseSnapshot`), INV-21 (`SettledAt`), INV-22 (`InvestorClaimed`),
and INV-25 (`PrimaryAttestationHash`) ensure once-only writes for snapshot / single-write keys.
The unset / zero manipulation of `InvestorContribution` (in `unfund`/`refund`) and
`InvestorRefunded` (in `refund`) is **not** a deletion — the `bool` entry persists at value
`true` so subsequent calls short-circuit; see Phase 5 below.

## Phase 5 — Cleanup

There is no dedicated "cleanup" entrypoint — storage is preserved for the lifetime of the
contract instance. The only post-deployment deterministic reduction in storage footprint
occurs when an investor's contribution is zeroed by `refund` or a full `unfund`:

- `InvestorContribution(addr)` is **set to `0`** (the entry remains; it is not removed).
  Reading the key still returns `0`. No allocation churn.
- `InvestorRefunded(addr) = true` is preserved so that a second `refund` is a no-op.
- `DistributedPrincipal` retains the running total so that
  `sweep_terminal_dust` can compute `outstanding = funded_amount − DistributedPrincipal`.

The Soroban rent/archival model means that persistent entries remain subject to TTL
expiry; `bump_ttl` (INV-33) is the only re-activation path for TTL-expired keys.

---

# Entrypoints

This section enumerates every `pub fn` on the `LiquifactEscrow` contract and tags which keys it
**reads**, **writes**, or **deletes**. Every entrypoint listed here is declared in
`escrow/src/lib.rs`. The line numbers below are reproduced from the same source file at the time
of this document's authorship.

## Initialization & version

| Entrypoint | Reads | Writes | Deletes |
|------------|-------|--------|---------|
| `init` (line 1791) | empty base | `FundingToken`, `Treasury`, `Version`, possibly `RegistryRef`, `YieldTierTable`, `LegalHoldClearDelay`, `MaxUniqueInvestorsCap`, `MaxPerInvestorCap`, `MaturityMaxHorizon`, `FundingDeadline`, `MinContributionFloor`, `ProtocolFeeBps`, `UniqueFunderCount=0`, `AllowlistActive=false`, `Paused=false`, `LegalHold=false` — followed by `Escrow` (last) | — |
| `migrate` (line 3790) | `Version` | (intended for future migration paths; currently fails typed — see `AlreadyCurrentSchemaVersion` / `NoMigrationPath`) | — |
| `upgrade` (line 3911) | — | — | — (replaces WASM only; touches no storage) |
| `get_version` (line 2284) | `Version` | — | — |

## Funding

| Entrypoint | Reads | Writes | Deletes |
|------------|-------|--------|---------|
| `fund` (line 3933) | `Escrow`, `LegalHold`, `Paused`, `MinContributionFloor`, `MaxUniqueInvestorsCap`, `MaxPerInvestorCap`, `FundingDeadline`, `UniqueFunderCount`, `AllowlistActive`, `InvestorAllowlisted(addr)`, `InvestorContribution(addr)`, `InvestorEffectiveYield(addr)`, `InvestorClaimNotBefore(addr)`, `FundingCloseSnapshot` (on status flip) | `Escrow`, `InvestorContribution(addr)`, possibly `InvestorIndex`, `UniqueFunderCount`, `FundingCloseSnapshot` (single-write), persistent TTL extensions | — |
| `fund_with_commitment` (line 3944) | same as `fund` plus tier lookup | same as `fund` plus `InvestorEffectiveYield` / `InvestorClaimNotBefore` on first deposit only | — |
| `fund_batch` (line 3975) | per-entry: as `fund` | per-entry: as `fund` | — |
| `unfund` (line 5456) | `Escrow`, `LegalHold`, `InvestorContribution(addr)`, `UniqueFunderCount` | `Escrow.funded_amount`, `InvestorContribution(addr)`, possibly `UniqueFunderCount` | — |
| `cancel_funding` (line 5276) | `Escrow`, `LegalHold` | `Escrow.status = 4` | — |
| `refund` (line 5308) | `Escrow`, `InvestorContribution(addr)`, `InvestorRefunded(addr)`, `FundingToken`, `Treasury` | `InvestorContribution(addr) ← 0`, `InvestorRefunded(addr) ← true` (instance), `DistributedPrincipal`, `Escrow.funded_amount` | — |
| `refund_batch` (line 5392) | per-investor: as `refund` | per-investor: as `refund` | — |
| `is_investor_refunded` | `InvestorRefunded(addr)` | — | — |
| `get_distributed_principal` | `DistributedPrincipal` | — | — |
| `get_remaining_investor_slots` (line 4915) | `MaxUniqueInvestorsCap`, `UniqueFunderCount` | — | — |

## Settlement / payout / withdrawal

| Entrypoint | Reads | Writes | Deletes |
|------------|-------|--------|---------|
| `partial_settle` (line 4277) | `Escrow` (caller is `sme_address` or `admin`), `LegalHold`, `Maturity` | `Escrow.status = 1` (funded) and `FundingCloseSnapshot` (single-write, only when absent) | — |
| `settle` (line 4320) | `Escrow`, `LegalHold`, `Paused`, `Maturity`, `FundingCloseSnapshot` | `Escrow.status = 2`, `SettledAt` | — |
| `withdraw` (line 4396) | `Escrow`, `FundingToken`, `Treasury`, `ProtocolFeeBps`, `LegalHold`, `Paused`, contract token balance | `Escrow.status = 3` (after token transfer); `DistributedPrincipal` increased by `funded_amount`; never mutates per-investor keys | — |
| `claim_investor_payout` (line 4526) | `Escrow`, `FundingToken`, `InvestorContribution(addr)`, `InvestorClaimed(addr)`, `InvestorClaimNotBefore(addr)`, `InvestorEffectiveYield(addr)`, `FundingCloseSnapshot`, `LegalHold`, `Paused` | `InvestorClaimed(addr) ← true` (idempotent) | — |
| `is_settleable` | `Escrow`, `LegalHold`, `maturity` vs. ledger timestamp | — | — |
| `get_settlement_readiness` | `Escrow`, `LegalHold`, `Maturity` | — | — |
| `get_settlement_pool`, `compute_investor_payout`, `get_claimable_payout` | `Escrow`, `InvestorContribution(addr)`, `InvestorEffectiveYield(addr)`, `FundingCloseSnapshot` | — | — |
| `get_settled_at` | `SettledAt` | — | — |
| `get_funding_close_snapshot` | `FundingCloseSnapshot` | — | — |

## Configuration / governance

| Entrypoint | Reads | Writes | Deletes |
|------------|-------|--------|---------|
| `update_funding_target` (line 3461) | `Escrow` | `Escrow.funding_target` | — |
| `update_maturity` (line 4793) | `Escrow` | `Escrow.maturity` | — |
| `update_maturity_max_horizon` | `MaturityMaxHorizon` | `MaturityMaxHorizon` | — |
| `raise_maturity_max_horizon` | `MaturityMaxHorizon` | `MaturityMaxHorizon` | — |
| `extend_funding_deadline` | `FundingDeadline`, `Escrow.maturity` | `FundingDeadline` | — |
| `lower_max_unique_investors` | `MaxUniqueInvestorsCap`, `UniqueFunderCount` | `MaxUniqueInvestorsCap` | — |
| `raise_max_unique_investors` | `MaxUniqueInvestorsCap` | `MaxUniqueInvestorsCap` | — |
| `lower_min_contribution_floor` | `MinContributionFloor` | `MinContributionFloor` | — |
| `lower_protocol_fee_bps` | `ProtocolFeeBps` | `ProtocolFeeBps` | — |
| `raise_max_per_investor` | `MaxPerInvestorCap` | `MaxPerInvestorCap` | — |
| `set_paused` | `Paused` | `Paused` | — |
| `is_paused`, `set_legal_hold` | `LegalHold` (read) / `LegalHold`, `LegalHoldClearableAt` (write for legal-hold clear) | as appropriate | — |
| `request_clear_legal_hold` | `LegalHoldClearDelay` | `LegalHoldClearableAt` | — |
| `clear_legal_hold` / `clear_legal_hold_after_delay` / `cancel_clear_legal_hold` | `LegalHold`, `LegalHoldClearableAt`, `LegalHoldClearDelay` | `LegalHold` | — |
| `rotate_beneficiary` | `Escrow`, `LegalHold` | `Escrow.sme_address` | — |
| `set_allowlist_active` | `AllowlistActive` | `AllowlistActive` | — |
| `is_allowlist_active` | `AllowlistActive` | — | — |
| `set_investor_allowlisted` | `AllowlistIndex` | `InvestorAllowlisted(addr)`, `AllowlistIndex` (append) | — |
| `set_investors_allowlisted` | `AllowlistIndex` (per addr) | `InvestorAllowlisted(addr)`, `AllowlistIndex` (append per unique addr) | — |
| `is_investor_allowlisted` | `AllowlistActive`, `InvestorAllowlisted(addr)` | — | — |
| `get_allowlisted_investors`, `get_allowlisted_investors_count` | `AllowlistIndex` | — | — |
| `rebind_registry_ref` | `RegistryRef` | `RegistryRef` | `RegistryRef` (when `None`) |
| `clear_registry_ref` | `RegistryRef` | — | `RegistryRef` |
| `get_pending_admin`, `get_pending_admin_expiry`, `get_pending_admin_remaining_secs` | `PendingAdmin`, `PendingAdminExpiry` | — | — |
| `propose_admin` | `Escrow.admin`, `PendingAdmin`, `PendingAdminExpiry` | `PendingAdmin`, `PendingAdminExpiry` | — |
| `accept_admin` | `PendingAdmin`, `PendingAdminExpiry` | `Escrow.admin` | `PendingAdmin`, `PendingAdminExpiry` |
| `transfer_admin` (deprecated shim) | `Escrow` | (errors — single-step admin is removed; emits `DeprecatedTransferAdminUsed`) | — |
| `cancel_pending_admin` | `PendingAdmin` | — | `PendingAdmin`, `PendingAdminExpiry` |
| `record_sme_collateral_commitment` | `SmeCollateralPledge` | `SmeCollateralPledge` | — |
| `clear_sme_collateral_commitment` | `SmeCollateralPledge` | — | `SmeCollateralPledge` |
| `get_sme_collateral_commitment` | `SmeCollateralPledge` | — | — |
| `get_yield_tiers`, `preview_yield_tier` | `YieldTierTable`, `Escrow` | — | — |
| `bind_primary_attestation_hash` | `PrimaryAttestationHash` | `PrimaryAttestationHash` (single-set) | — |
| `append_attestation_digest` | `AttestationAppendLog` | `AttestationAppendLog` (append) | — |
| `revoke_attestation_digest` | `AttestationAppendLog`, `AttestationRevoked(index)` | `AttestationRevoked(index)` | — |
| `revoke_attestation_digests` | `AttestationAppendLog`, `AttestationRevoked(index)` per idx | `AttestationRevoked(index)` per idx | — |
| `unrevoke_attestation_digest` | `AttestationRevoked(index)` | `AttestationRevoked(index)` | — |
| `get_primary_attestation_hash`, `get_attestation_append_log`, `get_attestation_digest_at`, `is_attestation_revoked`, `get_revoked_attestation_digests` | `PrimaryAttestationHash`, `AttestationAppendLog`, `AttestationRevoked` | — | — |

## TTL ops

| Entrypoint | Reads | Writes / TTL | Deletes |
|------------|-------|--------------|---------|
| `bump_ttl` (line 4999) | instance keys gated: `Escrow`, `Version`, `LegalHold`, `AllowlistActive`, `FundingCloseSnapshot`; persistent keys per call args: each `InvestorAllowlisted`, optionally `InvestorContribution(addr)`, `InvestorEffectiveYield(addr)`, `InvestorClaimNotBefore(addr)`, `InvestorClaimed(addr)` | `instance().extend_ttl(...)` and `persistent().extend_ttl(...)` for the targeted keys (INV-33) | — |

## Composite reads

| Entrypoint | Reads |
|------------|-------|
| `get_escrow_summary` | `Escrow`, `LegalHold`, `FundingCloseSnapshot`, `UniqueFunderCount`, `AllowlistActive`, `Version`, `SmeCollateralPledge`, `PrimaryAttestationHash`, `AttestationAppendLog` |
| `get_reconciliation` (line 5586) | `Escrow`, `DistributedPrincipal`, `FundingCloseSnapshot`, `InvestorIndex`, plus per-investor `InvestorContribution(addr)` (iterated from `InvestorIndex`) |
| `get_investors` | `InvestorIndex` |
| `get_contribution` | `InvestorContribution(addr)` |
| `get_contributions` | `InvestorContribution(addr)` per addr |
| `get_investor_yield_bps` | `Escrow`, `InvestorEffectiveYield(addr)` |
| `get_investor_claim_not_before` | `InvestorClaimNotBefore(addr)` |
| `is_investor_claimed` | `InvestorClaimed(addr)` |

---

# Worked Example

This walk-through reflects the **actual** lifecycle of a real escrow, using the
implementation's typical path. Source references are line-numbered to `escrow/src/lib.rs`.

## 1. Initialize escrow

```text
contract.init(
    invoice_id: "INV_2024_001",
    admin:      ADMIN_ADDR,            // multisig or DAO
    sme_address:SME_ADDR,
    amount:     100_000_000_000,
    funding_target: 80_000_000_000,
    yield_bps:  800,
    maturity:   1_730_000_000,
    treasury:   TREASURY_ADDR,
    yield_tiers: None,
    min_contribution: Some(10_000),
    max_unique_investors: Some(100),
    max_per_investor: None,
    legal_hold_clear_delay: None,
    registry:           None,
    funding_deadline:   None,
    protocol_fee_bps:   Some(50),
    maturity_max_horizon_secs: None,
);
```

**Storage writes** (instance):

- `DataKey::Escrow` ← `InvoiceEscrow { invoice_id, admin, sme_address, amount, funding_target, funded_amount=0, yield_bps=800, maturity=…, status=0 }`
- `DataKey::Version` ← `6`
- `DataKey::FundingToken` ← funding token address
- `DataKey::Treasury` ← treasury address
- `DataKey::MinContributionFloor` ← `10_000`
- `DataKey::MaxUniqueInvestorsCap` ← `100`
- `DataKey::ProtocolFeeBps` ← `50`
- `DataKey::UniqueFunderCount` ← `0`
- `DataKey::AllowlistActive` ← `false`
- `DataKey::Paused` ← `false`
- `DataKey::LegalHold` ← `false`

After init, `Escrow.status == 0` (Open). No persistent keys exist yet.

## 2. Funding updates

Two investors fund:

```text
contract.fund(&INVESTOR_A, &30_000_000_000);   // 30 units
contract.fund(&INVESTOR_B, &55_000_000_000);   // 55 units — crosses funding_target = 80
```

For each call, `fund_impl` performs:

1. Guards: no legal hold (`LegalHold = false`), not paused (`Paused = false`),
   `status == 0`, amount ≥ floor (10 000), allowlist inactive.
2. `investor.require_auth()` (INV — `Address::require_auth`).
3. Reads investor's current contribution (absent ⇒ `0`).
4. **`funded_amount += amount`** (atomic via `Escrow` rewrite).
5. For **new investor** (INV-A first, then B):
   - `InvestorContribution(addr) := amount` (persistent; INV-12, INV-13).
   - `InvestorEffectiveYield(addr) := InvoiceEscrow.yield_bps` (persistent; first deposit).
   - `InvestorClaimNotBefore(addr) := 0` (persistent; no commitment lock).
   - `InvestorIndex.push(addr)` (instance).
   - `UniqueFunderCount += 1` (instance).
6. Persistent TTL extended for all four new keys (INV-34).
7. On B's call, since `funded_amount = 85_000_000_000 ≥ funding_target = 80_000_000_000`:
   - `Escrow.status` becomes `1` (funded).
   - `DataKey::FundingCloseSnapshot` is written **once** at the moment of crossing
     (`total_principal = 85_000_000_000`, including the over-funding margin) — INV-20.
8. Emits `EscrowFunded` per investor; the crossing event also flips `status`.

## 3. Settlement snapshot

Now that the escrow is funded, settlement may be attempted:

```text
contract.settle();
```

1. Reads `Escrow` (status `1`, funded), `LegalHold`, `Paused`, `maturity`, `FundingCloseSnapshot`.
2. Asserts `maturity == 0` OR `now ≥ maturity` (else `MaturityNotReached`, code 122).
3. Asserts no legal hold (else `LegalHoldBlocksSettlement`, code 120).
4. Asserts not paused (else `PausedBlocksSettlement`, code 211).
5. Writes `Escrow.status = 2` (settled).
6. Writes `DataKey::SettledAt` ← ledger timestamp (single-write; INV-21).
7. Emits `EscrowSettled`.

## 4. Cancel funding (alternative path)

If the SME or admin decides the raise should be winded down before reaching target (mutually
exclusive with `settle`/`withdraw`), the escrow transitions to `cancelled` first; only then
may investors reclaim principal via `refund`.

```text
contract.cancel_funding();   // admin-auth; status 0 -> 4; blocked while LegalHold
```

Writes:
- `Escrow.status = 4` (instance, single mutation).

## 5. Investor claims

Each investor calls `claim_investor_payout`:

```text
contract.claim_investor_payout(&INVESTOR_A);
contract.claim_investor_payout(&INVESTOR_B);
```

For each call:

1. Reads `Escrow` (status `2`), `InvestorClaimed(addr)` (false), `InvestorContribution(addr)`,
   `InvestorEffectiveYield(addr)`, `InvestorClaimNotBefore(addr)` (0),
   `FundingCloseSnapshot` (denominator `85_000_000_000`).
2. Computes pro-rata payout:
   - `coupon = total_principal × yield_bps / 10_000` (INV-23 analogue)
   - `settle_pool = total_principal + coupon`
   - `payout = contribution × settle_pool / total_principal`
3. Token transfer via `external_calls::transfer_funding_token_with_balance_checks`.
4. `InvestorClaimed(addr) := true` (persistent, single-write; INV-22).
5. Persistent TTL extension on `InvestorClaimed` (INV-34).
6. Subsequent call by the same investor short-circuits at step 2 — second transfer is never
   executed.

## 6. Storage cleanup

There is no bulk cleanup entrypoint. After settlement and full claim:

- `InvestorClaimed(A) = true`, `InvestorClaimed(B) = true` (persistent, retained).
- `InvestorContribution(A)`, `InvestorContribution(B)`, `InvestorEffectiveYield(A|B)`,
  `InvestorClaimNotBefore(A|B)` are all retained but no longer mutated.
- `Escrow.status = 2` (settled) with `funded_amount` preserved in the snapshot record, or
  `Escrow.status = 3` and `DistributedPrincipal = funded_amount` if the SME called
  `withdraw()` instead of `claim_investor_payout`.
- `SettledAt = <timestamp>` retained.
- `FundingCloseSnapshot` retained as the historical denominator.

Operators may call `sweep_terminal_dust(amount)` to move rounding residue to `Treasury` (only
in terminal statuses; INV-24 ensures the liability floor is preserved). The contract
**never** deletes `InvestorContribution` entries once written — TTL is the only terminal
mechanism, and `bump_ttl` can reactivate expired entries.

---

# Audit Notes

This section describes why the storage layout is safe, the assumptions auditors should verify,
and the explicit out-of-scope behaviors. Every assertion traces back to the code.

## Why the layout is safe

- **Schema version discipline.** `DataKey::Version` is set exactly once at init to
  `SCHEMA_VERSION = 6`. `migrate` is intentionally gated by typed errors so no silent
  rewrites are performed; additive growth follows [`ADR-007`](adr/ADR-007-storage-key-evolution.md).
- **No silent mutations.** Every state transition is preceded by `require_auth` (auth) and
  guarded by `ensure` (typed errors). Reads that are not gated by auth are explicitly
  read-only (e.g. `is_settleable`, `get_escrow_summary`).
- **Single-write keys are single-write.** `FundingCloseSnapshot`, `SettledAt`,
  `InvestorClaimed`, `InvestorEffectiveYield`, `InvestorClaimNotBefore`, `PrimaryAttestationHash`
  are physically prevented from being written twice by explicit `!has(...)` checks before
  the write — INV-18, INV-20, INV-21, INV-22, INV-25.
- **Cap arithmetic is overflow-safe.** `funded_amount` and per-investor contribution use
  `checked_add`/`checked_sub`; `MAX_INVOICE_AMOUNT` is set to `2⁶³ − 1` so the
  pro-rata triple (coupon, settle_pool, gross_payout) is overflow-free even at the
  worst-case tier values (see `MAX_INVOICE_AMOUNT` derivation in `escrow/src/lib.rs`).
- **Liability floor preserved.** `sweep_terminal_dust` checks
  `balance − sweep_amt ≥ funded_amount − DistributedPrincipal` so refunds prior to a
  dust sweep cannot be reaped (INV-24).
- **TTL only extends, never shortens.** Permissionless `bump_ttl` is monotonic and cannot
  harm contract state (INV-33).

## Assumptions auditors should verify

| Assumption | Where to verify | Out-of-scope case |
|------------|-----------------|--------------------|
| Funding token is standard SEP-41 (no fee-on-transfer, no rebasing, no transfer hooks) | `escrow/src/external_calls.rs` `transfer_funding_token_with_balance_checks` wrapper | Fee-on-transfer / rebasing tokens cause safe panic via `SenderBalanceDeltaMismatch` (code 40) or `RecipientBalanceDeltaMismatch` (code 41). |
| `admin` is a multisig or governed DAO | Module-level rustdoc in `escrow/src/lib.rs` | A single-key admin can strand funds during a legal hold. |
| Distinction between `Paused` (operational) and `LegalHold` (compliance) is preserved by integrators | [`DataKey::Paused`](../escrow/src/lib.rs) variant documentation | Treating them as interchangeable would conflate incident response with compliance gating. |
| Investor addresses are not enumerable from storage | ADR-007 Rule 5; `migrate` returns `NoMigrationPath` for `from_version != 6` reset paths | Storage foot-print can only grow; aggregator contracts must read per-address via `get_contributions(...)` or `get_investors(...)` pagination. |
| `RegistryRef` is hint-only — read the registry yourself before trusting it | Module-level rustdoc in `escrow/src/lib.rs` "Funding token and registry (immutable hints)" | Using it as authority on-chain is unsafe. |
| `SmeCollateralCommitment` is metadata only — not custody, lien, or encumbrance | Module-level rustdoc in `escrow/src/lib.rs` "SME collateral commitment metadata" | On-chain enforcement would require a new typed record + entrypoints. |
| `FundingCloseSnapshot.total_principal` is the immutable pro-rata denominator | INV-20 single-write guard at transition `0 → 1` | Re-running the snapshot writer — disabled by invariant; the migration path is not enumerably safe. |
| Tiered yield fairness: the ladder is monotonic, and selection is locked at first deposit | INV-18 enforcement of `fund_with_commitment`; `YieldTier` validation at init | Arbitrary re-selection would violate invariant fairness guarantee. |
| Token conservation: every fund transfer pulls tokens **into** escrow; every refund/withdraw/claim pushes tokens **out** via the SEP-41 balance-checked wrapper | `escrow/src/external_calls.rs` | Out-of-scope token behaviors cause safe panic in the wrapper. |

## Operations guide summary

- **Bootstrapping**: deploy, then call `init` exactly once.
- **Tiered yield, protocol fee, deadline**: configure at init; only lowering is allowed post-init
  (`lower_*`) and only while `status == 0`.
- **Settlement**: requires `status == 1`, no legal hold, no pause, maturity gate cleared.
- **Migration**: redeploy for any storage-shape change. Additive `DataKey` variants do not
  require migration (see ADR-007).
- **TTL hygiene**: integrate `bump_ttl` into off-chain monitoring to extend lease on
  long-running escrows. The call is permissionless and cannot harm state.

## Out-of-scope (by design)

- No use of `temp_storage` (`env.storage().temporary()`).
- No in-place reordering of `DataKey` variants.
- No bulk investor enumeration from on-chain storage; aggregators use paginated reads
  (`get_investors`, `get_contributions`).
- No schema rewriting under `migrate`; `migrate` is a typed no-op for the supported shape.

---

# Cross-reference index

- [`DataKey` enum](../escrow/src/lib.rs) — line ~755 onward.
- [`InvoiceEscrow`](../escrow/src/lib.rs) — line ~1100 onward.
- [`FundingCloseSnapshot`](../escrow/src/lib.rs) — line ~1180 onward.
- [`SmeCollateralCommitment`](../escrow/src/lib.rs) — line ~1140 onward.
- [`YieldTier`](../escrow/src/lib.rs) — line ~1165 onward.
- `SCHEMA_VERSION`, `MAX_*` bounds, `*_TTL_MIN_EXTENSION_LEDGERS` — lines ~99–225.
- Authorization order — module-level rustdoc and [`ADR-002`](adr/ADR-002-auth-boundaries.md).
- Additive-key policy — [`ADR-007`](adr/ADR-007-storage-key-evolution.md).
- Storage-growth/TTL reasoning — [`docs/escrow-gas-storage-notes.md`](escrow-gas-storage-notes.md).
- State machine — [`docs/escrow-lifecycle.md`](escrow-lifecycle.md) and
  [`ADR-001`](adr/ADR-001-state-model.md).
- Data shape reference — [`docs/escrow-data-model.md`](escrow-data-model.md).
- Schema versioning policy — [`docs/escrow-schema-versioning.md`](escrow-schema-versioning.md).
- Token transfer safety — [`docs/escrow-host-model.md`](escrow-host-model.md) and
  [`escrow/src/external_calls.rs`](../escrow/src/external_calls.rs).
