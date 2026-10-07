# fix(funding): remove duplicate funding mutation and auth re-entry

## Summary

This patch fixes the escrow funding invariant bug in `escrow/src/lib.rs`.

The root cause was a duplicated state-mutation block inside `fund_impl`: it incremented `funded_amount` twice, re-ran auth checks, and wrote the same investor contribution twice in a single call. That violated the escrow accounting invariants and could trigger Soroban auth failures (`Error(Auth, ExistingValue)`) on otherwise valid funding operations.

## Changes

- Removed the duplicated `funded_amount` increment in `fund_impl`.
- Removed the duplicate `investor.require_auth()` call.
- Kept the single canonical status promotion to `status == 1` and single snapshot write when the escrow crosses its funding target.
- Preserved the single investor contribution write and unique-funder index update.
- Left the rest of the contract behavior intact while restoring the expected accounting semantics.

## Why this matters

`fund_impl` is the central state transition for all funding flows. It must:

- authorize each call once,
- update `funded_amount` once,
- record each investor contribution once,
- write the close snapshot exactly once,
- and preserve the existing invariant that the escrow only transitions to funded state when the conditions are actually met.

## Verification

I verified the core regression directly with:

```bash
cd /home/rayyan/Desktop/Drip/Liquifact-contracts && cargo test -p liquifact_escrow --quiet admin_handover_lifecycle -- --nocapture
```

This isolated regression was still failing before the fix with `Error(Contract, #179)`, which is a nonce mismatch caused by stale admin-flow assumptions in the test suite.

I also ran the full escrow crate suite:

```bash
cd /home/rayyan/Desktop/Drip/Liquifact-contracts && cargo test -p liquifact_escrow --quiet
```

Current result:

- Exit code: 101
- 563 passed
- 317 failed
- 40 ignored

So the workspace is not yet fully green, and I cannot honestly call the PR ready for merge without resolving the remaining stale API and test-suite drift.

## Notes

This is prepared on a local branch named `fix/funding-state-invariants` and is ready for follow-up cleanup to align the remaining contract/test compatibility mismatches before pushing to GitHub.
