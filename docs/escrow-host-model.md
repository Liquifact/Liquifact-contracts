# Escrow Host Call Model (Soroban)

This note explains the escrow contract's token transfer safety model for reviewers, focused on
Soroban execution semantics and the checks implemented in `escrow/src/external_calls.rs`.

## Short model

- The escrow calls the configured SEP-41 token contract via Soroban host.
- The token call executes and returns to escrow.
- Escrow validates post-call balance deltas for both sender and recipient.
- Any mismatch (fees, rebasing side effects, non-standard behavior) fails closed with panic.

## Transfer + balance-check timeline

```
Escrow contract
  -> read from_before, treasury_before
  -> call token.transfer(from, treasury, amount)
     -> token contract executes in host
     -> returns to escrow
  -> read from_after, treasury_after
  -> assert (from_before - from_after) == amount
  -> assert (treasury_after - treasury_before) == amount
```

## Soroban vs EVM reentrancy framing (contrast only)

- In Soroban, the host call boundary is explicit: escrow logic resumes after the token call returns.
- This avoids relying on EVM-style "checks-effects-interactions" folklore as the primary argument.
- The escrow still assumes the token can be adversarial from an accounting perspective, so
  invariant checks are mandatory after external calls.

## Security assumptions and out-of-scope behavior

The escrow wrapper is intentionally strict and supports standard SEP-41-like token behavior only.

- Assumed: balance deltas exactly match transfer amount.
- Out of scope: fee-on-transfer, rebasing, or hook-driven token economics.
- Policy: unsupported behavior should be blocked by allowlisting and integration review.
- Runtime fallback: if unsupported behavior occurs, transfer checks panic (safe failure).

## Reviewer checklist

- Confirm all token transfers that matter for accounting use the wrapper in
  `escrow/src/external_calls.rs`.
- Confirm both sender and recipient deltas are asserted exactly.
- Confirm out-of-scope token behavior is documented in integration/security docs.
