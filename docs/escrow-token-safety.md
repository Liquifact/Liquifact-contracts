# Escrow Token Safety Wrappers

LiquiFact escrow accepts one configured funding token address at `init`. That address is trusted as the token contract for the escrow lifetime, so governance must only configure standard SEP-41 tokens with stable, explicit-transfer balance accounting.

The escrow does not try to support fee-on-transfer, rebasing, callback-heavy, or otherwise non-standard token economics. Instead, every token movement goes through balance-delta wrappers that fail closed when observed balances do not match the requested transfer amount.

## Wrapper Model

Both wrappers in [`escrow/src/external_calls.rs`](../escrow/src/external_calls.rs) follow the same sequence:

1. Reject non-positive transfer amounts.
2. Read sender and recipient balances before transfer.
3. Check the sender has at least `amount`.
4. Call the SEP-41 `transfer` entrypoint.
5. Read sender and recipient balances after transfer.
6. Require the sender delta and recipient delta to equal `amount`.

This proves the escrow observed a standard transfer for the two accounts involved in the call. It does not prove the token is globally honest or immutable after `init`.

## Outbound Transfers

`transfer_funding_token_with_balance_checks` is used when the escrow-controlled balance sends funding-token units to another address, such as treasury dust sweep or investor refund paths.

| Guard | Error code |
| --- | --- |
| `amount <= 0` | `TransferAmountNotPositive` |
| sender balance below `amount` | `InsufficientTokenBalanceBeforeTransfer` |
| sender delta underflows | `SenderBalanceUnderflow` |
| recipient delta underflows | `RecipientBalanceUnderflow` |
| sender spent something other than `amount` | `SenderBalanceDeltaMismatch` |
| recipient received something other than `amount` | `RecipientBalanceDeltaMismatch` |

## Inbound Transfers

`transfer_funding_token_inbound_with_balance_checks` is used when an investor sends funding-token units into the escrow contract during funding flows.

| Guard | Error code |
| --- | --- |
| `amount <= 0` | `InboundTransferAmountNotPositive` |
| investor balance below `amount` | `InboundInsufficientTokenBalanceBeforeTransfer` |
| investor delta underflows | `InboundSenderBalanceUnderflow` |
| escrow delta underflows | `InboundRecipientBalanceUnderflow` |
| investor spent something other than `amount` | `InboundSenderBalanceDeltaMismatch` |
| escrow received something other than `amount` | `InboundRecipientBalanceDeltaMismatch` |

See [`docs/escrow-error-messages.md`](escrow-error-messages.md) for the authoritative numeric error-code table.

## Threat Model

The wrappers mitigate integration mistakes where a token reports balances that do not conserve the requested amount across the transfer boundary:

- Fee-on-transfer or deflationary tokens that debit `amount` but credit less than `amount`.
- Rebase-like behavior that changes sender or recipient balances outside the explicit transfer amount.
- Hook or callback-driven accounting that shifts balances during the token call.
- Lying or malicious token implementations that report inconsistent balance deltas.
- Wrong-token configuration caught only after attempted movement.

When one of these behaviors is observed, the wrapper emits the corresponding typed error and aborts the escrow operation before accounting can silently drift.

## Residual Assumptions

The wrappers are a safety boundary, not a token-audit substitute:

- The escrow trusts the configured `FundingToken` address set at `init`.
- Governance must verify the token's admin or upgrade authority before configuration.
- A later token-contract upgrade can invalidate prior review.
- The wrappers only compare the sender and recipient balances used in the call.
- A token can still be unsuitable for operational, legal, liquidity, or custody reasons even if delta checks pass.

## Safe Funding Token Class

Configure only standard SEP-41 tokens that:

- Move exactly `amount` from sender to recipient.
- Do not charge transfer fees.
- Do not rebase balances.
- Do not run hooks that mutate balances outside the explicit transfer.
- Have stable governance and upgrade controls acceptable to LiquiFact operators.

If a candidate token fails any item above, treat it as unsupported and deploy a fresh escrow with a compliant funding token.

## Test Coverage

The wrapper guarantees are covered in [`escrow/src/tests/external_calls_mocked.rs`](../escrow/src/tests/external_calls_mocked.rs) and related external-call tests:

- Fee-on-transfer mock rejection.
- Zero and negative amount rejection.
- Insufficient-balance rejection.
- Compliant-token transfer conservation.
- Multiple sequential transfers.
- Balance underflow and mismatch paths where the test harness can model them.

When adding a new funding-token movement, route it through one of these wrappers and add a test showing which wrapper protects the accounting boundary.
