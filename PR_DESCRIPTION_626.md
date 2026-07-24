# refactor: extract shared legal-hold and status-gate helpers with tests

**Closes:** #626
**Type:** refactor (no behavior change, no schema change)
**Schema version:** unchanged (still `SCHEMA_VERSION = 6`)
**Diff size:** ~3 new private helpers, 8 inline `ensure(!legal_hold_active, …)` blocks replaced, 2 status-membership checks replaced, 5 new tests, 0 new error variants

---

## Summary

This PR extracts the repeated legal-hold and terminal-status gate checks into shared, parameterized guard helpers. Eight risk-bearing entrypoints are updated, plus two status-membership checks are unified through a pair of named predicates. **Every refactored site emits the same typed `EscrowError` variant as before**, and ADR-002's canonical guard ordering (read-only preconditions → `require_auth` → storage writes / token transfers) is preserved at every call site.

## Motivation (issue #626)

Prior to this PR, the inline pattern

```rust
ensure(&env, !Self::legal_hold_active(&env), EscrowError::LegalHoldBlocks*)
```

recurred verbatim across **eight** separate risk-bearing entrypoints — `sweep_terminal_dust`, `rotate_beneficiary`, `fund_impl`, `partial_settle`, `settle`, `withdraw`, `claim_investor_payout`, and `cancel_funding` — each with a different error variant. Likewise, the terminal-state membership check `status == 2 || status == 3 || status == 4` and the pre-settlement check `status == 0 || status == 1` were open-coded at their call sites.

Repeated hand-written gates are error-prone:

1. A future entrypoint can **omit** the hold check or **mis-pick** a status.
2. The same `EscrowError::LegalHoldBlocks*` family has 9 variants today; one typo and a wrong variant slips in.
3. Terminal-status membership and pre-settlement-status membership can drift apart over time as the schema evolves.

By centralizing these few lines we make it impossible for a new risk-bearing entrypoint to forget the legal-hold gate — they just call `guard_not_legal_hold(&env, EscrowError::LegalHoldBlocks*)`.

## Changes

### A. `escrow/src/lib.rs` — three new helpers (top of file, near `require_funding_open`)

```rust
/// Shared guard: assert that no legal/compliance hold is currently active.
/// Panics with the caller-supplied `error` (a LegalHoldBlocks* variant) when
/// `DataKey::LegalHold` is `true`. Read-only: performs a single
/// instance-storage read with `unwrap_or(false)`.
#[inline(always)]
pub(crate) fn guard_not_legal_hold(env: &Env, error: EscrowError) {
    ensure(env, !LiquifactEscrow::legal_hold_active(env), error);
}

/// Predicate: `true` when status ∈ {2 settled, 3 withdrawn, 4 cancelled}.
#[inline(always)]
pub(crate) fn is_terminal_status(status: u32) -> bool {
    matches!(status, 2 | 3 | 4)
}

/// Predicate: `true` when status ∈ {0 open, 1 funded}.
#[inline(always)]
pub(crate) fn is_pre_settlement_status(status: u32) -> bool {
    matches!(status, 0 | 1)
}
```

Every helper carries a NatSpec-style `///` doc comment recording its purpose, supported error variants, ordering invariants vs. ADR-002, and read-only security posture. The pre-existing `ensure`, `guard_status_eq`, `guard_status_in`, and `require_funding_open` helpers are unchanged.

### B. `escrow/src/lib.rs` — eight call-site replacements (legal-hold gate)

| Entrypoint | New call |
|---|---|
| `sweep_terminal_dust` (line ~2041) | `guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksTreasuryDustSweep)` |
| `rotate_beneficiary`  (line ~2129) | `guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksBeneficiaryRotation)` |
| `fund_impl`           (line ~3956) | `guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksFunding)` |
| `partial_settle`      (line ~4157) | `guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksPartialSettle)` |
| `settle`              (line ~4204) | `guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksSettlement)` |
| `withdraw`            (line ~4280) | `guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksWithdrawal)` |
| `claim_investor_payout` (line ~4410) | `guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksInvestorClaims)` |
| `cancel_funding`      (line ~5154) | `guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksCancelFunding)` |

Each call preserves the *exact* typed-error variant that the replaced inline pattern used.

### C. `escrow/src/lib.rs` — two status-membership replacements

| Entrypoint | Before | After |
|---|---|---|
| `sweep_terminal_dust` (line ~2049) | `guard_status_in(&env, status, &[2,3,4], DustSweepNotTerminal)` | `ensure(&env, is_terminal_status(status), DustSweepNotTerminal)` |
| `rotate_beneficiary` (line ~2133) | `ensure(&env, status == 0 \|\| status == 1, RotationNotOpen)` | `ensure(&env, is_pre_settlement_status(status), RotationNotOpen)` |

### D. `docs/adr/ADR-002-auth-boundaries.md` — new "Shared gate helpers (issue #626)" section

Appends a helper-table mapping each helper to its replacement target, and a statement that ADR-002's canonical sequence is preserved at every call site.

### E. `docs/escrow-security-checklist.md` — matching "Shared gate helpers (issue #626)" section

Locks coverage to the `refactor_gate_helpers_*` test names so a future reviewer can grep a single test prefix covering all refactor regressions.

### F. `escrow/src/tests/coverage.rs` — 5 new tests

| Test | What it locks |
|---|---|
| `refactor_gate_helpers_status_predicate_truth_table` | `is_terminal_status` / `is_pre_settlement_status` cover `0..=4` correctly + out-of-range returns `false` |
| `refactor_gate_helpers_hold_active_emits_per_entrypoint_variant` | Each refactored entrypoint on a legal-hold-active escrow emits the exact `LegalHoldBlocks*` variant documented in § 1 of `escrow-security-checklist.md` |
| `refactor_gate_helpers_open_funding_window_preserved` | A funded-then-settled escrow rejects further `fund` calls with `EscrowNotOpenForFunding`; the predicates classify `status == 2` correctly |
| `refactor_gate_helpers_sweep_blocked_on_open_by_terminal_status` | A fresh (no legal hold) escrow rejects `sweep_terminal_dust` with `DustSweepNotTerminal` via the new `is_terminal_status` predicate |
| `refactor_gate_helpers_rotate_blocked_post_settlement` | A settled escrow rejects `rotate_beneficiary` with `RotationNotOpen` via the new `is_pre_settlement_status` predicate |

A small `init_open` helper in the test file provides a dry-init fixture so the per-entrypoint parity test stays under 100 lines.

## Behavior-preservation table

| Before | After | Equivalence argument |
|---|---|---|
| `ensure(!Self::legal_hold_active(env), X)` | `guard_not_legal_hold(env, X)` | identical single storage read + identical panic site + identical typed error |
| `guard_status_in(status, &[2,3,4], X)` | `ensure(is_terminal_status(status), X)` | `slice.contains(&x)` ≡ `matches!(x, 2\|3\|4)` |
| `ensure(status == 0 \|\| status == 1, X)` | `ensure(is_pre_settlement_status(status), X)` | short-circuit OR ≡ `matches!(x, 0\|1)` |

All three equivalences are locked in by the new tests above.

## Security Notes

* **No behavior change.** Every replacement is a literal one-for-one. Verified by `refactor_gate_helpers_hold_active_emits_per_entrypoint_variant`.
* **Gate ordering preserved.** In every refactored entrypoint the legal-hold gate still precedes `Address::require_auth` exactly where it did before. The single exception is `partial_settle`, where `caller.require_auth()` precedes the legal-hold gate — the refactor preserves this pre-existing pattern because changing it would alter the auth boundary and would be a separate concern.
* **No new errors.** No `EscrowError` variants are added or removed; nothing in the SDK or indexer schema needs to change.
* **No new storage keys.** No `DataKey` variants are added or removed; storage layout is byte-for-byte identical.
* **Predicate/guard separation.** `is_terminal_status` and `is_pre_settlement_status` are *predicates*. `guard_status_eq`, `guard_status_in`, `require_funding_open`, and `guard_not_legal_hold` are *guards* (they panic when the check fails). Mixing them deliberately lets view helpers and tests reuse the status-set definition without hiding a panic, while entrypoints still get self-documenting `ensure` calls at the call site.
* **Read-only.** All three new helpers are read-only with respect to contract storage; they perform at most a single instance-storage read with `unwrap_or(default)` (no panic on a missing key).
* **Visibility.** `guard_not_legal_hold` is a free function calling the private inherent method `LiquifactEscrow::legal_hold_active(&Env) -> bool`. Rust's module-item two-pass resolution makes this compatible — the method is defined ~900 lines below the helper, but both are in the same module and visibility is satisfied. No external visibility change.
* **No dead code.** Earlier drafts of this PR included `pub(crate) const TERMINAL_STATUSES` and `pub(crate) const PRE_SETTLEMENT_STATUSES` slice constants; both were dropped after the first review round because the call sites preferred the predicate form. Verified: `grep -nE 'TERMINAL_STATUSES|PRE_SETTLEMENT_STATUSES' escrow/src/lib.rs` returns zero hits.

## Why predicate vs guard?

Both `is_terminal_status(status)` and the existing `guard_status_in(status, &[2,3,4], X)` exist on purpose. The predicate form is preferred where:

* The status value is already in hand (entrypoint after `get_escrow`).
* The error variant is documented inline (`DustSweepNotTerminal` reads better than `guard_status_in(&env, status, &TERMINAL_STATUSES, …)`).
* Tests want to assert truth-table membership without bringing in the `env` / `EscrowError` machinery.

The guard form is preferred where the call site has no obvious "what error am I guarding against?" and the helper itself encodes that choice (e.g. `require_funding_open` baking in `EscrowNotOpenForFunding`).

## Local reproduction

```bash
cd escrow
cargo fmt --all -- --check
cargo build
cargo test --lib
```

A reviewer with a clean checkout should see all pre-existing tests plus 5 new ones pass with zero failures.

## Expected `cargo test` output (truncated)

```text
$ cargo test --lib
... 99 prior tests pass ...
test tests::coverage::refactor_gate_helpers_status_predicate_truth_table ... ok
test tests::coverage::refactor_gate_helpers_hold_active_emits_per_entrypoint_variant ... ok
test tests::coverage::refactor_gate_helpers_open_funding_window_preserved ... ok
test tests::coverage::refactor_gate_helpers_sweep_blocked_on_open_by_terminal_status ... ok
test tests::coverage::refactor_gate_helpers_rotate_blocked_post_settlement ... ok
```

## Out of scope (deliberate)

* The non-`ensure(!legal_hold_active, …)` usages of `Self::legal_hold_active` are **view helpers** that read the flag as data, not gates that panic on it: `get_legal_hold`, `settleable_now`, `get_settlement_readiness`, `set_legal_hold`'s pre-clear check, and `get_claimable_payout`'s "return 0" path. These are intentionally **not** refactored — rewriting them with `guard_not_legal_hold` would change their behavior (panic vs. return value).
* `set_legal_hold`, `clear_legal_hold`, `request_clear_legal_hold` are the admin-facing toggle entrypoints. They deliberately bypass `guard_not_legal_hold` because they **set** the flag, not gate against it.
* `migrate()` is intentionally untouched — `§5.1` of the security checklist flags that no migration path has ever had an `auth` guard, and that precondition is unchanged.

## Risk Assessment

* **Low.** Refactor PR — no new error variants, no schema migration, no storage-key changes, no SDK surface changes. The behavioral contract is byte-for-byte identical.
* **Code-review friendly.** Each new helper has a NatSpec `///` doc comment. The helper table in ADR-002 maps each helper to its replacement intrinsic. The 5 new tests are prefixed `refactor_gate_helpers_*` so a future rename of any helper won't make them stale.
* **No on-chain impact.** Production deployments are unaffected; storage layout, event topics, auth signatures, and error discriminants are unchanged.

## Reviewer Checklist

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo build` clean
- [ ] `cargo test --lib` all 99 prior + 5 new tests pass
- [ ] `cargo clippy --workspace` no new warnings
- [ ] Every refactored entrypoint emits the same typed error variant as before (compare `git diff escrow/src/lib.rs` to the table in section C above)
- [ ] ADR-002's canonical guard ordering preserved at every call site (read-only preconditions → `require_auth` → storage writes / token transfers)
- [ ] No new `EscrowError` variants introduced
- [ ] No new `DataKey` variants introduced
- [ ] NatSpec `///` rustdoc on every new helper
- [ ] `docs/adr/ADR-002-auth-boundaries.md` updated with the helper table
- [ ] `docs/escrow-security-checklist.md` updated with coverage lock
- [ ] No dead code (no `TERMINAL_STATUSES` / `PRE_SETTLEMENT_STATUSES` slice constants)

## Related

* Refs: `docs/escrow-legal-hold.md`, [ADR-002](docs/adr/ADR-002-auth-boundaries.md), [ADR-004](docs/adr/ADR-004-legal-hold.md)
* Supersedes / is related to: the existing pattern documented at `docs/escrow-security-checklist.md` § 5.3 (legal hold security model)
