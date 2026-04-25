use super::{
    external_calls, DataKey, LiquifactEscrow, LiquifactEscrowClient, YieldTier,
    MAX_DUST_SWEEP_AMOUNT, SCHEMA_VERSION,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    Address, Env, String, Vec as SorobanVec,
};

// Focused test tree for escrow behavior. Shared helpers live here so feature
// modules stay assertion-focused and each test still owns a fresh Env.
mod admin;
mod funding;
mod init;
mod integration;
mod properties;
mod settlement;

/// Registers a new escrow contract instance and returns its contract id.
pub(super) fn deploy_id(env: &Env) -> Address {
    env.register(LiquifactEscrow, ())
}

pub(super) fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = deploy_id(env);
    LiquifactEscrowClient::new(env, &id)
}

pub(super) fn deploy_with_id(env: &Env) -> (Address, LiquifactEscrowClient<'_>) {
    let id = deploy_id(env);
    let client = LiquifactEscrowClient::new(env, &id);
    (id, client)
}

pub(super) fn setup(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    (client, admin, sme)
}

pub(super) fn free_addresses(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

pub(super) struct StellarTestToken<'a> {
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
pub(super) fn install_stellar_asset_token<'a>(env: &'a Env) -> StellarTestToken<'a> {
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let id = sac.address();
    StellarTestToken {
        id: id.clone(),
        token: TokenClient::new(env, &id),
        stellar: StellarAssetClient::new(env, &id),
    }
}

pub(super) fn default_init(
    client: &LiquifactEscrowClient<'_>,
    env: &Env,
    admin: &Address,
    sme: &Address,
) {
    let (token, treasury) = free_addresses(env);
    client.init(
        admin,
        &String::from_str(env, "INV001"),
        sme,
        &100_000_000_000i128,
        &800i64,
        &1000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );
}

pub(super) const TARGET: i128 = 100_000_000_000i128;
