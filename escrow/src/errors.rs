
use soroban_sdk::contracterror;

/// Stable typed errors emitted by LiquiFact escrow entrypoints.
///
/// Codes are append-only: never reuse or renumber a variant.
/// Client SDKs should branch on the numeric code rather than legacy panic strings.
/// See `docs/escrow-error-messages.md`.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u32)]
pub enum EscrowError {
    // -------------------------------------------------------------------------
    // Initialization & State Errors (1..19)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::init`] rejected a non-positive invoice amount.
    AmountMustBePositive = 1,
    /// [`LiquifactEscrow::init`] rejected `yield_bps` outside `0..=10_000`.
    YieldBpsOutOfRange = 2,
    /// [`LiquifactEscrow::init`] called when escrow storage already exists.
    ///
    /// Returned for every second initialization attempt — same parameters, a different
    /// admin, a different token, or a re-entrant initialization during `init` — before
    /// any state mutation or event emission. Existing admin, token metadata, and escrow
    /// state are left unchanged.
    EscrowAlreadyInitialized = 3,
    /// [`LiquifactEscrow::init`] rejected an `invoice_id` outside the allowed length range.
    InvoiceIdInvalidLength = 4,
    /// [`LiquifactEscrow::init`] rejected an `invoice_id` with disallowed characters.
    InvoiceIdInvalidCharset = 5,
    /// [`LiquifactEscrow::init`] configured `min_contribution` but it is not positive.
    MinContributionNotPositive = 6,
    /// [`LiquifactEscrow::init`] configured `min_contribution` above the target hint.
    MinContributionExceedsAmount = 7,
    /// [`LiquifactEscrow::init`] configured `max_unique_investors` but it is not positive.
    MaxUniqueInvestorsNotPositive = 8,
    /// [`LiquifactEscrow::init`] configured `max_per_investor` but it is not positive.
    MaxPerInvestorNotPositive = 9,
    /// [`LiquifactEscrow::init`] rejected a tier with `yield_bps` outside `0..=10_000`.
    TierYieldOutOfRange = 10,
    /// [`LiquifactEscrow::init`] rejected a tier yield below the base `yield_bps`.
    TierYieldBelowBase = 11,
    /// [`LiquifactEscrow::init`] rejected tiers whose `min_lock_secs` are not strictly increasing.
    TierLockNotIncreasing = 12,
    /// [`LiquifactEscrow::init`] rejected tiers whose `yield_bps` decrease across tiers.
    TierYieldNotNonDecreasing = 13,
    /// [`LiquifactEscrow::init`] rejected an invoice amount too large to keep
    /// `compute_investor_payout` arithmetic overflow-free.
    AmountExceedsMax = 14,

    // -------------------------------------------------------------------------
    // Initialization Guards (20..29)
    // -------------------------------------------------------------------------
    /// Escrow storage is missing; entrypoint requires prior [`LiquifactEscrow::init`].
    EscrowNotInitialized = 20,
    /// [`DataKey::FundingToken`] is unset (escrow not fully initialized).
    FundingTokenNotSet = 21,
    /// [`DataKey::Treasury`] is unset (escrow not fully initialized).
    TreasuryNotSet = 22,

    // -------------------------------------------------------------------------
    // Terminal Dust Sweep & Safety Wrappers (30..49)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::sweep_terminal_dust`] blocked while a legal hold is active.
    LegalHoldBlocksTreasuryDustSweep = 30,
    /// [`LiquifactEscrow::sweep_terminal_dust`] received a non-positive sweep amount.
    SweepAmountNotPositive = 31,
    /// [`LiquifactEscrow::sweep_terminal_dust`] exceeded [`MAX_DUST_SWEEP_AMOUNT`].
    SweepAmountExceedsMax = 32,
    /// [`LiquifactEscrow::sweep_terminal_dust`] called before a terminal escrow status.
    DustSweepNotTerminal = 33,
    /// [`LiquifactEscrow::sweep_terminal_dust`] found no funding-token balance to sweep.
    NoFundingTokenBalanceToSweep = 34,
    /// [`LiquifactEscrow::sweep_terminal_dust`] computed an effective sweep amount of zero.
    EffectiveSweepAmountZero = 35,
    /// Token transfer wrapper received a non-positive amount (see `external_calls`).
    TransferAmountNotPositive = 36,
    /// Token transfer wrapper found insufficient sender balance before transfer.
    InsufficientTokenBalanceBeforeTransfer = 37,
    /// Token transfer wrapper detected sender balance delta underflow.
    SenderBalanceUnderflow = 38,
    /// Token transfer wrapper detected recipient balance delta underflow.
    RecipientBalanceUnderflow = 39,
    /// Token transfer wrapper detected sender spent amount differs from requested transfer.
    SenderBalanceDeltaMismatch = 40,
    /// Token transfer wrapper detected recipient received amount differs from requested transfer.
    RecipientBalanceDeltaMismatch = 41,
    /// Sweep would reduce the contract balance below outstanding investor liabilities.
    SweepExceedsLiabilityFloor = 42,

    // -------------------------------------------------------------------------
    // Attestation Digest Registry (50..59)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::bind_primary_attestation_hash`] called when a primary hash exists.
    PrimaryAttestationAlreadyBound = 50,
    /// [`LiquifactEscrow::append_attestation_digest`] exceeded [`MAX_ATTESTATION_APPEND_ENTRIES`].
    AttestationAppendLogCapacityReached = 51,
    /// [`LiquifactEscrow::revoke_attestation_digest`] received an `index >= log.len()`.
    AttestationIndexOutOfRange = 52,
    /// [`LiquifactEscrow::revoke_attestation_digest`] called on an already-revoked index.
    AttestationAlreadyRevoked = 53,
    /// [`LiquifactEscrow::revoke_attestation_digests`] received an empty indices list.
    AttestationBatchEmpty = 54,
    /// [`LiquifactEscrow::revoke_attestation_digests`] exceeded [`MAX_ATTESTATION_REVOKE_BATCH`].
    AttestationBatchTooLarge = 55,
    /// [`LiquifactEscrow::unrevoke_attestation_digest`] called on an index that is not revoked.
    AttestationNotRevoked = 56,
    /// [`LiquifactEscrow::get_revoked_attestation_digests`] received a zero page limit.
    AttestationReadLimitZero = 57,
    /// [`LiquifactEscrow::get_revoked_attestation_digests`] exceeded [`MAX_ATTESTATION_READ_PAGE`].
    AttestationReadLimitTooLarge = 58,

    // -------------------------------------------------------------------------
    // SME Collateral Records (60..69)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::record_sme_collateral_commitment`] received a non-positive amount.
    CollateralAmountNotPositive = 60,
    /// [`LiquifactEscrow::record_sme_collateral_commitment`] received an empty asset symbol.
    CollateralAssetEmpty = 61,
    /// [`LiquifactEscrow::record_sme_collateral_commitment`] received a timestamp before stored record.
    CollateralTimestampBackwards = 62,
    /// [`LiquifactEscrow::clear_sme_collateral_commitment`] called when no pledge exists.
    NoCollateralToClear = 63,

    // -------------------------------------------------------------------------
    // Allowlist, Caps & Batches (70..89)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::set_investors_allowlisted`] received an empty batch.
    InvestorBatchEmpty = 70,
    /// [`LiquifactEscrow::set_investors_allowlisted`] exceeded [`MAX_INVESTOR_ALLOWLIST_BATCH`].
    InvestorBatchTooLarge = 71,
    /// [`LiquifactEscrow::update_funding_target`] received a non-positive target.
    TargetNotPositive = 72,
    /// [`LiquifactEscrow::update_funding_target`] called while escrow is not open.
    TargetUpdateNotOpen = 73,
    /// [`LiquifactEscrow::update_funding_target`] set target below already-funded principal.
    TargetBelowFundedAmount = 74,
    /// [`LiquifactEscrow::lower_max_unique_investors`] called while escrow is not open.
    CapLowerNotOpen = 75,
    /// [`LiquifactEscrow::lower_max_unique_investors`] called with no investor cap configured.
    NoInvestorCapConfigured = 76,
    /// [`LiquifactEscrow::lower_max_unique_investors`] did not strictly lower the cap.
    NewCapNotLower = 77,
    /// [`LiquifactEscrow::lower_max_unique_investors`] set cap below current unique funder count.
    NewCapBelowCurrentFunderCount = 78,
    /// [`LiquifactEscrow::update_maturity`] called while escrow is not open.
    MaturityUpdateNotOpen = 79,
    /// [`LiquifactEscrow::propose_admin`] nominated the current admin address.
    NewAdminSameAsCurrent = 80,
    /// [`LiquifactEscrow::update_maturity`] set maturity to the same value as current.
    MaturityUnchanged = 81,
    /// [`LiquifactEscrow::fund_batch`] received an empty entries vector.
    FundingBatchEmpty = 82,
    /// [`LiquifactEscrow::fund_batch`] exceeded [`MAX_FUND_BATCH`].
    FundingBatchTooLarge = 83,
    /// [`LiquifactEscrow::fund_batch`] contains two or more entries with the same investor address.
    FundingBatchDuplicateInvestor = 84,
    /// [`LiquifactEscrow::accept_admin`] called after proposal expiry.
    AdminProposalExpired = 85,
    /// Attempted to accept admin role when no pending admin exists.
    NoPendingAdmin = 86,
    /// Admin-nonce replay protection: the supplied nonce does not match current expected nonce.
    AdminNonceMismatch = 87,

    // -------------------------------------------------------------------------
    // Migration & Upgrades (90..99)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::migrate`] `from_version` does not match stored version.
    MigrationVersionMismatch = 90,
    /// [`LiquifactEscrow::migrate`] called at or above [`SCHEMA_VERSION`].
    AlreadyCurrentSchemaVersion = 91,
    /// [`LiquifactEscrow::migrate`] has no implemented path from requested version.
    NoMigrationPath = 92,

    // -------------------------------------------------------------------------
    // Funding & Deposit Operations (100..119)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::fund`] / [`LiquifactEscrow::fund_with_commitment`] received non-positive amount.
    FundingAmountNotPositive = 100,
    /// Funding amount is below configured `min_contribution`.
    FundingBelowMinContribution = 101,
    /// Funding blocked while a legal hold is active.
    LegalHoldBlocksFunding = 102,
    /// Funding attempted while escrow is not in open status.
    EscrowNotOpenForFunding = 103,
    /// Allowlist gate active and investor address is not allowlisted.
    InvestorNotAllowlisted = 104,
    /// Adding funding would overflow the investor's stored contribution.
    InvestorContributionOverflow = 105,
    /// Funding would exceed configured `max_per_investor`.
    InvestorContributionExceedsCap = 106,
    /// A new investor would exceed configured `max_unique_investors`.
    UniqueInvestorCapReached = 107,
    /// [`LiquifactEscrow::fund_with_commitment`] called after investor already has principal.
    TieredSecondDeposit = 108,
    /// Computing investor claim-not-before timestamp would overflow.
    InvestorClaimTimeOverflow = 109,
    /// Adding funding would overflow escrow `funded_amount`.
    FundedAmountOverflow = 110,
    /// Commitment lock would push `now + committed_lock_secs` past escrow maturity.
    CommitmentLockExceedsMaturity = 111,

    // -------------------------------------------------------------------------
    // Settlement, Withdrawal & Claims (120..139)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::settle`] blocked while a legal hold is active.
    LegalHoldBlocksSettlement = 120,
    /// [`LiquifactEscrow::settle`] called before escrow reached funded status.
    SettlementNotFunded = 121,
    /// [`LiquifactEscrow::settle`] called before configured maturity timestamp.
    MaturityNotReached = 122,
    /// [`LiquifactEscrow::withdraw`] blocked while a legal hold is active.
    LegalHoldBlocksWithdrawal = 123,
    /// [`LiquifactEscrow::withdraw`] called before escrow reached funded status.
    WithdrawalNotFunded = 124,
    /// [`LiquifactEscrow::claim_investor_payout`] blocked while a legal hold is active.
    LegalHoldBlocksInvestorClaims = 125,
    /// [`LiquifactEscrow::claim_investor_payout`] for an address with zero contribution.
    NoContributionToClaim = 126,
    /// [`LiquifactEscrow::claim_investor_payout`] before escrow is settled.
    InvestorClaimNotSettled = 127,
    /// [`LiquifactEscrow::claim_investor_payout`] before tier commitment lock expires.
    InvestorCommitmentLockNotExpired = 128,
    /// Checked arithmetic overflow in [`LiquifactEscrow::compute_investor_payout`].
    ComputePayoutArithmeticOverflow = 129,

    // -------------------------------------------------------------------------
    // Cancellation & Refunds (140..149)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::cancel_funding`] blocked while a legal hold is active.
    LegalHoldBlocksCancelFunding = 140,
    /// [`LiquifactEscrow::cancel_funding`] called while escrow is not open.
    CancelFundingNotOpen = 141,
    /// [`LiquifactEscrow::refund`] called while escrow is not cancelled.
    RefundNotCancelled = 142,
    /// [`LiquifactEscrow::refund`] for an address with zero contribution.
    NoContributionToRefund = 143,
    /// [`LiquifactEscrow::refund_batch`] received an empty investors vector.
    RefundBatchEmpty = 144,
    /// [`LiquifactEscrow::refund_batch`] exceeded [`MAX_REFUND_BATCH`].
    RefundBatchTooLarge = 145,

    // -------------------------------------------------------------------------
    // Two-Phase Legal Hold & Admin Management (150..169)
    // -------------------------------------------------------------------------
    /// `clear_legal_hold` was called without a prior `request_legal_hold_clear`.
    LegalHoldClearRequestMissing = 150,
    /// The two-phase legal-hold clear delay has not elapsed yet.
    LegalHoldClearNotReady = 151,
    /// Computing the legal-hold clear ready-at timestamp would overflow.
    LegalHoldClearDelayOverflow = 152,
    /// A legal hold blocks rotating the beneficiary (SME) address.
    LegalHoldBlocksBeneficiaryRotation = 160,
    /// Beneficiary rotation attempted while escrow not in pre-settlement status.
    RotationNotOpen = 161,
    /// The proposed new SME address is identical to the current beneficiary.
    NewSmeSameAsCurrent = 162,
    /// Funding deadline has passed, new deposits are rejected.
    FundingDeadlinePassed = 164,
    /// Contract's funding-token balance is less than `funded_amount` at withdraw time.
    InsufficientContractBalance = 165,
    /// [`validate_maturity_bounds`] rejected a maturity timestamp in the past.
    MaturityInPast = 166,
    /// [`validate_maturity_bounds`] rejected a maturity timestamp beyond configured horizon.
    MaturityExceedsMaxHorizon = 167,
    /// [`LiquifactEscrow::update_funding_deadline`] called while escrow is not open.
    FundingDeadlineUpdateNotOpen = 169,

    // -------------------------------------------------------------------------
    // Inbound Transfer Guards & Payout Limits (170..199)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::claim_investor_payout`] computed a zero payout.
    PayoutZero = 170,
    /// Inbound token transfer received a non-positive amount.
    InboundTransferAmountNotPositive = 171,
    /// Inbound token transfer found insufficient sender balance before transfer.
    InboundInsufficientTokenBalanceBeforeTransfer = 172,
    /// Inbound token transfer detected sender balance delta underflow.
    InboundSenderBalanceUnderflow = 173,
    /// Inbound token transfer detected sender spent amount differs from requested transfer.
    InboundSenderBalanceDeltaMismatch = 174,
    /// Inbound token transfer detected recipient balance delta underflow.
    InboundRecipientBalanceUnderflow = 175,
    /// Inbound token transfer detected recipient received amount differs from requested transfer.
    InboundRecipientBalanceDeltaMismatch = 176,
    /// [`LiquifactEscrow::propose_admin`] repeated the already-pending admin address.
    PendingAdminUnchanged = 177,
    /// [`LiquifactEscrow::raise_max_unique_investors`] did not strictly raise the cap.
    NewCapNotHigher = 178,

    // -------------------------------------------------------------------------
    // Operational Pause (210..229)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::fund`] blocked while operational pause is active.
    PausedBlocksFunding = 210,
    /// [`LiquifactEscrow::settle`] blocked while operational pause is active.
    PausedBlocksSettlement = 211,
    /// [`LiquifactEscrow::withdraw`] blocked while operational pause is active.
    PausedBlocksWithdrawal = 212,
    /// [`LiquifactEscrow::claim_investor_payout`] blocked while operational pause is active.
    PausedBlocksInvestorClaims = 213,
    /// [`LiquifactEscrow::init`] rejected `protocol_fee_bps` outside `0..=10_000`.
    ProtocolFeeBpsOutOfRange = 215,
    /// Arithmetic overflow computing protocol fee at [`LiquifactEscrow::withdraw`].
    WithdrawFeeArithmeticOverflow = 216,
    /// Arithmetic underflow computing net SME payout at [`LiquifactEscrow::withdraw`].
    WithdrawNetArithmeticUnderflow = 217,
    /// [`LiquifactEscrow::init`] rejected a `funding_deadline` at or after maturity.
    FundingDeadlineAtOrAfterMaturity = 218,
    /// [`LiquifactEscrow::unfund`] called when [`InvoiceEscrow::status`] is not 0 (open).
    UnfundEscrowNotOpen = 220,
    /// [`LiquifactEscrow::unfund`] requested amount exceeds investor's recorded contribution.
    OverWithdrawal = 221,
    /// [`LiquifactEscrow::unfund`] blocked because compliance/legal hold is active.
    UnfundLegalHoldActive = 222,
    /// [`LiquifactEscrow::settle_batch`] received an empty escrow addresses vector.
    SettlementBatchEmpty = 223,
    /// [`LiquifactEscrow::settle_batch`] exceeded [`MAX_SETTLE_BATCH`].
    SettlementBatchTooLarge = 224,
    /// [`LiquifactEscrow::set_pause_rate_limit`] received window outside allowed bounds.
    PauseToggleWindowOutOfRange = 225,
    /// [`LiquifactEscrow::set_pause_rate_limit`] received inconsistent limit/window configuration.
    PauseRateLimitInvalidCombination = 226,
    /// [`LiquifactEscrow::set_paused`] blocked because configured toggle rate limit exceeded.
    PauseToggleRateLimitExceeded = 227,
    /// [`LiquifactEscrow::update_yield_bps`] called while escrow is not in open status.
    YieldBpsUpdateNotOpen = 228,
    /// [`LiquifactEscrow::update_yield_bps`] received a `new_yield_bps` equal to current value.
    YieldBpsUnchanged = 229,

    // -------------------------------------------------------------------------
    // Storage, TTL & Concurrency Hardening (230..239)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::set_pause_max_duration`] received value outside allowed bounds.
    PauseMaxDurationOutOfRange = 230,
    /// [`LiquifactEscrow::set_pause_rate_limit`] received limit outside allowed bounds.
    PauseToggleLimitOutOfRange = 231,
    /// [`LiquifactEscrow::set_storage_limit`] received a non-positive limit.
    StorageLimitNotPositive = 232,
    /// [`LiquifactEscrow::set_storage_limit`] received a limit outside allowed range.
    StorageLimitOutOfRange = 233,
    /// [`LiquifactEscrow::bump_ttl_batch`] received an empty escrow addresses vector.
    BumpTtlBatchEmpty = 234,
    /// [`LiquifactEscrow::bump_ttl_batch`] exceeded [`MAX_BUMP_TTL_BATCH`].
    BumpTtlBatchTooLarge = 235,
    /// A second [`LiquifactEscrow::settle`] was attempted on an already settled escrow.
    EscrowAlreadySettled = 236,
    /// A dispute is active and blocks value release from the escrow.
    DisputeBlocksWithdrawal = 237,
    /// Settlement is blocked while a dispute remains active.
    DisputeBlocksSettlement = 238,
    /// Investor claims are blocked while a dispute remains active.
    DisputeBlocksInvestorClaims = 239,

    // -------------------------------------------------------------------------
    // Cross-Contract Callbacks & Immutability (240..249)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::execute_callback`] called from unexpected origin address.
    CallbackWrongOrigin = 240,
    /// [`LiquifactEscrow::execute_callback`] called with mismatched nonce.
    CallbackWrongNonce = 241,
    /// [`LiquifactEscrow::execute_callback`] called with mismatched lifecycle phase.
    CallbackWrongPhase = 242,
    /// [`LiquifactEscrow::execute_callback`] called with already consumed context (replay attempt).
    CallbackReplayed = 243,
    /// Callback attempted after escrow was cancelled.
    CallbackAfterCancellation = 244,
    /// Callback nonce not found in registered context.
    CallbackNotFound = 245,
    /// Registry reference is immutable once funding begins.
    RegistryImmutableAfterFunding = 246,
    /// Beneficiary address is immutable once funding begins.
    BeneficiaryImmutableAfterFunding = 247,
    /// Admin recovery timelock has not expired.
    AdminRecoveryNotExpired = 248,
    /// Unconditional ceiling [`MAX_UNIQUE_INVESTORS`] reached.
    UniqueInvestorHardCapReached = 249,

    // -------------------------------------------------------------------------
    // Disputes & Releases (250..285)
    // -------------------------------------------------------------------------
    /// Partial-settlement is blocked while a dispute remains active.
    DisputeBlocksPartialSettle = 250,
    /// Refund processing is blocked while a dispute remains active.
    DisputeBlocksRefund = 251,
    /// Unfunding is blocked while a dispute remains active.
    DisputeBlocksUnfund = 252,
    /// Terminal dust sweep is blocked while a dispute remains active.
    DisputeBlocksSweep = 253,
    /// The caller is not authorized to open a dispute.
    DisputeOpenUnauthorized = 254,
    /// The caller is not authorized to close the active dispute.
    DisputeCloseUnauthorized = 255,
    /// A dispute has already been opened and is still active.
    DisputeAlreadyOpen = 256,
    /// No dispute is active for this escrow.
    DisputeNotOpen = 257,
    /// Release amount is not positive.
    ReleaseAmountNotPositive = 258,
    /// Release called when escrow is not in funded status.
    ReleaseNotFunded = 259,
    /// Release amount exceeds remaining unreleased principal.
    ReleaseExceedsRemaining = 260,
    /// Legal hold blocks principal release.
    LegalHoldBlocksRelease = 261,
    /// Operational pause blocks principal release.
    PausedBlocksRelease = 262,
    /// Decimal scale validation failed for token transfer amount.
    FundingTokenScaleInvalid = 263,
    /// Decimal scale was not configured at init.
    FundingTokenScaleNotSet = 264,
    /// Funding deadline could not be extended.
    FundingDeadlineNotExtended = 265,
    /// Maturity horizon not raised.
    HorizonNotRaised = 266,
    /// Lowering contribution floor called when escrow not open.
    FloorLowerNotOpen = 267,
    /// New contribution floor is not strictly lower.
    NewFloorNotLower = 268,
    /// New contribution floor is not positive.
    NewFloorNotPositive = 269,
    /// Max per investor cap was not configured.
    MaxPerInvestorCapNotConfigured = 270,
    /// Raising max per investor cap did not strictly raise the cap.
    MaxPerInvestorCapNotRaised = 271,
    /// Collateral batch is empty.
    CollateralBatchEmpty = 272,
    /// Collateral batch exceeds limit.
    CollateralBatchTooLarge = 273,
    /// Funding deadline must precede escrow maturity.
    FundingDeadlineBeyondMaturity = 274,
    /// Operational pause scope does not match requested action.
    PauseScopeMismatch = 275,
    /// Legal hold blocks payer rotation.
    LegalHoldBlocksPayerRotation = 276,
    /// Payer rotation called when escrow is not open.
    PayerRotationNotOpen = 277,
    /// Proposed new payer address is identical to current payer.
    NewPayerSameAsCurrent = 278,
    /// Caller unauthorized for partial settlement.
    PartialSettleUnauthorizedCaller = 279,
    /// Legal hold blocks partial settlement.
    LegalHoldBlocksPartialSettle = 280,
    /// Partial settlement called when escrow is not open.
    PartialSettleNotOpen = 281,
    /// [`LiquifactEscrow::get_contributions`] exceeded [`MAX_INVESTOR_READ_BATCH`].
    ContributionReadBatchTooLarge = 282,

    // -------------------------------------------------------------------------
    // Transfer self-transfer guards (283..284)
    // -------------------------------------------------------------------------
    /// Outbound token transfer rejected because sender and recipient are the same address.
    ///
    /// A self-transfer circumvents the balance-delta conservation model: the same address appears
    /// on both sides, so any balance mutation can be "explained away" while no net movement
    /// actually occurs. Checked before balance reads so the pre-flight invariant cannot be
    /// bypassed by supplying a zero or negative amount.
    TransferSameSenderRecipient = 283,
    /// Inbound token transfer rejected because sender and recipient are the same address.
    ///
    /// Same reasoning as [`TransferSameSenderRecipient`] but for the inbound
    /// (investor → escrow) leg.
    InboundTransferSameSenderRecipient = 284,
}
