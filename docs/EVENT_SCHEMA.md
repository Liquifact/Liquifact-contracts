# LiquiFact Escrow Event Schema Reference

Authoritative event reference for indexers, analytics jobs, read API
projections, and audit tooling that consume events emitted by
`escrow/src/lib.rs`.

Related docs:

- `docs/escrow-events.md` provides the shorter consumer-facing overview.
- `docs/escrow-indexer.md` describes subscription, cursoring, and
  reconciliation.
- `docs/openapi.yaml` does not expose raw Soroban events; decoded events should
  feed the API projections for escrow status, funding, settlement, claims,
  holds, attestations, allowlist state, and refunds.

## Soroban Layout

Each event is defined with `#[contractevent]`. Per the
[Soroban SDK `contractevent` model](https://docs.rs/soroban-sdk/latest/soroban_sdk/attr.contractevent.html),
the emitted topic list contains:

1. A fixed topic generated from the Rust event struct name in snake case.
2. Every field marked with `#[topic]`, in struct field order.

Fields not marked with `#[topic]` are encoded in the event data payload. The
default `#[contractevent]` data format is a map keyed by field name. Indexers
should treat field order in this document as the canonical struct order from
`escrow/src/lib.rs`.

The `name` field is a `#[topic] Symbol` in every LiquiFact event. It carries the
short routing symbol passed with `symbol_short!(...)`, such as `funded` or
`escrow_sd`.

## Event Catalog

The current contract defines 19 event structs.

| Rust event | `name` symbol | Entrypoint(s) |
|---|---:|---|
| `EscrowInitialized` | `escrow_ii` | `init` |
| `MaxUniqueInvestorsCapLowered` | `inv_cap` | `lower_max_unique_investors` |
| `EscrowFunded` | `funded` | `fund`, `fund_with_commitment` |
| `EscrowSettled` | `escrow_sd` | `settle` |
| `MaturityUpdatedEvent` | `maturity` | `update_maturity` |
| `AdminTransferredEvent` | `admin` | `accept_admin` |
| `AdminProposedEvent` | `adm_prop` | `propose_admin`, `transfer_admin` |
| `FundingTargetUpdated` | `fund_tgt` | `update_funding_target` |
| `LegalHoldChanged` | `legalhld` | `set_legal_hold`, `clear_legal_hold` |
| `CollateralRecordedEvt` | `coll_rec` | `record_sme_collateral_commitment` |
| `SmeWithdrew` | `sme_wd` | `withdraw` |
| `InvestorPayoutClaimed` | `inv_claim` | `claim_investor_payout` |
| `FundingCancelled` | `fund_can` | `cancel_funding` |
| `InvestorRefundedEvt` | `refunded` | `refund` |
| `TreasuryDustSwept` | `dust_sw` | `sweep_terminal_dust` |
| `PrimaryAttestationBound` | `att_bind` | `bind_primary_attestation_hash` |
| `AttestationDigestAppended` | `att_app` | `append_attestation_digest` |
| `AllowlistEnabledChanged` | `al_ena` | `set_allowlist_active` |
| `InvestorAllowlistChanged` | `al_set` | `set_investor_allowlisted`, `set_investors_allowlisted` |

## Complete Topic And Data Layout

### `EscrowInitialized`

Emitted after successful `init`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `escrow_initialized` |
| 1 | `name` | `Symbol` | `escrow_ii` |

Data:

| Field | Type |
|---|---|
| `escrow` | `InvoiceEscrow` |
| `funding_token` | `Address` |
| `treasury` | `Address` |
| `registry` | `Option<Address>` |
| `has_maturity_lock` | `bool` |

### `MaxUniqueInvestorsCapLowered`

Emitted after successful `lower_max_unique_investors`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `max_unique_investors_cap_lowered` |
| 1 | `name` | `Symbol` | `inv_cap` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `old_cap` | `u32` |
| `new_cap` | `u32` |

### `EscrowFunded`

Emitted after successful `fund` or `fund_with_commitment`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `escrow_funded` |
| 1 | `name` | `Symbol` | `funded` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |
| 3 | `investor` | `Address` | Funding investor |

Data:

| Field | Type |
|---|---|
| `amount` | `i128` |
| `funded_amount` | `i128` |
| `status` | `u32` |
| `investor_effective_yield_bps` | `i64` |

### `EscrowSettled`

Emitted after successful `settle`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `escrow_settled` |
| 1 | `name` | `Symbol` | `escrow_sd` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `funded_amount` | `i128` |
| `yield_bps` | `i64` |
| `maturity` | `u64` |

### `MaturityUpdatedEvent`

Emitted after successful `update_maturity`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `maturity_updated_event` |
| 1 | `name` | `Symbol` | `maturity` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `old_maturity` | `u64` |
| `new_maturity` | `u64` |

### `AdminTransferredEvent`

Emitted after successful `accept_admin`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `admin_transferred_event` |
| 1 | `name` | `Symbol` | `admin` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `new_admin` | `Address` |

### `AdminProposedEvent`

Emitted after successful `propose_admin`. The deprecated `transfer_admin`
shim delegates to `propose_admin`, so it emits this event rather than
`AdminTransferredEvent`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `admin_proposed_event` |
| 1 | `name` | `Symbol` | `adm_prop` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `current_admin` | `Address` |
| `pending_admin` | `Address` |

### `FundingTargetUpdated`

Emitted after successful `update_funding_target`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `funding_target_updated` |
| 1 | `name` | `Symbol` | `fund_tgt` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `old_target` | `i128` |
| `new_target` | `i128` |

### `LegalHoldChanged`

Emitted after successful `set_legal_hold`; `clear_legal_hold` calls
`set_legal_hold(false)`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `legal_hold_changed` |
| 1 | `name` | `Symbol` | `legalhld` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type | Values |
|---|---|---|
| `active` | `u32` | `1` = enabled, `0` = cleared |

### `CollateralRecordedEvt`

Emitted after successful `record_sme_collateral_commitment`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `collateral_recorded_evt` |
| 1 | `name` | `Symbol` | `coll_rec` |

Data:

| Field | Type |
|---|---|
| `invoice_id` | `Symbol` |
| `amount` | `i128` |
| `prior_amount` | `i128` |

Note: this event records SME-reported collateral metadata only. It is not proof
of custody, token movement, lien, or enforceable on-chain collateral.

### `SmeWithdrew`

Emitted after successful `withdraw`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `sme_withdrew` |
| 1 | `name` | `Symbol` | `sme_wd` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `amount` | `i128` |

### `InvestorPayoutClaimed`

Emitted after the first successful `claim_investor_payout` for an investor.
Repeated claims by the same investor are idempotent no-ops and do not re-emit.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `investor_payout_claimed` |
| 1 | `name` | `Symbol` | `inv_claim` |
| 2 | `investor` | `Address` | Claiming investor |
| 3 | `invoice_id` | `Symbol` | Escrow invoice id |

Data: empty map; this struct has no non-topic fields.

### `FundingCancelled`

Emitted after successful `cancel_funding`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `funding_cancelled` |
| 1 | `name` | `Symbol` | `fund_can` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `funded_amount` | `i128` |

### `InvestorRefundedEvt`

Emitted after successful `refund`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `investor_refunded_evt` |
| 1 | `name` | `Symbol` | `refunded` |
| 2 | `investor` | `Address` | Refunded investor |
| 3 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `amount` | `i128` |

### `TreasuryDustSwept`

Emitted after successful `sweep_terminal_dust`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `treasury_dust_swept` |
| 1 | `name` | `Symbol` | `dust_sw` |

Data:

| Field | Type |
|---|---|
| `invoice_id` | `Symbol` |
| `token` | `Address` |
| `amount` | `i128` |

### `PrimaryAttestationBound`

Emitted after successful `bind_primary_attestation_hash`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `primary_attestation_bound` |
| 1 | `name` | `Symbol` | `att_bind` |

Data:

| Field | Type |
|---|---|
| `invoice_id` | `Symbol` |
| `digest` | `BytesN<32>` |

### `AttestationDigestAppended`

Emitted after successful `append_attestation_digest`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `attestation_digest_appended` |
| 1 | `name` | `Symbol` | `att_app` |

Data:

| Field | Type |
|---|---|
| `invoice_id` | `Symbol` |
| `index` | `u32` |
| `digest` | `BytesN<32>` |

### `AllowlistEnabledChanged`

Emitted after successful `set_allowlist_active`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `allowlist_enabled_changed` |
| 1 | `name` | `Symbol` | `al_ena` |

Data:

| Field | Type | Values |
|---|---|---|
| `invoice_id` | `Symbol` | Escrow invoice id |
| `active` | `u32` | `1` = enabled, `0` = disabled |

### `InvestorAllowlistChanged`

Emitted after successful `set_investor_allowlisted`. The batch entrypoint
`set_investors_allowlisted` emits one event per investor in input order.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `investor_allowlist_changed` |
| 1 | `name` | `Symbol` | `al_set` |

Data:

| Field | Type | Values |
|---|---|---|
| `invoice_id` | `Symbol` | Escrow invoice id |
| `investor` | `Address` | Updated investor |
| `allowed` | `u32` | `1` = allowed, `0` = blocked |

## Nested Types

### `InvoiceEscrow`

Used in `EscrowInitialized.escrow`.

| Field | Type |
|---|---|
| `invoice_id` | `Symbol` |
| `admin` | `Address` |
| `sme_address` | `Address` |
| `amount` | `i128` |
| `funding_target` | `i128` |
| `funded_amount` | `i128` |
| `yield_bps` | `i64` |
| `maturity` | `u64` |
| `status` | `u32` |

Status values:

| Value | Meaning |
|---:|---|
| 0 | Open |
| 1 | Funded |
| 2 | Settled |
| 3 | Withdrawn |
| 4 | Cancelled |

## Indexer Notes

- Prefer filtering by `contractId` plus `topic[1] == name` when routing by the
  LiquiFact short event symbol. `topic[0]` is the generated Rust event-struct
  symbol.
- Use `(ledger, txHash, eventIndex)` as the idempotency cursor, as described in
  `docs/escrow-indexer.md`.
- Some useful correlation fields are data fields rather than topics:
  `CollateralRecordedEvt.invoice_id`, `TreasuryDustSwept.invoice_id`,
  `PrimaryAttestationBound.invoice_id`, `AttestationDigestAppended.invoice_id`,
  `AllowlistEnabledChanged.invoice_id`, and
  `InvestorAllowlistChanged.invoice_id`.
- Do not treat collateral or attestation events as proof of off-chain custody,
  KYC status, or legal enforceability. They are metadata/audit records emitted
  after the corresponding authenticated write succeeds.

## Security And State Invariants

- Events are emitted only after the entrypoint guard checks and storage writes
  in the successful path.
- Unauthorized calls, invalid zero/negative amounts, overflow paths,
  double-spend paths, legal-hold blocks, and invalid state-machine transitions
  fail before event emission.
- Investor claim and refund events are deduplicated by persistent markers or
  contribution zeroing before emission.
- Event emission is O(1) for all entrypoints except
  `set_investors_allowlisted`, which emits O(n) `InvestorAllowlistChanged`
  events for `n <= MAX_INVESTOR_ALLOWLIST_BATCH`.

## Changelog

| Date | Version | Change |
|---|---|---|
| 2026-03-23 | v0.1 | Initial event schema reference |
| 2026-05-27 | v0.2 | Added initialization references and investor-cap event notes |
| 2026-05-31 | v0.3 | Issue #272: replaced drifted reference with complete `#[contractevent]` topic and data layout from `escrow/src/lib.rs` |
