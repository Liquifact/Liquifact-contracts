# Escrow Token Integration Security Checklist

This checklist describes the supported assumptions and explicit unsupported token behaviors for integrations that use the LiquiFact escrow contract with cross-contract token assets.

## Supported token assumptions

- Amounts are recorded in the escrow contract as raw smallest units using `i128`.
  - Integration layers must convert external human-readable amounts into smallest units before calling `fund`.
  - Do not rely on asset decimals inside the escrow contract; the contract stores integer amounts only.
- The escrow contract does not itself perform token transfers or custody assets.
  - `record_sme_collateral_commitment` stores SME-reported metadata only and does not lock assets, verify custody, or create an enforceable on-chain claim.
  - Token movement must be handled separately by the integration layer.
- The contract uses strong signer authorization for state changes (`require_auth(...)` for admin, SME, and investor roles).
- Token asset identity should be established by token contract ID or audited registry, not by symbol alone.

## Integration-layer responsibilities

- Validate the token contract before use:
  - confirm the contract ID or hash is expected and audited
  - confirm the token contract is not paused, frozen, or blacklisted
  - confirm the token implements standard transfer semantics without hidden fees
- Normalize decimals outside the contract:
  - convert human-facing amounts into the token's smallest unit
  - reject tokens with nonstandard decimals or dynamic fractional behavior
- Protect against malicious tokens:
  - do not integrate with fee-on-transfer or deflationary transfer tokens
  - do not integrate with tokens that have reentrant hooks or unexpected callback behavior
  - do not assume token contract invariants beyond the audited interface
- Use separate transfer preflight logic or atomic transfer flows to ensure on-chain escrow state matches actual token movement.

## Explicit unsupported token behavior warnings

The escrow contract and its documented assumptions do not support direct integration with the following token behaviors:

- Fee-on-transfer or deflationary tokens
- Paused, frozen, or blacklisted token contracts
- Nonstandard transfer semantics or callback-based reentrancy
- Dynamic decimals, fractional units outside integer smallest-unit semantics
- Malicious token contracts that alter balances in unexpected ways or change transfer metadata

## Terminal dust sweep (`sweep_terminal_dust`)

- The escrow uses [`escrow/src/external_calls.rs`](../escrow/src/external_calls.rs) to assert **exact** sender/recipient balance deltas for the configured **funding** token.
- Integrations must still treat **fee-on-transfer** and other non-standard tokens as **unsupported**; such tokens can cause the sweep to panic when deltas do not match `amount`.

## Why this matters

Because the contract only records numeric state and collateral metadata (aside from the guarded dust sweep transfer path), token integration security is enforced by the surrounding application or bridge logic.

- The escrow contract is safe for algebraic accounting of on-chain amounts.
- The integration layer must reject unsupported token patterns before calling escrow entrypoints.
- The collateral commitment record is not an on-chain asset lock and should not be treated as proof of custody; see [`escrow-sme-collateral.md`](escrow-sme-collateral.md).

---

## Adversarial token test coverage (added: contracts-23)

The balance-delta wrapper in `escrow/src/external_calls.rs` is covered by adversarial mock-token tests in `escrow/src/tests/external_calls_mocked.rs`. The table below maps each typed error to its test(s) and the token archetype that triggers it.

| Error code | `EscrowError` variant                    | Trigger archetype               | Test name(s)                                                                                                                                                                                                                                                |
| ---------- | ---------------------------------------- | ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 36         | `TransferAmountNotPositive`              | amount ≤ 0                      | `test_zero_amount_rejected`, `test_negative_amount_rejected`, `test_min_i128_amount_rejected`                                                                                                                                                               |
| 37         | `InsufficientTokenBalanceBeforeTransfer` | sender balance < amount         | `test_insufficient_balance_rejected`, `test_zero_balance_rejected`, `test_balance_one_less_than_amount_rejected`                                                                                                                                            |
| 40         | `SenderBalanceDeltaMismatch`             | fee-on-transfer, no-op transfer | `test_fee_on_transfer_token_rejected`, `test_fee_on_transfer_small_amount_rejected`, `test_large_fee_token_rejected`, `test_no_op_transfer_token_rejected`, `test_no_op_transfer_exact_balance_rejected`                                                    |
| 41         | `RecipientBalanceDeltaMismatch`          | rebasing / hook tokens          | `test_rebasing_token_over_credits_rejected`, `test_hook_token_extra_mint_rejected`                                                                                                                                                                          |
| —          | (no error, control)                      | standard SEP-41 token           | `test_compliant_token_passes`, `test_minimum_amount_passes`, `test_large_transfer_no_overflow`, `test_multiple_sequential_transfers`, `test_sender_ends_at_zero_balance`, `test_exact_balance_transfer_passes`, `test_fee_token_boundary_amount_one_passes` |

### Token archetypes tested

**Fee-on-transfer (`FeeOnTransferToken`)** — steals 1% on every `transfer`; sender is fully debited but recipient receives only 99%. Models real-world DeFi tokens that fund a DAO treasury from each transfer. Triggers `SenderBalanceDeltaMismatch` (the wrapper's first conservation assertion).

**Large-fee (`LargeFeeToken`)** — deducts 50% per transfer. Tests that the same error path fires for extreme deflationary tokens where the discrepancy is large.

**Rebasing (`RebasingToken`)** — credits the recipient with `amount * 2`. Models auto-compounding or interest-bearing tokens that mint bonus supply during transfer. Triggers `RecipientBalanceDeltaMismatch`.

**Hook (`HookToken`)** — correctly debits the sender but also mints an extra 10% to the recipient as a "rewards hook". Triggers `RecipientBalanceDeltaMismatch` with a smaller over-credit than the rebasing token.

**No-op transfer (`NoOpTransferToken`)** — `transfer` accepts the call but moves nothing. Models a paused or frozen token where `transfer` silently succeeds without updating balances. Triggers `SenderBalanceDeltaMismatch` because `spent == 0`.

### Security invariant summary

Every non-compliant token path **safe-fails**: the wrapper panics with a typed `EscrowError` code rather than silently accepting a mismatched transfer. No production code changes were required; the existing wrapper already enforces all delta invariants. These tests prove the invariants hold for each adversarial archetype.
