use super::{deploy, deploy_with_id, install_stellar_asset_token, setup, StellarTestToken, TARGET};
use crate::{
    LiquifactEscrow, LiquifactEscrowClient, YieldTier, MAX_ATTESTATION_REVOKE_BATCH,
    MAX_INVESTOR_READ_BATCH,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    Address, Env, String as SorobanString, Vec as SorobanVec,
};

// ──────────────────────────────────────────────────────────────────────────────
// Helpers — uses real StellarAssetContract so fund() can transfer tokens
// ──────────────────────────────────────────────────────────────────────────────

/// Set up an escrow backed by a real SAC, mint `TARGET` to investor, fund to target,
/// and mint extra tokens into the escrow for settle/withdraw flows.
fn setup_funded<'a>(
    env: &'a Env,
) -> (
    LiquifactEscrowClient<'a>,
    Address,
    StellarTestToken<'a>,
    Address,
    Address,
) {
    env.mock_all_auths();
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
        &SorobanString::from_str(env, "COVFC01"),
        &sme,
        &TARGET,
        &800i64,
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
        &None::<i64>,
    );

    let investor = Address::generate(env);
    sac_admin.mint(&investor, &TARGET);
    sac_admin.mint(&escrow_id, &(TARGET * 2));
    client.fund(&investor, &TARGET);

    (
        client,
        admin,
        StellarTestToken {
            id: token_id.clone(),
            token: soroban_sdk::token::TokenClient::new(env, &token_id),
            stellar: sac_admin,
        },
        sme,
        treasury,
    )
}

/// Set up an escrow backed by a real SAC with protocol fee, fund to target,
/// mint extra tokens for withdraw.
fn setup_funded_with_fee<'a>(
    env: &'a Env,
    fee_bps: i64,
) -> (
    LiquifactEscrowClient<'a>,
    Address,
    Address,
    StellarTestToken<'a>,
    Address,
) {
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 0;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);
    env.mock_all_auths();

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
        &SorobanString::from_str(env, "COVFF01"),
        &sme,
        &TARGET,
        &800i64,
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

    let investor = Address::generate(env);
    sac_admin.mint(&investor, &TARGET);
    sac_admin.mint(&escrow_id, &(TARGET * 2));
    client.fund(&investor, &TARGET);

    (
        client,
        admin,
        sme,
        StellarTestToken {
            id: token_id.clone(),
            token: soroban_sdk::token::TokenClient::new(env, &token_id),
            stellar: sac_admin,
        },
        treasury,
    )
}

/// Set up an escrow with a real SAC but do NOT fund it (read-only helpers only).
fn setup_unfunded<'a>(env: &'a Env) -> (LiquifactEscrowClient<'a>, StellarTestToken<'a>) {
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 0;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);
    env.mock_all_auths();

    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token_id = sac.address();

    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &escrow_id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &SorobanString::from_str(env, "COVUF01"),
        &sme,
        &TARGET,
        &800i64,
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
        &None::<i64>,
    );

    (
        client,
        StellarTestToken {
            id: token_id.clone(),
            token: soroban_sdk::token::TokenClient::new(env, &token_id),
            stellar: StellarAssetClient::new(env, &token_id),
        },
    )
}

fn settle_escrow(client: &LiquifactEscrowClient<'_>, env: &Env) {
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 1000;
    env.ledger().set(ledger_info);
    client.settle();
}

// ──────────────────────────────────────────────────────────────────────────────
// get_token_balance
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn get_token_balance_returns_live_balance_after_init() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    assert_eq!(client.get_token_balance(), 0);
}

#[test]
fn get_token_balance_reflects_deposits() {
    let env = Env::default();
    let (client, _token_admin, _token, _sme, _treasury) = setup_funded(&env);
    let balance = client.get_token_balance();
    assert_eq!(balance, TARGET * 3);
}

// ──────────────────────────────────────────────────────────────────────────────
// get_protocol_fee_bps
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn get_protocol_fee_bps_defaults_to_zero() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

#[test]
fn get_protocol_fee_bps_returns_configured_value() {
    let env = Env::default();
    let (client, _admin, _sme, _token, _treasury) = setup_funded_with_fee(&env, 500);
    assert_eq!(client.get_protocol_fee_bps(), 500);
}

// ──────────────────────────────────────────────────────────────────────────────
// get_contributions (batch read)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn get_contributions_returns_zero_for_unknown() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let mut addrs = SorobanVec::new(&env);
    addrs.push_back(inv_a);
    addrs.push_back(inv_b);
    let result = client.get_contributions(&addrs);
    assert_eq!(result.len(), 2);
    assert_eq!(result.get(0).unwrap(), 0);
    assert_eq!(result.get(1).unwrap(), 0);
}

#[test]
fn get_contributions_returns_recorded_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let half = TARGET / 2;

    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &token_id);
    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &SorobanString::from_str(&env, "COVCB02"),
        &sme,
        &TARGET,
        &800i64,
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
        &None::<i64>,
    );
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    sac_admin.mint(&inv_a, &half);
    sac_admin.mint(&inv_b, &(TARGET - half));
    sac_admin.mint(&escrow_id, &(TARGET * 2));
    client.fund(&inv_a, &half);
    client.fund(&inv_b, &(TARGET - half));

    let mut addrs = SorobanVec::new(&env);
    addrs.push_back(inv_a);
    addrs.push_back(inv_b);
    let result = client.get_contributions(&addrs);
    assert_eq!(result.len(), 2);
    assert_eq!(result.get(0).unwrap(), half);
    assert_eq!(result.get(1).unwrap(), TARGET - half);
}

#[test]
fn get_contributions_empty_input() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let addrs: soroban_sdk::Vec<Address> = SorobanVec::new(&env);
    let result = client.get_contributions(&addrs);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_contributions_rejects_oversized_batch() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let mut addrs: soroban_sdk::Vec<Address> = SorobanVec::new(&env);
    for _ in 0..=MAX_INVESTOR_READ_BATCH {
        addrs.push_back(Address::generate(&env));
    }
    assert!(client.try_get_contributions(&addrs).is_err());
}

// ──────────────────────────────────────────────────────────────────────────────
// get_settlement_pool
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn get_settlement_pool_returns_zero_before_funding() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    assert_eq!(client.get_settlement_pool(), 0);
}

#[test]
fn get_settlement_pool_computes_correctly() {
    let env = Env::default();
    let (client, _admin, _token, _sme, _treasury) = setup_funded(&env);
    let pool = client.get_settlement_pool();
    assert_eq!(pool, TARGET + (TARGET * 800 / 10_000));
}

#[test]
fn get_settlement_pool_zero_yield() {
    let env = Env::default();
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 0;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);
    env.mock_all_auths();

    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &token_id);

    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let target: i128 = 500;
    client.init(
        &admin,
        &SorobanString::from_str(&env, "COVPOOL"),
        &sme,
        &target,
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
        &None::<i64>,
    );
    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &target);
    sac_admin.mint(&escrow_id, &(target * 2));
    client.fund(&investor, &target);
    assert_eq!(client.get_settlement_pool(), target);
}

// ──────────────────────────────────────────────────────────────────────────────
// get_claimable_payout
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn get_claimable_payout_zero_when_not_settled() {
    let env = Env::default();
    let (client, _admin, _token, _sme, _treasury) = setup_funded(&env);
    let investor = Address::generate(&env);
    assert_eq!(client.get_claimable_payout(&investor), 0);
}

#[test]
fn get_claimable_payout_returns_gross_after_settle() {
    let env = Env::default();
    env.mock_all_auths();
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &token_id);
    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &SorobanString::from_str(&env, "COVCP02"),
        &sme,
        &TARGET,
        &800i64,
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
        &None::<i64>,
    );
    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &TARGET);
    sac_admin.mint(&escrow_id, &(TARGET * 2));
    client.fund(&inv, &TARGET);
    settle_escrow(&client, &env);
    let payout = client.get_claimable_payout(&inv);
    assert!(payout > 0);
    assert_eq!(payout, TARGET + (TARGET * 800 / 10_000));
}

#[test]
fn get_claimable_payout_zero_when_legal_hold() {
    let env = Env::default();
    env.mock_all_auths();
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &token_id);
    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &SorobanString::from_str(&env, "COVCP03"),
        &sme,
        &TARGET,
        &800i64,
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
        &None::<i64>,
    );
    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &TARGET);
    sac_admin.mint(&escrow_id, &(TARGET * 2));
    client.fund(&inv, &TARGET);
    settle_escrow(&client, &env);
    client.set_legal_hold(&true);
    assert_eq!(client.get_claimable_payout(&inv), 0);
}

#[test]
fn get_claimable_payout_zero_for_non_participant() {
    let env = Env::default();
    let (client, _admin, _token, _sme, _treasury) = setup_funded(&env);
    settle_escrow(&client, &env);
    let stranger = Address::generate(&env);
    assert_eq!(client.get_claimable_payout(&stranger), 0);
}

// ──────────────────────────────────────────────────────────────────────────────
// compute_investor_payout guards (lines 4634, 4639)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn compute_investor_payout_zero_when_no_snapshot() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let inv = Address::generate(&env);
    assert_eq!(client.compute_investor_payout(&inv), 0);
}

#[test]
fn compute_investor_payout_zero_for_unknown_investor_after_settle() {
    let env = Env::default();
    let (client, _admin, _token, _sme, _treasury) = setup_funded(&env);
    settle_escrow(&client, &env);
    let stranger = Address::generate(&env);
    assert_eq!(client.compute_investor_payout(&stranger), 0);
}

// ──────────────────────────────────────────────────────────────────────────────
// get_allowlisted_investors / get_allowlisted_investors_count
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn cov_allowlisted_count_zero_by_default() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    assert_eq!(client.get_allowlisted_investors_count(), 0);
}

#[test]
fn cov_allowlisted_count_reflects_additions() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    client.set_investor_allowlisted(&inv_a, &true);
    client.set_investor_allowlisted(&inv_b, &true);
    assert_eq!(client.get_allowlisted_investors_count(), 2);
}

#[test]
fn cov_allowlisted_count_after_removal() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    client.set_investor_allowlisted(&inv_a, &true);
    client.set_investor_allowlisted(&inv_b, &true);
    client.set_investor_allowlisted(&inv_a, &false);
    assert_eq!(client.get_allowlisted_investors_count(), 1);
}

#[test]
fn cov_allowlisted_investors_empty_before_additions() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let result = client.get_allowlisted_investors(&0, &10);
    assert_eq!(result.len(), 0);
}

#[test]
fn cov_allowlisted_investors_returns_added() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    client.set_investor_allowlisted(&inv_a, &true);
    client.set_investor_allowlisted(&inv_b, &true);
    let result = client.get_allowlisted_investors(&0, &10);
    assert_eq!(result.len(), 2);
}

#[test]
fn cov_allowlisted_investors_excludes_revoked() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    client.set_investor_allowlisted(&inv_a, &true);
    client.set_investor_allowlisted(&inv_b, &true);
    client.set_investor_allowlisted(&inv_a, &false);
    let result = client.get_allowlisted_investors(&0, &10);
    assert_eq!(result.len(), 1);
}

#[test]
fn cov_allowlisted_investors_pagination() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);
    client.set_investor_allowlisted(&inv_a, &true);
    client.set_investor_allowlisted(&inv_b, &true);
    client.set_investor_allowlisted(&inv_c, &true);

    let page = client.get_allowlisted_investors(&0, &2);
    assert_eq!(page.len(), 2);
    let page2 = client.get_allowlisted_investors(&2, &2);
    assert_eq!(page2.len(), 1);
}

#[test]
fn cov_allowlisted_investors_start_beyond_len() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let inv_a = Address::generate(&env);
    client.set_investor_allowlisted(&inv_a, &true);
    let result = client.get_allowlisted_investors(&100, &10);
    assert_eq!(result.len(), 0);
}

#[test]
fn cov_allowlisted_investors_limit_zero() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let inv_a = Address::generate(&env);
    client.set_investor_allowlisted(&inv_a, &true);
    let result = client.get_allowlisted_investors(&0, &0);
    assert_eq!(result.len(), 0);
}

#[test]
fn cov_allowlisted_count_after_batch_add_and_remove() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let inv_a = Address::generate(&env);
    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);
    let mut invs_add: soroban_sdk::Vec<Address> = SorobanVec::new(&env);
    invs_add.push_back(inv_a.clone());
    invs_add.push_back(inv_b.clone());
    invs_add.push_back(inv_c.clone());
    client.set_investors_allowlisted(&invs_add, &true);
    assert_eq!(client.get_allowlisted_investors_count(), 3);

    let mut invs_rem: soroban_sdk::Vec<Address> = SorobanVec::new(&env);
    invs_rem.push_back(inv_b);
    client.set_investors_allowlisted(&invs_rem, &false);
    assert_eq!(client.get_allowlisted_investors_count(), 2);
}

// ──────────────────────────────────────────────────────────────────────────────
// revoke_attestation_digests (batch)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn cov_revoke_attestation_batch_happy_path() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let d1 = soroban_sdk::BytesN::from_array(&env, &[0x01u8; 32]);
    let d2 = soroban_sdk::BytesN::from_array(&env, &[0x02u8; 32]);
    let d3 = soroban_sdk::BytesN::from_array(&env, &[0x03u8; 32]);
    client.append_attestation_digest(&d1);
    client.append_attestation_digest(&d2);
    client.append_attestation_digest(&d3);

    let mut indices: soroban_sdk::Vec<u32> = SorobanVec::new(&env);
    indices.push_back(0);
    indices.push_back(2);
    client.revoke_attestation_digests(&indices);

    assert!(client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(client.is_attestation_revoked(&2));
}

#[test]
fn cov_revoke_attestation_batch_rejects_empty() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let indices: soroban_sdk::Vec<u32> = SorobanVec::new(&env);
    assert!(client.try_revoke_attestation_digests(&indices).is_err());
}

#[test]
fn cov_revoke_attestation_batch_rejects_oversized() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let mut indices: soroban_sdk::Vec<u32> = SorobanVec::new(&env);
    for _ in 0..=MAX_ATTESTATION_REVOKE_BATCH {
        indices.push_back(0);
    }
    assert!(client.try_revoke_attestation_digests(&indices).is_err());
}

#[test]
fn cov_revoke_attestation_batch_rejects_out_of_range() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let d1 = soroban_sdk::BytesN::from_array(&env, &[0x01u8; 32]);
    client.append_attestation_digest(&d1);

    let mut indices: soroban_sdk::Vec<u32> = SorobanVec::new(&env);
    indices.push_back(5);
    assert!(client.try_revoke_attestation_digests(&indices).is_err());
}

#[test]
fn cov_revoke_attestation_batch_rejects_already_revoked() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let d1 = soroban_sdk::BytesN::from_array(&env, &[0x01u8; 32]);
    client.append_attestation_digest(&d1);

    let mut indices: soroban_sdk::Vec<u32> = SorobanVec::new(&env);
    indices.push_back(0);
    client.revoke_attestation_digests(&indices);

    let mut indices2: soroban_sdk::Vec<u32> = SorobanVec::new(&env);
    indices2.push_back(0);
    assert!(client.try_revoke_attestation_digests(&indices2).is_err());
}

#[test]
fn cov_revoke_attestation_batch_requires_admin() {
    let env = Env::default();
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 0;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);
    env.mock_all_auths();

    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &SorobanString::from_str(&env, "COVRA01"),
        &sme,
        &TARGET,
        &800i64,
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
        &None::<i64>,
    );
    let d1 = soroban_sdk::BytesN::from_array(&env, &[0x01u8; 32]);
    client.append_attestation_digest(&d1);

    env.mock_auths(&[]);
    let mut indices: soroban_sdk::Vec<u32> = SorobanVec::new(&env);
    indices.push_back(0);
    assert!(client.try_revoke_attestation_digests(&indices).is_err());
}

// ──────────────────────────────────────────────────────────────────────────────
// withdraw with protocol fee (lines 4418-4425)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn cov_withdraw_with_nonzero_fee_splits_to_treasury() {
    let env = Env::default();
    let (client, _admin, sme, token, _treasury) = setup_funded_with_fee(&env, 500);

    client.withdraw();

    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 3);

    let fee = TARGET * 500 / 10_000;
    let net = TARGET - fee;
    let sme_balance = token.token.balance(&sme);
    assert_eq!(sme_balance, net);
}

// ──────────────────────────────────────────────────────────────────────────────
// upgrade auth guard (line 3849-3851)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn cov_upgrade_requires_admin_auth() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);
    let fake_hash = soroban_sdk::BytesN::from_array(&env, &[0xABu8; 32]);
    assert!(client.try_upgrade(&fake_hash).is_err());
}

// ──────────────────────────────────────────────────────────────────────────────
// validate_yield_tiers_table empty tiers (line 1696)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn cov_init_with_empty_yield_tiers_succeeds() {
    let env = Env::default();
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 0;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);
    let empty_tiers: SorobanVec<YieldTier> = SorobanVec::new(&env);
    client.init(
        &admin,
        &SorobanString::from_str(&env, "COVTIER"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &Some(empty_tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let tiers = client.get_yield_tiers();
    assert_eq!(tiers.len(), 0);
}

// ──────────────────────────────────────────────────────────────────────────────
// effective_yield_for_commitment with no tiers (line 1756)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn cov_fund_with_commitment_zero_lock_uses_base_yield() {
    let env = Env::default();
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 0;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);
    env.mock_all_auths();

    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &token_id);

    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &SorobanString::from_str(&env, "COVWC01"),
        &sme,
        &TARGET,
        &800i64,
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
        &None::<i64>,
    );

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &(TARGET / 2));
    sac_admin.mint(&escrow_id, &(TARGET * 2));
    client.fund_with_commitment(&inv, &(TARGET / 2), &0u64);
    let ybps = client.get_investor_yield_bps(&inv);
    assert_eq!(ybps, 800);
}

// ──────────────────────────────────────────────────────────────────────────────
// refund_impl batch skip zero contribution (line 5278)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn cov_refund_batch_skips_zero_contribution() {
    let env = Env::default();
    let (client, _token) = setup_unfunded(&env);

    client.cancel_funding();

    let inv_b = Address::generate(&env);
    let inv_c = Address::generate(&env);

    let mut batch: soroban_sdk::Vec<Address> = SorobanVec::new(&env);
    batch.push_back(inv_b);
    batch.push_back(inv_c);

    client.refund_batch(&batch);
}

// ──────────────────────────────────────────────────────────────────────────────
// get_claimable_payout after claim (already claimed → 0)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn cov_claimable_payout_zero_after_claim() {
    let env = Env::default();
    env.mock_all_auths();
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &token_id);
    let escrow_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &SorobanString::from_str(&env, "COVCP01"),
        &sme,
        &TARGET,
        &800i64,
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
        &None::<i64>,
    );
    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &TARGET);
    sac_admin.mint(&escrow_id, &(TARGET * 2));
    client.fund(&investor, &TARGET);
    settle_escrow(&client, &env);

    client.claim_investor_payout(&investor);
    assert_eq!(client.get_claimable_payout(&investor), 0);
}
