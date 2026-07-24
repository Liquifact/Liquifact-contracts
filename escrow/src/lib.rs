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
//!
//! ## Immutable protocol fee (SME disbursement split)
//!
//! [`LiquifactEscrow::init`] accepts an optional `protocol_fee_bps` (basis points, `0..=10_000`,
//! default `0`) stored immutably under [`DataKey::ProtocolFeeBps`]. At
//! [`LiquifactEscrow::withdraw`] the funded principal is split:
//!
//! ```text
//! fee        = funded_amount * protocol_fee_bps / 10_000   (floor, checked)
//! sme_payout = funded_amount - fee                          (checked)
//! ```
//!
//! `fee` is routed to [`DataKey::Treasury`] and `sme_payout` to [`InvoiceEscrow::sme_address`].
//! **Conservation invariant:** `sme_payout + fee == funded_amount` for every withdrawal, so no
//! principal is created or destroyed by the split. Rounding is **floor**, so any sub-`10_000`
//! residue stays with the SME (never over-charges the treasury). With `protocol_fee_bps == 0`
//! the behavior is byte-for-byte identical to the pre-fee contract: the full `funded_amount`
//! goes to the SME and no treasury transfer occurs.
//!
//! **Interaction with on-chain disbursement:** the fee is only realized when principal is
//! custodied on-chain and the SME calls [`LiquifactEscrow::withdraw`] — this feature depends on
//! the on-chain disbursement path. It does **not** apply to off-chain settlement
//! ([`LiquifactEscrow::settle`]), investor refunds ([`LiquifactEscrow::refund`]), or investor
//! claims ([`LiquifactEscrow::claim_investor_payout`]). The treasury here is the same immutable
//! address used by [`LiquifactEscrow::sweep_terminal_dust`]; the fee transfer reuses the same
//! SEP-41 balance-delta–checked path in [`external_calls`].

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
// See the schema version contract documentation: [Escrow schema versioning](../docs/escrow-schema-versioning.md)

/// Upper bound on [`LiquifactEscrow::append_attestation_digest`] entries to keep storage bounded.
/// Revocation via [`LiquifactEscrow::revoke_attestation_digest`] does not consume a slot.
pub const MAX_ATTESTATION_APPEND_ENTRIES: u32 = 32;

/// Maximum number of indices that can be revoked in a single batch call.
pub const MAX_ATTESTATION_REVOKE_BATCH: u32 = 32;

/// Default maximum maturity horizon in seconds (~5 years) when no explicit horizon is configured.
pub const DEFAULT_MATURITY_MAX_HORIZON_SECS: u64 = 157_680_000; // ~5 years (365.25 * 24 * 3600 * 5)

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
/// coupon       = total_principal × yield_bps / 10_000  (floor)   (1)
/// settle_pool  = total_principal + coupon                        (2)
/// gross_payout = contribution × settle_pool / total_principal    (3)
/// ```
///
/// Each step uses `checked_*` arithmetic on `i128`. We need the tightest
/// bound that keeps all three steps overflow-free for every valid
/// `yield_bps ∈ [0, 10_000]` and every `contribution ∈ (0, total_principal]`.
///
/// **Step (1)** — `total_principal × 10_000 ≤ i128::MAX` ⇒
/// `total_principal ≤ i128::MAX / 10_000` (≈ 1.7×10³⁴).
///
/// **Step (2)** — worst-case coupon is `total_principal` (when
/// `yield_bps = 10_000` and division is exact), so
/// `settle_pool = 2 × total_principal ≤ i128::MAX` ⇒
/// `total_principal ≤ i128::MAX / 2` (≈ 8.5×10³⁷).
///
/// **Step (3)** — the tightest gate: `contribution × settle_pool`
/// must not overflow. Maximise the product by setting
/// `contribution = total_principal` (single investor) and
/// `yield_bps = 10_000` so that `settle_pool = 2 × total_principal`.
/// Then
///
/// ```text
/// contribution × settle_pool = total_principal × 2 × total_principal
///                            = 2 × total_principal²
/// ```
///
/// Requiring `2 × total_principal² ≤ i128::MAX` gives
///
/// ```text
/// total_principal ≤ floor(√(i128::MAX / 2))
///                 = floor(√(2¹²⁷ − 1) / 2)
///                 = 2⁶³ − 1
///                 = 9_223_372_036_854_775_807
/// ```
///
/// This is the limiting constraint: it is tighter than both (1) and (2)
/// by many orders of magnitude. All intermediate `checked_*` operations
/// are overflow-free by construction for every valid init.
pub const MAX_INVOICE_AMOUNT: i128 = (1i128 << 63) - 1; // floor(√(i128::MAX / 2))

/// Upper bound on [`LiquifactEscrow::fund_batch`] entries to keep storage/CPU bounded.
/// Mirrors the spirit of `MAX_ATTESTATION_APPEND_ENTRIES` to limit per-call work.
pub const MAX_FUND_BATCH: u32 = 50;

/// Upper bound on [`LiquifactEscrow::refund_batch`] entries to keep storage/CPU bounded.
pub const MAX_REFUND_BATCH: u32 = 50;

/// Upper bound on [`LiquifactEscrow::set_investors_allowlisted`] batch size.
pub const MAX_INVESTOR_ALLOWLIST_BATCH: u32 = 32;

/// Upper bound on [`LiquifactEscrow::get_contributions`] / investor read batch size.
pub const MAX_INVESTOR_READ_BATCH: u32 = 50;

/// Upper bound on attestation digest read page size.
pub const MAX_ATTESTATION_READ_PAGE: u32 = 20;

/// Upper bound on [`LiquifactEscrow::sweep_terminal_dust`] per call (base units of the funding token).
///
/// Caps blast radius if instrumentation mis-estimates “dust”; tune per asset decimals off-chain.
pub const MAX_DUST_SWEEP_AMOUNT: i128 = 100_000_000;

/// Maximum UTF-8 byte length for the invoice `String` at init (matches Soroban [`Symbol`] max).
pub const MAX_INVOICE_ID_STRING_LEN: u32 = 32;

/// Default validity window for [`LiquifactEscrow::propose_admin`] when no explicit window is supplied.
///
/// After `ledger.timestamp() + DEFAULT_ADMIN_PROPOSAL_VALIDITY_SECS`, [`LiquifactEscrow::accept_admin`]
/// rejects the stale proposal with [`EscrowError::AdminProposalExpired`].
pub const DEFAULT_ADMIN_PROPOSAL_VALIDITY_SECS: u64 = 604_800; // 7 days

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
    /// [`LiquifactEscrow::init`] called when escrow storage already exists.
    EscrowAlreadyInitialized = 3,
    /// [`LiquifactEscrow::init`] rejected an invoice amount too large to keep
    /// `compute_investor_payout` arithmetic overflow-free.
    AmountExceedsMax = 14,
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
    /// [`LiquifactEscrow::revoke_attestation_digests`] received an empty indices list.
    AttestationBatchEmpty = 54,
    /// [`LiquifactEscrow::revoke_attestation_digests`] exceeded [`MAX_ATTESTATION_REVOKE_BATCH`].
    AttestationBatchTooLarge = 55,
    /// [`LiquifactEscrow::unrevoke_attestation_digest`] called on an index that is not revoked.
    AttestationNotRevoked = 56,

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
    /// [`LiquifactEscrow::fund_batch`] contains two or more entries with the same investor address.
    ///
    /// Every investor address in the batch must be unique. Duplicate addresses indicate a
    /// malformed batch and the entire call is rejected atomically before any state mutation.
    FundingBatchDuplicateInvestor = 84,
    /// [`LiquifactEscrow::get_contributions`] exceeded [`MAX_INVESTOR_READ_BATCH`].
    ContributionReadBatchTooLarge = 203,
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
    /// [`LiquifactEscrow::raise_max_unique_investors`] did not strictly raise the cap.
    NewCapNotHigher = 176,
    /// [`LiquifactEscrow::lower_max_unique_investors`] set cap below current unique funder count.
    NewCapBelowCurrentFunderCount = 78,
    /// [`LiquifactEscrow::update_maturity`] called while escrow is not open.
    MaturityUpdateNotOpen = 79,
    /// [`LiquifactEscrow::propose_admin`] nominated the current admin address.
    NewAdminSameAsCurrent = 80,
    /// [`LiquifactEscrow::propose_admin`] repeated the already-pending admin address.
    PendingAdminUnchanged = 177,
    /// [`LiquifactEscrow::update_maturity`] set maturity to the same value as current.
    MaturityUnchanged = 81,
    /// [`LiquifactEscrow::accept_admin`] called after the proposal expiry recorded at
    /// [`DataKey::PendingAdminExpiry`]. Re-propose to nominate a fresh successor.
    AdminProposalExpired = 85,

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
    ///
    /// Tier and lock selection are immutable after the first deposit leg. Once an investor
    /// has a non-zero contribution recorded under [`DataKey::InvestorContribution`], the
    /// yield rate and claim-lock timestamp are permanently fixed; calling
    /// [`LiquifactEscrow::fund_with_commitment`] again would allow re-selecting a tier,
    /// violating the fairness guarantee.
    ///
    /// **Client action:** Use [`LiquifactEscrow::fund`] for all additional principal from
    /// the same investor. `fund()` reads the stored effective yield set on the first leg
    /// and does not allow tier re-selection.
    ///
    /// **Code:** `108` — stable, append-only.
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
    /// [`LiquifactEscrow::refund_batch`] received an empty investors vector.
    RefundBatchEmpty = 144,
    /// [`LiquifactEscrow::refund_batch`] exceeded [`MAX_REFUND_BATCH`].
    RefundBatchTooLarge = 145,

    /// `clear_legal_hold` was called without a prior `request_legal_hold_clear`.
    LegalHoldClearRequestMissing = 150,
    /// The two-phase legal-hold clear delay has not elapsed yet.
    LegalHoldClearNotReady = 151,
    /// Computing the legal-hold clear ready-at timestamp would overflow.
    LegalHoldClearDelayOverflow = 152,
    /// Funding deadline has passed, new deposits are rejected.
    FundingDeadlinePassed = 164,

    /// A legal hold blocks rotating the beneficiary (SME) address.
    LegalHoldBlocksBeneficiaryRotation = 160,
    /// Beneficiary rotation was attempted while the escrow was not in a
    /// pre-settlement state (`status` must be 0 = open or 1 = funded).
    RotationNotOpen = 161,
    /// The proposed new SME address is identical to the current beneficiary.
    NewSmeSameAsCurrent = 162,

    /// Attempted to accept or cancel admin role when no pending admin exists.
    NoPendingAdmin = 172,
    /// The contract's funding-token balance is less than `funded_amount` at withdraw time.
    /// Funds must be custodied in this contract before the SME can pull them.
    InsufficientContractBalance = 165,
    /// The maturity timestamp is in the past relative to the current ledger time.
    MaturityInPast = 166,
    /// The maturity timestamp exceeds the configured maximum horizon from the current ledger time.
    MaturityExceedsMaxHorizon = 167,
    /// `clear_sme_collateral_commitment` was called when no commitment pledge exists.
    NoCollateralToClear = 169,
    /// The computed investor payout is zero; nothing to transfer.
    PayoutZero = 170,
    /// `update_funding_deadline` was called on a non-open escrow (status != 0).
    FundingDeadlineUpdateNotOpen = 171,
    /// [`LiquifactEscrow::extend_funding_deadline`] did not strictly extend the stored deadline.
    FundingDeadlineNotExtended = 206,
    /// [`LiquifactEscrow::extend_funding_deadline`] would place the deadline at or beyond maturity.
    FundingDeadlineBeyondMaturity = 204,
    /// [`LiquifactEscrow::extend_funding_deadline`] called when no funding deadline is configured.
    FundingDeadlineNotSet = 205,

    /// [`LiquifactEscrow::lower_min_contribution_floor`] called while escrow is not open.
    FloorLowerNotOpen = 173,
    /// [`LiquifactEscrow::lower_min_contribution_floor`] did not strictly lower the floor.
    NewFloorNotLower = 174,
    /// [`LiquifactEscrow::lower_min_contribution_floor`] received a non-positive floor.
    NewFloorNotPositive = 175,
    /// Caller is not authorized to perform partial settlement.
    /// Only the escrow's `sme_address` or `admin` may call [`LiquifactEscrow::partial_settle`].
    PartialSettleUnauthorizedCaller = 200,
    /// [`LiquifactEscrow::partial_settle`] blocked while a legal hold is active.
    LegalHoldBlocksPartialSettle = 201,
    /// [`LiquifactEscrow::partial_settle`] called while escrow is not in open status (`status != 0`).
    PartialSettleNotOpen = 202,
    MaxPerInvestorCapNotConfigured = 24, // new
    MaxPerInvestorCapNotRaised = 25,     // new
    /// [`LiquifactEscrow::raise_maturity_max_horizon`] received a `new_horizon` that is
    /// not strictly greater than the current stored horizon.
    HorizonNotRaised = 214,

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
    /// Unfunding is only valid while the escrow is still accepting contributions.
    UnfundEscrowNotOpen = 220,

    /// [`LiquifactEscrow::unfund`] requested amount exceeds the investor's recorded contribution.
    /// Never withdraw more than was contributed; checked via [`i128::checked_sub`].
    OverWithdrawal = 221,

    /// [`LiquifactEscrow::unfund`] blocked because a compliance/legal hold is active.
    /// No fund movement is permitted until the hold is cleared by the admin.
    UnfundLegalHoldActive = 222,
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

/// Assert that `actual_status == expected_status`, emitting `error` otherwise.
///
/// This is the shared primitive used by all status gate helpers. Callers that need a
/// specific named status check (e.g. [`require_funding_open`]) delegate here so the
/// exact error code is preserved at every call site.
#[inline(always)]
pub(crate) fn guard_status_eq(
    env: &Env,
    actual_status: u32,
    expected_status: u32,
    error: EscrowError,
) {
    ensure(env, actual_status == expected_status, error);
}

/// Assert that `actual_status` is one of the values in `allowed`, emitting `error` otherwise.
///
/// Used for terminal-state checks where multiple valid statuses apply (e.g. sweep dust
/// is allowed in settled/withdrawn/cancelled).
#[allow(dead_code)]
#[inline(always)]
pub(crate) fn guard_status_in(env: &Env, actual_status: u32, allowed: &[u32], error: EscrowError) {
    ensure(env, allowed.contains(&actual_status), error);
}

/// Shared guard: assert that the escrow is in the **open funding window** (status == 0).
///
/// Every entrypoint that accepts new principal — [`LiquifactEscrow::fund`],
/// [`LiquifactEscrow::fund_with_commitment`], [`LiquifactEscrow::fund_batch`],
/// [`LiquifactEscrow::update_funding_target`], [`LiquifactEscrow::lower_max_unique_investors`],
/// and [`LiquifactEscrow::lower_min_contribution_floor`] — must call this helper instead of
/// inlining the status comparison. Centralising the gate means adding a new open-window
/// operation cannot accidentally omit or diverge from the check.
///
/// # Errors
/// Panics with [`EscrowError::EscrowNotOpenForFunding`] when `escrow.status != 0`.
///
/// # Security notes
/// This helper is intentionally **read-only** (no storage writes). Callers must complete their
/// own `Address::require_auth()` before performing any storage mutation; this guard only
/// validates escrow state and cannot substitute for an authorization check.
#[inline(always)]
pub(crate) fn require_funding_open(env: &Env, status: u32) {
    guard_status_eq(env, status, 0, EscrowError::EscrowNotOpenForFunding);
}

/// Shared guard: assert that no legal/compliance hold is currently active.
///
/// Replaces the repeated inline pattern
/// `ensure(&env, !Self::legal_hold_active(&env), EscrowError::LegalHoldBlocks*)` that previously
/// appeared at every risk-bearing entrypoint — `sweep_terminal_dust`, `rotate_beneficiary`,
/// `fund_impl`, `partial_settle`, `settle`, `withdraw`, `claim_investor_payout`, and
/// `cancel_funding`. By centralising the read of [`DataKey::LegalHold`] and the negation we
/// guarantee that adding a new risk-bearing entrypoint cannot accidentally forget the hold
/// check or pick the wrong `LegalHoldBlocks*` variant — the caller passes the typed error
/// variant that documents which entrypoint was blocked.
///
/// # Errors
/// Panics with the caller-supplied `error` (one of the `EscrowError::LegalHoldBlocks*`
/// variants) when [`DataKey::LegalHold`] is `true`.
///
/// # Security notes
/// - Read-only: performs a single instance-storage read with `unwrap_or(false)` (no panic on
///   missing key). Does not write or delete any storage key.
/// - This helper is **not** an authorization check. Callers must still call
///   `Address::require_auth()` for the entrypoint's bound role before any storage mutation
///   or token transfer, per [ADR-002](docs/adr/ADR-002-auth-boundaries.md).
/// - The `LegalHold` flag is independent of the operational pause ([`DataKey::Paused`]); an
///   entrypoint that needs both gates must compose `guard_not_legal_hold` with
///   `ensure(!paused_active(env), PausedBlocks*)` itself.
#[inline(always)]
pub(crate) fn guard_not_legal_hold(env: &Env, error: EscrowError) {
    ensure(env, !LiquifactEscrow::legal_hold_active(env), error);
}

/// Predicate: `true` when `status` is one of the **terminal** escrow states
/// (`2` = settled, `3` = withdrawn, `4` = cancelled).
///
/// Used to gate entries that only make sense after the escrow has reached a final
/// disposition — e.g. [`LiquifactEscrow::sweep_terminal_dust`], which sweeps
/// rounding-residue / stray-transfer balances only in terminal states, or liability-floor
/// checks that must only run when no further principal inbound is possible.
///
/// Centralising this predicate keeps the `settled | withdrawn | cancelled` set definitionally
/// identical across every call site — adding a new status code (e.g. a future
/// `claimed` state) only requires editing this helper and a single call-site comment.
///
/// # Notes
/// Pure function: no storage access, no token interaction. Safe to call from
/// any context where a `status: u32` value is in hand (entrypoint, view function, test).
///
/// # Security notes
/// This is a **predicate**, not a guard — callers that need to *enforce* the terminal
/// precondition must wrap the call in `ensure(&env, is_terminal_status(status), error)`.
/// Mixing predicates and guards deliberately: predicates let view helpers and tests reuse
/// the definition without hiding a panic, while `guard_status_eq` /
/// `guard_status_in` keep the call-site `ensure` self-documenting at entrypoints.
#[inline(always)]
pub(crate) fn is_terminal_status(status: u32) -> bool {
    matches!(status, 2..=4)
}

/// Predicate: `true` when `status` is one of the **pre-settlement** escrow states
/// (`0` = open, `1` = funded).
///
/// Used by entrypoints that must run after funding closed but before settlement
/// finalised — e.g. [`LiquifactEscrow::rotate_beneficiary`], which lets the SME/admin
/// re-point the payout destination only while the escrow is still open or funded.
///
/// Centralising the predicate keeps the `open | funded` set definitionally identical across
/// every call site.
///
/// # Notes
/// Pure function: no storage access, no token interaction.
///
/// # Security notes
/// This is a **predicate**, not a guard. Callers that need to *enforce* the pre-settlement
/// precondition must wrap it in
/// `ensure(&env, is_pre_settlement_status(status), error)`.
#[inline(always)]
pub(crate) fn is_pre_settlement_status(status: u32) -> bool {
    matches!(status, 0 | 1)
}

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
    /// Ledger timestamp (seconds) after which [`LiquifactEscrow::accept_admin`] rejects the
    /// pending proposal. Written alongside [`DataKey::PendingAdmin`] on every
    /// [`LiquifactEscrow::propose_admin`] call; cleared on acceptance or cancellation.
    PendingAdminExpiry,
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
    /// Index of allowlisted addresses for paginated enumeration.
    AllowlistIndex,
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
    /// Optional funding deadline timestamp; absent ⇒ no deadline.
    /// Written by [`LiquifactEscrow::init`] and extended by
    /// [`LiquifactEscrow::extend_funding_deadline`]; checked during [`LiquifactEscrow::fund`].
    FundingDeadline,
    /// Ordered list of all investor addresses; used for pagination via [`LiquifactEscrow::get_investors`].
    /// Absent ⇒ empty list (no investors yet funded).
    InvestorIndex,
    /// Ledger timestamp recorded when [`LiquifactEscrow::settle`] transitions status to 2.
    /// Absent ⇒ not yet settled, or legacy instance. Read via [`LiquifactEscrow::get_settled_at`].
    SettledAt,
    /// When true, a lightweight **operational pause** blocks risk-bearing entrypoints
    /// (`fund`, `settle`, `withdraw`, `claim_investor_payout`) for incident response.
    /// Absent ⇒ `false` (not paused). Toggled by admin via [`LiquifactEscrow::set_paused`].
    ///
    /// Orthogonal to [`DataKey::LegalHold`]: the pause has **no** compliance semantics and
    /// **no** two-phase clear delay — it is a single-call admin switch for incidents such as a
    /// suspected token bug. Either flag independently blocks the gated entrypoints.
    Paused,
    /// Immutable protocol fee in basis points (0..=10_000) applied to the SME disbursement
    /// at [`LiquifactEscrow::withdraw`]; set once in [`LiquifactEscrow::init`].
    /// Written as `0` even when unconfigured so reads always succeed (`.unwrap_or(0)`).
    /// Stored as `i64` to match the [`InvoiceEscrow::yield_bps`] basis-point convention.
    /// **Additive key (ADR-007):** absent on instances predating this key ⇒ read as `0`
    /// (no fee), preserving legacy full-principal disbursement semantics.
    ProtocolFeeBps,
}

// --- Data types ---

/// Full state of an invoice escrow persisted in contract storage (`DataKey::Escrow`).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
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

/// Bundled settlement-readiness snapshot returned by
/// [`LiquifactEscrow::get_settlement_readiness`].
///
/// Lets an integrator decide whether [`LiquifactEscrow::settle`] will succeed on the current
/// ledger with a single host invocation, instead of stitching together [`LiquifactEscrow::is_settleable`],
/// [`LiquifactEscrow::get_legal_hold`], [`LiquifactEscrow::has_maturity_lock`], and the maturity
/// timestamp — and re-deriving the contract's own precedence rules off-chain (which drifts).
///
/// # Precedence
/// `ready_now` is computed from the **same** single-source-of-truth gate `settle`/`partial_settle`
/// apply (legal hold blocks first, then funded status, then maturity). A `true` value reliably
/// predicts a successful `settle` on the current ledger.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementReadiness {
    /// Mirrors [`LiquifactEscrow::is_settleable`]: funded, matured, and not on legal hold.
    pub is_settleable: bool,
    /// `true` when a legal/compliance hold is currently active (blocks settlement).
    pub legal_hold_active: bool,
    /// `true` when there is no maturity lock (`maturity == 0`) or the maturity timestamp has
    /// been reached (`now >= maturity`).
    pub maturity_reached: bool,
    /// Single derived flag: `true` exactly when `settle` would succeed on the current ledger.
    pub ready_now: bool,
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
pub struct MaxUniqueInvestorsCapRaised {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_cap: u32,
    pub new_cap: u32,
}

#[contractevent]
pub struct MinContributionFloorLowered {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_floor: i128,
    pub new_floor: i128,
}

#[contractevent]
pub struct MaxPerInvestorCapRaised {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_cap: i128,
    pub new_cap: i128,
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
    /// Total settlement pool (principal + coupon) at settlement time.
    /// Computed using the same checked arithmetic and floor rounding as
    /// [`LiquifactEscrow::compute_investor_payout`]: `coupon = funded_amount × yield_bps / 10_000` (floor),
    /// then `settle_pool = funded_amount + coupon`.
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
pub struct AdminTransferredEvent {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub new_admin: Address,
}

#[contractevent]
pub struct AdminAcceptedEvent {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub prior_admin: Address,
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

/// Emitted by [`LiquifactEscrow::propose_admin`] when a different pending admin proposal is
/// replaced before it is accepted or cancelled.
///
/// Indexers can distinguish a true supersede from a first-time proposal without inferring it from
/// storage diffs.
#[contractevent]
pub struct AdminProposalSuperseded {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub previous_pending: Address,
    pub new_pending: Address,
}

/// Emitted by [`LiquifactEscrow::cancel_pending_admin`] when a pending admin proposal is cancelled.
///
/// Indexers and operators can monitor this event to track when nominations are retracted.
///
/// # Fields
/// - `name`: hardcoded `adm_can` symbol.
/// - `invoice_id`: escrow invoice identifier.
/// - `cancelled_pending`: the address whose pending admin nomination was revoked.
#[contractevent]
pub struct AdminProposalCancelled {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub cancelled_pending: Address,
}

/// Emitted by [`LiquifactEscrow::transfer_admin`] (the deprecated one-step
/// admin transfer shim) so indexers and operators can flag integrators
/// still using the legacy single-step path.
///
/// # Fields
/// - `name`: hardcoded `depr_xfer` symbol.
/// - `invoice_id`: escrow invoice identifier.
/// - `proposed_address`: the address that was passed through the deprecated shim.
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

/// Emitted by [\LiquifactEscrow::extend_funding_deadline\] when the admin pushes the
/// funding deadline forward while the escrow is open.
#[contractevent]
pub struct FundingDeadlineExtended {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_deadline: u64,
    pub new_deadline: u64,
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

/// Emitted by [`LiquifactEscrow::set_paused`] whenever the operational pause flag is written.
///
/// Independent of [`LegalHoldChanged`]: this signals the lightweight incident-response switch,
/// not the compliance hold.
#[contractevent]
pub struct PausedChanged {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    /// `1` = pause enabled, `0` = cleared.
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

#[allow(dead_code)]
#[contractevent]
/// NOTE: Defined but never emitted — no `update_legal_hold_clear_delay` setter
/// exists yet.  Marked as dead code; remove or wire up when the feature is added.
pub struct LegalHoldClearDelayUpdated {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_delay: u64,
    pub new_delay: u64,
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

/// Emitted when the SME clears the stored metadata-only collateral commitment.
///
/// This event is the removal-side counterpart to [`CollateralRecordedEvt`]. It
/// copies the stored commitment fields before deletion so off-chain indexers can
/// reconstruct which SME-reported asset record was retired without polling
/// storage after the mutation.
///
/// # Fields
/// - `name`: Hardcoded `coll_clr` symbol.
/// - `invoice_id`: Symbol representation of the invoice.
/// - `asset`: Cleared SME-reported off-chain asset symbol.
/// - `amount`: Cleared SME-reported amount.
/// - `recorded_at`: Ledger timestamp from the original commitment record.
#[contractevent]
pub struct CollateralClearedEvt {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    /// SME-reported off-chain asset symbol that was cleared from storage.
    pub asset: Symbol,
    /// SME-reported amount that was cleared from storage.
    pub amount: i128,
    /// Ledger timestamp from the original recorded commitment.
    pub recorded_at: u64,
}

/// Emitted after [`LiquifactEscrow::clear_sme_collateral_commitment`] removes
/// the SME-reported collateral metadata.
///
/// This supplements [`CollateralClearedEvt`] with a short event name and the
/// full cleared commitment identity so indexers can reconstruct the
/// record-to-clear lifecycle without a storage read after removal.
#[contractevent]
pub struct CollateralCommitmentCleared {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub asset: Symbol,
    pub amount: i128,
    pub recorded_at: u64,
}

#[contractevent]
pub struct SmeWithdrew {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    /// Net principal transferred to the SME `recipient` (`funded_amount - fee`).
    pub amount: i128,
    pub recipient: Address,
    /// Protocol fee routed to [`DataKey::Treasury`] (`0` when `protocol_fee_bps == 0`).
    pub fee: i128,
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
pub struct SettlementStateChanged {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_status: u32,
    pub new_status: u32,
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

/// Emitted after a successful [`LiquifactEscrow::unfund`] call.
///
/// The investor partially or fully exits their principal position while the escrow
/// remains open (status 0). Carries the withdrawal amount, the investor's remaining
/// contribution, the escrow's updated `funded_amount`, and the ledger timestamp.
#[contractevent]
pub struct EscrowUnfunded {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    #[topic]
    pub investor: Address,
    /// Amount withdrawn in this call.
    pub amount: i128,
    /// Investor's remaining contribution after this withdrawal.
    pub remaining_contribution: i128,
    /// Escrow's total funded_amount after this withdrawal.
    pub new_funded_amount: i128,
    /// Ledger timestamp at which the withdrawal occurred.
    pub timestamp: u64,
}

#[contractevent]
pub struct RegistryRefRebound {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    /// New registry hint; `None` clears the stored value.
    pub registry: Option<Address>,
}

/// Emitted after a successful [`LiquifactEscrow::sweep_terminal_dust`] transfer.
///
/// Carries the **effective** swept amount (after balance and liability-floor capping),
/// the treasury recipient, the funding token, and the invoice id for indexer reconciliation.
#[contractevent]
pub struct TreasuryDustSwept {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    /// Immutable treasury address that received the sweep.
    pub recipient: Address,
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

#[contractevent]
pub struct AttestationDigestUnrevoked {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub index: u32,
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

/// Emitted by [`LiquifactEscrow::raise_maturity_max_horizon`] when the maturity ceiling is
/// monotonically raised. Carries the `invoice_id` and the old/new horizon values.
#[contractevent]
pub struct MaturityMaxHorizonRaised {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_horizon: u64,
    pub new_horizon: u64,
}

/// Digest entry with revocation status returned by `get_attestation_digest_at`.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttestationDigestInfo {
    /// The 32‑byte digest stored at the requested index.
    pub digest: BytesN<32>,
    /// `true` if the entry has been revoked via `revoke_attestation_digest`.
    pub revoked: bool,
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

/// Emitted once per [`LiquifactEscrow::set_investors_allowlisted`] call after
/// all per-investor `InvestorAllowlistChanged` events, providing a single
/// batch-level signal for indexers.
#[contractevent]
pub struct InvestorAllowlistBatchApplied {
    /// Hardcoded `"al_batch"` symbol (topic).
    #[topic]
    pub name: Symbol,
    /// The escrow's `invoice_id` (topic, for indexer correlation).
    #[topic]
    pub invoice_id: Symbol,
    /// Number of addresses in the batch.
    pub batch_size: u32,
    /// `1` = allowed, `0` = blocked.
    pub allowed: u32,
}

#[contractevent]
pub struct LegalHoldClearCancelled {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
}

/// Emitted by [`LiquifactEscrow::upgrade`] immediately before the WASM is replaced.
///
/// The event is published **before** `env.deployer().update_current_contract_wasm` so that
/// the record is captured even if the deployer call somehow reverts. Indexers and operators
/// can correlate this event with the `invoice_id` to audit the upgrade history of a specific
/// escrow instance.
///
/// # Fields
/// - `name`: hardcoded `"upgrade"` symbol (topic).
/// - `invoice_id`: the escrow's `invoice_id` (topic, for indexer correlation).
/// - `new_wasm_hash`: the 32-byte hash of the incoming WASM binary.
#[contractevent]
pub struct ContractUpgraded {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub new_wasm_hash: BytesN<32>,
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

    /// Read the operational pause flag ([`DataKey::Paused`]); defaults to `false` when unset.
    ///
    /// Orthogonal to [`LiquifactEscrow::legal_hold_active`] — neither flag affects the other.
    fn paused_active(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
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

    /// Returns the contract's current funding-token balance for on-chain custody reconciliation.
    ///
    /// Reads [`DataKey::FundingToken`] and queries the token contract for the live balance
    /// held by the escrow contract address.
    ///
    /// # Errors
    /// Panics with [`EscrowError::FundingTokenNotSet`] if called before [`LiquifactEscrow::init`].
    ///
    /// **Pure read** — no authorization required, no state mutation.
    pub fn get_token_balance(env: Env) -> i128 {
        let token_addr = Self::funding_token_or_fail(&env);
        let this = env.current_contract_address();
        TokenClient::new(&env, &token_addr).balance(&this)
    }

    /// Read the immutable treasury address, failing with [`EscrowError::TreasuryNotSet`]
    /// when the escrow has not been initialized.
    fn treasury_or_fail(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Treasury)
            .unwrap_or_else(|| fail(env, EscrowError::TreasuryNotSet))
    }
    /// Validates the optional yield-tier table supplied at `init`.
    ///
    /// # Rules
    ///
    /// | Rule | Error |
    /// |------|-------|
    /// | Each `yield_bps` in `0..=10_000` | `TierYieldOutOfRange` |
    /// | Each `yield_bps >= base_yield` | `TierYieldBelowBase` |
    /// | `min_lock_secs` strictly increasing across tiers | `TierLockNotIncreasing` |
    /// | `yield_bps` non-decreasing across tiers | `TierYieldNotNonDecreasing` |
    ///
    /// # Accepted example
    /// ```text
    /// base_yield = 800 bps
    /// tiers = [(min_lock=100, yield=900), (min_lock=200, yield=1000)]
    /// valid: locks increase (100 < 200), yields non-decrease (900 <= 1000), both >= 800
    /// ```
    ///
    /// # Rejected examples
    /// ```text
    /// tiers = [(min_lock=200, yield=900), (min_lock=100, yield=1000)]
    /// TierLockNotIncreasing: 200 > 100
    ///
    /// tiers = [(min_lock=100, yield=700)]
    /// TierYieldBelowBase: 700 < 800
    ///
    /// tiers = [(min_lock=100, yield=1000), (min_lock=200, yield=900)]
    /// TierYieldNotNonDecreasing: 1000 > 900
    /// ```
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
    ///
    /// Scans [`DataKey::YieldTierTable`] and picks the tier with the highest `yield_bps`
    /// where `committed_lock_secs >= tier.min_lock_secs`. Returns base yield when:
    /// `committed_lock_secs == 0`, no tier table exists, or table is empty.
    ///
    /// Example with `base=800, tiers=[(100,900),(200,1000),(300,1200)]`:
    /// - lock=50  -> (800, 0)    no tier matched
    /// - lock=100 -> (900, 100)  tier 0
    /// - lock=250 -> (1000, 200) tier 1
    /// - lock=300 -> (1200, 300) tier 2 (highest)
    ///
    /// `matched_lock_secs` is the `min_lock_secs` of the matched tier, or `0` for base yield.
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
        funding_deadline: Option<u64>,
        allowlist_active: Option<bool>,
        protocol_fee_bps: Option<i64>,
    ) -> InvoiceEscrow {
        admin.require_auth();

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
        // Immutable protocol fee in basis points (default 0 = no fee). Validated to the same
        // 0..=10_000 envelope as `yield_bps`; `10_000` routes the entire `funded_amount` to the
        // treasury at withdrawal. See `docs/escrow-numeric-model.md` for the split math.
        let protocol_fee_bps = protocol_fee_bps.unwrap_or(0);
        ensure(
            &env,
            (0..=10_000).contains(&protocol_fee_bps),
            EscrowError::ProtocolFeeBpsOutOfRange,
        );
        ensure(
            &env,
            !env.storage().instance().has(&DataKey::Escrow),
            EscrowError::EscrowAlreadyInitialized,
        );

        Self::validate_yield_tiers_table(&env, &yield_tiers, yield_bps);

        let max_horizon = maturity_max_horizon.unwrap_or(DEFAULT_MATURITY_MAX_HORIZON_SECS);
        validate_maturity_bounds(&env, maturity, max_horizon);
        env.storage()
            .instance()
            .set(&DataKey::MaturityMaxHorizon, &max_horizon);

        env.storage()
            .instance()
            .set(&DataKey::FundingToken, &funding_token);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage()
            .instance()
            .set(&DataKey::Version, &SCHEMA_VERSION);

        if let Some(reg) = &registry {
            env.storage().instance().set(&DataKey::RegistryRef, reg);
        }

        if let Some(tiers) = &yield_tiers {
            env.storage()
                .instance()
                .set(&DataKey::YieldTierTable, tiers);
        }
        if let Some(mc) = min_contribution {
            ensure(&env, mc > 0, EscrowError::MinContributionNotPositive);
            ensure(
                &env,
                mc <= amount,
                EscrowError::MinContributionExceedsAmount,
            );
        }

        let floor = min_contribution.unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::MinContributionFloor, &floor);
        // Always persist the fee (even the `0` default) so `withdraw` reads never branch on absence.
        env.storage()
            .instance()
            .set(&DataKey::ProtocolFeeBps, &protocol_fee_bps);
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
            env.storage()
                .instance()
                .set(&DataKey::AllowlistActive, &active);
        }

        if let Some(deadline) = funding_deadline {
            let now = env.ledger().timestamp();
            ensure(&env, deadline > now, EscrowError::FundingDeadlinePassed);
            if maturity > 0 {
                ensure(
                    &env,
                    deadline < maturity,
                    EscrowError::FundingDeadlineBeyondMaturity,
                );
            }
            env.storage()
                .instance()
                .set(&DataKey::FundingDeadline, &deadline);
        }

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

        let has_maturity_lock = maturity != 0;
        EscrowInitialized {
            name: symbol_short!("escrow_ii"),
            escrow: escrow.clone(),
            funding_token,
            treasury,
            registry,
            has_maturity_lock,
        }
        .publish(&env);

        escrow
    }

    /// Returns the full escrow snapshot ([`InvoiceEscrow`]) from [`DataKey::Escrow`].
    ///
    /// Emits [`EscrowError::EscrowNotInitialized`] (code 20) if called before [`LiquifactEscrow::init`].
    pub fn get_escrow(env: Env) -> InvoiceEscrow {
        env.storage()
            .instance()
            .get(&DataKey::Escrow)
            .unwrap_or_else(|| fail(&env, EscrowError::EscrowNotInitialized))
    }

    /// Returns the remaining funding capacity before the funding target is reached.
    ///
    /// Clamped to `0` via `saturating_sub` if the escrow is over-funded.
    pub fn get_remaining_funding_capacity(env: Env) -> i128 {
        let escrow = Self::get_escrow(env);
        escrow
            .funding_target
            .saturating_sub(escrow.funded_amount)
            .max(0)
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

    /// Admin-only: rebind the off-chain registry hint stored under [`DataKey::RegistryRef`].
    ///
    /// This registry reference is a **hint only** for off-chain indexers and must not be used
    /// as an authority boundary in on-chain logic.
    ///
    /// # Authorization
    /// Requires the signature of the current [`InvoiceEscrow::admin`].
    ///
    /// # Events
    /// Emits [`RegistryRefRebound`] with the new value (`Some(addr)` or `None` to clear).
    pub fn rebind_registry_ref(env: Env, registry: Option<Address>) {
        let escrow = Self::load_escrow_require_admin(&env);

        match registry.clone() {
            Some(_) => {
                env.storage()
                    .instance()
                    .set(&DataKey::RegistryRef, &registry);
            }
            None => {
                env.storage().instance().remove(&DataKey::RegistryRef);
            }
        }

        RegistryRefRebound {
            name: Symbol::new(&env, "reg_rebind"),
            invoice_id: escrow.invoice_id,
            registry,
        }
        .publish(&env);
    }

    /// Admin-only: clear the off-chain registry hint.
    ///
    /// Convenience wrapper around `rebind_registry_ref` with `None`.
    /// Emits the same `RegistryRefRebound` event with `registry = None`.
    pub fn clear_registry_ref(env: Env) {
        Self::rebind_registry_ref(env, None);
    }

    /// Returns the optional pending admin address waiting for [`LiquifactEscrow::accept_admin`],
    /// or [`None`] when no admin handover is in progress.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::PendingAdmin)
    }

    /// Returns the ledger timestamp after which [`LiquifactEscrow::accept_admin`] rejects the
    /// current proposal, or [`None`] when no expiry is recorded (no handover in progress).
    pub fn get_pending_admin_expiry(env: Env) -> Option<u64> {
        env.storage().instance().get(&DataKey::PendingAdminExpiry)
    }

    pub fn get_pending_admin_remaining_secs(env: Env) -> Option<u64> {
        let pending: Option<Address> = env.storage().instance().get(&DataKey::PendingAdmin);
        #[allow(clippy::question_mark)]
        if pending.is_none() {
            return None;
        }
        let expiry: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdminExpiry)
            .unwrap_or(0);
        let now = env.ledger().timestamp();
        if now >= expiry {
            Some(0)
        } else {
            Some(expiry.saturating_sub(now))
        }
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

    /// Move up to `amount` (capped by balance and [`MAX_DUST_SWEEP_AMOUNT`]) of the **funding token**
    /// from this contract to [`DataKey::Treasury`].
    ///
    /// See [`docs/escrow-cancellation-refunds.md`](../../docs/escrow-cancellation-refunds.md)
    /// for more details on the liability floor, operator guidelines, and worked examples.
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
        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksTreasuryDustSweep);
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
            is_terminal_status(escrow.status),
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

        TreasuryDustSwept {
            name: symbol_short!("dust_sw"),
            invoice_id: escrow.invoice_id.clone(),
            recipient: treasury.clone(),
            token: token_addr,
            amount: sweep_amt,
        }
        .publish(&env);

        sweep_amt
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
        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksBeneficiaryRotation);

        let mut escrow = Self::get_escrow(env.clone());

        // Only permitted in pre-settlement states (open or funded).
        ensure(
            &env,
            is_pre_settlement_status(escrow.status),
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

    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Version).unwrap_or(0)
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

    /// Whether the lightweight operational pause is active (defaults to `false` if unset).
    ///
    /// Independent of [`LiquifactEscrow::get_legal_hold`]: this reports the incident-response
    /// switch toggled by [`LiquifactEscrow::set_paused`], not the compliance hold.
    pub fn is_paused(env: Env) -> bool {
        Self::paused_active(&env)
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

    /// Immutable protocol fee in basis points (`0..=10_000`) applied to the SME disbursement at
    /// [`LiquifactEscrow::withdraw`]; `0` means no fee (full `funded_amount` goes to the SME).
    ///
    /// Set once at [`LiquifactEscrow::init`] and never mutated. Reads `0` for instances predating
    /// [`DataKey::ProtocolFeeBps`] (additive-key default), matching legacy disbursement behavior.
    pub fn get_protocol_fee_bps(env: Env) -> i64 {
        env.storage()
            .instance()
            .get(&DataKey::ProtocolFeeBps)
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

    /// Returns the digest and revocation flag at `index`.
    /// Returns `None` when `index >= log.len()`.
    pub fn get_attestation_digest_at(env: Env, index: u32) -> Option<AttestationDigestInfo> {
        let log = Self::get_attestation_append_log(env.clone());
        if index >= log.len() {
            return None;
        }
        let digest = log.get(index).unwrap();
        let revoked = env
            .storage()
            .instance()
            .get(&DataKey::AttestationRevoked(index))
            .unwrap_or(false);
        Some(AttestationDigestInfo { digest, revoked })
    }

    // --- Persistent per-investor storage helpers ---
    fn get_persistent_investor_contribution(env: &Env, investor: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::InvestorContribution(investor))
            .unwrap_or(0)
    }

    fn set_persistent_investor_contribution(env: &Env, investor: Address, amount: i128) {
        env.storage()
            .persistent()
            .set(&DataKey::InvestorContribution(investor), &amount);
    }

    fn get_persistent_investor_effective_yield(env: &Env, investor: Address) -> Option<i64> {
        env.storage()
            .persistent()
            .get(&DataKey::InvestorEffectiveYield(investor))
    }

    fn set_persistent_investor_effective_yield(env: &Env, investor: Address, value: i64) {
        env.storage()
            .persistent()
            .set(&DataKey::InvestorEffectiveYield(investor), &value);
    }

    fn get_persistent_investor_claim_not_before(env: &Env, investor: Address) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::InvestorClaimNotBefore(investor))
            .unwrap_or(0)
    }

    fn set_persistent_investor_claim_not_before(env: &Env, investor: Address, value: u64) {
        env.storage()
            .persistent()
            .set(&DataKey::InvestorClaimNotBefore(investor), &value);
    }

    fn get_persistent_investor_claimed(env: &Env, investor: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::InvestorClaimed(investor))
            .unwrap_or(false)
    }

    fn set_persistent_investor_claimed(env: &Env, investor: Address, value: bool) {
        env.storage()
            .persistent()
            .set(&DataKey::InvestorClaimed(investor), &value);
    }

    /// Public API: contribution recorded for `investor` (persistent storage).
    pub fn get_contribution(env: Env, investor: Address) -> i128 {
        Self::get_persistent_investor_contribution(&env, investor)
    }

    /// Public API: contributions recorded for `investors` in the same order as the input.
    ///
    /// This bounded read batches the same persistent-storage lookup used by
    /// [`LiquifactEscrow::get_contribution`]. Unknown addresses return `0`.
    ///
    /// # Errors
    /// Panics with [`EscrowError::ContributionReadBatchTooLarge`] when `investors.len()`
    /// exceeds [`MAX_INVESTOR_READ_BATCH`].
    pub fn get_contributions(env: Env, investors: Vec<Address>) -> Vec<i128> {
        let len = investors.len();
        ensure(
            &env,
            len <= MAX_INVESTOR_READ_BATCH,
            EscrowError::ContributionReadBatchTooLarge,
        );

        let mut result = Vec::new(&env);
        for i in 0..len {
            let investor = investors.get(i).unwrap();
            result.push_back(Self::get_persistent_investor_contribution(&env, investor));
        }
        result
    }

    /// Returns a paginated list of investor addresses who have contributed to this escrow.
    ///
    /// Legacy instances that predate this feature will return an empty list (backward compatible under ADR-007).
    ///
    /// # Arguments
    /// * `start` - The starting index (0-based) of the pagination.
    /// * `limit` - The maximum number of investor addresses to return (capped at [`MAX_INVESTOR_READ_BATCH`]).
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

        let actual_limit = limit.min(MAX_INVESTOR_READ_BATCH);
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

    /// Effective yield (bps) for this investor after their **first** deposit; later [`LiquifactEscrow::fund`]
    /// calls add principal at this rate. Defaults to [`InvoiceEscrow::yield_bps`] when unset (legacy positions).
    ///
    /// Note: reads `DataKey::Escrow` for the base yield fallback; callers that already hold the
    /// escrow should prefer reading `DataKey::InvestorEffectiveYield` directly.
    pub fn get_investor_yield_bps(env: Env, investor: Address) -> i64 {
        // env.clone(): env is used again after this call for the InvestorEffectiveYield read.
        let escrow = Self::get_escrow(env.clone());
        Self::get_persistent_investor_effective_yield(&env, investor.clone())
            .unwrap_or(escrow.yield_bps)
    }

    /// Earliest ledger timestamp for [`LiquifactEscrow::claim_investor_payout`]; `0` if not gated.
    pub fn get_investor_claim_not_before(env: Env, investor: Address) -> u64 {
        Self::get_persistent_investor_claim_not_before(&env, investor)
    }
    /// Returns the yield-tier table configured at `init`.
    /// Returns an empty `Vec` when no tiers were configured.
    /// Order matches the validated non-decreasing ordering enforced at `init`.
    /// Pure read — no auth required, no state mutation.
    pub fn get_yield_tiers(env: Env) -> Vec<YieldTier> {
        env.storage()
            .instance()
            .get::<DataKey, Vec<YieldTier>>(&DataKey::YieldTierTable)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Pure read — no auth, no storage writes, safe for simulation.
    ///
    /// Returns `(effective_yield_bps, matched_lock_secs)` for a hypothetical contribution of
    /// `amount` with `lock` seconds, using the **exact same tier-selection rule** applied at
    /// the first [`LiquifactEscrow::fund_with_commitment`] deposit.
    ///
    /// The `amount` parameter is accepted to mirror the `fund_with_commitment` signature and
    /// enable future amount-based tier selection; it is not used in the current lock-only
    /// tier-selection rule.
    ///
    /// # Resolution
    ///
    /// - If no [`DataKey::YieldTierTable`] is configured, or `lock == 0`, returns the escrow base
    ///   `yield_bps` with `matched_lock_secs = 0` (the no-tier fallback).
    /// - Otherwise returns the highest-yield tier whose `min_lock_secs <= lock`. If no tier
    ///   qualifies, returns the base yield with `matched_lock_secs = 0`.
    ///
    /// > **Note:** this preview reflects the rule applied at **first deposit only**. A
    /// > follow-on [`LiquifactEscrow::fund`] call does not re-select a tier.
    pub fn preview_yield_tier(env: Env, amount: i128, lock: u64) -> (i64, u64) {
        let _ = amount; // accepted for signature parity with fund_with_commitment; unused in lock-only selection
        let escrow = Self::get_escrow(env.clone());
        Self::effective_yield_for_commitment(&env, escrow.yield_bps, lock)
    }

    /// Retrieve the currently recorded SME collateral commitment metadata from storage.
    /// Returns `None` if no commitment has been recorded yet.
    pub fn get_sme_collateral_commitment(env: Env) -> Option<SmeCollateralCommitment> {
        env.storage().instance().get(&DataKey::SmeCollateralPledge)
    }

    /// Retire the recorded SME collateral pledge.
    ///
    /// Metadata-only: no tokens are moved. Requires SME auth.
    ///
    /// Guard ordering (ADR-002):
    /// 1. Read-only existence check — returns [`EscrowError::NoCollateralToClear`] if absent.
    /// 2. `require_auth` on the SME address (via `load_escrow_require_sme`).
    /// 3. Remove storage entry and emit [`CollateralClearedEvt`].
    pub fn clear_sme_collateral_commitment(env: Env) {
        let commitment: SmeCollateralCommitment = env
            .storage()
            .instance()
            .get(&DataKey::SmeCollateralPledge)
            .unwrap_or_else(|| fail(&env, EscrowError::NoCollateralToClear));

        let escrow = Self::load_escrow_require_sme(&env);

        env.storage()
            .instance()
            .remove(&DataKey::SmeCollateralPledge);

        CollateralClearedEvt {
            name: symbol_short!("coll_clr"),
            invoice_id: escrow.invoice_id.clone(),
            asset: commitment.asset.clone(),
            amount: commitment.amount,
            recorded_at: commitment.recorded_at,
        }
        .publish(&env);

        CollateralCommitmentCleared {
            name: symbol_short!("coll_clr"),
            invoice_id: escrow.invoice_id.clone(),
            asset: commitment.asset.clone(),
            amount: commitment.amount,
            recorded_at: commitment.recorded_at,
        }
        .publish(&env);
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

    /// Atomically revoke multiple attestation-digest indices in a single call.
    ///
    /// Each index is validated identically to the single-index
    /// [`LiquifactEscrow::revoke_attestation_digest`].
    ///
    /// # Authorization
    /// Requires `InvoiceEscrow::admin` auth.
    ///
    /// # Batch bounds
    /// - `indices` must be non-empty (panics with [`EscrowError::AttestationBatchEmpty`]).
    /// - `indices.len()` must not exceed [`MAX_ATTESTATION_REVOKE_BATCH`] (panics with
    ///   [`EscrowError::AttestationBatchTooLarge`]).
    ///
    /// # Per-index validation (in order)
    /// - [`EscrowError::AttestationIndexOutOfRange`] if `index >= log.len()`.
    /// - [`EscrowError::AttestationAlreadyRevoked`] if the entry at `index` is already revoked.
    ///
    /// # Atomicity
    /// If **any** per-index validation fails, the entire batch is rolled back (no partial
    /// revocation). Duplicate indices in the batch are **not** pre-deduplicated — the second
    /// occurrence will fail with [`EscrowError::AttestationAlreadyRevoked`].
    ///
    /// # Events
    /// One [`AttestationDigestRevoked`] event per newly revoked index, preserving the same event
    /// shape as the single-index entrypoint.
    pub fn revoke_attestation_digests(env: Env, indices: Vec<u32>) {
        let n = indices.len();

        ensure(&env, n > 0, EscrowError::AttestationBatchEmpty);
        ensure(
            &env,
            n <= MAX_ATTESTATION_REVOKE_BATCH,
            EscrowError::AttestationBatchTooLarge,
        );

        let escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();

        let log: Vec<BytesN<32>> = env
            .storage()
            .instance()
            .get(&DataKey::AttestationAppendLog)
            .unwrap_or_else(|| Vec::new(&env));

        for i in 0..n {
            let index = indices.get(i).unwrap();

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

    pub fn get_revoked_attestation_digests(
        env: Env,
        start: u32,
        limit: u32,
    ) -> Vec<AttestationDigestInfo> {
        let log = Self::get_attestation_append_log(env.clone());
        let len = log.len();
        if start >= len || limit == 0 {
            return Vec::new(&env);
        }

        let actual_limit = limit.min(MAX_ATTESTATION_READ_PAGE);
        let mut result = Vec::new(&env);
        let mut i = start;
        while i < len && result.len() < actual_limit {
            if Self::is_attestation_revoked(env.clone(), i) {
                let digest = log.get(i).unwrap();
                result.push_back(AttestationDigestInfo {
                    digest,
                    revoked: true,
                });
            }
            i += 1;
        }
        result
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
        ensure(
            &env,
            index < log.len(),
            EscrowError::AttestationIndexOutOfRange,
        );
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

    fn settleable_now(env: &Env) -> bool {
        if Self::legal_hold_active(env) {
            return false;
        }
        let escrow = Self::get_escrow(env.clone());
        if escrow.status != 1 {
            return false;
        }
        if escrow.maturity > 0 && env.ledger().timestamp() < escrow.maturity {
            return false;
        }
        true
    }

    /// Returns `true` when [`LiquifactEscrow::settle`] would succeed for the current ledger state.
    ///
    /// Settlement requires:
    /// - escrow funded
    /// - maturity reached
    /// - no active legal hold
    pub fn is_settleable(env: Env) -> bool {
        Self::settleable_now(&env)
    }

    /// Bundle the settleable flag, legal-hold state, maturity-reached state, and a single derived
    /// `ready_now` boolean into one [`SettlementReadiness`] result.
    ///
    /// Integrators otherwise have to call [`LiquifactEscrow::is_settleable`],
    /// [`LiquifactEscrow::get_legal_hold`], [`LiquifactEscrow::has_maturity_lock`], and read the
    /// maturity timestamp separately, then replicate the contract's precedence rules — which drifts
    /// out of sync and produces confusing UIs ("settleable" but blocked by a legal hold).
    ///
    /// # Precedence
    /// `ready_now` and `is_settleable` are computed from the **same** single-source-of-truth gate
    /// (`Self::settleable_now`) that [`LiquifactEscrow::settle`] and
    /// [`LiquifactEscrow::partial_settle`] apply: a legal hold blocks first, then funded status,
    /// then maturity. A `ready_now == true` value therefore reliably predicts a successful `settle`
    /// on the current ledger.
    ///
    /// # Read-only
    /// Pure view: no `require_auth`, no storage writes, and no TTL bump.
    pub fn get_settlement_readiness(env: Env) -> SettlementReadiness {
        let legal_hold_active = Self::legal_hold_active(&env);
        let escrow = Self::get_escrow(env.clone());
        let maturity_reached = escrow.maturity == 0 || env.ledger().timestamp() >= escrow.maturity;

        // Reuse the single-source-of-truth gate so this view cannot drift from `settle`.
        let is_settleable = Self::settleable_now(&env);

        SettlementReadiness {
            is_settleable,
            legal_hold_active,
            maturity_reached,
            ready_now: is_settleable,
        }
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

    /// Set or clear the lightweight **operational pause**. Only the **current**
    /// [`InvoiceEscrow::admin`] may call.
    ///
    /// This is an incident-response circuit breaker (e.g. a suspected token bug) that is
    /// **orthogonal to the compliance legal hold**: it carries no compliance semantics and,
    /// unlike [`LiquifactEscrow::set_legal_hold`], has **no** two-phase clear delay — a single
    /// authorized call toggles it on or off. While active it blocks [`LiquifactEscrow::fund`],
    /// [`LiquifactEscrow::settle`], [`LiquifactEscrow::withdraw`], and
    /// [`LiquifactEscrow::claim_investor_payout`]. Legal-hold state is neither read nor written.
    ///
    /// Emits [`PausedChanged`].
    pub fn set_paused(env: Env, active: bool) {
        let escrow = Self::load_escrow_require_admin(&env);

        env.storage().instance().set(&DataKey::Paused, &active);

        PausedChanged {
            name: symbol_short!("paused"),
            invoice_id: escrow.invoice_id.clone(),
            active: if active { 1 } else { 0 },
        }
        .publish(&env);
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

    /// Enable or disable the investor allowlist. When enabled, only addresses with
    /// [`DataKey::InvestorAllowlisted`] set to true may fund the escrow.
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

    pub fn is_allowlist_active(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::AllowlistActive)
            .unwrap_or(false)
    }

    /// Add or remove an investor from the allowlist.
    pub fn set_investor_allowlisted(env: Env, investor: Address, allowed: bool) {
        let escrow = Self::load_escrow_require_admin(&env);

        let was_allowlisted: bool = env
            .storage()
            .persistent()
            .get(&DataKey::InvestorAllowlisted(investor.clone()))
            .unwrap_or(false);

        env.storage()
            .persistent()
            .set(&DataKey::InvestorAllowlisted(investor.clone()), &allowed);

        // Maintain the allowlist index
        let mut index: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AllowlistIndex)
            .unwrap_or_else(|| Vec::new(&env));

        if allowed && !was_allowlisted {
            index.push_back(investor.clone());
        } else if !allowed && was_allowlisted {
            // Remove from index by position
            for i in 0..index.len() {
                if index.get(i).unwrap() == investor {
                    index.remove(i);
                    break;
                }
            }
        }

        env.storage()
            .instance()
            .set(&DataKey::AllowlistIndex, &index);

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
    /// once. The call is rejected for empty vectors or vectors longer than
    /// `MAX_INVESTOR_ALLOWLIST_BATCH` to keep storage and CPU bounded.
    ///
    /// Invariant: the end state and emitted events are identical to calling
    /// `set_investor_allowlisted` individually for each element in `investors`.
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes when the escrow is uninitialized, the batch is empty, or
    /// the batch exceeds [`MAX_INVESTOR_ALLOWLIST_BATCH`].
    pub fn set_investors_allowlisted(env: Env, investors: Vec<Address>, allowed: bool) {
        let escrow = Self::load_escrow_require_admin(&env);

        let n = investors.len();
        ensure(&env, n > 0, EscrowError::InvestorBatchEmpty);
        ensure(
            &env,
            n <= MAX_INVESTOR_ALLOWLIST_BATCH,
            EscrowError::InvestorBatchTooLarge,
        );

        // Load index once for the entire batch
        let mut index: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AllowlistIndex)
            .unwrap_or_else(|| Vec::new(&env));

        for i in 0..n {
            let inv = investors.get(i).unwrap();

            let was_allowlisted: bool = env
                .storage()
                .persistent()
                .get(&DataKey::InvestorAllowlisted(inv.clone()))
                .unwrap_or(false);

            env.storage()
                .persistent()
                .set(&DataKey::InvestorAllowlisted(inv.clone()), &allowed);

            if allowed && !was_allowlisted {
                index.push_back(inv.clone());
            } else if !allowed && was_allowlisted {
                for j in 0..index.len() {
                    if index.get(j).unwrap() == inv {
                        index.remove(j);
                        break;
                    }
                }
            }

            InvestorAllowlistChanged {
                name: symbol_short!("al_set"),
                invoice_id: escrow.invoice_id.clone(),
                investor: inv.clone(),
                allowed: if allowed { 1 } else { 0 },
            }
            .publish(&env);
        }

        InvestorAllowlistBatchApplied {
            name: symbol_short!("al_batch"),
            invoice_id: escrow.invoice_id.clone(),
            batch_size: n,
            allowed: if allowed { 1 } else { 0 },
        }
        .publish(&env);
    }

    pub fn is_investor_allowlisted(env: Env, investor: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::InvestorAllowlisted(investor))
            .unwrap_or(false)
    }

    /// Returns a paginated list of allowlisted investor addresses.
    ///
    /// Reads the allowlist index and filters by live `InvestorAllowlisted` status
    /// so revoked addresses never appear in the result.
    ///
    /// # Arguments
    /// * `start` - The starting index (0-based) of the pagination.
    /// * `limit` - The maximum number of addresses to return (capped at a hard limit of 50).
    ///
    /// # Returns
    /// A `Vec<Address>` containing the allowlisted addresses within the requested page.
    pub fn get_allowlisted_investors(env: Env, start: u32, limit: u32) -> Vec<Address> {
        let index: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AllowlistIndex)
            .unwrap_or_else(|| Vec::new(&env));

        let len = index.len();
        if start >= len || limit == 0 {
            return Vec::new(&env);
        }

        let actual_limit = limit.min(50);
        let end = (start + actual_limit).min(len);

        let mut result = Vec::new(&env);
        for i in start..end {
            let addr = index.get(i).unwrap();
            // Only include addresses that are still allowlisted
            let is_al: bool = env
                .storage()
                .persistent()
                .get(&DataKey::InvestorAllowlisted(addr.clone()))
                .unwrap_or(false);
            if is_al {
                result.push_back(addr);
            }
        }
        result
    }

    /// Returns the total number of currently-allowlisted addresses.
    ///
    /// Reads the allowlist index and counts entries where the live
    /// `InvestorAllowlisted` flag is still `true`.
    pub fn get_allowlisted_investors_count(env: Env) -> u32 {
        let index: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AllowlistIndex)
            .unwrap_or_else(|| Vec::new(&env));

        let mut count: u32 = 0;
        for i in 0..index.len() {
            let addr = index.get(i).unwrap();
            let is_al: bool = env
                .storage()
                .persistent()
                .get(&DataKey::InvestorAllowlisted(addr.clone()))
                .unwrap_or(false);
            if is_al {
                count += 1;
            }
        }
        count
    }

    /// Convenience alias for [`LiquifactEscrow::set_legal_hold`] with `active = false`.
    pub fn clear_legal_hold(env: Env) {
        Self::set_legal_hold(env, false);
    }

    /// Clear the legal hold after the timelock delay has expired.
    ///
    /// Requires [`DataKey::LegalHoldClearableAt`] to be set and the current
    /// ledger timestamp to be >= that value. This is the timelocked path;
    /// [`LiquifactEscrow::set_legal_hold`] with `active = false` remains
    /// available as an immediate emergency override.
    ///
    /// **Authorization:** [`InvoiceEscrow::admin`].
    ///
    /// # Panics
    /// - If no clear request is pending.
    /// - If the timelock has not yet expired.
    pub fn clear_legal_hold_after_delay(env: Env) {
        let escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();

        ensure(
            &env,
            env.storage().instance().has(&DataKey::LegalHoldClearableAt),
            EscrowError::LegalHoldClearRequestMissing,
        );
        let clearable_at: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LegalHoldClearableAt)
            .unwrap();

        let now = env.ledger().timestamp();
        ensure(
            &env,
            now >= clearable_at,
            EscrowError::LegalHoldClearNotReady,
        );

        env.storage()
            .instance()
            .remove(&DataKey::LegalHoldClearableAt);

        env.storage().instance().set(&DataKey::LegalHold, &false);

        LegalHoldChanged {
            name: symbol_short!("legal_h"),
            invoice_id: escrow.invoice_id,
            active: 0,
        }
        .publish(&env);
    }
    /// Cancel a pending legal-hold clear request.
    ///
    /// Removes [`DataKey::LegalHoldClearableAt`], aborting the timelock. The hold
    /// stays active. A fresh [`LiquifactEscrow::request_clear_legal_hold`] restarts
    /// the full delay.
    ///
    /// **Authorization:** [`InvoiceEscrow::admin`].
    ///
    /// # Panics
    /// If no clear request is pending.
    pub fn cancel_clear_legal_hold(env: Env) {
        let escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();

        ensure(
            &env,
            env.storage().instance().has(&DataKey::LegalHoldClearableAt),
            EscrowError::LegalHoldClearRequestMissing,
        );

        env.storage()
            .instance()
            .remove(&DataKey::LegalHoldClearableAt);

        LegalHoldClearCancelled {
            name: symbol_short!("lh_cancel"),
            invoice_id: escrow.invoice_id.clone(),
        }
        .publish(&env);
    }

    pub fn update_funding_target(env: Env, new_target: i128) -> InvoiceEscrow {
        let mut escrow = Self::load_escrow_require_admin(&env);

        ensure(&env, new_target > 0, EscrowError::TargetNotPositive);
        guard_status_eq(&env, escrow.status, 0, EscrowError::TargetUpdateNotOpen);
        ensure(
            &env,
            new_target >= escrow.funded_amount,
            EscrowError::TargetBelowFundedAmount,
        );

        let old_target = escrow.funding_target;
        escrow.funding_target = new_target;

        // If lowering the target causes it to equal (or fall to) the already-funded
        // amount, promote the escrow to funded and capture the immutable close snapshot
        // exactly once — mirroring the promotion logic in `fund`/`fund_with_commitment`.
        if escrow.funded_amount > 0
            && escrow.funded_amount >= new_target
            && !env.storage().instance().has(&DataKey::FundingCloseSnapshot)
        {
            escrow.status = 1;
            env.storage().instance().set(
                &DataKey::FundingCloseSnapshot,
                &FundingCloseSnapshot {
                    total_principal: escrow.funded_amount,
                    funding_target: new_target,
                    closed_at_ledger_timestamp: env.ledger().timestamp(),
                    closed_at_ledger_sequence: env.ledger().sequence(),
                },
            );
        }

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

        guard_status_eq(&env, escrow.status, 0, EscrowError::CapLowerNotOpen);

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

    /// Raise the maximum unique investor cap while the escrow is still open.
    ///
    /// This is an admin-only counterpart to `lower_max_unique_investors`.
    /// The new cap must be strictly higher than the current cap.
    ///
    /// # Panics
    /// - If the escrow is not open.
    /// - If no unique-investor cap was configured at initialization.
    /// - If `new_cap` is not strictly higher than the current cap.
    pub fn raise_max_unique_investors(env: Env, new_cap: u32) -> u32 {
        let escrow = Self::load_escrow_require_admin(&env);

        require_funding_open(&env, escrow.status);

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

        ensure(&env, new_cap > old_cap, EscrowError::NewCapNotHigher);

        env.storage()
            .instance()
            .set(&DataKey::MaxUniqueInvestorsCap, &new_cap);

        MaxUniqueInvestorsCapRaised {
            name: symbol_short!("raise_cap"),
            invoice_id: escrow.invoice_id.clone(),
            old_cap,
            new_cap,
        }
        .publish(&env);

        new_cap
    }

    /// Lower the minimum contribution floor while the escrow is still open.
    ///
    /// This is admin-only and intentionally cannot raise the floor or set a non-positive
    /// value. The new floor applies to all subsequent [`LiquifactEscrow::fund`] /
    /// [`LiquifactEscrow::fund_with_commitment`] calls, including follow-on deposits from
    /// existing investors.
    ///
    /// # Panics
    /// - If the escrow is not open (status != 0).
    /// - If `new_floor` is not strictly lower than the current floor.
    /// - If `new_floor` is not positive.
    pub fn lower_min_contribution_floor(env: Env, new_floor: i128) -> i128 {
        let escrow = Self::load_escrow_require_admin(&env);

        guard_status_eq(&env, escrow.status, 0, EscrowError::FloorLowerNotOpen);
        ensure(&env, new_floor > 0, EscrowError::NewFloorNotPositive);

        let old_floor: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinContributionFloor)
            .unwrap_or(0);
        ensure(&env, new_floor < old_floor, EscrowError::NewFloorNotLower);

        env.storage()
            .instance()
            .set(&DataKey::MinContributionFloor, &new_floor);

        MinContributionFloorLowered {
            name: symbol_short!("floor_lo"),
            invoice_id: escrow.invoice_id.clone(),
            old_floor,
            new_floor,
        }
        .publish(&env);

        new_floor
    }

    /// Raises the per-investor contribution cap.
    ///
    /// # Requirements
    /// - Caller must be the admin.
    /// - Escrow must be in Open state (status == 0).
    /// - A per-investor cap must already be configured.
    /// - `new_cap` must be strictly greater than the current cap.
    ///
    /// # Arguments
    /// * `env` — The Soroban environment.
    /// * `new_cap` — The new per-investor cap, must be > current cap.
    ///
    /// # Returns
    /// The new cap value on success.
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes:
    /// - [`EscrowError::Unauthorized`] if caller is not admin (via `load_escrow_require_admin`).
    /// - [`EscrowError::CapLowerNotOpen`] if escrow is not in Open state.
    /// - [`EscrowError::MaxPerInvestorCapNotConfigured`] if no cap was set at init.
    /// - [`EscrowError::MaxPerInvestorCapNotRaised`] if `new_cap <= current_cap`.
    pub fn raise_max_per_investor(env: Env, new_cap: i128) -> i128 {
        let escrow = Self::load_escrow_require_admin(&env);

        guard_status_eq(&env, escrow.status, 0, EscrowError::CapLowerNotOpen);

        let old_cap: Option<i128> = env.storage().instance().get(&DataKey::MaxPerInvestorCap);
        ensure(
            &env,
            old_cap.is_some(),
            EscrowError::MaxPerInvestorCapNotConfigured,
        );
        let old_cap = old_cap.unwrap();

        ensure(
            &env,
            new_cap > old_cap,
            EscrowError::MaxPerInvestorCapNotRaised,
        );

        env.storage()
            .instance()
            .set(&DataKey::MaxPerInvestorCap, &new_cap);

        MaxPerInvestorCapRaised {
            name: symbol_short!("inv_cap"),
            invoice_id: escrow.invoice_id,
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

    /// Replaces the deployed WASM bytecode for this contract instance while preserving all
    /// stored state (instance, persistent, and temporary storage tiers are all unchanged).
    ///
    /// This is the **in-place WASM upgrade** path. The contract address, contract ID,
    /// and all stored ledger entries are preserved. Only the executable code is swapped.
    ///
    /// ## Division of labor: `upgrade` vs `migrate`
    ///
    /// | Concern | Function | Notes |
    /// |---------|----------|-------|
    /// | Replace running WASM code | `upgrade(new_wasm_hash)` | Admin-gated; preserves all storage |
    /// | Validate + rewrite stored structs | `migrate(from_version)` | Admin-gated; currently errors on all paths |
    /// | Additive new `DataKey` | Neither (no call needed) | Old instances default missing keys |
    /// | Breaking struct/key change | Redeploy | In-place migration only if `migrate` is extended |
    ///
    /// ## Authorization
    ///
    /// Requires [`InvoiceEscrow::admin`] authorization (`admin.require_auth()`) before any
    /// deployer interaction. This is enforced via [`Self::load_escrow_require_admin`], which
    /// reads `DataKey::Escrow` and calls `require_auth()` on `escrow.admin`. Unauthenticated
    /// callers cause the Soroban transaction to revert before the WASM is touched.
    ///
    /// ## State preservation guarantee
    ///
    /// After a successful `upgrade` call:
    /// - **Instance storage**: all keys (including `DataKey::Escrow`, `DataKey::Version`,
    ///   `DataKey::FundingToken`, `DataKey::LegalHold`, etc.) are unchanged.
    /// - **Persistent storage**: all per-investor keys (`DataKey::InvestorContribution(addr)`,
    ///   `DataKey::InvestorEffectiveYield(addr)`, `DataKey::InvestorClaimNotBefore(addr)`,
    ///   `DataKey::InvestorClaimed(addr)`, `DataKey::InvestorAllowlisted(addr)`) are unchanged.
    /// - **SCHEMA_VERSION** (compile-time constant in new WASM) is updated, but
    ///   `DataKey::Version` (on-chain stored value) is **not** changed by this call.
    ///   A mismatch between them after upgrade is the signal that `migrate()` may be needed.
    /// - **Token balances** are not transferred. The escrow's custody balance is unaffected.
    ///
    /// ## Additive-key safety contract (ADR-007, Rule 1)
    ///
    /// A WASM upgrade is safe when the new WASM only **adds** new `DataKey` variants that:
    /// 1. Are read with `.get(...).unwrap_or(default)` so pre-existing instances return
    ///    the expected default when the key is absent.
    /// 2. Do not change the XDR shape of any existing stored `#[contracttype]` struct
    ///    (e.g. `InvoiceEscrow`, `FundingCloseSnapshot`, `YieldTier`, `SmeCollateralCommitment`).
    /// 3. Do not rename or remove any existing `DataKey` variant.
    ///
    /// **Critically: `DataKey` variant ordering in the enum determines the XDR discriminant
    /// (encoded as an integer). Reordering existing variants changes their on-chain discriminant,
    /// causing reads of those keys to silently decode the wrong storage slot or return nothing.
    /// Never reorder existing `DataKey` variants; only append new ones at the end of the enum.**
    ///
    /// A WASM upgrade is **unsafe / breaking** when:
    /// - An existing `DataKey` variant is renamed, removed, or reordered.
    /// - An existing stored `#[contracttype]` struct gains a non-optional field.
    /// - An existing stored `#[contracttype]` struct changes a field type.
    /// - The XDR discriminant of any existing variant changes (caused by reordering).
    ///
    /// These breaking changes require either a `migrate` path (extend `migrate` first,
    /// then upgrade, then call `migrate`) or a full redeploy. See `docs/OPERATOR_RUNBOOK.md` §1
    /// and `docs/adr/ADR-007-storage-key-evolution.md` for the decision tree.
    ///
    /// ## Event emission (before deployer call)
    ///
    /// A [`ContractUpgraded`] event is emitted *before* the deployer call as a defensive
    /// ordering: the event is recorded even if the deployer interaction somehow reverts.
    /// The event carries `invoice_id` (for indexer correlation) and `new_wasm_hash`.
    ///
    /// ## When to call `migrate` after upgrading
    ///
    /// - **Additive-only new `DataKey` variants**: do **not** call `migrate()`. Old instances
    ///   return defaults for absent keys; no rewrite is needed.
    /// - **Schema-breaking changes where `migrate()` has been extended**: call `migrate(stored_version)`
    ///   after the upgrade. The stored version before upgrade is readable via `get_version()`.
    /// - **Current release (SCHEMA_VERSION = 6)**: `migrate()` errors on all paths.
    ///   Do not call it as a bookkeeping step after an additive upgrade.
    ///
    /// ## Operator pre-flight checklist
    ///
    /// Before invoking `upgrade` on a live instance, operators must:
    /// 1. Activate a legal hold (`set_legal_hold(true)`) to block in-flight settlements/claims.
    /// 2. Build and upload the new WASM: `cargo build --target wasm32v1-none --release`.
    /// 3. Upload to the network: `stellar contract upload --wasm ...` → captures `NEW_WASM_HASH`.
    /// 4. Diff the new `DataKey` enum against the deployed version: verify only additive changes.
    /// 5. Test on Testnet with a mirror instance before Mainnet.
    /// 6. Call `upgrade(NEW_WASM_HASH)` with admin credentials.
    /// 7. Verify `get_version()` and `get_escrow()` return expected values.
    /// 8. Clear legal hold: `clear_legal_hold()`.
    /// See `docs/OPERATOR_RUNBOOK.md` §§3–7 for the complete procedure.
    ///
    /// ## Rollback
    ///
    /// Re-upload the previous WASM (already recorded on-chain) and call `upgrade(PREV_WASM_HASH)`.
    /// This works only when stored data is still compatible with old WASM types. If stored data
    /// was already rewritten by a `migrate` call, rollback requires a redeploy.
    ///
    /// ## Risks
    ///
    /// Deploying an incompatible WASM (one that reorders or removes existing `DataKey` variants,
    /// or changes a stored struct's XDR shape) will silently corrupt stored state on the next read.
    /// There is no on-chain undo once `update_current_contract_wasm` completes. Test thoroughly
    /// on Testnet before upgrading production contracts.
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

    /// Record investor deposit: transfer tokens from investor to escrow.
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes for invalid status, authorization, amount, caps,
    /// allowance, or insufficient balance.
    pub fn fund(env: Env, investor: Address, amount: i128) -> InvoiceEscrow {
        Self::fund_impl(env, investor, amount, true, 0)
    }

    /// First deposit only (per investor): optional longer lock and tier ladder from [`DataKey::YieldTierTable`].
    /// Sets [`DataKey::InvestorClaimNotBefore`] when `committed_lock_secs > 0`. Additional principal
    /// from the same investor must use [`LiquifactEscrow::fund`].
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes for the same funding guards as [`LiquifactEscrow::fund`],
    /// plus tiered follow-on deposit misuse and claim-lock timestamp overflow.
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

        // ── Atomicity guarantee (issue #557) ──────────────────────────────────
        // Validate the per-entry positivity and min-contribution-floor invariants for
        // EVERY entry up front, before any `fund_impl` call performs a storage write
        // or counter increment. A single malformed entry (zero/negative amount, or an
        // amount below the configured floor) at any position must fail the entire call
        // atomically, leaving contributions, the unique-funder count, and the funded
        // total unchanged. These are the same typed errors `fund_impl` raises per entry
        // (`FundingAmountNotPositive`, `FundingBelowMinContribution`); checking them here
        // first turns a half-applied batch into an all-or-nothing rejection.
        //
        // Stateful per-entry guards (per-investor cap, unique-investor cap, overflow)
        // remain enforced inside `fund_impl` against the running accumulated state.
        let floor: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinContributionFloor)
            .unwrap_or(0);
        for i in 0..n {
            let (_, amount) = entries.get(i).unwrap();
            ensure(&env, amount > 0, EscrowError::FundingAmountNotPositive);
            if floor > 0 {
                ensure(
                    &env,
                    amount >= floor,
                    EscrowError::FundingBelowMinContribution,
                );
            }
        }

        // ── Duplicate-address guard (issue #643) ──────────────────────────────
        // Reject the entire batch atomically if any two entries share an investor address.
        // Each investor must appear at most once per call; duplicates suggest a malformed
        // batch and could incorrectly accumulate principal or consume unique-investor slots.
        //
        // Algorithm: O(n²) pairwise comparison, bounded by MAX_FUND_BATCH = 50 (≤ 2 500
        // iterations). No heap allocation required; `soroban_sdk` does not expose a set
        // type, so we do an explicit nested scan over the already-validated entries.
        for i in 0..n {
            let (addr_i, _) = entries.get(i).unwrap();
            for j in (i + 1)..n {
                let (addr_j, _) = entries.get(j).unwrap();
                ensure(
                    &env,
                    addr_i != addr_j,
                    EscrowError::FundingBatchDuplicateInvestor,
                );
            }
        }

        let mut escrow = Self::get_escrow(env.clone());

        for i in 0..n {
            let (investor, amount) = entries.get(i).unwrap();

            // Each entry is now known to satisfy positivity and the floor; remaining
            // per-entry invariants (auth, caps, overflow) are enforced inside fund_impl.
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
        investor.require_auth();

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

        // env.clone(): env is used again after this call for storage writes and publish.
        let mut escrow = Self::get_escrow(env.clone());
        // Operational pause gate (read-only), independent of the compliance legal hold below.
        ensure(
            &env,
            !Self::paused_active(&env),
            EscrowError::PausedBlocksFunding,
        );
        // Legal hold check is intentionally after the escrow read: the escrow is needed for
        // status and yield_bps regardless, and hoisting the hold check before the escrow read
        // would not reduce storage operations (both keys are always read on this path).
        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksFunding);
        require_funding_open(&env, escrow.status);

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

        // Hoist UniqueFunderCount read: used for both the cap assertion (below) and the
        // increment write (after contribution is recorded). A single read covers both uses,
        // eliminating one storage read on every new-investor funding call.
        let cur_funder_count: u32 = if prev == 0 {
            env.storage()
                .instance()
                .get(&DataKey::UniqueFunderCount)
                .unwrap_or(0)
        } else {
            0 // prev != 0: count is not needed; skip the read entirely.
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

        // Capture the effective yield and tier lock threshold in locals so event fields can
        // be populated without post-write storage reads.
        let (investor_effective_yield_bps, tier_lock_secs) = if simple_fund {
            // Non-tiered deposits never carry a commitment lock.
            if prev == 0 {
                Self::set_persistent_investor_effective_yield(
                    &env,
                    investor.clone(),
                    escrow.yield_bps,
                );
                Self::set_persistent_investor_claim_not_before(&env, investor.clone(), 0u64);
                (escrow.yield_bps, 0u64)
            } else {
                // Returning investor: yield was set on first deposit; read it for the event.
                // If prev > 0, preserve existing effective yield and claim lock.
                // Read stored yield for the event (falls back to escrow default for new investors).
                (
                    Self::get_persistent_investor_effective_yield(&env, investor.clone())
                        .unwrap_or(escrow.yield_bps),
                    0u64,
                )
            }
        } else {
            ensure(&env, prev == 0, EscrowError::TieredSecondDeposit);
            let (eff, lock) =
                Self::effective_yield_for_commitment(&env, escrow.yield_bps, committed_lock_secs);
            Self::set_persistent_investor_effective_yield(&env, investor.clone(), eff);
            let now = env.ledger().timestamp();
            let claim_nb = if committed_lock_secs == 0 {
                0u64
            } else {
                now.checked_add(committed_lock_secs)
                    .unwrap_or_else(|| fail(&env, EscrowError::InvestorClaimTimeOverflow))
            };
            // Bound: reject if the claim lock would expire after the escrow maturity.
            // Only constrained when both committed_lock_secs > 0 and maturity > 0.
            if claim_nb > 0 && escrow.maturity > 0 {
                ensure(
                    &env,
                    claim_nb <= escrow.maturity,
                    EscrowError::CommitmentLockExceedsMaturity,
                );
            }
            Self::set_persistent_investor_claim_not_before(&env, investor.clone(), claim_nb);
            (eff, lock)
        };

        escrow.funded_amount = escrow
            .funded_amount
            .checked_add(amount)
            .unwrap_or_else(|| fail(&env, EscrowError::FundedAmountOverflow));

        if escrow.status == 0 && escrow.funded_amount >= escrow.funding_target {
            escrow.status = 1;
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
        }

        Self::set_persistent_investor_contribution(&env, investor.clone(), new_contribution);

        if prev == 0 {
            env.storage()
                .instance()
                .set(&DataKey::UniqueFunderCount, &(cur_funder_count + 1));

            let mut index: Vec<Address> = env
                .storage()
                .instance()
                .get(&DataKey::InvestorIndex)
                .unwrap_or_else(|| Vec::new(&env));
            index.push_back(investor.clone());
            env.storage()
                .instance()
                .set(&DataKey::InvestorIndex, &index);
        }

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        // 4. Token transfer
        let token_addr = env
            .storage()
            .instance()
            .get(&DataKey::FundingToken)
            .unwrap_or_else(|| fail(&env, EscrowError::FundingTokenNotSet));
        let this = env.current_contract_address();

        #[cfg(any(test, feature = "testutils"))]
        register_mock_token_if_needed(&env, &token_addr);

        external_calls::transfer_into_escrow_with_balance_checks(
            &env,
            &token_addr,
            &investor,
            &this,
            amount,
        );

        EscrowFunded {
            name: symbol_short!("funded"),
            invoice_id: escrow.invoice_id.clone(),
            investor: investor.clone(),
            amount,
            funded_amount: escrow.funded_amount,
            status: escrow.status,
            // Locals set at write time; no post-write storage reads required.
            investor_effective_yield_bps,
            tier_lock_secs,
        }
        .publish(&env);

        escrow
    }

    /// Closes funding early for an under-funded invoice, transitioning the escrow to a settleable state.
    ///
    /// # Authorization
    /// The configured **SME** address must authorize this call.
    ///
    /// Blocked while [`DataKey::LegalHold`] is active.
    /// Closes funding early for an under-funded invoice, transitioning the escrow to a settleable state.
    ///
    /// # Authorization
    /// The configured **SME** or **Admin** address must authorize this call.
    ///
    /// Blocked while [`DataKey::LegalHold`] is active.
    pub fn partial_settle(env: Env, caller: Address) -> InvoiceEscrow {
        caller.require_auth();

        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksPartialSettle);

        let mut escrow = Self::get_escrow(env.clone());

        ensure(
            &env,
            caller == escrow.sme_address || caller == escrow.admin,
            EscrowError::PartialSettleUnauthorizedCaller,
        );

        guard_status_eq(&env, escrow.status, 0, EscrowError::PartialSettleNotOpen);

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
        // Operational pause gate (read-only), before require_auth and orthogonal to legal hold.
        ensure(
            &env,
            !Self::paused_active(&env),
            EscrowError::PausedBlocksSettlement,
        );
        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksSettlement);

        // env.clone(): env is used again after this call for ledger timestamp, storage set, and publish.
        let mut escrow = Self::load_escrow_require_sme(&env);

        ensure(&env, escrow.status == 1, EscrowError::SettlementNotFunded);

        let now = env.ledger().timestamp();
        if escrow.maturity > 0 {
            ensure(
                &env,
                now >= escrow.maturity,
                EscrowError::MaturityNotReached,
            );
        }

        // Compute settle_pool using the same arithmetic as compute_investor_payout.
        // coupon = funded_amount × yield_bps / 10_000  (floor)
        // settle_pool = funded_amount + coupon
        let coupon = escrow
            .funded_amount
            .checked_mul(escrow.yield_bps as i128)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow))
            .checked_div(10_000)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow));

        let settle_pool = escrow
            .funded_amount
            .checked_add(coupon)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow));

        escrow.status = 2;

        env.storage().instance().set(&DataKey::SettledAt, &now);
        env.storage().instance().set(&DataKey::Escrow, &escrow);

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

    /// SME pulls funded liquidity, net of the immutable protocol fee.
    ///
    /// Splits `funded_amount` of the bound funding token into a treasury **fee** and an SME
    /// **net payout**, then transitions status to 3 (withdrawn). Blocked when a legal hold or
    /// operational pause is active.
    ///
    /// # Fee split
    /// ```text
    /// fee_bps    = DataKey::ProtocolFeeBps   (0..=10_000, default 0)
    /// fee        = funded_amount * fee_bps / 10_000   (floor, checked)
    /// sme_payout = funded_amount - fee                 (checked)
    /// ```
    /// `fee` is sent to [`DataKey::Treasury`] (only when `> 0`) and `sme_payout` to
    /// [`InvoiceEscrow::sme_address`]. **Conservation:** `sme_payout + fee == funded_amount`.
    /// Floor rounding means any residue below one `10_000`-th stays with the SME. With
    /// `fee_bps == 0` no treasury transfer is made and the SME receives the full `funded_amount`.
    ///
    /// # Guard ordering
    ///
    /// 1. Operational pause + legal-hold gates (read-only).
    /// 2. `sme_address.require_auth()` (via `load_escrow_require_sme`).
    /// 3. Status == 1 (funded) check.
    /// 4. Contract balance sufficiency check ([`EscrowError::InsufficientContractBalance`]).
    /// 5. Checked fee/net computation.
    /// 6. Status transition to 3, `DistributedPrincipal` update (by the full gross
    ///    `funded_amount`), storage write.
    /// 7. SEP-41 token transfers (fee → treasury, net → SME) with balance-delta verification.
    /// 8. Event emission ([`SmeWithdrew`], carrying `amount = sme_payout` and `fee`).
    ///
    /// # Errors
    /// - [`EscrowError::LegalHoldBlocksWithdrawal`] — hold is active.
    /// - [`EscrowError::WithdrawalNotFunded`] — escrow not in funded state.
    /// - [`EscrowError::InsufficientContractBalance`] — contract holds less than `funded_amount`.
    /// - [`EscrowError::WithdrawFeeArithmeticOverflow`] — `funded_amount * fee_bps` overflowed `i128`.
    /// - [`EscrowError::WithdrawNetArithmeticUnderflow`] — `funded_amount - fee` underflowed (unreachable for in-range `fee_bps`).
    pub fn withdraw(env: Env) -> InvoiceEscrow {
        // Operational pause gate (read-only), before require_auth and orthogonal to legal hold.
        ensure(
            &env,
            !Self::paused_active(&env),
            EscrowError::PausedBlocksWithdrawal,
        );
        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksWithdrawal);

        let mut escrow = Self::load_escrow_require_sme(&env);

        guard_status_eq(&env, escrow.status, 1, EscrowError::WithdrawalNotFunded);

        let amount = escrow.funded_amount;
        let sme = escrow.sme_address.clone();

        // Immutable protocol fee split. `fee = funded_amount * fee_bps / 10_000` (floor), with the
        // remainder going to the SME. All arithmetic is checked: `funded_amount` may exceed the
        // overflow-safe envelope when an escrow is over-funded, so the multiplication is the only
        // place this can overflow. Conservation `net + fee == funded_amount` holds by construction.
        let fee_bps: i64 = env
            .storage()
            .instance()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0);
        let fee: i128 = amount
            .checked_mul(fee_bps as i128)
            .and_then(|scaled| scaled.checked_div(10_000))
            .unwrap_or_else(|| fail(&env, EscrowError::WithdrawFeeArithmeticOverflow));
        let net: i128 = amount
            .checked_sub(fee)
            .unwrap_or_else(|| fail(&env, EscrowError::WithdrawNetArithmeticUnderflow));

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::FundingToken)
            .unwrap_or_else(|| fail(&env, EscrowError::FundingTokenNotSet));

        // Verify the contract holds enough before mutating state. The check uses the gross
        // `funded_amount` because the contract must fund both the SME payout and the treasury fee.
        let this = env.current_contract_address();
        let contract_balance = TokenClient::new(&env, &token_addr).balance(&this);
        ensure(
            &env,
            contract_balance >= amount,
            EscrowError::InsufficientContractBalance,
        );

        // State transition and accounting (checks-effects-interactions). `DistributedPrincipal`
        // advances by the full gross `funded_amount` (net + fee), keeping the liability accounting
        // consistent regardless of how principal is split.
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

        // Token transfers with SEP-41 balance-delta verification. The treasury transfer is skipped
        // when `fee == 0` so the zero-fee path makes exactly one transfer (preserving legacy
        // behavior and gas profile). `transfer_*` rejects non-positive amounts, so `net` could only
        // be zero in the degenerate `fee_bps == 10_000` case — guard it the same way.
        if fee > 0 {
            let treasury = Self::treasury_or_fail(&env);
            external_calls::transfer_funding_token_with_balance_checks(
                &env,
                &token_addr,
                &this,
                &treasury,
                fee,
            );
        }
        if net > 0 {
            external_calls::transfer_funding_token_with_balance_checks(
                &env,
                &token_addr,
                &this,
                &sme,
                net,
            );
        }

        SmeWithdrew {
            name: symbol_short!("sme_wd"),
            invoice_id: escrow.invoice_id.clone(),
            amount: net,
            recipient: sme,
            fee,
        }
        .publish(&env);

        escrow
    }

    /// Investor records a payout claim after settlement. Idempotent marker per investor.
    ///
    /// # Idempotency
    ///
    /// A second call for the same investor is a silent no-op: the `InvestorClaimed` marker is
    /// written **before** `InvestorPayoutClaimed` is emitted, so re-entrant or replayed calls
    /// return early without re-emitting the event.
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
    /// 7. Storage write + event emit.
    ///
    /// # Claim-lock enforcement
    /// `InvestorClaimNotBefore = deposit_timestamp + committed_lock_secs`.
    /// Enforces `now >= not_before` (inclusive boundary):
    /// - deposit at t=1000, lock=500 -> not_before=1500
    /// - claim at t=1499 -> InvestorCommitmentLockNotExpired
    /// - claim at t=1500 -> succeeds
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes for legal hold, missing contribution, unsettled escrow,
    /// or an unexpired commitment lock.
    pub fn claim_investor_payout(env: Env, investor: Address) {
        // Operational pause gate (read-only), before require_auth and orthogonal to legal hold.
        ensure(
            &env,
            !Self::paused_active(&env),
            EscrowError::PausedBlocksInvestorClaims,
        );
        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksInvestorClaims);

        investor.require_auth();

        // Single fetch: consolidates the previous two reads of InvestorContribution.
        // Retains the participation guard without a redundant second storage access.
        let contribution: i128 = Self::get_persistent_investor_contribution(&env, investor.clone());
        ensure(&env, contribution > 0, EscrowError::NoContributionToClaim);

        // env.clone(): env is used again after this call for storage reads, ledger timestamp, and publish.
        let escrow = Self::get_escrow(env.clone());
        guard_status_eq(&env, escrow.status, 2, EscrowError::InvestorClaimNotSettled);

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

        // coupon = total_principal × effective_yield_bps / 10_000  (floor)
        let coupon = total_principal
            .checked_mul(effective_yield_bps as i128)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow))
            .checked_div(10_000)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow));

        let settle_pool = total_principal
            .checked_add(coupon)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow));

        // gross_payout = contribution × settle_pool / total_principal  (floor)
        contribution
            .checked_mul(settle_pool)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow))
            .checked_div(total_principal)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow))
    }

    /// Authoritative on-chain aggregate view of the total settlement pool owed by the SME.
    ///
    /// Returns `total_principal + floor(total_principal × yield_bps / 10_000)`, computed
    /// from [`DataKey::FundingCloseSnapshot`] and the escrow's **base** `yield_bps` using
    /// the same [`i128::checked_mul`] / [`i128::checked_div`] arithmetic and
    /// [`EscrowError::ComputePayoutArithmeticOverflow`] guard as
    /// [`LiquifactEscrow::compute_investor_payout`].
    ///
    /// # Rounding
    ///
    /// The coupon is computed with truncating (floor) integer division, identical to the
    /// per-investor formula:
    ///
    /// ```text
    /// coupon       = total_principal × yield_bps / 10_000  (floor)
    /// settle_pool  = total_principal + coupon
    /// ```
    ///
    /// # Yield note
    ///
    /// This view uses the escrow **base yield** (`InvoiceEscrow::yield_bps`). Per-investor
    /// effective yields from [`LiquifactEscrow::fund_with_commitment`] tier selection are
    /// reflected individually in [`LiquifactEscrow::compute_investor_payout`] but are **not**
    /// aggregated here. The result is therefore an authoritative lower-bound aggregate that
    /// avoids per-investor enumeration; it matches the base-yield pool denominator used by
    /// all non-tiered investors.
    ///
    /// # Returns
    ///
    /// - `0` when [`DataKey::FundingCloseSnapshot`] does not exist (escrow not yet funded).
    /// - Computed floor `total_principal + coupon` otherwise.
    ///
    /// # Overflow safety
    ///
    /// All intermediate multiplications use [`i128::checked_mul`]; divisions use
    /// [`i128::checked_div`]. Emits [`EscrowError::ComputePayoutArithmeticOverflow`] (code 129)
    /// rather than silently producing a wrong value.
    ///
    /// # Authorization
    ///
    /// None — pure read; no auth required and no state mutation.
    pub fn get_settlement_pool(env: Env) -> i128 {
        // Snapshot must exist (written when escrow first reaches status == 1).
        // Return 0 before funding, matching compute_investor_payout semantics.
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

        // Read the escrow base yield_bps.
        let escrow: InvoiceEscrow = env
            .storage()
            .instance()
            .get(&DataKey::Escrow)
            .unwrap_or_else(|| fail(&env, EscrowError::EscrowNotInitialized));

        // coupon = total_principal × yield_bps / 10_000  (floor)
        let coupon = total_principal
            .checked_mul(escrow.yield_bps as i128)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow))
            .checked_div(10_000)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow));

        // settle_pool = total_principal + coupon
        total_principal
            .checked_add(coupon)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow))
    }

    pub fn update_maturity(env: Env, new_maturity: u64) -> InvoiceEscrow {
        let mut escrow = Self::load_escrow_require_admin(&env);

        guard_status_eq(&env, escrow.status, 0, EscrowError::MaturityUpdateNotOpen);

        ensure(
            &env,
            new_maturity != escrow.maturity,
            EscrowError::MaturityUnchanged,
        );

        let max_horizon = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::MaturityMaxHorizon)
            .unwrap_or(DEFAULT_MATURITY_MAX_HORIZON_SECS);
        validate_maturity_bounds(&env, new_maturity, max_horizon);

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

    /// Extend the configured funding deadline while the escrow is still open.
    ///
    /// This is intentionally stricter than a generic setter: the call requires an
    /// existing deadline, rejects equal or earlier values, rejects calls after the
    /// current funding window has already closed, and never allows the funding
    /// window to reach or pass a non-zero maturity timestamp.
    ///
    /// # Authorization
    /// Requires the signature of the current [`InvoiceEscrow::admin`].
    ///
    /// # Errors
    /// - [`EscrowError::FundingDeadlineUpdateNotOpen`] if the escrow is not status `0`.
    /// - [`EscrowError::FundingDeadlinePassed`] if the existing deadline has already elapsed.
    /// - [`EscrowError::FundingDeadlineNotExtended`] if no deadline exists or `new_deadline`
    ///   is not strictly greater than the current deadline.
    /// - [`EscrowError::FundingDeadlineAtOrAfterMaturity`] if a non-zero maturity exists and
    ///   `new_deadline >= maturity`.
    ///
    /// # Events
    /// Emits [`FundingDeadlineExtended`] with `invoice_id`, `old_deadline`, and `new_deadline`.
    pub fn extend_funding_deadline(env: Env, new_deadline: u64) -> u64 {
        let escrow = Self::load_escrow_require_admin(&env);

        guard_status_eq(
            &env,
            escrow.status,
            0,
            EscrowError::FundingDeadlineUpdateNotOpen,
        );

        let old_deadline = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::FundingDeadline)
            .unwrap_or_else(|| fail(&env, EscrowError::FundingDeadlineNotExtended));

        ensure(
            &env,
            env.ledger().timestamp() <= old_deadline,
            EscrowError::FundingDeadlinePassed,
        );
        ensure(
            &env,
            new_deadline > old_deadline,
            EscrowError::FundingDeadlineNotExtended,
        );
        if escrow.maturity > 0 {
            ensure(
                &env,
                new_deadline < escrow.maturity,
                EscrowError::FundingDeadlineAtOrAfterMaturity,
            );
        }

        env.storage()
            .instance()
            .set(&DataKey::FundingDeadline, &new_deadline);
        env.storage().instance().extend_ttl(
            INSTANCE_TTL_MIN_EXTENSION_LEDGERS,
            INSTANCE_TTL_MIN_EXTENSION_LEDGERS,
        );

        FundingDeadlineExtended {
            name: symbol_short!("fund_ext"),
            invoice_id: escrow.invoice_id,
            old_deadline,
            new_deadline,
        }
        .publish(&env);

        new_deadline
    }

    /// Update the configured maximum maturity horizon for this escrow instance.
    ///
    /// Only the current admin may call this. The new horizon applies to subsequent
    /// [`LiquifactEscrow::update_maturity`] calls; existing maturity values are unaffected.
    ///
    /// Emits [`MaturityMaxHorizonUpdated`] with the old and new horizon values.
    /// Returns the currently configured maximum maturity horizon (seconds from ledger time).
    /// Falls back to [`DEFAULT_MATURITY_MAX_HORIZON_SECS`] if not overridden.
    pub fn get_maturity_max_horizon(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::MaturityMaxHorizon)
            .unwrap_or(DEFAULT_MATURITY_MAX_HORIZON_SECS)
    }

    pub fn get_remaining_investor_slots(env: Env) -> Option<u32> {
        let cap_opt = Self::get_max_unique_investors_cap(env.clone());
        if let Some(cap) = cap_opt {
            let count = Self::get_unique_funder_count(env);
            Some(cap.saturating_sub(count))
        } else {
            None
        }
    }

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

    /// Monotonically **raise** the maturity-max-horizon ceiling — a forward-only governance lever.
    ///
    /// Unlike the general [`LiquifactEscrow::update_maturity_max_horizon`] setter (which accepts any
    /// value), this entrypoint guarantees the horizon can only ever be raised, never lowered or held
    /// equal. This supports a "term-extension only" policy and avoids the confusing invalid
    /// configuration that arises when a horizon is lowered below an already-set maturity.
    ///
    /// # Authorization
    /// Requires the signature of the current [`InvoiceEscrow::admin`].
    ///
    /// # Errors
    /// - [`EscrowError::HorizonNotRaised`] if `new_horizon` is not strictly greater than the current
    ///   stored horizon (rejects equal or lower values).
    ///
    /// # Events
    /// Emits [`MaturityMaxHorizonRaised`] carrying `invoice_id`, the old horizon, and the new horizon.
    ///
    /// # Returns
    /// The newly stored horizon.
    pub fn raise_maturity_max_horizon(env: Env, new_horizon: u64) -> u64 {
        let escrow = Self::load_escrow_require_admin(&env);

        let old_horizon = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::MaturityMaxHorizon)
            .unwrap_or(DEFAULT_MATURITY_MAX_HORIZON_SECS);

        // Forward-only guard: strictly greater than the current ceiling.
        ensure(
            &env,
            new_horizon > old_horizon,
            EscrowError::HorizonNotRaised,
        );

        env.storage()
            .instance()
            .set(&DataKey::MaturityMaxHorizon, &new_horizon);

        MaturityMaxHorizonRaised {
            name: symbol_short!("mtry_rse"),
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

        // Extend persistent TTL for allowlisted investor entries.
        for addr in allowlisted.iter() {
            // Persistent allowlist entry.
            env.storage().persistent().extend_ttl(
                &DataKey::InvestorAllowlisted(addr.clone()),
                PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
                PERSISTENT_TTL_MIN_EXTENSION_LEDGERS,
            );
            // Instance keys that may be per‑investor (contribution & claim lock).
            env.storage().instance().extend_ttl(
                INSTANCE_TTL_MIN_EXTENSION_LEDGERS,
                INSTANCE_TTL_MIN_EXTENSION_LEDGERS,
            );
        }

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
    /// If a pending proposal already exists, re-proposing the same address is rejected while
    /// replacing it with a different address emits [`AdminProposalSuperseded`].
    ///
    /// Persists [`DataKey::PendingAdmin`] as the proposed successor address and
    /// [`DataKey::PendingAdminExpiry`] as `ledger.timestamp() + window`, where `window`
    /// is `validity_window_secs` when supplied or [`DEFAULT_ADMIN_PROPOSAL_VALIDITY_SECS`] when
    /// `None`.
    ///
    /// The successor must then call [`LiquifactEscrow::accept_admin`] before the expiry timestamp
    /// to complete the handover. If the proposal is not accepted by the expiry, or if the current
    /// admin cancels it via [`LiquifactEscrow::cancel_pending_admin`], the nomination is retracted.
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes when the escrow is uninitialized, the caller is not the
    /// current admin, `new_admin` is the current admin ([`EscrowError::NewAdminSameAsCurrent`]),
    /// or `new_admin` is already pending ([`EscrowError::PendingAdminUnchanged`]).
    ///
    /// # Events
    /// Emits [`AdminProposedEvent`] (topic: `adm_prop`) containing the `invoice_id`, the `current_admin`,
    /// and the `pending_admin` address.
    pub fn propose_admin(
        env: Env,
        new_admin: Address,
        validity_window_secs: Option<u64>,
    ) -> Address {
        let escrow = Self::load_escrow_require_admin(&env);

        ensure(
            &env,
            escrow.admin != new_admin,
            EscrowError::NewAdminSameAsCurrent,
        );

        let previous_pending: Option<Address> =
            env.storage().instance().get(&DataKey::PendingAdmin);
        if let Some(pending) = previous_pending {
            ensure(
                &env,
                pending != new_admin,
                EscrowError::PendingAdminUnchanged,
            );
            AdminProposalSuperseded {
                name: symbol_short!("adm_sup"),
                invoice_id: escrow.invoice_id.clone(),
                previous_pending: pending,
                new_pending: new_admin.clone(),
            }
            .publish(&env);
        }

        let window = validity_window_secs.unwrap_or(DEFAULT_ADMIN_PROPOSAL_VALIDITY_SECS);
        let expiry = env.ledger().timestamp().saturating_add(window);

        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);
        env.storage()
            .instance()
            .set(&DataKey::PendingAdminExpiry, &expiry);

        AdminProposedEvent {
            name: symbol_short!("adm_prop"),
            invoice_id: escrow.invoice_id.clone(),
            current_admin: escrow.admin,
            pending_admin: new_admin.clone(),
        }
        .publish(&env);

        new_admin
    }

    /// Accept a pending admin handover — step 2 of a two-step handover.
    ///
    /// The address stored in [`DataKey::PendingAdmin`] must authorize this call. On success, the
    /// successor is promoted into [`InvoiceEscrow::admin`], and the pending proposal keys
    /// ([`DataKey::PendingAdmin`] and [`DataKey::PendingAdminExpiry`]) are cleared from storage.
    ///
    /// Once accepted, the new admin gains exclusive authority over all admin-gated functions,
    /// including the critical legal-hold recovery path (clearing active holds via
    /// [`LiquifactEscrow::clear_legal_hold`] or [`LiquifactEscrow::clear_legal_hold_after_delay`]).
    /// The previous admin is immediately locked out from admin-gated entrypoints.
    ///
    /// # Expiry
    /// If [`DataKey::PendingAdminExpiry`] is present, `ledger.timestamp()` must be `<=` the
    /// stored expiry (inclusive). Otherwise, the call fails with [`EscrowError::AdminProposalExpired`].
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes:
    /// - [`EscrowError::NoPendingAdmin`] if no admin proposal is currently active.
    /// - [`EscrowError::AdminProposalExpired`] if the proposal's validity window has passed.
    ///
    /// # Events
    /// Emits [`AdminTransferredEvent`] (topic: `admin`) containing the `invoice_id` and the `new_admin` address.
    pub fn accept_admin(env: Env) -> InvoiceEscrow {
        let pending: Option<Address> = env.storage().instance().get(&DataKey::PendingAdmin);
        ensure(&env, pending.is_some(), EscrowError::NoPendingAdmin);
        let pending = pending.unwrap();

        if let Some(expiry) = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::PendingAdminExpiry)
        {
            let now = env.ledger().timestamp();
            ensure(&env, now <= expiry, EscrowError::AdminProposalExpired);
        }

        pending.require_auth();

        let mut escrow = Self::get_escrow(env.clone());
        let prior_admin = escrow.admin.clone();
        escrow.admin = pending.clone();

        env.storage().instance().set(&DataKey::Escrow, &escrow);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.storage()
            .instance()
            .remove(&DataKey::PendingAdminExpiry);

        AdminAcceptedEvent {
            name: symbol_short!("adm_acc"),
            invoice_id: escrow.invoice_id.clone(),
            prior_admin,
            new_admin: pending,
        }
        .publish(&env);

        escrow
    }

    /// Deprecated shim for the former one-step admin transfer API.
    ///
    /// # Warning
    /// This function is deprecated. It does **not** perform an immediate transfer of admin authority.
    /// Instead, it only acts as step 1 by proposing the `new_admin` and delegating to
    /// [`LiquifactEscrow::propose_admin`] with a default expiry.
    ///
    /// The nominated successor address must still explicitly call [`LiquifactEscrow::accept_admin`]
    /// to complete the handover and assume active admin authority. Operators should migrate existing
    /// integrations to call `propose_admin` followed by `accept_admin`.
    #[deprecated(note = "use propose_admin followed by accept_admin")]
    pub fn transfer_admin(env: Env, new_admin: Address) -> InvoiceEscrow {
        let invoice_id = Self::get_escrow(env.clone()).invoice_id;
        Self::propose_admin(env.clone(), new_admin.clone(), None);
        DeprecatedTransferAdminUsed {
            name: symbol_short!("depr_xfer"),
            invoice_id,
            proposed_address: new_admin,
        }
        .publish(&env);
        Self::get_escrow(env)
    }

    /// Cancel a pending admin handover proposal.
    ///
    /// Removes [`DataKey::PendingAdmin`] and [`DataKey::PendingAdminExpiry`] so the previously
    /// nominated address can no longer call [`LiquifactEscrow::accept_admin`]. The current admin
    /// address and all other escrow state remain unchanged.
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
        env.storage()
            .instance()
            .remove(&DataKey::PendingAdminExpiry);

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
    /// See [`docs/escrow-cancellation-refunds.md`](../../docs/escrow-cancellation-refunds.md)
    /// for details on the cancellation lifecycle.
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes when legal hold is active, the escrow is uninitialized,
    /// or the escrow is not in status 0 (open).
    pub fn cancel_funding(env: Env) -> InvoiceEscrow {
        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksCancelFunding);

        let mut escrow = Self::load_escrow_require_admin(&env);

        guard_status_eq(&env, escrow.status, 0, EscrowError::CancelFundingNotOpen);

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
    /// See [`docs/escrow-cancellation-refunds.md`](../../docs/escrow-cancellation-refunds.md)
    /// for details on refund mechanics and idempotency safeguards.
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes when the escrow is not cancelled, the investor has no
    /// refundable contribution, initialized token data is missing, or the refund transfer fails
    /// token-balance invariants.
    pub fn refund(env: Env, investor: Address) {
        Self::refund_impl(&env, investor, false);
    }

    /// Core refund logic shared by [`LiquifactEscrow::refund`] and [`LiquifactEscrow::refund_batch`].
    ///
    /// When `skip_zero_contribution` is `true`, investors with no recorded contribution are
    /// skipped silently (batch mode). Otherwise a zero contribution fails with
    /// [`EscrowError::NoContributionToRefund`].
    fn refund_impl(env: &Env, investor: Address, skip_zero_contribution: bool) {
        investor.require_auth();

        let escrow = Self::get_escrow(env.clone());
        guard_status_eq(env, escrow.status, 4, EscrowError::RefundNotCancelled);

        let amount: i128 = Self::get_persistent_investor_contribution(env, investor.clone());
        if amount <= 0 {
            if skip_zero_contribution {
                return;
            }
            fail(env, EscrowError::NoContributionToRefund);
        }

        // Zero out contribution before transfer (checks-effects-interactions).
        Self::set_persistent_investor_contribution(env, investor.clone(), 0i128);
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

        let token_addr = Self::funding_token_or_fail(env);
        let this = env.current_contract_address();

        external_calls::transfer_funding_token_with_balance_checks(
            env,
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
        .publish(env);
    }

    /// Batch refund entrypoint: refund multiple investors in a single call.
    ///
    /// Each address is processed sequentially with per-investor [`Address::require_auth()`].
    /// All existing [`LiquifactEscrow::refund`] invariants (cancelled-status gate, non-zero
    /// contribution, checks-effects-interactions, liability-floor accounting) are enforced
    /// per entry.
    ///
    /// Already-refunded entries (where [`DataKey::InvestorRefunded`] is already `true`) are
    /// **skipped** without failing the batch — this makes the entrypoint idempotent and
    /// allows relayer-style retry without needing to prune the list.
    ///
    /// # Parameters
    /// - `investors`: `Vec<Address>` of investor addresses to refund.
    ///
    /// # Errors
    /// - [`EscrowError::RefundBatchEmpty`] if `investors` is empty
    /// - [`EscrowError::RefundBatchTooLarge`] if `investors.len() > [`MAX_REFUND_BATCH`]
    ///
    /// Per-entry errors (non-cancelled status, zero contribution, auth failure) are **not**
    /// silently skipped — they terminate the entire batch. Only already-refunded entries
    /// (where [`DataKey::InvestorRefunded`] is `true`) are safely skipped.
    ///
    /// # Events
    /// One [`InvestorRefundedEvt`] per newly-refunded investor.
    pub fn refund_batch(env: Env, investors: Vec<Address>) {
        let n = investors.len();

        ensure(&env, n > 0, EscrowError::RefundBatchEmpty);
        ensure(
            &env,
            n <= MAX_REFUND_BATCH,
            EscrowError::RefundBatchTooLarge,
        );

        for i in 0..n {
            let investor = investors.get(i).unwrap();

            // Skip already-refunded entries without failing.
            if env
                .storage()
                .instance()
                .get(&DataKey::InvestorRefunded(investor.clone()))
                .unwrap_or(false)
            {
                continue;
            }

            // Apply identical per-investor gates as single refund().
            Self::refund(env.clone(), investor);
        }
    }

    /// Allow an investor to partially or fully withdraw their principal while the escrow
    /// remains open (status 0).
    ///
    /// An investor may call `unfund` any number of times before the escrow transitions out
    /// of the open state. Each call decrements the investor's recorded contribution and the
    /// escrow's `funded_amount` by `amount`, then transfers `amount` tokens back to the
    /// investor via the SEP-41 balance-delta wrapper.
    ///
    /// When the investor's contribution reaches zero the `DataKey::UniqueFunderCount` is
    /// decremented by one (floor: 0, via saturating arithmetic). Status always remains 0;
    /// `unfund` never triggers a state transition in either direction.
    ///
    /// # Parameters
    /// - `investor`: The address whose contribution is being reduced. Must match `require_auth`.
    /// - `amount`: The positive amount to withdraw (must be ≤ recorded contribution).
    ///
    /// # Errors
    /// - [`EscrowError::UnfundEscrowNotOpen`] if `status != 0`.
    /// - [`EscrowError::UnfundLegalHoldActive`] if a compliance hold is currently active.
    /// - [`EscrowError::OverWithdrawal`] if `amount` exceeds the investor's contribution.
    ///
    /// # Events
    /// Emits [`EscrowUnfunded`] on success.
    pub fn unfund(env: Env, investor: Address, amount: i128) -> InvoiceEscrow {
        // 1. Status guard (read-only; checked before auth to fail fast).
        let mut escrow = Self::get_escrow(env.clone());
        ensure(&env, escrow.status == 0, EscrowError::UnfundEscrowNotOpen);

        // 2. Legal-hold guard (read-only; checked before auth to fail fast).
        ensure(
            &env,
            !Self::legal_hold_active(&env),
            EscrowError::UnfundLegalHoldActive,
        );

        // 3. Investor auth.
        investor.require_auth();

        // 4. Over-withdrawal guard: the withdrawn amount must not exceed the
        // investor's recorded contribution. `checked_sub` alone is insufficient
        // because `contribution - amount` stays a valid (negative) i128 when
        // `amount > contribution`; an explicit bound is required.
        let contribution: i128 = Self::get_persistent_investor_contribution(&env, investor.clone());
        ensure(&env, amount <= contribution, EscrowError::OverWithdrawal);
        let remaining_contribution = contribution
            .checked_sub(amount)
            .unwrap_or_else(|| fail(&env, EscrowError::OverWithdrawal));

        // Guard: amount must be > 0 (a zero amount would pass checked_sub but is nonsensical).
        // checked_sub on a negative amount would yield a value > contribution — still caught
        // above — but a zero withdrawal is explicitly rejected here for clarity.
        if amount <= 0 {
            fail(&env, EscrowError::OverWithdrawal);
        }

        // 5. funded_amount decrement.
        let new_funded_amount = escrow
            .funded_amount
            .checked_sub(amount)
            .unwrap_or_else(|| fail(&env, EscrowError::OverWithdrawal));

        // 6. Effects — update contribution (checks-effects-interactions).
        Self::set_persistent_investor_contribution(&env, investor.clone(), remaining_contribution);

        // 7. Decrement UniqueFunderCount when contribution reaches zero.
        if remaining_contribution == 0 {
            let cur: u32 = env
                .storage()
                .instance()
                .get(&DataKey::UniqueFunderCount)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::UniqueFunderCount, &cur.saturating_sub(1));
        }

        // 8. Persist updated escrow.
        escrow.funded_amount = new_funded_amount;
        env.storage().instance().set(&DataKey::Escrow, &escrow);

        // 9. Token transfer (interactions last — checks-effects-interactions pattern).
        let token_addr = Self::funding_token_or_fail(&env);
        let this = env.current_contract_address();
        external_calls::transfer_funding_token_with_balance_checks(
            &env,
            &token_addr,
            &this,
            &investor,
            amount,
        );

        // 10. Event emission.
        let timestamp = env.ledger().timestamp();
        EscrowUnfunded {
            name: symbol_short!("unfunded"),
            invoice_id: escrow.invoice_id.clone(),
            investor: investor.clone(),
            amount,
            remaining_contribution,
            new_funded_amount,
            timestamp,
        }
        .publish(&env);

        escrow
    }

    /// Whether an investor has already received a refund in a cancelled escrow.
    pub fn is_investor_refunded(env: Env, investor: Address) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::InvestorRefunded(investor))
            .unwrap_or(false)
    }

    /// Total principal already returned to investors via [`LiquifactEscrow::refund`].
    ///
    /// Used by [`LiquifactEscrow::sweep_terminal_dust`] to compute outstanding liabilities.
    /// Absent ⇒ `0` (no refunds have occurred).
    pub fn get_distributed_principal(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::DistributedPrincipal)
            .unwrap_or(0)
    }

    /// Read-only reconciliation position: the live funding-token balance held by
    /// the contract, the outstanding investor liability, and the resulting
    /// surplus (sweepable dust) or deficit.
    ///
    /// `outstanding_liability` is computed with the **same liability floor** that
    /// [`LiquifactEscrow::sweep_terminal_dust`] enforces (see line
    /// `outstanding = funded_amount - distributed_principal` in that function):
    ///
    /// ```text
    /// outstanding_liability = max(funded_amount - distributed_principal, 0)
    /// surplus               = token_balance - outstanding_liability
    /// ```
    ///
    /// so a caller's view of "what may be swept" never disagrees with the on-chain
    /// invariant. In settled (`2`) and withdrawn (`3`) states `distributed_principal`
    /// stays `0` by design, so `outstanding_liability` reflects the full
    /// `funded_amount`; the reported `surplus` is therefore never larger than what
    /// `sweep_terminal_dust` would actually permit (it only applies the floor in the
    /// cancelled state `4`). `surplus` is negative in a deficit.
    ///
    /// This is a pure read: no authorization, no storage writes. All arithmetic is
    /// saturating, so the view cannot panic on extreme balances or amounts.
    ///
    /// # Errors
    ///
    /// Fails with [`EscrowError::EscrowNotInitialized`] / [`EscrowError::FundingTokenNotSet`]
    /// only when the escrow has not been initialized; it never panics on numeric values.
    pub fn get_reconciliation(env: Env) -> ReconciliationView {
        let escrow = Self::get_escrow(env.clone());

        let distributed: i128 = env
            .storage()
            .instance()
            .get(&DataKey::DistributedPrincipal)
            .unwrap_or(0);

        // Same formula as sweep_terminal_dust's liability floor, floored at zero.
        let outstanding_liability = escrow.funded_amount.saturating_sub(distributed).max(0);

        let token_addr = Self::funding_token_or_fail(&env);
        let this = env.current_contract_address();
        let token_balance = TokenClient::new(&env, &token_addr).balance(&this);

        // Surplus is sweepable dust when positive, a deficit when negative.
        let surplus = token_balance.saturating_sub(outstanding_liability);

        ReconciliationView {
            token_balance,
            outstanding_liability,
            surplus,
        }
    }
}

/// Read-only reconciliation snapshot returned by
/// [`LiquifactEscrow::get_reconciliation`].
///
/// Derive rationale:
/// - `Debug`: improves failure diagnostics in tests.
/// - `PartialEq`: allows exact assertions in tests.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ReconciliationView {
    /// Live SEP-41 funding-token balance held by the contract address.
    pub token_balance: i128,
    /// Principal still owed to investors:
    /// `max(funded_amount - distributed_principal, 0)`. Uses the identical floor
    /// to [`LiquifactEscrow::sweep_terminal_dust`] so the two never disagree.
    pub outstanding_liability: i128,
    /// `token_balance - outstanding_liability`. Positive means sweepable dust
    /// (a surplus); negative means the contract is in deficit for its remaining
    /// obligations.
    pub surplus: i128,
}

#[cfg(test)]
mod test_allowlist_tests;

#[cfg(test)]
mod tests;

/// Default starting balance assigned to any address that has never been seen by the
/// [`DefaultMockToken`] contract.
///
/// The value (100 trillion stroops, i.e. 10 000 000 XLM at 7 decimal places) is large
/// enough that ordinary test escrow amounts never accidentally overdraw an account,
/// while still being representable in a signed 64-bit integer.  Defined once here so
/// that `balance` and `transfer` stay in sync and a single edit suffices to change the
/// test-harness funding level.  Large-principal tests that fund above this ceiling must
/// provision balances via a real Stellar asset token (see `install_stellar_asset_token`).
#[cfg(any(test, feature = "testutils"))]
pub const MOCK_TOKEN_DEFAULT_BALANCE: i128 = 100_000_000_000_000i128;

#[cfg(any(test, feature = "testutils"))]
#[soroban_sdk::contract]
pub struct DefaultMockToken;

#[cfg(any(test, feature = "testutils"))]
#[soroban_sdk::contractimpl]
impl DefaultMockToken {
    pub fn balance(env: soroban_sdk::Env, addr: soroban_sdk::Address) -> i128 {
        let key = soroban_sdk::symbol_short!("balances");
        let balances: soroban_sdk::Map<soroban_sdk::Address, i128> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| soroban_sdk::Map::new(&env));
        balances.get(addr).unwrap_or(MOCK_TOKEN_DEFAULT_BALANCE)
    }

    pub fn transfer(
        env: soroban_sdk::Env,
        from: soroban_sdk::Address,
        to: soroban_sdk::Address,
        amount: i128,
    ) {
        let key = soroban_sdk::symbol_short!("balances");
        let mut balances: soroban_sdk::Map<soroban_sdk::Address, i128> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| soroban_sdk::Map::new(&env));
        let from_bal = balances
            .get(from.clone())
            .unwrap_or(MOCK_TOKEN_DEFAULT_BALANCE);
        let to_bal = balances
            .get(to.clone())
            .unwrap_or(MOCK_TOKEN_DEFAULT_BALANCE);
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
        env.register_at(token_addr, DefaultMockToken, ());
    }
}
