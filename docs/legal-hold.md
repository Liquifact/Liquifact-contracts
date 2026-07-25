# Legal Hold — Clear-Delay Bound and Rationale

This document describes the `legal_hold_clear_delay` parameter, its accepted range, the
failure it prevents, and how the hold interacts with settlement. It is the authoritative
reference for operators and integrators configuring a compliance hold on a LiquiFact escrow.

For the operational security model, enforcement semantics, and gated entrypoints, see
[`docs/escrow-legal-hold.md`](escrow-legal-hold.md). For the design rationale, see
[ADR-004](adr/ADR-004-legal-hold.md).

---

## Table of contents

- [Parameter: `legal_hold_clear_delay`](#parameter-legal_hold_clear_delay)
- [Accepted range](#accepted-range)
- [The failure it prevents](#the-failure-it-prevents)
- [How the hold interacts with settlement](#how-the-hold-interacts-with-settlement)
- [Worked example: two-phase clear with a 7-day delay](#worked-example-two-phase-clear-with-a-7-day-delay)
- [Cross-references](#cross-references)

---

## Parameter: `legal_hold_clear_delay`

The `legal_hold_clear_delay` is an **optional** `u64` parameter passed to
[`LiquifactEscrow::init`](../escrow/src/lib.rs). It configures the minimum number of
**ledger seconds** that must elapse between an admin signalling intent to lift a compliance
hold ([`request_clear_legal_hold`](../escrow/src/lib.rs)) and the hold actually being cleared
([`set_legal_hold(env, false)`](../escrow/src/lib.rs) or
[`clear_legal_hold`](../escrow/src/lib.rs)).

### Storage

At `init`, the delay value is stored under [`DataKey::LegalHoldClearDelay`](../escrow/src/lib.rs)
(instance storage):

```rust
// escrow/src/lib.rs — init (abridged)
let delay = legal_hold_clear_delay.unwrap_or(0);
if delay > 0 {
    env.storage()
        .instance()
        .set(&DataKey::LegalHoldClearDelay, &delay);
}
```

When the parameter is `None` or `0`, the key is **not written** and the default delay of `0`
applies. This preserves backward compatibility: an escrow deployed without a clear delay
allows the admin to toggle the hold in a single step.

### Read view

The stored delay is exposed via [`get_legal_hold_clear_delay`](../escrow/src/lib.rs), which
returns `0` when the key is absent:

```rust
pub fn get_legal_hold_clear_delay(env: Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::LegalHoldClearDelay)
        .unwrap_or(0)
}
```

---

## Accepted range

| Bound | Value | Enforced at |
|---|---|---|
| **Lower** | `0` | `init` — `0` and `None` are both accepted and mean "no mandatory delay" |
| **Upper** | `u64::MAX` (18 446 744 073 709 551 615) | `init` — no explicit cap is enforced |
| **Effective upper** | `u64::MAX − now` | `request_clear_legal_hold` — the computation `now + delay` must not overflow |

### Lower bound: `0`

When the delay is `0` (or omitted), the admin can clear the hold in a single step —
`set_legal_hold(false)` skips the delay check entirely. Deployments that want the two-step
**audit trail** without a mandatory waiting period can set `delay = 0` and still call
`request_clear_legal_hold` to emit the `LegalHoldClearRequested` event while clearing
immediately.

### Upper bound: overflow guard

There is **no explicit constant** (e.g. `MAX_LEGAL_HOLD_CLEAR_DELAY_SECS`) capping the delay
at `init`. The practical upper bound is the `u64` overflow check performed at
`request_clear_legal_hold`:

```rust
// escrow/src/lib.rs — request_clear_legal_hold (abridged)
let now = env.ledger().timestamp();
let delay = Self::get_legal_hold_clear_delay(env.clone());
let clearable_at = if delay == 0 {
    now
} else {
    now.checked_add(delay)
        .unwrap_or_else(|| fail(&env, EscrowError::LegalHoldClearDelayOverflow))
};
```

If `now + delay` overflows a `u64`, the request is rejected with
[`EscrowError::LegalHoldClearDelayOverflow`](../escrow/src/lib.rs) (error code **152**).

**Rationale for no explicit cap at init:** The `u64` type constrains the delay to a
fixed-width integer. Adding an arbitrary constant (e.g. 1 year) would bake in a policy
decision that belongs to the deployer. The overflow guard is the minimal, mathematical
bound: a delay that overflows `now + delay` is so large that the hold can never be cleared
(because no ledger timestamp can satisfy `now >= clearable_at`). The overflow guard catches
this pathological case at request time rather than init time, keeping `init` simple while
still preventing an unclearable hold.

### Reasonable operational range

In practice, deployers should choose a delay measured in **hours to weeks**, not decades:

| Use case | Suggested delay |
|---|---|
| Cooling-off window for compliance review | 24–72 hours (86 400–259 200 seconds) |
| Multi-signatory approval window | 7 days (604 800 seconds) |
| Regulatory notification period | 14–30 days (1 209 600–2 592 000 seconds) |

Choosing a delay on the order of years or decades makes the hold effectively permanent
and defeats its purpose as a compliance tool. Indexers should monitor the
`LegalHoldClearRequested` event and alert if the delay exceeds operational policy.

---

## The failure it prevents

The `legal_hold_clear_delay` mechanism prevents a single category of failure: a **permanent,
unclearable compliance hold** caused by a misconfigured delay.

### Scenario: overflow-level delay

If an init call passes a delay value where `now + delay` overflows `u64`, the hold can be
**set** (because `set_legal_hold(true)` does not check the delay), but it can **never be
cleared** because `request_clear_legal_hold` will always fail with
`LegalHoldClearDelayOverflow`. The overflow guard rejects the request before the
`LegalHoldClearableAt` key is written, so the admin can retry with a corrected delay.

### Scenario: decades-long delay

A delay of, say, 100 years (≈ 3 155 760 000 seconds) does **not** overflow on a ledger
with a current timestamp in the 2020s, but it makes the hold practically permanent. An admin
who sets the hold must wait 100 years of ledger time before clearing it. If the admin key is
then lost, funds are frozen with no realistic recovery timeline.

This is the more dangerous failure mode because the overflow guard does **not** catch it.
Deployers must treat the delay as a **policy parameter** and enforce a reasonable upper
bound off-chain (e.g. via a DAO vote or multisig review of the `init` arguments).

### Recovery

In both cases, if a hold becomes effectively permanent and the current admin is still
available, recovery requires admin handover to a new contract instance with a corrected
delay. If the admin key is lost, see
[`docs/escrow-legal-hold.md` §"Failure mode: hold + lost admin key"](escrow-legal-hold.md#failure-mode-hold--lost-admin-key).

---

## How the hold interacts with settlement

The legal hold and settlement are independent mechanisms that **compose** at the
`LiquifactEscrow::settle` entrypoint. The interaction follows a strict precedence order.

### Guard ordering at `settle`

```text
settle()
 ├─ guard_not_legal_hold(env, LegalHoldBlocksSettlement)   ← (1)
 ├─ guard_status_eq(env, status, 1, SettlementNotFunded)   ← (2)
 ├─ maturity check (maturity == 0 || now >= maturity)      ← (3)
 └─ … storage writes, event emission
```

1. **Legal hold fires first.** Even if the escrow is funded and matured, a hold blocks
   settlement with `EscrowError::LegalHoldBlocksSettlement` (120).

2. **Status check.** If the hold is not active, the escrow must be in `status == 1` (funded).

3. **Maturity.** If a maturity timestamp is configured (`maturity > 0`), settlement is
   gated on `now >= maturity`.

### Settlement-readiness view

The [`get_settlement_readiness`](../escrow/src/lib.rs) view bundles these checks into a
single host invocation via [`SettlementReadiness`](../escrow/src/lib.rs):

| Field | Meaning |
|---|---|
| `is_settleable` | `true` when funded, matured, and **not** on legal hold |
| `legal_hold_active` | The current value of `DataKey::LegalHold` |
| `maturity_reached` | `true` when `maturity == 0` or `now >= maturity` |
| `ready_now` | `true` exactly when `settle` would succeed on the current ledger |

### What the hold does NOT affect

The legal hold **does not** block:

- `settle` once cleared (it is a temporary gate, not a permanent disablement)
- Read views (`get_escrow`, `get_settlement_readiness`, etc.)
- Admin handover (`propose_admin`, `accept_admin`) — intentional: recovery lever
- Metadata operations (`record_sme_collateral_commitment`, `bind_primary_attestation_hash`)

### Pause vs. hold

The contract has two independent freeze mechanisms:

| | Legal hold (`DataKey::LegalHold`) | Operational pause (`DataKey::Paused`) |
|---|---|---|
| **Purpose** | Compliance / regulatory freeze | Incident response (e.g. suspected token bug) |
| **Clear mechanism** | Two-phase with configurable delay | Single-call toggle, no delay |
| **Gated entrypoints** | `fund`, `fund_with_commitment`, `settle`, `withdraw`, `claim_investor_payout`, `sweep_terminal_dust`, `cancel_funding`, `rotate_beneficiary`, `partial_settle`, `unfund` | `fund`, `settle`, `withdraw`, `claim_investor_payout` |
| **Blocks admin handover?** | No | No |

Both flags are independent — an escrow can be paused and held simultaneously. Each gate
fires independently at the gated entrypoints.

---

## Worked example: two-phase clear with a 7-day delay

### Setup

An escrow is deployed with `legal_hold_clear_delay = Some(604_800)` (7 days in seconds).

### Timeline

```
t=0          Admin calls set_legal_hold(true)
               → DataKey::LegalHold = true
               → LegalHoldChanged { active: 1 } emitted
               → All risk-bearing entrypoints are now blocked

t=1 hour     Admin calls request_clear_legal_hold()
               → computes clearable_at = now + 604800
               → DataKey::LegalHoldClearableAt = clearable_at
               → LegalHoldClearRequested { clearable_at } emitted

t=1 hour+1s  Admin calls set_legal_hold(false)
               → reads clearable_at, compares now < clearable_at
               → REJECTED: EscrowError::LegalHoldClearNotReady (151)

t=7 days     Admin calls set_legal_hold(false)  (or clear_legal_hold())
               → now >= clearable_at → accepted
               → DataKey::LegalHold = false
               → DataKey::LegalHoldClearableAt removed
               → LegalHoldChanged { active: 0 } emitted

t=7 days+1s  Admin (or SME) calls settle()
               → guard_not_legal_hold passes (hold is false)
               → status == 1 (funded) → passes
               → maturity check passes
               → EscrowSettled emitted
```

### Key invariants

- The clear request must **precede** the clear call — you cannot skip the request step
  when `delay > 0`.
- The boundary check is **inclusive**: `now >= clearable_at`. At exactly `clearable_at`,
  the clear is accepted.
- After a successful clear, `LegalHoldClearableAt` is removed from storage so a subsequent
  hold-set + hold-clear cycle starts fresh with a new request.
- The two-step sequence is **mandatory** only when `delay > 0`. When `delay == 0`,
  `set_legal_hold(false)` skips the delay check and clears immediately.

### Typed errors summary

| Condition | Error | Code |
|---|---|---|
| `set_legal_hold(false)` without prior `request_clear_legal_hold` (delay > 0) | `LegalHoldClearRequestMissing` | 150 |
| `now < clearable_at` | `LegalHoldClearNotReady` | 151 |
| `now + delay` overflows `u64` at request time | `LegalHoldClearDelayOverflow` | 152 |

---

## Cross-references

| Document | Covers |
|---|---|
| [`docs/escrow-legal-hold.md`](escrow-legal-hold.md) | Operational security model, enforcement semantics, gated entrypoints, governance expectations, failure mode & recovery |
| [`docs/adr/ADR-004-legal-hold.md`](adr/ADR-004-legal-hold.md) | Design rationale and rejected alternatives |
| [`docs/adr/ADR-002-auth-boundaries.md`](adr/ADR-002-auth-boundaries.md) | Authorization guard ordering (legal hold check before `require_auth`) |
| [`docs/escrow-ledger-time.md`](escrow-ledger-time.md) | Ledger time trust model (timestamps are validator-observed, not wall-clock) |
| [`docs/OPERATOR_RUNBOOK.md`](OPERATOR_RUNBOOK.md) | Pre-flight checklists, admin key hygiene, upgrade coordination |
| [`docs/EVENT_SCHEMA.md`](EVENT_SCHEMA.md) | Event payloads (`LegalHoldChanged`, `LegalHoldClearRequested`) |
| [`escrow/src/lib.rs`](../escrow/src/lib.rs) | Source of truth for all entrypoints and typed errors |
| [`escrow/src/tests/legal_hold.rs`](../escrow/src/tests/legal_hold.rs) | Comprehensive test matrix for legal-hold timing windows |

### Relevant entrypoints

| Entrypoint | Role | Auth |
|---|---|---|
| `init` | Sets `legal_hold_clear_delay` (immutable after init) | `admin` |
| `set_legal_hold(active)` | Activates or (after delay) clears the hold | `admin` |
| `request_clear_legal_hold` | Schedules `clearable_at = now + delay` | `admin` |
| `clear_legal_hold` | Convenience alias for `set_legal_hold(false)` | `admin` |
| `clear_legal_hold_after_delay` | Clears hold after delay has elapsed (shorthand) | `admin` |
| `cancel_clear_legal_hold` | Cancels a pending clear request | `admin` |
| `get_legal_hold` | Reads current hold state | — (read-only) |
| `get_legal_hold_clear_delay` | Reads configured delay | — (read-only) |
| `get_legal_hold_clearable_at` | Reads pending `clearable_at` (if any) | — (read-only) |
| `get_settlement_readiness` | Bundles legal-hold, funded, and maturity state | — (read-only) |
