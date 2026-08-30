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
| `PausedBlocksSettlement` | 211 | The operational pause is active when `settle` is called. |
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

Codes 36–41 (SEP-41 transfer-wrapper guards, also raised by `withdraw` and
`claim_investor_payout`) are documented in
[`docs/escrow-token-safety.md`](escrow-token-safety.md).