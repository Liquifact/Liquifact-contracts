# Escrow Ledger Time Semantics

This document explains how the LiquiFact Escrow contract handles time,
why it uses ledger timestamps instead of wall-clock time, and how
integrators and testers should reason about time-dependent operations.

---

## What is Ledger Time?

The Soroban runtime exposes time through `Env::ledger().timestamp()`,
which returns the **validator-observed Unix timestamp** (seconds since
epoch) recorded in the ledger header at the time the transaction is
processed.

This is **not** wall-clock time from your local machine or any external
oracle. It is the timestamp validators agreed upon when they closed the
ledger containing your transaction.

---

## Where Ledger Time Is Used in This Contract

### 1. `settle` — Maturity Gate

```rust
if escrow.maturity > 0 {
    let now = env.ledger().timestamp();
    assert!(now >= escrow.maturity, "Escrow has not yet reached maturity");
}
```

- The comparison is `>=` (inclusive boundary).
- When `maturity == 0`, the gate is skipped — no time lock.
- Maturity is stored as a raw `u64` of seconds.
- `has_maturity_lock()` and `get_escrow_summary().has_maturity_lock`
  surface this explicitly: `false` means `maturity == 0` and settlement is
  not time-gated.

### 2. `claim_investor_payout` — Commitment Lock

```rust
let now = env.ledger().timestamp();
assert!(now >= not_before, "Investor commitment lock not expired (ledger timestamp)");
```

Same `>=` semantics: the claim is allowed at exactly `not_before`,
not one second after.

`not_before` is computed at fund time as:

```
not_before = deposit_ledger_timestamp + committed_lock_secs
```

When `committed_lock_secs == 0`, `not_before` is stored as `0`.
Because every ledger timestamp satisfies `now >= 0`, a zero-lock
investor is never time-gated and may claim immediately after settlement.

### 3. `record_sme_collateral_commitment` — Timestamp Metadata

The `recorded_at` field is set to `env.ledger().timestamp()` for
indexing only. It does not gate any operations.

---

## `claim_investor_payout` — Idempotency

`claim_investor_payout` is **idempotent**: calling it more than once for
the same investor is safe and produces no additional side effects.

### Contract-level guarantee

```rust
let key = DataKey::InvestorClaimed(investor.clone());
if env.storage().instance().get(&key).unwrap_or(false) {
    return; // ← early return, no event emitted
}
env.storage().instance().set(&key, &true);
// … emit InvestorPayoutClaimed
```

| Call | Behaviour |
|------|-----------|
| First successful claim | Writes `InvestorClaimed = true`, emits `InvestorPayoutClaimed` |
| Second (and later) call | Returns immediately; **no event emitted**, state unchanged |

### Invariants

| # | Invariant |
|---|-----------|
| I-1 | A claim before `not_before` panics with `"Investor commitment lock not expired (ledger timestamp)"` |
| I-2 | The first post-lock claim emits exactly **one** `InvestorPayoutClaimed` event |
| I-3 | Every subsequent call for the same investor emits **zero** events and does not panic |
| I-4 | The gate boundary is inclusive (`now >= not_before`): the claim succeeds at exactly `deposit_ts + committed_lock_secs` |
| I-5 | Per-investor claim keys are independent: investor A's idempotent repeat calls do not affect investor B's unclaimed state |

### Security notes

- **No double-spend:** the `InvestorClaimed` flag is set atomically
  before the event is emitted. Re-entry or transaction replay cannot
  trigger a second `InvestorPayoutClaimed` event.
- **Auth still required:** the early return path is reached only after
  `investor.require_auth()` passes — there is no way to skip auth by
  replaying a previously-settled claim.
- **Lock is per-investor, not per-escrow:** one investor's unexpired
  lock does not block other investors with shorter or zero locks from
  claiming.

---

## Simulating Time in Tests

Soroban's test environment lets you set the ledger timestamp manually:

```rust
env.ledger().set_timestamp(5000);
// OR the equivalent mutable-closure form:
env.ledger().with_mut(|l| l.timestamp = 5000);
```

### Example: Testing the Commitment Lock Boundary

```rust
let deposit_ts: u64 = 1_000;
let lock_secs: u64 = 400;
let not_before = deposit_ts + lock_secs; // 1400

env.ledger().set_timestamp(deposit_ts);
client.fund_with_commitment(&inv, &1_000i128, &lock_secs);
client.settle();

// One second before expiry → must panic
env.ledger().set_timestamp(not_before - 1);
let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    client.claim_investor_payout(&inv);
}));
assert!(r.is_err());

// Exactly at expiry → must succeed (inclusive boundary)
env.ledger().set_timestamp(not_before);
client.claim_investor_payout(&inv);
assert!(client.is_investor_claimed(&inv));

// Snapshot event count after first claim
let count = env.events().all().len();

// Second call → no new event (idempotency)
client.claim_investor_payout(&inv);
assert_eq!(env.events().all().len(), count);
```

### Example: Testing the Maturity Boundary

```rust
// Escrow initialized with maturity = 5000
env.ledger().set_timestamp(4999); // with_mut variant also valid
// settle() panics — one second before maturity

env.ledger().set_timestamp(5000);
// settle() succeeds — exactly at maturity
```

### Zero Maturity Means No Settlement Time Lock

`maturity = 0` is not "midnight" or an unset future timestamp. It is the
contract's explicit no-lock mode:

```rust
assert!(!client.has_maturity_lock());
assert!(!client.get_escrow_summary().has_maturity_lock);

// After funding, SME settlement is allowed immediately unless legal hold or
// another status/auth guard blocks the call.
client.settle();
```

Operators who expect a settlement delay must deploy with a positive ledger
timestamp and should confirm `has_maturity_lock == true` in the
`EscrowInitialized` event or read API before accepting investor deposits.

---

## Skew Between Test/Simulation and Mainnet

> **Important:** ledger timestamps on testnets and in local simulation
> may not match mainnet validator observations.

- **Local simulation** sets an arbitrary timestamp with no relation to
  real time.
- **Testnet** ledgers close faster than mainnet.
- **Mainnet** validators agree on timestamps by consensus; actual
  timestamps may differ slightly from wall-clock time due to network
  conditions and validator clock skew (~±30s is normal).

### Practical Guidance

| Scenario | Recommendation |
|----------|---------------|
| Unit tests | Use `env.ledger().set_timestamp` to set exact timestamps |
| Integration tests on testnet | Add a safety buffer of at least 60s to maturity and lock values |
| Production / mainnet | Treat time boundaries as approximate to ±30s of wall clock |
| Off-chain monitoring | Poll `get_escrow().maturity` and compare to latest ledger timestamp from Horizon |
| Claim unlock monitoring | Read `get_investor_claim_not_before(addr)` and compare to the latest ledger timestamp |

---

## `update_maturity` — Open State Only

Maturity can only be changed while the escrow is **Open** (status == 0):

| Status | `update_maturity` result |
|--------|--------------------------|
| 0 — Open | ✅ Allowed |
| 1 — Funded | ❌ Panics: "Maturity can only be updated in Open state" |
| 2 — Settled | ❌ Panics: "Maturity can only be updated in Open state" |
| 3 — Withdrawn | ❌ Panics: "Maturity can only be updated in Open state" |

This prevents retroactive maturity changes after investors have
committed funds.

---

## `MaturityUpdatedEvent`

Every successful `update_maturity` emits:

```rust
pub struct MaturityUpdatedEvent {
    #[topic] pub name: Symbol,       // symbol_short!("maturity")
    #[topic] pub invoice_id: Symbol,
    pub old_maturity: u64,           // previous ledger timestamp
    pub new_maturity: u64,           // new ledger timestamp
}
```

Indexers should listen for this event to track maturity changes
per invoice without polling contract state on every ledger.

---

## `InvestorClaimNotBefore` Storage Key

`DataKey::InvestorClaimNotBefore(Address)` stores the earliest ledger
timestamp at which an investor may call `claim_investor_payout`.

| Condition | Stored value |
|-----------|-------------|
| `fund_with_commitment` with `committed_lock_secs > 0` | `deposit_timestamp + committed_lock_secs` |
| `fund_with_commitment` with `committed_lock_secs == 0` | `0` (never gated) |
| Plain `fund` (first deposit) | `0` (never gated) |
| Key absent (legacy positions before v2) | Defaults to `0` via `.unwrap_or(0)` |

Read with `get_investor_claim_not_before(investor)`.  A return value of
`0` means there is no lock; any positive value is the earliest claimable
ledger timestamp (seconds since Unix epoch, inclusive).

---

## Security Notes

- **No wall-clock oracle:** all time comparisons use
  `env.ledger().timestamp()` only — no external time source.
- **No negative time:** `maturity` and `InvestorClaimNotBefore` are
  `u64` — they cannot be negative.
- **Overflow guard:** `committed_lock_secs` addition uses
  `checked_add(...).expect("investor claim time overflow")`.
- **Idempotency is not trustless skipping:** the early return in
  `claim_investor_payout` only fires after `investor.require_auth()`;
  it cannot be exploited to skip the auth check.
- **Token economics:** time-based yield calculations are out of scope.
  See `escrow/src/external_calls.rs` for token transfer assumptions.
