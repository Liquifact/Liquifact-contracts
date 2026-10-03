//! Tests for [`LiquifactEscrow::get_collateral_config`],
//! [`LiquifactEscrow::get_collateral_limit`], [`LiquifactEscrow::set_collateral_limit`],
//! and the round-trip through [`LiquifactEscrow::record_sme_collateral_commitment`].
//!
//! Covers:
//! - Default values before [`LiquifactEscrow::init`] is called.
//! - Values remain at their documented defaults after `init` (no overwrite).
//! - The bundled `get_collateral_config` matches `get_collateral_limit` +
//!   `get_sme_collateral_commitment` individual reads (atomicity / no drift).
//! - Pure reads are idempotent and never require auth.
//! - `set_collateral_limit` admin guard, positive-amount guard, and
//!   `MAX_INVOICE_AMOUNT` boundary guard.
//! - `record_sme_collateral_commitment` honors the configured ceiling.
//! - The struct shape is pinned via field-by-field destructuring so adding or
//!   renaming fields produces a compile error.
//! - `CollateralLimitExceeded` is raised for commitments over the ceiling.

use super::super::{
    CollateralCommitmentSnapshot, CollateralConfig, LiquifactEscrow, LiquifactEscrowClient,
    MAX_INVOICE_AMOUNT,
};
use crate::tests::assert_contract_error;
use crate::EscrowError;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, Symbol};

// ── helpers ──────────────────────────────────────────────────────────────────

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn deploy_and_init(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "COLCFG01"),
        &sme,
        &10_000i128,
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
        &None,
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );

    (client, admin, sme)
}

// ── Defaults ─────────────────────────────────────────────────────────────────

/// Before `init`: `collateral_limit == MAX_INVOICE_AMOUNT`, `sme_commitment == None`.
#[test]
fn test_defaults_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);

    let CollateralConfig {
        collateral_limit,
        sme_commitment,
    } = client.get_collateral_config();

    assert_eq!(collateral_limit, MAX_INVOICE_AMOUNT);
    assert_eq!(sme_commitment, CollateralCommitmentSnapshot::None);
}

/// After `init`: keys are still absent so defaults persist (no silent init-time overwrite).
#[test]
fn test_defaults_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);

    let cfg = client.get_collateral_config();
    assert_eq!(cfg.collateral_limit, MAX_INVOICE_AMOUNT);
    assert_eq!(cfg.sme_commitment, CollateralCommitmentSnapshot::None);
}

// ── Consistency with individual getters ──────────────────────────────────────

/// `get_collateral_config().collateral_limit` must always equal `get_collateral_limit()`,
/// and the commitment must match `get_sme_collateral_commitment()` (lifted to snapshot).
#[test]
fn test_consistency_with_individual_getters_before_mut() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    let cfg = client.get_collateral_config();
    assert_eq!(cfg.collateral_limit, client.get_collateral_limit());
    assert_eq!(cfg.sme_commitment, CollateralCommitmentSnapshot::None);
    assert_eq!(client.get_sme_collateral_commitment(), None);
}

/// After `set_collateral_limit` + `record_sme_collateral_commitment`, the bundled
/// config still agrees with the authoritative per-key reads.
#[test]
fn test_consistency_with_individual_getters_after_mut() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, sme) = deploy_and_init(&env);

    client.set_collateral_limit(&7_500i128);
    let asset = Symbol::new(&env, "USDC");
    let commitment = client.record_sme_collateral_commitment(&asset, &5_000i128);

    let cfg = client.get_collateral_config();
    assert_eq!(cfg.collateral_limit, client.get_collateral_limit());
    assert_eq!(cfg.collateral_limit, 7_500i128);
    assert_eq!(
        cfg.sme_commitment,
        CollateralCommitmentSnapshot::Some(commitment.clone())
    );
    assert_eq!(client.get_sme_collateral_commitment(), Some(commitment));
}

// ── Idempotency ──────────────────────────────────────────────────────────────

/// Calling views multiple times returns the same value.
#[test]
fn test_idempotent_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let a = client.get_collateral_config();
    let b = client.get_collateral_config();
    assert_eq!(a.collateral_limit, b.collateral_limit);
    assert_eq!(a.sme_commitment, b.sme_commitment);
    assert_eq!(client.get_collateral_limit(), client.get_collateral_limit());
}

/// Calling views multiple times after mutation returns stable, equal snapshots.
#[test]
fn test_idempotent_after_init_and_mut() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    client.set_collateral_limit(&2_000i128);

    let a = client.get_collateral_config();
    let b = client.get_collateral_config();
    let c = client.get_collateral_config();
    assert_eq!(a.collateral_limit, b.collateral_limit);
    assert_eq!(b.collateral_limit, c.collateral_limit);
    assert_eq!(a.sme_commitment, CollateralCommitmentSnapshot::None);
}

// ── Struct shape pin ─────────────────────────────────────────────────────────

/// Destructuring pins the struct's public field names and arity at compile time.
#[test]
fn test_struct_shape_pin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, sme) = deploy_and_init(&env);
    client.set_collateral_limit(&12_000i128);
    let asset = Symbol::new(&env, "EURC");
    let recorded = client.record_sme_collateral_commitment(&asset, &3_000i128);

    let CollateralConfig {
        collateral_limit,
        sme_commitment,
    } = client.get_collateral_config();

    assert_eq!(collateral_limit, 12_000i128);
    match sme_commitment {
        CollateralCommitmentSnapshot::Some(c) => {
            assert_eq!(c.asset, recorded.asset);
            assert_eq!(c.amount, recorded.amount);
            assert_eq!(c.recorded_at, recorded.recorded_at);
        }
        CollateralCommitmentSnapshot::None => panic!("expected commitment to be set"),
    }
}

// ── State transitions ────────────────────────────────────────────────────────

/// `set_collateral_limit` writes the new ceiling; view reflects it immediately.
#[test]
fn test_after_set_collateral_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);

    client.set_collateral_limit(&5_000i128);
    assert_eq!(client.get_collateral_limit(), 5_000i128);
    assert_eq!(client.get_collateral_config().collateral_limit, 5_000i128);

    // overwrite
    client.set_collateral_limit(&9_999i128);
    assert_eq!(client.get_collateral_limit(), 9_999i128);
    assert_eq!(client.get_collateral_config().collateral_limit, 9_999i128);
}

/// After recording a commitment, `sme_commitment` becomes `Some(...)`.
#[test]
fn test_after_record_sme_collateral_commitment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let asset = Symbol::new(&env, "XLM");

    let commitment = client.record_sme_collateral_commitment(&asset, &1_000i128);
    let cfg = client.get_collateral_config();
    match cfg.sme_commitment {
        CollateralCommitmentSnapshot::Some(c) => {
            assert_eq!(c, commitment);
        }
        CollateralCommitmentSnapshot::None => panic!("expected Some"),
    }
}

/// After clearing, `sme_commitment` reverts to `None`; limit is preserved.
#[test]
fn test_after_clear_sme_collateral_commitment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    client.set_collateral_limit(&6_000i128);

    let asset = Symbol::new(&env, "BTC");
    client.record_sme_collateral_commitment(&asset, &1_500i128);
    assert_ne!(
        client.get_collateral_config().sme_commitment,
        CollateralCommitmentSnapshot::None
    );

    client.clear_sme_collateral_commitment();

    let cfg = client.get_collateral_config();
    assert_eq!(cfg.sme_commitment, CollateralCommitmentSnapshot::None);
    assert_eq!(cfg.collateral_limit, 6_000i128);
}

// ── Boundary / validation for set_collateral_limit ───────────────────────────

/// `limit = 1` (minimum valid positive) succeeds.
#[test]
fn test_set_limit_minimum_valid() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    client.set_collateral_limit(&1i128);
    assert_eq!(client.get_collateral_limit(), 1i128);
}

/// `limit = MAX_INVOICE_AMOUNT` succeeds (equality allowed).
#[test]
fn test_set_limit_boundary_max_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    client.set_collateral_limit(&MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

/// `limit = 0` → `CollateralLimitNotPositive`.
#[test]
fn test_set_limit_zero_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    assert_contract_error(
        Ok(client.try_set_collateral_limit(&0i128)),
        EscrowError::CollateralLimitNotPositive,
    );
}

/// `limit = -1` → `CollateralLimitNotPositive`.
#[test]
fn test_set_limit_negative_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    assert_contract_error(
        Ok(client.try_set_collateral_limit(&-1i128)),
        EscrowError::CollateralLimitNotPositive,
    );
}

/// `limit = MAX_INVOICE_AMOUNT + 1` → `CollateralLimitExceedsMax`.
#[test]
fn test_set_limit_over_max_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let too_big = MAX_INVOICE_AMOUNT
        .checked_add(1)
        .expect("MAX_INVOICE_AMOUNT + 1 must fit in i128");
    assert_contract_error(
        Ok(client.try_set_collateral_limit(&too_big)),
        EscrowError::CollateralLimitExceedsMax,
    );
}

/// Non-admin caller: `set_collateral_limit` is admin-guarded via
/// `load_escrow_require_admin` so it fails when `mock_all_auths` is disabled and
/// a non-admin address invokes it.
#[test]
fn test_set_limit_requires_admin_auth() {
    let env = Env::default();
    let (client, _admin, sme) = deploy_and_init(&env);
    env.set_source_account(&sme);
    let res = client.try_set_collateral_limit(&5_000i128);
    match res {
        Err(_) => {} // auth failure expected
        Ok(Err(_)) => {} // EscrowError::NotAdmin via mock is also a failure path
        _ => panic!("expected auth/NotAdmin failure for non-admin setter call"),
    }
}

// ── CollateralLimitExceeded guard in record ──────────────────────────────────

/// Recording exactly at the ceiling succeeds.
#[test]
fn test_record_at_ceiling_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    client.set_collateral_limit(&3_000i128);
    let asset = Symbol::new(&env, "USDC");
    let c = client.record_sme_collateral_commitment(&asset, &3_000i128);
    assert_eq!(c.amount, 3_000i128);
}

/// Recording amount > ceiling → `CollateralLimitExceeded`.
#[test]
fn test_record_over_ceiling_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    client.set_collateral_limit(&3_000i128);
    let asset = Symbol::new(&env, "USDC");
    assert_contract_error(
        Ok(client.try_record_sme_collateral_commitment(&asset, &3_001i128)),
        EscrowError::CollateralLimitExceeded,
    );
}

/// Default ceiling (MAX_INVOICE_AMOUNT) still enforces: a deliberately absurd
/// value that exceeds it is rejected.
#[test]
fn test_record_over_default_max_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    let too_big = MAX_INVOICE_AMOUNT
        .checked_add(1)
        .expect("MAX_INVOICE_AMOUNT + 1 must fit");
    let asset = Symbol::new(&env, "XLM");
    assert_contract_error(
        Ok(client.try_record_sme_collateral_commitment(&asset, &too_big)),
        EscrowError::CollateralLimitExceeded,
    );
}

/// Batch record: any single item over the ceiling rejects the entire batch
/// atomically — prior commitment should be unchanged.
#[test]
fn test_batch_record_over_ceiling_atomic_reject() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = deploy_and_init(&env);
    client.set_collateral_limit(&5_000i128);

    // pre-state: record a valid commitment first
    let a0 = Symbol::new(&env, "A0");
    let prior = client.record_sme_collateral_commitment(&a0, &2_000i128);

    // build a batch where item 1 is fine but item 2 exceeds ceiling
    let a1 = Symbol::new(&env, "A1");
    let a2 = Symbol::new(&env, "A2");
    let items = soroban_sdk::Vec::from_array(
        &env,
        [(a1.clone(), 3_000i128), (a2.clone(), 5_001i128)],
    );
    assert_contract_error(
        Ok(client.try_batch_record_collateral(&items)),
        EscrowError::CollateralLimitExceeded,
    );

    // atomic: prior commitment should still be A0 / 2000
    let stored = client.get_sme_collateral_commitment().expect("present");
    assert_eq!(stored.amount, prior.amount);
    assert_eq!(stored.asset, prior.asset);
}
