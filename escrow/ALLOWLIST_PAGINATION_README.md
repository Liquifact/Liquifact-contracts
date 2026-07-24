# Allowlist Pagination Implementation

## Quick Start

The `get_allowlist_page` function provides bounded, paginated read-only access to allowlisted investors and their yield tiers.

```rust
// Get first 10 allowlist entries
let page = client.get_allowlist_page(&0, &10);

for i in 0..page.len() {
    let entry = page.get(i).unwrap();
    // entry.investor = Address
    // entry.tier = 0 (base yield) or 1+ (tier index)
}
```

## What's Included

### Implementation Files
- **`escrow/src/lib.rs`**
  - `AllowlistEntry` struct (line ~945)
  - `get_allowlist_page(env, start, limit)` function (line ~3385)
  - `get_tier_index_for_investor()` helper (line ~3440)

### Test Files
- **`escrow/src/test_allowlist_tests.rs`**
  - 20 comprehensive test cases
  - Coverage: empty lists, pagination, revocation, tiers, edge cases

### Documentation Files
- **`ALLOWLIST_PAGINATION.md`** - Complete feature documentation
- **`IMPLEMENTATION_SUMMARY.md`** - Technical implementation details
- **`USAGE_EXAMPLES.md`** - Practical integration examples
- **`ALLOWLIST_PAGINATION_README.md`** - This file (quick reference)

## Key Features

✅ **Bounded Execution** - Max 50 entries per call  
✅ **Zero Panics** - Returns empty vector for invalid queries  
✅ **Read-Only** - No storage writes or token transfers  
✅ **Filters Revoked** - Only returns active allowlist entries  
✅ **Tier Support** - Returns tier index (0 = base, 1+ = tier)  

## API Reference

### Function Signature
```rust
pub fn get_allowlist_page(
    env: Env,
    start: u32,
    limit: u32
) -> Vec<AllowlistEntry>
```

### Parameters
- `start`: Zero-based starting index
- `limit`: Max entries to return (capped at 50)

### Returns
```rust
pub struct AllowlistEntry {
    pub investor: Address,
    pub tier: u32,
}
```

### Tier Values
- `0` = Base yield or not funded
- `1` = Tier 1 from yield table
- `2` = Tier 2 from yield table
- etc.

## Common Patterns

### Complete Enumeration
```rust
let mut start = 0u32;
let page_size = 50u32;

loop {
    let page = client.get_allowlist_page(&start, &page_size);
    if page.len() == 0 {
        break;
    }
    
    // Process page...
    
    start += page_size;
}
```

### Filter by Tier
```rust
let page = client.get_allowlist_page(&0, &50);

for i in 0..page.len() {
    let entry = page.get(i).unwrap();
    if entry.tier == 1 {
        // Process Tier 1 investors
    }
}
```

### Check Empty
```rust
let first_page = client.get_allowlist_page(&0, &1);
let is_empty = first_page.len() == 0;
```

## Edge Cases

All of these return empty vectors (no panics):

```rust
// Empty allowlist
client.get_allowlist_page(&0, &10); // => []

// Start beyond bounds
client.get_allowlist_page(&1000, &10); // => []

// Zero limit
client.get_allowlist_page(&0, &0); // => []
```

## Performance

### Gas Costs
For a page of `n` entries:
- ~3 instance storage reads (fixed)
- ~2n persistent storage reads (per entry)

Maximum: ~103 reads for 50 entries

### Recommended Page Sizes
- **Small allowlists (<100)**: Use `limit = 50`
- **Large allowlists (>100)**: Use `limit = 20-30`
- **Real-time UIs**: Use `limit = 10-20`

## Integration Examples

### Frontend (TypeScript)
```typescript
async function fetchAllowlistPage(
  contractId: string,
  start: number,
  limit: number
): Promise<AllowlistEntry[]> {
  const contract = new Contract(contractId);
  return await contract.call('get_allowlist_page', start, limit);
}
```

### Backend (Rust)
```rust
fn get_all_allowlist_entries(
    client: &LiquifactEscrowClient
) -> Vec<AllowlistEntry> {
    let mut all_entries = Vec::new();
    let mut start = 0u32;
    
    loop {
        let page = client.get_allowlist_page(&start, &50);
        if page.len() == 0 {
            break;
        }
        
        for i in 0..page.len() {
            all_entries.push(page.get(i).unwrap());
        }
        
        start += 50;
    }
    
    all_entries
}
```

## Testing

Run the test suite:
```bash
cd escrow
cargo test test_get_allowlist_page
```

All 20 tests should pass:
- ✅ Empty allowlist
- ✅ Pagination
- ✅ Limit capping
- ✅ Revocation filtering
- ✅ Tier resolution
- ✅ Edge cases

## Comparison with Existing Functions

| Function | Returns | Use Case |
|----------|---------|----------|
| `is_investor_allowlisted(addr)` | `bool` | Check single investor |
| `get_allowlisted_investors(start, limit)` | `Vec<Address>` | Get addresses only |
| `get_allowlisted_investors_count()` | `u32` | Count active entries |
| **`get_allowlist_page(start, limit)`** | **`Vec<AllowlistEntry>`** | **Get addresses + tiers** |

## Documentation

For detailed information, see:

1. **[ALLOWLIST_PAGINATION.md](./ALLOWLIST_PAGINATION.md)** - Complete feature documentation
   - Tier resolution logic
   - Performance considerations
   - Security notes
   - Migration guide

2. **[IMPLEMENTATION_SUMMARY.md](./IMPLEMENTATION_SUMMARY.md)** - Technical details
   - Design decisions
   - Storage access patterns
   - Test coverage
   - Known limitations

3. **[USAGE_EXAMPLES.md](./USAGE_EXAMPLES.md)** - Integration examples
   - React/TypeScript examples
   - Reconciliation tools
   - Event-driven updates
   - Error handling

## Requirements Met

All original requirements satisfied:

✅ Function name: `get_allowlist_page`  
✅ Parameters: `(env, start, limit)`  
✅ Returns: `Vec` of `(Address, u32)` pairs (via `AllowlistEntry`)  
✅ Bounded execution (max 50 per call)  
✅ No panics on edge cases  
✅ Reuses existing pagination pattern  
✅ Zero storage writes  
✅ Respects workspace max limit  

## Support

For questions or issues:
1. Review documentation files
2. Check test cases in `test_allowlist_tests.rs`
3. See usage examples in `USAGE_EXAMPLES.md`

## Version

**Implementation Version**: 1.0  
**Schema Compatibility**: Version 6+  
**Contract**: LiquiFact Escrow (Soroban/Stellar)  

---

**Quick Links**
- [Main Documentation](./ALLOWLIST_PAGINATION.md)
- [Implementation Details](./IMPLEMENTATION_SUMMARY.md)
- [Usage Examples](./USAGE_EXAMPLES.md)
- [Test File](../src/test_allowlist_tests.rs)
