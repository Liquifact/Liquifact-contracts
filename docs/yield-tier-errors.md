# Yield-Tier Errors

The LiquiFact escrow contract supports yield-tiers, allowing investors to commit to a `min_lock_secs` duration in exchange for a higher yield. Errors specific to configuring and using this feature emit typed `EscrowError` codes.

## Error Reference

| Code | Variant | Entrypoint(s) | Trigger | How to avoid it |
| ---: | --- | --- | --- | --- |
| 10 | `TierYieldOutOfRange` | `init` | A tier's `yield_bps` is outside the `0..=10_000` bounds. | Ensure every tier's yield is between 0 and 10,000 basis points. |
| 11 | `TierYieldBelowBase` | `init` | A tier's `yield_bps` is less than the escrow's base `yield_bps`. | Ensure every tier offers a yield greater than or equal to the base yield. |
| 12 | `TierLockNotIncreasing` | `init` | Tiers in the table do not have strictly increasing `min_lock_secs`. | Sort the tier table by `min_lock_secs` in ascending order and avoid duplicates. |
| 13 | `TierYieldNotNonDecreasing` | `init` | A tier's `yield_bps` is lower than the preceding tier's yield. | Ensure tier yields never decrease as `min_lock_secs` increases. |
| 108 | `TieredSecondDeposit` | `fund_with_commitment` | An investor already has principal and calls `fund_with_commitment` again. | Yield-tiers are only selected on the *first* deposit. Use `fund()` for subsequent deposits. |
| 111 | `CommitmentLockExceedsMaturity` | `fund_with_commitment` | The `committed_lock_secs` extends beyond the escrow's maturity timestamp. | Ensure the lock duration fits within the remaining time until maturity. |
| 128 | `InvestorCommitmentLockNotExpired` | `claim_investor_payout` | An investor attempts to claim payout before their tier's lock period expires. | Wait until the lock period expires before claiming the payout. |

See also [Escrow Error Messages](escrow-error-messages.md) for the full code reference.
