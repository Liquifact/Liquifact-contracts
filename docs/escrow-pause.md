# Escrow Pause Switch — Incident Response Reference

`DataKey::Paused` is a lightweight boolean flag stored in contract instance
storage. When `true` it blocks every risk-bearing entrypoint for rapid incident
response. This document describes who can toggle it, which entrypoints are
gated, how auto-expiry and rate limiting work, and how the pause relates to the
compliance legal hold.

---

## Who can pause

Only the **current** [`InvoiceEscrow::admin`] may call `set_paused`. There is
no secondary role, timelock, or multisig break-glass — production `admin` must
be a governed address (multisig or DAO). This matches the same auth boundary
as [`set_legal_hold`].

```rust
pub fn set_paused(env: Env, active: bool) {
    let escrow = Self::load_escrow_require_admin(&env);
    // rate-limit check, storage write, event emission
}
```

---

## Gated entrypoints

When `paused_active()` returns `true`, the following entrypoints are blocked:

| Function | Error variant |
|---|---|
| `fund` / `fund_with_commitment` | `PausedBlocksFunding` (210) |
| `settle` | `PausedBlocksSettlement` (211) |
| `withdraw` | `PausedBlocksWithdrawal` (212) |
| `claim_investor_payout` | `PausedBlocksInvestorClaims` (213) |

The check calls `paused_active(&env)`, which:

1. Reads `DataKey::Paused` — returns `false` if absent.
2. Reads `DataKey::PausedAt` — returns `false` if absent (inconsistent state).
3. Reads `DataKey::PauseMaxDuration` — if non-zero, computes `expiry = paused_at + max_duration`. If `ledger.timestamp() >= expiry`, the pause is considered expired (auto-cleared) even if `DataKey::Paused` is still `true`.

Operations that are **not** gated by the pause:

- `get_*` accessors
- `set_paused` itself (admin can always unpause)
- `set_pause_max_duration`, `get_pause_max_duration`, `set_pause_rate_limit`
- `set_legal_hold`, `clear_legal_hold`, `request_clear_legal_hold`
- `record_sme_collateral_commitment`
- `bind_primary_attestation_hash` / `append_attestation_digest`
- `update_maturity`, `update_funding_target`, `propose_admin`, `accept_admin`, `migrate`

---

## How unpausing differs from clearing a legal hold

| Property | Pause (`set_paused`) | Legal hold (`set_legal_hold`) |
|---|---|---|
| Activation | Single admin call | Single admin call |
| Clearing | Single admin call (`set_paused(false)`) | Two-step: `request_clear_legal_hold` + `set_legal_hold(false)` (when delay > 0) |
| Auto-expiry | Optional via `set_pause_max_duration` | None — manual clear required |
| Rate limiting | Configurable via `set_pause_rate_limit` | None |
| Semantic intent | Incident response (e.g. token bug) | Compliance/legal freeze |
| Persistence | Cleared on auto-expiry or admin action | Persists until admin clears |

The pause is designed for **operational** incidents where a quick toggle is
needed. The legal hold is designed for **compliance** scenarios that may require
a mandated cooling-off window before clearing.

---

## Precedence: pause checked first

When **both** `DataKey::Paused` and `DataKey::LegalHold` are active, the
pause gate is evaluated **first** in every gated entrypoint. This means:

- The transaction fails with a `PausedBlocks*` variant (210–213), **not** a
  `LegalHoldBlocks*` variant (102, 120, 123, 125).
- The legal hold error is never surfaced while the pause is active on the same
  entrypoint.
- Clearing the pause (or its auto-expiry) will **then** expose the legal hold
  gate if it is still active.

This ordering is intentional: the operational pause is a faster, more
fine-grained tool for active incidents, while the legal hold is a heavier
compliance lock. The lighter gate fires first to minimise latency during an
incident. See [`escrow-legal-hold.md`](escrow-legal-hold.md) for the full legal
hold reference.

---

## Auto-expiry (`set_pause_max_duration`)

The admin may configure a maximum duration (in ledger seconds) after which the
pause automatically expires:

```
set_pause_max_duration(duration: u64)
  ├─ 0            → no auto-expiry (legacy behaviour)
  ├─ 300 .. 2_592_000 → pause expires after `duration` seconds
  └─ outside bounds → EscrowError::PauseMaxDurationOutOfRange (230)
```

Auto-expiry is read inside `paused_active()`: if `PausedAt + max_duration <=
ledger.timestamp()`, the function returns `false` even though `DataKey::Paused`
is still `true` in storage. The stored flag is **not** automatically cleared —
a subsequent admin call to `set_paused(true)` will re-activate with a fresh
timestamp.

The current pause activation timestamp is stored in `DataKey::PausedAt` and
overwritten on each `set_paused(true)` call.

---

## Rate limiting (`set_pause_rate_limit`)

The admin may configure a toggle rate limit to prevent rapid on/off cycling:

```
set_pause_rate_limit(limit: u32, window_secs: u64)
  ├─ (0, 0)        → rate limiting disabled
  ├─ (limit, window) → max `limit` toggles per rolling `window_secs` window
  └─ invalid combo  → EscrowError::PauseRateLimitInvalidCombination (233)
     limit out of bounds → EscrowError::PauseToggleLimitOutOfRange (231)
     window out of bounds → EscrowError::PauseToggleWindowOutOfRange (232)
```

When a rate limit is configured, `set_paused` increments a counter in
`DataKey::PauseToggleCountInWindow`. If the counter reaches `limit` before the
window expires, further toggles fail with
`EscrowError::PauseToggleRateLimitExceeded` (234). The window resets when the
ledger timestamp exceeds `PauseToggleWindowStart + window_secs`.

---

## Configuration functions summary

| Function | Effect | Error if |
|---|---|---|
| `set_paused(true/false)` | Toggle pause state | Rate limit exceeded |
| `set_pause_max_duration(duration)` | Set auto-expiry window | Duration out of bounds |
| `get_pause_max_duration()` | Read current auto-expiry | — |
| `set_pause_rate_limit(limit, window)` | Set toggle rate limit | Invalid combination or out of bounds |
| `is_paused()` | Read current pause state | — |

---

## Incident response procedure

1. **Assess** whether the incident needs an operational pause (quick) or a
   compliance legal hold (timelocked). Refer to the decision tree in
   [`OPERATOR_RUNBOOK.md`](OPERATOR_RUNBOOK.md) §7.
2. **Pause** via `set_paused(true)` (single admin call, no delay).
3. **Optionally configure** an auto-expiry via `set_pause_max_duration` so the
   pause self-clears after the incident window.
4. **Resolve** the underlying incident off-chain.
5. **Unpause** via `set_paused(false)` (or wait for auto-expiry).
6. **If a legal hold was also active:** the gated entrypoints will remain
   blocked with `LegalHoldBlocks*` errors until the hold is cleared via the
   two-step process documented in [`escrow-legal-hold.md`](escrow-legal-hold.md).

---

## Test coverage

The matrix in `escrow/src/tests/pause.rs` covers:

1. Default state after `init`: `is_paused()` is `false`.
2. `set_paused(true)` blocks `fund`, `settle`, `withdraw`, `claim_investor_payout`.
3. `set_paused(false)` restores normal operation.
4. Non-admin call to `set_paused` panics with auth failure.
5. Pause is orthogonal to legal hold — both can be active independently.
6. Pause check fires before legal hold check (precedence).
7. Auto-expiry: `set_pause_max_duration` + ledger advancement.
8. `set_pause_max_duration` bounds enforcement (min/max, zero).
9. `set_pause_rate_limit` bounds enforcement.
10. Rate limit blocks excessive toggles.
11. `PauseMaxDurationUpdated` and `PauseRateLimitUpdated` events are emitted on
    configuration changes.
12. `PausedChanged` event is emitted on every toggle.

---

## Out-of-scope items

| Item | Status |
|---|---|
| Multi-party approval to pause | Out of scope — use a governed `admin` |
| Pause on non-risk-bearing reads | Out of scope — reads are always safe |
| Fee-on-transfer or rebasing tokens | Out of scope — unsupported by design |
| Automatic storage cleanup on expiry | Out of scope — `PausedAt` and `Paused` are not rewritten by auto-expiry |
