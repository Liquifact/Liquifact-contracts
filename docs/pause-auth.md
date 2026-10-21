# Pause Authorization Rules

> Issue: [#826](https://github.com/Liquifact/Liquifact-contracts/issues/826)
> Status: Accepted
> Scope: `escrow/src/lib.rs`

---

## Overview

The **operational pause** is a lightweight incident-response circuit breaker controlled exclusively by the escrow's current `admin`. It is a single-call toggle (no clear delay, no multi-phase handshake) designed for rapid response to events such as a suspected token bug, oracle feed anomaly, or operational incident.

An optional, dedicated **pauser** key may also activate a scoped pause (see §2.4) without waiting on a governed multisig. The pauser is deliberately **one-way**: it can pause but never unpause, and it can change no pause configuration.

The pause is **orthogonal** to the compliance/legal hold: each flag blocks its gated entrypoints independently, and toggling one never reads or writes the other's storage key.

---

## 1. Roles

| Role | Can call `set_paused`? | Can call `pause_as_pauser`? | Gated by pause? |
|---|---|---|---|
| `escrow.admin` (current) | **Yes** — sole authorizer | No | No — `set_paused` itself has no pause gate |
| `DataKey::Pauser` (optional, dedicated) | No | **Yes** — activate only, never clear | No |
| `escrow.sme_address` | No — panics on `require_auth` | No — may not be installed as pauser | **Yes** — `settle`, `withdraw` are pause-gated |
| Investors | No | No | **Yes** — `fund`, `fund_with_commitment`, `fund_batch`, `claim_investor_payout` are pause-gated |
| `DataKey::PendingAdmin` | No | No | No |
| Treasury | No | No — may not be installed as pauser | No — `sweep_terminal_dust` is **not** pause-gated |
| Any other address | No | No — rejected with `NotPauser` (263) | N/A |

**Key insight:** Only the current `escrow.admin` may call `set_paused`. Admin rotation (`propose_admin` + `accept_admin`) is **not** blocked by pause, so a governance-controlled admin can always rotate authority to clear a stale pause.

**Pauser insight:** The pauser widens the *response* surface without widening the *control* surface. It can only ever add a pause; every lever that could remove, weaken, or reconfigure one stays with the admin (see §2.4).

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

### 2.4 The emergency-pauser role

> Issue: [#1548](https://github.com/opratem/Liquifact-contracts/issues/1548)

Three new entrypoints and one new storage key implement a dedicated operational key that can
**activate** a pause immediately, without waiting on a governed multisig.

| Entrypoint | Authorized role | Purpose |
|---|---|---|
| `set_pauser(pauser: Option<Address>) -> Option<Address>` | `escrow.admin` | Install / revoke the pauser. Returns the previously configured pauser. |
| `get_pauser() -> Option<Address>` | none (read-only) | Read the configured pauser; `None` when unset. |
| `pause_as_pauser(pauser, scope, reason)` | the stored pauser | Activate (or widen) a scoped pause. |

#### 2.4.1 `set_pauser` — installation rules

```text
escrow/src/lib.rs — set_pauser
```

| Step | Detail |
|---|---|
| 1. Load escrow + admin auth | `Self::load_escrow_require_admin(&env)` — panics if the caller is not the current admin |
| 2. Role-conflict check | Reject the SME address and the treasury address with `PauserRoleConflict` (262) |
| 3. Storage write | `DataKey::Pauser ← Address`, or `remove` when `None` |
| 4. Emit event | `PauserChanged { name: "pauser", invoice_id, pauser: Option<Address> }` |

The pauser must be a **dedicated** key. It may not be the SME address (which already carries
risk-bearing authority) nor the treasury address (which receives protocol fees) — otherwise a
single key would wear two hats on the same escrow.

`None` revokes the pauser: the key is removed from storage and every later
`pause_as_pauser` call fails with `NotPauser` (263).

#### 2.4.2 `pause_as_pauser` — guard ordering

```text
escrow/src/lib.rs — pause_as_pauser
```

| Step | Detail |
|---|---|
| 1. Load escrow | `DataKey::Escrow` — `EscrowNotInitialized` (20) if absent |
| 2. Verify the caller is the stored pauser | `stored_pauser == pauser` else `NotPauser` (263) |
| 3. Auth | `pauser.require_auth()` — the pauser can never be impersonated |
| 4. Rate-limit window | `pause_toggle_window(&env)` — **shared** with the admin, one combined count |
| 5. Activate | `activate_pause(&env, invoice_id, scope, reason)` — identical state write and event as the admin path |
| 6. Commit the rate-limit slot | `commit_pause_toggle(&env, …)` |

Steps 2 and 3 are ordered so that a **wrong** caller gets the typed `NotPauser` error rather
than an opaque auth panic, while a **correct** caller can never be impersonated: the address
passed as the argument must be the one stored on-chain, and that same address must sign the
invocation.

#### 2.4.3 What the pauser may **not** do

| Action | Who may do it | Enforced by |
|---|---|---|
| Un-pause | `escrow.admin` only | `set_paused(false, …)` calls `load_escrow_require_admin`; the pauser has no un-pause entrypoint at all |
| Change pause configuration | `escrow.admin` only | `set_pause_max_duration`, `set_pause_cooldown`, `set_pause_rate_limit` are all admin-only |
| Install / revoke a pauser | `escrow.admin` only | `set_pauser` is admin-only |
| Extend a pause past its max duration | nobody | `activate_pause` never resets `PausedAt`; a re-pause only changes `scope`/`reason` |
| Narrow an active pause's scope | nobody | `PauseAlreadyActive` (259) — the pauser may only widen |
| Pause again during the cooldown | nobody | `PauseCooldownActive` (260) |

#### 2.4.4 Shared semantics with the admin path

`pause_as_pauser` and `set_paused(true, …)` call the **same** `activate_pause` helper, so they
write byte-identical state and emit a byte-identical `PausedChanged { active: 1 }` event. They
also draw from the **same** rolling rate-limit window: the configured limit applies to the
*combined* count of admin toggles and pauser toggles, so a pauser cannot be used to bypass a
rate limit configured to protect the admin.

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
| `pause_as_pauser` | `DataKey::Pauser` | The pauser may only *add* a pause; it is never gated by one |
| `set_pauser` | `escrow.admin` | Role management — a gated pauser could not be revoked |
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
    pub scope: PauseScope,     // which flows the pause blocks (on clear, the scope lifted)
    pub reason: PauseReason,   // why the pause was applied (on clear, the lifted reason)
}
```

Emitted by **every** `set_paused` call, including no-op calls (`set_paused(true)` when already
true), and by every successful `pause_as_pauser` call. A pauser-initiated pause emits a
byte-identical event to an admin-initiated one.

Topics for indexer filtering:
- **Topic 1:** `"paused"` (Symbol)
- **Topic 2:** `invoice_id` (Symbol)

See also: [`docs/escrow-events.md` § `PausedChanged`](escrow-events.md#pausedchanged).

---

## 5.1 Event: `PauserChanged`

```rust
#[contractevent]
pub struct PauserChanged {
    #[topic]
    pub name: Symbol,            // "pauser"
    #[topic]
    pub invoice_id: Symbol,
    pub pauser: Option<Address>, // None = revoked
}
```

Emitted by **every** `set_pauser` call — on installation, on replacement, and on revocation
(`pauser: None`).

Topics for indexer filtering:
- **Topic 1:** `"pauser"` (Symbol)
- **Topic 2:** `invoice_id` (Symbol)

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

### 7.1 Storage Key: `DataKey::Pauser`

- **Key:** `DataKey::Pauser`
- **Type:** `Option<Address>` (stored as a bare `Address`; absence means "no pauser")
- **Storage class:** Instance
- **Default when absent:** `None` — no pauser configured, so `pause_as_pauser` always fails with
  `NotPauser` (263)
- **Written by:** `set_pauser` (admin-only)
- **Additive key (ADR-007):** appended at the end of the `DataKey` enum, so instances deployed
  before this change simply read the default.

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

6. **The pauser is a one-way key:** The pauser can only ever *add* a pause. It cannot un-pause,
   cannot change any pause configuration (max duration, cooldown, rate limit), cannot appoint or
   revoke a pauser, and cannot extend a pause past its max duration — every one of those levers
   remains admin-only. A compromised pauser therefore causes at most a temporary DoS that the
   admin can clear, and nothing else.

7. **Pauser blast radius is bounded:** A pauser-initiated pause is subject to exactly the same
   max-duration expiry, cooldown, and shared rate limit as an admin-initiated one. The pauser
   cannot pause indefinitely and cannot use its own toggles to exhaust the admin's rate-limit
   budget without also consuming its own share of that same budget.

8. **Dedicated key enforced at install time:** `set_pauser` rejects the SME address and the
   treasury address with `PauserRoleConflict` (262), so the pauser can never be a key that
   already carries risk-bearing or fee-receiving authority on the same escrow.

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

Full test coverage for the emergency-pauser role in `escrow/src/tests/pauser_role.rs`:

| Test category | Tests |
|---|---|
| Core flow | `pauser_pauses_funding_scope_and_admin_unpauses` |
| Scope rules | `pauser_may_widen_but_not_narrow_the_scope` |
| Cannot un-pause | `pauser_cannot_unpause` |
| Cannot reconfigure | `pauser_cannot_change_pause_configuration` |
| Cannot appoint | `pauser_cannot_set_pauser` |
| Authorization | `non_pauser_cannot_pause_as_pauser`, `no_pauser_configured_fails_with_not_pauser`, `clearing_pauser_revokes_it` |
| Role conflicts | `pauser_may_not_be_sme_or_treasury`, `set_pauser_by_non_admin_panics` |
| Shared rate limit | `rate_limit_is_shared_across_admin_and_pauser`, `rate_limit_is_shared_across_pauser_and_admin` |
| Events | `event_payloads_are_identical_between_admin_and_pauser`, `set_pauser_emits_pauser_changed_event` |
| Bounded blast radius | `pauser_cannot_extend_the_max_duration`, `pauser_pause_respects_the_cooldown` |

---

## 11. Cross-References

- **Implementation:** `escrow/src/lib.rs` — `set_paused`, `set_pauser`, `get_pauser`,
  `pause_as_pauser`, `activate_pause`, `pause_toggle_window`, `commit_pause_toggle`,
  `paused_active`, `is_paused`
- **Tests:** `escrow/src/tests/pause.rs`, `escrow/src/tests/pause_bypass.rs`,
  `escrow/src/tests/pauser_boundary_tests.rs`, `escrow/src/tests/pauser_role.rs`
- **Events:** `docs/escrow-events.md` § `PausedChanged`
- **Security checklist:** `docs/escrow-security-checklist.md` §5.10, §6
- **Auth boundaries:** `docs/adr/ADR-002-auth-boundaries.md`
- **State machine:** `docs/STATE_MACHINE_IMPLEMENTATION.md`
- **Lifecycle:** `docs/escrow-lifecycle.md`
