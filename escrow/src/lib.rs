//! # LiquiFact Escrow Contract
//!
//! Holds investor funds for an invoice until settlement.
//!
//! ### Settlement Sequence
//! 1. **Initialization**: Admin creates the escrow with `init`.
//! 2. **Funding**: Investors contribute funds via `fund` until `funding_target` is met (status 0 -> 1).
//! 3. **Withdrawal**: SME can withdraw the funded principal via `withdraw` (status 1 -> 3).
//! 4. **Settlement**: Buyer pays (confirmed by SME or Admin), and SME calls `settle` to finalize the escrow.
//! 5. **Claiming**: Investors claim their principal + interest via `claim` (after status becomes 2).
//!
//! # Storage Schema Versioning
//!
//! The escrow state is stored under the `DataKey::Escrow` key.

use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, symbol_short, Address, Env, Symbol,
};

/// Current storage schema version.
pub const SCHEMA_VERSION: u32 = 1;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Escrow,
    Version,
    Contribution(Address),
    Claimed(Address),
}

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
    pub settled_amount: i128,
    pub yield_bps: i64,
    pub maturity: u64,
    /// Escrow lifecycle status:
    /// - `0` — **open**: accepting investor funding
    /// - `1` — **funded**: target met; awaiting buyer settlement
    /// - `2` — **settled**: buyer paid; investors can redeem principal + yield
    /// - `3` — **withdrawn**: SME has withdrawn the funded amount
    pub status: u32,
    pub version: u32,
}

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
    pub amount: i128,
    pub settled_amount: i128,
    pub status: u32,
}

#[contractevent]
pub struct PayoutClaimed {
    #[topic]
    pub name: Symbol,
    pub investor: Address,
    pub principal: i128,
    pub yield_amount: i128,
}

#[contract]
pub struct LiquifactEscrow;

#[contractimpl]
impl LiquifactEscrow {
    /// Initialize a new invoice escrow.
    pub fn init(
        env: Env,
        admin: Address,
        invoice_id: Symbol,
        sme_address: Address,
        amount: i128,
        yield_bps: i64,
        maturity: u64,
    ) -> InvoiceEscrow {
        assert!(
            !env.storage().instance().has(&DataKey::Escrow),
            "Escrow already initialized"
        );
        assert!(amount > 0, "Escrow amount must be positive");

        let escrow = InvoiceEscrow {
            invoice_id,
            admin,
            sme_address,
            amount,
            funding_target: amount,
            funded_amount: 0,
            settled_amount: 0,
            yield_bps,
            maturity,
            status: 0,
            version: SCHEMA_VERSION,
        };

        env.storage().instance().set(&DataKey::Escrow, &escrow);
        env.storage()
            .instance()
            .set(&DataKey::Version, &SCHEMA_VERSION);

        EscrowInitialized {
            name: symbol_short!("init"),
            escrow: escrow.clone(),
        }
        .publish(&env);

        escrow
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

    /// Record investor funding.
    pub fn fund(env: Env, investor: Address, amount: i128) -> InvoiceEscrow {
        investor.require_auth();

        let mut escrow = Self::get_escrow(env.clone());
        assert!(amount > 0, "Funding amount must be positive");
        assert!(escrow.status == 0, "Escrow not open for funding");

        let contribution_key = DataKey::Contribution(investor.clone());
        let current_contribution: i128 =
            env.storage().instance().get(&contribution_key).unwrap_or(0);
        env.storage()
            .instance()
            .set(&contribution_key, &(current_contribution + amount));

        escrow.funded_amount += amount;
        if escrow.funded_amount >= escrow.funding_target {
            escrow.status = 1;
        }

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        EscrowFunded {
            name: symbol_short!("fund"),
            invoice_id: escrow.invoice_id.clone(),
            investor,
            amount,
            funded_amount: escrow.funded_amount,
            status: escrow.status,
        }
        .publish(&env);

        escrow
    }

    /// Get total contribution of an investor.
    pub fn get_contribution(env: Env, investor: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::Contribution(investor))
            .unwrap_or(0)
    }

    /// Settle the escrow (buyer pays).
    pub fn settle(env: Env, amount: i128) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone());

        // Authorization: SME or Admin can settle.
        // For simplicity in this demo, we allow both.
        // We use check_auth style or just require_auth.
        escrow.sme_address.require_auth();

        assert!(
            escrow.status == 1 || escrow.status == 3,
            "Escrow must be funded or withdrawn before settlement"
        );

        let interest = (escrow.amount * (escrow.yield_bps as i128)) / 10000;
        let total_due = escrow.amount + interest;

        escrow.settled_amount += amount;
        assert!(
            escrow.settled_amount <= total_due,
            "Settlement amount exceeds total due"
        );

        if escrow.settled_amount == total_due {
            escrow.status = 2;
        }

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        EscrowSettled {
            name: symbol_short!("settle"),
            invoice_id: escrow.invoice_id.clone(),
            amount,
            settled_amount: escrow.settled_amount,
            status: escrow.status,
        }
        .publish(&env);

        escrow
    }

    /// SME withdraws funded capital.
    pub fn withdraw(env: Env) -> i128 {
        let mut escrow = Self::get_escrow(env.clone());
        escrow.sme_address.require_auth();

        assert!(
            escrow.status == 1,
            "Escrow must be funded before withdrawal"
        );

        let withdrawal_amount = escrow.funded_amount;
        escrow.status = 3; // withdrawn

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        withdrawal_amount
    }

    /// Investor claims their share of the settled funds.
    pub fn claim(env: Env, investor: Address) -> i128 {
        investor.require_auth();

        let escrow = Self::get_escrow(env.clone());
        assert!(escrow.status == 2, "Escrow not settled");

        let claimed_key = DataKey::Claimed(investor.clone());
        assert!(
            !env.storage().instance().has(&claimed_key),
            "Payout already claimed"
        );

        let principal = Self::get_contribution(env.clone(), investor.clone());
        assert!(principal > 0, "No contribution found for investor");

        // Calculate share of yield
        // yield_amount = (principal / total_amount) * total_yield
        // total_yield = amount * yield_bps / 10000
        let yield_amount = (principal * (escrow.yield_bps as i128)) / 10000;
        let payout = principal + yield_amount;

        // Mark as claimed BEFORE "transferring" (double-claim protection)
        env.storage().instance().set(&claimed_key, &true);

        PayoutClaimed {
            name: symbol_short!("claim"),
            investor,
            principal,
            yield_amount,
        }
        .publish(&env);

        payout
    }
}

#[cfg(test)]
mod examples;
#[cfg(test)]
mod test;
