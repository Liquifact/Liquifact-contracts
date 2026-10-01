## Description

Harden fee-schedule state transitions against duplicate submissions, competing updates, and repeated or boundary-time activation. Identical submissions are idempotent before and after activation, and due schedules are staged in memory and persisted only after validation succeeds.

## Type of Change

- [x] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Files Modified

- `escrow/src/lib.rs` — route schedule submission, activation, and getters through one state-transition model; add focused regression tests.
- `escrow/src/types.rs` — define fee-schedule state loading, boundary-aware reads, promotion, and persistence using the existing storage keys.

## Testing

- [ ] Tested locally (the repository test command is blocked by existing compile errors)
- [x] Added unit tests
- [ ] Tested on Stellar Testnet (for wallet/contract changes)

## Code Quality checks

- `cargo test --workspace` (Ubuntu WSL): blocked by existing project compile failures listed below; tests could not execute.
- `cargo fmt -p liquifact_escrow -- --check`: blocked because `mod tests` resolves to both `escrow/src/tests.rs` and `escrow/src/tests/mod.rs`.
- Rust parsing check for `escrow/src/lib.rs`: passed.
- `rustfmt --check escrow/src/types.rs`: passed.
- `git diff --check`: passed.

# Behavioural Changes

- A matching pending schedule submission remains a successful no-op, including after that schedule becomes active.
- A competing schedule cannot replace an existing pending schedule.
- Activation is due at `current_ledger >= activation_ledger`; repeated activation after promotion is a no-op.
- Failed validation does not persist a staged activation or partial schedule update.
- Existing public entrypoints, `FeeSchedule` layout, and storage keys are preserved.

## Tests Errors

The baseline `cargo test --workspace` already failed before these changes with unrelated compile errors, including:

- Ambiguous `tests` module: both `escrow/src/tests.rs` and `escrow/src/tests/mod.rs` exist.
- Duplicate declarations, including `MAX_INVESTOR_ALLOWLIST_BATCH`, `AttestationNotRevoked`, `AdminProposalCancelled`, `CollateralClearedEvt`, and `clear_sme_collateral_commitment`.
- Duplicate `EscrowError` discriminants (including 81, 85, 176, and 240–247).
- Unresolved identifiers and missing API members, including `was_allowlisted`, `FundingDeadlineUpdated`, `transfer_into_escrow_with_balance_checks`, missing `DataKey` variants, and missing `EscrowError` variants.
- Existing test source errors, including unresolved test variables, obsolete event/type references, missing initializer fields, and calls with missing arguments.

These unrelated baseline errors were not changed as part of this implementation.

Closes #<issue number>
