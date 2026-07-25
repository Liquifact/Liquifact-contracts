# ADR-006: Treasury Dust Sweep and Token Safety

**Status:** Accepted  
**Date:** 2026-03-28  
**Updated:** 2026-05-28 — liability floor invariant  
**Refs:** `escrow/src/lib.rs` — `sweep_terminal_dust`, `refund`, `DataKey::FundingToken`, `DataKey::Treasury`, `DataKey::DistributedPrincipal`; `escrow/src/external_calls.rs` — `transfer_funding_token_with_balance_checks`

---

## Context

After settlement or withdrawal, small residual token balances (rounding dust, stray transfers) may remain in the contract. A recovery path is needed that cannot be abused to drain live principal.

The original design relied on off-chain reconciliation to ensure sweeps only touched true dust. This left an on-chain gap: nothing prevented the treasury from sweeping tokens that were still owed to unredeemed investors in a cancelled escrow.

## Decision

`sweep_terminal_dust(amount)` transfers `min(amount, balance, MAX_DUST_SWEEP_AMOUNT)` of the bound funding token to the immutable treasury address. Guards:

1. `status` must be `2` (settled), `3` (withdrawn), or `4` (cancelled) — open/funded escrows are rejected.
2. `amount <= MAX_DUST_SWEEP_AMOUNT` (100,000,000 base units) — caps blast radius per call.
3. Legal hold blocks the sweep.
4. Treasury auth required.
5. **Liability floor (new):** the sweep is rejected if it would reduce the contract balance below outstanding investor liabilities.

### Liability floor invariant

The floor only applies in **cancelled (status 4)** escrows, where `refund` is the on-chain redemption path. In settled (2) and withdrawn (3) states, disbursement is off-chain and `distributed_principal` stays `0`, so the floor is not applicable.

```
outstanding = funded_amount - distributed_principal
assert balance - sweep_amt >= outstanding   [only when status == 4]
```

`DataKey::DistributedPrincipal` is a running total incremented atomically by `refund` each time an investor's principal is returned. This makes the invariant computable on-chain without iterating over all investor addresses.

- In a **settled** or **withdrawn** escrow, `funded_amount` reflects the total principal committed. If the integration has not yet disbursed principal off-chain, `distributed_principal` remains `0` and `outstanding == funded_amount`. The sweep is blocked until the balance genuinely exceeds liabilities.
- In a **cancelled** escrow, each `refund` call increments `distributed_principal`, reducing `outstanding`. Once all investors are refunded, `outstanding == 0` and any remaining balance is true dust.

All token transfers go through `external_calls::transfer_funding_token_with_balance_checks`, which:
- Records sender and recipient balances before the transfer.
- Calls `token.transfer`.
- Asserts sender decreased by exactly `amount` and recipient increased by exactly `amount`.

This catches fee-on-transfer tokens and malicious implementations at the host boundary (safe failure via panic).

`MAX_DUST_SWEEP_AMOUNT` is a compile-time constant. Tune it per asset decimals off-chain before deployment.

## Consequences

- Only the configured SEP-41 funding token can be swept; other assets sent to the contract are untouched.
- Soroban does not allow classic EVM-style synchronous reentrancy, but the pre/post balance check still catches non-standard token economics.
- `DataKey::DistributedPrincipal` is an additive key (absent ⇒ `0`), backward-compatible per ADR-007.
- Integrations that custody principal on-chain must keep token balances reconciled with `funded_amount`. The liability floor now enforces this on-chain for the cancelled-state refund path.
- For settled/withdrawn escrows where disbursement is off-chain, the floor does not apply and dust sweeps work as before.

## Rejected alternatives

- **Unrestricted sweep in any state:** would allow draining live principal as "dust."
- **No balance delta check:** would silently accept fee-on-transfer tokens and produce incorrect accounting.
- **Admin auth on sweep instead of treasury:** admin and treasury are separate roles by design; conflating them reduces separation of concerns.
- **Iterate all investor contributions to compute outstanding:** unbounded gas cost; the `DistributedPrincipal` counter achieves the same result in O(1).

## Test Coverage

Liability floor tests live in `escrow/src/tests/external_calls.rs`:

| Test name | What it checks | Expected result |
|-----------|---------------|-----------------|
| `sweep_liability_floor_allows_true_dust_after_all_refunded` | All refunded → outstanding = 0, dust swept | Passes |
| `sweep_liability_floor_blocks_sweep_when_investor_not_yet_refunded` | No refunds, balance = outstanding | Panics |
| `sweep_liability_floor_allows_sweep_of_excess_above_outstanding` | Partial refund, sweep only the surplus | Passes |
| `sweep_liability_floor_blocks_sweep_that_would_eat_into_outstanding` | Sweep exceeds surplus | Panics |
| `sweep_liability_floor_zero_funded_amount_allows_sweep` | Cancelled before any funding, stray tokens | Passes |
| `distributed_principal_accumulates_across_multiple_refunds` | Counter increments correctly per refund | Passes |

Balance-delta invariant tests live in `escrow/src/tests/external_calls_mocked.rs`:

| Test name | What it checks | Expected result |
|-----------|---------------|-----------------|
| `test_fee_on_transfer_token_rejected` | 1% fee token under-credits recipient | Panics |
| `test_balance_delta_conservation_with_standard_token` | Normal token, happy path | Passes |
| `test_zero_amount_rejected` | amount = 0 | Panics |
| `test_negative_amount_rejected` | amount = -50 | Panics |
| `test_insufficient_balance_rejected` | sender has 500, tries to send 1000 | Panics |
| `test_balance_delta_invariants_with_large_transfers` | Very large amount, no overflow | Passes |
| `test_balance_delta_invariants_with_multiple_recipients` | Two sequential transfers | Passes |

**Core invariant:** value is always conserved exactly, or the call panics. There is no partial-credit success path.

## On-chain custody reconciliation view

The `get_token_balance()` view function exposes the contract's SEP-41 token balance directly for auditor convenience. This eliminates the need for auditors to independently query the token contract with the funding-token address.

```
balance = get_token_balance()
outstanding = get_escrow().funded_amount - get_distributed_principal()
excess = balance - outstanding   // tokens that may be swept as dust

// In cancelled escrows, sweep_terminal_dust enforces: balance - sweep_amt >= outstanding
```

- `get_token_balance()` **reads only**; it does not mutate state or require authorization.
- It fails with `FundingTokenNotSet` if the escrow has not been initialized, matching the behavior of `get_funding_token()`.
- Integrations custodying principal on-chain should verify `balance >= funded_amount - distributed_principal` at any point in time.
