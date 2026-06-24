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
| 163 | `FundingDeadlinePassed` | `Funding deadline passed` |
| 164 | `NoPendingAdmin` | `No pending admin` |

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
