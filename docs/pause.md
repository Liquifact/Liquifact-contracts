# Escrow Operational Pause — Model & Invariants

`DataKey::Paused` is a boolean circuit-breaker stored in contract instance
storage. When `true` it blocks the four risk-bearing entrypoints that move or
commit capital. This document describes the data model, gated entrypoints,
invariants, and the enforcement model — and explicitly contrasts the pause with
the legal hold.

---

## Data model

| Key | Storage | Type | Default (absent) | Set by | Mutable |
|-----|---------|------|------------------|--------|---------|
| `DataKey::Paused` | instance | `bool` | `false` (not paused) | `set_paused` | Yes — toggled by admin |

The key is read by the private helper `paused_active(&env)`:

```rust
fn paused_active(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}
```

The public read entrypoint `is_paused(env)` delegates to this helper. Both are
read-only and carry no `require_auth`.

---

## Enforcement model

```
set_paused(active: bool)
    └─ escrow.admin.require_auth()          ← Soroban auth, cannot be spoofed
    └─ storage().instance().set(DataKey::Paused, active)
    └─ emits PausedChanged { active: 1 | 0 }
```

Key properties:

- **Single role.** Only `InvoiceEscrow::admin` may set or clear the pause.
  There is no secondary role or emergency bypass.
- **Single-call toggle.** Unlike the legal hold, there is **no** two-phase
  clear delay or request-before-clear protocol. One authorized call flips the
  flag on or off. This is intentional: the pause is designed for fast incident
  response (e.g. a suspected token bug, an oracle anomaly, or unexpected
  on-chain behaviour) where the operator needs to halt risk-bearing flows
  immediately.
- **Default off.** `paused_active` returns `false` when `DataKey::Paused` has
  never been written, so newly deployed escrows are not accidentally frozen.
- **Idempotent.** Calling `set_paused(true)` when already `true` (or `false`
  when already `false`) is a no-op for state but still requires admin auth and
  emits a `PausedChanged` event.
- **Persistent across state transitions.** The pause flag is stored
  independently of `InvoiceEscrow::status`. A pause set while the escrow is
  `open` (0) remains active when the escrow becomes `funded` (1), `settled`
  (2), or `withdrawn` (3). It does **not** block status transitions themselves
  — only the four gated entrypoints listed below.
- **No TTL expiry.** The pause has no programmatic timeout. The flag stays
  `true` until an admin explicitly clears it.

---

## Gated entrypoints

| Entrypoint | Precondition check | Error code | Error name |
|---|---|---|---|
| `fund` | `!paused_active(&env)` | `210` | `PausedBlocksFunding` |
| `fund_with_commitment` | via `fund_impl` → `!paused_active(&env)` | `210` | `PausedBlocksFunding` |
| `fund_batch` | via `fund_impl` per entry → `!paused_active(&env)` | `210` | `PausedBlocksFunding` |
| `settle` | `!paused_active(&env)` | `211` | `PausedBlocksSettlement` |
| `withdraw` | `!paused_active(&env)` | `212` | `PausedBlocksWithdrawal` |
| `claim_investor_payout` | `!paused_active(&env)` | `213` | `PausedBlocksInvestorClaims` |

Each gated entrypoint checks the pause **before** `require_auth` as a read-only
precondition, consistent with the canonical guard ordering (ADR-002 / security
checklist §6). The pause check fires before any storage write or token
transfer.

The `fund_batch` entrypoint delegates to `fund_impl` per entry. The
per-entry validation (positivity, min-contribution floor) runs first; then each
`fund_impl` call checks the pause independently. Because the pause flag cannot
change during a single host-function invocation, the check is effectively
constant across the batch — a batch either fully succeeds or the first
pause-check failure stops it.

---

## Entrypoints NOT gated by the pause

All other entrypoints are **not** gated by `DataKey::Paused`. This is
intentional: the pause is a surgical circuit-breaker for the four
capital-moving operations, not a global freeze.

| Category | Entrypoints | Rationale |
|---|---|---|
| Pause control | `set_paused` | Must be callable to clear the pause |
| Read accessors | `is_paused`, all `get_*` functions | Reads are always safe |
| Admin governance | `propose_admin`, `accept_admin`, `cancel_pending_admin` | Admin rotation must work during a pause |
| Legal / compliance | `set_legal_hold`, `clear_legal_hold`, `request_clear_legal_hold` | Legal hold is orthogonal; must be independently manageable |
| Allowlist | `set_allowlist_active`, `set_investor_allowlisted`, `set_investors_allowlisted` | Admin housekeeping |
| Attestations | `bind_primary_attestation_hash`, `append_attestation_digest`, `revoke_attestation_digest`, `unrevoke_attestation_digest` | Metadata-only operations |
| Config updates | `update_maturity`, `update_maturity_max_horizon`, `raise_maturity_max_horizon`, `update_funding_target`, `lower_max_unique_investors`, `raise_max_unique_investors`, `lower_min_contribution_floor`, `raise_max_per_investor`, `extend_funding_deadline` | Admin config; not capital-moving |
| SME metadata | `record_sme_collateral_commitment`, `clear_sme_collateral_commitment`, `rotate_beneficiary` | Metadata or role update only |
| Early transition | `partial_settle` | Only gated by legal hold; pause left open so admin/SME can transition early if needed |
| Cancellation | `cancel_funding`, `refund`, `refund_batch` | Refunds return capital to investors; blocking them would trap funds |
| Dust sweep | `sweep_terminal_dust` | Capped at `MAX_DUST_SWEEP_AMOUNT`; gated by legal hold only |
| Lifecycle | `init`, `upgrade`, `migrate` | Init is one-time; upgrade/migrate are admin operations |

---

## Orthogonality to legal hold

`DataKey::Paused` and `DataKey::LegalHold` are **completely independent**:

| Property | Pause | Legal hold |
|---|---|---|
| Semantics | Operational circuit-breaker | Compliance / legal freeze |
| Clear delay | None — single call | Optional two-phase with configurable delay |
| Auth | Admin only | Admin only |
| Storage key | `DataKey::Paused` | `DataKey::LegalHold` |
| Event | `PausedChanged` | `LegalHoldChanged` |
| Blocks `fund` | Yes (`PausedBlocksFunding`) | Yes (`LegalHoldBlocksFunding`) |
| Blocks `settle` | Yes (`PausedBlocksSettlement`) | Yes (`LegalHoldBlocksSettlement`) |
| Blocks `withdraw` | Yes (`PausedBlocksWithdrawal`) | Yes (`LegalHoldBlocksWithdrawal`) |
| Blocks `claim_investor_payout` | Yes (`PausedBlocksInvestorClaims`) | Yes (`LegalHoldBlocksInvestorClaims`) |
| Blocks `sweep_terminal_dust` | No | Yes (`LegalHoldBlocksTreasuryDustSweep`) |
| Blocks `cancel_funding` | No | Yes (`LegalHoldBlocksCancelFunding`) |
| Blocks `partial_settle` | No | Yes (`LegalHoldBlocksPartialSettle`) |

**Either flag independently blocks** the four shared gated entrypoints. There
is no precedence or override: if **either** `Paused` or `LegalHold` is `true`,
a `fund` / `settle` / `withdraw` / `claim_investor_payout` call will fail.
Clearing one flag does not clear the other; `set_paused` never reads or writes
any legal-hold key, and the legal-hold entrypoints never touch `DataKey::Paused`.

The guard ordering in gated entrypoints is pause-first, then legal hold, then
`require_auth`:

```text
1. ensure(!paused_active)   → PausedBlocks*        (read-only, instance storage)
2. ensure(!legal_hold_active) → LegalHoldBlocks*    (read-only, instance storage)
3. require_auth()                                   (Soroban auth boundary)
4. Storage writes & token transfers                 (mutation begins)
```

Both checks are read-only preconditions that execute before any side effect.
This is enforced at every call site (see `escrow/src/lib.rs`: `fund_impl`,
`settle`, `withdraw`, `claim_investor_payout`).

---

## Invariants

### I-P1: Pause is default-off

`DataKey::Paused` is absent after `init`. `paused_active` returns `false` for
the key's absence. A freshly deployed escrow is never paused.

**Verification:** `paused_active` uses `.unwrap_or(false)`. `init` does not
write `DataKey::Paused`. Confirmed by absence of any
`.set(&DataKey::Paused, …)` call in the `init` body.

### I-P2: Pause gate fires before any mutation

In every gated entrypoint, the `ensure(!Self::paused_active(&env), …)` call
appears **before** the first storage write and **before** the first token
transfer. There is no code path where the pause check can be bypassed or
reordered.

**Verification:** grep for `PausedBlocksFunding`, `PausedBlocksSettlement`,
`PausedBlocksWithdrawal`, and `PausedBlocksInvestorClaims` in `escrow/src/lib.rs`.
Each appears exactly once in a guard block. No conditional or fallback path
skips the check.

### I-P3: Pause is orthogonal to legal hold

`set_paused` reads and writes only `DataKey::Paused`. It never reads or writes
`DataKey::LegalHold`, `DataKey::LegalHoldClearableAt`, or
`DataKey::LegalHoldClearDelay`. Conversely, `set_legal_hold`,
`clear_legal_hold`, and `request_clear_legal_hold` never read or write
`DataKey::Paused`. The two flags share no storage keys and no code paths.

**Verification:** cross-grep for `DataKey::Paused` in legal-hold entrypoints
and `DataKey::LegalHold` in `set_paused` — zero matches.

### I-P4: Pause is admin-only

`set_paused` calls `Self::load_escrow_require_admin(&env)`, which reads
`DataKey::Escrow` and calls `escrow.admin.require_auth()`. No other
entrypoint writes `DataKey::Paused`. A non-admin caller cannot toggle the
pause.

**Verification:** `load_escrow_require_admin` is the only code path that reads
`DataKey::Escrow` and calls `admin.require_auth()` in a single step. `set_paused`
is the only caller of that pattern that writes `DataKey::Paused`.

### I-P5: Pause is idempotent

Calling `set_paused(true)` when `DataKey::Paused` is already `true` overwrites
`true` with `true` — a storage no-op that still emits `PausedChanged { active:
1 }`. Same for `set_paused(false)` when already `false`. No error is raised.

**Verification:** `set_paused` contains no guard that reads the current value
before writing. It unconditionally `.set(&DataKey::Paused, &active)`.

### I-P6: Pause persists across status transitions

`DataKey::Paused` is stored independently of `InvoiceEscrow::status`. Writing
`DataKey::Escrow` (which carries `status`) never reads or clears
`DataKey::Paused`. A pause set at `status == 0` will still block gated
entrypoints at `status == 1`, `2`, or `3`.

**Verification:** no entrypoint that writes `DataKey::Escrow` (e.g. `settle`,
`withdraw`, `fund_impl`) reads or removes `DataKey::Paused`.

### I-P7: Pause has no TTL or expiry

There is no stored timestamp, no `require_auth`-bypassed expiry path, and no
`set_paused(false)` call triggered by any entrypoint other than `set_paused`
itself. The pause stays active until an admin explicitly clears it.

**Verification:** `DataKey::Paused` is only written by `set_paused`. No
`env.ledger().timestamp()` comparison exists on the pause path.

---

## Worked example

Consider an escrow with these parameters:

| Field | Value |
|---|---|
| `invoice_id` | `"INV-0042"` |
| `amount` | `100_000_000` (100M base units) |
| `funding_target` | `100_000_000` |
| `yield_bps` | `500` (5%) |
| `maturity` | `1_800_000_000` (ledger timestamp) |

### Step-by-step lifecycle with a pause

1. **Init.** Admin calls `init(…)`. Escrow created with `status = 0` (open).
   `DataKey::Paused` is absent → `paused_active()` returns `false`.

2. **Investors fund.** Alice calls `fund(50_000_000)`, Bob calls
   `fund(50_000_000)`. Both succeed — the pause is clear. `funded_amount` reaches
   `100_000_000`, `status` transitions to `1` (funded), and
   `FundingCloseSnapshot` is written.

3. **Incident: admin pauses.** A suspected bug is reported in the funding token
   contract. Admin calls `set_paused(true)`:
   - `admin.require_auth()` succeeds.
   - `DataKey::Paused` is set to `true`.
   - `PausedChanged { name: "paused", invoice_id: "INV-0042", active: 1 }` is emitted.

4. **SME attempts to settle — blocked.** SME calls `settle()`:
   - `paused_active()` returns `true`.
   - `ensure(!true, PausedBlocksSettlement)` → panics with `PausedBlocksSettlement` (code 211).
   - The call reverts before `require_auth()` or any storage write.

5. **Investor attempts to claim — blocked.** Charlie (who funded earlier) calls
   `claim_investor_payout(…)`:
   - `paused_active()` returns `true`.
   - Panics with `PausedBlocksInvestorClaims` (code 213).

6. **Admin governance still works.** Admin calls `propose_admin(new_admin)` and
   `new_admin` calls `accept_admin()`. Both succeed — neither is gated by the
   pause. The new admin inherits the pause state.

7. **Incident resolved — admin clears pause.** New admin calls
   `set_paused(false)`:
   - `admin.require_auth()` succeeds.
   - `DataKey::Paused` is set to `false`.
   - `PausedChanged { name: "paused", invoice_id: "INV-0042", active: 0 }` is emitted.

8. **Normal flow resumes.** SME calls `settle()` → succeeds (`status = 2`).
   Charlie calls `claim_investor_payout(…)` → succeeds (claim-lock permitting).

### Simultaneous pause + legal hold

If **both** `Paused` and `LegalHold` are `true`:

1. Charlie calls `claim_investor_payout(…)`.
2. Guard 1: `paused_active()` → `true` → panics with `PausedBlocksInvestorClaims`.
3. The legal-hold check is never reached. The call fails at the first blocker.

If the pause is cleared but the legal hold remains:

1. Charlie calls `claim_investor_payout(…)`.
2. Guard 1: `paused_active()` → `false` → passes.
3. Guard 2: `legal_hold_active()` → `true` → panics with `LegalHoldBlocksInvestorClaims`.
4. Either flag independently blocks the call.

Clearing **both** flags (two separate admin calls: `set_paused(false)` and
`set_legal_hold(false)`) restores full functionality.

---

## Event schema

### `PausedChanged`

Emitted by `set_paused` whenever the operational pause flag is written.

| Field | Type | Description |
|---|---|---|
| `name` | `Symbol` | Hardcoded `"paused"` |
| `invoice_id` | `Symbol` | Escrow invoice identifier |
| `active` | `u32` | `1` = pause enabled, `0` = cleared |

Independent of `LegalHoldChanged`: this signals the lightweight
incident-response switch, not the compliance hold.

---

## Governance expectations

This contract does **not** embed a timelock, council multisig, or on-chain
governance vote for pause operations. Production deployments must treat `admin`
as a governed address:

- **Multisig wallet** (e.g. Stellar multisig account with M-of-N signers) so
  no single key can pause funds indefinitely.
- **Protocol DAO contract** that requires an on-chain vote before calling
  `set_paused`.
- **Off-chain playbook** covering: who may initiate a pause, required evidence
  or incident-severity threshold, maximum pause duration, escalation path if
  the admin key is lost or compromised, and emergency recovery via
  `propose_admin` + `accept_admin` with governance approval.

Without one of the above, a single compromised admin key can pause all
risk-bearing entrypoints with no on-chain recourse.

---

## Failure mode: pause + lost admin key

When `DataKey::Paused` is `true` and the **current** admin signing key is lost
or destroyed:

- `fund`, `settle`, `withdraw`, and `claim_investor_payout` remain blocked.
- `set_paused(false)` requires authorization from whoever is stored as
  `InvoiceEscrow::admin` — the lost key cannot satisfy this.
- There is **no** timelock expiry or protocol-level bypass.

**On-chain recovery (only path):**

1. Governance executes `propose_admin` using a **still-available** current-admin
   authorization (e.g. remaining multisig signers or DAO vote output). This
   entrypoint is **not** blocked by the pause.
2. The proposed successor executes `accept_admin` with its own authorization.
   This promotes the successor into `InvoiceEscrow::admin`.
3. The **new** admin calls `set_paused(false)`.
4. Risk-bearing flows resume.

**Invariant:** a pause is always clearable by the current admin; recovery
requires controlling admin authority. If governance cannot produce a valid
current-admin signature for `propose_admin`, funds remain blocked until
off-chain recovery restores signing capability.

---

## Cross-references

- [Escrow Legal Hold](escrow-legal-hold.md) — the compliance gate that is
  orthogonal to the pause.
- [Security Checklist §5.10](escrow-security-checklist.md) — operational pause
  risks and governance requirements.
- [Security Checklist §6](escrow-security-checklist.md) — canonical guard
  ordering (ADR-002) enforced at every gated call site.
- [ADR-002](adr/ADR-002-auth-boundaries.md) — auth boundary design and
  `require_auth` placement.
- [Operator Runbook](OPERATOR_RUNBOOK.md) — incident-response playbook and
  admin-key hygiene.
- [Escrow Error Messages](escrow-error-messages.md) — complete error code
  reference including `PausedBlocks*` codes.

---

## Source references

All line numbers reference `escrow/src/lib.rs` at schema version 6.

| Item | Location |
|---|---|
| `DataKey::Paused` variant | `DataKey` enum, doc comment: "lightweight operational pause" |
| `paused_active` helper | `fn paused_active(env: &Env) -> bool` |
| `is_paused` public read | `pub fn is_paused(env: Env) -> bool` |
| `set_paused` entrypoint | `pub fn set_paused(env: Env, active: bool)` |
| `PausedChanged` event | `struct PausedChanged` |
| `PausedBlocksFunding` guard | `fund_impl`, before `require_auth` |
| `PausedBlocksSettlement` guard | `settle`, before `require_auth` |
| `PausedBlocksWithdrawal` guard | `withdraw`, before `require_auth` |
| `PausedBlocksInvestorClaims` guard | `claim_investor_payout`, before `require_auth` |
| Error codes | `EscrowError` enum: 210–213 (duplicates at 177–180) |
