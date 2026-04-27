# SME Collateral Commitment Metadata

`record_sme_collateral_commitment(asset, amount)` is a metadata-only Soroban escrow entrypoint. It lets the configured SME address report collateral information for off-chain review, but it does not move, reserve, escrow, freeze, or verify any asset on-chain.

## On-chain behavior

- Authorization: only `InvoiceEscrow.sme_address` may call the entrypoint.
- Validation: `amount` must be positive.
- Storage: the call writes one `SmeCollateralCommitment` under `DataKey::SmeCollateralPledge`, replacing any previous record for the escrow.
- Timestamp: `recorded_at` is the current Soroban ledger timestamp from `Env::ledger()`.
- Event: the call emits `CollateralRecordedEvt` to signal that the metadata record changed.

## What `CollateralRecordedEvt` means

`CollateralRecordedEvt` is a record-change event, not an asset-control event. It is not proof of custody and should be indexed as reported collateral metadata only.

The event is not proof of:

- token custody or asset possession
- a lien, security interest, or enforceable encumbrance
- a reserved, frozen, or escrowed balance
- a transfer, approval, allowance, or payment instruction
- valuation, eligibility, perfection, or priority of the referenced asset

The event intentionally does not include a token contract ID, custodian address, transfer receipt, oracle price, registry attestation, or enforcement state. Consumers that need those facts must verify them through separate off-chain controls or dedicated contracts.

## Off-chain risk-team handling

Risk teams should treat the record as SME-provided input and verify it before using it in underwriting, monitoring, reporting, or operational decisions.

Recommended checks:

- Confirm the SME signer and invoice context.
- Confirm the asset identifier maps to the intended off-chain asset or token contract.
- Verify supporting documents, custody statements, account-control evidence, and valuation sources outside this contract.
- Track the ledger timestamp and sequence where the record was written.
- Label indexed fields as `reported_collateral_metadata`, `sme_reported_asset`, and `sme_reported_amount`; avoid labels that imply a locked balance or enforceable claim.
- Reconcile any separate asset-control workflow independently from this escrow record.

## Out of scope

The escrow contract does not implement collateral custody, token transfer enforcement, pricing, registry validation, margining, or automatic risk actions from this metadata. Token-transfer assumptions and unsupported token economics are documented separately in [`escrow/src/external_calls.rs`](../escrow/src/external_calls.rs) and [`ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`](ESCROW_TOKEN_INTEGRATION_CHECKLIST.md).
