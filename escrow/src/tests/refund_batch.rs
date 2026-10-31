use super::*;
use crate::{RefundBatchProcessed, MAX_REFUND_BATCH};
use soroban_sdk::{
    testutils::{EnvTestConfig, MockAuth, MockAuthInvoke},
    IntoVal,
};

fn env() -> Env {
    // These tests assert live state and events, not diagnostic snapshot files.
    Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    })
}

fn funded<'a>(
    env: &'a Env,
    amounts: &[i128],
) -> (
    LiquifactEscrowClient<'a>,
    StellarTestToken<'a>,
    SorobanVec<Address>,
    Address,
) {
    let (client, admin, sme) = setup(env);
    let token = install_stellar_asset_token(env);
    let treasury = Address::generate(env);
    let target = amounts.iter().sum::<i128>() + 1;
    client.init(
        &admin,
        &String::from_str(env, "REFUND_BATCH"),
        &sme,
        &target,
        &800,
        &0,
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
    let mut investors = SorobanVec::new(env);
    for amount in amounts {
        let investor = Address::generate(env);
        token.stellar.mint(&investor, amount);
        client.fund(&investor, amount);
        investors.push_back(investor);
    }
    (client, token, investors, treasury)
}

#[test]
fn unrelated_caller_refunds_three_investors_without_their_signatures() {
    let env = env();
    let (client, token, investors, _) = funded(&env, &[100, 200, 300]);
    client.cancel_funding();
    let operator = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &operator,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "refund_batch",
            args: (investors.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = client.refund_batch(&investors);
    assert_eq!(result, (3, 600));
    assert!(env.auths().is_empty());
    for (i, amount) in [100, 200, 300].iter().enumerate() {
        let investor = investors.get(i as u32).unwrap();
        assert_eq!(token.token.balance(&investor), *amount);
        assert_eq!(client.get_contribution(&investor), 0);
        assert!(client.is_investor_refunded(&investor));
    }
    assert_eq!(token.token.balance(&operator), 0);
    assert_eq!(token.token.balance(&client.address), 0);
    assert_eq!(client.get_distributed_principal(), 600);
}

#[test]
fn mixed_entries_skip_refunded_and_zero_contributions_and_report_events() {
    let env = env();
    let (client, token, investors, _) = funded(&env, &[100, 200, 300]);
    client.cancel_funding();
    client.refund(&investors.get(0).unwrap());
    let zero = Address::generate(&env);
    let mut batch = investors.clone();
    batch.insert(1, zero.clone());
    env.mock_auths(&[]);
    assert_eq!(client.refund_batch(&batch), (2, 500));
    let events = env.events().all();
    let invoice_id = client.get_escrow().invoice_id;
    let mut expected = std::vec::Vec::new();
    for (i, amount) in [(1, 200), (2, 300)] {
        expected.push(
            InvestorRefundedEvt {
                name: symbol_short!("refunded"),
                investor: investors.get(i).unwrap(),
                invoice_id: invoice_id.clone(),
                amount,
            }
            .to_xdr(&env, &client.address),
        );
    }
    expected.push(
        RefundBatchProcessed {
            name: symbol_short!("refund_bt"),
            invoice_id,
            count: 2,
            total: 500,
        }
        .to_xdr(&env, &client.address),
    );
    assert_eq!(events.filter_by_contract(&client.address), expected);
    assert_eq!(client.get_distributed_principal(), 600);
    assert!(!client.is_investor_refunded(&zero));
    assert_eq!(token.token.balance(&zero), 0);
    assert_eq!(client.refund_batch(&batch), (0, 0));
    assert_eq!(client.get_distributed_principal(), 600);
}

#[test]
fn duplicates_reject_atomically_even_when_the_duplicate_would_be_skipped() {
    for skip in [false, true] {
        let env = env();
        let (client, token, investors, _) = funded(&env, &[100, 200]);
        client.cancel_funding();
        let duplicate = if skip {
            Address::generate(&env)
        } else {
            investors.get(1).unwrap()
        };
        let batch = SorobanVec::from_array(
            &env,
            [investors.get(0).unwrap(), duplicate.clone(), duplicate],
        );
        env.mock_auths(&[]);
        assert_contract_error(
            client.try_refund_batch(&batch),
            EscrowError::RefundBatchDuplicateInvestor,
        );
        assert_eq!(client.get_distributed_principal(), 0);
        assert_eq!(token.token.balance(&client.address), 300);
        for (i, amount) in [(0, 100), (1, 200)] {
            let investor = investors.get(i).unwrap();
            assert_eq!(client.get_contribution(&investor), amount);
            assert_eq!(token.token.balance(&investor), 0);
            assert!(!client.is_investor_refunded(&investor));
        }
    }
}

#[test]
fn empty_and_oversize_errors_precede_per_entry_checks() {
    let env = env();
    let (client, _, investors, _) = funded(&env, &[100]);
    env.mock_auths(&[]);
    assert_contract_error(
        client.try_refund_batch(&SorobanVec::new(&env)),
        EscrowError::RefundBatchEmpty,
    );
    let mut oversize = SorobanVec::new(&env);
    for _ in 0..=MAX_REFUND_BATCH {
        oversize.push_back(investors.get(0).unwrap());
    }
    assert_contract_error(
        client.try_refund_batch(&oversize),
        EscrowError::RefundBatchTooLarge,
    );
}

#[test]
fn maximum_batch_is_accepted() {
    let env = env();
    // Like the existing MAX_FUND_BATCH test, check the functional API bound.
    // Fifty real token transfers exceed default network footprint/event limits;
    // the other regressions retain normal metering for realistic smaller batches.
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let (client, token, investors, _) = funded(&env, &[1; MAX_REFUND_BATCH as usize]);
    client.cancel_funding();
    env.mock_auths(&[]);
    assert_eq!(
        client.refund_batch(&investors),
        (MAX_REFUND_BATCH, i128::from(MAX_REFUND_BATCH))
    );
    assert_eq!(token.token.balance(&client.address), 0);
}

#[test]
fn partial_batch_preserves_sweep_floor_then_full_batch_releases_it() {
    let env = env();
    let (client, token, investors, treasury) = funded(&env, &[100, 200, 300]);
    client.cancel_funding();
    token.stellar.mint(&client.address, &50);
    env.mock_auths(&[]);
    let first = SorobanVec::from_array(&env, [investors.get(0).unwrap()]);
    assert_eq!(client.refund_batch(&first), (1, 100));
    let position = client.get_reconciliation();
    assert_eq!(position.token_balance, 550);
    assert_eq!(position.outstanding_liability, 500);
    assert_eq!(position.surplus, 50);
    env.mock_all_auths();
    assert_contract_error(
        client.try_sweep_terminal_dust(&51),
        EscrowError::SweepExceedsLiabilityFloor,
    );
    assert_eq!(client.sweep_terminal_dust(&50), 50);
    assert_eq!(token.token.balance(&treasury), 50);
    env.mock_auths(&[]);
    assert_eq!(client.refund_batch(&investors), (2, 500));
    assert_eq!(client.get_distributed_principal(), 600);
    assert_eq!(client.get_reconciliation().outstanding_liability, 0);
    env.mock_all_auths();
    token.stellar.mint(&client.address, &10);
    assert_eq!(client.sweep_terminal_dust(&10), 10);
}

#[test]
fn single_refund_keeps_auth_and_zero_contribution_errors() {
    let env = env();
    let (client, token, investors, _) = funded(&env, &[100]);
    client.cancel_funding();
    let investor = investors.get(0).unwrap();
    env.mock_auths(&[]);
    assert!(client.try_refund(&investor).is_err());
    assert_eq!(client.get_contribution(&investor), 100);
    assert_eq!(token.token.balance(&client.address), 100);
    env.mock_all_auths();
    client.refund(&investor);
    assert_contract_error(
        client.try_refund(&investor),
        EscrowError::NoContributionToRefund,
    );
}

#[test]
fn batch_still_requires_cancelled_status() {
    let env = env();
    let (client, token, investors, _) = funded(&env, &[100]);
    env.mock_auths(&[]);
    assert_contract_error(
        client.try_refund_batch(&investors),
        EscrowError::RefundNotCancelled,
    );
    assert_eq!(client.get_contribution(&investors.get(0).unwrap()), 100);
    assert_eq!(token.token.balance(&client.address), 100);
}
