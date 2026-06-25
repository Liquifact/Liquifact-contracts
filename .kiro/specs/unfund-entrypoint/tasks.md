# Tasks: `unfund(investor, amount)` Entrypoint

## Task 1 — Add typed errors and `EscrowUnfunded` event

**File:** `escrow/src/lib.rs`

Append three new error variants to the `EscrowError` enum (append-only — do not renumber or
remove existing variants):

```rust
/// [`LiquifactEscrow::unfund`] called when [`InvoiceEscrow::status`] is not 0 (open).
/// Unfunding is only valid while the escrow is still accepting contributions.
EscrowNotOpen = 165,

/// [`LiquifactEscrow::unfund`] requested amount exceeds the investor's recorded contribution.
/// Never withdraw more than was contributed; checked via [`i128::checked_sub`].
OverWithdrawal = 166,

/// [`LiquifactEscrow::unfund`] blocked because a compliance/legal hold is active.
/// No fund movement is permitted until the hold is cleared by the admin.
LegalHoldActive = 167,
```

Add the `EscrowUnfunded` event struct immediately after the existing `InvestorRefundedEvt`
struct:

```rust
#[contractevent]
pub struct EscrowUnfunded {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    #[topic]
    pub investor: Address,
    /// Amount withdrawn in this call.
    pub amount: i128,
    /// Investor's remaining contribution after this withdrawal.
    pub remaining_contribution: i128,
    /// Escrow's total funded_amount after this withdrawal.
    pub new_funded_amount: i128,
    /// Ledger timestamp at which the withdrawal occurred.
    pub timestamp: u64,
}
```

Add `EscrowUnfunded` to the `use super::*;` imports in the tests module so tests can
reference it.

**Verification:** `cargo build` passes; `cargo fmt --all -- --check` passes.

---

## Task 2 — Implement the `unfund` entrypoint

**File:** `escrow/src/lib.rs`

Add the `unfund` entrypoint inside `#[contractimpl] impl LiquifactEscrow`, immediately after
the `refund` entrypoint (before `is_investor_refunded`).

The implementation must follow this exact sequence:

### 1. Status guard (read-only)
```rust
let escrow = Self::get_escrow(env.clone());
ensure(&env, escrow.status == 0, EscrowError::EscrowNotOpen);
```

### 2. Legal-hold guard (read-only)
```rust
ensure(&env, !Self::legal_hold_active(&env), EscrowError::LegalHoldActive);
```

### 3. Investor auth
```rust
investor.require_auth();
```

### 4. Amount positive (matches existing fund pattern — panic)
Use the same zero-amount guard pattern as `fund_impl` (panic, not typed error, for zero/negative).
Or mirror `refund`'s implicit guard (contribution > 0 would already catch amount=0 in the
over-withdrawal check — choose the simpler approach consistent with `refund`).

### 5. Contribution read and over-withdrawal guard
```rust
let contribution: i128 =
    Self::get_persistent_investor_contribution(&env, investor.clone());
let remaining_contribution = contribution
    .checked_sub(amount)
    .unwrap_or_else(|| fail(&env, EscrowError::OverWithdrawal));
```

### 6. funded_amount decrement
```rust
let new_funded_amount = escrow
    .funded_amount
    .checked_sub(amount)
    .unwrap_or_else(|| fail(&env, EscrowError::OverWithdrawal));
let mut escrow = escrow; // rebind as mut
escrow.funded_amount = new_funded_amount;
```

Wait — `get_escrow` returns an owned value, so rebinding is fine. Just declare `mut escrow`
from the start.

### 7. Update InvestorContribution (persistent storage)
```rust
Self::set_persistent_investor_contribution(&env, investor.clone(), remaining_contribution);
```

### 8. Decrement UniqueFunderCount when contribution reaches zero
```rust
if remaining_contribution == 0 {
    let cur: u32 = env
        .storage()
        .instance()
        .get(&DataKey::UniqueFunderCount)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&DataKey::UniqueFunderCount, &cur.saturating_sub(1));
}
```

### 9. Persist updated escrow
```rust
env.storage().instance().set(&DataKey::Escrow, &escrow);
```

### 10. Token transfer (checks-effects-interactions)
```rust
let token_addr = Self::funding_token_or_fail(&env);
let this = env.current_contract_address();
// NOTE: If on-chain custody is disabled, the funding token address still exists but the
// contract holds no tokens; the transfer wrapper will fail. For off-chain custody setups,
// operators must ensure the contract balance covers outstanding unfund requests, or integrate
// a separate settlement layer. This path mirrors the refund() entrypoint behavior.
external_calls::transfer_funding_token_with_balance_checks(
    &env,
    &token_addr,
    &this,
    &investor,
    amount,
);
```

### 11. Event emission
```rust
let timestamp = env.ledger().timestamp();
EscrowUnfunded {
    name: symbol_short!("unfunded"),
    invoice_id: escrow.invoice_id.clone(),
    investor: investor.clone(),
    amount,
    remaining_contribution,
    new_funded_amount,
    timestamp,
}
.publish(&env);
```

### 12. Return updated escrow
```rust
escrow
```

**Full doc comment on the function** (see design.md for the template).

**Verification:** `cargo build` passes with no warnings; `cargo fmt --all -- --check` passes.

---

## Task 3 — Write tests in `escrow/src/tests/funding.rs`

Add the following tests at the bottom of `escrow/src/tests/funding.rs`. All tests must use
the existing `setup()`, `deploy()`, `default_init()`, `TARGET`, and `free_addresses()`
helpers from `tests.rs`.

For tests that require token transfers (on-chain custody path), use
`init_and_fund_with_real_token` and mint tokens into the escrow contract before calling
`unfund`. For accounting-only tests (no token check), use the standard mock-auth setup.

### Tests to implement:

**1. `test_unfund_partial`**
- Setup: fund investor with `TARGET / 2` twice (total `TARGET`)  
  Wait — `TARGET / 2 + TARGET / 2 = TARGET` which transitions to status 1, not 0.  
  Instead: fund with `TARGET / 4`. Verify status is still 0.
- Call `unfund(investor, TARGET / 8)`
- Assert: `get_contribution(investor) == TARGET / 8`, `get_escrow().funded_amount == TARGET / 8`,
  `get_unique_funder_count() == 1`, `escrow.status == 0`

**2. `test_unfund_full`**
- Fund investor with `TARGET / 4` (status stays 0)
- Call `unfund(investor, TARGET / 4)` (full exit)
- Assert: `get_contribution(investor) == 0`, `funded_amount == 0`,
  `get_unique_funder_count() == 0`, `status == 0`

**3. `test_unfund_funder_count_floor`**
- Use `env.as_contract` to inject `UniqueFunderCount = 0` and a contribution of 1
- Call unfund for the full amount
- Assert: `get_unique_funder_count() == 0` (not underflowed to u32::MAX)

**4. `test_unfund_over_withdrawal`**
- Fund investor with 1_000
- Call `unfund(investor, 1_001)` via `try_unfund`
- Assert: returns `EscrowError::OverWithdrawal`

**5. `test_unfund_wrong_status_funded`**
- Fund to TARGET (triggers status 1)
- Call `try_unfund(investor, 1)` 
- Assert: returns `EscrowError::EscrowNotOpen`

**6. `test_unfund_wrong_status_settled`**
- Fund to TARGET (status 1), then `settle()`
- Assert: `try_unfund` returns `EscrowError::EscrowNotOpen`

**7. `test_unfund_wrong_status_withdrawn`**
- Use `init_and_fund_with_real_token`; call `withdraw()`
- Assert: `try_unfund` returns `EscrowError::EscrowNotOpen`

**8. `test_unfund_wrong_status_cancelled`**
- Fund partially; call `cancel_funding()`
- Assert: `try_unfund` returns `EscrowError::EscrowNotOpen`

**9. `test_unfund_legal_hold_blocked`**
- Fund partially; call `set_legal_hold(true)`
- Assert: `try_unfund` returns `EscrowError::LegalHoldActive`

**10. `test_unfund_requires_investor_auth`**
- Fund and unfund successfully
- Assert: `env.auths().iter().any(|(addr, _)| *addr == investor)`

**11. `test_unfund_no_underflow`**
- Fund with 1_000; call `unfund(investor, 1_000)` (exact boundary)
- Assert: succeeds without panic; `get_contribution == 0`

**12. `test_unfund_multiple_investors_isolation`**
- Fund inv_a with 30_000, inv_b with 50_000
- Unfund inv_a partially (10_000)
- Assert: inv_a contribution = 20_000, inv_b contribution = 50_000 (unchanged),
  funded_amount = 70_000

**13. `test_unfund_then_fund_again`**
- Fund with 20_000; unfund 10_000; fund 5_000
- Assert: contribution = 15_000; funded_amount = 15_000

**14. `test_unfund_event_emitted` (uses Events testutil)**
- Deploy with `deploy_with_id`; fund and unfund
- Assert: `env.events().all()` contains an `EscrowUnfunded` event with correct fields

**Verification:** `cargo test` passes; no existing tests regress.

---

## Task 4 — Update `docs/escrow-lifecycle.md`

**File:** `docs/escrow-lifecycle.md`

### 4a — State diagram

Add a self-loop annotation to the `open (0)` box:

```text
                ┌─────────────┐
                │   (init)    │
                │  status = 0 │◄──── unfund(investor, amount) [investor]
                │    open     │      (partial or full; status stays 0)
                └──────┬──────┘
```

### 4b — Valid transitions table

Add a new row after the `0 → 4` row:

| From | To | Trigger | Auth required | Notes |
|------|----|---------|--------------|-------|
| `0` (open) | `0` (open) | `unfund(investor, amount)` | Investor auth; legal hold must be inactive | Partial unfund: funded_amount decreases, status stays 0. Full unfund (contribution → 0): UniqueFunderCount decrements, status stays 0 |

### 4c — Legal hold interaction table

Add `unfund()` → Yes (blocked by legal hold):

| Function | Blocked by legal hold |
|----------|----------------------|
| `unfund()` | Yes |

(Insert in alphabetical order with existing entries.)

### 4d — New section: Investor unfund path (status 0 — open)

Add the following section after the "Investor refund path" section:

```markdown
## Investor unfund path (status 0 — open)

While an escrow is open, investors may reduce or fully exit their principal position
without requiring admin cancellation:

1. Investor calls `unfund(investor, amount)` — decrements `DataKey::InvestorContribution`
   and `InvoiceEscrow::funded_amount` by `amount`.
2. If contribution reaches zero: `DataKey::InvestorContribution` entry is zeroed,
   `DataKey::UniqueFunderCount` is decremented (floor: 0).
3. Status remains 0 (open) in all cases — `unfund` never transitions status.
4. Tokens are returned to the investor via `external_calls::transfer_funding_token_with_balance_checks`
   (SEP-41 balance-delta invariants enforced).
5. `EscrowUnfunded` is emitted with the investor, amount, remaining contribution,
   new funded_amount, and ledger timestamp.

### Invariants

- Investor can only unfund their own contribution; no third-party unfunding.
- `unfund` is blocked while a legal hold is active.
- `unfund` is blocked in any state other than open (0).
- `amount` must be ≤ `DataKey::InvestorContribution[investor]` (OverWithdrawal error otherwise).
- `funded_amount` never goes negative (checked arithmetic).
- `UniqueFunderCount` never goes negative (saturating_sub).

### Events emitted

| Event | When |
|-------|------|
| `EscrowUnfunded` | `unfund()` succeeds |
```

**Verification:** doc renders correctly; no broken markdown; content matches implementation.

---

## Task 5 — Verification and cleanup

Run the full test suite and confirm:

1. `cargo fmt --all -- --check` — no formatting issues
2. `cargo build` — no warnings (use `RUSTFLAGS="-D warnings"` or equivalent)
3. `cargo test` — all tests pass, including new unfund tests
4. All previously passing tests still pass (no regressions)

Fix any issues found. Clean up any temporary files.

Produce a brief summary including:
- Full `cargo test` output (pass/fail counts)
- Security notes section covering:
  - Underflow prevention (checked_sub, saturating_sub)
  - Auth enforcement (require_auth placement)
  - Hold guard (read-only before auth)
  - Status guard (read-only before auth)
  - Custody path safety (SEP-41 delta invariants)
