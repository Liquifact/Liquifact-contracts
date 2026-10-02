
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

    // ------------------------------------------------------------------------------
    // Settlement & Bounds Validation Errors (100..109)
    // -------------------------------------------------------------------------------
    SettlementAmountInvalid = 100,
    MaturityNotReached = 101,
    EscrowNotInFundedState = 102,
    WithdrawAmountInvalid = 103,

    // ------------------------------------------------------------------------------
    // Legal Hold & Operational Pause (200..209)
    // ------------------------------------------------------------------------------
    LegalHoldActive = 200,
    ContractPaused = 201,

    // -------------------------------------------------------------------------------
    // SME Collateral Errors (300..309)
    // ------------------------------------------------------------------------------
    NoCollateralToClear = 300,

    // ------------------------------------------------------------------------------
    // Pause Configuration & Rate-Limit Errors (223..234)
    // ------------------------------------------------------------------------------
    /// `LiquifactEscrow::set_pause_max_duration` received a duration outside
    /// `MIN_PAUSE_MAX_DURATION_SECS`..=[`MAX_PAUSE_MAX_DURATION_SECS`]. Zero is always allowed.
    PauseMaxDurationOutOfRange = 223,
    /// `LiquifactEscrow::set_pause_rate_limit` received a toggle limit outside
    /// `MIN_PAUSE_TOGGLE_LIMIT`..=[`MAX_PAUSE_TOGGLE_LIMIT`]. Zero is allowed only with zero window.
    PauseToggleLimitOutOfRange = 224,
    /// `LiquifactEscrow::set_pause_rate_limit` received a window outside
    /// `MIN_PAUSE_TOGGLE_WINDOW_SECS`..=[`MAX_PAUSE_TOGGLE_WINDOW_SECS`]. Zero is allowed only with zero toggles.
    PauseToggleWindowOutOfRange = 225,
    /// `LiquifactEscrow::set_pause_rate_limit` received an inconsistent configuration:
    /// nonzero toggles must have a nonzero window, and nonzero window must have nonzero toggles.
    PauseRateLimitInvalidCombination = 226,
    /// `LiquifactEscrow::set_paused` blocked because the admin has exceeded the configured pause toggle rate limit.
    PauseToggleRateLimitExceeded = 227,

    // ------------------------------------------------------------------------------
    // Fee Schedule Errors (240..249)
    // ------------------------------------------------------------------------------
    /// `LiquifactEscrow::set_fee_schedule` received a fee outside the schedule's declared min/max bounds.
    FeeScheduleOutOfBounds = 240,
    /// `LiquifactEscrow::set_fee_schedule` attempted to create a second pending schedule before the first activates.
    FeeScheduleAlreadyPending = 241,
    /// `LiquifactEscrow::set_fee_schedule` received an activation ledger in the past.
    FeeScheduleInvalidActivation = 242,
    /// `LiquifactEscrow::set_fee_schedule` attempted to submit a schedule identical to the active schedule.
    FeeScheduleSameAsActive = 243,
    FundingTokenScaleInvalid = 244,
    FundingTokenScaleNotSet = 245,
}
