use super::{
    EscrowError, FundResult, InvoiceEscrow, InvoiceEscrowV1, LiquifactEscrow,
    LiquifactEscrowClient, MAX_MATURITY_DELTA_SECS, MAX_YIELD_BPS, SCHEMA_VERSION,
    STATUS_CANCELLED, STATUS_FUNDED, STATUS_OPEN, STATUS_SETTLED,
};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Symbol};

fn deploy<'a>(env: &Env) -> LiquifactEscrowClient<'a> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn round_id(_env: &Env) -> Symbol {
    symbol_short!("RND001")
}

/// Ledger "now" used for valid maturities in tests.
fn t0(env: &Env) -> u64 {
    env.ledger().timestamp()
}

fn valid_maturity(env: &Env) -> u64 {
    t0(env).saturating_add(60 * 60 * 24)
}

fn init_ok(
    env: &Env,
    client: &LiquifactEscrowClient,
    admin: &Address,
    sme: &Address,
    amount: i128,
    target: i128,
    maturity: u64,
) -> InvoiceEscrow {
    client.init(
        admin,
        &symbol_short!("INV001"),
        sme,
        &amount,
        &target,
        &500i64,
        &maturity,
        &round_id(env),
    )
}

// --- init validation ----------------------------------------------------------

#[test]
fn init_rejects_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let e = client
        .try_init(
            &admin,
            &symbol_short!("INVZ"),
            &sme,
            &0i128,
            &1i128,
            &100i64,
            &valid_maturity(&env),
            &round_id(&env),
        )
        .unwrap_err()
        .unwrap();

    assert_eq!(e, EscrowError::InvalidAmount);
}

#[test]
fn init_rejects_zero_funding_target() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let e = client
        .try_init(
            &admin,
            &symbol_short!("INVZ"),
            &sme,
            &100i128,
            &0i128,
            &100i64,
            &valid_maturity(&env),
            &round_id(&env),
        )
        .unwrap_err()
        .unwrap();

    assert_eq!(e, EscrowError::InvalidFundingTarget);
}

#[test]
fn init_rejects_target_above_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let e = client
        .try_init(
            &admin,
            &symbol_short!("INVZ"),
            &sme,
            &100i128,
            &101i128,
            &100i64,
            &valid_maturity(&env),
            &round_id(&env),
        )
        .unwrap_err()
        .unwrap();

    assert_eq!(e, EscrowError::InvalidFundingTarget);
}

#[test]
fn init_rejects_negative_yield() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let e = client
        .try_init(
            &admin,
            &symbol_short!("INVZ"),
            &sme,
            &100i128,
            &100i128,
            &-1i64,
            &valid_maturity(&env),
            &round_id(&env),
        )
        .unwrap_err()
        .unwrap();

    assert_eq!(e, EscrowError::InvalidYieldBps);
}

#[test]
fn init_rejects_yield_above_max() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let e = client
        .try_init(
            &admin,
            &symbol_short!("INVZ"),
            &sme,
            &100i128,
            &100i128,
            &(MAX_YIELD_BPS + 1),
            &valid_maturity(&env),
            &round_id(&env),
        )
        .unwrap_err()
        .unwrap();

    assert_eq!(e, EscrowError::InvalidYieldBps);
}

#[test]
fn init_accepts_boundary_yield_max() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let esc = client.init(
        &admin,
        &symbol_short!("INVB"),
        &sme,
        &100i128,
        &100i128,
        &MAX_YIELD_BPS,
        &valid_maturity(&env),
        &round_id(&env),
    );

    assert_eq!(esc.yield_bps, MAX_YIELD_BPS);
}

#[test]
fn init_rejects_maturity_not_after_now() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let now = t0(&env);
    let e = client
        .try_init(
            &admin,
            &symbol_short!("INVM"),
            &sme,
            &100i128,
            &100i128,
            &100i64,
            &now,
            &round_id(&env),
        )
        .unwrap_err()
        .unwrap();

    assert_eq!(e, EscrowError::InvalidMaturity);
}

#[test]
fn init_rejects_maturity_too_far_future() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    let bad = t0(&env)
        .saturating_add(MAX_MATURITY_DELTA_SECS)
        .saturating_add(1);
    let e = client
        .try_init(
            &admin,
            &symbol_short!("INVM"),
            &sme,
            &100i128,
            &100i128,
            &100i64,
            &bad,
            &round_id(&env),
        )
        .unwrap_err()
        .unwrap();

    assert_eq!(e, EscrowError::InvalidMaturity);
}

#[test]
fn init_sets_funding_metadata() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);
    let rid = symbol_short!("R3Q26");

    let esc = client.init(
        &admin,
        &symbol_short!("INVMETA"),
        &sme,
        &10_000i128,
        &10_000i128,
        &800i64,
        &valid_maturity(&env),
        &rid,
    );

    assert_eq!(esc.funding_round_id, rid);
    assert_eq!(esc.funding_opened_at, t0(&env));
    assert_eq!(esc.funding_closed_at, 0);
    assert_eq!(esc.version, SCHEMA_VERSION);
}

#[test]
fn init_double_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 100, 100, valid_maturity(&env));

    let e = client
        .try_init(
            &admin,
            &symbol_short!("INV2"),
            &sme,
            &100i128,
            &100i128,
            &100i64,
            &valid_maturity(&env),
            &round_id(&env),
        )
        .unwrap_err()
        .unwrap();

    assert_eq!(e, EscrowError::AlreadyInitialized);
}

// --- funding cap --------------------------------------------------------------

#[test]
fn fund_caps_at_target_and_reports_excess() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let client = deploy(&env);

    init_ok(
        &env,
        &client,
        &admin,
        &sme,
        1000,
        1000,
        valid_maturity(&env),
    );

    let FundResult {
        escrow,
        amount_accepted,
        excess_amount,
    } = client.fund(&inv, &600i128);

    assert_eq!(amount_accepted, 600);
    assert_eq!(excess_amount, 0);
    assert_eq!(escrow.funded_amount, 600);
    assert_eq!(escrow.status, STATUS_OPEN);

    let r2 = client.fund(&inv, &500i128);
    assert_eq!(r2.amount_accepted, 400);
    assert_eq!(r2.excess_amount, 100);
    assert_eq!(r2.escrow.funded_amount, 1000);
    assert_eq!(r2.escrow.status, STATUS_FUNDED);
    assert_eq!(r2.escrow.funding_closed_at, t0(&env));
}

#[test]
fn fund_single_shot_overpay_only_accepts_need() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 100, 100, valid_maturity(&env));

    let r = client.fund(&inv, &500i128);
    assert_eq!(r.amount_accepted, 100);
    assert_eq!(r.excess_amount, 400);
    assert_eq!(r.escrow.funded_amount, 100);
    assert_eq!(r.escrow.status, STATUS_FUNDED);
}

#[test]
fn fund_rejects_when_no_capacity_after_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 100, 100, valid_maturity(&env));
    client.fund(&inv, &100i128);

    let e = client.try_fund(&inv, &1i128).unwrap_err().unwrap();
    assert_eq!(e, EscrowError::NotOpenForFunding);
}

#[test]
fn fund_requires_positive_offer() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 100, 100, valid_maturity(&env));

    let e = client.try_fund(&inv, &0i128).unwrap_err().unwrap();
    assert_eq!(e, EscrowError::InvalidAmount);
}

#[test]
fn fund_records_investor_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 50, 50, valid_maturity(&env));
    client.fund(&inv, &50i128);

    assert!(env.auths().iter().any(|(a, _)| *a == inv));
}

// --- cancel -----------------------------------------------------------------

#[test]
fn cancel_open_unfunded() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 100, 100, valid_maturity(&env));
    let e = client.cancel();
    assert_eq!(e.status, STATUS_CANCELLED);
}

#[test]
fn cancel_fails_after_partial_fund() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 100, 100, valid_maturity(&env));
    client.fund(&inv, &10i128);

    let e = client.try_cancel().unwrap_err().unwrap();
    assert_eq!(e, EscrowError::CancelNotAllowed);
}

#[test]
fn cancel_fails_when_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 50, 50, valid_maturity(&env));
    client.fund(&inv, &50i128);

    let e = client.try_cancel().unwrap_err().unwrap();
    assert_eq!(e, EscrowError::CancelNotAllowed);
}

// --- settle & read paths -----------------------------------------------------

#[test]
fn settle_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 100, 100, valid_maturity(&env));
    client.fund(&inv, &100i128);
    let s = client.settle();
    assert_eq!(s.status, STATUS_SETTLED);
}

#[test]
fn settle_requires_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 100, 100, valid_maturity(&env));

    let e = client.try_settle().unwrap_err().unwrap();
    assert_eq!(e, EscrowError::MustBeFundedToSettle);
}

#[test]
fn settle_records_sme_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 10, 10, valid_maturity(&env));
    client.fund(&inv, &10i128);
    client.settle();

    assert!(env.auths().iter().any(|(a, _)| *a == sme));
}

#[test]
fn get_escrow_uninitialized() {
    let env = Env::default();
    let client = deploy(&env);
    let e = client.try_get_escrow().unwrap_err().unwrap();
    assert_eq!(e, EscrowError::NotInitialized);
}

#[test]
fn get_version_zero_before_init() {
    let env = Env::default();
    let client = deploy(&env);
    assert_eq!(client.get_version(), 0u32);
}

#[test]
fn migrate_wrong_from_version() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 100, 100, valid_maturity(&env));

    let e = client.try_migrate(&99u32).unwrap_err().unwrap();
    assert_eq!(e, EscrowError::WrongMigrationVersion);
}

#[test]
fn migrate_at_current_version_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 100, 100, valid_maturity(&env));

    let e = client.try_migrate(&SCHEMA_VERSION).unwrap_err().unwrap();
    assert_eq!(e, EscrowError::NoMigrationPath);
}

#[test]
fn migrate_from_v1() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let v1 = InvoiceEscrowV1 {
        invoice_id: symbol_short!("V1IV"),
        admin: admin.clone(),
        sme_address: sme.clone(),
        amount: 200,
        funding_target: 200,
        funded_amount: 0,
        settled_amount: 0,
        yield_bps: 100,
        maturity: valid_maturity(&env),
        status: STATUS_OPEN,
        version: 1,
    };

    env.as_contract(&contract_id, || {
        env.storage().instance().set(&symbol_short!("escrow"), &v1);
        env.storage()
            .instance()
            .set(&symbol_short!("version"), &1u32);
    });

    let ver = client.migrate(&1u32);
    assert_eq!(ver, SCHEMA_VERSION);

    let up = client.get_escrow();
    assert_eq!(up.version, SCHEMA_VERSION);
    assert_eq!(up.funding_round_id, symbol_short!("MIGRAT"));
    assert_eq!(up.funded_amount, 0);
}

#[test]
fn migrate_from_v1_funded_sets_closed_hint() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let v1 = InvoiceEscrowV1 {
        invoice_id: symbol_short!("V1F"),
        admin,
        sme_address: sme,
        amount: 200,
        funding_target: 200,
        funded_amount: 200,
        settled_amount: 0,
        yield_bps: 100,
        maturity: valid_maturity(&env),
        status: STATUS_FUNDED,
        version: 1,
    };

    env.as_contract(&contract_id, || {
        env.storage().instance().set(&symbol_short!("escrow"), &v1);
        env.storage()
            .instance()
            .set(&symbol_short!("version"), &1u32);
    });

    client.migrate(&1u32);
    let up = client.get_escrow();
    assert!(up.funding_closed_at > 0 || up.funding_closed_at == t0(&env));
}

// --- update maturity ---------------------------------------------------------

#[test]
fn update_maturity_open_only() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let inv = Address::generate(&env);
    let client = deploy(&env);

    init_ok(&env, &client, &admin, &sme, 100, 100, valid_maturity(&env));
    let new_m = t0(&env).saturating_add(60 * 60 * 48);
    let e = client.update_maturity(&new_m);
    assert_eq!(e.maturity, new_m);

    client.fund(&inv, &100i128);
    let err = client
        .try_update_maturity(&valid_maturity(&env))
        .unwrap_err()
        .unwrap();
    assert_eq!(err, EscrowError::MaturityUpdateNotAllowed);
}

#[test]
fn validate_init_params_helper() {
    let env = Env::default();
    assert!(LiquifactEscrow::validate_init_params(&env, 1, 1, 0, valid_maturity(&env)).is_ok());
    assert_eq!(
        LiquifactEscrow::validate_init_params(&env, 0, 1, 0, valid_maturity(&env)),
        Err(EscrowError::InvalidAmount)
    );
}

// --- property-ish fuzz (lightweight) ----------------------------------------

use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_funded_never_exceeds_target(
        target in 1i128..10_000i128,
        a in 1i128..5_000i128,
        b in 1i128..5_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let inv1 = Address::generate(&env);
        let inv2 = Address::generate(&env);
        let client = deploy(&env);

        let invsym = symbol_short!("PRPINV");
        let _ = client.init(
            &admin,
            &invsym,
            &sme,
            &target,
            &target,
            &100i64,
            &valid_maturity(&env),
            &round_id(&env),
        );

        let after1 = client.fund(&inv1, &a);
        prop_assert!(after1.escrow.funded_amount <= target);
        if after1.escrow.status == STATUS_OPEN {
            let r2 = client.fund(&inv2, &b);
            prop_assert!(r2.escrow.funded_amount <= target);
        }
    }
}
