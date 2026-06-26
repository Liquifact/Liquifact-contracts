# Escrow — SME Collateral Commitment

## Overview

The LiquiFact escrow contract supports **metadata-only** collateral pledge recording.
No tokens are moved or reserved by these operations; they exist solely for indexers
and dashboards to surface off-chain pledge intent alongside an invoice's on-chain state.

---

## Entrypoints

### `record_sme_collateral_commitment(env, amount) -> Result<(), EscrowError>`

Records an off-chain collateral pledge against the escrow's invoice.

- **Auth**: SME address (`sme_address` from the escrow record).
- **Storage**: writes `DataKey::SmeCollateralPledge` (instance storage).
- **Event**: emits `CollateralRecordedEvt { invoice_id, amount }` under topic
  `(col_rec, sme_address)`.
- **Idempotency**: calling again overwrites the previous amount.
- **Token movement**: none.

### `get_sme_collateral_commitment(env) -> Option<CollateralPledge>`

Returns the current pledge record, or `None` if none has been recorded (or it was cleared).

- **Auth**: none required (read-only).

### `clear_sme_collateral_commitment(env) -> Result<(), EscrowError>`

Retires a previously recorded pledge, removing it from storage.

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

Additional collateral scenarios (happy-path and validation) are also exercised in:
- [`escrow/src/tests/admin.rs`](../escrow/src/tests/admin.rs) — collateral record in admin-flow baselines.
- [`escrow/src/tests/integration.rs`](../escrow/src/tests/integration.rs) — `test_collateral_record_event_payload_is_metadata_only` and `test_collateral_replacement_event_contains_prior_amount` for full event-payload verification.

## Off-chain Risk-Team Handling

---

## Guard ordering (ADR-002)

`clear_sme_collateral_commitment` applies guards in this order to keep auth
checks from masking informative errors:

1. **Read-only existence check** — return `NoCollateralToClear` immediately if
   `DataKey::SmeCollateralPledge` is absent (no auth consumed).
2. **`require_auth`** — assert the caller is the SME address.
3. **Mutation** — remove the storage entry and emit `CollateralClearedEvt`.

---

## Data types

```rust
pub struct CollateralPledge {
    pub invoice_id: Symbol,
    pub amount: i128,
}

pub struct CollateralRecordedEvt {
    pub invoice_id: Symbol,
    pub amount: i128,
}

pub struct CollateralClearedEvt {
    pub invoice_id: Symbol,
    pub amount: i128,   // carried from the pledge at the time of removal
}
```

---

## Error codes

| Code | Variant              | Trigger                                            |
|------|----------------------|----------------------------------------------------|
| 1    | `NotInitialized`     | Escrow not yet created via `init`                  |
| 2    | `NotOpen`            | Reserved for future status guards                  |
| 3    | `NotFunded`          | Reserved for future status guards                  |
| 4    | `NoCollateralToClear`| `clear_sme_collateral_commitment` with no pledge   |

---

## Security notes

- **Metadata-only**: neither `record_sme_collateral_commitment` nor
  `clear_sme_collateral_commitment` transfers or locks tokens.
- **SME-only writes**: all mutating operations require `sme_address.require_auth()`.
- **No status dependency**: collateral metadata can be cleared regardless of escrow
  status (open / funded / settled), allowing clean-up after settlement or cancellation.
- **No double-clear risk**: the existence check on entry ensures a second clear call
  returns `NoCollateralToClear` rather than silently succeeding.

---

## Example flow

```
SME calls record_sme_collateral_commitment(5_000_0000000)
  → DataKey::SmeCollateralPledge stored
  → CollateralRecordedEvt emitted

[invoice settled off-chain; pledge released]

SME calls clear_sme_collateral_commitment()
  → DataKey::SmeCollateralPledge removed
  → CollateralClearedEvt { invoice_id: "INV001", amount: 5_000_0000000 } emitted
```
