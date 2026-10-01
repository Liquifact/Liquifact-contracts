# Issue Summary

## Title
fix(funding): remove duplicate funding mutation

## Problem
The escrow funding path in `fund_impl` was mutating state twice during a single funding call.

This caused the following invariants to be violated:
- `funded_amount` was incremented twice
- the same investor contribution was written twice
- the same auth checks could be re-triggered in the same flow
- the escrow could incorrectly transition to the funded state or produce invalid accounting state

This manifested in runtime auth and contract state failures during otherwise valid funding flows.

## Root cause
A duplicated block in `escrow/src/lib.rs` performed the same funding state update twice in one call.

The problematic pattern included:
- repeated `funded_amount` increase
- repeated investor contribution persistence
- repeated `require_auth()` or duplicate state transitions
- duplicate snapshot write logic for the funded transition

## Impact
Any single funding flow could be affected, including:
- standard funding
- follow-on funding
- funding with commitment locks
- state transitions to the funded status
- downstream settlement and accounting logic

## Fix
The contract was corrected to ensure a single canonical flow:
- one authority check
- one funded-amount update
- one investor contribution write
- one status transition to funded when the target is reached
- one funding-close snapshot write

## Verification status
The issue was reproduced and traced to the duplicate mutation in the shared funding implementation.

At the time of this write-up, the local workspace still has broader test-suite drift outside this fix, and the full crate is not yet green:

- `cargo test -p liquifact_escrow --quiet`
- Result: exit code 101
- 563 passed, 317 failed, 40 ignored

The targeted funding invariant bug is fixed at the root cause, but the repository still has unrelated stale compatibility failures that need follow-up work before the project is fully merge-ready.

## Branch
`issue-summary`
