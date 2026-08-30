//! Settlement arithmetic boundary and property tests (issue #1228).
//!
//! The settlement math — [`crate::LiquifactEscrow::settle`] (coupon /
//! `settle_pool`), [`crate::LiquifactEscrow::compute_investor_payout`] (pro-rata
//! gross payout), [`crate::LiquifactEscrow::get_settlement_pool`], and the
//! protocol-fee split inside [`crate::LiquifactEscrow::withdraw`] — is integer
//! math on `i128` with floor division:
//!
//! ```text
//! coupon       = total_principal × bps / 10_000   (floor)
//! settle_pool  = total_principal + coupon
//! payout_i     = contribution_i × settle_pool / total_principal   (floor)
//! fee          = funded_amount × fee_bps / 10_000   (floor)
//! sme_net      = funded_amount − fee
//! ```
//!
//! Hand-picked unit tests miss interactions between rounding (floor division),
//! overflow (the `checked_*` guards), and magnitude boundaries (tokens with
//! very different decimal scales). This module closes that gap with:
//!
//! 1. **Generated inputs within supported bounds** — proptests asserting
//!    conservation (`fee + sme_net == funded_amount`, `Σ payout_i ≤ settle_pool`),
//!    monotonicity (payout non-decreasing in contribution; pool consistency with
//!    the mirror formula), and residue bounds.
//! 2. **Generated inputs outside supported bounds** — proptests asserting the
//!    typed rejection of zero / negative / over-`MAX_INVOICE_AMOUNT` amounts,
//!    out-of-range `yield_bps` / `protocol_fee_bps`, and non-positive funding.
//! 3. **Edge cases, each with a dedicated test** — zero, one unit, maximum
//!    representable, overflow attempt, and mixed decimal scales.
//!
//! # Security notes
//!
//! - The overflow attempts below reach the `checked_*` guards by *injecting
//!   storage values that the public API cannot create* (e.g. a synthetic
//!   `FundingCloseSnapshot` with `total_principal ≈ i128::MAX`). Through the
//!   public API, `init` caps `amount` at [`MAX_INVOICE_AMOUNT`], which was
//!   derived so `2 × total_principal² ≤ i128::MAX`; the injected values prove
//!   the guards themselves are total (they surface
//!   [`EscrowError::ComputePayoutArithmeticOverflow`] instead of wrapping).
//! - Conservation is asserted against **real token balance deltas** (SAC), not
//!   just returned values, so the fee split cannot "look correct" while the
//!   transfers move a different amount.
//! - No production code, storage layout, error code, or event shape is changed;
//!   this module is purely additive test coverage.
//!
//! # Placement note
//!
//! This module deliberately does **not** depend on the crate's `tests/` module tree, which is
//! currently disabled pending reconciliation with the lib API (see the note near the end of
//! `lib.rs`). It is declared from `lib.rs` as `#[cfg(test)] mod settlement_math_boundaries_tests;`
//! — the same wiring used by `settlement_guard_tests` — so the boundary coverage actually runs
//! under `cargo test` today.

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    token::{StellarAssetClient, TokenClient},
    Address, Env, Error, Event, InvokeError, String,
};

use super::{
    DataKey, EscrowError, FundingCloseSnapshot, InvoiceEscrow, LiquifactEscrow,
    LiquifactEscrowClient, SmeWithdrew, MAX_INVOICE_AMOUNT,
};
use proptest::prelude::*;

/// Assert that a `try_*` client call failed with exactly the expected typed
/// [`EscrowError`] code (mirrors the helper in `tests/mod.rs`).
pub(crate) fn assert_contract_error<T, E>(
    result: Result<Result<T, E>, Result<Error, InvokeError>>,
    expected: EscrowError,
) where
    T: core::fmt::Debug,
    E: core::fmt::Debug,
{
    let expected_code = expected as u32;
    match result {
        Err(Ok(error)) => {
            assert_eq!(error, Error::from_contract_error(expected_code));
        }
        Err(Err(InvokeError::Contract(code))) => {
            assert_eq!(code, expected_code);
        }
        other => panic!("expected ContractError({expected_code}), got {other:?}"),
    }
}

/// Deploy a fresh [`LiquifactEscrow`] contract and return its client.
fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

/// Test-only wrapper around a real Stellar asset contract (SAC) token.
struct Sac<'a> {
    id: Address,
    token: TokenClient<'a>,
    stellar: StellarAssetClient<'a>,
}

/// Install a standard Stellar asset token (SAC v2) with mint access.
fn install_sac<'a>(env: &'a Env) -> Sac<'a> {
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let id = sac.address();
    Sac {
        id: id.clone(),
        token: TokenClient::new(env, &id),
        stellar: StellarAssetClient::new(env, &id),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Mirror of the on-chain `settle_pool` formula (floor coupon).
fn settle_pool_for(total_principal: i128, yield_bps: i64) -> i128 {
    total_principal + total_principal * (yield_bps as i128) / 10_000
}

/// Mirror of the on-chain protocol-fee split at `withdraw`.
/// Returns `(fee, sme_net)` with `fee + sme_net == amount` for `0..=10_000` bps.
fn model_fee_split(amount: i128, fee_bps: i64) -> (i128, i128) {
    let fee = amount * (fee_bps as i128) / 10_000;
    (fee, amount - fee)
}

/// Deploy + `init` an escrow backed by the lazily-registered mock token
/// (dummy address; default balance `MOCK_TOKEN_DEFAULT_BALANCE`). Suitable for
/// small principals where the mock default balance is sufficient.
///
/// Returns `(client, token_addr, sme, treasury)`.
fn deploy_init_mock<'a>(
    env: &'a Env,
    invoice_id: &str,
    amount: i128,
    yield_bps: i64,
    fee_bps: Option<i64>,
) -> (LiquifactEscrowClient<'a>, Address, Address, Address) {
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &amount,
        &yield_bps,
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
    (client, token, sme, treasury)
}

/// Deploy + `init` an escrow backed by a real Stellar asset token (SAC) so
/// principals beyond the mock-token default balance (e.g. `MAX_INVOICE_AMOUNT`,
/// `10^18`-scale) can be funded and withdrawn.
///
/// Returns `(client, sac, sme, treasury)`.
fn deploy_init_sac<'a>(
    env: &'a Env,
    invoice_id: &str,
    amount: i128,
    yield_bps: i64,
    fee_bps: Option<i64>,
) -> (LiquifactEscrowClient<'a>, Sac<'a>, Address, Address) {
    env.mock_all_auths();
    let sac = install_sac(env);
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &amount,
        &yield_bps,
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
        &fee_bps,
    );
    (client, sac, sme, treasury)
}

/// Mint `amount` to `investor` on the SAC and fund the escrow with it.
fn mint_and_fund(
    client: &LiquifactEscrowClient<'_>,
    sac: &Sac<'_>,
    investor: &Address,
    amount: i128,
) {
    sac.stellar.mint(investor, &amount);
    client.fund(investor, &amount);
}

/// Deploy an escrow (mock token), fund `contributions`, and settle it.
/// `contributions` must sum to `target`.
fn funded_and_settled_mock<'a>(
    env: &'a Env,
    invoice_id: &str,
    target: i128,
    yield_bps: i64,
    contributions: &[(Address, i128)],
) -> LiquifactEscrowClient<'a> {
    let (client, _token, _sme, _treasury) =
        deploy_init_mock(env, invoice_id, target, yield_bps, None);
    for (investor, amount) in contributions {
        client.fund(investor, amount);
    }
    client.settle();
    client
}

// ─────────────────────────────────────────────────────────────────────────────
// Property tests — generated inputs within supported bounds
// ─────────────────────────────────────────────────────────────────────────────

// Conservation of the protocol-fee split across the full valid input space.
//
// Generates `funded_amount` across magnitudes (including the `1` and
// `MAX_INVOICE_AMOUNT` endpoints) and `protocol_fee_bps` across the full
// `0..=10_000` range (including both endpoints), then asserts:
// - `fee + sme_net == funded_amount` exactly (no value created or destroyed),
// - `fee ≥ 0`, `sme_net ≥ 0`, `fee ≤ funded_amount`,
// - treasury and SME token-balance deltas match the computed legs,
// - the `SmeWithdrew` event carries exactly the computed `(amount, fee)`,
// - `DistributedPrincipal` advances by the full gross `funded_amount`.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn prop_withdraw_fee_split_conservation(
        funded_amount in prop_oneof![
            1 => Just(1i128),
            1 => Just(MAX_INVOICE_AMOUNT),
            2 => 2i128..=100_000i128,
            2 => 100_001i128..=10_000_000i128,
            2 => 10_000_001i128..=MAX_INVOICE_AMOUNT,
        ],
        fee_bps in prop_oneof![
            1 => Just(0i64),
            1 => Just(10_000i64),
            3 => 1i64..=9_999i64,
        ],
    ) {
        let env = Env::default();
        let (client, sac, sme, treasury) =
            deploy_init_sac(&env, "FEECONS", funded_amount, 0i64, Some(fee_bps));

        let investor = Address::generate(&env);
        mint_and_fund(&client, &sac, &investor, funded_amount);
        prop_assert_eq!(
            client.get_escrow().status,
            1,
            "funding exactly the target must close the escrow as funded"
        );

        let treasury_before = sac.token.balance(&treasury);
        let sme_before = sac.token.balance(&sme);

        client.withdraw();

        // Capture the event log immediately: `env.events().all()` only returns the events of
        // the **last** invocation, so any later client call (e.g. the balance reads below)
        // would clobber the `SmeWithdrew` log.
        let events = env.events().all();

        let (fee_exp, net_exp) = model_fee_split(funded_amount, fee_bps);
        prop_assert!(fee_exp >= 0, "fee must never be negative");
        prop_assert!(net_exp >= 0, "sme_net must never be negative");
        prop_assert!(fee_exp <= funded_amount, "fee must never exceed funded_amount");
        prop_assert_eq!(
            fee_exp + net_exp,
            funded_amount,
            "conservation: fee + sme_net must equal funded_amount exactly"
        );

        // Real token-balance deltas match the computed legs (endpoints included):
        // fee_bps == 0  → no treasury transfer, SME receives everything.
        // fee_bps == 10_000 → SME receives nothing, treasury receives everything.
        prop_assert_eq!(
            sac.token.balance(&treasury) - treasury_before,
            fee_exp,
            "treasury balance delta must equal the computed fee"
        );
        prop_assert_eq!(
            sac.token.balance(&sme) - sme_before,
            net_exp,
            "SME balance delta must equal the computed net payout"
        );

        // The published event must carry exactly the computed legs.
        let last_event = events.events().last().unwrap().clone();
        let expected = SmeWithdrew {
            name: symbol_short!("sme_wd"),
            invoice_id: client.get_escrow().invoice_id.clone(),
            amount: net_exp,
            recipient: sme.clone(),
            fee: fee_exp,
        }
        .to_xdr(&env, &client.address);
        prop_assert_eq!(last_event, expected, "SmeWithdrew event must match the computed split");

        // Liability accounting advances by the gross amount, not the net.
        prop_assert_eq!(
            client.get_distributed_principal(),
            funded_amount,
            "DistributedPrincipal must advance by the full gross funded_amount"
        );
    }
}

// Monotonicity, no-loss, conservation, and residue bound for pro-rata payouts.
//
// One escrow with two investors contributing `c_a < c_b` (summing to `total`)
// at a generated `yield_bps`. Asserts:
// - `payout(c_a) ≤ payout(c_b)` — payout is non-decreasing in contribution,
// - `payout_i ≥ contribution_i` — floor division never loses an investor's
//   whole principal (no "penny-bleeding" of a contributor to zero),
// - `Σ payout_i ≤ settle_pool` with residue `< n_investors`,
// - `get_settlement_pool()` matches the mirror formula.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn prop_payout_monotonic_no_loss_conservation(
        total in 200i128..=100_000i128,
        frac_a in 1u32..=49u32,
        yield_bps in 0i64..=10_000i64,
    ) {
        let env = Env::default();
        let c_a = total * (frac_a as i128) / 100;
        let c_b = total - c_a;
        // `frac_a <= 49` guarantees 1 <= c_a < c_b < total.
        prop_assert!(c_a >= 1);
        prop_assert!(c_a < c_b);

        let inv_a = Address::generate(&env);
        let inv_b = Address::generate(&env);
        let client = funded_and_settled_mock(
            &env,
            "MONOPAY",
            total,
            yield_bps,
            &[(inv_a.clone(), c_a), (inv_b.clone(), c_b)],
        );

        let pa = client.compute_investor_payout(&inv_a);
        let pb = client.compute_investor_payout(&inv_b);
        let pool = settle_pool_for(total, yield_bps);

        prop_assert!(
            pa <= pb,
            "monotonicity: larger contribution must not yield a smaller payout ({pa} > {pb})"
        );
        prop_assert!(pa >= c_a, "investor A must recover at least principal");
        prop_assert!(pb >= c_b, "investor B must recover at least principal");

        let sum = pa + pb;
        prop_assert!(sum <= pool, "conservation: sum of payouts must not exceed settle_pool");
        let residue = pool - sum;
        prop_assert!(residue >= 0, "residue must be non-negative");
        prop_assert!(residue < 2, "floor division drops at most 1 unit per investor");

        // Cross-entrypoint consistency with the authoritative aggregate view.
        prop_assert_eq!(client.get_settlement_pool(), pool);
    }
}

// `settle()` and `get_settlement_pool()` must agree with the documented
// floor formula for generated `(total_principal, yield_bps)` pairs.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn prop_settle_pool_matches_mirror_formula(
        total in 1i128..=100_000i128,
        yield_bps in 0i64..=10_000i64,
    ) {
        let env = Env::default();
        let investor = Address::generate(&env);
        let (client, _token, _sme, _treasury) =
            deploy_init_mock(&env, "POOLCONS", total, yield_bps, None);
        client.fund(&investor, &total);

        let expected_pool = settle_pool_for(total, yield_bps);
        let expected_coupon = total * (yield_bps as i128) / 10_000;

        let result = client.settle();
        prop_assert_eq!(result.coupon, expected_coupon, "coupon must match floor formula");
        prop_assert_eq!(result.settle_pool, expected_pool, "settle_pool must match floor formula");
        prop_assert_eq!(
            result.coupon + result.escrow.funded_amount,
            result.settle_pool,
            "coupon + principal must equal settle_pool"
        );
        prop_assert_eq!(client.get_settlement_pool(), expected_pool);
    }
}

// Generated inputs spanning 0–18 decimal scales (`10^0` … `10^18` base units,
// mimicking tokens of very different decimal precision) must still satisfy
// conservation, no-loss, and monotonicity.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn prop_mixed_decimal_scales_payout_invariants(
        exps in proptest::collection::vec(0u32..=18u32, 2usize..=6usize),
        coeffs in proptest::collection::vec(1i128..=9i128, 2usize..=6usize),
        yield_bps in 0i64..=10_000i64,
    ) {
        let env = Env::default();
        let n = exps.len().min(coeffs.len());
        let amounts: Vec<i128> = (0..n)
            .map(|i| coeffs[i] * 10i128.pow(exps[i]))
            .collect();
        let total: i128 = amounts.iter().sum();
        // Escrow principals are bounded by MAX_INVOICE_AMOUNT; skip the rare
        // generated combinations that exceed it.
        if total <= 0 || total > MAX_INVOICE_AMOUNT {
            return Ok(());
        }

        let (client, sac, _sme, _treasury) =
            deploy_init_sac(&env, "MIXSCALE", total, yield_bps, None);
        let investors: Vec<Address> = (0..n).map(|_| Address::generate(&env)).collect();
        for i in 0..n {
            mint_and_fund(&client, &sac, &investors[i], amounts[i]);
        }
        client.settle();

        let payouts: Vec<i128> = investors
            .iter()
            .map(|inv| client.compute_investor_payout(inv))
            .collect();
        let pool = settle_pool_for(total, yield_bps);
        let sum: i128 = payouts.iter().sum();

        prop_assert!(sum <= pool, "conservation across mixed decimal scales");
        let residue = pool - sum;
        prop_assert!(residue >= 0 && residue < n as i128, "residue must be < n_investors");

        for i in 0..n {
            prop_assert!(
                payouts[i] >= amounts[i],
                "no-loss: investor {i} must recover at least principal"
            );
        }

        // Monotonicity: sorting by contribution, payouts are non-decreasing.
        let mut pairs: Vec<(i128, i128)> = amounts
            .iter()
            .cloned()
            .zip(payouts.iter().cloned())
            .collect();
        pairs.sort_by_key(|(amount, _)| *amount);
        for w in pairs.windows(2) {
            prop_assert!(w[0].1 <= w[1].1, "payout must be non-decreasing in contribution");
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property tests — generated inputs outside supported bounds (typed rejection)
// ─────────────────────────────────────────────────────────────────────────────

// Generated out-of-bounds inputs must be rejected with the stable typed
// [`EscrowError`] codes — never a raw wrap, panic string, or a different code.
//
// | Scenario | Invalid input | Expected error |
// |----------|---------------|----------------|
// | 0 | `init` amount `0` | `AmountMustBePositive` (1) |
// | 1 | `init` amount negative | `AmountMustBePositive` (1) |
// | 2 | `init` amount `> MAX_INVOICE_AMOUNT` | `AmountExceedsMax` (14) |
// | 3 | `init` `yield_bps` above `10_000` | `YieldBpsOutOfRange` (2) |
// | 4 | `init` `yield_bps` negative | `YieldBpsOutOfRange` (2) |
// | 5 | `init` `protocol_fee_bps` above `10_000` | `ProtocolFeeBpsOutOfRange` (215) |
// | 6 | `init` `protocol_fee_bps` negative | `ProtocolFeeBpsOutOfRange` (215) |
// | 7 | `fund` amount `0` | `FundingAmountNotPositive` (100) |
// | 8 | `fund` amount negative | `FundingAmountNotPositive` (100) |
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn prop_typed_rejection_of_out_of_bounds_values(
        delta in 0i128..=1_000_000i128,
        scenario in 0u32..=8u32,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let client = deploy(&env);

        let bad_i64 = (delta % 1_000_000) as i64;
        match scenario {
            0 => assert_contract_error(
                client.try_init(&admin, &String::from_str(&env, "BADOOB"), &sme, &0i128, &0i64, &0u64, &token, &None, &treasury, &None, &None, &None, &None, &None, &None, &None, &None, &None::<i64>, &None::<u32>),
                EscrowError::AmountMustBePositive,
            ),
            1 => assert_contract_error(
                client.try_init(&admin, &String::from_str(&env, "BADOOB"), &sme, &-(delta + 1), &0i64, &0u64, &token, &None, &treasury, &None, &None, &None, &None, &None, &None, &None, &None, &None::<i64>, &None::<u32>),
                EscrowError::AmountMustBePositive,
            ),
            2 => assert_contract_error(
                client.try_init(&admin, &String::from_str(&env, "BADOOB"), &sme, &(MAX_INVOICE_AMOUNT + 1 + delta), &0i64, &0u64, &token, &None, &treasury, &None, &None, &None, &None, &None, &None, &None, &None, &None::<i64>, &None::<u32>),
                EscrowError::AmountExceedsMax,
            ),
            3 => assert_contract_error(
                client.try_init(&admin, &String::from_str(&env, "BADOOB"), &sme, &1_000i128, &(10_001 + bad_i64), &0u64, &token, &None, &treasury, &None, &None, &None, &None, &None, &None, &None, &None, &None::<i64>, &None::<u32>),
                EscrowError::YieldBpsOutOfRange,
            ),
            4 => assert_contract_error(
                client.try_init(&admin, &String::from_str(&env, "BADOOB"), &sme, &1_000i128, &(-1 - bad_i64), &0u64, &token, &None, &treasury, &None, &None, &None, &None, &None, &None, &None, &None, &None::<i64>, &None::<u32>),
                EscrowError::YieldBpsOutOfRange,
            ),
            5 => assert_contract_error(
                client.try_init(&admin, &String::from_str(&env, "BADOOB"), &sme, &1_000i128, &0i64, &0u64, &token, &None, &treasury, &None, &None, &None, &None, &None, &None, &None, &None, &Some(10_001 + bad_i64)),
                EscrowError::ProtocolFeeBpsOutOfRange,
            ),
            6 => assert_contract_error(
                client.try_init(&admin, &String::from_str(&env, "BADOOB"), &sme, &1_000i128, &0i64, &0u64, &token, &None, &treasury, &None, &None, &None, &None, &None, &None, &None, &None, &Some(-1 - bad_i64)),
                EscrowError::ProtocolFeeBpsOutOfRange,
            ),
            7 => {
                client.init(&admin, &String::from_str(&env, "BADOOB"), &sme, &1_000i128, &0i64, &0u64, &token, &None, &treasury, &None, &None, &None, &None, &None, &None, &None, &None, &None::<i64>, &None::<u32>);
                let investor = Address::generate(&env);
                assert_contract_error(
                    client.try_fund(&investor, &0i128),
                    EscrowError::FundingAmountNotPositive,
                );
            }
            _ => {
                client.init(&admin, &String::from_str(&env, "BADOOB"), &sme, &1_000i128, &0i64, &0u64, &token, &None, &treasury, &None, &None, &None, &None, &None, &None, &None, &None, &None::<i64>, &None::<u32>);
                let investor = Address::generate(&env);
                assert_contract_error(
                    client.try_fund(&investor, &-(delta + 1)),
                    EscrowError::FundingAmountNotPositive,
                );
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge case — zero
// ─────────────────────────────────────────────────────────────────────────────

/// Zero-valued inputs in settlement math are handled deterministically:
/// typed rejection where the API requires positivity, and exact zero results
/// where zero is a legitimate state (no funding yet, non-participant).
#[test]
fn edge_zero_values_in_settlement_math() {
    // `init` with amount 0 → typed rejection.
    {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let client = deploy(&env);
        assert_contract_error(
            client.try_init(
                &admin,
                &String::from_str(&env, "ZERO0"),
                &sme,
                &0i128,
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
        &None::<u32>,
            ),
            EscrowError::AmountMustBePositive,
        );
    }

    // `fund` with 0 or negative → typed rejection.
    {
        let env = Env::default();
        let (client, _token, _sme, _treasury) =
            deploy_init_mock(&env, "ZERO1", 10_000i128, 0i64, None);
        let investor = Address::generate(&env);
        assert_contract_error(
            client.try_fund(&investor, &0i128),
            EscrowError::FundingAmountNotPositive,
        );
        assert_contract_error(
            client.try_fund(&investor, &-1i128),
            EscrowError::FundingAmountNotPositive,
        );
    }

    // Before any funding: settle-pool view is 0 and non-participants get 0.
    {
        let env = Env::default();
        let (client, _token, _sme, _treasury) =
            deploy_init_mock(&env, "ZERO2", 10_000i128, 0i64, None);
        assert_eq!(client.get_settlement_pool(), 0, "no snapshot → pool 0");
        assert_eq!(
            client.compute_investor_payout(&Address::generate(&env)),
            0,
            "non-participant payout is 0, never a panic"
        );
    }

    // Zero yield: settle_pool == principal and every payout == contribution.
    {
        let env = Env::default();
        let inv_a = Address::generate(&env);
        let inv_b = Address::generate(&env);
        let client = funded_and_settled_mock(
            &env,
            "ZERO3",
            10_000i128,
            0i64,
            &[(inv_a.clone(), 3_000i128), (inv_b.clone(), 7_000i128)],
        );
        assert_eq!(client.get_settlement_pool(), 10_000);
        assert_eq!(client.compute_investor_payout(&inv_a), 3_000);
        assert_eq!(client.compute_investor_payout(&inv_b), 7_000);
    }

    // Zero fee bps: fee == 0, the full principal reaches the SME, treasury
    // balance is untouched.
    {
        let env = Env::default();
        let (client, token, sme, treasury) =
            deploy_init_mock(&env, "ZERO4", 1_000_000i128, 0i64, Some(0i64));
        let investor = Address::generate(&env);
        client.fund(&investor, &1_000_000i128);
        let treasury_before = TokenClient::new(&env, &token).balance(&treasury);
        let sme_before = TokenClient::new(&env, &token).balance(&sme);

        client.withdraw();

        assert_eq!(
            TokenClient::new(&env, &token).balance(&treasury),
            treasury_before,
            "zero fee must not touch the treasury"
        );
        assert_eq!(
            TokenClient::new(&env, &token).balance(&sme),
            sme_before + 1_000_000,
            "SME receives the full principal at zero fee"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge case — one unit
// ─────────────────────────────────────────────────────────────────────────────

/// One-unit boundaries: a 1-unit target, a 1-unit contribution, and a 1-unit
/// contributor inside a large pool. The 1-unit contributor must always recover
/// at least their single unit (floor division must not round it to zero).
#[test]
fn edge_one_unit_boundaries() {
    // Target of exactly one unit, zero yield: pool == 1, payout == 1.
    {
        let env = Env::default();
        let investor = Address::generate(&env);
        let (client, _token, _sme, _treasury) = deploy_init_mock(&env, "ONEU0", 1i128, 0i64, None);
        client.fund(&investor, &1i128);
        let result = client.settle();
        assert_eq!(result.settle_pool, 1, "one unit at zero yield");
        assert_eq!(client.compute_investor_payout(&investor), 1);
    }

    // Target of exactly one unit at 100% yield: pool == 2, payout == 2.
    {
        let env = Env::default();
        let investor = Address::generate(&env);
        let (client, _token, _sme, _treasury) =
            deploy_init_mock(&env, "ONEU1", 1i128, 10_000i64, None);
        client.fund(&investor, &1i128);
        let result = client.settle();
        assert_eq!(result.settle_pool, 2, "one unit at 100% yield doubles");
        assert_eq!(client.compute_investor_payout(&investor), 2);
    }

    // 1-unit contributor in a 10_000-unit pool at 500 bps: payout is exactly 1
    // (never rounded to zero) and aggregate conservation holds with residue < 2.
    {
        let env = Env::default();
        let inv_a = Address::generate(&env);
        let inv_b = Address::generate(&env);
        let client = funded_and_settled_mock(
            &env,
            "ONEU2",
            10_000i128,
            500i64,
            &[(inv_a.clone(), 1i128), (inv_b.clone(), 9_999i128)],
        );
        // pool = 10_000 + 500 = 10_500
        let pa = client.compute_investor_payout(&inv_a);
        let pb = client.compute_investor_payout(&inv_b);
        assert_eq!(pa, 1, "single unit must survive floor division");
        assert_eq!(pb, 10_498, "floor(9_999 * 10_500 / 10_000)");
        assert_eq!(pa + pb, 10_499, "conservation");
        assert_eq!(10_500 - (pa + pb), 1, "residue < n_investors");
    }

    // 1-unit contributor inside a 10^7-unit (7-decimal token) pool.
    {
        let env = Env::default();
        let inv_a = Address::generate(&env);
        let inv_b = Address::generate(&env);
        let client = funded_and_settled_mock(
            &env,
            "ONEU3",
            10_000_000i128,
            800i64,
            &[(inv_a.clone(), 1i128), (inv_b.clone(), 9_999_999i128)],
        );
        // pool = 10_000_000 + 800_000 = 10_800_000
        let pa = client.compute_investor_payout(&inv_a);
        let pb = client.compute_investor_payout(&inv_b);
        assert_eq!(pa, 1, "1 unit in a 10^7 pool still pays its unit back");
        assert!(pa + pb <= 10_800_000);
        assert!(pa >= 1 && pb >= 9_999_999, "no investor loses principal");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge case — maximum representable
// ─────────────────────────────────────────────────────────────────────────────

/// Maximum representable values: `MAX_INVOICE_AMOUNT` (≈ 9.22 × 10^18) is the
/// tightest bound that keeps `contribution × settle_pool` overflow-free; the
/// largest possible settle pool is `2 × MAX_INVOICE_AMOUNT`, and the fee split
/// conserves value exactly at the boundary.
#[test]
fn edge_maximum_representable_values() {
    // Single investor at MAX_INVOICE_AMOUNT, 100% yield:
    // settle_pool == 2 * MAX_INVOICE_AMOUNT, payout == settle_pool.
    {
        let env = Env::default();
        let investor = Address::generate(&env);
        let (client, sac, _sme, _treasury) =
            deploy_init_sac(&env, "MAXREP", MAX_INVOICE_AMOUNT, 10_000i64, None);
        mint_and_fund(&client, &sac, &investor, MAX_INVOICE_AMOUNT);

        let result = client.settle();
        let max_pool = MAX_INVOICE_AMOUNT
            .checked_mul(2)
            .expect("2 * MAX_INVOICE_AMOUNT fits in i128 by construction");
        assert_eq!(result.settle_pool, max_pool);
        assert_eq!(client.get_settlement_pool(), max_pool);
        assert_eq!(client.compute_investor_payout(&investor), max_pool);
    }

    // Fee split at the boundary with a non-trivial rate: conservation exact,
    // treasury and SME deltas match the computed legs.
    {
        let env = Env::default();
        let investor = Address::generate(&env);
        let fee_bps = 3_333i64;
        let (client, sac, sme, treasury) =
            deploy_init_sac(&env, "MAXFEE", MAX_INVOICE_AMOUNT, 0i64, Some(fee_bps));
        mint_and_fund(&client, &sac, &investor, MAX_INVOICE_AMOUNT);

        let (fee_exp, net_exp) = model_fee_split(MAX_INVOICE_AMOUNT, fee_bps);
        let treasury_before = sac.token.balance(&treasury);
        let sme_before = sac.token.balance(&sme);

        client.withdraw();

        assert_eq!(
            fee_exp + net_exp,
            MAX_INVOICE_AMOUNT,
            "fee + net must conserve the principal exactly at the max boundary"
        );
        assert_eq!(sac.token.balance(&treasury) - treasury_before, fee_exp);
        assert_eq!(sac.token.balance(&sme) - sme_before, net_exp);
        assert_eq!(client.get_distributed_principal(), MAX_INVOICE_AMOUNT);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge case — overflow attempt
// ─────────────────────────────────────────────────────────────────────────────

/// Overflow attempts must surface the stable typed [`EscrowError`] codes via the
/// `checked_*` guards rather than wrapping.
///
/// The public API cannot produce these states — `init` caps principal at
/// [`MAX_INVOICE_AMOUNT]`, which was derived to keep the payout multiplication
/// overflow-free — so the snapshots/escrow state are injected directly into
/// storage to prove the guards themselves are total.
#[test]
fn edge_overflow_attempt_typed_rejection() {
    // (a) coupon computation: total_principal * yield_bps overflows i128.
    {
        let env = Env::default();
        let investor = Address::generate(&env);
        let (client, _token, _sme, _treasury) =
            deploy_init_mock(&env, "OVF129A", 1_000i128, 0i64, None);
        client.fund(&investor, &1_000i128);
        client.settle();

        env.as_contract(&client.address, || {
            let mut snap: FundingCloseSnapshot = env
                .storage()
                .instance()
                .get(&DataKey::FundingCloseSnapshot)
                .unwrap();
            snap.total_principal = i128::MAX;
            env.storage()
                .instance()
                .set(&DataKey::FundingCloseSnapshot, &snap);

            let mut escrow: InvoiceEscrow = env.storage().instance().get(&DataKey::Escrow).unwrap();
            escrow.yield_bps = 10_000;
            env.storage().instance().set(&DataKey::Escrow, &escrow);
        });

        assert_contract_error(
            client.try_compute_investor_payout(&investor),
            EscrowError::ComputePayoutArithmeticOverflow,
        );
        assert_contract_error(
            client.try_get_settlement_pool(),
            EscrowError::ComputePayoutArithmeticOverflow,
        );
    }

    // (b) pro-rata product: contribution × settle_pool overflows i128 even
    // though the coupon and settle_pool individually fit.
    {
        let env = Env::default();
        let investor = Address::generate(&env);
        let (client, _token, _sme, _treasury) =
            deploy_init_mock(&env, "OVF129B", 1_000i128, 0i64, None);
        client.fund(&investor, &1_000i128);
        client.settle();

        let huge = i128::MAX / 2;
        env.as_contract(&client.address, || {
            let mut snap: FundingCloseSnapshot = env
                .storage()
                .instance()
                .get(&DataKey::FundingCloseSnapshot)
                .unwrap();
            snap.total_principal = huge;
            env.storage()
                .instance()
                .set(&DataKey::FundingCloseSnapshot, &snap);

            let mut escrow: InvoiceEscrow = env.storage().instance().get(&DataKey::Escrow).unwrap();
            escrow.yield_bps = 1; // coupon = huge / 10_000 fits; settle_pool fits.
            env.storage().instance().set(&DataKey::Escrow, &escrow);

            env.storage()
                .persistent()
                .set(&DataKey::InvestorEffectiveYield(investor.clone()), &1i64);
            env.storage()
                .persistent()
                .set(&DataKey::InvestorContribution(investor.clone()), &huge);
        });

        assert_contract_error(
            client.try_compute_investor_payout(&investor),
            EscrowError::ComputePayoutArithmeticOverflow,
        );
    }

    // (c) withdraw fee path: funded_amount × fee_bps overflows i128.
    {
        let env = Env::default();
        let investor = Address::generate(&env);
        let (client, _token, _sme, _treasury) =
            deploy_init_mock(&env, "OVF216", 1_000i128, 0i64, None);
        client.fund(&investor, &1_000i128);

        env.as_contract(&client.address, || {
            let mut escrow: InvoiceEscrow = env.storage().instance().get(&DataKey::Escrow).unwrap();
            escrow.funded_amount = i128::MAX;
            escrow.status = 1;
            env.storage().instance().set(&DataKey::Escrow, &escrow);
            env.storage()
                .instance()
                .set(&DataKey::ProtocolFeeBps, &10_000i64);
        });

        assert_contract_error(
            client.try_withdraw(),
            EscrowError::WithdrawFeeArithmeticOverflow,
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge case — mixed decimal scales
// ─────────────────────────────────────────────────────────────────────────────

/// A single escrow whose contributions span 0–18 decimal scales
/// (`1` unit through `10^18` units, e.g. USDC/XLM 7 dp vs. an 18 dp token) must
/// still conserve value and never round a whole unit away from any investor.
#[test]
fn edge_mixed_decimal_scales_conservation() {
    let env = Env::default();
    // Contributions in strictly increasing base-unit magnitudes.
    let amounts = [
        1i128,                         // 0 dp
        1_000i128,                     // 3 dp
        10_000_000i128,                // 7 dp (XLM/USDC-like)
        10_000_000_000_000i128,        // 13 dp
        3_000_000_000_000_000i128,     // 15 dp
        1_000_000_000_000_000_000i128, // 18 dp
    ];
    let total: i128 = amounts.iter().sum();
    assert!(
        total <= MAX_INVOICE_AMOUNT,
        "test principal must fit the invoice bound"
    );
    let yield_bps = 800i64;

    let investors: Vec<Address> = (0..amounts.len())
        .map(|_| Address::generate(&env))
        .collect();
    let (client, sac, _sme, _treasury) = deploy_init_sac(&env, "MIXEDEC", total, yield_bps, None);
    for i in 0..amounts.len() {
        mint_and_fund(&client, &sac, &investors[i], amounts[i]);
    }
    client.settle();

    let pool = settle_pool_for(total, yield_bps);
    assert_eq!(client.get_settlement_pool(), pool);

    let payouts: Vec<i128> = investors
        .iter()
        .map(|inv| client.compute_investor_payout(inv))
        .collect();
    let sum: i128 = payouts.iter().sum();

    // Conservation with bounded residue.
    assert!(sum <= pool, "sum of payouts must not exceed settle_pool");
    assert!(
        pool - sum < amounts.len() as i128,
        "residue must be < n_investors across decimal scales"
    );

    // No investor loses a whole unit, including the 1-unit contributor.
    for i in 0..amounts.len() {
        assert!(
            payouts[i] >= amounts[i],
            "investor {i} must recover principal"
        );
    }
    assert_eq!(payouts[0], 1, "the 1-unit investor keeps their unit");

    // Strictly increasing contributions ⇒ strictly increasing payouts.
    let mut pairs: Vec<(i128, i128)> = amounts
        .iter()
        .cloned()
        .zip(payouts.iter().cloned())
        .collect();
    pairs.sort_by_key(|(amount, _)| *amount);
    for w in pairs.windows(2) {
        assert!(
            w[0].1 < w[1].1,
            "distinct contributions must give distinct payouts"
        );
    }
}
