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
//! and is the canonical source of truth for upgrade decisions. **Current value: 5.**
//!
//! [`LiquifactEscrow::migrate`] **panics in all current execution paths** — no silent migration
//! work is promised or performed. Operators must extend `migrate` before calling it, or redeploy
//! when stored struct layout changes. See `docs/OPERATOR_RUNBOOK.md` for the full decision tree.
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
//! **Recovery lever:** [`LiquifactEscrow::transfer_admin`] is **not** gated by the hold.
//! Governance rotates to a new admin, then the new admin clears the hold. Invariant: a hold
//! is always clearable by whoever holds `InvoiceEscrow::admin`; recovery requires controlling
//! that authority. See `docs/escrow-legal-hold.md` and [ADR-004](docs/adr/ADR-004-legal-hold.md).
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
//! escrow has reached a **terminal** [`InvoiceEscrow::status`] (settled or withdrawn). It cannot run
//! during a legal hold. Transfers go through [`crate::external_calls`] so **pre/post token balances**
//! must match the requested amount (standard SEP-41 behavior); fee-on-transfer or malicious tokens
//! are **explicitly out of scope** and will cause safe-failure panics at the balance-check boundary.
//! This is meant for rounding residue / stray transfers, not for settling live liabilities —
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
    contract, contractevent, contractimpl, contracttype, symbol_short, token::TokenClient, Address,
    BytesN, Env, String, Symbol, Vec,
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
///
/// See `docs/OPERATOR_RUNBOOK.md` for the full redeploy-vs-upgrade decision tree.
pub const SCHEMA_VERSION: u32 = 5;

/// Upper bound on [`LiquifactEscrow::append_attestation_digest`] entries to keep storage bounded.
pub const MAX_ATTESTATION_APPEND_ENTRIES: u32 = 32;

/// Upper bound on batch allowlist mutation entries to keep storage/CPU bounded.
/// Mirrors the spirit of `MAX_ATTESTATION_APPEND_ENTRIES` to limit per-call work.
pub const MAX_INVESTOR_ALLOWLIST_BATCH: u32 = 32;

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
pub const INSTANCE_TTL_MIN_EXTENSION_SECS: u64 = 60 * 60; // 1h

/// Minimum persistent storage TTL extension horizon for per-investor allowlist entries.
///
/// When the escrow uses the allowlist gate, investor funding depends on persistent entries.
/// Extending persistent allowlist TTL reduces the risk of silent allowlist disablement.
pub const PERSISTENT_TTL_MIN_EXTENSION_SECS: u64 = 60 * 60; // 1h


// --- Storage keys ---

#[contracttype]
#[derive(Clone)]
/// Storage discriminator for all persisted values in Soroban instance storage.
///
/// Every variant maps to a distinct XDR-encoded key in the contract’s instance storage map.
/// Optional and per-address keys are always read with `.get(...).unwrap_or(default)` so that
/// deployments predating a key behave as “unset / default” without panicking.
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
    /// Absent ⇒ `0`. One entry per investor address.
    InvestorContribution(Address),
    /// When true, compliance/legal hold blocks payouts and settlement finalization.
    /// Absent ⇒ `false` (no hold). Toggled by admin via [`LiquifactEscrow::set_legal_hold`].
    LegalHold,
    /// Optional SME collateral commitment metadata (record-only — not an on-chain asset lock).
    /// Absent when no commitment has been recorded. Replaceable by the SME.
    SmeCollateralPledge,
    /// Set to `true` when an investor has exercised a claim after settlement.
    /// Absent ⇒ `false`. Written once; a second claim panics.
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
    /// Absent ⇒ falls back to [`InvoiceEscrow::yield_bps`]. One entry per investor address.
    InvestorEffectiveYield(Address),
    /// Minimum [`Env::ledger`] timestamp before [`LiquifactEscrow::claim_investor_payout`] (0 = no extra gate).
    /// Absent ⇒ `0`. One entry per investor address; set on first deposit.
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
    /// Count of distinct investor addresses that have a non-zero [`DataKey::InvestorContribution`].
    /// Written as `0` at init; incremented once per new investor in `fund_impl`.
    UniqueFunderCount,
    /// Admin-only **single-set** off-chain attestation digest (e.g. SHA-256 of a legal/KYC bundle).
    /// Absent until [`LiquifactEscrow::bind_primary_attestation_hash`] is called; single-set thereafter.
    PrimaryAttestationHash,
    /// Append-only audit chain of digests (bounded by [`MAX_ATTESTATION_APPEND_ENTRIES`]).
    /// Absent ⇒ empty log. See [`LiquifactEscrow::append_attestation_digest`].
    AttestationAppendLog,
    /// When true, only allowlisted addresses may call [`LiquifactEscrow::fund`] or [`LiquifactEscrow::fund_with_commitment`].
    AllowlistActive,
    /// Whether a specific address is permitted to fund when [`DataKey::AllowlistActive`] is true.
    InvestorAllowlisted(Address),
    /// Set to `true` once an investor's principal has been refunded in a cancelled escrow.
    /// Absent ⇒ `false`. Written once; prevents double-refund.
    InvestorRefunded(Address),
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
/// settlement, SME withdrawal, investor-claim, or treasury-sweep behavior. Future versions that
/// enforce asset movement or custody must introduce explicit APIs and must not treat historical
/// records from this type as proof of locked assets.
#[contracttype]
#[derive(Debug, PartialEq)]
/// SME collateral commitment metadata (record-only).
///
/// Derive rationale:
/// - `Debug`: improves failure diagnostics in tests.
/// - `PartialEq`: allows deterministic assertion of stored/read values.
///
/// `Clone` is intentionally omitted to avoid accidental large-value duplication.
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

/// Captured at the first ledger transition to **funded** so partial settlement / claims can use a
/// stable total principal and target. **Immutable** once written.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FundingCloseSnapshot {
    /// Sum of principal credited when the invoice became funded (`funded_amount` at close), including overflow past target.
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

/// Comprehensive summary of the escrow contract state.
/// Bundles multiple read-only values to allow a single host invocation
/// for off-chain indexers and client rendering.
#[contracttype]
#[derive(Debug, PartialEq)]
pub struct EscrowSummary {
    /// Full escrow snapshot.
    pub escrow: InvoiceEscrow,
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
pub struct FundingTargetUpdated {
    #[topic]
    pub name: Symbol,
    #[topic]
    pub invoice_id: Symbol,
    pub old_target: i128,
    pub new_target: i128,
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

/// SME collateral commitment metadata recorded.
///
/// This event means only that [`DataKey::SmeCollateralPledge`] was written by the SME. It is not
/// proof of custody, lien, encumbrance, asset control, or token movement. The event intentionally
/// omits token contract, custodian, and transfer-receipt fields so consumers do not treat it as an
/// on-chain encumbrance.
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

#[contract]
pub struct LiquifactEscrow;

fn validate_invoice_id_string(env: &Env, invoice_id: &String) -> Symbol {
    let len = invoice_id.len();
    assert!(
        (1..=MAX_INVOICE_ID_STRING_LEN).contains(&len),
        "invoice_id length must be 1..=MAX_INVOICE_ID_STRING_LEN"
    );
    let len_u = len as usize;
    let mut buf = [0u8; 32];
    invoice_id.copy_into_slice(&mut buf[..len_u]);
    for &b in &buf[..len_u] {
        let ok =
            b.is_ascii_uppercase() || b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_';
        assert!(
            ok,
            "invoice_id must be [A-Za-z0-9_] only (Soroban Symbol charset subset)"
        );
    }
    let s = core::str::from_utf8(&buf[..len_u]).expect("invoice_id ascii");
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

    fn validate_yield_tiers_table(tiers: &Option<Vec<YieldTier>>, base_yield: i64) {
        let Some(tiers) = tiers else {
            return;
        };
        if tiers.is_empty() {
            return;
        }
        let n = tiers.len();
        for i in 0..n {
            let t = tiers.get(i).unwrap();
            assert!(
                (0..=10_000).contains(&t.yield_bps),
                "tier yield_bps must be 0..=10_000"
            );
            assert!(
                t.yield_bps >= base_yield,
                "tier yield_bps must be >= base yield_bps"
            );
            if i > 0 {
                let p = tiers.get(i - 1).unwrap();
                assert!(
                    t.min_lock_secs > p.min_lock_secs,
                    "tiers must have strictly increasing min_lock_secs"
                );
                assert!(
                    t.yield_bps >= p.yield_bps,
                    "tiers must have non-decreasing yield_bps"
                );
            }
        }
    }

    fn effective_yield_for_commitment(env: &Env, base_yield: i64, committed_lock_secs: u64) -> i64 {
        if committed_lock_secs == 0 {
            return base_yield;
        }
        let Some(tiers) = env
            .storage()
            .instance()
            .get::<DataKey, Vec<YieldTier>>(&DataKey::YieldTierTable)
        else {
            return base_yield;
        };
        if tiers.is_empty() {
            return base_yield;
        }
        let mut best = base_yield;
        let n = tiers.len();
        for i in 0..n {
            let t = tiers.get(i).unwrap();
            if committed_lock_secs >= t.min_lock_secs && t.yield_bps > best {
                best = t.yield_bps;
            }
        }
        best
    }

    /// Initialize escrow. `funding_target` defaults to `amount`.
    ///
    /// Binds **`funding_token`**, **`treasury`**, and optional **`registry`** for this instance only.
    /// The funding token and treasury addresses are **immutable** after this call; the registry id is
    /// optional metadata for off-chain indexers (not an on-chain authority).
    ///
    /// `invoice_id` must satisfy [`MAX_INVOICE_ID_STRING_LEN`] and charset rules (see
    /// [`validate_invoice_id_string`]).
    ///
    /// # Panics
    /// If `amount` or implied target is not positive, `yield_bps > 10_000`, invoice id invalid,
    /// or escrow exists.
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
    ) -> InvoiceEscrow {
        admin.require_auth();

        assert!(amount > 0, "Amount must be positive");
        assert!(
            (0..=10_000).contains(&yield_bps),
            "yield_bps must be between 0 and 10_000"
        );
        assert!(
            !env.storage().instance().has(&DataKey::Escrow),
            "Escrow already initialized"
        );

        Self::validate_yield_tiers_table(&yield_tiers, yield_bps);

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
        env.storage()
            .instance()
            .set(&DataKey::Version, &SCHEMA_VERSION);
        env.storage()
            .instance()
            .set(&DataKey::FundingToken, &funding_token);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        if let Some(ref r) = registry {
            env.storage().instance().set(&DataKey::RegistryRef, r);
        }
        if let Some(ref tiers) = yield_tiers {
            if !tiers.is_empty() {
                env.storage()
                    .instance()
                    .set(&DataKey::YieldTierTable, tiers);
            }
        }

        let floor = min_contribution.unwrap_or(0);
        if min_contribution.is_some() {
            assert!(
                floor > 0,
                "min_contribution must be positive when configured"
            );
            assert!(
                floor <= amount,
                "min_contribution cannot exceed initial invoice amount / target hint"
            );
        }
        env.storage()
            .instance()
            .set(&DataKey::MinContributionFloor, &floor);

        env.storage()
            .instance()
            .set(&DataKey::UniqueFunderCount, &0u32);

        if let Some(cap) = max_per_investor {
            assert!(cap > 0, "max_per_investor must be positive when configured");
            env.storage()
                .instance()
                .set(&DataKey::MaxPerInvestorCap, &cap);
        }

        if let Some(cap) = max_unique_investors {
            assert!(
                cap > 0,
                "max_unique_investors must be positive when configured"
            );
            env.storage()
                .instance()
                .set(&DataKey::MaxUniqueInvestorsCap, &cap);
        }

        EscrowInitialized {
            name: symbol_short!("escrow_ii"),
            // Read stored values so event fields match persisted keys (indexer single-event bootstrap).
            escrow: Self::get_escrow(env.clone()),
            funding_token: Self::get_funding_token(env.clone()),
            treasury: Self::get_treasury(env.clone()),
            registry: Self::get_registry_ref(env.clone()),
        }
        .publish(&env);

        escrow
    }

    /// Returns the SEP-41 funding token bound at [`LiquifactEscrow::init`] ([`DataKey::FundingToken`]).
    ///
    /// **Immutable:** set once at init; cannot change after deploy. Panics if called before init.
    pub fn get_funding_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::FundingToken)
            .unwrap_or_else(|| panic!("Funding token not set"))
    }

    /// Returns the protocol treasury address bound at [`LiquifactEscrow::init`] ([`DataKey::Treasury`]).
    ///
    /// **Immutable:** set once at init; cannot change after deploy. The treasury is the only
    /// recipient of [`LiquifactEscrow::sweep_terminal_dust`]. Panics if called before init.
    pub fn get_treasury(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Treasury)
            .unwrap_or_else(|| panic!("Treasury not set"))
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

    /// Move up to `amount` (capped by balance and [`MAX_DUST_SWEEP_AMOUNT`]) of the **funding token**
    /// from this contract to [`DataKey::Treasury`].
    ///
    /// # Terminal state requirement
    /// Only permitted when [`InvoiceEscrow::status`] is **2 (settled)** or **3 (withdrawn)**.
    /// Open (0) or funded (1) states reject the call so live principal cannot be swept as dust.
    ///
    /// # Authorization
    /// The configured **treasury** account must authorize this call; the admin cannot sweep unless
    /// it is also the treasury.
    ///
    /// Blocked while [`DataKey::LegalHold`] is active.
    pub fn sweep_terminal_dust(env: Env, amount: i128) -> i128 {
        assert!(
            !Self::legal_hold_active(&env),
            "Legal hold blocks treasury dust sweep"
        );
        assert!(amount > 0, "sweep amount must be positive");
        assert!(
            amount <= MAX_DUST_SWEEP_AMOUNT,
            "sweep amount exceeds MAX_DUST_SWEEP_AMOUNT"
        );

        // env.clone(): env is used again after this call for treasury/token reads and publish.
        let escrow = Self::get_escrow(env.clone());
        assert!(
            escrow.status == 2 || escrow.status == 3 || escrow.status == 4,
            "dust sweep only in terminal states (settled, withdrawn, or cancelled)"
        );

        let treasury: Address = env
            .storage()
            .instance()
            .get(&DataKey::Treasury)
            .expect("treasury must be initialized");
        treasury.require_auth();

        let token_addr = env
            .storage()
            .instance()
            .get(&DataKey::FundingToken)
            .expect("funding token must be initialized");
        let this = env.current_contract_address();

        let token = TokenClient::new(&env, &token_addr);
        let balance = token.balance(&this);
        assert!(balance > 0, "no funding token balance to sweep");
        let sweep_amt = amount.min(balance);
        assert!(sweep_amt > 0, "effective sweep amount is zero");

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
            token: token_addr,
            amount: sweep_amt,
        }
        .publish(&env);

        sweep_amt
    }

    pub fn get_escrow(env: Env) -> InvoiceEscrow {
        env.storage()
            .instance()
            .get(&DataKey::Escrow)
            .unwrap_or_else(|| panic!("Escrow not initialized"))
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Version).unwrap_or(0)
    }

    /// Whether a compliance/legal hold is active (defaults to `false` if unset).
    pub fn get_legal_hold(env: Env) -> bool {
        Self::legal_hold_active(&env)
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
        env.storage()
            .instance()
            .get(&DataKey::MaxPerInvestorCap)
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
        let schema_version = Self::get_version(env);

        let funding_close_snapshot = match funding_close_snapshot_opt {
            Some(snap) => EscrowCloseSnapshot::Some(snap),
            None => EscrowCloseSnapshot::None,
        };

        EscrowSummary {
            escrow,
            legal_hold,
            funding_close_snapshot,
            unique_funder_count,
            is_allowlist_active,
            schema_version,
        }
    }

    /// Bind a **primary** 32-byte digest (e.g. SHA-256 of an IPFS CID or document bundle). **Single-set:**
    /// the call succeeds only while no primary hash exists; use [`LiquifactEscrow::append_attestation_digest`]
    /// for an append-only audit trail.
    ///
    /// **Authorization:** [`InvoiceEscrow::admin`]. **Frontrunning:** whichever binding transaction lands
    /// first wins; observers must read on-chain state (or parse events) after finality—there is no replay lock.
    pub fn bind_primary_attestation_hash(env: Env, digest: BytesN<32>) {
        // env.clone(): env is used again after this call for storage has/set and publish.
        let escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();
        assert!(
            !env.storage()
                .instance()
                .has(&DataKey::PrimaryAttestationHash),
            "primary attestation already bound"
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
    pub fn append_attestation_digest(env: Env, digest: BytesN<32>) {
        // env.clone(): env is used again after this call for storage get/set and publish.
        let escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();

        let mut log: Vec<BytesN<32>> = env
            .storage()
            .instance()
            .get(&DataKey::AttestationAppendLog)
            .unwrap_or_else(|| Vec::new(&env));
        assert!(
            log.len() < MAX_ATTESTATION_APPEND_ENTRIES,
            "attestation append log capacity reached"
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

    pub fn get_contribution(env: Env, investor: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::InvestorContribution(investor))
            .unwrap_or(0)
    }

    /// Pro-rata denominator captured when the escrow first became **funded**; [`None`] until then.
    pub fn get_funding_close_snapshot(env: Env) -> Option<FundingCloseSnapshot> {
        env.storage().instance().get(&DataKey::FundingCloseSnapshot)
    }

    /// Effective yield (bps) for this investor after their **first** deposit; later [`LiquifactEscrow::fund`]
    /// calls add principal at this rate. Defaults to [`InvoiceEscrow::yield_bps`] when unset (legacy positions).
    ///
    /// Note: reads `DataKey::Escrow` for the base yield fallback; callers that already hold the
    /// escrow should prefer reading `DataKey::InvestorEffectiveYield` directly.
    pub fn get_investor_yield_bps(env: Env, investor: Address) -> i64 {
        // env.clone(): env is used again after this call for the InvestorEffectiveYield read.
        let escrow = Self::get_escrow(env.clone());
        env.storage()
            .instance()
            .get(&DataKey::InvestorEffectiveYield(investor.clone()))
            .unwrap_or(escrow.yield_bps)
    }

    /// Earliest ledger timestamp for [`LiquifactEscrow::claim_investor_payout`]; `0` if not gated.
    pub fn get_investor_claim_not_before(env: Env, investor: Address) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::InvestorClaimNotBefore(investor))
            .unwrap_or(0)
    }

    pub fn get_sme_collateral_commitment(env: Env) -> Option<SmeCollateralCommitment> {
        env.storage().instance().get(&DataKey::SmeCollateralPledge)
    }

    pub fn is_investor_claimed(env: Env, investor: Address) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::InvestorClaimed(investor))
            .unwrap_or(false)
    }

    /// Record or replace the optional SME collateral commitment metadata.
    ///
    /// **Metadata-only:** this writes [`DataKey::SmeCollateralPledge`] and emits
    /// [`CollateralRecordedEvt`]. It does not transfer tokens, reserve balances, verify custody,
    /// create an on-chain encumbrance, or block unrelated flows.
    ///
    /// # Validation
    ///
    /// - `amount` must be positive.
    /// - `asset` must be a non-empty symbol.
    /// - When replacing an existing commitment, the current ledger timestamp must not be
    ///   earlier than the prior `recorded_at` (defense-in-depth against stale writes).
    pub fn record_sme_collateral_commitment(
        env: Env,
        asset: Symbol,
        amount: i128,
    ) -> SmeCollateralCommitment {
        assert!(amount > 0, "Collateral amount must be positive");
        assert!(
            asset != Symbol::new(&env, ""),
            "Collateral asset symbol must not be empty"
        );

        // env.clone(): env is used again after this call for storage read/write, timestamp, and publish.
        let escrow = Self::get_escrow(env.clone());
        escrow.sme_address.require_auth();

        let now = env.ledger().timestamp();
        let prior: Option<SmeCollateralCommitment> =
            env.storage().instance().get(&DataKey::SmeCollateralPledge);
        let prior_amount = prior.as_ref().map(|c| c.amount).unwrap_or(0);

        if let Some(ref existing) = prior {
            assert!(
                now >= existing.recorded_at,
                "Collateral commitment timestamp must not go backward"
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
    /// council override, or break-glass entrypoint. After [`LiquifactEscrow::transfer_admin`],
    /// only the **new** admin can clear a persisted hold.
    ///
    /// **Governance posture:** production `admin` must be a multisig or governed contract so
    /// hold + key loss cannot strand funds without an off-chain recovery vote that executes
    /// `transfer_admin` then `clear_legal_hold`. See `docs/escrow-legal-hold.md`.
    pub fn set_legal_hold(env: Env, active: bool) {
        // env.clone(): env is used again after this call for storage set and publish.
        let escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();

        env.storage().instance().set(&DataKey::LegalHold, &active);

        LegalHoldChanged {
            name: symbol_short!("legalhld"),
            invoice_id: escrow.invoice_id.clone(),
            active: if active { 1 } else { 0 },
        }
        .publish(&env);
    }

    /// Enable or disable the investor allowlist. When enabled, only addresses with
    /// [`DataKey::InvestorAllowlisted`] set to true may fund the escrow.
    pub fn set_allowlist_active(env: Env, active: bool) {
        let escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();
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
        let escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();
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
    /// once. The call is rejected for empty vectors or vectors longer than
    /// `MAX_INVESTOR_ALLOWLIST_BATCH` to keep storage and CPU bounded.
    ///
    /// Invariant: the end state and emitted events are identical to calling
    /// `set_investor_allowlisted` individually for each element in `investors`.
    pub fn set_investors_allowlisted(env: Env, investors: Vec<Address>, allowed: bool) {
        // env.clone(): env is used again after this call for storage writes and publish.
        let escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();

        let n = investors.len();
        assert!(n > 0, "investors vector must be non-empty");
        assert!(
            (n as u32) <= MAX_INVESTOR_ALLOWLIST_BATCH,
            "investors vector length exceeds MAX_INVESTOR_ALLOWLIST_BATCH"
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
    }

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
        // env.clone(): env is used again after this call for storage set and publish.
        let mut escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();

        assert!(new_target > 0, "Target must be strictly positive");
        assert!(
            escrow.status == 0,
            "Target can only be updated in Open state"
        );
        assert!(
            new_target >= escrow.funded_amount,
            "Target cannot be less than already funded amount"
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

    /// Validate the stored schema version and apply a migration if one is implemented.
    ///
    /// # Behavior — **panics on all current paths**
    ///
    /// This entrypoint currently contains **no implemented migration logic**. Every call
    /// terminates with a `panic!` (aborts the Soroban transaction). This is intentional:
    /// it makes the "no migration" guarantee explicit rather than silently returning success.
    ///
    /// **Execution order:** the function first reads [`DataKey::Version`] from instance
    /// storage, then asserts the supplied `from_version` matches, then panics. No storage
    /// writes ever occur; the storage read is read-only and side-effect-free. There is
    /// **no [`Address::require_auth`] call** — any account may invoke `migrate`. This is
    /// safe only because the final `panic!` is reached on every code path and no state
    /// is mutated. Adding migration logic without also adding an auth guard would make
    /// this entrypoint callable by any account.
    ///
    /// Do **not** call `migrate` expecting it to perform bookkeeping work in the current
    /// release. To add a real migration path (e.g. rewriting a stored struct after a field
    /// addition), implement the transformation above the final `panic!` branch, update
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
    /// # Panics
    ///
    /// | Condition | Message |
    /// |-----------|--------|
    /// | `stored_version != from_version` | `"from_version does not match stored version"` |
    /// | `from_version >= SCHEMA_VERSION` | `"Already at current schema version"` |
    /// | Any `from_version < SCHEMA_VERSION` (all paths) | `"No migration path from version {N} - extend migrate or redeploy"` |
    ///
    /// See `docs/OPERATOR_RUNBOOK.md` §2 for step-by-step instructions on implementing
    /// a concrete migration path.
    pub fn migrate(env: Env, from_version: u32) -> u32 {
        let stored: u32 = env.storage().instance().get(&DataKey::Version).unwrap_or(0);

        assert!(
            stored == from_version,
            "from_version does not match stored version"
        );

        if from_version >= SCHEMA_VERSION {
            panic!("Already at current schema version");
        }

        // No migration path is implemented for any version below SCHEMA_VERSION.
        // To add one: implement the transformation here, call
        //   env.storage().instance().set(&DataKey::Version, &NEW_VERSION);
        // and return NEW_VERSION before reaching this panic.
        panic!("No migration path from version {from_version} - extend migrate or redeploy");
    }

    /// Record investor principal while the invoice is **open**. First deposit sets base
    /// [`InvoiceEscrow::yield_bps`] for this investor; further amounts must use this method (not
    /// [`LiquifactEscrow::fund_with_commitment`]) so tier selection stays immutable after the first leg.
    pub fn fund(env: Env, investor: Address, amount: i128) -> InvoiceEscrow {
        Self::fund_impl(env, investor, amount, true, 0)
    }

    /// First deposit only (per investor): optional longer lock and tier ladder from [`DataKey::YieldTierTable`].
    /// Sets [`DataKey::InvestorClaimNotBefore`] when `committed_lock_secs > 0`. Additional principal
    /// from the same investor must use [`LiquifactEscrow::fund`].
    pub fn fund_with_commitment(
        env: Env,
        investor: Address,
        amount: i128,
        committed_lock_secs: u64,
    ) -> InvoiceEscrow {
        Self::fund_impl(env, investor, amount, false, committed_lock_secs)
    }

    fn fund_impl(
        env: Env,
        investor: Address,
        amount: i128,
        simple_fund: bool,
        committed_lock_secs: u64,
    ) -> InvoiceEscrow {
        investor.require_auth();

        assert!(amount > 0, "Funding amount must be positive");

        let floor: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinContributionFloor)
            .unwrap_or(0);
        if floor > 0 {
            assert!(
                amount >= floor,
                "funding amount below min_contribution floor"
            );
        }

        // env.clone(): env is used again after this call for storage writes and publish.
        let mut escrow = Self::get_escrow(env.clone());
        // Legal hold check is intentionally after the escrow read: the escrow is needed for
        // status and yield_bps regardless, and hoisting the hold check before the escrow read
        // would not reduce storage operations (both keys are always read on this path).
        assert!(
            !Self::legal_hold_active(&env),
            "Legal hold blocks new funding while active"
        );
        assert!(escrow.status == 0, "Escrow not open for funding");

        if Self::is_allowlist_active(env.clone()) {
            assert!(
                Self::is_investor_allowlisted(env.clone(), investor.clone()),
                "Investor not on allowlist"
            );
        }

        let contribution_key = DataKey::InvestorContribution(investor.clone());
        let prev: i128 = env.storage().instance().get(&contribution_key).unwrap_or(0);
        let new_contribution: i128 = prev
            .checked_add(amount)
            .expect("investor contribution overflow");

        if let Some(cap) = env
            .storage()
            .instance()
            .get::<DataKey, i128>(&DataKey::MaxPerInvestorCap)
        {
            assert!(
                new_contribution <= cap,
                "investor contribution exceeds max_per_investor cap"
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
                assert!(cur_funder_count < cap, "unique investor cap reached");
            }
        }

        // Capture the effective yield in a local so the event field can be populated without
        // a post-write storage read of DataKey::InvestorEffectiveYield.
        let investor_effective_yield_bps: i64;

        if simple_fund {
            if prev == 0 {
                investor_effective_yield_bps = escrow.yield_bps;
                env.storage().instance().set(
                    &DataKey::InvestorEffectiveYield(investor.clone()),
                    &escrow.yield_bps,
                );
                env.storage()
                    .instance()
                    .set(&DataKey::InvestorClaimNotBefore(investor.clone()), &0u64);
            } else {
                // Returning investor: yield was set on first deposit; read it for the event.
                investor_effective_yield_bps = env
                    .storage()
                    .instance()
                    .get(&DataKey::InvestorEffectiveYield(investor.clone()))
                    .unwrap_or(escrow.yield_bps);
            }
            // If prev > 0, preserve existing effective yield and claim lock
        } else {
            assert!(
                prev == 0,
                "Additional principal after a tiered first deposit must use fund(), not fund_with_commitment()"
            );
            let eff =
                Self::effective_yield_for_commitment(&env, escrow.yield_bps, committed_lock_secs);
            investor_effective_yield_bps = eff;
            env.storage()
                .instance()
                .set(&DataKey::InvestorEffectiveYield(investor.clone()), &eff);
            let now = env.ledger().timestamp();
            let claim_nb = if committed_lock_secs == 0 {
                0u64
            } else {
                now.checked_add(committed_lock_secs)
                    .expect("investor claim time overflow")
            };
            env.storage().instance().set(
                &DataKey::InvestorClaimNotBefore(investor.clone()),
                &claim_nb,
            );
        }

        escrow.funded_amount = escrow
            .funded_amount
            .checked_add(amount)
            .expect("funded_amount overflow");

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

        let next_contribution = prev
            .checked_add(amount)
            .expect("investor contribution overflow");
        env.storage()
            .instance()
            .set(&contribution_key, &new_contribution);

        if prev == 0 {
            // Use the hoisted cur_funder_count; no second storage read needed.
            env.storage()
                .instance()
                .set(&DataKey::UniqueFunderCount, &(cur_funder_count + 1));
        }

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        EscrowFunded {
            name: symbol_short!("funded"),
            invoice_id: escrow.invoice_id.clone(),
            investor: investor.clone(),
            amount,
            funded_amount: escrow.funded_amount,
            status: escrow.status,
            // Local variable set at write time; no post-write storage read required.
            investor_effective_yield_bps,
        }
        .publish(&env);

        escrow
    }

    /// Finalize the escrow after funding is complete. Transitions status from **1 (funded)** to
    /// **2 (settled)** so investors can claim their payout. Requires SME auth.
    ///
    /// Blocked while [`DataKey::LegalHold`] is active — see [`LiquifactEscrow::set_legal_hold`].
    ///
    /// # Status guard
    /// Only permitted when [`InvoiceEscrow::status`] is **1 (funded)**. Open (0), settled (2), or
    /// withdrawn (3) escrows reject the call.
    ///
    /// # Maturity gate
    /// If [`InvoiceEscrow::maturity`] > 0, settlement is further gated on the ledger timestamp
    /// reaching `maturity`. A zero maturity means no time gate.
    pub fn settle(env: Env) -> InvoiceEscrow {
        assert!(
            !Self::legal_hold_active(&env),
            "Legal hold blocks settlement finalization"
        );

        // env.clone(): env is used again after this call for ledger timestamp, storage set, and publish.
        let mut escrow = Self::get_escrow(env.clone());

        escrow.sme_address.require_auth();
        assert!(
            escrow.status == 1,
            "Escrow must be funded before settlement"
        );

        if escrow.maturity > 0 {
            let now = env.ledger().timestamp();
            assert!(
                now >= escrow.maturity,
                "Escrow has not yet reached maturity"
            );
        }

        escrow.status = 2;

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        EscrowSettled {
            name: symbol_short!("escrow_sd"),
            invoice_id: escrow.invoice_id.clone(),
            funded_amount: escrow.funded_amount,
            yield_bps: escrow.yield_bps,
            maturity: escrow.maturity,
        }
        .publish(&env);

        escrow
    }

    /// SME pulls funded liquidity (accounting). Blocked when a legal hold is active.
    pub fn withdraw(env: Env) -> InvoiceEscrow {
        assert!(
            !Self::legal_hold_active(&env),
            "Legal hold blocks SME withdrawal"
        );

        // env.clone(): env is used again after this call for storage set and publish.
        let mut escrow = Self::get_escrow(env.clone());
        escrow.sme_address.require_auth();

        assert!(
            escrow.status == 1,
            "Escrow must be funded before withdrawal"
        );

        let amount = escrow.funded_amount;
        escrow.status = 3;

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        SmeWithdrew {
            name: symbol_short!("sme_wd"),
            invoice_id: escrow.invoice_id.clone(),
            amount,
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
    pub fn claim_investor_payout(env: Env, investor: Address) {
        assert!(
            !Self::legal_hold_active(&env),
            "Legal hold blocks investor claims"
        );

        investor.require_auth();

        // Single fetch: consolidates the previous two reads of InvestorContribution.
        // Retains the participation guard without a redundant second storage access.
        let contribution: i128 = env
            .storage()
            .instance()
            .get(&DataKey::InvestorContribution(investor.clone()))
            .unwrap_or(0);
        assert!(contribution > 0, "Address has no contribution to claim");

        // env.clone(): env is used again after this call for storage reads, ledger timestamp, and publish.
        let escrow = Self::get_escrow(env.clone());
        assert!(
            escrow.status == 2,
            "Escrow must be settled before investor claim"
        );

        let not_before: u64 = env
            .storage()
            .instance()
            .get(&DataKey::InvestorClaimNotBefore(investor.clone()))
            .unwrap_or(0);
        let now = env.ledger().timestamp();
        assert!(
            now >= not_before,
            "Investor commitment lock not expired (ledger timestamp)"
        );

        // Idempotent early-return: a second claim is a no-op (no re-emit).
        let key = DataKey::InvestorClaimed(investor.clone());
        if env.storage().instance().get(&key).unwrap_or(false) {
            return;
        }

        // Mark before emit — prevents re-emission on any re-entrant path.
        env.storage().instance().set(&key, &true);

        InvestorPayoutClaimed {
            name: symbol_short!("inv_claim"),
            investor,
            invoice_id: escrow.invoice_id.clone(),
        }
        .publish(&env);
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
    /// Panics with `"compute_investor_payout: arithmetic overflow"` rather than silently
    /// producing a wrong value.
    ///
    /// # Authorization
    ///
    /// None — pure read; no auth required.
    pub fn compute_investor_payout(env: Env, investor: Address) -> i128 {
        // Contribution fetch: returns 0 for non-participants without panicking.
        let contribution: i128 = env
            .storage()
            .instance()
            .get(&DataKey::InvestorContribution(investor.clone()))
            .unwrap_or(0);
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
        let effective_yield_bps: i64 = env
            .storage()
            .instance()
            .get(&DataKey::InvestorEffectiveYield(investor.clone()))
            .unwrap_or(escrow.yield_bps);

        // coupon = total_principal × effective_yield_bps / 10_000  (floor)
        let coupon = total_principal
            .checked_mul(effective_yield_bps as i128)
            .expect("compute_investor_payout: arithmetic overflow")
            .checked_div(10_000)
            .expect("compute_investor_payout: arithmetic overflow");

        let settle_pool = total_principal
            .checked_add(coupon)
            .expect("compute_investor_payout: arithmetic overflow");

        // gross_payout = contribution × settle_pool / total_principal  (floor)
        contribution
            .checked_mul(settle_pool)
            .expect("compute_investor_payout: arithmetic overflow")
            .checked_div(total_principal)
            .expect("compute_investor_payout: arithmetic overflow")
    }

    pub fn update_maturity(env: Env, new_maturity: u64) -> InvoiceEscrow {
        // env.clone(): env is used again after this call for storage set and publish.
        let mut escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();

        assert!(
            escrow.status == 0,
            "Maturity can only be updated in Open state"
        );

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

        // Instance storage keys required for settlement + gating behavior.
        let k_escrow: DataKey = DataKey::Escrow;
        env.storage().instance().extend_ttl(&k_escrow, &INSTANCE_TTL_MIN_EXTENSION_SECS);

        let k_version: DataKey = DataKey::Version;
        env.storage().instance().extend_ttl(&k_version, &INSTANCE_TTL_MIN_EXTENSION_SECS);

        let k_legal_hold: DataKey = DataKey::LegalHold;
        env.storage().instance().extend_ttl(&k_legal_hold, &INSTANCE_TTL_MIN_EXTENSION_SECS);

        let k_allowlist_active: DataKey = DataKey::AllowlistActive;
        env.storage()
            .instance()
            .extend_ttl(&k_allowlist_active, &INSTANCE_TTL_MIN_EXTENSION_SECS);

        let k_snapshot: DataKey = DataKey::FundingCloseSnapshot;
        env.storage()
            .instance()
            .extend_ttl(&k_snapshot, &INSTANCE_TTL_MIN_EXTENSION_SECS);

        // Investor contribution + claim gates are instance keys (per investor).
        // We cannot enumerate contributors on-chain; extend only what the caller provides.
        for addr in allowlisted.iter() {
            // Contribution + claim-gate entries are instance-scoped and per-investor.
            let k_contrib = DataKey::InvestorContribution(addr.clone());
            env.storage()
                .instance()
                .extend_ttl(&k_contrib, &INSTANCE_TTL_MIN_EXTENSION_SECS);

            let k_claim_not_before = DataKey::InvestorClaimNotBefore(addr.clone());
            env.storage()
                .instance()
                .extend_ttl(&k_claim_not_before, &INSTANCE_TTL_MIN_EXTENSION_SECS);
        }


        // Persistent allowlist entries.
        for addr in allowlisted.iter() {
            let k = DataKey::InvestorAllowlisted(addr.clone());
            env.storage()
                .persistent()
                .extend_ttl(&k, &PERSISTENT_TTL_MIN_EXTENSION_SECS);
        }
    }

    pub fn transfer_admin(env: Env, new_admin: Address) -> InvoiceEscrow {
        // env.clone(): env is used again after this call for storage set and publish.
        let mut escrow = Self::get_escrow(env.clone());


        escrow.admin.require_auth();

        assert!(
            escrow.admin != new_admin,
            "New admin must differ from current admin"
        );

        escrow.admin = new_admin;

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        AdminTransferredEvent {
            name: symbol_short!("admin"),
            invoice_id: escrow.invoice_id.clone(),
            new_admin: escrow.admin.clone(),
        }
        .publish(&env);

        escrow
    }

    /// Transition an **open** escrow (status 0) to **cancelled** (status 4).
    ///
    /// Only the [`InvoiceEscrow::admin`] may call this. Blocked while a legal hold is active.
    /// After cancellation, investors may recover their principal via [`LiquifactEscrow::refund`].
    ///
    /// # Panics
    /// - If legal hold is active.
    /// - If escrow is not in status 0 (open).
    pub fn cancel_funding(env: Env) -> InvoiceEscrow {
        assert!(
            !Self::legal_hold_active(&env),
            "Legal hold blocks cancel_funding"
        );

        let mut escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();

        assert!(
            escrow.status == 0,
            "cancel_funding only allowed in Open state"
        );

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
    /// second call is a no-op (contribution is 0 → panics with "no contribution to refund").
    ///
    /// # Panics
    /// - If escrow is not in status 4 (cancelled).
    /// - If the investor has no recorded contribution (or has already been refunded).
    pub fn refund(env: Env, investor: Address) {
        investor.require_auth();

        let escrow = Self::get_escrow(env.clone());
        assert!(escrow.status == 4, "refund only allowed in Cancelled state");

        let contribution_key = DataKey::InvestorContribution(investor.clone());
        let amount: i128 = env.storage().instance().get(&contribution_key).unwrap_or(0);
        assert!(amount > 0, "no contribution to refund");

        // Zero out contribution before transfer (checks-effects-interactions).
        env.storage().instance().set(&contribution_key, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::InvestorRefunded(investor.clone()), &true);

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::FundingToken)
            .expect("funding token must be initialized");
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
}

#[cfg(test)]
mod test_allowlist_tests;
#[cfg(test)]
mod tests;
