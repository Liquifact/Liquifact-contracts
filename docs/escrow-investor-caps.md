# Escrow Investor Caps - MaxUniqueInvestorsCap and UniqueFunderCount

## Overview

The LiquiFact escrow contract provides configurable limits on the number of distinct investor addresses that can contribute to an invoice escrow. This feature helps manage risk, compliance requirements, and operational complexity by enforcing Sybil-limited counter semantics.

## Key Concepts

### Sybil Limitations

**Important:** The investor cap limits distinct **chain addresses**, not real-world persons. The contract does not implement Sybil resistance mechanisms - it simply counts unique wallet addresses. This is explicitly documented as:

- **What is limited:** Distinct blockchain addresses (public keys)
- **What is NOT limited:** Real-world individuals or entities
- **Assumption:** One address = one investor for operational purposes

### Storage Schema

- `DataKey::MaxUniqueInvestorsCap`: Optional `u32` cap on distinct investors
- `DataKey::UniqueFunderCount`: Current count of distinct funders (initialized to 0)

## Implementation Details

### Counter Semantics

The `UniqueFunderCount` increments **only** when an address makes its **first non-zero contribution**:

```rust
// In fund_impl - simplified logic
let prev: i128 = env.storage().instance().get(&contribution_key).unwrap_or(0);

if prev == 0 {
    // First time this address is funding
    if let Some(cap) = max_unique_investors_cap {
        let cur: u32 = env.storage().instance().get(&DataKey::UniqueFunderCount).unwrap_or(0);
        assert!(cur < cap, "unique investor cap reached");
    }
}

// ... funding logic ...

if prev == 0 {
    // Increment counter after successful funding
    let cur: u32 = env.storage().instance().get(&DataKey::UniqueFunderCount).unwrap_or(0);
    env.storage().instance().set(&DataKey::UniqueFunderCount, &(cur + 1));
}
```

### Cap Enforcement

- **When checked:** Before processing a new investor's first contribution
- **What is checked:** `current_unique_funders < configured_cap`
- **Panic message:** `"unique investor cap reached"`
- **Edge case:** Existing investors can always add more principal (doesn't count against cap)

### Initialization

The cap is set during escrow initialization via the `max_unique_investors` parameter:

```rust
pub fn init(
    // ... other parameters
    max_unique_investors: Option<u32>,
) -> InvoiceEscrow
```

- `None`: No cap (unlimited investors)
- `Some(n)`: Cap of `n` distinct investors
- **Validation:** Cap must be positive if configured (`> 0`)

## API Reference

### Query Functions

#### `get_max_unique_investors_cap(env: Env) -> Option<u32>`

Returns the configured cap, or `None` if unlimited.

#### `get_unique_funder_count(env: Env) -> u32`

Returns the current count of distinct funders.

### Usage Examples

#### Initialize with Cap

```rust
// Cap of 10 investors
client.init(
    &admin,
    &invoice_id,
    &sme,
    &amount,
    &yield_bps,
    &maturity,
    &funding_token,
    &registry,
    &treasury,
    &yield_tiers,
    &min_contribution,
    &Some(10u32), // Max 10 investors
);
```

#### Initialize without Cap

```rust
// Unlimited investors
client.init(
    &admin,
    &invoice_id,
    &sme,
    &amount,
    &yield_bps,
    &maturity,
    &funding_token,
    &registry,
    &treasury,
    &yield_tiers,
    &min_contribution,
    &None, // No cap
);
```

## Edge Cases and Behavior

### 1. Re-funding Same Address

```rust
// Investor 1 funds first time
client.fund(&investor1, &1000);
// unique_funder_count = 1

// Same investor funds again
client.fund(&investor1, &500);
// unique_funder_count = 1 (unchanged)
```

### 2. Cap Exhaustion

```rust
// Cap = 2, 2 investors have funded
// unique_funder_count = 2

// New investor tries to fund
client.fund(&investor3, &1000); // PANICS: "unique investor cap reached"
```

### 3. Zero to Non-zero Transitions

```rust
// Address with 0 contribution (not counted)
assert_eq!(client.get_contribution(&investor), 0);
assert_eq!(client.get_unique_funder_count(), 0);

// First non-zero contribution
client.fund(&investor, &1000);
assert_eq!(client.get_unique_funder_count(), 1);
```

### 4. Interaction with Other Features

#### Minimum Contribution Floor

The cap works independently of the minimum contribution floor:

```rust
client.init(
    // ...
    &min_contribution: Some(1000),
    &max_unique_investors: Some(5),
);
```

Both validations are applied:
- Amount ≥ min_contribution
- unique_funder_count < max_unique_investors (for new investors)

#### Tiered Yield System

The cap applies to both `fund()` and `fund_with_commitment()`:

```rust
// First investor with commitment
client.fund_with_commitment(&investor1, &1000, &100);
// unique_funder_count = 1

// Second investor regular fund
client.fund(&investor2, &1000);
// unique_funder_count = 2
```

#### Allowlist System

The cap is checked AFTER allowlist validation:

```rust
// Process order for new investor:
// 1. Check allowlist (if active)
// 2. Check cap (if configured)
// 3. Check min contribution floor
// 4. Process funding
```

## Security Considerations

### Within Scope

- **Cap enforcement:** Strictly enforced with panic on violation
- **Counter accuracy:** Atomic operations prevent race conditions
- **Re-funding safety:** Existing investors can always add more principal

### Out of Scope

- **Sybil resistance:** No mechanism to prevent one person from using multiple addresses
- **Identity verification:** No KYC/AML integration
- **Dynamic caps:** Caps cannot be changed after initialization
- **Cap reduction:** No mechanism to lower caps post-deployment

### Token Economics Assumptions

Per `escrow/src/external_calls.rs`, the cap system assumes:

- **Well-behaved tokens:** Standard SEP-41 compliance
- **No fee-on-transfer:** Amounts received match amounts sent
- **No rebase tokens:** Stable accounting for contribution tracking

Malicious token contracts could theoretically interfere with contribution accounting, but this is explicitly out of scope for the cap system.

## Testing Coverage

The implementation includes comprehensive tests covering:

### Basic Functionality
- Counter initialization to zero
- Increment on first investor
- No increment on re-funding same address
- Multiple distinct investors

### Cap Enforcement
- Cap validation at initialization
- Enforcement at limit
- Panic on excess investors
- Clear error messages

### Edge Cases
- Zero cap validation (should panic)
- Exact limit behavior
- Large contributions with small caps
- Interaction with minimum contribution floors

### Integration Tests
- `fund()` vs `fund_with_commitment()` behavior
- Tiered yield system compatibility
- Allowlist system interaction

## Migration and Compatibility

### Schema Version

The investor cap features were added in **schema version 3**:

```rust
/// | Version | Summary | Upgrade path |
/// |---------|---------|-------------|
/// | 3 | Added `FundingCloseSnapshot`, `MinContributionFloor`, `MaxUniqueInvestorsCap`, `UniqueFunderCount` | Additive keys — old instances return defaults |
```

### Backward Compatibility

- **Old instances:** Return `None` for cap, `0` for counter
- **No migration required:** Additive keys with safe defaults
- **New instances:** Can configure caps during initialization

## Operational Guidance

### Setting Appropriate Caps

Consider these factors when setting investor caps:

1. **Compliance requirements:** Regulatory limits on investor counts
2. **Operational capacity:** Ability to handle investor relationships
3. **Risk management:** Concentration risk vs. diversification benefits
4. **Target raise size:** Balance cap with funding target

### Monitoring

Monitor these metrics during live operation:

- `unique_funder_count` vs. `max_unique_investors_cap`
- Time to reach cap (if any)
- Average contribution per unique investor
- Re-funding patterns (existing investors adding more)

### Emergency Procedures

If cap exhaustion becomes an issue:

1. **No on-chain solution:** Caps cannot be increased post-deployment
2. **New escrow deployment:** Required for different cap settings
3. **Off-chain coordination:** Direct investors to new escrow instances

## Best Practices

### Configuration

```rust
// Recommended: Set caps based on realistic operational capacity
let reasonable_cap = match target_amount {
    0..=1_000_000 => Some(50),      // Small deals: more investors
    1_000_001..=10_000_000 => Some(20), // Medium deals: moderate investors
    _ => Some(10),                  // Large deals: fewer investors
};
```

### Error Handling

```rust
// Client-side: Check cap before attempting funding
if let (Some(cap), current_count) = (client.get_max_unique_investors_cap(), client.get_unique_funder_count()) {
    if current_count >= cap {
        return Err(InvestorCapExceeded);
    }
}
client.fund(&investor, &amount);
```

### Documentation

When deploying capped escrows:

1. **Clearly communicate caps** to potential investors
2. **Document rationale** for cap selection
3. **Provide alternative escrows** if caps may be reached
4. **Monitor cap utilization** in real-time

## Conclusion

The MaxUniqueInvestorsCap and UniqueFunderCount functionality provides a robust, Sybil-limited mechanism for controlling investor participation in LiquiFact escrows. While it doesn't prevent Sybil attacks, it offers operational control and compliance benefits with clear semantics and comprehensive edge case handling.

The implementation prioritizes safety and predictability, with strict enforcement and clear error messages. Organizations should carefully consider their cap requirements during deployment, as caps cannot be modified after initialization.
