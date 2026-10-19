# Escrow Numeric Model

This contract uses Soroban host values and Rust integer types directly. It does not emulate EVM integer wrapping, fixed-point decimals, or token-specific decimal rules.

## Amounts: `i128`

- Funding amounts, targets, contributions, dust-sweep amounts, and collateral metadata amounts are stored as signed `i128` values.
- State-changing entrypoints that accept funding-like amounts require strictly positive values before storage updates.
- Token amounts must be passed in the token's smallest unit. The escrow contract does not read token decimals to rescale user-facing amounts.
- `funded_amount` is accumulated with `checked_add`. If `funded_amount + amount` exceeds `i128::MAX`, the contract panics with `funded_amount overflow` and the Soroban invocation aborts.
- Per-investor `InvestorContribution(Address)` is accumulated with `checked_add`. If `prev_contribution + amount` exceeds `i128::MAX`, the contract panics with `investor contribution overflow` and the Soroban invocation aborts.
- The contract does not saturate, clamp, or intentionally wrap funding totals.

### Init amount upper bound: `MAX_INVOICE_AMOUNT`

`LiquifactEscrow::init` rejects `amount > MAX_INVOICE_AMOUNT` with [`EscrowError::AmountExceedsMax`](../escrow/src/lib.rs) (code 14) to prevent overflow in settlement-time payout arithmetic. This is a **constructor-time guard** — no valid init can produce an escrow where `compute_investor_payout` overflows.

**Value:** `MAX_INVOICE_AMOUNT = (1 << 63) - 1 = 9_223_372_036_854_775_807` (i.e. `floor(√(i128::MAX / 2))`).

**Derivation** (see the constant's doc comment in [`escrow/src/lib.rs`](../escrow/src/lib.rs)):

```text
coupon       = total_principal × yield_bps / 10_000  (floor)   (1)
settle_pool  = total_principal + coupon                        (2)
gross_payout = contribution × settle_pool / total_principal    (3)
```

The tightest constraint is step (3): with worst-case `yield_bps = 10_000` and a single investor (`contribution = total_principal`), the intermediate product is `total_principal × 2 × total_principal = 2 × total_principal²`. Requiring this to stay within `i128` yields `total_principal ≤ floor(√(i128::MAX / 2)) = 2⁶³ − 1`. This is stricter than both the step (1) bound (`i128::MAX / 10_000`) and the step (2) bound (`i128::MAX / 2`).

**Tests:**
- `test_cost_baseline_init_max_amount` — accepting exactly `MAX_INVOICE_AMOUNT`
- `test_init_amount_exceeds_max_rejected` — rejecting `MAX_INVOICE_AMOUNT + 1` with `AmountExceedsMax`
- `test_max_bound_funded_escrow_compute_investor_payout_no_overflow` — full funding + settlement with `yield_bps = 10_000` at the bound

### Funding envelope: target and funded_amount bounds

Two additional guards keep every value feeding the payout math inside the
`init`-time envelope (issue #1535). Together they establish the chain:

```text
funding_target ≤ escrow.amount ≤ MAX_INVOICE_AMOUNT   (250 + init)
escrow.funded_amount ≤ MAX_INVOICE_AMOUNT             (251)
```

**`update_funding_target` (code 250 — `TargetExceedsInvoiceAmount`):** rejects
`new_target > escrow.amount`. Because `init` already bounds
`escrow.amount ≤ MAX_INVOICE_AMOUNT`, the target can never exceed
`MAX_INVOICE_AMOUNT`, so the payout math (which scales with `total_principal`)
stays inside its proven overflow-free envelope for every
`yield_bps ∈ [0, 10_000]`. Without this guard an admin could raise the target
up to `i128::MAX`, letting investors advance more than the invoice is worth and
push `funded_amount` past `MAX_INVOICE_AMOUNT`.

**`fund_impl` (code 251 — `FundedAmountExceedsSafeEnvelope`):** after the
`checked_add` on `funded_amount`, rejects `funded_amount > MAX_INVOICE_AMOUNT`.
This keeps over-funding (which the contract permits past the target) bounded
so that `FundingCloseSnapshot.total_principal ≤ MAX_INVOICE_AMOUNT` and
`compute_investor_payout` can never trap with
`ComputePayoutArithmeticOverflow` for any `yield_bps ∈ [0, 10_000]`.

**Per-investor contribution:** `fund_impl` also `checked_add`s the investor's
contribution and rejects `i128` overflow with `InvestorContributionOverflow`
(code 105). Because an individual investor's contribution is always
`≤ funded_amount ≤ MAX_INVOICE_AMOUNT`, no extra envelope check is needed on
`new_contribution` — the `funded_amount` bound (251) is the limiting one.

**Tests:**
- `tests::envelope_bounds::update_funding_target_above_invoice_amount_rejected` — 250
- `tests::envelope_bounds::update_funding_target_equal_to_invoice_amount_succeeds` — boundary inclusive
- `tests::envelope_bounds::update_funding_target_below_invoice_amount_succeeds` — lowering
- `tests::envelope_bounds::update_funding_target_lower_to_funded_promotes_to_funded` — auto-promotion preserved
- `tests::envelope_bounds::fund_at_max_invoice_amount_boundary_then_reject` — 251 + state unchanged
- `tests::envelope_bounds::fund_with_commitment_at_envelope_boundary_rejected` — 251 via commitment path
- `tests::envelope_bounds::compute_investor_payout_no_overflow_at_max_invoice_amount` — payout at the bound
- `tests::properties::prop_payout_no_overflow_within_envelope` — proptest sweep

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

## Refund conservation (cancelled escrows)

In status **cancelled** (4), the following invariants hold for all refund orderings:

- Each `refund(investor)` returns at most that investor's recorded contribution.
- `DistributedPrincipal` increases atomically per refund and never exceeds `funded_amount`.
- Once every investor has refunded, `DistributedPrincipal == funded_amount`.
- Double-refund is impossible: contribution is zeroed before the token transfer.

These properties are validated in `escrow/src/tests/properties.rs` (`prop_refund_conservation_never_exceeds_funded_principal`).

## Integration Guidance

- Off-chain callers should validate amount and lock-duration inputs before submitting transactions, especially when simulating near integer limits.
- Risk and accounting systems should use integer arithmetic for base-unit amounts and rational math for pro-rata ratios; avoid floating-point rounding when reconciling on-chain state.
- Maturity and claim-lock checks are ledger-time checks, not wall-clock oracle checks.
- Unsupported token economics remain out of scope. Fee-on-transfer, rebasing, malicious, or callback-heavy tokens are covered separately in [`escrow/src/external_calls.rs`](../escrow/src/external_calls.rs) and [`ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`](ESCROW_TOKEN_INTEGRATION_CHECKLIST.md).

