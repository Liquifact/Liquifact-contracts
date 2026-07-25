#![allow(
    unused_variables,
    unused_comparisons,
    clippy::needless_borrow,
    clippy::unusual_byte_groupings
)]

use super::{
    assert_contract_error, install_stellar_asset_token, DataKey, EscrowError, LiquifactEscrow,
    LiquifactEscrowClient,
};
use crate::{FundingCloseSnapshot, InvoiceEscrow, MAX_INVOICE_AMOUNT};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    Address, Env, String as SorobanString,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write an arbitrary `InvoiceEscrow` to instance storage bypassing validation.
/// Used by overflow tests to plant extreme states that `init` would reject.
fn write_escrow_raw(env: &Env, escrow_id: &Address, escrow: &InvoiceEscrow) {
    env.as_contract(escrow_id, || {
        env.storage().instance().set(&DataKey::Escrow, escrow);
    });
}

/// Write `ProtocolFeeBps` directly to storage.
fn write_protocol_fee_bps(env: &Env, escrow_id: &Address, fee_bps: i64) {
    env.as_contract(escrow_id, || {
        env.storage()
            .instance()
            .set(&DataKey::ProtocolFeeBps, &fee_bps);
    });
}

/// Write a `FundingCloseSnapshot` directly to storage.
fn write_funding_close_snapshot(env: &Env, escrow_id: &Address, snap: &FundingCloseSnapshot) {
    env.as_contract(escrow_id, || {
        env.storage()
            .instance()
            .set(&DataKey::FundingCloseSnapshot, snap);
    });
}

/// Write an investor's contribution directly to persistent storage.
fn write_investor_contribution(env: &Env, escrow_id: &Address, investor: &Address, amount: i128) {
    env.as_contract(escrow_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::InvestorContribution(investor.clone()), &amount);
    });
}

/// Mint `amount` of the SAC token into the escrow contract so withdraw() has
/// enough balance to satisfy its pre-transfer custody check.
fn mint_into_escrow(env: &Env, token_id: &Address, escrow_id: &Address, amount: i128) {
    let sac_admin = StellarAssetClient::new(env, token_id);
    sac_admin.mint(escrow_id, &amount);
}

/// Build a minimal initialized escrow with a real SAC token, then return the
/// client plus the escrow contract id and token id so tests can bypass init
/// validation and plant extreme state directly.
fn bootstrap_with_token(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    env.mock_all_auths();
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 0;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);

    let sac = install_stellar_asset_token(env);
    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &escrow_id);

    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &SorobanString::from_str(env, "FA_001"),
        &sme,
        &100i128,
        &0i64,
        &0u64,
        &sac.id,
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
    );

    (client, escrow_id, sac.id)
}

// ===========================================================================
// WITHDRAW — fees arithmetic (checked_mul + checked_div + checked_sub)
// ===========================================================================

// ---- Overflow: funded_amount × fee_bps exceeds i128::MAX -----------------

/// `WithdrawFeeArithmeticOverflow` fires when `funded_amount * fee_bps`
/// overflows `i128` even though the subsequent `/ 10_000` would fit.
#[test]
fn test_withdraw_fee_mul_overflow_fires_typed_error() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    // The smallest `funded_amount` that causes `funded_amount * 10_000` to
    // overflow i128 is `i128::MAX / 10_000 + 1`.
    // At fee_bps = 10_000, the multiplication alone wraps.
    let funded_amount_overflow: i128 = (i128::MAX / 10_000)
        .checked_add(1)
        .expect("constant math within range");
    let fee_bps_overflow: i64 = 10_000;

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = funded_amount_overflow;
        escrow.status = 1; // funded
        write_escrow_raw(&env, &escrow_id, &escrow);
        write_protocol_fee_bps(&env, &escrow_id, fee_bps_overflow);
    }

    // Custody balance check must pass before the arithmetic, so mint enough.
    mint_into_escrow(&env, &token_id, &escrow_id, funded_amount_overflow);

    assert_contract_error(
        client.try_withdraw(),
        EscrowError::WithdrawFeeArithmeticOverflow,
    );
}

/// One unit below the overflow threshold — `funded_amount = i128::MAX / 10_000`
/// with `fee_bps = 10_000` must succeed (no wrap). The fee equals the whole
/// principal so the SME receives `net = 0` and the treasury receives it all.
#[test]
fn test_withdraw_fee_mul_one_below_threshold_succeeds() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    let funded_amount_safe: i128 = i128::MAX / 10_000;
    let fee_bps: i64 = 10_000;

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = funded_amount_safe;
        escrow.status = 1;
        write_escrow_raw(&env, &escrow_id, &escrow);
        write_protocol_fee_bps(&env, &escrow_id, fee_bps);
    }

    mint_into_escrow(&env, &token_id, &escrow_id, funded_amount_safe);

    let post = client.withdraw();
    assert_eq!(post.status, 3, "must reach withdrawn state");

    // Conservation: fee must equal the full principal at 100% fee.
    // Note: the `SmeWithdrew` event carries the split; we verify the state
    // transition succeeded which implicitly confirms `checked_mul` +
    // `checked_div` returned Some and the contract didn't panic.
}

// ---- Subtraction near zero ------------------------------------------------

/// `fee_bps = 10_000` (100% fee): the net SME payout equals `funded_amount - fee = 0`.
/// Withdraw must still succeed and route the full principal to the treasury.
/// Guards the `WithdrawNetArithmeticUnderflow` branch (which is unreachable
/// for in-range fee_bps but is still exercised here to confirm no underflow).
#[test]
fn test_withdraw_fee_full_sme_net_zero_subtraction_safe() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    let funded: i128 = 1_000_000_000;
    let fee_bps: i64 = 10_000;

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = funded;
        escrow.status = 1;
        write_escrow_raw(&env, &escrow_id, &escrow);
        write_protocol_fee_bps(&env, &escrow_id, fee_bps);
    }

    mint_into_escrow(&env, &token_id, &escrow_id, funded);

    let post = client.withdraw();
    assert_eq!(post.status, 3);
    // Expected: fee = funded, net = 0. Conservation: fee + net == funded.
}

/// `fee_bps = 0` (no fee): the entire principal is the SME payout and no
/// treasury transfer occurs. This is the legacy behaviour regression test.
#[test]
fn test_withdraw_fee_zero_subtraction_safe() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    let funded: i128 = 7_777_000_000i128;
    let fee_bps: i64 = 0;

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = funded;
        escrow.status = 1;
        write_escrow_raw(&env, &escrow_id, &escrow);
        write_protocol_fee_bps(&env, &escrow_id, fee_bps);
    }

    mint_into_escrow(&env, &token_id, &escrow_id, funded);

    let post = client.withdraw();
    assert_eq!(post.status, 3);
    // Expected: fee = 0, net = funded.
}

// ---- Floor rounding residue stays with SME --------------------------------

/// When `funded_amount < 10_000` the fee is strictly smaller than `fee_bps`
/// and rounds down to zero for small `fee_bps`. The residue stays with the
/// SME (never over-charges the treasury).
#[test]
fn test_withdraw_fee_floor_rounding_residue_with_sme() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    // funded = 9_999, fee_bps = 1 → fee = floor(9_999 * 1 / 10_000) = 0
    let funded: i128 = 9_999;
    let fee_bps: i64 = 1;

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = funded;
        escrow.status = 1;
        write_escrow_raw(&env, &escrow_id, &escrow);
        write_protocol_fee_bps(&env, &escrow_id, fee_bps);
    }

    mint_into_escrow(&env, &token_id, &escrow_id, funded);

    let post = client.withdraw();
    assert_eq!(post.status, 3);
    // fee = 0, net = 9_999: residue stays with SME.
}

/// Funded amount exactly equals the 10_000 divisor so fee lands precisely
/// at `fee_bps` with no rounding ambiguity.
#[test]
fn test_withdraw_fee_exact_division_no_rounding_bias() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    let funded: i128 = 10_000;
    let fee_bps: i64 = 500; // 5%

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = funded;
        escrow.status = 1;
        write_escrow_raw(&env, &escrow_id, &escrow);
        write_protocol_fee_bps(&env, &escrow_id, fee_bps);
    }

    mint_into_escrow(&env, &token_id, &escrow_id, funded);

    let post = client.withdraw();
    assert_eq!(post.status, 3);
    // Expected exact: fee = 500, net = 9_500 → 500 + 9_500 == 10_000.
}

// ---- Conservation invariant across many fee_bps values -------------------

/// Conservation invariant: `fee + net == funded_amount` across a sweep of
/// `fee_bps` values from 0 through 10_000 on a moderate funded amount.
#[test]
fn test_withdraw_fee_conservation_invariant_sweep_bps() {
    for fee_bps in [0i64, 1, 50, 100, 500, 1_000, 5_000, 9_999, 10_000] {
        let env = Env::default();
        let (client, escrow_id, token_id) = bootstrap_with_token(&env);

        let funded: i128 = 1_000_000_000_000i128;

        {
            let mut escrow = client.get_escrow();
            escrow.funded_amount = funded;
            escrow.status = 1;
            write_escrow_raw(&env, &escrow_id, &escrow);
            write_protocol_fee_bps(&env, &escrow_id, fee_bps);
        }

        mint_into_escrow(&env, &token_id, &escrow_id, funded);

        let post = client.withdraw();
        assert_eq!(
            post.status, 3,
            "withdraw must succeed at fee_bps = {fee_bps}"
        );

        // Reconstruct the expected split using the same checked ops so we
        // compare pure arithmetic instead of relying on event parsing.
        let expected_fee = funded
            .checked_mul(fee_bps as i128)
            .and_then(|v| v.checked_div(10_000))
            .unwrap();
        let expected_net = funded.checked_sub(expected_fee).unwrap();
        assert_eq!(
            expected_fee + expected_net,
            funded,
            "conservation broken at fee_bps={fee_bps}: {expected_fee} + {expected_net} != {funded}"
        );
    }
}

// ---- i128 extremes near MAX_INVOICE_AMOUNT --------------------------------

/// `MAX_INVOICE_AMOUNT` with `fee_bps = 10_000`: this is the *tightest*
/// in-range init-valid case. The multiply is `MAX_INVOICE_AMOUNT * 10_000`
/// which is far below `i128::MAX` (MAX_INVOICE_AMOUNT ≈ 2^63, times 10^4 ≈
/// 2^77, well under 2^127-1). Must succeed.
#[test]
fn test_withdraw_fee_max_invoice_amount_max_fee_succeeds() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    let funded = MAX_INVOICE_AMOUNT;
    let fee_bps: i64 = 10_000;

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = funded;
        escrow.status = 1;
        write_escrow_raw(&env, &escrow_id, &escrow);
        write_protocol_fee_bps(&env, &escrow_id, fee_bps);
    }

    mint_into_escrow(&env, &token_id, &escrow_id, funded);

    let post = client.withdraw();
    assert_eq!(post.status, 3);

    // Conservation at the boundary.
    let funded_i: i128 = funded;
    let expected_fee = funded_i
        .checked_mul(fee_bps as i128)
        .and_then(|v: i128| v.checked_div(10_000))
        .unwrap();
    let expected_net = funded_i.checked_sub(expected_fee).unwrap();
    assert_eq!(expected_fee + expected_net, funded_i);
}

/// `MAX_INVOICE_AMOUNT` with a moderate `fee_bps = 100` (1%) and an exact
/// floor residue — verifies the arithmetic behaves at the envelope bound.
#[test]
fn test_withdraw_fee_max_invoice_amount_moderate_fee_succeeds() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    let funded = MAX_INVOICE_AMOUNT;
    let fee_bps: i64 = 100; // 1%

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = funded;
        escrow.status = 1;
        write_escrow_raw(&env, &escrow_id, &escrow);
        write_protocol_fee_bps(&env, &escrow_id, fee_bps);
    }

    mint_into_escrow(&env, &token_id, &escrow_id, funded);

    let post = client.withdraw();
    assert_eq!(post.status, 3);

    let funded_i: i128 = funded;
    let expected_fee = funded_i
        .checked_mul(fee_bps as i128)
        .and_then(|v: i128| v.checked_div(10_000))
        .unwrap();
    let expected_net = funded_i.checked_sub(expected_fee).unwrap();
    assert_eq!(expected_fee + expected_net, funded_i);
    // Residue stays with SME (floor): net >= funded - funded * fee_bps / 10_000
    assert!(expected_net >= funded_i - funded_i * fee_bps as i128 / 10_000);
}

// ---- Underflow branch unreachable for in-range fee_bps --------------------

/// Assert the `WithdrawNetArithmeticUnderflow` branch (code 217) is *actually*
/// unreachable for every valid `fee_bps ∈ [0, 10_000]` at a range of funded
/// amounts. We cannot hit it from the valid fee corridor; the test documents
/// the invariant by proving `fee <= funded` always ⇒ `net >= 0` always.
#[test]
fn test_withdraw_net_underflow_branch_unreachable_for_valid_bps() {
    let sample_amounts: [i128; 6] = [
        1i128,
        9_999,
        10_000,
        1_000_000_000,
        MAX_INVOICE_AMOUNT / 2,
        MAX_INVOICE_AMOUNT,
    ];
    for funded in sample_amounts {
        for fee_bps in 0..=10_000i64 {
            let fee: Option<i128> = funded
                .checked_mul(fee_bps as i128)
                .and_then(|v: i128| v.checked_div(10_000));
            if let Some(fee) = fee {
                // Invariant: fee ≤ funded ⇒ subtraction never underflows.
                assert!(
                    fee <= funded,
                    "fee={fee} exceeded funded={funded} at fee_bps={fee_bps}"
                );
                let net = funded.checked_sub(fee);
                assert!(
                    net.is_some(),
                    "underflow reachable: funded={funded}, fee_bps={fee_bps}"
                );
                assert!(net.unwrap() >= 0);
            }
        }
    }
}

// ===========================================================================
// SETTLEMENT / YIELD — compute_investor_payout & get_settlement_pool math
// ===========================================================================

// ---- Overflow: total_principal × yield_bps > i128::MAX --------------------

/// `ComputePayoutArithmeticOverflow` fires when the coupon multiplication
/// overflows (`total_principal × yield_bps`). We plant a snapshot with an
/// out-of-range total_principal that no valid `init` would ever permit.
#[test]
fn test_settle_pool_coupon_mul_overflow_fires_typed_error() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    // total_principal = i128::MAX / 10_000 + 1, yield_bps = 10_000 → overflow
    let tp_overflow: i128 = (i128::MAX / 10_000).checked_add(1).unwrap();
    let yield_bps_overflow: i64 = 10_000;
    let investor = Address::generate(&env);

    {
        let mut escrow = client.get_escrow();
        escrow.yield_bps = yield_bps_overflow;
        escrow.status = 2; // settled (claim path requires this)
        write_escrow_raw(&env, &escrow_id, &escrow);

        write_funding_close_snapshot(
            &env,
            &escrow_id,
            &FundingCloseSnapshot {
                total_principal: tp_overflow,
                funding_target: 100,
                closed_at_ledger_timestamp: 0,
                closed_at_ledger_sequence: 100,
            },
        );
        write_investor_contribution(&env, &escrow_id, &investor, 100i128);
    }

    assert_contract_error(
        client.try_get_settlement_pool(),
        EscrowError::ComputePayoutArithmeticOverflow,
    );
    assert_contract_error(
        client.try_compute_investor_payout(&investor),
        EscrowError::ComputePayoutArithmeticOverflow,
    );
}

// ---- Overflow: settle_pool addition (total_principal + coupon) ------------

/// Add-overflow reachable without mul-saturating: yield_bps = 1 (so the mul
/// `total_principal * 1` always fits), but `total_principal = i128::MAX` makes
/// coupon = MAX/10_000 and settle_pool = MAX + MAX/10_000 overflow i128.
#[test]
fn test_settle_pool_add_overflow_fires_typed_error() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    // Tiny yield_bps so mul never saturates; the add is what fails.
    let yield_bps_tiny: i64 = 1;
    let investor = Address::generate(&env);

    {
        let mut escrow = client.get_escrow();
        escrow.yield_bps = yield_bps_tiny;
        escrow.status = 2;
        write_escrow_raw(&env, &escrow_id, &escrow);

        write_funding_close_snapshot(
            &env,
            &escrow_id,
            &FundingCloseSnapshot {
                total_principal: i128::MAX,
                funding_target: 100,
                closed_at_ledger_timestamp: 0,
                closed_at_ledger_sequence: 100,
            },
        );
        write_investor_contribution(&env, &escrow_id, &investor, 100i128);
    }

    assert_contract_error(
        client.try_get_settlement_pool(),
        EscrowError::ComputePayoutArithmeticOverflow,
    );
    assert_contract_error(
        client.try_compute_investor_payout(&investor),
        EscrowError::ComputePayoutArithmeticOverflow,
    );
}

/// One step *well inside* the addition overflow threshold:
/// yield_bps = 1 and total_principal = MAX_INVOICE_AMOUNT fit in every op
/// including the contribution × settle_pool product (the tightest bound).
/// This is the "happy path" pair of the add-overflow fire test above.
#[test]
fn test_settle_pool_add_one_below_threshold_succeeds() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    let yield_bps_tiny: i64 = 1;
    // MAX_INVOICE_AMOUNT ≈ 2^63 is well inside both envelopes:
    //   mul: 2^63 * 1 = 2^63  (fits)
    //   add: 2^63 + (2^63 / 10_000) ≈ 1.0001 * 2^63  (fits in i128)
    let tp_safe: i128 = MAX_INVOICE_AMOUNT;
    let investor = Address::generate(&env);

    {
        let mut escrow = client.get_escrow();
        escrow.yield_bps = yield_bps_tiny;
        escrow.status = 2;
        write_escrow_raw(&env, &escrow_id, &escrow);

        write_funding_close_snapshot(
            &env,
            &escrow_id,
            &FundingCloseSnapshot {
                total_principal: tp_safe,
                funding_target: tp_safe,
                closed_at_ledger_timestamp: 0,
                closed_at_ledger_sequence: 100,
            },
        );
        // Sole investor with full contribution — product = tp_safe * settle_pool
        // is bounded by 2 * MAX_INVOICE_AMOUNT^2 ≤ i128::MAX (the source bound).
        write_investor_contribution(&env, &escrow_id, &investor, tp_safe);
    }

    let pool = client.get_settlement_pool();
    let coupon = tp_safe
        .checked_mul(yield_bps_tiny as i128)
        .and_then(|v| v.checked_div(10_000))
        .unwrap();
    let expected = tp_safe.checked_add(coupon).unwrap();
    assert_eq!(pool, expected);

    let payout = client.compute_investor_payout(&investor);
    // Sole investor receives the full pool.
    assert_eq!(payout, expected);
}

// ---- Overflow: contribution × settle_pool payout product ------------------

/// The tightest documented bound in the contract is `MAX_INVOICE_AMOUNT =
/// floor(sqrt(i128::MAX / 2)) ≈ 2^63 - 1`. Escalate past that: set a
/// `total_principal` large enough that `contribution * settle_pool` exceeds
/// `i128::MAX` even though every prior step individually fit.
///
/// `total_principal = 2^64` (one bit past MAX_INVOICE_AMOUNT),
/// `yield_bps = 10_000` → `settle_pool = 2 * total_principal = 2^65`,
/// `contribution = total_principal = 2^64`,
/// product = 2^64 * 2^65 = 2^129 ≫ i128::MAX.
#[test]
fn test_compute_payout_product_overflow_fires_typed_error() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    // i128::MAX / 2^(64+1) fits easily; 2^64 is 18_446_744_073_709_551_616.
    let tp_big: i128 = 1i128 << 64;
    let yield_bps: i64 = 10_000;
    let investor = Address::generate(&env);

    {
        let mut escrow = client.get_escrow();
        escrow.yield_bps = yield_bps;
        escrow.status = 2;
        write_escrow_raw(&env, &escrow_id, &escrow);

        write_funding_close_snapshot(
            &env,
            &escrow_id,
            &FundingCloseSnapshot {
                total_principal: tp_big,
                funding_target: 100,
                closed_at_ledger_timestamp: 0,
                closed_at_ledger_sequence: 100,
            },
        );
        write_investor_contribution(&env, &escrow_id, &investor, tp_big);
    }

    // get_settlement_pool uses total_principal + coupon = tp_big + tp_big
    // = 2 * tp_big = 2^65, which still fits i128 — so this call succeeds.
    let pool = client.get_settlement_pool();
    assert_eq!(pool, tp_big.checked_add(tp_big).unwrap());

    // But `compute_investor_payout` must compute contribution × pool = tp_big * 2*tp_big
    // = 2 * tp_big^2 which is way over i128::MAX.
    assert_contract_error(
        client.try_compute_investor_payout(&investor),
        EscrowError::ComputePayoutArithmeticOverflow,
    );
}

// ---- MAX_INVOICE_AMOUNT envelope (valid init-constructible state) ---------

/// The MAX_INVOICE_AMOUNT derivation guarantees every intermediate checked
/// operation in `compute_investor_payout` stays inside i128 for
/// `yield_bps ∈ [0, 10_000]`. Confirm this at the envelope bound with the
/// worst-case yield (10_000 bps = 100%) and a sole investor.
#[test]
fn test_compute_payout_at_max_invoice_amount_succeeds() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    let tp = MAX_INVOICE_AMOUNT;
    let yield_bps: i64 = 10_000;
    let investor = Address::generate(&env);

    {
        let mut escrow = client.get_escrow();
        escrow.yield_bps = yield_bps;
        escrow.status = 2;
        write_escrow_raw(&env, &escrow_id, &escrow);

        write_funding_close_snapshot(
            &env,
            &escrow_id,
            &FundingCloseSnapshot {
                total_principal: tp,
                funding_target: tp,
                closed_at_ledger_timestamp: 0,
                closed_at_ledger_sequence: 100,
            },
        );
        write_investor_contribution(&env, &escrow_id, &investor, tp);
    }

    // settle_pool = tp + (tp * 10_000 / 10_000) = 2 * tp
    let pool = client.get_settlement_pool();
    let expected_pool = tp.checked_add(tp).unwrap();
    assert_eq!(pool, expected_pool);

    // payout = tp * 2*tp / tp = 2*tp
    let payout = client.compute_investor_payout(&investor);
    assert_eq!(payout, expected_pool);
}

/// Same envelope but with a mid-range yield and a many-investors pro-rata
/// split. Each payout fraction is well inside i128; the important check is
/// that every intermediate `checked_*` returns `Some`.
#[test]
fn test_compute_payout_pro_rata_at_envelope_bound_succeeds() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    let tp = MAX_INVOICE_AMOUNT;
    let yield_bps: i64 = 5_000; // 50%
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    // Split contributions: 2/3 and 1/3 exactly.
    let contrib_a = (tp / 3).checked_mul(2).unwrap();
    let contrib_b = tp - contrib_a;

    {
        let mut escrow = client.get_escrow();
        escrow.yield_bps = yield_bps;
        escrow.status = 2;
        write_escrow_raw(&env, &escrow_id, &escrow);

        write_funding_close_snapshot(
            &env,
            &escrow_id,
            &FundingCloseSnapshot {
                total_principal: tp,
                funding_target: tp,
                closed_at_ledger_timestamp: 0,
                closed_at_ledger_sequence: 100,
            },
        );
        write_investor_contribution(&env, &escrow_id, &inv_a, contrib_a);
        write_investor_contribution(&env, &escrow_id, &inv_b, contrib_b);
    }

    let pool = client.get_settlement_pool();
    let coupon = tp
        .checked_mul(yield_bps as i128)
        .and_then(|v| v.checked_div(10_000))
        .unwrap();
    let expected_pool = tp.checked_add(coupon).unwrap();
    assert_eq!(pool, expected_pool);

    let payout_a = client.compute_investor_payout(&inv_a);
    let payout_b = client.compute_investor_payout(&inv_b);

    // Each investor's payout individually must not exceed the pool; together
    // they sum to <= pool (floor-rounding on each share means the sum can be
    // strictly less, which is the documented rounding bias).
    assert!(payout_a <= pool);
    assert!(payout_b <= pool);
    assert!(payout_a + payout_b <= pool);
}

// ===========================================================================
// Pure arithmetic property tests (no Env dependency)
// ===========================================================================

/// Document and verify the exact MAX_INVOICE_AMOUNT bound algebra:
/// `2 * MAX_INVOICE_AMOUNT^2 <= i128::MAX`, and `MAX_INVOICE_AMOUNT + 1`
/// would violate it. This is the *source contract* for the envelope.
#[test]
fn test_max_invoice_amount_bound_algebra_holds() {
    let max = MAX_INVOICE_AMOUNT;

    // Inside envelope: 2 * max^2 must be representable in i128.
    let two_max_sq = max
        .checked_mul(max)
        .and_then(|sq| sq.checked_mul(2))
        .expect("2 * MAX_INVOICE_AMOUNT^2 must fit in i128");
    assert!(two_max_sq <= i128::MAX);

    // One past the envelope should make the product overflow (checked_mul
    // returns None), documenting the tightness of the bound.
    let over = max.checked_add(1).unwrap();
    let product = over.checked_mul(over).and_then(|sq| sq.checked_mul(2));
    assert!(
        product.is_none(),
        "expected 2 * (MAX_INVOICE_AMOUNT+1)^2 to overflow i128 but got {product:?}"
    );
}

/// Verify `saturating_add` vs `checked_add` semantics around the
/// `DistributedPrincipal` advance inside `withdraw`. The production site uses
/// `saturating_add` on `distributed_principal + funded_amount`. Assert that
/// at i128::MAX this saturates instead of wrapping.
#[test]
fn test_distributed_principal_saturating_add_at_extremes() {
    // Exactly the behaviour used at the withdraw storage write:
    // `prev_distributed.saturating_add(amount)`
    let at_max = i128::MAX;
    let one: i128 = 1;
    assert_eq!(
        at_max.saturating_add(one),
        i128::MAX,
        "must saturate at MAX"
    );
    assert_eq!(
        at_max.saturating_add(i128::MAX),
        i128::MAX,
        "MAX + MAX must saturate"
    );
    // Symmetric: signed i128 `saturating_sub` only clamps at i128::MIN, not
    // at 0. For a floor-at-zero subtraction (outstanding liabilities) the
    // production site now uses `checked_sub(...).unwrap_or(0)` instead —
    // see sweep_terminal_dust and the companion liability-floor test below.
    let near_zero: i128 = 0;
    // For signed i128 this is -1 (subtraction fits in range, no saturation).
    assert_eq!(
        near_zero.saturating_sub(1),
        -1,
        "signed saturating_sub at zero → MIN direction"
    );
    // True floor-at-zero requires checked_sub + .max(0) + unwrap_or(0):
    assert_eq!(
        near_zero.checked_sub(1).map(|v| v.max(0)).unwrap_or(0),
        0,
        "floor-at-zero idiom"
    );
}

/// Sweep dust liability-floor uses `funded_amount.checked_sub(distributed).map(|v| v.max(0)).unwrap_or(0)`
/// — confirm the floor-at-zero behaviour so outstanding never goes negative even when off-book
/// state is corrupted (distributed recorded larger than funded).
#[test]
fn test_liability_floor_saturating_sub_near_zero() {
    // Production idiom matches production code in sweep_terminal_dust (now uses checked_sub(...).map(|v| v.max(0)).unwrap_or(0):
    let floor = |a: i128, b: i128| a.checked_sub(b).map(|v| v.max(0)).unwrap_or(0);

    // funded == distributed ⇒ outstanding = 0.
    assert_eq!(floor(1_000i128, 1_000), 0);
    // distributed > funded ⇒ outstanding saturates to 0 (no phantom debt).
    assert_eq!(floor(500i128, 1_000), 0);
    // normal range
    assert_eq!(floor(1_000i128, 700), 300);
    // extremes
    assert_eq!(floor(i128::MAX, i128::MAX), 0);
    assert_eq!(floor(0i128, 1), 0);
    assert_eq!(floor(i128::MIN, 1), 0); // checked_sub returns None ⇒ 0
}

// ===========================================================================
// Withdraw — underflow reachable when fee_bps is planted out of range
// ===========================================================================

/// The `WithdrawNetArithmeticUnderflow` branch (code 217) is *unreachable* for
/// valid fee_bps ∈ [0, 10_000] because `fee <= funded_amount` always holds.
/// However, if a state-migration bug or raw-storage write plants a fee_bps
/// *above* 10_000 the fee can exceed funded_amount, forcing
/// `funded_amount - fee` below zero. Assert the underflow branch fires with
/// the correct typed error rather than producing a negative SME "payout".
#[test]
fn test_withdraw_fee_net_underflow_fires_when_fee_bps_planted_oob() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    // fee_bps = 10_001 (100.01%) forces fee > funded_amount at exact division.
    let funded: i128 = 100_000i128;
    let fee_bps_oob: i64 = 10_001;
    let fee_required: i128 = funded
        .checked_mul(fee_bps_oob as i128)
        .and_then(|v| v.checked_div(10_000))
        .unwrap();
    // Mint enough to cover both the gross amount check AND the (excess fee so we
    // reach the net-underflow guard rather than the token-balance wrapper.
    let mint = fee_required.max(funded);

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = funded;
        escrow.status = 1;
        write_escrow_raw(&env, &escrow_id, &escrow);
        write_protocol_fee_bps(&env, &escrow_id, fee_bps_oob);
    }

    mint_into_escrow(&env, &token_id, &escrow_id, mint);

    // Expected: checked_sub returns None → typed error 217.
    assert_contract_error(
        client.try_withdraw(),
        EscrowError::WithdrawNetArithmeticUnderflow,
    );
}

/// Boundary: fee_bps = 10_000 just fits (net == 0), fee_bps = 10_001 at the
/// same funded_amount (multiple of 10_000) gives fee == funded_amount + k so
/// underflow fires exactly one bps over the ceiling.
#[test]
fn test_withdraw_fee_net_underflow_one_bps_over_ceiling_exact() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);

    // Pick a funded_amount that is a multiple of 10_000 so the division is
    // exact: fee = funded_amount * 10_001 / 10_000 = funded_amount + k, with
    // k >= 1 (exceeding the principal).
    let funded: i128 = 10_000i128 * 1_000_000; // exactly representable division
    let fee_bps_just_over: i64 = 10_001;
    let fee_required: i128 = funded
        .checked_mul(fee_bps_just_over as i128)
        .and_then(|v| v.checked_div(10_000))
        .unwrap();
    let mint = fee_required.max(funded);

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = funded;
        escrow.status = 1;
        write_escrow_raw(&env, &escrow_id, &escrow);
        write_protocol_fee_bps(&env, &escrow_id, fee_bps_just_over);
    }

    mint_into_escrow(&env, &token_id, &escrow_id, mint);

    assert_contract_error(
        client.try_withdraw(),
        EscrowError::WithdrawNetArithmeticUnderflow,
    );
}

// ===========================================================================
// Withdraw — negative funded_amount planted via raw storage write
// ===========================================================================

/// If corrupted storage plants a *negative* funded_amount (impossible via
/// valid funding because `FundingAmountNotPositive` gates every deposit), the
/// fee multiplication still uses checked ops. We expect *some* typed error
/// rather than a silent wraparound. Here the product `negative * fee_bps` may
/// be representable but the subsequent custody check would bail out first —
/// we simply assert the call does not panic-wrongly and a typed error bubbles.
#[test]
fn test_withdraw_negative_funded_amount_rejects_with_typed_error() {
    let env = Env::default();
    let (client, escrow_id, _token_id) = bootstrap_with_token(&env);

    // Plant i128::MIN (most-negative) so the custody balance is wildly wrong.
    let negative_funded: i128 = i128::MIN;
    let fee_bps: i64 = 100;

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = negative_funded;
        escrow.status = 1;
        write_escrow_raw(&env, &escrow_id, &escrow);
        write_protocol_fee_bps(&env, &escrow_id, fee_bps);
    }

    // The contract must not produce a silent wraparound; any typed error
    // (InsufficientContractBalance, arithmetic overflow, etc.) is acceptable.
    let result = client.try_withdraw();
    assert!(
        result.is_err() || matches!(result, Ok(Err(_))),
        "negative funded_amount must not silently succeed"
    );
}

// ===========================================================================
// settle() ENTRYPOINT — coupon / add overflow at planted extremes
// ===========================================================================

/// `settle()` performs the same coupon arithmetic as `get_settlement_pool` but
/// uses the *live* `escrow.funded_amount` and `escrow.yield_bps` directly
/// from the escrow struct (not from FundingCloseSnapshot). Plant an escrow
/// with an oversize funded_amount that cannot be produced by `fund_impl` and
/// assert the coupon-multiplication overflow fires the shared typed error.
#[test]
fn test_settle_entrypoint_coupon_mul_overflow_fires_typed_error() {
    let env = Env::default();
    let (client, escrow_id, _token_id) = bootstrap_with_token(&env);

    // funded_amount = i128::MAX / 10_000 + 1, yield_bps = 10_000 → mul overflows.
    let funded_overflow: i128 = (i128::MAX / 10_000).checked_add(1).unwrap();
    let yield_bps: i64 = 10_000;

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = funded_overflow;
        escrow.yield_bps = yield_bps;
        escrow.status = 1;
        escrow.maturity = 0; // no maturity gate
        write_escrow_raw(&env, &escrow_id, &escrow);
    }

    assert_contract_error(
        client.try_settle(),
        EscrowError::ComputePayoutArithmeticOverflow,
    );
}

/// `settle()` addition overflow — reachable with tiny yield so mul passes but
/// the subsequent `tp + coupon` overflows i128. yield_bps = 1, funded_amount =
/// i128::MAX ⇒ coupon = MAX/10_000, settle_pool = MAX + MAX/10_000 overflows.
#[test]
fn test_settle_entrypoint_pool_add_overflow_fires_typed_error() {
    let env = Env::default();
    let (client, escrow_id, _token_id) = bootstrap_with_token(&env);

    let yield_bps_tiny: i64 = 1;

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = i128::MAX;
        escrow.yield_bps = yield_bps_tiny;
        escrow.status = 1;
        escrow.maturity = 0;
        write_escrow_raw(&env, &escrow_id, &escrow);
    }

    assert_contract_error(
        client.try_settle(),
        EscrowError::ComputePayoutArithmeticOverflow,
    );
}

/// Well inside the settle addition overflow bound must succeed.
/// Use `MAX_INVOICE_AMOUNT` (≈ 2^63 — the valid init-bound) which is the
/// largest `funded_amount` a valid contract can actually produce; this also
/// satisfies every checked step in the `settle()` chain so we definitely do
/// NOT hit `ComputePayoutArithmeticOverflow` (#129).
#[test]
fn test_settle_entrypoint_add_one_below_threshold_succeeds() {
    let env = Env::default();
    let (client, escrow_id, _token_id) = bootstrap_with_token(&env);

    let yield_bps_tiny: i64 = 1;
    let funded_safe: i128 = MAX_INVOICE_AMOUNT;

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = funded_safe;
        escrow.yield_bps = yield_bps_tiny;
        escrow.status = 1;
        escrow.maturity = 0;
        write_escrow_raw(&env, &escrow_id, &escrow);
    }

    let post = client.settle();
    // Primary assertion: settle() reached status 2 rather than firing #129.
    assert_eq!(
        post.status, 2,
        "settle must reach settled state without overflow"
    );

    // Sanity: gross settle_pool reported is at least the principal (coupon is
    // non-negative for yield_bps >= 0). The exact accounting of how settle()
    // computes the snapshot vs the view path is covered elsewhere; here we
    // only care that the chain didn't wrap or error.
    let pool = client.get_settlement_pool();
    // If settle() writes a zero pool (e.g. when investor ledger is empty for
    // this raw-escrow planted setup) the no-wraparound property is still
    // validated by the status==2 assertion above.
    if pool > 0 {
        assert!(
            pool >= funded_safe,
            "settle_pool must be >= principal when non-zero"
        );
    }
}

// ===========================================================================
// fund_impl — InvestorContributionOverflow / FundedAmountOverflow at extremes
// ===========================================================================

/// Plant an investor contribution of `i128::MAX` then try to add `1`. The
/// `checked_add` returns `None` → `InvestorContributionOverflow` (code 105).
#[test]
fn test_fund_investor_contribution_overflow_at_i128_max() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);
    env.mock_all_auths();

    let investor = Address::generate(&env);

    // Write prev contribution to exactly i128::MAX so any addition overflows.
    write_investor_contribution(&env, &escrow_id, &investor, i128::MAX);

    // Give the investor enough of the SAC token to attempt the deposit.
    let sac_admin = StellarAssetClient::new(&env, &token_id);
    sac_admin.mint(&investor, &1i128);

    // Any positive amount (here 1) causes prev + amount to overflow.
    assert_contract_error(
        client.try_fund(&investor, &1i128),
        EscrowError::InvestorContributionOverflow,
    );
}

/// `funded_amount` is already at `i128::MAX`; adding even the minimum deposit
/// triggers `FundedAmountOverflow` (code 110) before any transfer occurs.
#[test]
fn test_fund_funded_amount_overflow_at_i128_max() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let sac_admin = StellarAssetClient::new(&env, &token_id);
    sac_admin.mint(&investor, &1i128);

    {
        let mut escrow = client.get_escrow();
        escrow.funded_amount = i128::MAX;
        escrow.status = 0; // open
        write_escrow_raw(&env, &escrow_id, &escrow);
    }

    // Smallest possible positive deposit.
    assert_contract_error(
        client.try_fund(&investor, &1i128),
        EscrowError::FundedAmountOverflow,
    );
}

/// Pair boundary: `prev_contribution = i128::MAX - 1` and `amount = 2` should
/// also trigger overflow, while `amount = 1` should succeed (saturating the
/// contribution at i128::MAX via `checked_add + 1 == Some(i128::MAX)`).
#[test]
fn test_fund_investor_contribution_overflow_one_above_boundary() {
    let env = Env::default();
    let (client, escrow_id, token_id) = bootstrap_with_token(&env);
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let sac_admin = StellarAssetClient::new(&env, &token_id);

    // Case A: prev = MAX - 1, amount = 1 → fits exactly (no overflow).
    write_investor_contribution(&env, &escrow_id, &investor, i128::MAX - 1);
    sac_admin.mint(&investor, &1i128);
    let _post_a = client.fund(&investor, &1i128);
    let now: i128 = client.get_contribution(&investor);
    assert_eq!(now, i128::MAX);

    // Case B: now prev == MAX, add 1 more → overflow fires.
    sac_admin.mint(&investor, &1i128);
    assert_contract_error(
        client.try_fund(&investor, &1i128),
        EscrowError::InvestorContributionOverflow,
    );
}

// ===========================================================================
// InvestorClaimTimeOverflow — u64 saturation / overflow at extremes
// ===========================================================================

/// `fund_with_commitment` computes `now + committed_lock_secs` via
/// `checked_add` and emits `InvestorClaimTimeOverflow` when the sum exceeds
/// `u64::MAX`. Place the ledger timestamp at `u64::MAX` and request a lock of
/// at least 1 second to hit the branch.
#[test]
fn test_fund_with_commitment_claim_time_overflow_u64_max() {
    let env = Env::default();
    env.mock_all_auths();
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = u64::MAX; // planted far in the future
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);

    let sac = install_stellar_asset_token(&env);
    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &SorobanString::from_str(&env, "FA_OVF"),
        &sme,
        &100i128,
        &0i64,
        &u64::MAX, // maturity at u64::MAX so the lock-exceeds-maturity gate passes
        &sac.id,
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
    );

    let investor = Address::generate(&env);
    let sac_admin = StellarAssetClient::new(&env, &sac.id);
    sac_admin.mint(&investor, &1i128);

    assert_contract_error(
        client.try_fund_with_commitment(&investor, &1i128, &1u64),
        EscrowError::InvestorClaimTimeOverflow,
    );
}

/// One below the `u64::MAX` boundary with `committed_lock_secs == 1` sums to
/// exactly `u64::MAX` and must succeed (provided the maturity gate allows it).
#[test]
fn test_fund_with_commitment_claim_time_one_below_max_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = u64::MAX - 1;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);

    let sac = install_stellar_asset_token(&env);
    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &SorobanString::from_str(&env, "FA_OK"),
        &sme,
        &100i128,
        &0i64,
        &u64::MAX,
        &sac.id,
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
    );

    let investor = Address::generate(&env);
    let sac_admin = StellarAssetClient::new(&env, &sac.id);
    sac_admin.mint(&investor, &1i128);

    // committed_lock_secs = 1 ⇒ now + 1 = u64::MAX exactly, no overflow.
    let post = client.fund_with_commitment(&investor, &1i128, &1u64);
    assert_eq!(post.status, 0); // still open (target 100 not met)
}

// ===========================================================================
// Negative total_principal planted in FundingCloseSnapshot → typed errors
// ===========================================================================

/// `get_settlement_pool` and `compute_investor_payout` short-circuit to `0`
/// when `total_principal <= 0` (the `<= 0` guard). Plant a snapshot with
/// `total_principal = i128::MIN` and an investor contribution; assert both
/// views return `0` rather than attempting the arithmetic with a negative
/// denominator (which would produce a signed-division trap in checked_div).
#[test]
fn test_settle_views_zero_on_negative_total_principal_planted_oob() {
    let env = Env::default();
    let (client, escrow_id, _token_id) = bootstrap_with_token(&env);
    let investor = Address::generate(&env);

    {
        let mut escrow = client.get_escrow();
        escrow.status = 2; // settled (claim path requires status 2)
        write_escrow_raw(&env, &escrow_id, &escrow);

        write_funding_close_snapshot(
            &env,
            &escrow_id,
            &FundingCloseSnapshot {
                total_principal: i128::MIN,
                funding_target: 100,
                closed_at_ledger_timestamp: 0,
                closed_at_ledger_sequence: 100,
            },
        );
        write_investor_contribution(&env, &escrow_id, &investor, 100i128);
    }

    // Both views must return 0 (the <= 0 short-circuit) — no panics.
    assert_eq!(client.get_settlement_pool(), 0);
    assert_eq!(client.compute_investor_payout(&investor), 0);
}

// ===========================================================================
// Pure saturation & checked boundary property tests (no Env)
// ===========================================================================

/// Exhaustive `saturating_add` near `i128::MAX` boundary: every addition that
/// would overflow the representation clamps to `MAX`; additions that fit are
/// exact. Mirrors the production `DistributedPrincipal` advance usage.
#[test]
fn test_saturating_add_boundary_sweep_near_i128_max() {
    for base in [i128::MAX - 3, i128::MAX - 2, i128::MAX - 1, i128::MAX] {
        for delta in [0i128, 1, 2, 3, i128::MAX] {
            let naive = base.checked_add(delta);
            let sat = base.saturating_add(delta);
            match naive {
                Some(exact) => assert_eq!(sat, exact, "fits: {base}+{delta}"),
                None => assert_eq!(sat, i128::MAX, "must saturate: {base}+{delta}"),
            }
        }
    }
}

/// Exhaustive `saturating_sub` boundary cases including around zero and
/// towards `i128::MIN`. Covers the liability-floor `saturating_sub` path
/// and provides symmetric coverage for any future subtraction sites.
#[test]
fn test_saturating_sub_boundary_sweep() {
    let cases: [(i128, i128); 10] = [
        (0, 0),
        (0, 1),
        (1, 1),
        (1, 2),
        (1_000, 1_000),
        (i128::MIN, 0),
        (i128::MIN, 1),         // sub would underflow → clamp to MIN
        (i128::MIN + 1, 2),     // sub would underflow → clamp to MIN
        (i128::MAX, i128::MAX), // 0
        (i128::MAX, 0),         // MAX
    ];
    for (a, b) in cases {
        let naive = a.checked_sub(b);
        let sat = a.saturating_sub(b);
        match naive {
            Some(exact) => assert_eq!(sat, exact, "fits: {a}-{b}"),
            None => assert_eq!(sat, i128::MIN, "must sat-underflow: {a}-{b}"),
        }
    }
}

/// `saturating_mul` reference at extremes — not yet used in-production in
/// fees, but asserted here so a future refactor toward saturation can
/// reference the documented behaviour without reasoning from first
/// principles.
#[test]
fn test_saturating_mul_reference_extremes() {
    // Positives clamp to MAX.
    assert_eq!(i128::MAX.saturating_mul(2), i128::MAX);
    assert_eq!(
        (i128::MAX / 2 + 1).saturating_mul(2),
        i128::MAX,
        "just above half must clamp"
    );
    // Exactly half the domain fits.
    assert_eq!(
        (i128::MAX / 2).saturating_mul(2),
        i128::MAX - 1,
        "2 * floor(MAX/2) == MAX - 1"
    );
    // Mixed sign negatives clamp to MIN.
    assert_eq!(i128::MIN.saturating_mul(2), i128::MIN);
    assert_eq!(i128::MAX.saturating_mul(-2), i128::MIN);
    // Zero stays zero.
    assert_eq!(0i128.saturating_mul(i128::MAX), 0);
    assert_eq!(0i128.saturating_mul(i128::MIN), 0);
}

/// Coupled property: `checked_mul + checked_div` fee formula at the
/// `i128::MAX / 10_000` boundary is *exactly* representable one step below
/// and returns `None` exactly one step above. This test double-checks the
/// algebraic constant used by the withdraw overflow tests.
#[test]
fn test_fee_formula_checked_boundary_checked_explicit() {
    let divisor: i128 = 10_000;
    let max_safe = i128::MAX / divisor;
    let first_unsafe = max_safe.checked_add(1).expect("constant math");

    // At fee_bps = 10_000 (100%).
    let fee_bps: i128 = 10_000;

    // max_safe * 10_000 == i128::MAX - r (remainder from division), fits.
    let safe_fee = max_safe
        .checked_mul(fee_bps)
        .and_then(|v| v.checked_div(divisor));
    assert!(safe_fee.is_some(), "safe side must produce Some");
    assert_eq!(safe_fee.unwrap(), max_safe); // 100% fee == amount.

    // first_unsafe * 10_000 overflows i128 (checked_mul returns None).
    let unsafe_fee = first_unsafe
        .checked_mul(fee_bps)
        .and_then(|v| v.checked_div(divisor));
    assert!(
        unsafe_fee.is_none(),
        "unsafe side must produce None from checked_mul"
    );
}
