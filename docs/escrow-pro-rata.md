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
