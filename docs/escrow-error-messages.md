# Liquifact Escrow Typed Error Codes

LiquiFact escrow emits typed Soroban contract errors through `EscrowError`. Clients should branch
on the numeric `ContractError(code)` value, not on panic strings or diagnostic text.

## Stability Policy

Error codes are append-only. Once a code is assigned, it must not be renamed for a different
meaning, reused, or renumbered. New failures must receive new codes after the existing range.

Legacy panic messages are listed only to help integrators migrate old simulations and logs.

## Canonical Code Table

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
| 42 | `SweepExceedsLiabilityFloor` | `sweep would reduce balance below outstanding investor liabilities` |
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
| 150 | `LegalHoldClearRequestMissing` | `clear_legal_hold called before request_legal_hold_clear` |
| 151 | `LegalHoldClearNotReady` | `legal-hold clear delay has not elapsed` |
| 152 | `LegalHoldClearDelayOverflow` | `legal-hold clear ready-at timestamp overflow` |
| 160 | `LegalHoldBlocksBeneficiaryRotation` | `legal hold blocks beneficiary rotation` |
| 161 | `RotationNotOpen` | `beneficiary rotation only allowed before settlement` |
| 162 | `NewSmeSameAsCurrent` | `new SME address must differ from current beneficiary` |

## Range grouping

The numeric ranges are intentionally sparse so new variants can be appended without renumbering existing codes:

| Codes | Group | Notes |
| --- | --- | --- |
| 1-13 | Initialization and invoice configuration | `init` validation, invoice id validation, contribution caps, yield tiers |
| 20-22 | Missing initialized metadata | Uninitialized escrow, missing funding token, missing treasury |
| 30-42 | Terminal dust sweep and token transfer boundary | Legal hold, sweep caps, token balance/delta checks, liability floor |
| 50-51 | Attestations | Primary digest single-write and append-log capacity |
| 60-62 | SME collateral metadata | Positive amount, non-empty asset, monotonic timestamps |
| 70-80 | Administrative validation | Investor allowlist batch, target/cap/maturity/admin updates |
| 90-92 | Schema migration | Version mismatch, already-current version, missing migration path |
| 100-110 | Funding | Funding amount, min contribution, legal hold, allowlist, caps, overflow |
| 120-129 | Settlement, withdrawal, investor claims | Maturity, funded/settled state, legal hold, payout overflow |
| 140-143 | Cancellation and refund | Open/cancelled state checks and refund availability |
| 150-152 | Two-phase legal-hold clearing | Missing request, not-ready delay, timestamp overflow |
| 160-162 | Beneficiary rotation | Legal hold and pre-settlement rotation constraints |

## Entrypoint and client handling matrix

| Code | Variant | Entrypoint(s) | Trigger condition | Recommended client action |
| ---: | --- | --- | --- | --- |
| 1 | `AmountMustBePositive` | `init` | Initial invoice amount is zero or negative. | Reject form input before submission. |
| 2 | `YieldBpsOutOfRange` | `init` | Base yield is outside `0..=10_000` bps. | Clamp or reject yield configuration. |
| 3 | `EscrowAlreadyInitialized` | `init` | Contract storage already has an escrow. | Treat init as non-idempotent; do not retry without redeploying. |
| 4 | `InvoiceIdInvalidLength` | `init` | Invoice id length is outside the allowed range. | Validate invoice ids client-side. |
| 5 | `InvoiceIdInvalidCharset` | `init` | Invoice id contains unsupported characters. | Use the documented invoice id charset. |
| 6 | `MinContributionNotPositive` | `init` | Optional minimum contribution is zero or negative. | Reject invalid minimum contribution. |
| 7 | `MinContributionExceedsAmount` | `init` | Minimum contribution exceeds target amount. | Lower the minimum or raise the target. |
| 8 | `MaxUniqueInvestorsNotPositive` | `init` | Optional investor-count cap is zero or negative. | Remove the cap or set a positive value. |
| 9 | `MaxPerInvestorNotPositive` | `init` | Optional per-investor cap is zero or negative. | Remove the cap or set a positive value. |
| 10 | `TierYieldOutOfRange` | `init` | A tier yield is outside `0..=10_000` bps. | Reject the tier table. |
| 11 | `TierYieldBelowBase` | `init` | A tier yield is below the base yield. | Raise tier yield or lower base yield. |
| 12 | `TierLockNotIncreasing` | `init` | Tier lock durations are not strictly increasing. | Sort and validate tiers before submission. |
| 13 | `TierYieldNotNonDecreasing` | `init` | Tier yields decrease as lock durations increase. | Make tier yields non-decreasing. |
| 20 | `EscrowNotInitialized` | read/admin/migration/funding helpers | Entrypoint requires existing escrow state. | Refresh state; initialize or select a valid escrow. |
| 21 | `FundingTokenNotSet` | `get_funding_token`, sweep/refund internals | Funding token address is missing. | Treat as deployment/configuration error. |
| 22 | `TreasuryNotSet` | `get_treasury`, `sweep_terminal_dust` | Treasury address is missing. | Treat as deployment/configuration error. |
| 30 | `LegalHoldBlocksTreasuryDustSweep` | `sweep_terminal_dust` | Legal hold is active. | Surface compliance hold; retry only after hold clears. |
| 31 | `SweepAmountNotPositive` | `sweep_terminal_dust` | Sweep amount is zero or negative. | Reject amount input. |
| 32 | `SweepAmountExceedsMax` | `sweep_terminal_dust` | Sweep amount exceeds `MAX_DUST_SWEEP_AMOUNT`. | Split or reduce sweep amount. |
| 33 | `DustSweepNotTerminal` | `sweep_terminal_dust` | Escrow is not in a terminal state. | Wait for settlement/cancellation before sweeping. |
| 34 | `NoFundingTokenBalanceToSweep` | `sweep_terminal_dust` | Contract has no funding-token balance. | Hide or disable sweep action. |
| 35 | `EffectiveSweepAmountZero` | `sweep_terminal_dust` | Effective sweep after caps is zero. | Recompute sweepable dust before submitting. |
| 36 | `TransferAmountNotPositive` | token transfer boundary | Internal transfer amount is invalid. | Treat as invariant/configuration failure. |
| 37 | `InsufficientTokenBalanceBeforeTransfer` | token transfer boundary | Sender balance is insufficient before transfer. | Refresh balances and prevent retry until funded. |
| 38 | `SenderBalanceUnderflow` | token transfer boundary | Sender balance delta underflowed. | Treat as token integration failure. |
| 39 | `RecipientBalanceUnderflow` | token transfer boundary | Recipient balance delta underflowed. | Treat as token integration failure. |
| 40 | `SenderBalanceDeltaMismatch` | token transfer boundary | Sender balance delta does not equal transfer amount. | Treat as malicious/incompatible token behavior. |
| 41 | `RecipientBalanceDeltaMismatch` | token transfer boundary | Recipient balance delta does not equal transfer amount. | Treat as malicious/incompatible token behavior. |
| 42 | `SweepExceedsLiabilityFloor` | `sweep_terminal_dust` | Sweep would reduce contract balance below outstanding liabilities. | Recompute liabilities and sweep only available dust. |
| 50 | `PrimaryAttestationAlreadyBound` | `bind_primary_attestation_hash` | Primary digest is already set. | Show immutable attestation state; do not retry. |
| 51 | `AttestationAppendLogCapacityReached` | `append_attestation_digest` | Append log reached its max capacity. | Stop appending and use off-chain archive/reference. |
| 60 | `CollateralAmountNotPositive` | `record_sme_collateral_commitment` | Collateral amount is zero or negative. | Reject collateral input. |
| 61 | `CollateralAssetEmpty` | `record_sme_collateral_commitment` | Collateral asset symbol is empty. | Require an asset symbol. |
| 62 | `CollateralTimestampBackwards` | `record_sme_collateral_commitment` | New collateral timestamp is older than the stored timestamp. | Refresh state and submit a monotonic timestamp. |
| 70 | `InvestorBatchEmpty` | allowlist batch update | Investor vector is empty. | Disable empty batch submissions. |
| 71 | `InvestorBatchTooLarge` | allowlist batch update | Investor vector exceeds max batch size. | Split the batch. |
| 72 | `TargetNotPositive` | target update | New target is zero or negative. | Reject target input. |
| 73 | `TargetUpdateNotOpen` | target update | Target update attempted outside open state. | Refresh escrow state and hide action after funding. |
| 74 | `TargetBelowFundedAmount` | target update | New target is below already-funded amount. | Set target at or above funded amount. |
| 75 | `CapLowerNotOpen` | investor cap lowering | Cap lowering attempted outside open state. | Only allow cap updates while open. |
| 76 | `NoInvestorCapConfigured` | investor cap lowering | No cap exists to lower. | Hide cap-lowering action. |
| 77 | `NewCapNotLower` | investor cap lowering | New cap is not lower than current cap. | Require a strictly lower cap. |
| 78 | `NewCapBelowCurrentFunderCount` | investor cap lowering | New cap is below current unique funder count. | Set cap at least to current funder count. |
| 79 | `MaturityUpdateNotOpen` | maturity update | Maturity update attempted outside open state. | Only allow maturity updates while open. |
| 80 | `NewAdminSameAsCurrent` | admin rotation | New admin equals current admin. | Require a distinct admin address. |
| 90 | `MigrationVersionMismatch` | `migrate` | Supplied `from_version` does not match stored version. | Refresh version and retry with the stored value. |
| 91 | `AlreadyCurrentSchemaVersion` | `migrate` | Stored version is already current. | Treat migration as unnecessary. |
| 92 | `NoMigrationPath` | `migrate` | No migration branch exists for the version. | Redeploy or implement an explicit migration. |
| 100 | `FundingAmountNotPositive` | `fund`, `fund_with_commitment` | Funding amount is zero or negative. | Reject amount input. |
| 101 | `FundingBelowMinContribution` | `fund`, `fund_with_commitment` | Amount is below min contribution. | Raise amount to the displayed minimum. |
| 102 | `LegalHoldBlocksFunding` | `fund`, `fund_with_commitment` | Legal hold is active. | Surface compliance hold and block funding. |
| 103 | `EscrowNotOpenForFunding` | `fund`, `fund_with_commitment` | Escrow status is not open. | Refresh state and disable funding. |
| 104 | `InvestorNotAllowlisted` | `fund`, `fund_with_commitment` | Investor is not on the allowlist. | Ask investor to complete allowlist/KYC flow. |
| 105 | `InvestorContributionOverflow` | `fund`, `fund_with_commitment` | Contribution arithmetic overflowed. | Treat as invariant failure; do not retry blindly. |
| 106 | `InvestorContributionExceedsCap` | `fund`, `fund_with_commitment` | Investor total exceeds per-investor cap. | Show remaining per-investor capacity. |
| 107 | `UniqueInvestorCapReached` | `fund`, `fund_with_commitment` | Unique investor cap is already reached. | Disable new-investor funding. |
| 108 | `TieredSecondDeposit` | `fund_with_commitment` | Investor already used tiered first deposit. | Use `fund()` for additional principal. |
| 109 | `InvestorClaimTimeOverflow` | `fund_with_commitment` | Lock expiry timestamp overflowed. | Reject excessive lock duration. |
| 110 | `FundedAmountOverflow` | `fund`, `fund_with_commitment` | Funded amount arithmetic overflowed. | Treat as invariant failure. |
| 120 | `LegalHoldBlocksSettlement` | `settle` | Legal hold is active. | Block settlement until hold clears. |
| 121 | `SettlementNotFunded` | `settle` | Escrow is not funded. | Wait until target is funded. |
| 122 | `MaturityNotReached` | `settle` | Current ledger timestamp is before maturity. | Show maturity time and retry later. |
| 123 | `LegalHoldBlocksWithdrawal` | `withdraw` | Legal hold is active. | Block SME withdrawal until hold clears. |
| 124 | `WithdrawalNotFunded` | `withdraw` | Escrow is not funded. | Disable withdrawal until funded. |
| 125 | `LegalHoldBlocksInvestorClaims` | `claim_investor_payout` | Legal hold is active. | Block claims until hold clears. |
| 126 | `NoContributionToClaim` | `claim_investor_payout` | Investor has no recorded contribution. | Hide claim action for the address. |
| 127 | `InvestorClaimNotSettled` | `claim_investor_payout` | Escrow is not settled. | Wait for settlement. |
| 128 | `InvestorCommitmentLockNotExpired` | `claim_investor_payout` | Investor lock has not expired. | Show claim unlock time. |
| 129 | `ComputePayoutArithmeticOverflow` | payout computation | Checked arithmetic overflowed. | Treat as invariant failure and escalate. |
| 140 | `LegalHoldBlocksCancelFunding` | `cancel_funding` | Legal hold is active. | Block cancellation until hold clears. |
| 141 | `CancelFundingNotOpen` | `cancel_funding` | Escrow is not open. | Refresh state and hide cancellation. |
| 142 | `RefundNotCancelled` | `refund` | Escrow is not cancelled. | Only allow refunds after cancellation. |
| 143 | `NoContributionToRefund` | `refund` | Investor has no refundable contribution. | Hide refund action. |
| 150 | `LegalHoldClearRequestMissing` | `clear_legal_hold` | No prior clear request exists. | Require `request_legal_hold_clear` first. |
| 151 | `LegalHoldClearNotReady` | `clear_legal_hold` | Clear delay has not elapsed. | Show ready-at time and retry later. |
| 152 | `LegalHoldClearDelayOverflow` | legal-hold clear scheduling | Ready-at timestamp overflowed. | Treat as invariant failure. |
| 160 | `LegalHoldBlocksBeneficiaryRotation` | beneficiary rotation | Legal hold is active. | Block rotation until hold clears. |
| 161 | `RotationNotOpen` | beneficiary rotation | Escrow is not open or funded pre-settlement. | Refresh state and hide rotation. |
| 162 | `NewSmeSameAsCurrent` | beneficiary rotation | New SME address equals current SME. | Require a distinct beneficiary address. |

## Client Guidance

In tests and SDK simulations, `try_*` clients surface typed traps as contract errors. For example,
`FundingAmountNotPositive` is observable as `ContractError(100)` / `Error(Contract, #100)`.

Recommended SDK mappings:

| Codes | Suggested client category |
| --- | --- |
| 1-13 | Invalid initialization or pricing configuration |
| 20-22 | Missing initialized escrow metadata |
| 30-41 | Dust sweep or token integration failure |
| 50-62 | Attestation or collateral metadata failure |
| 70-80 | Administrative validation failure |
| 90-92 | Migration failure |
| 100-110 | Funding failure |
| 120-129 | Settlement, withdrawal, or investor payout failure |
| 140-143 | Cancellation or refund failure |

## Security Notes

- Auth boundaries from ADR-002 remain unchanged. Typed errors do not replace `require_auth`.
- Overflow-sensitive paths use checked arithmetic and map each overflow to a stable code.
- Dust sweep and refund transfers keep balance-delta checks at the external token boundary.
- Refund uses checks-effects-interactions by zeroing contribution before transfer to prevent
  double-spend. Investor payout remains idempotent after the claim marker is written.
- Storage TTL behavior is unchanged by the error migration; `bump_ttl` still extends contract
  instance storage and persistent allowlist entries.
