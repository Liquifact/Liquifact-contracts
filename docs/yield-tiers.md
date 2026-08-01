# Yield Tier Selection and Rounding Specification

This document details how investor contribution amounts map to yield tiers in the Liquifact Escrow contract (`contracts/escrow/src/lib.rs`). It defines the tier table structure, the tier lookup algorithm, boundary rules, and payout rounding behavior.

---

## 1. Tier Table Structure

The yield tier table is configured during contract initialization (`init`) or table configuration entrypoints and stored as `DataKey::YieldTierTable`.

Each entry in the table follows the `YieldTier` structure (defined in `contracts/escrow/src/types.rs`):

| Field | Type | Description |
| :--- | :--- | :--- |
| `min_amount` | `i128` | Minimum contribution amount required to qualify for this tier (in token base units). |
| `yield_bps` | `u32` | Yield rate expressed in Basis Points ($1\text{ BPS} = 0.01\% = 0.0001$). |
| `committed_lock_secs` | `u64` | Minimum lock duration associated with this tier (if applicable). |

### Configuration Invariants (`validate_yield_tiers_table`)

When the tier table is initialized, `validate_yield_tiers_table` enforces the following rules:
1. **Monotonicity**: Tiers must be sorted in strictly ascending order by `min_amount`.
2. **Non-Decreasing Yield**: Tiers with higher `min_amount` must have equal or higher `yield_bps`.
3. **Non-Negative Amounts**: `min_amount` for any tier must be greater than or equal to `0`.

---

## 2. Selection Algorithm and Entrypoints

### Cross-Referenced Contract Entrypoints (`contracts/escrow/src/lib.rs`)

* `init(env, ...)`: Accepts and validates the `YieldTierTable` configuration via `validate_yield_tiers_table`.
* `fund_with_commitment(env, investor, amount, lock_secs)`: Reads `YieldTierTable` and calls `effective_yield_for_commitment` to evaluate the contribution against configured tiers.
* `get_yield_tiers(env)`: Read-only view returning the active ordered tier list.

### Lookup Logic (`effective_yield_for_commitment`)

When a user contributes an `amount`, the contract iterates through the sorted `YieldTierTable` to find the highest qualifying tier:

```rust
// Logical equivalent of effective_yield_for_commitment tier evaluation
pub fn select_yield_tier(tiers: &[YieldTier], amount: i128) -> Option<YieldTier> {
    tiers.iter()
         .filter(|tier| amount >= tier.min_amount)
         .last()
         .cloned()
}

If the contribution amount is smaller than the min_amount of the lowest configured tier, the base yield rate (base_yield_bps) applies.
```

### 3. Boundary Rules

```rust
Tier thresholds use inclusive lower bounds ($\ge$):
    Exact Threshold Match: If contribution_amount == tier.min_amount, the contribution qualifies for that higher tier.
    Below Threshold: If contribution_amount == tier.min_amount - 1, the contribution falls into the lower preceding tier (or base yield).
        $$\text{Tier Selected} = \max_{k} \{ \text{Tier}_k \mid \text{Amount} \ge \text{Tier}_k.\text{min\_amount} \}$$
```
### 4. Rate Calculation and Rounding Rules
```rust
Yield payouts are calculated using basis points integer arithmetic:
$$\text{Yield Amount} = \left\lfloor \frac{\text{Contribution Amount} \times \text{Yield BPS}}{10\,000} \right\rfloor$$
### Rounding Policy
    Integer Truncation (Floor Rounding): Rust's standard / operator for integer types performs floor division (truncation toward zero).
    Dust Amount Handling: Any remaining fractional token unit resulting from division is discarded in favor of the contract reserve (preventing contract over-distribution).
```

## 5. Worked Numeric Examples

### Example Tier Configuration Table

| Tier ID | `min_amount` (Base Units / XLM) | `yield_bps` | Effective Yield % |
| :--- | :--- | :--- | :--- |
| **Base** | < 1,000 | 500 BPS | 5.00% |
| **Tier 1** | 1,000 | 750 BPS | 7.50% |
| **Tier 2** | 5,000 | 1,000 BPS | 10.00% |
| **Tier 3** | 10,000 | 1,500 BPS | 15.00% |

## 3. Unit Test Verification

To ensure your documentation remains in sync with code execution, verify boundary behavior by running unit tests in `contracts/escrow/src/tests/funding.rs` or adding assertions similar to:

```rust
#[test]
fn test_tier_boundary_selection() {
    let tiers = vec![
        YieldTier { min_amount: 1_000, yield_bps: 750, committed_lock_secs: 0 },
        YieldTier { min_amount: 5_000, yield_bps: 1_000, committed_lock_secs: 0 },
    ];

    // Below Tier 1 -> None (Base yield)
    assert_eq!(select_yield_tier(&tiers, 999), None);
    
    // Inclusive boundary match -> Tier 1
    assert_eq!(select_yield_tier(&tiers, 1_000).unwrap().yield_bps, 750);
    
    // Just below Tier 2 -> Tier 1
    assert_eq!(select_yield_tier(&tiers, 4_999).unwrap().yield_bps, 750);
    
    // Inclusive boundary match -> Tier 2
    assert_eq!(select_yield_tier(&tiers, 5_000).unwrap().yield_bps, 1_000);
}


