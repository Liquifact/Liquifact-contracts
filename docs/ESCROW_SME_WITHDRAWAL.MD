# Escrow SME Withdrawal — Accounting Guide for Funded Liquidity

**Scope:** `escrow/src/lib.rs` — `withdraw` entrypoint  
**Related ADRs:** [ADR-001](adr/ADR-001-state-model.md) · [ADR-003](adr/ADR-003-settlement-flow.md) · [ADR-004](adr/ADR-004-legal-hold.md)  
**Last updated:** 2026-04-25

---

## 1. What `withdraw` does (and does not do)

`withdraw` is a **pure on-chain state transition**. It moves the escrow from
`status 1` (funded) to `status 3` (withdrawn) by rewriting the
`DataKey::Escrow` entry atomically inside a single Soroban host-function call.

It does **not**:

- Transfer tokens to the SME or any investor.
- Zero or modify `funded_amount`, `funding_target`, or any investor contribution record.
- Emit anything resembling a payment instruction.

Token movement is the responsibility of the **integration layer** that calls
into this contract. See `escrow/src/external_calls.rs` and the
[Token Integration Checklist](../ESCROW_TOKEN_INTEGRATION_CHECKLIST.md).

---

## 2. State model

```
status 0  open
  │
  │  fund() — investors contribute; funded_amount accumulates
  │  (blocked once status transitions to 1)
  ▼
status 1  funded   ◀── funded_amount >= funding_target
  │
  ├──── settle(sme) ──────────────────────────▶  status 2  settled
  │                                                │
  │                                                └── claim_investor_payout(investor)
  │                                                    (state marker; no token transfer)
  │
  └──── withdraw(sme) ─────────────────────────▶  status 3  withdrawn
                                                   (terminal — no further transitions)
```

Transitions are strictly forward. No entrypoint can decrease `status`. The
property test `prop_status_only_increases` in `escrow/src/test/properties.rs`
enforces this invariant across arbitrary fund amounts.

---

## 3. Preconditions for `withdraw`

All of the following must hold or the call panics and the ledger entry is not modified:

| Precondition | Storage checked | Error behaviour |
|---|---|---|
| `status == 1` | `DataKey::Escrow.status` | panic — wrong status |
| No legal hold active | `DataKey::LegalHold` (absent → `false`) | panic — hold active |
| Caller is the SME | `InvoiceEscrow.sme` via `require_auth()` | auth failure |

There is no maturity gate on `withdraw`. The maturity field (`InvoiceEscrow.maturity`)
is only evaluated by `settle`. An SME may withdraw a funded escrow at any time
as long as no legal hold is active.

---

## 4. Accounting expectations for the integration layer

### 4.1 What the contract guarantees at `status 3`

After `withdraw` succeeds the following storage values are immutable and safe
to read for any downstream accounting:

| Value | Storage key | Meaning |
|---|---|---|
| `funded_amount` | `DataKey::Escrow` | Total principal received from all investors |
| `funding_target` | `DataKey::Escrow` | The target that was reached to trigger funding closure |
| Per-investor amounts | `DataKey::InvestorContribution(addr)` | Each investor's principal contribution |
| `FundingCloseSnapshot` | `DataKey::FundingCloseSnapshot` | Immutable snapshot: `total_principal`, `funding_target`, ledger timestamp, ledger sequence |

The `FundingCloseSnapshot` is written exactly once — at the moment the escrow
transitions from `status 0` to `status 1` inside `fund_impl`. Neither `withdraw`
nor any other subsequent entrypoint modifies it. This means the denominator for
pro-rata calculations is always stable after funding closes.

### 4.2 Pro-rata refund arithmetic

When an SME withdraws, each investor is typically owed a refund proportional to
their contribution. The correct denominator is `FundingCloseSnapshot.total_principal`
(not `funding_target`), because overfunded escrows absorb excess into the snapshot
so late investors are not disadvantaged.

```
investor_refund_share =
    get_investor_contribution(investor) / snapshot.total_principal
```

The integration layer must:

1. Read `get_funding_close_snapshot()` for the denominator.
2. Iterate known investor addresses (tracked off-chain or via `get_investor_count` +
   indexed events) and call `get_investor_contribution(addr)` for each.
3. Compute each investor's refund in off-chain arithmetic (or in a separate
   distribution contract) and trigger the SEP-41 token transfer(s).

This contract stores no knowledge of token balances. The integration layer must
independently verify that the token contract's actual balance matches expected
obligations before disbursing.

### 4.3 Overfunding

If `funded_amount > funding_target` the excess was deliberately accepted. The
snapshot records the full `funded_amount` as `total_principal`. Refund
arithmetic for all investors (including overfund contributors) uses that larger
denominator, which means each investor recovers exactly the fraction they put in
— no rounding loss is hidden inside the contract.

### 4.4 Partial funding (unfunded path)

`withdraw` requires `status == 1`, which means the escrow **has already reached
its target**. An escrow that never reaches `funding_target` stays at `status 0`
and cannot be `withdraw`n by the SME — it must be handled by an admin-governed
cancellation flow (not yet implemented in v1; see open issues). Accounting for
the partial-fund cancellation case is therefore **out of scope** for this
document and for `withdraw`.

---

## 5. Legal hold interaction

A legal hold (`DataKey::LegalHold == true`) blocks `withdraw` unconditionally:

```
withdraw(sme)
  ├── status != 1          → panic
  ├── legal_hold_active()  → panic   ◀── checked before auth
  └── require_auth(sme)
```

The hold can be placed at any lifecycle stage — including after the escrow is
funded but before the SME calls `withdraw`. Once a hold is applied:

- Neither the SME nor investors can move funds.
- Only the `admin` can clear the hold via `set_legal_hold(admin, false)`.
- Read-only entrypoints (`get_escrow`, `get_investor_contribution`,
  `get_funding_close_snapshot`, etc.) remain accessible.

Production deployments should use a multisig or DAO as `admin` so holds cannot
be used unilaterally to strand funds. See [ADR-004](adr/ADR-004-legal-hold.md)
for the governance rationale.

---

## 6. Token economics — what this contract does not handle

The following are **explicitly out of scope** for this contract and must live in
the integration layer or off-chain:

- Actual SEP-41 token transfers to the SME or investors.
- Yield or penalty calculation (tiered yield lives in ADR-005 and
  `claim_investor_payout`, not `withdraw`).
- Fee deduction, platform commission, or withholding.
- FX or stablecoin conversion.
- Any cross-contract invocation that re-enters this contract after `withdraw`.

See `escrow/src/external_calls.rs` for the supported SEP-41 wrapper interface
and the security constraints (balance-equality post-transfer checks, SEP-41
conformance assertions).

---

## 7. Event trail

`withdraw` emits one contract event that indexers should record:

| Event | Fields | Purpose |
|---|---|---|
| `EscrowWithdrawn` (or equivalent) | `invoice_id`, `sme`, `ledger_timestamp` | Audit trail; marks the escrow as terminal in the `withdrawn` path |

Indexers can reconstruct the full lifecycle from events alone (see ADR-001).
After `EscrowWithdrawn` no further state-changing events will be emitted for
this escrow (except `LegalHoldChanged` if a hold is applied or cleared on the
already-terminal escrow for record-keeping purposes).

---

## 8. Test coverage summary

All tests live in `escrow/src/test/settlement.rs`. The following scenarios are covered:

| Category | Test name | What it asserts |
|---|---|---|
| Happy path | `withdraw_sets_status_to_three` | `status == 3` after withdraw |
| Happy path | `withdraw_preserves_accounting_fields` | `funded_amount` / `funding_target` untouched |
| Happy path | `withdraw_emits_event` | At least one event emitted |
| Wrong status | `withdraw_on_open_escrow_panics` | Rejects `status == 0` |
| Wrong status | `withdraw_on_settled_escrow_panics` | Rejects `status == 2` |
| Wrong status | `withdraw_twice_panics` | Rejects `status == 3` (double-withdraw) |
| Wrong status | `settle_after_withdraw_panics` | `settle` blocked after `status == 3` |
| Wrong status | `fund_after_withdraw_panics` | `fund` blocked after `status == 3` |
| Legal hold | `withdraw_blocked_by_legal_hold` | Hold blocks withdraw at `status == 1` |
| Legal hold | `withdraw_succeeds_after_hold_cleared` | Cleared hold unblocks withdraw |
| Legal hold | `legal_hold_set_by_non_admin_panics` | Non-admin cannot place hold |
| Settle path | `settle_sets_status_to_two` | Complementary settle happy path |
| Settle path | `settle_blocked_by_legal_hold` | Settle also blocked by hold |
| Settle path | `settle_on_open_escrow_panics` | Settle requires `status == 1` |
| Settle path | `settle_twice_panics` | Settle idempotency guard |
| Maturity | `settle_before_maturity_panics` | Ledger timestamp gate enforced |
| Maturity | `settle_at_maturity_succeeds` | Inclusive boundary accepted |
| Investor claim | `claim_investor_payout_succeeds_after_settle` | Happy-path claim |
| Investor claim | `claim_investor_payout_twice_panics` | Idempotency guard |
| Investor claim | `claim_investor_payout_blocked_by_legal_hold` | Hold blocks claim |
| Investor claim | `claim_investor_payout_before_settle_panics` | Requires `status == 2` |
| Investor claim | `claim_investor_payout_non_participant_panics` | Non-investor rejected |
| Snapshot | `funding_snapshot_survives_withdraw` | Snapshot immutable after withdraw |
| Snapshot | `funding_snapshot_survives_settle` | Snapshot immutable after settle |
| Accounting | `investor_contribution_readable_after_withdraw` | Contribution available for refunds |
| Accounting | `multi_investor_contributions_preserved_after_withdraw` | Multi-investor accounting correct |
| Terminal | `no_state_mutation_possible_after_withdraw` | All write paths blocked at `status == 3` |

---

## 9. Security notes

- **State transition atomicity:** Soroban's single-writer model means the
  `DataKey::Escrow` rewrite in `withdraw` is atomic. There is no window where
  `status` is partially updated.
- **No re-entrancy risk inside withdraw:** `withdraw` makes no cross-contract
  calls. The integration layer's token transfer happens after the `withdraw`
  state-change is committed.
- **Assumption — token economics out of scope:** This contract intentionally
  stores only numeric amounts and state labels. All token-level risks (fee-on-
  transfer tokens, rebasing assets, wrong-asset deposits) are delegated to the
  integration layer per `external_calls.rs`.
- **Legal hold cannot be bypassed:** The hold check precedes auth in the call
  order. Even a valid SME key cannot move past the hold without admin clearance.
- **No backward transitions:** The forward-only invariant is property-tested
  (`prop_status_only_increases`). A future schema migration that adds a
  `cancel` path from `status 0` must not reuse integer values 1–3.
