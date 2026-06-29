# SEP-41 Token-Safety Wrappers and Balance-Delta Threat Model

**Status:** Accepted  
**Refs:** `escrow/src/lib.rs` — `EscrowError`; `escrow/src/external_calls.rs` — `transfer_funding_token_with_balance_checks`; `docs/escrow-error-messages.md`

---

## Overview

The LiquiFact escrow contract enforces strict token-safety invariants when transferring the funding token. This document describes the wrapper function, the threat model it mitigates, and its limitations.

## The `transfer_funding_token_with_balance_checks` Wrapper

Located in [`escrow/src/external_calls.rs`](../escrow/src/external_calls.rs), this wrapper performs all funding-token transfers with defensive pre/post balance assertions.

### Pre-transfer Checks

| Check | Condition | Error code |
|-------|-----------|------------|
| Amount validity | `amount > 0` | `TransferAmountNotPositive` (36) |
| Balance sufficiency | `sender.balance >= amount` | `InsufficientTokenBalanceBeforeTransfer` (37) |

### Post-transfer Delta Assertions

After invoking `token.transfer`, the wrapper computes:

```rust
let spent = from_before.checked_sub(from_after).unwrap_or_else(|| fail(SenderBalanceUnderflow));
let received = treasury_after.checked_sub(treasury_before).unwrap_or_else(|| fail(RecipientBalanceUnderflow));
```

Then asserts:

| Assertion | Expected | Error code |
|-----------|----------|------------|
| Sender delta equals amount | `spent == amount` | `SenderBalanceDeltaMismatch` (40) |
| Recipient delta equals amount | `received == amount` | `RecipientBalanceDeltaMismatch` (41) |

### Execution Timeline

Per the Soroban host-call boundary (see [`external_calls.rs`](../escrow/src/external_calls.rs) rustdoc):

1. Read sender/recipient balances before transfer.
2. Invoke SEP-41 `transfer` on the configured token contract.
3. Soroban host executes that token call to completion, then returns.
4. Read sender/recipient balances after transfer.
5. Assert exact conservation (`spent == amount` and `received == amount`).

## Threat Model

### Fee-on-Transfer Tokens

Tokens that subtract a fee on every transfer cause `received < amount` while `spent == amount`. This triggers `RecipientBalanceDeltaMismatch` (41).

**Test coverage:** [`test_fee_on_transfer_token_rejected`](escrow/src/tests/external_calls_mocked.rs) in `external_calls_mocked.rs` validates this detection using a 1%-fee token mock.

### Rebasing Tokens

Tokens that modify holder balances without transfer calls (e.g., elastic-supply tokens) would cause either:
- `SenderBalanceDeltaMismatch` if the sender's post-balance differs from expected (supply shrinkage)
- `RecipientBalanceDeltaMismatch` if the recipient's post-balance differs from expected

**Mitigation in Soroban:** Rebasing tokens require periodic supply adjustments via admin/minter functions. The pre/post balance checks detect any non-standard balance changes across the transfer boundary.

### Reentrant Tokens (Hook Patterns)

Unlike EVM, Soroban does not allow classic synchronous reentrancy where a token callback can interleave state changes. The host function runs to completion before control returns.

**However:** The wrapper still records balances around transfers so that any token hooks modifying balances post-transfer (asynchronously or via internal logic) are caught by the delta assertions.

### Lying Tokens (Balance Reporting Mismatches)

A malicious token could report incorrect balances or execute transfers incorrectly. The wrapper detects:

- `SenderBalanceUnderflow` (38) — occurs when `from_after > from_before` (balance increased) or when subtraction overflows
- `RecipientBalanceUnderflow` (39) — occurs when `treasury_after < treasury_before` (balance decreased) or subtraction overflows

## Residual Assumptions

### Trusted Configuration

| Assumption | Risk if violated |
|------------|------------------|
| `DataKey::FundingToken` points to a well-behaved SEP-41 token contract | Fee-on-transfer tokens cause balance-delta panics; the integration layer must validate token contract before initialization |
| `DataKey::Treasury` is a correct, controlled address | Sweep proceeds to wrong recipient (governance risk, not technical) |

### Token Economics

The contract assumes the configured token follows standard SEP-41 semantics:

1. Transfer amounts are exact, no hidden fees
2. Transfer amounts preserve total supply (no mint/burn side-effects)
3. Balance changes are observable via `balance()` after the host call completes
4. No callback hooks that modify balances after `transfer` returns

## Recommended Token Classes

| Class | Compatibility | Notes |
|-------|---------------|-------|
| **Stellar Asset Contract (SAC)** | ✅ Recommended | Standard SEP-41, no fees, supply immutable |
| **Standard ERC-20 style wrapped assets** | ✅ Recommended | Verified no fee-on-transfer or hook patterns |
| **Fee-on-transfer tokens** | ❌ Rejected | Will panic with `RecipientBalanceDeltaMismatch` |
| **Elastic-supply (rebasing) tokens** | ❌ Rejected | Supply changes break delta equality guarantees |
| **Tokens with transfer hooks/callbacks** | ❌ Rejected | Hook effects may not be detectable; may cause silent failures |
| **Governed stablecoins** | ✅ Conditionally | Must be confirmed fee-free and non-rebasing |

## Error Code Reference

All errors from the token-safety wrapper are typed `EscrowError` variants with stable numeric codes:

| Code | Variant | Trigger | Meaning |
|------|---------|---------|---------|
| 36 | `TransferAmountNotPositive` | `amount <= 0` | Transfer amount validation failed |
| 37 | `InsufficientTokenBalanceBeforeTransfer` | `sender.balance < amount` | Sender lacks sufficient funds |
| 38 | `SenderBalanceUnderflow` | `from_before - from_after` underflows | Sender balance paradox detected |
| 39 | `RecipientBalanceUnderflow` | `treasury_after - treasury_before` underflows | Recipient balance paradox detected |
| 40 | `SenderBalanceDeltaMismatch` | `spent != amount` | Token took more than requested (fee/hook) |
| 41 | `RecipientBalanceDeltaMismatch` | `received != amount` | Token delivered less than requested (fee/hook) |

See `docs/escrow-error-messages.md` for the complete typed error reference.

## Test Coverage

The wrapper's guarantees are validated through mocked token scenarios in `escrow/src/tests/external_calls_mocked.rs`:

| Test | Scenario | Expected outcome |
|------|----------|------------------|
| `test_fee_on_transfer_token_rejected` | 1% fee token | Panics with RecipientBalanceDeltaMismatch |
| `test_compliant_token_passes` | Standard SAC | Balance deltas match exactly |
| `test_zero_amount_rejected` | amount = 0 | Panics with TransferAmountNotPositive |
| `test_negative_amount_rejected` | amount = -50 | Panics with TransferAmountNotPositive |
| `test_insufficient_balance_rejected` | Sender has 500, tries 1000 | Panics with InsufficientTokenBalanceBeforeTransfer |
| `test_large_transfer_no_overflow` | Large amount near i128 bounds | Passes; exact deltas verified |
| `test_multiple_sequential_transfers` | Two transfers to different recipients | Each transfer individually verified |
| `test_sender_ends_at_zero_balance` | Transfer exact balance | Final balance is zero, deltas exact |

## Integration Guidance

### Before Initialization

1. Audit the token contract source or verify its identity via trusted registry
2. Confirm no hidden transfer fees or hook callbacks
3. Verify supply immutability (no rebasing)
4. Record the token contract address in the governance allowlist

### After Initialization

If token-safety errors occur (codes 36–41), treat them as integration misconfiguration:

1. Do not retry with the same token contract
2. Verify the funding-token address was correctly set at init
3. Use the allowlist path to select a compliant replacement token

## Cross-References

- [`escrow/src/external_calls.rs`](../escrow/src/external_calls.rs) — Implementation
- [`docs/escrow-error-messages.md`](escrow-error-messages.md) — Error code reference
- [`docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`](ESCROW_TOKEN_INTEGRATION_CHECKLIST.md) — Integration checklist
- [`docs/adr/ADR-006-dust-sweep-and-token-safety.md`](adr/ADR-006-dust-sweep-and-token-safety.md) — ADR context