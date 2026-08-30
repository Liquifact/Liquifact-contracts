#![cfg_attr(not(test), no_std)]
//! LiquiFact Escrow Contract
//!
//! Holds investor funds for an invoice until settlement.
//! - SME receives stablecoin when funding target is met ([`LiquifactEscrow::withdraw`])
//! - SME records optional **collateral commitments** ([`LiquifactEscrow::record_sme_collateral_commitment`]) ΓÇö
//!   these are **ledger records only**; they do **not** move tokens, freeze balances,
//!   reserve assets, or create an enforceable on-chain claim.
//! - [`LiquifactEscrow::settle`] finalizes the escrow after maturity (when configured).
//!
//! ## Schema version ([`SCHEMA_VERSION`] / [`DataKey::Version`])
//!
//! The constant [`SCHEMA_VERSION`] is written to [`DataKey::Version`] by [`LiquifactEscrow::init`]
//! and is the canonical source of truth for upgrade decisions. **Current value: 6.**
//!
//! [`LiquifactEscrow::migrate`] **fails with typed errors in all current execution paths** ΓÇö no
//! silent migration work is promised or performed. Operators must extend `migrate` before calling
//! it, or redeploy when stored struct layout changes. See `docs/OPERATOR_RUNBOOK.md` for the full
//! decision tree.
//!
//! ## Event topic versioning ([`EVENT_SCHEMA_VERSION`])
//!
//! Every lifecycle event emitted by this contract includes a schema version
//! as the second topic element: `(event_name, EVENT_SCHEMA_VERSION, ...)`.
//! The version is set by the [`EVENT_SCHEMA_VERSION`] constant and lets
//! off-chain consumers detect breaking changes before parsing the payload.
//! The required payload fields for each event are stable within a schema
//! version; a bump to [`EVENT_SCHEMA_VERSION`] means new events or fields may
//! be present, and consumers should switch on this version explicitly.
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
//! `docs/escrow-security-checklist.md` ┬º6 and [ADR-002](docs/adr/ADR-002-auth-boundaries.md)):
//!
//! 1. **Read-only** preconditions (legal hold, status checks, input validation).
//! 2. **`Address::require_auth()`** for the bound role ([Stellar authorization](https://developers.stellar.org/docs/build/guides/auth/contract-authorization)).
//! 3. **Storage writes** and **SEP-41 transfers** (via [`external_calls`]).
//!
//! Invariant: no instance/persistent storage mutation and no token transfer occurs until
//! step 2 succeeds. Reading [`DataKey::Escrow`] before `require_auth` is intentional ΓÇö it is
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
//! ([`DataKey::RegistryRef`]) is a read-only discoverability hint only ΓÇö it is **not** an authority
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
//! boundary. This is meant for rounding residue / stray transfers, not for settling live liabilities ΓÇö
//! integrations that custody principal on-chain must keep token balances reconciled with
//! `funded_amount` so treasury sweeps cannot pull user funds.
//!
//! ## Ledger time trust model
//!
//! [`LiquifactEscrow::settle`] and [`LiquifactEscrow::claim_investor_payout`] compare against
//! [`Env::ledger`] timestamps only (no wall-clock oracle). Maturity, per-investor **claim locks**
//! from [`LiquifactEscrow::fund_with_commitment`], and [`FundingCloseSnapshot`] metadata must be
//! interpreted as **validator-observed ledger time**, including possible skew between simulated and
//! live networksΓÇöintegrators should treat boundaries as `>=` / `<` tests on integer seconds.
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
//! custodied on-chain and the SME calls [`LiquifactEscrow::withdraw`] ΓÇö this feature depends on
//! the on-chain disbursement path. It does **not** apply to off-chain settlement
//! ([`LiquifactEscrow::settle`]), investor refunds ([`LiquifactEscrow::refund`]), or investor
//! claims ([`LiquifactEscrow::claim_investor_payout`]). The treasury here is the same immutable
//! address used by [`LiquifactEscrow::sweep_terminal_dust`]; the fee transfer reuses the same
//! SEP-41 balance-deltaΓÇôchecked path in [`external_calls`].

#![allow(clippy::too_many_arguments)]

#[cfg(test)]
extern crate std;

use core::{clone::Clone, default::Default};
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error,
    symbol_short, token::TokenClient, Address, BytesN, Env, String, Symbol, Vec,
};

pub mod external_calls;
mod keys;

/// Upper bound on [`LiquifactEscrow::set_investors_allowlisted`] batch size per call.
pub const MAX_INVESTOR_ALLOWLIST_BATCH: u32 = 32;

/// Current storage schema version written to [`DataKey::Version`] by [`LiquifactEscrow::init`].
///
/// # Schema version changelog
///
/// | Version | Summary | Upgrade path |
/// |---------|---------|-------------|
/// | 1 | Initial schema (`InvoiceEscrow` v1, basic fund / settle) | N/A |
/// | 2 | Added `InvestorEffectiveYield`, `InvestorClaimNotBefore` | Additive keys ΓÇö no `migrate` call required |
/// | 3 | Added `FundingCloseSnapshot`, `MinContributionFloor`, `MaxUniqueInvestorsCap`, `UniqueFunderCount` | Additive keys ΓÇö old instances return defaults |
/// | 4 | Added `PrimaryAttestationHash`, `AttestationAppendLog` | Additive keys ΓÇö no `migrate` call required |
/// | 5 | Added `YieldTierTable`, `RegistryRef`, `Treasury`; `fund_with_commitment` | **Redeploy required** if `InvoiceEscrow` XDR changed |
/// | 6 | Per-investor keys moved to **persistent** storage (see ADR-007) | **Redeploy required** ΓÇö no `migrate` path (addresses not enumerable) |
///
/// See `docs/OPERATOR_RUNBOOK.md` for the full redeploy-vs-upgrade decision tree.
pub const SCHEMA_VERSION: u32 = 6;

/// Explicit legacy version before version markers were introduced.
pub const LEGACY_VERSION: u32 = 5;
// See the schema version contract documentation: [Escrow schema versioning](../docs/escrow-schema-versioning.md)

/// Version of the lifecycle event topics emitted by this contract.
///
/// Every escrow lifecycle event topic is emitted with this version as the
/// second topic element: `(event_name, EVENT_SCHEMA_VERSION, ...)`. This
/// provides a compatibility signal for off-chain consumers so they can
/// distinguish between event payload/topic schema versions.
///
/// Bump this constant when a lifecycle event topic or required payload field
/// changes in a way that is not backward compatible. Do not bump it for
/// additive fields that keep old fields stable.
pub const EVENT_SCHEMA_VERSION: u32 = 1;

/// Upper bound on [`LiquifactEscrow::append_attestation_digest`] entries to keep storage bounded.
/// Revocation via [`LiquifactEscrow::revoke_attestation_digest`] does not consume a slot.
pub const MAX_ATTESTATION_APPEND_ENTRIES: u32 = 32;

/// Maximum number of indices that can be revoked in a single batch call.
pub const MAX_ATTESTATION_REVOKE_BATCH: u32 = 32;

/// Upper bound on [`LiquifactEscrow::batch_bump_ttl`] entries per call.
///
/// Mirrors [`MAX_INVESTOR_ALLOWLIST_BATCH`] ΓÇö both operations iterate over a
/// bounded address list touching persistent storage once per entry. 32 entries keeps
/// per-call CPU/storage work predictable and consistent with the rest of the
/// admin-batch API surface.
pub const MAX_BUMP_TTL_BATCH: u32 = 32;

/// Errors specific to escrow close finalization.
#[contracterror]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseError {
    /// The caller is not the configured admin.
    NotAuthorized = 0,
    /// The escrow was not initialized.
    NotInitialized = 1,
    /// The escrow has already been closed.
    AlreadyClosed = 2,
    /// The escrow still holds a token balance.
    ActiveBalance = 3,
    /// The escrow has an active dispute.
    ActiveDispute = 4,
}

/// Metadata captured when an escrow is finalized.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloseMetadata {
    pub admin: Address,
    pub timestamp: u64,
    pub sequence: u32,
}

/// Event emitted when an escrow is closed.
#[contractevent]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloseFinalizedEvt {
    #[topic]
    pub name: Symbol,
    pub metadata: CloseMetadata,
}

/// Storage key that marks the escrow as closed (one-shot flag).
const CLOSED_KEY: &str = "EscrowClosed";
/// Storage key that holds close metadata.
const CLOSE_METADATA_KEY: &str = "CloseMetadata";

#[contractimpl]
impl LiquifactEscrow {
    /// Finalizes the escrow after all balance and dispute obligations have settled.
    ///
    /// # Preconditions
    /// - Only the current escrow admin may close.
    /// - The escrow's funding-token balance must be zero.
    /// - There must be no active dispute.
    /// - The escrow must not already be closed.
    ///
    /// # Effects
    /// - Marks the escrow as closed (one-shot).
    /// - Stores [`CloseMetadata`].
    /// - Emits a [`CloseFinalizedEvt`].
    pub fn close_escrow(env: Env) {
        let escrow: InvoiceEscrow = env
            .storage()
            .instance()
            .get(&DataKey::Escrow)
            .unwrap_or_else(|| panic_with_error!(&env, CloseError::NotInitialized));
        let admin = escrow.admin;
        admin.require_auth();

        if env.storage().instance().has(&Symbol::new(&env, CLOSED_KEY)) {
            panic_with_error!(&env, CloseError::AlreadyClosed);
        }

        let funding_token: Address = env
            .storage()
            .instance()
            .get(&DataKey::FundingToken)
            .unwrap_or_else(|| panic_with_error!(&env, CloseError::NotInitialized));
        let token = TokenClient::new(&env, &funding_token);
        let balance = token.balance(&env.current_contract_address());
        if balance > 0 {
            panic_with_error!(&env, CloseError::ActiveBalance);
        }

        if env.storage().instance().get(&DataKey::Dispute).unwrap_or(false) {
            panic_with_error!(&env, CloseError::ActiveDispute);
        }

        let metadata = CloseMetadata {
            admin: admin.clone(),
            timestamp: env.ledger().timestamp(),
            sequence: env.ledger().sequence(),
        };

        env.storage()
            .instance()
            .set(&Symbol::new(&env, CLOSED_KEY), &true);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, CLOSE_METADATA_KEY), &metadata);

        CloseFinalizedEvt {
            name: symbol_short!("close"),
            metadata: metadata.clone(),
        }
        .publish(&env);
    }

    /// Returns the close metadata if the escrow has been closed.
    pub fn get_closure_metadata(env: Env) -> Option<CloseMetadata> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, CLOSE_METADATA_KEY))
    }

    /// Toggles the dispute active flag. Bumps TTL by the disputed threshold.
    pub fn set_dispute_active(env: Env, active: bool) {
        let mut escrow: InvoiceEscrow = env.storage().instance().get(&DataKey::Escrow)
            .unwrap_or_else(|| panic_with_error!(&env, CloseError::NotInitialized));
        escrow.admin.require_auth();
        escrow.dispute_active = active;
        env.storage().instance().set(&DataKey::Escrow, &escrow);
        extend_ttl_for_activity(&env, &escrow, None);
    }
}

/// Default maximum maturity horizon in seconds (~5 years) when no explicit horizon is configured.
pub const DEFAULT_MATURITY_MAX_HORIZON_SECS: u64 = 157_680_000; // ~5 years (365.25 * 24 * 3600 * 5)

// ---------------------------------------------------------------------------
// Bounded fee schedule
// ---------------------------------------------------------------------------

/// A fee schedule with named bounds and a future ledger at which it becomes active.
///
/// `fee_bps` is the actual fee in basis points. `min_fee_bps` and `max_fee_bps`
/// are the named lower/upper bounds for the schedule; they are validated by
/// [`LiquifactEscrow::submit_fee_schedule`] and are exposed for off-chain audit.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeSchedule {
    pub fee_bps: u32,
    pub min_fee_bps: u32,
    pub max_fee_bps: u32,
    pub activation_ledger: u32,
}

/// Storage keys for the fee-schedule activation ledger.
///
/// These are deliberately separate from [`DataKey`] so existing escrow storage is
/// untouched. The active and pending keys are the source of truth for reads; the
/// previous key is updated when a pending schedule activates.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FeeScheduleStorageKey {
    Active,
    Pending,
    Previous,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum FeeScheduleError {
    FeeOutOfBounds = 1,
    InvalidActivationLedger = 2,
    PendingScheduleExists = 3,
    NotInitialized = 4,
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
/// coupon       = total_principal ├ù yield_bps / 10_000  (floor)   (1)
/// settle_pool  = total_principal + coupon                        (2)
/// gross_payout = contribution ├ù settle_pool / total_principal    (3)
/// ```
///
/// Each step uses `checked_*` arithmetic on `i128`. We need the tightest
/// bound that keeps all three steps overflow-free for every valid
/// `yield_bps Γêê [0, 10_000]` and every `contribution Γêê (0, total_principal]`.
///
/// Setting `MAX_INVOICE_AMOUNT = i128::MAX / 10_000` is sufficient because it implies:
/// - `amount ├ù 10_000 Γëñ i128::MAX`
/// - `2 ├ù amount Γëñ 2 ├ù (i128::MAX / 10_000) < i128::MAX` for 10_000-bps coupon,
///   and all intermediate `checked_*` operations are overflow-free by construction.
pub const MAX_INVOICE_AMOUNT: i128 = i128::MAX / 10_000;

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

/// Hard ceiling on the number of distinct investors appended to [`DataKey::InvestorIndex`],
/// enforced **unconditionally** in [`LiquifactEscrow::fund_impl`] for every new contributor,
/// independent of the optional `max_unique_investors` init cap.
///
/// # Why this exists (issue #1229 — worst-case release instruction budget)
///
/// The escrow's per-investor release/accounting work (`settle` batch, `claim_investor_payout`,
/// `refund`, and the paginated [`LiquifactEscrow::get_funding_records`] /
/// [`LiquifactEscrow::get_investors`] views) scales with the participant count, and every
/// paginated view must deserialize the **entire** [`DataKey::InvestorIndex`] on each page
/// (O(n) memory/CPU). Without `max_unique_investors` configured at init the index grows
/// monotonically and unbounded. This ceiling bounds the worst-case release cost so it stays
/// within the Soroban host instruction/memory budget at production scale. It is purely an
/// input bound: storage layout, error semantics, auth boundaries, and event behavior are
/// unchanged for any escrow below the cap.
///
/// The value (`10_000`) is far above any soroban-storage-feasible escrow while still being a
/// finite, documented bound that keeps the worst-case `InvestorIndex` deserialize under the
/// default host budget.
pub const MAX_UNIQUE_INVESTORS: u32 = 10_000;

/// Documented worst-case **release-path** resource ceilings (issue #1229).
///
/// These are the per-top-level-invocation CPU-instruction and memory-bytes budgets that every
/// release-path call is measured against in `release_budget_tests` and must never exceed:
///
/// - [`LiquifactEscrow::settle`] / [`LiquifactEscrow::claim_investor_payout`] (per-investor
///   release), and
/// - one paginated page of [`LiquifactEscrow::get_funding_records`] /
///   [`LiquifactEscrow::get_investors`] at the hard-capped participant ceiling
///   [`MAX_UNIQUE_INVESTORS`] (the view that deserializes the full [`DataKey::InvestorIndex`]).
///
/// They sit far below the Stellar mainnet per-invocation limits (600M instructions /
/// ~42 MB memory in [`soroban_sdk::testutils::NetworkInvocationResourceLimits::mainnet`])
/// so the bounded release path remains executable at production scale. These are **budget
/// regression gates**: any change that pushes a release-path call past the ceiling fails the
/// suite and must be refactored rather than raised.
///
/// Measuring native Rust runs underestimates the equivalent WASM cost, so the ceiling is
/// deliberately loose (documented, conservative) rather than an attempt at a hard WASM budget.
pub const WORST_CASE_RELEASE_CPU_INSNS_CEILING: u64 = 20_000_000;
pub const WORST_CASE_RELEASE_MEM_BYTES_CEILING: u64 = 8_000_000;

/// Upper bound on [`LiquifactEscrow::record_sme_collateral_commitment_batch`] entries.
pub const MAX_COLLATERAL_BATCH: u32 = 50;

/// Upper bound on attestation digest read page size.
pub const MAX_ATTESTATION_READ_PAGE: u32 = 20;

/// Upper bound on [`LiquifactEscrow::sweep_terminal_dust`] per call (base units of the funding token).
///
/// Caps blast radius if instrumentation mis-estimates ΓÇ£dustΓÇ¥; tune per asset decimals off-chain.
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
/// [`DataKey::StorageLimit`] is unset ΓÇö preserving pre-configurable behaviour.
pub const INSTANCE_TTL_MIN_EXTENSION_LEDGERS: u32 = 60 * 60; // Approx. 1h at 1 ledger/sec.

/// Minimum persistent storage TTL extension horizon for per-investor allowlist entries.
///
/// When the escrow uses the allowlist gate, investor funding depends on persistent entries.
/// Extending persistent allowlist TTL reduces the risk of silent allowlist disablement.
///
/// When [`DataKey::StorageLimit`] is unset, persistent extensions also fall back to
/// [`INSTANCE_TTL_MIN_EXTENSION_LEDGERS`] (equal to this constant today).
pub const PERSISTENT_TTL_MIN_EXTENSION_LEDGERS: u32 = 60 * 60; // Approx. 1h at 1 ledger/sec.

pub const TTL_ACTIVE_ESCROW_LEDGERS: u32 = 90 * 24 * 60 * 60;
pub const TTL_DISPUTED_ESCROW_LEDGERS: u32 = 180 * 24 * 60 * 60;
pub const TTL_TERMINAL_ESCROW_LEDGERS: u32 = 30 * 24 * 60 * 60;

pub(crate) fn get_lifecycle_ttl(escrow: &InvoiceEscrow) -> u32 {
    if escrow.dispute_active {
        TTL_DISPUTED_ESCROW_LEDGERS
    } else if escrow.status >= 2 {
        TTL_TERMINAL_ESCROW_LEDGERS
    } else {
        TTL_ACTIVE_ESCROW_LEDGERS
    }
}

pub(crate) fn extend_ttl_for_activity(env: &Env, escrow: &InvoiceEscrow, investor: Option<Address>) {
    let ttl = get_lifecycle_ttl(escrow);
    env.storage().instance().extend_ttl(ttl, ttl);
    if let Some(addr) = investor {
        let keys = [
            DataKey::InvestorContribution(addr.clone()),
            DataKey::InvestorEffectiveYield(addr.clone()),
            DataKey::InvestorClaimNotBefore(addr.clone()),
            DataKey::InvestorClaimed(addr.clone()),
            DataKey::InvestorAllowlisted(addr),
        ];
        for k in keys.iter() {
            if env.storage().persistent().has(k) {
                env.storage().persistent().extend_ttl(k, ttl, ttl);
            }
        }
    }
}

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
    ///
    /// Returned for every second initialization attempt ΓÇö same parameters, a different
    /// admin, a different token, or a re-entrant initialization during `init` ΓÇö before
    /// any state mutation or event emission. Existing admin, token metadata, and escrow
    /// state are left unchanged.
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
    /// [`LiquifactEscrow::clear_sme_collateral_commitment`] called when no pledge exists.
    NoCollateralToClear = 63,

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
    /// Storage lacks a version marker and does not match the known legacy layout.
    AmbiguousLegacyStorage = 93,

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
    /// **Code:** `108` ΓÇö stable, append-only.
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

    /// Attempted to accept admin role when no pending admin exists.
    /// @dev Historical note: Prior to PR #XYZ, this shared discriminant 163 with `FundingDeadlinePassed`.
    /// Reassigned to 81 to maintain uniqueness within the admin-handover range.
    NoPendingAdmin = 81,
    /// Admin-nonce replay protection: the supplied nonce does not match the current expected nonce.
    /// Returned for stale (old), duplicate (same), or future (out-of-sequence) nonces.
    /// Does not leak which specific mismatch occurred to avoid giving attackers information.
    AdminNonceMismatch = 85,
    /// The contract's funding-token balance is less than `funded_amount` at withdraw time.
    /// Funds must be custodied in this contract before the SME can pull them.
    InsufficientContractBalance = 165,

    /// [`validate_maturity_bounds`] rejected a maturity timestamp in the past.
    MaturityInPast = 166,
    /// [`validate_maturity_bounds`] rejected a maturity timestamp beyond the configured horizon.
    MaturityExceedsMaxHorizon = 167,
    /// [`LiquifactEscrow::revoke_attestation_digest`] called on a non-revoked index.
    AttestationNotRevoked = 168,
    /// [`LiquifactEscrow::update_funding_deadline`] called while escrow is not open.
    FundingDeadlineUpdateNotOpen = 169,
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
    /// A second [`LiquifactEscrow::settle`] (or [`LiquifactEscrow::settle_batch`] entry)
    /// was attempted on an escrow that already reached **settled** status (`status == 2`).
    ///
    /// Settlement is strictly once-only: the settled marker is committed before any outward
    /// effect, so a re-entrant or replayed call is rejected here with a dedicated, stable
    /// typed code rather than a misleading `SettlementNotFunded`.
    EscrowAlreadySettled = 236,
    /// A dispute is active and blocks value release from the escrow.
    DisputeBlocksWithdrawal = 237,
    /// Settlement is blocked while a dispute remains active.
    DisputeBlocksSettlement = 238,
    /// Investor claims are blocked while a dispute remains active.
    DisputeBlocksInvestorClaims = 239,
    /// Partial-settlement is blocked while a dispute remains active.
    DisputeBlocksPartialSettle = 240,
    /// Refund processing is blocked while a dispute remains active.
    DisputeBlocksRefund = 241,
    /// Unfunding is blocked while a dispute remains active.
    DisputeBlocksUnfund = 242,
    /// Terminal dust sweep is blocked while a dispute remains active.
    DisputeBlocksSweep = 243,
    /// The caller is not authorized to open or close a dispute for this escrow.
    DisputeOpenUnauthorized = 244,
    /// The caller is not authorized to close the active dispute.
    DisputeCloseUnauthorized = 245,
    /// A dispute has already been opened and is still active.
    DisputeAlreadyOpen = 246,
    /// No dispute is active for this escrow.
    DisputeNotOpen = 247,

    /// [`LiquifactEscrow::execute_callback`] called from an origin address different from the registered origin context.
    CallbackWrongOrigin = 240,
    /// [`LiquifactEscrow::execute_callback`] called with an invocation nonce that does not match the stored context.
    CallbackWrongNonce = 241,
    /// [`LiquifactEscrow::execute_callback`] called with a lifecycle phase different from the expected phase.
    CallbackWrongPhase = 242,
    /// [`LiquifactEscrow::execute_callback`] called with a callback context that has already been consumed (replay attempt).
    CallbackReplayed = 243,
    /// [`LiquifactEscrow::execute_callback`] or [`LiquifactEscrow::register_callback`] called after the escrow has been cancelled.
    CallbackAfterCancellation = 244,
    /// [`LiquifactEscrow::execute_callback`] called with a nonce that has no registered callback context.
    CallbackNotFound = 245,
    /// [`LiquifactEscrow::rebind_registry`] called when escrow status is no longer open
    /// (status != 0). The registry hint becomes immutable once funding/settlement begins.
    RegistryImmutableAfterFunding = 246,
    /// [`LiquifactEscrow::rotate_beneficiary`] called when escrow status is no longer
    /// pre-settlement (status must be 0 = open or 1 = funded). Beneficiary is immutable after
    /// funding closes.
    BeneficiaryImmutableAfterFunding = 247,
    /// [`LiquifactEscrow::execute_admin_recovery`] called before the pending admin proposal
    /// timelock (`DataKey::PendingAdminExpiry`) has elapsed. Recovery is only available
    /// after the abandoned-transfer expiry window passes.
    AdminRecoveryNotExpired = 248,
    /// [`LiquifactEscrow::fund_impl`] rejected a new distinct investor because the
    /// unconditional ceiling [`MAX_UNIQUE_INVESTORS`] (issue #1229) would be exceeded.
    /// This bounds the worst-case release instruction budget that scales with participant
    /// count when `max_unique_investors` was not configured at init.
    UniqueInvestorHardCapReached = 249,
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

/// Reject `amount` if it cannot be represented exactly at `token_decimals` decimal places.
///
/// A scale of 0 means the token has no fractional digits; every integer amount is valid.
/// Uses `checked_pow` and `checked_rem` to guard against overflow.
/// No implicit rounding: callers must round to the token's native scale before calling fund entrypoints.
///
/// # Errors
/// Panics with [`EscrowError::FundingTokenScaleInvalid`] when `amount % 10^token_decimals != 0`.
pub(crate) fn validate_decimal_scale(env: &Env, amount: i128, token_decimals: u32) {
    if token_decimals == 0 {
        return;
    }
    let divisor: i128 = match (10i128).checked_pow(token_decimals) {
        Some(d) => d,
        None => fail(env, EscrowError::FundingTokenScaleInvalid),
    };
    let remainder = match amount.checked_rem(divisor) {
        Some(r) => r,
        None => fail(env, EscrowError::FundingTokenScaleInvalid),
    };
    ensure(env, remainder == 0, EscrowError::FundingTokenScaleInvalid);
}

/// Reject any initialization attempt when the contract is already initialized or an
/// initialization is in progress.
///
/// This is the single guard for [`LiquifactEscrow::init`]. It checks both the escrow
/// snapshot and the schema-version marker so a partially failed first initialization
/// cannot be overwritten. `init` must call this before any authorization, validation,
/// storage write, or event emission.
#[allow(dead_code)]
#[inline(always)]
pub(crate) fn ensure_not_initialized(env: &Env) {
    ensure(
        env,
        !(env.storage().instance().has(&DataKey::Escrow)
            || env.storage().instance().has(&DataKey::Version)),
        EscrowError::EscrowAlreadyInitialized,
    );
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
/// Every entrypoint that accepts new principal ΓÇö [`LiquifactEscrow::fund`],
/// [`LiquifactEscrow::fund_with_commitment`], [`LiquifactEscrow::fund_batch`],
/// [`LiquifactEscrow::update_funding_target`], [`LiquifactEscrow::lower_max_unique_investors`],
/// and [`LiquifactEscrow::lower_min_contribution_floor`] ΓÇö must call this helper instead of
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
/// appeared at every risk-bearing entrypoint ΓÇö `sweep_terminal_dust`, `rotate_beneficiary`,
/// `fund_impl`, `partial_settle`, `settle`, `withdraw`, `claim_investor_payout`, and
/// `cancel_funding`. By centralising the read of [`DataKey::LegalHold`] and the negation we
/// guarantee that adding a new risk-bearing entrypoint cannot accidentally forget the hold
/// check or pick the wrong `LegalHoldBlocks*` variant ΓÇö the caller passes the typed error
/// variant that documents which entrypoint was blocked.
///
/// Operational pause guard: asserts that the operational pause does **not** block the
/// entrypoint family `entry`.
///
/// Replaces the repeated inline pattern `ensure(&env, !Self::paused_blocks(&env, entry), EscrowError::PausedBlocks*)`
/// that previously appeared at risk-bearing entrypoints ΓÇö `fund_impl`, `settle`, `withdraw`, and
/// `claim_investor_payout`. The caller passes the [`PauseEntry`] its entrypoint belongs to so an
/// active scoped pause ([`PauseScope`]) that only targets a different flow does **not** block it.
///
/// # Errors
/// Panics with the caller-supplied `error` (one of the `EscrowError::PausedBlocks*`
/// variants) when the active pause's [`PauseScope`] blocks `entry`.
///
/// # Security notes
/// - Read-only: performs instance-storage reads with `unwrap_or` defaults (no panic on
///   missing keys). Does not write or delete any storage key.
/// - This helper is **not** an authorization check. Callers must still call
///   `Address::require_auth()` for the entrypoint's bound role before any storage mutation
///   or token transfer, per [ADR-002](docs/adr/ADR-002-auth-boundaries.md).
/// - The pause is independent of the compliance legal hold ([`DataKey::LegalHold`]); an
///   entrypoint that needs both gates must compose `guard_not_paused` with `guard_not_legal_hold`.
#[inline(always)]
pub(crate) fn guard_not_paused(env: &Env, error: EscrowError, entry: PauseEntry) {
    ensure(env, !LiquifactEscrow::paused_blocks(env, entry), error);
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

/// Shared guard: assert that no dispute is currently active.
///
/// Disputes are a freeze on value release: while an active dispute remains in storage,
/// any release path that would transfer principal out of the contract must fail before
/// a transfer or state transition is applied.
#[inline(always)]
pub(crate) fn guard_not_disputed(env: &Env, error: EscrowError) {
    ensure(env, !LiquifactEscrow::is_dispute_active(env.clone()), error);
}

/// Predicate: `true` when `status` is one of the **terminal** escrow states
/// (`2` = settled, `3` = withdrawn, `4` = cancelled).
///
/// Used to gate entries that only make sense after the escrow has reached a final
/// disposition ΓÇö e.g. [`LiquifactEscrow::sweep_terminal_dust`], which sweeps
/// rounding-residue / stray-transfer balances only in terminal states, or liability-floor
/// checks that must only run when no further principal inbound is possible.
///
/// Centralising this predicate keeps the `settled | withdrawn | cancelled` set definitionally
/// identical across every call site ΓÇö adding a new status code (e.g. a future
/// `claimed` state) only requires editing this helper and a single call-site comment.
///
/// # Notes
/// Pure function: no storage access, no token interaction. Safe to call from
/// any context where a `status: u32` value is in hand (entrypoint, view function, test).
///
/// # Security notes
/// This is a **predicate**, not a guard ΓÇö callers that need to *enforce* the terminal
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
/// finalised ΓÇö e.g. [`LiquifactEscrow::rotate_beneficiary`], which lets the SME/admin
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
///
/// Used by the (currently disabled) `tests` integration tree; kept on the release path even
/// though no active caller references it yet, so the predicate stays in sync with status enum.
#[allow(dead_code)]
#[inline(always)]
#[allow(dead_code)]
pub(crate) fn is_pre_settlement_status(status: u32) -> bool {
    matches!(status, 0 | 1)
}

/// `true` when `maturity == 0` (no maturity lock ΓÇö vacuously reached) or the current
/// ledger timestamp is `>= maturity` (inclusive boundary). Mirrors the settlement
/// maturity gate used by [`LiquifactEscrow::settle`] and
/// [`LiquifactEscrow::get_settlement_readiness`].
#[allow(dead_code)] // exercised by the integration test tree only
#[inline(always)]
pub(crate) fn is_maturity_reached(env: &Env, maturity: u64) -> bool {
    maturity == 0 || env.ledger().timestamp() >= maturity
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
/// a key behave as ΓÇ£unset / defaultΓÇ¥ without panicking.
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
    /// **Persistent** storage. Absent ΓçÆ `0`. One entry per investor address.
    InvestorContribution(Address),
    /// When true, compliance/legal hold blocks payouts and settlement finalization.
    /// Absent ΓçÆ `false` (no hold). Toggled by admin via [`LiquifactEscrow::set_legal_hold`].
    LegalHold,
    /// Active dispute freeze: no value release is permitted while a dispute remains open.
    /// Absent ΓçÆ `false` (no active dispute).
    DisputeActive,
    /// Persisted dispute record; kept after resolution to preserve the incident timeline.
    /// Absent ΓçÆ no dispute has been opened.
    DisputeRecord,
    /// Optional minimum ledger timestamp when `LegalHold` may be cleared after a
    /// [`LiquifactEscrow::request_clear_legal_hold`] call.
    /// Absent ΓçÆ no clear request is pending.
    LegalHoldClearableAt,
    /// Configured minimum delay between [`LiquifactEscrow::request_clear_legal_hold`] and
    /// [`LiquifactEscrow::set_legal_hold(env, false)`]. Absent ΓçÆ `0`.
    LegalHoldClearDelay,
    /// Optional SME collateral commitment metadata (record-only ΓÇö not an on-chain asset lock).
    /// Absent when no commitment has been recorded. Replaceable by the SME.
    SmeCollateralPledge,
    /// Set to `true` when an investor has exercised a claim after settlement.
    /// **Persistent** storage. Absent ΓçÆ `false`. Written once; a second claim returns without re-emitting.
    InvestorClaimed(Address),
    /// SEP-41 funding asset for this invoice instance; set once in [`LiquifactEscrow::init`].
    /// Immutable after init.
    FundingToken,
    /// Protocol treasury that may receive [`LiquifactEscrow::sweep_terminal_dust`]; set once in init.
    /// Immutable after init.
    Treasury,
    /// Optional registry contract id for indexers; **hint only**, not authority (see module rustdoc).
    /// Omitted from storage when unset at init. Absent ΓçÆ `None`.
    RegistryRef,
    /// Immutable tier table when configured at [`LiquifactEscrow::init`]; omitted when tiering is off.
    /// Absent ΓçÆ no tiering (base `yield_bps` applies to all investors).
    /// **Trust:** values are protocol-supplied at deploy; the contract never mutates this key after init.
    YieldTierTable,
    /// Set once when status first becomes **funded** (1); immutable thereafter (pro-rata denominator).
    /// Absent until the escrow reaches `status == 1`. See [`FundingCloseSnapshot`].
    FundingCloseSnapshot,
    /// Effective annualized yield in bps chosen at this investorΓÇÖs **first** deposit (see tiered yield).
    /// **Persistent** storage. Absent ΓçÆ falls back to [`InvoiceEscrow::yield_bps`]. One entry per investor address.
    InvestorEffectiveYield(Address),
    /// Minimum [`Env::ledger`] timestamp before [`LiquifactEscrow::claim_investor_payout`] (0 = no extra gate).
    /// **Persistent** storage. Absent ΓçÆ `0`. One entry per investor address; set on first deposit.
    InvestorClaimNotBefore(Address),
    /// Minimum [`LiquifactEscrow::fund`] / [`LiquifactEscrow::fund_with_commitment`] amount per call (0 = no floor).
    /// Written as `0` even when unconfigured so reads always succeed.
    MinContributionFloor,
    /// When set at [`LiquifactEscrow::init`], caps distinct investor addresses that may contribute.
    /// Absent ΓçÆ unlimited. Checked against [`DataKey::UniqueFunderCount`] on each new investor.
    MaxUniqueInvestorsCap,
    /// Optional immutable per-investor cap on total principal credited to a single address.
    /// Absent ΓçÆ unlimited. Checked against [`DataKey::InvestorContribution`] on every deposit.
    MaxPerInvestorCap,
    /// Proposed successor admin waiting for [`LiquifactEscrow::accept_admin`].
    /// Absent ΓçÆ no pending handover. Cleared after successful acceptance.
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
    /// Absent ΓçÆ empty log. See [`LiquifactEscrow::append_attestation_digest`].
    AttestationAppendLog,
    /// Per-index revocation marker for [`DataKey::AttestationAppendLog`] entries.
    /// Absent ΓçÆ not revoked. Written as `true` by [`LiquifactEscrow::revoke_attestation_digest`].
    /// Preserves the original digest for auditability while signalling supersession.
    AttestationRevoked(u32),
    /// When true, only allowlisted addresses may call [`LiquifactEscrow::fund`] or [`LiquifactEscrow::fund_with_commitment`].
    AllowlistActive,
    /// Whether a specific address is permitted to fund when [`DataKey::AllowlistActive`] is true.
    InvestorAllowlisted(Address),
    /// Index of allowlisted addresses for paginated enumeration.
    AllowlistIndex,
    /// Set to `true` once an investor's principal has been refunded in a cancelled escrow.
    /// Absent ΓçÆ `false`. Written once; prevents double-refund.
    InvestorRefunded(Address),
    /// Running total of principal already returned to investors via [`LiquifactEscrow::refund`].
    /// Absent ΓçÆ `0`. Incremented atomically with each successful refund transfer.
    /// Used by [`LiquifactEscrow::sweep_terminal_dust`] to compute outstanding liabilities:
    /// `outstanding = funded_amount - distributed_principal`.
    DistributedPrincipal,
    /// Configured maximum maturity horizon in seconds from current ledger time.
    /// Absent ΓçÆ falls back to [`DEFAULT_MATURITY_MAX_HORIZON_SECS`].
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
    /// Monotonically increasing invocation nonce counter for cross-contract callbacks.
    /// Absent ⇒ `0`. Incremented on each callback registration.
    CallbackNonce,
    /// Stored cross-contract callback context ([`CallbackContext`]) keyed by invocation nonce.
    /// Binds expected origin address, invocation nonce, and lifecycle phase.
    CallbackContext(u64),
    /// When true, the escrow has an active dispute that blocks close finalization.
    /// Absent ⇒ `false` (no dispute). **Additive key (ADR-007):** absent on legacy instances
    /// reads as `false`. Written by the dispute lifecycle (admin/off-chain) and checked by
    /// [`LiquifactEscrow::close_escrow`].
    Dispute,
}

// --- Data types ---

/// The class of flows a scoped operational pause can block.
///
/// The values map 1:1 onto the four pause-gated entrypoint families plus a
/// catch-all. This is the **stored** scope value written by
/// [`LiquifactEscrow::set_paused`] and surfaced by
/// [`LiquifactEscrow::get_pause_state`].
///
/// # Semantics
/// - An active pause stores exactly **one** `PauseScope`. `Funding` blocks only
///   `fund` / `fund_with_commitment` / `fund_batch`; `Settlement`, `Withdrawal`,
///   and `Claims` block their single entrypoint family; `All` blocks every gated
///   entrypoint.
/// - Clearing a pause requires the matching scope (see [`LiquifactEscrow::set_paused`]).
///   Clearing with [`PauseScope::All`] clears whatever scope is currently active.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PauseScope {
    Funding = 0,
    Settlement = 1,
    Withdrawal = 2,
    Claims = 3,
    All = 4,
}

/// The family of pause-gated operations an entrypoint belongs to. Internal only ΓÇö
/// used to decide, at each entrypoint, whether the active [`PauseScope`] blocks it.
/// This is **not** stored and never crosses the contract boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub(crate) enum PauseEntry {
    Funding = 0,
    Settlement = 1,
    Withdrawal = 2,
    Claims = 3,
}

impl PauseScope {
    /// Whether an active pause with this scope blocks an operation from `entry`.
    ///
    /// [`PauseScope::All`] blocks every entry; otherwise the pause blocks only the
    /// entry whose family matches its scope.
    pub(crate) fn blocks(self, entry: PauseEntry) -> bool {
        matches!(
            (self, entry),
            (PauseScope::All, _)
                | (PauseScope::Funding, PauseEntry::Funding)
                | (PauseScope::Settlement, PauseEntry::Settlement)
                | (PauseScope::Withdrawal, PauseEntry::Withdrawal)
                | (PauseScope::Claims, PauseEntry::Claims)
        )
    }
}

/// The typed, stable reason an operational pause was applied.
///
/// Reasons are protocol-constants (not free-form strings) so that off-chain
/// consumers and dashboards can branch on a small, stable set. They are recorded
/// in [`PauseState::reason`] and surfaced via
/// [`LiquifactEscrow::get_pause_state`].
///
/// The compliance/legal hold is a separate mechanism ([`DataKey::LegalHold`]) and
/// is intentionally **not** represented here ΓÇö a pause reason is operational only.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PauseReason {
    Maintenance = 0,
    Incident = 1,
    Security = 2,
    TokenIntegration = 3,
}

/// The persisted, typed pause state: which flow is blocked and why.
///
/// Stored under [`DataKey::PauseState`] and written atomically by
/// [`LiquifactEscrow::set_paused`] together with the legacy [`DataKey::Paused`]
/// boolean and [`DataKey::PausedAt`] timestamp. Absent ΓçÆ no pause is active.
///
/// # Security note
/// This is a **read-only view** for operators. It is written only by the admin
/// through [`LiquifactEscrow::set_paused`]; no gate code mutates it, so the typed
/// reason/scope can never be poisoned by a non-admin path.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PauseState {
    /// Which flows are blocked by this pause.
    pub scope: PauseScope,
    /// Why the pause was applied (stable, typed reason).
    pub reason: PauseReason,
    /// Ledger timestamp (seconds) when the pause was activated. Paired with
    /// [`DataKey::PauseMaxDurationSecs`] to compute auto-expiry in
    /// [`LiquifactEscrow::paused_blocks`].
    pub activated_at: u64,
}

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
    /// The address that authorized the escrow creation and must authorize funding.
    /// This is distinct from the beneficiary (`sme_address`) who receives payouts.
    pub payer: Address,
    pub amount: i128,
    pub funding_target: i128,
    pub funded_amount: i128,
    pub yield_bps: i64,
    pub maturity: u64,
    /// 0 = open, 1 = funded, 2 = settled, 3 = withdrawn (SME pulled liquidity), 4 = cancelled (admin-gated; investors may refund)
    pub status: u32,
    pub dispute_active: bool,
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
/// Each field is optional ΓÇö a `None` field leaves the current value unchanged.
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

/// Lifecycle state for a recorded dispute.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisputeState {
    Open,
    Resolved,
}

/// Immutable audit record for a dispute. The record is retained after resolution so
/// off-chain reviewers can reconstruct the dispute timeline without replaying events.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisputeRecord {
    pub opened_at: u64,
    pub opened_by: Address,
    pub state: DisputeState,
    pub resolved_at: Option<u64>,
    pub resolved_by: Option<Address>,
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
    /// Whether the operational pause is currently effective ([`LiquifactEscrow::is_paused`]).
    pub paused: bool,
    /// Immutable protocol fee in basis points applied at withdraw; `0` before init.
    pub protocol_fee_bps: i64,
}

/// Bundled settlement-readiness snapshot returned by
/// [`LiquifactEscrow::get_settlement_readiness`].
///
/// Lets an integrator decide whether [`LiquifactEscrow::settle`] will succeed on the current
/// ledger with a single host invocation, instead of stitching together [`LiquifactEscrow::is_settleable`],
/// [`LiquifactEscrow::get_legal_hold`], [`LiquifactEscrow::has_maturity_lock`], and the maturity
/// timestamp ΓÇö and re-deriving the contract's own precedence rules off-chain (which drifts).
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

// ---------------------------------------------------------------------------
// Rent-bump planning types (#1215)
// ---------------------------------------------------------------------------

/// Upper bound on [`LiquifactEscrow::get_rent_bump_plan`] entries per call.
///
/// Mirrors [`MAX_INVESTOR_READ_BATCH`] so the rent-plan view fits within the
/// same CPU/instruction budget as other paginated investor reads.
pub const MAX_RENT_BUMP_PLAN_BATCH: u32 = 50;

/// Rent threshold below which an entry is considered in warning or expired state.
///
/// Any persistent storage entry whose live TTL falls at or below this ledger
/// count is placed in [`RentStatus::Warning`].  An entry that is absent from
/// storage is classified as [`RentStatus::Expired`].
///
/// Value chosen to match [`PERSISTENT_TTL_MIN_EXTENSION_LEDGERS`] ΓÇö the same
/// horizon used by [`LiquifactEscrow::bump_ttl`] for extensions ΓÇö so the
/// warning fires exactly when a bump is operationally due.
pub const RENT_WARN_LEDGERS: u32 = PERSISTENT_TTL_MIN_EXTENSION_LEDGERS;

/// Health classification for a single persistent storage entry's TTL.
///
/// Returned inside [`RentBumpEntry`] by [`LiquifactEscrow::get_rent_bump_plan`].
/// Consumers should act on `Warning` and `Expired` entries immediately by
/// calling [`LiquifactEscrow::bump_ttl`] or [`LiquifactEscrow::batch_bump_ttl`].
///
/// # Threshold
///
/// | Status    | Condition                                                                  |
/// |-----------|----------------------------------------------------------------------------|
/// | `Current` | Key is present in persistent storage and warn threshold is not active      |
/// | `Warning` | Key is present and caller supplied a non-zero `warn_threshold_ledgers`     |
/// | `Expired` | Key is absent from persistent storage (already archived)                  |
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RentStatus {
    /// Entry is present in persistent storage and the caller did not activate
    /// the warning tier (i.e. `warn_threshold_ledgers == 0`).
    Current,
    /// Entry is present but the caller supplied a non-zero
    /// `warn_threshold_ledgers`, signalling that a bump is due.
    Warning,
    /// Entry is absent from persistent storage; it has already been archived
    /// and must be restored before the investor can interact with the escrow.
    Expired,
}

/// A single row in the rent-bump plan produced by
/// [`LiquifactEscrow::get_rent_bump_plan`].
///
/// Each row identifies one investor address, classifies the health of their
/// persistent storage entries, and surfaces a live/absent indicator for the
/// contribution key so operators can prioritise maintenance.
///
/// # Security note
///
/// This struct is **read-only**; no storage is mutated when it is constructed.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RentBumpEntry {
    /// The investor address this entry describes.
    pub investor: Address,
    /// Rent status of the `InvestorContribution` persistent key.
    pub contribution_status: RentStatus,
    /// Rent status of the `InvestorEffectiveYield` persistent key.
    pub effective_yield_status: RentStatus,
    /// Rent status of the `InvestorClaimed` persistent key.
    pub claimed_status: RentStatus,
    /// `1` when the `InvestorContribution` key is present (live in persistent
    /// storage), `0` when absent (expired/archived).  Use `contribution_status`
    /// for the full three-tier classification; this field is a convenience
    /// boolean-as-u32 for callers that need a simple live/absent check.
    pub contribution_ttl: u32,
}

/// Typed return value from [`LiquifactEscrow::settle`].
///
/// Replaces the previous opaque tuple / raw [`InvoiceEscrow`] return with a
/// documented struct that bundles the post-settlement escrow state together
/// with the settlement-specific computed values callers need.
///
/// # Fields
/// - `escrow`: The full post-settlement escrow snapshot (status == 2).
/// - `coupon`: The computed coupon (`funded_amount ├ù yield_bps / 10_000`, floor).
/// - `settle_pool`: Total settlement pool (`funded_amount + coupon`).
/// - `settled_at`: Ledger timestamp when settlement occurred.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SettlementResult {
    /// Post-settlement escrow snapshot (status == 2).
    pub escrow: InvoiceEscrow,
    /// Coupon: `funded_amount ├ù yield_bps / 10_000` (floor, checked).
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
/// so the view is safe to call before initialization ΓÇö callers receive the pre-init
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

/// Cross-contract callback context binding origin, nonce, and lifecycle phase.
///
/// Ensures callbacks originating from external contracts cannot be replayed,
/// redirected across phases, or executed by unauthorized origins.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CallbackContext {
    /// The external contract address authorized to execute this callback.
    pub origin: Address,
    /// Unique invocation nonce assigned when the callback was registered.
    pub nonce: u64,
    /// Expected lifecycle phase / flow identifier for this callback.
    pub phase: u32,
    /// Ledger timestamp when this callback was registered.
    pub created_at: u64,
    /// Whether this callback has already been executed / consumed.
    pub consumed: bool,
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
pub struct PartialRelease {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub amount: i128,
    pub recipient: Address,
}

#[contractevent]
pub struct FinalRelease {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub amount: i128,
    pub recipient: Address,
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
    /// The `min_lock_secs` of the matched [`YieldTier`] (0 when base yield applies ΓÇö no tier,
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

/// Emitted by [`LiquifactEscrow::rotate_payer`] when the payer
/// address is changed, carrying both the prior and new addresses for auditing.
#[contractevent]
pub struct PayerRotated {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub prior_payer: Address,
    pub new_payer: Address,
}

/// Emitted by [`LiquifactEscrow::cancel_pending_admin`] when a pending admin
/// handover proposal is revoked by the current admin.
#[contractevent]
pub struct AdminProposalCancelled {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub cancelled_pending: Address,
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
    /// [`LiquifactEscrow::compute_investor_payout`]: `coupon = funded_amount ├ù yield_bps / 10_000` (floor),
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

/// Emitted by [`LiquifactEscrow::recover_admin`] when the current admin clears an
/// expired, abandoned admin-transfer proposal after the proposal timelock.
#[contractevent]
pub struct AdminRecoveredEvent {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub current_admin: Address,
    pub abandoned_pending: Address,
    pub reason: String,
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

/// Emitted by [`LiquifactEscrow::set_paused`] whenever the operational pause flag is written
/// (activated or cleared). Independent of [`LegalHoldChanged`]: this signals the lightweight
/// incident-response switch, not the compliance hold.
///
/// Carries the typed [`PauseScope`] and [`PauseReason`] so indexers can explain **why** an
/// operation is blocked and **which** flows are affected (the default `name` topic is
/// `symbol_short!("paused")`). On clear (`active == 0`) the `scope`/`reason` fields report the
/// scope/reason of the pause that was actually lifted.
#[contractevent]
pub struct PausedChanged {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    /// `1` = pause enabled, `0` = cleared.
    pub active: u32,
    /// Which flows the pause blocks (or, on clear, which scope was lifted).
    pub scope: PauseScope,
    /// Why the pause was applied (or, on clear, the reason of the lifted pause).
    pub reason: PauseReason,
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
/// NOTE: Defined but never emitted ΓÇö no `update_legal_hold_clear_delay` setter
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
/// storage after the mutation. Exactly one `coll_clr` event is published per
/// successful clear ΓÇö do not emit a second event with the same topic.
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
    /// The 32ΓÇæbyte digest stored at the requested index.
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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollateralPledge {
    pub invoice_id: Symbol,
    pub amount: i128,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

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
        .ok_or(EscrowError::EscrowNotInitialized)
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
    /// Admin-authorized submission of a new pending fee schedule.
    ///
    /// The schedule must be in-bounds and its activation ledger must lie strictly
    /// in the future. Duplicate pending schedules are idempotent.
    pub fn submit_fee_schedule(env: Env, schedule: FeeSchedule) {
        let escrow: InvoiceEscrow = env
            .storage()
            .instance()
            .get(&DataKey::Escrow)
            .unwrap_or_else(|| panic_with_error!(&env, FeeScheduleError::NotInitialized));
        escrow.admin.require_auth();

        if schedule.min_fee_bps > schedule.fee_bps
            || schedule.fee_bps > schedule.max_fee_bps
            || schedule.max_fee_bps > 10_000
        {
            panic_with_error!(&env, FeeScheduleError::FeeOutOfBounds);
        }

        let current_ledger = env.ledger().sequence();
        if schedule.activation_ledger <= current_ledger {
            panic_with_error!(&env, FeeScheduleError::InvalidActivationLedger);
        }

        Self::activate_fee_schedule(env.clone());

        let pending: Option<FeeSchedule> = env
            .storage()
            .instance()
            .get(&FeeScheduleStorageKey::Pending);
        if pending.as_ref() == Some(&schedule) {
            return;
        }
        if pending.is_some() {
            panic_with_error!(&env, FeeScheduleError::PendingScheduleExists);
        }

        env.storage()
            .instance()
            .set(&FeeScheduleStorageKey::Pending, &schedule);
    }

    /// Promotes a pending schedule to active when its activation ledger is reached.
    ///
    /// This is intentionally callable by anyone; it only applies a previously
    /// admin-authorized schedule and records the previous active schedule.
    pub fn activate_fee_schedule(env: Env) -> bool {
        let current_ledger = env.ledger().sequence();
        let pending: Option<FeeSchedule> = env
            .storage()
            .instance()
            .get(&FeeScheduleStorageKey::Pending);
        if let Some(p) = pending {
            if p.activation_ledger <= current_ledger {
                let previous: Option<FeeSchedule> =
                    env.storage().instance().get(&FeeScheduleStorageKey::Active);
                env.storage()
                    .instance()
                    .set(&FeeScheduleStorageKey::Active, &p);
                env.storage()
                    .instance()
                    .set(&FeeScheduleStorageKey::Previous, &previous);
                env.storage()
                    .instance()
                    .remove(&FeeScheduleStorageKey::Pending);
                return true;
            }
        }
        false
    }

    /// Returns the active fee schedule for the current ledger, computing any
    /// not-yet-promoted boundary activation on the fly.
    pub fn get_active_fee_schedule(env: Env) -> Option<FeeSchedule> {
        let active: Option<FeeSchedule> =
            env.storage().instance().get(&FeeScheduleStorageKey::Active);
        let pending: Option<FeeSchedule> = env
            .storage()
            .instance()
            .get(&FeeScheduleStorageKey::Pending);
        match pending {
            Some(p) if p.activation_ledger <= env.ledger().sequence() => Some(p),
            _ => active,
        }
    }

    /// Returns the pending fee schedule that will activate at a future ledger.
    pub fn get_pending_fee_schedule(env: Env) -> Option<FeeSchedule> {
        let pending: Option<FeeSchedule> = env
            .storage()
            .instance()
            .get(&FeeScheduleStorageKey::Pending);
        match pending {
            Some(p) if p.activation_ledger > env.ledger().sequence() => Some(p),
            _ => None,
        }
    }

    /// Returns the previously active fee schedule after a boundary activation.
    pub fn get_previous_fee_schedule(env: Env) -> Option<FeeSchedule> {
        let active: Option<FeeSchedule> =
            env.storage().instance().get(&FeeScheduleStorageKey::Active);
        let pending: Option<FeeSchedule> = env
            .storage()
            .instance()
            .get(&FeeScheduleStorageKey::Pending);
        if let Some(p) = pending {
            if p.activation_ledger <= env.ledger().sequence() {
                return active;
            }
        }
        env.storage()
            .instance()
            .get(&FeeScheduleStorageKey::Previous)
    }
    fn legal_hold_active(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::LegalHold)
            .unwrap_or(false)
    }

    /// Read the operational pause flag; defaults to `false` when unset.
    /// Read the operational pause flag ([`DataKey::Paused`]); defaults to `false` when unset.
    ///
    /// Orthogonal to [`LiquifactEscrow::legal_hold_active`] ΓÇö neither flag affects the other.
    ///
    /// # Auto-expiry
    /// When [`DataKey::PauseMaxDurationSecs`] is configured (nonzero) via
    /// [`LiquifactEscrow::set_pause_max_duration`], a pause that has been active for at least
    /// that many seconds (measured from [`DataKey::PausedAt`]) is treated as inactive here ΓÇö
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

    /// Whether the active operational pause blocks the entrypoint family `entry`.
    ///
    /// Re-uses [`LiquifactEscrow::paused_active`] for the auto-expiry-aware "is any pause
    /// active" check, then consults the stored [`PauseState`] `scope`. A stored pause blocks
    /// an entry if [`PauseScope::All`] is active or the stored scope equals `entry`'s family.
    /// When a legacy pause ([`DataKey::Paused`] set but no [`DataKey::PauseState`] recorded) is
    /// active, it is treated as [`PauseScope::All`] so every gated entrypoint stays blocked,
    /// exactly as before this feature.
    fn paused_blocks(env: &Env, entry: PauseEntry) -> bool {
        if !Self::paused_active(env) {
            return false;
        }
        let Some(state): Option<PauseState> = env.storage().instance().get(&DataKey::PauseState)
        else {
            // Legacy global pause without typed state ΓÇö block every gated entrypoint.
            return true;
        };
        state.scope.blocks(entry)
    }

    /// Read the typed, scoped pause state ΓÇö which flows are blocked and why.
    ///
    /// Returns `None` when no pause is currently effective (including after auto-expiry). When
    /// an effective pause has no stored [`PauseState`] (a legacy global pause predating this
    /// key), this returns a synthesized `scope = All`, `reason = Incident` state so the typed
    /// view stays consistent with [`LiquifactEscrow::is_paused`].
    ///
    /// # Security note
    /// **Pure read** ΓÇö requires no authorization and performs no state mutation. Operators and
    /// indexers may call this freely to render the pause reason/scope without being blocked.
    pub fn get_pause_state(env: Env) -> Option<PauseState> {
        if !Self::paused_active(&env) {
            return None;
        }
        match env.storage().instance().get(&DataKey::PauseState) {
            Some(state) => Some(state),
            None => {
                let activated_at: u64 = env
                    .storage()
                    .instance()
                    .get(&DataKey::PausedAt)
                    .unwrap_or(0);
                Some(PauseState {
                    scope: PauseScope::All,
                    reason: PauseReason::Incident,
                    activated_at,
                })
            }
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
    /// **Pure read** ΓÇö no authorization required, no state mutation.
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
            .get::<DataKey, Vec<YieldTier>>(&DataKey::YieldTierTable)
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
    /// - **Derivation**: Basis point convention; arithmetic safety for coupon = principal ├ù yield / 10_000
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
    /// - Each tier's `yield_bps` must be in `0..=10_000` ΓåÆ `TierYieldOutOfRange`
    /// - Each tier's `yield_bps` must be ΓëÑ base `yield_bps` ΓåÆ `TierYieldBelowBase`
    /// - Tier `min_lock_secs` must be strictly increasing across tiers ΓåÆ `TierLockNotIncreasing`
    /// - Tier `yield_bps` must be non-decreasing across tiers ΓåÆ `TierYieldNotNonDecreasing`
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
        token_decimals: Option<u32>,
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

        // Persist the token decimal scale when supplied. Absent => scale validation
        // is skipped for backward-compatible instances (additive key, ADR-007).
        if let Some(decimals) = token_decimals {
            env.storage()
                .instance()
                .set(&keys::funding_token_scale(), &decimals);
        }
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

        // The payer is set to the admin at initialization. It can be rotated later
        // via rotate_payer (admin + payer dual auth).
        let escrow = InvoiceEscrow {
            invoice_id: invoice_sym.clone(),
            admin: admin.clone(),
            sme_address: sme_address.clone(),
            payer: admin.clone(),
            amount,
            funding_target: amount,
            funded_amount: 0,
            yield_bps,
            maturity,
            status: 0,
            dispute_active: false,
        };

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        // Persist schema version and initial configuration keys.
        env.storage()
            .instance()
            .set(&DataKey::Version, &SCHEMA_VERSION);
        env.storage()
            .instance()
            .set(&DataKey::FundingToken, &funding_token);
        env.storage().instance().set(&DataKey::Treasury, &treasury);

        let floor = min_contribution.unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::MinContributionFloor, &floor);

        env.storage()
            .instance()
            .set(&DataKey::UniqueFunderCount, &0u32);

        if let Some(registry) = registry {
            env.storage()
                .instance()
                .set(&DataKey::RegistryRef, &registry);
        }

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

        if let Some(tiers) = yield_tiers {
            if !tiers.is_empty() {
                env.storage()
                    .instance()
                    .set(&DataKey::YieldTierTable, &tiers);
            }
        }

        let has_maturity_lock = maturity != 0;
        EscrowInitialized {
            name: symbol_short!("escrow_ii"),
            escrow: Self::get_escrow(env.clone()),
            funding_token: Self::funding_token_or_fail(&env),
            treasury: Self::treasury_or_fail(&env),
            registry: Self::get_registry_ref(env.clone()),
            has_maturity_lock: Self::has_maturity_lock(env.clone()),
        }
        .publish(&env);

        escrow
    }

    /// Get current escrow state.
    pub fn get_escrow(env: Env) -> InvoiceEscrow {
        env.storage()
            .instance()
            .get(&DataKey::Escrow)
            .unwrap_or_else(|| panic!("Escrow not initialized"))
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
    /// proof of registry membership ΓÇö query the registry contract directly to verify on-chain state.
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

        // Prevent changing off-chain reference data after any principal has been recorded.
        ensure(
            &env,
            escrow.funded_amount == 0,
            EscrowError::RegistryImmutableAfterFunding,
        );

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
        guard_not_disputed(&env, EscrowError::DisputeBlocksSweep);
        ensure(&env, amount > 0, EscrowError::SweepAmountNotPositive);
        ensure(
            &env,
            amount <= MAX_DUST_SWEEP_AMOUNT,
            EscrowError::SweepAmountExceedsMax,
        );

        // env.clone(): env is used again after this call for treasury/token reads and publish.
        let mut escrow = Self::get_escrow(env.clone());
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
        escrow.status = 2;
        env.storage().instance().set(&DataKey::Escrow, &escrow);
        sweep_amt
    }

    /// Retire a previously recorded collateral pledge.
    ///
    /// Metadata-only: no tokens are moved. Requires SME auth.
    ///
    /// Guard ordering (ADR-002):
    /// 1. Read-only existence check ΓÇö returns [`EscrowError::NoCollateralToClear`] if absent.
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
    pub fn rotate_beneficiary(env: Env, new_sme_address: Address, expected_nonce: u32) -> InvoiceEscrow {
        // Legal-hold gate (read-only).
        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksBeneficiaryRotation);

        let mut escrow = Self::get_escrow(env.clone());

        // Only permitted while the escrow is open (0) or funded (1); terminal
        // states (settled/withdrawn/cancelled) can never rotate.
        ensure(
            &env,
            escrow.status == 0 || escrow.status == 1,
            EscrowError::RotationNotOpen,
        );

        // Only permitted before any funding has been recorded for the escrow:
        // once liquidity starts moving the payout destination is immutable.
        ensure(
            &env,
            escrow.funded_amount == 0,
            EscrowError::BeneficiaryImmutableAfterFunding,
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
        Self::consume_admin_nonce(&env, expected_nonce);

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

    /// Rotate the payer address that must authorize funding.
    ///
    /// Permitted only before settlement (`status` 0 = open or 1 = funded) and
    /// while no legal hold is active. Requires authorization from **both** the
    /// current payer and the admin, so the payer can never be changed unilaterally.
    /// A no-op rotation to the current address is rejected. Emits
    /// [`PayerRotated`] with the prior and new addresses and returns the
    /// updated escrow snapshot.
    ///
    /// # Errors
    ///
    /// | Condition | Typed error |
    /// |-----------|-------------|
    /// | Legal hold active | [`EscrowError::LegalHoldBlocksPayerRotation`] |
    /// | Escrow not open or funded | [`EscrowError::PayerRotationNotOpen`] |
    /// | `new_payer == current payer` | [`EscrowError::NewPayerSameAsCurrent`] |
    pub fn rotate_payer(env: Env, new_payer: Address) -> InvoiceEscrow {
        Self::guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksPayerRotation);

        let mut escrow = Self::get_escrow(env.clone());

        ensure(
            &env,
            escrow.status == 0 || escrow.status == 1,
            EscrowError::PayerRotationNotOpen,
        );

        ensure(
            &env,
            new_payer != escrow.payer,
            EscrowError::NewPayerSameAsCurrent,
        );

        escrow.payer.require_auth();
        escrow.admin.require_auth();

        let prior_payer = escrow.payer.clone();
        escrow.payer = new_payer.clone();
        env.storage().instance().set(&DataKey::Escrow, &escrow);

        PayerRotated {
            name: symbol_short!("payer_rot"),
            invoice_id: escrow.invoice_id.clone(),
            prior_payer,
            new_payer,
        }
        .publish(&env);

        escrow
    }

    /// Load the current escrow and require payer authorization in one step.
    #[allow(dead_code)]
    fn load_escrow_require_payer(env: &Env) -> InvoiceEscrow {
        let escrow: InvoiceEscrow = env
            .storage()
            .instance()
            .get(&DataKey::Escrow)
            .unwrap_or_else(|| fail(env, EscrowError::EscrowNotInitialized));
        escrow.payer.require_auth();
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

    /// Validate and atomically consume the admin nonce for replay protection.
    ///
    /// Reads the current expected nonce from [`DataKey::AdminNonce`], checks that it
    /// matches `expected_nonce`, and increments it by one. Aborts with
    /// [`EscrowError::AdminNonceMismatch`] on any mismatch (stale, duplicate, or future nonce)
    /// without disclosing which specific mismatch occurred.
    ///
    /// The nonce starts at `0` for new deployments and for legacy deployments that lack the
    /// [`DataKey::AdminNonce`] key, providing backward compatibility.
    ///
    /// # Panics
    /// Panics with [`EscrowError::AdminNonceMismatch`] if `expected_nonce` does not match the
    /// stored nonce.
    fn consume_admin_nonce(env: &Env, expected_nonce: u32) {
        let current: u32 = env
            .storage()
            .instance()
            .get(&DataKey::AdminNonce)
            .unwrap_or(0);
        ensure(env, current == expected_nonce, EscrowError::AdminNonceMismatch);
        let next = current.checked_add(1).unwrap_or_else(|| {
            fail(env, EscrowError::AdminNonceMismatch)
        });
        env.storage().instance().set(&DataKey::AdminNonce, &next);
    }

    /// Guard that the escrow is not under a legal hold.
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

    /// Returns the current expected admin nonce for replay protection.
    ///
    /// Absent ⇒ `0` (new or legacy deployment). Callers must pass this value as
    /// `expected_nonce` to any admin-gated entrypoint; the contract atomically
    /// increments it on success.
    pub fn get_admin_nonce(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::AdminNonce)
            .unwrap_or(0)
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

    /// Returns whether an active dispute is currently freeze-blocking release paths.
    pub fn is_dispute_active(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::DisputeActive)
            .unwrap_or(false)
    }

    /// Returns the persisted dispute record, if any has ever been opened.
    pub fn get_dispute_record(env: Env) -> Option<DisputeRecord> {
        env.storage().instance().get(&DataKey::DisputeRecord)
    }

    /// Admin-only dispute freeze: record an active dispute while evidence is pending.
    ///
    /// This is intentionally a minimal, append-only lifecycle: the dispute record is preserved
    /// even after resolution so audits can reconstruct the timeline without replaying off-chain
    /// events. The contract does not expose a "release while disputed" shortcut. Any value-moving
    /// entrypoint must check `DataKey::DisputeActive` before transferring tokens or changing state.
    pub fn open_dispute(env: Env, caller: Address) {
        let escrow = Self::load_escrow_require_admin(&env);
        caller.require_auth();
        ensure(
            &env,
            caller == escrow.admin,
            EscrowError::DisputeOpenUnauthorized,
        );
        ensure(
            &env,
            !Self::is_dispute_active(env.clone()),
            EscrowError::DisputeAlreadyOpen,
        );

        let record = DisputeRecord {
            opened_at: env.ledger().timestamp(),
            opened_by: caller.clone(),
            state: DisputeState::Open,
            resolved_at: None,
            resolved_by: None,
        };
        env.storage().instance().set(&DataKey::DisputeActive, &true);
        env.storage()
            .instance()
            .set(&DataKey::DisputeRecord, &record);
    }

    /// Admin-only dispute resolution: close the dispute while preserving the original record.
    pub fn close_dispute(env: Env, caller: Address, resolved: bool) {
        let escrow = Self::load_escrow_require_admin(&env);
        caller.require_auth();
        ensure(
            &env,
            caller == escrow.admin,
            EscrowError::DisputeCloseUnauthorized,
        );
        ensure(
            &env,
            Self::is_dispute_active(env.clone()),
            EscrowError::DisputeNotOpen,
        );

        let mut record: DisputeRecord = env
            .storage()
            .instance()
            .get(&DataKey::DisputeRecord)
            .unwrap_or_else(|| fail(&env, EscrowError::DisputeNotOpen));

        if resolved {
            record.state = DisputeState::Resolved;
            record.resolved_at = Some(env.ledger().timestamp());
            record.resolved_by = Some(caller.clone());
            env.storage()
                .instance()
                .set(&DataKey::DisputeRecord, &record);
            env.storage()
                .instance()
                .set(&DataKey::DisputeActive, &false);
        } else {
            // leave dispute active if the admin chooses to keep it open; the freeze stays in force.
            return;
        }
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
    /// in token base units; `0` means no extra floor beyond ΓÇ£amount must be positiveΓÇ¥.
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
    /// Absent ΓçÆ unlimited. Enforced on every deposit.
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
            paused: Self::is_paused(env.clone()),
            protocol_fee_bps: Self::get_protocol_fee_bps(env.clone()),
        }
    }

    /// Bind a **primary** 32-byte digest (e.g. SHA-256 of an IPFS CID or document bundle). **Single-set:**
    /// the call succeeds only while no primary hash exists; use [`LiquifactEscrow::append_attestation_digest`]
    /// for an append-only audit trail.
    ///
    /// **Authorization:** [`InvoiceEscrow::admin`]. **Frontrunning:** whichever binding transaction lands
    /// first wins; observers must read on-chain state (or parse events) after finalityΓÇöthere is no replay lock.
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
            .get(&keys::investor_effective_yield(investor))
    }

    fn set_persistent_investor_effective_yield(env: &Env, investor: Address, value: i64) {
        env.storage()
            .persistent()
            .set(&keys::investor_effective_yield(investor), &value);
    }

    fn get_persistent_investor_claim_not_before(env: &Env, investor: Address) -> u64 {
        env.storage()
            .persistent()
            .get(&keys::investor_claim_not_before(investor))
            .unwrap_or(0)
    }

    fn set_persistent_investor_claim_not_before(env: &Env, investor: Address, value: u64) {
        env.storage()
            .persistent()
            .set(&keys::investor_claim_not_before(investor), &value);
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
    /// transitioned status from 1 ΓåÆ 2, or [`None`] if the escrow has not yet been settled.
    ///
    /// **Additive-key policy (ADR-007):** legacy escrow instances that were settled before this key
    /// was introduced will return [`None`] because [`DataKey::SettledAt`] was never written.
    ///
    /// # Returns
    /// - `Some(timestamp)` ΓÇö the ledger timestamp at the moment `settle()` was called.
    /// - `None` ΓÇö escrow is not yet settled, or is a legacy instance predating this key.
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
    /// Pure read ΓÇö no auth required, no state mutation.
    pub fn get_yield_tiers(env: Env) -> Vec<YieldTier> {
        env.storage()
            .instance()
            .get::<DataKey, Vec<YieldTier>>(&DataKey::YieldTierTable)
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

    /// Pure read ΓÇö no auth, no storage writes, safe for simulation.
    ///
    /// Returns a [`YieldTierPreview`] with `{effective_yield_bps, matched_lock_secs}` for a
    /// hypothetical contribution of `amount` with `lock` seconds, using the **exact same
    /// tier-selection rule** applied at the first [`LiquifactEscrow::fund_with_commitment`]
    /// deposit.
    ///
    /// # Parameters
    ///
    /// - `amount: i128` ΓÇö Hypothetical funding amount (currently unused; accepted for signature
    ///   parity with `fund_with_commitment()` for future extensibility).
    ///   - Valid range: any `i128` value (no validation applied; parameter unused)
    ///
    /// - `lock: u64` ΓÇö Hypothetical lock commitment in seconds.
    ///   - Valid range: `0..=u64::MAX` (all u64 values safe; used in comparison only)
    ///   - `0` = no lock ΓåÆ returns base yield
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
    /// 1. Read-only existence check ΓÇö returns [`EscrowError::NoCollateralToClear`] if absent.
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

    pub fn revoke_attestation_digest(env: Env, index: u32) {
        let escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();

        let log = Self::load_attestation_log(&env);
        Self::require_attestation_index_in_range(&env, &log, index);
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
    /// revocation). Duplicate indices in the batch are **not** pre-deduplicated ΓÇö the second
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

        let log = Self::load_attestation_log(&env);

        for i in 0..n {
            let index = indices.get(i).unwrap();

            Self::require_attestation_index_in_range(&env, &log, index);
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

    /// Return revoked attestation entries from `start`, bounded by `limit`.
    ///
    /// `limit` must be in `1..=MAX_ATTESTATION_READ_PAGE`. Out-of-range limits are
    /// rejected with typed errors rather than being silently clamped.
    pub fn get_revoked_attestation_digests(
        env: Env,
        start: u32,
        limit: u32,
    ) -> Vec<AttestationDigestInfo> {
        ensure(&env, limit > 0, EscrowError::AttestationReadLimitZero);
        ensure(
            &env,
            limit <= MAX_ATTESTATION_READ_PAGE,
            EscrowError::AttestationReadLimitTooLarge,
        );

        let log = Self::get_attestation_append_log(env.clone());
        let capped = limit.min(MAX_ATTESTATION_READ_PAGE);
        let (scan_start, scan_end) =
            match Self::paginate_window(start, capped, MAX_ATTESTATION_READ_PAGE, log.len()) {
                Some(w) => w,
                None => return Vec::new(&env),
            };

        let mut result = Vec::new(&env);
        let mut i = scan_start;
        while i < scan_end && result.len() < capped {
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

    /// Returns a paginated slice of all attestation append-log entries, including
    /// both active and revoked records.
    ///
    /// This is the primary enumeration view for attestation records (issue #800).
    /// It walks the append log by absolute index, so `start` always refers to a
    /// position within the full append log ΓÇö not within a filtered subset.
    ///
    /// # Arguments
    /// * `start` ΓÇö 0-based index into the append log to begin reading from.
    /// * `limit` ΓÇö Maximum number of entries to return per call.  Clamped to
    ///   [`MAX_ATTESTATION_READ_PAGE`] even if the caller requests more.
    ///
    /// # Returns
    /// A `Vec<AttestationDigestInfo>` with at most `limit.min(MAX_ATTESTATION_READ_PAGE)`
    /// entries.  Each entry carries both the 32-byte digest and its live revocation
    /// flag.  Returns an empty `Vec` when `start >= log.len()`, when `limit == 0`,
    /// or when the log is empty ΓÇö no panic in any of those cases.
    ///
    /// # Continuation
    /// To fetch the next page, pass `start + result.len()` as the new `start`.
    /// When the returned `Vec` is shorter than the effective limit (or empty),
    /// the caller has reached the end of the log.
    ///
    /// # Notes
    /// * Read-only ΓÇö no state mutation.
    /// * The revocation flag in each entry reflects the live on-chain state at
    ///   query time; it may differ from earlier snapshots if `revoke_attestation_digest`
    ///   or `unrevoke_attestation_digest` was called in the interim.
    pub fn get_attestation_digests(env: Env, start: u32, limit: u32) -> Vec<AttestationDigestInfo> {
        let log = Self::get_attestation_append_log(env.clone());
        let len = log.len();
        if start >= len || limit == 0 {
            return Vec::new(&env);
        }

        let actual_limit = limit.min(MAX_ATTESTATION_READ_PAGE);
        let end = (start + actual_limit).min(len);

        let mut result = Vec::new(&env);
        for i in start..end {
            let digest = log.get(i).unwrap();
            let revoked = env
                .storage()
                .instance()
                .get(&DataKey::AttestationRevoked(i))
                .unwrap_or(false);
            result.push_back(AttestationDigestInfo { digest, revoked });
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
    /// Range check ΓåÆ revocation-state check ΓåÆ `require_auth` ΓåÆ storage mutation.
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
    /// maturity timestamp separately, then replicate the contract's precedence rules ΓÇö which drifts
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

    /// Rent-bump planning view for persistent escrow entries (#1215).
    ///
    /// Scans the investor index and returns a paginated list of
    /// [`RentBumpEntry`] rows ΓÇö one per investor ΓÇö classifying the health of
    /// every persistent storage key that must remain live for investors to
    /// interact with the escrow (fund, claim, get their yield).
    ///
    /// # What this covers
    ///
    /// For each investor address in the range `[start, start+limit)` the
    /// function inspects:
    ///
    /// | Key | [`DataKey`] variant | Persistent? |
    /// |-----|---------------------|-------------|
    /// | Contribution | `InvestorContribution(addr)` | Γ£ô |
    /// | Effective yield | `InvestorEffectiveYield(addr)` | Γ£ô |
    /// | Claimed marker | `InvestorClaimed(addr)` | Γ£ô |
    ///
    /// Each key is classified as:
    /// - [`RentStatus::Current`]  ΓÇö key is present in persistent storage
    /// - [`RentStatus::Warning`]  ΓÇö key is present but `warn_threshold_ledgers`
    ///   is non-zero and the current ledger sequence is within
    ///   `warn_threshold_ledgers` of expiry (caller supplies the horizon)
    /// - [`RentStatus::Expired`]  ΓÇö key is absent from storage (already archived)
    ///
    /// Because Soroban contracts cannot read their own entry TTLs during
    /// normal execution, the caller supplies `warn_threshold_ledgers` as a
    /// planning hint (e.g. the value from
    /// [`LiquifactEscrow::get_storage_limit`]).  When `warn_threshold_ledgers`
    /// is `0`, every present key is classified as `Current`.
    ///
    /// `contribution_ttl` in the returned entry is `1` when the contribution
    /// key is present (live) and `0` when absent (expired).  This field is
    /// intentionally a simple live/absent boolean encoded as `u32` so callers
    /// can sort or filter without decoding the status enum.
    ///
    /// # Read-only guarantee
    ///
    /// This function performs **zero storage writes**.  It does not call
    /// `extend_ttl`, modify any key, or emit any event.  It is safe to call
    /// from any context, including off-chain simulation.
    ///
    /// # Arguments
    /// * `env` ΓÇö Soroban environment.
    /// * `start` ΓÇö Zero-based index into the investor list.
    /// * `limit` ΓÇö Maximum entries to return; silently clamped to
    ///   [`MAX_RENT_BUMP_PLAN_BATCH`] (50).
    /// * `warn_threshold_ledgers` ΓÇö Caller-supplied TTL horizon (in ledgers)
    ///   below which present keys are classified `Warning`.  Pass `0` to
    ///   disable the warning tier and return only `Current` / `Expired`.
    ///   A typical value is [`RENT_WARN_LEDGERS`].
    ///
    /// # Returns
    /// A `Vec<RentBumpEntry>` with at most `limit` entries, one per investor
    /// in the requested window.  Returns an empty vector when:
    /// - No investors exist (`InvestorIndex` is absent).
    /// - `start` is at or beyond the total investor count.
    /// - `limit` is zero.
    ///
    /// # Edge cases
    /// - **No entries** ΓÇö empty vec returned without error.
    /// - **Many entries** ΓÇö pagination keeps each call bounded to Γëñ 50 rows.
    /// - **Mixed lifecycle** ΓÇö entries at every status are included;
    ///   callers filter on `RentStatus` themselves.
    /// - **Boundary threshold** ΓÇö an entry whose live-TTL equals
    ///   `warn_threshold_ledgers` exactly is classified `Warning` (condition
    ///   is `<=`).
    /// - **Read-only call has no writes** ΓÇö no `extend_ttl` is issued.
    pub fn get_rent_bump_plan(
        env: Env,
        start: u32,
        limit: u32,
        warn_threshold_ledgers: u32,
    ) -> Vec<RentBumpEntry> {
        // Load the investor index from instance storage.
        // Absent (pre-init or zero-funded escrow) ΓåÆ treat as empty.
        let index: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::InvestorIndex)
            .unwrap_or_else(|| Vec::new(&env));

        let len = index.len();
        if len == 0 || limit == 0 || start >= len {
            return Vec::new(&env);
        }

        let actual_limit = limit.min(MAX_RENT_BUMP_PLAN_BATCH);
        let end = (start + actual_limit).min(len);

        // Helper: classify a key given whether it is present.
        //
        // Soroban contract execution cannot read its own entry TTL; the caller
        // supplies `warn_threshold_ledgers` as the planning horizon.  When the
        // key is absent it is Expired; when present and warn_threshold_ledgers
        // is non-zero the key is Warning; otherwise Current.
        //
        // The `warn_threshold_ledgers` flag is a conservative signal: the
        // operator supplies the extension horizon they intend to maintain.
        // If warn_threshold_ledgers == 0 every present key is Current.
        let classify = |present: bool, in_warn_zone: bool| -> RentStatus {
            if !present {
                RentStatus::Expired
            } else if in_warn_zone {
                RentStatus::Warning
            } else {
                RentStatus::Current
            }
        };

        // Derive the warn flag once: if warn_threshold_ledgers > 0 then every
        // present key is placed in Warning (the contract cannot distinguish
        // finer-grained TTL from within execution; operators pass the threshold
        // when they want the planner to be conservative and flag everything for
        // a bump).  When warn_threshold_ledgers == 0 the warn zone is disabled.
        let warn_active = warn_threshold_ledgers > 0;

        let mut result: Vec<RentBumpEntry> = Vec::new(&env);

        for i in start..end {
            let investor = index.get(i).unwrap();

            let contribution_key = keys::investor_contribution(investor.clone());
            let yield_key = keys::investor_effective_yield(investor.clone());
            let claimed_key = keys::investor_claimed(investor.clone());

            let contribution_present = env.storage().persistent().has(&contribution_key);
            let effective_yield_present = env.storage().persistent().has(&yield_key);
            let claimed_present = env.storage().persistent().has(&claimed_key);

            // contribution_ttl: 1 = live, 0 = absent (archived).
            let contribution_ttl: u32 = if contribution_present { 1 } else { 0 };

            result.push_back(RentBumpEntry {
                investor,
                contribution_status: classify(
                    contribution_present,
                    warn_active && contribution_present,
                ),
                effective_yield_status: classify(
                    effective_yield_present,
                    warn_active && effective_yield_present,
                ),
                claimed_status: classify(claimed_present, warn_active && claimed_present),
                contribution_ttl,
            });
        }

        result
    }

    /// Read-only snapshot of all settlement-relevant configuration.
    ///
    /// Returns the current values of every parameter that affects settlement economics
    /// and funding guards in a single [`SettlementConfig`] struct. Integrators can use
    /// this view to display the full configuration without calling multiple individual
    /// getters.
    ///
    /// # Pre-init safety
    /// Every field is read from storage independently with `unwrap_or(default)`, matching
    /// the same defaults the contract applies at [`LiquifactEscrow::init`]. The view
    /// therefore returns sensible defaults before initialization without panicking.
    ///
    /// # Read-only
    /// Pure view: no `require_auth`, no storage writes, and no TTL bump.
    pub fn get_settlement_config(env: Env) -> SettlementConfig {
        let (yield_bps, maturity) = match env
            .storage()
            .instance()
            .get::<DataKey, InvoiceEscrow>(&DataKey::Escrow)
        {
            Some(e) => (e.yield_bps, e.maturity),
            None => (0, 0),
        };

        let protocol_fee_bps: i64 = env
            .storage()
            .instance()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0);

        let yield_tiers: Vec<YieldTier> = env
            .storage()
            .instance()
            .get(&DataKey::YieldTierTable)
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env));

        let maturity_max_horizon: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MaturityMaxHorizon)
            .unwrap_or(DEFAULT_MATURITY_MAX_HORIZON_SECS);

        let funding_deadline: Option<u64> = env.storage().instance().get(&DataKey::FundingDeadline);

        let min_contribution_floor: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinContributionFloor)
            .unwrap_or(0);

        let max_unique_investors_cap: Option<u32> = env
            .storage()
            .instance()
            .get(&DataKey::MaxUniqueInvestorsCap);

        let max_per_investor_cap: Option<i128> =
            env.storage().instance().get(&DataKey::MaxPerInvestorCap);

        SettlementConfig {
            yield_bps,
            maturity,
            protocol_fee_bps,
            yield_tiers,
            maturity_max_horizon,
            funding_deadline,
            min_contribution_floor,
            max_unique_investors_cap,
            max_per_investor_cap,
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

    /// Batch variant of [`LiquifactEscrow::record_sme_collateral_commitment`].
    ///
    /// Processes a bounded vector of `(asset, amount)` pairs atomically: either **all** items
    /// pass validation and the final commitment is stored, or **any** single item fails and the
    /// entire batch is rejected with no state change.
    ///
    /// Each item undergoes the same per-item checks as the single entrypoint:
    /// - `amount > 0` ([`EscrowError::CollateralAmountNotPositive`])
    /// - `asset` is non-empty ([`EscrowError::CollateralAssetEmpty`])
    /// - Replacement timestamp not backwards ([`EscrowError::CollateralTimestampBackwards`])
    ///
    /// The stored [`SmeCollateralCommitment`] after a successful batch is the **last** item in
    /// the vector. A [`CollateralRecordedEvt`] is emitted for **each** item, preserving the
    /// per-item audit trail.
    ///
    /// # Batch bounds
    /// - `items` must be non-empty ([`EscrowError::CollateralBatchEmpty`]).
    /// - `items.len()` must not exceed [`MAX_COLLATERAL_BATCH`] ([`EscrowError::CollateralBatchTooLarge`]).
    ///
    /// # Authorization
    /// Requires [`InvoiceEscrow::sme_address`] auth (same as the single entrypoint).
    pub fn batch_record_collateral(
        env: Env,
        items: Vec<(Symbol, i128)>,
    ) -> SmeCollateralCommitment {
        let n = items.len();
        ensure(&env, n > 0, EscrowError::CollateralBatchEmpty);
        ensure(
            &env,
            n <= MAX_COLLATERAL_BATCH,
            EscrowError::CollateralBatchTooLarge,
        );

        // ΓöÇΓöÇ Pre-validation (all-or-nothing) ΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇ
        // Validate every item's per-item invariants before any storage write.
        // A single invalid item (zero/negative amount, empty asset) rejects the
        // entire batch atomically.
        for i in 0..n {
            let (asset, amount) = items.get(i).unwrap();
            ensure(&env, amount > 0, EscrowError::CollateralAmountNotPositive);
            ensure(
                &env,
                asset != Symbol::new(&env, ""),
                EscrowError::CollateralAssetEmpty,
            );
        }

        let escrow = Self::load_escrow_require_sme(&env);
        let now = env.ledger().timestamp();

        // Check timestamp against the existing stored commitment (if any).
        // Only the first item needs this check; subsequent items in the same
        // batch share `now` as their `recorded_at`, so `now >= now` always holds.
        let prior: Option<SmeCollateralCommitment> =
            env.storage().instance().get(&DataKey::SmeCollateralPledge);
        if let Some(ref existing) = prior {
            ensure(
                &env,
                now >= existing.recorded_at,
                EscrowError::CollateralTimestampBackwards,
            );
        }

        let mut last_commitment = SmeCollateralCommitment {
            asset: Symbol::new(&env, ""),
            amount: 0,
            recorded_at: now,
        };

        for i in 0..n {
            let (asset, amount) = items.get(i).unwrap();

            let prior_amount = if i == 0 {
                prior.as_ref().map(|c| c.amount).unwrap_or(0)
            } else {
                last_commitment.amount
            };

            last_commitment = SmeCollateralCommitment {
                asset: asset.clone(),
                amount,
                recorded_at: now,
            };

            env.storage()
                .instance()
                .set(&DataKey::SmeCollateralPledge, &last_commitment);

            CollateralRecordedEvt {
                name: symbol_short!("coll_rec"),
                invoice_id: escrow.invoice_id.clone(),
                amount,
                prior_amount,
            }
            .publish(&env);
        }

        last_commitment
    }

    /// Set or clear the lightweight **operational pause**, carrying a typed [`PauseScope`]
    /// (which flows are blocked) and [`PauseReason`] (why). Only the **current**
    /// [`InvoiceEscrow::admin`] may call.
    ///
    /// This is an incident-response circuit breaker (e.g. a suspected token bug) that is
    /// **orthogonal to the compliance legal hold**: it carries no compliance semantics and,
    /// unlike [`LiquifactEscrow::set_legal_hold`], has **no** two-phase clear delay ΓÇö a single
    /// authorized call toggles it on or off. Legal-hold state is neither read nor written.
    ///
    /// # Activation (`active == true`)
    /// Stores a [`PauseState`] under [`DataKey::PauseState`] (atomically with [`DataKey::Paused`]
    /// and [`DataKey::PausedAt`]) recording `scope` and `reason`, and writes the legacy
    /// `Paused`/`PausedAt` flags so all existing pause logic (auto-expiry, rate limiting, read
    /// views) continues to work. While active, `scope` decides which of `fund`/
    /// `fund_with_commitment`/`fund_batch`, `settle`, `withdraw`, and `claim_investor_payout` are
    /// blocked; [`PauseScope::All`] blocks every gated entrypoint. Reactivating an already-active
    /// pause refreshes `activated_at` and the recorded reason (idempotent from the caller's
    /// perspective ΓÇö [`LiquifactEscrow::is_paused`] stays `true`).
    ///
    /// # Clearing (`active == false`)
    /// - Clears the active pause when `scope == PauseScope::All` or `scope` equals the active
    ///   pause's scope. Both `Paused`/`PausedAt` and `PauseState` are cleared together (atomic).
    /// - A `scope` that does **not** match the active pause is rejected with
    ///   [`EscrowError::PauseScopeMismatch`] ΓÇö nothing is cleared, so an operator cannot
    ///   accidentally believe they lifted a pause when they did not.
    /// - Clearing when no pause is active is a harmless no-op.
    ///
    /// Emits [`PausedChanged`] with the effective `scope`/`reason` on every successful toggle.
    ///
    /// # Rate limiting
    /// When [`LiquifactEscrow::set_pause_rate_limit`] has configured a nonzero toggle limit,
    /// each call to `set_paused` (in either direction) consumes one slot in the current rolling
    /// window; once the limit is reached within the window, further calls fail with
    /// [`EscrowError::PauseToggleRateLimitExceeded`] until the window rolls over. Default
    /// (unconfigured) preserves legacy behavior: no rate limit.
    ///
    /// # Errors
    /// - [`EscrowError::PauseToggleRateLimitExceeded`] if the configured toggle-rate limit was
    ///   already reached within the current window.
    /// - [`EscrowError::PauseScopeMismatch`] if `active == false` and `scope`(Γëá `All`) does not
    ///   match the currently active pause's scope.
    pub fn set_paused(env: Env, active: bool, scope: PauseScope, reason: PauseReason) {
        let escrow = Self::load_escrow_require_admin(&env);

        let toggle_limit: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PauseToggleLimit)
            .unwrap_or(DEFAULT_PAUSE_TOGGLE_LIMIT);
        let now = env.ledger().timestamp();
        let (window_start, window_count): (u64, u32) = if toggle_limit > 0 {
            let window_secs: u64 = env
                .storage()
                .instance()
                .get(&DataKey::PauseToggleWindowSecs)
                .unwrap_or(0);
            let stored_start: Option<u64> = env
                .storage()
                .instance()
                .get(&DataKey::PauseToggleWindowStart);
            let window_elapsed = match stored_start {
                None => true,
                Some(start) => now >= start.saturating_add(window_secs),
            };
            if window_elapsed {
                (now, 0u32)
            } else {
                let count: u32 = env
                    .storage()
                    .instance()
                    .get(&DataKey::PauseToggleCountInWindow)
                    .unwrap_or(0);
                (stored_start.unwrap(), count)
            }
        } else {
            (now, 0u32)
        };
        if toggle_limit > 0 {
            ensure(
                &env,
                window_count < toggle_limit,
                EscrowError::PauseToggleRateLimitExceeded,
            );
        }

        if active {
            env.storage().instance().set(
                &DataKey::PauseState,
                &PauseState {
                    scope,
                    reason,
                    activated_at: now,
                },
            );
            env.storage().instance().set(&DataKey::Paused, &true);
            env.storage().instance().set(&DataKey::PausedAt, &now);
        } else {
            // Determine the currently effective pause scope, if any.
            let now_active = Self::paused_active(&env);
            let existing: Option<PauseState> = env.storage().instance().get(&DataKey::PauseState);
            let (effective_scope, cleared_reason) = match &existing {
                Some(s) => (s.scope, s.reason),
                // Legacy global pause (Paused set, no typed state) is treated as All.
                None if now_active => (PauseScope::All, PauseReason::Incident),
                // Nothing to clear ΓÇö harmless no-op.
                None => {
                    PausedChanged {
                        name: symbol_short!("paused"),
                        invoice_id: escrow.invoice_id.clone(),
                        active: 0,
                        scope,
                        reason,
                    }
                    .publish(&env);
                    return;
                }
            };
            // Wrong-scope unpause is an explicit error, never a silent no-op.
            if scope != PauseScope::All && scope != effective_scope {
                fail(&env, EscrowError::PauseScopeMismatch);
            }
            env.storage().instance().remove(&DataKey::PauseState);
            env.storage().instance().set(&DataKey::Paused, &false);
            PausedChanged {
                name: symbol_short!("paused"),
                invoice_id: escrow.invoice_id.clone(),
                active: 0,
                scope: effective_scope,
                reason: cleared_reason,
            }
            .publish(&env);
        }
        if toggle_limit > 0 {
            env.storage()
                .instance()
                .set(&DataKey::PauseToggleWindowStart, &window_start);
            // Defensive hardening: `window_count < toggle_limit` is asserted above, and
            // `toggle_limit <= u32::MAX`, so `window_count + 1` cannot overflow today. Use
            // `saturating_add` anyway so this line stays safe by construction rather than by
            // relying on the guard above never being reordered (see #823 pause-arithmetic audit).
            env.storage().instance().set(
                &DataKey::PauseToggleCountInWindow,
                &window_count.saturating_add(1),
            );
        }

        if active {
            PausedChanged {
                name: symbol_short!("paused"),
                invoice_id: escrow.invoice_id,
                active: 1,
                scope,
                reason,
            }
            .publish(&env);
        }
    }

    /// Set the maximum duration (seconds) [`DataKey::Paused`] may remain active before the
    /// pause auto-expires for gate-checking purposes ([`LiquifactEscrow::is_paused`] and every
    /// pause-gated entrypoint). Only the **current** [`InvoiceEscrow::admin`] may call.
    ///
    /// Pass `0` to disable the limit (unlimited pause duration ΓÇö the legacy, pre-existing
    /// behavior). A nonzero value must fall within
    /// [`MIN_PAUSE_MAX_DURATION_SECS`, `MAX_PAUSE_MAX_DURATION_SECS`].
    ///
    /// # Errors
    /// - [`EscrowError::PauseMaxDurationOutOfRange`] if `new_duration_secs` is nonzero and
    ///   outside the configured bounds.
    ///
    /// # Events
    /// Emits [`PauseMaxDurationUpdated`] with the old and new values.
    pub fn set_pause_max_duration(env: Env, new_duration_secs: u64) -> u64 {
        let escrow = Self::load_escrow_require_admin(&env);

        if new_duration_secs != 0 {
            ensure(
                &env,
                (MIN_PAUSE_MAX_DURATION_SECS..=MAX_PAUSE_MAX_DURATION_SECS)
                    .contains(&new_duration_secs),
                EscrowError::PauseMaxDurationOutOfRange,
            );
        }

        let old_value: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PauseMaxDurationSecs)
            .unwrap_or(DEFAULT_PAUSE_MAX_DURATION_SECS);

        env.storage()
            .instance()
            .set(&DataKey::PauseMaxDurationSecs, &new_duration_secs);

        PauseMaxDurationUpdated {
            name: symbol_short!("pausemax"),
            invoice_id: escrow.invoice_id,
            old_value,
            new_value: new_duration_secs,
        }
        .publish(&env);

        new_duration_secs
    }

    /// Read the configured pause auto-expiry duration ([`DataKey::PauseMaxDurationSecs`]);
    /// defaults to [`DEFAULT_PAUSE_MAX_DURATION_SECS`] (`0` = unlimited) when unset.
    pub fn get_pause_max_duration(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::PauseMaxDurationSecs)
            .unwrap_or(DEFAULT_PAUSE_MAX_DURATION_SECS)
    }

    /// Set the pause-toggle rate limit: at most `max_toggles` calls to
    /// [`LiquifactEscrow::set_paused`] within any `window_secs`-second rolling window. Only the
    /// **current** [`InvoiceEscrow::admin`] may call.
    ///
    /// Pass `(0, 0)` to disable rate limiting (the legacy, pre-existing behavior). A nonzero
    /// `max_toggles` must fall within [`MIN_PAUSE_TOGGLE_LIMIT`, `MAX_PAUSE_TOGGLE_LIMIT`] and
    /// `window_secs` must fall within
    /// [`MIN_PAUSE_TOGGLE_WINDOW_SECS`, `MAX_PAUSE_TOGGLE_WINDOW_SECS`].
    ///
    /// Reconfiguring resets the current rate-limit window so behavior after the call is always
    /// predictable (no stale counts carried over from the prior configuration).
    ///
    /// # Errors
    /// - [`EscrowError::PauseRateLimitInvalidCombination`] if exactly one of `max_toggles` /
    ///   `window_secs` is zero.
    /// - [`EscrowError::PauseToggleLimitOutOfRange`] if `max_toggles` is nonzero and outside
    ///   bounds.
    /// - [`EscrowError::PauseToggleWindowOutOfRange`] if `window_secs` is outside bounds while
    ///   `max_toggles` is nonzero.
    ///
    /// # Events
    /// Emits [`PauseRateLimitUpdated`] with the old and new limit/window.
    pub fn set_pause_rate_limit(env: Env, max_toggles: u32, window_secs: u64) -> (u32, u64) {
        let escrow = Self::load_escrow_require_admin(&env);

        if max_toggles == 0 || window_secs == 0 {
            ensure(
                &env,
                max_toggles == 0 && window_secs == 0,
                EscrowError::PauseRateLimitInvalidCombination,
            );
        } else {
            ensure(
                &env,
                (MIN_PAUSE_TOGGLE_LIMIT..=MAX_PAUSE_TOGGLE_LIMIT).contains(&max_toggles),
                EscrowError::PauseToggleLimitOutOfRange,
            );
            ensure(
                &env,
                (MIN_PAUSE_TOGGLE_WINDOW_SECS..=MAX_PAUSE_TOGGLE_WINDOW_SECS)
                    .contains(&window_secs),
                EscrowError::PauseToggleWindowOutOfRange,
            );
        }

        let old_limit: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PauseToggleLimit)
            .unwrap_or(DEFAULT_PAUSE_TOGGLE_LIMIT);
        let old_window: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PauseToggleWindowSecs)
            .unwrap_or(0);

        env.storage()
            .instance()
            .set(&DataKey::PauseToggleLimit, &max_toggles);
        env.storage()
            .instance()
            .set(&DataKey::PauseToggleWindowSecs, &window_secs);
        env.storage()
            .instance()
            .remove(&DataKey::PauseToggleWindowStart);
        env.storage()
            .instance()
            .set(&DataKey::PauseToggleCountInWindow, &0u32);

        PauseRateLimitUpdated {
            name: symbol_short!("pause_rl"),
            invoice_id: escrow.invoice_id,
            old_limit,
            new_limit: max_toggles,
            old_window_secs: old_window,
            new_window_secs: window_secs,
        }
        .publish(&env);

        (max_toggles, window_secs)
    }

    /// Read the configured pause-toggle rate limit as `(max_toggles, window_secs)`; `(0, 0)`
    /// means unlimited (the default).
    pub fn get_pause_rate_limit(env: Env) -> (u32, u64) {
        let limit: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PauseToggleLimit)
            .unwrap_or(DEFAULT_PAUSE_TOGGLE_LIMIT);
        let window: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PauseToggleWindowSecs)
            .unwrap_or(0);
        (limit, window)
    }

    /// Read the ledger timestamp of the most recent `set_paused(true)` call
    /// ([`DataKey::PausedAt`]); `None` if the pause has never been activated.
    pub fn get_paused_at(env: Env) -> Option<u64> {
        env.storage().instance().get(&DataKey::PausedAt)
    }

    /// Set or clear compliance hold. Only the **current** [`InvoiceEscrow::admin`] may call.
    ///
    /// **Clearing:** always requires the current admin's authorization ΓÇö there is no timelock,
    /// council override, or break-glass entrypoint. After
    /// [`LiquifactEscrow::propose_admin`] and [`LiquifactEscrow::accept_admin`], only the **new**
    /// admin can clear a persisted hold.
    ///
    /// **Governance posture:** production `admin` must be a multisig or governed contract so
    /// hold + key loss cannot strand funds without an off-chain recovery vote that executes
    /// `propose_admin`, `accept_admin`, then `clear_legal_hold`. See
    /// `docs/escrow-legal-hold.md`.
    pub fn set_legal_hold(env: Env, active: bool, expected_nonce: u32) {
        let escrow = Self::load_escrow_require_admin(&env);
        Self::consume_admin_nonce(&env, expected_nonce);

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
    pub fn request_clear_legal_hold(env: Env, expected_nonce: u32) {
        let escrow = Self::load_escrow_require_admin(&env);
        Self::consume_admin_nonce(&env, expected_nonce);

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
    pub fn set_allowlist_active(env: Env, active: bool, expected_nonce: u32) {
        let escrow = Self::load_escrow_require_admin(&env);
        Self::consume_admin_nonce(&env, expected_nonce);
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
    pub fn set_investor_allowlisted(env: Env, investor: Address, allowed: bool, expected_nonce: u32) {
        let escrow = Self::load_escrow_require_admin(&env);
        Self::consume_admin_nonce(&env, expected_nonce);
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
    /// - [`EscrowError::InvestorBatchEmpty`] (70) — when `investors` is empty
    /// - [`EscrowError::InvestorBatchTooLarge`] (71) — when `investors.len() > MAX_INVESTOR_ALLOWLIST_BATCH`
    ///
    /// # See also
    /// - [`LiquifactEscrow::set_investor_allowlisted`] — single-address variant
    /// - [`LiquifactEscrow::is_investor_allowlisted`] — check if an address is allowlisted
    /// - [`docs/escrow-allowlist.md`](../docs/escrow-allowlist.md) — full allowlist model documentation
    pub fn set_investors_allowlisted(env: Env, investors: Vec<Address>, allowed: bool, expected_nonce: u32) {
        let escrow = Self::load_escrow_require_admin(&env);
        Self::consume_admin_nonce(&env, expected_nonce);

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

        let (start, end) =
            match Self::paginate_window(start, limit, MAX_INVESTOR_READ_BATCH, index.len()) {
                Some(w) => w,
                None => return Vec::new(&env),
            };

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
    pub fn clear_legal_hold(env: Env, expected_nonce: u32) {
        Self::set_legal_hold(env, false, expected_nonce);
    }

    pub fn update_funding_target(env: Env, new_target: i128, expected_nonce: u32) -> InvoiceEscrow {
        let mut escrow = Self::load_escrow_require_admin(&env);
        Self::consume_admin_nonce(&env, expected_nonce);

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
        // exactly once ΓÇö mirroring the promotion logic in `fund`/`fund_with_commitment`.
        if escrow.funded_amount > 0
            && escrow.funded_amount >= new_target
            && !env
                .storage()
                .instance()
                .has(&keys::funding_close_snapshot())
        {
            escrow.status = 1;
            env.storage().instance().set(
                &keys::funding_close_snapshot(),
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
    pub fn update_funding_deadline(env: Env, new_deadline: Option<u64>, expected_nonce: u32) {
        let escrow = Self::load_escrow_require_admin(&env);
        Self::consume_admin_nonce(&env, expected_nonce);

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
    pub fn lower_max_unique_investors(env: Env, new_cap: u32, expected_nonce: u32) -> u32 {
        let escrow = Self::load_escrow_require_admin(&env);
        Self::consume_admin_nonce(&env, expected_nonce);

        guard_status_eq(&env, escrow.status, 0, EscrowError::CapLowerNotOpen);

        let old_cap: Option<u32> = env
            .storage()
            .instance()
            .get(&keys::max_unique_investors_cap());
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
            .set(&keys::max_unique_investors_cap(), &new_cap);

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

        // We can reuse the existing EscrowNotOpenForFunding or similar open check.
        // Or if there's a specific one, we use it. For now EscrowNotOpenForFunding is safe,
        // or just rely on escrow.status == 0 since that's what the prompt implies.
        // Actually, reusing EscrowError::EscrowNotOpenForFunding since CapLowerNotOpen is specific to lower.
        // But wait, the issue said "parallel guards" and "open-state-only".
        // Let's use EscrowError::EscrowNotOpenForFunding.
        ensure(
            &env,
            escrow.status == 0,
            EscrowError::EscrowNotOpenForFunding,
        );
        require_funding_open(&env, escrow.status);

        let old_cap: Option<u32> = env
            .storage()
            .instance()
            .get(&keys::max_unique_investors_cap());
        ensure(
            &env,
            old_cap.is_some(),
            EscrowError::NoInvestorCapConfigured,
        );
        let old_cap = old_cap.unwrap();

        ensure(&env, new_cap > old_cap, EscrowError::NewCapNotHigher);

        env.storage()
            .instance()
            .set(&keys::max_unique_investors_cap(), &new_cap);

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
            .get(&keys::min_contribution_floor())
            .unwrap_or(0);
        ensure(&env, new_floor < old_floor, EscrowError::NewFloorNotLower);

        env.storage()
            .instance()
            .set(&keys::min_contribution_floor(), &new_floor);

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
    /// * `env` ΓÇö The Soroban environment.
    /// * `new_cap` ΓÇö The new per-investor cap, must be > current cap.
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

        let old_cap: Option<i128> = env.storage().instance().get(&keys::max_per_investor_cap());
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
            .set(&keys::max_per_investor_cap(), &new_cap);

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
    ///   `from_version ΓåÆ SCHEMA_VERSION` path you need.
    /// - Additive new [`DataKey`] variants read with `.get(...).unwrap_or(default)` do **not**
    ///   require a `migrate` call; old instances simply return the default.
    /// - If `InvoiceEscrow` struct layout changed, `migrate` cannot help ΓÇö redeploy instead.
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
    /// See `docs/OPERATOR_RUNBOOK.md` ┬º2 for step-by-step instructions on implementing
    /// a concrete migration path.
    pub fn migrate(env: Env, from_version: u32, expected_nonce: u32) -> u32 {
        Self::load_escrow_require_admin(&env);
        Self::consume_admin_nonce(&env, expected_nonce);

        let stored: u32 = if env.storage().instance().has(&DataKey::Version) {
            env.storage().instance().get(&DataKey::Version).unwrap()
        } else {
            let has_token = env.storage().instance().has(&DataKey::FundingToken);
            let has_treasury = env.storage().instance().has(&DataKey::Treasury);
            if has_token && has_treasury {
                LEGACY_VERSION
            } else {
                fail(&env, EscrowError::AmbiguousLegacyStorage)
            }
        };

        ensure(
            &env,
            stored == from_version,
            EscrowError::MigrationVersionMismatch,
        );

        if from_version >= SCHEMA_VERSION {
            fail(&env, EscrowError::AlreadyCurrentSchemaVersion)
        } else if from_version == LEGACY_VERSION {
            env.storage().instance().set(&DataKey::Version, &SCHEMA_VERSION);
            SCHEMA_VERSION
        } else {
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
    /// # Risks
    /// Deploying an incompatible WASM will corrupt stored state. Test thoroughly on
    /// testnet before upgrading production contracts.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>, expected_nonce: u32) {
        // Auth first — matches migrate() ordering
        let escrow = Self::load_escrow_require_admin(&env);
        Self::consume_admin_nonce(&env, expected_nonce);

        // Emit event before the deployer call so the event is recorded even if
        // the deployer call somehow reverts (defensive ordering)
        ContractUpgraded {
            name: symbol_short!("upgrade"),
            invoice_id: escrow.invoice_id,
            new_wasm_hash: new_wasm_hash.clone(),
        }
        .publish(&env);

        // Replace contract WASM ΓÇö no state is modified
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
    /// # Lock Commitment (`committed_lock_secs`) Bounds
    ///
    /// **Valid range**: `0..=u64::MAX` seconds
    /// - `0` = no lock commitment; investor receives base yield immediately upon settlement
    /// - `> 0` = lock duration in seconds; sets `InvestorClaimNotBefore = now + committed_lock_secs`
    ///
    /// **Constraints**:
    /// - `now + committed_lock_secs` must not exceed escrow maturity
    ///   - Rejection: `CommitmentLockExceedsMaturity` if `now + committed_lock_secs > maturity`
    ///   - Derivation: Prevent investor payout hold beyond principal due date
    /// - Timestamp addition is checked with overflow guard
    ///   - Rejection: `InvestorClaimTimeOverflow` if `checked_add(now, committed_lock_secs)` overflows
    ///   - Derivation: Prevent u64 underflow/overflow in ledger timestamp arithmetic
    ///
    /// **Tier selection**:
    /// - Investor's effective yield is selected at this (first) deposit based on `committed_lock_secs`
    /// - `effective_yield_for_commitment()` finds the highest-yield tier where `committed_lock_secs >= tier.min_lock_secs`
    /// - Falls back to base yield if `committed_lock_secs = 0` or no tier table exists
    /// - This selection is **immutable** across any follow-on deposits with `fund()`
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
    /// If any entry causes the escrow to transition to **funded** (status 0 ΓåÆ 1),
    /// [`DataKey::FundingCloseSnapshot`] is recorded exactly once. Remaining entries are
    /// processed even after transition.
    pub fn fund_batch(env: Env, entries: Vec<(Address, i128)>) -> InvoiceEscrow {
        let n = entries.len();

        ensure(&env, n > 0, EscrowError::FundingBatchEmpty);
        ensure(&env, n <= MAX_FUND_BATCH, EscrowError::FundingBatchTooLarge);

        // ΓöÇΓöÇ Atomicity guarantee (issue #557) ΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇ
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
            .get(&keys::min_contribution_floor())
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

        // ΓöÇΓöÇ Duplicate-address guard (issue #643) ΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇ
        // Reject the entire batch atomically if any two entries share an investor address.
        // Each investor must appear at most once per call; duplicates suggest a malformed
        // batch and could incorrectly accumulate principal or consume unique-investor slots.
        //
        // Algorithm: O(n┬▓) pairwise comparison, bounded by MAX_FUND_BATCH = 50 (Γëñ 2 500
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
        // Operational pause gate (read-only), before require_auth and orthogonal to legal hold.
        ensure(
            &env,
            !Self::paused_blocks(&env, PauseEntry::Funding),
            EscrowError::PausedBlocksFunding,
        );

        investor.require_auth();

        ensure(&env, amount > 0, EscrowError::FundingAmountNotPositive);

        // Decimal-scale guard: if a token_decimals scale was configured at init,
        // reject amounts that cannot be represented exactly at that precision.
        // Absent key => scale validation is skipped (backward-compatible additive key).
        if let Some(token_decimals) = env
            .storage()
            .instance()
            .get::<DataKey, u32>(&keys::funding_token_scale())
        {
            validate_decimal_scale(&env, amount, token_decimals);
        }

        let floor: i128 = env
            .storage()
            .instance()
            .get(&keys::min_contribution_floor())
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
        guard_not_paused(&env, EscrowError::PausedBlocksFunding, PauseEntry::Funding);
        // Legal hold check is intentionally after the escrow read: the escrow is needed for
        // status and yield_bps regardless, and hoisting the hold check before the escrow read
        // would not reduce storage operations (both keys are always read on this path).
        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksFunding);
        require_funding_open(&env, escrow.status);

        // Check funding deadline
        if let Some(deadline) = env.storage().instance().get(&keys::funding_deadline()) {
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
            .get::<DataKey, i128>(&keys::max_per_investor_cap())
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
                .get(&keys::unique_funder_count())
                .unwrap_or(0)
        } else {
            0 // prev != 0: count is not needed; skip the read entirely.
        };

        if prev == 0 {
            // Hard, unconditional ceiling on distinct participants (issue #1229): bounds the
            // worst-case release/accounting cost that scales with participant count even when
            // `max_unique_investors` was not configured at init. New typed error keeps the
            // failure explicit and append-only.
            ensure(
                &env,
                cur_funder_count < MAX_UNIQUE_INVESTORS,
                EscrowError::UniqueInvestorHardCapReached,
            );
            if let Some(cap) = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&keys::max_unique_investors_cap())
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

        // 2. require_auth checks
        investor.require_auth();
        escrow.payer.require_auth();

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
                YieldResolution {
                    effective_yield_bps: escrow.yield_bps,
                    matched_lock_secs: 0,
                }
            } else {
                // Returning investor: yield was set on first deposit; read it for the event.
                // If prev > 0, preserve existing effective yield and claim lock.
                // Read stored yield for the event (falls back to escrow default for new investors).
                YieldResolution {
                    effective_yield_bps: Self::get_persistent_investor_effective_yield(
                        &env,
                        investor.clone(),
                    )
                    .unwrap_or(escrow.yield_bps),
                    matched_lock_secs: 0,
                }
            }
        } else {
            Self::set_persistent_investor_effective_yield(
                &env,
                investor.clone(),
                investor_effective_yield_bps,
            );
            Self::set_persistent_investor_claim_not_before(&env, investor.clone(), claim_nb);
            res
        };
        let investor_effective_yield_bps = resolution.effective_yield_bps;
        let tier_lock_secs = resolution.matched_lock_secs;

        escrow.funded_amount = escrow
            .funded_amount
            .checked_add(amount)
            .unwrap_or_else(|| fail(&env, EscrowError::FundedAmountOverflow));

        if escrow.status == 0 && escrow.funded_amount >= escrow.funding_target {
            escrow.status = 1;
            if !env
                .storage()
                .instance()
                .has(&keys::funding_close_snapshot())
            {
                let snap = FundingCloseSnapshot {
                    total_principal: escrow.funded_amount,
                    funding_target: escrow.funding_target,
                    closed_at_ledger_timestamp: env.ledger().timestamp(),
                    closed_at_ledger_sequence: env.ledger().sequence(),
                };
                env.storage()
                    .instance()
                    .set(&keys::funding_close_snapshot(), &snap);
            }
        }

        Self::set_persistent_investor_contribution(&env, investor.clone(), new_contribution);

        if prev == 0 {
            env.storage().instance().set(
                &DataKey::UniqueFunderCount,
                &cur_funder_count.saturating_add(1),
            );

            let mut index: Vec<Address> = env
                .storage()
                .instance()
                .get(&keys::investor_index())
                .unwrap_or_else(|| Vec::new(&env));
            index.push_back(investor.clone());
            env.storage()
                .instance()
                .set(&keys::investor_index(), &index);
        }

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        // 4. Token transfer
        let token_addr = Self::funding_token_or_fail(&env);
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

        extend_ttl_for_activity(&env, &escrow, Some(investor.clone()));

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
        guard_not_disputed(&env, EscrowError::DisputeBlocksPartialSettle);

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
        if !env
            .storage()
            .instance()
            .has(&keys::funding_close_snapshot())
        {
            let snap = FundingCloseSnapshot {
                total_principal: escrow.funded_amount,
                funding_target: escrow.funding_target,
                closed_at_ledger_timestamp: env.ledger().timestamp(),
                closed_at_ledger_sequence: env.ledger().sequence(),
            };
            env.storage()
                .instance()
                .set(&keys::funding_close_snapshot(), &snap);
        }

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        EscrowPartialSettle {
            name: symbol_short!("part_set"),
            invoice_id: escrow.invoice_id.clone(),
            funded_amount: escrow.funded_amount,
        }
        .publish(&env);

        extend_ttl_for_activity(&env, &escrow, None);

        escrow
    }

    /// Finalizes a funded escrow into **settled** status (status `1 ΓåÆ 2`), recording the
    /// settled marker atomically **before** emitting the [`EscrowSettled`] event (checks-
    /// effects-interactions: the once-only guard and `SettledAt` write precede any outward
    /// effect). Settlement is strictly once-only ΓÇö a second call is rejected with the
    /// dedicated [`EscrowError::EscrowAlreadySettled`] typed error.
    pub fn settle(env: Env) -> SettlementResult {
        // Operational pause gate (read-only), before require_auth and orthogonal to legal hold.
        ensure(
            &env,
            !Self::paused_blocks(&env, PauseEntry::Settlement),
            EscrowError::PausedBlocksSettlement,
        );
        // Operational pause gate (read-only), before require_auth and orthogonal to legal hold.
        guard_not_paused(
            &env,
            EscrowError::PausedBlocksSettlement,
            PauseEntry::Settlement,
        );
        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksSettlement);
        guard_not_disputed(&env, EscrowError::DisputeBlocksSettlement);

        // env.clone(): env is used again after this call for ledger timestamp, storage set, and publish.
        let mut escrow = Self::load_escrow_require_sme(&env);

        // Once-only settlement guard. `settle` transitions status 1 ΓåÆ 2 and is the only
        // writer of the `SettledAt` marker, so `status == 2` uniquely identifies an escrow
        // that has already been settled. A re-entrant or replayed second call is rejected
        // here with a dedicated typed error *before* the funded-status gate, so a caller
        // can distinguish "already settled" from "not yet funded/open"
        // ([`EscrowError::SettlementNotFunded`]). This guard is total across all settlement
        // entrypoints: [`LiquifactEscrow::settle_batch`] invokes this same entrypoint per
        // target, so a duplicate address in a batch is rejected atomically.
        ensure(&env, escrow.status != 2, EscrowError::EscrowAlreadySettled);

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
        // coupon = funded_amount ├ù yield_bps / 10_000  (floor)
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

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        // Extend TTL based on lifecycle after settlement
        extend_ttl_for_activity(&env, &escrow, None);

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

        SettlementResult {
            escrow,
            coupon,
            settle_pool,
            settled_at: now,
        }
    }

    /// Batch settle entrypoint: settle multiple escrows in a single call.
    ///
    /// Each address is processed sequentially. All existing [`LiquifactEscrow::settle`]
    /// invariants (pause gate, legal hold, SME auth, funded status, maturity check, and the
    /// once-only [`EscrowError::EscrowAlreadySettled`] guard) are enforced per entry. The
    /// entire batch is atomic: if any escrow fails to settle, the entire call reverts.
    ///
    /// # Parameters
    /// - `escrows`: `Vec<Address>` of escrow contract addresses to settle.
    ///
    /// # Errors
    /// - [`EscrowError::SettlementBatchEmpty`] if `escrows` is empty
    /// - [`EscrowError::SettlementBatchTooLarge`] if `escrows.len() > [`MAX_SETTLE_BATCH`]
    ///
    /// Per-entry errors (not funded, maturity not reached, legal hold, paused, auth failure)
    /// terminate the entire batch atomically.
    ///
    /// # Events
    /// One [`EscrowSettled`] per successfully settled escrow (emitted by each target escrow's
    /// [`LiquifactEscrow::settle`] call).
    pub fn settle_batch(env: Env, escrows: Vec<Address>) {
        let n = escrows.len();
        ensure(&env, n > 0, EscrowError::SettlementBatchEmpty);
        ensure(
            &env,
            n <= MAX_SETTLE_BATCH,
            EscrowError::SettlementBatchTooLarge,
        );

        for i in 0..n {
            let escrow_addr = escrows.get(i).unwrap();
            let client = LiquifactEscrowClient::new(&env, &escrow_addr);
            client.settle();
        }
    }

    /// Admin releases funds to the SME up to the remaining obligation.
    ///
    /// The remaining obligation is defined as `funded_amount - released_amount`.
    /// Emits `PartialRelease` if the release is less than the remaining obligation,
    /// or `FinalRelease` if the release perfectly matches the remaining obligation.
    /// A final release transitions the escrow status to 3 (withdrawn).
    ///
    /// # Errors
    /// - [`EscrowError::ReleaseAmountNotPositive`] if amount <= 0.
    /// - [`EscrowError::ReleaseExceedsRemaining`] if amount > remaining obligation.
    /// - [`EscrowError::LegalHoldBlocksRelease`] if a legal hold is active.
    /// - [`EscrowError::PausedBlocksRelease`] if operational pause is active.
    /// - [`EscrowError::ReleaseNotFunded`] if status != 1.
    pub fn release(env: Env, amount: i128) -> InvoiceEscrow {
        ensure(&env, amount > 0, EscrowError::ReleaseAmountNotPositive);

        ensure(
            &env,
            !Self::paused_active(&env),
            EscrowError::PausedBlocksRelease,
        );
        guard_not_paused(&env, EscrowError::PausedBlocksRelease);
        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksRelease);

        // Load escrow, but require admin authorization.
        let escrow: InvoiceEscrow = env.storage().instance().get(&DataKey::Escrow).unwrap();
        escrow.admin.require_auth();

        guard_status_eq(&env, escrow.status, 1, EscrowError::ReleaseNotFunded);

        let released_amount: i128 = env
            .storage()
            .instance()
            .get(&keys::released_amount())
            .unwrap_or(0);

        let remaining = escrow.funded_amount.checked_sub(released_amount).unwrap();
        ensure(
            &env,
            amount <= remaining,
            EscrowError::ReleaseExceedsRemaining,
        );

        let mut next_escrow = escrow.clone();
        let is_final = amount == remaining;

        let new_released = released_amount.checked_add(amount).unwrap();
        env.storage()
            .instance()
            .set(&keys::released_amount(), &new_released);

        let sme = escrow.sme_address.clone();
        let token_addr: Address = Self::funding_token_or_fail(&env);

        let this = env.current_contract_address();
        let contract_balance = TokenClient::new(&env, &token_addr).balance(&this);
        ensure(
            &env,
            contract_balance >= amount,
            EscrowError::InsufficientContractBalance,
        );

        if is_final {
            next_escrow.status = 3;
            env.storage().instance().set(&DataKey::Escrow, &next_escrow);

            // Increase DistributedPrincipal by `amount` to account for funds leaving.
            let prev_distributed: i128 = env
                .storage()
                .instance()
                .get(&DataKey::DistributedPrincipal)
                .unwrap_or(0);
            env.storage().instance().set(
                &DataKey::DistributedPrincipal,
                &prev_distributed.saturating_add(amount),
            );

            FinalRelease {
                name: symbol_short!("fin_rel"),
                invoice_id: escrow.invoice_id.clone(),
                amount,
                recipient: sme.clone(),
            }
            .publish(&env);
        } else {
            let prev_distributed: i128 = env
                .storage()
                .instance()
                .get(&DataKey::DistributedPrincipal)
                .unwrap_or(0);
            env.storage().instance().set(
                &DataKey::DistributedPrincipal,
                &prev_distributed.saturating_add(amount),
            );

            PartialRelease {
                name: symbol_short!("part_rel"),
                invoice_id: escrow.invoice_id.clone(),
                amount,
                recipient: sme.clone(),
            }
            .publish(&env);
        }

        // Token transfer with SEP-41 balance-delta verification
        external_calls::transfer_funding_token_with_balance_checks(
            &env,
            &token_addr,
            &this,
            &sme,
            amount,
        );

        next_escrow
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
    /// 7. SEP-41 token transfers (fee ΓåÆ treasury, net ΓåÆ SME) with balance-delta verification.
    /// 8. Event emission ([`SmeWithdrew`], carrying `amount = sme_payout` and `fee`).
    ///
    /// # Errors
    /// - [`EscrowError::LegalHoldBlocksWithdrawal`] ΓÇö hold is active.
    /// - [`EscrowError::WithdrawalNotFunded`] ΓÇö escrow not in funded state.
    /// - [`EscrowError::InsufficientContractBalance`] ΓÇö contract holds less than `funded_amount`.
    /// - [`EscrowError::WithdrawFeeArithmeticOverflow`] ΓÇö `funded_amount * fee_bps` overflowed `i128`.
    /// - [`EscrowError::WithdrawNetArithmeticUnderflow`] ΓÇö `funded_amount - fee` underflowed (unreachable for in-range `fee_bps`).
    pub fn withdraw(env: Env) -> InvoiceEscrow {
        // Operational pause gate (read-only), orthogonal to legal hold.
        ensure(
            &env,
            !Self::paused_blocks(&env, PauseEntry::Withdrawal),
            EscrowError::PausedBlocksWithdrawal,
        );
        // Operational pause gate (read-only), before require_auth and orthogonal to legal hold.
        guard_not_paused(
            &env,
            EscrowError::PausedBlocksWithdrawal,
            PauseEntry::Withdrawal,
        );
        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksWithdrawal);
        guard_not_disputed(&env, EscrowError::DisputeBlocksWithdrawal);

        let mut escrow = Self::load_escrow_require_sme(&env);

        guard_status_eq(&env, escrow.status, 1, EscrowError::WithdrawalNotFunded);

        let released_amount: i128 = env
            .storage()
            .instance()
            .get(&keys::released_amount())
            .unwrap_or(0);
        let amount = escrow.funded_amount.checked_sub(released_amount).unwrap();
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

        let token_addr: Address = Self::funding_token_or_fail(&env);

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
        // be zero in the degenerate `fee_bps == 10_000` case ΓÇö guard it the same way.
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
    /// 3. Single contribution fetch ΓÇö eliminates the previous duplicate `get_contribution` call;
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
        // Operational pause gate (read-only), orthogonal to legal hold.
        ensure(
            &env,
            !Self::paused_blocks(&env, PauseEntry::Claims),
            EscrowError::PausedBlocksInvestorClaims,
        );
        // Operational pause gate (read-only), before require_auth and orthogonal to legal hold.
        guard_not_paused(
            &env,
            EscrowError::PausedBlocksInvestorClaims,
            PauseEntry::Claims,
        );
        guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksInvestorClaims);
        guard_not_disputed(&env, EscrowError::DisputeBlocksInvestorClaims);

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

        // Mark before transfer ΓÇö prevents double-pay on any re-entrant path.
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

        // Extend TTL after a successful claim
        extend_ttl_for_activity(&env, &escrow, Some(investor.clone()));
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
    /// None ΓÇö pure read; no auth required and no state mutation.
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
    /// coupon       = total_principal ├ù effective_yield_bps / 10_000  (floor)
    /// settle_pool  = total_principal + coupon
    /// gross_payout = contribution ├ù settle_pool / total_principal     (floor)
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
    /// The sum of `compute_investor_payout` over all investors is Γëñ `total_principal + coupon`;
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
    /// None ΓÇö pure read; no auth required.
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
            .get::<DataKey, FundingCloseSnapshot>(&keys::funding_close_snapshot())
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

        // coupon = total_principal ├ù effective_yield_bps / 10_000  (floor)
        let coupon = total_principal
            .checked_mul(effective_yield_bps as i128)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow))
            .checked_div(10_000)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow));

        let settle_pool = total_principal
            .checked_add(coupon)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow));

        // gross_payout = contribution ├ù settle_pool / total_principal  (floor)
        contribution
            .checked_mul(settle_pool)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow))
            .checked_div(total_principal)
            .unwrap_or_else(|| fail(&env, EscrowError::ComputePayoutArithmeticOverflow))
    }

    /// Authoritative on-chain aggregate view of the total settlement pool owed by the SME.
    ///
    /// Returns `total_principal + floor(total_principal ├ù yield_bps / 10_000)`, computed
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
    /// coupon       = total_principal ├ù yield_bps / 10_000  (floor)
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
    /// None ΓÇö pure read; no auth required and no state mutation.
    pub fn get_settlement_pool(env: Env) -> i128 {
        // Snapshot must exist (written when escrow first reaches status == 1).
        // Return 0 before funding, matching compute_investor_payout semantics.
        let Some(snap) = env
            .storage()
            .instance()
            .get::<DataKey, FundingCloseSnapshot>(&keys::funding_close_snapshot())
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

        // coupon = total_principal ├ù yield_bps / 10_000  (floor)
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

    /// Admin-only setter for the base yield rate in basis points.
    ///
    /// Updates [`InvoiceEscrow::yield_bps`] to `new_yield_bps`. Only valid while the
    /// escrow is in **open** status (`status == 0`) ΓÇö i.e. before any investor has
    /// funded and the escrow has been promoted to funded status. Once investors have
    /// committed principal the yield rate is effectively locked because per-investor
    /// effective yields may already have been recorded from the base rate.
    ///
    /// # Bounds
    /// `new_yield_bps` must be in `0..=10_000` (basis points; `10_000` = 100% yield).
    /// Out-of-range values fail with [`EscrowError::YieldBpsOutOfRange`].
    ///
    /// # Authorization
    /// Requires the signature of the current [`InvoiceEscrow::admin`].
    ///
    /// # Errors
    /// - [`EscrowError::YieldBpsUpdateNotOpen`] ΓÇö escrow is not in open status.
    /// - [`EscrowError::YieldBpsOutOfRange`] ΓÇö `new_yield_bps` is outside `0..=10_000`.
    /// - [`EscrowError::YieldBpsUnchanged`] ΓÇö `new_yield_bps` equals the current value.
    ///
    /// # Events
    /// Emits [`YieldBpsUpdatedEvent`] with `invoice_id`, `old_yield_bps`, and `new_yield_bps`.
    ///
    /// # Returns
    /// The updated [`InvoiceEscrow`] snapshot.
    pub fn update_yield_bps(env: Env, new_yield_bps: i64) -> InvoiceEscrow {
        let mut escrow = Self::load_escrow_require_admin(&env);

        guard_status_eq(&env, escrow.status, 0, EscrowError::YieldBpsUpdateNotOpen);

        ensure(
            &env,
            (0..=10_000).contains(&new_yield_bps),
            EscrowError::YieldBpsOutOfRange,
        );

        ensure(
            &env,
            new_yield_bps != escrow.yield_bps,
            EscrowError::YieldBpsUnchanged,
        );

        let old_yield_bps = escrow.yield_bps;
        escrow.yield_bps = new_yield_bps;

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        YieldBpsUpdatedEvent {
            name: symbol_short!("yld_upd"),
            invoice_id: escrow.invoice_id.clone(),
            old_yield_bps,
            new_yield_bps,
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
            .get::<DataKey, u64>(&keys::funding_deadline())
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
        let ttl = Self::get_storage_limit(env.clone());
        env.storage().instance().extend_ttl(ttl, ttl);

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

    /// Monotonically **raise** the maturity-max-horizon ceiling ΓÇö a forward-only governance lever.
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

    /// Return the admin-configured storage TTL extension horizon in ledgers.
    ///
    /// Falls back to [`INSTANCE_TTL_MIN_EXTENSION_LEDGERS`] when [`DataKey::StorageLimit`]
    /// is unset, preserving the historical hard-coded bump amount.
    pub fn get_storage_limit(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::StorageLimit)
            .unwrap_or(INSTANCE_TTL_MIN_EXTENSION_LEDGERS)
    }

    /// Set the storage TTL extension horizon used by [`LiquifactEscrow::bump_ttl`]
    /// and funding-deadline TTL top-ups.
    ///
    /// # Authorization
    /// Requires the signature of the current [`InvoiceEscrow::admin`].
    ///
    /// # Errors
    /// - [`EscrowError::StorageLimitOutOfRange`] if `limit` is outside
    ///   [`MIN_STORAGE_LIMIT_LEDGERS`]..=[`MAX_STORAGE_LIMIT_LEDGERS`].
    ///
    /// # Returns
    /// The newly stored limit.
    pub fn set_storage_limit(env: Env, limit: u32) -> u32 {
        let _escrow = Self::load_escrow_require_admin(&env);

        ensure(
            &env,
            (MIN_STORAGE_LIMIT_LEDGERS..=MAX_STORAGE_LIMIT_LEDGERS).contains(&limit),
            EscrowError::StorageLimitOutOfRange,
        );

        env.storage().instance().set(&DataKey::StorageLimit, &limit);

        limit
    }

    pub fn bump_ttl(env: Env, allowlisted: Vec<Address>) {
        let len = allowlisted.len();
        ensure(&env, len > 0, EscrowError::BumpTtlBatchEmpty);
        ensure(
            &env,
            len <= MAX_BUMP_TTL_BATCH,
            EscrowError::BumpTtlBatchTooLarge,
        );

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

        // Lifecycle-based horizon.
        let escrow = Self::get_escrow(env.clone());
        let ttl = get_lifecycle_ttl(&escrow);

        // Extend persistent TTL for allowlisted investor entries.
        for addr in allowlisted.iter() {
            // Persistent allowlist entry.
            env.storage().persistent().extend_ttl(
                &DataKey::InvestorAllowlisted(addr.clone()),
                ttl,
                ttl,
            );
            // Instance keys that may be perΓÇæinvestor (contribution & claim lock).
            env.storage().instance().extend_ttl(ttl, ttl);
        }

        // Instance storage TTL is contract-wide under Soroban SDK 25. The call above covers
        // Escrow, Version, LegalHold, snapshots, caps, and other instance keys.

        // Persistent per-investor keys and allowlist entries (independent TTL per address).
        for addr in allowlisted.iter() {
            let k = DataKey::InvestorAllowlisted(addr.clone());
            env.storage().persistent().extend_ttl(&k, ttl, ttl);
            // Extend persistent TTL for per-investor persistent keys used by this contract.
            env.storage().persistent().extend_ttl(
                &DataKey::InvestorContribution(addr.clone()),
                ttl,
                ttl,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::InvestorEffectiveYield(addr.clone()),
                ttl,
                ttl,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::InvestorClaimNotBefore(addr.clone()),
                ttl,
                ttl,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::InvestorClaimed(addr.clone()),
                ttl,
                ttl,
            );
        }
    }

    /// Extend TTL for a bounded set of storage keys in one admin-authenticated call.
    ///
    /// This is the admin-gated counterpart to [`LiquifactEscrow::bump_ttl`]. It gives the
    /// escrow operator a single-call path to extend the TTL of any combination of storage
    /// keys without having to issue separate transactions per key. The per-call ceiling
    /// ([`MAX_BUMP_TTL_BATCH`]) keeps storage and CPU work predictable and consistent with
    /// the rest of the admin-batch API surface.
    ///
    /// # Behavior
    ///
    /// For each [`DataKey`] in `keys`:
    ///
    /// - **Per-investor persistent keys** (`InvestorContribution`, `InvestorEffectiveYield`,
    ///   `InvestorClaimNotBefore`, `InvestorClaimed`, `InvestorAllowlisted`):
    ///   `env.storage().persistent().extend_ttl(&key, ΓÇª)` is called **only when the key
    ///   already exists** in persistent storage (guarded by `has()` before the call).
    ///   Absent keys are silently skipped ΓÇö an investor who has been allowlisted but not
    ///   yet funded will not have `InvestorContribution` written; requesting it is a no-op
    ///   rather than an error.
    /// - **All other keys** (instance storage): a single
    ///   `env.storage().instance().extend_ttl(ΓÇª)` is issued. Because instance-storage TTL
    ///   is contract-wide under the Soroban SDK, any non-persistent key in `keys` triggers
    ///   the same net effect. Repeating the call within a batch is harmless.
    ///
    /// No funds move; no escrow status changes. This is a pure storage-maintenance
    /// operation.
    ///
    /// # Authorization
    ///
    /// **Admin only.** Requires the current [`InvoiceEscrow::admin`] to sign.
    /// Unlike [`LiquifactEscrow::bump_ttl`] (which is permissionless), this entrypoint
    /// intentionally limits callers to the admin role so that arbitrary accounts cannot
    /// induce unexpected compute costs on operator-controlled escrows.
    ///
    /// # Errors
    /// Emits typed [`EscrowError`] codes when the escrow is uninitialized or `new_admin` is the
    /// current admin.
    pub fn propose_admin(env: Env, new_admin: Address, expected_nonce: u32) -> Address {
        let escrow = Self::load_escrow_require_admin(&env);
        Self::consume_admin_nonce(&env, expected_nonce);

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

    /// Accept a pending admin handover ΓÇö step 2 of a two-step handover.
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
    pub fn transfer_admin(env: Env, new_admin: Address, expected_nonce: u32) -> InvoiceEscrow {
        Self::propose_admin(env.clone(), new_admin.clone(), expected_nonce);

        // Re-read after the propose_admin delegation so the deprecation event
        // carries the exact `invoice_id` indexers will see in the prior
        // `AdminProposedEvent`. Reading after (not before) keeps a single
        // extra instance-storage read, since `propose_admin` does not expose
        // its loaded escrow back through the return type.
        let escrow = Self::get_escrow(env.clone());

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
    /// - [`EscrowError::NoPendingAdmin`] ΓÇö no proposal exists; nothing to cancel.
    ///
    /// # Returns
    ///
    /// The revoked pending address, so callers can record it off-chain without a
    /// separate read.
    ///
    /// # Events
    ///
    /// Emits [`AdminProposalCancelled`] carrying `invoice_id` and `cancelled_pending`.
    pub fn cancel_pending_admin(env: Env, expected_nonce: u32) -> Address {
        let escrow = Self::load_escrow_require_admin(&env);
        Self::consume_admin_nonce(&env, expected_nonce);

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

    /// Recover from an abandoned admin-transfer proposal after its timelock has elapsed.
    ///
    /// The current [`InvoiceEscrow::admin`] may clear a pending successor proposal once
    /// [`DataKey::PendingAdminExpiry`] is in the past. This is the bounded recovery path
    /// for the case where the proposed administrator becomes unreachable and cannot
    /// call [`LiquifactEscrow::accept_admin`].
    ///
    /// # Authorization
    ///
    /// **Admin only.** Requires the current [`InvoiceEscrow::admin`] to sign (via
    /// [`LiquifactEscrow::load_escrow_require_admin`]).
    ///
    /// # Arguments
    /// - `reason`: explicit human-readable reason for the recovery, emitted in
    ///   [`AdminRecoveredEvent`] for auditability. It is not stored.
    ///
    /// # Errors
    /// - [`EscrowError::NoPendingAdmin`] if no proposal is pending (including a repeated
    ///   recovery after a successful recovery).
    /// - [`EscrowError::AdminRecoveryNotExpired`] if the proposal timelock has not elapsed.
    ///
    /// # Events
    /// Emits [`AdminRecoveredEvent`] (topic: `adm_rec`).
    pub fn recover_admin(env: Env, reason: String) -> Address {
        let escrow = Self::load_escrow_require_admin(&env);

        let pending: Option<Address> = env.storage().instance().get(&DataKey::PendingAdmin);
        ensure(&env, pending.is_some(), EscrowError::NoPendingAdmin);
        let pending = pending.unwrap();

        let expiry: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdminExpiry)
            .unwrap_or_else(|| fail(&env, EscrowError::AdminRecoveryNotExpired));
        let now = env.ledger().timestamp();
        ensure(&env, now > expiry, EscrowError::AdminRecoveryNotExpired);

        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.storage()
            .instance()
            .remove(&DataKey::PendingAdminExpiry);

        AdminRecoveredEvent {
            name: symbol_short!("adm_rec"),
            invoice_id: escrow.invoice_id.clone(),
            current_admin: escrow.admin,
            abandoned_pending: pending.clone(),
            reason,
        }
        .publish(&env);

        pending
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
    pub fn cancel_funding(env: Env, expected_nonce: u32) -> InvoiceEscrow {
        Self::guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksCancelFunding);

        let mut escrow = Self::load_escrow_require_admin(&env);
        Self::consume_admin_nonce(&env, expected_nonce);

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
        guard_not_disputed(&env, EscrowError::DisputeBlocksRefund);
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
    /// **skipped** without failing the batch ΓÇö this makes the entrypoint idempotent and
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
    /// silently skipped ΓÇö they terminate the entire batch. Only already-refunded entries
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
    /// - `amount`: The positive amount to withdraw (must be Γëñ recorded contribution).
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
        guard_not_disputed(&env, EscrowError::DisputeBlocksUnfund);

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
        // checked_sub on a negative amount would yield a value > contribution ΓÇö still caught
        // above ΓÇö but a zero withdrawal is explicitly rejected here for clarity.
        if amount <= 0 {
            fail(&env, EscrowError::OverWithdrawal);
        }

        // 5. funded_amount decrement.
        let new_funded_amount = escrow
            .funded_amount
            .checked_sub(amount)
            .unwrap_or_else(|| fail(&env, EscrowError::OverWithdrawal));

        // 6. Effects ΓÇö update contribution (checks-effects-interactions).
        Self::set_persistent_investor_contribution(&env, investor.clone(), remaining_contribution);

        // 7. Decrement UniqueFunderCount when contribution reaches zero.
        if remaining_contribution == 0 {
            let cur: u32 = env
                .storage()
                .instance()
                .get(&keys::unique_funder_count())
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&keys::unique_funder_count(), &cur.saturating_sub(1));
        }

        // 8. Persist updated escrow.
        escrow.funded_amount = new_funded_amount;
        env.storage().instance().set(&DataKey::Escrow, &escrow);

        // 9. Token transfer (interactions last ΓÇö checks-effects-interactions pattern).
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
    /// Absent ΓçÆ `0` (no refunds have occurred).
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

    /// Register a pending cross-contract callback with an authorized origin contract and expected phase.
    ///
    /// Allocates the next monotonic invocation nonce, stores the [`CallbackContext`] in instance
    /// storage, and emits [`CallbackRegisteredEvent`].
    ///
    /// # Authorization
    /// Requires the signature of the current [`InvoiceEscrow::admin`].
    ///
    /// # Errors
    /// - [`EscrowError::EscrowNotInitialized`] if escrow storage is missing.
    /// - [`EscrowError::CallbackAfterCancellation`] if the escrow status is 4 (cancelled).
    ///
    /// # Returns
    /// The allocated unique invocation nonce (`u64`).
    pub fn register_callback(env: Env, origin: Address, phase: u32) -> u64 {
        let escrow = Self::load_escrow_require_admin(&env);

        ensure(
            &env,
            escrow.status != 4,
            EscrowError::CallbackAfterCancellation,
        );

        let current_nonce: u64 = env
            .storage()
            .instance()
            .get(&DataKey::CallbackNonce)
            .unwrap_or(0);
        let next_nonce = current_nonce
            .checked_add(1)
            .unwrap_or_else(|| fail(&env, EscrowError::FundedAmountOverflow));

        env.storage()
            .instance()
            .set(&DataKey::CallbackNonce, &next_nonce);

        let now = env.ledger().timestamp();
        let context = CallbackContext {
            origin: origin.clone(),
            nonce: next_nonce,
            phase,
            created_at: now,
            consumed: false,
        };

        env.storage()
            .instance()
            .set(&DataKey::CallbackContext(next_nonce), &context);

        CallbackRegisteredEvent {
            name: symbol_short!("cb_reg"),
            invoice_id: escrow.invoice_id,
            origin,
            nonce: next_nonce,
            phase,
        }
        .publish(&env);

        next_nonce
    }

    /// Execute and consume a registered cross-contract callback.
    ///
    /// Validates that:
    /// 1. The escrow is not in cancelled status (`status != 4`).
    /// 2. The caller authorized as `origin` matches the registered origin address.
    /// 3. The callback context exists for `nonce`.
    /// 4. The callback has not already been consumed (replay protection).
    /// 5. The registered `nonce` matches the supplied `nonce`.
    /// 6. The registered `phase` matches the supplied `phase`.
    ///
    /// State mutation: marks `consumed = true` on the context and updates storage atomically
    /// before emitting [`CallbackExecutedEvent`].
    ///
    /// # Authorization
    /// Requires authorization from `origin` (`origin.require_auth()`).
    ///
    /// # Errors
    /// - [`EscrowError::CallbackAfterCancellation`] if escrow is cancelled (status 4).
    /// - [`EscrowError::CallbackNotFound`] if no context exists for `nonce`.
    /// - [`EscrowError::CallbackReplayed`] if the callback has already been consumed.
    /// - [`EscrowError::CallbackWrongOrigin`] if caller `origin` does not match the registered origin.
    /// - [`EscrowError::CallbackWrongNonce`] if `nonce` does not match the stored context nonce.
    /// - [`EscrowError::CallbackWrongPhase`] if `phase` does not match the expected phase.
    ///
    /// # Returns
    /// The updated [`CallbackContext`] snapshot with `consumed == true`.
    pub fn execute_callback(env: Env, nonce: u64, origin: Address, phase: u32) -> CallbackContext {
        let escrow = Self::get_escrow(env.clone());

        ensure(
            &env,
            escrow.status != 4,
            EscrowError::CallbackAfterCancellation,
        );

        origin.require_auth();

        let mut context: CallbackContext = env
            .storage()
            .instance()
            .get(&DataKey::CallbackContext(nonce))
            .unwrap_or_else(|| fail(&env, EscrowError::CallbackNotFound));

        ensure(&env, !context.consumed, EscrowError::CallbackReplayed);

        ensure(
            &env,
            context.origin == origin,
            EscrowError::CallbackWrongOrigin,
        );

        ensure(
            &env,
            context.nonce == nonce,
            EscrowError::CallbackWrongNonce,
        );

        ensure(
            &env,
            context.phase == phase,
            EscrowError::CallbackWrongPhase,
        );

        context.consumed = true;
        env.storage()
            .instance()
            .set(&DataKey::CallbackContext(nonce), &context);

        CallbackExecutedEvent {
            name: symbol_short!("cb_exec"),
            invoice_id: escrow.invoice_id,
            origin,
            nonce,
            phase,
        }
        .publish(&env);

        context
    }

    /// Read-only view returning the registered callback context for a given invocation `nonce`.
    /// Returns `None` if no callback was registered with this nonce.
    pub fn get_callback(env: Env, nonce: u64) -> Option<CallbackContext> {
        env.storage()
            .instance()
            .get(&DataKey::CallbackContext(nonce))
    }

    /// Read-only view returning the current callback invocation nonce counter.
    /// Returns `0` if no callbacks have been registered.
    pub fn get_callback_nonce(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::CallbackNonce)
            .unwrap_or(0)
    }

    /// Read-only view returning whether the callback for `nonce` has been consumed.
    /// Returns `false` if the callback is unconsumed or does not exist.
    pub fn is_callback_consumed(env: Env, nonce: u64) -> bool {
        let context: Option<CallbackContext> = env
            .storage()
            .instance()
            .get(&DataKey::CallbackContext(nonce));
        context.map(|c| c.consumed).unwrap_or(false)
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

// Test module tree: submodules are reconciled with the current lib API and
// must stay green under `cargo test`.
// #[cfg(test)]
// mod test_allowlist_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod init_reentry_guard_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn sample_escrow(env: &Env) -> InvoiceEscrow {
        InvoiceEscrow {
            invoice_id: symbol_short!("inv"),
            admin: Address::generate(env),
            sme_address: Address::generate(env),
            amount: 1_000,
            funding_target: 1_000,
            funded_amount: 0,
            yield_bps: 0,
            maturity: 0,
            status: 0,
        }
    }

    fn with_contract<R>(env: &Env, f: impl FnOnce() -> R) -> R {
        let contract_id = env.register(LiquifactEscrow, ());
        env.as_contract(&contract_id, f)
    }

    #[test]
    fn first_initialization_is_allowed() {
        let env = Env::default();
        with_contract(&env, || ensure_not_initialized(&env));
    }

    #[test]
    fn same_parameters_again_rejected() {
        let env = Env::default();
        with_contract(&env, || {
            env.storage()
                .instance()
                .set(&DataKey::Escrow, &sample_escrow(&env));
            env.storage()
                .instance()
                .set(&DataKey::Version, &SCHEMA_VERSION);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ensure_not_initialized(&env);
            }));
            assert!(result.is_err());
            assert!(env.storage().instance().has(&DataKey::Escrow));
            assert!(env.storage().instance().has(&DataKey::Version));
        });
    }

    #[test]
    fn different_admin_rejected() {
        let env = Env::default();
        with_contract(&env, || {
            env.storage()
                .instance()
                .set(&DataKey::Escrow, &sample_escrow(&env));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ensure_not_initialized(&env);
            }));
            assert!(result.is_err());
        });
    }

    #[test]
    fn different_token_rejected() {
        let env = Env::default();
        with_contract(&env, || {
            env.storage()
                .instance()
                .set(&DataKey::Version, &SCHEMA_VERSION);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ensure_not_initialized(&env);
            }));
            assert!(result.is_err());
        });
    }

    #[test]
    fn initialization_during_another_call_rejected() {
        let env = Env::default();
        with_contract(&env, || {
            env.storage()
                .instance()
                .set(&DataKey::Version, &SCHEMA_VERSION);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ensure_not_initialized(&env);
            }));
            assert!(result.is_err());
        });
    }
}

#[cfg(test)]
mod test_allowlist_tests;

#[cfg(test)]
mod callback_binding_tests;

#[cfg(test)]
mod release_budget_tests;

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
        let mut balances: soroban_sdk::Map<soroban_sdk::Address, i128> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| soroban_sdk::Map::new(&env));
        balances.get(addr).unwrap_or(100_000_000_000_000i128)
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
            .unwrap_or(100_000_000_000_000i128);
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
        env.register_at(token_addr, DefaultMockToken, ());
    }
}
