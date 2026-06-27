# Pro-rata Payout Mathematics

This document specifies the rational math requirements for calculating investor shares and payouts based on the `FundingCloseSnapshot`.

## 🧮 Core Formula

The share of an investor is determined by their contribution relative to the **total principal** captured at the moment funding closed.

$$Share_{investor} = \frac{Contribution_{investor}}{TotalPrincipal_{snapshot}}$$

### Why `TotalPrincipal`?
Liquifact escrows allow **over-funding** (deposits that exceed the `funding_target` in the same ledger). The `TotalPrincipal` in the `FundingCloseSnapshot` is the authoritative denominator for all pro-rata calculations.

## 📏 Rounding Policy

To ensure protocol solvency and prevent "penny-bleeding" attacks, integrators should follow these rounding rules:

1.  **Investor Payouts**: Round **DOWN** (Floor) to the nearest base unit of the funding token.
2.  **Protocol Fees**: Calculated on the remainder after all investor payouts are rounded.
3.  **Intermediate Math**: Use high-precision rational arithmetic (e.g., `BigInt` or 256-bit fixed point) before the final rounding step.

## 📝 Example Calculation

**Scenario:**
- `funding_target`: 10,000 USDC
- `TotalPrincipal` (at close): 10,050 USDC (due to over-funding)
- `Investor A Contribution`: 1,000 USDC
- `Yield`: 500 USDC (5% fixed yield for this example)
- `Total Settle Amount`: 10,550 USDC

**Calculation:**
1.  **Pro-rata Share**: $1,000 / 10,050 \approx 0.09950248756...$
2.  **Gross Payout**: $0.09950248756 * 10,550 = 1,049.75124378...$
3.  **Final Payout (USDC base units)**: `1,049.75` (assuming 2 decimals) or `104,975,124` (assuming 7 decimals, rounded down).

## ⚠️ Security Notes

### Pro-rata Denominator Stability
The `FundingCloseSnapshot` is **immutable** once written. It is captured in two scenarios:
1.  **Full Funding**: Automatically written when `funded_amount >= funding_target`.
2.  **Partial Settlement**: Explicitly written by an Admin or SME via `partial_settle` for under-funded invoices.

Once captured, the pro-rata denominator remains fixed even if the SME withdraws a partial amount or more funds are somehow transferred to the contract. This ensures predictable payouts regardless of how the funding phase ended.

### Integer Overflow
When implementing off-chain in JS/Python, ensure you are using libraries that handle large integers (e.g., `BigInt` in JS) to prevent overflow during the `Contribution * TotalSettle` multiplication step before division.

```javascript
// Example JS implementation
function calculatePayout(contribution, totalPrincipal, settleAmount) {
  const c = BigInt(contribution);
  const tp = BigInt(totalPrincipal);
  const sa = BigInt(settleAmount);
  
  // Multiply before divide for precision
  return (c * sa) / tp; 
}
```

## 🔗 On-Chain View: `compute_investor_payout`

The contract exposes an authoritative on-chain implementation of the formula above:

```
LiquifactEscrow::compute_investor_payout(investor: Address) → i128
```

This view derives `effective_yield_bps` from `DataKey::InvestorEffectiveYield` (tiered ladder
selection from `fund_with_commitment`) and falls back to `InvoiceEscrow::yield_bps` for investors
who used plain `fund`. Off-chain tools **must** call this view rather than re-implementing the
formula to guarantee identical rounding.

### On-chain integer safety

- All intermediate multiplications use `i128::checked_mul`.
- All divisions use `i128::checked_div`.
- The function panics with `"compute_investor_payout: arithmetic overflow"` on overflow rather than
  silently returning a wrong value.
- `total_principal` is always positive when a `FundingCloseSnapshot` exists; the function
  guards against the `≤ 0` edge case and returns `0` early.

### Reference

See `docs/escrow-read-api.md` → `compute_investor_payout` for the full parameter, return-value,
and authorization documentation.

## ✅ Aggregate Payout Invariant (issue #483)

### Uniform yield

When all investors share the same `yield_bps`:

```
settled_pool = total_principal + total_principal × yield_bps / 10_000   (floor)
Σ payout_i  ≤ settled_pool
```

The inequality holds because floor division drops at most 1 unit per investor, so:

```
0 ≤ residue (= settled_pool − Σ payout_i) < n_investors
```

The residue is swept by the treasury via `sweep_terminal_dust` after all investors have claimed.

### Tiered / mixed yield

When investors carry per-investor effective yields (from `fund_with_commitment` tier selection),
each investor `i` has their own `settle_pool_i`:

```
settle_pool_i = total_principal + total_principal × effective_yield_bps_i / 10_000
payout_i      = contribution_i × settle_pool_i / total_principal   (floor)
```

Because floor division always gives `payout_i ≤ exact_i`, summing across all investors:

```
Σ payout_i ≤ Σ exact_i = Σ (contribution_i × settle_pool_i / total_principal)
           ≤ total_principal × (1 + max_effective_yield_bps / 10_000)
```

### Rounding guarantee

Rounding always favors the contract, never the investors collectively:

- No individual investor receives more than their exact rational entitlement.
- The aggregate can never exceed the exact weighted sum of entitlements.
- The residue is always non-negative — no shortfall is possible from rounding.

### Snapshot-denominator consistency

`FundingCloseSnapshot` is written exactly once when the escrow first reaches `status == 1`.
It is stored under `DataKey::FundingCloseSnapshot` and **never overwritten**. All
`compute_investor_payout` calls read the same `total_principal` denominator from this snapshot,
so the denominator cannot shift between investor claims.

This invariant is verified in `escrow/src/tests/properties.rs` by
`snapshot_denominator_consistent_across_all_payout_reads`, which reads the snapshot before
and after every individual payout call and asserts identity.

### Property-based test coverage (issue #483)

| Test | Coverage |
|------|----------|
| `prop_payout_sum_le_settle_pool` | Uniform yield, 2–6 investors, arbitrary amounts & yield |
| `prop_aggregate_payout_le_settle_pool_tiered` | Tiered/mixed yield, snapshot consistency, max-pool bound |
| `payout_single_investor_equals_settle_pool` | Single investor receives exact `settle_pool` |
| `payout_equal_split_conservation` | Equal contributions, sum ≤ `settle_pool` |
| `payout_zero_yield_returns_principal_only` | Zero yield: `payout_i == contribution_i` |
| `payout_max_yield_conservation` | 100% yield: conservation still holds |
| `payout_prime_denominator_residue_bounded` | Prime total → residue < `n_investors` |
| `payout_highly_skewed_contributions` | 99%/1% split; residue bounded |
| `payout_many_small_investors_conservation` | 8 investors × 1 unit; aggregate ≤ `settle_pool` |
| `payout_single_large_single_tiny` | Extreme asymmetry stress test |
| `payout_tiered_mixed_yield_conservation` | 3-tier mixed yield; per-investor & aggregate bounds |
| `snapshot_denominator_consistent_across_all_payout_reads` | Snapshot immutable across all reads |
| `fuzz_payout_conservation_multi_investor` | 64-case fuzz, 1–8 investors, full yield range |

## 🔗 On-Chain Payout Transfer: `claim_investor_payout`

When a settled investor calls `claim_investor_payout`, the contract:

1. Computes the gross payout via `compute_investor_payout`.
2. Guards against a zero payout (floor division edge case — `PayoutZero = 165`).
3. Marks the investor as claimed (persistent storage write).
4. Calls `external_calls::transfer_funding_token_with_balance_checks` to transfer the gross
   payout from the contract to the investor.
5. Emits `InvestorPayoutClaimed`.

### Atomicity

The claimed marker is written **before** the token transfer. If the transfer fails (e.g.,
insufficient contract balance, token transfer error, host trap), the Soroban host rolls back
all storage writes including the marker. The investor may retry.

### Idempotency

A second call from the same investor returns early (the marker is already set) and does **not**
re-transfer. The balance check in `transfer_funding_token_with_balance_checks` would fail a
second transfer anyway, but the early return avoids the call entirely.
