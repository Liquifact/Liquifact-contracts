// Tests for issue #1212: Validate token decimal scale before amount conversion.
//
// Edge cases required by the issue:
//   1. zero decimals         — every integer amount is representable; fund succeeds
//   2. too many fractional digits — amount has more precision than the scale allows; rejected
//   3. maximum amount        — MAX_INVOICE_AMOUNT is exactly representable at the configured scale
//   4. negative or blank amount — non-positive amounts are rejected before scale check
//   5. token scale changes after funding — second funder is validated against the original scale

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

use super::{deploy, install_stellar_asset_token};
use crate::{EscrowError, LiquifactEscrow, MAX_INVOICE_AMOUNT};

// ---------------------------------------------------------------------------
// Shared helper
// ---------------------------------------------------------------------------

/// Deploy a fresh escrow backed by a real SAC token and initialise it with
/// `token_decimals`.  Returns `(client, investor_address, sac_admin_client)`.
fn deploy_with_scale<'a>(
    env: &'a Env,
    token_decimals: u32,
    target: i128,
) -> (
    crate::LiquifactEscrowClient<'a>,
    Address,
    soroban_sdk::token::StellarAssetClient<'a>,
) {
    env.mock_all_auths();
    let sat = install_stellar_asset_token(env);
    let client = deploy(env);

    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "INV-DS"),
        &sme,
        &target,
        &800i64,
        &0u64,
        &sat.id,
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
        &Some(token_decimals),
    );

    let investor = Address::generate(env);
    (client, investor, sat.stellar)
}

// ---------------------------------------------------------------------------
// Edge case 1: zero decimals
// ---------------------------------------------------------------------------

/// With `token_decimals = 0` every positive integer amount is representable
/// (divisor = 10^0 = 1; remainder is always 0).  `fund` must succeed.
#[test]
fn decimal_scale_zero_decimals_any_integer_is_valid() {
    let env = Env::default();
    env.mock_all_auths();

    // target slightly above the fund amount so escrow stays open
    let (client, investor, sac) = deploy_with_scale(&env, 0, 1_000_001);
    sac.mint(&investor, &1_000_000);
    // Any integer — including odd amounts — is representable with 0 decimals
    client.fund(&investor, &1_000_000);

    let escrow = client.get_escrow();
    assert_eq!(escrow.funded_amount, 1_000_000);
}

/// A smaller odd amount also works with zero decimals.
#[test]
fn decimal_scale_zero_decimals_odd_amount_valid() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, investor, sac) = deploy_with_scale(&env, 0, 10_000_000);
    sac.mint(&investor, &777);
    client.fund(&investor, &777);

    assert_eq!(client.get_escrow().funded_amount, 777);
}

// ---------------------------------------------------------------------------
// Edge case 2: too many fractional digits
// ---------------------------------------------------------------------------

/// `token_decimals = 2` means the divisor is 100.  An amount like `150` (1.50)
/// is fine, but `151` (1.51) has a remainder of 1 — rejected.
#[test]
fn decimal_scale_too_many_fractional_digits_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, investor, sac) = deploy_with_scale(&env, 2, 10_000_000);
    sac.mint(&investor, &151);

    let result = client.try_fund(&investor, &151);
    assert!(
        result.is_err(),
        "amount with too many fractional digits should be rejected"
    );

    // Verify it fails with FundingTokenScaleInvalid specifically
    match result {
        Err(Ok(e)) => {
            assert_eq!(
                e,
                soroban_sdk::Error::from_contract_error(
                    EscrowError::FundingTokenScaleInvalid as u32
                )
            );
        }
        Err(Err(soroban_sdk::InvokeError::Contract(code))) => {
            assert_eq!(code, EscrowError::FundingTokenScaleInvalid as u32);
        }
        other => panic!("expected FundingTokenScaleInvalid, got {:?}", other),
    }
}

/// `token_decimals = 6` — amount `1_000_001` has 1 leftover unit (sub-micro);
/// `1_000_000` (exactly 1 token) is valid.
#[test]
fn decimal_scale_6_decimals_sub_unit_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, investor, sac) = deploy_with_scale(&env, 6, 100_000_000);
    sac.mint(&investor, &1_000_001);

    // 1_000_001 is not a multiple of 10^6 = 1_000_000
    let bad = client.try_fund(&investor, &1_000_001);
    assert!(bad.is_err());

    // 1_000_000 is an exact multiple — must succeed
    sac.mint(&investor, &1_000_000); // already has 1_000_001, mint more to have enough
    client.fund(&investor, &1_000_000);
    assert_eq!(client.get_escrow().funded_amount, 1_000_000);
}

/// Exact multiples of the divisor are always valid regardless of scale.
#[test]
fn decimal_scale_exact_multiple_valid() {
    let env = Env::default();
    env.mock_all_auths();

    // 2 decimals → divisor 100; amount 500 = 5.00 tokens → valid
    let (client, investor, sac) = deploy_with_scale(&env, 2, 10_000_000);
    sac.mint(&investor, &500);
    client.fund(&investor, &500);
    assert_eq!(client.get_escrow().funded_amount, 500);
}

// ---------------------------------------------------------------------------
// Edge case 3: maximum amount
// ---------------------------------------------------------------------------

/// `MAX_INVOICE_AMOUNT` must be exactly representable when `token_decimals = 0`
/// (divisor = 1; remainder is always 0 for any integer).
#[test]
fn decimal_scale_max_amount_zero_decimals_valid() {
    let env = Env::default();
    env.mock_all_auths();

    // Use a target equal to MAX_INVOICE_AMOUNT so the escrow accepts the full amount.
    let (client, investor, sac) = deploy_with_scale(&env, 0, MAX_INVOICE_AMOUNT);
    sac.mint(&investor, &MAX_INVOICE_AMOUNT);
    client.fund(&investor, &MAX_INVOICE_AMOUNT);

    assert_eq!(client.get_escrow().funded_amount, MAX_INVOICE_AMOUNT);
}

/// With `token_decimals = 1` and `MAX_INVOICE_AMOUNT` being odd, the full max may
/// not be representable; but the largest multiple of 10 ≤ MAX_INVOICE_AMOUNT is.
#[test]
fn decimal_scale_max_amount_rounded_to_scale_valid() {
    let env = Env::default();
    env.mock_all_auths();

    // Largest multiple of 10 ≤ MAX_INVOICE_AMOUNT
    let valid_max = (MAX_INVOICE_AMOUNT / 10) * 10;
    let (client, investor, sac) = deploy_with_scale(&env, 1, MAX_INVOICE_AMOUNT);
    sac.mint(&investor, &valid_max);
    client.fund(&investor, &valid_max);

    assert_eq!(client.get_escrow().funded_amount, valid_max);
}

// ---------------------------------------------------------------------------
// Edge case 4: negative or blank (zero) amount
// ---------------------------------------------------------------------------

/// A zero amount is rejected by the pre-existing `FundingAmountNotPositive` guard
/// before the scale check is reached — the error code is independent of scale.
#[test]
fn decimal_scale_zero_amount_rejected_before_scale_check() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, investor, _sac) = deploy_with_scale(&env, 2, 10_000_000);

    let result = client.try_fund(&investor, &0);
    match result {
        Err(Ok(e)) => {
            assert_eq!(
                e,
                soroban_sdk::Error::from_contract_error(
                    EscrowError::FundingAmountNotPositive as u32
                )
            );
        }
        Err(Err(soroban_sdk::InvokeError::Contract(code))) => {
            assert_eq!(code, EscrowError::FundingAmountNotPositive as u32);
        }
        other => panic!("expected FundingAmountNotPositive, got {:?}", other),
    }
}

/// A negative amount is also rejected by `FundingAmountNotPositive` before scale.
#[test]
fn decimal_scale_negative_amount_rejected_before_scale_check() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, investor, _sac) = deploy_with_scale(&env, 2, 10_000_000);

    let result = client.try_fund(&investor, &-1);
    match result {
        Err(Ok(e)) => {
            assert_eq!(
                e,
                soroban_sdk::Error::from_contract_error(
                    EscrowError::FundingAmountNotPositive as u32
                )
            );
        }
        Err(Err(soroban_sdk::InvokeError::Contract(code))) => {
            assert_eq!(code, EscrowError::FundingAmountNotPositive as u32);
        }
        other => panic!("expected FundingAmountNotPositive, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Edge case 5: token scale persists — second funder validated against original scale
// ---------------------------------------------------------------------------

/// The `FundingTokenScale` key is written once at init and is never mutated.
/// A second investor calling `fund` after the escrow already has principal is
/// subject to the same scale validation as the first investor.
///
/// This tests the "token scale changes after funding" scenario from the issue:
/// the scale is immutable after init (stored in instance storage under an
/// additive key), so there is no on-chain mutation path — the test verifies
/// that the stored scale is re-read and enforced on every `fund` call.
#[test]
fn decimal_scale_second_funder_validated_against_stored_scale() {
    let env = Env::default();
    env.mock_all_auths();

    // 2 decimals → divisor 100
    // Target is large enough for two funders
    let (client, investor1, sac) = deploy_with_scale(&env, 2, 10_000_000);

    // First investor funds a valid amount (500 = multiple of 100)
    sac.mint(&investor1, &500);
    client.fund(&investor1, &500);
    assert_eq!(client.get_escrow().funded_amount, 500);

    // Second investor attempts an amount with too many fractional digits (501 % 100 == 1)
    let investor2 = Address::generate(&env);
    sac.mint(&investor2, &501);

    let bad = client.try_fund(&investor2, &501);
    match bad {
        Err(Ok(e)) => {
            assert_eq!(
                e,
                soroban_sdk::Error::from_contract_error(
                    EscrowError::FundingTokenScaleInvalid as u32
                )
            );
        }
        Err(Err(soroban_sdk::InvokeError::Contract(code))) => {
            assert_eq!(code, EscrowError::FundingTokenScaleInvalid as u32);
        }
        other => panic!(
            "expected FundingTokenScaleInvalid for second investor, got {:?}",
            other
        ),
    }

    // First investor's contribution is unchanged — scale error is atomic
    let contribution = client.get_contribution(&investor1);
    assert_eq!(
        contribution, 500,
        "first investor contribution must be unchanged after second investor rejection"
    );

    // Second investor with a valid amount (600 = multiple of 100) succeeds
    sac.mint(&investor2, &600);
    client.fund(&investor2, &600);
    assert_eq!(client.get_escrow().funded_amount, 1100);
}

/// Escrow initialized WITHOUT `token_decimals` (None) skips scale validation
/// entirely — any positive amount is accepted (backward-compatible additive key).
#[test]
fn decimal_scale_no_token_decimals_skips_validation() {
    let env = Env::default();
    env.mock_all_auths();

    let sat = install_stellar_asset_token(&env);
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Init without token_decimals (None)
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV-NOSCALE"),
        &sme,
        &10_000_000,
        &800i64,
        &0u64,
        &sat.id,
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

    let investor = Address::generate(&env);
    // Amount with fractional remainder that would fail under 2-decimal scale
    sat.stellar.mint(&investor, &151);
    client.fund(&investor, &151);

    assert_eq!(
        client.get_escrow().funded_amount,
        151,
        "scale validation must be skipped when token_decimals is not configured"
    );
}
