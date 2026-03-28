//! LiquiFact Escrow Contract
//!
//! Holds investor funds for an invoice until settlement.
//! - SME receives stablecoin when funding target is met ([`LiquifactEscrow::withdraw`])
//! - SME records optional **collateral commitments** ([`LiquifactEscrow::record_sme_collateral_commitment`]) —
//!   these are **ledger records only**; they do **not** move tokens or trigger liquidation.
//! - [`LiquifactEscrow::settle`] finalizes the escrow after maturity (when configured).
//!
//! ### Settlement Sequence
//! 1. **Initialization**: Admin creates the escrow with `init`.
//! 2. **Funding**: Investors contribute funds via `fund` until `funding_target` is met (status 0 -> 1).
//! 3. **Withdrawal**: SME can withdraw the funded principal via `withdraw` (status 1 -> 3).
//! 4. **Settlement**: Buyer pays (confirmed by SME or Admin), and SME calls `settle` to finalize the escrow.
//! 5. **Claiming**: Investors claim their principal + interest via `claim` (after status becomes 2).
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
    contract, contractevent, contractimpl, contracttype, symbol_short, token::TokenClient, Address,
    Env, String, Symbol, Vec,
};

pub(crate) mod external_calls;

/// Current storage schema version (`DataKey::Version`).
pub const SCHEMA_VERSION: u32 = 4;

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

#[contract]
pub struct LiquifactEscrow;

fn validate_invoice_id_string(env: &Env, invoice_id: &String) -> Symbol {
    let len = invoice_id.len();
    assert!(
        len >= 1 && len <= MAX_INVOICE_ID_STRING_LEN,
        "invoice_id length must be 1..=MAX_INVOICE_ID_STRING_LEN"
    );
    let len_u = len as usize;
    let mut buf = [0u8; 32];
    invoice_id.copy_into_slice(&mut buf[..len_u]);
    for &b in &buf[..len_u] {
        let ok = (b >= b'A' && b <= b'Z')
            || (b >= b'a' && b <= b'z')
            || (b >= b'0' && b <= b'9')
            || b == b'_';
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
    /// Initialize a new invoice escrow.
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
        if tiers.len() == 0 {
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
    ) -> InvoiceEscrow {
        funding_token: Address,
        registry: Option<Address>,
        treasury: Address,
        yield_tiers: Option<Vec<YieldTier>>,
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
        assert!(amount > 0, "Escrow amount must be positive");

        let escrow = InvoiceEscrow {
            invoice_id,
            admin,
            sme_address,

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
            version: SCHEMA_VERSION,
        };

        env.storage().instance().set(&DataKey::Escrow, &escrow);
        env.storage()
            .instance()
            .set(&DataKey::Version, &SCHEMA_VERSION);

        EscrowInitialized {
            name: symbol_short!("init"),
            escrow: escrow.clone(),
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

        EscrowInitialized {
            name: symbol_short!("escrow_ii"),
            // Read the stored value so we do not clone an in-memory escrow snapshot.
            escrow: Self::get_escrow(env.clone()),
        }
        .publish(&env);

        escrow
    }

    /// Bound funding token (immutable after [`LiquifactEscrow::init`]).
    pub fn get_funding_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::FundingToken)
            .unwrap_or_else(|| panic!("Funding token not set"))
    }

    /// Treasury that may receive terminal dust sweeps (immutable after init).
    pub fn get_treasury(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Treasury)
            .unwrap_or_else(|| panic!("Treasury not set"))
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

        let escrow = Self::get_escrow(env.clone());
        assert!(
            escrow.status == 2 || escrow.status == 3,
            "dust sweep only in terminal states (settled or withdrawn)"
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

    /// Record investor funding.
    pub fn fund(env: Env, investor: Address, amount: i128) -> InvoiceEscrow {
        investor.require_auth();

        let mut escrow = Self::get_escrow(env.clone());
    }

    /// Whether a compliance/legal hold is active (defaults to `false` if unset).
    pub fn get_legal_hold(env: Env) -> bool {
        Self::legal_hold_active(&env)
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
    pub fn record_sme_collateral_commitment(
        env: Env,
        asset: Symbol,
        amount: i128,
    ) -> SmeCollateralCommitment {
        assert!(amount > 0, "Collateral amount must be positive");
        let escrow = Self::get_escrow(env.clone());
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

        commitment
    }

    /// Set or clear compliance hold. Only [`InvoiceEscrow::admin`] may call.
    ///
    /// **Emergency / override:** clearing always goes through this admin-gated path. Deployments
    /// should use a governed `admin` (multisig or protocol DAO). There is no separate “break glass”
    /// entrypoint in this version — operational playbooks live off-chain.
    pub fn set_legal_hold(env: Env, active: bool) {
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

    /// Convenience alias for [`LiquifactEscrow::set_legal_hold`] with `active = false`.
    pub fn clear_legal_hold(env: Env) {
        Self::set_legal_hold(env, false);
    }

    pub fn update_funding_target(env: Env, new_target: i128) -> InvoiceEscrow {
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

    /// Migrate stored schema version.
    ///
    /// New optional keys (`LegalHold`, `SmeCollateralPledge`, etc.) are **additive**: older
    /// bytecode can ignore unknown instance keys. Changing stored `InvoiceEscrow` layout still
    /// requires a coordinated migration or redeploy — see repository README.
    pub fn migrate(env: Env, from_version: u32) -> u32 {
        let stored: u32 = env.storage().instance().get(&DataKey::Version).unwrap_or(0);

        assert!(
            stored == from_version,
            "from_version does not match stored version"
        );

        if from_version >= SCHEMA_VERSION {
            panic!("Already at current schema version");
        }

        panic!(
            "No migration path from version {} — extend migrate or redeploy",
            from_version
        );
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

        let mut escrow = Self::get_escrow(env.clone());
        assert!(
            !Self::legal_hold_active(&env),
            "Legal hold blocks new funding while active"
        );
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
        let contribution_key = DataKey::InvestorContribution(investor.clone());
        let prev: i128 = env.storage().instance().get(&contribution_key).unwrap_or(0);

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
            assert!(
                prev == 0,
                "Additional principal after a tiered first deposit must use fund(), not fund_with_commitment()"
            );
            let eff =
                Self::effective_yield_for_commitment(&env, escrow.yield_bps, committed_lock_secs);
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

        env.storage()
            .instance()
            .set(&contribution_key, &(prev + amount));

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
    pub fn settle(env: Env) -> InvoiceEscrow {
        assert!(
            !Self::legal_hold_active(&env),
            "Legal hold blocks settlement finalization"
        );

        let mut escrow = Self::get_escrow(env.clone());

        escrow.sme_address.require_auth();
        assert!(
            escrow.status == 1 || escrow.status == 3,
            "Escrow must be funded or withdrawn before settlement"
        );

        let interest = (escrow.amount * (escrow.yield_bps as i128)) / 10000;
        let total_due = escrow.amount + interest;

        escrow.settled_amount += amount;
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

        let mut escrow = Self::get_escrow(env.clone());
        escrow.sme_address.require_auth();

        assert!(
            escrow.status == 1,
            "Escrow must be funded before withdrawal"
        );

        if escrow.settled_amount == total_due {
            escrow.status = 2;
        let amount = escrow.funded_amount;
        escrow.status = 3;

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        SmeWithdrew {
            name: symbol_short!("sme_wd"),
            invoice_id: escrow.invoice_id.clone(),
            amount,
        }
        .publish(&env);

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        EscrowSettled {
            name: symbol_short!("settle"),
            invoice_id: escrow.invoice_id.clone(),
            amount,
            settled_amount: escrow.settled_amount,
            status: escrow.status,
        escrow
    }

    /// Investor records a payout claim after settlement. Idempotent marker per investor.
    pub fn claim_investor_payout(env: Env, investor: Address) {
        assert!(
            !Self::legal_hold_active(&env),
            "Legal hold blocks investor claims"
        );

        investor.require_auth();

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

        let key = DataKey::InvestorClaimed(investor.clone());
        assert!(
            !env.storage().instance().get(&key).unwrap_or(false),
            "Investor already claimed"
        );

        env.storage().instance().set(&key, &true);

        InvestorPayoutClaimed {
            name: symbol_short!("inv_claim"),
            invoice_id: escrow.invoice_id.clone(),
            investor,
        }
        .publish(&env);
    }

    pub fn update_maturity(env: Env, new_maturity: u64) -> InvoiceEscrow {
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
mod test;

#[cfg(test)]
mod test_funding_target;

#[cfg(test)]
mod test_token_integration;
