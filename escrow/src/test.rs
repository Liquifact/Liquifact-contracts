//! Unit tests for the LiquiFact escrow contract.
//!
//! Coverage targets (Issue #26 requirement: ≥ 95 %):
//!
//! | Area                    | Tests                                              |
//! |-------------------------|----------------------------------------------------|
//! | `version()`             | correct value, idempotent, type, constant sync     |
//! | `init()`                | happy-path, boundary amounts, invalid inputs       |
//! | `get_escrow()`          | returns same reference / values                    |
//! | `fund()`                | partial, exact, over-fund, status transitions      |
//! | `settle()`              | happy-path, guards (pending / settled re-settle)   |
//! | Full lifecycle          | init → fund → settle end-to-end                   |

use crate::{Env, EscrowContract, EscrowStatus, CONTRACT_VERSION};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_escrow() -> crate::Escrow {
    EscrowContract::init(42, "GABC123".to_string(), 1_000_000, 500, 1_700_000_000)
}

// ===========================================================================
// version() tests
// ===========================================================================

/// The version string must exactly equal the `CONTRACT_VERSION` constant.
#[test]
fn test_version_matches_constant() {
    let env = Env::default();
    let v = EscrowContract::version(&env);
    assert_eq!(
        v.to_string(),
        CONTRACT_VERSION,
        "version() must return CONTRACT_VERSION"
    );
}

/// The version must follow SemVer `MAJOR.MINOR.PATCH` format.
#[test]
fn test_version_is_semver_format() {
    let env = Env::default();
    let v = EscrowContract::version(&env).to_string();
    let parts: Vec<&str> = v.split('.').collect();
    assert_eq!(
        parts.len(),
        3,
        "version must have three dot-separated segments"
    );
    for part in &parts {
        part.parse::<u32>()
            .expect("each version segment must be a non-negative integer");
    }
}

/// Calling version() twice must return the same value (idempotent / pure).
#[test]
fn test_version_is_idempotent() {
    let env = Env::default();
    let v1 = EscrowContract::version(&env).to_string();
    let v2 = EscrowContract::version(&env).to_string();
    assert_eq!(v1, v2, "version() must be pure and idempotent");
}

/// The current version must start with "1." (MAJOR = 1 for initial release).
#[test]
fn test_version_major_is_one() {
    let env = Env::default();
    let v = EscrowContract::version(&env).to_string();
    assert!(
        v.starts_with("1."),
        "initial release must have MAJOR = 1, got: {v}"
    );
}

/// The hard-coded constant itself must be non-empty.
#[test]
fn test_contract_version_constant_not_empty() {
    assert!(
        !CONTRACT_VERSION.is_empty(),
        "CONTRACT_VERSION must not be empty"
    );
}

/// version() must not be "0.0.0" (sentinel / uninitialised value).
#[test]
fn test_version_not_zero() {
    let env = Env::default();
    let v = EscrowContract::version(&env).to_string();
    assert_ne!(v, "0.0.0", "version must not be the zero sentinel");
}

/// The SorobanString returned by version() round-trips through to_string().
#[test]
fn test_version_soroban_string_roundtrip() {
    let env = Env::default();
    let soroban_str = EscrowContract::version(&env);
    let rust_str = soroban_str.to_string();
    // Re-wrap and compare.
    let rewrapped = crate::SorobanString::from_str(&env, &rust_str);
    assert_eq!(soroban_str, rewrapped);
}

// ===========================================================================
// init() tests
// ===========================================================================

#[test]
fn test_init_happy_path() {
    let e = default_escrow();
    assert_eq!(e.invoice_id, 42);
    assert_eq!(e.sme_address, "GABC123");
    assert_eq!(e.amount, 1_000_000);
    assert_eq!(e.yield_bps, 500);
    assert_eq!(e.maturity, 1_700_000_000);
    assert_eq!(e.funded_amount, 0);
    assert_eq!(e.status, EscrowStatus::Pending);
}

#[test]
fn test_init_minimum_amount() {
    let e = EscrowContract::init(1, "GSME".to_string(), 1, 0, 0);
    assert_eq!(e.amount, 1);
}

#[test]
fn test_init_zero_yield_bps() {
    let e = EscrowContract::init(1, "GSME".to_string(), 100, 0, 0);
    assert_eq!(e.yield_bps, 0);
}

#[test]
fn test_init_max_yield_bps() {
    let e = EscrowContract::init(1, "GSME".to_string(), 100, 10_000, 0);
    assert_eq!(e.yield_bps, 10_000);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_init_zero_amount_panics() {
    EscrowContract::init(1, "GSME".to_string(), 0, 0, 0);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_init_negative_amount_panics() {
    EscrowContract::init(1, "GSME".to_string(), -1, 0, 0);
}

#[test]
#[should_panic(expected = "yield_bps must be <= 10000")]
fn test_init_yield_bps_overflow_panics() {
    EscrowContract::init(1, "GSME".to_string(), 100, 10_001, 0);
}

// ===========================================================================
// get_escrow() tests
// ===========================================================================

#[test]
fn test_get_escrow_returns_correct_state() {
    let e = default_escrow();
    let read = EscrowContract::get_escrow(&e);
    assert_eq!(read.invoice_id, e.invoice_id);
    assert_eq!(read.amount, e.amount);
    assert_eq!(read.status, EscrowStatus::Pending);
}

// ===========================================================================
// fund() tests
// ===========================================================================

#[test]
fn test_fund_partial_stays_pending() {
    let mut e = default_escrow();
    EscrowContract::fund(&mut e, 500_000);
    assert_eq!(e.funded_amount, 500_000);
    assert_eq!(e.status, EscrowStatus::Pending);
}

#[test]
fn test_fund_exact_becomes_funded() {
    let mut e = default_escrow();
    EscrowContract::fund(&mut e, 1_000_000);
    assert_eq!(e.funded_amount, 1_000_000);
    assert_eq!(e.status, EscrowStatus::Funded);
}

#[test]
fn test_fund_over_amount_becomes_funded() {
    let mut e = default_escrow();
    EscrowContract::fund(&mut e, 1_500_000);
    assert_eq!(e.funded_amount, 1_500_000);
    assert_eq!(e.status, EscrowStatus::Funded);
}

#[test]
fn test_fund_multiple_tranches() {
    let mut e = default_escrow();
    EscrowContract::fund(&mut e, 300_000);
    EscrowContract::fund(&mut e, 300_000);
    EscrowContract::fund(&mut e, 400_000);
    assert_eq!(e.funded_amount, 1_000_000);
    assert_eq!(e.status, EscrowStatus::Funded);
}

#[test]
#[should_panic(expected = "fund_amount must be positive")]
fn test_fund_zero_panics() {
    let mut e = default_escrow();
    EscrowContract::fund(&mut e, 0);
}

#[test]
#[should_panic(expected = "fund_amount must be positive")]
fn test_fund_negative_panics() {
    let mut e = default_escrow();
    EscrowContract::fund(&mut e, -1);
}

#[test]
#[should_panic(expected = "cannot fund a settled escrow")]
fn test_fund_settled_escrow_panics() {
    let mut e = default_escrow();
    EscrowContract::fund(&mut e, 1_000_000);
    EscrowContract::settle(&mut e);
    EscrowContract::fund(&mut e, 1); // must panic
}

// ===========================================================================
// settle() tests
// ===========================================================================

#[test]
fn test_settle_happy_path() {
    let mut e = default_escrow();
    EscrowContract::fund(&mut e, 1_000_000);
    EscrowContract::settle(&mut e);
    assert_eq!(e.status, EscrowStatus::Settled);
}

#[test]
#[should_panic(expected = "escrow must be funded before settlement")]
fn test_settle_pending_escrow_panics() {
    let mut e = default_escrow();
    EscrowContract::settle(&mut e);
}

#[test]
#[should_panic(expected = "escrow must be funded before settlement")]
fn test_settle_already_settled_panics() {
    let mut e = default_escrow();
    EscrowContract::fund(&mut e, 1_000_000);
    EscrowContract::settle(&mut e);
    EscrowContract::settle(&mut e); // double-settle must panic
}

// ===========================================================================
// Full lifecycle integration test
// ===========================================================================

/// End-to-end: init → version check → partial fund → full fund → settle.
#[test]
fn test_full_lifecycle_with_version_check() {
    let env = Env::default();

    // Tooling: verify contract version before interacting.
    let v = EscrowContract::version(&env).to_string();
    assert_eq!(v, "1.0.0", "unexpected contract version for this lifecycle");

    // Init.
    let mut escrow = EscrowContract::init(
        99,
        "GSME_FULL_LIFECYCLE".to_string(),
        2_000_000,
        300,
        1_800_000_000,
    );
    assert_eq!(escrow.status, EscrowStatus::Pending);

    // Partial funding.
    EscrowContract::fund(&mut escrow, 999_999);
    assert_eq!(escrow.status, EscrowStatus::Pending);

    // Final tranche hits the target.
    EscrowContract::fund(&mut escrow, 1_000_001);
    assert_eq!(escrow.status, EscrowStatus::Funded);
    assert_eq!(escrow.funded_amount, 1_999_999 + 1); // 2_000_000

    // Settlement.
    EscrowContract::settle(&mut escrow);
    assert_eq!(escrow.status, EscrowStatus::Settled);

    // State is preserved post-settlement.
    let read = EscrowContract::get_escrow(&escrow);
    assert_eq!(read.invoice_id, 99);
    assert_eq!(read.yield_bps, 300);
}