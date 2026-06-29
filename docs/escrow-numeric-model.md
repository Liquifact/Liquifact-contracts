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

## Protocol fee on SME withdrawal: `i64` basis points

The escrow supports an **immutable** protocol fee on the SME disbursement, configured once at
`init` via the optional `protocol_fee_bps: Option<i64>` parameter and stored under
`DataKey::ProtocolFeeBps`.

- **Range:** `protocol_fee_bps` is validated to `0..=10_000` at `init`
  (`EscrowError::ProtocolFeeBpsOutOfRange`). The default when omitted is `0` (no fee), which
  preserves the legacy behavior of routing the full `funded_amount` to the SME.
- **Split math (`withdraw`):**

  ```text
  fee        = funded_amount * protocol_fee_bps / 10_000   (integer floor)
  sme_payout = funded_amount - fee
  ```

  `fee` is transferred to `DataKey::Treasury` and `sme_payout` to `sme_address`. The treasury
  transfer is skipped entirely when `fee == 0`, and the SME transfer is skipped when
  `sme_payout == 0` (only reachable at `protocol_fee_bps == 10_000`).
- **Rounding:** the division floors, so any residue below one `10_000`-th of the principal stays
  with the SME. The treasury is never over-credited by rounding.
- **Conservation:** `sme_payout + fee == funded_amount` for every withdrawal — the split neither
  creates nor destroys principal. `DistributedPrincipal` still advances by the full gross
  `funded_amount`.
- **Overflow safety:** the multiplication `funded_amount * protocol_fee_bps` and the division use
  checked arithmetic. Because an escrow may be over-funded, `funded_amount` is not bounded by
  `MAX_INVOICE_AMOUNT`; if `funded_amount * 10_000` would exceed `i128::MAX` the contract panics
  with `EscrowError::WithdrawFeeArithmeticOverflow`. The subtraction is likewise checked
  (`EscrowError::WithdrawNetArithmeticUnderflow`, unreachable for in-range `fee_bps`).
- **Dependency on on-chain disbursement:** the fee is only realized when principal is custodied
  on-chain and the SME calls `withdraw`. It does **not** apply to off-chain `settle`, investor
  `refund`, or `claim_investor_payout`. See [`ESCROW_SME_WITHDRAWAL.MD`](ESCROW_SME_WITHDRAWAL.MD).
- **Event:** `SmeWithdrew` is extended append-only with a `fee` field; its `amount` field carries
  the **net** SME payout, and `amount + fee` reconstructs the gross `funded_amount`.

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

