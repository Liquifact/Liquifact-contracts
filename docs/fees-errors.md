# Fees Error Codes

This document lists every typed [`EscrowError`](../escrow/src/lib.rs) code that the
fee-family entrypoints can emit, the exact condition that triggers each one, and how to
avoid it.

All codes are **stable and append-only** — SDKs must branch on the numeric
`ContractError(code)`, not on panic-string text.

## Scope

"Fees" in this contract means the immutable `protocol_fee_bps` split applied to the
SME's principal at [`LiquifactEscrow::withdraw`]. The fee rate is configured once at
`init` (or updated via `set_protocol_fee_bps`) and realized only when the SME calls
`withdraw`. Settlement, investor claims, and refunds never apply or reference
`protocol_fee_bps`.

## Fee-related entrypoints

| Entrypoint | Auth role | Description |
| --- | --- | --- |
| `init(... protocol_fee_bps)` | Admin | Configures `protocol_fee_bps` (basis points, `0..=10_000`). Stored immutably under `DataKey::ProtocolFeeBps`. |
| `set_protocol_fee_bps(new_bps)` | Admin | Updates `DataKey::ProtocolFeeBps`. Emits `ProtocolFeeUpdated`. |
| `get_protocol_fee_bps()` | None (read-only) | Returns the stored `protocol_fee_bps`, or `0` for pre-fee escrows. |
| `withdraw()` | SME | Splits `funded_amount` into a treasury fee and SME net payout. Fee = `funded_amount * fee_bps / 10_000` (floor, checked). Status transitions to 3. |

---

## Error summary

### Fee-owned errors

| Code | Variant | Category |
| ---: | --- | --- |
| 215 | `ProtocolFeeBpsOutOfRange` | Fee configuration |
| 216 | `WithdrawFeeArithmeticOverflow` | Fee arithmetic |
| 217 | `WithdrawNetArithmeticUnderflow` | Fee arithmetic |
| 165 | `InsufficientContractBalance` | Fee execution |

### Shared prerequisite errors

| Code | Variant | Category |
| ---: | --- | --- |
| 21 | `FundingTokenNotSet` | Init readiness |
| 22 | `TreasuryNotSet` | Init readiness |

### Shared SEP-41 token-safety errors

| Code | Variant | Category |
| ---: | --- | --- |
| 36 | `TransferAmountNotPositive` | Token transfer |
| 37 | `InsufficientTokenBalanceBeforeTransfer` | Token transfer |
| 38 | `SenderBalanceUnderflow` | Token transfer |
| 39 | `RecipientBalanceUnderflow` | Token transfer |
| 40 | `SenderBalanceDeltaMismatch` | Token transfer |
| 41 | `RecipientBalanceDeltaMismatch` | Token transfer |

---

## Fee configuration errors

### 215 — `ProtocolFeeBpsOutOfRange`

| Field | Value |
| --- | --- |
| **Variant** | `EscrowError::ProtocolFeeBpsOutOfRange` |
| **Code** | 215 |
| **Entrypoints** | `init`, `set_protocol_fee_bps` |
| **Trigger** | The `protocol_fee_bps` parameter is outside `0..=10_000`. A negative value or a value greater than `10_000` (100%) is rejected. |
| **Avoidance** | Supply a value in `0..=10_000`. A value of `0` means no fee (full `funded_amount` goes to the SME). A value of `10_000` means 100% fee (entire `funded_amount` goes to the treasury). |

---

## Fee readiness errors

These errors indicate the escrow contract was not fully initialized before a fee-related
entrypoint was called. They are shared across multiple entrypoints, not specific to fees.

### 21 — `FundingTokenNotSet`

| Field | Value |
| --- | --- |
| **Variant** | `EscrowError::FundingTokenNotSet` |
| **Code** | 21 |
| **Entrypoints** | `withdraw`, `sweep_terminal_dust`, `get_funding_token`, `fund_impl` |
| **Trigger** | `DataKey::FundingToken` has not been written to instance storage. This means `init` has not been called, or the storage key was removed. |
| **Avoidance** | Call `init` before invoking any entrypoint that reads the funding token. The token address is immutable after `init`. |

### 22 — `TreasuryNotSet`

| Field | Value |
| --- | --- |
| **Variant** | `EscrowError::TreasuryNotSet` |
| **Code** | 22 |
| **Entrypoints** | `withdraw`, `sweep_terminal_dust`, `get_treasury` |
| **Trigger** | `DataKey::Treasury` has not been written to instance storage. This means `init` has not been called, or the storage key was removed. |
| **Avoidance** | Call `init` with a valid treasury address before invoking `withdraw` or `sweep_terminal_dust`. The treasury address is immutable after `init`. |

---

## Withdrawal and fee execution errors

### 165 — `InsufficientContractBalance`

| Field | Value |
| --- | --- |
| **Variant** | `EscrowError::InsufficientContractBalance` |
| **Code** | 165 |
| **Entrypoints** | `withdraw` |
| **Trigger** | The contract's funding-token SEP-41 balance is less than `funded_amount`. The contract must hold enough tokens to cover both the SME net payout and the treasury fee. |
| **Avoidance** | Ensure investors have transferred tokens into the contract via `fund` or `fund_with_commitment` before the SME calls `withdraw`. If tokens were drained externally or the escrow was never funded on-chain, this error fires. Check `TokenClient::balance(&contract_address)` before calling `withdraw`. |

### 216 — `WithdrawFeeArithmeticOverflow`

| Field | Value |
| --- | --- |
| **Variant** | `EscrowError::WithdrawFeeArithmeticOverflow` |
| **Code** | 216 |
| **Entrypoints** | `withdraw` |
| **Trigger** | `funded_amount * fee_bps` overflowed `i128` during protocol-fee computation. The multiplication uses `i128::checked_mul`. |
| **Avoidance** | Practically unreachable for compliant escrows: `funded_amount` is bounded by `MAX_INVOICE_AMOUNT` (2^63 - 1) and `fee_bps` is in `0..=10_000`, so the product always fits in `i128`. If encountered, the escrow was initialized with an out-of-range amount or fee. |

### 217 — `WithdrawNetArithmeticUnderflow`

| Field | Value |
| --- | --- |
| **Variant** | `EscrowError::WithdrawNetArithmeticUnderflow` |
| **Code** | 217 |
| **Entrypoints** | `withdraw` |
| **Trigger** | `funded_amount - fee` underflowed during net SME payout computation. The subtraction uses `i128::checked_sub`. |
| **Avoidance** | Unreachable for in-range `fee_bps` (0–10,000) because the fee can never exceed `funded_amount`. This is a defensive guard for checked arithmetic on principle. If encountered, it indicates a logic or storage corruption error — not a normal integration failure. |

---

## Shared SEP-41 token-safety errors

These errors fire inside the `transfer_funding_token_with_balance_checks` wrapper used
by `withdraw` for both the treasury fee leg and the SME net-payout leg. They are not
fee-specific — the same codes apply to `claim_investor_payout`, `refund`, and
`sweep_terminal_dust`. See [`docs/escrow-token-safety.md`](escrow-token-safety.md) for the
full threat model.

| Code | Variant | Trigger | Avoidance |
| ---: | --- | --- | --- |
| 36 | `TransferAmountNotPositive` | The amount passed to the transfer wrapper is ≤ 0. | Internal guard; only reachable via a logic error in the calling entrypoint. |
| 37 | `InsufficientTokenBalanceBeforeTransfer` | The sender's balance is less than the requested transfer amount immediately before the `transfer` call. | Ensure the contract holds sufficient tokens. Pre-check with `TokenClient::balance`. |
| 38 | `SenderBalanceUnderflow` | Post-transfer arithmetic detected that the sender's balance decreased by less than expected. | The token contract behaved unexpectedly. Fee-on-transfer and rebasing tokens are explicitly unsupported. |
| 39 | `RecipientBalanceUnderflow` | Post-transfer arithmetic detected that the recipient's balance increased by less than expected. | Same as above. |
| 40 | `SenderBalanceDeltaMismatch` | The sender spent a different amount than requested (fee-on-transfer token detected). | Use only standard SEP-41 compliant tokens. |
| 41 | `RecipientBalanceDeltaMismatch` | The recipient received a different amount than requested. | Use only standard SEP-41 compliant tokens. |

---

## Guard ordering

Understanding the order in which guards are evaluated helps predict which code fires when
multiple conditions are true simultaneously.

### `init(... protocol_fee_bps)`

1. `AlreadyInitialized` (3) — escrow storage already exists
2. Input validation for each parameter (amount, yield_bps, invoice_id, etc.)
3. `ProtocolFeeBpsOutOfRange` (215) — `protocol_fee_bps` outside `0..=10_000`
4. Storage writes and event emission

### `set_protocol_fee_bps(new_bps)`

1. `EscrowNotInitialized` (20) — escrow not initialized
2. Admin authorization — `admin.require_auth()` (Soroban host auth failure, not a typed error)
3. `ProtocolFeeBpsOutOfRange` (215) — `new_bps` outside `0..=10_000`
4. Storage write and `ProtocolFeeUpdated` event emission

### `withdraw()`

1. `PausedBlocksWithdrawal` (212) — operational pause active
2. `LegalHoldBlocksWithdrawal` (123) — legal hold active
3. `sme_address.require_auth()` — Soroban host auth failure
4. `WithdrawalNotFunded` (124) — `status != 1`
5. `InsufficientContractBalance` (165) — contract balance < `funded_amount`
6. `WithdrawFeeArithmeticOverflow` (216) — `funded_amount * fee_bps` overflowed
7. `WithdrawNetArithmeticUnderflow` (217) — `funded_amount - fee` underflowed
8. Token transfer → codes 36–41

---

## Stability policy

All codes listed here are **append-only and will never be renumbered or reassigned**. New
fee-related failures will receive new codes at the end of the relevant range. See
[`docs/escrow-error-messages.md`](escrow-error-messages.md) for the full contract-wide code table.

## Cross-references

- [`docs/fees-auth.md`](fees-auth.md) — authorization and access rules for the fee subsystem
- [`docs/settlement-errors.md`](settlement-errors.md) — related settlement error codes (overlaps on `withdraw` guard ordering)
- [`docs/escrow-token-safety.md`](escrow-token-safety.md) — SEP-41 token-safety wrapper (codes 36–41)
- [`docs/escrow-error-messages.md`](escrow-error-messages.md) — full contract-wide error code table
- [`docs/adr/ADR-002-auth-boundaries.md`](adr/ADR-002-auth-boundaries.md) — canonical guard ordering (read-only → auth → writes)
