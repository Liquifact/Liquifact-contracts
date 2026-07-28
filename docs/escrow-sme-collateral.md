# Escrow — SME Collateral Commitment

## Overview

The LiquiFact escrow contract supports **metadata-only** collateral commitment recording.
No tokens are moved, reserved, or locked by these operations. The stored
`SmeCollateralCommitment` and emitted collateral events are **not proof of custody**,
lien, encumbrance, or asset control — they exist solely for indexers and off-chain risk
teams to surface reported collateral intent alongside an invoice's on-chain state. Risk
teams must verify supporting evidence outside this contract.

---

## Entrypoints

### `record_sme_collateral_commitment(env, asset: Symbol, amount: i128) -> SmeCollateralCommitment`

Records (or replaces) an off-chain collateral commitment against the escrow's invoice.

- **Auth**: SME address (`sme_address` from the escrow record).
- **Storage**: writes `DataKey::SmeCollateralPledge` (instance storage) as an
  `SmeCollateralCommitment { asset, amount, recorded_at }`.
- **Event**: emits exactly one `CollateralRecordedEvt` with
  `{ name: "coll_rec", invoice_id, amount, prior_amount }`.
- **Idempotency**: calling again replaces the previous commitment; `prior_amount` on the
  event carries the amount that was overwritten (`0` on the first call).
- **Ordering guard**: a replacement's ledger timestamp must be `>=` the prior commitment's
  `recorded_at`, or the call is rejected with `CollateralTimestampBackwards`.
- **Token movement**: none.

### `get_sme_collateral_commitment(env) -> Option<SmeCollateralCommitment>`

Returns the current commitment, or `None` if none has been recorded (or it was cleared).

- **Auth**: none required (read-only).

### `clear_sme_collateral_commitment(env)`

Retires a previously recorded commitment, removing it from storage.

- **Auth**: SME address (`sme_address` from the escrow record).
- **Storage**: removes `DataKey::SmeCollateralPledge` (instance storage).
- **Event**: emits exactly one `CollateralClearedEvt`
  `{ name: "coll_clr", invoice_id, asset, amount, recorded_at }` under topics
  `(coll_clr, invoice_id)`.
- **Token movement**: none.
- **Absent commitment**: returns `NoCollateralToClear` (no event).

## Test Coverage

The scenarios below are covered by the focused collateral suite in
[`escrow/src/tests/coverage.rs`](../escrow/src/tests/coverage.rs):

| Test | Scenario |
|------|----------|
| `test_collateral_first_record_returns_correct_fields_and_prior_amount_is_zero` | First record returns the correct asset/amount/timestamp; `get_sme_collateral_commitment` reflects it. |
| `test_collateral_first_record_event_prior_amount_is_zero` | `CollateralRecordedEvt` emitted by the first record has `prior_amount = 0`. |
| `test_collateral_replacement_overwrites_stored_value_and_emits_prior_amount` | Replacement overwrites storage; event carries the previous record's amount as `prior_amount`. |
| `test_collateral_backwards_timestamp_rejected` | Replacing with a ledger timestamp earlier than `recorded_at` is rejected with `CollateralTimestampBackwards`; original record is preserved. |
| `test_collateral_same_timestamp_replacement_is_allowed` | Equal timestamps (`now >= prior.recorded_at`) are accepted (monotonic, not strictly increasing). |
| `test_collateral_zero_amount_rejected` | Zero amount is rejected with `CollateralAmountNotPositive`. |
| `test_collateral_negative_amount_rejected` | Negative amount is rejected with `CollateralAmountNotPositive`. |
| `test_collateral_empty_asset_rejected` | Empty asset symbol is rejected with `CollateralAssetEmpty`. |
| `test_collateral_non_sme_caller_rejected` | A caller that is not the SME address is rejected (auth failure). |
| `test_collateral_record_does_not_change_token_balances` | No token balances change on the escrow contract, SME, or admin after recording. |
| `test_clear_emits_exactly_one_coll_clr_event` | Clear emits exactly one `coll_clr` event with the cleared commitment payload. |
| `test_collateral_state_change_topics_are_distinct` | `coll_rec` and `coll_clr` routing symbols do not collide. |

Additional collateral scenarios (happy-path and validation) are also exercised in:
- [`escrow/src/tests/admin.rs`](../escrow/src/tests/admin.rs) — collateral record in admin-flow baselines.
- [`escrow/src/tests/integration.rs`](../escrow/src/tests/integration.rs) — `test_collateral_record_event_payload_is_metadata_only`, `test_collateral_replacement_event_contains_prior_amount`, and `test_collateral_clear_emits_one_dedicated_event_with_cleared_payload` for full event-payload verification.

## Off-chain Risk-Team Handling

Risk teams and indexers must treat `SmeCollateralCommitment`, `CollateralRecordedEvt`,
and `CollateralClearedEvt` as **reported metadata only**. They are not proof of custody,
lien, encumbrance, or asset control, and do not alter funding, settlement, withdrawal,
investor-claim, compliance-hold, or treasury-sweep behavior. Supporting evidence must be
verified off-chain.

---

## Guard ordering (ADR-002)

`clear_sme_collateral_commitment` applies guards in this order to keep auth
checks from masking informative errors:

1. **Read-only existence check** — return `NoCollateralToClear` immediately if
   `DataKey::SmeCollateralPledge` is absent (no auth consumed).
2. **`require_auth`** — assert the caller is the SME address.
3. **Mutation** — remove the storage entry and emit exactly one `CollateralClearedEvt`.

---

## Data types

```rust
pub struct SmeCollateralCommitment {
    pub asset: Symbol,
    pub amount: i128,
    pub recorded_at: u64,
}

pub struct CollateralRecordedEvt {
    pub name: Symbol,       // "coll_rec"
    pub invoice_id: Symbol,
    pub amount: i128,
    pub prior_amount: i128, // 0 on the first record
}

pub struct CollateralClearedEvt {
    pub name: Symbol,       // hardcoded coll_clr topic
    pub invoice_id: Symbol,
    pub asset: Symbol,      // carried from the pledge at the time of removal
    pub amount: i128,       // carried from the pledge at the time of removal
    pub recorded_at: u64,   // original pledge ledger timestamp
}
```

---

## Error codes

| Code | Variant                        | Trigger                                                |
|------|--------------------------------|---------------------------------------------------------|
| 60   | `CollateralAmountNotPositive`  | `record_sme_collateral_commitment` with `amount <= 0`    |
| 61   | `CollateralAssetEmpty`         | `record_sme_collateral_commitment` with empty asset symbol |
| 62   | `CollateralTimestampBackwards` | Replacement timestamp precedes the stored `recorded_at`  |
| 63   | `NoCollateralToClear`          | `clear_sme_collateral_commitment` with no commitment recorded |

---

## Security notes

- **Metadata-only**: neither `record_sme_collateral_commitment` nor
  `clear_sme_collateral_commitment` transfers or locks tokens. This is
  **not proof of custody** — the contract does not verify off-chain asset control.
- **SME-only writes**: all mutating operations require `sme_address.require_auth()`.
- **No status dependency**: collateral metadata can be recorded or cleared regardless of
  escrow status (open / funded / settled), allowing clean-up after settlement or cancellation.
- **No double-clear risk**: the existence check on entry ensures a second clear call
  returns `NoCollateralToClear` rather than silently succeeding.
- **No duplicate emissions**: each successful record or clear publishes exactly one
  dedicated event (`coll_rec` or `coll_clr`).

---

## Example flow

```
SME calls record_sme_collateral_commitment("USDC", 5_000_0000000)
  → DataKey::SmeCollateralPledge stored as SmeCollateralCommitment
  → CollateralRecordedEvt { amount: 5_000_0000000, prior_amount: 0 } emitted

[invoice settled off-chain; commitment released]

SME calls clear_sme_collateral_commitment()
  → DataKey::SmeCollateralPledge removed
  → CollateralClearedEvt { name: "coll_clr", invoice_id: "INV001", asset: "USDC", amount: 5_000_0000000, recorded_at } emitted
```
