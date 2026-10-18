//! Focused boundary tests for the validation logic in `escrow/src/lib.rs`.
//!
//! Each test targets one validator and exercises exactly the acceptance boundary,
//! the rejection boundary one unit beyond it, and representative invalid inputs.
//! The goal is a regression lock: if a future change silently widens or narrows a
//! validated range, a test here catches it before deployment.
//!
//! # Covered validators
//!
//! | Validator | Boundary tested |
//! |-----------|----------------|
//! | `amount` | `1` (min), `MAX_INVOICE_AMOUNT` (max), `0` and `MAX+1` (rejected) |
//! | `yield_bps` | `0`, `10_000` (valid), `-1`, `10_001` (rejected) |
//! | `protocol_fee_bps` | `0`, `10_000` (valid), `10_001` (rejected) |
//! | `validate_invoice_id_string` | length 1, length 32 (valid), length 33 (rejected), charset |
//! | `validate_maturity_bounds` | zero (no lock), exact horizon (valid), horizon+1 (rejected), past (rejected) |
//! | `validate_decimal_scale` | scale 0 (skip), scale 6 exact (valid), scale 6 inexact (rejected), overflow scale |
//! | `validate_yield_tiers_table` | empty, single valid, decreasing lock, decreasing yield, below base |
//! | `funding_deadline` at init | deadline > now (valid), deadline == now (rejected), deadline >= maturity (rejected) |
//! | `fund` amount guards | positive (valid), zero (rejected), negative (rejected) |
//! | `min_contribution` vs `amount` | exact-equal (valid), one-over (rejected) |

use super::{assert_contract_error, deploy, free_addresses, setup};
use crate::{EscrowError, YieldTier, MAX_INVOICE_AMOUNT};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, Vec as SorobanVec,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Call `init` with the minimal required fields, overriding only what the test needs.
/// Returns the client so callers can make further assertions.
fn init_with(
    env: &Env,
    invoice_id: &str,
    amount: i128,
    yield_bps: i64,
    maturity: u64,
    protocol_fee_bps: Option<i64>,
    token_decimals: Option<u32>,
    min_contribution: Option<i128>,
    maturity_max_horizon: Option<u64>,
    funding_deadline: Option<u64>,
    yield_tiers: Option<SorobanVec<YieldTier>>,
) -> Result<
    Result<crate::InvoiceEscrow, crate::EscrowError>,
    Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
> {
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let (token, treasury) = free_addresses(env);
    client.try_init(
        &admin,
        &soroban_sdk::String::from_str(env, invoice_id),
        &sme,
        &amount,
        &yield_bps,
        &maturity,
        &token,
        &None,
        &treasury,
        &yield_tiers,
        &min_contribution,
        &None,
        &None,
        &None,
        &maturity_max_horizon,
        &funding_deadline,
        &None,
        &protocol_fee_bps,
        &token_decimals,
    )
}

// ---------------------------------------------------------------------------
// 1. Amount boundaries
// ---------------------------------------------------------------------------

#[test]
fn amount_zero_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    assert_contract_error(
        init_with(&env, "AMT0", 0, 500, 0, None, None, None, None, None, None),
        EscrowError::AmountMustBePositive,
    );
}

#[test]
fn amount_one_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    assert!(
        init_with(&env, "AMT1", 1, 0, 0, None, None, None, None, None, None).is_ok(),
        "amount = 1 must be accepted"
    );
}

#[test]
fn amount_max_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    assert!(
        init_with(
            &env,
            "AMTMX",
            MAX_INVOICE_AMOUNT,
            0,
            0,
            None,
            None,
            None,
            None,
            None,
            None
        )
        .is_ok(),
        "MAX_INVOICE_AMOUNT must be accepted"
    );
}

#[test]
fn amount_max_plus_one_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    assert_contract_error(
        init_with(
            &env,
            "AMTOVR",
            MAX_INVOICE_AMOUNT + 1,
            0,
            0,
            None,
            None,
            None,
            None,
            None,
            None,
        ),
        EscrowError::AmountExceedsMax,
    );
}

#[test]
fn amount_negative_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    assert_contract_error(
        init_with(&env, "AMTNEG", -1, 500, 0, None, None, None, None, None, None),
        EscrowError::AmountMustBePositive,
    );
}

// ---------------------------------------------------------------------------
// 2. yield_bps boundaries
// ---------------------------------------------------------------------------

#[test]
fn yield_bps_zero_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    assert!(
        init_with(&env, "YLD0", 1000, 0, 0, None, None, None, None, None, None).is_ok()
    );
}

#[test]
fn yield_bps_ten_thousand_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    assert!(
        init_with(&env, "YLD1", 1000, 10_000, 0, None, None, None, None, None, None).is_ok()
    );
}

#[test]
fn yield_bps_ten_thousand_one_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    assert_contract_error(
        init_with(&env, "YLDOVR", 1000, 10_001, 0, None, None, None, None, None, None),
        EscrowError::YieldBpsOutOfRange,
    );
}

#[test]
fn yield_bps_negative_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    assert_contract_error(
        init_with(&env, "YLDNEG", 1000, -1, 0, None, None, None, None, None, None),
        EscrowError::YieldBpsOutOfRange,
    );
}

// ---------------------------------------------------------------------------
// 3. protocol_fee_bps boundaries
// ---------------------------------------------------------------------------

#[test]
fn protocol_fee_bps_zero_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    assert!(
        init_with(&env, "FEE0", 1000, 500, 0, Some(0), None, None, None, None, None).is_ok()
    );
}

#[test]
fn protocol_fee_bps_ten_thousand_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    assert!(
        init_with(
            &env,
            "FEE1",
            1000,
            500,
            0,
            Some(10_000),
            None,
            None,
            None,
            None,
            None
        )
        .is_ok()
    );
}

#[test]
fn protocol_fee_bps_ten_thousand_one_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    assert_contract_error(
        init_with(
            &env,
            "FEEOVR",
            1000,
            500,
            0,
            Some(10_001),
            None,
            None,
            None,
            None,
            None,
        ),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );
}

#[test]
fn protocol_fee_bps_negative_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    assert_contract_error(
        init_with(
            &env,
            "FEENEG",
            1000,
            500,
            0,
            Some(-1),
            None,
            None,
            None,
            None,
            None,
        ),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );
}

// ---------------------------------------------------------------------------
// 4. invoice_id charset / length boundaries
// ---------------------------------------------------------------------------

#[test]
fn invoice_id_length_one_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    assert!(
        init_with(&env, "A", 1000, 500, 0, None, None, None, None, None, None).is_ok(),
        "single-char invoice_id must be accepted"
    );
}

#[test]
fn invoice_id_length_32_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    // Exactly 32 characters — the documented maximum.
    let id32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345";
    assert_eq!(id32.len(), 32);
    assert!(
        init_with(&env, id32, 1000, 500, 0, None, None, None, None, None, None).is_ok(),
        "32-char invoice_id must be accepted"
    );
}

#[test]
fn invoice_id_length_33_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let id33 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456";
    assert_eq!(id33.len(), 33);
    assert_contract_error(
        init_with(&env, id33, 1000, 500, 0, None, None, None, None, None, None),
        EscrowError::InvoiceIdInvalidLength,
    );
}

#[test]
fn invoice_id_empty_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    assert_contract_error(
        init_with(&env, "", 1000, 500, 0, None, None, None, None, None, None),
        EscrowError::InvoiceIdInvalidLength,
    );
}

#[test]
fn invoice_id_underscore_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    // Underscore is the only non-alphanumeric character allowed.
    assert!(
        init_with(&env, "INV_001", 1000, 500, 0, None, None, None, None, None, None).is_ok()
    );
}

#[test]
fn invoice_id_hyphen_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    assert_contract_error(
        init_with(&env, "INV-001", 1000, 500, 0, None, None, None, None, None, None),
        EscrowError::InvoiceIdInvalidCharset,
    );
}

#[test]
fn invoice_id_space_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    assert_contract_error(
        init_with(&env, "INV 001", 1000, 500, 0, None, None, None, None, None, None),
        EscrowError::InvoiceIdInvalidCharset,
    );
}

#[test]
fn invoice_id_dot_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    assert_contract_error(
        init_with(&env, "INV.001", 1000, 500, 0, None, None, None, None, None, None),
        EscrowError::InvoiceIdInvalidCharset,
    );
}

// ---------------------------------------------------------------------------
// 5. maturity / validate_maturity_bounds
// ---------------------------------------------------------------------------

#[test]
fn maturity_zero_accepted_no_lock() {
    // maturity == 0 means no maturity lock; the validator must skip all checks.
    let env = Env::default();
    env.mock_all_auths();
    assert!(
        init_with(&env, "MAT0", 1000, 500, 0, None, None, None, None, None, None).is_ok()
    );
}

#[test]
fn maturity_in_future_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    // Set ledger timestamp to 1000 so maturity 2000 is clearly in the future.
    let mut info = env.ledger().get();
    info.timestamp = 1_000;
    env.ledger().set(info);
    assert!(
        init_with(&env, "MATFUT", 1000, 500, 2_000, None, None, None, None, None, None).is_ok()
    );
}

#[test]
fn maturity_in_past_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    // Ledger at 5000; maturity of 4000 is in the past.
    let mut info = env.ledger().get();
    info.timestamp = 5_000;
    env.ledger().set(info);
    assert_contract_error(
        init_with(&env, "MATPST", 1000, 500, 4_000, None, None, None, None, None, None),
        EscrowError::MaturityInPast,
    );
}

#[test]
fn maturity_equal_to_now_rejected() {
    // maturity must be strictly >= now; equal is rejected as "in the past".
    let env = Env::default();
    env.mock_all_auths();
    let mut info = env.ledger().get();
    info.timestamp = 3_000;
    env.ledger().set(info);
    assert_contract_error(
        init_with(&env, "MATEQ", 1000, 500, 3_000, None, None, None, None, None, None),
        EscrowError::MaturityInPast,
    );
}

#[test]
fn maturity_exactly_at_horizon_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let now = 1_000u64;
    let horizon = 10_000u64;
    let mut info = env.ledger().get();
    info.timestamp = now;
    env.ledger().set(info);
    // maturity = now + horizon is exactly the maximum allowed value.
    let maturity = now + horizon;
    assert!(
        init_with(
            &env,
            "MATHZ",
            1000,
            500,
            maturity,
            None,
            None,
            None,
            Some(horizon),
            None,
            None
        )
        .is_ok(),
        "maturity == now + horizon must be accepted"
    );
}

#[test]
fn maturity_one_over_horizon_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let now = 1_000u64;
    let horizon = 10_000u64;
    let mut info = env.ledger().get();
    info.timestamp = now;
    env.ledger().set(info);
    let maturity = now + horizon + 1;
    assert_contract_error(
        init_with(
            &env,
            "MATHZOV",
            1000,
            500,
            maturity,
            None,
            None,
            None,
            Some(horizon),
            None,
            None,
        ),
        EscrowError::MaturityExceedsMaxHorizon,
    );
}

// ---------------------------------------------------------------------------
// 6. validate_decimal_scale boundaries
// ---------------------------------------------------------------------------

#[test]
fn token_decimals_zero_skips_scale_check() {
    // scale = 0 means every integer is valid — the validator should early-return.
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);
    // Init with scale 0, then fund with any amount.
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "DEC0"),
        &sme,
        &1_000_000i128,
        &500i64,
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
        &None::<i64>,
        &Some(0u32),
    );
    // Any amount is valid at scale 0.
    let inv = Address::generate(&env);
    client.fund(&inv, &777i128);
    assert_eq!(client.get_contribution(&inv), 777i128);
}

#[test]
fn token_decimals_six_exact_amount_accepted() {
    // At scale 6 the divisor is 1_000_000; amount must be a multiple.
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "DEC6OK"),
        &sme,
        &10_000_000i128,
        &500i64,
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
        &None::<i64>,
        &Some(6u32),
    );
    let inv = Address::generate(&env);
    // 2_000_000 is exactly divisible by 10^6.
    client.fund(&inv, &2_000_000i128);
    assert_eq!(client.get_contribution(&inv), 2_000_000i128);
}

#[test]
fn token_decimals_six_inexact_amount_rejected() {
    // At scale 6, an amount with a fractional tail must be rejected.
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "DEC6BAD"),
        &sme,
        &10_000_000i128,
        &500i64,
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
        &None::<i64>,
        &Some(6u32),
    );
    let inv = Address::generate(&env);
    // 1_500_001 has a fractional tail at 6 decimals.
    assert_contract_error(
        client.try_fund(&inv, &1_500_001i128),
        EscrowError::FundingTokenScaleInvalid,
    );
}

#[test]
fn token_decimals_overflow_scale_rejected_at_fund_time() {
    // A decimals value so large that 10^decimals overflows i128 must be rejected.
    // 10^39 overflows i128::MAX (~1.7e38), so decimals=39 should fail.
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "DECOVR"),
        &sme,
        &MAX_INVOICE_AMOUNT,
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
        &None::<i64>,
        &Some(39u32), // 10^39 overflows i128
    );
    let inv = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&inv, &1_000_000_000i128),
        EscrowError::FundingTokenScaleInvalid,
    );
}

// ---------------------------------------------------------------------------
// 7. validate_yield_tiers_table boundaries
// ---------------------------------------------------------------------------

#[test]
fn yield_tiers_empty_vec_accepted() {
    // An explicitly provided empty tier vec must be treated identically to None.
    let env = Env::default();
    env.mock_all_auths();
    let tiers: SorobanVec<YieldTier> = SorobanVec::new(&env);
    assert!(
        init_with(
            &env,
            "TIER0",
            1000,
            500,
            0,
            None,
            None,
            None,
            None,
            None,
            Some(tiers)
        )
        .is_ok()
    );
}

#[test]
fn yield_tiers_single_valid_tier_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 900, // > base 800
    });
    assert!(
        init_with(
            &env,
            "TIER1",
            1000,
            800,
            0,
            None,
            None,
            None,
            None,
            None,
            Some(tiers)
        )
        .is_ok()
    );
}

#[test]
fn yield_tiers_non_increasing_lock_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier { min_lock_secs: 200, yield_bps: 900 });
    tiers.push_back(YieldTier { min_lock_secs: 100, yield_bps: 1000 }); // lock goes down
    assert_contract_error(
        init_with(
            &env,
            "TIERLK",
            1000,
            800,
            0,
            None,
            None,
            None,
            None,
            None,
            Some(tiers),
        ),
        EscrowError::TierLockNotIncreasing,
    );
}

#[test]
fn yield_tiers_equal_lock_rejected() {
    // Locks must be strictly increasing, so equal is also rejected.
    let env = Env::default();
    env.mock_all_auths();
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier { min_lock_secs: 100, yield_bps: 900 });
    tiers.push_back(YieldTier { min_lock_secs: 100, yield_bps: 1000 }); // equal lock
    assert_contract_error(
        init_with(
            &env,
            "TIEREQ",
            1000,
            800,
            0,
            None,
            None,
            None,
            None,
            None,
            Some(tiers),
        ),
        EscrowError::TierLockNotIncreasing,
    );
}

#[test]
fn yield_tiers_decreasing_yield_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier { min_lock_secs: 100, yield_bps: 1000 });
    tiers.push_back(YieldTier { min_lock_secs: 200, yield_bps: 900 }); // yield goes down
    assert_contract_error(
        init_with(
            &env,
            "TIERYD",
            1000,
            800,
            0,
            None,
            None,
            None,
            None,
            None,
            Some(tiers),
        ),
        EscrowError::TierYieldNotNonDecreasing,
    );
}

#[test]
fn yield_tiers_below_base_yield_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let mut tiers = SorobanVec::new(&env);
    // Tier yield 700 < base yield 800.
    tiers.push_back(YieldTier { min_lock_secs: 100, yield_bps: 700 });
    assert_contract_error(
        init_with(
            &env,
            "TIERBS",
            1000,
            800,
            0,
            None,
            None,
            None,
            None,
            None,
            Some(tiers),
        ),
        EscrowError::TierYieldBelowBase,
    );
}

#[test]
fn yield_tiers_out_of_range_bps_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier { min_lock_secs: 100, yield_bps: 10_001 });
    assert_contract_error(
        init_with(
            &env,
            "TIERRNG",
            1000,
            800,
            0,
            None,
            None,
            None,
            None,
            None,
            Some(tiers),
        ),
        EscrowError::TierYieldOutOfRange,
    );
}

// ---------------------------------------------------------------------------
// 8. funding_deadline at init
// ---------------------------------------------------------------------------

#[test]
fn funding_deadline_in_future_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let mut info = env.ledger().get();
    info.timestamp = 1_000;
    env.ledger().set(info);
    // deadline strictly after now and before maturity.
    assert!(
        init_with(
            &env,
            "DLOK",
            1000,
            500,
            5_000, // maturity
            None,
            None,
            None,
            None,
            Some(3_000), // deadline 1000 < 3000 < 5000
            None,
        )
        .is_ok()
    );
}

#[test]
fn funding_deadline_equal_to_now_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let mut info = env.ledger().get();
    info.timestamp = 2_000;
    env.ledger().set(info);
    // deadline == now means it has already passed.
    assert_contract_error(
        init_with(
            &env,
            "DLEQ",
            1000,
            500,
            5_000,
            None,
            None,
            None,
            None,
            Some(2_000), // == now
            None,
        ),
        EscrowError::FundingDeadlinePassed,
    );
}

#[test]
fn funding_deadline_in_past_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let mut info = env.ledger().get();
    info.timestamp = 3_000;
    env.ledger().set(info);
    assert_contract_error(
        init_with(
            &env,
            "DLPST",
            1000,
            500,
            5_000,
            None,
            None,
            None,
            None,
            Some(2_000), // < now
            None,
        ),
        EscrowError::FundingDeadlinePassed,
    );
}

#[test]
fn funding_deadline_at_maturity_rejected() {
    // Deadline must be strictly before maturity when maturity > 0.
    let env = Env::default();
    env.mock_all_auths();
    let mut info = env.ledger().get();
    info.timestamp = 1_000;
    env.ledger().set(info);
    assert_contract_error(
        init_with(
            &env,
            "DLEQMAT",
            1000,
            500,
            5_000,
            None,
            None,
            None,
            None,
            Some(5_000), // == maturity
            None,
        ),
        EscrowError::FundingDeadlineBeyondMaturity,
    );
}

#[test]
fn funding_deadline_after_maturity_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let mut info = env.ledger().get();
    info.timestamp = 1_000;
    env.ledger().set(info);
    assert_contract_error(
        init_with(
            &env,
            "DLPOST",
            1000,
            500,
            5_000,
            None,
            None,
            None,
            None,
            Some(6_000), // > maturity
            None,
        ),
        EscrowError::FundingDeadlineBeyondMaturity,
    );
}

// ---------------------------------------------------------------------------
// 9. fund() amount guards
// ---------------------------------------------------------------------------

#[test]
fn fund_positive_amount_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FUNDOK"),
        &sme,
        &100_000i128,
        &500i64,
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
        &None::<i64>,
        &None::<u32>,
    );
    let inv = Address::generate(&env);
    client.fund(&inv, &1_000i128);
    assert_eq!(client.get_contribution(&inv), 1_000i128);
}

#[test]
fn fund_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FUND0"),
        &sme,
        &100_000i128,
        &500i64,
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
        &None::<i64>,
        &None::<u32>,
    );
    let inv = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&inv, &0i128),
        EscrowError::FundingAmountNotPositive,
    );
}

#[test]
fn fund_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FUNDNEG"),
        &sme,
        &100_000i128,
        &500i64,
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
        &None::<i64>,
        &None::<u32>,
    );
    let inv = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&inv, &-1i128),
        EscrowError::FundingAmountNotPositive,
    );
}

// ---------------------------------------------------------------------------
// 10. min_contribution vs amount boundary
// ---------------------------------------------------------------------------

#[test]
fn min_contribution_equal_to_amount_accepted() {
    // min_contribution == amount is the tightest valid configuration:
    // only a single full-amount deposit is possible.
    let env = Env::default();
    env.mock_all_auths();
    assert!(
        init_with(
            &env,
            "MCEQ",
            1000,
            500,
            0,
            None,
            None,
            Some(1000), // min == amount
            None,
            None,
            None,
        )
        .is_ok(),
        "min_contribution == amount must be accepted"
    );
}

#[test]
fn min_contribution_one_over_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    assert_contract_error(
        init_with(
            &env,
            "MCOVR",
            1000,
            500,
            0,
            None,
            None,
            Some(1001), // min > amount
            None,
            None,
            None,
        ),
        EscrowError::MinContributionExceedsAmount,
    );
}

// ---------------------------------------------------------------------------
// 11. Duplicate init is idempotent-safe (no partial-write on second call)
// ---------------------------------------------------------------------------

#[test]
fn second_init_leaves_state_unchanged() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "DUPE1"),
        &sme,
        &1_000i128,
        &500i64,
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
        &None::<i64>,
        &None::<u32>,
    );
    let state_before = client.get_escrow();
    let other_admin = Address::generate(&env);
    assert_contract_error(
        client.try_init(
            &other_admin,
            &soroban_sdk::String::from_str(&env, "DUPE1"),
            &sme,
            &9_999i128, // different amount
            &999i64,
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
            &None::<i64>,
            &None::<u32>,
        ),
        EscrowError::EscrowAlreadyInitialized,
    );
    // State must be byte-identical to pre-call snapshot.
    assert_eq!(
        client.get_escrow(),
        state_before,
        "duplicate init must not mutate any stored state"
    );
}
