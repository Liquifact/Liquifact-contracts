//! LiquiFact Escrow Contract
//!
//! Holds investor funds for an invoice until settlement.
//! - SME receives stablecoin when funding target is met
//! - Investors receive principal + yield when buyer pays at maturity
//!
//! ### Settlement Sequence
//! 1. **Initialization**: Admin creates the escrow with `init`.
//! 2. **Funding**: Investors contribute funds via `fund` until `funding_target` is met (status 0 -> 1).
//! 3. **Settlement**: SME calls `settle` to finalize the escrow, moving it to status 2.
//!
//! The contract emits the following Soroban events for off-chain indexers:
//!
//! | Version | Changes |
//! |---------|---------|
//! | 1       | Initial schema |

use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, symbol_short, Address, Env, Symbol,
};

/// Current storage schema version.
pub const SCHEMA_VERSION: u32 = 1;

// ── Storage key ───────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Escrow,
    InvestorContribution(Address),
    Version,
}

// ── Data types ────────────────────────────────────────────────────────────────

/// Full state of an invoice escrow persisted in contract storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceEscrow {
    pub invoice_id: Symbol,
    pub admin: Address,
    pub sme_address: Address,
    pub amount: i128,
    pub funding_target: i128,
    pub funded_amount: i128,
    pub yield_bps: i64,
    pub maturity: u64,
    /// 0 = open, 1 = funded, 2 = settled, 3 = withdrawn
    pub status: u32,
}

// ── Event types ───────────────────────────────────────────────────────────────

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

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct LiquifactEscrow;

#[contractimpl]
impl LiquifactEscrow {
    // ── init ──────────────────────────────────────────────────────────────────

    /// Initialize a new invoice escrow.
    ///
    /// # Authorization
    /// Requires authorization from `admin`.
    ///
    /// # Panics
    /// - If `amount <= 0`
    /// - If `yield_bps > 10_000`
    /// - If the escrow has already been initialized
    pub fn init(
        env: Env,
        invoice_id: Symbol,
        sme_address: Address,
        amount: i128,
        yield_bps: i64,
        maturity: u64,
    ) -> InvoiceEscrow {
        admin.require_auth();

        assert!(
            !env.storage().instance().has(&ESCROW_KEY),
            "Escrow already initialized"
        );

        let escrow = InvoiceEscrow {
            invoice_id: invoice_id.clone(),
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

        EscrowInitialized {
            name: symbol_short!("escrow_ii"),
            escrow: escrow.clone(),
        }
        .publish(&env);

        escrow
    }

    // ── get_escrow ────────────────────────────────────────────────────────────

    /// Return the current escrow state without modifying storage.
    pub fn get_escrow(env: Env) -> InvoiceEscrow {
        env.storage()
            .instance()
            .get(&DataKey::Escrow)
            .unwrap_or_else(|| panic!("Escrow not initialized"))
    }

    /// Returns the stored schema version.
    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Version).unwrap_or(0)
    }

    /// Returns the contribution amount for a given investor.
    pub fn get_contribution(env: Env, investor: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::InvestorContribution(investor))
            .unwrap_or(0)
    }

    // ── migrate ───────────────────────────────────────────────────────────────

    /// Migrate storage from an older schema version to the current one.
    pub fn migrate(env: Env, from_version: u32) -> u32 {
        let stored: u32 = env.storage().instance().get(&DataKey::Version).unwrap_or(0);

        assert!(
            stored_version == from_version,
            "from_version does not match stored version"
        );
        assert!(
            from_version < SCHEMA_VERSION,
            "Already at current schema version"
        );

        panic!("No migration path from version {}", from_version);
    }

    // ── fund ──────────────────────────────────────────────────────────────────

    /// Record investor funding.
    ///
    /// # Authorization
    /// Requires authorization from `investor`.
    ///
    /// # Panics
    /// - If the escrow is not in the open (status = 0) state.
    /// - If `amount` is zero or negative.
    pub fn fund(env: Env, investor: Address, amount: i128) -> InvoiceEscrow {
        investor.require_auth();

        let mut escrow = Self::get_escrow(env.clone());

        assert!(amount > 0, "Funding amount must be positive");
        assert!(escrow.status == 0, "Escrow not open for funding");
        escrow.funded_amount += amount;
        if escrow.funded_amount >= escrow.funding_target {
            escrow.status = 1;
        }

        // Track per-investor contribution
        let prev: i128 = env
            .storage()
            .instance()
            .get(&DataKey::InvestorContribution(investor.clone()))
            .unwrap_or(0);
        env.storage().instance().set(
            &DataKey::InvestorContribution(investor.clone()),
            &(prev + amount),
        );

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        EscrowFunded {
            name: symbol_short!("funded"),
            invoice_id: escrow.invoice_id.clone(),
            investor,
            amount,
            funded_amount: escrow.funded_amount,
            status: escrow.status,
            is_paid: false,
        }
        .publish(&env);

        escrow
    }

    // ── settle ────────────────────────────────────────────────────────────────

    /// Mark escrow as settled (buyer paid).
    ///
    /// # Authorization
    /// Requires authorization from the `sme_address` stored in the escrow.
    ///
    /// # Panics
    /// - If the escrow is not in the funded (status = 1) state.
    /// - If the current ledger timestamp is before `maturity` (when maturity > 0).
    pub fn settle(env: Env) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone());

        escrow.sme_address.require_auth();
        assert!(
            escrow.status == 1,
            "Escrow must be funded before settlement"
        );

        // Maturity check: if maturity is set (> 0), ledger time must have reached it
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

    // ── update_maturity ───────────────────────────────────────────────────────

    /// Update maturity timestamp. Only allowed by admin in Open state.
    pub fn update_maturity(env: Env, new_maturity: u64) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone());

        escrow.admin.require_auth();

        assert!(
            escrow.status == 0,
            "Maturity can only be updated in Open state"
        );

        let (yield_amount, total_payout) =
            Self::compute_expected_payout(&escrow, record.contribution);

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        MaturityUpdatedEvent {
            name: symbol_short!("maturity"),
            invoice_id: escrow.invoice_id.clone(),
            old_maturity,
            new_maturity,
        }
        .publish(&env);

        // Audit event.
        InvestorRedeemed {
            name: symbol_short!("redeemed"),
            invoice_id: escrow.invoice_id.clone(),
            investor: investor.clone(),
            principal: record.contribution,
            yield_amount,
            total_payout,
        }
        .publish(&env);

        Self::build_position_view(&escrow, &investor, &record)
    }

    // ── withdraw ──────────────────────────────────────────────────────────────

    /// Withdraw funded liquidity to the SME wallet.
    ///
    /// # Authorization
    /// Requires authorization from the `sme_address` stored in the escrow.
    ///
    /// # Panics
    /// - If the escrow is not in the funded (status = 1) state.
    pub fn withdraw(env: Env) -> i128 {
        let mut escrow = Self::get_escrow(env.clone());

        escrow.sme_address.require_auth();

        assert!(
            escrow.status == 1,
            "Escrow must be funded before withdrawal"
        );
        assert!(
            escrow.invoice_id == target_invoice_id,
            "Target escrow invoice_id does not match"
        );

        let withdrawal_amount = escrow.funded_amount;
        escrow.status = 3;
        escrow.funded_amount = 0;
        env.storage().instance().set(&DataKey::Escrow, &escrow);

        withdrawal_amount
    }

    // ── transfer_admin ────────────────────────────────────────────────────────

    /// Transfer admin role to a new address.
    ///
    /// # Authorization
    /// Requires authorization from the current `admin`.
    ///
    /// # Panics
    /// - If `new_admin` is the same as the current admin.
    /// - If the escrow is not initialized.
    pub fn transfer_admin(env: Env, new_admin: Address) -> InvoiceEscrow {
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
}

// ---------------------------------------------------------------------------
#[cfg(test)]
mod test;
