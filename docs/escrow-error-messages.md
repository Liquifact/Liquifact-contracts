# Liquifact Escrow Typed Error Codes

LiquiFact escrow emits typed Soroban contract errors through [`EscrowError`](../escrow/src/lib.rs).
Client SDKs **must branch on the numeric `ContractError(code)` value**, not on panic strings or
diagnostic text.

## Stability Policy

Error codes are **append-only**. Once a code is assigned:

- It must **not** be renamed for a different meaning.
- It must **not** be reused after removal of a variant.
- It must **not** be renumbered.

New failures receive new codes **after** the highest code in their range group (or in a new range).
Intentional numeric **gaps** between groups (e.g. 14–19, 23–29) are reserved for future codes in
that domain without renumbering existing SDK mappings.

Legacy panic strings listed in the reference table are **migration aids only** — they may differ
from typed error text and must not be used for production branching logic.

## Range-Group Convention

Codes are grouped by domain so SDKs can map coarse categories without parsing variant names:

| Group | Span | Purpose | Boundary codes |
| --- | --- | --- | --- |
| Init / pricing | 1–13 | Initialization, invoice id, yield tiers, optional caps | 1, 13 |
| Uninitialized metadata | 20–22 | Escrow or required addresses not configured | 20, 22 |
| Dust sweep + SEP-41 safety | 30–42 | Terminal dust sweep and token transfer invariants | 30, 42 |
| Attestation | 50–51 | Primary hash binding and append-only digest log | 50, 51 |
| SME collateral | 60–62 | Off-chain collateral metadata record | 60, 62 |
| Admin validation | 70–80 | Allowlist batch, funding target, investor cap, maturity, admin handover | 70, 80 |
| Schema migration | 90–92 | `migrate` version checks | 90, 92 |
| Funding | 100–111 | Investor deposits, batch funding, and contribution limits | 100, 111 |
| Funding batch | 82–83 | [`fund_batch`] entry count bounds | 82, 83 |
| Settlement / payout | 120–129 | Settle, withdraw, investor claims, payout math | 120, 129 |
| Cancel / refund | 140–143 | Cancel funding and investor refunds | 140, 143 |
| Legal-hold clear (two-phase) | 150–152 | Delayed compliance-hold lift workflow | 150, 152 |
| Beneficiary rotation | 160–162 | Governed SME address rotation | 160, 162 |
| Admin handover / funding deadline | 163–164 | `accept_admin` and post-deadline funding | 163, 164 |

See also [`docs/escrow-legal-hold.md`](escrow-legal-hold.md),
[`docs/ESCROW_BENEFICIARY_ROTATION.md`](ESCROW_BENEFICIARY_ROTATION.md), and
[`docs/adr/ADR-006-dust-sweep-and-token-safety.md`](adr/ADR-006-dust-sweep-and-token-safety.md).

## Canonical Reference Table

| Code | Variant | Entrypoint(s) | Trigger | Recommended client action | Emission |
| ---: | --- | --- | --- | --- | --- |
| 1 | `AmountMustBePositive` | `init` | `amount <= 0` | Reject input; show invalid invoice amount | typed |
| 2 | `YieldBpsOutOfRange` | `init` | `yield_bps` not in `0..=10_000` | Fix yield configuration | typed |
| 3 | `EscrowAlreadyInitialized` | `init` | `DataKey::Escrow` already exists | Do not call `init` again; use read APIs | typed |
| 4 | `InvoiceIdInvalidLength` | `init` | `invoice_id` length outside `1..=MAX_INVOICE_ID_STRING_LEN` | Fix invoice id length | typed |
| 5 | `InvoiceIdInvalidCharset` | `init` | `invoice_id` contains characters outside `[A-Za-z0-9_]` | Fix invoice id charset | typed |
| 6 | `MinContributionNotPositive` | `init` | `min_contribution` configured but `<= 0` | Omit or set a positive floor | typed |
| 7 | `MinContributionExceedsAmount` | `init` | `min_contribution > amount` (target hint) | Lower floor or raise target | typed |
| 8 | `MaxUniqueInvestorsNotPositive` | `init` | `max_unique_investors` configured but `<= 0` | Omit or set a positive cap | typed |
| 9 | `MaxPerInvestorNotPositive` | `init` | `max_per_investor` configured but `<= 0` | Omit or set a positive cap | typed |
| 10 | `TierYieldOutOfRange` | `init` | tier `yield_bps` not in `0..=10_000` | Fix tier table | typed |
| 11 | `TierYieldBelowBase` | `init` | tier `yield_bps < base yield_bps` | Raise tier yield or lower base | typed |
| 12 | `TierLockNotIncreasing` | `init` | tier `min_lock_secs` not strictly increasing | Sort tiers by lock duration | typed |
| 13 | `TierYieldNotNonDecreasing` | `init` | tier `yield_bps` decreases across tiers | Ensure non-decreasing tier yields | typed |
| 20 | `EscrowNotInitialized` | `get_escrow`, `load_escrow_require_admin`, `load_escrow_require_sme`, most entrypoints | `DataKey::Escrow` missing | Call `init` first | typed |
| 21 | `FundingTokenNotSet` | `get_funding_token`, `sweep_terminal_dust`, `refund` | `DataKey::FundingToken` missing | Complete initialization | typed |
| 22 | `TreasuryNotSet` | `get_treasury`, `sweep_terminal_dust` | `DataKey::Treasury` missing | Complete initialization | typed |
| 30 | `LegalHoldBlocksTreasuryDustSweep` | `sweep_terminal_dust` | legal hold active | Complete legal-hold clear workflow before sweep | typed |
| 31 | `SweepAmountNotPositive` | `sweep_terminal_dust` | `amount <= 0` | Pass a positive sweep amount | typed |
| 32 | `SweepAmountExceedsMax` | `sweep_terminal_dust` | `amount > MAX_DUST_SWEEP_AMOUNT` | Reduce per-call sweep size | typed |
| 33 | `DustSweepNotTerminal` | `sweep_terminal_dust` | escrow status not terminal (`settled`, `withdrawn`, or `cancelled`) | Wait until terminal state | typed |
| 34 | `NoFundingTokenBalanceToSweep` | `sweep_terminal_dust` | contract token balance `<= 0` | Nothing to sweep; verify token balance | typed |
| 35 | `EffectiveSweepAmountZero` | `sweep_terminal_dust` | `min(amount, balance) == 0` | Adjust amount or wait for balance | typed |
| 36 | `TransferAmountNotPositive` | `transfer_funding_token_with_balance_checks` (via `sweep_terminal_dust`, `refund`) | `amount <= 0` | Fix transfer amount | typed |
| 37 | `InsufficientTokenBalanceBeforeTransfer` | `transfer_funding_token_with_balance_checks` | sender balance `< amount` | Insufficient escrow balance | typed |
| 38 | `SenderBalanceUnderflow` | `transfer_funding_token_with_balance_checks` | post-transfer sender delta underflows | Token non-compliant; abort integration | typed |
| 39 | `RecipientBalanceUnderflow` | `transfer_funding_token_with_balance_checks` | post-transfer recipient delta underflows | Token non-compliant; abort integration | typed |
| 40 | `SenderBalanceDeltaMismatch` | `transfer_funding_token_with_balance_checks` | sender spent `!= amount` | Token fee/hook detected; use allowlisted token | typed |
| 41 | `RecipientBalanceDeltaMismatch` | `transfer_funding_token_with_balance_checks` | recipient received `!= amount` | Token fee/hook detected; use allowlisted token | typed |
| 42 | `SweepExceedsLiabilityFloor` | `sweep_terminal_dust` | `balance - sweep_amt < funded_amount - distributed_principal` | Reduce sweep; wait until liabilities refunded | typed |
| 50 | `PrimaryAttestationAlreadyBound` | `bind_primary_attestation_hash` | primary hash already stored | Use `append_attestation_digest` for updates | typed |
| 51 | `AttestationAppendLogCapacityReached` | `append_attestation_digest` | log length `>= MAX_ATTESTATION_APPEND_ENTRIES` | Archive off-chain; log is bounded | typed |
| 60 | `CollateralAmountNotPositive` | `record_sme_collateral_commitment` | `amount <= 0` | Provide positive metadata amount | typed |
| 61 | `CollateralAssetEmpty` | `record_sme_collateral_commitment` | asset symbol empty | Provide non-empty asset label | typed |
| 62 | `CollateralTimestampBackwards` | `record_sme_collateral_commitment` | new timestamp `<` stored timestamp | Use monotonic timestamps | typed |
| 70 | `InvestorBatchEmpty` | `set_investors_allowlisted` | `investors.len() == 0` | Pass at least one address | typed |
| 71 | `InvestorBatchTooLarge` | `set_investors_allowlisted` | `investors.len() > MAX_INVESTOR_ALLOWLIST_BATCH` | Split into smaller batches | typed |
| 72 | `TargetNotPositive` | `update_funding_target` | `new_target <= 0` | Set a positive target | typed |
| 73 | `TargetUpdateNotOpen` | `update_funding_target` | escrow status `!= 0` (open) | Only update while open | typed |
| 74 | `TargetBelowFundedAmount` | `update_funding_target` | `new_target < funded_amount` | Target must cover already-funded principal | typed |
| 75 | `CapLowerNotOpen` | `lower_max_unique_investors` | escrow status `!= 0` | Only lower cap while open | typed |
| 76 | `NoInvestorCapConfigured` | `lower_max_unique_investors` | no `max_unique_investors` configured | Configure cap at init first | typed |
| 77 | `NewCapNotLower` | `lower_max_unique_investors` | `new_cap >= current cap` | Pass a strictly lower cap | typed |
| 78 | `NewCapBelowCurrentFunderCount` | `lower_max_unique_investors` | `new_cap < unique funder count` | Cap cannot evict existing funders | typed |
| 79 | `MaturityUpdateNotOpen` | `update_maturity` | escrow status `!= 0` | Only update maturity while open | typed |
| 80 | `NewAdminSameAsCurrent` | `propose_admin` | proposed admin equals current admin | Nominate a different admin | typed |
| 82 | `FundingBatchEmpty` | `fund_batch` | `entries.len() == 0` | Pass at least one `(investor, amount)` pair | typed |
| 83 | `FundingBatchTooLarge` | `fund_batch` | `entries.len() > MAX_FUND_BATCH` | Split into smaller batches | typed |
| 90 | `MigrationVersionMismatch` | `migrate` | stored version `!= from_version` | Pass matching `from_version` | typed |
| 91 | `AlreadyCurrentSchemaVersion` | `migrate` | `from_version >= SCHEMA_VERSION` | No migration needed | typed |
| 92 | `NoMigrationPath` | `migrate` | `from_version < SCHEMA_VERSION` and no transform implemented | Redeploy or extend `migrate` | typed |
| 100 | `FundingAmountNotPositive` | `fund`, `fund_with_commitment` | `amount <= 0` | Pass positive funding amount | typed |
| 101 | `FundingBelowMinContribution` | `fund`, `fund_with_commitment` | `amount < min_contribution` | Increase deposit to meet floor | typed |
| 102 | `LegalHoldBlocksFunding` | `fund`, `fund_with_commitment` | legal hold active | Complete legal-hold clear workflow | typed |
| 103 | `EscrowNotOpenForFunding` | `fund`, `fund_with_commitment` | escrow status `!= 0` | Funding closed; check lifecycle state | typed |
| 104 | `InvestorNotAllowlisted` | `fund`, `fund_with_commitment` | allowlist active and investor not allowlisted | Add investor to allowlist | typed |
| 105 | `InvestorContributionOverflow` | `fund`, `fund_with_commitment` | investor contribution addition overflows | Reduce deposit size | typed |
| 106 | `InvestorContributionExceedsCap` | `fund`, `fund_with_commitment` | contribution exceeds `max_per_investor` | Reduce deposit or raise cap at init | typed |
| 107 | `UniqueInvestorCapReached` | `fund`, `fund_with_commitment` | new investor and `unique funder count >= max_unique_investors` | Cap reached; wait or use existing investor | typed |
| 108 | `TieredSecondDeposit` | `fund_with_commitment` | investor already has principal and calls `fund_with_commitment` again | Use `fund()` for additional principal | typed |
| 109 | `InvestorClaimTimeOverflow` | `fund_with_commitment` | `timestamp + lock_secs` overflows | Reduce lock duration | typed |
| 110 | `FundedAmountOverflow` | `fund`, `fund_with_commitment`, `fund_batch` | `funded_amount + amount` overflows | Reduce deposit size | typed |
| 111 | `CommitmentLockExceedsMaturity` | `fund_with_commitment` | `now + committed_lock_secs > maturity` (when maturity > 0) | Shorten lock or extend maturity before deposit | typed |
| 120 | `LegalHoldBlocksSettlement` | `settle` | legal hold active | Complete legal-hold clear workflow | typed |
| 121 | `SettlementNotFunded` | `settle` | escrow status `!= 1` (funded) | Fund escrow before settlement | typed |
| 122 | `MaturityNotReached` | `settle` | `ledger.timestamp() < maturity` (when maturity > 0) | Wait until maturity timestamp | typed |
| 123 | `LegalHoldBlocksWithdrawal` | `withdraw` | legal hold active | Complete legal-hold clear workflow | typed |
| 124 | `WithdrawalNotFunded` | `withdraw` | escrow status `!= 1` | Fund escrow before withdrawal | typed |
| 125 | `LegalHoldBlocksInvestorClaims` | `claim_investor_payout` | legal hold active | Complete legal-hold clear workflow | typed |
| 126 | `NoContributionToClaim` | `claim_investor_payout` | investor contribution `== 0` | Caller is not a funder | typed |
| 127 | `InvestorClaimNotSettled` | `claim_investor_payout` | escrow not settled | Wait for settlement | typed |
| 128 | `InvestorCommitmentLockNotExpired` | `claim_investor_payout` | `ledger.timestamp() < claim_not_before` | Wait for tier lock to expire | typed |
| 129 | `ComputePayoutArithmeticOverflow` | `compute_investor_payout`, `claim_investor_payout` | checked multiply/divide overflow in payout math | Escalate; values exceed safe range | typed |
| 140 | `LegalHoldBlocksCancelFunding` | `cancel_funding` | legal hold active | Complete legal-hold clear workflow | typed |
| 141 | `CancelFundingNotOpen` | `cancel_funding` | escrow status `!= 0` | Only cancel while open | typed |
| 142 | `RefundNotCancelled` | `refund` | escrow status `!= 4` (cancelled) | Cancel funding first | typed |
| 143 | `NoContributionToRefund` | `refund` | investor contribution `== 0` | Caller has nothing to refund | typed |
| 150 | `LegalHoldClearRequestMissing` | `set_legal_hold(false)`, `clear_legal_hold` | clearing with non-zero delay but no prior `request_clear_legal_hold` | Call `request_clear_legal_hold` first | typed |
| 151 | `LegalHoldClearNotReady` | `set_legal_hold(false)`, `clear_legal_hold` | `ledger.timestamp() < clearable_at` | Wait until clear delay elapses | typed |
| 152 | `LegalHoldClearDelayOverflow` | `request_clear_legal_hold` | `timestamp + delay` overflows `u64` | Reduce delay or timestamp | typed |
| 160 | `LegalHoldBlocksBeneficiaryRotation` | `rotate_beneficiary` | legal hold active | Clear hold before rotation | typed |
| 161 | `RotationNotOpen` | `rotate_beneficiary` | status not `0` (open) or `1` (funded) | Rotation only before settlement | typed |
| 162 | `NewSmeSameAsCurrent` | `rotate_beneficiary` | `new_sme == current sme_address` | Pass a different beneficiary | typed |
| 163 | `NoPendingAdmin` | `accept_admin` | no pending admin nomination stored | Call `propose_admin` first | typed |
| 164 | `FundingDeadlinePassed` | `init`, `fund`, `fund_with_commitment`, `fund_batch` | `funding_deadline` configured and `ledger.timestamp()` past deadline | Funding window closed; do not retry deposits | typed |

### Legacy panic strings (migration aid)

| Code | Legacy failure |
| ---: | --- |
| 1 | `Amount must be positive` |
| 2 | `yield_bps must be between 0 and 10_000` |
| 3 | `Escrow already initialized` |
| 4 | `invoice_id length must be 1..=MAX_INVOICE_ID_STRING_LEN` |
| 5 | `invoice_id must be [A-Za-z0-9_] only` |
| 6 | `min_contribution must be positive when configured` |
| 7 | `min_contribution cannot exceed initial invoice amount / target hint` |
| 8 | `max_unique_investors must be positive when configured` |
| 9 | `max_per_investor must be positive when configured` |
| 10 | `tier yield_bps must be 0..=10_000` |
| 11 | `tier yield_bps must be >= base yield_bps` |
| 12 | `tiers must have strictly increasing min_lock_secs` |
| 13 | `tiers must have non-decreasing yield_bps` |
| 20 | `Escrow not initialized` |
| 21 | `Funding token not set` |
| 22 | `Treasury not set` |
| 30 | `Legal hold blocks treasury dust sweep` |
| 31 | `sweep amount must be positive` |
| 32 | `sweep amount exceeds MAX_DUST_SWEEP_AMOUNT` |
| 33 | `dust sweep only in terminal states` |
| 34 | `no funding token balance to sweep` |
| 35 | `effective sweep amount is zero` |
| 36 | `transfer amount must be positive` |
| 37 | `insufficient token balance before transfer` |
| 38 | `balance underflow on sender` |
| 39 | `balance underflow on recipient` |
| 40 | `sender balance delta must equal transfer amount` |
| 41 | `recipient balance delta must equal transfer amount` |
| 42 | `sweep would exceed liability floor` |
| 50 | `primary attestation already bound` |
| 51 | `attestation append log capacity reached` |
| 60 | `Collateral amount must be positive` |
| 61 | `Collateral asset symbol must not be empty` |
| 62 | `Collateral commitment timestamp must not go backward` |
| 70 | `investors vector must be non-empty` |
| 71 | `investors vector length exceeds MAX_INVESTOR_ALLOWLIST_BATCH` |
| 72 | `Target must be strictly positive` |
| 73 | `Target can only be updated in Open state` |
| 74 | `Target cannot be less than already funded amount` |
| 75 | `Cap can only be lowered in Open state` |
| 76 | `no investor cap configured` |
| 77 | `new cap must be strictly lower than current cap` |
| 78 | `new cap cannot be below current unique funder count` |
| 79 | `Maturity can only be updated in Open state` |
| 80 | `New admin must differ from current admin` |
| 82 | `fund_batch entries vector must be non-empty` |
| 83 | `fund_batch entries vector length exceeds MAX_FUND_BATCH` |
| 90 | `from_version does not match stored version` |
| 91 | `Already at current schema version` |
| 92 | `No migration path from version 0 - extend migrate or redeploy` |
| 100 | `Funding amount must be positive` |
| 101 | `funding amount below min_contribution floor` |
| 102 | `Legal hold blocks new funding while active` |
| 103 | `Escrow not open for funding` |
| 104 | `Investor not on allowlist` |
| 105 | `investor contribution overflow` |
| 106 | `investor contribution exceeds max_per_investor cap` |
| 107 | `unique investor cap reached` |
| 108 | `Additional principal after a tiered first deposit must use fund()` |
| 109 | `investor claim time overflow` |
| 110 | `funded_amount overflow` |
| 111 | `commitment lock exceeds escrow maturity` |
| 120 | `Legal hold blocks settlement finalization` |
| 121 | `Escrow must be funded before settlement` |
| 122 | `Escrow has not yet reached maturity` |
| 123 | `Legal hold blocks SME withdrawal` |
| 124 | `Escrow must be funded before withdrawal` |
| 125 | `Legal hold blocks investor claims` |
| 126 | `Address has no contribution to claim` |
| 127 | `Escrow must be settled before investor claim` |
| 128 | `Investor commitment lock not expired` |
| 129 | `compute_investor_payout: arithmetic overflow` |
| 140 | `Legal hold blocks cancel_funding` |
| 141 | `cancel_funding only allowed in Open state` |
| 142 | `refund only allowed in Cancelled state` |
| 143 | `no contribution to refund` |
| 150 | `legal hold clear requested but clearable_at not set` |
| 151 | `legal hold clear delay has not elapsed` |
| 152 | `legal hold clear delay overflow` |
| 160 | `Legal hold blocks beneficiary rotation` |
| 161 | `Beneficiary rotation not permitted in current escrow state` |
| 162 | `New SME address must differ from current beneficiary` |
| 163 | `No pending admin` |
| 164 | `Funding deadline has passed` |

## Client Guidance

In tests and SDK simulations, `try_*` clients surface typed traps as contract errors. For example,
`FundingAmountNotPositive` is observable as `ContractError(100)` / `Error(Contract, #100)`.

Recommended SDK category mappings:

| Codes | Suggested client category |
| --- | --- |
| 1–13 | Invalid initialization or pricing configuration |
| 20–22 | Missing initialized escrow metadata |
| 30–42 | Dust sweep or token integration failure |
| 50–51 | Attestation failure |
| 60–62 | Collateral metadata failure |
| 70–80, 82–83 | Administrative validation or batch-funding bounds failure |
| 90–92 | Migration failure |
| 100–111 | Funding failure |
| 163 | Admin handover not pending |
| 164 | Funding deadline expired |
| 120–129 | Settlement, withdrawal, or investor payout failure |
| 140–143 | Cancellation or refund failure |
| 150–152 | Legal-hold clear workflow failure |
| 160–162 | Beneficiary rotation failure |

## Security Notes

- **Code stability:** numeric codes are append-only; SDKs must branch on `ContractError(code)`.
  Never depend on panic string text in production paths.
- **Auth boundaries:** typed errors do not replace `require_auth`. Authorization failures remain
  separate from contract error codes (see ADR-002).
- **Overflow safety:** funding, payout, and timestamp paths use checked arithmetic; each overflow
  maps to a stable code (105, 109, 110, 129, 152).
- **Token boundary:** codes 36–41 and 42 enforce SEP-41 conservation and liability floors at the
  external token boundary. Non-compliant tokens fail closed.
- **Refund CEI:** `refund` zeroes contribution before transfer (code 143 on repeat). Investor payout
  remains idempotent after the claim marker is written.
- **Legal-hold clear:** codes 150–152 enforce the two-phase clear workflow when a non-zero delay is
  configured at init.
- **Beneficiary rotation:** codes 160–162 require dual SME+admin auth and pre-settlement state;
  rotation is blocked under legal hold.
- **Storage TTL:** error migration does not change `bump_ttl` behavior for instance or persistent
  allowlist storage.
