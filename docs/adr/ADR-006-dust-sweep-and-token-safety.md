# ADR-006: Treasury Dust Sweep and Token Safety

**Status:** Accepted  
**Date:** 2026-03-28  
**Refs:** `escrow/src/lib.rs` — `sweep_terminal_dust`, `DataKey::FundingToken`, `DataKey::Treasury`; `escrow/src/external_calls.rs` — `transfer_funding_token_with_balance_checks`

---

## Context

After settlement or withdrawal, small residual token balances (rounding dust, stray transfers) may remain in the contract. A recovery path is needed that cannot be abused to drain live principal.

## Decision

`sweep_terminal_dust(amount)` transfers `min(amount, balance, MAX_DUST_SWEEP_AMOUNT)` of the bound funding token to the immutable treasury address. Guards:

1. `status` must be `2` (settled) or `3` (withdrawn) — open/funded escrows are rejected.
2. `amount <= MAX_DUST_SWEEP_AMOUNT` (100,000,000 base units) — caps blast radius per call.
3. Legal hold blocks the sweep.
4. Treasury auth required.

All token transfers go through `external_calls::transfer_funding_token_with_balance_checks`, which:
- Records sender and recipient balances before the transfer.
- Calls `token.transfer`.
- Asserts sender decreased by exactly `amount` and recipient increased by exactly `amount`.

This catches fee-on-transfer tokens and malicious implementations at the host boundary (safe failure via panic).

`MAX_DUST_SWEEP_AMOUNT` is a compile-time constant. Tune it per asset decimals off-chain before deployment.

## Consequences

- Only the configured SEP-41 funding token can be swept; other assets sent to the contract are untouched.
- Soroban does not allow classic EVM-style synchronous reentrancy, but the pre/post balance check still catches non-standard token economics.
- Integrations that custody principal on-chain must keep token balances reconciled with `funded_amount` so sweeps cannot pull user funds.

## Rejected alternatives

- **Unrestricted sweep in any state:** would allow draining live principal as "dust."
- **No balance delta check:** would silently accept fee-on-transfer tokens and produce incorrect accounting.
- **Admin auth on sweep instead of treasury:** admin and treasury are separate roles by design; conflating them reduces separation of concerns.
