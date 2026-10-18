## Description

Hardens escrow funding state updates so threshold-crossing contributions are accounted for once and late contributions are rejected without changing the closed funding state. Prevents an investor who fully unfunds and later re-funds from being duplicated in the historical investor index.

## Type of Change

- [x] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Files Modified

- `escrow/src/lib.rs`
- `escrow/src/tests/funding.rs`

## Testing

- [ ] Tested locally (Rust test execution is blocked by existing compile errors; see Tests Errors)
- [x] Added unit tests
- [ ] Tested on Stellar Testnet

## Code Quality checks

- `git diff --check` passed.
- Modified Rust files parsed with `rustfmt` using `skip_children=true`.
- `cargo test --workspace` and targeted Clippy were attempted in Ubuntu WSL; both are blocked before tests can execute by existing compilation errors.
- `cargo fmt -p liquifact_escrow -- --check` is blocked by the existing ambiguous `tests` module layout.

## Behavioural Changes

- Funding principal is incremented once, and the close snapshot captures the resulting total.
- Funding validates before requesting investor and payer authorization; storage changes and the inbound token transfer remain part of the atomic contract invocation.
- The investor hard cap is checked against historical index growth, and re-funding a previously indexed investor does not append a duplicate.
- Active-funder count increments use checked arithmetic.
- Regression tests cover sequential competing funding at the target, rejection of a late contribution, and full unfund followed by re-funding.

## Tests Errors

`cargo test --workspace` fails during compilation, before tests run. The current baseline diagnostics include:

- `E0761`: `tests` is declared with both `escrow/src/tests.rs` and `escrow/src/tests/mod.rs`.
- `E0428`: duplicate definitions, including `MAX_INVESTOR_ALLOWLIST_BATCH`, event types, and generated contract-event symbols.
- `E0422` / `E0425`: missing event structs and unresolved identifiers in other contract paths.
- `E0599`: references to missing enum variants or methods, including `PauseState` and `paginate_window`.

These compile errors are outside the funding changes in this PR. The prior `YieldResolution` destructuring diagnostic at the edited funding path was corrected; it is not listed as a current baseline error.

Closes # (issue number not provided)
