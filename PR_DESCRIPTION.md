# PR Documentation

## Summary

Add a token integration security checklist for the LiquiFact escrow contract and wire it into repository documentation.

## What changed

- Added `docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`
  - Documents supported token assumptions for escrow operations
  - Lists explicit unsupported token behaviors such as fee-on-transfer tokens, paused/frozen token contracts, callback-based reentrancy, and dynamic decimals
  - Clarifies that the escrow contract stores numeric amount state and collateral metadata only, rather than performing token custody
- Updated `README.md`
  - Added a new `Token integration security checklist` section
  - Linked the README to the new docs file
  - Added a note that token transfer safety belongs in the integration layer
- Updated `escrow/src/lib.rs`
  - Added crate-level documentation referencing the new checklist
  - Registered the new `test_token_integration` module
- Added `escrow/src/test_token_integration.rs`
  - Confirms SME collateral commitments are metadata-only
  - Verifies the checklist docs contain key guidance and warnings

## Why this matters

The escrow contract does not itself move tokens. This PR makes the cross-contract token integration assumptions explicit for reviewers, implementers, and auditors.

By documenting supported token behavior and unsupported cases, the repository now gives integration teams a clear checklist for safe token handling.

## Validation

- `cargo fmt --all -- --check`
- `cargo test -p liquifact_escrow`

Note: cargo was not available in the earlier terminal environment, so run the validation commands locally if needed.

## Files changed

- `README.md`
- `escrow/src/lib.rs`
- `docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`
- `escrow/src/test_token_integration.rs`
