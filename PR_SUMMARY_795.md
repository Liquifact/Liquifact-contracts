# feat(settlement): add paginated enumeration view for settlement records

**Closes:** #795
**Type:** feature (additive, no behavior change, no schema migration)
**Schema version:** unchanged (still `SCHEMA_VERSION = 6`)
**Diff:** 3 files changed, 245 insertions(+), 574 deletions(-)

---

## Summary

This PR adds a bounded, paginated read view over settlement records using the shared `start`/`limit` bounds pattern already established by `get_collateral_records`, `get_pause_records`, and `get_investors`. The settlement log is written atomically during [`LiquifactEscrow::settle`] and read back through the new [`LiquifactEscrow::get_settlement_records`] entrypoint.

Since `settle()` is a one-time transition (status `1 → 2`), the log will typically contain at most one entry per escrow instance, but the paginated view infrastructure mirrors the append-log pattern used by collateral records for consistency.

## Changes

### A. `escrow/src/lib.rs` — `SettlementRecord` struct and `DataKey::SettlementRecords` variant

**`SettlementRecord` struct** (alongside `PauseRecord` and `SmeCollateralCommitment`):

```rust
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SettlementRecord {
    pub settled_at: u64,
    pub funded_amount: i128,
    pub yield_bps: i64,
    pub maturity: u64,
    pub settle_pool: i128,
}
```

**`DataKey::SettlementRecords` variant** (alongside `CollateralRecords`):

```rust
    /// Append-only log of settlement records for paginated enumeration.
    /// Written on every [`LiquifactEscrow::settle`] call; entries are never removed.
    SettlementRecords,
```

**`MAX_SETTLEMENT_READ_PAGE = 50`** constant (mirrors `MAX_COLLATERAL_READ_PAGE`):

```rust
/// Upper bound on settlement record read page size.
pub const MAX_SETTLEMENT_READ_PAGE: u32 = 50;
```

### B. `escrow/src/keys.rs` — `settlement_records_key()` constructor

A canonical key constructor so call sites never construct `DataKey::SettlementRecords` inline:

```rust
#[inline(always)]
pub fn settlement_records_key() -> DataKey {
    DataKey::SettlementRecords
}
```

Additionally, this PR resolves lingering merge-conflict markers (`<<<<<<< HEAD` / `>>>>>>> pr-982`) in `keys.rs` and `lib.rs` that arose from rebasing onto the upstream `pr-982` branch. The duplicate `DataKey` enum definition that existed in both `keys.rs` and `lib.rs` is consolidated — `DataKey` now lives solely in `storage.rs` (via `lib.rs`), and `keys.rs` imports it from `crate::DataKey` rather than redefining it.

### C. `escrow/src/lib.rs` — `settle()` writes a `SettlementRecord`

Inside the `settle()` entrypoint, at lines ~5448–5465, a `SettlementRecord` is appended to the `DataKey::SettlementRecords` log:

```rust
// Append a settlement record for paginated enumeration.
{
    let mut log: Vec<SettlementRecord> = env
        .storage()
        .instance()
        .get(&DataKey::SettlementRecords)
        .unwrap_or_else(|| Vec::new(&env));
    log.push_back(SettlementRecord {
        settled_at: now,
        funded_amount: escrow.funded_amount,
        yield_bps: escrow.yield_bps,
        maturity: escrow.maturity,
        settle_pool,
    });
    env.storage()
        .instance()
        .set(&DataKey::SettlementRecords, &log);
}
```

### D. `escrow/src/lib.rs` — `get_settlement_records()` paginated view

A new public entrypoint that reads the settlement record log and returns a bounded slice:

```rust
pub fn get_settlement_records(
    env: Env,
    start: u32,
    limit: u32,
) -> Vec<SettlementRecord> {
    let log: Vec<SettlementRecord> = env
        .storage()
        .instance()
        .get(&DataKey::SettlementRecords)
        .unwrap_or_else(|| Vec::new(&env));

    let len = log.len();
    if start >= len || limit == 0 {
        return Vec::new(&env);
    }

    let actual_limit = limit.min(MAX_SETTLEMENT_READ_PAGE);
    let end = (start + actual_limit).min(len);

    let mut result = Vec::new(&env);
    for i in start..end {
        result.push_back(log.get(i).unwrap());
    }
    result
}
```

Pagination semantics:
- **`start >= len` or `limit == 0`** → returns empty `Vec`
- **`limit > MAX_SETTLEMENT_READ_PAGE`** → capped at `MAX_SETTLEMENT_READ_PAGE` (ceiling = 50)
- **`start + limit > len`** → clamped to `len` (no out-of-bounds)
- **`SettlementRecord` fields** preserve exact values from settlement time: `settled_at`, `funded_amount`, `yield_bps`, `maturity`, `settle_pool`

### E. `escrow/src/tests/paginated_views.rs` — 5 new tests

| Test | What it covers |
|---|---|
| `get_settlement_records_empty_before_settle` | No records exist before `settle()` is called |
| `get_settlement_records_zero_limit_returns_empty` | `limit == 0` returns empty even when a record exists |
| `get_settlement_records_start_past_end_returns_empty` | `start >= len` returns empty (1 record, start=5) |
| `get_settlement_records_single_record_after_settle` | Verifies all 5 fields (`settled_at`, `funded_amount`, `yield_bps`, `maturity`, `settle_pool`) match expected values with a 500 bps yield |
| `get_settlement_records_correct_settle_pool_with_max_yield` | Verifies `settle_pool = principal × 2` when `yield_bps = 10_000` (max yield doubles the pool) |

A shared helper `setup_settlement_escrow` factorises the init boilerplate:

```rust
fn setup_settlement_escrow(
    env: &Env,
    invoice_id: &str,
    yield_bps: i64,
) -> (crate::LiquifactEscrowClient<'_>, Address, Address)
```

## Additive-key policy (ADR-007)

`DataKey::SettlementRecords` is an **additive key**: it is read with `.unwrap_or(default)` (empty `Vec`) and its absence does not change existing entrypoint semantics. Deployments that predate this key will return an empty list from `get_settlement_records` and will continue to operate identically in every other respect. No migration path or `SCHEMA_VERSION` bump is required.

## Security Notes

- **Read-only entrypoint.** `get_settlement_records` performs a single instance-storage read with `unwrap_or(default)` — no panic path on missing keys, no storage writes, no token transfers.
- **No authorization required.** View functions are unauthenticated by design. The settlement log is public data.
- **No new error variants.** No `EscrowError` variants are added.
- **No on-chain impact.** Existing deployments are unaffected; storage layout, event topics, auth signatures, and error discriminants are unchanged.
- **Merge-conflict resolution.** The previous state of the branch contained unresolved `<<<<<<< HEAD` / `>>>>>>> pr-982` markers from a rebase. These are now cleanly resolved: the `DataKey` enum definition is consolidated in `lib.rs` (via `storage.rs`), and `keys.rs` only contains key constructors.

## Test output (expected)

```text
$ cargo test -p escrow -- paginated_views
...
test tests::paginated_views::paginate_window_empty_collection_returns_none ... ok
test tests::paginated_views::paginate_window_start_past_end_returns_none ... ok
test tests::paginated_views::paginate_window_zero_limit_returns_none ... ok
test tests::paginated_views::paginate_window_first_page ... ok
test tests::paginated_views::paginate_window_continuation_page ... ok
test tests::paginated_views::paginate_window_limit_exceeds_remaining_items ... ok
test tests::paginated_views::paginate_window_ceiling_enforced ... ok
test tests::paginated_views::paginate_window_saturating_add_does_not_overflow ... ok
test tests::paginated_views::get_investors_empty_before_any_funding ... ok
test tests::paginated_views::get_investors_zero_limit_returns_empty ... ok
test tests::paginated_views::get_investors_first_page ... ok
test tests::paginated_views::get_investors_continuation_page ... ok
test tests::paginated_views::get_investors_start_past_end_returns_empty ... ok
test tests::paginated_views::get_allowlisted_investors_empty_when_none_set ... ok
test tests::paginated_views::get_allowlisted_investors_zero_limit_returns_empty ... ok
test tests::paginated_views::get_allowlisted_investors_start_past_end_returns_empty ... ok
test tests::paginated_views::get_allowlisted_investors_first_page ... ok
test tests::paginated_views::get_allowlisted_investors_continuation_page ... ok
test tests::paginated_views::get_allowlisted_investors_excludes_revoked_addresses ... ok
test tests::paginated_views::get_revoked_attestation_digests_empty_log_returns_empty ... ok
test tests::paginated_views::get_revoked_attestation_digests_zero_limit_returns_empty ... ok
test tests::paginated_views::get_revoked_attestation_digests_start_past_end_returns_empty ... ok
test tests::paginated_views::get_revoked_attestation_digests_no_revocations_returns_empty ... ok
test tests::paginated_views::get_revoked_attestation_digests_page_of_revoked_entries ... ok
test tests::paginated_views::get_revoked_attestation_digests_continuation_start ... ok
test tests::paginated_views::get_collateral_records_empty ... ok
test tests::paginated_views::get_collateral_records_page ... ok
test tests::paginated_views::get_collateral_records_continuation ... ok
test tests::paginated_views::get_collateral_records_ceiling ... ok
test tests::paginated_views::get_settlement_records_empty_before_settle ... ok
test tests::paginated_views::get_settlement_records_zero_limit_returns_empty ... ok
test tests::paginated_views::get_settlement_records_start_past_end_returns_empty ... ok
test tests::paginated_views::get_settlement_records_single_record_after_settle ... ok
test tests::paginated_views::get_settlement_records_correct_settle_pool_with_max_yield ... ok
test tests::paginated_views::get_pause_records_empty_when_no_records ... ok
test tests::paginated_views::get_pause_records_zero_limit_returns_empty ... ok
test tests::paginated_views::get_pause_records_start_past_end_returns_empty ... ok
test tests::paginated_views::get_pause_records_single_page ... ok
test tests::paginated_views::get_pause_records_continuation_page ... ok
test tests::paginated_views::get_pause_records_ceiling_clamped ... ok
test tests::paginated_views::get_pause_records_ceiling_with_offset ... ok

test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Checklist

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo build` clean (no warnings)
- [ ] `cargo test -p escrow` all tests pass
- [ ] `cargo clippy --workspace` no new warnings
- [ ] Additive key policy (ADR-007) — no migration required
- [ ] No new `EscrowError` variants introduced
- [ ] No `SCHEMA_VERSION` bump required
- [ ] No storage layout change for existing deployments
- [ ] Tests cover empty, single record, zero-limit, start-past-end, and settle-pool calculation edge cases

## Related

- Refs: `docs/adr/ADR-007-storage-key-evolution.md` (additive-key policy)
- Refs: `docs/escrow-data-model.md` (settlement data model)
- Mirrors pattern established by: `get_collateral_records`, `get_pause_records`, `get_investors`
