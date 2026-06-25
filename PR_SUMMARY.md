# PR Description: Test fund_with_commitment rejects a second deposit from the same investor (#260)

## Description

This PR implements comprehensive test cases for the LiquiFact escrow contract to enforce state-machine invariants regarding investor funding behavior, fully resolving issue **#260**.

Specifically, the contract requires that the selection of tier-based yield and claim-lock gates is permanently set during the investor's **first deposit**. Any subsequent contributions by the same investor must use the simple `fund()` entrypoint, which preserves their originally assigned yield and claim-lock gate.

This PR adds a dedicated suite of unit tests in `escrow/src/tests/funding.rs` and updates the optional tiered yield ADR (`docs/adr/ADR-005-tiered-yield.md`) to document the complete test coverage.

Closes #260
Closes #244

---

## Technical Details & Invariants Tested

We implemented **6 new unit tests** to comprehensively verify the state machine's boundary conditions and invariants:

1. **`test_commitment_claim_lock_preserved_after_follow_on_fund`**:
   Verifies that after a tiered deposit via `fund_with_commitment(lock_secs > 0)`, a subsequent plain `fund()` call by the same investor succeeds and leaves both `InvestorEffectiveYield` and `InvestorClaimNotBefore` (absolute timestamp lock) unchanged.
   
2. **`test_commitment_invariant_across_multiple_follow_on_funds`**:
   Ensures that tier and claim-lock selection remain immutable across multiple consecutive follow-on `fund()` calls from the same investor.

3. **`test_commitment_zero_lock_follow_on_fund_no_claim_gate`**:
   Confirms that zero-lock commitments correctly assign base yield and no claim gate, and that subsequent `fund()` calls preserve these zero-valued guards.

4. **`test_second_fund_with_commitment_panics_without_tier_table`**:
   Asserts that a second `fund_with_commitment` call from an existing investor correctly triggers a panic with the expected error message: `"Additional principal after a tiered first deposit must use fund(), not fund_with_commitment()"` even when no tier table is configured.

5. **`test_fund_first_then_commitment_second_panics`**:
   Verifies the inverse rule: a plain `fund()` first deposit permanently closes the tier selection window, so any follow-on `fund_with_commitment` by the same investor panics with the expected error.

6. **`test_fund_first_deposit_sets_base_yield_and_no_claim_gate`**:
   Sanity checks that a simple `fund()` first deposit correctly defaults the investor's effective yield to base yield and sets no claim gate.

---

## Verification Plan

### Automated Verification
Run the contract test suite:
```bash
cargo test
```
*Tests added under:* `escrow/src/tests/funding.rs`
*Documentation updated under:* `docs/adr/ADR-005-tiered-yield.md`

### Manual Verification
- Verified compilation cleanliness of the `liquifact_escrow` library.
- Verified test suite integration and invariant assertions match ADR-005 spec.
