use super::*;

use crate::{EscrowError, InvoiceEscrow};

// Role/address separation (issue #1547): role parties must not fund their own
// invoice, and init must not accept self-referencing or conflicting addresses.

/// Helper: init an escrow whose funding token is a real Stellar Asset Contract
/// so funding attempts reach the role-conflict guard with balances available.
fn init_with_sac(
    env: &Env,
    client: &LiquifactEscrowClient<'_>,
    admin: &Address,
    sme: &Address,
    treasury: &Address,
    sac_id: &Address,
) -> InvoiceEscrow {
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "ROLE001"),
        sme,
        &TARGET,
        &800i64,
        &0u64,
        sac_id,
        &None,
        treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    )
}

/// On `main`, `fund(sme, x)` succeeds: the SME can push its own invoice to
/// funded status with its own money, faking investor demand and diluting real
/// investors' pro-rata share. On this branch it must revert.
#[test]
fn test_fund_rejects_sme_as_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let sac = install_stellar_asset_token(&env);
    init_with_sac(&env, &client, &admin, &sme, &treasury, &sac.id);

    // The SME holds plenty of the funding token — balance is not the blocker.
    sac.stellar.mint(&sme, &TARGET);

    let result = client.try_fund(&sme, &100_000i128);
    assert_contract_error(result, EscrowError::InvestorRoleConflict);

    // Nothing was recorded.
    assert_eq!(client.get_contribution(&sme), 0);
    assert_eq!(client.get_escrow().funded_amount, 0);
}

#[test]
fn test_fund_rejects_admin_as_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let sac = install_stellar_asset_token(&env);
    init_with_sac(&env, &client, &admin, &sme, &treasury, &sac.id);

    sac.stellar.mint(&admin, &TARGET);

    let result = client.try_fund(&admin, &100_000i128);
    assert_contract_error(result, EscrowError::InvestorRoleConflict);
    assert_eq!(client.get_escrow().funded_amount, 0);
}

#[test]
fn test_fund_rejects_treasury_as_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let sac = install_stellar_asset_token(&env);
    init_with_sac(&env, &client, &admin, &sme, &treasury, &sac.id);

    sac.stellar.mint(&treasury, &TARGET);

    let result = client.try_fund(&treasury, &100_000i128);
    assert_contract_error(result, EscrowError::InvestorRoleConflict);
    assert_eq!(client.get_escrow().funded_amount, 0);
}

#[test]
fn test_fund_rejects_contract_itself_as_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let sac = install_stellar_asset_token(&env);
    init_with_sac(&env, &client, &admin, &sme, &treasury, &sac.id);

    let contract = client.address.clone();
    sac.stellar.mint(&contract, &TARGET);

    let result = client.try_fund(&contract, &100_000i128);
    assert_contract_error(result, EscrowError::InvestorRoleConflict);
    assert_eq!(client.get_escrow().funded_amount, 0);
}

/// `fund_with_commitment` shares `fund_impl`, so the guard applies there too.
#[test]
fn test_fund_with_commitment_rejects_sme_as_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let sac = install_stellar_asset_token(&env);
    init_with_sac(&env, &client, &admin, &sme, &treasury, &sac.id);

    sac.stellar.mint(&sme, &TARGET);

    let result = client.try_fund_with_commitment(&sme, &100_000i128, &0u64);
    assert_contract_error(result, EscrowError::InvestorRoleConflict);
    assert_eq!(client.get_escrow().funded_amount, 0);
}

/// A batch containing the admin at position 3 reverts atomically: positions 1
/// and 2 record no contribution, the unique-funder count stays 0, and the
/// funded total is unchanged.
#[test]
fn test_fund_batch_with_admin_reverts_atomically() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    let sac = install_stellar_asset_token(&env);
    init_with_sac(&env, &client, &admin, &sme, &treasury, &sac.id);

    sac.stellar.mint(&inv1, &TARGET);
    sac.stellar.mint(&inv2, &TARGET);
    sac.stellar.mint(&admin, &TARGET);

    let mut entries = SorobanVec::new(&env);
    entries.push_back((inv1.clone(), 25_000i128));
    entries.push_back((inv2.clone(), 25_000i128));
    entries.push_back((admin.clone(), 25_000i128)); // role conflict at position 3

    let result = client.try_fund_batch(&entries);
    assert_contract_error(result, EscrowError::InvestorRoleConflict);

    // Atomicity: no contribution recorded for positions 1-2, no counters moved.
    assert_eq!(client.get_contribution(&inv1), 0);
    assert_eq!(client.get_contribution(&inv2), 0);
    assert_eq!(client.get_contribution(&admin), 0);
    assert_eq!(client.get_escrow().funded_amount, 0);
    assert_eq!(client.get_escrow().status, 0);
    assert_eq!(client.get_unique_funder_count(), 0);
}

// -- init address/role validation ------------------------------------------------

#[test]
fn test_init_rejects_contract_as_funding_token() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract = client.address.clone();

    let result = client.try_init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROLEI1"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &contract, // self-referencing funding token bricks transfers
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
    assert_contract_error(result, EscrowError::InitAddressInvalid);
}

#[test]
fn test_init_rejects_contract_as_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let sac = install_stellar_asset_token(&env);
    let contract = client.address.clone();

    let result = client.try_init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROLEI2"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &sac.id,
        &None,
        &contract, // self-referencing treasury bricks fee routing
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
    assert_contract_error(result, EscrowError::InitAddressInvalid);
}

#[test]
fn test_init_rejects_contract_as_sme() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let sac = install_stellar_asset_token(&env);
    let contract = client.address.clone();

    let result = client.try_init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROLEI3"),
        &contract, // the contract must not act as beneficiary
        &TARGET,
        &800i64,
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
    assert_contract_error(result, EscrowError::InitAddressInvalid);
}

#[test]
fn test_init_rejects_treasury_equal_to_sme() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let sac = install_stellar_asset_token(&env);

    let result = client.try_init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROLEI4"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &sac.id,
        &None,
        &sme, // treasury == sme: the beneficiary would receive its own fees
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
    assert_contract_error(result, EscrowError::InitRoleConflict);
}

/// The valid default init still succeeds with fully distinct role addresses.
#[test]
fn test_init_accepts_distinct_role_addresses() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);
    let sac = install_stellar_asset_token(&env);

    let escrow = init_with_sac(&env, &client, &admin, &sme, &treasury, &sac.id);
    assert_eq!(escrow.admin, admin);
    assert_eq!(escrow.sme_address, sme);
}

/// `admin == sme_address` stays allowed (dev-only convenience, documented).
#[test]
fn test_init_allows_admin_equal_to_sme() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let sac = install_stellar_asset_token(&env);

    let escrow = client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROLE002"),
        &admin, // dev-only: admin == sme is permitted
        &TARGET,
        &800i64,
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
    assert_eq!(escrow.admin, admin);
    assert_eq!(escrow.sme_address, admin);
}

/// A rejected init must not have written any state: a subsequent init with
/// valid addresses on the same contract still succeeds.
#[test]
fn test_init_rejection_leaves_contract_uninitialized() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let sac = install_stellar_asset_token(&env);

    // Failed init: treasury == sme.
    let bad = client.try_init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROLEI5"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &sac.id,
        &None,
        &sme,
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
    assert_contract_error(bad, EscrowError::InitRoleConflict);

    // A valid init afterwards must still work (nothing was persisted).
    let treasury = Address::generate(&env);
    init_with_sac(&env, &client, &admin, &sme, &treasury, &sac.id);
    assert_eq!(client.get_escrow().sme_address, sme);
}
