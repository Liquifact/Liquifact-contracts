# Verification note for #1314

The tests added in this PR could not be executed locally because the
upstream `main` branch does not currently compile. Failures observed
when running `cargo test -p liquifact_escrow` on a clean checkout of
`main` (before any of this PR's changes):

## Production code (`escrow/src/lib.rs`)
1. `EscrowError::AttestationNotRevoked` is declared twice:
   - line ~662: `AttestationNotRevoked = 56`
   - line ~835: `AttestationNotRevoked = 168`
   `#[contracterror]` fails with E0004 (non-exhaustive match) and
   an unreachable-pattern warning.
2. `claim_investor_payout` (line ~7243) moves `investor` and then
   borrows it again — E0382.

## Test files (pre-existing)
- `test_allowlist_tests.rs`: signature drift — `set_investor_allowlisted`,
  `set_investors_allowlisted`, `set_allowlist_active`, and `cancel_funding`
  all gained an `expected_nonce: u32` argument that the tests do not
  pass. `AllowlistCapacityReached` and `set_allowlist_limit` /
  `get_allowlist_limit` / `get_allowlist_metadata` no longer exist on the
  contract. Several `HashSet<soroban_sdk::String>` uses fail trait bounds.
- `callback_binding_tests.rs`: `cancel_funding` missing nonce.
- `release_budget_tests.rs`: `init` missing 19th argument
  (`token_decimals: Option<u32>`).

## Effect on this PR
The new tests in `escrow/src/tests/attestation_config_view.rs` are
independently well-formed and reference only public entrypoints,
constants, and error variants that exist in `escrow/src/lib.rs`
(verified by grep — see commands in the PR description).

Once the trunk compiles, this PR's tests should be runnable with:

    cargo test -p liquifact_escrow attestation_config_view -- --nocapture

## Recommendation
Fix the `EscrowError` duplicate variant and the `claim_investor_payout`
move/borrow in a separate PR before merging any test-only change.
