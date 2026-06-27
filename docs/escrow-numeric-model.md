# Escrow Numeric Model

This contract uses Soroban host values and Rust integer types directly. It does not emulate EVM integer wrapping, fixed-point decimals, or token-specific decimal rules.

## Amounts: `i128`

- Funding amounts, targets, contributions, dust-sweep amounts, and collateral metadata amounts are stored as signed `i128` values.
- State-changing entrypoints that accept funding-like amounts require strictly positive values before storage updates.
- Token amounts must be passed in the token's smallest unit. The escrow contract does not read token decimals to rescale user-facing amounts.
- `funded_amount` is accumulated with `checked_add`. If `funded_amount + amount` exceeds `i128::MAX`, the contract panics with `funded_amount overflow` and the Soroban invocation aborts.
- Per-investor `InvestorContribution(Address)` is accumulated with `checked_add`. If `prev_contribution + amount` exceeds `i128::MAX`, the contract panics with `investor contribution overflow` and the Soroban invocation aborts.
- The contract does not saturate, clamp, or intentionally wrap funding totals.

## Commitment locks: `u64`

- Ledger timestamps and lock durations use `u64` seconds from `Env::ledger().timestamp()`.
- `fund_with_commitment` stores `InvestorClaimNotBefore` as `now + committed_lock_secs` when the commitment is non-zero.
- That addition uses `checked_add`. If the result would exceed `u64::MAX`, the contract panics with `investor claim time overflow` and the Soroban invocation aborts.
- A zero commitment stores `0`, meaning no additional investor claim-time gate.
- Boundary values are inclusive: a timestamp plus commitment that equals `u64::MAX` is representable; only values above `u64::MAX` fail.

## Funding invariants (property-based)

This contract’s funding accounting and state transitions are intended to obey these invariants for all orderings of `fund` / `fund_with_commitment` calls.

- **Conservation (principal accounting):** while the escrow is open, `escrow.funded_amount` must equal the sum of every investor’s stored `get_contribution(addr)`.
- **Unique funder count:** `get_unique_funder_count()` must equal the number of distinct investor addresses whose `get_contribution(addr) > 0`.
- **Cap enforcement (never exceeded):**
  - When `max_per_investor` is configured, each investor’s running contribution must never exceed the configured cap.
  - When `max_unique_investors` is configured, the contract must never allow more distinct funders than the configured cap.
- **Status transition:** `escrow.status` must flip from `0` (open) to `1` (funded) **exactly at the first call** where `funded_amount >= funding_target` becomes true.
- **FundingCloseSnapshot semantics:** on the funded transition, `FundingCloseSnapshot` is written once with `total_principal == escrow.funded_amount` (including over-funding), and it must remain immutable across later reads.

These invariants are validated with randomized property tests in `escrow/src/tests/properties.rs`.

## Integration Guidance

- Off-chain callers should validate amount and lock-duration inputs before submitting transactions, especially when simulating near integer limits.
- Risk and accounting systems should use integer arithmetic for base-unit amounts and rational math for pro-rata ratios; avoid floating-point rounding when reconciling on-chain state.
- Maturity and claim-lock checks are ledger-time checks, not wall-clock oracle checks.
- Unsupported token economics remain out of scope. Fee-on-transfer, rebasing, malicious, or callback-heavy tokens are covered separately in [`escrow/src/external_calls.rs`](../escrow/src/external_calls.rs) and [`ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`](ESCROW_TOKEN_INTEGRATION_CHECKLIST.md).

