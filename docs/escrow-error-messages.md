# Liquifact Escrow Typed Error Codes

LiquiFact escrow emits typed Soroban contract errors through `EscrowError`. Clients should branch
on the numeric `ContractError(code)` value, not on panic strings or diagnostic text.

## Stability Policy

Error codes are append-only. Once a code is assigned, it must not be renamed for a different
meaning, reused, or renumbered. New failures must receive new codes after the existing range.

Keep the numeric ranges stable by area:

| Codes | Area |
| --- | --- |
| 1-13 | Initialization, invoice id, and tier configuration validation |
| 20-22 | Missing initialized escrow metadata |
| 30-42 | Treasury dust sweep and token integration safety |
| 50-62 | Attestation and collateral metadata |
| 70-80 | Administrative configuration updates |
| 90-92 | Schema migration |
| 100-110 | Funding |
| 120-129 | Settlement, withdrawal, and investor payout |
| 140-143 | Cancellation and refund |
| 150-152 | Legal hold clear scheduling |
| 160-162 | Beneficiary rotation |

Legacy panic messages are listed only to help integrators migrate old simulations and logs.

## Canonical Code Table

| Code | Variant | Emitting entrypoint(s) | Trigger condition | Client action |
| ---: | --- | --- | --- | --- |
| 1 | `AmountMustBePositive` | `init` | Initial invoice amount is not positive. | Send a positive amount before retrying. |
| 2 | `YieldBpsOutOfRange` | `init` | Base yield is outside `0..=10_000` basis points. | Clamp or reject the configured yield. |
| 3 | `EscrowAlreadyInitialized` | `init` | Escrow storage is already initialized. | Treat the instance as already deployed, or use a new contract instance. |
| 4 | `InvoiceIdInvalidLength` | `init` via invoice id validation | Invoice id length is outside `1..=MAX_INVOICE_ID_STRING_LEN`. | Regenerate the invoice id with a valid length. |
| 5 | `InvoiceIdInvalidCharset` | `init` via invoice id validation | Invoice id contains a character outside `[A-Za-z0-9_]`. | Sanitize the invoice id before submission. |
| 6 | `MinContributionNotPositive` | `init` | Configured `min_contribution` is zero or negative. | Remove the floor or set a positive value. |
| 7 | `MinContributionExceedsAmount` | `init` | `min_contribution` exceeds the initial amount or target hint. | Lower the floor or raise the target amount. |
| 8 | `MaxUniqueInvestorsNotPositive` | `init` | Configured distinct investor cap is zero. | Omit the cap or set a positive value. |
| 9 | `MaxPerInvestorNotPositive` | `init` | Configured per-investor cap is zero or negative. | Omit the cap or set a positive value. |
| 10 | `TierYieldOutOfRange` | `init` via tier validation | A tier yield is outside `0..=10_000`. | Correct the tier schedule. |
| 11 | `TierYieldBelowBase` | `init` via tier validation | A tier yield is lower than base `yield_bps`. | Raise the tier yield or lower the base yield. |
| 12 | `TierLockNotIncreasing` | `init` via tier validation | Tier `min_lock_secs` values are not strictly increasing. | Sort and deduplicate tier locks. |
| 13 | `TierYieldNotNonDecreasing` | `init` via tier validation | Tier yields decrease across the lock schedule. | Make tier yields monotonic. |
| 20 | `EscrowNotInitialized` | `get_escrow`, admin/SME-gated entrypoints | Escrow storage is absent. | Initialize the contract before calling state-dependent entrypoints. |
| 21 | `FundingTokenNotSet` | `get_funding_token`, `init`, `refund`, `sweep_terminal_dust` | Funding token address is missing. | Verify initialization and token configuration. |
| 22 | `TreasuryNotSet` | `get_treasury`, `sweep_terminal_dust` | Treasury address is missing. | Verify initialization and treasury configuration. |
| 30 | `LegalHoldBlocksTreasuryDustSweep` | `sweep_terminal_dust` | Legal hold is active. | Wait for governance to clear the hold. |
| 31 | `SweepAmountNotPositive` | `sweep_terminal_dust` | Requested sweep amount is zero or negative. | Send a positive sweep amount. |
| 32 | `SweepAmountExceedsMax` | `sweep_terminal_dust` | Requested sweep exceeds `MAX_DUST_SWEEP_AMOUNT`. | Split or lower the sweep request. |
| 33 | `DustSweepNotTerminal` | `sweep_terminal_dust` | Escrow is not settled, withdrawn, or cancelled. | Wait for a terminal escrow state. |
| 34 | `NoFundingTokenBalanceToSweep` | `sweep_terminal_dust` | Contract token balance is zero. | Do not submit a sweep; there is no dust to collect. |
| 35 | `EffectiveSweepAmountZero` | `sweep_terminal_dust` | Requested amount min balance resolves to zero. | Refresh balance and retry only if funds exist. |
| 36 | `TransferAmountNotPositive` | `sweep_terminal_dust`, `refund` via token transfer helper | Transfer helper received a non-positive amount. | Treat as client or integration input error. |
| 37 | `InsufficientTokenBalanceBeforeTransfer` | `sweep_terminal_dust`, `refund` via token transfer helper | Contract balance is below the requested transfer amount. | Refresh balances and retry only after funding is available. |
| 38 | `SenderBalanceUnderflow` | `sweep_terminal_dust`, `refund` via token transfer helper | Sender balance delta underflowed after token transfer. | Flag token integration review; token behavior is non-standard. |
| 39 | `RecipientBalanceUnderflow` | `sweep_terminal_dust`, `refund` via token transfer helper | Recipient balance delta underflowed after token transfer. | Flag token integration review; token behavior is non-standard. |
| 40 | `SenderBalanceDeltaMismatch` | `sweep_terminal_dust`, `refund` via token transfer helper | Sender balance did not decrease by exactly `amount`. | Reject the token integration until SEP-41 balance behavior is verified. |
| 41 | `RecipientBalanceDeltaMismatch` | `sweep_terminal_dust`, `refund` via token transfer helper | Recipient balance did not increase by exactly `amount`. | Reject the token integration until SEP-41 balance behavior is verified. |
| 42 | `SweepExceedsLiabilityFloor` | `sweep_terminal_dust` | Cancelled escrow sweep would leave less balance than outstanding refunds. | Lower the sweep amount or wait until more refunds are distributed. |
| 50 | `PrimaryAttestationAlreadyBound` | `bind_primary_attestation_hash` | Primary attestation hash is already set. | Treat as immutable; append new evidence instead. |
| 51 | `AttestationAppendLogCapacityReached` | `append_attestation_digest` | Append-only attestation log reached capacity. | Stop appending or deploy a replacement workflow. |
| 60 | `CollateralAmountNotPositive` | `record_sme_collateral_commitment` | Collateral amount is zero or negative. | Submit a positive collateral amount. |
| 61 | `CollateralAssetEmpty` | `record_sme_collateral_commitment` | Collateral asset symbol is empty. | Provide a non-empty asset symbol. |
| 62 | `CollateralTimestampBackwards` | `record_sme_collateral_commitment` | New collateral timestamp is older than the stored commitment. | Retry with a monotonic timestamp. |
| 70 | `InvestorBatchEmpty` | investor allowlist batch entrypoints | Investor batch is empty. | Submit at least one investor address. |
| 71 | `InvestorBatchTooLarge` | investor allowlist batch entrypoints | Investor batch exceeds `MAX_INVESTOR_ALLOWLIST_BATCH`. | Split the batch into smaller chunks. |
| 72 | `TargetNotPositive` | `update_funding_target` | New funding target is not positive. | Submit a positive target. |
| 73 | `TargetUpdateNotOpen` | `update_funding_target` | Escrow is not in open state. | Update targets only before funding closes. |
| 74 | `TargetBelowFundedAmount` | `update_funding_target` | New target is below amount already funded. | Set the target at or above current funded amount. |
| 75 | `CapLowerNotOpen` | `lower_max_unique_investors` | Escrow is not in open state. | Lower caps only before funding closes. |
| 76 | `NoInvestorCapConfigured` | `lower_max_unique_investors` | Escrow has no distinct investor cap. | Do not call cap lowering for uncapped escrows. |
| 77 | `NewCapNotLower` | `lower_max_unique_investors` | New cap is not below the current cap. | Submit a strictly lower cap. |
| 78 | `NewCapBelowCurrentFunderCount` | `lower_max_unique_investors` | New cap is below current unique funder count. | Choose a cap at or above current unique funders. |
| 79 | `MaturityUpdateNotOpen` | `update_maturity` | Escrow is not in open state. | Update maturity only before funding closes. |
| 80 | `NewAdminSameAsCurrent` | `propose_admin` | Proposed admin equals current admin. | Submit a different admin address. |
| 90 | `MigrationVersionMismatch` | `migrate` | Supplied version does not match stored version. | Re-read stored version and retry only if migration is supported. |
| 91 | `AlreadyCurrentSchemaVersion` | `migrate` | Supplied version is already current or newer. | Skip migration. |
| 92 | `NoMigrationPath` | `migrate` | No migration path is implemented for the supplied version. | Redeploy or implement and audit a migration path. |
| 100 | `FundingAmountNotPositive` | `fund`, `fund_on_behalf` via funding helper | Funding amount is zero or negative. | Submit a positive funding amount. |
| 101 | `FundingBelowMinContribution` | `fund`, `fund_on_behalf` via funding helper | Funding amount is below `min_contribution`. | Increase amount or show the minimum to the investor. |
| 102 | `LegalHoldBlocksFunding` | `fund`, `fund_on_behalf` via funding helper | Legal hold is active. | Pause funding until governance clears the hold. |
| 103 | `EscrowNotOpenForFunding` | `fund`, `fund_on_behalf` via funding helper | Escrow status is no longer open. | Stop accepting contributions for this escrow. |
| 104 | `InvestorNotAllowlisted` | `fund`, `fund_on_behalf` via funding helper | Allowlist is active and investor is not approved. | Ask admin to allowlist the investor or block the contribution. |
| 105 | `InvestorContributionOverflow` | `fund`, `fund_on_behalf` via funding helper | Investor contribution addition overflowed. | Reject the transaction and investigate amount bounds. |
| 106 | `InvestorContributionExceedsCap` | `fund`, `fund_on_behalf` via funding helper | Investor would exceed max-per-investor cap. | Lower contribution or show remaining per-investor capacity. |
| 107 | `UniqueInvestorCapReached` | `fund`, `fund_on_behalf` via funding helper | Distinct investor cap is reached. | Block new investors; existing investors may still add principal when allowed. |
| 108 | `TieredSecondDeposit` | `fund`, `fund_on_behalf` via funding helper | Investor tries additional principal after a tiered first deposit. | Direct investor to the supported funding path or disallow the extra deposit. |
| 109 | `InvestorClaimTimeOverflow` | `fund`, `fund_on_behalf` via funding helper | Claim timestamp calculation overflowed. | Reject and review tier lock configuration. |
| 110 | `FundedAmountOverflow` | `fund`, `fund_on_behalf` via funding helper | Escrow funded amount addition overflowed. | Reject and investigate amount bounds. |
| 120 | `LegalHoldBlocksSettlement` | `settle` | Legal hold is active. | Wait for governance to clear the hold. |
| 121 | `SettlementNotFunded` | `settle` | Escrow is not funded. | Settle only after funding closes. |
| 122 | `MaturityNotReached` | `settle` | Escrow maturity timestamp has not been reached. | Retry after maturity or use a no-maturity configuration. |
| 123 | `LegalHoldBlocksWithdrawal` | `withdraw` | Legal hold is active. | Wait for governance to clear the hold. |
| 124 | `WithdrawalNotFunded` | `withdraw` | Escrow is not funded. | Withdraw only after funding closes. |
| 125 | `LegalHoldBlocksInvestorClaims` | `claim_investor_payout` | Legal hold is active. | Wait for governance to clear the hold. |
| 126 | `NoContributionToClaim` | `claim_investor_payout` | Investor has no contribution. | Hide or disable claim for non-participants. |
| 127 | `InvestorClaimNotSettled` | `claim_investor_payout` | Escrow is not settled. | Allow claims only after settlement. |
| 128 | `InvestorCommitmentLockNotExpired` | `claim_investor_payout` | Investor commitment lock has not expired. | Show the unlock timestamp and retry later. |
| 129 | `ComputePayoutArithmeticOverflow` | `claim_investor_payout`, `compute_investor_payout` | Payout arithmetic overflowed. | Reject and investigate principal/yield bounds. |
| 140 | `LegalHoldBlocksCancelFunding` | `cancel_funding` | Legal hold is active. | Wait for governance to clear the hold. |
| 141 | `CancelFundingNotOpen` | `cancel_funding` | Escrow is not open. | Cancel funding only before funding closes. |
| 142 | `RefundNotCancelled` | `refund` | Escrow is not cancelled. | Offer refund only for cancelled escrows. |
| 143 | `NoContributionToRefund` | `refund` | Investor has no refundable contribution or already refunded. | Hide refund action or mark it complete. |
| 150 | `LegalHoldClearRequestMissing` | `set_legal_hold(false)` | Non-zero clear delay is configured but no clear request exists. | Call `request_clear_legal_hold` first. |
| 151 | `LegalHoldClearNotReady` | `set_legal_hold(false)` | Clear request exists but the delay has not elapsed. | Show the clearable timestamp and retry later. |
| 152 | `LegalHoldClearDelayOverflow` | `request_clear_legal_hold` | `ledger.timestamp + delay` overflowed. | Reject the configured delay and require governance correction. |
| 160 | `LegalHoldBlocksBeneficiaryRotation` | `rotate_beneficiary` | Legal hold is active. | Wait for governance to clear the hold. |
| 161 | `RotationNotOpen` | `rotate_beneficiary` | Escrow is not open or funded. | Rotate only before settlement or withdrawal. |
| 162 | `NewSmeSameAsCurrent` | `rotate_beneficiary` | New SME address equals the current beneficiary. | Submit a different SME address. |

## Legacy Panic Mapping

The legacy messages below are compatibility hints for older logs and simulations; the numeric code
and variant remain the canonical contract interface.

| Code | Variant | Legacy failure |
| ---: | --- | --- |
| 1 | `AmountMustBePositive` | `Amount must be positive` |
| 2 | `YieldBpsOutOfRange` | `yield_bps must be between 0 and 10_000` |
| 3 | `EscrowAlreadyInitialized` | `Escrow already initialized` |
| 4 | `InvoiceIdInvalidLength` | `invoice_id length must be 1..=MAX_INVOICE_ID_STRING_LEN` |
| 5 | `InvoiceIdInvalidCharset` | `invoice_id must be [A-Za-z0-9_] only` |
| 6 | `MinContributionNotPositive` | `min_contribution must be positive when configured` |
| 7 | `MinContributionExceedsAmount` | `min_contribution cannot exceed initial invoice amount / target hint` |
| 8 | `MaxUniqueInvestorsNotPositive` | `max_unique_investors must be positive when configured` |
| 9 | `MaxPerInvestorNotPositive` | `max_per_investor must be positive when configured` |
| 10 | `TierYieldOutOfRange` | `tier yield_bps must be 0..=10_000` |
| 11 | `TierYieldBelowBase` | `tier yield_bps must be >= base yield_bps` |
| 12 | `TierLockNotIncreasing` | `tiers must have strictly increasing min_lock_secs` |
| 13 | `TierYieldNotNonDecreasing` | `tiers must have non-decreasing yield_bps` |
| 20 | `EscrowNotInitialized` | `Escrow not initialized` |
| 21 | `FundingTokenNotSet` | `Funding token not set` |
| 22 | `TreasuryNotSet` | `Treasury not set` |
| 30 | `LegalHoldBlocksTreasuryDustSweep` | `Legal hold blocks treasury dust sweep` |
| 31 | `SweepAmountNotPositive` | `sweep amount must be positive` |
| 32 | `SweepAmountExceedsMax` | `sweep amount exceeds MAX_DUST_SWEEP_AMOUNT` |
| 33 | `DustSweepNotTerminal` | `dust sweep only in terminal states` |
| 34 | `NoFundingTokenBalanceToSweep` | `no funding token balance to sweep` |
| 35 | `EffectiveSweepAmountZero` | `effective sweep amount is zero` |
| 36 | `TransferAmountNotPositive` | `transfer amount must be positive` |
| 37 | `InsufficientTokenBalanceBeforeTransfer` | `insufficient token balance before transfer` |
| 38 | `SenderBalanceUnderflow` | `balance underflow on sender` |
| 39 | `RecipientBalanceUnderflow` | `balance underflow on recipient` |
| 40 | `SenderBalanceDeltaMismatch` | `sender balance delta must equal transfer amount` |
| 41 | `RecipientBalanceDeltaMismatch` | `recipient balance delta must equal transfer amount` |
| 42 | `SweepExceedsLiabilityFloor` | `sweep would violate cancelled escrow refund liability floor` |
| 50 | `PrimaryAttestationAlreadyBound` | `primary attestation already bound` |
| 51 | `AttestationAppendLogCapacityReached` | `attestation append log capacity reached` |
| 60 | `CollateralAmountNotPositive` | `Collateral amount must be positive` |
| 61 | `CollateralAssetEmpty` | `Collateral asset symbol must not be empty` |
| 62 | `CollateralTimestampBackwards` | `Collateral commitment timestamp must not go backward` |
| 70 | `InvestorBatchEmpty` | `investors vector must be non-empty` |
| 71 | `InvestorBatchTooLarge` | `investors vector length exceeds MAX_INVESTOR_ALLOWLIST_BATCH` |
| 72 | `TargetNotPositive` | `Target must be strictly positive` |
| 73 | `TargetUpdateNotOpen` | `Target can only be updated in Open state` |
| 74 | `TargetBelowFundedAmount` | `Target cannot be less than already funded amount` |
| 75 | `CapLowerNotOpen` | `Cap can only be lowered in Open state` |
| 76 | `NoInvestorCapConfigured` | `no investor cap configured` |
| 77 | `NewCapNotLower` | `new cap must be strictly lower than current cap` |
| 78 | `NewCapBelowCurrentFunderCount` | `new cap cannot be below current unique funder count` |
| 79 | `MaturityUpdateNotOpen` | `Maturity can only be updated in Open state` |
| 80 | `NewAdminSameAsCurrent` | `New admin must differ from current admin` |
| 90 | `MigrationVersionMismatch` | `from_version does not match stored version` |
| 91 | `AlreadyCurrentSchemaVersion` | `Already at current schema version` |
| 92 | `NoMigrationPath` | `No migration path from version 0 - extend migrate or redeploy` |
| 100 | `FundingAmountNotPositive` | `Funding amount must be positive` |
| 101 | `FundingBelowMinContribution` | `funding amount below min_contribution floor` |
| 102 | `LegalHoldBlocksFunding` | `Legal hold blocks new funding while active` |
| 103 | `EscrowNotOpenForFunding` | `Escrow not open for funding` |
| 104 | `InvestorNotAllowlisted` | `Investor not on allowlist` |
| 105 | `InvestorContributionOverflow` | `investor contribution overflow` |
| 106 | `InvestorContributionExceedsCap` | `investor contribution exceeds max_per_investor cap` |
| 107 | `UniqueInvestorCapReached` | `unique investor cap reached` |
| 108 | `TieredSecondDeposit` | `Additional principal after a tiered first deposit must use fund()` |
| 109 | `InvestorClaimTimeOverflow` | `investor claim time overflow` |
| 110 | `FundedAmountOverflow` | `funded_amount overflow` |
| 120 | `LegalHoldBlocksSettlement` | `Legal hold blocks settlement finalization` |
| 121 | `SettlementNotFunded` | `Escrow must be funded before settlement` |
| 122 | `MaturityNotReached` | `Escrow has not yet reached maturity` |
| 123 | `LegalHoldBlocksWithdrawal` | `Legal hold blocks SME withdrawal` |
| 124 | `WithdrawalNotFunded` | `Escrow must be funded before withdrawal` |
| 125 | `LegalHoldBlocksInvestorClaims` | `Legal hold blocks investor claims` |
| 126 | `NoContributionToClaim` | `Address has no contribution to claim` |
| 127 | `InvestorClaimNotSettled` | `Escrow must be settled before investor claim` |
| 128 | `InvestorCommitmentLockNotExpired` | `Investor commitment lock not expired` |
| 129 | `ComputePayoutArithmeticOverflow` | `compute_investor_payout: arithmetic overflow` |
| 140 | `LegalHoldBlocksCancelFunding` | `Legal hold blocks cancel_funding` |
| 141 | `CancelFundingNotOpen` | `cancel_funding only allowed in Open state` |
| 142 | `RefundNotCancelled` | `refund only allowed in Cancelled state` |
| 143 | `NoContributionToRefund` | `no contribution to refund` |
| 150 | `LegalHoldClearRequestMissing` | `legal hold clear request missing` |
| 151 | `LegalHoldClearNotReady` | `legal hold clear delay has not elapsed` |
| 152 | `LegalHoldClearDelayOverflow` | `legal hold clear delay overflow` |
| 160 | `LegalHoldBlocksBeneficiaryRotation` | `legal hold blocks beneficiary rotation` |
| 161 | `RotationNotOpen` | `beneficiary rotation only allowed before settlement` |
| 162 | `NewSmeSameAsCurrent` | `new SME address must differ from current SME` |

## Client Guidance

In tests and SDK simulations, `try_*` clients surface typed traps as contract errors. For example,
`FundingAmountNotPositive` is observable as `ContractError(100)` / `Error(Contract, #100)`.

Recommended SDK mappings:

| Codes | Suggested client category |
| --- | --- |
| 1-13 | Invalid initialization or pricing configuration |
| 20-22 | Missing initialized escrow metadata |
| 30-42 | Dust sweep or token integration failure |
| 50-62 | Attestation or collateral metadata failure |
| 70-80 | Administrative validation failure |
| 90-92 | Migration failure |
| 100-110 | Funding failure |
| 120-129 | Settlement, withdrawal, or investor payout failure |
| 140-143 | Cancellation or refund failure |
| 150-152 | Legal hold clear scheduling failure |
| 160-162 | Beneficiary rotation failure |

## Security Notes

- Auth boundaries from ADR-002 remain unchanged. Typed errors do not replace `require_auth`.
- Overflow-sensitive paths use checked arithmetic and map each overflow to a stable code.
- Dust sweep and refund transfers keep balance-delta checks at the external token boundary.
- Cancelled escrow dust sweeps must preserve the outstanding refund liability floor.
- Refund uses checks-effects-interactions by zeroing contribution before transfer to prevent
  double-spend. Investor payout remains idempotent after the claim marker is written.
- Storage TTL behavior is unchanged by the error migration; `bump_ttl` still extends contract
  instance storage and persistent allowlist entries.
