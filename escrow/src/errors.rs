use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u32)]
pub enum EscrowError {
    // -------------------------------------------------------------------------
    // Initialization & State Errors (1..19)
    // -------------------------------------------------------------------------
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidStatus = 3,
    EscrowExpired = 4,

    // -------------------------------------------------------------------------
    // Authorization & Admin Errors (20..35)
    // -------------------------------------------------------------------------
    Unauthorized = 20,
    AdminAlreadySet = 21,
    PendingAdminNotFound = 22,

    // -------------------------------------------------------------------------
    // Token / SEP-41 Safety Wrapper Errors (36..45)
    // -------------------------------------------------------------------------
    FundingTokenTransferFailed = 36,
    BalanceMismatchAfterTransfer = 37,
    NonPositiveTransferAmount = 38,
    TokenBalanceUnderflow = 39,
    TokenBalanceOverflow = 40,
    TokenWrapperInvariantViolation = 41,

    // -------------------------------------------------------------------------
    // Funding & Contribution Errors (50..69)
    // -------------------------------------------------------------------------
    FundingTargetExceeded = 50,
    ZeroContributionAmount = 51,
    InvestorCapReached = 52,
    BelowMinContributionFloor = 53,
    FundingClosed = 54,

    // -------------------------------------------------------------------------
    // Batch Operations Errors (80..89)
    // -------------------------------------------------------------------------
    FundingBatchEmpty = 80,
    FundingBatchExceedsLimit = 81,
    FundingBatchInvalidAmount = 82,
    FundingBatchDuplicateInvestor = 84,

    ClaimBatchEmpty = 85,
    ClaimBatchExceedsLimit = 86,

    // -------------------------------------------------------------------------
    // Migration & Upgrade Errors (90..99)
    // -------------------------------------------------------------------------
    MigrationVersionMismatch = 90,
    AlreadyCurrentSchemaVersion = 91,
    NoMigrationPath = 92,

    // -------------------------------------------------------------------------
    // Settlement & Bounds Validation Errors (100..109)
    // -------------------------------------------------------------------------
    SettlementAmountInvalid = 100,
    MaturityNotReached = 101,
    EscrowNotInFundedState = 102,
    WithdrawAmountInvalid = 103,

    // -------------------------------------------------------------------------
    // Legal Hold & Operational Pause (200..209)
    // -------------------------------------------------------------------------
    LegalHoldActive = 200,
    ContractPaused = 201,

    // -------------------------------------------------------------------------
    // SME Collateral Errors (300..309)
    // -------------------------------------------------------------------------
    NoCollateralToClear = 300,

    // -------------------------------------------------------------------------
    // Pause Configuration & Rate-Limit Errors (230..239)
    // -------------------------------------------------------------------------
    /// [`LiquifactEscrow::set_pause_max_duration`] received a duration outside
    /// [`MIN_PAUSE_MAX_DURATION_SECS`]..=[`MAX_PAUSE_MAX_DURATION_SECS`]. Zero is always allowed.
    PauseMaxDurationOutOfRange = 230,
    /// [`LiquifactEscrow::set_pause_rate_limit`] received a toggle limit outside
    /// [`MIN_PAUSE_TOGGLE_LIMIT`]..=[`MAX_PAUSE_TOGGLE_LIMIT`]. Zero is allowed only with zero window.
    PauseToggleLimitOutOfRange = 231,
    /// [`LiquifactEscrow::set_pause_rate_limit`] received a window outside
    /// [`MIN_PAUSE_TOGGLE_WINDOW_SECS`]..=[`MAX_PAUSE_TOGGLE_WINDOW_SECS`]. Zero is allowed only with zero toggles.
    PauseToggleWindowOutOfRange = 232,
    /// [`LiquifactEscrow::set_pause_rate_limit`] received an inconsistent configuration:
    /// nonzero toggles must have a nonzero window, and nonzero window must have nonzero toggles.
    PauseRateLimitInvalidCombination = 233,
    /// [`LiquifactEscrow::set_paused`] blocked because the admin has exceeded the configured pause toggle rate limit.
    PauseToggleRateLimitExceeded = 234,
}
