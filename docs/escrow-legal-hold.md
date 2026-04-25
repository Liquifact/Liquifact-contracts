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
- `update_maturity`, `update_funding_target`, `transfer_admin`, `migrate`

---

## Enforcement model

```
set_legal_hold(active: bool)
    └─ escrow.admin.require_auth()   ← Soroban auth check, cannot be spoofed
    └─ storage().instance().set(DataKey::LegalHold, active)
    └─ emits LegalHoldChanged { active: 1 | 0 }

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
- **Idempotent.** Calling `set_legal_hold(true)` when already `true` (or
  `false` when already `false`) is a no-op for state but still requires admin
  auth and emits an event.
- **Default off.** `legal_hold_active` returns `false` when the key has never
  been written, so newly deployed escrows are not accidentally frozen.

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
  compromised, and emergency recovery via `transfer_admin` + governance vote.

Without one of the above, a single compromised admin key can freeze all
investor funds with no on-chain recourse.

---

## Admin rotation during a hold

`transfer_admin` is not gated by the hold. This is intentional: if the current
admin is compromised or unresponsive, governance must be able to rotate the
admin key even while a hold is active. After rotation the new admin inherits
the hold state and must explicitly call `clear_legal_hold` to unfreeze.

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

The matrix in `escrow/src/test/legal_hold.rs` covers:

1. Each gated function panics with the exact message when hold is `true`.
2. Each gated function succeeds normally when hold is `false` (or cleared).
3. `set_legal_hold` requires admin auth; non-admin call panics.
4. `clear_legal_hold` requires admin auth; non-admin call panics.
5. Hold defaults to `false` after `init`.
6. Hold persists across status transitions (no bypass via state change).
7. Hold can be toggled and re-blocks operations after re-set.
8. Hold persists after `transfer_admin`; new admin must explicitly clear it.
