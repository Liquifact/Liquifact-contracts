# fix: bound commitment lock to settlement maturity in `fund_with_commitment`

## Branch
`security/contracts-commitment-lock-bound` → `main`

## PR Link
```
https://github.com/Liquifact/Liquifact-contracts/compare/main...Oyixah:security/contracts-commitment-lock-bound?expand=1
```

---

## Summary

Closes the security gap where a `fund_with_commitment` deposit with a
`committed_lock_secs` longer than the escrow's maturity window would set
`InvestorClaimNotBefore` past the point where principal is due — trapping a
settled investor's payout claim until the lock expired.

The fix rejects such deposits **at deposit time** with the new typed error
`CommitmentLockExceedsMaturity` (code 111).

---

## Changes

### `escrow/src/lib.rs`

- **Bound check in `fund_impl` tiered branch** — after computing
  `claim_nb = now + committed_lock_secs`, rejects the deposit if
  `claim_nb > escrow.maturity` when both values are positive.
  Zero-lock (`committed_lock_secs == 0`) and zero-maturity escrows are
  unaffected (guard condition: `claim_nb > 0 && escrow.maturity > 0`).
- **`register_mock_token_if_needed` cfg fix** — changed cfg guard from
  `#[cfg(any(test, feature = "testutils"))]` to `#[cfg(test)]` to resolve
  a pre-existing `std::panic` unavailable compile error when building with
  `--features testutils` outside the test runner.

### `escrow/src/tests/funding.rs`

- Fixed 4 commitment-lock tests that set the ledger timestamp **before**
  calling `setup()` — `setup()` resets the timestamp to 0, causing wrong
  `claim_nb` assertions and a false-pass on the rejection test.
  Moved `env.ledger().set(...)` to after `setup()` in all affected tests.

### `docs/adr/ADR-005-tiered-yield.md`

- Added the maturity bound rule to the first-deposit decision record.

### `docs/escrow-legal-hold.md`

- Added "Claim timing boundary" bullet documenting the
  `CommitmentLockExceedsMaturity` guard and its interaction with legal hold.

---

## Edge cases covered

| Scenario | Behaviour |
|---|---|
| `lock < maturity` | Accepted ✓ |
| `lock == maturity` (exact boundary) | Accepted ✓ (inclusive) |
| `lock = maturity + 1` (one second over) | Rejected → `CommitmentLockExceedsMaturity` (111) |
| `lock >> maturity` (far over) | Rejected → `CommitmentLockExceedsMaturity` (111) |
| `committed_lock_secs == 0` | Always accepted — no claim gate set |
| `maturity == 0` (no maturity) | Always accepted — bound not applied |
| Plain `fund()` call | Unaffected — `simple_fund=true` never sets a claim lock |

---

## Security notes

| Concern | Mitigation |
|---|---|
| Payout locked past maturity | `claim_nb <= escrow.maturity` enforced at deposit time |
| Overflow on `now + lock` | Existing `InvestorClaimTimeOverflow` guard retained (checked_add) |
| Additive-only error codes | `CommitmentLockExceedsMaturity = 111` — no existing codes renumbered |
| Existing entrypoints unchanged | Only the tiered branch of `fund_impl` gains the new guard |
| Zero-lock / zero-maturity semantics preserved | Guard condition requires both `claim_nb > 0` and `maturity > 0` |

---

## Test results

```
cargo fmt --all -- --check   ✓
cargo build                  ✓  (0 warnings)
cargo test                   ✓  844 passed, 0 failed, 50 ignored
```
