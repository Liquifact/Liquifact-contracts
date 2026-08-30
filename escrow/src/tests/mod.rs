#![allow(
    unused_imports,
    unused_variables,
    dead_code,
    unused_comparisons,
    unused_doc_comments,
    unused_macros,
    unused_assignments,
    clippy::needless_borrow,
    clippy::len_zero,
    clippy::explicit_counter_loop,
    clippy::empty_line_after_doc_comments,
    clippy::empty_line_after_outer_attr,
    clippy::absurd_extreme_comparisons,
    clippy::needless_range_loop,
    clippy::mutable_key_type,
    clippy::unusual_byte_groupings
)]
use super::{
    AttestationDigestAppended, AttestationDigestRevoked, AttestationDigestUnrevoked,
    CollateralRecordedEvt, ContractUpgraded, DataKey, DeprecatedTransferAdminUsed, EscrowError,
    EscrowFunded, EscrowInitialized, EscrowUnfunded, FundingCancelled, FundingStateChanged,
    FundingTargetUpdated, InvestorRefundedEvt, LiquifactEscrow, LiquifactEscrowClient,
    MaturityMaxHorizonUpdated, MaxUniqueInvestorsCapLowered, PrimaryAttestationBound,
    RegistryRefRebound, RentStatus, TreasuryDustSwept, YieldTier, MAX_ATTESTATION_APPEND_BATCH,
    MAX_ATTESTATION_APPEND_ENTRIES, MAX_DUST_SWEEP_AMOUNT, MAX_FUND_BATCH, RENT_WARN_LEDGERS,
    SCHEMA_VERSION,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    Address, Env, Error, Event, InvokeError, String, Val, Vec as SorobanVec,
};
use std::fmt::Debug;

pub use soroban_sdk::Symbol;

pub(crate) fn assert_contract_error<T, E>(
    result: Result<Result<T, E>, Result<Error, InvokeError>>,
    expected: EscrowError,
) where
    T: Debug,
    E: Debug,
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

// Focused test tree for escrow behavior. Shared helpers live here so feature
// modules stay assertion-focused and each test still owns a fresh Env.
mod admin;
mod attestations;
mod auth_matrix;
mod cap_validation;
// mod collateral_boundary_tests; // file not present in this tree
// mod collateral_config_view;    // file not present in this tree
// mod collateral_limit_setter;   // file not present in this tree
mod dispute_release;
#[rustfmt::skip]
mod coverage;
mod external_calls;
mod external_calls_mocked;
mod funding;
mod init;
// `integration` (integration.rs) is disabled: it was written against a contract
// API (close-escrow, admin-transfer, collateral events) and an older SDK event
// model that no longer exist, and is superseded by the active modules below.
// mod integration;
mod integration_status_guards;
mod legal_hold;
mod migration_errors;
mod paginated_views;
mod pause;
mod pauser_boundary_tests;
mod properties;
mod reconciliation_lifecycle;
mod settlement;
mod settlement_config_view;
// mod settlement_limit; // file not present in this tree
mod yield_tier_boundaries;
// mod admin_recovery;  // file not present in this tree
mod decimal_scale_tests;
mod release_tests;

/// Registers a new escrow contract instance and returns its contract id.
pub fn deploy_id(env: &Env) -> Address {
    env.register(LiquifactEscrow, ())
}

pub fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = deploy_id(env);
    LiquifactEscrowClient::new(env, &id)
}

#[allow(dead_code)]
pub fn deploy_with_id(env: &Env) -> (Address, LiquifactEscrowClient<'_>) {
    let id = deploy_id(env);
    let client = LiquifactEscrowClient::new(env, &id);
    (id, client)
}

pub fn setup(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 0;
    ledger_info.sequence_number = 100;
    env.ledger().set(ledger_info);
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    (client, admin, sme)
}

pub fn free_addresses(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

pub struct StellarTestToken<'a> {
    pub id: Address,
    pub token: TokenClient<'a>,
    pub stellar: StellarAssetClient<'a>,
}

pub fn install_stellar_asset_token<'a>(env: &'a Env) -> StellarTestToken<'a> {
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let id = sac.address();
    StellarTestToken {
        id: id.clone(),
        token: TokenClient::new(env, &id),
        stellar: StellarAssetClient::new(env, &id),
    }
}

#[allow(dead_code)]
pub fn default_init(client: &LiquifactEscrowClient<'_>, env: &Env, admin: &Address, sme: &Address) {
    let (token, treasury) = free_addresses(env);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "INV001"),
        sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None, // No funding deadline
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );
}

#[allow(dead_code)]
pub const TARGET: i128 = 100_000_000_000i128;

pub fn init_and_fund_with_real_token<'a>(
    env: &'a Env,
    target: i128,
    invoice_id: &str,
) -> (LiquifactEscrowClient<'a>, Address, Address) {
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
        &None::<u32>,
    );

    let investor = Address::generate(env);
    sac_admin.mint(&investor, &target);
    client.fund(&investor, &target);

    sac_admin.mint(&escrow_id, &target);

    (client, escrow_id, sme)
}
