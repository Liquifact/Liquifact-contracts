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

### 2. `claim_investor_payout` — Commitment Lock

```rust
let now = env.ledger().timestamp();
assert!(now >= not_before, "Investor commitment lock not expired (ledger timestamp)");
```

Same `>=` semantics: the claim is allowed at exactly `not_before`,
not one second after.

### 3. `record_sme_collateral_commitment` — Timestamp Metadata

The `recorded_at` field is set to `env.ledger().timestamp()` for
indexing only. It does not gate any operations.

---

## Simulating Time in Tests

Soroban's test environment lets you set the ledger timestamp manually:

```rust
env.ledger().with_mut(|l| l.timestamp = 5000);
```

### Example: Testing the Maturity Boundary

```rust
// Escrow initialized with maturity = 5000
env.ledger().with_mut(|l| l.timestamp = 4999);
// settle() panics — one second before maturity

env.ledger().with_mut(|l| l.timestamp = 5000);
// settle() succeeds — exactly at maturity
```

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
| Unit tests | Use `env.ledger().with_mut` to set exact timestamps |
| Integration tests on testnet | Add a safety buffer of at least 60s to maturity values |
| Production / mainnet | Treat maturity boundaries as approximate to ±30s of wall clock |
| Off-chain monitoring | Poll `get_escrow().maturity` and compare to latest ledger timestamp from Horizon |

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

## Security Notes

- **No wall-clock oracle:** all time comparisons use
  `env.ledger().timestamp()` only — no external time source.
- **No negative time:** `maturity` and `InvestorClaimNotBefore` are
  `u64` — they cannot be negative.
- **Overflow guard:** `committed_lock_secs` addition uses
  `checked_add(...).expect("investor claim time overflow")`.
- **Token economics:** time-based yield calculations are out of scope.
  See `escrow/src/external_calls.rs` for token transfer assumptions.