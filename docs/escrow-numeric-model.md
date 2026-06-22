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

### Tiered-yield examples

Tiered yield uses integer basis points. A base yield of `500` means 5%, and a
tier yield of `800` means 8%. The contract stores the selected yield in
`InvestorEffectiveYield(Address)` on the investor's first deposit.

With this tier table:

| `min_lock_secs` | `yield_bps` |
|---:|---:|
| `2_592_000` | `650` |
| `7_776_000` | `800` |
| `15_552_000` | `950` |

and base `yield_bps = 500`, the selection rules are:

| `committed_lock_secs` | Stored effective yield | Stored claim lock |
|---:|---:|---|
| `0` | `500` | `0` |
| `2_592_000` | `650` | `ledger.timestamp() + 2_592_000` |
| `5_184_000` | `650` | `ledger.timestamp() + 5_184_000` |
| `7_776_000` | `800` | `ledger.timestamp() + 7_776_000` |
| `15_552_000` | `950` | `ledger.timestamp() + 15_552_000` |

`tier_lock_secs` in `EscrowFunded` records the matched tier threshold, while
`InvestorClaimNotBefore` records the full investor commitment duration. A
60-day commitment can therefore emit `tier_lock_secs = 2_592_000` while storing
`InvestorClaimNotBefore = now + 5_184_000`.

Follow-on principal uses `fund()` and does not recalculate these values.

## Integration Guidance

- Off-chain callers should validate amount and lock-duration inputs before submitting transactions, especially when simulating near integer limits.
- Risk and accounting systems should use integer arithmetic for base-unit amounts and rational math for pro-rata ratios; avoid floating-point rounding when reconciling on-chain state.
- Maturity and claim-lock checks are ledger-time checks, not wall-clock oracle checks.
- UI and SDK code should display both the matched tier threshold and the full
  commitment unlock timestamp when they differ.
- Unsupported token economics remain out of scope. Fee-on-transfer, rebasing, malicious, or callback-heavy tokens are covered separately in [`escrow/src/external_calls.rs`](../escrow/src/external_calls.rs) and [`ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`](ESCROW_TOKEN_INTEGRATION_CHECKLIST.md).
