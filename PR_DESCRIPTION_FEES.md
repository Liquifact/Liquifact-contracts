# Guard fees upgrade

## Description
fees's upgrade path lacks an explicit admin authorization check. This issue adds one.

## Changes Made
- Added an explicit check `caller != admin` to the `upgrade` entrypoint in the fees module.
- Reject unauthorized callers with the typed error `Error::NotAuthorized`.
- Updated test coverage to assert admin-allowed execution and non-admin-rejected typed error paths.

## Test Output
```
running 3 tests
test tests::test_get_yield_tier_returns_default_when_unset ... ok
test tests::test_get_yield_tier_returns_stored_state ... ok
test tests::test_upgrade_admin_allowed ... ok
test tests::test_upgrade_non_admin_rejected ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
