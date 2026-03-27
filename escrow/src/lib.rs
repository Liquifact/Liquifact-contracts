//! # LiquiFact Escrow Contract
//!
//! Holds investor funds for an invoice until settlement.
//!
//! ## Lifecycle
//! 1. **Initialization** — Admin creates the escrow with [`LiquifactEscrow::init`].
//! 2. **Funding** — Investors call [`LiquifactEscrow::fund`] until `funding_target` is met;
//!    any amount above the remaining need is not counted toward escrow state (see cap semantics).
//! 3. **Settlement** — SME calls [`LiquifactEscrow::settle`] after the invoice is repaid.
//!
//! **Cancellation** — While still open with no funds, the admin may call [`LiquifactEscrow::cancel`].
//!
//! ## Schema
//! Instance storage keys: `"escrow"` ([`InvoiceEscrow`]), `"version"` (`u32`).

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, symbol_short, Address, Env,
    Symbol,
};

/// Current storage schema version. Bump when [`InvoiceEscrow`] layout changes.
pub const SCHEMA_VERSION: u32 = 2;

/// Open for funding (`funded_amount < funding_target`).
pub const STATUS_OPEN: u32 = 0;
/// Target met; awaiting settlement.
pub const STATUS_FUNDED: u32 = 1;
/// Buyer repaid; investors redeem off-chain using events + state.
pub const STATUS_SETTLED: u32 = 2;
/// Admin cancelled before any funds were accepted.
pub const STATUS_CANCELLED: u32 = 3;

/// Maximum yield in basis points (100% APY semantics for the stored rate).
pub const MAX_YIELD_BPS: i64 = 10_000;

/// Reject maturities more than this many seconds after the current ledger time.
pub const MAX_MATURITY_DELTA_SECS: u64 = 10 * 365 * 24 * 60 * 60;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidAmount = 3,
    InvalidFundingTarget = 4,
    InvalidMaturity = 5,
    InvalidYieldBps = 6,
    NotOpenForFunding = 7,
    NoFundingCapacity = 8,
    MustBeFundedToSettle = 9,
    CancelNotAllowed = 10,
    MaturityUpdateNotAllowed = 11,
    WrongMigrationVersion = 12,
    NoMigrationPath = 13,
    ArithmeticOverflow = 14,
}

// ---------------------------------------------------------------------------
// Legacy V1 schema (must match deployed layout byte-for-byte)
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceEscrowV1 {
    pub invoice_id: Symbol,
    pub admin: Address,
    pub sme_address: Address,
    pub amount: i128,
    pub funding_target: i128,
    pub funded_amount: i128,
    pub settled_amount: i128,
    pub yield_bps: i64,
    pub maturity: u64,
    pub status: u32,
    pub version: u32,
}

/// Full escrow state (schema version ≥ 2).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceEscrow {
    pub invoice_id: Symbol,
    pub admin: Address,
    pub sme_address: Address,
    pub amount: i128,
    pub funding_target: i128,
    pub funded_amount: i128,
    pub settled_amount: i128,
    pub yield_bps: i64,
    pub maturity: u64,
    pub status: u32,
    pub version: u32,
    /// Opaque round label for analytics (e.g. `"R3Q26"`).
    pub funding_round_id: Symbol,
    /// Ledger timestamp (seconds) when [`LiquifactEscrow::init`] completed.
    pub funding_opened_at: u64,
    /// Ledger timestamp when status became [`STATUS_FUNDED`], or `0` while still open.
    pub funding_closed_at: u64,
}

/// Result of [`LiquifactEscrow::fund`]: updated escrow plus accepted / excess amounts.
///
/// `excess_amount` is the portion of the caller's offer that was **not** applied to
/// `funded_amount`. Off-chain code should return those funds to the investor when custodying assets.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundResult {
    pub escrow: InvoiceEscrow,
    /// Amount actually added to `funded_amount` (≤ offer, and caps at remaining target).
    pub amount_accepted: i128,
    /// Offer minus accepted — not escrowed (remainder must be handled off-chain).
    pub excess_amount: i128,
}

#[contractevent]
pub struct EscrowInitialized {
    pub escrow: InvoiceEscrow,
}

#[contractevent]
pub struct EscrowFunded {
    pub invoice_id: Symbol,
    pub investor: Address,
    pub amount_offered: i128,
    pub amount_accepted: i128,
    pub excess_amount: i128,
    pub funded_amount: i128,
    pub status: u32,
}

#[contractevent]
pub struct EscrowCancelled {
    pub invoice_id: Symbol,
    pub admin: Address,
}

#[contractevent]
pub struct EscrowSettled {
    pub invoice_id: Symbol,
    pub funded_amount: i128,
    pub yield_bps: i64,
    pub maturity: u64,
}

#[contractevent]
pub struct MaturityUpdated {
    pub invoice_id: Symbol,
    pub old_maturity: u64,
    pub new_maturity: u64,
}

const ESCROW_KEY: Symbol = symbol_short!("escrow");
const VERSION_KEY: Symbol = symbol_short!("version");

#[contract]
pub struct LiquifactEscrow;

impl LiquifactEscrow {
    fn load(env: &Env) -> Result<InvoiceEscrow, EscrowError> {
        env.storage()
            .instance()
            .get(&ESCROW_KEY)
            .ok_or(EscrowError::NotInitialized)
    }

    fn store(env: &Env, escrow: &InvoiceEscrow) {
        env.storage().instance().set(&ESCROW_KEY, escrow);
        env.storage().instance().set(&VERSION_KEY, &escrow.version);
    }

    /// Validate economic and time inputs for a new escrow.
    ///
    /// - `amount` and `funding_target` must be positive; target cannot exceed invoice `amount`.
    /// - `yield_bps` must be in `0..=[`MAX_YIELD_BPS`]`.
    /// - `maturity` must be strictly after the current ledger timestamp and within
    ///   [`MAX_MATURITY_DELTA_SECS`] of it (prevents garbage / unbounded far-future values).
    pub fn validate_init_params(
        env: &Env,
        amount: i128,
        funding_target: i128,
        yield_bps: i64,
        maturity: u64,
    ) -> Result<(), EscrowError> {
        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }
        if funding_target <= 0 || funding_target > amount {
            return Err(EscrowError::InvalidFundingTarget);
        }
        if yield_bps < 0 || yield_bps > MAX_YIELD_BPS {
            return Err(EscrowError::InvalidYieldBps);
        }
        let now = env.ledger().timestamp();
        if maturity <= now {
            return Err(EscrowError::InvalidMaturity);
        }
        let latest = now.saturating_add(MAX_MATURITY_DELTA_SECS);
        if maturity > latest {
            return Err(EscrowError::InvalidMaturity);
        }
        Ok(())
    }
}

#[contractimpl]
impl LiquifactEscrow {
    /// Initialize a new invoice escrow.
    ///
    /// # Authorization
    /// The `admin` must authorize this call.
    ///
    /// # Errors
    /// See [`Self::validate_init_params`]. Also fails if storage already holds an escrow.
    pub fn init(
        env: Env,
        admin: Address,
        invoice_id: Symbol,
        sme_address: Address,
        amount: i128,
        funding_target: i128,
        yield_bps: i64,
        maturity: u64,
        funding_round_id: Symbol,
    ) -> Result<InvoiceEscrow, EscrowError> {
        admin.require_auth();

        if env.storage().instance().has(&ESCROW_KEY) {
            return Err(EscrowError::AlreadyInitialized);
        }

        Self::validate_init_params(&env, amount, funding_target, yield_bps, maturity)?;

        let now = env.ledger().timestamp();
        let escrow = InvoiceEscrow {
            invoice_id: invoice_id.clone(),
            admin: admin.clone(),
            sme_address,
            amount,
            funding_target,
            funded_amount: 0,
            settled_amount: 0,
            yield_bps,
            maturity,
            status: STATUS_OPEN,
            version: SCHEMA_VERSION,
            funding_round_id,
            funding_opened_at: now,
            funding_closed_at: 0,
        };

        Self::store(&env, &escrow);

        EscrowInitialized {
            escrow: escrow.clone(),
        }
        .publish(&env);

        Ok(escrow)
    }

    /// Current escrow snapshot.
    pub fn get_escrow(env: Env) -> Result<InvoiceEscrow, EscrowError> {
        Self::load(&env)
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&VERSION_KEY).unwrap_or(0)
    }

    /// Record investor funding. Accepts only up to the remaining `funding_target`; the rest is reported as `excess_amount`.
    ///
    /// # Cap semantics
    /// Let `need = funding_target - funded_amount`. The contract sets
    /// `amount_accepted = min(offer, need)` and `excess_amount = offer - amount_accepted`.
    /// `funded_amount` increases by `amount_accepted` only, so it never exceeds `funding_target`.
    ///
    /// # Authorization
    /// The investor must authorize the call.
    ///
    /// # Errors
    /// Escrow must be [`STATUS_OPEN`] with `need > 0`, and `offer > 0`.
    pub fn fund(env: Env, investor: Address, amount: i128) -> Result<FundResult, EscrowError> {
        investor.require_auth();

        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }

        let mut escrow = Self::load(&env)?;

        if escrow.status != STATUS_OPEN {
            return Err(EscrowError::NotOpenForFunding);
        }

        let need = escrow
            .funding_target
            .checked_sub(escrow.funded_amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        if need <= 0 {
            return Err(EscrowError::NoFundingCapacity);
        }

        let amount_accepted = core::cmp::min(amount, need);
        let excess_amount = amount
            .checked_sub(amount_accepted)
            .ok_or(EscrowError::ArithmeticOverflow)?;

        escrow.funded_amount = escrow
            .funded_amount
            .checked_add(amount_accepted)
            .ok_or(EscrowError::ArithmeticOverflow)?;

        if escrow.funded_amount == escrow.funding_target {
            escrow.status = STATUS_FUNDED;
            escrow.funding_closed_at = env.ledger().timestamp();
        }

        Self::store(&env, &escrow);

        EscrowFunded {
            invoice_id: escrow.invoice_id.clone(),
            investor,
            amount_offered: amount,
            amount_accepted,
            excess_amount,
            funded_amount: escrow.funded_amount,
            status: escrow.status,
        }
        .publish(&env);

        Ok(FundResult {
            escrow: escrow.clone(),
            amount_accepted,
            excess_amount,
        })
    }

    /// Mark the escrow settled after repayment (SME attestation).
    ///
    /// # Authorization
    /// `sme_address` must authorize.
    pub fn settle(env: Env) -> Result<InvoiceEscrow, EscrowError> {
        let mut escrow = Self::load(&env)?;
        escrow.sme_address.require_auth();

        if escrow.status != STATUS_FUNDED {
            return Err(EscrowError::MustBeFundedToSettle);
        }

        escrow.status = STATUS_SETTLED;

        Self::store(&env, &escrow);

        EscrowSettled {
            invoice_id: escrow.invoice_id.clone(),
            funded_amount: escrow.funded_amount,
            yield_bps: escrow.yield_bps,
            maturity: escrow.maturity,
        }
        .publish(&env);

        Ok(escrow)
    }

    /// Cancel an open escrow that has not yet received any funds.
    ///
    /// # Rules
    /// - Only [`STATUS_OPEN`] with `funded_amount == 0`.
    /// - Admin must authorize.
    pub fn cancel(env: Env) -> Result<InvoiceEscrow, EscrowError> {
        let mut escrow = Self::load(&env)?;
        escrow.admin.require_auth();

        if escrow.status != STATUS_OPEN || escrow.funded_amount != 0 {
            return Err(EscrowError::CancelNotAllowed);
        }

        escrow.status = STATUS_CANCELLED;

        Self::store(&env, &escrow);

        EscrowCancelled {
            invoice_id: escrow.invoice_id.clone(),
            admin: escrow.admin.clone(),
        }
        .publish(&env);

        Ok(escrow)
    }

    /// Update maturity while the escrow is still open.
    pub fn update_maturity(env: Env, new_maturity: u64) -> Result<InvoiceEscrow, EscrowError> {
        let mut escrow = Self::load(&env)?;
        escrow.admin.require_auth();

        if escrow.status != STATUS_OPEN {
            return Err(EscrowError::MaturityUpdateNotAllowed);
        }

        Self::validate_init_params(
            &env,
            escrow.amount,
            escrow.funding_target,
            escrow.yield_bps,
            new_maturity,
        )?;

        let old = escrow.maturity;
        escrow.maturity = new_maturity;
        Self::store(&env, &escrow);

        MaturityUpdated {
            invoice_id: escrow.invoice_id.clone(),
            old_maturity: old,
            new_maturity,
        }
        .publish(&env);

        Ok(escrow)
    }

    /// Upgrade stored schema. `from_version` must match the stored version.
    pub fn migrate(env: Env, from_version: u32) -> Result<u32, EscrowError> {
        let stored: u32 = env.storage().instance().get(&VERSION_KEY).unwrap_or(0);

        if stored != from_version {
            return Err(EscrowError::WrongMigrationVersion);
        }
        if from_version >= SCHEMA_VERSION {
            return Err(EscrowError::NoMigrationPath);
        }

        if from_version == 1 {
            let old: InvoiceEscrowV1 = env
                .storage()
                .instance()
                .get(&ESCROW_KEY)
                .ok_or(EscrowError::NotInitialized)?;

            let opened = env.ledger().timestamp();
            let mut closed: u64 = 0;
            if old.status >= STATUS_FUNDED && old.funded_amount >= old.funding_target {
                closed = opened;
            }

            let new = InvoiceEscrow {
                invoice_id: old.invoice_id.clone(),
                admin: old.admin.clone(),
                sme_address: old.sme_address.clone(),
                amount: old.amount,
                funding_target: old.funding_target,
                funded_amount: old.funded_amount,
                settled_amount: old.settled_amount,
                yield_bps: old.yield_bps,
                maturity: old.maturity,
                status: old.status,
                version: SCHEMA_VERSION,
                funding_round_id: symbol_short!("MIGRAT"),
                funding_opened_at: 0,
                funding_closed_at: closed,
            };
            Self::store(&env, &new);
            return Ok(SCHEMA_VERSION);
        }

        Err(EscrowError::NoMigrationPath)
    }
}

#[cfg(test)]
mod test;
