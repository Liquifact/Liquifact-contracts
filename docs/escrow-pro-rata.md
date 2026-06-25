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

## On-Chain Aggregate View: `get_settlement_pool`

The contract also exposes the aggregate base-yield repayment pool:

```
LiquifactEscrow::get_settlement_pool() -> i128
```

This view returns `0` until `DataKey::FundingCloseSnapshot` exists. After funding closes, it returns:

```text
coupon          = total_principal * escrow.yield_bps / 10_000  (floor)
settlement_pool = total_principal + coupon
```

`get_settlement_pool` uses the escrow base `yield_bps` only. It is the SME-facing aggregate amount
owed at settlement and is not the sum of investor-specific tiered payouts. Tier-specific effective
yields remain investor-level accounting and are reflected by `compute_investor_payout(investor)`.

The view uses the same floor rounding and checked arithmetic guard as the payout formula, raising
`EscrowError::ComputePayoutArithmeticOverflow` on overflow instead of returning a divergent value.
