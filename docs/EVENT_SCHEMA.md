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

Every event in this contract emits a trailing schema version topic. It is a
`#[topic] Symbol` named `version` with value `v1`. It is always the final topic
in the topic list, after all other `#[topic]` fields. Indexers MUST ignore this
extra topic when reading from a known schema version and SHOULD reject events
whose version is not `v1` when strict compatibility is required.

The trailing version topic is a **compatibility contract**: it is additive and
must never be reordered, renamed, or removed. Consumers that pin to `v1` rely on
the exact topic ordering and data field names documented below. Any future
schema change MUST introduce a new version value (for example `v2`) rather than
mutating the `v1` layout in place.

## Event Catalog

The current contract defines the event structs listed below.

| Rust event | `name` symbol | Entrypoint(s) |
|---|---:|---|
| `EscrowInitialized` | `escrow_ii` | `init` |
| `MaxUniqueInvestorsCapLowered` | `inv_cap` | `lower_max_unique_investors` |
| `EscrowFunded` | `funded` | `fund`, `fund_with_commitment` |
| `FundingStateChanged` | `fund_st_ch` | `fund`, `fund_with_commitment`, `fund_with_reference`, `fund_batch`, `update_funding_target`, `partial_settle`, `cancel_funding` |
| `EscrowSettled` | `escrow_sd` | `settle` |
| `EscrowPartialSettle` | `part_set` | `partial_settle` |
| `EscrowRepaid` | `repaid` | `repay` |
| `SettlementFunded` | `set_fund` | `settle` |
| `SettleBatchProcessed` | `stl_batch` | `settle_batch` |
| `PartialCloseMinBpsUpdated` | `part_bps` | `set_partial_close_min_bps` |
| `MaturityUpdatedEvent` | `maturity` | `update_maturity` |
| `AdminTransferredEvent` | `admin` | `accept_admin` |
| `AdminProposedEvent` | `adm_prop` | `propose_admin`, `transfer_admin` |
| `BeneficiaryRotated` | `ben_rot` | `rotate_beneficiary` |
| `FundingTargetUpdated` | `fund_tgt` | `update_funding_target` |
| `LegalHoldChanged` | `legalhld` | `set_legal_hold`, `clear_legal_hold` |
| `CollateralRecordedEvt` | `coll_rec` | `record_sme_collateral_commitment` |
| `SmeWithdrew` | `sme_wd` | `withdraw` |
| `InvestorPayoutClaimed` | `inv_claim` | `claim_investor_payout` |
| `FundingCancelled` | `fund_can` | `cancel_funding`, `expire_funding` |
| `FundingExpired` | `fund_exp` | `expire_funding` |
| `InvestorRefundedEvt` | `refunded` | `refund` |
| `TreasuryDustSwept` | `dust_sw` | `sweep_terminal_dust` |
| `PrimaryAttestationBound` | `att_bind` | `bind_primary_attestation_hash` |
| `AttestationDigestAppended` | `att_app` | `append_attestation_digest` |
| `AttestationDigestRevoked` | `att_rev` | `revoke_attestation_digest`, `revoke_attestation_digests` |
| `AttestationDigestUnrevoked` | `att_unrev` | `unrevoke_attestation_digest` |
| `AllowlistEnabledChanged` | `al_ena` | `set_allowlist_active` |
| `InvestorAllowlistChanged` | `al_set` | `set_investor_allowlisted`, `set_investors_allowlisted` |
| `AdminProposalSuperseded` | `adm_sup` | `propose_admin`, `transfer_admin` |
| `AdminProposalCancelled` | `adm_can` | `cancel_pending_admin` |
| `AdminRecoveredEvent` | `adm_rec` | `recover_admin` |
| `MaxUniqueInvestorsCapRaised` | `raise_cap` | `raise_max_unique_investors` |
| `MinContributionFloorLowered` | `floor_lo` | `lower_min_contribution_floor` |
| `MaxPerInvestorCapRaised` | `inv_cap` | `raise_max_per_investor` |
| `FundingDeadlineExtended` | `fund_ext` | `extend_funding_deadline` |
| `PayoutBatchProcessed` | `pay_batch` | `push_investor_payouts` |
| `RefundBatchProcessed` | `refund_bt` | `refund_batch` |
| `EscrowUnfunded` | `unfunded` | `unfund` |
| `InvestorFreezeChanged` | `inv_frz` | `set_investor_frozen` |
| `ContributionReferenced` | `fund_ref` | `fund_with_reference` |
| `UpgradeProposed` | `upg_prop` | `propose_upgrade`, `upgrade` (deprecated) |
| `UpgradeCancelled` | `upg_cncl` | `cancel_upgrade` |
| `ContractUpgraded` | `upgrade` | `execute_upgrade` |
| `DeprecatedUpgradeUsed` | `depr_upg` | `upgrade` (deprecated) |
| `DeprecatedTransferAdminUsed` | `depr_xfer` | `transfer_admin` (deprecated) |
| `PausedChanged` | `paused` | `set_paused`, `pause_as_pauser` |
| `PauseMaxDurationUpdated` | `pausemax` | `set_pause_max_duration` |
| `PauseRateLimitUpdated` | `pause_rl` | `set_pause_rate_limit` |
| `PauseCooldownUpdated` | `pausecd` | `set_pause_cooldown` |
| `PauserChanged` | `pauser` | `set_pauser` |
| `FeeScheduleSubmitted` | `fee_sub` | `submit_fee_schedule` |
| `FeeScheduleActivated` | `fee_act` | `activate_fee_schedule`, `submit_fee_schedule` (lazy activation) |
| `FeeScheduleCancelled` | `fee_can` | `cancel_pending_fee_schedule` |
| `ProtocolFeeUpdated` | `fee_upd` | `set_protocol_fee_bps` |
| `PerformanceFeeUpdated` | `perf_upd` | `set_performance_fee_bps` |
| `PerformanceFeeCollected` | `perf_fee` | `claim_investor_payout`, `push_investor_payouts` |

The `name` symbol for each event is part of the compatibility contract. Symbols
are stable identifiers: they MUST NOT be reused for a different event and MUST
NOT be changed without a versioned migration plan. Indexers route on
`topic[1] == name`, so symbol drift is a breaking change.

## Complete Topic And Data Layout

All topic tables below omit the trailing schema version topic for brevity.
Every event's complete topic list is the table shown plus a final row of the
form `| <last_index+1> | version | Symbol | v1 |`. The value of `<last_index+1>`
is one greater than the largest index shown in the table. The version topic is
additive: it does not change the order, meaning, or data payload of any
pre-existing topic or field.

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

### `FundingStateChanged`

Emitted whenever a call actually changes the escrow funding `status`.

It is emitted from one shared helper (`emit_funding_state_changed`) so every entrypoint that
can move the status reports the transition the same way. Indexers should subscribe to this
event instead of reconstructing the transition from `EscrowFunded` / `FundingCancelled`
payloads.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | `name` | `Symbol` | `fund_st_ch` |
| 1 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type | Notes |
|---|---|---|
| `old_status` | `u32` | Status before the call. Currently `0` (open) on every transition. |
| `new_status` | `u32` | Status after the call: `1` (funded) or `4` (cancelled). |

Transitions and emitters:

| Transition | Entrypoint(s) |
|---|---|
| `0 → 1` (open → funded) | `fund`, `fund_with_commitment`, `fund_with_reference`, `fund_batch` (once per batch, not per entry), `update_funding_target` (when lowering the target promotes the escrow), `partial_settle` |
| `0 → 4` (open → cancelled) | `cancel_funding` |

**Emission guarantee:** at most one `FundingStateChanged` event per call, and exactly one per
status change. Calls that leave the status unchanged emit nothing — deposits below the target,
`unfund`, a target raise that keeps the escrow open, and repeated funding of an already-funded
escrow. `fund_with_reference` inherits the event through `fund_impl`. Status never returns to
`0` once it leaves it (funding cannot be reopened), so the `0 → 1` / `0 → 4` edges occur once
per escrow instance.

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

### `EscrowPartialSettle`

Emitted after successful `partial_settle`, which closes funding early for an
under-funded invoice. `funded_amount` is the escrow's funded amount at the time
funding is closed.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `escrow_partial_settle` |
| 1 | `name` | `Symbol` | `part_set` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `funded_amount` | `i128` |

### `EscrowRepaid`

Emitted on every successful `repay` call while the escrow is in withdrawn
(status `3`) state. `repaid_total` and `outstanding` are the cumulative values
after this instalment. When the pool becomes fully repaid, `repay` also emits
`EscrowSettled` (before this event).

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `escrow_repaid` |
| 1 | `name` | `Symbol` | `repaid` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type | Notes |
|---|---|---|
| `payer` | `Address` | Address that funded this instalment |
| `amount` | `i128` | Amount transferred in this call |
| `repaid_total` | `i128` | Cumulative amount repaid after this call |
| `outstanding` | `i128` | Amount still required before the escrow settles |

### `SettlementFunded`

Emitted by `settle` only when the SME must transfer a coupon shortfall into the
escrow before claims open. It is published before `EscrowSettled`; calls with no
shortfall emit only `EscrowSettled`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `settlement_funded` |
| 1 | `name` | `Symbol` | `set_fund` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type | Notes |
|---|---|---|
| `payer` | `Address` | SME address that funded the shortfall |
| `amount` | `i128` | Shortfall transferred in this settlement |
| `settle_pool` | `i128` | Full settlement pool required before claims open |

### `SettleBatchProcessed`

Emitted once at the end of every successful `settle_batch` call, after all
entries have been processed. `ok_count` and `fail_count` summarize the batch;
each successful entry also emits its own settlement events from its target
escrow. This event has no `invoice_id`, so indexers should correlate it with the
batch call rather than a single escrow.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `settle_batch_processed` |
| 1 | `name` | `Symbol` | `stl_batch` |

Data:

| Field | Type |
|---|---|
| `ok_count` | `u32` |
| `fail_count` | `u32` |

### `PartialCloseMinBpsUpdated`

Emitted after successful `set_partial_close_min_bps`, the admin setter for the
minimum funded basis-points threshold required before `partial_settle` may close
funding early. Accepted `bps` values are `0..=10_000`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `partial_close_min_bps_updated` |
| 1 | `name` | `Symbol` | `part_bps` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `old_bps` | `u32` |
| `new_bps` | `u32` |

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

`expire_funding` emits this event too (same schema), immediately before `FundingExpired`.

### `FundingExpired`

Emitted after successful `expire_funding`, right after the `FundingCancelled` event for the same transition.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `funding_expired` |
| 1 | `name` | `Symbol` | `fund_exp` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `funded_amount` | `i128` |
| `deadline` | `u64` (the funding deadline that was exceeded) |
| `expired_at` | `u64` (ledger timestamp of the call) |

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

### `AttestationDigestRevoked`

Emitted after successful `revoke_attestation_digest`. Marks a previously appended
digest as superseded without deleting the original entry from the append log.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `attestation_digest_revoked` |
| 1 | `name` | `Symbol` | `att_rev` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |
| 3 | `index` | `u32` | Revoked attestation index |

Data: empty map; this struct has no non-topic fields.

### `AttestationDigestUnrevoked`

Emitted after successful `unrevoke_attestation_digest`. Reverses a prior
revocation for the given append-log index without altering the original digest
entry.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `attestation_digest_unrevoked` |
| 1 | `name` | `Symbol` | `att_unrev` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |
| 3 | `index` | `u32` | Unrevoked attestation index |

Data: empty map; this struct has no non-topic fields.

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

`set_investors_allowlisted` emits one `InvestorAllowlistChanged` event per
investor in input order. The batch is bounded by `MAX_INVESTOR_ALLOWLIST_BATCH`;
if the input exceeds the bound the call fails before any event is emitted, so
consumers never observe a partial batch. Duplicate investors within a single
batch are rejected before emission to keep the event stream deterministic.

### `AdminProposalSuperseded`

Emitted during `propose_admin` (including via the deprecated `transfer_admin`
shim) when an existing pending admin proposal is replaced with a different
address.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `admin_proposal_superseded` |
| 1 | `name` | `Symbol` | `adm_sup` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `previous_pending` | `Address` |
| `new_pending` | `Address` |

### `AdminProposalCancelled`

Emitted after successful `cancel_pending_admin`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `admin_proposal_cancelled` |
| 1 | `name` | `Symbol` | `adm_can` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `cancelled_pending` | `Address` |

### `AdminRecoveredEvent`

Emitted after successful `recover_admin` when the current admin clears an
expired, abandoned admin-transfer proposal after the proposal timelock.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `admin_recovered_event` |
| 1 | `name` | `Symbol` | `adm_rec` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `current_admin` | `Address` |
| `abandoned_pending` | `Address` |
| `reason` | `String` |

### `MaxUniqueInvestorsCapRaised`

Emitted after successful `raise_max_unique_investors`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `max_unique_investors_cap_raised` |
| 1 | `name` | `Symbol` | `raise_cap` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `old_cap` | `u32` |
| `new_cap` | `u32` |

### `MinContributionFloorLowered`

Emitted after successful `lower_min_contribution_floor`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `min_contribution_floor_lowered` |
| 1 | `name` | `Symbol` | `floor_lo` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `old_floor` | `i128` |
| `new_floor` | `i128` |

### `MaxPerInvestorCapRaised`

Emitted after successful `raise_max_per_investor`. Note that its `name` symbol
(`inv_cap`) is shared with `MaxUniqueInvestorsCapLowered`, so indexers should
disambiguate using `topic[0]` (`max_per_investor_cap_raised`).

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `max_per_investor_cap_raised` |
| 1 | `name` | `Symbol` | `inv_cap` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `old_cap` | `i128` |
| `new_cap` | `i128` |

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

### `PayoutBatchProcessed`

Emitted once at the end of a successful `push_investor_payouts` batch call,
summarizing the number of payouts disbursed and the total gross amount paid.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `payout_batch_processed` |
| 1 | `name` | `Symbol` | `pay_batch` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `count` | `u32` |
| `total` | `i128` |

### `RefundBatchProcessed`

Emitted once at the end of every successful `refund_batch` call (including an
all-skipped retry returning `(0, 0)`). Each individual refund in the batch also
emits `InvestorRefundedEvt`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `refund_batch_processed` |
| 1 | `name` | `Symbol` | `refund_bt` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `count` | `u32` |
| `total` | `i128` |

### `EscrowUnfunded`

Emitted after a successful `unfund` call when an investor partially or fully
withdraws their principal while the escrow remains open (status 0).

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `escrow_unfunded` |
| 1 | `name` | `Symbol` | `unfunded` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |
| 3 | `investor` | `Address` | Withdrawing investor |

Data:

| Field | Type |
|---|---|
| `amount` | `i128` |
| `remaining_contribution` | `i128` |
| `new_funded_amount` | `i128` |
| `timestamp` | `u64` |

### `InvestorFreezeChanged`

Emitted after successful `set_investor_frozen` whenever an investor's freeze
status changes.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `investor_freeze_changed` |
| 1 | `name` | `Symbol` | `inv_frz` |
| 2 | `investor` | `Address` | Affected investor |
| 3 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type | Values |
|---|---|---|
| `frozen` | `u32` | `1` = frozen, `0` = unfrozen |
| `reason_code` | `u32` | Caller-supplied compliance reason code |

### `ContributionReferenced`

Emitted after successful `fund_with_reference`, immediately after
`EscrowFunded`, tying the on-chain contribution to an off-chain payment
reference.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `contribution_referenced` |
| 1 | `name` | `Symbol` | `fund_ref` |
| 2 | `investor` | `Address` | Funding investor |

Data:

| Field | Type |
|---|---|
| `invoice_id` | `Symbol` |
| `amount` | `i128` |
| `reference` | `BytesN<32>` |

### `UpgradeProposed`

Emitted after successful `propose_upgrade`. The deprecated `upgrade` entrypoint calls `propose_upgrade`, so it also emits this event (followed by `DeprecatedUpgradeUsed`). A new proposal replaces any earlier pending one; `eta` is the earliest ledger timestamp at which `execute_upgrade` may run.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `upgrade_proposed` |
| 1 | `name` | `Symbol` | `upg_prop` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `new_wasm_hash` | `BytesN<32>` |
| `eta` | `u64` |

### `UpgradeCancelled`

Emitted after successful `cancel_upgrade`, once the pending proposal has been removed. `cancelled_wasm_hash` is the hash of the proposal that was discarded.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `upgrade_cancelled` |
| 1 | `name` | `Symbol` | `upg_cncl` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `cancelled_wasm_hash` | `BytesN<32>` |

### `ContractUpgraded`

Emitted by `execute_upgrade` after the pending proposal is cleared and immediately before the contract WASM is replaced. Note that the `name` symbol is `upgrade`, not a `upg_*` symbol.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `contract_upgraded` |
| 1 | `name` | `Symbol` | `upgrade` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `new_wasm_hash` | `BytesN<32>` |

### `DeprecatedUpgradeUsed`

Emitted by the deprecated `upgrade` entrypoint after it has delegated to `propose_upgrade` (so an `UpgradeProposed` event is emitted first, in the same call). Indexers can use it to detect integrators that still need to migrate to `propose_upgrade` + `execute_upgrade`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `deprecated_upgrade_used` |
| 1 | `name` | `Symbol` | `depr_upg` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `new_wasm_hash` | `BytesN<32>` |

### `DeprecatedTransferAdminUsed`

Emitted by the deprecated `transfer_admin` entrypoint after it has delegated to `propose_admin` (so the events emitted by `propose_admin` come first, in the same call). `proposed_address` is the address passed to the deprecated shim. Migrate to `propose_admin` + `accept_admin`.

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

### `PausedChanged`

Emitted after a successful `set_paused` or `pause_as_pauser` call. A no-op
clear also emits the event so indexers can observe the requested toggle.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `paused_changed` |
| 1 | `name` | `Symbol` | `paused` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `active` | `u32` (`1` enabled, `0` cleared) |
| `scope` | `PauseScope` |
| `reason` | `PauseReason` |

### `PauseMaxDurationUpdated`

Emitted after successful `set_pause_max_duration`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `pause_max_duration_updated` |
| 1 | `name` | `Symbol` | `pausemax` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `old_value` | `u64` |
| `new_value` | `u64` |

### `PauseRateLimitUpdated`

Emitted after successful `set_pause_rate_limit`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `pause_rate_limit_updated` |
| 1 | `name` | `Symbol` | `pause_rl` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `old_limit` | `u32` |
| `new_limit` | `u32` |
| `old_window_secs` | `u64` |
| `new_window_secs` | `u64` |

### `PauseCooldownUpdated`

Emitted after successful `set_pause_cooldown`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `pause_cooldown_updated` |
| 1 | `name` | `Symbol` | `pausecd` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `old_value` | `u64` |
| `new_value` | `u64` |

### `PauserChanged`

Emitted after successful `set_pauser`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `pauser_changed` |
| 1 | `name` | `Symbol` | `pauser` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `pauser` | `Option<Address>` (new address, or `None` when revoked) |

### `FeeScheduleSubmitted`

Emitted after `submit_fee_schedule` stores a new pending fee schedule.
Resubmitting an identical still-pending schedule is a successful no-op and does
not re-emit this event.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `fee_schedule_submitted` |
| 1 | `name` | `Symbol` | `fee_sub` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `fee_bps` | `u32` |
| `activation_ledger` | `u32` |

### `FeeScheduleActivated`

Emitted by `activate_fee_schedule` when a pending schedule reaches its activation
ledger. `submit_fee_schedule` calls `activate_fee_schedule` first, so a submission
can lazily activate an earlier schedule and emit this event before
`FeeScheduleSubmitted`.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `fee_schedule_activated` |
| 1 | `name` | `Symbol` | `fee_act` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `fee_bps` | `u32` |

### `FeeScheduleCancelled`

Emitted after `cancel_pending_fee_schedule` removes a pending fee schedule.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `fee_schedule_cancelled` |
| 1 | `name` | `Symbol` | `fee_can` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `fee_bps` | `u32` |

### `ProtocolFeeUpdated`

Emitted by `set_protocol_fee_bps` when the stored protocol fee changes. A no-op
update emits no event.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `protocol_fee_updated` |
| 1 | `name` | `Symbol` | `fee_upd` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `old_fee_bps` | `i64` |
| `new_fee_bps` | `i64` |

### `PerformanceFeeUpdated`

Emitted by `set_performance_fee_bps` when the stored performance fee changes. A
no-op update emits no event.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `performance_fee_updated` |
| 1 | `name` | `Symbol` | `perf_upd` |
| 2 | `invoice_id` | `Symbol` | Escrow invoice id |

Data:

| Field | Type |
|---|---|
| `old_fee_bps` | `u32` |
| `new_fee_bps` | `u32` |

### `PerformanceFeeCollected`

Emitted by the shared `claim_core` helper during `claim_investor_payout` or
`push_investor_payouts` when a positive performance fee is transferred to the
treasury, before `InvestorPayoutClaimed`. Unlike the other fee events, `name`
and `invoice_id` are data fields; only `investor` is a dynamic topic.

Topics:

| Index | Field | Type | Value |
|---:|---|---|---|
| 0 | fixed event topic | `Symbol` | `performance_fee_collected` |
| 1 | `investor` | `Address` | Investor whose payout generated the fee |

Data:

| Field | Type | Value |
|---|---|---|
| `name` | `Symbol` | `perf_fee` |
| `invoice_id` | `Symbol` | Escrow invoice id |
| `fee` | `i128` | Performance fee transferred to the treasury |

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
- Treat the topic list as an ordered tuple. The trailing `version` topic is
  always last; consumers MUST NOT assume any topic after it. New optional
  topics, if ever introduced, will be added before `version` and gated by a
  version bump.
- Data payloads are maps keyed by field name. Consumers MUST tolerate unknown
  keys within a known version (forward-compatible reads) but MUST NOT rely on
  key ordering.

## Security And State Invariants

- Events are emitted only after the entrypoint guard checks and storage writes
  in the successful path.
- Unauthorized calls, invalid zero/negative amounts, overflow paths,
  double-spend paths, legal-hold blocks, and invalid state-machine transitions
  fail before event emission.
- Investor claim and refund events are deduplicated by persistent markers or
  contribution zeroing before emission.
- Event emission is O(1) for all entrypoints except
  `set_investors_allowlisted` and `revoke_attestation_digests`, which emit O(n)
  events for `n <= MAX_INVESTOR_ALLOWLIST_BATCH` and `n <= MAX_ATTESTATION_REVOKE_BATCH`
  respectively.
- Event emission is atomic with respect to the entrypoint's storage writes: a
  failed or reverted call emits no events, so indexers never observe a state
  transition that did not commit.
- `FundingStateChanged` is emitted only when a call actually changes the
  funding status, exactly once per transition (`0 → 1` for the funded edges,
  `0 → 4` for `cancel_funding`); non-transition funding calls emit nothing.

## Changelog

| Date | Version | Change |
|---|---|---|
| 2026-03-23 | v0.1 | Initial event schema reference |
| 2026-05-27 | v0.2 | Added initialization references and investor-cap event notes |
| 2026-05-31 | v0.3 | Issue #272: replaced drifted reference with complete `#[contractevent]` topic and data layout from `escrow/src/lib.rs` |
| 2026-06-24 | v0.4 | Added `settled_at_ledger_timestamp` field to `EscrowSettled` event; added `is_settleable` view |
| 2026-07-27 | v0.5 | Added `AttestationDigestUnrevoked` event for `unrevoke_attestation_digest`; updated `AttestationDigestRevoked` to include `revoke_attestation_digests` |
| 2026-08-14 | v0.6 | Issue #allowlist_event_payloads: documented compatibility contracts for topic ordering, `name` symbols, version topic, batch emission atomicity, and forward-compatible data reads |
| 2026-11-22 | v0.7 | Documented `AdminProposalSuperseded`, `AdminProposalCancelled`, `AdminRecoveredEvent`, `MaxUniqueInvestorsCapRaised`, `MinContributionFloorLowered`, `MaxPerInvestorCapRaised`, and `FundingDeadlineExtended` events |
| 2026-11-22 | v0.7 | Issue #1657: implemented `FundingStateChanged` (`fund_st_ch`) with `old_status` / `new_status` and documented the transitions it covers |
| 2026-11-22 | v0.7 | Documented `PayoutBatchProcessed`, `RefundBatchProcessed`, `EscrowUnfunded`, `InvestorFreezeChanged`, and `ContributionReferenced` events |
| 2026-11-23 | v0.8 | Issue #1636: documented `UpgradeProposed`, `UpgradeCancelled`, `ContractUpgraded`, `DeprecatedUpgradeUsed`, and `DeprecatedTransferAdminUsed` events |
| 2026-11-23 | v0.9 | Issue #1637: documented `EscrowPartialSettle`, `EscrowRepaid`, `SettlementFunded`, `SettleBatchProcessed`, and `PartialCloseMinBpsUpdated` events |
| 2026-11-23 | v0.10 | Issue #1634: documented `FeeScheduleSubmitted`, `FeeScheduleActivated`, `FeeScheduleCancelled`, `ProtocolFeeUpdated`, `PerformanceFeeUpdated`, and `PerformanceFeeCollected` events |
