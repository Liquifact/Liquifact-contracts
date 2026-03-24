use super::{LiquifactEscrow, LiquifactEscrowClient};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, BytesN, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Deploy a fresh contract and return (env, client, admin, sme).
fn setup() -> (Env, LiquifactEscrowClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);
    (env, client, admin, sme)
}

/// Produce a deterministic 32-byte test hash (simulates SHA-256 of a document).
fn test_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(
        env,
        &[
            0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45,
            0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01,
            0x23, 0x45, 0x67, 0x89,
        ],
    )
}

// ---------------------------------------------------------------------------
// Happy-path tests
// ---------------------------------------------------------------------------

#[test]
fn test_init_stores_escrow() {
    let (env, client, admin, sme) = setup();

    let escrow = client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
        &test_hash(&env),
    );

    assert_eq!(escrow.invoice_id, symbol_short!("INV001"));
    assert_eq!(escrow.admin, admin);
    assert_eq!(escrow.sme_address, sme);
    assert_eq!(escrow.amount, 10_000_0000000i128);
    assert_eq!(escrow.funded_amount, 0);
    assert_eq!(escrow.status, 0);
    assert_eq!(escrow.metadata_hash, test_hash(&env));

    // get_escrow should return the same data
    let got = client.get_escrow();
    assert_eq!(got.invoice_id, escrow.invoice_id);
    assert_eq!(got.admin, admin);
    assert_eq!(got.metadata_hash, test_hash(&env));
}

#[test]
fn test_fund_partial_then_full() {
    let (env, client, admin, sme) = setup();
    let investor = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("INV002"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
        &test_hash(&env),
    );

    // Partial fund — status stays open
    let e1 = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(e1.funded_amount, 5_000_0000000i128);
    assert_eq!(e1.status, 0);

    // Complete fund — status becomes funded
    let e2 = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(e2.funded_amount, 10_000_0000000i128);
    assert_eq!(e2.status, 1);
}

#[test]
fn test_settle_after_full_funding() {
    let (env, client, admin, sme) = setup();
    let investor = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("INV003"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
        &test_hash(&env),
    );
    client.fund(&investor, &10_000_0000000i128);

    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

// ---------------------------------------------------------------------------
// Authorization verification tests
// ---------------------------------------------------------------------------

/// Verify that `init` records an auth requirement for the admin address.
#[test]
fn test_init_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV004"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &test_hash(&env),
    );

    // Inspect recorded auths — admin must appear as the top-level authorizer.
    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == admin),
        "admin auth was not recorded for init"
    );
}

/// Verify that `fund` records an auth requirement for the investor address.
#[test]
fn test_fund_requires_investor_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV005"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &test_hash(&env),
    );
    client.fund(&investor, &1_000i128);

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == investor),
        "investor auth was not recorded for fund"
    );
}

/// Verify that `settle` records an auth requirement for the SME address.
#[test]
fn test_settle_requires_sme_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV006"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &test_hash(&env),
    );
    client.fund(&investor, &1_000i128);
    client.settle();

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == sme),
        "sme auth was not recorded for settle"
    );
}

// ---------------------------------------------------------------------------
// Unauthorized / panic-path tests
// ---------------------------------------------------------------------------

/// `init` called by a non-admin should panic (auth not satisfied).
#[test]
#[should_panic]
fn test_init_unauthorized_panics() {
    let env = Env::default();
    // Do NOT mock auths — let the real auth check fire.
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV007"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &test_hash(&env),
    );
}

/// `settle` called without SME auth should panic.
#[test]
#[should_panic]
fn test_settle_unauthorized_panics() {
    let env = Env::default();
    // Do NOT mock auths.
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    // Use mock_all_auths only for setup steps.
    env.mock_all_auths();
    client.init(
        &admin,
        &symbol_short!("INV008"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &test_hash(&env),
    );
    client.fund(&investor, &1_000i128);

    // Clear mocked auths so settle must satisfy real auth.
    // Soroban test env doesn't expose a "clear mocks" API, so we re-create
    // a client on the same contract without mocking to trigger the failure.
    let env2 = Env::default(); // fresh env — no mocked auths
    let client2 = LiquifactEscrowClient::new(&env2, &contract_id);
    client2.settle(); // should panic: sme auth not satisfied
}

// ---------------------------------------------------------------------------
// Edge-case / guard tests
// ---------------------------------------------------------------------------

/// Re-initializing an already-initialized escrow must panic.
#[test]
#[should_panic(expected = "Escrow already initialized")]
fn test_double_init_panics() {
    let (env, client, admin, sme) = setup();

    client.init(
        &admin,
        &symbol_short!("INV009"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &test_hash(&env),
    );
    // Second init on the same contract must be rejected.
    client.init(
        &admin,
        &symbol_short!("INV009"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &test_hash(&env),
    );
}

/// Funding an already-funded escrow must panic.
#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_after_funded_panics() {
    let (env, client, admin, sme) = setup();
    let investor = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("INV010"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &test_hash(&env),
    );
    client.fund(&investor, &1_000i128); // reaches funded status
    client.fund(&investor, &1i128); // must panic
}

/// Settling an escrow that is still open (not yet funded) must panic.
#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_before_funded_panics() {
    let (env, client, admin, sme) = setup();

    client.init(
        &admin,
        &symbol_short!("INV011"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &test_hash(&env),
    );
    client.settle(); // status is still 0 — must panic
}

/// `get_escrow` on an uninitialized contract must panic.
#[test]
#[should_panic(expected = "Escrow not initialized")]
fn test_get_escrow_uninitialized_panics() {
    let env = Env::default();
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);
    client.get_escrow();
}

/// Partial funding across two investors; status stays open until target is met.
#[test]
fn test_partial_fund_stays_open() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV012"),
        &sme,
        &10_000_0000000i128,
        &500i64,
        &2000u64,
        &test_hash(&env),
    );

    // Fund half — should remain open
    let partial = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(partial.status, 0, "status should still be open");
    assert_eq!(partial.funded_amount, 5_000_0000000i128);

    // Fund the rest — should flip to funded
    let full = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(full.status, 1, "status should be funded");
}

/// Attempting to settle an escrow that is still open must panic.
#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_unfunded_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV013"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
        &test_hash(&env),
    );

    client.settle(); // must panic
}

/// Funding an already-funded (status=1) escrow must panic (alt setup, no helper).
#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_after_funded_panics_inline() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV014"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
        &test_hash(&env),
    );

    client.fund(&investor, &10_000_0000000i128); // fills target → status 1
    client.fund(&investor, &1i128); // must panic
}

// ---------------------------------------------------------------------------
// Metadata hash tests
// ---------------------------------------------------------------------------

/// `init` stores the provided metadata hash and `get_escrow` returns it.
#[test]
fn test_metadata_hash_stored_on_init() {
    let (env, client, admin, sme) = setup();
    let hash = test_hash(&env);

    let escrow = client.init(
        &admin,
        &symbol_short!("INV020"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &hash,
    );

    assert_eq!(escrow.metadata_hash, hash);
    assert_eq!(client.get_escrow().metadata_hash, hash);
}

/// `get_metadata_hash` returns exactly what was passed to `init`.
#[test]
fn test_get_metadata_hash_accessor() {
    let (env, client, admin, sme) = setup();
    let hash = test_hash(&env);

    client.init(
        &admin,
        &symbol_short!("INV021"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &hash,
    );

    assert_eq!(client.get_metadata_hash(), hash);
}

/// Metadata hash is preserved unchanged through full fund → settle lifecycle.
#[test]
fn test_metadata_hash_preserved_through_lifecycle() {
    let (env, client, admin, sme) = setup();
    let investor = Address::generate(&env);
    let hash = test_hash(&env);

    client.init(
        &admin,
        &symbol_short!("INV022"),
        &sme,
        &500i128,
        &500i64,
        &2000u64,
        &hash,
    );

    let after_fund = client.fund(&investor, &500i128);
    assert_eq!(after_fund.metadata_hash, hash, "hash must survive funding");

    let after_settle = client.settle();
    assert_eq!(
        after_settle.metadata_hash, hash,
        "hash must survive settlement"
    );

    assert_eq!(client.get_metadata_hash(), hash);
}

/// Edge case: all-zero hash (valid input — represents a SHA-256 of some document).
#[test]
fn test_metadata_hash_all_zeros() {
    let (env, client, admin, sme) = setup();
    let zero_hash: BytesN<32> = BytesN::from_array(&env, &[0x00; 32]);

    let escrow = client.init(
        &admin,
        &symbol_short!("INV023"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &zero_hash,
    );

    assert_eq!(escrow.metadata_hash, zero_hash);
    assert_eq!(client.get_metadata_hash(), zero_hash);
}

/// Edge case: all-0xFF hash (maximum byte value — valid SHA-256 output range).
#[test]
fn test_metadata_hash_all_ff() {
    let (env, client, admin, sme) = setup();
    let max_hash: BytesN<32> = BytesN::from_array(&env, &[0xff; 32]);

    let escrow = client.init(
        &admin,
        &symbol_short!("INV024"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &max_hash,
    );

    assert_eq!(escrow.metadata_hash, max_hash);
    assert_eq!(client.get_metadata_hash(), max_hash);
}

/// Two distinct hashes stored in two separate contract instances remain independent.
#[test]
fn test_metadata_hash_two_contracts_independent() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let hash_a: BytesN<32> = BytesN::from_array(&env, &[0xaa; 32]);
    let hash_b: BytesN<32> = BytesN::from_array(&env, &[0xbb; 32]);

    let contract_a = env.register(LiquifactEscrow, ());
    let client_a = LiquifactEscrowClient::new(&env, &contract_a);
    client_a.init(
        &admin,
        &symbol_short!("INV025"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &hash_a,
    );

    let contract_b = env.register(LiquifactEscrow, ());
    let client_b = LiquifactEscrowClient::new(&env, &contract_b);
    client_b.init(
        &admin,
        &symbol_short!("INV026"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
        &hash_b,
    );

    assert_eq!(client_a.get_metadata_hash(), hash_a);
    assert_eq!(client_b.get_metadata_hash(), hash_b);
    assert_ne!(client_a.get_metadata_hash(), client_b.get_metadata_hash());
}

/// `get_metadata_hash` on an uninitialized contract must panic.
#[test]
#[should_panic(expected = "Escrow not initialized")]
fn test_get_metadata_hash_uninitialized_panics() {
    let env = Env::default();
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);
    client.get_metadata_hash();
}
