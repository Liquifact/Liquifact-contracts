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

The current contract defines 37 event structs.

| Rust event | `name` symbol | Entrypoint(s) |
|---|---:|---|
| `AdminAcceptedEvent` | `adm_acc` | `accept_admin` |
| `AdminProposalCancelled` | `adm_can` | `cancel_pending_admin` |
| `AdminProposedEvent` | `adm_prop` | `propose_admin`, `transfer_admin` |
| `AdminTransferredEvent` | `admin` | (never emitted; placeholder for `accept_admin` doc) |
| `AllowlistEnabledChanged` | `al_ena` | `set_allowlist_active` |
| `AttestationDigestAppended` | `att_app` | `append_attestation_digest` |
| `AttestationDigestRevoked` | `att_rev` | `revoke_attestation_digest` |
| `AttestationDigestUnrevoked` | `att_unrev` | `unrevoke_attestation_digest` |
| `BeneficiaryRotated` | `ben_rot` | `rotate_beneficiary` |
| `CollateralClearedEvt` | — | `clear_sme_collateral_commitment` |
| `CollateralRecordedEvt` | `coll_rec` | `record_sme_collateral_commitment` |
| `ContractUpgraded` | `upgrade` | `upgrade` |
| `DeprecatedTransferAdminUsed` | `depr_xfer` | `transfer_admin` |
| `EscrowFunded` | `funded` | `fund`, `fund_with_commitment` |
| `EscrowInitialized` | `escrow_ii` | `init` |
| `EscrowPartialSettle` | `part_set` | `partial_settle` |
| `EscrowSettled` | `escrow_sd` | `settle` |
| `FundingCancelled` | `fund_can` | `cancel_funding` |
| `FundingTargetUpdated` | `fund_tgt` | `update_funding_target` |
| `FundingDeadlineExtended` | `fund_ext` | `extend_funding_deadline` |
| `InvestorAllowlistChanged` | `al_set` | `set_investor_allowlisted`, `set_investors_allowlisted` |
| `InvestorAllowlistBatchApplied` | `al_batch` | `set_investors_allowlisted` |
| `InvestorPayoutClaimed` | `inv_claim` | `claim_investor_payout` |
| `InvestorRefundedEvt` | `refunded` | `refund` |
| `LegalHoldChanged` | `legalhld` | `set_legal_hold`, `clear_legal_hold` |
| `LegalHoldClearCancelled` | `lh_cancel` | `cancel_clear_legal_hold` |
| `LegalHoldClearDelayUpdated` | — | (dead code; never emitted) |
| `LegalHoldClearRequested` | `lh_req` | `request_clear_legal_hold` |
| `MaturityMaxHorizonUpdated` | `mtry_max` | `update_maturity_max_horizon` |
| `MaturityUpdatedEvent` | `maturity` | `update_maturity` |
| `MaxPerInvestorCapRaised` | `inv_cap` | `raise_max_per_investor` |
| `MaxUniqueInvestorsCapLowered` | `inv_cap` | `lower_max_unique_investors` |
| `MaxUniqueInvestorsCapRaised` | `raise_cap` | `raise_max_unique_investors` |
| `MinContributionFloorLowered` | `floor_lo` | `lower_min_contribution_floor` |
| `PrimaryAttestationBound` | `att_bind` | `bind_primary_attestation_hash` |
| `ProtocolFeeWithdrawn` | `prot_fee` | `withdraw` |
| `RegistryRefRebound` | `reg_rebind` | `set_registry` |
| `SmeWithdrew` | `sme_wd` | `withdraw` |
| `TreasuryDustSwept` | `dust_sw` | `sweep_terminal_dust` |

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
|---|---|---|
| `funded_amount` | `i128` |
| `yield_bps` | `i64` |
| `maturity` | `u64` |
| `settled_at_ledger_timestamp` | `u64` |
| `settle_pool` | `i128` |

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

Defined but not currently emitted. Use `AdminAcceptedEvent` (`adm_acc`) for
incoming admin acceptances. This struct is retained for schema compatibility
and may be removed in a future cleanup.

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
shim delegates to `propose_admin`, so it also emits this event. When
issued from the deprecated shim, this event is paired with a follow-up
[`DeprecatedTransferAdminUsed`](#deprecatedtransferadminused) event in
the same transaction so indexers can distinguish legacy one-step callers
from those using the canonical two-step flow.

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

### `DeprecatedTransferAdminUsed`

Emitted by the deprecated one-step `transfer_admin` shim. Pairs with
`AdminProposedEvent` in the same transaction so indexers can flag legacy callers.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `deprecated_transfer_admin_used` |
| 1 | `name` | `Symbol` | `depr_xfer` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `proposed_address` | `Address` |

### `BeneficiaryRotated`

Emitted after successful `rotate_beneficiary`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `beneficiary_rotated` |
| 1 | `name` | `Symbol` | `ben_rot` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `prior_sme` | `Address` |
| `new_sme` | `Address` |

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
| `recipient` | `Address` |
| `fee` | `i128` |

### `ProtocolFeeWithdrawn`

Emitted after successful `withdraw` only when the computed protocol fee is
non-zero. This records the treasury leg separately from the SME net withdrawal
event so accounting integrations do not need to re-derive the protocol cut.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `protocol_fee_withdrawn` |
| 1 | `name` | `Symbol` | `prot_fee` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `fee_amount` | `i128` |
| `fee_bps` | `i64` |
| `treasury` | `Address` |
| `sme_net_amount` | `i128` |

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

### `FundingDeadlineExtended`

Emitted after successful `extend_funding_deadline`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `funding_deadline_extended` |
| 1 | `name` | `Symbol` | `fund_ext` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `old_deadline` | `u64` |
| `new_deadline` | `u64` |

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

### `InvestorAllowlistBatchApplied`

Emitted once per `set_investors_allowlisted` call, after all per-investor
`InvestorAllowlistChanged` (`al_set`) events have been emitted. Allows indexers
to identify completed batch operations without counting individual `al_set`
events. Supplements — does not replace — the per-investor events.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `investor_allowlist_batch_applied` |
| 1 | `name` | `Symbol` | `al_batch` |

Data:

| Field | Type | Values |
|---|---|---|
| `invoice_id` | `Symbol` | Escrow invoice id |
| `batch_size` | `u32` | Number of investors processed (`1`–`MAX_INVESTOR_ALLOWLIST_BATCH`) |
| `allowed` | `u32` | `1` = allowed, `0` = blocked |

**Indexer guidance:** filter on `topic[1] == "al_batch"` to detect batch
operations. `batch_size` equals the count of `al_set` events emitted in the
same transaction. Existing indexers that only consume `al_set` remain fully
compatible — `al_batch` is purely additive.

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
- For admin handover routing, treat `AdminProposedEvent` as the **canonical
  two-step** signal (`propose_admin`). When an `AdminProposedEvent` is
  immediately followed by a `DeprecatedTransferAdminUsed` in the same
  transaction, the proposal originated from the legacy one-step `transfer_admin`
  shim. Operators driving the deprecation should count occurrences of
  `DeprecatedTransferAdminUsed` per `(contractId, invoice_id)` and notify
  remaining callers until the count is zero for a full release window before
  the shim entrypoint is removed.

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
  events for `n <= MAX_INVESTOR_ALLOWLIST_BATCH` followed by exactly one
  `InvestorAllowlistBatchApplied` event.

## Changelog

| Date | Version | Change |
|---|---|---|
| 2026-03-23 | v0.1 | Initial event schema reference |
| 2026-05-27 | v0.2 | Added initialization references and investor-cap event notes |
| 2026-05-31 | v0.3 | Issue #272: replaced drifted reference with complete `#[contractevent]` topic and data layout from `escrow/src/lib.rs` |
| 2026-06-24 | v0.4 | Added `settled_at_ledger_timestamp` field to `EscrowSettled` event; added `is_settleable` view |
| 2026-06-26 | v0.5 | Issue #379: Added `InvestorAllowlistBatchApplied` (`al_batch`) event emitted once per `set_investors_allowlisted` call for indexer disambiguation |
| 2026-06-29 | v0.6 | Added `DeprecatedTransferAdminUsed` (`depr_xfer`) for deprecated one-step admin transfer shim; renamed `EscrowInitialized.name` from `escrow` to `escrow_ii` to fix upstream collision; fixed duplicate `BeneficiaryRotated` struct; added missing event struct sections (`AdminAcceptedEvent`, `AdminProposalCancelled`, `CollateralClearedEvt`, `ContractUpgraded`, `EscrowPartialSettle`, `LegalHoldClearCancelled`, `LegalHoldClearRequested`, `MaturityMaxHorizonUpdated`, `MaxPerInvestorCapRaised`, `MaxUniqueInvestorsCapRaised`, `MinContributionFloorLowered`, `RegistryRefRebound`) |
