# PR Summary: test(allowlist): cover auth negative paths

**Closes #936**

## Description

This PR adds comprehensive negative-authorization test coverage for all admin-guarded allowlist entrypoints in the LiquiFact escrow contract. The tests assert that unauthorized callers (no auth, wrong role) are correctly rejected with panics across every state-mutating allowlist function.

Previously, the allowlist module's auth-negative paths were covered only by `#[should_panic]` tests in `test_allowlist_tests.rs` (5 tests) without wrong-signer differentiation, and a single test in `admin.rs`. This PR brings allowlist auth coverage up to the same exhaustive standard used by the `auth_matrix.rs` module for collateral operations.

## Technical Details

### Entrypoints Covered

All 3 admin-only allowlist entrypoints (`load_escrow_require_admin` guard):

| Entrypoint | Guard | Role |
|---|---|---|
| `set_allowlist_active` | `load_escrow_require_admin` | admin |
| `set_investor_allowlisted` | `load_escrow_require_admin` | admin |
| `set_investors_allowlisted` | `load_escrow_require_admin` | admin |

### Auth-Negative Test Matrix (9 new tests)

For each entrypoint, 3 rejection scenarios are tested:

1. **No auth** (`mock_auths(&[])`) → panics with host auth error
2. **Wrong signer — SME** (SME address tries to call admin function) → panics with host auth error
3. **Wrong signer — stranger** (random generated address) → panics with host auth error

### Test Implementation

All tests live in `escrow/src/tests/auth_matrix.rs`, following the existing module pattern:

- Uses the established `assert_no_auth_panics!` macro for zero-auth scenarios
- Uses the established `assert_wrong_auth_panics!` macro for wrong-signer scenarios
- Reuses the existing `setup_inited()` helper for escrow deployment/initialization
- No production code changes — tests only

### Test List

| # | Test Name | Entrypoint | Scenario |
|---|---|---|---|
| 1 | `set_allowlist_active_no_auth_panics` | `set_allowlist_active` | No auth |
| 2 | `set_allowlist_active_wrong_signer_sme_panics` | `set_allowlist_active` | SME signs |
| 3 | `set_allowlist_active_wrong_signer_stranger_panics` | `set_allowlist_active` | Random signs |
| 4 | `set_investor_allowlisted_no_auth_panics` | `set_investor_allowlisted` | No auth |
| 5 | `set_investor_allowlisted_wrong_signer_sme_panics` | `set_investor_allowlisted` | SME signs |
| 6 | `set_investor_allowlisted_wrong_signer_stranger_panics` | `set_investor_allowlisted` | Random signs |
| 7 | `set_investors_allowlisted_no_auth_panics` | `set_investors_allowlisted` | No auth |
| 8 | `set_investors_allowlisted_wrong_signer_sme_panics` | `set_investors_allowlisted` | SME signs |
| 9 | `set_investors_allowlisted_wrong_signer_stranger_panics` | `set_investors_allowlisted` | Random signs |

## Files Changed

| File | Change |
|---|---|
| `escrow/src/tests/auth_matrix.rs` | Added 9 new auth-negative tests for allowlist entrypoints |

## Verification

Run the auth matrix tests:

```bash
cd escrow && cargo test auth_matrix
```

Expected: All 11 tests pass (2 existing collateral tests + 9 new allowlist tests).

Run full lint suite:

```bash
cd escrow && cargo fmt --check && cargo clippy --all-targets -- -D warnings
```

## Relationship to Existing Coverage

The 5 `#[should_panic]` auth tests in `test_allowlist_tests.rs` and the 1 in `admin.rs` are **preserved** — this PR adds orthogonal wrong-signer detection that the should_panic pattern cannot express. Together they provide complete auth-negative coverage for the allowlist module.
