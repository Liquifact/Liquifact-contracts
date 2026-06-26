#![cfg_attr(not(test), no_std)]
//! LiquiFact Escrow Contract
//!
//! Holds investor funds for an invoice until settlement.
//! - SME receives stablecoin when funding target is met ([`LiquifactEscrow::withdraw`])
//! - SME records optional **collateral commitments** ([`LiquifactEscrow::record_sme_collateral_commitment`]) —
//!   these are **ledger records only**; they do **not** move tokens, freeze balances,
//!   reserve assets, or create an enforceable on-chain claim.
//! - [`LiquifactEscrow::settle`] finalizes the escrow after maturity (when configured).
//!
//! ## Schema version ([`SCHEMA_VERSION`] / [`DataKey::Version`])
//!
//! The constant [`SCHEMA_VERSION`] is written to [`DataKey::Version`] by [`LiquifactEscrow::init`]
//! and is the canonical source of truth for upgrade decisions. **Current value: 6.**
//!
//! [`LiquifactEscrow::migrate`] **fails with typed errors in all current execution paths** — no
//! silent migration work is promised or performed. Operators must extend `migrate` before calling
//! it, or redeploy when stored struct layout changes. See `docs/OPERATOR_RUNBOOK.md` for the full
//! decision tree.
//!
//! ## SME collateral commitment metadata
//!
//! [`LiquifactEscrow::record_sme_collateral_commitment`] is an SME-authenticated metadata write for
//! off-chain risk review. The stored [`SmeCollateralCommitment`] and emitted
//! [`CollateralRecordedEvt`] are not proof of custody, lien, encumbrance, asset control, or token
//! movement. Risk teams and indexers must label this state as reported collateral metadata and must
//! verify supporting evidence outside this contract.
//!
//! ## Compliance hold (legal hold)
//!
//! An admin may set [`DataKey::LegalHold`] to block risk-bearing transitions until cleared:
//! [`LiquifactEscrow::settle`], SME [`LiquifactEscrow::withdraw`], and
//! [`LiquifactEscrow::claim_investor_payout`]. **Clearing** requires the **current**
//! [`InvoiceEscrow::admin`] to call [`LiquifactEscrow::set_legal_hold`] with `active = false`
//! (or [`LiquifactEscrow::clear_legal_hold`]). This contract does not embed a timelock or
//! council multisig: production deployments **must** use a governed `admin` (multisig or
//! protocol DAO) so a single lost key cannot strand funds indefinitely.
//!
//! **Failure mode:** a hold plus loss of the current admin signing key leaves funds blocked
//! on-chain until governance regains control of admin authority. There is no break-glass bypass.
//!
//! **Recovery lever:** [`LiquifactEscrow::propose_admin`] and
//! [`LiquifactEscrow::accept_admin`] are **not** gated by the hold. Governance proposes a new
//! admin, the proposed address accepts, then the new admin clears the hold. Invariant: a hold is
//! always clearable by whoever holds `InvoiceEscrow::admin`; recovery requires controlling that
//! authority. See `docs/escrow-legal-hold.md` and [ADR-004](docs/adr/ADR-004-legal-hold.md).
//!
//! ## Authorization guard ordering
//!
//! Every state-mutating entrypoint follows a canonical sequence (see
//! `docs/escrow-security-checklist.md` §6 and [ADR-002](docs/adr/ADR-002-auth-boundaries.md)):
//!
//! 1. **Read-only** preconditions (legal hold, status checks, input validation).
//! 2. **`Address::require_auth()`** for the bound role ([Stellar authorization](https://developers.stellar.org/docs/build/guides/auth/contract-authorization)).
//! 3. **Storage writes** and **SEP-41 transfers** (via [`external_calls`]).
//!
//! Invariant: no instance/persistent storage mutation and no token transfer occurs until
//! step 2 succeeds. Reading [`DataKey::Escrow`] before `require_auth` is intentional — it is
//! read-only and does not weaken the auth boundary.
//!
//! ## Invoice identifier (`invoice_id`)
//!
//! At initialization, `invoice_id` is supplied as a Soroban [`String`] and validated for length
//! and charset before conversion to [`Symbol`] for storage. Align off-chain invoice slugs with the
//! same rules (ASCII alphanumeric + `_`, max length [`MAX_INVOICE_ID_STRING_LEN`]) so indexers stay
//! unambiguous.
//!
//! ## Funding token and registry (immutable hints)
//!
//! Each escrow instance binds exactly one **funding token** contract ([`DataKey::FundingToken`])
//! at [`LiquifactEscrow::init`]; it cannot be changed after deploy. An optional **registry**
//! ([`DataKey::RegistryRef`]) is a read-only discoverability hint only — it is **not** an authority
//! for this contract and must not be used on-chain as proof of registry state without calling the
//! registry yourself.
//!
//! ## Terminal dust sweep
//!
//! [`LiquifactEscrow::sweep_terminal_dust`] moves at most [`MAX_DUST_SWEEP_AMOUNT`] units of the
//! bound funding token from this contract to the immutable **treasury** address, only when the
//! escrow has reached a **terminal** [`InvoiceEscrow::status`] (settled, withdrawn, or cancelled).
//! It cannot run during a legal hold. Transfers go through [`crate::external_calls`] so **pre/post
//! token balances** must match the requested amount (standard SEP-41 behavior); fee-on-transfer or
//! malicious tokens are **explicitly out of scope** and fail with typed errors at the balance-check
//! boundary. This is meant for rounding residue / stray transfers, not for settling live liabilities —
//! integrations that custody principal on-chain must keep token balances reconciled with
//! `funded_amount` so treasury sweeps cannot pull user funds.
//!
//! ## Ledger time trust model
//!
//! [`LiquifactEscrow::settle`] and [`LiquifactEscrow::claim_investor_payout`] compare against
//! [`Env::ledger`] timestamps only (no wall-clock oracle). Maturity, per-investor **claim locks**
//! from [`LiquifactEscrow::fund_with_commitment`], and [`FundingCloseSnapshot`] metadata must be
//! interpreted as **validator-observed ledger time**, including possible skew between simulated and
//! live networks—integrators should treat boundaries as `>=` / `<` tests on integer seconds.
//!
//! ## Optional tiered yield (immutable table at init)
//!
//! Pass `yield_tiers` to [`LiquifactEscrow::init`] as [`Option`] of a Soroban [`Vec`] of [`YieldTier`].
//! The table is **immutable** for the escrow instance. Investors who use [`LiquifactEscrow::fund_with_commitment`]
//! on their **first** deposit select an effective [`DataKey::InvestorEffectiveYield`] from the ladder;
//! further principal from that address must use [`LiquifactEscrow::fund`]. **Fairness:** tiers are
//! validated non-decreasing in both `min_lock_secs` and `yield_bps` relative to the base [`InvoiceEscrow::yield_bps`].
//!
//! ## Funding-close snapshot (pro-rata)
//!
//! When status first becomes **funded**, [`DataKey::FundingCloseSnapshot`] stores total principal
//! (including over-funding past target), the target, and ledger timestamp/sequence. **Immutable** once
//! written; see `docs/escrow-pro-rata.md` for the authoritative pro-rata payout math and rounding rules.
//! Off-chain share for an investor is `get_contribution(addr) / snapshot.total_principal`.

#![allow(clippy::too_many_arguments)]

#[cfg(test)]
extern crate std;

use core::{clone::Clone, default::Default};
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error,
    symbol_short, token::TokenClient, Address, BytesN, Env, String, Symbol, Vec,
};

pub mod external_calls;

/// Current storage schema version written to [`DataKey::Version`] by [`LiquifactEscrow::init`].
///
/// # Schema version changelog
///
/// | Version | Summary | Upgrade path |
/// |---------|---------|-------------|
/// | 1 | Initial schema (`InvoiceEscrow` v1, basic fund / settle) | N/A |
/// | 2 | Added `InvestorEffectiveYield`, `InvestorClaimNotBefore` | Additive keys — no `migrate` call required |
/// | 3 | Added `FundingCloseSnapshot`, `MinContributionFloor`, `MaxUniqueInvestorsCap`, `UniqueFunderCount` | Additive keys — old instances return defaults |
/// | 4 | Added `PrimaryAttestationHash`, `AttestationAppendLog` | Additive keys — no `migrate` call required |
/// | 5 | Added `YieldTierTable`, `RegistryRef`, `Treasury`; `fund_with_commitment` | **Redeploy required** if `InvoiceEscrow` XDR changed |
/// | 6 | Per-investor keys moved to **persistent** storage (see ADR-007) | **Redeploy required** — no `migrate` path (addresses not enumerable) |
///
/// See `docs/OPERATOR_RUNBOOK.md` for the full redeploy-vs-upgrade decision tree.
pub const SCHEMA_VERSION: u32 = 6;

/// Upper bound on [`LiquifactEscrow::append_attestation_digest`] entries to keep storage bounded.
/// Revocation via [`LiquifactEscrow::revoke_attestation_digest`] does not consume a slot.
pub const MAX_ATTESTATION_APPEND_ENTRIES: u32 = 32;

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Env, Symbol,
};

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
pub enum DataKey {
    Escrow,
    SmeCollateralPledge,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowError {
    NotInitialized = 1,
    NotOpen = 2,
    NotFunded = 3,
    /// Raised by clear_sme_collateral_commitment when no pledge is recorded.
    NoCollateralToClear = 4,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Maximum invoice `amount` accepted by [`LiquifactEscrow::init`].
///
/// # Derivation (overflow-free coupon math)
///
/// `compute_investor_payout` uses this integer math (see docs/escrow-pro-rata.md):
///
/// ```text
/// coupon       = total_principal × yield_bps / 10_000  (floor)
/// settle_pool  = total_principal + coupon
/// gross_payout = contribution × settle_pool / total_principal
/// ```
///
/// With `yield_bps ∈ [0, 10_000]`, we require that for worst-case
/// `yield_bps = 10_000`:
/// - `coupon = total_principal × 10_000 / 10_000 = total_principal`
/// - `total_principal × 10_000` fits in `i128`
/// - `settle_pool = 2 × total_principal` fits in `i128`
/// - `contribution × settle_pool` fits in `i128` (with `contribution ≤ total_principal`)
///
/// Setting `MAX_INVOICE_AMOUNT = i128::MAX / 10_000` is sufficient because it implies:
/// - `amount × 10_000 ≤ i128::MAX`
/// - `2 × amount ≤ 2 × (i128::MAX / 10_000) < i128::MAX` for 10_000-bps coupon,
///   and all intermediate `checked_*` operations are overflow-free by construction.
pub const MAX_INVOICE_AMOUNT: i128 = i128::MAX / 10_000;


/// Upper bound on [`LiquifactEscrow::fund_batch`] entries to keep storage/CPU bounded.
/// Mirrors the spirit of `MAX_ATTESTATION_APPEND_ENTRIES` to limit per-call work.
pub const MAX_FUND_BATCH: u32 = 50;

/// Upper bound on [`LiquifactEscrow::sweep_terminal_dust`] per call (base units of the funding token).
///
/// Caps blast radius if instrumentation mis-estimates “dust”; tune per asset decimals off-chain.
pub const MAX_DUST_SWEEP_AMOUNT: i128 = 100_000_000;

/// Maximum UTF-8 byte length for the invoice `String` at init (matches Soroban [`Symbol`] max).
pub const MAX_INVOICE_ID_STRING_LEN: u32 = 32;

/// Minimum instance storage TTL extension horizon for time-sensitive escrow entries.
///
/// `bump_ttl` extends instance-storage entries to avoid rent/archival edge cases when
/// maturity/claim locks are far in the future.
///
/// Named as a constant so operators can reason about and audit the threshold.
pub const INSTANCE_TTL_MIN_EXTENSION_LEDGERS: u32 = 60 * 60; // Approx. 1h at 1 ledger/sec.

/// Minimum persistent storage TTL extension horizon for per-investor allowlist entries.
///
/// When the escrow uses the allowlist gate, investor funding depends on persistent entries.
/// Extending persistent allowlist TTL reduces the risk of silent allowlist disablement.
pub const PERSISTENT_TTL_MIN_EXTENSION_LEDGERS: u32 = 60 * 60; // Approx. 1h at 1 ledger/sec.

/// Default maximum maturity horizon in seconds (~5 years) when no explicit horizon is configured.
///
/// Used as the upper bound in [`validate_maturity_bounds`] unless overridden at init or via
/// [`LiquifactEscrow::update_maturity_max_horizon`]. Prevents accidental permanent settlement locks
/// from mistakenly far-future maturity timestamps.
pub const DEFAULT_MATURITY_MAX_HORIZON_SECS: u64 = 157_680_000; // ~5 years (365.25 * 24 * 3600 * 5)

/// Stable typed errors emitted by LiquiFact escrow entrypoints.
///
/// Codes are append-only: never reuse or renumber a variant. Client SDKs should branch on the
/// numeric code rather than legacy panic strings. See `docs/escrow-error-messages.md`.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    /// [`LiquifactEscrow::init`] rejected a non-positive invoice amount.
    AmountMustBePositive = 1,
    /// [`LiquifactEscrow::init`] rejected `yield_bps` outside `0..=10_000`.
    YieldBpsOutOfRange = 2,
    /// [`LiquifactEscrow::init`] rejected an invoice amount too large to keep
    /// `compute_investor_payout` arithmetic overflow-free.
    AmountExceedsMax = 14,
    /// [`LiquifactEscrow::init`] called when escrow storage already exists.
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

    /// Escrow storage is missing; entrypoint requires prior [`LiquifactEscrow::init`].
    EscrowNotInitialized = 20,
    /// [`DataKey::FundingToken`] is unset (escrow not fully initialized).
    FundingTokenNotSet = 21,
    /// [`DataKey::Treasury`] is unset (escrow not fully initialized).
    TreasuryNotSet = 22,

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
    /// `balance - sweep_amt` must be `>= funded_amount - distributed_principal`.
    SweepExceedsLiabilityFloor = 42,

    /// [`LiquifactEscrow::bind_primary_attestation_hash`] called when a primary hash exists.
    PrimaryAttestationAlreadyBound = 50,
    /// [`LiquifactEscrow::append_attestation_digest`] exceeded [`MAX_ATTESTATION_APPEND_ENTRIES`].
    AttestationAppendLogCapacityReached = 51,
    /// [`LiquifactEscrow::revoke_attestation_digest`] received an `index >= log.len()`.
    AttestationIndexOutOfRange = 52,
    /// [`LiquifactEscrow::revoke_attestation_digest`] called on an already-revoked index.
    AttestationAlreadyRevoked = 53,

    /// [`LiquifactEscrow::record_sme_collateral_commitment`] received a non-positive amount.
    CollateralAmountNotPositive = 60,
    /// [`LiquifactEscrow::record_sme_collateral_commitment`] received an empty asset symbol.
    CollateralAssetEmpty = 61,
    /// [`LiquifactEscrow::record_sme_collateral_commitment`] received a timestamp before the stored record.
    CollateralTimestampBackwards = 62,

    /// [`LiquifactEscrow::set_investors_allowlisted`] received an empty batch.
    InvestorBatchEmpty = 70,
    /// [`LiquifactEscrow::set_investors_allowlisted`] exceeded [`MAX_INVESTOR_ALLOWLIST_BATCH`].
    InvestorBatchTooLarge = 71,
    /// [`LiquifactEscrow::fund_batch`] received an empty entries vector.
    FundingBatchEmpty = 82,
    /// [`LiquifactEscrow::fund_batch`] exceeded [`MAX_FUND_BATCH`].
    FundingBatchTooLarge = 83,
    /// [`LiquifactEscrow::fund_batch`] rejected because the batch contains the same investor address more than once.
    FundingBatchDuplicateInvestor = 84,
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

    /// [`LiquifactEscrow::migrate`] `from_version` does not match stored version.
    MigrationVersionMismatch = 90,
    /// [`LiquifactEscrow::migrate`] called at or above [`SCHEMA_VERSION`].
    AlreadyCurrentSchemaVersion = 91,
    /// [`LiquifactEscrow::migrate`] has no implemented path from the requested version.
    NoMigrationPath = 92,

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
    /// Commitment lock would push `now + committed_lock_secs` past the escrow maturity.
    /// Reject at deposit time so a settled escrow cannot hold an investor's payout
    /// claim hostage beyond the point where principal is due.
    CommitmentLockExceedsMaturity = 111,

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

    /// [`LiquifactEscrow::cancel_funding`] blocked while a legal hold is active.
    LegalHoldBlocksCancelFunding = 140,
    /// [`LiquifactEscrow::cancel_funding`] called while escrow is not open.
    CancelFundingNotOpen = 141,
    /// [`LiquifactEscrow::refund`] called while escrow is not cancelled.
    RefundNotCancelled = 142,
    /// [`LiquifactEscrow::refund`] for an address with zero contribution.
    NoContributionToRefund = 143,

    /// `clear_legal_hold` was called without a prior `request_legal_hold_clear`.
    LegalHoldClearRequestMissing = 150,
    /// The two-phase legal-hold clear delay has not elapsed yet.
    LegalHoldClearNotReady = 151,
    /// Computing the legal-hold clear ready-at timestamp would overflow.
    LegalHoldClearDelayOverflow = 152,
    /// Funding deadline has passed, new deposits are rejected.
    FundingDeadlinePassed = 163,

    /// A legal hold blocks rotating the beneficiary (SME) address.
    LegalHoldBlocksBeneficiaryRotation = 160,
    /// Beneficiary rotation was attempted while the escrow was not in a
    /// pre-settlement state (`status` must be 0 = open or 1 = funded).
    RotationNotOpen = 161,
    /// The proposed new SME address is identical to the current beneficiary.
    NewSmeSameAsCurrent = 162,

    /// Attempted to accept admin role when no pending admin exists.
    /// @dev Historical note: Prior to PR #XYZ, this shared discriminant 163 with `FundingDeadlinePassed`.
    /// Reassigned to 81 to maintain uniqueness within the admin-handover range.
    NoPendingAdmin = 81,
    /// The contract's funding-token balance is less than `funded_amount` at withdraw time.
    /// Funds must be custodied in this contract before the SME can pull them.
    InsufficientContractBalance = 165,
}

#[inline(always)]
pub(crate) fn fail(env: &Env, error: EscrowError) -> ! {
    panic_with_error!(env, error)
}

#[inline(always)]
pub(crate) fn ensure(env: &Env, condition: bool, error: EscrowError) {
    if !condition {
        fail(env, error);
    }
}

/// Validates that a non-zero maturity timestamp is not in the past and not beyond the
/// configured maximum horizon from the current ledger time.
///
/// # Arguments
/// * `maturity` - The maturity timestamp to validate (0 means "no lock" and is always accepted).
/// * `max_horizon` - The maximum allowed horizon in seconds from the current ledger time.
///
/// # Panics
/// * [`EscrowError::MaturityInPast`] if `maturity` is less than the current ledger timestamp.
/// * [`EscrowError::MaturityExceedsMaxHorizon`] if `maturity` exceeds `now + max_horizon`.
pub(crate) fn validate_maturity_bounds(env: &Env, maturity: u64, max_horizon: u64) {
    if maturity == 0 {
        return;
    }
    let now = env.ledger().timestamp();

    ensure(env, maturity >= now, EscrowError::MaturityInPast);

    let max_allowed = now.saturating_add(max_horizon);
    ensure(
        env,
        maturity <= max_allowed,
        EscrowError::MaturityExceedsMaxHorizon,
    );
}

// --- Storage keys ---

#[contracttype]
#[derive(Clone)]
/// Storage discriminator for persisted contract state.
///
/// Most variants live in **instance** storage (shared TTL with the contract instance, bounded
/// aggregate size). Per-investor variants
/// [`InvestorContribution`], [`InvestorEffectiveYield`], [`InvestorClaimNotBefore`], and
/// [`InvestorClaimed`] use **persistent** storage (independent per-address TTL; see ADR-007 and
/// `docs/escrow-gas-storage-notes.md`). [`InvestorAllowlisted`] also uses persistent storage.
///
/// Optional keys are always read with `.get(...).unwrap_or(default)` so that deployments predating
/// a key behave as “unset / default” without panicking.
///
/// ## Additive-key policy (see ADR-007)
///
/// Adding a new variant is **backward-compatible** when the new key is read with
/// `.unwrap_or(default)` and its absence does not change existing entrypoint semantics.
/// Renaming a variant, changing its XDR discriminant, or altering the stored type of an
/// existing key is **breaking** and requires a `migrate` path or a full redeploy.
///
/// Derive rationale:
/// - `Clone`: required because keys are passed by reference into storage APIs and reused
///   across lookups/sets in the same execution path.
pub enum DataKey {
    /// Full escrow snapshot ([`InvoiceEscrow`]); rewritten atomically on every state transition.
    Escrow,
    /// Stored schema version; written once by [`LiquifactEscrow::init`] to [`SCHEMA_VERSION`]
    /// and updated by [`LiquifactEscrow::migrate`] when a migration path is implemented.
    /// Read with [`LiquifactEscrow::get_version`]. Never delete or rename this variant.
    Version,
    /// Per-investor contributed principal recorded during [`LiquifactEscrow::fund`].
    /// **Persistent** storage. Absent ⇒ `0`. One entry per investor address.
    InvestorContribution(Address),
    /// When true, compliance/legal hold blocks payouts and settlement finalization.
    /// Absent ⇒ `false` (no hold). Toggled by admin via [`LiquifactEscrow::set_legal_hold`].
    LegalHold,
    /// Optional minimum ledger timestamp when `LegalHold` may be cleared after a
    /// [`LiquifactEscrow::request_clear_legal_hold`] call.
    /// Absent ⇒ no clear request is pending.
    LegalHoldClearableAt,
    /// Configured minimum delay between [`LiquifactEscrow::request_clear_legal_hold`] and
    /// [`LiquifactEscrow::set_legal_hold(env, false)`]. Absent ⇒ `0`.
    LegalHoldClearDelay,
    /// Optional SME collateral commitment metadata (record-only — not an on-chain asset lock).
    /// Absent when no commitment has been recorded. Replaceable by the SME.
    SmeCollateralPledge,
    /// Set to `true` when an investor has exercised a claim after settlement.
    /// **Persistent** storage. Absent ⇒ `false`. Written once; a second claim returns without re-emitting.
    InvestorClaimed(Address),
    /// SEP-41 funding asset for this invoice instance; set once in [`LiquifactEscrow::init`].
    /// Immutable after init.
    FundingToken,
    /// Protocol treasury that may receive [`LiquifactEscrow::sweep_terminal_dust`]; set once in init.
    /// Immutable after init.
    Treasury,
    /// Optional registry contract id for indexers; **hint only**, not authority (see module rustdoc).
    /// Omitted from storage when unset at init. Absent ⇒ `None`.
    RegistryRef,
    /// Immutable tier table when configured at [`LiquifactEscrow::init`]; omitted when tiering is off.
    /// Absent ⇒ no tiering (base `yield_bps` applies to all investors).
    /// **Trust:** values are protocol-supplied at deploy; the contract never mutates this key after init.
    YieldTierTable,
    /// Set once when status first becomes **funded** (1); immutable thereafter (pro-rata denominator).
    /// Absent until the escrow reaches `status == 1`. See [`FundingCloseSnapshot`].
    FundingCloseSnapshot,
    /// Effective annualized yield in bps chosen at this investor’s **first** deposit (see tiered yield).
    /// **Persistent** storage. Absent ⇒ falls back to [`InvoiceEscrow::yield_bps`]. One entry per investor address.
    InvestorEffectiveYield(Address),
    /// Minimum [`Env::ledger`] timestamp before [`LiquifactEscrow::claim_investor_payout`] (0 = no extra gate).
    /// **Persistent** storage. Absent ⇒ `0`. One entry per investor address; set on first deposit.
    InvestorClaimNotBefore(Address),
    /// Minimum [`LiquifactEscrow::fund`] / [`LiquifactEscrow::fund_with_commitment`] amount per call (0 = no floor).
    /// Written as `0` even when unconfigured so reads always succeed.
    MinContributionFloor,
    /// When set at [`LiquifactEscrow::init`], caps distinct investor addresses that may contribute.
    /// Absent ⇒ unlimited. Checked against [`DataKey::UniqueFunderCount`] on each new investor.
    MaxUniqueInvestorsCap,
    /// Optional immutable per-investor cap on total principal credited to a single address.
    /// Absent ⇒ unlimited. Checked against [`DataKey::InvestorContribution`] on every deposit.
    MaxPerInvestorCap,
    /// Proposed successor admin waiting for [`LiquifactEscrow::accept_admin`].
    /// Absent ⇒ no pending handover. Cleared after successful acceptance.
    PendingAdmin,
    /// Count of distinct investor addresses that have a non-zero [`DataKey::InvestorContribution`].
    /// Written as `0` at init; incremented once per new investor in `fund_impl`.
    UniqueFunderCount,
    /// Admin-only **single-set** off-chain attestation digest (e.g. SHA-256 of a legal/KYC bundle).
    /// Absent until [`LiquifactEscrow::bind_primary_attestation_hash`] is called; single-set thereafter.
    PrimaryAttestationHash,
    /// Append-only audit chain of digests (bounded by [`MAX_ATTESTATION_APPEND_ENTRIES`]).
    /// Absent ⇒ empty log. See [`LiquifactEscrow::append_attestation_digest`].
    AttestationAppendLog,
    /// Per-index revocation marker for [`DataKey::AttestationAppendLog`] entries.
    /// Absent ⇒ not revoked. Written as `true` by [`LiquifactEscrow::revoke_attestation_digest`].
    /// Preserves the original digest for auditability while signalling supersession.
    AttestationRevoked(u32),
    /// When true, only allowlisted addresses may call [`LiquifactEscrow::fund`] or [`LiquifactEscrow::fund_with_commitment`].
    AllowlistActive,
    /// Whether a specific address is permitted to fund when [`DataKey::AllowlistActive`] is true.
    InvestorAllowlisted(Address),
    /// Set to `true` once an investor's principal has been refunded in a cancelled escrow.
    /// Absent ⇒ `false`. Written once; prevents double-refund.
    InvestorRefunded(Address),
    /// Running total of principal already returned to investors via [`LiquifactEscrow::refund`].
    /// Absent ⇒ `0`. Incremented atomically with each successful refund transfer.
    /// Used by [`LiquifactEscrow::sweep_terminal_dust`] to compute outstanding liabilities:
    /// `outstanding = funded_amount - distributed_principal`.
    DistributedPrincipal,
    /// Configured maximum maturity horizon in seconds from current ledger time.
    /// Absent ⇒ falls back to [`DEFAULT_MATURITY_MAX_HORIZON_SECS`].
    /// Set at init and updatable via [`LiquifactEscrow::update_maturity_max_horizon`].
    MaturityMaxHorizon,
}

// --- Data types ---

/// Full state of an invoice escrow persisted in contract storage (`DataKey::Escrow`).
#[contracttype]
#[derive(Debug, PartialEq)]
/// Full escrow snapshot persisted at [`DataKey::Escrow`].
///
/// Derive rationale:
/// - `Debug`: improves failure diagnostics in tests.
/// - `PartialEq`: allows exact state assertions in tests.
///
/// `Clone` is intentionally omitted to avoid accidental full-state copies.
pub struct InvoiceEscrow {
    pub invoice_id: Symbol,
    pub admin: Address,
    pub sme_address: Address,
    pub amount: i128,
    pub funding_target: i128,
    pub funded_amount: i128,
    pub yield_bps: i64,
    pub maturity: u64,
    /// 0 = open, 1 = funded, 2 = settled, 3 = withdrawn (SME pulled liquidity), 4 = cancelled (admin-gated; investors may refund)
    pub status: u32,
}

/// SME-reported collateral metadata for off-chain risk review.
///
/// **Record-only:** this struct is stored for transparency and indexing. It does **not**
/// custody, escrow, transfer, freeze, reserve, or verify assets. It also does not alter funding,
/// settlement, SME withdrawal, investor-claim, compliance hold, or treasury-sweep behavior.
/// Future versions that enforce asset movement or custody must introduce explicit APIs and must
/// not treat historical records from this type as proof of locked assets.
///
/// # Fields
/// - `asset`: The off-chain asset symbol (cannot be empty).
/// - `amount`: The reported collateral amount (must be positive).
/// - `recorded_at`: The Soroban ledger timestamp when this record was written.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
/// SME collateral commitment metadata (record-only).
///
/// Derive rationale:
/// - `Clone`: required for `Option<SmeCollateralCommitment>` used in `EscrowSummary`.
/// - `Debug`: improves failure diagnostics in tests.
/// - `PartialEq`: allows deterministic assertion of stored/read values.
pub struct SmeCollateralCommitment {
    pub asset: Symbol,
    pub amount: i128,
    pub recorded_at: u64,
}

/// One step in an optional tier ladder: investors who commit to at least `min_lock_secs` (on first
/// deposit via [`LiquifactEscrow::fund_with_commitment`]) may receive `yield_bps` for pro-rata /
/// off-chain coupon math. **Immutable** after `init`: the table is fixed for the escrow instance.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct YieldTier {
    pub min_lock_secs: u64,
    pub yield_bps: i64,
}

/// Captured exactly once at the first ledger transition to **funded** so settlement and claims can
/// use a stable total principal and target. If the threshold-crossing deposit overshoots
/// [`InvoiceEscrow::funding_target`], [`FundingCloseSnapshot::total_principal`] records the full
/// credited [`InvoiceEscrow::funded_amount`] at close and becomes the pro-rata denominator.
/// **Immutable** once written.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FundingCloseSnapshot {
    /// Sum of principal credited when the invoice became funded (`funded_amount` at close),
    /// including over-funding past target.
    pub total_principal: i128,
    pub funding_target: i128,
    pub closed_at_ledger_timestamp: u64,
    pub closed_at_ledger_sequence: u32,
}

/// Custom option-like enum to represent the captured funding close snapshot.
/// Models standard option semantics as a contracttype to avoid standard library
/// blanket trait limitations in Soroban SDK testutils.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum EscrowCloseSnapshot {
    None,
    Some(FundingCloseSnapshot),
}

/// Custom option-like enum to represent the SME collateral commitment.
/// Models standard option semantics as a contracttype to avoid standard library
/// blanket trait limitations in Soroban SDK testutils.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum CollateralCommitmentSnapshot {
    None,
    Some(SmeCollateralCommitment),
}

/// Comprehensive summary of the escrow contract state.
/// Bundles multiple read-only values to allow a single host invocation
/// for off-chain indexers and client rendering.
#[contracttype]
#[derive(Debug, PartialEq)]
pub struct EscrowSummary {
    /// Full escrow snapshot.
    pub escrow: InvoiceEscrow,
    /// True when `escrow.maturity > 0`; false means settlement has no maturity time lock.
    pub has_maturity_lock: bool,
    /// Active legal or compliance hold flag.
    pub legal_hold: bool,
    /// The captured funding close snapshot (Option).
    pub funding_close_snapshot: EscrowCloseSnapshot,
    /// Unique investors count who funded the escrow.
    pub unique_funder_count: u32,
    /// Whether the investor allowlist is active.
    pub is_allowlist_active: bool,
    /// Persisted schema version of the contract data.
    pub schema_version: u32,
    /// SME collateral commitment metadata (None when never recorded).
    pub sme_collateral_commitment: CollateralCommitmentSnapshot,
    /// Whether a primary attestation hash has been bound.
    pub has_primary_attestation: bool,
    /// Number of entries in the attestation append log.
    pub attestation_log_length: u32,
}

// --- Events ---

#[contractevent]
pub struct EscrowInitialized {
    #[topic]
    pub name: Symbol,
    pub escrow: InvoiceEscrow,
    /// Bound funding token; equals [`DataKey::FundingToken`].
    pub funding_token: Address,
    /// Bound treasury; equals [`DataKey::Treasury`].
    pub treasury: Address,
    /// Optional registry hint; equals [`DataKey::RegistryRef`] (`None` when unset).
    pub registry: Option<Address>,
    /// False when `escrow.maturity == 0`, which means `settle` has no maturity time lock.
    pub has_maturity_lock: bool,
}

#[contractevent]
pub struct MaxUniqueInvestorsCapLowered {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_cap: u32,
    pub new_cap: u32,
}

#[contractevent]
pub struct EscrowFunded {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    #[topic]
    pub investor: Address,
    pub amount: i128,
    pub funded_amount: i128,
    pub status: u32,
    /// Investor-specific effective yield (bps) after this fund; see [`DataKey::InvestorEffectiveYield`].
    pub investor_effective_yield_bps: i64,
    /// The `min_lock_secs` of the matched [`YieldTier`] (0 when base yield applies — no tier,
    /// no lock commitment, or simple fund). See [`LiquifactEscrow::effective_yield_for_commitment`].
    pub tier_lock_secs: u64,
}

/// Emitted by [`LiquifactEscrow::rotate_beneficiary`] when the SME (beneficiary)
/// address is changed, carrying both the prior and new addresses for auditing.
#[contractevent]
pub struct BeneficiaryRotated {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub prior_sme: Address,
    pub new_sme: Address,
}

#[contractevent]
pub struct EscrowPartialSettle {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub funded_amount: i128,
}

#[contractevent]
pub struct EscrowSettled {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub funded_amount: i128,
    pub yield_bps: i64,
    pub maturity: u64,
    /// Ledger timestamp at which the settlement occurred.
    pub settled_at_ledger_timestamp: u64,
    /// Realized settlement pool: `total_principal + floor(total_principal × yield_bps / 10_000)`.
    /// Derived from [`FundingCloseSnapshot::total_principal`] and the base [`InvoiceEscrow::yield_bps`].
    /// Zero when no [`FundingCloseSnapshot`] is present (legacy escrows without the snapshot key).
    pub settle_pool: i128,
}

#[contractevent]
pub struct MaturityUpdatedEvent {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_maturity: u64,
    pub new_maturity: u64,
}

#[contractevent]
pub struct MaturityMaxHorizonUpdated {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_horizon: u64,
    pub new_horizon: u64,
}

#[contractevent]
pub struct AdminTransferredEvent {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub new_admin: Address,
}

#[contractevent]
pub struct AdminProposedEvent {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub current_admin: Address,
    pub pending_admin: Address,
}

/// Emitted by [`LiquifactEscrow::transfer_admin`] (the deprecated one-step
/// admin transfer shim) so indexers and operators can flag integrators
/// still using the legacy single-step path.
///
/// This is purely **observability** — handover behavior is unchanged:
/// the shim still delegates to [`LiquifactEscrow::propose_admin`], which
/// still emits [`AdminProposedEvent`] as its primary signal. Operators
/// can aggregate this event over a deployment window to count callers
/// still using the deprecated entrypoint and drive them to the canonical
/// `propose_admin` → `accept_admin` two-step flow before `transfer_admin`
/// is removed.
///
/// # Fields
/// - `name`: Hardcoded `depr_xfer` symbol for routing/indexer filtering.
/// - `invoice_id`: Symbol representation of the escrow invoice.
/// - `proposed_address`: The address proposed via the deprecated shim.
///   Equal to `pending_admin` on the [`AdminProposedEvent`] emitted in the
///   same transaction, so indexers can correlate the two events one-to-one.
#[contractevent]
pub struct DeprecatedTransferAdminUsed {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub proposed_address: Address,
}

#[contractevent]
pub struct FundingTargetUpdated {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_target: i128,
    pub new_target: i128,
}

#[contractevent]
pub struct FundingDeadlineUpdated {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub prior_deadline: Option<u64>,
    pub new_deadline: Option<u64>,
}

#[contractevent]
pub struct LegalHoldChanged {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    /// `1` = hold enabled, `0` = cleared.
    pub active: u32,
}

#[contractevent]
pub struct LegalHoldClearRequested {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    /// Inclusive ledger timestamp when clearing may occur.
    pub clearable_at: u64,
}

/// SME collateral commitment metadata recorded.
///
/// This event is emitted when [`DataKey::SmeCollateralPledge`] is written or replaced by the SME.
/// It acts as a metadata-update signal and is not proof of custody, lien, encumbrance, asset control,
/// or token movement. The event intentionally omits token contract, custodian, and transfer-receipt
/// fields so consumers do not treat it as an on-chain encumbrance.
///
/// # Fields
/// - `name`: Hardcoded `coll_rec` symbol.
/// - `invoice_id`: Symbol representation of the invoice.
/// - `amount`: Newly recorded positive collateral amount.
/// - `prior_amount`: Prior recorded collateral amount (or `0` if none existed).
#[contractevent]
pub struct CollateralRecordedEvt {
    #[topic]
    pub name: Symbol,
    /// Invoice whose SME-reported metadata was updated.
    pub invoice_id: Symbol,
    /// SME-reported amount in the off-chain asset's own units; not a locked token balance.
    pub amount: i128,
    /// Prior recorded amount, or 0 if no prior commitment existed.
    pub prior_amount: i128,
}

#[contractevent]
pub struct SmeWithdrew {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub amount: i128,
    pub recipient: Address,
}

#[contractevent]
pub struct InvestorPayoutClaimed {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub investor: Address,
    #[topic]
    pub invoice_id: Symbol,
}

#[contractevent]
pub struct FundingCancelled {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub funded_amount: i128,
}

#[contractevent]
pub struct InvestorRefundedEvt {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub investor: Address,
    #[topic]
    pub invoice_id: Symbol,
    pub amount: i128,
}

#[contractevent]
pub struct TreasuryDustSwept {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub token: Address,
    pub amount: i128,
}

#[contractevent]
pub struct PrimaryAttestationBound {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub digest: BytesN<32>,
}

#[contractevent]
pub struct AttestationDigestAppended {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub index: u32,
    pub digest: BytesN<32>,
}

#[contractevent]
pub struct AttestationDigestRevoked {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub index: u32,
}

/// Emitted by [`LiquifactEscrow::unrevoke_attestation_digest`] when the revocation marker
/// for a previously revoked append-log entry is cleared by admin.
///
/// # Fields
/// - `invoice_id` — escrow identifier from [`InvoiceEscrow`].
/// - `index` — 0-based position in the append log whose revocation was cleared.
#[contractevent]
pub struct AttestationDigestUnrevoked {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub index: u32,
}

#[contractevent]
pub struct AllowlistEnabledChanged {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    /// `1` = enabled, `0` = disabled.
    pub active: u32,
}

#[contractevent]
pub struct InvestorAllowlistChanged {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub investor: Address,
    /// `1` = allowed, `0` = blocked.
    pub allowed: u32,
}

/// Batch-level summary event emitted once per `set_investors_allowlisted` call.
///
/// ## Purpose
/// Allows indexers and audit tooling to detect that a batch allowlist operation has
/// completed without scanning individual `al_set` events for a particular ledger
/// range. Useful for triggering downstream reconciliation or audit checks after a
/// bulk admin action.
///
/// ## Why it exists
/// The per-investor `InvestorAllowlistChanged` (`al_set`) events emitted by
/// `set_investors_allowlisted` are identical in shape to those emitted by the
/// single-address `set_investor_allowlisted`. Without a distinct batch marker,
/// indexers cannot tell whether a block of `al_set` events came from one batch
/// call or N individual calls. `InvestorAllowlistBatchApplied` (`al_batch`)
/// provides that signal with the full count in one event.
///
/// ## Relationship to per-investor events
/// This event **supplements** — it does not replace — the per-investor `al_set`
/// events. Both are emitted for every batch call:
/// - N × `InvestorAllowlistChanged` (`al_set`), one per address in input order.
/// - 1 × `InvestorAllowlistBatchApplied` (`al_batch`), after all per-investor writes.
///
/// ## Indexer / audit guidance
/// - Filter on `topic[1] == "al_batch"` to identify completed batch operations.
/// - `batch_size` matches the number of `al_set` events immediately preceding this
///   event in the same transaction. Auditors can cross-check by counting `al_set`
///   emissions in the same tx.
/// - `allowed` mirrors the single `allowed` flag passed to the batch call.
#[contractevent]
pub struct InvestorAllowlistBatchApplied {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub batch_size: u32,
    /// `1` = allowed, `0` = blocked.
    pub allowed: u32,
}

#[contractevent]
pub struct ContractUpgraded {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub new_wasm_hash: BytesN<32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollateralPledge {
    pub invoice_id: Symbol,
    pub amount: i128,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted by record_sme_collateral_commitment.
#[contractevent(topics = ["collateral_recorded"])]
pub struct CollateralRecordedEvt {
    #[topic]
    pub invoice_id: Symbol,
    pub amount: i128,
}

/// Emitted by clear_sme_collateral_commitment when a pledge is retired.
///
/// `amount` carries the value from the removed pledge record.
#[contractevent(topics = ["collateral_cleared"])]
pub struct CollateralClearedEvt {
    #[topic]
    pub invoice_id: Symbol,
    /// The amount that was recorded in the retired pledge.
    pub amount: i128,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_escrow(env: &Env) -> Result<InvoiceEscrow, EscrowError> {
    env.storage()
        .instance()
        .get(&DataKey::Escrow)
        .ok_or(EscrowError::NotInitialized)
}

/// Load escrow and require the caller to be the SME address.
fn load_escrow_require_sme(env: &Env) -> Result<InvoiceEscrow, EscrowError> {
    let escrow = load_escrow(env)?;
    escrow.sme_address.require_auth();
    Ok(escrow)
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct LiquifactEscrow;

/// Validates and converts a workspace-provided invoice identifier string into a Soroban [`Symbol`].
///
/// ### Constraints
/// - **Length**: Must be between 1 and [`MAX_INVOICE_ID_STRING_LEN`] (inclusive).
/// - **Charset**: Must only contain `[A-Za-z0-9_]`. This is a subset of the valid Symbol charset
///   enforced to ensure stable, URL-safe slugs in off-chain systems.
///
/// ### Security
/// This function performs a bounds-checked copy into a fixed stack buffer to prevent
/// uninitialized memory leaks. Only the exact byte-length of the input is converted
/// to the final symbol, ensuring no trailing null bytes or buffer remnants are preserved.
fn validate_invoice_id_string(env: &Env, invoice_id: &String) -> Symbol {
    let len = invoice_id.len();
    ensure(
        env,
        (1..=MAX_INVOICE_ID_STRING_LEN).contains(&len),
        EscrowError::InvoiceIdInvalidLength,
    );
    let len_u = len as usize;
    let mut buf = [0u8; 32];
    invoice_id.copy_into_slice(&mut buf[..len_u]);
    for &b in &buf[..len_u] {
        let ok =
            b.is_ascii_uppercase() || b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_';
        ensure(env, ok, EscrowError::InvoiceIdInvalidCharset);
    }
    let s = core::str::from_utf8(&buf[..len_u])
        .unwrap_or_else(|_| fail(env, EscrowError::InvoiceIdInvalidCharset));
    Symbol::new(env, s)
}

#[contractimpl]
impl LiquifactEscrow {
    fn legal_hold_active(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::LegalHold)
            .unwrap_or(false)
    }

    /// Read the immutable funding token address, failing with [`EscrowError::FundingTokenNotSet`]
    /// when the escrow has not been initialized.
    fn funding_token_or_fail(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::FundingToken)
            .unwrap_or_else(|| fail(env, EscrowError::FundingTokenNotSet))
    }

    /// Read the immutable treasury address, failing with [`EscrowError::TreasuryNotSet`]
    /// when the escrow has not been initialized.
    fn treasury_or_fail(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Treasury)
            .unwrap_or_else(|| fail(env, EscrowError::TreasuryNotSet))
    }

    fn validate_yield_tiers_table(env: &Env, tiers: &Option<Vec<YieldTier>>, base_yield: i64) {
        let Some(tiers) = tiers else {
            return;
        };
        if tiers.is_empty() {
            return;
        }
        let n = tiers.len();
        for i in 0..n {
            let t = tiers.get(i).unwrap();
            ensure(
                env,
                (0..=10_000).contains(&t.yield_bps),
                EscrowError::TierYieldOutOfRange,
            );
            ensure(
                env,
                t.yield_bps >= base_yield,
                EscrowError::TierYieldBelowBase,
            );
            if i > 0 {
                let p = tiers.get(i - 1).unwrap();
                ensure(
                    env,
                    t.min_lock_secs > p.min_lock_secs,
                    EscrowError::TierLockNotIncreasing,
                );
                ensure(
                    env,
                    t.yield_bps >= p.yield_bps,
                    EscrowError::TierYieldNotNonDecreasing,
                );
            }
        }
    }

    /// Returns `(effective_yield_bps, matched_lock_secs)` for a given commitment.
    /// `matched_lock_secs` is the [`YieldTier::min_lock_secs`] of the best matching tier,
    /// or `0` when no tier was matched (base yield applies).
    fn effective_yield_for_commitment(
        env: &Env,
        base_yield: i64,
        committed_lock_secs: u64,
    ) -> (i64, u64) {
        if committed_lock_secs == 0 {
            return (base_yield, 0);
        }
        let Some(tiers) = env
            .storage()
            .instance()
            .get::<DataKey, Vec<YieldTier>>(&DataKey::YieldTierTable)
        else {
            return (base_yield, 0);
        };
        if tiers.is_empty() {
            return (base_yield, 0);
        }
        let mut best = base_yield;
        let mut best_lock = 0u64;
        let n = tiers.len();
        for i in 0..n {
            let t = tiers.get(i).unwrap();
            if committed_lock_secs >= t.min_lock_secs && t.yield_bps > best {
                best = t.yield_bps;
                best_lock = t.min_lock_secs;
            }
        }
        (best, best_lock)
    }

    /// Initialize escrow. `funding_target` defaults to `amount`.
    ///
    /// Binds **`funding_token`**, **`treasury`**, and optional **`registry`** for this instance only.
    /// The funding token and treasury addresses are **immutable** after this call; the registry id is
    /// optional metadata for off-chain indexers (not an on-chain authority).
    ///
    /// `maturity == 0` is an explicit "no maturity lock" configuration: once funded, the SME may
    /// call [`LiquifactEscrow::settle`] immediately. Positive maturity values are validator-observed
    /// ledger timestamps and are enforced with an inclusive `ledger.timestamp() >= maturity` check.
    ///
    /// `invoice_id` must satisfy [`MAX_INVOICE_ID_STRING_LEN`] and charset rules (see
    /// [`validate_invoice_id_string`]).
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes for invalid amounts, yield bounds, invoice id validation,
    /// duplicate initialization, malformed optional caps, and invalid tier configuration.
    pub fn init(
        env: Env,
        admin: Address,
        invoice_id: String,
        sme_address: Address,
        amount: i128,
        yield_bps: i64,
        maturity: u64,
        funding_token: Address,
        registry: Option<Address>,
        treasury: Address,
        yield_tiers: Option<Vec<YieldTier>>,
        min_contribution: Option<i128>,
        max_unique_investors: Option<u32>,
        max_per_investor: Option<i128>,
        legal_hold_clear_delay: Option<u64>,
        maturity_max_horizon: Option<u64>,
    ) -> InvoiceEscrow {
        admin.require_auth();

        #[cfg(any(test, feature = "testutils"))]
        {
            register_mock_token_if_needed(&env, &funding_token);
        }

        ensure(&env, amount > 0, EscrowError::AmountMustBePositive);
        ensure(
            &env,
            amount <= MAX_INVOICE_AMOUNT,
            EscrowError::AmountExceedsMax,
        );
        ensure(
            &env,
            (0..=10_000).contains(&yield_bps),
            EscrowError::YieldBpsOutOfRange,
        );

        ensure(
            &env,
            !env.storage().instance().has(&DataKey::Escrow),
            EscrowError::EscrowAlreadyInitialized,
        );

        // Validate funding deadline
        if let Some(deadline) = funding_deadline {
            ensure(
                &env,
                deadline > env.ledger().timestamp(),
                EscrowError::FundingDeadlinePassed,
            );
            env.storage()
                .instance()
                .set(&DataKey::FundingDeadline, &deadline);
        }

        Self::validate_yield_tiers_table(&env, &yield_tiers, yield_bps);

        // Resolve and persist the maturity max horizon before validation so
        // validate_maturity_bounds can use the configured value.
        let max_horizon = maturity_max_horizon.unwrap_or(DEFAULT_MATURITY_MAX_HORIZON_SECS);
        validate_maturity_bounds(&env, maturity, max_horizon);
        env.storage()
            .instance()
            .set(&DataKey::MaturityMaxHorizon, &max_horizon);

        let invoice_sym = validate_invoice_id_string(&env, &invoice_id);

        let escrow = InvoiceEscrow {
            invoice_id: invoice_sym.clone(),
            admin: admin.clone(),
            sme_address: sme_address.clone(),
            amount,
            funding_target: amount,
            funded_amount: 0,
            yield_bps,
            maturity,
            status: 0,
        };
        env.storage().instance().set(&DataKey::Escrow, &escrow);
        escrow
    }

    /// Get current escrow state.
    pub fn get_escrow(env: Env) -> InvoiceEscrow {
        env.storage()
            .instance()
            .get(&DataKey::Escrow)
            .unwrap_or_else(|| panic!("Escrow not initialized"))
    }

    /// Record investor funding. In production, this would be called with token transfer.
    pub fn fund(env: Env, _investor: Address, amount: i128) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone());
        assert!(escrow.status == 0, "Escrow not open for funding");
        escrow.funded_amount += amount;
        if escrow.funded_amount >= escrow.funding_target {
            escrow.status = 1;
        }
        env.storage()
            .instance()
            .set(&DataKey::MinContributionFloor, &floor);

        env.storage()
            .instance()
            .set(&DataKey::UniqueFunderCount, &0u32);

        if let Some(cap) = max_per_investor {
            ensure(&env, cap > 0, EscrowError::MaxPerInvestorNotPositive);
            env.storage()
                .instance()
                .set(&DataKey::MaxPerInvestorCap, &cap);
        }

        if let Some(cap) = max_unique_investors {
            ensure(&env, cap > 0, EscrowError::MaxUniqueInvestorsNotPositive);
            env.storage()
                .instance()
                .set(&DataKey::MaxUniqueInvestorsCap, &cap);
        }

        let delay = legal_hold_clear_delay.unwrap_or(0);
        if delay > 0 {
            env.storage()
                .instance()
                .set(&DataKey::LegalHoldClearDelay, &delay);
        }

        if let Some(active) = allowlist_active {
            if active {
                env.storage()
                    .instance()
                    .set(&DataKey::AllowlistActive, &true);
            }
        }

        EscrowInitialized {
            name: symbol_short!("escrow_ii"),
            // Read stored values so event fields match persisted keys (indexer single-event bootstrap).
            escrow: Self::get_escrow(env.clone()),
            funding_token: Self::get_funding_token(env.clone()),
            treasury: Self::get_treasury(env.clone()),
            registry: Self::get_registry_ref(env.clone()),
            has_maturity_lock: Self::has_maturity_lock(env.clone()),
        }
        .publish(&env);

        escrow
    }

    /// Returns the SEP-41 funding token bound at [`LiquifactEscrow::init`] ([`DataKey::FundingToken`]).
    ///
    /// **Immutable:** set once at init; cannot change after deploy. Emits
    /// [`EscrowError::FundingTokenNotSet`] if called before init.
    pub fn get_funding_token(env: Env) -> Address {
        Self::funding_token_or_fail(&env)
    }

    /// Returns the protocol treasury address bound at [`LiquifactEscrow::init`] ([`DataKey::Treasury`]).
    ///
    /// **Immutable:** set once at init; cannot change after deploy. The treasury is the only
    /// recipient of [`LiquifactEscrow::sweep_terminal_dust`]. Emits
    /// [`EscrowError::TreasuryNotSet`] if called before init.
    pub fn get_treasury(env: Env) -> Address {
        Self::treasury_or_fail(&env)
    }

    /// Returns the optional off-chain registry hint stored at [`DataKey::RegistryRef`], or [`None`]
    /// when no registry was supplied at [`LiquifactEscrow::init`].
    ///
    /// **Non-authority:** this address is a read-only discoverability hint for off-chain indexers.
    /// No on-chain logic in this contract consults it. Callers must **not** treat its presence as
    /// proof of registry membership — query the registry contract directly to verify on-chain state.
    pub fn get_registry_ref(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::RegistryRef)
    }

    /// Returns the optional pending admin address waiting for [`LiquifactEscrow::accept_admin`],
    /// or [`None`] when no admin handover is in progress.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::PendingAdmin)
    }

    /// Return whether this escrow has a configured maturity time lock.
    ///
    /// `true` means [`InvoiceEscrow::maturity`] is positive and [`LiquifactEscrow::settle`] requires
    /// `Env::ledger().timestamp() >= maturity`. `false` means `maturity == 0`: there is no maturity
    /// gate, so a funded escrow can be settled immediately by the SME, subject to legal-hold and
    /// status guards.
    pub fn has_maturity_lock(env: Env) -> bool {
        Self::get_escrow(env).maturity > 0
    }

    /// Boolean view: is the escrow currently in a state where [`LiquifactEscrow::settle`]
    /// would succeed if invoked right now?
    ///
    /// Returns `true` when **all** of the following hold:
    ///
    /// 1. The escrow is initialized and [`InvoiceEscrow::status`] is `1` (funded).
    /// 2. No legal/compliance hold is active (`LegalHoldBlocksSettlement` would
    ///    not fire).
    /// 3. Either the escrow has no maturity lock (`maturity == 0`) OR
    ///    [`Env::ledger`] time is at or past `maturity`.
    ///
    /// Existing terminal states (`2` settled, `3` withdrawn, `4` cancelled)
    /// return `false` because settlement is no longer applicable; the open
    /// state (`0`) returns `false` because the funding target has not been met.
    ///
    /// **Pure view:** no storage mutation. The result reflects the ledger
    /// timestamp observed when the contract was called; reverify after any
    /// boundary-crossing transaction (especially near `maturity`).
    pub fn is_settleable(env: Env) -> bool {
        let escrow = Self::get_escrow(env.clone());
        if escrow.status != 1 {
            return false;
        }
        if Self::legal_hold_active(&env) {
            return false;
        }
        if escrow.maturity == 0 {
            return true;
        }
        env.ledger().timestamp() >= escrow.maturity
    }

    /// Move up to `amount` (capped by balance and [`MAX_DUST_SWEEP_AMOUNT`]) of the **funding token**
    /// from this contract to [`DataKey::Treasury`].
    ///
    /// # Terminal state requirement
    /// Only permitted when [`InvoiceEscrow::status`] is **2 (settled)**, **3 (withdrawn)**, or
    /// **4 (cancelled)**. Open (0) or funded (1) states reject the call so live principal cannot
    /// be swept as dust.
    ///
    /// # Liability floor invariant
    /// In **cancelled** (status 4) escrows, the sweep is rejected if it would reduce the
    /// contract's token balance below the amount still owed to investors who have not yet
    /// called [`LiquifactEscrow::refund`]:
    ///
    /// ```text
    /// outstanding = funded_amount - distributed_principal
    /// assert balance - sweep_amt >= outstanding
    /// ```
    ///
    /// `distributed_principal` ([`DataKey::DistributedPrincipal`]) is incremented atomically
    /// by [`LiquifactEscrow::refund`] each time an investor's principal is returned. This makes
    /// the invariant computable on-chain without iterating over all investor addresses.
    ///
    /// In **settled** (2) and **withdrawn** (3) states, disbursement is off-chain and this
    /// floor does not apply.
    ///
    /// # Authorization
    /// The configured **treasury** account must authorize this call; the admin cannot sweep unless
    /// it is also the treasury.
    ///
    /// Blocked while [`DataKey::LegalHold`] is active.
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes for legal hold, invalid sweep amount, non-terminal state,
    /// missing initialized addresses, empty balances, liability floor violation, and token
    /// transfer invariant failures.
    pub fn sweep_terminal_dust(env: Env, amount: i128) -> i128 {
        Self::guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksTreasuryDustSweep);
        ensure(&env, amount > 0, EscrowError::SweepAmountNotPositive);
        ensure(
            &env,
            amount <= MAX_DUST_SWEEP_AMOUNT,
            EscrowError::SweepAmountExceedsMax,
        );

        // env.clone(): env is used again after this call for treasury/token reads and publish.
        let escrow = Self::get_escrow(env.clone());
        ensure(
            &env,
            Self::is_terminal_status(escrow.status),
            EscrowError::DustSweepNotTerminal,
        );

        let treasury = Self::treasury_or_fail(&env);
        treasury.require_auth();

        let token_addr = Self::funding_token_or_fail(&env);
        let this = env.current_contract_address();

        let token = TokenClient::new(&env, &token_addr);
        let balance = token.balance(&this);
        ensure(&env, balance > 0, EscrowError::NoFundingTokenBalanceToSweep);
        let sweep_amt = amount.min(balance);
        ensure(&env, sweep_amt > 0, EscrowError::EffectiveSweepAmountZero);

        // Liability floor (cancelled escrows only): sweep must not reduce the balance below
        // principal still owed to investors who have not yet called refund().
        //
        // In settled (2) and withdrawn (3) states, disbursement is off-chain and
        // distributed_principal stays 0, so the floor is not applicable there.
        // In cancelled (4) state, refund() is the on-chain redemption path and increments
        // distributed_principal atomically, making the invariant computable here.
        //
        // outstanding = funded_amount - distributed_principal
        // Invariant: balance - sweep_amt >= outstanding
        if escrow.status == 4 {
            let distributed: i128 = env
                .storage()
                .instance()
                .get(&DataKey::DistributedPrincipal)
                .unwrap_or(0);
            let outstanding = escrow.funded_amount.saturating_sub(distributed);
            // sweep_amt <= balance (from amount.min(balance) above), so this subtraction is safe.
            let balance_after_sweep = balance - sweep_amt;
            ensure(
                &env,
                balance_after_sweep >= outstanding,
                EscrowError::SweepExceedsLiabilityFloor,
            );
        }

        external_calls::transfer_funding_token_with_balance_checks(
            &env,
            &token_addr,
            &this,
            &treasury,
            sweep_amt,
        );
        escrow.status = 2;
        env.storage().instance().set(&DataKey::Escrow, &escrow);
        escrow
    }

    /// Returns the full escrow snapshot ([`InvoiceEscrow`]) from [`DataKey::Escrow`].
    ///
    /// Emits [`EscrowError::EscrowNotInitialized`] (code 20) if called before [`LiquifactEscrow::init`].
    pub fn get_escrow(env: Env) -> InvoiceEscrow {
        env.storage()
            .instance()
            .set(&DataKey::SmeCollateralPledge, &pledge);
        CollateralRecordedEvt {
            invoice_id: escrow.invoice_id,
            amount,
        }
        .publish(&env);
        Ok(())
    }

    /// Return the current collateral pledge, if any.
    pub fn get_sme_collateral_commitment(env: Env) -> Option<CollateralPledge> {
        env.storage().instance().get(&DataKey::SmeCollateralPledge)
    }

    /// Retire a previously recorded collateral pledge.
    ///
    /// Metadata-only: no tokens are moved. Requires SME auth.
    ///
    /// Guard ordering (ADR-002):
    /// 1. Read-only existence check — returns [`EscrowError::NoCollateralToClear`] if absent.
    /// 2. `require_auth` on the SME address.
    /// 3. Remove storage entry and emit [`CollateralClearedEvt`].
    pub fn clear_sme_collateral_commitment(env: Env) -> Result<(), EscrowError> {
        // 1. Read-only existence check (no auth yet).
        let pledge: CollateralPledge = env
            .storage()
            .instance()
            .get(&DataKey::SmeCollateralPledge)
            .ok_or(EscrowError::NoCollateralToClear)?;

        // 2. Load escrow and require SME auth.
        let escrow = load_escrow_require_sme(&env)?;

        // 3. Remove entry and emit retirement event.
        env.storage()
            .instance()
            .remove(&DataKey::SmeCollateralPledge);
        CollateralClearedEvt {
            invoice_id: escrow.invoice_id,
            amount: pledge.amount,
        }
        .publish(&env);
        Ok(())
    }

    /// Returns the remaining funding capacity before the funding target is reached.
    ///
    /// The remaining capacity is calculated as `funding_target - funded_amount`.
    /// If the escrow is over-funded (i.e., `funded_amount > funding_target`), the return value
    /// is clamped to `0` via `saturating_sub` to ensure it never goes negative.
    ///
    /// This view is informational only. The `fund` method may still accept deposits
    /// that over-fund past the target as long as the escrow status is `0` (Open).
    ///
    /// # Errors
    /// Panics with [`EscrowError::EscrowNotInitialized`] if the escrow has not been initialized.
    ///
    /// # Complexity
    /// - Time Complexity: O(1) read from storage.
    /// - Space Complexity: O(1) in-memory logic.
    pub fn get_remaining_funding_capacity(env: Env) -> i128 {
        let escrow = Self::get_escrow(env);
        escrow
            .funding_target
            .saturating_sub(escrow.funded_amount)
            .max(0)
    }

    /// Returns `true` when `settle` would succeed without a legal-hold or maturity error:
    /// escrow is funded (`status == 1`), no legal hold is active, and maturity is satisfied
    /// (either `maturity == 0` or `ledger.timestamp() >= maturity`).
    pub fn is_settleable(env: Env) -> bool {
        let escrow = Self::get_escrow(env.clone());
        if escrow.status != 1 {
            return false;
        }
        let hold: bool = env
            .storage()
            .instance()
            .get(&DataKey::LegalHold)
            .unwrap_or(false);
        if hold {
            return false;
        }
        if escrow.maturity > 0 && env.ledger().timestamp() < escrow.maturity {
            return false;
        }
        true
    }

    /// Rotate the beneficiary (SME) address that receives liquidity on
    /// settlement / `withdraw`.
    ///
    /// Permitted only before settlement (`status` 0 = open or 1 = funded) and
    /// while no legal hold is active. Requires authorization from **both** the
    /// current SME and the admin, so the payout destination can never be changed
    /// unilaterally. A no-op rotation to the current address is rejected. Emits
    /// [`BeneficiaryRotated`] with the prior and new addresses and returns the
    /// updated escrow snapshot.
    ///
    /// # Errors
    ///
    /// | Condition | Typed error |
    /// |-----------|-------------|
    /// | Legal hold active | [`EscrowError::LegalHoldBlocksBeneficiaryRotation`] |
    /// | Escrow not open or funded | [`EscrowError::RotationNotOpen`] |
    /// | `new_sme_address == current SME` | [`EscrowError::NewSmeSameAsCurrent`] |
    pub fn rotate_beneficiary(env: Env, new_sme_address: Address) -> InvoiceEscrow {
        // Legal-hold gate (read-only).
        Self::guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksBeneficiaryRotation);

        let mut escrow = Self::get_escrow(env.clone());

        // Only permitted in pre-settlement states (open or funded).
        ensure(
            &env,
            escrow.status == 0 || escrow.status == 1,
            EscrowError::RotationNotOpen,
        );

        // Reject a no-op rotation to the current beneficiary.
        ensure(
            &env,
            new_sme_address != escrow.sme_address,
            EscrowError::NewSmeSameAsCurrent,
        );

        // Dual authorization: the outgoing SME and the admin must both sign.
        escrow.sme_address.require_auth();
        escrow.admin.require_auth();

        let prior_sme = escrow.sme_address.clone();
        escrow.sme_address = new_sme_address.clone();
        env.storage().instance().set(&DataKey::Escrow, &escrow);

        BeneficiaryRotated {
            name: symbol_short!("ben_rot"),
            invoice_id: escrow.invoice_id.clone(),
            prior_sme,
            new_sme: new_sme_address,
        }
        .publish(&env);

        escrow
    }

    /// Load the current escrow and require admin authorization in one step.
    ///
    /// Consolidates the repeated `let escrow = Self::get_escrow(env.clone()); escrow.admin.require_auth();`
    /// pattern used across multiple admin-gated entrypoints.
    fn load_escrow_require_admin(env: &Env) -> InvoiceEscrow {
        let escrow: InvoiceEscrow = env
            .storage()
            .instance()
            .get(&DataKey::Escrow)
            .unwrap_or_else(|| fail(env, EscrowError::EscrowNotInitialized));
        escrow.admin.require_auth();
        escrow
    }

    /// Load the current escrow and require SME authorization in one step.
    ///
    /// Consolidates the repeated `let escrow = Self::get_escrow(env.clone()); escrow.sme_address.require_auth();`
    /// pattern used across multiple SME-gated entrypoints.
    fn load_escrow_require_sme(env: &Env) -> InvoiceEscrow {
        let escrow: InvoiceEscrow = env
            .storage()
            .instance()
            .get(&DataKey::Escrow)
            .unwrap_or_else(|| fail(env, EscrowError::EscrowNotInitialized));
        escrow.sme_address.require_auth();
        escrow
    }

    /// Guard that the escrow is not under a legal hold.
    ///
    /// # Panics
    /// Panics with `err` if a legal hold is active.
    fn guard_not_legal_hold(env: &Env, err: EscrowError) {
        ensure(env, !Self::legal_hold_active(env), err);
    }

    /// Returns `true` when `status` is a terminal state (settled, withdrawn, or cancelled).
    fn is_terminal_status(status: u32) -> bool {
        status == 2 || status == 3 || status == 4
    }

    /// Guard that `escrow_status` equals `expected`.
    ///
    /// # Panics
    /// Panics with `err` if the statuses do not match.
    fn guard_status_eq(env: &Env, escrow_status: u32, expected: u32, err: EscrowError) {
        ensure(env, escrow_status == expected, err);
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Version).unwrap_or(0)
    }

    /// Returns true when funded_amount >= funding_target.
    /// Mirrors the exact funding completion predicate used
    /// by the funding transition logic.
    pub fn is_fully_funded(env: Env) -> bool {
        let escrow = Self::get_escrow(env);
        escrow.funded_amount >= escrow.funding_target
    }

    /// Get the optional funding deadline (ledger timestamp), returns None if not set.
    pub fn get_funding_deadline(env: Env) -> Option<u64> {
        env.storage().instance().get(&DataKey::FundingDeadline)
    }

    /// Check if funding has expired (deadline set and now > deadline).
    pub fn is_funding_expired(env: Env) -> bool {
        if let Some(deadline) = env.storage().instance().get(&DataKey::FundingDeadline) {
            env.ledger().timestamp() > deadline
        } else {
            false
        }
    }

    /// Whether a compliance/legal hold is active (defaults to `false` if unset).
    pub fn get_legal_hold(env: Env) -> bool {
        Self::legal_hold_active(&env)
    }

    /// Configured minimum delay between [`LiquifactEscrow::request_clear_legal_hold`]
    /// and [`LiquifactEscrow::set_legal_hold(env, false)`]. Defaults to `0`.
    pub fn get_legal_hold_clear_delay(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LegalHoldClearDelay)
            .unwrap_or(0)
    }

    /// Reserved minimum ledger timestamp at which a pending legal-hold clear may be applied.
    /// `None` means no request has been recorded.
    pub fn get_legal_hold_clearable_at(env: Env) -> Option<u64> {
        env.storage().instance().get(&DataKey::LegalHoldClearableAt)
    }

    /// Minimum principal per [`LiquifactEscrow::fund`] or [`LiquifactEscrow::fund_with_commitment`] call
    /// in token base units; `0` means no extra floor beyond “amount must be positive”.
    ///
    /// **Ceilings:** [`InvoiceEscrow::funding_target`] and over-funding behavior are unchanged; the floor
    /// applies to **each** call, so follow-on deposits from the same investor must also meet the floor.
    pub fn get_min_contribution_floor(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::MinContributionFloor)
            .unwrap_or(0)
    }

    /// Optional cap on **distinct** investor addresses (`prev == 0` at fund time); [`None`] if unlimited.
    ///
    /// Reflects the current stored cap, including any admin reduction via
    /// [`LiquifactEscrow::lower_max_unique_investors`].
    pub fn get_max_unique_investors_cap(env: Env) -> Option<u32> {
        env.storage()
            .instance()
            .get(&DataKey::MaxUniqueInvestorsCap)
    }

    /// Optional cap on total principal for a single investor address.
    /// Absent ⇒ unlimited. Enforced on every deposit.
    pub fn get_max_per_investor_cap(env: Env) -> Option<i128> {
        env.storage().instance().get(&DataKey::MaxPerInvestorCap)
    }

    /// Distinct funders counted so far (each address counted once when it first receives principal).
    ///
    /// **Sybil:** this limits distinct **chain accounts**, not real-world persons; Sybil resistance is
    /// not a goal of this counter.
    pub fn get_unique_funder_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::UniqueFunderCount)
            .unwrap_or(0)
    }

    /// Bundles multiple read-only values to return a comprehensive summary of the escrow state
    /// in a single host invocation.
    pub fn get_escrow_summary(env: Env) -> EscrowSummary {
        let escrow = Self::get_escrow(env.clone());
        let legal_hold = Self::get_legal_hold(env.clone());
        let funding_close_snapshot_opt = Self::get_funding_close_snapshot(env.clone());
        let unique_funder_count = Self::get_unique_funder_count(env.clone());
        let is_allowlist_active = Self::is_allowlist_active(env.clone());
        let schema_version = Self::get_version(env.clone());
        let sme_collateral_commitment = Self::get_sme_collateral_commitment(env.clone());
        let primary_attestation_hash = Self::get_primary_attestation_hash(env.clone());
        let attestation_append_log = Self::get_attestation_append_log(env.clone());

        let funding_close_snapshot = match funding_close_snapshot_opt {
            Some(snap) => EscrowCloseSnapshot::Some(snap),
            None => EscrowCloseSnapshot::None,
        };

        let sme_collateral_commitment = match sme_collateral_commitment {
            Some(collateral) => CollateralCommitmentSnapshot::Some(collateral),
            None => CollateralCommitmentSnapshot::None,
        };

        EscrowSummary {
            escrow,
            has_maturity_lock: Self::has_maturity_lock(env.clone()),
            legal_hold,
            funding_close_snapshot,
            unique_funder_count,
            is_allowlist_active,
            schema_version,
            sme_collateral_commitment,
            has_primary_attestation: primary_attestation_hash.is_some(),
            attestation_log_length: attestation_append_log.len(),
        }
    }

    /// Bind a **primary** 32-byte digest (e.g. SHA-256 of an IPFS CID or document bundle). **Single-set:**
    /// the call succeeds only while no primary hash exists; use [`LiquifactEscrow::append_attestation_digest`]
    /// for an append-only audit trail.
    ///
    /// **Authorization:** [`InvoiceEscrow::admin`]. **Frontrunning:** whichever binding transaction lands
    /// first wins; observers must read on-chain state (or parse events) after finality—there is no replay lock.
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes when the escrow is uninitialized or the primary digest has
    /// already been bound.
    pub fn bind_primary_attestation_hash(env: Env, digest: BytesN<32>) {
        let escrow = Self::load_escrow_require_admin(&env);
        ensure(
            &env,
            !env.storage()
                .instance()
                .has(&DataKey::PrimaryAttestationHash),
            EscrowError::PrimaryAttestationAlreadyBound,
        );
        env.storage()
            .instance()
            .set(&DataKey::PrimaryAttestationHash, &digest);
        PrimaryAttestationBound {
            name: symbol_short!("att_bind"),
            invoice_id: escrow.invoice_id.clone(),
            digest: digest.clone(),
        }
        .publish(&env);
    }

    pub fn get_primary_attestation_hash(env: Env) -> Option<BytesN<32>> {
        env.storage()
            .instance()
            .get(&DataKey::PrimaryAttestationHash)
    }

    /// Append a digest to a bounded on-chain log (see [`MAX_ATTESTATION_APPEND_ENTRIES`]) for **versioned**
    /// or incremental attestation updates. Does not replace [`LiquifactEscrow::bind_primary_attestation_hash`].
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes when the escrow is uninitialized or the append log is full.
    pub fn append_attestation_digest(env: Env, digest: BytesN<32>) {
        let escrow = Self::load_escrow_require_admin(&env);

        let mut log: Vec<BytesN<32>> = env
            .storage()
            .instance()
            .get(&DataKey::AttestationAppendLog)
            .unwrap_or_else(|| Vec::new(&env));
        ensure(
            &env,
            log.len() < MAX_ATTESTATION_APPEND_ENTRIES,
            EscrowError::AttestationAppendLogCapacityReached,
        );
        let idx = log.len();
        log.push_back(digest.clone());
        env.storage()
            .instance()
            .set(&DataKey::AttestationAppendLog, &log);

        AttestationDigestAppended {
            name: symbol_short!("att_app"),
            invoice_id: escrow.invoice_id.clone(),
            index: idx,
            digest,
        }
        .publish(&env);
    }

    pub fn get_attestation_append_log(env: Env) -> Vec<BytesN<32>> {
        env.storage()
            .instance()
            .get(&DataKey::AttestationAppendLog)
            .unwrap_or_else(|| Vec::new(&env))
    }

    // --- Persistent per-investor storage helpers ---
    fn get_persistent_investor_contribution(env: &Env, investor: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::InvestorContribution(investor))
            .unwrap_or(0)
    }

    fn set_persistent_investor_contribution(env: &Env, investor: Address, amount: i128) {
        let key = DataKey::InvestorContribution(investor);
        env.storage().persistent().set(&key, &amount);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
            PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
        );
    }

    fn get_persistent_investor_effective_yield(env: &Env, investor: Address) -> Option<i64> {
        env.storage()
            .persistent()
            .get(&DataKey::InvestorEffectiveYield(investor))
    }

    fn set_persistent_investor_effective_yield(env: &Env, investor: Address, value: i64) {
        let key = DataKey::InvestorEffectiveYield(investor);
        env.storage().persistent().set(&key, &value);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
            PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
        );
    }

    fn get_persistent_investor_claim_not_before(env: &Env, investor: Address) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::InvestorClaimNotBefore(investor))
            .unwrap_or(0)
    }

    fn set_persistent_investor_claim_not_before(env: &Env, investor: Address, value: u64) {
        let key = DataKey::InvestorClaimNotBefore(investor);
        env.storage().persistent().set(&key, &value);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
            PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
        );
    }

    fn get_persistent_investor_claimed(env: &Env, investor: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::InvestorClaimed(investor))
            .unwrap_or(false)
    }

    fn set_persistent_investor_claimed(env: &Env, investor: Address, value: bool) {
        let key = DataKey::InvestorClaimed(investor);
        env.storage().persistent().set(&key, &value);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
            PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
        );
    }

    /// Public API: contribution recorded for `investor` (persistent storage).
    pub fn get_contribution(env: Env, investor: Address) -> i128 {
        Self::get_persistent_investor_contribution(&env, investor)
    }

    /// Returns a paginated list of investor addresses who have contributed to this escrow.
    ///
    /// Legacy instances that predate this feature will return an empty list (backward compatible under ADR-007).
    ///
    /// # Arguments
    /// * `start` - The starting index (0-based) of the pagination.
    /// * `limit` - The maximum number of investor addresses to return (capped at a hard limit of 50).
    ///
    /// # Returns
    /// A `Vec<Address>` containing the investor addresses within the requested page.
    pub fn get_investors(env: Env, start: u32, limit: u32) -> Vec<Address> {
        let index: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::InvestorIndex)
            .unwrap_or_else(|| Vec::new(&env));

        let len = index.len();
        if start >= len || limit == 0 {
            return Vec::new(&env);
        }

        let actual_limit = limit.min(50);
        let end = (start + actual_limit).min(len);

        let mut result = Vec::new(&env);
        for i in start..end {
            result.push_back(index.get(i).unwrap());
        }
        result
    }


    /// Pro-rata denominator captured when the escrow first became **funded**; [`None`] until then.
    ///
    /// The snapshot is write-once. It records the full `funded_amount` at the threshold-crossing
    /// funding call, including any over-funding past `funding_target`, plus the close ledger time
    /// and sequence used by off-chain auditors.
    pub fn get_funding_close_snapshot(env: Env) -> Option<FundingCloseSnapshot> {
        env.storage().instance().get(&DataKey::FundingCloseSnapshot)
    }

    /// Returns the ledger timestamp (seconds since Unix epoch) at which [`LiquifactEscrow::settle`]
    /// transitioned status from 1 → 2, or [`None`] if the escrow has not yet been settled.
    ///
    /// **Additive-key policy (ADR-007):** legacy escrow instances that were settled before this key
    /// was introduced will return [`None`] because [`DataKey::SettledAt`] was never written.
    ///
    /// # Returns
    /// - `Some(timestamp)` — the ledger timestamp at the moment `settle()` was called.
    /// - `None` — escrow is not yet settled, or is a legacy instance predating this key.
    pub fn get_settled_at(env: Env) -> Option<u64> {
        env.storage().instance().get(&DataKey::SettledAt)
    }

    /// Returns `true` if the escrow can be settled now, `false` otherwise.
    ///
    /// Checks:
    /// - Status must be 1 (funded)
    /// - No legal hold active
    /// - If `maturity > 0`, ledger timestamp must be `>= maturity`
    ///
    /// # Panics
    /// Panics with [`EscrowError::EscrowNotInitialized`] if `init` has not been called.
    pub fn is_settleable(env: Env) -> bool {
        if Self::legal_hold_active(&env) {
            return false;
        }
        let escrow = Self::get_escrow(env.clone());
        if escrow.status != 1 {
            return false;
        }
        if escrow.maturity > 0 {
            let now = env.ledger().timestamp();
            return now >= escrow.maturity;
        }
        true
    }

    /// Effective yield (bps) for this investor after their **first** deposit; later [`LiquifactEscrow::fund`]
    /// calls add principal at this rate. Defaults to [`InvoiceEscrow::yield_bps`] when unset (legacy positions).
    ///
    /// Note: reads `DataKey::Escrow` for the base yield fallback; callers that already hold the
    /// escrow should prefer reading `DataKey::InvestorEffectiveYield` directly.
    ///
    /// # See also
    ///
    /// [`LiquifactEscrow::get_effective_yield_bps`] is a semantic alias that exposes the **resolved**
    /// rate the payout math uses, with documentation framed around the
    /// stored-tier-or-base resolution rather than the per-investor storage slot. The two views
    /// return identical values for the same investor; prefer `get_effective_yield_bps` when the
    /// intent is "the rate [`LiquifactEscrow::compute_investor_payout`] will actually apply."
    pub fn get_investor_yield_bps(env: Env, investor: Address) -> i64 {
        // env.clone(): env is used again after this call for the InvestorEffectiveYield read.
        let escrow = Self::get_escrow(env.clone());
        Self::get_persistent_investor_effective_yield(&env, investor.clone())
            .unwrap_or(escrow.yield_bps)
    }

    /// Returns the **resolved effective yield (bps)** an investor would earn at settlement — exactly
    /// the rate [`LiquifactEscrow::compute_investor_payout`] applies when computing the coupon.
    ///
    /// # Resolution (identical to `compute_investor_payout`)
    ///
    /// ```text
    /// effective_yield_bps = InvestorEffectiveYield(investor)   // tier locked at first deposit
    ///                       .unwrap_or(escrow.yield_bps)        // else the escrow base yield
    /// ```
    ///
    /// - **Tiered investor**: returns the tier `yield_bps` selected at their first
    ///   [`LiquifactEscrow::fund_with_commitment`] deposit (see [`DataKey::InvestorEffectiveYield`]).
    /// - **Base-only / non-tiered investor**: returns the escrow base [`InvoiceEscrow::yield_bps`].
    /// - **Unknown investor** (never funded): returns the escrow base [`InvoiceEscrow::yield_bps`],
    ///   i.e. the rate they *would* receive at the base level — no per-investor slot exists yet.
    ///
    /// # Stored vs resolved (vs [`LiquifactEscrow::get_investor_yield_bps`])
    ///
    /// [`DataKey::InvestorEffectiveYield`] is the **stored** per-investor slot: present only after a
    /// tiered first deposit, absent otherwise. This view returns the **resolved** value — the stored
    /// slot when present, otherwise the base yield fallback — so integrators read the same number the
    /// payout math uses without re-implementing the `unwrap_or` fallback themselves.
    /// `get_investor_yield_bps` is a historical alias with the same return value.
    ///
    /// # Authorization
    ///
    /// None — pure read; no auth required and no state mutation.
    pub fn get_effective_yield_bps(env: Env, investor: Address) -> i64 {
        // env.clone(): env is used again after this call for the InvestorEffectiveYield read.
        // Mirrors compute_investor_payout exactly: tier slot when set, else escrow base yield.
        let escrow = Self::get_escrow(env.clone());
        Self::get_persistent_investor_effective_yield(&env, investor.clone())
            .unwrap_or(escrow.yield_bps)
    }

    /// Earliest ledger timestamp for [`LiquifactEscrow::claim_investor_payout`]; `0` if not gated.
    pub fn get_investor_claim_not_before(env: Env, investor: Address) -> u64 {
        Self::get_persistent_investor_claim_not_before(&env, investor)
    }

    /// Retrieve the currently recorded SME collateral commitment metadata from storage.
    /// Returns `None` if no commitment has been recorded yet.
    pub fn get_sme_collateral_commitment(env: Env) -> Option<SmeCollateralCommitment> {
        env.storage().instance().get(&DataKey::SmeCollateralPledge)
    }

    pub fn revoke_attestation_digest(env: Env, index: u32) {
        let escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();

        let log: Vec<BytesN<32>> = env
            .storage()
            .instance()
            .get(&DataKey::AttestationAppendLog)
            .unwrap_or_else(|| Vec::new(&env));
        ensure(
            &env,
            index < log.len(),
            EscrowError::AttestationIndexOutOfRange,
        );
        ensure(
            &env,
            !env.storage()
                .instance()
                .has(&DataKey::AttestationRevoked(index)),
            EscrowError::AttestationAlreadyRevoked,
        );

        env.storage()
            .instance()
            .set(&DataKey::AttestationRevoked(index), &true);

        AttestationDigestRevoked {
            name: symbol_short!("att_rev"),
            invoice_id: escrow.invoice_id.clone(),
            index,
        }
        .publish(&env);
    }

    /// Returns `true` when the append-log entry at `index` has been revoked via
    /// [`LiquifactEscrow::revoke_attestation_digest`].
    /// Defaults to `false` when the key is absent (not revoked).
    pub fn is_attestation_revoked(env: Env, index: u32) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::AttestationRevoked(index))
            .unwrap_or(false)
    }

    /// Clears the revocation marker for a previously revoked append-log entry.
    ///
    /// Use this to correct a mistaken revocation (fat-finger on a 0-based index)
    /// without polluting the audit chain permanently.
    ///
    /// # Authorization
    /// Requires `InvoiceEscrow::admin` auth.
    ///
    /// # Guard ordering (ADR-002)
    /// Range check → revocation-state check → `require_auth` → storage mutation.
    ///
    /// # Errors
    /// - [`EscrowError::AttestationIndexOutOfRange`] if `index >= log.len()`.
    /// - [`EscrowError::AttestationNotRevoked`] if the index is not currently revoked.
    pub fn unrevoke_attestation_digest(env: Env, index: u32) {
        let log: Vec<BytesN<32>> = env
            .storage()
            .instance()
            .get(&DataKey::AttestationAppendLog)
            .unwrap_or_else(|| Vec::new(&env));
        ensure(&env, index < log.len(), EscrowError::AttestationIndexOutOfRange);
        ensure(
            &env,
            env.storage()
                .instance()
                .has(&DataKey::AttestationRevoked(index)),
            EscrowError::AttestationNotRevoked,
        );

        let escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();

        env.storage()
            .instance()
            .remove(&DataKey::AttestationRevoked(index));

        AttestationDigestUnrevoked {
            name: symbol_short!("att_unrev"),
            invoice_id: escrow.invoice_id.clone(),
            index,
        }
        .publish(&env);
    }

    pub fn is_investor_claimed(env: Env, investor: Address) -> bool {
        Self::get_persistent_investor_claimed(&env, investor)
    }

    /// Record or replace the optional SME collateral commitment metadata.
    ///
    /// **Metadata-only:** this writes [`DataKey::SmeCollateralPledge`] and emits
    /// [`CollateralRecordedEvt`]. It does not transfer tokens, reserve balances, verify custody,
    /// create an on-chain encumbrance, or block any contract flows (such as settlement, withdrawals,
    /// or claims).
    ///
    /// # Authorization
    /// - Requires the signature of the configured SME (`InvoiceEscrow::sme_address`). Enforced via
    ///   `sme_address.require_auth()` during execution.
    ///
    /// # Validation Rules
    /// - **Positive Amount:** The `amount` parameter must be strictly positive (`amount > 0`).
    /// - **Non-empty Asset Symbol:** The `asset` parameter must be a non-empty Symbol (not equal to `Symbol::new(&env, "")`).
    /// - **Monotonic Timestamp:** When replacing an existing commitment, the current ledger timestamp must not
    ///   be earlier than the prior `recorded_at` value (`now >= prior.recorded_at`).
    ///
    /// # Errors
    /// - [`EscrowError::CollateralAmountNotPositive`] if `amount <= 0`.
    /// - [`EscrowError::CollateralAssetEmpty`] if `asset` is empty.
    /// - [`EscrowError::CollateralTimestampBackwards`] if the replacement timestamp is in the past.
    /// - Standard uninitialized check via `load_escrow_require_sme`.
    pub fn record_sme_collateral_commitment(
        env: Env,
        asset: Symbol,
        amount: i128,
    ) -> SmeCollateralCommitment {
        ensure(&env, amount > 0, EscrowError::CollateralAmountNotPositive);
        ensure(
            &env,
            asset != Symbol::new(&env, ""),
            EscrowError::CollateralAssetEmpty,
        );

        // env.clone(): env is used again after this call for storage read/write, timestamp, and publish.
        let escrow = Self::load_escrow_require_sme(&env);

        let now = env.ledger().timestamp();
        let prior: Option<SmeCollateralCommitment> =
            env.storage().instance().get(&DataKey::SmeCollateralPledge);
        let prior_amount = prior.as_ref().map(|c| c.amount).unwrap_or(0);

        if let Some(ref existing) = prior {
            ensure(
                &env,
                now >= existing.recorded_at,
                EscrowError::CollateralTimestampBackwards,
            );
        }

        let commitment = SmeCollateralCommitment {
            asset,
            amount,
            recorded_at: now,
        };
        env.storage()
            .instance()
            .set(&DataKey::SmeCollateralPledge, &commitment);

        CollateralRecordedEvt {
            name: symbol_short!("coll_rec"),
            invoice_id: escrow.invoice_id.clone(),
            amount,
            prior_amount,
        }
        .publish(&env);

        commitment
    }

    /// Set or clear compliance hold. Only the **current** [`InvoiceEscrow::admin`] may call.
    ///
    /// **Clearing:** always requires the current admin's authorization — there is no timelock,
    /// council override, or break-glass entrypoint. After
    /// [`LiquifactEscrow::propose_admin`] and [`LiquifactEscrow::accept_admin`], only the **new**
    /// admin can clear a persisted hold.
    ///
    /// **Governance posture:** production `admin` must be a multisig or governed contract so
    /// hold + key loss cannot strand funds without an off-chain recovery vote that executes
    /// `propose_admin`, `accept_admin`, then `clear_legal_hold`. See
    /// `docs/escrow-legal-hold.md`.
    pub fn set_legal_hold(env: Env, active: bool) {
        let escrow = Self::load_escrow_require_admin(&env);

        if !active && Self::legal_hold_active(&env) {
            let delay = Self::get_legal_hold_clear_delay(env.clone());
            if delay > 0 {
                let clearable_at: Option<u64> =
                    env.storage().instance().get(&DataKey::LegalHoldClearableAt);
                ensure(
                    &env,
                    clearable_at.is_some(),
                    EscrowError::LegalHoldClearRequestMissing,
                );
                let now = env.ledger().timestamp();
                ensure(
                    &env,
                    now >= clearable_at.unwrap(),
                    EscrowError::LegalHoldClearNotReady,
                );
            }
        }

        env.storage()
            .instance()
            .remove(&DataKey::LegalHoldClearableAt);

        env.storage().instance().set(&DataKey::LegalHold, &active);

        LegalHoldChanged {
            name: symbol_short!("legalhld"),
            invoice_id: escrow.invoice_id.clone(),
            active: if active { 1 } else { 0 },
        }
        .publish(&env);
    }

    /// Schedule a compliance hold clear window. The current admin must authorize.
    ///
    /// If a non-zero clear delay is configured, the hold may not be lifted until the
    /// returned ledger timestamp is reached.
    ///
    /// # Errors
    ///
    /// | Condition | Typed error |
    /// |-----------|-------------|
    /// | `timestamp + delay` overflows | [`EscrowError::LegalHoldClearDelayOverflow`] |
    pub fn request_clear_legal_hold(env: Env) {
        let escrow = Self::load_escrow_require_admin(&env);

        let now = env.ledger().timestamp();
        let delay = Self::get_legal_hold_clear_delay(env.clone());
        let clearable_at = if delay == 0 {
            now
        } else {
            now.checked_add(delay)
                .unwrap_or_else(|| fail(&env, EscrowError::LegalHoldClearDelayOverflow))
        };

        env.storage()
            .instance()
            .set(&DataKey::LegalHoldClearableAt, &clearable_at);

        LegalHoldClearRequested {
            name: symbol_short!("lh_req"),
            invoice_id: escrow.invoice_id.clone(),
            clearable_at,
        }
        .publish(&env);
    }

    /// Enable or disable the investor allowlist gate.
    ///
    /// When enabled (active = true), only addresses with [`DataKey::InvestorAllowlisted`] set to
    /// `true` may call [`LiquifactEscrow::fund`] or [`LiquifactEscrow::fund_with_commitment`].
    /// When disabled (active = false), the gate is bypassed and any address may fund.
    ///
    /// The toggle state is stored in **instance** storage ([`DataKey::AllowlistActive`]) and
    /// defaults to `false` (disabled) when absent. Per-address allowlist entries in
    /// [`DataKey::InvestorAllowlisted`] live in **persistent** storage and persist independently
    /// of the toggle state—disabling the gate does not delete entries.
    ///
    /// # Authorization
    /// Requires admin authorization via [`Address::require_auth()`].
    ///
    /// # Events
    /// Emits [`AllowlistEnabledChanged`] with the new state.
    ///
    /// # Storage
    /// - Writes to instance storage: [`DataKey::AllowlistActive`]
    /// - Bumps instance storage TTL via [`Env::storage().instance().extend_ttl()`]
    ///
    /// # See also
    /// - [`LiquifactEscrow::is_allowlist_active`] — read the current toggle state
    /// - [`LiquifactEscrow::set_investor_allowlisted`] — set per-address allowlist entry
    /// - [`LiquifactEscrow::set_investors_allowlisted`] — batch set per-address entries
    /// - [`docs/escrow-allowlist.md`](../docs/escrow-allowlist.md) — full allowlist model documentation
    pub fn set_allowlist_active(env: Env, active: bool) {
        let escrow = Self::load_escrow_require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::AllowlistActive, &active);
        AllowlistEnabledChanged {
            name: symbol_short!("al_ena"),
            invoice_id: escrow.invoice_id.clone(),
            active: if active { 1 } else { 0 },
        }
        .publish(&env);
    }

    /// Read whether the investor allowlist gate is currently active.
    ///
    /// Returns `true` when [`DataKey::AllowlistActive`] is set to `true`, `false` otherwise.
    /// The key lives in instance storage and defaults to `false` when absent.
    ///
    /// This is a read-only function and does not require authorization.
    ///
    /// # Returns
    /// - `true` if the allowlist gate is active (only allowlisted addresses may fund)
    /// - `false` if the allowlist gate is inactive (any address may fund)
    ///
    /// # See also
    /// - [`LiquifactEscrow::set_allowlist_active`] — toggle the gate state
    /// - [`LiquifactEscrow::is_investor_allowlisted`] — check if a specific address is allowlisted
    pub fn is_allowlist_active(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::AllowlistActive)
            .unwrap_or(false)
    }

    /// Set whether a specific investor address is allowlisted.
    ///
    /// Writes a boolean entry to **persistent** storage under [`DataKey::InvestorAllowlisted`].
    /// The entry has an independent TTL per address and persists even when the allowlist gate
    /// is disabled via [`LiquifactEscrow::set_allowlist_active`].
    ///
    /// When the allowlist gate is active, [`LiquifactEscrow::fund`] and
    /// [`LiquifactEscrow::fund_with_commitment`] check this entry and reject funding with
    /// [`EscrowError::InvestorNotAllowlisted`] if the entry is absent or `false`.
    ///
    /// # Authorization
    /// Requires admin authorization via [`Address::require_auth()`].
    ///
    /// # Events
    /// Emits [`InvestorAllowlistChanged`] for the address.
    ///
    /// # Storage
    /// - Writes to persistent storage: [`DataKey::InvestorAllowlisted(investor)`]
    /// - Bumps persistent storage TTL via [`Env::storage().persistent().extend_ttl()`]
    ///   with [`PERSISTENT_TTL_MIN_EXTENSION_LEDGERS`] (≈1 hour at 1 ledger/sec)
    ///
    /// # Arguments
    /// - `investor` — The address to allowlist or remove
    /// - `allowed` — `true` to allowlist, `false` to remove from allowlist
    ///
    /// # See also
    /// - [`LiquifactEscrow::is_investor_allowlisted`] — check if an address is allowlisted
    /// - [`LiquifactEscrow::set_investors_allowlisted`] — batch variant for multiple addresses
    /// - [`docs/escrow-allowlist.md`](../docs/escrow-allowlist.md) — full allowlist model documentation
    pub fn set_investor_allowlisted(env: Env, investor: Address, allowed: bool) {
        let escrow = Self::load_escrow_require_admin(&env);
        env.storage()
            .persistent()
            .set(&DataKey::InvestorAllowlisted(investor.clone()), &allowed);

        InvestorAllowlistChanged {
            name: symbol_short!("al_set"),
            invoice_id: escrow.invoice_id.clone(),
            investor,
            allowed: if allowed { 1 } else { 0 },
        }
        .publish(&env);
    }

    /// Batch add or remove investors from the allowlist.
    ///
    /// Accepts a `Vec<Address>` and a single `allowed` flag. Requires admin authorization
    /// once for the entire batch. The call is rejected for empty vectors or vectors longer than
    /// [`MAX_INVESTOR_ALLOWLIST_BATCH`] (32) to keep storage and CPU bounded.
    ///
    /// **Invariant:** the end state and emitted events are identical to calling
    /// [`LiquifactEscrow::set_investor_allowlisted`] individually for each element in `investors`.
    /// Each address receives its own persistent storage write, TTL bump, and event emission.
    ///
    /// # Authorization
    /// Requires admin authorization via [`Address::require_auth()`] once for the entire batch.
    ///
    /// # Events
    /// Emits one [`InvestorAllowlistChanged`] event per address in the batch.
    ///
    /// # Storage
    /// - Writes to persistent storage: [`DataKey::InvestorAllowlisted(investor)`] for each address
    /// - Bumps persistent storage TTL via [`Env::storage().persistent().extend_ttl()`]
    ///   with [`PERSISTENT_TTL_MIN_EXTENSION_LEDGERS`] (≈1 hour at 1 ledger/sec) for each entry
    ///
    /// # Arguments
    /// - `investors` — Vector of addresses to allowlist or remove (must be non-empty and ≤32 entries)
    /// - `allowed` — `true` to allowlist all addresses, `false` to remove all from allowlist
    ///
    /// # Errors
    /// - [`EscrowError::InvestorBatchEmpty`] (70) — when `investors` is empty
    /// - [`EscrowError::InvestorBatchTooLarge`] (71) — when `investors.len() > MAX_INVESTOR_ALLOWLIST_BATCH`
    ///
    /// # See also
    /// - [`LiquifactEscrow::set_investor_allowlisted`] — single-address variant
    /// - [`LiquifactEscrow::is_investor_allowlisted`] — check if an address is allowlisted
    /// - [`docs/escrow-allowlist.md`](../docs/escrow-allowlist.md) — full allowlist model documentation
    pub fn set_investors_allowlisted(env: Env, investors: Vec<Address>, allowed: bool) {
        let escrow = Self::load_escrow_require_admin(&env);

        let n = investors.len();
        ensure(&env, n > 0, EscrowError::InvestorBatchEmpty);
        ensure(
            &env,
            n <= MAX_INVESTOR_ALLOWLIST_BATCH,
            EscrowError::InvestorBatchTooLarge,
        );

        // Iterate and perform per-address persistent storage write and event emission.
        for i in 0..n {
            let inv = investors.get(i).unwrap();
            env.storage()
                .persistent()
                .set(&DataKey::InvestorAllowlisted(inv.clone()), &allowed);

            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: escrow.invoice_id.clone(),
                investor: inv.clone(),
                allowed: if allowed { 1 } else { 0 },
            }
            .publish(&env);
        }

        // Emit exactly one batch-level summary event so indexers can distinguish a
        // batch call from N individual `set_investor_allowlisted` calls.
        InvestorAllowlistBatchApplied {
            name: symbol_short!("al_batch"),
            invoice_id: escrow.invoice_id.clone(),
            batch_size: n,
            allowed: if allowed { 1 } else { 0 },
        }
        .publish(&env);
    }

    /// Check whether a specific investor address is allowlisted.
    ///
    /// Reads from **persistent** storage under [`DataKey::InvestorAllowlisted`]. Returns `false`
    /// when the entry is absent or explicitly set to `false`. This default-to-deny behavior
    /// ensures that archived/evicted entries or addresses never added to the allowlist are
    /// treated as not allowlisted when the gate is active.
    ///
    /// This is a read-only function and does not require authorization.
    ///
    /// # Arguments
    /// - `investor` — The address to check
    ///
    /// # Returns
    /// - `true` if the address has an allowlist entry set to `true`
    /// - `false` if the entry is absent, `false`, or has been archived
    ///
    /// # See also
    /// - [`LiquifactEscrow::set_investor_allowlisted`] — set per-address allowlist entry
    /// - [`LiquifactEscrow::set_allowlist_active`] — toggle the gate state
    pub fn is_investor_allowlisted(env: Env, investor: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::InvestorAllowlisted(investor))
            .unwrap_or(false)
    }

    /// Convenience alias for [`LiquifactEscrow::set_legal_hold`] with `active = false`.
    pub fn clear_legal_hold(env: Env) {
        Self::set_legal_hold(env, false);
    }

    pub fn update_funding_target(env: Env, new_target: i128) -> InvoiceEscrow {
        let mut escrow = Self::load_escrow_require_admin(&env);

        ensure(&env, new_target > 0, EscrowError::TargetNotPositive);
        Self::guard_status_eq(&env, escrow.status, 0, EscrowError::TargetUpdateNotOpen);
        ensure(
            &env,
            new_target >= escrow.funded_amount,
            EscrowError::TargetBelowFundedAmount,
        );

        let old_target = escrow.funding_target;
        escrow.funding_target = new_target;

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        FundingTargetUpdated {
            name: symbol_short!("fund_tgt"),
            invoice_id: escrow.invoice_id.clone(),
            old_target,
            new_target,
        }
        .publish(&env);

        escrow
    }

    /// Update or clear the optional funding deadline while the escrow is still **open** (status == 0).
    ///
    /// Only the current [`InvoiceEscrow::admin`] may call. This is the only entrypoint that
    /// mutates [`DataKey::FundingDeadline`] after [`LiquifactEscrow::init`].
    ///
    /// # Parameters
    ///
    /// - `new_deadline`: `Some(u64)` sets a new ledger timestamp; `None` removes the deadline.
    ///
    /// # Validation
    ///
    /// - Admin authorization required.
    /// - Escrow must be in **open** state (`status == 0`).
    /// - When `Some(d)`: `d` must be strictly greater than the current ledger timestamp
    ///   (same rule as [`LiquifactEscrow::init`]).
    /// - When `None`: the stored [`DataKey::FundingDeadline`] is removed; funding becomes
    ///   unrestricted by time.
    ///
    /// # Events
    ///
    /// Emits [`FundingDeadlineUpdated`] with the prior and new deadline values.
    ///
    /// # Errors
    ///
    /// | Condition | Typed error |
    /// |-----------|-------------|
    /// | Escrow not open | [`EscrowError::FundingDeadlineUpdateNotOpen`] |
    /// | `Some(d)` and `d <= now` | [`EscrowError::FundingDeadlinePassed`] |
    pub fn update_funding_deadline(env: Env, new_deadline: Option<u64>) {
        let escrow = Self::load_escrow_require_admin(&env);

        ensure(
            &env,
            escrow.status == 0,
            EscrowError::FundingDeadlineUpdateNotOpen,
        );

        let prior: Option<u64> = env.storage().instance().get(&DataKey::FundingDeadline);

        match new_deadline {
            Some(d) => {
                ensure(
                    &env,
                    d > env.ledger().timestamp(),
                    EscrowError::FundingDeadlinePassed,
                );
                env.storage().instance().set(&DataKey::FundingDeadline, &d);
            }
            None => {
                env.storage().instance().remove(&DataKey::FundingDeadline);
            }
        }

        FundingDeadlineUpdated {
            name: symbol_short!("fund_dl"),
            invoice_id: escrow.invoice_id.clone(),
            prior_deadline: prior,
            new_deadline,
        }
        .publish(&env);
    }

    /// Lower the configured distinct-investor cap while the escrow is still open.
    ///
    /// This is admin-only and intentionally cannot raise a cap or impose one on an unlimited
    /// escrow. Existing investors remain able to add principal after the cap is lowered; only new
    /// investor addresses are blocked once `UniqueFunderCount >= new_cap`.
    ///
    /// # Panics
    /// - If the escrow is not open.
    /// - If no unique-investor cap was configured at initialization.
    /// - If `new_cap` is not strictly lower than the current cap.
    /// - If `new_cap` is below the current unique funder count.
    pub fn lower_max_unique_investors(env: Env, new_cap: u32) -> u32 {
        let escrow = Self::load_escrow_require_admin(&env);

        Self::guard_status_eq(&env, escrow.status, 0, EscrowError::CapLowerNotOpen);

        let old_cap: Option<u32> = env
            .storage()
            .instance()
            .get(&DataKey::MaxUniqueInvestorsCap);
        ensure(
            &env,
            old_cap.is_some(),
            EscrowError::NoInvestorCapConfigured,
        );
        let old_cap = old_cap.unwrap();
        let unique_count = Self::get_unique_funder_count(env.clone());

        ensure(&env, new_cap < old_cap, EscrowError::NewCapNotLower);
        ensure(
            &env,
            new_cap >= unique_count,
            EscrowError::NewCapBelowCurrentFunderCount,
        );

        env.storage()
            .instance()
            .set(&DataKey::MaxUniqueInvestorsCap, &new_cap);

        MaxUniqueInvestorsCapLowered {
            name: symbol_short!("inv_cap"),
            invoice_id: escrow.invoice_id.clone(),
            old_cap,
            new_cap,
        }
        .publish(&env);

        new_cap
    }

    /// Validate the stored schema version and apply a migration if one is implemented.
    ///
    /// # Behavior - **typed error on all current paths**
    ///
    /// This entrypoint currently contains **no implemented migration logic**. Every call
    /// terminates with a typed contract error (aborts the Soroban transaction). This is intentional:
    /// it makes the "no migration" guarantee explicit rather than silently returning success.
    ///
    /// **Execution order:** the function first requires current admin authorization, then reads
    /// [`DataKey::Version`] from instance storage, validates the supplied `from_version`, and emits
    /// a typed error. No storage writes ever occur in the current release. The authorization guard
    /// is intentionally placed before version checks so future migration logic remains admin-gated
    /// by construction.
    ///
    /// Do **not** call `migrate` expecting it to perform bookkeeping work in the current
    /// release. To add a real migration path (e.g. rewriting a stored struct after a field
    /// addition), implement the transformation above the final error branch, update
    /// [`DataKey::Version`], and bump [`SCHEMA_VERSION`].
    ///
    /// # When to call
    ///
    /// - **Only** when you have extended `migrate` with a concrete transformation for the
    ///   `from_version → SCHEMA_VERSION` path you need.
    /// - Additive new [`DataKey`] variants read with `.get(...).unwrap_or(default)` do **not**
    ///   require a `migrate` call; old instances simply return the default.
    /// - If `InvoiceEscrow` struct layout changed, `migrate` cannot help — redeploy instead.
    ///
    /// # Errors
    ///
    /// Requires current admin authorization before any version checks or future storage rewrites.
    ///
    /// | Condition | Typed error |
    /// |-----------|--------|
    /// | `stored_version != from_version` | [`EscrowError::MigrationVersionMismatch`] |
    /// | `from_version >= SCHEMA_VERSION` | [`EscrowError::AlreadyCurrentSchemaVersion`] |
    /// | Any `from_version < SCHEMA_VERSION` (all paths) | [`EscrowError::NoMigrationPath`] |
    ///
    /// See `docs/OPERATOR_RUNBOOK.md` §2 for step-by-step instructions on implementing
    /// a concrete migration path.
    pub fn migrate(env: Env, from_version: u32) -> u32 {
        Self::load_escrow_require_admin(&env);

        let stored: u32 = env.storage().instance().get(&DataKey::Version).unwrap_or(0);

        ensure(
            &env,
            stored == from_version,
            EscrowError::MigrationVersionMismatch,
        );

        if from_version >= SCHEMA_VERSION {
            fail(&env, EscrowError::AlreadyCurrentSchemaVersion)
        } else {
            // No migration path is implemented for any version below SCHEMA_VERSION.
            // To add one: implement the transformation here, call
            //   env.storage().instance().set(&DataKey::Version, &NEW_VERSION);
            // and return NEW_VERSION before reaching this typed error.
            fail(&env, EscrowError::NoMigrationPath)
        }
    }

    /// Replaces the deployed contract WASM with the binary identified by `new_wasm_hash`.
    ///
    /// # Authorization
    /// Only the current escrow admin may call this function. Authorization is verified
    /// before any deployer call. Unauthorised callers will cause the transaction to revert.
    ///
    /// # State
    /// No persistent storage keys, escrow records, or balances are modified.
    /// Only the contract code is replaced. `SCHEMA_VERSION` is not incremented here;
    /// run `migrate()` after upgrading if schema changes accompany the new WASM.
    ///
    /// # Interaction with ADR-007
    /// This function does not add, remove, or rename any `DataKey` variants.
    /// It is safe to upgrade to a WASM that adds new `DataKey` variants (additive-key
    /// policy) without calling `migrate()`, but removing or reordering variants in the
    /// new WASM would corrupt stored data. Operators must verify additive-only changes
    /// before upgrading.
    ///
    /// # Risks
    /// Deploying an incompatible WASM will corrupt stored state. Test thoroughly on
    /// testnet before upgrading production contracts.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        // Auth first — matches migrate() ordering
        let escrow = Self::load_escrow_require_admin(&env);

        // Emit event before the deployer call so the event is recorded even if
        // the deployer call somehow reverts (defensive ordering)
        ContractUpgraded {
            name: symbol_short!("upgrade"),
            invoice_id: escrow.invoice_id,
            new_wasm_hash: new_wasm_hash.clone(),
        }
        .publish(&env);

        // Replace contract WASM — no state is modified
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    /// Record investor principal while the invoice is **open**, atomically transferring the funding
    /// token from the investor's address to the contract. First deposit sets base
    /// [`InvoiceEscrow::yield_bps`] for this investor; further amounts must use this method (not
    /// [`LiquifactEscrow::fund_with_commitment`]) so tier selection stays immutable after the first leg.
    ///
    /// # Token Custody & Invariants
    /// atomically pulls the specified `amount` of [`DataKey::FundingToken`] from the `investor`
    /// to `env.current_contract_address()`, performing strict pre/post balance-delta validation.
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes for invalid amount, legal hold, closed funding state,
    /// allowlist rejection, cap violations, checked-arithmetic overflow, and inbound token transfer failures.
    pub fn fund(env: Env, investor: Address, amount: i128) -> InvoiceEscrow {
        Self::fund_impl(env, investor, amount, true, 0)
    }

    /// First deposit only (per investor): optional longer lock and tier ladder from [`DataKey::YieldTierTable`].
    /// Atomically pulls the specified `amount` of [`DataKey::FundingToken`] from the `investor`
    /// to `env.current_contract_address()`, performing strict pre/post balance-delta validation.
    /// Sets [`DataKey::InvestorClaimNotBefore`] when `committed_lock_secs > 0`. Additional principal
    /// from the same investor must use [`LiquifactEscrow::fund`].
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes for the same funding guards as [`LiquifactEscrow::fund`],
    /// plus tiered follow-on deposit misuse, claim-lock timestamp overflow, and inbound token transfer failures.
    pub fn fund_with_commitment(
        env: Env,
        investor: Address,
        amount: i128,
        committed_lock_secs: u64,
    ) -> InvoiceEscrow {
        Self::fund_impl(env, investor, amount, false, committed_lock_secs)
    }

    /// Batch funding entrypoint: record multiple investor principals in a single call.
    ///
    /// Each entry is processed sequentially with per-investor [`Address::require_auth()`].
    /// All existing [`LiquifactEscrow::fund`] invariants (allowlist, caps, min contribution,
    /// overflow guards) are enforced per entry. If an entry fails its invariants,
    /// the call returns an error without corrupting prior entries.
    ///
    /// # Parameters
    /// - `entries`: `Vec<(Address, i128)>` of (investor address, funding amount) tuples.
    ///
    /// # Errors
    /// - [`EscrowError::FundingBatchEmpty`] if entries is empty
    /// - [`EscrowError::FundingBatchTooLarge`] if entries.len() > [`MAX_FUND_BATCH`]
    /// - Per-entry: all errors from [`LiquifactEscrow::fund`] for that investor/amount pair
    ///
    /// # Events
    /// One [`EscrowFunded`] event per entry (identical to single [`LiquifactEscrow::fund`] semantics).
    ///
    /// # Funded-target snapshot
    /// If any entry causes the escrow to transition to **funded** (status 0 → 1),
    /// [`DataKey::FundingCloseSnapshot`] is recorded exactly once. Remaining entries are
    /// processed even after transition.
    pub fn fund_batch(env: Env, entries: Vec<(Address, i128)>) -> InvoiceEscrow {
        let n = entries.len();

        ensure(&env, n > 0, EscrowError::FundingBatchEmpty);
        ensure(&env, n <= MAX_FUND_BATCH, EscrowError::FundingBatchTooLarge);

        let mut escrow = Self::get_escrow(env.clone());

        for i in 0..n {
            let (investor, amount) = entries.get(i).unwrap();

            // Call fund_impl for each entry, but we need to reconstruct the escrow
            // after each call. However, fund_impl returns the updated escrow,
            // so we capture it for the next iteration.
            escrow = Self::fund_impl(env.clone(), investor, amount, true, 0);
        }

        escrow
    }

    fn fund_impl(
        env: Env,
        investor: Address,
        amount: i128,
        simple_fund: bool,
        committed_lock_secs: u64,
    ) -> InvoiceEscrow {
        ensure(&env, amount > 0, EscrowError::FundingAmountNotPositive);

        let floor: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinContributionFloor)
            .unwrap_or(0);
        if floor > 0 {
            ensure(
                &env,
                amount >= floor,
                EscrowError::FundingBelowMinContribution,
            );
        }

        let mut escrow = Self::get_escrow(env.clone());
        // Legal hold check is intentionally after the escrow read: the escrow is needed for
        // status and yield_bps regardless, and hoisting the hold check before the escrow read
        // would not reduce storage operations (both keys are always read on this path).
        Self::guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksFunding);
        Self::guard_status_eq(&env, escrow.status, 0, EscrowError::EscrowNotOpenForFunding);

        // Check funding deadline
        if let Some(deadline) = env.storage().instance().get(&DataKey::FundingDeadline) {
            ensure(
                &env,
                env.ledger().timestamp() <= deadline,
                EscrowError::FundingDeadlinePassed,
            );
        }

        if Self::is_allowlist_active(env.clone()) {
            ensure(
                &env,
                Self::is_investor_allowlisted(env.clone(), investor.clone()),
                EscrowError::InvestorNotAllowlisted,
            );
        }

        let prev: i128 = Self::get_persistent_investor_contribution(&env, investor.clone());
        let new_contribution: i128 = prev
            .checked_add(amount)
            .unwrap_or_else(|| fail(&env, EscrowError::InvestorContributionOverflow));

        if let Some(cap) = env
            .storage()
            .instance()
            .get::<DataKey, i128>(&DataKey::MaxPerInvestorCap)
        {
            ensure(
                &env,
                new_contribution <= cap,
                EscrowError::InvestorContributionExceedsCap,
            );
        }

        let cur_funder_count: u32 = if prev == 0 {
            env.storage()
                .instance()
                .get(&DataKey::UniqueFunderCount)
                .unwrap_or(0)
        } else {
            0
        };

        if prev == 0 {
            if let Some(cap) = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&DataKey::MaxUniqueInvestorsCap)
            {
                ensure(
                    &env,
                    cur_funder_count < cap,
                    EscrowError::UniqueInvestorCapReached,
                );
            }
        }

        let investor_effective_yield_bps: i64;
        let tier_lock_secs: u64;
        let mut claim_nb = 0u64;

        if simple_fund {
            tier_lock_secs = 0;
            if prev == 0 {
                investor_effective_yield_bps = escrow.yield_bps;
            } else {
                investor_effective_yield_bps =
                    Self::get_persistent_investor_effective_yield(&env, investor.clone())
                        .unwrap_or(escrow.yield_bps);
            }
        } else {
            ensure(&env, prev == 0, EscrowError::TieredSecondDeposit);
            let (eff, lock) =
                Self::effective_yield_for_commitment(&env, escrow.yield_bps, committed_lock_secs);
            investor_effective_yield_bps = eff;
            tier_lock_secs = lock;
            let now = env.ledger().timestamp();
            claim_nb = if committed_lock_secs == 0 {
                0u64
            } else {
                now.checked_add(committed_lock_secs)
                    .unwrap_or_else(|| fail(&env, EscrowError::InvestorClaimTimeOverflow))
            };
            if claim_nb > 0 && escrow.maturity > 0 {
                ensure(
                    &env,
                    claim_nb <= escrow.maturity,
                    EscrowError::CommitmentLockExceedsMaturity,
                );
            }
        }

        escrow.funded_amount = escrow
            .funded_amount
            .checked_add(amount)
            .unwrap_or_else(|| fail(&env, EscrowError::FundedAmountOverflow));

        let mut next_status = escrow.status;
        let mut snapshot_to_write = None;
        if escrow.status == 0 && escrow.funded_amount >= escrow.funding_target {
            next_status = 1;
            if !env.storage().instance().has(&DataKey::FundingCloseSnapshot) {
                snapshot_to_write = Some(FundingCloseSnapshot {
                    total_principal: escrow.funded_amount,
                    funding_target: escrow.funding_target,
                    closed_at_ledger_timestamp: env.ledger().timestamp(),
                    closed_at_ledger_sequence: env.ledger().sequence(),
                });
            }
        }

        // 2.require_auth checks
        investor.require_auth();

        // 3. Storage writes
        Self::set_persistent_investor_contribution(&env, investor.clone(), new_contribution);

        if simple_fund {
            if prev == 0 {
                Self::set_persistent_investor_effective_yield(
                    &env,
                    investor.clone(),
                    escrow.yield_bps,
                );
                Self::set_persistent_investor_claim_not_before(&env, investor.clone(), 0u64);
            }
        } else {
            Self::set_persistent_investor_effective_yield(&env, investor.clone(), investor_effective_yield_bps);
            Self::set_persistent_investor_claim_not_before(&env, investor.clone(), claim_nb);
        }

        if prev == 0 {
            let mut index: Vec<Address> = env
                .storage()
                .instance()
                .get(&DataKey::InvestorIndex)
                .unwrap_or_else(|| Vec::new(&env));
            index.push_back(investor.clone());
            env.storage()
                .instance()
                .set(&DataKey::InvestorIndex, &index);

            // Use the hoisted cur_funder_count; no second storage read needed.
            env.storage()
                .instance()
                .set(&DataKey::UniqueFunderCount, &(cur_funder_count + 1));
        }

        escrow.status = next_status;
        if let Some(snap) = snapshot_to_write {
            env.storage()
                .instance()
                .set(&DataKey::FundingCloseSnapshot, &snap);
        }

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        // Extend persistent TTL for the investor contribution key
        env.storage().persistent().extend_ttl(
            &DataKey::InvestorContribution(investor.clone()),
            PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
            PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
        );

        // 4. Token transfer
        let token_addr = env
            .storage()
            .instance()
            .get(&DataKey::FundingToken)
            .unwrap_or_else(|| fail(&env, EscrowError::FundingTokenNotSet));
        let this = env.current_contract_address();

        external_calls::transfer_funding_token_inbound_with_balance_checks(
            &env,
            &token_addr,
            &investor,
            &this,
            amount,
        );

        // 5. Emit event
        EscrowFunded {
            name: symbol_short!("funded"),
            invoice_id: escrow.invoice_id.clone(),
            investor: investor.clone(),
            amount,
            funded_amount: escrow.funded_amount,
            status: escrow.status,
            investor_effective_yield_bps,
            tier_lock_secs,
        }
        .publish(&env);

        escrow
    }

    /// Closes funding early for an under-funded invoice, transitioning the escrow to a settleable state.
    ///
    /// # Authorization
    /// The configured **SME** or **Admin** address must authorize this call.
    ///
    /// Blocked while [`DataKey::LegalHold`] is active.
    pub fn partial_settle(env: Env, caller: Address) -> InvoiceEscrow {
        caller.require_auth();

        assert!(
            !Self::legal_hold_active(&env),
            "Legal hold blocks partial settlement"
        );

        let mut escrow = Self::get_escrow(env.clone());

        assert!(
            caller == escrow.sme_address || caller == escrow.admin,
            "Unauthorized caller for partial settlement"
        );

        assert!(
            escrow.status == 0,
            "Escrow must be in Open state for partial settlement"
        );

        // Transition to funded status early.
        escrow.status = 1;

        // Write FundingCloseSnapshot if not already present.
        if !env.storage().instance().has(&DataKey::FundingCloseSnapshot) {
            let snap = FundingCloseSnapshot {
                total_principal: escrow.funded_amount,
                funding_target: escrow.funding_target,
                closed_at_ledger_timestamp: env.ledger().timestamp(),
                closed_at_ledger_sequence: env.ledger().sequence(),
            };
            env.storage()
                .instance()
                .set(&DataKey::FundingCloseSnapshot, &snap);
        }

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        EscrowPartialSettle {
            name: symbol_short!("part_set"),
            invoice_id: escrow.invoice_id.clone(),
            funded_amount: escrow.funded_amount,
        }
        .publish(&env);

        escrow
    }

    pub fn settle(env: Env) -> InvoiceEscrow {
        Self::guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksSettlement);

        // env.clone(): env is used again after this call for ledger timestamp, storage set, and publish.
        let mut escrow = Self::load_escrow_require_sme(&env);

        Self::guard_status_eq(&env, escrow.status, 1, EscrowError::SettlementNotFunded);

        let now = env.ledger().timestamp();
        if escrow.maturity > 0 {
            ensure(
                &env,
                now >= escrow.maturity,
                EscrowError::MaturityNotReached,
            );
        }

        escrow.status = 2;

        // Write-once settlement timestamp (ADR-007 additive-key policy).
        // settle() is only reachable from status 1, so this key is set exactly once per escrow.
        env.storage().instance().set(&DataKey::SettledAt, &now);

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        // Compute the realized settle pool from the funding-close snapshot.
        // Falls back to 0 for legacy escrows that pre-date the snapshot key.
        let settle_pool: i128 = env
            .storage()
            .instance()
            .get::<DataKey, FundingCloseSnapshot>(&DataKey::FundingCloseSnapshot)
            .map(|snap| {
                let coupon = snap
                    .total_principal
                    .checked_mul(escrow.yield_bps as i128)
                    .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow))
                    .checked_div(10_000)
                    .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow));
                snap.total_principal
                    .checked_add(coupon)
                    .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow))
            })
            .unwrap_or(0);

        EscrowSettled {
            name: symbol_short!("escrow_sd"),
            invoice_id: escrow.invoice_id.clone(),
            funded_amount: escrow.funded_amount,
            yield_bps: escrow.yield_bps,
            maturity: escrow.maturity,
            settled_at_ledger_timestamp: now,
            settle_pool,
        }
        .publish(&env);

        escrow
    }

    /// Read-only check: whether this escrow is currently settleable.
    ///
    /// Returns `true` when all of the following hold:
    /// - `status == 1` (funded)
    /// - `maturity == 0` **or** `env.ledger().timestamp() >= maturity`
    /// - No legal hold is active
    ///
    /// # Security
    /// Pure read — no authorization required, no state mutation.
    pub fn is_settleable(env: Env) -> bool {
        if Self::legal_hold_active(&env) {
            return false;
        }
        let escrow = Self::get_escrow(env.clone());
        if escrow.status != 1 {
            return false;
        }
        if escrow.maturity > 0 {
            let now = env.ledger().timestamp();
            if now < escrow.maturity {
                return false;
            }
        }
        true
    }

    /// SME pulls funded liquidity. Transfers `funded_amount` of the bound funding token
    /// from this contract to `sme_address`, then transitions status to 3 (withdrawn).
    /// Blocked when a legal hold is active.
    ///
    /// # Guard ordering
    ///
    /// 1. Legal-hold gate (read-only).
    /// 2. `sme_address.require_auth()` (via `load_escrow_require_sme`).
    /// 3. Status == 1 (funded) check.
    /// 4. Contract balance sufficiency check ([`EscrowError::InsufficientContractBalance`]).
    /// 5. Status transition to 3, `DistributedPrincipal` update, storage write.
    /// 6. SEP-41 token transfer with balance-delta verification.
    /// 7. Event emission.
    ///
    /// # Errors
    /// - [`EscrowError::LegalHoldBlocksWithdrawal`] — hold is active.
    /// - [`EscrowError::WithdrawalNotFunded`] — escrow not in funded state.
    /// - [`EscrowError::InsufficientContractBalance`] — contract holds less than `funded_amount`.
    pub fn withdraw(env: Env) -> InvoiceEscrow {
        Self::guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksWithdrawal);

        let mut escrow = Self::load_escrow_require_sme(&env);

        Self::guard_status_eq(&env, escrow.status, 1, EscrowError::WithdrawalNotFunded);

        let amount = escrow.funded_amount;
        let sme = escrow.sme_address.clone();

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::FundingToken)
            .unwrap_or_else(|| fail(&env, EscrowError::FundingTokenNotSet));

        // Verify the contract holds enough before mutating state.
        let this = env.current_contract_address();
        let contract_balance = TokenClient::new(&env, &token_addr).balance(&this);
        ensure(
            &env,
            contract_balance >= amount,
            EscrowError::InsufficientContractBalance,
        );

        // State transition and accounting (checks-effects-interactions).
        escrow.status = 3;
        env.storage().instance().set(&DataKey::Escrow, &escrow);

        let prev_distributed: i128 = env
            .storage()
            .instance()
            .get(&DataKey::DistributedPrincipal)
            .unwrap_or(0);
        env.storage().instance().set(
            &DataKey::DistributedPrincipal,
            &prev_distributed.saturating_add(amount),
        );

        // Token transfer with SEP-41 balance-delta verification.
        external_calls::transfer_funding_token_with_balance_checks(
            &env,
            &token_addr,
            &this,
            &sme,
            amount,
        );

        SmeWithdrew {
            name: symbol_short!("sme_wd"),
            invoice_id: escrow.invoice_id.clone(),
            amount,
            recipient: sme,
        }
        .publish(&env);

        escrow
    }

    /// Investor records a payout claim after settlement and receives the gross payout on-chain.
    ///
    /// # Idempotency
    ///
    /// A second call for the same investor is a silent no-op: the `InvestorClaimed` marker is
    /// written **before** the token transfer, so re-entrant or replayed calls return early
    /// without a second transfer.
    ///
    /// # Payout flow
    ///
    /// 1. Compute the gross pro-rata payout via [`LiquifactEscrow::compute_investor_payout`].
    /// 2. Guard against zero-payout edge cases (floor division rounding).
    /// 3. Mark the investor as claimed (effect).
    /// 4. Transfer the payout from this contract to the investor (interaction).
    /// 5. Emit `InvestorPayoutClaimed`.
    ///
    /// If the transfer fails (e.g. insufficient contract balance), the host rolls back all
    /// storage writes including the claimed marker, so the investor may retry.
    ///
    /// # Guard ordering (ADR-002)
    ///
    /// 1. Legal-hold gate (read-only).
    /// 2. `investor.require_auth()`.
    /// 3. Single contribution fetch — eliminates the previous duplicate `get_contribution` call;
    ///    the value is reused for the participation guard.
    /// 4. Settled-status gate (escrow read).
    /// 5. `not_before` ledger-time gate (see `docs/escrow-ledger-time.md`).
    /// 6. Idempotent early-return on `InvestorClaimed`.
    /// 7. On-chain payout computation via [`LiquifactEscrow::compute_investor_payout`].
    /// 8. Zero-payout guard (floor-division edge case).
    /// 9. Storage write + token transfer + event emit.
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes for legal hold, missing contribution, unsettled escrow,
    /// an unexpired commitment lock, or a zero computed payout.
    pub fn claim_investor_payout(env: Env, investor: Address) {
        Self::guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksInvestorClaims);

        investor.require_auth();

        // Single fetch: consolidates the previous two reads of InvestorContribution.
        // Retains the participation guard without a redundant second storage access.
        let contribution: i128 = Self::get_persistent_investor_contribution(&env, investor.clone());
        ensure(&env, contribution > 0, EscrowError::NoContributionToClaim);

        // env.clone(): env is used again after this call for storage reads, ledger timestamp, and publish.
        let escrow = Self::get_escrow(env.clone());
        Self::guard_status_eq(&env, escrow.status, 2, EscrowError::InvestorClaimNotSettled);

        let not_before: u64 =
            Self::get_persistent_investor_claim_not_before(&env, investor.clone());
        let now = env.ledger().timestamp();
        ensure(
            &env,
            now >= not_before,
            EscrowError::InvestorCommitmentLockNotExpired,
        );

        // Idempotent early-return: a second claim is a no-op (no re-emit).
        if Self::get_persistent_investor_claimed(&env, investor.clone()) {
            return;
        }

        // Compute on-chain gross payout via pro-rata math.
        let payout = Self::compute_investor_payout(env.clone(), investor.clone());
        ensure(&env, payout > 0, EscrowError::PayoutZero);

        // Mark before transfer — prevents double-pay on any re-entrant path.
        Self::set_persistent_investor_claimed(&env, investor.clone(), true);

        // Transfer gross payout from this contract to the investor.
        let this = env.current_contract_address();
        let token_addr = Self::funding_token_or_fail(&env);
        external_calls::transfer_funding_token_with_balance_checks(
            &env,
            &token_addr,
            &this,
            &investor,
            payout,
        );

        InvestorPayoutClaimed {
            name: symbol_short!("inv_claim"),
            investor,
            invoice_id: escrow.invoice_id.clone(),
        }
        .publish(&env);
    }

    /// Checked coupon math for settlement reporting.
    fn settlement_coupon(env: &Env, total_principal: i128, yield_bps: i64) -> i128 {
        total_principal
            .checked_mul(yield_bps as i128)
            .unwrap_or_else(|| fail(env, EscrowError::ComputePayoutArithmeticOverflow))
            .checked_div(10_000)
            .unwrap_or_else(|| fail(env, EscrowError::ComputePayoutArithmeticOverflow))
    }

    /// Checked investor gross payout arithmetic.
    fn gross_payout(env: &Env, contribution: i128, total_principal: i128, yield_bps: i64) -> i128 {
        let coupon = Self::settlement_coupon(env, total_principal, yield_bps);
        let settle_pool = total_principal
            .checked_add(coupon)
            .unwrap_or_else(|| fail(env, EscrowError::ComputePayoutArithmeticOverflow));
        contribution
            .checked_mul(settle_pool)
            .unwrap_or_else(|| fail(env, EscrowError::ComputePayoutArithmeticOverflow))
            .checked_div(total_principal)
            .unwrap_or_else(|| fail(env, EscrowError::ComputePayoutArithmeticOverflow))
    }

    /// On-chain read-only view that returns the **claimable payout** for an investor, applying
    /// all gating rules that [`LiquifactEscrow::claim_investor_payout`] uses.
    ///
    /// # Comparison with [`LiquifactEscrow::compute_investor_payout`]
    ///
    /// - [`LiquifactEscrow::compute_investor_payout`] returns the **gross theoretical payout**
    ///   (no gating applied).
    /// - This function returns the **net claimable amount** (0 if any gate blocks a claim).
    ///
    /// # Returns
    ///
    /// - `0` when escrow is not yet settled (status != 2)
    /// - `0` when a legal hold blocks investor claims
    /// - `0` when the investor has already claimed their payout
    /// - `0` when the current ledger timestamp is before the investor's claim-not-before time
    /// - Otherwise, the gross payout from [`LiquifactEscrow::compute_investor_payout`]
    ///
    /// # Authorization
    ///
    /// None — pure read; no auth required and no state mutation.
    pub fn get_claimable_payout(env: Env, investor: Address) -> i128 {
        // Check 1: Escrow must be settled
        let escrow = Self::get_escrow(env.clone());
        if escrow.status != 2 {
            return 0;
        }

        // Check 2: Legal hold must not be active
        if Self::legal_hold_active(&env) {
            return 0;
        }

        // Check 3: Investor must not have claimed yet
        if Self::get_persistent_investor_claimed(&env, investor.clone()) {
            return 0;
        }

        // Check 4: Current time must be >= investor's claim-not-before
        let not_before = Self::get_persistent_investor_claim_not_before(&env, investor.clone());
        let now = env.ledger().timestamp();
        if now < not_before {
            return 0;
        }

        // All gates passed: return the gross payout
        Self::compute_investor_payout(env, investor)
    }

    /// On-chain read-only pro-rata gross payout for `investor`.
    ///
    /// Derives the **gross payout** (principal share plus `InvestorEffectiveYield`-adjusted
    /// coupon) from [`FundingCloseSnapshot`], providing an authoritative on-chain implementation
    /// of the math specified in `docs/escrow-pro-rata.md`. Off-chain tooling should call this
    /// view rather than re-implementing the formula to guarantee identical rounding.
    ///
    /// # Formula (floor / truncating integer division)
    ///
    /// ```text
    /// coupon       = total_principal × effective_yield_bps / 10_000  (floor)
    /// settle_pool  = total_principal + coupon
    /// gross_payout = contribution × settle_pool / total_principal     (floor)
    /// ```
    ///
    /// # Returns
    ///
    /// - `0` when [`DataKey::FundingCloseSnapshot`] does not exist (escrow not yet funded).
    /// - `0` when `investor` has no contribution (`DataKey::InvestorContribution` absent or zero).
    /// - Computed floor payout otherwise.
    ///
    /// # Invariant
    ///
    /// The sum of `compute_investor_payout` over all investors is ≤ `total_principal + coupon`;
    /// any rounding residual is swept by [`LiquifactEscrow::sweep_terminal_dust`].
    ///
    /// # Overflow safety
    ///
    /// All multiplications use [`i128::checked_mul`] and divisions use [`i128::checked_div`].
    /// Emits [`EscrowError::ComputePayoutArithmeticOverflow`] rather than silently producing a
    /// wrong value.
    ///
    /// # Authorization
    ///
    /// None — pure read; no auth required.
    pub fn compute_investor_payout(env: Env, investor: Address) -> i128 {
        // Contribution fetch: returns 0 for non-participants without panicking.
        let contribution: i128 = Self::get_persistent_investor_contribution(&env, investor.clone());
        if contribution == 0 {
            return 0;
        }

        // Snapshot must exist (written when escrow first reaches status == 1).
        let Some(snap) = env
            .storage()
            .instance()
            .get::<DataKey, FundingCloseSnapshot>(&DataKey::FundingCloseSnapshot)
        else {
            return 0;
        };

        let total_principal = snap.total_principal;
        if total_principal <= 0 {
            return 0;
        }

        // Resolve effective yield: investor-specific tier (set at first deposit) or escrow base.
        // env.clone(): env is used again after this call for InvestorEffectiveYield read.
        let escrow = Self::get_escrow(env.clone());
        let effective_yield_bps: i64 =
            Self::get_persistent_investor_effective_yield(&env, investor.clone())
                .unwrap_or(escrow.yield_bps);

        Self::gross_payout(&env, contribution, total_principal, effective_yield_bps)
    }

    pub fn update_maturity(env: Env, new_maturity: u64) -> InvoiceEscrow {
        let mut escrow = Self::load_escrow_require_admin(&env);

        Self::guard_status_eq(&env, escrow.status, 0, EscrowError::MaturityUpdateNotOpen);

        let old_maturity = escrow.maturity;
        escrow.maturity = new_maturity;

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        MaturityUpdatedEvent {
            name: symbol_short!("maturity"),
            invoice_id: escrow.invoice_id.clone(),
            old_maturity,
            new_maturity,
        }
        .publish(&env);

        escrow
    }

    /// Update the configured maximum maturity horizon for this escrow instance.
    ///
    /// Only the current admin may call this. The new horizon applies to subsequent
    /// [`LiquifactEscrow::update_maturity`] calls; existing maturity values are unaffected.
    ///
    /// Emits [`MaturityMaxHorizonUpdated`] with the old and new horizon values.
    pub fn update_maturity_max_horizon(env: Env, new_horizon: u64) -> u64 {
        let escrow = Self::load_escrow_require_admin(&env);

        let old_horizon = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::MaturityMaxHorizon)
            .unwrap_or(DEFAULT_MATURITY_MAX_HORIZON_SECS);

        env.storage()
            .instance()
            .set(&DataKey::MaturityMaxHorizon, &new_horizon);

        MaturityMaxHorizonUpdated {
            name: symbol_short!("mtry_max"),
            invoice_id: escrow.invoice_id,
            old_horizon,
            new_horizon,
        }
        .publish(&env);

        new_horizon
    }

    pub fn bump_ttl(env: Env, allowlisted: Vec<Address>) {
        // Permissionless TTL extension.
        //
        // Invariant: Soroban's `extend_ttl` never shortens TTL; this entrypoint only extends.
        // No other state is mutated.
        //
        // Rationale: long-dated escrows (maturity far in the future) write time-sensitive
        // data (`DataKey::Escrow`, snapshot, and per-investor claim gates). Under rent/archival
        // semantics, instance storage can expire and cause defaulted reads (e.g. allowlist
        // gate falls back to `false`), breaking settlement/claim readiness.
        //
        // Documentation references:
        // - ADR-007: storage key evolution policy (additive changes / key semantics).
        // - docs/escrow-ledger-time.md: all gating uses `Env::ledger().timestamp()` with `>=`.

        env.storage().instance().extend_ttl(
            INSTANCE_TTL_MIN_EXTENSION_LEDGERS,
            INSTANCE_TTL_MIN_EXTENSION_LEDGERS,
        );

        // Instance storage TTL is contract-wide under Soroban SDK 25. The call above covers
        // Escrow, Version, LegalHold, snapshots, caps, and other instance keys.

        // Persistent per-investor keys and allowlist entries (independent TTL per address).
        for addr in allowlisted.iter() {
            let k = DataKey::InvestorAllowlisted(addr.clone());
            env.storage().persistent().extend_ttl(
                &k,
                PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
                PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
            );
            // Extend persistent TTL for per-investor persistent keys used by this contract.
            env.storage().persistent().extend_ttl(
                &DataKey::InvestorContribution(addr.clone()),
                PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
                PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::InvestorEffectiveYield(addr.clone()),
                PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
                PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::InvestorClaimNotBefore(addr.clone()),
                PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
                PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::InvestorClaimed(addr.clone()),
                PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
                PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
            );
        }
    }

    /// Propose a new admin (`PendingAdmin`) — step 1 of a two-step handover.
    ///
    /// Requires current admin authorization. The destination must differ from the current admin.
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes when the escrow is uninitialized or `new_admin` is the
    /// current admin.
    pub fn propose_admin(env: Env, new_admin: Address) -> Address {
        let escrow = Self::load_escrow_require_admin(&env);

        ensure(
            &env,
            escrow.admin != new_admin,
            EscrowError::NewAdminSameAsCurrent,
        );

        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);

        AdminProposedEvent {
            name: symbol_short!("adm_prop"),
            invoice_id: escrow.invoice_id.clone(),
            current_admin: escrow.admin,
            pending_admin: new_admin.clone(),
        }
        .publish(&env);

        new_admin
    }

    /// Accept a pending admin handover.
    ///
    /// The address stored in [`DataKey::PendingAdmin`] must authorize this call. On success it is
    /// promoted into [`InvoiceEscrow::admin`] and the pending key is cleared, so admin authority
    /// changes only after both the current admin and successor have explicitly authorized.
    pub fn accept_admin(env: Env) -> InvoiceEscrow {
        let pending: Option<Address> = env.storage().instance().get(&DataKey::PendingAdmin);
        ensure(&env, pending.is_some(), EscrowError::NoPendingAdmin);
        let pending = pending.unwrap();
        pending.require_auth();

        let mut escrow = Self::get_escrow(env.clone());
        escrow.admin = pending.clone();

        env.storage().instance().set(&DataKey::Escrow, &escrow);
        env.storage().instance().remove(&DataKey::PendingAdmin);

        AdminTransferredEvent {
            name: symbol_short!("admin"),
            invoice_id: escrow.invoice_id.clone(),
            new_admin: pending,
        }
        .publish(&env);

        escrow
    }

    /// Deprecated shim for the former one-step admin transfer API.
    ///
    /// This function now only proposes `new_admin` by delegating to
    /// [`LiquifactEscrow::propose_admin`]. The proposed address must still call
    /// [`LiquifactEscrow::accept_admin`] before admin authority changes.
    ///
    /// # Deprecation observability (issue #386)
    ///
    /// Each successful call publishes **two** events, in this order:
    ///
    /// 1. [`AdminProposedEvent`] from the inner
    ///    [`LiquifactEscrow::propose_admin`] delegation.
    /// 2. [`DeprecatedTransferAdminUsed`], carrying the same proposed
    ///    address, so indexers and operators can distinguish legacy
    ///    one-step callers from those using the canonical two-step flow.
    ///
    /// Intended removal path: aggregate [`DeprecatedTransferAdminUsed`] over a
    /// deployment window, migrate remaining integrations to
    /// [`LiquifactEscrow::propose_admin`] followed by
    /// [`LiquifactEscrow::accept_admin`], then drop this shim. Until removal,
    /// handover semantics are unchanged and no new authority is granted
    /// beyond what [`LiquifactEscrow::propose_admin`] already exposes.
    #[deprecated(note = "use propose_admin followed by accept_admin")]
    pub fn transfer_admin(env: Env, new_admin: Address) -> InvoiceEscrow {
        Self::propose_admin(env.clone(), new_admin.clone());

        // Re-read after the propose_admin delegation so the deprecation event
        // carries the exact `invoice_id` indexers will see in the prior
        // `AdminProposedEvent`. Reading after (not before) keeps a single
        // extra instance-storage read, since `propose_admin` does not expose
        // its loaded escrow back through the return type.
        let escrow = Self::get_escrow(env.clone());

        DeprecatedTransferAdminUsed {
            name: symbol_short!("depr_xfer"),
            invoice_id: escrow.invoice_id.clone(),
            proposed_address: new_admin,
        }
        .publish(&env);

        escrow
    }

    /// Cancel a pending admin handover proposal.
    ///
    /// Removes [`DataKey::PendingAdmin`] so the previously nominated address can no longer
    /// call [`LiquifactEscrow::accept_admin`]. The current admin address and all other escrow
    /// state remain unchanged.
    ///
    /// # Authorization
    ///
    /// The current [`InvoiceEscrow::admin`] must authorize this call (via
    /// [`LiquifactEscrow::load_escrow_require_admin`]).
    ///
    /// # Errors
    ///
    /// - [`EscrowError::NoPendingAdmin`] — no proposal exists; nothing to cancel.
    ///
    /// # Returns
    ///
    /// The revoked pending address, so callers can record it off-chain without a
    /// separate read.
    ///
    /// # Events
    ///
    /// Emits [`AdminProposalCancelled`] carrying `invoice_id` and `cancelled_pending`.
    pub fn cancel_pending_admin(env: Env) -> Address {
        let escrow = Self::load_escrow_require_admin(&env);

        let pending: Option<Address> = env.storage().instance().get(&DataKey::PendingAdmin);
        ensure(&env, pending.is_some(), EscrowError::NoPendingAdmin);
        let cancelled = pending.unwrap();

        env.storage().instance().remove(&DataKey::PendingAdmin);

        AdminProposalCancelled {
            name: symbol_short!("adm_can"),
            invoice_id: escrow.invoice_id.clone(),
            cancelled_pending: cancelled.clone(),
        }
        .publish(&env);

        cancelled
    }

    /// Transition an **open** escrow (status 0) to **cancelled** (status 4).
    ///
    /// Only the [`InvoiceEscrow::admin`] may call this. Blocked while a legal hold is active.
    /// After cancellation, investors may recover their principal via [`LiquifactEscrow::refund`].
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes when legal hold is active, the escrow is uninitialized,
    /// or the escrow is not in status 0 (open).
    pub fn cancel_funding(env: Env) -> InvoiceEscrow {
        Self::guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksCancelFunding);

        let mut escrow = Self::load_escrow_require_admin(&env);

        Self::guard_status_eq(&env, escrow.status, 0, EscrowError::CancelFundingNotOpen);

        escrow.status = 4;
        env.storage().instance().set(&DataKey::Escrow, &escrow);

        FundingCancelled {
            name: symbol_short!("fund_can"),
            invoice_id: escrow.invoice_id.clone(),
            funded_amount: escrow.funded_amount,
        }
        .publish(&env);

        escrow
    }

    /// Return an investor's recorded principal when the escrow is **cancelled** (status 4).
    ///
    /// Requires `investor` auth. Zeroes [`DataKey::InvestorContribution`] after transfer so a
    /// second call fails with [`EscrowError::NoContributionToRefund`].
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes when the escrow is not cancelled, the investor has no
    /// refundable contribution, initialized token data is missing, or the refund transfer fails
    /// token-balance invariants.
    pub fn refund(env: Env, investor: Address) {
        investor.require_auth();

        let escrow = Self::get_escrow(env.clone());
        Self::guard_status_eq(&env, escrow.status, 4, EscrowError::RefundNotCancelled);

        let amount: i128 = Self::get_persistent_investor_contribution(&env, investor.clone());
        ensure(&env, amount > 0, EscrowError::NoContributionToRefund);

        // Zero out contribution before transfer (checks-effects-interactions).
        Self::set_persistent_investor_contribution(&env, investor.clone(), 0i128);
        env.storage()
            .instance()
            .set(&DataKey::InvestorRefunded(investor.clone()), &true);

        // Track distributed principal so sweep_terminal_dust can enforce the liability floor.
        let prev_distributed: i128 = env
            .storage()
            .instance()
            .get(&DataKey::DistributedPrincipal)
            .unwrap_or(0);
        env.storage().instance().set(
            &DataKey::DistributedPrincipal,
            &prev_distributed.saturating_add(amount),
        );

        let token_addr = Self::funding_token_or_fail(&env);
        let this = env.current_contract_address();

        external_calls::transfer_funding_token_with_balance_checks(
            &env,
            &token_addr,
            &this,
            &investor,
            amount,
        );

        InvestorRefundedEvt {
            name: symbol_short!("refunded"),
            investor: investor.clone(),
            invoice_id: escrow.invoice_id.clone(),
            amount,
        }
        .publish(&env);
    }

    /// Whether an investor has already received a refund in a cancelled escrow.
    pub fn is_investor_refunded(env: Env, investor: Address) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::InvestorRefunded(investor))
            .unwrap_or(false)
    }

    /// Returns the total principal already returned to investors via [`LiquifactEscrow::refund`].
    ///
    /// Used by [`LiquifactEscrow::sweep_terminal_dust`] to compute outstanding liabilities.
    /// Absent ⇒ `0` (no refunds have occurred).
    pub fn get_distributed_principal(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::DistributedPrincipal)
            .unwrap_or(0)
    }

    /// Returns the contract's current funding-token balance for on-chain custody reconciliation.
    ///
    /// Reads the bound funding token via [`DataKey::FundingToken`] and queries its balance
    /// at the contract's address. This enables auditors to reconcile on-chain custody against
    /// funded_amount and distributed_principal.
    ///
    /// **Reconciliation formula for cancelled escrows:**
    /// ```text
    /// outstanding_liability = funded_amount - distributed_principal
    /// excess_balance = balance - outstanding_liability  // tokens available for sweep
    /// ```
    ///
    /// # Errors
    /// - [`EscrowError::FundingTokenNotSet`] if the escrow has not been initialized.
    ///
    /// # Authorization
    /// None — pure read; no auth required.
    ///
    /// # Security
    /// This is a view-only entrypoint: it performs no storage writes and does not invoke
    /// any sensitive operations. It safely exposes on-chain state for auditing purposes.
    pub fn get_token_balance(env: Env) -> i128 {
        let token_addr = Self::funding_token_or_fail(&env);
        let this = env.current_contract_address();
        TokenClient::new(&env, &token_addr).balance(&this)
    }
}

#[cfg(test)]
mod test_allowlist_tests;
#[cfg(test)]
mod test;

#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "testutils"))]
#[soroban_sdk::contract]
pub struct DefaultMockToken;

#[cfg(any(test, feature = "testutils"))]
#[soroban_sdk::contractimpl]
impl DefaultMockToken {
    pub fn balance(env: soroban_sdk::Env, addr: soroban_sdk::Address) -> i128 {
        let key = soroban_sdk::symbol_short!("balances");
        let mut balances: soroban_sdk::Map<soroban_sdk::Address, i128> = env.storage().instance().get(&key).unwrap_or_else(|| soroban_sdk::Map::new(&env));
        balances.get(addr).unwrap_or(100_000_000_000_000i128)
    }

    pub fn transfer(env: soroban_sdk::Env, from: soroban_sdk::Address, to: soroban_sdk::Address, amount: i128) {
        let key = soroban_sdk::symbol_short!("balances");
        let mut balances: soroban_sdk::Map<soroban_sdk::Address, i128> = env.storage().instance().get(&key).unwrap_or_else(|| soroban_sdk::Map::new(&env));
        let from_bal = balances.get(from.clone()).unwrap_or(100_000_000_000_000i128);
        let to_bal = balances.get(to.clone()).unwrap_or(100_000_000_000_000i128);
        balances.set(from.clone(), from_bal - amount);
        balances.set(to.clone(), to_bal + amount);
        env.storage().instance().set(&key, &balances);
    }
}

#[cfg(any(test, feature = "testutils"))]
fn register_mock_token_if_needed(env: &Env, token_addr: &Address) {
    use std::panic::AssertUnwindSafe;
    let env_clone = env.clone();
    let token_clone = token_addr.clone();
    let result = std::panic::catch_unwind(AssertUnwindSafe(move || {
        let client = TokenClient::new(&env_clone, &token_clone);
        let _ = client.balance(&token_clone);
    }));
    if result.is_err() {
        env.register_contract(token_addr, DefaultMockToken);
    }
}
