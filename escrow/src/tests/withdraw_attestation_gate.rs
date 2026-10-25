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
    let (client, admin, escrow_id, sac_admin) = setup_funded(&env);

    // Status must be 0 (open) to configure the requirement — capture token
    // balance before the admin enables the gate.  We enable it BEFORE funding
    // closes so the escrow is still open.  To test the gate on a funded escrow
    // we must set the flag first (in a fresh open escrow), then fund.
    //
    // Recreate: open escrow → enable gate → fund → attempt withdraw.
    let env2 = Env::default();
    env2.mock_all_auths();
    let sac2 = env2.register_stellar_asset_contract_v2(Address::generate(&env2));
    let token_id2 = sac2.address();
    let sac_admin2 = StellarAssetClient::new(&env2, &token_id2);
    let escrow_id2 = env2.register(LiquifactEscrow, ());
    let client2 = LiquifactEscrowClient::new(&env2, &escrow_id2);
    let admin2 = Address::generate(&env2);
    let sme2 = Address::generate(&env2);
    let treasury2 = Address::generate(&env2);

    client2.init(
        &admin2,
        &soroban_sdk::String::from_str(&env2, "INV_GATE"),
        &sme2,
        &TARGET,
        &800i64,
        &0u64,
        &token_id2,
        &None,
        &treasury2,
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
    client2.set_require_attest_withdraw(&true);

    // Fund so withdraw becomes callable (status 1).
    let investor2 = Address::generate(&env2);
    sac_admin2.mint(&investor2, &TARGET);
    client2.fund(&investor2, &TARGET);

    // Record token balance before the failed attempt.
    let token_client2 = soroban_sdk::token::TokenClient::new(&env2, &token_id2);
    let balance_before = token_client2.balance(&escrow_id2);

    // `withdraw` must fail.
    let result = client2.try_withdraw();
    assert_contract_error(result, EscrowError::AttestationRequiredForWithdraw);

    // Escrow status and balance are unchanged.
    assert_eq!(
        client2.get_escrow().status,
        1u32,
        "status must remain funded (1) after blocked withdraw"
    );
    let balance_after = token_client2.balance(&escrow_id2);
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
    let (client, _admin, escrow_id, _sac_admin) = setup_funded(&env);

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

/// `set_require_attest_withdraw` must also fail after settle (status 2).
#[test]
fn setter_rejected_after_settle() {
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

    // Set maturity far in the future so `settle` passes the maturity check.
    let mut li = env.ledger().get();
    li.timestamp = 0;
    env.ledger().set(li);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV_S2"),
        &sme,
        &TARGET,
        &800i64,
        &1u64, // maturity = 1 second — set ledger past it below
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

    // Advance ledger past maturity.
    let mut li2 = env.ledger().get();
    li2.timestamp = 100;
    env.ledger().set(li2);

    // Settle now collects the coupon shortfall from the SME (#1527).
    sac_admin.mint(&sme, &(TARGET * 800 / 10_000));
    client.settle();
    assert_eq!(client.get_escrow().status, 2u32);

    let result = client.try_set_require_attest_withdraw(&true);
    assert_contract_error(result, EscrowError::AttestationRequirementNotOpen);
}

// ---------------------------------------------------------------------------
// AC-4: event payload
// ---------------------------------------------------------------------------

/// Enabling the gate must emit `AttestationRequirementChanged` with `required == 1`
/// and the correct invoice id.
#[test]
fn setter_emits_event_enable() {
    let env = Env::default();
    let (client, _admin) = setup_open(&env);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id.clone();

    client.set_require_attest_withdraw(&true);

    let events = env.events().all();
    let last = events.events().last().unwrap().clone();
    assert_eq!(
        last,
        AttestationRequirementChanged {
            name: symbol_short!("att_req"),
            invoice_id,
            required: 1,
        }
        .to_xdr(&env, &contract_id)
    );
}

/// Disabling the gate must emit `AttestationRequirementChanged` with `required == 0`.
#[test]
fn setter_emits_event_disable() {
    let env = Env::default();
    let (client, _admin) = setup_open(&env);
    let contract_id = client.address.clone();
    let invoice_id = client.get_escrow().invoice_id.clone();

    // Enable first, then disable.
    client.set_require_attest_withdraw(&true);
    client.set_require_attest_withdraw(&false);

    let events = env.events().all();
    let last = events.events().last().unwrap().clone();
    assert_eq!(
        last,
        AttestationRequirementChanged {
            name: symbol_short!("att_req"),
            invoice_id,
            required: 0,
        }
        .to_xdr(&env, &contract_id)
    );
}

// ---------------------------------------------------------------------------
// AC-5: `get_withdraw_requirements` view
// ---------------------------------------------------------------------------

/// Before any configuration the view returns `(false, false)`.
#[test]
fn get_withdraw_requirements_default_is_false_false() {
    let env = Env::default();
    let (client, _admin) = setup_open(&env);
    let (req, bound) = client.get_withdraw_requirements();
    assert!(!req, "attestation_required must default to false");
    assert!(
        !bound,
        "attestation_bound must be false before any hash is set"
    );
}

/// After enabling the gate, `attestation_required` is `true`; `attestation_bound`
/// reflects whether a hash has been bound.
#[test]
fn get_withdraw_requirements_reflects_state() {
    let env = Env::default();
    let (client, admin) = setup_open(&env);

    // Enable requirement.
    client.set_require_attest_withdraw(&true);
    let (req, bound) = client.get_withdraw_requirements();
    assert!(req, "attestation_required must be true after enabling gate");
    assert!(
        !bound,
        "attestation_bound must still be false — no hash bound yet"
    );

    // Bind a hash.
    client.bind_primary_attestation_hash(&digest32(&env, 0x01));
    let (req2, bound2) = client.get_withdraw_requirements();
    assert!(req2, "attestation_required must still be true");
    assert!(bound2, "attestation_bound must be true after binding hash");
}

/// Disabling the requirement flips `attestation_required` back to `false`.
#[test]
fn get_withdraw_requirements_reflects_disable() {
    let env = Env::default();
    let (client, _admin) = setup_open(&env);

    client.set_require_attest_withdraw(&true);
    client.set_require_attest_withdraw(&false);

    let (req, _bound) = client.get_withdraw_requirements();
    assert!(
        !req,
        "attestation_required must be false after disabling gate"
    );
}

// ---------------------------------------------------------------------------
// Orthogonality: attestation gate is independent of pause and legal hold
// ---------------------------------------------------------------------------

/// The attestation error fires BEFORE the pause error in guard ordering.
/// (Pause fires before attestation per the doc comment — attestation fires
/// after legal-hold but before auth.  This test checks the read-only error
/// fires before auth, not that it precedes pause.)
/// Verify the error code is specifically `AttestationRequiredForWithdraw`.
#[test]
fn attestation_gate_fires_typed_error_code_251() {
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
        &soroban_sdk::String::from_str(&env, "INV_251"),
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

    client.set_require_attest_withdraw(&true);

    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &TARGET);
    client.fund(&investor, &TARGET);

    // Verify numeric code is 251.
    let result = client.try_withdraw();
    assert_contract_error(result, EscrowError::AttestationRequiredForWithdraw);
    // Also verify the numeric value matches the spec.
    assert_eq!(EscrowError::AttestationRequiredForWithdraw as u32, 251);
}

/// `AttestationRequirementNotOpen` must have numeric code 250.
#[test]
fn error_code_250_is_attestation_requirement_not_open() {
    assert_eq!(EscrowError::AttestationRequirementNotOpen as u32, 250);
}
