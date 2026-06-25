#![allow(
    unused_imports,
    unused_variables,
    dead_code,
    clippy::needless_borrow,
    clippy::len_zero,
    clippy::explicit_counter_loop
)]
#[allow(unused_imports)]
use super::{
    AttestationDigestRevoked, CollateralRecordedEvt, DataKey, EscrowError, EscrowFunded,
    EscrowInitialized, FundingTargetUpdated, LiquifactEscrow, LiquifactEscrowClient,
    MaxUniqueInvestorsCapLowered, YieldTier, MAX_ATTESTATION_APPEND_ENTRIES, MAX_DUST_SWEEP_AMOUNT,
    MAX_FUND_BATCH, SCHEMA_VERSION,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    Address, Env, Error, Event, InvokeError, String, Val, Vec as SorobanVec,
};
use std::fmt::Debug;

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
mod cap_validation;
mod coverage;
mod external_calls;
mod external_calls_mocked;
mod funding;
mod init;
mod integration;
mod legal_hold;
mod properties;
mod settlement;

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
    ledger_info.timestamp = 12345;
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
    /// Contract id for the standard Stellar asset token.
    pub id: Address,
    /// SEP-41 interface (the same interface the escrow uses in `external_calls`).
    pub token: TokenClient<'a>,
    /// Test-only admin client used for minting balances into accounts/contracts.
    pub stellar: StellarAssetClient<'a>,
}

/// Install a **standard** Stellar asset token contract (Soroban StellarAsset contract v2).
///
/// This is intentionally used for tests that require "well-behaved" SEP-41 semantics:
/// - No fee-on-transfer / rebasing / callback side-effects.
/// - `balance` deltas match transfer amounts (as asserted by `external_calls` wrappers).
///
/// **Out of scope:** non-standard/malicious token economics; see `escrow/src/external_calls.rs`
/// and `docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`.
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
    );
}

#[allow(dead_code)]
pub const TARGET: i128 = 100_000_000_000i128;

/// Create a **new** escrow contract backed by a real Stellar asset contract (SAC),
/// initialise it with a funded target, fund it to exactly `target`, and mint `target`
/// tokens into the escrow contract address so that `withdraw()` can actually transfer
/// them.
///
/// Returns `(client, escrow_id, sme, token_client)`.  The caller must have called
/// `env.mock_all_auths()` (or equivalent) before invoking this helper.
#[allow(dead_code)]
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
    );

    let investor = Address::generate(env);
    client.fund(&investor, &target);

    // Mint funded_amount into the escrow so withdraw() can actually transfer tokens.
    sac_admin.mint(&escrow_id, &target);

    (client, escrow_id, sme)
}
