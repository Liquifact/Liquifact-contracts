# Pause Authorization Rules

> Issue: [#826](https://github.com/Liquifact/Liquifact-contracts/issues/826)
> Status: Accepted
> Scope: `escrow/src/lib.rs`

---

## Overview

The **operational pause** is a lightweight incident-response circuit breaker controlled exclusively by the escrow's current `admin`. It is a single-call toggle (no clear delay, no multi-phase handshake) designed for rapid response to events such as a suspected token bug, oracle feed anomaly, or operational incident.

The pause is **orthogonal** to the compliance/legal hold: each flag blocks its gated entrypoints independently, and toggling one never reads or writes the other's storage key.

---

## 1. Roles

| Role | Can call `set_paused`? | Gated by pause? |
|---|---|---|
| `escrow.admin` (current) | **Yes** — sole authorizer | No — `set_paused` itself has no pause gate |
| `escrow.sme_address` | No — panics on `require_auth` | **Yes** — `settle`, `withdraw` are pause-gated |
| Investors | No | **Yes** — `fund`, `fund_with_commitment`, `fund_batch`, `claim_investor_payout` are pause-gated |
| `DataKey::PendingAdmin` | No | No |
| Treasury | No | No — `sweep_terminal_dust` is **not** pause-gated |
| Any other address | No | N/A |

**Key insight:** Only the current `escrow.admin` may call `set_paused`. Admin rotation (`propose_admin` + `accept_admin`) is **not** blocked by pause, so a governance-controlled admin can always rotate authority to clear a stale pause.

---

## 2. Auth Implementation

### 2.1 `set_paused(env, active: bool)`

```text
escrow/src/lib.rs line ~3096
```

| Step | Detail |
|---|---|
| 1. Load escrow + auth | `let escrow = Self::load_escrow_require_admin(&env);` |
| 2. Storage write | `env.storage().instance().set(&DataKey::Paused, &active);` |
| 3. Emit event | `PausedChanged { name: "paused", invoice_id, active: 0\|1 }` |

`load_escrow_require_admin` (line ~2285) does:

```rust
fn load_escrow_require_admin(env: &Env) -> InvoiceEscrow {
    let escrow: InvoiceEscrow = env
        .storage()
        .instance()
        .get(&DataKey::Escrow)
        .unwrap_or_else(|| fail(env, EscrowError::EscrowNotInitialized));
    escrow.admin.require_auth();       // ← panics if caller != escrow.admin
    escrow
}
```

If the caller is not the current `escrow.admin`, `require_auth()` panics the host function — no storage write occurs, no event is emitted. If `DataKey::Escrow` is absent (escrow not initialized), the call fails with `EscrowError::EscrowNotInitialized` (20).

### 2.2 `paused_active(env) -> bool` (internal)

```text
escrow/src/lib.rs line ~1643
```

```rust
fn paused_active(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)   // absent key ⇒ default false (not paused)
}
```

### 2.3 `is_paused(env) -> bool` (public read-only view)

```text
escrow/src/lib.rs line ~2361
```

```rust
pub fn is_paused(env: Env) -> bool {
    Self::paused_active(&env)
}
```

No `require_auth` — any caller can read the flag.

---

## 3. Entrypoints Gated by Pause

The pause gate is a **read-only precondition** that fires **before** `Address::require_auth()`, per the canonical authorization ordering (ADR-002):

```
1. Read-only preconditions (pause, legal hold, status checks, input validation)
2. Address::require_auth() for the bound role
3. Storage writes and token transfers (external_calls only)
```

### 3.1 Gated entrypoints and their typed errors

| Entrypoint | Pause error code | Error variant | Auth after pause gate? |
|---|---|---|---|
| `fund` | 210 | `PausedBlocksFunding` | `investor.require_auth()` |
| `fund_with_commitment` | 210 | `PausedBlocksFunding` | `investor.require_auth()` |
| `fund_batch` | 210 | `PausedBlocksFunding` | Per-investor `require_auth()` in loop |
| `settle` | 211 | `PausedBlocksSettlement` | `sme_address.require_auth()` |
| `withdraw` | 212 | `PausedBlocksWithdrawal` | `sme_address.require_auth()` |
| `claim_investor_payout` | 213 | `PausedBlocksInvestorClaims` | `investor.require_auth()` |

All six entrypoints apply the gate as:

```rust
ensure(
    &env,
    !Self::paused_active(&env),
    EscrowError::PausedBlocks<Entrypoint>,
);
```

### 3.2 Guard ordering: pause fires before legal hold

When **both** pause and legal hold are active, the pause gate fires **first** (it appears earlier in the pre-`require_auth` sequence). For example, `settle`:

```text
1. paused_active() check     → panics with PausedBlocksSettlement (211)
2. legal_hold_active() check → would panic with LegalHoldBlocksSettlement (120)
3. status != 2 check         → would panic with EscrowAlreadySettled (236) if already settled
4. status == 1 check         → would panic with SettlementNotFunded (121)
5. maturity check            → would panic with MaturityNotReached (122)
6. sme_address.require_auth()
7. Storage write
```

This ordering is intentional and tested: `escrow/src/tests/pause.rs` §12 covers the precedence with tests like `pause_gate_fires_before_legal_hold_settle`.

### 3.3 Worked example: pause / unpause lifecycle

Start with an initialized, funded escrow (`status = 1`).

**Step 1 — Admin pauses:**
```
caller: escrow.admin
set_paused(env, true)
→ loads escrow, escrow.admin.require_auth() ✓
→ DataKey::Paused ← true
→ emits PausedChanged { name: "paused", invoice_id: "INV_001", active: 1 }
```

**Step 2 — SME tries to settle while paused:**
```
caller: escrow.sme_address
settle(env)
→ paused_active() → true → PANIC with PausedBlocksSettlement (211)
→ (never reaches sme_address.require_auth(), no storage write)
```

**Step 3 — Investor tries to claim while paused:**
```
caller: investor_address
claim_investor_payout(env, investor)
→ paused_active() → true → PANIC with PausedBlocksInvestorClaims (213)
→ (never reaches investor.require_auth())
```

**Step 4 — Admin unpauses:**
```
caller: escrow.admin
set_paused(env, false)
→ loads escrow, escrow.admin.require_auth() ✓
→ DataKey::Paused ← false
→ emits PausedChanged { name: "paused", invoice_id: "INV_001", active: 0 }
```

**Step 5 — SME settles successfully after unpause:**
```
caller: escrow.sme_address
settle(env)
→ paused_active() → false ✓
→ legal_hold_active() → false ✓
→ status == 1 ✓
→ maturity check ✓ (if applicable)
→ sme_address.require_auth() ✓
→ DataKey::Escrow set (status ← 2)
→ emits EscrowSettled
```

---

## 4. Entrypoints NOT Gated by Pause

The following state-mutating entrypoints are **not** affected by the operational pause. They remain callable by their authorized roles regardless of the pause flag:

| Entrypoint | Authorized role | Reason excluded |
|---|---|---|
| `set_paused` | `escrow.admin` | Self-evident — can't clear pause if gated |
| `set_legal_hold` / `clear_legal_hold` | `escrow.admin` | Compliance hold is orthogonal to operational pause |
| `propose_admin` | `escrow.admin` | Admin rotation must remain available to un-pause |
| `accept_admin` | `DataKey::PendingAdmin` | Admin rotation must remain available to un-pause |
| `cancel_pending_admin` | `escrow.admin` | Admin rotation maintenance |
| `update_maturity` | `escrow.admin` | Configuration change; no risk-bearing token movement |
| `update_funding_target` | `escrow.admin` | Configuration change; only while open |
| `lower_max_unique_investors` | `escrow.admin` | Configuration change; only while open |
| `raise_max_unique_investors` | `escrow.admin` | Configuration change |
| `lower_min_contribution_floor` | `escrow.admin` | Configuration change; only while open |
| `update_maturity_max_horizon` | `escrow.admin` | Configuration change |
| `extend_funding_deadline` | `escrow.admin` | Configuration change |
| `partial_settle` | `sme_address` or `admin` | Deliberately excluded — admin-controlled oversight |
| `sweep_terminal_dust` | `treasury` | Only legal-hold-gated; runs in terminal states only |
| `refund` / `refund_batch` | investor | Operational in cancelled state |
| `cancel_funding` | `escrow.admin` | Operational for cancellation path |
| `unfund` | investor | Operational while open |
| `record_sme_collateral_commitment` | `escrow.sme_address` | Metadata-only (no token movement) |
| `clear_sme_collateral_commitment` | `escrow.sme_address` | Metadata-only |
| `rotate_beneficiary` | `sme_address` or `admin` | Pre-settlement administration |
| `bind_primary_attestation_hash` | `escrow.admin` | Attestation management |
| `append_attestation_digest` | `escrow.admin` | Attestation management |
| `revoke_attestation_digest` | `escrow.admin` | Attestation management |
| `set_allowlist_active` | `escrow.admin` | Allowlist management |
| `set_investor_allowlisted` | `escrow.admin` | Allowlist management |
| `rebind_registry_ref` | `escrow.admin` | Registry hint (read-only metadata) |
| `migrate` | none (all paths panic) | No-op; must add auth guard before implementing |

All `get_*` and `is_*` read-only views are unaffected by pause — they carry no `require_auth` and read storage without gates.

---

## 5. Event: `PausedChanged`

```rust
#[contractevent]
pub struct PausedChanged {
    #[topic]
    pub name: Symbol,         // "paused"
    #[topic]
    pub invoice_id: Symbol,
    pub active: u32,           // 1 = pause enabled, 0 = cleared
}
```

Emitted by **every** `set_paused` call, including no-op calls (`set_paused(true)` when already true).

Topics for indexer filtering:
- **Topic 1:** `"paused"` (Symbol)
- **Topic 2:** `invoice_id` (Symbol)

See also: [`docs/escrow-events.md` § `PausedChanged`](escrow-events.md#pausedchanged).

---

## 6. No-Op Behavior

`set_paused` intentionally does **not** guard against redundant calls. Calling `set_paused(true)` when pause is already active succeeds silently (still emits `PausedChanged`). Same for `set_paused(false)` when already cleared.

| Call | Current state | Result |
|---|---|---|
| `set_paused(true)` | `paused = false` | `paused ← true`, event emitted (`active: 1`) |
| `set_paused(true)` | `paused = true` | no state change, event emitted (`active: 1`) |
| `set_paused(false)` | `paused = true` | `paused ← false`, event emitted (`active: 0`) |
| `set_paused(false)` | `paused = false` | no state change, event emitted (`active: 0`) |
| `set_paused(_)` | escrow not initialized | panics with `EscrowNotInitialized` (20) |

---

## 7. Storage Key

- **Key:** `DataKey::Paused`
- **Type:** `bool`
- **Storage class:** Instance
- **Default when absent:** `false` (not paused)
- **Read pattern:** `env.storage().instance().get(&DataKey::Paused).unwrap_or(false)`

---

## 8. Orthogonality to Legal Hold

| Property | Operational pause | Legal hold |
|---|---|---|
| Storage key | `DataKey::Paused` | `DataKey::LegalHold` |
| Setter | `set_paused(active)` | `set_legal_hold(active)` / `clear_legal_hold()` |
| Clear delay | None (single-call) | Optional two-phase (`request_clear_legal_hold` → `clear_legal_hold`) |
| Event | `PausedChanged` | `LegalHoldChanged` |
| Compliance semantics | None — operational only | Yes — compliance/regulatory |
| Gated entrypoints | `fund`, `settle`, `withdraw`, `claim_investor_payout` | Same six + `sweep_terminal_dust`, `partial_settle`, `cancel_funding`, `rotate_beneficiary` |

**Key:** Either flag independently blocks the gated entrypoints. Clearing one does not clear the other. The setter functions never read or write the other flag's storage key.

When both flags are simultaneously active, the pause gate fires first in the canonical guard-ordering sequence (see §3.2).

---

## 9. Security Considerations

1. **Admin custody:** A compromised or lost admin key can pause the escrow indefinitely. Production deployments **must** use a governed admin (multisig or DAO) so the pause cannot strand funds. See `docs/escrow-security-checklist.md` §5.10.

2. **No expiry:** The pause has no programmatic expiry. It remains active until the current (or rotated-in) admin calls `set_paused(false)`.

3. **Read-only gate:** The pause check is a read-only storage read — it does not change state and does not depend on timestamps or external oracles. A failed pause gate leaves the contract completely untouched (no partial writes).

4. **No-pause bypass:** There is no entrypoint to bypass the pause — not even for the admin. The only way to make a paused entrypoint callable again is `set_paused(false)`.

5. **Pause during legal hold:** Pausing while a legal hold is active adds an extra layer of blocking. Even if the legal hold is cleared, the pause must also be cleared before the gated entrypoints become callable.

---

## 10. Test Coverage

Full test coverage in `escrow/src/tests/pause.rs`:

| Test category | Tests |
|---|---|
| Admin gating | `set_paused_by_admin_succeeds`, `set_paused_by_non_admin_panics` |
| Blocked entrypoints | `fund_blocked_when_paused`, `fund_with_commitment_blocked_when_paused`, `fund_batch_blocked_when_paused`, `settle_blocked_when_paused`, `withdraw_blocked_when_paused`, `claim_investor_payout_blocked_when_paused` |
| Unblock after unpause | `fund_succeeds_after_unpause`, `fund_with_commitment_succeeds_after_unpause`, `fund_batch_succeeds_after_unpause`, `settle_succeeds_after_unpause`, `withdraw_succeeds_after_unpause`, `claim_investor_payout_succeeds_after_unpause` |
| No-op calls | `set_paused_true_when_already_true_is_noop`, `set_paused_false_when_already_false_is_noop` |
| Events | `set_paused_emits_event` |
| Read views unaffected | `read_views_unaffected_by_pause`, `read_views_unaffected_by_pause_on_open_escrow` |
| Orthogonality | `pause_orthogonal_to_legal_hold` |
| Precedence (pause > legal hold) | `pause_gate_fires_before_status_validation_fund`, `pause_gate_fires_before_legal_hold_fund`, `pause_gate_fires_before_legal_hold_settle`, `pause_gate_fires_before_legal_hold_withdraw`, `pause_gate_fires_before_legal_hold_claim` |
| Typed errors | `fund_returns_typed_error_when_paused`, `settle_returns_typed_error_when_paused`, `withdraw_returns_typed_error_when_paused`, `claim_returns_typed_error_when_paused` |
| Toggle cycle | `pause_toggle_cycle` |

---

## 11. Cross-References

- **Implementation:** `escrow/src/lib.rs` — `set_paused` (~line 3096), `paused_active` (~line 1643), `is_paused` (~line 2361)
- **Tests:** `escrow/src/tests/pause.rs`
- **Events:** `docs/escrow-events.md` § `PausedChanged`
- **Security checklist:** `docs/escrow-security-checklist.md` §5.10, §6
- **Auth boundaries:** `docs/adr/ADR-002-auth-boundaries.md`
- **State machine:** `docs/STATE_MACHINE_IMPLEMENTATION.md`
- **Lifecycle:** `docs/escrow-lifecycle.md`
