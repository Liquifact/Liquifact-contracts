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

#![allow(clippy::too_many_arguments, dead_code)]

mod keys;


#[cfg(test)]
extern crate std;

use core::{clone::Clone, default::Default};
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error,
    symbol_short, token::TokenClient, Address, BytesN, Env, String, Symbol, Vec,
};

pub mod external_calls;
pub use keys::{
    collateral_pledge_key, investor_claim_not_before_key, investor_effective_yield_key,
    yield_tier_table_key,
};

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

/// Upper bound on [`LiquifactEscrow::batch_bump_ttl`] entries per call.
///
/// Mirrors [`MAX_INVESTOR_ALLOWLIST_BATCH`] — both operations iterate over a
/// bounded address list touching persistent storage once per entry. 32 entries keeps
/// per-call CPU/storage work predictable and consistent with the rest of the
/// admin-batch API surface.
pub const MAX_BUMP_TTL_BATCH: u32 = 32;

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

/// Upper bound on [`LiquifactEscrow::settle_batch`] entries to keep storage/CPU bounded.
pub const MAX_SETTLE_BATCH: u32 = 50;

/// Upper bound on [`LiquifactEscrow::refund_batch`] entries to keep storage/CPU bounded.
pub const MAX_REFUND_BATCH: u32 = 50;

/// Upper bound on [`LiquifactEscrow::set_investors_allowlisted`] batch size.
pub const MAX_INVESTOR_ALLOWLIST_BATCH: u32 = 32;

/// Upper bound on [`LiquifactEscrow::get_contributions`] / investor read batch size.
pub const MAX_INVESTOR_READ_BATCH: u32 = 50;

/// Upper bound on [`LiquifactEscrow::record_sme_collateral_commitment_batch`] entries.
pub const MAX_COLLATERAL_BATCH: u32 = 50;

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
/// Also the **default** for [`LiquifactEscrow::get_storage_limit`] when
/// [`DataKey::StorageLimit`] is unset — preserving pre-configurable behaviour.
pub const INSTANCE_TTL_MIN_EXTENSION_LEDGERS: u32 = 60 * 60; // Approx. 1h at 1 ledger/sec.

/// Minimum persistent storage TTL extension horizon for per-investor allowlist entries.
///
/// When the escrow uses the allowlist gate, investor funding depends on persistent entries.
/// Extending persistent allowlist TTL reduces the risk of silent allowlist disablement.
///
/// When [`DataKey::StorageLimit`] is unset, persistent extensions also fall back to
/// [`INSTANCE_TTL_MIN_EXTENSION_LEDGERS`] (equal to this constant today).
pub const PERSISTENT_TTL_MIN_EXTENSION_LEDGERS: u32 = 60 * 60; // Approx. 1h at 1 ledger/sec.

/// Minimum allowed value for [`LiquifactEscrow::set_storage_limit`].
///
/// One ledger is the smallest meaningful TTL extension; zero would be a no-op.
pub const MIN_STORAGE_LIMIT_LEDGERS: u32 = 1;

/// Maximum allowed value for [`LiquifactEscrow::set_storage_limit`].
///
/// Approx. 1 year at 1 ledger/sec; generous enough for long-lived escrows
/// while staying well within Soroban's archival window.
pub const MAX_STORAGE_LIMIT_LEDGERS: u32 = 31_536_000; // ~365 days

/// Default maximum duration (seconds) an operational pause ([`DataKey::Paused`]) may remain
/// active before it auto-expires for gate-checking purposes. `0` = unlimited, which reproduces
/// the legacy (pre-configurable) behavior exactly: a pause set with no duration limit configured
/// blocks gated entrypoints until an admin explicitly calls [`LiquifactEscrow::set_paused`] with
/// `active = false`.
pub const DEFAULT_PAUSE_MAX_DURATION_SECS: u64 = 0;

/// Minimum non-zero value accepted by [`LiquifactEscrow::set_pause_max_duration`].
/// Prevents configuring a duration so short it defeats the purpose of the incident-response
/// circuit breaker.
pub const MIN_PAUSE_MAX_DURATION_SECS: u64 = 3_600; // 1 hour

/// Maximum value accepted by [`LiquifactEscrow::set_pause_max_duration`].
pub const MAX_PAUSE_MAX_DURATION_SECS: u64 = 7_776_000; // 90 days

/// Default maximum number of [`LiquifactEscrow::set_paused`] calls allowed within
/// [`DataKey::PauseToggleWindowSecs`]. `0` = unlimited, reproducing legacy behavior: no rate
/// limit on how often the pause can be toggled.
pub const DEFAULT_PAUSE_TOGGLE_LIMIT: u32 = 0;

/// Minimum non-zero toggle count accepted by [`LiquifactEscrow::set_pause_rate_limit`].
pub const MIN_PAUSE_TOGGLE_LIMIT: u32 = 1;

/// Maximum toggle count accepted by [`LiquifactEscrow::set_pause_rate_limit`].
pub const MAX_PAUSE_TOGGLE_LIMIT: u32 = 1_000;

/// Minimum rate-limit window (seconds) accepted by [`LiquifactEscrow::set_pause_rate_limit`]
/// when a non-zero toggle limit is configured.
pub const MIN_PAUSE_TOGGLE_WINDOW_SECS: u64 = 60; // 1 minute

/// Maximum rate-limit window (seconds) accepted by [`LiquifactEscrow::set_pause_rate_limit`].
pub const MAX_PAUSE_TOGGLE_WINDOW_SECS: u64 = 7_776_000; // 90 days

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
    /// [`LiquifactEscrow::get_revoked_attestation_digests`] received a zero page limit.
    AttestationReadLimitZero = 57,
    /// [`LiquifactEscrow::get_revoked_attestation_digests`] exceeded
    /// [`MAX_ATTESTATION_READ_PAGE`].
    AttestationReadLimitTooLarge = 58,

    /// [`LiquifactEscrow::record_sme_collateral_commitment`] received a non-positive amount.
    CollateralAmountNotPositive = 60,
    /// [`LiquifactEscrow::record_sme_collateral_commitment`] received an empty asset symbol.
    CollateralAssetEmpty = 61,
    /// [`LiquifactEscrow::record_sme_collateral_commitment`] received a timestamp before the stored record.
    CollateralTimestampBackwards = 62,
    /// [`LiquifactEscrow::record_sme_collateral_commitment_batch`] received an empty items vector.
    CollateralBatchEmpty = 63,
    /// [`LiquifactEscrow::record_sme_collateral_commitment_batch`] exceeded [`MAX_COLLATERAL_BATCH`].
    CollateralBatchTooLarge = 64,

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
    /// [`LiquifactEscrow::upgrade_allowlist`] was called by a `caller` other than the
    /// configured [`InvoiceEscrow::admin`]. The upgrade authorization is rejected and no
    /// event is emitted / no WASM is swapped.
    AllowlistUpgradeUnauthorizedCaller = 93,

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

    /// [`LiquifactEscrow::settle_batch`] received an empty escrow addresses vector.
    SettlementBatchEmpty = 223,
    /// [`LiquifactEscrow::settle_batch`] exceeded [`MAX_SETTLE_BATCH`].
    SettlementBatchTooLarge = 224,
    /// [`LiquifactEscrow::unfund`] called when [`InvoiceEscrow::status`] is not 0 (open).
    /// Unfunding is only valid while the escrow is still accepting contributions.
    UnfundEscrowNotOpen = 220,

    /// [`LiquifactEscrow::unfund`] requested amount exceeds the investor's recorded contribution.
    /// Never withdraw more than was contributed; checked via [`i128::checked_sub`].
    OverWithdrawal = 221,

    /// [`LiquifactEscrow::unfund`] blocked because a compliance/legal hold is active.
    /// No fund movement is permitted until the hold is cleared by the admin.
    UnfundLegalHoldActive = 222,

    /// [`LiquifactEscrow::set_pause_max_duration`] received a nonzero value outside
    /// [`MIN_PAUSE_MAX_DURATION_SECS`, `MAX_PAUSE_MAX_DURATION_SECS`].
    PauseMaxDurationOutOfRange = 230,
    /// [`LiquifactEscrow::set_pause_rate_limit`] received a nonzero `max_toggles` outside
    /// [`MIN_PAUSE_TOGGLE_LIMIT`, `MAX_PAUSE_TOGGLE_LIMIT`].
    PauseToggleLimitOutOfRange = 231,
    /// [`LiquifactEscrow::set_pause_rate_limit`] received a `window_secs` outside
    /// [`MIN_PAUSE_TOGGLE_WINDOW_SECS`, `MAX_PAUSE_TOGGLE_WINDOW_SECS`] while `max_toggles > 0`.
    PauseToggleWindowOutOfRange = 225,
    /// [`LiquifactEscrow::set_pause_rate_limit`] received `max_toggles == 0` paired with a
    /// nonzero `window_secs`, or vice versa. Both must be zero together (disabled) or both
    /// nonzero (enabled).
    PauseRateLimitInvalidCombination = 226,
    /// [`LiquifactEscrow::set_paused`] blocked because the configured pause-toggle rate limit
    /// was already reached within the current window. Wait for the window to roll over or ask
    /// the admin to raise the limit via [`LiquifactEscrow::set_pause_rate_limit`].
    PauseToggleRateLimitExceeded = 227,
    /// [`LiquifactEscrow::update_yield_bps`] called while escrow is not in open status (`status != 0`).
    /// Base yield may only be updated before any investor has committed principal.
    YieldBpsUpdateNotOpen = 228,
    /// [`LiquifactEscrow::update_yield_bps`] received a `new_yield_bps` equal to the current value.
    /// No-op updates are rejected to prevent spurious events and unnecessary storage writes.
    YieldBpsUnchanged = 229,
    /// [`LiquifactEscrow::set_storage_limit`] received a non-positive limit.
    StorageLimitNotPositive = 232,
    /// [`LiquifactEscrow::set_storage_limit`] received a limit outside allowed range.
    StorageLimitOutOfRange = 233,
    /// [`LiquifactEscrow::bump_ttl_batch`] received an empty escrow addresses vector.
    BumpTtlBatchEmpty = 234,
    /// [`LiquifactEscrow::bump_ttl_batch`] exceeded [`MAX_BUMP_TTL_BATCH`].
    BumpTtlBatchTooLarge = 235,
    /// The yield-tier table supplied to [`LiquifactEscrow::set_yield_tiers`] violates an
    /// invariant: the table is empty, a `yield_bps` falls outside `0..=10_000`,
    /// `min_lock_secs` is not strictly increasing, or `yield_bps` decreases between tiers.
    YieldTierTableInvalid = 236,

    /// [`LiquifactEscrow::set_collateral_parameters`] received an amount exceeding the maximum allowed.
    CollateralAmountExceedsMax = 239,
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

/// Validate a single yield tier against the base yield and (optionally) the
/// preceding tier in the ladder.
///
/// This is the shared validation primitive for yield-tier tables: it encodes the
/// per-tier rules exactly once so that every call site (init-time table
/// validation and any admin tier-setter) applies identical checks. It returns a
/// typed [`EscrowError`] rather than panicking so callers can decide how to
/// surface the failure.
///
/// Rules (checked in order; the first violation is returned):
/// - `tier.yield_bps` must be within `0..=10_000` → [`EscrowError::TierYieldOutOfRange`]
/// - `tier.yield_bps` must be `>= base_yield`     → [`EscrowError::TierYieldBelowBase`]
/// - when `prev` is `Some`, `tier.min_lock_secs` must be strictly greater than
///   `prev.min_lock_secs` → [`EscrowError::TierLockNotIncreasing`]
/// - when `prev` is `Some`, `tier.yield_bps` must be `>= prev.yield_bps`
///   (non-decreasing) → [`EscrowError::TierYieldNotNonDecreasing`]
pub(crate) fn validate_yield_tier(
    tier: &YieldTier,
    base_yield: i64,
    prev: Option<&YieldTier>,
) -> Result<(), EscrowError> {
    if !(0..=10_000).contains(&tier.yield_bps) {
        return Err(EscrowError::TierYieldOutOfRange);
    }
    if tier.yield_bps < base_yield {
        return Err(EscrowError::TierYieldBelowBase);
    }
    if let Some(prev) = prev {
        if tier.min_lock_secs <= prev.min_lock_secs {
            return Err(EscrowError::TierLockNotIncreasing);
        }
        if tier.yield_bps < prev.yield_bps {
            return Err(EscrowError::TierYieldNotNonDecreasing);
        }
    }
    Ok(())
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

/// Helper: ensures investor is allowlisted if the allowlist is active.
pub(crate) fn require_investor_allowlisted(
    env: &Env,
    investor: &Address,
) -> Result<(), EscrowError> {
    if LiquifactEscrow::is_allowlist_active(env.clone())
        && !LiquifactEscrow::is_investor_allowlisted(env.clone(), investor.clone())
    {
        return Err(EscrowError::InvestorNotAllowlisted);
    }
    Ok(())
}

/// Shared guard: validate funding amount against positivity and minimum contribution floor.
///
/// Ensures the `amount` is strictly positive, and if a [`DataKey::MinContributionFloor`]
/// is configured, ensures the amount meets or exceeds that floor. This logic is shared
/// by all funding entrypoints to prevent inline validation repetition.
///
/// # Errors
/// Returns [`EscrowError::FundingAmountNotPositive`] if `amount <= 0`.
/// Returns [`EscrowError::FundingBelowMinContribution`] if `amount < floor`.
pub(crate) fn validate_funding_amount(env: &Env, amount: i128) -> Result<(), EscrowError> {
    if amount <= 0 {
        return Err(EscrowError::FundingAmountNotPositive);
    }
    let floor: i128 = env
        .storage()
        .instance()
        .get(&keys::min_contribution_floor())
        .unwrap_or(0);
    if floor > 0 && amount < floor {
        return Err(EscrowError::FundingBelowMinContribution);
    }
    Ok(())
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
/// Operational pause guard: asserts that the operational pause ([`DataKey::Paused`]) is not active.
///
/// Replaces the repeated inline pattern `ensure(&env, !Self::paused_active(&env), EscrowError::PausedBlocks*)`
/// that previously appeared at risk-bearing entrypoints — `fund_impl`, `settle`, `withdraw`, and
/// `claim_investor_payout`.
///
/// # Errors
/// Panics with the caller-supplied `error` (one of the `EscrowError::PausedBlocks*`
/// variants) when [`DataKey::Paused`] is `true`.
///
/// # Security notes
/// - Read-only: performs a single instance-storage read with `unwrap_or(false)` (no panic on
///   missing key). Does not write or delete any storage key.
/// - This helper is **not** an authorization check. Callers must still call
///   `Address::require_auth()` for the entrypoint's bound role before any storage mutation
///   or token transfer, per [ADR-002](docs/adr/ADR-002-auth-boundaries.md).
/// - The `Paused` flag is independent of the compliance legal hold ([`DataKey::LegalHold`]); an
///   entrypoint that needs both gates must compose `guard_not_paused` with `guard_not_legal_hold`.
#[inline(always)]
pub(crate) fn guard_not_paused(env: &Env, error: EscrowError) {
    ensure(env, !LiquifactEscrow::paused_active(env), error);
}

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
///   `guard_not_paused(env, PausedBlocks*)` itself.
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
    /// Optional cap (seconds) on how long [`DataKey::Paused`] may remain active before
    /// [`LiquifactEscrow::is_paused`] and the pause gates treat it as expired. Absent ⇒ `0`
    /// (unlimited), identical to pre-existing behavior. Set via
    /// [`LiquifactEscrow::set_pause_max_duration`].
    PauseMaxDurationSecs,
    /// Ledger timestamp recorded on the most recent `set_paused(true)` call; paired with
    /// [`DataKey::PauseMaxDurationSecs`] to compute auto-expiry. Absent ⇒ pause was never
    /// activated.
    PausedAt,
    /// Optional cap on the number of [`LiquifactEscrow::set_paused`] calls allowed within
    /// [`DataKey::PauseToggleWindowSecs`]. Absent ⇒ `0` (unlimited), identical to pre-existing
    /// behavior. Set via [`LiquifactEscrow::set_pause_rate_limit`].
    PauseToggleLimit,
    /// Rolling rate-limit window length (seconds), paired with [`DataKey::PauseToggleLimit`].
    /// Absent ⇒ `0`.
    PauseToggleWindowSecs,
    /// Ledger timestamp when the current pause-toggle rate-limit window started.
    /// Absent ⇒ no window open yet (next `set_paused` call starts one).
    PauseToggleWindowStart,
    /// Number of [`LiquifactEscrow::set_paused`] calls recorded within the current rate-limit
    /// window. Absent ⇒ `0`.
    PauseToggleCountInWindow,
    /// Admin-configured ceiling on storage entries processed per batch operation.
    /// **Additive key (ADR-007):** absent ⇒ [`DEFAULT_SETTLEMENT_LIMIT`]. Updatable via
    /// [`LiquifactEscrow::set_storage_limit`].
    StorageLimit,
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

/// Result of yield-tier resolution for a given commitment.
///
/// Returned by [`LiquifactEscrow::preview_yield_tier`] and produced internally by
/// `effective_yield_for_commitment`. Replaces the former `(i64, u64)` tuple so that
/// callers can reference fields by name instead of by position.
///
/// # Fields
/// - `effective_yield_bps`: The resolved yield in basis points. Equals the escrow base
///   yield when no tier matched, or the highest qualifying tier's `yield_bps` otherwise.
/// - `matched_lock_secs`: The `min_lock_secs` of the matched tier, or `0` when the base
///   yield applies (no tier table, empty table, zero-lock commitment, or no tier qualified).
///
/// Derive rationale:
/// - `Clone`: required for use in `Option` and event fields.
/// - `Debug`: improves failure diagnostics in tests.
/// - `PartialEq`: allows deterministic assertion in tests.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct YieldResolution {
    /// Resolved yield in basis points for this commitment.
    pub effective_yield_bps: i64,
    /// `min_lock_secs` of the matched tier, or `0` when base yield applies.
    pub matched_lock_secs: u64,
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

/// Admin-configurable funding parameters that may be updated atomically after init.
///
/// Each field is optional — a `None` field leaves the current value unchanged.
/// All `Some` fields are validated against the same bounds enforced by the individual
/// parameter setters before any storage write occurs. On success a single
/// [`FundingParametersUpdated`] event is emitted carrying the updated values.
///
/// Derive rationale:
/// - `Clone`: required by event emission (event struct is consumed by `.publish`).
/// - `Debug`: improves failure diagnostics in tests.
/// - `PartialEq`: allows deterministic assertion of stored/read values.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FundingParameters {
    /// Minimum per-call contribution floor. When `Some`, must be positive and strictly
    /// lower than the current floor (same rule as [`LiquifactEscrow::lower_min_contribution_floor`]).
    pub min_contribution_floor: Option<i128>,
    /// Maximum distinct investor addresses. When `Some`, a cap must already exist and
    /// the new value must be strictly higher (same rule as [`LiquifactEscrow::raise_max_unique_investors`]).
    pub max_unique_investors_cap: Option<u32>,
    /// Maximum principal per investor address. When `Some`, a cap must already exist and
    /// the new value must be strictly higher (same rule as [`LiquifactEscrow::raise_max_per_investor`]).
    pub max_per_investor_cap: Option<i128>,
    /// Optional funding deadline. When `Some`, a deadline must already exist, must not
    /// have passed, must be strictly later, and must be before maturity if set
    /// (same rule as [`LiquifactEscrow::extend_funding_deadline`]).
    pub funding_deadline: Option<u64>,
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

/// Typed return value from [`LiquifactEscrow::settle`].
///
/// Replaces the previous opaque tuple / raw [`InvoiceEscrow`] return with a
/// documented struct that bundles the post-settlement escrow state together
/// with the settlement-specific computed values callers need.
///
/// # Fields
/// - `escrow`: The full post-settlement escrow snapshot (status == 2).
/// - `coupon`: The computed coupon (`funded_amount × yield_bps / 10_000`, floor).
/// - `settle_pool`: Total settlement pool (`funded_amount + coupon`).
/// - `settled_at`: Ledger timestamp when settlement occurred.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SettlementResult {
    /// Post-settlement escrow snapshot (status == 2).
    pub escrow: InvoiceEscrow,
    /// Coupon: `funded_amount × yield_bps / 10_000` (floor, checked).
    pub coupon: i128,
    /// Total settlement pool: `funded_amount + coupon`.
    pub settle_pool: i128,
    /// Ledger timestamp at which settlement was recorded.
    pub settled_at: u64,
}

/// Read-only snapshot of all settlement-relevant configuration.
///
/// Returned by [`LiquifactEscrow::get_settlement_config`]. Every field is read from
/// on-chain storage with the same defaults the contract applies at [`LiquifactEscrow::init`],
/// so the view is safe to call before initialization — callers receive the pre-init
/// defaults without a panic.
///
/// # Fields
/// - `yield_bps`: Base coupon yield in basis points (`0..=10_000`).
/// - `maturity`: Maturity timestamp; `0` means no maturity lock.
/// - `protocol_fee_bps`: Immutable protocol fee applied at [`LiquifactEscrow::withdraw`].
/// - `yield_tiers`: Optional tier ladder for investor-specific yields.
/// - `maturity_max_horizon`: Maximum allowed maturity horizon (seconds from ledger time).
/// - `funding_deadline`: Optional deadline after which funding is rejected.
/// - `min_contribution_floor`: Minimum per-deposit amount (0 = no floor).
/// - `max_unique_investors_cap`: Optional cap on distinct investor addresses.
/// - `max_per_investor_cap`: Optional cap on principal per single investor.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SettlementConfig {
    /// Base coupon yield in basis points (`0..=10_000`); `0` before init.
    pub yield_bps: i64,
    /// Maturity timestamp; `0` means no maturity lock.
    pub maturity: u64,
    /// Immutable protocol fee in basis points applied at withdraw; `0` before init.
    pub protocol_fee_bps: i64,
    /// Optional tier ladder for investor-specific yields; empty before init.
    pub yield_tiers: Vec<YieldTier>,
    /// Maximum allowed maturity horizon in seconds from current ledger time.
    /// Falls back to [`DEFAULT_MATURITY_MAX_HORIZON_SECS`].
    pub maturity_max_horizon: u64,
    /// Optional deadline after which new deposits are rejected.
    pub funding_deadline: Option<u64>,
    /// Minimum per-deposit amount in token base units; `0` means no floor.
    pub min_contribution_floor: i128,
    /// Optional cap on distinct investor addresses; `None` means unlimited.
    pub max_unique_investors_cap: Option<u32>,
    /// Optional cap on total principal per single investor; `None` means unlimited.
    pub max_per_investor_cap: Option<i128>,
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

/// Emitted by [`LiquifactEscrow::update_funding_parameters`] after one or more
/// funding parameters are updated atomically. Each field that changed carries
/// `Some(new_value)`; unchanged fields are `None`.
#[contractevent]
pub struct FundingParametersUpdated {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    /// New minimum contribution floor, or `None` if unchanged.
    pub min_contribution_floor: Option<i128>,
    /// New maximum unique investor cap, or `None` if unchanged.
    pub max_unique_investors_cap: Option<u32>,
    /// New per-investor cap, or `None` if unchanged.
    pub max_per_investor_cap: Option<i128>,
    /// New funding deadline, or `None` if unchanged.
    pub funding_deadline: Option<u64>,
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
pub struct BenChange {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub prior_sme: Address,
    pub new_sme: Address,
    pub amount: i128,
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
pub struct ProtocolFeeUpdated {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_fee_bps: i64,
    pub new_fee_bps: i64,
}

/// Emitted by [`LiquifactEscrow::update_yield_bps`] when the base yield rate is changed.
///
/// # Fields
/// - `name`: hardcoded `yld_upd` symbol.
/// - `invoice_id`: escrow invoice identifier.
/// - `old_yield_bps`: prior base yield in basis points.
/// - `new_yield_bps`: new base yield in basis points after the update.
#[contractevent]
pub struct YieldBpsUpdatedEvent {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_yield_bps: i64,
    pub new_yield_bps: i64,
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

/// Emitted by [`LiquifactEscrow::set_pause_max_duration`] whenever the configured auto-expiry
/// duration for [`DataKey::Paused`] changes.
#[contractevent]
pub struct PauseMaxDurationUpdated {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_value: u64,
    pub new_value: u64,
}

/// Emitted by [`LiquifactEscrow::set_pause_rate_limit`] whenever the pause-toggle rate limit
/// or its window changes.
#[contractevent]
pub struct PauseRateLimitUpdated {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_limit: u32,
    pub new_limit: u32,
    pub old_window_secs: u64,
    pub new_window_secs: u64,
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
/// Yield-tier table replaced by the admin.
///
/// Emitted by [`LiquifactEscrow::set_yield_tiers`] once the replacement table has passed
/// every `init`-equivalent invariant and has been written to [`DataKey::YieldTierTable`].
/// A rejected call emits nothing, so the presence of this event is a reliable signal that
/// the stored ladder actually changed.
///
/// # Fields
/// - `name`: Hardcoded `yt_upd` symbol.
/// - `invoice_id`: Invoice identifier of the escrow whose table changed.
/// - `tier_count`: Number of tiers in the newly stored table.
#[contractevent]
pub struct YieldTierTableUpdated {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub tier_count: u32,
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
/// storage after the mutation. Exactly one `coll_clr` event is published per
/// successful clear — do not emit a second event with the same topic.
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

/// Emitted by [`LiquifactEscrow::set_collateral_parameters`] when the admin updates
/// the collateral commitment parameters.
#[contractevent]
pub struct CollateralParametersUpdated {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub asset: Symbol,
    pub amount: i128,
    pub prior_amount: i128,
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

/// Emitted after a successful [`LiquifactEscrow::unrevoke_attestation_digest`].
/// Clears the revocation marker for a previously revoked attestation digest entry.
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

#[contractevent]
pub struct AllowlistStateChanged {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub total_count: u32,
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

/// Emitted by [`LiquifactEscrow::upgrade_allowlist`] once the caller has passed the explicit
/// admin authorization check, immediately before the WASM is replaced.
///
/// This is the allowlist subsystem's dedicated upgrade-authorization audit trail. Unlike
/// [`ContractUpgraded`] (emitted by the generic [`LiquifactEscrow::upgrade`]), this event also
/// carries the authorizing `admin` address as a topic so indexers can attribute every allowlist
/// upgrade to the exact account that authorized it. Like [`ContractUpgraded`], it is published
/// **before** `env.deployer().update_current_contract_wasm` (defensive ordering).
///
/// # Fields
/// - `name`: hardcoded `"al_upg"` symbol (topic).
/// - `invoice_id`: the escrow's `invoice_id` (topic, for indexer correlation).
/// - `admin`: the admin address that authorized the upgrade (topic).
/// - `new_wasm_hash`: the 32-byte hash of the incoming WASM binary.
#[contractevent]
pub struct AllowlistUpgradeAuthorized {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    #[topic]
    pub admin: Address,
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

    /// Read the operational pause flag; defaults to `false` when unset.
    /// Read the operational pause flag ([`DataKey::Paused`]); defaults to `false` when unset.
    ///
    /// Orthogonal to [`LiquifactEscrow::legal_hold_active`] — neither flag affects the other.
    ///
    /// # Auto-expiry
    /// When [`DataKey::PauseMaxDurationSecs`] is configured (nonzero) via
    /// [`LiquifactEscrow::set_pause_max_duration`], a pause that has been active for at least
    /// that many seconds (measured from [`DataKey::PausedAt`]) is treated as inactive here —
    /// even though the stored `Paused` flag itself is left `true` until an admin explicitly
    /// calls [`LiquifactEscrow::set_paused`]. This is a pure read computation (no storage
    /// mutation), so it cannot violate the read-only-precondition invariant documented on
    /// [ADR-002](docs/adr/ADR-002-auth-boundaries.md). Default (`0` / unset) reproduces the
    /// legacy behavior exactly: a pause blocks gates indefinitely until explicitly cleared.
    fn paused_active(env: &Env) -> bool {
        let stored: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        if !stored {
            return false;
        }
        let max_duration: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PauseMaxDurationSecs)
            .unwrap_or(DEFAULT_PAUSE_MAX_DURATION_SECS);
        if max_duration == 0 {
            return true;
        }
        let paused_at: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PausedAt)
            .unwrap_or(0);
        let now = env.ledger().timestamp();
        match paused_at.checked_add(max_duration) {
            Some(expires_at) => now < expires_at,
            None => true,
        }
    }

    /// Read the immutable funding token address, failing with [`EscrowError::FundingTokenNotSet`]
    /// when the escrow has not been initialized.
    fn funding_token_or_fail(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&keys::funding_token())
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

    pub fn is_allowlist_active(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::AllowlistActive)
            .unwrap_or(false)
    }

    pub fn is_investor_allowlisted(env: Env, investor: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::InvestorAllowlisted(investor))
            .unwrap_or(false)
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
            let prev = if i > 0 {
                Some(tiers.get(i - 1).unwrap())
            } else {
                None
            };
            if let Err(e) = validate_yield_tier(&t, base_yield, prev.as_ref()) {
                fail(env, e);
            }
        }
    }

    /// Returns a [`YieldResolution`] for a given commitment.
    ///
    /// Scans [`DataKey::YieldTierTable`] and picks the tier with the highest `yield_bps`
    /// where `committed_lock_secs >= tier.min_lock_secs`. Returns base yield when:
    /// `committed_lock_secs == 0`, no tier table exists, or table is empty.
    ///
    /// Example with `base=800, tiers=[(100,900),(200,1000),(300,1200)]`:
    /// - lock=50  -> `{ effective_yield_bps: 800, matched_lock_secs: 0 }`   no tier matched
    /// - lock=100 -> `{ effective_yield_bps: 900, matched_lock_secs: 100 }` tier 0
    /// - lock=250 -> `{ effective_yield_bps: 1000, matched_lock_secs: 200 }` tier 1
    /// - lock=300 -> `{ effective_yield_bps: 1200, matched_lock_secs: 300 }` tier 2 (highest)
    ///
    /// `matched_lock_secs` is the `min_lock_secs` of the matched tier, or `0` for base yield.
    fn effective_yield_for_commitment(
        env: &Env,
        base_yield: i64,
        committed_lock_secs: u64,
    ) -> YieldResolution {
        if committed_lock_secs == 0 {
            return YieldResolution {
                effective_yield_bps: base_yield,
                matched_lock_secs: 0,
            };
        }
        let Some(tiers) = env
            .storage()
            .instance()
            .get::<DataKey, Vec<YieldTier>>(&yield_tier_table_key())
        else {
            return YieldResolution {
                effective_yield_bps: base_yield,
                matched_lock_secs: 0,
            };
        };
        if tiers.is_empty() {
            return YieldResolution {
                effective_yield_bps: base_yield,
                matched_lock_secs: 0,
            };
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
        YieldResolution {
            effective_yield_bps: best,
            matched_lock_secs: best_lock,
        }
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
    /// # Yield & Fee Parameter Bounds
    ///
    /// **Base yield (`yield_bps`):**
    /// - Valid range: `0..=10_000` basis points (0% to 100%)
    /// - `0` = no yield (valid; passive bond)
    /// - `10_000` = 100% yield (valid; maximum coupon)
    /// - Rejection: `YieldBpsOutOfRange` if outside `0..=10_000`
    /// - **Derivation**: Basis point convention; arithmetic safety for coupon = principal × yield / 10_000
    ///
    /// **Protocol fee (`protocol_fee_bps`):**
    /// - Valid range: `0..=10_000` basis points (0% to 100%)
    /// - `0` = no fee, SME receives full disbursement (default)
    /// - `10_000` = full disbursement routed to treasury
    /// - Rejection: `ProtocolFeeBpsOutOfRange` if outside `0..=10_000`
    /// - **Derivation**: Same basis point convention as yield; fee split math at withdrawal
    ///
    /// **Yield tiers (`yield_tiers`):**
    /// When configured, each tier receives validation:
    /// - Each tier's `yield_bps` must be in `0..=10_000` → `TierYieldOutOfRange`
    /// - Each tier's `yield_bps` must be ≥ base `yield_bps` → `TierYieldBelowBase`
    /// - Tier `min_lock_secs` must be strictly increasing across tiers → `TierLockNotIncreasing`
    /// - Tier `yield_bps` must be non-decreasing across tiers → `TierYieldNotNonDecreasing`
    /// - Individual `min_lock_secs` values require no explicit bound (u64 range is inherently safe;
    ///   used only for comparison in tier selection, no arithmetic risk)
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

        if let Some(deadline) = &funding_deadline {
            env.storage()
                .instance()
                .set(&DataKey::FundingDeadline, deadline);
        }

        env.storage()
            .instance()
            .set(&keys::funding_token(), &funding_token);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage()
            .instance()
            .set(&DataKey::Version, &SCHEMA_VERSION);

        if let Some(reg) = &registry {
            env.storage().instance().set(&DataKey::RegistryRef, reg);
        }

        if let Some(tiers) = &yield_tiers {
            env.storage().instance().set(&yield_tier_table_key(), tiers);
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
            .set(&keys::min_contribution_floor(), &floor);
        // Always persist the fee (even the `0` default) so `withdraw` reads never branch on absence.
        env.storage()
            .instance()
            .set(&DataKey::ProtocolFeeBps, &protocol_fee_bps);
        env.storage()
            .instance()
            .set(&keys::unique_funder_count(), &0u32);

        if let Some(cap) = max_per_investor {
            ensure(&env, cap > 0, EscrowError::MaxPerInvestorNotPositive);
            env.storage()
                .instance()
                .set(&keys::max_per_investor_cap(), &cap);
        }

        if let Some(cap) = max_unique_investors {
            ensure(&env, cap > 0, EscrowError::MaxUniqueInvestorsNotPositive);
            env.storage()
                .instance()
                .set(&keys::max_unique_investors_cap(), &cap);
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
                .set(&keys::funding_deadline(), &deadline);
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

    /// Returns the current beneficiary (SME) address that receives funded principal on
    /// [`LiquifactEscrow::withdraw`], or [`None`] when the escrow has not yet been
    /// initialized.
    ///
    /// The beneficiary is [`InvoiceEscrow::sme_address`] stored in [`DataKey::Escrow`].
    /// This is a focused O(1) read view that avoids forcing callers to reconstruct the
    /// full escrow state when only the payout destination is needed.
    ///
    /// # Returns
    /// - `None` — escrow not yet initialized (no [`DataKey::Escrow`] entry in storage).
    /// - `Some(addr)` — the current beneficiary address; updated by
    ///   [`LiquifactEscrow::rotate_beneficiary`].
    ///
    /// # Authorization
    /// None — this is a read-only view entrypoint. No `require_auth` is called.
    ///
    /// # Storage mutations
    /// None — this entrypoint never writes to storage.
    pub fn get_beneficiary(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get::<DataKey, InvoiceEscrow>(&DataKey::Escrow)
            .map(|escrow| escrow.sme_address)
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
            prior_sme: prior_sme.clone(),
            new_sme: new_sme_address.clone(),
        }
        .publish(&env);

        BenChange {
            name: symbol_short!("ben_chg"),
            invoice_id: escrow.invoice_id.clone(),
            prior_sme,
            new_sme: new_sme_address,
            amount: escrow.amount,
        }
        .publish(&env);

        escrow
    }

    /// Resolve a `(start, limit)` pagination request against a collection of `len` items.
    ///
    /// Returns `Some((start, end))` where `end` is exclusive and `end <= len`, or `None` when
    /// the requested page is entirely out of range (i.e. when `start >= len` or `limit == 0`).
    /// Arithmetic is saturating: a `start + capped_limit` that would overflow `u32` is clamped
    /// at `len` rather than wrapping.
    ///
    /// The caller is responsible for supplying the appropriate per-operation ceiling as
    /// `ceiling` so that the returned window never exceeds that cap.
    ///
    /// # Arguments
    /// * `start`   — 0-based index of the first item to include (inclusive).
    /// * `limit`   — requested page size (caller-supplied, uncapped).
    /// * `ceiling` — maximum page size enforced by this entrypoint (e.g. [`MAX_INVESTOR_READ_BATCH`]).
    /// * `len`     — total number of items in the backing collection.
    ///
    /// # Returns
    /// * `Some((start, end))` — the resolved `[start, end)` window.
    /// * `None`               — the page is empty (out-of-bounds or zero limit).
    pub(crate) fn paginate_window(
        start: u32,
        limit: u32,
        ceiling: u32,
        len: u32,
    ) -> Option<(u32, u32)> {
        if start >= len || limit == 0 {
            return None;
        }
        let capped = limit.min(ceiling);
        // Saturating add: if start + capped would overflow, clamp at len (which is <= u32::MAX).
        let end = start.saturating_add(capped).min(len);
        Some((start, end))
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

    /// Load the attestation append-log from instance storage.
    ///
    /// Consolidates the repeated pattern of reading `DataKey::AttestationAppendLog` with an
    /// empty-vec fallback used by `append_attestation_digest`, `revoke_attestation_digest`,
    /// `revoke_attestation_digests`, and `unrevoke_attestation_digest`.
    fn load_attestation_log(env: &Env) -> Vec<BytesN<32>> {
        env.storage()
            .instance()
            .get(&DataKey::AttestationAppendLog)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Assert that `index` falls within the current append-log bounds.
    ///
    /// Panics with [`EscrowError::AttestationIndexOutOfRange`] when `index >= log.len()`.
    /// Consolidates the identical range guard shared by `revoke_attestation_digest`,
    /// `revoke_attestation_digests`, and `unrevoke_attestation_digest`.
    fn require_attestation_index_in_range(env: &Env, log: &Vec<BytesN<32>>, index: u32) {
        ensure(
            env,
            index < log.len(),
            EscrowError::AttestationIndexOutOfRange,
        );
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Version).unwrap_or(0)
    }

    /// Get the optional funding deadline (ledger timestamp), returns None if not set.
    pub fn get_funding_deadline(env: Env) -> Option<u64> {
        env.storage().instance().get(&keys::funding_deadline())
    }

    /// Check if funding has expired (deadline set and now > deadline).
    pub fn is_funding_expired(env: Env) -> bool {
        if let Some(deadline) = env.storage().instance().get(&keys::funding_deadline()) {
            env.ledger().timestamp() > deadline
        } else {
            false
        }
    }

    /// Whether a compliance/legal hold is active (defaults to `false` if unset).
    pub fn get_legal_hold(env: Env) -> bool {
        Self::legal_hold_active(&env)
    }

    /// Read the operational pause flag; defaults to `false` when unset.
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
            .get(&keys::min_contribution_floor())
            .unwrap_or(0)
    }

    /// Current protocol fee in basis points (`0..=10_000`) applied to the SME disbursement at
    /// [`LiquifactEscrow::withdraw`]; `0` means no fee (full `funded_amount` goes to the SME).
    ///
    /// Reads `0` for instances predating [`DataKey::ProtocolFeeBps`] (additive-key default),
    /// matching legacy disbursement behavior. The current admin may update the value via
    /// [`LiquifactEscrow::set_protocol_fee_bps`].
    pub fn get_protocol_fee_bps(env: Env) -> i64 {
        env.storage()
            .instance()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0)
    }

    /// Admin-only setter for the protocol fee in basis points.
    ///
    /// Valid values are `0..=10_000`. Out-of-range values fail with
    /// [`EscrowError::ProtocolFeeBpsOutOfRange`]. The call requires the current escrow admin to
    /// authorize it and emits [`ProtocolFeeUpdated`] when the stored fee changes.
    pub fn set_protocol_fee_bps(env: Env, new_fee_bps: i64) -> i64 {
        let escrow = Self::load_escrow_require_admin(&env);

        ensure(
            &env,
            (0..=10_000).contains(&new_fee_bps),
            EscrowError::ProtocolFeeBpsOutOfRange,
        );

        let old_fee_bps: i64 = env
            .storage()
            .instance()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0);

        if new_fee_bps == old_fee_bps {
            return old_fee_bps;
        }

        env.storage()
            .instance()
            .set(&DataKey::ProtocolFeeBps, &new_fee_bps);

        let invoice_id = escrow.invoice_id.clone();
        ProtocolFeeUpdated {
            name: symbol_short!("fee_upd"),
            invoice_id: invoice_id.clone(),
            old_fee_bps,
            new_fee_bps,
        }
        .publish(&env);

        new_fee_bps
    }

    /// Optional cap on **distinct** investor addresses (`prev == 0` at fund time); [`None`] if unlimited.
    ///
    /// Reflects the current stored cap, including any admin reduction via
    /// [`LiquifactEscrow::lower_max_unique_investors`].
    pub fn get_max_unique_investors_cap(env: Env) -> Option<u32> {
        env.storage()
            .instance()
            .get(&keys::max_unique_investors_cap())
    }

    /// Optional cap on total principal for a single investor address.
    /// Absent ⇒ unlimited. Enforced on every deposit.
    pub fn get_max_per_investor_cap(env: Env) -> Option<i128> {
        env.storage().instance().get(&keys::max_per_investor_cap())
    }

    /// Distinct funders counted so far (each address counted once when it first receives principal).
    ///
    /// **Sybil:** this limits distinct **chain accounts**, not real-world persons; Sybil resistance is
    /// not a goal of this counter.
    pub fn get_unique_funder_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&keys::unique_funder_count())
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

        let mut log: Vec<BytesN<32>> = Self::load_attestation_log(&env);
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
        Self::load_attestation_log(&env)
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
            .get(&keys::investor_contribution(investor))
            .unwrap_or(0)
    }

    fn set_persistent_investor_contribution(env: &Env, investor: Address, amount: i128) {
        env.storage()
            .persistent()
            .set(&keys::investor_contribution(investor), &amount);
    }

    fn get_persistent_investor_effective_yield(env: &Env, investor: Address) -> Option<i64> {
        env.storage()
            .persistent()
            .get(&investor_effective_yield_key(&investor))
    }

    fn set_persistent_investor_effective_yield(env: &Env, investor: Address, value: i64) {
        env.storage()
            .persistent()
            .set(&investor_effective_yield_key(&investor), &value);
    }

    fn get_persistent_investor_claim_not_before(env: &Env, investor: Address) -> u64 {
        env.storage()
            .persistent()
            .get(&investor_claim_not_before_key(&investor))
            .unwrap_or(0)
    }

    fn set_persistent_investor_claim_not_before(env: &Env, investor: Address, value: u64) {
        env.storage()
            .persistent()
            .set(&investor_claim_not_before_key(&investor), &value);
    }

    fn get_persistent_investor_claimed(env: &Env, investor: Address) -> bool {
        env.storage()
            .persistent()
            .get(&keys::investor_claimed(investor))
            .unwrap_or(false)
    }

    fn set_persistent_investor_claimed(env: &Env, investor: Address, value: bool) {
        env.storage()
            .persistent()
            .set(&keys::investor_claimed(investor), &value);
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
            .get(&keys::investor_index())
            .unwrap_or_else(|| Vec::new(&env));

        let (start, end) =
            match Self::paginate_window(start, limit, MAX_INVESTOR_READ_BATCH, index.len()) {
                Some(w) => w,
                None => return Vec::new(&env),
            };

        let mut result = Vec::new(&env);
        for i in start..end {
            result.push_back(index.get(i).unwrap());
        }
        result
    }

    /// Enumerate all funding records (investor address + contribution amount) with pagination.
    ///
    /// Returns a paginated view of all investor funding records. Each record is a tuple of
    /// (investor address, principal contribution amount in base units of the funding token).
    /// The records are returned in the order they appear in the internal investor index.
    ///
    /// This is a read-only view with no state mutation. If zero funding records exist,
    /// or if `start` is beyond the last record, returns an empty vector.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `start` - Zero-based starting index for pagination.
    /// * `limit` - Maximum number of records to return.
    ///   If `limit` exceeds [`MAX_INVESTOR_READ_BATCH`] (50), it is silently clamped to the ceiling.
    ///
    /// # Returns
    /// A `Vec<(Address, i128)>` where each tuple is an investor address and their cumulative
    /// principal contribution. Returns an empty vector if:
    /// - No funding records exist (escrow has zero investors).
    /// - `start` is at or beyond the total record count.
    /// - `limit` is zero.
    ///
    /// # Pagination and Continuation
    /// To iterate through all records, the caller should:
    /// 1. Call with `start=0, limit=50` (or any value up to the ceiling).
    /// 2. The returned vector length (e.g., 50) indicates the number of records in this page.
    /// 3. Next call uses `start = previous_start + items_returned.len()`.
    /// 4. Stop when the returned vector is shorter than requested (indicates end of records)
    ///    or is empty.
    ///
    /// # Example
    /// ```ignore
    /// let mut start = 0;
    /// loop {
    ///     let page = LiquifactEscrow::get_funding_records(&env, start, 50);
    ///     if page.is_empty() {
    ///         break; // No more records
    ///     }
    ///     // Process page...
    ///     start += page.len() as u32;
    /// }
    /// ```
    pub fn get_funding_records(env: Env, start: u32, limit: u32) -> Vec<(Address, i128)> {
        let index: Vec<Address> = env
            .storage()
            .instance()
            .get(&keys::investor_index())
            .unwrap_or_else(|| Vec::new(&env));

        let len = index.len();
        if start >= len || limit == 0 {
            return Vec::new(&env);
        }

        let actual_limit = limit.min(MAX_INVESTOR_READ_BATCH);
        let end = (start + actual_limit).min(len);

        let mut result = Vec::new(&env);
        for i in start..end {
            let investor = index.get(i).unwrap();
            let contribution = Self::get_persistent_investor_contribution(&env, investor.clone());
            result.push_back((investor, contribution));
        }
        result
    }

    /// Pro-rata denominator captured when the escrow first became **funded**; [`None`] until then.
    ///
    /// The snapshot is write-once. It records the full `funded_amount` at the threshold-crossing
    /// funding call, including any over-funding past `funding_target`, plus the close ledger time
    /// and sequence used by off-chain auditors.
    pub fn get_funding_close_snapshot(env: Env) -> Option<FundingCloseSnapshot> {
        env.storage()
            .instance()
            .get(&keys::funding_close_snapshot())
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
            .get::<DataKey, Vec<YieldTier>>(&yield_tier_table_key())
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Returns a paginated view of the configured yield-tier ladder.
    ///
    /// Reads the same immutable table as [`LiquifactEscrow::get_yield_tiers`] and preserves
    /// the validated ordering enforced at `init`.
    ///
    /// # Arguments
    /// * `start` - The starting index (0-based) of the pagination.
    /// * `limit` - The maximum number of yield tiers to return (capped at [`MAX_INVESTOR_READ_BATCH`]).
    ///
    /// # Returns
    /// A `Vec<YieldTier>` containing the yield tiers within the requested page.
    pub fn get_yield_tiers_page(env: Env, start: u32, limit: u32) -> Vec<YieldTier> {
        let tiers = Self::get_yield_tiers(env.clone());
        let len = tiers.len();
        if start >= len || limit == 0 {
            return Vec::new(&env);
        }

        let actual_limit = limit.min(MAX_INVESTOR_READ_BATCH);
        let end = (start + actual_limit).min(len);

        let mut result = Vec::new(&env);
        for i in start..end {
            result.push_back(tiers.get(i).unwrap());
        }
        result
    }

    /// Admin-only setter that replaces the entire yield-tier ladder.
    ///
    /// Before this entrypoint existed the ladder was fixed at [`LiquifactEscrow::init`] with
    /// no update path, so correcting a mis-configured tier required redeploying the escrow.
    /// `set_yield_tiers` closes that gap while enforcing exactly the invariants `init`
    /// enforces, so no ladder reachable through this setter is unreachable through `init`.
    ///
    /// # Invariants
    ///
    /// 1. The table must be non-empty.
    /// 2. Every `yield_bps` must satisfy `0 <= yield_bps <= 10_000`.
    /// 3. `min_lock_secs` must be **strictly increasing** across tiers.
    /// 4. `yield_bps` must be **non-decreasing** across tiers.
    ///
    /// Validation runs over the whole table **before** any storage write, so a rejected
    /// call leaves the previously stored ladder byte-for-byte untouched. There is no
    /// partial application.
    ///
    /// # Authorization
    ///
    /// [`InvoiceEscrow::admin`], enforced via [`Self::load_escrow_require_admin`]. A caller
    /// without the admin authorization is rejected before any validation runs.
    ///
    /// # Errors
    ///
    /// - [`EscrowError::YieldTierTableInvalid`] (236) if any invariant above is violated.
    ///
    /// # Events
    ///
    /// Emits [`YieldTierTableUpdated`] carrying the new `tier_count` on success.
    pub fn set_yield_tiers(env: Env, tiers: Vec<YieldTier>) {
        let escrow = Self::load_escrow_require_admin(&env);

        let n = tiers.len();
        ensure(&env, n > 0, EscrowError::YieldTierTableInvalid);

        // `prev_lock` starts at 0 so the first tier must declare a positive lock, and
        // `prev_bps` starts at -1 so a first tier of 0 bps is accepted.
        let mut prev_lock: u64 = 0;
        let mut prev_bps: i64 = -1;

        for i in 0..n {
            let tier = tiers.get(i).unwrap();
            ensure(
                &env,
                tier.yield_bps >= 0,
                EscrowError::YieldTierTableInvalid,
            );
            ensure(
                &env,
                tier.yield_bps <= 10_000,
                EscrowError::YieldTierTableInvalid,
            );
            ensure(
                &env,
                tier.min_lock_secs > prev_lock,
                EscrowError::YieldTierTableInvalid,
            );
            ensure(
                &env,
                tier.yield_bps >= prev_bps,
                EscrowError::YieldTierTableInvalid,
            );
            prev_lock = tier.min_lock_secs;
            prev_bps = tier.yield_bps;
        }

        env.storage()
            .instance()
            .set(&DataKey::YieldTierTable, &tiers);

        YieldTierTableUpdated {
            name: symbol_short!("yt_upd"),
            invoice_id: escrow.invoice_id.clone(),
            tier_count: n,
        }
        .publish(&env);
    }

    /// Pure read — no auth, no storage writes, safe for simulation.
    ///
    /// Returns a [`YieldTierPreview`] with `{effective_yield_bps, matched_lock_secs}` for a
    /// hypothetical contribution of `amount` with `lock` seconds, using the **exact same
    /// tier-selection rule** applied at the first [`LiquifactEscrow::fund_with_commitment`]
    /// deposit.
    ///
    /// # Parameters
    ///
    /// - `amount: i128` — Hypothetical funding amount (currently unused; accepted for signature
    ///   parity with `fund_with_commitment()` for future extensibility).
    ///   - Valid range: any `i128` value (no validation applied; parameter unused)
    ///
    /// - `lock: u64` — Hypothetical lock commitment in seconds.
    ///   - Valid range: `0..=u64::MAX` (all u64 values safe; used in comparison only)
    ///   - `0` = no lock → returns base yield
    ///   - `> 0` = seconds; matched against tier `min_lock_secs` for highest-yield tier selection
    ///   - **Derivation**: Pure comparison logic `lock >= tier.min_lock_secs` is overflow-free
    ///
    /// # Returns
    ///
    /// Tuple `(effective_yield_bps, matched_lock_secs)`:
    /// - `effective_yield_bps`: The selected tier's yield, or base yield if no tier matches
    /// - `matched_lock_secs`: The matched tier's `min_lock_secs`, or `0` if no tier matched
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
    pub fn preview_yield_tier(env: Env, amount: i128, lock: u64) -> YieldResolution {
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
    }

    /// Admin-only setter for collateral commitment parameters.
    ///
    /// Updates the stored collateral commitment metadata. Validates bounds:
    /// - `asset` must be non-empty
    /// - `amount` must be positive and within configured bounds
    ///
    /// # Authorization
    /// Requires the signature of the current [`InvoiceEscrow::admin`].
    ///
    /// # Errors
    /// - [`EscrowError::CollateralAssetEmpty`] if `asset` is empty (code 61)
    /// - [`EscrowError::CollateralAmountNotPositive`] if `amount <= 0` (code 60)
    /// - [`EscrowError::CollateralAmountExceedsMax`] if `amount > max_allowed` (code 239)
    ///
    /// # Events
    /// Emits [`CollateralParametersUpdated`] on success.
    pub fn set_collateral_parameters(
        env: Env,
        asset: Symbol,
        amount: i128,
    ) -> SmeCollateralCommitment {
        let escrow = Self::load_escrow_require_admin(&env);

        // Validate asset is non-empty (uses existing error 61)
        ensure(
            &env,
            asset != Symbol::new(&env, ""),
            EscrowError::CollateralAssetEmpty,
        );

        // Validate amount is positive (uses existing error 60)
        ensure(&env, amount > 0, EscrowError::CollateralAmountNotPositive);

        // Validate amount doesn't exceed max (new error 239)
        let max_amount: i128 = 1_000_000_000_000_000; // Example max
        ensure(
            &env,
            amount <= max_amount,
            EscrowError::CollateralAmountExceedsMax,
        );

        let now = env.ledger().timestamp();
        let commitment = SmeCollateralCommitment {
            asset: asset.clone(),
            amount,
            recorded_at: now,
        };

        // Get prior amount for event
        let prior: Option<SmeCollateralCommitment> =
            env.storage().instance().get(&DataKey::SmeCollateralPledge);
        let prior_amount = prior.as_ref().map(|c| c.amount).unwrap_or(0);

        env.storage()
            .instance()
            .set(&DataKey::SmeCollateralPledge, &commitment);

        CollateralParametersUpdated {
            name: symbol_short!("coll_upd"),
            invoice_id: escrow.invoice_id.clone(),
            asset: asset.clone(),
            amount,
            prior_amount,
        }
        .publish(&env);

        commitment
    }

    /// Get the storage limit
    pub fn get_storage_limit(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::StorageLimit)
            .unwrap_or(INSTANCE_TTL_MIN_EXTENSION_LEDGERS)
    }

    /// Set the storage limit
    pub fn set_storage_limit(env: Env, new_limit: u32) -> u32 {
        let _escrow = Self::load_escrow_require_admin(&env);
        ensure(
            &env,
            (MIN_STORAGE_LIMIT_LEDGERS..=MAX_STORAGE_LIMIT_LEDGERS).contains(&new_limit),
            EscrowError::StorageLimitOutOfRange,
        );
        env.storage()
            .instance()
            .set(&DataKey::StorageLimit, &new_limit);
        new_limit
    }

    /// Bump TTL for storage entries
    pub fn bump_ttl_batch(env: Env, addresses: Vec<Address>) {
        let n = addresses.len();
        ensure(&env, n > 0, EscrowError::BumpTtlBatchEmpty);
        ensure(
            &env,
            n <= MAX_BUMP_TTL_BATCH,
            EscrowError::BumpTtlBatchTooLarge,
        );
        let limit = Self::get_storage_limit(env.clone());
        for i in 0..n {
            let addr = addresses.get(i).unwrap();
            // Bump persistent storage
            env.storage().persistent().extend_ttl(
                &DataKey::InvestorContribution(addr.clone()),
                limit,
                limit,
            );
            env.storage().persistent().extend_ttl(
                &investor_effective_yield_key(&addr),
                limit,
                limit,
            );
            env.storage().persistent().extend_ttl(
                &investor_claim_not_before_key(&addr),
                limit,
                limit,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::InvestorClaimed(addr.clone()),
                limit,
                limit,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::InvestorAllowlisted(addr.clone()),
                limit,
                limit,
            );
        }
        env.storage().instance().extend_ttl(limit, limit);
    }
} // End of impl LiquifactEscrow
