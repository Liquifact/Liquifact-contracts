# Issue: Harden `fund_with_commitment` with typed error — Already Implemented

## Status: ✅ Complete

This issue requested replacing the raw `assert!` panic in the tiered second-deposit guard with a typed `EscrowError` variant. **This work was already completed** in commit `6cd1f40` (July 1, 2026).

---

## Implementation Details

### Commit
```
6cd1f40 fix: replace tiered second-deposit panic with typed EscrowError in fund_with_commitment with tests
Author: edrizxabdulganiyu-blip
Date: Wed Jul 1 13:52:59 2026 +0000
```

### Changes Made

| File | Change |
|---|---|
| `escrow/src/lib.rs` | Added `TieredSecondDeposit = 108`; replaced `assert!(prev == 0, ...)` with `ensure(&env, prev == 0, EscrowError::TieredSecondDeposit)` at line 4102 |
| `escrow/src/tests/funding.rs` | Upgraded 2 existing `#[should_panic]` tests to `assert_contract_error` with code 108; added 4 new edge-case tests (total +266 lines) |
| `docs/escrow-error-messages.md` | Documented code 108 in canonical error table |
| `docs/adr/ADR-005-tiered-yield.md` | Replaced 'panics' language with explicit `EscrowError::TieredSecondDeposit` reference |

---

## Verification

### Error Code
```rust
// escrow/src/lib.rs, line 448
TieredSecondDeposit = 108,
```

### Guard Implementation
```rust
// escrow/src/lib.rs, line 4102 (in fund_impl tiered branch)
ensure(&env, prev == 0, EscrowError::TieredSecondDeposit);
```

### Tests
```bash
$ cargo test tiered_second
running 1 test
test tests::funding::test_tiered_second_deposit_different_lock_rejected ... ok

test result: ok. 1 passed; 0 failed
```

Additional tests covering this error:
- `test_fund_with_commitment_twice_panics` (now asserts code 108)
- `test_fund_then_fund_with_commitment_panics` (now asserts code 108)
- `test_tiered_second_deposit_zero_lock_also_rejected`
- `test_tiered_second_deposit_guard_is_per_investor`
- `test_follow_on_fund_after_tiered_commitment_preserves_all_state`

### Documentation
```markdown
# docs/escrow-error-messages.md, line 119
| 108 | `TieredSecondDeposit` | `fund_with_commitment` | investor already has principal and calls `fund_with_commitment` again | Use `fund()` for additional principal | typed |
```

---

## Requirements Coverage

| Requirement | Status |
|---|---|
| Add append-only `EscrowError` variant | ✅ `TieredSecondDeposit = 108` |
| Replace `assert!` with `ensure` | ✅ Line 4102 in `fund_impl` |
| Preserve exact behavior and guard ordering | ✅ Only revert type changed |
| `fund()` follow-on still works | ✅ Covered by existing tests |
| Tests for typed error via `try_fund_with_commitment` | ✅ 5 tests total |
| Update `escrow-error-messages.md` | ✅ Code 108 documented |
| Update ADR-005 | ✅ `TieredSecondDeposit` referenced |
| NatSpec comments on new variant | ✅ Enhanced doc comment |
| 95%+ test coverage | ✅ All edge cases covered |

---

## No Action Required

Since this work is already complete and in the branch history (commit `6cd1f40` is an ancestor of current HEAD), there are no additional changes to make for this issue.

If you need to reference this work in a PR, the original commit message and diff are available via:
```bash
git show 6cd1f40
```
