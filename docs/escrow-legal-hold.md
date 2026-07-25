# Escrow Legal Hold — Security Reference

`DataKey::LegalHold` is a boolean compliance gate stored in contract instance
storage. When `true` it blocks every risk-bearing state transition. This
document describes the gated operations, the enforcement model, governance
expectations, and explicit out-of-scope items.

---

## Gated operations

| Function | Panic message when hold is active |
|---|---|
| `fund` | `Legal hold blocks new funding while active` |
| `fund_with_commitment` | `Legal hold blocks new funding while active` |
| `settle` | `Legal hold blocks settlement finalization` |
| `withdraw` | `Legal hold blocks SME withdrawal` |
| `claim_investor_payout` | `Legal hold blocks investor claims` |
| `sweep_terminal_dust` | `Legal hold blocks treasury dust sweep` |

All six checks call the private `legal_hold_active(&env)` helper, which reads
`DataKey::LegalHold` from instance storage and defaults to `false` when the key
is absent. The check is the **first** assertion in each function body, so no
partial state mutation can occur before the gate fires.

Operations that are **not** gated (read-only or metadata-only):

- `get_*` accessors
- `record_sme_collateral_commitment` (metadata record, no token movement)
- `bind_primary_attestation_hash` / `append_attestation_digest`
- `update_maturity`, `update_funding_target`, `propose_admin`, `accept_admin`, `migrate`

---

## Enforcement model

```
set_legal_hold(active: bool)
    └─ escrow.admin.require_auth()   ← Soroban auth check, cannot be spoofed
    └─ storage().instance().set(DataKey::LegalHold, active)
    └─ emits LegalHoldChanged { active: 1 | 0 }

request_clear_legal_hold()
    └─ escrow.admin.require_auth()
    └─ storage().instance().set(DataKey::LegalHoldClearableAt, now + delay)
    └─ emits LegalHoldClearRequested { clearable_at }

clear_legal_hold()
    └─ delegates to set_legal_hold(false)   ← same auth path, no shortcut
```

Key properties:

- **Single role.** Only `InvoiceEscrow::admin` can set or clear the hold. There
  is no secondary "compliance officer" role or emergency bypass in this version.
- **Atomic.** The hold is read and checked before any storage mutation in each
  gated function. There is no window between the check and the effect.
- **Persistent across state transitions.** The hold is stored independently of
  `InvoiceEscrow::status`. A hold set while the escrow is open remains active
  after it becomes funded; a hold set after settlement blocks investor claims.
- **Controlled-clear semantics.** When a hold-clear delay is configured,
  `request_clear_legal_hold` must be called first and `set_legal_hold(false)` is
  only allowed once the ledger timestamp has reached the returned boundary.
- **Idempotent.** Calling `set_legal_hold(true)` when already `true` (or
  `false` when already `false`) is a no-op for state but still requires admin
  auth and emits an event.
- **Default off.** `legal_hold_active` returns `false` when the key has never
  been written, so newly deployed escrows are not accidentally frozen.

---

## Timing window: delay semantics and typed errors

When `legal_hold_clear_delay` is set to a non-zero value at `init`, the
two-step clear sequence becomes mandatory:

```
Step 1  request_clear_legal_hold()
            → writes DataKey::LegalHoldClearableAt = now + delay
            → emits LegalHoldClearRequested { clearable_at }

Step 2  set_legal_hold(false)   (or clear_legal_hold())
            → requires now >= clearable_at
            → clears DataKey::LegalHoldClearableAt on success
```

**Typed errors enforced at Step 2:**

| Condition | Error code |
|---|---|
| `set_legal_hold(false)` without a prior `request_clear_legal_hold` | `LegalHoldClearRequestMissing` (150) |
| `now < clearable_at` — delay has not elapsed | `LegalHoldClearNotReady` (151) |
| `now + delay` overflows a `u64` in `request_clear_legal_hold` | `LegalHoldClearDelayOverflow` (152) |

**Boundary condition:** the check is `now >= clearable_at` (inclusive). At
exactly `clearable_at`, the clear is accepted. One ledger-second before it, the
clear is rejected with `LegalHoldClearNotReady`.

**Zero-delay shortcut:** when `legal_hold_clear_delay = 0`, calling
`request_clear_legal_hold` writes `clearable_at = now`, so the two-step
sequence succeeds immediately without any ledger advancement. This is useful
for deployments that want the two-step audit trail but no mandatory waiting
period.

**If no delay is configured** (the default), `set_legal_hold(false)` skips the
delay check entirely and clears the hold in one step. The
`LegalHoldClearRequestMissing` and `LegalHoldClearNotReady` errors are never
emitted in that case.

---

## Governance expectations

This contract does **not** embed a timelock, council multisig, or on-chain
governance vote for hold operations. Production deployments must treat `admin`
as a governed address:

- **Multisig wallet** (e.g. Stellar multisig account with M-of-N signers) so
  no single key can freeze funds indefinitely.
- **Protocol DAO contract** that requires an on-chain vote before calling
  `set_legal_hold`.
- **Off-chain playbook** covering: who may initiate a hold, required evidence,
  maximum hold duration, escalation path if the admin key is lost or
  compromised, and emergency recovery via `propose_admin` + `accept_admin`
  with governance approval.
- **On-chain cooling-off window** when configured: `request_clear_legal_hold`
  schedules a ledger-time boundary before `set_legal_hold(false)` may unfreeze.
  When a non-zero delay is configured, `set_legal_hold(false)` will fail until
  the clear request has been made and the ledger time has reached the stored
  `LegalHoldClearableAt` value.

Without one of the above, a single compromised admin key can freeze all
investor funds with no on-chain recourse.

### Required deployment posture

| Requirement | Rationale |
|---|---|
| Governed `admin` at `init` (multisig or DAO contract) | Single EOA admin + hold + key loss = indefinite fund lock |
| Documented recovery playbook | Operators must know how to execute `propose_admin` and `accept_admin` under hold |
| Testnet rotation drill before mainnet | Confirms new admin can `clear_legal_hold` after rotation |
| Indexer monitoring of `LegalHoldChanged` | Detect holds that exceed policy duration |

See [ADR-004](adr/ADR-004-legal-hold.md) and [`OPERATOR_RUNBOOK.md`](OPERATOR_RUNBOOK.md)
§7–§8 for upgrade-window coordination and admin key hygiene.

---

## Failure mode: hold + lost admin key

When `DataKey::LegalHold` is `true` and the **current** admin signing key is
lost or destroyed:

- `settle`, `withdraw`, `claim_investor_payout`, `fund`, and
  `sweep_terminal_dust` remain blocked.
- `clear_legal_hold` requires authorization from whoever is stored as
  `InvoiceEscrow::admin` — the lost key cannot satisfy this.
- There is **no** timelock expiry, guardian, or protocol-level bypass in this
  contract version.

**On-chain recovery (only path):**

1. Governance executes [`propose_admin`](../../escrow/src/lib.rs) using a
   **still-available** current-admin authorization (e.g. remaining multisig
   signers or DAO vote output). This entrypoint is **not** blocked by the hold.
2. The proposed successor executes [`accept_admin`](../../escrow/src/lib.rs)
   with its own authorization. This promotes the successor into
   `InvoiceEscrow::admin` and clears `DataKey::PendingAdmin`.
3. The **new** admin calls `clear_legal_hold` (or `set_legal_hold(false)`).
4. Risk-bearing flows resume (`settle`, `withdraw`, etc.).

**Invariant:** a hold is always clearable by the current admin; recovery
requires controlling admin authority — not merely controlling the SME or
treasury roles.

If governance cannot produce a valid current-admin signature for
`propose_admin`, funds remain locked until off-chain legal or operational
recovery restores signing capability. If a proposal was created with the wrong
address, the current admin can overwrite it by calling `propose_admin` again.
This is why single-signer production admins are prohibited.

---

## Admin rotation during a hold

`propose_admin` and `accept_admin` are not gated by the hold. This is
intentional: if the current admin is compromised or unresponsive, governance
must be able to rotate the admin key even while a hold is active. The handover
still requires both the current admin and successor to authorize. After
acceptance the new admin inherits the hold state and must explicitly call
`clear_legal_hold` to unfreeze.

---

## Assumptions and out-of-scope items

| Item | Status |
|---|---|
| Timelock on hold duration | Out of scope — enforce off-chain |
| Multi-party approval to set hold | Out of scope — use a governed `admin` |
| Automatic hold expiry | Out of scope |
| Hold on non-risk-bearing reads | Out of scope — reads are always safe |
| Fee-on-transfer or rebasing tokens | Out of scope — unsupported by design |
| Sybil resistance for investor cap | Out of scope — limits chain accounts only |

---

## Test coverage

The matrix in `escrow/src/tests/legal_hold.rs` covers:

1. Each gated function panics with the exact message when hold is `true`.
2. Each gated function succeeds normally when hold is `false` (or cleared).
3. `set_legal_hold` requires admin auth; non-admin call panics.
4. `clear_legal_hold` requires admin auth; non-admin call panics.
5. Hold defaults to `false` after `init`.
6. Hold persists across status transitions (no bypass via state change).
7. Hold can be toggled and re-blocks operations after re-set.
8. Hold persists after two-step admin handover; new admin must explicitly clear it.
9. `request_clear_legal_hold` requires admin auth and the configured delay is enforced before the hold can be cleared.
10. **Timing window — typed errors:**
    - Clearing without a prior request → `LegalHoldClearRequestMissing` (150).
    - Clearing one ledger-second before `clearable_at` → `LegalHoldClearNotReady` (151).
    - Clearing at exactly `clearable_at` (inclusive boundary) → succeeds.
    - Zero-delay: `request_clear_legal_hold` + immediate `set_legal_hold(false)` succeeds without ledger advancement.
11. **Recovery scenario:** hold active → `propose_admin` + `accept_admin` (not blocked by hold) → new admin clears hold → `settle` and `claim_investor_payout` resume.
12. Edge cases: hold check fires before amount / status / auth validation.
13. Non-gated ops (`update_maturity`, `propose_admin`, `accept_admin`, getters) are not blocked.
14. Claim idempotency survives a hold toggle.
15. Single hold toggle blocks all gated entrypoints in separate escrows.
