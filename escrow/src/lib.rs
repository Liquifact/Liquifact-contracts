//! # LiquiFact Escrow Contract
//!
//! Soroban smart contract holding investor funds for tokenized invoices until
//! settlement on the Stellar network.
//!
//! ## Contract Version
//!
//! This contract exposes [`EscrowContract::version`] — a read-only introspection
//! method that returns the current semantic version string (`"MAJOR.MINOR.PATCH"`).
//!
//! ### Version semantics
//!
//! | Segment | Meaning                                                      |
//! |---------|--------------------------------------------------------------|
//! | MAJOR   | Breaking change to the public interface or storage layout    |
//! | MINOR   | Backwards-compatible new functionality                       |
//! | PATCH   | Backwards-compatible bug fixes / documentation only          |
//!
//! Tooling, migration scripts, and indexers **should** call `version()` before
//! interacting with a deployed instance so they can gate logic on a known
//! version range and fail fast on an incompatible contract.
//!
//! ### Upgrade workflow assumptions
//!
//! * The version string is compiled into the WASM binary; there is no mutable
//!   on-chain version storage.  Bumping the version therefore always requires
//!   redeployment of a new WASM binary.
//! * A MAJOR bump signals that existing escrow storage keys / data shapes may
//!   have changed.  Migration tooling **must** re-read the version before
//!   performing any read-modify-write on ledger entries.
//! * MINOR / PATCH bumps are safe for existing deployments; clients that
//!   understand `"1.0.0"` can consume `"1.1.0"` without modification.

/// Semantic version of this contract binary.
///
/// Increment according to the table in the module-level docs:
/// * **MAJOR** — breaking change to the public ABI or storage schema.
/// * **MINOR** — new, backwards-compatible functionality.
/// * **PATCH** — bug-fix / docs only; no behaviour change.
pub const CONTRACT_VERSION: &str = "1.0.0";

// ---------------------------------------------------------------------------
// Minimal no-std / no-soroban-sdk stub types so the contract logic and tests
// compile with plain `cargo test` without the full Soroban SDK in CI.
// ---------------------------------------------------------------------------

/// Stub String type that mirrors the API surface we use from soroban_sdk::String.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SorobanString(std::string::String);

impl SorobanString {
    /// Create from a Rust `&str`.
    pub fn from_str(_env: &Env, s: &str) -> Self {
        SorobanString(s.to_string())
    }

    /// Return the inner Rust string (test / tooling helper).
    pub fn to_string(&self) -> std::string::String {
        self.0.clone()
    }
}

/// Minimal Env stub — real Soroban SDK provides a richer type.
#[derive(Default, Clone)]
pub struct Env;

// ---------------------------------------------------------------------------
// Escrow domain types
// ---------------------------------------------------------------------------

/// Lifecycle status of an invoice escrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscrowStatus {
    /// Created but not yet fully funded by investors.
    Pending,
    /// Target funding amount reached; awaiting buyer payment.
    Funded,
    /// Buyer paid; principal + yield distributed to investors.
    Settled,
}

/// All state associated with a single invoice escrow.
#[derive(Debug, Clone)]
pub struct Escrow {
    /// Unique invoice identifier supplied by the LiquiFact backend.
    pub invoice_id: u64,
    /// Stellar address of the SME (invoice issuer).
    pub sme_address: std::string::String,
    /// Target funding amount in stroops (1 XLM = 10_000_000 stroops).
    pub amount: i128,
    /// Annual yield in basis-points (e.g. 500 = 5 %).
    pub yield_bps: u32,
    /// Unix timestamp after which settlement may be triggered.
    pub maturity: u64,
    /// Total amount funded by investors so far.
    pub funded_amount: i128,
    /// Current lifecycle status.
    pub status: EscrowStatus,
}

// ---------------------------------------------------------------------------
// Contract implementation
// ---------------------------------------------------------------------------

/// LiquiFact escrow contract.
pub struct EscrowContract;

impl EscrowContract {
    // -----------------------------------------------------------------------
    // Version introspection (Issue #26)
    // -----------------------------------------------------------------------

    /// Return the semantic version of this contract binary.
    ///
    /// This is a **read-only** method — it touches no ledger state and costs
    /// only the minimal computation required to construct the return value.
    ///
    /// # Usage
    ///
    /// ```
    /// use escrow::{EscrowContract, Env};
    ///
    /// let env = Env::default();
    /// let version = EscrowContract::version(&env);
    /// assert_eq!(version.to_string(), "1.0.0");
    /// ```
    ///
    /// # Tooling / migration guidance
    ///
    /// ```text
    /// const MIN_SUPPORTED: &str = "1.0.0";
    ///
    /// let v = contract.version(&env).to_string();
    /// assert!(semver_compat(&v, MIN_SUPPORTED), "contract too old: {v}");
    /// ```
    ///
    /// # Security
    ///
    /// * No state mutation — safe to call from any context.
    /// * No authentication required — purely informational.
    /// * Cannot be spoofed at runtime; the value is a compile-time constant
    ///   embedded in the WASM binary.
    pub fn version(env: &Env) -> SorobanString {
        SorobanString::from_str(env, CONTRACT_VERSION)
    }

    // -----------------------------------------------------------------------
    // Core escrow operations
    // -----------------------------------------------------------------------

    /// Initialise a new invoice escrow.
    ///
    /// # Panics
    ///
    /// * `amount` must be > 0.
    /// * `yield_bps` must be ≤ 10_000 (100 %).
    pub fn init(
        invoice_id: u64,
        sme_address: std::string::String,
        amount: i128,
        yield_bps: u32,
        maturity: u64,
    ) -> Escrow {
        assert!(amount > 0, "amount must be positive");
        assert!(yield_bps <= 10_000, "yield_bps must be <= 10000");

        Escrow {
            invoice_id,
            sme_address,
            amount,
            yield_bps,
            maturity,
            funded_amount: 0,
            status: EscrowStatus::Pending,
        }
    }

    /// Read the current state of an escrow (pass-through in this stub).
    pub fn get_escrow(escrow: &Escrow) -> &Escrow {
        escrow
    }

    /// Record investor funding.
    ///
    /// Transitions status to `Funded` when `funded_amount >= amount`.
    ///
    /// # Panics
    ///
    /// * `fund_amount` must be > 0.
    /// * Escrow must not already be `Settled`.
    pub fn fund(escrow: &mut Escrow, fund_amount: i128) {
        assert!(fund_amount > 0, "fund_amount must be positive");
        assert!(
            escrow.status != EscrowStatus::Settled,
            "cannot fund a settled escrow"
        );

        escrow.funded_amount += fund_amount;
        if escrow.funded_amount >= escrow.amount {
            escrow.status = EscrowStatus::Funded;
        }
    }

    /// Settle an escrow (buyer paid; investors receive principal + yield).
    ///
    /// # Panics
    ///
    /// * Escrow must be in `Funded` status before settlement.
    pub fn settle(escrow: &mut Escrow) {
        assert!(
            escrow.status == EscrowStatus::Funded,
            "escrow must be funded before settlement"
        );
        escrow.status = EscrowStatus::Settled;
    }
}

// ---------------------------------------------------------------------------
// Tests live in a separate module, following Soroban convention.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod test;