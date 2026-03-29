//! LiquiFact Escrow Contract
//!
//! Holds investor funds for an invoice until settlement.
//! - SME receives stablecoin when funding target is met ([`LiquifactEscrow::withdraw`])
//! - SME records optional **collateral commitments** ([`LiquifactEscrow::record_sme_collateral_commitment`]) —
//!   these are **ledger records only**; they do **not** move tokens or trigger liquidation.
//! - [`LiquifactEscrow::settle`] finalizes the escrow after maturity (when configured).
//!
//! ## Compliance hold (legal hold)
//!
//! An admin may set [`DataKey::LegalHold`] to block risk-bearing transitions until cleared:
//! [`LiquifactEscrow::settle`], SME [`LiquifactEscrow::withdraw`], and
//! [`LiquifactEscrow::claim_investor_payout`]. **Clearing** requires the same governance admin
//! to call [`LiquifactEscrow::set_legal_hold`] with `active = false`. This contract does not
//! embed a timelock or council multisig: production deployments should treat `admin` as a
//! governed contract or multisig so holds cannot be used for indefinite fund lock **without**
//! off-chain governance recovery (rotation, vote, emergency procedures).
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
//! are out of scope and should fail safe assertions. This is meant for rounding residue / stray
//! transfers, not for settling live liabilities — integrations that custody principal on-chain must
//! keep token balances reconciled with `funded_amount` so treasury sweeps cannot pull user funds.
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
//! written; off-chain pro-rata share for an investor is `get_contribution(addr) / snapshot.total_principal`
//! in rational arithmetic (watch integer rounding off-chain).

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, symbol_short,
    token::TokenClient, Address, BytesN, Env, String, Symbol, Vec,
};

pub(crate) mod external_calls;

/// Structured errors for [`LiquifactEscrow`] operations.
///
/// Each variant maps to a specific failure mode with client-actionable guidance.
/// Errors are encoded as u32 codes following Soroban's error convention.
///
/// ## Error Code Layout
/// - `0x0001_0000` - `0x0001_00FF`: Initialization errors
/// - `0x0002_0000` - `0x0002_00FF`: State transition errors
/// - `0x0003_0000` - `0x0003_00FF`: Authorization errors
/// - `0x0004_0000` - `0x0004_00FF`: Funding errors
/// - `0x0005_0000` - `0x0005_00FF`: Settlement/withdrawal errors
/// - `0x0006_0000` - `0x0006_00FF`: Attestation errors
/// - `0x0007_0000` - `0x0007_00FF`: Configuration errors
#[contracterror]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    // === Initialization Errors (0x0001_0000) ===

    /// Escrow has already been initialized.
    ///
    /// **Recovery**: Read the existing escrow state with [`LiquifactEscrow::get_escrow`]
    /// instead of calling [`LiquifactEscrow::init`] again.
    EscrowAlreadyInitialized = 0x0001_0001,

    /// Invoice ID string exceeds maximum allowed length.
    ///
    /// **Recovery**: Truncate the invoice ID to [`MAX_INVOICE_ID_STRING_LEN`] (32) bytes
    /// or fewer.
    InvoiceIdTooLong = 0x0001_0002,

    /// Invoice ID contains invalid characters.
    ///
    /// **Recovery**: Use only ASCII alphanumeric characters (`A-Za-z0-9`) and underscores (`_`).
    InvoiceIdInvalidChars = 0x0001_0003,

    /// Invoice amount must be positive.
    ///
    /// **Recovery**: Provide a positive integer amount for the invoice.
    AmountNotPositive = 0x0001_0004,

    /// Funding token address not configured.
    ///
    /// **Recovery**: Call [`LiquifactEscrow::init`] before accessing the funding token.
    FundingTokenNotSet = 0x0001_0005,

    /// Treasury address not configured.
    ///
    /// **Recovery**: Call [`LiquifactEscrow::init`] before accessing the treasury.
    TreasuryNotSet = 0x0001_0006,

    /// Escrow has not been initialized.
    ///
    /// **Recovery**: Call [`LiquifactEscrow::init`] first.
    EscrowNotInitialized = 0x0001_0007,

    // === State Transition Errors (0x0002_0000) ===

    /// Escrow is not in the open state for this operation.
    ///
    /// **Recovery**: Check [`InvoiceEscrow::status`] and only perform this action
    /// when status is 0 (open).
    EscrowNotOpen = 0x0002_0001,

    /// Escrow must be funded before this operation.
    ///
    /// **Recovery**: Complete funding to reach status 1 (funded) before retrying.
    EscrowNotFunded = 0x0002_0002,

    /// Escrow must be settled before this operation.
    ///
    /// **Recovery**: Call [`LiquifactEscrow::settle`] first.
    EscrowNotSettled = 0x0002_0003,

    /// Escrow has not reached maturity.
    ///
    /// **Recovery**: Wait for ledger timestamp to reach [`InvoiceEscrow::maturity`].
    EscrowNotMature = 0x0002_0004,

    /// Escrow is not in a terminal state for this operation.
    ///
    /// **Recovery**: Ensure escrow is either settled (status 2) or withdrawn (status 3).
    EscrowNotTerminal = 0x0002_0005,

    /// Legal hold is active and blocks this operation.
    ///
    /// **Recovery**: Admin must call [`LiquifactEscrow::clear_legal_hold`] to deactivate.
    LegalHoldActive = 0x0002_0006,

    // === Authorization Errors (0x0003_0000) ===

    /// Caller is not authorized to perform this action.
    ///
    /// **Recovery**: Ensure the correct address with proper authorization calls this method.
    Unauthorized = 0x0003_0001,

    /// Admin address is required for this operation.
    ///
    /// **Recovery**: Use the address that was set as `admin` during [`LiquifactEscrow::init`].
    AdminRequired = 0x0003_0002,

    /// SME address is required for this operation.
    ///
    /// **Recovery**: Use the address that was set as `sme_address` during [`LiquifactEscrow::init`].
    SmeRequired = 0x0003_0003,

    /// Investor address is required for this operation.
    ///
    /// **Recovery**: Use the investor's own address that holds the contribution.
    InvestorRequired = 0x0003_0004,

    /// Treasury address is required for this operation.
    ///
    /// **Recovery**: Use the treasury address configured during [`LiquifactEscrow::init`].
    TreasuryRequired = 0x0003_0005,

    // === Funding Errors (0x0004_0000) ===

    /// Funding amount must be positive.
    ///
    /// **Recovery**: Provide a positive integer amount.
    FundingAmountNotPositive = 0x0004_0001,

    /// Funding amount is below the minimum contribution floor.
    ///
    /// **Recovery**: Increase the funding amount to meet the floor set via
    /// [`LiquifactEscrow::init`].
    BelowMinContributionFloor = 0x0004_0002,

    /// Unique investor cap has been reached.
    ///
    /// **Recovery**: No more distinct investor addresses can contribute. This is a protocol
    /// limit, not a funding amount issue.
    UniqueInvestorCapReached = 0x0004_0003,

    /// Commitment-based funding must be the first deposit from this investor.
    ///
    /// **Recovery**: Use [`LiquifactEscrow::fund`] for additional contributions after an
    /// initial [`LiquifactEscrow::fund_with_commitment`].
    FundWithCommitmentNotFirstDeposit = 0x0004_0004,

    /// Investor claim is still locked by commitment.
    ///
    /// **Recovery**: Wait for the ledger timestamp to reach the `InvestorClaimNotBefore`
    /// value set during the commitment deposit.
    CommitmentLockNotExpired = 0x0004_0005,

    /// Investor has already claimed their payout.
    ///
    /// **Recovery**: This operation is idempotent; the investor payout has already been claimed.
    InvestorAlreadyClaimed = 0x0004_0006,

    /// Funding amount overflow detected.
    ///
    /// **Recovery**: The total funded amount exceeds the maximum representable i128 value.
    /// This indicates an arithmetic error or malicious input.
    FundedAmountOverflow = 0x0004_0007,

    // === Settlement/Withdrawal Errors (0x0005_0000) ===

    /// No funding token balance available to sweep.
    ///
    /// **Recovery**: This is expected if the escrow has no token balance. Verify the token
    /// transfer logic or check if tokens were actually received.
    NoTokenBalanceToSweep = 0x0005_0001,

    /// Sweep amount is zero after balance calculation.
    ///
    /// **Recovery**: The requested sweep amount may have been zero or the balance is
    /// insufficient. Verify the sweep amount and contract balance.
    SweepAmountZero = 0x0005_0002,

    /// Sweep amount is not positive.
    ///
    /// **Recovery**: Provide a positive integer amount for the sweep.
    SweepAmountNotPositive = 0x0005_0003,

    /// Sweep amount exceeds the maximum allowed per call.
    ///
    /// **Recovery**: Reduce the sweep amount to [`MAX_DUST_SWEEP_AMOUNT`] or fewer units.
    SweepAmountExceedsMax = 0x0005_0004,

    // === Attestation Errors (0x0006_0000) ===

    /// Primary attestation hash already bound.
    ///
    /// **Recovery**: The primary attestation is single-set. Use
    /// [`LiquifactEscrow::append_attestation_digest`] for additional audit entries.
    PrimaryAttestationAlreadyBound = 0x0006_0001,

    /// Attestation append log has reached capacity.
    ///
    /// **Recovery**: The log is bounded at [`MAX_ATTESTATION_APPEND_ENTRIES`]. No more
    /// digests can be appended until the protocol is upgraded.
    AttestationLogCapacityReached = 0x0006_0002,

    // === Configuration Errors (0x0007_0000) ===

    /// Minimum contribution floor must be positive when configured.
    ///
    /// **Recovery**: Set `min_contribution` to a positive value or `None` to disable the floor.
    MinContributionNotPositive = 0x0007_0001,

    /// Minimum contribution exceeds invoice amount.
    ///
    /// **Recovery**: Reduce `min_contribution` to be less than or equal to the invoice amount.
    MinContributionExceedsAmount = 0x0007_0002,

    /// Maximum unique investors must be positive when configured.
    ///
    /// **Recovery**: Set `max_unique_investors` to a positive value or `None` for unlimited.
    MaxInvestorsNotPositive = 0x0007_0003,

    /// Target must be strictly positive.
    ///
    /// **Recovery**: Provide a positive integer for the new funding target.
    TargetNotPositive = 0x0007_0004,

    /// Target cannot be less than already funded amount.
    ///
    /// **Recovery**: Set target to be at least the current [`InvoiceEscrow::funded_amount`].
    TargetBelowFundedAmount = 0x0007_0005,

    /// New admin must differ from current admin.
    ///
    /// **Recovery**: Provide a different address for the new admin.
    AdminNotDifferent = 0x0007_0006,

    /// Maturity can only be updated in open state.
    ///
    /// **Recovery**: Check [`InvoiceEscrow::status`] is 0 (open) before updating maturity.
    MaturityUpdateNotOpen = 0x0007_0007,

    /// Collateral amount must be positive.
    ///
    /// **Recovery**: Provide a positive integer for the collateral amount.
    CollateralAmountNotPositive = 0x0007_0008,

    // === Yield Tier Errors (0x0008_0000) ===

    /// Tier yield_bps must be between 0 and 10,000.
    ///
    /// **Recovery**: Adjust the tier's `yield_bps` to be in range [0, 10000].
    TierYieldBpsOutOfRange = 0x0008_0001,

    /// Tier yield_bps must be greater than or equal to base yield.
    ///
    /// **Recovery**: Set the tier's `yield_bps` to be >= the base [`InvoiceEscrow::yield_bps`].
    TierYieldBpsBelowBase = 0x0008_0002,

    /// Tiers must have strictly increasing min_lock_secs.
    ///
    /// **Recovery**: Ensure each tier's `min_lock_secs` is strictly greater than the
    /// previous tier's value.
    TierLockSecsNotIncreasing = 0x0008_0003,

    /// Tiers must have non-decreasing yield_bps.
    ///
    /// **Recovery**: Ensure each tier's `yield_bps` is >= the previous tier's value.
    TierYieldNotNonDecreasing = 0x0008_0004,

    /// Commitment lock time would overflow.
    ///
    /// **Recovery**: Use a smaller `committed_lock_secs` value that doesn't cause
    /// ledger timestamp overflow when added to current time.
    CommitmentLockOverflow = 0x0008_0005,

    // === Migration Errors (0x0009_0000) ===

    /// Stored version does not match expected migration version.
    ///
    /// **Recovery**: Verify the `from_version` parameter matches the current stored version.
    VersionMismatch = 0x0009_0001,

    /// Already at current schema version, migration not needed.
    ///
    /// **Recovery**: Skip migration or use the correct `from_version`.
    AlreadyAtCurrentVersion = 0x0009_0002,

    /// No migration path exists for the specified version.
    ///
    /// **Recovery**: A new migration must be implemented or the contract must be redeployed.
    NoMigrationPath = 0x0009_0003,
}

/// Current storage schema version (`DataKey::Version`).
pub const SCHEMA_VERSION: u32 = 5;

/// Upper bound on [`LiquifactEscrow::append_attestation_digest`] entries to keep storage bounded.
pub const MAX_ATTESTATION_APPEND_ENTRIES: u32 = 32;

/// Upper bound on [`LiquifactEscrow::sweep_terminal_dust`] per call (base units of the funding token).
///
/// Caps blast radius if instrumentation mis-estimates “dust”; tune per asset decimals off-chain.
pub const MAX_DUST_SWEEP_AMOUNT: i128 = 100_000_000;

/// Maximum UTF-8 byte length for the invoice `String` at init (matches Soroban [`Symbol`] max).
pub const MAX_INVOICE_ID_STRING_LEN: u32 = 32;

// --- Storage keys ---

#[contracttype]
#[derive(Clone)]
/// Storage discriminator for all persisted values.
///
/// Derive rationale:
/// - `Clone`: required because keys are passed by reference into storage APIs and reused
///   across lookups/sets in the same execution path.
pub enum DataKey {
    Escrow,
    Version,
    /// Per-investor contributed principal recorded during [`LiquifactEscrow::fund`].
    InvestorContribution(Address),
    /// When true, compliance/legal hold blocks payouts and settlement finalization.
    LegalHold,
    /// Optional SME collateral pledge metadata (record-only — not an on-chain asset lock).
    SmeCollateralPledge,
    /// Set when an investor has exercised a claim after settlement.
    InvestorClaimed(Address),
    /// SEP-41 funding asset for this invoice instance; set once in [`LiquifactEscrow::init`].
    FundingToken,
    /// Protocol treasury that may receive [`LiquifactEscrow::sweep_terminal_dust`]; set once in init.
    Treasury,
    /// Optional registry contract id for indexers; **hint only**, not authority (see module rustdoc).
    /// Omitted from storage when unset at init.
    RegistryRef,
    /// Immutable tier table when configured at [`LiquifactEscrow::init`]; omitted when tiering is off.
    /// **Trust:** values are protocol-supplied at deploy; the contract never mutates this key after init.
    YieldTierTable,
    /// Set once when status first becomes **funded** (1); immutable thereafter (pro-rata denominator).
    FundingCloseSnapshot,
    /// Effective annualized yield in bps chosen at this investor’s **first** deposit (see tiered yield).
    InvestorEffectiveYield(Address),
    /// Minimum [`Env::ledger`] timestamp before [`LiquifactEscrow::claim_investor_payout`] (0 = no extra gate).
    InvestorClaimNotBefore(Address),
    /// Minimum [`LiquifactEscrow::fund`] / [`LiquifactEscrow::fund_with_commitment`] amount per call (0 = no floor).
    MinContributionFloor,
    /// When set at [`LiquifactEscrow::init`], caps distinct investor addresses that may contribute (`prev == 0`).
    MaxUniqueInvestorsCap,
    /// Count of distinct investor addresses that have a non-zero [`DataKey::InvestorContribution`].
    UniqueFunderCount,
    /// Admin-only **single-set** off-chain attestation digest (e.g. SHA-256 of a legal/KYC bundle).
    /// See [`LiquifactEscrow::bind_primary_attestation_hash`].
    PrimaryAttestationHash,
    /// Append-only audit chain of digests (bounded by [`MAX_ATTESTATION_APPEND_ENTRIES`]).
    /// See [`LiquifactEscrow::append_attestation_digest`].
    AttestationAppendLog,
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
    /// 0 = open, 1 = funded, 2 = settled, 3 = withdrawn (SME pulled liquidity)
    pub status: u32,
}

/// SME-reported collateral intended for future liquidation hooks.
///
/// **Record-only:** this struct is stored for transparency and indexing. It does **not**
/// custody collateral, freeze tokens, or invoke automated liquidation. A future version could
/// optionally enforce transfers, but that would be explicit in the API and must not reuse
/// this record as proof of locked assets without on-chain enforcement changes.
#[contracttype]
#[derive(Debug, PartialEq)]
/// SME collateral pledge metadata (record-only).
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

// --- Events ---

#[contractevent]
pub struct EscrowInitialized {
    #[topic]
    pub name: Symbol,
    pub escrow: InvoiceEscrow,
}

#[contractevent]
pub struct EscrowFunded {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
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
    pub invoice_id: Symbol,
    pub funded_amount: i128,
    pub yield_bps: i64,
    pub maturity: u64,
}

#[contractevent]
pub struct MaturityUpdatedEvent {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub old_maturity: u64,
    pub new_maturity: u64,
}

#[contractevent]
pub struct AdminTransferredEvent {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub new_admin: Address,
}

#[contractevent]
pub struct FundingTargetUpdated {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub old_target: i128,
    pub new_target: i128,
}

#[contractevent]
pub struct LegalHoldChanged {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    /// `1` = hold enabled, `0` = cleared.
    pub active: u32,
}

/// Collateral pledge recorded; asset code is read from [`DataKey::SmeCollateralPledge`].
#[contractevent]
pub struct CollateralRecordedEvt {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub amount: i128,
}

#[contractevent]
pub struct SmeWithdrew {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub amount: i128,
}

#[contractevent]
pub struct InvestorPayoutClaimed {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub investor: Address,
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

#[contract]
pub struct LiquifactEscrow;

fn validate_invoice_id_string(env: &Env, invoice_id: &String) -> Result<Symbol, Error> {
    let len = invoice_id.len();
    if !(len >= 1 && len <= MAX_INVOICE_ID_STRING_LEN) {
        return Err(Error::InvoiceIdTooLong);
    }
    let len_u = len as usize;
    let mut buf = [0u8; 32];
    invoice_id.copy_into_slice(&mut buf[..len_u]);
    for &b in &buf[..len_u] {
        let ok = (b >= b'A' && b <= b'Z')
            || (b >= b'a' && b <= b'z')
            || (b >= b'0' && b <= b'9')
            || b == b'_';
        if !ok {
            return Err(Error::InvoiceIdInvalidChars);
        }
    }
    let s = core::str::from_utf8(&buf[..len_u]).expect("invoice_id ascii");
    Ok(Symbol::new(env, s))
}

#[contractimpl]
impl LiquifactEscrow {
    fn legal_hold_active(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::LegalHold)
            .unwrap_or(false)
    }

    fn validate_yield_tiers_table(tiers: &Option<Vec<YieldTier>>, base_yield: i64) -> Result<(), Error> {
        let Some(tiers) = tiers else {
            return Ok(());
        };
        if tiers.len() == 0 {
            return Ok(());
        }
        let n = tiers.len();
        for i in 0..n {
            let t = tiers.get(i).unwrap();
            if !(0..=10_000).contains(&t.yield_bps) {
                return Err(Error::TierYieldBpsOutOfRange);
            }
            if t.yield_bps < base_yield {
                return Err(Error::TierYieldBpsBelowBase);
            }
            if i > 0 {
                let p = tiers.get(i - 1).unwrap();
                if !(t.min_lock_secs > p.min_lock_secs) {
                    return Err(Error::TierLockSecsNotIncreasing);
                }
                if !(t.yield_bps >= p.yield_bps) {
                    return Err(Error::TierYieldNotNonDecreasing);
                }
            }
        }
        Ok(())
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
        if tiers.len() == 0 {
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
    /// # Errors
    /// Returns [`Error::AmountNotPositive`] if `amount <= 0`.
    /// Returns [`Error::YieldBpsOutOfRange`] if `yield_bps > 10_000` or `< 0`.
    /// Returns [`Error::EscrowAlreadyInitialized`] if escrow already exists.
    /// Returns [`Error::MinContributionNotPositive`] if `min_contribution` is set but `<= 0`.
    /// Returns [`Error::MinContributionExceedsAmount`] if `min_contribution > amount`.
    /// Returns [`Error::MaxInvestorsNotPositive`] if `max_unique_investors` is set but `<= 0`.
    /// Returns [`Error::InvoiceIdTooLong`] if invoice ID exceeds max length.
    /// Returns [`Error::InvoiceIdInvalidChars`] if invoice ID contains invalid chars.
    /// Returns tier validation errors if `yield_tiers` is malformed.
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
    ) -> Result<InvoiceEscrow, Error> {
        admin.require_auth();

        if amount <= 0 {
            return Err(Error::AmountNotPositive);
        }
        if !(0..=10_000).contains(&yield_bps) {
            return Err(Error::TierYieldBpsOutOfRange);
        }
        if env.storage().instance().has(&DataKey::Escrow) {
            return Err(Error::EscrowAlreadyInitialized);
        }

        Self::validate_yield_tiers_table(&yield_tiers, yield_bps)?;

        let invoice_sym = validate_invoice_id_string(&env, &invoice_id)?;

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
            if tiers.len() > 0 {
                env.storage()
                    .instance()
                    .set(&DataKey::YieldTierTable, tiers);
            }
        }

        let floor = min_contribution.unwrap_or(0);
        if min_contribution.is_some() {
            if floor <= 0 {
                return Err(Error::MinContributionNotPositive);
            }
            if floor > amount {
                return Err(Error::MinContributionExceedsAmount);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::MinContributionFloor, &floor);

        env.storage()
            .instance()
            .set(&DataKey::UniqueFunderCount, &0u32);

        if let Some(cap) = max_unique_investors {
            if cap == 0 {
                return Err(Error::MaxInvestorsNotPositive);
            }
            env.storage()
                .instance()
                .set(&DataKey::MaxUniqueInvestorsCap, &cap);
        }

        EscrowInitialized {
            name: symbol_short!("escrow_ii"),
            // Read the stored value so we do not clone an in-memory escrow snapshot.
            escrow: Self::get_escrow(env.clone())?,
        }
        .publish(&env);

        Ok(escrow)
    }

    /// Bound funding token (immutable after [`LiquifactEscrow::init`]).
    ///
    /// # Errors
    /// Returns [`Error::FundingTokenNotSet`] if init has not been called.
    pub fn get_funding_token(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::FundingToken)
            .ok_or(Error::FundingTokenNotSet)
    }

    /// Treasury that may receive terminal dust sweeps (immutable after init).
    ///
    /// # Errors
    /// Returns [`Error::TreasuryNotSet`] if init has not been called.
    pub fn get_treasury(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Treasury)
            .ok_or(Error::TreasuryNotSet)
    }

    /// Optional registry contract id (**hint only** — not authority for this escrow).
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
    ///
    /// # Errors
    /// Returns [`Error::LegalHoldActive`] if legal hold is enabled.
    /// Returns [`Error::SweepAmountNotPositive`] if `amount <= 0`.
    /// Returns [`Error::SweepAmountExceedsMax`] if `amount > MAX_DUST_SWEEP_AMOUNT`.
    /// Returns [`Error::EscrowNotTerminal`] if escrow status is not settled or withdrawn.
    /// Returns [`Error::NoTokenBalanceToSweep`] if contract has no token balance.
    /// Returns [`Error::SweepAmountZero`] if balance is less than requested amount.
    pub fn sweep_terminal_dust(env: Env, amount: i128) -> Result<i128, Error> {
        if Self::legal_hold_active(&env) {
            return Err(Error::LegalHoldActive);
        }
        if amount <= 0 {
            return Err(Error::SweepAmountNotPositive);
        }
        if amount > MAX_DUST_SWEEP_AMOUNT {
            return Err(Error::SweepAmountExceedsMax);
        }

        let escrow = Self::get_escrow(env.clone())?;
        if !(escrow.status == 2 || escrow.status == 3) {
            return Err(Error::EscrowNotTerminal);
        }

        let treasury: Address = env
            .storage()
            .instance()
            .get(&DataKey::Treasury)
            .ok_or(Error::TreasuryNotSet)?;
        treasury.require_auth();

        let token_addr = env
            .storage()
            .instance()
            .get(&DataKey::FundingToken)
            .ok_or(Error::FundingTokenNotSet)?;
        let this = env.current_contract_address();

        let token = TokenClient::new(&env, &token_addr);
        let balance = token.balance(&this);
        if balance <= 0 {
            return Err(Error::NoTokenBalanceToSweep);
        }
        let sweep_amt = amount.min(balance);
        if sweep_amt <= 0 {
            return Err(Error::SweepAmountZero);
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
            token: token_addr,
            amount: sweep_amt,
        }
        .publish(&env);

        Ok(sweep_amt)
    }

    /// Get the current escrow state.
    ///
    /// # Errors
    /// Returns [`Error::EscrowNotInitialized`] if init has not been called.
    pub fn get_escrow(env: Env) -> Result<InvoiceEscrow, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Escrow)
            .ok_or(Error::EscrowNotInitialized)
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
    pub fn get_max_unique_investors_cap(env: Env) -> Option<u32> {
        env.storage()
            .instance()
            .get(&DataKey::MaxUniqueInvestorsCap)
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

    /// Bind a **primary** 32-byte digest (e.g. SHA-256 of an IPFS CID or document bundle). **Single-set:**
    /// the call succeeds only while no primary hash exists; use [`LiquifactEscrow::append_attestation_digest`]
    /// for an append-only audit trail.
    ///
    /// **Authorization:** [`InvoiceEscrow::admin`]. **Frontrunning:** whichever binding transaction lands
    /// first wins; observers must read on-chain state (or parse events) after finality—there is no replay lock.
    ///
    /// # Errors
    /// Returns [`Error::EscrowNotInitialized`] if init has not been called.
    /// Returns [`Error::PrimaryAttestationAlreadyBound`] if primary attestation already exists.
    pub fn bind_primary_attestation_hash(env: Env, digest: BytesN<32>) -> Result<(), Error> {
        let escrow = Self::get_escrow(env.clone())?;
        escrow.admin.require_auth();
        if env.storage()
            .instance()
            .has(&DataKey::PrimaryAttestationHash)
        {
            return Err(Error::PrimaryAttestationAlreadyBound);
        }
        env.storage()
            .instance()
            .set(&DataKey::PrimaryAttestationHash, &digest);
        PrimaryAttestationBound {
            name: symbol_short!("att_bind"),
            invoice_id: escrow.invoice_id.clone(),
            digest: digest.clone(),
        }
        .publish(&env);
        Ok(())
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
    /// Returns [`Error::EscrowNotInitialized`] if init has not been called.
    /// Returns [`Error::AttestationLogCapacityReached`] if append log is full.
    pub fn append_attestation_digest(env: Env, digest: BytesN<32>) -> Result<(), Error> {
        let escrow = Self::get_escrow(env.clone())?;
        escrow.admin.require_auth();

        let mut log: Vec<BytesN<32>> = env
            .storage()
            .instance()
            .get(&DataKey::AttestationAppendLog)
            .unwrap_or_else(|| Vec::new(&env));
        if log.len() >= MAX_ATTESTATION_APPEND_ENTRIES {
            return Err(Error::AttestationLogCapacityReached);
        }
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
        Ok(())
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
    pub fn get_investor_yield_bps(env: Env, investor: Address) -> i64 {
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

    /// Record or replace the optional SME collateral pledge (metadata only).
    ///
    /// **Not an enforced on-chain lock** — cannot by itself trigger liquidation or block unrelated flows.
    ///
    /// # Errors
    /// Returns [`Error::CollateralAmountNotPositive`] if `amount <= 0`.
    /// Returns [`Error::EscrowNotInitialized`] if init has not been called.
    pub fn record_sme_collateral_commitment(
        env: Env,
        asset: Symbol,
        amount: i128,
    ) -> Result<SmeCollateralCommitment, Error> {
        if amount <= 0 {
            return Err(Error::CollateralAmountNotPositive);
        }
        let escrow = Self::get_escrow(env.clone())?;
        escrow.sme_address.require_auth();

        let commitment = SmeCollateralCommitment {
            asset,
            amount,
            recorded_at: env.ledger().timestamp(),
        };
        env.storage()
            .instance()
            .set(&DataKey::SmeCollateralPledge, &commitment);

        CollateralRecordedEvt {
            name: symbol_short!("coll_rec"),
            invoice_id: escrow.invoice_id.clone(),
            amount,
        }
        .publish(&env);

        Ok(commitment)
    }

    /// Set or clear compliance hold. Only [`InvoiceEscrow::admin`] may call.
    ///
    /// **Emergency / override:** clearing always goes through this admin-gated path. Deployments
    /// should use a governed `admin` (multisig or protocol DAO). There is no separate “break glass”
    /// entrypoint in this version — operational playbooks live off-chain.
    pub fn set_legal_hold(env: Env, active: bool) -> Result<(), Error> {
        let escrow = Self::get_escrow(env.clone())?;
        escrow.admin.require_auth();

        env.storage().instance().set(&DataKey::LegalHold, &active);

        LegalHoldChanged {
            name: symbol_short!("legalhld"),
            invoice_id: escrow.invoice_id.clone(),
            active: if active { 1 } else { 0 },
        }
        .publish(&env);
        Ok(())
    }

    /// Convenience alias for [`LiquifactEscrow::set_legal_hold`] with `active = false`.
    pub fn clear_legal_hold(env: Env) {
        Self::set_legal_hold(env, false);
    }

    /// Update the funding target.
    ///
    /// # Errors
    /// Returns [`Error::EscrowNotInitialized`] if init has not been called.
    /// Returns [`Error::TargetNotPositive`] if `new_target <= 0`.
    /// Returns [`Error::MaturityUpdateNotOpen`] if escrow status is not open (0).
    /// Returns [`Error::TargetBelowFundedAmount`] if `new_target < funded_amount`.
    pub fn update_funding_target(env: Env, new_target: i128) -> Result<InvoiceEscrow, Error> {
        let mut escrow = Self::get_escrow(env.clone())?;
        escrow.admin.require_auth();

        if new_target <= 0 {
            return Err(Error::TargetNotPositive);
        }
        if escrow.status != 0 {
            return Err(Error::MaturityUpdateNotOpen);
        }
        if new_target < escrow.funded_amount {
            return Err(Error::TargetBelowFundedAmount);
        }

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

        Ok(escrow)
    }

    /// Migrate stored schema version.
    ///
    /// New optional keys (`LegalHold`, `SmeCollateralPledge`, etc.) are **additive**: older
    /// bytecode can ignore unknown instance keys. Changing stored `InvoiceEscrow` layout still
    /// requires a coordinated migration or redeploy — see repository README.
    pub fn migrate(env: Env, from_version: u32) -> Result<u32, Error> {
        let stored: u32 = env.storage().instance().get(&DataKey::Version).unwrap_or(0);

        if stored != from_version {
            return Err(Error::VersionMismatch);
        }

        if from_version >= SCHEMA_VERSION {
            return Err(Error::AlreadyAtCurrentVersion);
        }

        Err(Error::NoMigrationPath)
    }

    /// Record investor principal while the invoice is **open**. First deposit sets base
    /// [`InvoiceEscrow::yield_bps`] for this investor; further amounts must use this method (not
    /// [`LiquifactEscrow::fund_with_commitment`]) so tier selection stays immutable after the first leg.
    ///
    /// # Errors
    /// Returns [`Error::FundingAmountNotPositive`] if `amount <= 0`.
    /// Returns [`Error::BelowMinContributionFloor`] if amount is below configured floor.
    /// Returns [`Error::LegalHoldActive`] if legal hold is enabled.
    /// Returns [`Error::EscrowNotOpen`] if escrow status is not open (0).
    /// Returns [`Error::UniqueInvestorCapReached`] if cap is set and reached.
    pub fn fund(env: Env, investor: Address, amount: i128) -> Result<InvoiceEscrow, Error> {
        Self::fund_impl(env, investor, amount, true, 0)
    }

    /// First deposit only (per investor): optional longer lock and tier ladder from [`DataKey::YieldTierTable`].
    /// Sets [`DataKey::InvestorClaimNotBefore`] when `committed_lock_secs > 0`. Additional principal
    /// from the same investor must use [`LiquifactEscrow::fund`].
    ///
    /// # Errors
    /// Returns [`Error::FundingAmountNotPositive`] if `amount <= 0`.
    /// Returns [`Error::BelowMinContributionFloor`] if amount is below configured floor.
    /// Returns [`Error::LegalHoldActive`] if legal hold is enabled.
    /// Returns [`Error::EscrowNotOpen`] if escrow status is not open (0).
    /// Returns [`Error::UniqueInvestorCapReached`] if cap is set and reached.
    /// Returns [`Error::FundWithCommitmentNotFirstDeposit`] if investor already has a contribution.
    /// Returns [`Error::CommitmentLockOverflow`] if committed_lock_secs causes timestamp overflow.
    pub fn fund_with_commitment(
        env: Env,
        investor: Address,
        amount: i128,
        committed_lock_secs: u64,
    ) -> Result<InvoiceEscrow, Error> {
        Self::fund_impl(env, investor, amount, false, committed_lock_secs)
    }

    fn fund_impl(
        env: Env,
        investor: Address,
        amount: i128,
        simple_fund: bool,
        committed_lock_secs: u64,
    ) -> Result<InvoiceEscrow, Error> {
        investor.require_auth();

        if amount <= 0 {
            return Err(Error::FundingAmountNotPositive);
        }

        let floor: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinContributionFloor)
            .unwrap_or(0);
        if floor > 0 && amount < floor {
            return Err(Error::BelowMinContributionFloor);
        }

        let mut escrow = Self::get_escrow(env.clone())?;
        if Self::legal_hold_active(&env) {
            return Err(Error::LegalHoldActive);
        }
        if escrow.status != 0 {
            return Err(Error::EscrowNotOpen);
        }

        let contribution_key = DataKey::InvestorContribution(investor.clone());
        let prev: i128 = env.storage().instance().get(&contribution_key).unwrap_or(0);

        if prev == 0 {
            if let Some(cap) = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&DataKey::MaxUniqueInvestorsCap)
            {
                let cur: u32 = env
                    .storage()
                    .instance()
                    .get(&DataKey::UniqueFunderCount)
                    .unwrap_or(0);
                if cur >= cap {
                    return Err(Error::UniqueInvestorCapReached);
                }
            }
        }

        if simple_fund {
            if prev == 0 {
                env.storage().instance().set(
                    &DataKey::InvestorEffectiveYield(investor.clone()),
                    &escrow.yield_bps,
                );
                env.storage()
                    .instance()
                    .set(&DataKey::InvestorClaimNotBefore(investor.clone()), &0u64);
            }
        } else {
            if prev != 0 {
                return Err(Error::FundWithCommitmentNotFirstDeposit);
            }
            let eff =
                Self::effective_yield_for_commitment(&env, escrow.yield_bps, committed_lock_secs);
            env.storage()
                .instance()
                .set(&DataKey::InvestorEffectiveYield(investor.clone()), &eff);
            let now = env.ledger().timestamp();
            let claim_nb = if committed_lock_secs == 0 {
                0u64
            } else {
                let computed = now.checked_add(committed_lock_secs);
                match computed {
                    Some(v) => v,
                    None => return Err(Error::CommitmentLockOverflow),
                }
            };
            env.storage().instance().set(
                &DataKey::InvestorClaimNotBefore(investor.clone()),
                &claim_nb,
            );
        }

        let new_funded = escrow
            .funded_amount
            .checked_add(amount)
            .ok_or(Error::FundedAmountOverflow)?;
        escrow.funded_amount = new_funded;

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

        env.storage()
            .instance()
            .set(&contribution_key, &(prev + amount));

        if prev == 0 {
            let cur: u32 = env
                .storage()
                .instance()
                .get(&DataKey::UniqueFunderCount)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::UniqueFunderCount, &(cur + 1));
        }

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        let investor_effective_yield_bps = env
            .storage()
            .instance()
            .get(&DataKey::InvestorEffectiveYield(investor.clone()))
            .unwrap_or(escrow.yield_bps);

        EscrowFunded {
            name: symbol_short!("funded"),
            invoice_id: escrow.invoice_id.clone(),
            investor: investor.clone(),
            amount,
            funded_amount: escrow.funded_amount,
            status: escrow.status,
            investor_effective_yield_bps,
        }
        .publish(&env);

        Ok(escrow)
    }

    /// Settle the escrow, transitioning to settled state.
    ///
    /// # Errors
    /// Returns [`Error::LegalHoldActive`] if legal hold is enabled.
    /// Returns [`Error::EscrowNotInitialized`] if init has not been called.
    /// Returns [`Error::EscrowNotFunded`] if escrow is not funded (status != 1).
    /// Returns [`Error::EscrowNotMature`] if maturity timestamp has not been reached.
    pub fn settle(env: Env) -> Result<InvoiceEscrow, Error> {
        if Self::legal_hold_active(&env) {
            return Err(Error::LegalHoldActive);
        }

        let mut escrow = Self::get_escrow(env.clone())?;

        escrow.sme_address.require_auth();
        if escrow.status != 1 {
            return Err(Error::EscrowNotFunded);
        }

        if escrow.maturity > 0 {
            let now = env.ledger().timestamp();
            if now < escrow.maturity {
                return Err(Error::EscrowNotMature);
            }
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

        Ok(escrow)
    }

    /// SME pulls funded liquidity (accounting). Blocked when a legal hold is active.
    ///
    /// # Errors
    /// Returns [`Error::LegalHoldActive`] if legal hold is enabled.
    /// Returns [`Error::EscrowNotInitialized`] if init has not been called.
    /// Returns [`Error::EscrowNotFunded`] if escrow is not funded (status != 1).
    pub fn withdraw(env: Env) -> Result<InvoiceEscrow, Error> {
        if Self::legal_hold_active(&env) {
            return Err(Error::LegalHoldActive);
        }

        let mut escrow = Self::get_escrow(env.clone())?;
        escrow.sme_address.require_auth();

        if escrow.status != 1 {
            return Err(Error::EscrowNotFunded);
        }

        let amount = escrow.funded_amount;
        escrow.status = 3;

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        SmeWithdrew {
            name: symbol_short!("sme_wd"),
            invoice_id: escrow.invoice_id.clone(),
            amount,
        }
        .publish(&env);

        Ok(escrow)
    }

    /// Investor records a payout claim after settlement. Idempotent marker per investor.
    ///
    /// # Errors
    /// Returns [`Error::LegalHoldActive`] if legal hold is enabled.
    /// Returns [`Error::EscrowNotInitialized`] if init has not been called.
    /// Returns [`Error::EscrowNotSettled`] if escrow is not settled (status != 2).
    /// Returns [`Error::CommitmentLockNotExpired`] if the investor's claim lock has not expired.
    /// Returns [`Error::InvestorAlreadyClaimed`] if the investor has already claimed.
    pub fn claim_investor_payout(env: Env, investor: Address) -> Result<(), Error> {
        if Self::legal_hold_active(&env) {
            return Err(Error::LegalHoldActive);
        }

        investor.require_auth();

        let escrow = Self::get_escrow(env.clone())?;
        if escrow.status != 2 {
            return Err(Error::EscrowNotSettled);
        }

        let not_before: u64 = env
            .storage()
            .instance()
            .get(&DataKey::InvestorClaimNotBefore(investor.clone()))
            .unwrap_or(0);
        let now = env.ledger().timestamp();
        if now < not_before {
            return Err(Error::CommitmentLockNotExpired);
        }

        let key = DataKey::InvestorClaimed(investor.clone());
        if env.storage().instance().get(&key).unwrap_or(false) {
            return Err(Error::InvestorAlreadyClaimed);
        }

        env.storage().instance().set(&key, &true);

        InvestorPayoutClaimed {
            name: symbol_short!("inv_claim"),
            invoice_id: escrow.invoice_id.clone(),
            investor,
        }
        .publish(&env);
        Ok(())
    }

    /// Update the maturity timestamp.
    ///
    /// # Errors
    /// Returns [`Error::EscrowNotInitialized`] if init has not been called.
    /// Returns [`Error::MaturityUpdateNotOpen`] if escrow status is not open (0).
    pub fn update_maturity(env: Env, new_maturity: u64) -> Result<InvoiceEscrow, Error> {
        let mut escrow = Self::get_escrow(env.clone())?;
        escrow.admin.require_auth();

        if escrow.status != 0 {
            return Err(Error::MaturityUpdateNotOpen);
        }

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

        Ok(escrow)
    }

    /// Transfer admin role to a new address.
    ///
    /// # Errors
    /// Returns [`Error::EscrowNotInitialized`] if init has not been called.
    /// Returns [`Error::AdminNotDifferent`] if `new_admin` equals current admin.
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<InvoiceEscrow, Error> {
        let mut escrow = Self::get_escrow(env.clone())?;

        escrow.admin.require_auth();

        if escrow.admin == new_admin {
            return Err(Error::AdminNotDifferent);
        }

        escrow.admin = new_admin;

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        AdminTransferredEvent {
            name: symbol_short!("admin"),
            invoice_id: escrow.invoice_id.clone(),
            new_admin: escrow.admin.clone(),
        }
        .publish(&env);

        Ok(escrow)
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_funding_target;

#[cfg(test)]
mod test_token_integration;
