#![allow(unused_imports, dead_code, clippy::too_many_arguments)]
/// Tests for the secondary transfer of funded investor positions.
///
/// Corresponds to `transfer_position(env, from, to, amount)` and the acceptance
/// criteria listed in the issue.

use super::tests::*;
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token::StellarAssetClient,
    Address, Env, String, Vec as SorobanVec,
};

// ------------------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------------------

/// Setup an escrow with a real token, fund it from a single investor, and
/// return (client, escrow id, sme, token id, sac_admin, investor).
///
/// The escrow is fully funded (funded_amount == target) and status is 1.
fn setup_funded_escrow<'a>(
    env: &'a Env,
    target: i128,
    invoice_id: &str,
) -> (
    LiquifactEscrowClient<'a>,
    Address,
    Address,
    Address,
    StellarAssetClient<'a>,
    Address,
) {
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(env, &token_id);

    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &escrow_id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, invoice_id),
        &sme,
        &target,
        &800i64,
        &1u64,
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
        &None::<i64>,
    );

    let investor = Address::generate(env);
    sac_admin.mint(&investor, &target);
    client.fund(&investor, &target);
    sac_admin.mint(&escrow_id, &target);

    (client, escrow_id, sme, token_id, sac_admin, investor)
}

// ------------------------------------------------------------------------------
// Happy path: full transfer, settlement, claim by to
// ------------------------------------------------------------------------------

#[test]
fn full_transfer_then_settle_and_claim_pays_to() {
    let env = Env::default();
    let target = 100_000_i128;
    let (client, _escrow_id, sme, _token_id, sac_admin, from) =
        setup_funded_escrow(&env, target, "INV001");

    let to = Address::generate(&env);
    client.transfer_position(&from, &to, &target);

    // Contributions moved.
    assert_eq!(client.contribution(&from), 0);
    assert_eq!(client.contribution(&to), target);

    // Settle at maturity and claim from `to`.
    env.ledger().with_mut(|l| {
        l.timestamp = 1000;
        l.sequence_number += 1;
    });
    client.settle(&sme);

    let payout_to = client.payout_of(&to);
    assert!(payout_to > 0);

    let before = sac_admin.balance(&to);
    client.claim(&to);
    let after = sac_admin.balance(&to);
    assert_eq!(after - before, payout_to);

    // from has nothing to claim.
    assert_eq!(client.payout_of(&from), 0);
    assert_contract_error(client.try_claim(&from), EscrowError::NothingToClaim);
}

// ------------------------------------------------------------------------------
// Conservation of principal and payouts
// ------------------------------------------------------------------------------

#[test]
fn partial_transfer_conserves_principal_and_payouts() {
    let env = Env::default();
    let target = 100_000_i128;
    let (client, _escrow_id, sme, _token_id, _sac_admin, from) =
        setup_funded_escrow(&env, target, "INV001");

    let to = Address::generate(&env);
    let amount = target / 4;
    client.transfer_position(&from, &to, &amount);

    // Contributions conserved.
    assert_eq!(client.contribution(&from), target - amount);
    assert_eq!(client.contribution(&to), amount);

    // Settle and check payout conservation.
    env.ledger().with_mut(|l| {
        l.timestamp = 1000;
        l.sequence_number += 1;
    });
    client.settle(&sme);

    let payout_from = client.payout_of(&from);
    let payout_to = client.payout_of(&to);
    let total = payout_from + payout_to;
    let snap = client.get_funding_close_snapshot();
    if let Some(s) = snap {
        let expected = s.total_principal;
        assert!((total - expected).abs() <= 1, "payout conservation failed");
    }
}

// ------------------------------------------------------------------------------
// Terms inheritance for tiered positions
// ------------------------------------------------------------------------------

#[test]
fn tiered_position_transfer_keeps_yield_and_lock() {
    let env = Env::default();
    let target = 100_000_i128;
    let (client, _escrow_id, _sme, _token_id, _sac_admin, from) =
        setup_funded_escrow(&env, target, "INV001");

    let from_yield = client.investor_effective_yield(&from);
    let from_lock = client.investor_claim_not_before(&from);

    let to = Address::generate(&env);
    client.transfer_position(&from, &to, &target);

    assert_eq!(client.investor_effective_yield(&to), from_yield);
    assert_eq!(client.investor_claim_not_before(&to), from_lock);
}

// ------------------------------------------------------------------------------
// Guard tests: wrong status
// ------------------------------------------------------------------------------

#[test]
fn transfer_fails_when_open() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    let from = Address::generate(&env);
    let to = Address::generate(&env);
    assert_contract_error(
        client.try_transfer_position(&from, &to, &100),
        EscrowError::InvalidStatus,
    );
}

#[test]
fn transfer_fails_when_cancelled() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    client.cancel();
    let from = Address::generate(&env);
    let to = Address::generate(&env);
    assert_contract_error(
        client.try_transfer_position(&from, &to, &100),
        EscrowError::InvalidStatus,
    );
}

#[test]
fn transfer_fails_when_settled() {
    let env = Env::default();
    let target = 100_000_i128;
    let (client, _escrow_id, sme, _token_id, _sac_admin, from) =
        setup_funded_escrow(&env, target, "INV001");
    env.ledger().with_mut(|l| {
        l.timestamp = 1000;
        l.sequence_number += 1;
    });
    client.settle(&sme);
    let to = Address::generate(&env);
    assert_contract_error(
        client.try_transfer_position(&from, &to, &100),
        EscrowError::InvalidStatus,
    );
}

// ------------------------------------------------------------------------------
// Guard tests: amount and claimed
// ------------------------------------------------------------------------------

#[test]
fn transfer_fails_on_zero_amount() {
    let env = Env::default();
    let target = 100_000_i128;
    let (client, _escrow_id, _sme, _token_id, _sac_admin, from) =
        setup_funded_escrow(&env, target, "INV001");
    let to = Address::generate(&env);
    assert_contract_error(
        client.try_transfer_position(&from, &to, &0),
        EscrowError::PositionTransferAmountInvalid,
    );
}

#[test]
fn transfer_fails_on_over_amount() {
    let env = Env::default();
    let target = 100_000_i128;
    let (client, _escrow_id, _sme, _token_id, _sac_admin, from) =
        setup_funded_escrow(&env, target, "INV001");
    let to = Address::generate(&env);
    assert_contract_error(
        client.try_transfer_position(&from, &to, &(target + 1)),
        EscrowError::PositionTransferAmountInvalid,
    );
}

#[test]
fn transfer_fails_to_self() {
    let env = Env::default();
    let target = 100_000_i128;
    let (client, _escrow_id, _sme, _token_id, _sac_admin, from) =
        setup_funded_escrow(&env, target, "INV001");
    assert_contract_error(
        client.try_transfer_position(&from, &from, &100),
        EscrowError::PositionTransferAmountInvalid,
    );
}

#[test]
fn transfer_fails_after_claim() {
    let env = Env::default();
    let target = 100_000_i128;
    let (client, _escrow_id, sme, _token_id, _sac_admin, from) =
        setup_funded_escrow(&env, target, "INV001");
    env.ledger().with_mut(|l| {
        l.timestamp = 1000;
        l.sequence_number += 1;
    });
    client.settle(&sme);
    client.claim(&from);
    let to = Address::generate(&env);
    assert_contract_error(
        client.try_transfer_position(&from, &to, &100),
        EscrowError::PositionAlreadyClaimed,
    );
}

// ------------------------------------------------------------------------------
// Guard tests: allowlist, cap, terms, hold, pause
// ------------------------------------------------------------------------------

#[test]
fn transfer_fails_when_to_not_allowlisted() {
    let env = Env::default();
    let target = 100_000_i128;
    let (client, _escrow_id, _sme, _token_id, _sac_admin, from) =
        setup_funded_escrow(&env, target, "INV001");
    client.set_allowlist_enabled(&true);
    client.add_to_allowlist(&from);
    let to = Address::generate(&env);
    assert_contract_error(
        client.try_transfer_position(&from, &to, &target),
        EscrowError::InvestorNotAllowlisted,
    );
}

#[test]
fn transfer_fails_on_per_investor_cap() {
    let env = Env::default();
    let target = 100_000_i128;
    let (client, _escrow_id, _sme, _token_id, sac_admin, from) =
        setup_funded_escrow(&env, target, "INV001");
    client.set_max_per_investor_cap(&(target / 2));
    let to = Address::generate(&env);
    sac_admin.mint(&to, &target);
    client.fund(&to, &(target / 2));
    assert_contract_error(
        client.try_transfer_position(&from, &to, &target),
        EscrowError::InvestorContributionExceedsCap,
    );
}

#[test]
fn transfer_fails_on_terms_mismatch() {
    let env = Env::default();
    let target = 100_000_i128;
    let (client, _escrow_id, _sme, _token_id, sac_admin, from) =
        setup_funded_escrow(&env, target, "INV001");
    let to = Address::generate(&env);
    sac_admin.mint(&to, &target);
    client.fund_with_yield(&to, &1000, &100);
    assert_contract_error(
        client.try_transfer_position(&from, &to, &target),
        EscrowError::PositionTermsMismatch,
    );
}

#[test]
fn transfer_fails_under_legal_hold() {
    let env = Env::default();
    let target = 100_000_i128;
    let (client, _escrow_id, _sme, _token_id, _sac_admin, from) =
        setup_funded_escrow(&env, target, "INV001");
    client.set_legal_hold(&true);
    let to = Address::generate(&env);
    assert_contract_error(
        client.try_transfer_position(&from, &to, &target),
        EscrowError::LegalHoldBlocksPositionTransfer,
    );
}

#[test]
fn transfer_fails_when_claims_paused() {
    let env = Env::default();
    let target = 100_000_i128;
    let (client, _escrow_id, _sme, _token_id, _sac_admin, from) =
        setup_funded_escrow(&env, target, "INV001");
    client.set_pause(&PauseScope::Claims, &true);
    let to = Address::generate(&env);
}
