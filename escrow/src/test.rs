use super::{
    external_calls, LiquifactEscrow, LiquifactEscrowClient, YieldTier, MAX_DUST_SWEEP_AMOUNT,
    SCHEMA_VERSION,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
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

pub(super) fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
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
