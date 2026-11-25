# Escrow Contract Error Messages

This document contains reference codes for typed `EscrowError` values emitted by the LiquiFact escrow smart contract.

## Settlement & Bounds Errors

Codes raised by the settlement-family entrypoints (`partial_settle`, `settle`, `withdraw`,
`claim_investor_payout`). For the exact condition that triggers each one, guard ordering, and how
to avoid it, see [`docs/settlement-errors.md`](settlement-errors.md) — this table is a stable
code/name summary only.

| Error Name | Code | Description |
|---|---|---|
| `PartialSettleUnauthorizedCaller` | 200 | `caller` passed to `partial_settle` is neither the SME address nor the admin. |
| `LegalHoldBlocksPartialSettle` | 201 | A legal hold is active when `partial_settle` is called. |
| `PartialSettleNotOpen` | 202 | `partial_settle` called while escrow status is not `Open` (0). |
| `LegalHoldBlocksSettlement` | 120 | A legal hold is active when `settle` is called. |
| `SettlementNotFunded` | 121 | `settle` called while the escrow is not in the `Funded` state (1) and not already `Settled` (2). For an already-settled escrow see `EscrowAlreadySettled` (236). |
| `EscrowAlreadySettled` | 236 | `settle` (or a `settle_batch` entry) called on an escrow already in the `Settled` state (2). Settlement is strictly once-only. |
| `MaturityNotReached` | 122 | `settle` called before the configured maturity timestamp. |
| `LegalHoldBlocksWithdrawal` | 123 | A legal hold is active when `withdraw` is called. |
| `WithdrawalNotFunded` | 124 | `withdraw` called before the escrow reached `Funded` status (1). |
| `LegalHoldBlocksInvestorClaims` | 125 | A legal hold is active when `claim_investor_payout` is called. |
| `NoContributionToClaim` | 126 | `claim_investor_payout` called for an investor with zero recorded contribution. |
| `InvestorClaimNotSettled` | 127 | `claim_investor_payout` called before the escrow reached `Settled` status (2). |
| `InvestorCommitmentLockNotExpired` | 128 | `claim_investor_payout` called before the investor's tiered-commitment lock expires. |
| `ComputePayoutArithmeticOverflow` | 129 | Checked arithmetic overflowed while computing the settlement pool or investor payout. |
| `InsufficientContractBalance` | 165 | The contract's funding-token balance is below `funded_amount` at `withdraw` time. |
| `PayoutZero` | 170 | The computed investor payout is zero. |
| `PausedBlocksSettlement` | 211 | The operational pause is active when `settle` (or `repay`) is called. |
| `PausedBlocksWithdrawal` | 212 | The operational pause is active when `withdraw` is called. |
| `PausedBlocksInvestorClaims` | 213 | The operational pause is active when `claim_investor_payout` is called. |
| `WithdrawFeeArithmeticOverflow` | 216 | `funded_amount * fee_bps` overflowed `i128` while computing the protocol fee at `withdraw`. |
| `WithdrawNetArithmeticUnderflow` | 217 | `funded_amount - fee` underflowed while computing the net SME payout at `withdraw`. |

## Cross-Contract Callback Errors

Codes raised by cross-contract callback entrypoints (`register_callback`, `execute_callback`).

| Error Name | Code | Description |
|---|---|---|
| `CallbackWrongOrigin` | 240 | Callback executed from an origin address different from the registered origin context. |
| `CallbackWrongNonce` | 241 | Callback executed with an invocation nonce that does not match the registered context. |
| `CallbackWrongPhase` | 242 | Callback executed with a lifecycle phase different from the expected phase. |
| `CallbackReplayed` | 243 | Callback execution attempted on an already-consumed callback context (replay attempt). |
| `CallbackAfterCancellation` | 244 | Callback registration or execution attempted on a cancelled escrow (`status == 4`). |
| `CallbackNotFound` | 245 | Callback execution attempted with a nonce that has no registered context in storage. |

## Fee-Schedule Errors

| Error Name | Code | Description |
|---|---:|---|
| `FeeScheduleOutOfBounds` | 289 | `submit_fee_schedule` received bounds that do not contain `fee_bps`, or a maximum above 10,000 basis points. |
| `FeeScheduleActivationInvalid` | 290 | The activation ledger is not strictly future or is more than 2,592,000 ledgers (about 30 days at one ledger/second) ahead. |
| `FeeSchedulePendingExists` | 291 | A different pending schedule already exists. An identical submission is a successful no-op. |
| `FeeScheduleNotPending` | 292 | `cancel_pending_fee_schedule` was called without a pending schedule. |

`cancel_pending_fee_schedule` is administrator-only and returns the removed schedule. Fee-schedule
events are `FeeScheduleSubmitted` (`name`, `invoice_id`, `fee_bps`, `activation_ledger`),
`FeeScheduleActivated` (`name`, `invoice_id`, `fee_bps`), and `FeeScheduleCancelled` (`name`,
`invoice_id`, `fee_bps`). Activation is lazy when a later submission encounters a matured pending
schedule, as well as explicit through `activate_fee_schedule`. `FeeScheduleError` remains in the
contract specification only for compatibility and is deprecated; fee-schedule entrypoints now use
the `EscrowError` codes above. Scheduled fees are not used by `withdraw`, which continues to use
`ProtocolFeeBps`.

## Initialization Errors

Codes raised by `init` (and the shared validators it calls) before any state is written.

| Error Name | Code | Description |
|---|---:|---|
| `AmountMustBePositive` | 1 | `init` received a non-positive invoice `amount`. |
| `YieldBpsOutOfRange` | 2 | `yield_bps` is outside `0..=10_000`; raised by `init` and by `update_yield_bps`. |
| `EscrowAlreadyInitialized` | 3 | `init` called when escrow storage already exists (any second initialization attempt). Existing state is left unchanged. |
| `InvoiceIdInvalidLength` | 4 | `invoice_id` length is outside `1..=MAX_INVOICE_ID_STRING_LEN`. |
| `InvoiceIdInvalidCharset` | 5 | `invoice_id` contains a character other than ASCII letters, digits, or `_`. |
| `MinContributionNotPositive` | 6 | `init` configured a `min_contribution` that is not positive. |
| `MinContributionExceedsAmount` | 7 | `init` configured a `min_contribution` above the invoice amount (target hint). |
| `MaxUniqueInvestorsNotPositive` | 8 | `init` configured a `max_unique_investors` cap that is not positive. |
| `MaxPerInvestorNotPositive` | 9 | `init` configured a `max_per_investor` cap that is not positive. |
| `TierYieldOutOfRange` | 10 | A yield tier has `yield_bps` outside `0..=10_000`. |
| `TierYieldBelowBase` | 11 | A yield tier's `yield_bps` is below the escrow's base `yield_bps`. |
| `TierLockNotIncreasing` | 12 | Yield tiers' `min_lock_secs` are not strictly increasing. |
| `TierYieldNotNonDecreasing` | 13 | Yield tiers' `yield_bps` decrease from one tier to the next. |
| `AmountExceedsMax` | 14 | `init` received an invoice `amount` above `MAX_INVOICE_AMOUNT`, the bound that keeps payout arithmetic overflow-free. |
| `ProtocolFeeBpsOutOfRange` | 215 | `protocol_fee_bps` is outside `0..=10_000`; raised by `init` and by `set_protocol_fee_bps`. |
| `InitAddressInvalid` | 294 | `init` received a `funding_token`, `treasury`, or `sme_address` equal to the escrow contract's own address. |
| `InitRoleConflict` | 295 | `init` received `treasury == sme_address`; the treasury and the beneficiary must be distinct. |

## Storage & Configuration Errors

Codes raised when required instance storage is missing.

| Error Name | Code | Description |
|---|---:|---|
| `EscrowNotInitialized` | 20 | Escrow storage is missing; the entrypoint requires a prior successful `init`. |
| `FundingTokenNotSet` | 21 | The funding-token address (`DataKey::FundingToken`) is unset. |
| `TreasuryNotSet` | 22 | The treasury address (`DataKey::Treasury`) is unset. |

## Treasury Dust Sweep & Token Transfer Errors

Codes raised by `sweep_terminal_dust` and by the SEP-41 transfer wrappers in `external_calls` (used by `withdraw`, `claim_investor_payout`, refunds, repayment, and the sweep). See [`docs/escrow-token-safety.md`](escrow-token-safety.md) for the transfer-wrapper guards in detail.

| Error Name | Code | Description |
|---|---:|---|
| `LegalHoldBlocksTreasuryDustSweep` | 30 | A legal hold is active when `sweep_terminal_dust` is called. |
| `SweepAmountNotPositive` | 31 | `sweep_terminal_dust` received a non-positive amount. |
| `SweepAmountExceedsMax` | 32 | `sweep_terminal_dust` amount exceeds `MAX_DUST_SWEEP_AMOUNT` (100,000,000). |
| `DustSweepNotTerminal` | 33 | `sweep_terminal_dust` called before the escrow reached a terminal status. |
| `NoFundingTokenBalanceToSweep` | 34 | `sweep_terminal_dust` found a zero funding-token balance on the contract. |
| `EffectiveSweepAmountZero` | 35 | `sweep_terminal_dust` computed an effective sweep amount (requested amount capped by balance) of zero. |
| `TransferAmountNotPositive` | 36 | A token transfer wrapper received a non-positive amount. |
| `InsufficientTokenBalanceBeforeTransfer` | 37 | A token transfer wrapper found the sender's balance below the amount before transferring. |
| `SenderBalanceUnderflow` | 38 | A token transfer wrapper's sender balance-delta computation underflowed. |
| `RecipientBalanceUnderflow` | 39 | A token transfer wrapper's recipient balance-delta computation underflowed. |
| `SenderBalanceDeltaMismatch` | 40 | The sender's observed balance decrease differs from the requested transfer amount. |
| `RecipientBalanceDeltaMismatch` | 41 | The recipient's observed balance increase differs from the requested transfer amount. |
| `SweepExceedsLiabilityFloor` | 42 | The sweep would leave the contract balance below outstanding investor liabilities (`funded_amount - distributed_principal`). |

## Attestation Errors

Codes raised by the attestation entrypoints. See [`docs/attestation-errors.md`](attestation-errors.md) for guard ordering.

| Error Name | Code | Description |
|---|---:|---|
| `PrimaryAttestationAlreadyBound` | 50 | `bind_primary_attestation_hash` called when a primary hash is already bound. |
| `AttestationAppendLogCapacityReached` | 51 | Appending a digest would exceed the configured attestation limit (`get_attestation_limit`). |
| `AttestationIndexOutOfRange` | 52 | A revoke/unrevoke call received an `index >= log.len()`. |
| `AttestationAlreadyRevoked` | 53 | `revoke_attestation_digest` or `revoke_attestation_digests` targeted an index that is already revoked. |
| `AttestationBatchEmpty` | 54 | `revoke_attestation_digests` received an empty indices list. |
| `AttestationBatchTooLarge` | 55 | `revoke_attestation_digests` exceeded `MAX_ATTESTATION_REVOKE_BATCH` (32). |
| `AttestationNotRevoked` | 56 | `unrevoke_attestation_digest` targeted an index that is not revoked. |
| `AttestationReadLimitZero` | 57 | `get_revoked_attestation_digests` received a page limit of zero. |
| `AttestationReadLimitTooLarge` | 58 | `get_revoked_attestation_digests` page limit exceeds `MAX_ATTESTATION_READ_PAGE` (20). |
| `AttestationRequirementNotOpen` | 250 | `set_require_attest_withdraw` called while the escrow is not open (`status != 0`). |
| `AttestationRequiredForWithdraw` | 251 | `withdraw` blocked: attestation is required for withdrawal and no primary attestation hash is bound. |
| `NotAttestor` | 269 | `append_attestation_digest_as` called by an address that is not the designated attestor. |
| `AttestationDigestDuplicate` | 270 | The appended digest already exists as the primary attestation or in the append log. |
| `AttestationLimitOutOfRange` | 312 | `set_attestation_limit` received a limit outside `MIN_ATTESTATION_LIMIT..=MAX_ATTESTATION_LIMIT`. |

## SME Collateral Errors

Codes raised by the SME collateral commitment entrypoints.

| Error Name | Code | Description |
|---|---:|---|
| `CollateralAmountNotPositive` | 60 | `record_sme_collateral_commitment` / `batch_record_collateral` received a non-positive amount. |
| `CollateralAssetEmpty` | 61 | A collateral record has an empty asset symbol. |
| `CollateralTimestampBackwards` | 62 | A collateral record's timestamp is earlier than the stored record's. |
| `CollateralBatchEmpty` | 63 | `batch_record_collateral` received an empty items vector. |
| `CollateralBatchTooLarge` | 64 | `batch_record_collateral` exceeded `MAX_COLLATERAL_BATCH` (50). |
| `NoCollateralToClear` | 169 | `clear_sme_collateral_commitment` called when no commitment is recorded. |
| `CollateralLockedAfterFunding` | 303 | `clear_sme_collateral_commitment` blocked: the escrow has investor principal and is not in a terminal state. |
| `LegalHoldBlocksCollateralChange` | 304 | A legal hold is active when recording or clearing the collateral commitment. |
| `CollateralAssetChangeAfterFunding` | 305 | A collateral record tried to change the asset after funding began. |
| `CollateralDecreaseAfterFunding` | 306 | A collateral record tried to decrease the amount after funding began. |
| `CollateralBatchMixedAssets` | 307 | `batch_record_collateral` entries reference different assets. |

## Allowlist & Investor Batch Errors

| Error Name | Code | Description |
|---|---:|---|
| `InvestorBatchEmpty` | 70 | `set_investors_allowlisted`, `set_investors_allowlist_expiry`, or `set_investors_frozen` received an empty batch. |
| `InvestorBatchTooLarge` | 71 | `set_investors_allowlisted`, `set_investors_allowlist_expiry`, or `set_investors_frozen` exceeded `MAX_INVESTOR_ALLOWLIST_BATCH` (32). |
| `AllowlistExpiryInPast` | 281 | `set_investor_allowlist_expiry` / `set_investors_allowlist_expiry` received an expiry in the past. |
| `InvestorAllowlistExpired` | 282 | Funding attempted by an investor whose allowlist entry has expired. |
| `AllowlistSlotCorrupted` | 308 | The persistent allowlist index is inconsistent (missing slot or zero count) while updating an allowlist entry. |

## Funding Errors

Codes raised by `fund`, `fund_with_commitment`, `fund_with_reference`, `fund_batch`, and related reads.

| Error Name | Code | Description |
|---|---:|---|
| `FundingBatchEmpty` | 82 | `fund_batch` received an empty entries vector. |
| `FundingBatchTooLarge` | 83 | `fund_batch` exceeded `MAX_FUND_BATCH` (50). |
| `FundingBatchDuplicateInvestor` | 84 | `fund_batch` contains the same investor more than once; the whole batch is rejected. |
| `FundingAmountNotPositive` | 100 | A funding call received a non-positive amount. |
| `FundingBelowMinContribution` | 101 | The funding amount is below the configured `min_contribution`. |
| `LegalHoldBlocksFunding` | 102 | A legal hold is active when funding. |
| `EscrowNotOpenForFunding` | 103 | Funding attempted while the escrow is not open; also raised by `raise_max_unique_investors` on a non-open escrow. |
| `InvestorNotAllowlisted` | 104 | The allowlist gate is active and the investor is not allowlisted. |
| `InvestorContributionOverflow` | 105 | Adding the amount would overflow the investor's stored contribution. |
| `InvestorContributionExceedsCap` | 106 | The investor's total contribution would exceed `max_per_investor`. |
| `UniqueInvestorCapReached` | 107 | A new investor would exceed `max_unique_investors`. |
| `TieredSecondDeposit` | 108 | `fund_with_commitment` called by an investor who already has principal; use `fund` for follow-on deposits. |
| `InvestorClaimTimeOverflow` | 109 | Computing the investor's claim-not-before timestamp (`now + committed_lock_secs`) overflowed. |
| `FundedAmountOverflow` | 110 | Adding the amount would overflow `funded_amount`; also raised if the callback nonce overflows in `register_callback`. |
| `CommitmentLockExceedsMaturity` | 111 | `now + committed_lock_secs` would fall after the escrow maturity. |
| `FundingDeadlinePassed` | 164 | The funding deadline has passed: `fund` after the deadline, `init` with a deadline not in the future, or `extend_funding_deadline` after the current deadline. |
| `ContributionReadBatchTooLarge` | 203 | `get_contributions` exceeded `MAX_INVESTOR_READ_BATCH` (50). |
| `PausedBlocksFunding` | 210 | The operational pause (funding scope) is active when funding or calling `partial_settle`. |
| `FundedAmountExceedsSafeEnvelope` | 268 | The resulting `funded_amount` would exceed `MAX_INVOICE_AMOUNT`. |
| `YieldWeightOverflow` | 283 | Checked arithmetic on the yield-weighted principal overflowed or went negative (`fund`, `unfund`, settlement pool aggregation). |
| `ContributionReferenceReused` | 293 | `fund_with_reference` received a `reference` already used by a prior contribution. |
| `InvestorRoleConflict` | 296 | The investor is the escrow's `sme_address`, `admin`, `treasury`, or the contract itself. |
| `FundingProgressArithmeticOverflow` | 316 | Checked arithmetic overflowed in `get_funding_progress_bps`. |
| `ZeroContributionAmount` | 317 | `preview_yield_tier` received a non-positive `amount`. |
| `FundingNotExpired` | 318 | `expire_funding` called when no deadline is set or `now <= deadline`. |
| `AttestationAppendBatchEmpty` | 313 | `append_attestation_digests` received an empty digests vector. |
| `AttestationAppendBatchTooLarge` | 314 | `append_attestation_digests` exceeded `MAX_ATTESTATION_APPEND_BATCH`. |

## Admin Parameter Update Errors

Codes raised by admin setters that adjust caps, targets, maturity, deadlines, and fees while the escrow is open.

| Error Name | Code | Description |
|---|---:|---|
| `MaxPerInvestorCapNotConfigured` | 24 | `raise_max_per_investor` called with no per-investor cap set at `init`. |
| `MaxPerInvestorCapNotRaised` | 25 | `raise_max_per_investor` did not strictly raise the cap (`new_cap <= current_cap`). |
| `TargetNotPositive` | 72 | `update_funding_target` received a non-positive target. |
| `TargetUpdateNotOpen` | 73 | `update_funding_target` called while the escrow is not open. |
| `TargetBelowFundedAmount` | 74 | `update_funding_target` set the target below the already-funded principal. |
| `CapLowerNotOpen` | 75 | `lower_max_unique_investors` or `raise_max_per_investor` called while the escrow is not open. |
| `NoInvestorCapConfigured` | 76 | `lower_max_unique_investors` or `raise_max_unique_investors` called with no unique-investor cap configured. |
| `NewCapNotLower` | 77 | `lower_max_unique_investors` did not strictly lower the cap. |
| `NewCapBelowCurrentFunderCount` | 78 | `lower_max_unique_investors` set the cap below the current unique funder count. |
| `MaturityUpdateNotOpen` | 79 | `update_maturity` called while the escrow is not open. |
| `MaturityUnchanged` | 81 | `update_maturity` set maturity to its current value. |
| `MaturityInPast` | 166 | The maturity timestamp is before the current ledger time (`init`, `update_maturity`). |
| `MaturityExceedsMaxHorizon` | 167 | The maturity timestamp is beyond `now + maturity max horizon` (`init`, `update_maturity`). |
| `FundingDeadlineUpdateNotOpen` | 171 | `extend_funding_deadline` called while the escrow is not open (`status != 0`). |
| `FloorLowerNotOpen` | 173 | `lower_min_contribution_floor` called while the escrow is not open. |
| `NewFloorNotLower` | 174 | `lower_min_contribution_floor` did not strictly lower the floor. |
| `NewFloorNotPositive` | 175 | `lower_min_contribution_floor` received a non-positive floor. |
| `NewCapNotHigher` | 176 | `raise_max_unique_investors` did not strictly raise the cap. |
| `FundingDeadlineBeyondMaturity` | 204 | `init` received a `funding_deadline` at or after a non-zero maturity. |
| `FundingDeadlineNotSet` | 205 | Reserved: no funding deadline is configured. Not currently returned; `extend_funding_deadline` reports a missing deadline as `FundingDeadlineNotExtended` (206). |
| `FundingDeadlineNotExtended` | 206 | `extend_funding_deadline` did not strictly extend the stored deadline, or no deadline is configured. |
| `HorizonNotRaised` | 214 | `raise_maturity_max_horizon` received a horizon not strictly greater than the current one. |
| `FundingDeadlineAtOrAfterMaturity` | 218 | `extend_funding_deadline` would place the deadline at or after a non-zero maturity. |
| `YieldBpsUpdateNotOpen` | 228 | `update_yield_bps` called while the escrow is not open. |
| `YieldBpsUnchanged` | 229 | `update_yield_bps` received the current value. |
| `RegistryImmutableAfterFunding` | 247 | `rebind_registry_ref` called after investor principal was recorded (`funded_amount > 0`). |
| `FundingDeadlineExtensionLimitReached` | 252 | `extend_funding_deadline` called after `MAX_FUNDING_DEADLINE_EXTENSIONS` (3) extensions. |
| `FundingDeadlineExtensionTooLong` | 253 | `extend_funding_deadline` total added time would exceed `MAX_FUNDING_DEADLINE_TOTAL_EXTENSION_SECS` (30 days). |
| `MaturityHorizonOutOfRange` | 259 | A maturity horizon update is outside the inclusive one-day to ten-year range. |
| `MaturityHorizonUpdateNotOpen` | 260 | A maturity horizon update was attempted while the escrow is not open. |
| `MaturityHorizonUnchanged` | 261 | `update_maturity_max_horizon` received the current horizon value. |
| `MaturityHorizonBelowMaturity` | 262 | The requested horizon would place the configured maturity beyond `now + horizon`. |
| `PartialCloseMinBpsOutOfRange` | 265 | `set_partial_close_min_bps` received `bps > 10_000`. |
| `TargetExceedsInvoiceAmount` | 267 | `update_funding_target` set the target above the invoice face value (`amount`). |
| `PerformanceFeeBpsOutOfRange` | 301 | `set_performance_fee_bps` received a fee outside `0..=5_000`. |
| `PerformanceFeeUpdateNotOpen` | 302 | `set_performance_fee_bps` called after funding began (`status != 0` or `funded_amount > 0`). |

## Admin Handover Errors

Codes raised by the two-step admin transfer and admin recovery flows.

| Error Name | Code | Description |
|---|---:|---|
| `NewAdminSameAsCurrent` | 80 | `propose_admin` nominated the current admin. |
| `AdminProposalExpired` | 85 | `accept_admin` called after the pending proposal expired; re-propose. |
| `NoPendingAdmin` | 172 | `accept_admin`, `cancel_pending_admin`, or `recover_admin` called with no pending admin. |
| `PendingAdminUnchanged` | 177 | `propose_admin` repeated the address that is already pending. |
| `AdminRecoveryNotExpired` | 249 | `recover_admin` called before the admin-recovery timelock deadline. |
| `AdminProposalWindowOutOfRange` | 286 | `propose_admin` validity window is outside `MIN_ADMIN_PROPOSAL_VALIDITY_SECS..=MAX_ADMIN_PROPOSAL_VALIDITY_SECS` (1 hour to 30 days). |
| `AdminCandidateConflictsWithRole` | 287 | `propose_admin` / `accept_admin` candidate is the SME, the treasury, or the contract itself. |
| `AdminProposalExpiryOverflow` | 288 | `propose_admin` could not compute the proposal expiry without overflowing. |

## Migration Errors

Codes raised by `migrate`.

| Error Name | Code | Description |
|---|---:|---|
| `MigrationVersionMismatch` | 90 | `migrate` `from_version` does not match the stored schema version. |
| `AlreadyCurrentSchemaVersion` | 91 | `migrate` called at or above `SCHEMA_VERSION`. |
| `NoMigrationPath` | 92 | `migrate` has no implemented path from the requested version. |

## Additional Settlement Errors

Further codes raised by `partial_settle`, `settle`, and `push_investor_payouts`.

| Error Name | Code | Description |
|---|---:|---|
| `PartialSettleNothingFunded` | 263 | `partial_settle` called while `funded_amount == 0`. |
| `PartialSettleBelowThreshold` | 264 | `partial_settle` called while the funded share of target is below `PartialCloseMinBps`. |
| `SettlementPoolUnderfunded` | 275 | `settle` post-condition failed: the escrow balance is below the settlement pool after collecting the SME shortfall. |
| `ClaimPushBatchEmpty` | 309 | `push_investor_payouts` received an empty investors vector. |
| `ClaimPushBatchTooLarge` | 310 | `push_investor_payouts` exceeded `MAX_CLAIM_PUSH_BATCH` (50). |
| `ClaimPushBatchDuplicate` | 311 | `push_investor_payouts` received the same investor more than once. |

## Batch Settlement Errors

Codes raised by `settle_batch`.

| Error Name | Code | Description |
|---|---:|---|
| `SettlementBatchEmpty` | 223 | `settle_batch` received an empty escrow addresses vector. |
| `SettlementBatchTooLarge` | 224 | `settle_batch` exceeded `MAX_SETTLE_BATCH` (50). |
| `SettlementBatchDuplicate` | 284 | `settle_batch` contains the same escrow address more than once. |
| `SettlementBatchSelfReference` | 285 | `settle_batch` contains the batch contract's own address. |

## Cancellation & Refund Errors

| Error Name | Code | Description |
|---|---:|---|
| `LegalHoldBlocksCancelFunding` | 140 | A legal hold is active when `cancel_funding` is called. |
| `CancelFundingNotOpen` | 141 | `cancel_funding` called while the escrow is not open. |
| `RefundNotCancelled` | 142 | `refund` / `refund_batch` called while the escrow is not cancelled. |
| `NoContributionToRefund` | 143 | `refund` called for an investor with zero contribution. |
| `RefundBatchEmpty` | 144 | `refund_batch` received an empty investors vector. |
| `RefundBatchTooLarge` | 145 | `refund_batch` exceeded `MAX_REFUND_BATCH` (50). |
| `RefundBatchDuplicateInvestor` | 266 | `refund_batch` contains the same investor more than once. |

## Unfund Errors

Codes raised by `unfund`.

| Error Name | Code | Description |
|---|---:|---|
| `UnfundEscrowNotOpen` | 220 | `unfund` called while the escrow is not open. |
| `OverWithdrawal` | 221 | `unfund` amount exceeds the investor's recorded contribution. |
| `UnfundLegalHoldActive` | 222 | A legal hold is active when `unfund` is called. |

## Legal Hold Errors

| Error Name | Code | Description |
|---|---:|---|
| `LegalHoldClearRequestMissing` | 150 | A legal-hold clear (`set_legal_hold(false)` with a delay, `clear_legal_hold_after_delay`, or `cancel_clear_legal_hold`) was attempted without a prior clear request. |
| `LegalHoldClearNotReady` | 151 | The two-phase legal-hold clear delay has not elapsed yet. |
| `LegalHoldClearDelayOverflow` | 152 | `request_clear_legal_hold` overflowed computing the clear ready-at timestamp. |
| `LegalHoldNotActive` | 299 | `request_clear_legal_hold` or `clear_legal_hold_after_delay` called with no active legal hold. |
| `LegalHoldAlreadyActive` | 300 | `set_legal_hold(true)` called while a legal hold is already active. |

## Beneficiary Rotation Errors

Codes raised by `rotate_beneficiary`.

| Error Name | Code | Description |
|---|---:|---|
| `LegalHoldBlocksBeneficiaryRotation` | 160 | A legal hold is active when `rotate_beneficiary` is called. |
| `RotationNotOpen` | 161 | `rotate_beneficiary` called outside a pre-settlement status (must be open or funded). |
| `NewSmeSameAsCurrent` | 162 | The proposed SME address equals the current beneficiary. |
| `BeneficiaryImmutableAfterFunding` | 248 | `rotate_beneficiary` called after investor principal was recorded (`funded_amount > 0`). |

## Operational Pause Errors

Codes raised by the pause configuration and toggle entrypoints. See [`docs/escrow-pause.md`](escrow-pause.md).

| Error Name | Code | Description |
|---|---:|---|
| `PauseToggleWindowOutOfRange` | 225 | `set_pause_rate_limit` `window_secs` is outside `MIN_PAUSE_TOGGLE_WINDOW_SECS..=MAX_PAUSE_TOGGLE_WINDOW_SECS` while `max_toggles > 0`. |
| `PauseRateLimitInvalidCombination` | 226 | `set_pause_rate_limit` received exactly one of `max_toggles` / `window_secs` as zero; both must be zero or both non-zero. |
| `PauseToggleRateLimitExceeded` | 227 | A pause toggle was rejected because the configured toggle limit was reached in the current window. |
| `PauseMaxDurationOutOfRange` | 230 | `set_pause_max_duration` received a non-zero value outside `MIN_PAUSE_MAX_DURATION_SECS..=MAX_PAUSE_MAX_DURATION_SECS` (1 hour to 90 days). |
| `PauseToggleLimitOutOfRange` | 231 | `set_pause_rate_limit` received a non-zero `max_toggles` outside `MIN_PAUSE_TOGGLE_LIMIT..=MAX_PAUSE_TOGGLE_LIMIT` (1 to 1,000). |
| `PauseScopeMismatch` | 246 | `set_paused(false, scope, _)` named a scope that does not match the active pause (and is not `All`); nothing was cleared. |
| `PauseAlreadyActive` | 276 | A pause activation requested a scope narrower than the active pause; the active pause is left untouched. |
| `PauseCooldownActive` | 277 | A pause activation was attempted after an auto-expired pause but before the cooldown elapsed. |
| `PauseCooldownOutOfRange` | 278 | `set_pause_cooldown` received a value outside `0..=MAX_PAUSE_MAX_DURATION_SECS`. |
| `PauserRoleConflict` | 279 | `set_pauser` was asked to install the SME or treasury address as pauser. |
| `NotPauser` | 280 | `pause_as_pauser` called by an address that is not the configured pauser (or no pauser is configured). |

## Storage TTL Errors

| Error Name | Code | Description |
|---|---:|---|
| `StorageLimitNotPositive` | 232 | Reserved: non-positive storage limit. Not currently returned; `set_storage_limit` reports `0` as `StorageLimitOutOfRange` (233). |
| `StorageLimitOutOfRange` | 233 | `set_storage_limit` received a limit outside `MIN_STORAGE_LIMIT_LEDGERS..=MAX_STORAGE_LIMIT_LEDGERS` (1 to 31,536,000). |
| `BumpTtlBatchEmpty` | 234 | `bump_ttl` or `batch_bump_ttl` received an empty vector. |
| `BumpTtlBatchTooLarge` | 235 | `bump_ttl` or `batch_bump_ttl` exceeded `MAX_BUMP_TTL_BATCH` (32). |

## Timelocked Upgrade Errors

Codes raised by `propose_upgrade`, `execute_upgrade`, and `cancel_upgrade`.

| Error Name | Code | Description |
|---|---:|---|
| `UpgradeTimelockActive` | 254 | `execute_upgrade` called before the timelock ETA. |
| `UpgradeProposalExpired` | 255 | `execute_upgrade` called after the execution window closed; re-propose. |
| `NoPendingUpgrade` | 256 | `execute_upgrade` or `cancel_upgrade` called with no pending proposal. |
| `LegalHoldBlocksUpgrade` | 257 | A legal hold is active when `execute_upgrade` is called. |
| `UpgradeEtaOverflow` | 258 | `propose_upgrade` overflowed computing `now + UPGRADE_TIMELOCK_SECS`. |

## Repayment Errors

Codes raised by `repay`.

| Error Name | Code | Description |
|---|---:|---|
| `LegalHoldBlocksRepayment` | 271 | A legal hold is active when `repay` is called. |
| `RepaymentNotOpen` | 272 | `repay` called while the escrow is not withdrawn (`status != 3`). |
| `RepaymentAmountNotPositive` | 273 | `repay` received a non-positive amount. |
| `RepaymentExceedsOutstanding` | 274 | `repay` amount exceeds the outstanding settlement pool. |

## Investor Freeze Errors

| Error Name | Code | Description |
|---|---:|---|
| `InvestorFreezeUnchanged` | 297 | `set_investor_frozen` / `set_investors_frozen` requested the freeze state already stored. |
| `InvestorFrozen` | 298 | A frozen investor called `fund`, `unfund`, `claim_investor_payout`, or `refund`. |
| `InvestorFreezeBatchDuplicate` | 315 | `set_investors_frozen` received the same investor more than once. |

Codes 36–41 (SEP-41 transfer-wrapper guards, also raised by `withdraw` and
`claim_investor_payout`) are listed above; their guard semantics are documented in
[`docs/escrow-token-safety.md`](escrow-token-safety.md).
