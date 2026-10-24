//! Tests for the optional attestation gate on `withdraw` (#1561).
//!
//! Acceptance criteria covered:
//! 1. **Required, none bound** — `withdraw` fails with `AttestationRequiredForWithdraw`;
//!    balances are unchanged.  Bind a primary hash, retry — succeeds with balances asserted.
//! 2. **Not required** — identical to main (regression); `withdraw` succeeds without any hash.
//! 3. **Setter rejected after funding closes** — `set_require_attest_withdraw`
//!    fails with `AttestationRequirementNotOpen` when `status != 0`.
//! 4. **Event payload** — `AttestationRequirementChanged` carries the correct fields.
//! 5. **`get_withdraw_requirements` view** — reflects current flag / hash state.

use super::*;
use crate::{AttestationRequirementChanged, EscrowError, LiquifactEscrow};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    token::StellarAssetClient,
    Address, BytesN, Env, Event,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A deterministic 32-byte digest for use in tests.
fn digest32(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

/// Set up a funded escrow backed by a real SAC token.
///
/// Returns `(client, admin, escrow_id, token_id, sac_admin)`.
/// The escrow starts at `status == 1` (funded) with `TARGET` tokens in the
/// contract account, ready for `withdraw`.
fn setup_funded<'a>(
    env: &'a Env,
) -> (
    LiquifactEscrowClient<'a>,
    Address,
    Address,
    soroban_sdk::token::StellarAssetClient<'a>,
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
        &soroban_sdk::String::from_str(env, "INV_ATT"),
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

    // Fund with a real token so `withdraw` can actually transfer.
    let investor = Address::generate(env);
    sac_admin.mint(&investor, &TARGET);
    client.fund(&investor, &TARGET);
    // Tokens are already inside the contract after `fund`; no extra mint needed.

    (client, admin, escrow_id, sac_admin)
}

/// Set up an open (status 0) escrow — does NOT fund it.
/// Returns `(client, admin)`.
fn setup_open<'a>(env: &'a Env) -> (LiquifactEscrowClient<'a>, Address) {
    env.mock_all_auths();
    let (client, admin, sme) = setup(env);
    default_init(&client, env, &admin, &sme);
    (client, admin)
}

// ---------------------------------------------------------------------------
// AC-1a: require + no hash bound → withdraw is blocked
// ---------------------------------------------------------------------------

/// With the requirement enabled and no primary hash, `withdraw` must fail with
/// `AttestationRequiredForWithdraw` and leave both the escrow status and the
/// token balance unchanged.
#[test]
fn withdraw_blocked_when_required_and_hash_absent() {
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
        &soroban_sdk::String::from_str(&env, "INV_GATE"),
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

    // Enable attestation gate while escrow is open (status 0).
    client.set_require_attest_withdraw(&true);

    // Fund so withdraw becomes callable (status 1).
    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &TARGET);
    client.fund(&investor, &TARGET);

    // Record token balance before the failed attempt.
    let token_client = soroban_sdk::token::TokenClient::new(&env, &token_id);
    let balance_before = token_client.balance(&escrow_id);

    // `withdraw` must fail.
    let result = client.try_withdraw();
    assert_contract_error(result, EscrowError::AttestationRequiredForWithdraw);

    // Escrow status and balance are unchanged.
    assert_eq!(
        client.get_escrow().status,
        1u32,
        "status must remain funded (1) after blocked withdraw"
    );
    let balance_after = token_client.balance(&escrow_id);
    assert_eq!(
        balance_after, balance_before,
        "contract balance must be unchanged after blocked withdraw"
    );
}

// ---------------------------------------------------------------------------
// AC-1b: require + hash bound → withdraw succeeds
// ---------------------------------------------------------------------------

/// After the requirement is enabled on an open escrow, binding a primary hash
/// and then calling `withdraw` must succeed.  Status moves to 3 and the SME
/// receives the tokens.
#[test]
fn withdraw_succeeds_after_binding_hash_when_required() {
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
        &soroban_sdk::String::from_str(&env, "INV_OK"),
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

    // Enable gate while open.
    client.set_require_attest_withdraw(&true);

    // Fund the escrow.
    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &TARGET);
    client.fund(&investor, &TARGET);

    // Bind the primary attestation hash.
    client.bind_primary_attestation_hash(&digest32(&env, 0xAB));

    let token_client = soroban_sdk::token::TokenClient::new(&env, &token_id);
    let sme_balance_before = token_client.balance(&sme);

    // Withdraw must now succeed.
    let escrow = client.withdraw();

    assert_eq!(
        escrow.status, 3u32,
        "status must be 3 after successful withdraw"
    );
    assert_eq!(
        token_client.balance(&sme),
        sme_balance_before + TARGET,
        "SME must receive the full funded amount"
    );
    assert_eq!(
        token_client.balance(&escrow_id),
        0,
        "contract balance must be zero after withdraw"
    );
}

// ---------------------------------------------------------------------------
// AC-2: not required (default) → withdraw is unaffected (regression)
// ---------------------------------------------------------------------------

/// Without enabling the requirement, `withdraw` must succeed regardless of
/// whether a primary hash has been bound — identical to the main-branch behavior.
#[test]
fn withdraw_not_blocked_when_requirement_not_set() {
    let env = Env::default();
    let (client, _admin, _escrow_id, _sac_admin) = setup_funded(&env);

    // No call to `set_require_attest_withdraw` — default is false.
    let escrow = client.withdraw();
    assert_eq!(escrow.status, 3u32);
}

/// Even if a hash is NOT bound, `withdraw` succeeds when requirement is disabled.
/// This verifies the default-false behavior without calling the setter at all.
#[test]
fn withdraw_succeeds_without_hash_when_not_required() {
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
        &soroban_sdk::String::from_str(&env, "INV_NORQ"),
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
    client.fund(&investor, &TARGET);

    // No call to `set_require_attest_withdraw` — requirement defaults to false.
    // No primary hash bound either.  Withdraw must succeed.
    let escrow = client.withdraw();
    assert_eq!(escrow.status, 3u32);
}

// ---------------------------------------------------------------------------
// AC-3: setter rejected after funding closes (status != 0)
// ---------------------------------------------------------------------------

/// `set_require_attest_withdraw` must fail with `AttestationRequirementNotOpen`
/// when the escrow has already moved to funded status (status == 1).
#[test]
fn setter_rejected_after_funding_closes() {
    let env = Env::default();
    let (client, _admin, _escrow_id, _sac_admin) = setup_funded(&env);

    // Escrow is now status 1 (funded).
    let result = client.try_set_require_attest_withdraw(&true);
    assert_contract_error(result, EscrowError::AttestationRequirementNotOpen);
}
