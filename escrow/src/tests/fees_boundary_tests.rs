//! Boundary and fuzz-style tests for the `protocol_fee_bps` subsystem.
//!
//! # Coverage map
//!
//! | Section | What is tested |
//! |---------|----------------|
//! | A — `init` validation | Boundary rejection at -1, 10_001, `i64::MIN`, `i64::MAX`; acceptance at 0, 1, 10_000; `None` default |
//! | B — `get_protocol_fee_bps` | Returns 0 before init; reflects every init-time value |
//! | C — `get_settlement_config` consistency | `protocol_fee_bps` field agrees with `get_protocol_fee_bps` |
//! | D — `withdraw` rounding | Floor division at 1 bps, 9_999 bps; single-unit principal edge cases |
//! | E — conservation invariant sweep | `net + fee == funded_amount` over a fixed set of (amount, bps) pairs |
//! | F — 100% fee path | All principal routes to treasury; SME receives zero; no token error |
//!
//! # Unguarded boundaries found
//!
//! See the bottom of this file for the `UNGUARDED_BOUNDARIES` note.

use crate::tests::assert_contract_error;
use crate::{EscrowError, LiquifactEscrow, LiquifactEscrowClient, MAX_INVOICE_AMOUNT};
use soroban_sdk::{
    testutils::Address as _,
    token::{StellarAssetClient, TokenClient},
    Address, Env, String,
};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Deploy a fresh contract with no init — used for pre-init getter tests.
fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

/// Call `init` with a dummy (non-SAC) token and return only the client.
/// The escrow is intentionally **not** funded; suitable for fee validation
/// and getter tests that never reach `withdraw`.
fn init_with_fee(env: &Env, invoice_id: &str, fee_bps: Option<i64>) -> LiquifactEscrowClient<'_> {
    env.mock_all_auths();
    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &10_000i128,
        &0i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &fee_bps,
    );
    client
}

/// Try `init` with an out-of-range `fee_bps` and expect `ProtocolFeeBpsOutOfRange` (215).
fn assert_init_fee_rejected(env: &Env, invoice_id: &str, fee_bps: i64) {
    env.mock_all_auths();
    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(env, invoice_id),
            &sme,
            &10_000i128,
            &0i64,
            &0u64,
            &token,
            &None,
            &treasury,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &Some(fee_bps),
        ),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );
}

/// Full end-to-end helper: init with a real SAC, fund to target, settle, then
/// return `(client, escrow_id, sme_addr, token_client, treasury_addr)` ready for
/// `withdraw()`.
fn setup_for_withdraw(
    env: &Env,
    invoice_id: &str,
    principal: i128,
    fee_bps: i64,
) -> (
    LiquifactEscrowClient<'_>,
    Address,
    Address,
    TokenClient<'_>,
    Address,
) {
    env.mock_all_auths();
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(env, &token_id);
    let token = TokenClient::new(env, &token_id);

    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &escrow_id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &principal,
        &0i64,
        &0u64,
        &token_id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &Some(fee_bps),
    );

    // Fund the escrow to exactly `principal`.
    let investor = Address::generate(env);
    sac_admin.mint(&investor, &principal);
    client.fund(&investor, &principal);

    // Settle (required before withdraw).
    client.settle();

    // Mint `principal` tokens directly into the escrow so `withdraw` can
    // transfer them (mirrors the SAC coupon-headroom pattern used elsewhere).
    sac_admin.mint(&escrow_id, &principal);

    (client, escrow_id, sme, token, treasury)
}

// ═══════════════════════════════════════════════════════════════════════════
// Section A — `init` validation: rejection and acceptance at boundaries
// ═══════════════════════════════════════════════════════════════════════════

/// Exactly `0` bps is the minimum valid fee; `init` must accept it.
#[test]
fn init_fee_bps_zero_is_accepted() {
    let env = Env::default();
    let client = init_with_fee(&env, "FEE_A01", Some(0));
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

/// Exactly `10_000` bps (100 %) is the maximum valid fee; `init` must accept it.
#[test]
fn init_fee_bps_max_10000_is_accepted() {
    let env = Env::default();
    let client = init_with_fee(&env, "FEE_A02", Some(10_000));
    assert_eq!(client.get_protocol_fee_bps(), 10_000);
}

/// `1` bps is one above the minimum; must be stored and returned correctly.
#[test]
fn init_fee_bps_one_is_accepted() {
    let env = Env::default();
    let client = init_with_fee(&env, "FEE_A03", Some(1));
    assert_eq!(client.get_protocol_fee_bps(), 1);
}

/// `9_999` bps is one below the maximum; must be stored and returned correctly.
#[test]
fn init_fee_bps_9999_is_accepted() {
    let env = Env::default();
    let client = init_with_fee(&env, "FEE_A04", Some(9_999));
    assert_eq!(client.get_protocol_fee_bps(), 9_999);
}

/// `None` defaults to `0`; `get_protocol_fee_bps` must return `0`.
#[test]
fn init_fee_bps_none_defaults_to_zero() {
    let env = Env::default();
    let client = init_with_fee(&env, "FEE_A05", None);
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

/// `-1` is one below the minimum valid value; must be rejected with
/// `ProtocolFeeBpsOutOfRange` (215).
#[test]
fn init_fee_bps_minus_one_rejected() {
    let env = Env::default();
    assert_init_fee_rejected(&env, "FEE_A06", -1);
}

/// `10_001` is one above the maximum valid value; must be rejected with
/// `ProtocolFeeBpsOutOfRange` (215).
#[test]
fn init_fee_bps_10001_rejected() {
    let env = Env::default();
    assert_init_fee_rejected(&env, "FEE_A07", 10_001);
}

/// `i64::MIN` is the most-negative possible value; must be rejected with
/// `ProtocolFeeBpsOutOfRange` (215).
#[test]
fn init_fee_bps_i64_min_rejected() {
    let env = Env::default();
    assert_init_fee_rejected(&env, "FEE_A08", i64::MIN);
}

/// `i64::MAX` is the largest possible value; must be rejected with
/// `ProtocolFeeBpsOutOfRange` (215).
#[test]
fn init_fee_bps_i64_max_rejected() {
    let env = Env::default();
    assert_init_fee_rejected(&env, "FEE_A09", i64::MAX);
}

/// A large but sub-max value (`i64::MAX - 1`) is still out of range and must
/// be rejected with `ProtocolFeeBpsOutOfRange` (215).
#[test]
fn init_fee_bps_i64_max_minus_one_rejected() {
    let env = Env::default();
    assert_init_fee_rejected(&env, "FEE_A10", i64::MAX - 1);
}

/// Rejection must be atomic: after a failed `init`, the contract must remain
/// uninitialized so the same instance can be initialized again.
#[test]
fn init_fee_bps_rejection_leaves_contract_uninitialized() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);

    // First call with an invalid fee — must fail.
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "FEE_A11"),
            &sme,
            &10_000i128,
            &0i64,
            &0u64,
            &token,
            &None,
            &treasury,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &Some(-1i64),
        ),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );

    // The contract must still be uninitialized — `get_protocol_fee_bps` returns the
    // additive-key default of `0` (not an error), confirming no partial write occurred.
    assert_eq!(
        client.get_protocol_fee_bps(),
        0,
        "fee must not have been partially written after a rejected init"
    );

    // A subsequent valid init on the same instance must succeed.
    client.init(
        &admin,
        &String::from_str(&env, "FEE_A11"),
        &sme,
        &10_000i128,
        &0i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &Some(250i64),
    );
    assert_eq!(client.get_protocol_fee_bps(), 250);
}

// ═══════════════════════════════════════════════════════════════════════════
// Section B — `get_protocol_fee_bps`: pre-init default and post-init values
// ═══════════════════════════════════════════════════════════════════════════

/// Before `init` is called `get_protocol_fee_bps` must return the additive-key
/// default of `0` (ADR-007: absent key ⇒ `unwrap_or(0)`).
#[test]
fn get_protocol_fee_bps_returns_zero_before_init() {
    let env = Env::default();
    let client = deploy(&env);
    assert_eq!(
        client.get_protocol_fee_bps(),
        0,
        "getter must return 0 before init (additive-key default)"
    );
}

/// After `init` with `None`, `get_protocol_fee_bps` must return `0`.
#[test]
fn get_protocol_fee_bps_returns_zero_for_none_init() {
    let env = Env::default();
    let client = init_with_fee(&env, "FEE_B02", None);
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

/// After `init` with `Some(0)`, `get_protocol_fee_bps` must return `0`.
#[test]
fn get_protocol_fee_bps_returns_zero_for_explicit_zero() {
    let env = Env::default();
    let client = init_with_fee(&env, "FEE_B03", Some(0));
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

/// After `init` with `Some(10_000)`, `get_protocol_fee_bps` must return `10_000`.
#[test]
fn get_protocol_fee_bps_returns_max_after_init() {
    let env = Env::default();
    let client = init_with_fee(&env, "FEE_B04", Some(10_000));
    assert_eq!(client.get_protocol_fee_bps(), 10_000);
}

/// After `init` with an arbitrary mid-range value, the getter reflects it exactly.
#[test]
fn get_protocol_fee_bps_reflects_arbitrary_mid_range_value() {
    let env = Env::default();
    let client = init_with_fee(&env, "FEE_B05", Some(333));
    assert_eq!(client.get_protocol_fee_bps(), 333);
}

/// Calling `get_protocol_fee_bps` multiple times returns the same value (no
/// side-effects or storage mutation from reading).
#[test]
fn get_protocol_fee_bps_is_idempotent() {
    let env = Env::default();
    let client = init_with_fee(&env, "FEE_B06", Some(750));
    let first = client.get_protocol_fee_bps();
    let second = client.get_protocol_fee_bps();
    assert_eq!(first, second, "repeated reads must return identical values");
    assert_eq!(first, 750);
}

// ═══════════════════════════════════════════════════════════════════════════
// Section C — `get_settlement_config` consistency
// ═══════════════════════════════════════════════════════════════════════════

/// `get_settlement_config().protocol_fee_bps` must equal `get_protocol_fee_bps()`
/// for a zero-fee escrow.
#[test]
fn settlement_config_protocol_fee_bps_matches_getter_at_zero() {
    let env = Env::default();
    let client = init_with_fee(&env, "FEE_C01", Some(0));
    let config = client.get_settlement_config();
    assert_eq!(
        config.protocol_fee_bps,
        client.get_protocol_fee_bps(),
        "settlement_config.protocol_fee_bps must match get_protocol_fee_bps for fee=0"
    );
}

/// `get_settlement_config().protocol_fee_bps` must equal `get_protocol_fee_bps()`
/// for the maximum fee (10_000).
#[test]
fn settlement_config_protocol_fee_bps_matches_getter_at_max() {
    let env = Env::default();
    let client = init_with_fee(&env, "FEE_C02", Some(10_000));
    let config = client.get_settlement_config();
    assert_eq!(
        config.protocol_fee_bps,
        client.get_protocol_fee_bps(),
        "settlement_config.protocol_fee_bps must match get_protocol_fee_bps for fee=10_000"
    );
}

/// `get_settlement_config().protocol_fee_bps` must equal `get_protocol_fee_bps()`
/// for an arbitrary mid-range fee.
#[test]
fn settlement_config_protocol_fee_bps_matches_getter_mid_range() {
    let env = Env::default();
    let client = init_with_fee(&env, "FEE_C03", Some(250));
    let config = client.get_settlement_config();
    assert_eq!(
        config.protocol_fee_bps,
        client.get_protocol_fee_bps(),
        "settlement_config.protocol_fee_bps must match get_protocol_fee_bps for fee=250"
    );
}

/// Before `init`, `get_settlement_config().protocol_fee_bps` must be `0`
/// (same default as `get_protocol_fee_bps`).
#[test]
fn settlement_config_protocol_fee_bps_is_zero_before_init() {
    let env = Env::default();
    let client = deploy(&env);
    let config = client.get_settlement_config();
    assert_eq!(
        config.protocol_fee_bps, 0,
        "settlement_config.protocol_fee_bps must be 0 before init"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Section D — `withdraw` rounding: floor-division edge cases
// ═══════════════════════════════════════════════════════════════════════════
//
// The fee formula is:  fee = floor(funded_amount * fee_bps / 10_000)
// Rounding always favours the SME — any sub-10_000th residue stays with them.

/// `1` bps on `10_000` base units produces exactly `1` fee unit.
/// Verifies the minimum non-zero fee resolves to a single token unit at the
/// smallest round principal.
#[test]
fn withdraw_fee_1_bps_on_10000_units_is_1() {
    let env = Env::default();
    let principal = 10_000i128;
    let fee_bps = 1i64;
    let (client, _id, sme, token, treasury) =
        setup_for_withdraw(&env, "FEE_D01", principal, fee_bps);

    client.withdraw();

    let expected_fee = principal * fee_bps as i128 / 10_000; // = 1
    assert_eq!(expected_fee, 1, "sanity: expected_fee must be 1");
    assert_eq!(
        token.balance(&treasury),
        expected_fee,
        "treasury must receive exactly 1 base unit"
    );
    assert_eq!(
        token.balance(&sme),
        principal - expected_fee,
        "SME must receive principal - 1"
    );
}

/// `9_999` bps on `10_000` base units produces `9_999` fee units, leaving `1`
/// for the SME.  The rounding residue (none here — exact) stays with the SME.
#[test]
fn withdraw_fee_9999_bps_on_10000_units_is_9999() {
    let env = Env::default();
    let principal = 10_000i128;
    let fee_bps = 9_999i64;
    let (client, _id, sme, token, treasury) =
        setup_for_withdraw(&env, "FEE_D02", principal, fee_bps);

    client.withdraw();

    let expected_fee = principal * fee_bps as i128 / 10_000; // = 9_999
    assert_eq!(expected_fee, 9_999, "sanity: expected_fee must be 9_999");
    assert_eq!(token.balance(&treasury), expected_fee);
    assert_eq!(token.balance(&sme), 1);
}

/// `1` bps on `1` base unit produces `0` fee (floor of 0.0001).
/// The entire unit goes to the SME; treasury receives nothing.
/// This is the most extreme floor-rounding scenario.
#[test]
fn withdraw_fee_1_bps_on_1_unit_floors_to_zero() {
    let env = Env::default();
    let principal = 1i128;
    let fee_bps = 1i64;
    let (client, _id, sme, token, treasury) =
        setup_for_withdraw(&env, "FEE_D03", principal, fee_bps);

    client.withdraw();

    // floor(1 * 1 / 10_000) = 0
    assert_eq!(
        token.balance(&treasury),
        0,
        "treasury must receive 0 when fee floors to zero"
    );
    assert_eq!(
        token.balance(&sme),
        1,
        "SME must receive full 1 unit when fee is zero"
    );
}

/// `9_999` bps on `1` base unit: floor(9_999/10_000) = 0.
/// Even at almost-100% fee, a single-unit principal entirely goes to the SME
/// because the fee truncates to zero.
#[test]
fn withdraw_fee_9999_bps_on_1_unit_floors_to_zero() {
    let env = Env::default();
    let principal = 1i128;
    let fee_bps = 9_999i64;
    let (client, _id, sme, token, treasury) =
        setup_for_withdraw(&env, "FEE_D04", principal, fee_bps);

    client.withdraw();

    // floor(1 * 9_999 / 10_000) = 0
    assert_eq!(token.balance(&treasury), 0);
    assert_eq!(token.balance(&sme), 1);
}

/// `333` bps on `1_000` base units: floor(1_000 * 333 / 10_000) = 33.
/// The 0.3 residue stays with the SME; treasury never over-charges.
/// Mirrors the worked example in `docs/fees-auth.md`.
#[test]
fn withdraw_fee_333_bps_on_1000_units_floors_to_33() {
    let env = Env::default();
    let principal = 1_000i128;
    let fee_bps = 333i64;
    let (client, _id, sme, token, treasury) =
        setup_for_withdraw(&env, "FEE_D05", principal, fee_bps);

    client.withdraw();

    // floor(1_000 * 333 / 10_000) = floor(33.3) = 33
    assert_eq!(token.balance(&treasury), 33);
    assert_eq!(token.balance(&sme), 967);
}

/// `1` bps on `9_999` base units: floor(9_999/10_000) = 0.
/// Boundary: principal just below 10_000, minimum bps → fee is still zero.
#[test]
fn withdraw_fee_1_bps_on_9999_units_floors_to_zero() {
    let env = Env::default();
    let principal = 9_999i128;
    let fee_bps = 1i64;
    let (client, _id, sme, token, treasury) =
        setup_for_withdraw(&env, "FEE_D06", principal, fee_bps);

    client.withdraw();

    // floor(9_999 * 1 / 10_000) = floor(0.9999) = 0
    assert_eq!(token.balance(&treasury), 0);
    assert_eq!(token.balance(&sme), 9_999);
}

/// `1` bps on `10_001` base units: floor(10_001/10_000) = 1.
/// This is the smallest principal that produces a non-zero fee at 1 bps.
#[test]
fn withdraw_fee_1_bps_on_10001_units_is_1() {
    let env = Env::default();
    let principal = 10_001i128;
    let fee_bps = 1i64;
    let (client, _id, sme, token, treasury) =
        setup_for_withdraw(&env, "FEE_D07", principal, fee_bps);

    client.withdraw();

    // floor(10_001 * 1 / 10_000) = floor(1.0001) = 1
    assert_eq!(token.balance(&treasury), 1);
    assert_eq!(token.balance(&sme), 10_000);
}

// ═══════════════════════════════════════════════════════════════════════════
// Section E — conservation invariant: `net + fee == funded_amount`
// ═══════════════════════════════════════════════════════════════════════════
//
// For every compliant escrow, `withdraw` must never create or destroy principal:
//   `sme_balance_delta + treasury_balance_delta == funded_amount`
//
// We verify this over a fixed table of (principal, fee_bps) pairs that cover
// the full range of interesting combinations without unbounded iteration.

/// Helper: assert the conservation invariant holds for one (principal, bps) pair.
fn assert_conservation(env: &Env, invoice_id: &str, principal: i128, fee_bps: i64) {
    let (client, _id, sme, token, treasury) =
        setup_for_withdraw(env, invoice_id, principal, fee_bps);

    let sme_before = token.balance(&sme);
    let treasury_before = token.balance(&treasury);

    client.withdraw();

    let sme_delta = token.balance(&sme) - sme_before;
    let treasury_delta = token.balance(&treasury) - treasury_before;

    assert_eq!(
        sme_delta + treasury_delta,
        principal,
        "conservation violated: net({sme_delta}) + fee({treasury_delta}) != principal({principal}) \
         for fee_bps={fee_bps}"
    );
}

/// Conservation sweep over representative (principal, bps) pairs.
///
/// Each pair has a unique invoice_id so they run in isolation.
#[test]
fn withdraw_conservation_invariant_at_zero_bps() {
    let env = Env::default();
    assert_conservation(&env, "FEE_E01", 1_000_000, 0);
}

#[test]
fn withdraw_conservation_invariant_at_1_bps() {
    let env = Env::default();
    assert_conservation(&env, "FEE_E02", 1_000_000, 1);
}

#[test]
fn withdraw_conservation_invariant_at_100_bps() {
    let env = Env::default();
    assert_conservation(&env, "FEE_E03", 1_000_000, 100);
}

#[test]
fn withdraw_conservation_invariant_at_250_bps() {
    let env = Env::default();
    assert_conservation(&env, "FEE_E04", 1_000_000, 250);
}

#[test]
fn withdraw_conservation_invariant_at_333_bps() {
    let env = Env::default();
    assert_conservation(&env, "FEE_E05", 1_000_000, 333);
}

#[test]
fn withdraw_conservation_invariant_at_5000_bps() {
    let env = Env::default();
    assert_conservation(&env, "FEE_E06", 1_000_000, 5_000);
}

#[test]
fn withdraw_conservation_invariant_at_9999_bps() {
    let env = Env::default();
    assert_conservation(&env, "FEE_E07", 1_000_000, 9_999);
}

#[test]
fn withdraw_conservation_invariant_at_max_10000_bps() {
    let env = Env::default();
    assert_conservation(&env, "FEE_E08", 1_000_000, 10_000);
}

/// Conservation with a non-round principal (odd number that resists exact splits).
#[test]
fn withdraw_conservation_invariant_odd_principal() {
    let env = Env::default();
    assert_conservation(&env, "FEE_E09", 999_999, 333);
}

/// Conservation with a single-unit principal at every interesting bps.
#[test]
fn withdraw_conservation_invariant_unit_principal_zero_bps() {
    let env = Env::default();
    assert_conservation(&env, "FEE_E10", 1, 0);
}

#[test]
fn withdraw_conservation_invariant_unit_principal_max_bps() {
    let env = Env::default();
    assert_conservation(&env, "FEE_E11", 1, 10_000);
}

/// Conservation at `MAX_INVOICE_AMOUNT` (2^63 - 1) with a small bps value.
/// Confirms checked arithmetic handles the largest valid principal.
#[test]
fn withdraw_conservation_invariant_max_invoice_amount_small_bps() {
    let env = Env::default();
    assert_conservation(&env, "FEE_E12", MAX_INVOICE_AMOUNT, 1);
}

/// Conservation at `MAX_INVOICE_AMOUNT` with 9_999 bps.
#[test]
fn withdraw_conservation_invariant_max_invoice_amount_near_max_bps() {
    let env = Env::default();
    assert_conservation(&env, "FEE_E13", MAX_INVOICE_AMOUNT, 9_999);
}

// ═══════════════════════════════════════════════════════════════════════════
// Section F — 100% fee path (10_000 bps): treasury gets all, SME gets zero
// ═══════════════════════════════════════════════════════════════════════════

/// At `fee_bps = 10_000`, the entire `funded_amount` routes to the treasury.
/// The SME net is exactly `0`. No token error must be raised for a zero-amount
/// SME transfer (the withdraw path skips zero-value transfers).
#[test]
fn withdraw_100_percent_fee_all_goes_to_treasury() {
    let env = Env::default();
    let principal = 1_000_000i128;
    let (client, _id, sme, token, treasury) =
        setup_for_withdraw(&env, "FEE_F01", principal, 10_000);

    let escrow = client.withdraw();

    assert_eq!(escrow.status, 3, "status must be 3 (withdrawn) after withdraw");
    assert_eq!(
        token.balance(&treasury),
        principal,
        "treasury must receive the full principal"
    );
    assert_eq!(
        token.balance(&sme),
        0,
        "SME must receive zero when fee_bps = 10_000"
    );
}

/// At `fee_bps = 10_000` with a single-unit principal, treasury gets `1` and
/// SME gets `0`.
#[test]
fn withdraw_100_percent_fee_unit_principal() {
    let env = Env::default();
    let (client, _id, sme, token, treasury) =
        setup_for_withdraw(&env, "FEE_F02", 1, 10_000);

    client.withdraw();

    assert_eq!(token.balance(&treasury), 1);
    assert_eq!(token.balance(&sme), 0);
}

/// At `fee_bps = 10_000` with `MAX_INVOICE_AMOUNT`, no arithmetic overflow
/// must occur and the full amount must be in the treasury.
#[test]
fn withdraw_100_percent_fee_max_invoice_amount_no_overflow() {
    let env = Env::default();
    let (client, _id, sme, token, treasury) =
        setup_for_withdraw(&env, "FEE_F03", MAX_INVOICE_AMOUNT, 10_000);

    let escrow = client.withdraw();

    assert_eq!(escrow.status, 3);
    assert_eq!(token.balance(&treasury), MAX_INVOICE_AMOUNT);
    assert_eq!(token.balance(&sme), 0);
}

/// At `fee_bps = 0`, the treasury receives nothing (zero-transfer skipped) and
/// the SME receives the full principal.
#[test]
fn withdraw_zero_fee_all_goes_to_sme_treasury_unchanged() {
    let env = Env::default();
    let principal = 1_000_000i128;
    let (client, _id, sme, token, treasury) =
        setup_for_withdraw(&env, "FEE_F04", principal, 0);

    let treasury_before = token.balance(&treasury);
    client.withdraw();

    assert_eq!(
        token.balance(&sme),
        principal,
        "SME must receive full principal when fee_bps = 0"
    );
    assert_eq!(
        token.balance(&treasury),
        treasury_before,
        "treasury balance must not change when fee_bps = 0"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// UNGUARDED_BOUNDARIES — findings for reviewer attention
// ═══════════════════════════════════════════════════════════════════════════
//
// After exhaustive boundary testing the following observations are recorded:
//
// 1. **No `set_protocol_fee_bps` entrypoint exists** (as of current `main`).
//    The `ProtocolFeeUpdated` event struct is defined and `docs/fees-errors.md`
//    describes a setter, but no corresponding `pub fn set_protocol_fee_bps`
//    appears in `lib.rs`. The fee is therefore truly immutable after `init`.
//    *Consequence:* there are no post-init setter boundaries to test; if the
//    setter is added later this test file must be extended.
//
// 2. **`withdraw` with `fee == 0` skips the treasury transfer call entirely.**
//    This is correct (zero-value transfers are a no-op / wasted gas), but it
//    means the SEP-41 balance-delta guards (codes 36–41) are never exercised
//    on the treasury leg when `fee_bps = 0`.  This is a documented gap, not a
//    bug, but an auditor should confirm the skip-transfer path is intentional.
//
// 3. **`net == 0` when `fee_bps = 10_000`** is likewise handled by skipping
//    the SME transfer.  The conservation invariant still holds; no typed error
//    is emitted for a zero net payout.  This is correct behaviour.
//
// 4. **`withdraw` with `funded_amount = 0`** is unreachable through the normal
//    API because `fund` requires a positive amount, so `funded_amount` is at
//    least 1 before the funded-state promotion occurs.  No boundary guard for
//    `funded_amount = 0` inside `withdraw` itself; the upstream invariant is
//    relied upon instead.
//
// 5. **`protocol_fee_bps` is stored as `i64` but validated to `0..=10_000`.**
//    Negative values and values above `10_000` are rejected at `init` with
//    error 215 (`ProtocolFeeBpsOutOfRange`). Values of `i64::MIN` and `i64::MAX`
//    are explicitly covered by tests `FEE_A08` and `FEE_A09` in Section A.
//
// No unguarded arithmetic paths or missing rejections were found.
