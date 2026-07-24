# Requirements: `unfund(investor, amount)` Entrypoint

## Background

The LiquiFact escrow contract exposes a `refund` entrypoint that returns investor principal,
but only when the escrow has been cancelled (status 4). Investors who wish to reduce or exit
their position while the escrow is still open (status 0) have no on-chain mechanism to do so.

This feature adds a new `unfund(investor, amount)` entrypoint to close that gap.

---

## Requirements

### R1 — Valid state guard

The `unfund` entrypoint **must** reject any call when `escrow.status != 0` with a typed error.
Unfunding is only permitted while the escrow is open and accepting contributions.

**Acceptance:** calling `unfund` with status 1, 2, 3, or 4 returns `EscrowNotOpen`.

---

### R2 — Legal hold guard

The `unfund` entrypoint **must** reject calls while a compliance/legal hold is active, returning
a typed error.

**Acceptance:** calling `unfund` while `DataKey::LegalHold` is `true` returns `LegalHoldActive`.

---

### R3 — Investor authorization

The `unfund` entrypoint **must** require authorization from the `investor` address.
Third-party callers cannot withdraw on behalf of an investor.

**Acceptance:** `env.auths()` records the investor address after a successful call.

---

### R4 — Over-withdrawal rejection

The `unfund` entrypoint **must** reject any `amount` that exceeds the investor's recorded
contribution, returning a typed error.

**Acceptance:** calling `unfund` with `amount > get_contribution(investor)` returns
`OverWithdrawal`.

---

### R5 — Partial unfund state correctness

After a successful partial unfund (amount < contribution):

- `get_contribution(investor)` decreases by exactly `amount`.
- `get_escrow().funded_amount` decreases by exactly `amount`.
- `get_unique_funder_count()` is **unchanged**.
- `escrow.status` remains 0.

**Acceptance:** all four assertions pass after a partial unfund call.

---

### R6 — Full unfund state correctness

After a successful full unfund (amount == contribution):

- `get_contribution(investor)` returns 0.
- `get_escrow().funded_amount` decreases by exactly `amount`.
- `get_unique_funder_count()` decreases by exactly 1.
- `escrow.status` remains 0.

**Acceptance:** all four assertions pass after a full unfund call.

---

### R7 — UniqueFunderCount floor

`UniqueFunderCount` **must never** go below zero, even when multiple full unfunds occur or
storage is in an adversarial state.

**Acceptance:** `saturating_sub(1)` is used; count stays ≥ 0 under all tested conditions.

---

### R8 — Arithmetic safety (no underflow panics)

All decrements inside `unfund` **must** use checked arithmetic. Specifically:

- `contribution.checked_sub(amount)` — returns `OverWithdrawal` on underflow rather than
  panicking.
- `escrow.funded_amount.checked_sub(amount)` — same error on underflow.

No `.unwrap()` on subtraction results.

**Acceptance:** underflow paths are exercised in tests and return typed errors, not panics.

---

### R9 — Token transfer

On a successful `unfund`, the contract **must** transfer exactly `amount` of the bound
funding token from the contract address to the `investor` address via
`external_calls::transfer_funding_token_with_balance_checks`.

This mirrors the `refund` entrypoint. The SEP-41 balance-delta invariants (sender decreases
by `amount`, recipient increases by `amount`) are enforced by the wrapper.

**Acceptance:** token balance of investor increases by `amount`; contract balance decreases by
`amount`; `EscrowError` propagates on any delta mismatch.

---

### R10 — Event emission

A successful `unfund` **must** emit an `EscrowUnfunded` event containing at minimum:
`investor`, `amount`, `remaining_contribution`, `new_funded_amount`, `timestamp`.

**Acceptance:** `env.events().all()` contains the expected `EscrowUnfunded` event after a
successful unfund call.

---

### R11 — Typed errors are append-only

Three new typed errors must be added to `EscrowError`:

| Variant | Code | Fires when |
|---------|------|-----------|
| `EscrowNotOpen` | 165 | `status != 0` |
| `OverWithdrawal` | 166 | `amount > contribution` |
| `LegalHoldActive` | 167 | hold is active |

Existing error codes must not be renumbered or removed.

**Acceptance:** `cargo build` passes; existing tests remain green; error numeric codes match.

---

### R12 — No modification to existing entrypoints

`unfund` is purely additive. The `refund`, `cancel_funding`, `fund`, `fund_impl`, and all
other existing entrypoints must be unchanged.

**Acceptance:** `git diff` shows no changes to any existing entrypoint logic; all existing
tests still pass.

---

### R13 — Lifecycle documentation updated

`docs/escrow-lifecycle.md` must be updated to:

- Show `unfund` as a self-loop on the `open (0)` state in the diagram.
- Include `unfund` in the valid transitions table.
- Include `unfund` in the legal hold interaction table.
- Add an "Investor unfund path" section documenting invariants.

**Acceptance:** doc file updated; content is accurate with respect to the implementation.
