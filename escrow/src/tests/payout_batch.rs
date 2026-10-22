use super::{assert_contract_error, deploy_with_id, install_stellar_asset_token};
use crate::{EscrowError, LiquifactEscrowClient};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, String, Symbol, Vec as SorobanVec,
};

fn settled_escrow<'a>(
    env: &'a Env,
    contributions: &[(i128, u64)],
) -> (
    LiquifactEscrowClient<'a>,
    super::StellarTestToken<'a>,
    Address,
    std::vec::Vec<Address>,
) {
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let token = install_stellar_asset_token(env);
    let (contract_id, client) = deploy_with_id(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);
    let target = contributions.iter().map(|(amount, _)| amount).sum();

    client.init(
        &admin,
        &String::from_str(env, "PAYBATCH"),
        &sme,
        &target,
        &400i64,
        &0u64,
        &token.id,
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

    let mut investors = std::vec::Vec::new();
    let mut yield_total = 0i128;
    for (amount, lock_secs) in contributions {
        let investor = Address::generate(env);
        token.stellar.mint(&investor, amount);
        if *lock_secs == 0 {
            client.fund(&investor, amount);
        } else {
            client.fund_with_commitment(&investor, amount, lock_secs);
        }
        yield_total += *amount * 400 / 10_000;
        investors.push(investor);
    }

    token.stellar.mint(&contract_id, &yield_total);
    client.settle();

    (client, token, contract_id, investors)
}

fn claim_event_count(env: &Env) -> u32 {
    env.events()
        .all()
        .events()
        .iter()
        .filter(|event| {
            let topic: Symbol = event
                .topics()
                .get(0)
                .unwrap()
                .try_into_val(env)
                .unwrap();
            topic == symbol_short!("inv_claim")
        })
        .count() as u32
}

#[test]
fn push_pays_each_investor_and_is_idempotent() {
    let env = Env::default();
    let (client, token, _contract_id, investors) =
        settled_escrow(&env, &[(1_000, 0), (2_000, 0), (3_000, 0)]);
    let mut expected_total = 0i128;
    for investor in &investors {
        expected_total += client.compute_investor_payout(investor);
    }
    let batch = SorobanVec::from_array(
        &env,
        [
            investors[0].clone(),
            investors[1].clone(),
            investors[2].clone(),
        ],
    );

    env.mock_auths(&[]);
    assert_eq!(client.push_investor_payouts(&batch), (3, expected_total));
    for investor in &investors {
        assert!(client.is_investor_claimed(investor));
        assert_eq!(
            token.token.balance(investor),
            client.compute_investor_payout(investor)
        );
    }
    assert_eq!(claim_event_count(&env), 3);

    assert_eq!(client.push_investor_payouts(&batch), (0, 0));
    assert_eq!(claim_event_count(&env), 3);
}

#[test]
fn push_skips_unexpired_commitment_then_pays_after_expiry() {
    let env = Env::default();
    let (client, token, _contract_id, investors) = settled_escrow(&env, &[(1_000, 100)]);
    let batch = SorobanVec::from_array(&env, [investors[0].clone()]);

    env.ledger().set_timestamp(1_099);
    assert_eq!(client.push_investor_payouts(&batch), (0, 0));
    assert_eq!(token.token.balance(&investors[0]), 0);
    assert!(!client.is_investor_claimed(&investors[0]));

    env.ledger().set_timestamp(1_100);
    let expected = client.compute_investor_payout(&investors[0]);
    assert_eq!(client.push_investor_payouts(&batch), (1, expected));
    assert_eq!(token.token.balance(&investors[0]), expected);
}

#[test]
fn push_rejects_duplicates_and_global_claim_gates() {
    let env = Env::default();
    let (client, _token, _contract_id, investors) =
        settled_escrow(&env, &[(1_000, 0)]);
    let duplicate_batch =
        SorobanVec::from_array(&env, [investors[0].clone(), investors[0].clone()]);
    assert_contract_error(
        client.try_push_investor_payouts(&duplicate_batch),
        EscrowError::ClaimPushBatchDuplicate,
    );

    let batch = SorobanVec::from_array(&env, [investors[0].clone()]);
    client.set_paused(
        &true,
        &crate::PauseScope::Claims,
        &crate::PauseReason::Incident,
    );
    assert_contract_error(
        client.try_push_investor_payouts(&batch),
        EscrowError::PausedBlocksInvestorClaims,
    );
    client.set_paused(
        &false,
        &crate::PauseScope::Claims,
        &crate::PauseReason::Incident,
    );

    client.set_legal_hold(&true);
    assert_contract_error(
        client.try_push_investor_payouts(&batch),
        EscrowError::LegalHoldBlocksInvestorClaims,
    );
}

#[test]
fn push_requires_settled_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let (_contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "PAYOPEN"),
        &sme,
        &1_000i128,
        &400i64,
        &0u64,
        &token.id,
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
    let batch = SorobanVec::from_array(&env, [investor]);
    assert_contract_error(
        client.try_push_investor_payouts(&batch),
        EscrowError::InvestorClaimNotSettled,
    );
}
