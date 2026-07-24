# Allowlist Pagination Feature

## Overview

The `get_allowlist_page` function provides bounded, paginated read-only access to the escrow contract's allowlist, enabling off-chain UIs and reconciliation tools to efficiently enumerate active allowlisted investors and their associated yield tiers.

## Function Signature

```rust
pub fn get_allowlist_page(env: Env, start: u32, limit: u32) -> Vec<AllowlistEntry>
```

### Parameters

- **`start`** (`u32`): Zero-based starting index in the allowlist for pagination
- **`limit`** (`u32`): Maximum number of entries to return (capped at `MAX_INVESTOR_READ_BATCH` = 50)

### Returns

`Vec<AllowlistEntry>` - A vector of allowlist entries containing:

```rust
pub struct AllowlistEntry {
    pub investor: Address,
    pub tier: u32,
}
```

Where:
- **`investor`**: The allowlisted investor's address
- **`tier`**: Zero-based tier index (0 = base yield or not funded, 1+ = tier from yield table)

## Tier Resolution Logic

The `tier` field is determined as follows:

1. **Investor has not funded**: Returns `tier = 0`
2. **Investor funded with base yield**: Returns `tier = 0`
3. **Investor funded with tier commitment**: Returns `tier = 1..n` where `n` is the 1-based index of the matching tier in the yield tier table
4. **No yield tier table configured**: All investors return `tier = 0`

## Key Features

### ✅ Bounded Execution
- Respects `MAX_INVESTOR_READ_BATCH` (50) limit to prevent unbounded gas usage
- Automatically caps `limit` parameter to the maximum allowed value

### ✅ Zero Panics
- Returns empty vector for:
  - Empty allowlist
  - `start >= allowlist_length`
  - `limit == 0`
- Never panics on out-of-bounds queries

### ✅ Read-Only
- No storage writes
- No token transfers
- Safe to call from any context

### ✅ Filters Revoked Entries
- Only returns investors where `InvestorAllowlisted(addr) == true`
- Automatically excludes revoked addresses from results

## Usage Examples

### Basic Pagination

```rust
// Get first 10 allowlist entries
let page1 = client.get_allowlist_page(&0, &10);

// Get next 10 entries
let page2 = client.get_allowlist_page(&10, &10);
```

### Complete Enumeration

```rust
let mut start = 0u32;
let page_size = 50u32;
let mut all_entries = Vec::new();

loop {
    let page = client.get_allowlist_page(&start, &page_size);
    if page.len() == 0 {
        break;
    }
    
    for i in 0..page.len() {
        all_entries.push(page.get(i).unwrap());
    }
    
    start += page_size;
}
```

### Processing Entries by Tier

```rust
let page = client.get_allowlist_page(&0, &50);

for i in 0..page.len() {
    let entry = page.get(i).unwrap();
    
    match entry.tier {
        0 => {
            // Base yield or not funded
            println!("Investor {:?} using base yield", entry.investor);
        }
        tier_idx => {
            // Using a specific tier
            println!("Investor {:?} using tier {}", entry.investor, tier_idx);
        }
    }
}
```

## Integration with Existing Functions

The `get_allowlist_page` function complements existing allowlist functions:

| Function | Purpose | Returns |
|----------|---------|---------|
| `is_investor_allowlisted(addr)` | Check single investor | `bool` |
| `get_allowlisted_investors(start, limit)` | Get addresses only | `Vec<Address>` |
| `get_allowlisted_investors_count()` | Count active entries | `u32` |
| **`get_allowlist_page(start, limit)`** | **Get addresses + tiers** | **`Vec<AllowlistEntry>`** |

## Performance Considerations

### Gas Efficiency
- Reads are bounded by `MAX_INVESTOR_READ_BATCH` (50)
- Single storage read for allowlist index
- One persistent storage read per active entry
- One tier table lookup (if configured)

### Recommended Page Size
- **Small allowlists (<100)**: Use `limit = 50` for single-page retrieval
- **Large allowlists (>100)**: Use `limit = 20-30` for better responsiveness
- **Real-time UIs**: Use `limit = 10-20` with progressive loading

## Edge Cases and Behavior

### Empty Allowlist
```rust
let page = client.get_allowlist_page(&0, &10);
assert_eq!(page.len(), 0); // Returns empty vector
```

### Out-of-Bounds Start
```rust
// Allowlist has 5 entries
let page = client.get_allowlist_page(&10, &5);
assert_eq!(page.len(), 0); // Returns empty vector
```

### Limit Exceeds Maximum
```rust
let page = client.get_allowlist_page(&0, &100);
assert!(page.len() <= 50); // Capped at MAX_INVESTOR_READ_BATCH
```

### Mixed Revocation States
```rust
// Allowlist index has [inv1, inv2, inv3]
// but inv2 is revoked (InvestorAllowlisted(inv2) = false)
let page = client.get_allowlist_page(&0, &10);
// Returns only [inv1, inv3]
```

### Allowlist Gate Disabled
The function works regardless of whether the allowlist gate is active:
```rust
assert!(!client.is_allowlist_active()); // Gate off
let page = client.get_allowlist_page(&0, &10);
// Still returns allowlisted investors
```

## Testing

The implementation includes comprehensive test coverage:

- ✅ Empty allowlist queries
- ✅ Out-of-bounds start index
- ✅ Zero limit
- ✅ Basic pagination (multiple pages)
- ✅ Limit capping at `MAX_INVESTOR_READ_BATCH`
- ✅ Revoked investor filtering
- ✅ Base yield tier resolution
- ✅ Tier 1 and Tier 2 resolution
- ✅ Mixed tier scenarios
- ✅ No tier table configured
- ✅ Large dataset pagination (100+ entries)
- ✅ Tier persistence after additional funding

All tests are located in `escrow/src/test_allowlist_tests.rs` under the section "get_allowlist_page tests".

## Implementation Details

### Storage Keys Read
1. **`DataKey::AllowlistIndex`** (instance): Ordered list of all allowlist addresses
2. **`DataKey::InvestorAllowlisted(addr)`** (persistent): Per-address allowlist flag
3. **`DataKey::InvestorEffectiveYield(addr)`** (persistent): Per-address effective yield (if funded)
4. **`DataKey::YieldTierTable`** (instance): Immutable tier table (if configured)
5. **`DataKey::Escrow`** (instance): Main escrow state for base yield

### Helper Function
The implementation includes a private helper function:

```rust
fn get_tier_index_for_investor(
    env: &Env,
    investor: &Address,
    base_yield: i64,
    tier_table: &Option<Vec<YieldTier>>,
) -> u32
```

This helper:
- Reads the investor's effective yield from persistent storage
- Compares it against the base yield and tier table
- Returns the appropriate tier index (0 for base, 1+ for tiers)

## Security Considerations

### ✅ No Authorization Required
This is a read-only view function that requires no authentication. This is intentional to allow:
- Public indexers to enumerate the allowlist
- Off-chain reconciliation tools to verify state
- UIs to display allowlist status to any viewer

### ✅ Privacy Note
The allowlist and tier information are **publicly readable on-chain**. If investor privacy is required, consider:
- Using off-chain allowlist management
- Encrypting investor identities
- Implementing permissioned read access (requires architecture changes)

### ✅ No State Mutations
The function cannot:
- Modify allowlist entries
- Change tier assignments
- Transfer tokens
- Update any storage keys

## Migration and Compatibility

### Schema Version
This feature is compatible with **schema version 6** and above.

### Backward Compatibility
- Works with escrows initialized without yield tiers (returns `tier = 0` for all)
- Works with empty allowlists (returns empty vector)
- Does not require migration of existing contracts

### Forward Compatibility
The `AllowlistEntry` struct can be extended in future versions by:
- Adding new fields with default values
- Maintaining the existing `investor` and `tier` fields

## Comparison with `get_allowlisted_investors`

| Feature | `get_allowlisted_investors` | `get_allowlist_page` |
|---------|----------------------------|---------------------|
| Returns addresses | ✅ | ✅ |
| Returns tier info | ❌ | ✅ |
| Bounded reads | ✅ (limit 50) | ✅ (limit 50) |
| Filters revoked | ✅ | ✅ |
| Pagination | ✅ | ✅ |
| Use case | Simple address enumeration | Tier-aware reconciliation |

## Future Enhancements

Potential future improvements:
1. **Streaming API**: For very large allowlists (1000+ entries)
2. **Filter by tier**: `get_allowlist_page_by_tier(tier_idx, start, limit)`
3. **Sorted by contribution**: Return entries sorted by funded amount
4. **Metadata inclusion**: Include contribution amount, effective yield value, etc.

## Related Documentation

- [Allowlist Feature](../docs/escrow-allowlist.md)
- [Tiered Yield](../docs/adr/ADR-005-tiered-yield.md)
- [Storage Keys](../docs/adr/ADR-007-storage-key-evolution.md)
- [Pagination Patterns](../docs/escrow-read-api.md)

## Changelog

### Version 1.0 (Initial Implementation)
- Added `AllowlistEntry` struct with `investor` and `tier` fields
- Implemented `get_allowlist_page(start, limit)` entrypoint
- Added `get_tier_index_for_investor` helper function
- Comprehensive test suite (20+ test cases)
- Full documentation
