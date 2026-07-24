# get_allowlist_page Usage Examples

This document provides practical examples for integrators using the `get_allowlist_page` function.

## Table of Contents
- [Basic Usage](#basic-usage)
- [Complete Enumeration](#complete-enumeration)
- [Filtering and Processing](#filtering-and-processing)
- [UI Integration](#ui-integration)
- [Reconciliation Tools](#reconciliation-tools)
- [Event-Driven Updates](#event-driven-updates)

---

## Basic Usage

### Simple Query
```rust
use soroban_sdk::{Env, Address};

// Get first 10 allowlist entries
let page = client.get_allowlist_page(&0, &10);

for i in 0..page.len() {
    let entry = page.get(i).unwrap();
    log!("Investor: {:?}, Tier: {}", entry.investor, entry.tier);
}
```

### Check if Allowlist is Empty
```rust
let first_page = client.get_allowlist_page(&0, &1);
let is_empty = first_page.len() == 0;

if is_empty {
    log!("No allowlisted investors");
} else {
    log!("Allowlist has entries");
}
```

---

## Complete Enumeration

### Iterate Through All Entries
```rust
fn get_all_allowlist_entries(client: &LiquifactEscrowClient) -> Vec<AllowlistEntry> {
    let mut all_entries = Vec::new();
    let mut start = 0u32;
    let page_size = 50u32; // Max allowed
    
    loop {
        let page = client.get_allowlist_page(&start, &page_size);
        
        if page.len() == 0 {
            break;
        }
        
        for i in 0..page.len() {
            all_entries.push(page.get(i).unwrap());
        }
        
        start += page_size;
        
        // Safety check (optional)
        if start > 10_000 {
            panic!("Allowlist too large");
        }
    }
    
    all_entries
}
```

### Count Total Active Investors by Tier
```rust
fn count_by_tier(client: &LiquifactEscrowClient) -> std::collections::HashMap<u32, u32> {
    let mut counts = std::collections::HashMap::new();
    let mut start = 0u32;
    let page_size = 50u32;
    
    loop {
        let page = client.get_allowlist_page(&start, &page_size);
        
        if page.len() == 0 {
            break;
        }
        
        for i in 0..page.len() {
            let entry = page.get(i).unwrap();
            *counts.entry(entry.tier).or_insert(0) += 1;
        }
        
        start += page_size;
    }
    
    counts
}

// Usage:
let tier_counts = count_by_tier(&client);
println!("Base yield (tier 0): {} investors", tier_counts.get(&0).unwrap_or(&0));
println!("Tier 1: {} investors", tier_counts.get(&1).unwrap_or(&0));
println!("Tier 2: {} investors", tier_counts.get(&2).unwrap_or(&0));
```

---

## Filtering and Processing

### Get Only Tier 1 Investors
```rust
fn get_tier1_investors(client: &LiquifactEscrowClient) -> Vec<Address> {
    let mut tier1_investors = Vec::new();
    let mut start = 0u32;
    let page_size = 50u32;
    
    loop {
        let page = client.get_allowlist_page(&start, &page_size);
        
        if page.len() == 0 {
            break;
        }
        
        for i in 0..page.len() {
            let entry = page.get(i).unwrap();
            if entry.tier == 1 {
                tier1_investors.push(entry.investor);
            }
        }
        
        start += page_size;
    }
    
    tier1_investors
}
```

### Get Investors Who Haven't Funded Yet
```rust
fn get_unfunded_allowlisted(client: &LiquifactEscrowClient) -> Vec<Address> {
    let mut unfunded = Vec::new();
    let mut start = 0u32;
    let page_size = 50u32;
    
    loop {
        let page = client.get_allowlist_page(&start, &page_size);
        
        if page.len() == 0 {
            break;
        }
        
        for i in 0..page.len() {
            let entry = page.get(i).unwrap();
            let contribution = client.get_contribution(&entry.investor);
            
            if contribution == 0 {
                unfunded.push(entry.investor);
            }
        }
        
        start += page_size;
    }
    
    unfunded
}
```

### Build Complete Investor Profile
```rust
struct InvestorProfile {
    address: Address,
    tier: u32,
    contribution: i128,
    effective_yield: i64,
}

fn build_investor_profiles(
    client: &LiquifactEscrowClient
) -> Vec<InvestorProfile> {
    let mut profiles = Vec::new();
    let mut start = 0u32;
    let page_size = 50u32;
    
    loop {
        let page = client.get_allowlist_page(&start, &page_size);
        
        if page.len() == 0 {
            break;
        }
        
        for i in 0..page.len() {
            let entry = page.get(i).unwrap();
            
            let contribution = client.get_contribution(&entry.investor);
            let effective_yield = client.get_investor_effective_yield(&entry.investor);
            
            profiles.push(InvestorProfile {
                address: entry.investor,
                tier: entry.tier,
                contribution,
                effective_yield,
            });
        }
        
        start += page_size;
    }
    
    profiles
}
```

---

## UI Integration

### React Component Example
```typescript
// TypeScript/JavaScript example for frontend integration

interface AllowlistEntry {
  investor: string;
  tier: number;
}

async function fetchAllowlistPage(
  contractId: string,
  start: number,
  limit: number
): Promise<AllowlistEntry[]> {
  const contract = new Contract(contractId);
  
  try {
    const result = await contract.call('get_allowlist_page', start, limit);
    return result;
  } catch (error) {
    console.error('Error fetching allowlist:', error);
    return [];
  }
}

// React component with pagination
function AllowlistViewer({ contractId }: { contractId: string }) {
  const [entries, setEntries] = useState<AllowlistEntry[]>([]);
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const pageSize = 20;
  
  useEffect(() => {
    async function loadPage() {
      setLoading(true);
      const data = await fetchAllowlistPage(
        contractId,
        page * pageSize,
        pageSize
      );
      setEntries(data);
      setLoading(false);
    }
    
    loadPage();
  }, [contractId, page]);
  
  return (
    <div>
      <h2>Allowlist (Page {page + 1})</h2>
      
      {loading ? (
        <div>Loading...</div>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Investor Address</th>
              <th>Tier</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((entry, idx) => (
              <tr key={idx}>
                <td>{entry.investor}</td>
                <td>
                  {entry.tier === 0 ? 'Base' : `Tier ${entry.tier}`}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      
      <div>
        <button 
          onClick={() => setPage(p => Math.max(0, p - 1))}
          disabled={page === 0}
        >
          Previous
        </button>
        <button 
          onClick={() => setPage(p => p + 1)}
          disabled={entries.length < pageSize}
        >
          Next
        </button>
      </div>
    </div>
  );
}
```

### Infinite Scroll Example
```typescript
function InfiniteAllowlistViewer({ contractId }: { contractId: string }) {
  const [entries, setEntries] = useState<AllowlistEntry[]>([]);
  const [hasMore, setHasMore] = useState(true);
  const [loading, setLoading] = useState(false);
  const pageSize = 20;
  
  const loadMore = async () => {
    if (loading || !hasMore) return;
    
    setLoading(true);
    const newEntries = await fetchAllowlistPage(
      contractId,
      entries.length,
      pageSize
    );
    
    if (newEntries.length === 0) {
      setHasMore(false);
    } else {
      setEntries([...entries, ...newEntries]);
    }
    
    setLoading(false);
  };
  
  return (
    <div>
      <h2>Allowlist</h2>
      
      {entries.map((entry, idx) => (
        <div key={idx}>
          <span>{entry.investor}</span>
          <span>Tier {entry.tier}</span>
        </div>
      ))}
      
      {hasMore && (
        <button onClick={loadMore} disabled={loading}>
          {loading ? 'Loading...' : 'Load More'}
        </button>
      )}
    </div>
  );
}
```

---

## Reconciliation Tools

### Export to CSV
```rust
fn export_allowlist_to_csv(client: &LiquifactEscrowClient, output_path: &str) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::Write;
    
    let mut file = File::create(output_path)?;
    writeln!(file, "Investor Address,Tier,Contribution,Effective Yield")?;
    
    let mut start = 0u32;
    let page_size = 50u32;
    
    loop {
        let page = client.get_allowlist_page(&start, &page_size);
        
        if page.len() == 0 {
            break;
        }
        
        for i in 0..page.len() {
            let entry = page.get(i).unwrap();
            let contribution = client.get_contribution(&entry.investor);
            let effective_yield = client.get_investor_effective_yield(&entry.investor);
            
            writeln!(
                file,
                "{},{},{},{}",
                entry.investor,
                entry.tier,
                contribution,
                effective_yield
            )?;
        }
        
        start += page_size;
    }
    
    Ok(())
}
```

### Verify Allowlist Integrity
```rust
fn verify_allowlist_integrity(client: &LiquifactEscrowClient) -> Result<(), String> {
    let mut start = 0u32;
    let page_size = 50u32;
    let mut seen_addresses = std::collections::HashSet::new();
    
    loop {
        let page = client.get_allowlist_page(&start, &page_size);
        
        if page.len() == 0 {
            break;
        }
        
        for i in 0..page.len() {
            let entry = page.get(i).unwrap();
            
            // Check for duplicates
            if seen_addresses.contains(&entry.investor) {
                return Err(format!("Duplicate investor: {:?}", entry.investor));
            }
            seen_addresses.insert(entry.investor.clone());
            
            // Verify investor is actually allowlisted
            let is_allowlisted = client.is_investor_allowlisted(&entry.investor);
            if !is_allowlisted {
                return Err(format!("Investor not allowlisted: {:?}", entry.investor));
            }
            
            // Verify tier is valid
            let tier_table = client.get_yield_tier_table();
            if entry.tier > tier_table.len() {
                return Err(format!(
                    "Invalid tier {} for investor: {:?}",
                    entry.tier,
                    entry.investor
                ));
            }
        }
        
        start += page_size;
    }
    
    Ok(())
}
```

### Compare Two Allowlist Snapshots
```rust
fn compare_allowlist_snapshots(
    client: &LiquifactEscrowClient,
    old_snapshot: &Vec<AllowlistEntry>,
) -> (Vec<Address>, Vec<Address>, Vec<(Address, u32, u32)>) {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut tier_changed = Vec::new();
    
    // Build current allowlist
    let mut current = std::collections::HashMap::new();
    let mut start = 0u32;
    let page_size = 50u32;
    
    loop {
        let page = client.get_allowlist_page(&start, &page_size);
        if page.len() == 0 {
            break;
        }
        
        for i in 0..page.len() {
            let entry = page.get(i).unwrap();
            current.insert(entry.investor.clone(), entry.tier);
        }
        
        start += page_size;
    }
    
    // Build old snapshot map
    let old: std::collections::HashMap<_, _> = old_snapshot
        .iter()
        .map(|e| (e.investor.clone(), e.tier))
        .collect();
    
    // Find added and tier changes
    for (addr, tier) in &current {
        match old.get(addr) {
            None => added.push(addr.clone()),
            Some(&old_tier) if old_tier != *tier => {
                tier_changed.push((addr.clone(), old_tier, *tier));
            }
            _ => {}
        }
    }
    
    // Find removed
    for addr in old.keys() {
        if !current.contains_key(addr) {
            removed.push(addr.clone());
        }
    }
    
    (added, removed, tier_changed)
}
```

---

## Event-Driven Updates

### Listen for Allowlist Changes
```typescript
// TypeScript example for event-driven UI updates

interface AllowlistChangeEvent {
  investor: string;
  allowed: boolean;
}

class AllowlistMonitor {
  private contractId: string;
  private cache: Map<string, AllowlistEntry> = new Map();
  
  constructor(contractId: string) {
    this.contractId = contractId;
  }
  
  async initialize() {
    // Load initial allowlist
    await this.refreshCache();
    
    // Subscribe to events
    this.subscribeToEvents();
  }
  
  private async refreshCache() {
    this.cache.clear();
    let start = 0;
    const pageSize = 50;
    
    while (true) {
      const page = await fetchAllowlistPage(
        this.contractId,
        start,
        pageSize
      );
      
      if (page.length === 0) break;
      
      page.forEach(entry => {
        this.cache.set(entry.investor, entry);
      });
      
      start += pageSize;
    }
  }
  
  private subscribeToEvents() {
    // Subscribe to InvestorAllowlistChanged events
    subscribeToContractEvents(this.contractId, 'al_set', (event: AllowlistChangeEvent) => {
      if (event.allowed) {
        // Refresh entry for this investor
        this.refreshInvestor(event.investor);
      } else {
        // Remove from cache
        this.cache.delete(event.investor);
      }
    });
    
    // Subscribe to EscrowFunded events to detect tier changes
    subscribeToContractEvents(this.contractId, 'funded', (event) => {
      this.refreshInvestor(event.investor);
    });
  }
  
  private async refreshInvestor(investor: string) {
    // Re-fetch this specific investor's tier
    const page = await fetchAllowlistPage(this.contractId, 0, 10000);
    const entry = page.find(e => e.investor === investor);
    
    if (entry) {
      this.cache.set(investor, entry);
    } else {
      this.cache.delete(investor);
    }
  }
  
  getCache(): AllowlistEntry[] {
    return Array.from(this.cache.values());
  }
}
```

---

## Performance Optimization

### Batch Processing
```rust
fn process_allowlist_in_batches<F>(
    client: &LiquifactEscrowClient,
    batch_size: u32,
    mut processor: F
) where
    F: FnMut(&[AllowlistEntry]),
{
    let mut start = 0u32;
    
    loop {
        let page = client.get_allowlist_page(&start, &batch_size);
        
        if page.len() == 0 {
            break;
        }
        
        // Convert to slice and process
        let mut entries = Vec::new();
        for i in 0..page.len() {
            entries.push(page.get(i).unwrap());
        }
        
        processor(&entries);
        
        start += batch_size;
    }
}

// Usage:
process_allowlist_in_batches(&client, 20, |batch| {
    println!("Processing batch of {} entries", batch.len());
    for entry in batch {
        // Process each entry
    }
});
```

### Parallel Fetching (for off-chain tools)
```rust
use rayon::prelude::*;

fn fetch_all_allowlist_parallel(client: &LiquifactEscrowClient) -> Vec<AllowlistEntry> {
    // First, estimate total size
    let first_page = client.get_allowlist_page(&0, &50);
    if first_page.len() < 50 {
        // Small allowlist, no need for parallelism
        return first_page.into_iter().collect();
    }
    
    // Fetch multiple pages in parallel
    let page_size = 50u32;
    let max_pages = 20; // Adjust based on expected size
    
    let results: Vec<_> = (0..max_pages)
        .into_par_iter()
        .map(|page_num| {
            let start = page_num * page_size;
            client.get_allowlist_page(&start, &page_size)
        })
        .collect();
    
    // Flatten results
    let mut all_entries = Vec::new();
    for page in results {
        if page.len() == 0 {
            break;
        }
        for i in 0..page.len() {
            all_entries.push(page.get(i).unwrap());
        }
    }
    
    all_entries
}
```

---

## Error Handling

### Robust Pagination with Retries
```rust
use std::time::Duration;

fn fetch_allowlist_page_with_retry(
    client: &LiquifactEscrowClient,
    start: u32,
    limit: u32,
    max_retries: u32,
) -> Result<Vec<AllowlistEntry>, String> {
    let mut retries = 0;
    
    loop {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.get_allowlist_page(&start, &limit)
        })) {
            Ok(page) => return Ok(page),
            Err(e) => {
                retries += 1;
                
                if retries >= max_retries {
                    return Err(format!("Failed after {} retries", max_retries));
                }
                
                // Exponential backoff
                let delay = Duration::from_millis(100 * 2u64.pow(retries));
                std::thread::sleep(delay);
            }
        }
    }
}
```

---

## Summary

These examples demonstrate:

1. **Basic queries** for simple use cases
2. **Complete enumeration** for full allowlist access
3. **Filtering and processing** for data analysis
4. **UI integration** for frontend applications
5. **Reconciliation tools** for operational monitoring
6. **Event-driven updates** for real-time synchronization
7. **Performance optimization** for large datasets
8. **Error handling** for production robustness

Choose the pattern that best fits your integration requirements!
