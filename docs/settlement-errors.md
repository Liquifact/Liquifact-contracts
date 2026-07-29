# Settlement Error Codes

This document lists every typed [`EscrowError`](../escrow/src/lib.rs) code that the
settlement-family entrypoints can emit, the exact condition that triggers each one, and how to
avoid it.

All codes are **stable and append-only** — SDKs must branch on the numeric
`ContractError(code)`, not on panic-string text.

## Covered entrypoints

| Entrypoint | Auth role | Description |
| --- | --- | --- |
| `partial_settle(caller)` | SME or Admin | Force-advances open escrow to funded status before the target is reached. |
| `settle()` | SME | Marks a funded escrow as settled once maturity is reached. |
| `withdraw()` | SME | SME pulls the funded principal (net of protocol fee). |
| `claim_investor_payout(investor)` | Investor | Investor records a payout claim after settlement. |

---

## Error Reference

### Legal-hold guards

These errors are checked **before** `require_auth` as read-only pre-conditions, so they fire even
if the caller's signature is valid.

| Code | Variant | Entrypoint(s) | When it fires | How to avoid it |
| ---: | --- | --- | --- | --- |
| 201 | `LegalHoldBlocksPartialSettle` | `partial_settle` | A legal hold is active when `partial_settle` is called. | Wait for the admin to clear the legal hold before calling `partial_settle`. |
| 120 | `LegalHoldBlocksSettlement` | `settle` | A legal hold is active when `settle` is called. | Wait for the admin to clear the legal hold before calling `settle`. |
| 123 | `LegalHoldBlocksWithdrawal` | `withdraw` | A legal hold is active when `withdraw` is called. | Wait for the admin to clear the legal hold before calling `withdraw`. |
| 125 | `LegalHoldBlocksInvestorClaims` | `claim_investor_payout` | A legal hold is active when `claim_investor_payout` is called. | Wait for the admin to clear the legal hold before claiming. |

### Operational-pause guards

Checked before `require_auth`, orthogonal to the legal hold. A pause auto-expires if
`set_pause_max_duration` is configured; otherwise it persists until cleared explicitly.

| Code | Variant | Entrypoint(s) | When it fires | How to avoid it |
| ---: | --- | --- | --- | --- |
| 211 | `PausedBlocksSettlement` | `settle` | The operational pause is active. | Wait for the pause to be cleared (`set_paused(false)`) or for the configured max duration to expire. |
| 212 | `PausedBlocksWithdrawal` | `withdraw` | The operational pause is active. | Same as above. |
| 213 | `PausedBlocksInvestorClaims` | `claim_investor_payout` | The operational pause is active. | Same as above. |

### Authorization and caller guards

| Code | Variant | Entrypoint(s) | When it fires | How to avoid it |
| ---: | --- | --- | --- | --- |
| 200 | `PartialSettleUnauthorizedCaller` | `partial_settle` | `caller` is neither `InvoiceEscrow::sme_address` nor `InvoiceEscrow::admin`. | Pass the SME address or the admin address as `caller` and sign with the matching key. |

### Status guards

| Code | Variant | Entrypoint(s) | When it fires | How to avoid it |
| ---: | --- | --- | --- | --- |
| 202 | `PartialSettleNotOpen` | `partial_settle` | `InvoiceEscrow::status != 0` (escrow is not open). | `partial_settle` is only valid while the escrow is still accepting contributions (`status = 0`). Check status before calling. |
| 121 | `SettlementNotFunded` | `settle` | `InvoiceEscrow::status != 1` (escrow has not reached funded status). | `settle` requires `status = 1`. Fund the escrow to the target (or call `partial_settle`) first. |
| 124 | `WithdrawalNotFunded` | `withdraw` | `InvoiceEscrow::status != 1`. | `withdraw` requires `status = 1`. Call `settle` before `withdraw` is not the correct sequence — the SME must call `withdraw` while the escrow is in the funded state (`status = 1`), after which `settle` transitions to `status = 2`. Wait for funding to complete first. |
| 127 | `InvestorClaimNotSettled` | `claim_investor_payout` | `InvoiceEscrow::status != 2`. | Investors may only claim after the escrow has been fully settled. Check `get_escrow().status == 2` before calling. |

### Maturity guard

| Code | Variant | Entrypoint(s) | When it fires | How to avoid it |
| ---: | --- | --- | --- | --- |
| 122 | `MaturityNotReached` | `settle` | `InvoiceEscrow::maturity > 0` and the current ledger timestamp is strictly less than `maturity`. | Wait until `env.ledger().timestamp() >= maturity` before calling `settle`. When `maturity == 0` this guard is skipped entirely. |

### Balance and arithmetic guards

| Code | Variant | Entrypoint(s) | When it fires | How to avoid it |
| ---: | --- | --- | --- | --- |
| 165 | `InsufficientContractBalance` | `withdraw` | The contract's funding-token balance is less than `InvoiceEscrow::funded_amount`. | Ensure the funding token was transferred into the contract by investors before the SME calls `withdraw`. This check fires if tokens were drained externally or the escrow was never funded via `fund`/`fund_with_commitment`. |
| 216 | `WithdrawFeeArithmeticOverflow` | `withdraw` | `funded_amount * fee_bps` overflowed `i128` during protocol-fee computation. | This is practically unreachable for compliant escrows: `funded_amount` is bounded by `MAX_INVOICE_AMOUNT` (≤ 2⁶³ − 1) and `fee_bps ∈ [0, 10_000]`, so the product fits in `i128`. If you encounter this, the escrow was initialised with an out-of-range amount or fee. |
| 217 | `WithdrawNetArithmeticUnderflow` | `withdraw` | `funded_amount - fee` underflowed. | Unreachable for in-range `fee_bps` (0–10 000). Indicates a logic or storage corruption. |
| 129 | `ComputePayoutArithmeticOverflow` | `settle`, `claim_investor_payout` | Checked arithmetic overflowed while computing `settle_pool` or the investor's gross payout. | Unreachable for compliant escrows whose `funded_amount` is within `MAX_INVOICE_AMOUNT`. Indicates a storage inconsistency. |

### Investor-payout guards

| Code | Variant | Entrypoint(s) | When it fires | How to avoid it |
| ---: | --- | --- | --- | --- |
| 126 | `NoContributionToClaim` | `claim_investor_payout` | The investor address has no recorded contribution (`DataKey::InvestorContribution == 0`). | Only investors who funded the escrow via `fund` or `fund_with_commitment` have a claimable payout. Verify contribution with `get_contribution(investor)` before calling. |
| 128 | `InvestorCommitmentLockNotExpired` | `claim_investor_payout` | The current ledger timestamp is before the investor's `InvestorClaimNotBefore` timestamp (set when the investor used `fund_with_commitment` with a `committed_lock_secs > 0`). | Wait until `env.ledger().timestamp() >= InvestorClaimNotBefore`. Read the lock-expiry time off-chain via `get_investor_claim_not_before(investor)`. |
| 170 | `PayoutZero` | `claim_investor_payout` | The computed on-chain payout for the investor is zero. | This can occur when `funded_amount` is tiny relative to `total_principal` and floor-rounding produces zero. If you encounter this, the investor's contribution was too small to generate a non-zero payout under the configured yield. |

### SEP-41 token-safety guards (transfer wrapper)

These codes fire inside the `transfer_funding_token_with_balance_checks` wrapper used by `withdraw`
and `claim_investor_payout`. See [`docs/escrow-token-safety.md`](escrow-token-safety.md) for the
full threat model.

| Code | Variant | Entrypoint(s) | When it fires | How to avoid it |
| ---: | --- | --- | --- | --- |
| 36 | `TransferAmountNotPositive` | `withdraw`, `claim_investor_payout` | The amount passed to the transfer wrapper is ≤ 0. | Internal guard; only reachable via a logic error in the calling entrypoint. |
| 37 | `InsufficientTokenBalanceBeforeTransfer` | `withdraw`, `claim_investor_payout` | The sender's balance is less than the requested transfer amount immediately before the `transfer` call. | Ensure the contract holds sufficient tokens. Pre-check with `TokenClient::balance`. |
| 38 | `SenderBalanceUnderflow` | `withdraw`, `claim_investor_payout` | Post-transfer arithmetic detected that the sender's balance decreased by less than expected (underflow in delta). | The token contract behaved unexpectedly. Fee-on-transfer and rebasing tokens are explicitly unsupported. |
| 39 | `RecipientBalanceUnderflow` | `withdraw`, `claim_investor_payout` | Post-transfer arithmetic detected that the recipient's balance increased by less than expected (underflow in delta). | Same as above. |
| 40 | `SenderBalanceDeltaMismatch` | `withdraw`, `claim_investor_payout` | The sender spent a different amount than requested (fee-on-transfer token detected). | Use only standard SEP-41 compliant tokens. |
| 41 | `RecipientBalanceDeltaMismatch` | `withdraw`, `claim_investor_payout` | The recipient received a different amount than requested. | Use only standard SEP-41 compliant tokens. |

---

## Guard-ordering summary

Understanding the order in which guards are evaluated helps predict which code fires when multiple
conditions are true simultaneously.

### `partial_settle(caller)`
1. Legal-hold gate → `LegalHoldBlocksPartialSettle` (201)
2. `caller.require_auth()` — Soroban host auth failure (not a typed `EscrowError`)
3. Caller identity check → `PartialSettleUnauthorizedCaller` (200)
4. Status == 0 check → `PartialSettleNotOpen` (202)

### `settle()`
1. Operational-pause gate → `PausedBlocksSettlement` (211)
2. Legal-hold gate → `LegalHoldBlocksSettlement` (120)
3. `sme_address.require_auth()` — Soroban host auth failure
4. Status == 1 check → `SettlementNotFunded` (121)
5. Maturity check → `MaturityNotReached` (122)
6. Payout arithmetic → `ComputePayoutArithmeticOverflow` (129)

### `withdraw()`
1. Operational-pause gate → `PausedBlocksWithdrawal` (212)
2. Legal-hold gate → `LegalHoldBlocksWithdrawal` (123)
3. `sme_address.require_auth()` — Soroban host auth failure
4. Status == 1 check → `WithdrawalNotFunded` (124)
5. Contract balance check → `InsufficientContractBalance` (165)
6. Fee arithmetic → `WithdrawFeeArithmeticOverflow` (216) / `WithdrawNetArithmeticUnderflow` (217)
7. Token transfers → codes 36–41

### `claim_investor_payout(investor)`
1. Operational-pause gate → `PausedBlocksInvestorClaims` (213)
2. Legal-hold gate → `LegalHoldBlocksInvestorClaims` (125)
3. `investor.require_auth()` — Soroban host auth failure
4. Contribution > 0 check → `NoContributionToClaim` (126)
5. Status == 2 check → `InvestorClaimNotSettled` (127)
6. Commitment-lock check → `InvestorCommitmentLockNotExpired` (128)
7. Idempotency check (silent no-op if already claimed — no error)
8. Payout computation → `ComputePayoutArithmeticOverflow` (129)
9. Payout > 0 check → `PayoutZero` (170)
10. Token transfer → codes 36–41

---

## Escrow-status reference

| Value | Name | Description |
| ---: | --- | --- |
| 0 | Open | Accepting contributions; `partial_settle` valid. |
| 1 | Funded | Target reached or `partial_settle` called; `settle` and `withdraw` valid. |
| 2 | Settled | SME called `settle`; investor claims valid. |
| 3 | Withdrawn | SME called `withdraw`; terminal. |
| 4 | Cancelled | Admin cancelled; refunds valid. |

---

## Stability policy

All codes listed here are **append-only and will never be renumbered or reassigned**. New
settlement-related failures will receive new codes at the end of the relevant range. See
[`docs/escrow-error-messages.md`](escrow-error-messages.md) for the full contract-wide code table.

## See also

- [`docs/escrow-error-messages.md`](escrow-error-messages.md) — full error code table
- [`docs/escrow-token-safety.md`](escrow-token-safety.md) — SEP-41 token-safety wrapper
- [`docs/settlement-auth.md`](settlement-auth.md) — authorization boundaries for settlement
- [`docs/escrow-legal-hold.md`](escrow-legal-hold.md) — legal hold lifecycle
- [`docs/escrow-ledger-time.md`](escrow-ledger-time.md) — maturity and ledger-time model
- [`docs/escrow-pro-rata.md`](escrow-pro-rata.md) — pro-rata payout arithmetic
- [`docs/adr/ADR-003-settlement-flow.md`](adr/ADR-003-settlement-flow.md) — settlement design decisions
