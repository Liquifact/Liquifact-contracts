# Escrow Investor Allowlist

The investor allowlist is an optional funding gate for an escrow. It lets the
admin restrict `fund` and `fund_with_commitment` to addresses that have an
explicit per-address allowlist entry.

The authoritative code paths are in `escrow/src/lib.rs`:

- `set_allowlist_active`
- `is_allowlist_active`
- `set_investor_allowlisted`
- `set_investors_allowlisted`
- `is_investor_allowlisted`
- `fund_impl`
- `bump_ttl`

## Storage Model

The allowlist uses two storage families:

| Key | Storage class | Type | Default when absent |
|---|---|---|---|
| `DataKey::AllowlistActive` | instance | `bool` | `false` |
| `DataKey::InvestorAllowlisted(Address)` | persistent | `bool` | `false` |

`AllowlistActive` is a single contract-wide instance-storage flag. It controls
whether the funding gate is enforced. Missing or unset instance storage reads as
inactive, so a newly initialized escrow accepts funders unless the admin enables
the gate.

`InvestorAllowlisted(Address)` is stored in persistent storage, one key per
address. Each address has an independent persistent-storage TTL. This keeps the
instance footprint bounded, but it also means membership entries can be archived
independently under Soroban rent/archival rules. If an entry is absent or has
expired/been archived and is not restored, `is_investor_allowlisted` returns
`false`.

See `docs/escrow-data-model.md` for the complete `DataKey` reference and
`docs/escrow-gas-storage-notes.md` for storage/rent background.

## Funding Gate Semantics

The gate is checked inside `fund_impl`, which is shared by `fund` and
`fund_with_commitment`.

| `AllowlistActive` | Investor entry | Funding result |
|---|---|---|
| absent or `false` | absent, `false`, or `true` | allowed |
| `true` | `true` | allowed |
| `true` | absent or `false` | rejected with `InvestorNotAllowlisted` |

Important implications:

- Disabling the allowlist does not delete membership entries.
- Re-enabling the allowlist reuses any persistent membership entries that still
  exist.
- Removing an investor writes `false` to that investor's persistent entry.
- A missing persistent entry is not distinguishable from an explicit `false`
  through the public read API; both are not allowlisted.

## Mutating Membership

`set_investor_allowlisted(env, investor, allowed)` writes exactly one
`InvestorAllowlisted(investor)` persistent key and emits one
`InvestorAllowlistChanged` event with `name = al_set`.

`set_investors_allowlisted(env, investors, allowed)` performs the same write and
event emission for every address in the supplied vector. It requires admin
authorization once, rejects an empty vector with `InvestorBatchEmpty`, and
rejects vectors longer than `MAX_INVESTOR_ALLOWLIST_BATCH`.

The current batch bound is:

```rust
MAX_INVESTOR_ALLOWLIST_BATCH = 32
```

For every address in the vector, the batch call is equivalent to calling
`set_investor_allowlisted` individually with the same `allowed` flag:

- the same persistent key is written;
- the same boolean value is stored;
- one `InvestorAllowlistChanged` event is emitted per address;
- the final allowlist membership state is the same.

The contract does not currently de-duplicate addresses inside the batch. If the
same address appears more than once, the later write wins, and each occurrence
still emits its event. Callers should prefer de-duplicated batches for clearer
operator logs.

## TTL And Archival Operations

`bump_ttl(env, allowlisted)` is permissionless and only extends TTLs; it never
shortens TTL and does not mutate the stored values.

For each address passed in `allowlisted`, `bump_ttl` extends:

- `InvestorAllowlisted(Address)`
- `InvestorContribution(Address)`
- `InvestorEffectiveYield(Address)`
- `InvestorClaimNotBefore(Address)`
- `InvestorClaimed(Address)`

Operators should include active allowlist members when bumping TTL for
long-dated escrows. If the allowlist is active and an investor's persistent
allowlist key is archived or missing, funding by that address will fail until
the entry is restored or written again by the admin.

## Security Notes

- The allowlist gate is deny-by-default only when `AllowlistActive` is `true`.
- The inactive state is intentionally permissive so deployments can operate
  without allowlist administration.
- Admin authorization protects all allowlist writes and the active/inactive
  toggle.
- Persistent membership keys should be treated as part of the escrow's funding
  policy state, not as transient UI cache.
- Indexers should track both `AllowlistEnabledChanged` (`al_ena`) and
  `InvestorAllowlistChanged` (`al_set`) to reconstruct the effective gate.
