// Tests for the shared paginate_window helper and the public paginated read views:
//   get_investors, get_allowlisted_investors, get_revoked_attestation_digests,
//   get_collateral_records, get_pause_records, and get_settlement_records.
//
// Each test uses a fresh Env so state cannot leak across cases.

use crate::MAX_PAUSE_READ_PAGE;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

// ── paginate_window unit tests ────────────────────────────────────────────────
//
// paginate_window is a private associated function, so we test it through
// LiquifactEscrow::paginate_window via `crate::` access inside the same crate.

#[test]
fn paginate_window_empty_collection_returns_none() {
    // len == 0 → always None regardless of start/limit
    assert_eq!(crate::LiquifactEscrow::paginate_window(0, 10, 50, 0), None);
    assert_eq!(crate::LiquifactEscrow::paginate_window(5, 10, 50, 0), None);
}

#[test]
fn paginate_window_start_past_end_returns_none() {
    // start >= len → None
    assert_eq!(crate::LiquifactEscrow::paginate_window(5, 10, 50, 5), None);
    assert_eq!(
        crate::LiquifactEscrow::paginate_window(100, 10, 50, 5),
        None
    );
}

#[test]
fn paginate_window_zero_limit_returns_none() {
    // limit == 0 → None even when start is valid
    assert_eq!(crate::LiquifactEscrow::paginate_window(0, 0, 50, 10), None);
    assert_eq!(crate::LiquifactEscrow::paginate_window(3, 0, 50, 10), None);
}

#[test]
fn paginate_window_first_page() {
    // start=0, limit=5, ceiling=50, len=10 → (0, 5)
    assert_eq!(
        crate::LiquifactEscrow::paginate_window(0, 5, 50, 10),
        Some((0, 5))
    );
}

#[test]
fn paginate_window_continuation_page() {
    // start=5, limit=5, ceiling=50, len=10 → (5, 10)
    assert_eq!(
        crate::LiquifactEscrow::paginate_window(5, 5, 50, 10),
        Some((5, 10))
    );
}

#[test]
fn paginate_window_limit_exceeds_remaining_items() {
    // start=7, limit=50, ceiling=50, len=10 → (7, 10)  (clamped at len)
    assert_eq!(
        crate::LiquifactEscrow::paginate_window(7, 50, 50, 10),
        Some((7, 10))
    );
}

#[test]
fn paginate_window_ceiling_enforced() {
    // limit > ceiling → ceiling is applied; start=0, limit=100, ceiling=20, len=50 → (0, 20)
    assert_eq!(
        crate::LiquifactEscrow::paginate_window(0, 100, 20, 50),
        Some((0, 20))
    );
}

#[test]
fn paginate_window_saturating_add_does_not_overflow() {
    // start near u32::MAX with a non-zero limit should not panic
    let result = crate::LiquifactEscrow::paginate_window(u32::MAX - 1, 50, 50, u32::MAX);
    // start (u32::MAX-1) < len (u32::MAX), limit > 0 → Some((u32::MAX-1, u32::MAX))
    assert_eq!(result, Some((u32::MAX - 1, u32::MAX)));
}

// ── Helper: init with real defaults ──────────────────────────────────────────

fn do_init(
    env: &Env,
    client: &crate::LiquifactEscrowClient<'_>,
    admin: &Address,
    sme: &Address,
    token: &Address,
    treasury: &Address,
) {
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "INV-PG-001"),
        sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        token,
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
    );
}

// ── get_investors ─────────────────────────────────────────────────────────────

#[test]
fn get_investors_empty_before_any_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let client = super::deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    do_init(
        &env,
        &client,
        &admin,
        &sme,
        &Address::generate(&env),
        &Address::generate(&env),
    );
    let result = client.get_investors(&0, &10);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_investors_zero_limit_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = super::setup(&env);
    super::default_init(&client, &env, &admin, &sme);

    let investor = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let _ = client.get_investors(&0, &0);
    // Just verify it doesn't panic and returns empty
    let result = client.get_investors(&0, &0);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_investors_first_page() {
    let env = Env::default();
    env.mock_all_auths();
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_id);

    let escrow_id = env.register(crate::LiquifactEscrow, ());
    let client = crate::LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV-PG-FIRST"),
        &sme,
        &500_000_000i128,
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

    // Fund with 5 investors
    let mut investors = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let inv = Address::generate(&env);
        sac_admin.mint(&inv, &100_000_000i128);
        client.fund(&inv, &100_000_000i128);
        investors.push_back(inv);
    }

    // First page of 3
    let page = client.get_investors(&0, &3);
    assert_eq!(page.len(), 3);
    assert_eq!(page.get(0).unwrap(), investors.get(0).unwrap());
    assert_eq!(page.get(1).unwrap(), investors.get(1).unwrap());
    assert_eq!(page.get(2).unwrap(), investors.get(2).unwrap());
}

#[test]
fn get_investors_continuation_page() {
    let env = Env::default();
    env.mock_all_auths();
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_id);

    let escrow_id = env.register(crate::LiquifactEscrow, ());
    let client = crate::LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV-PG-CONT"),
        &sme,
        &500_000_000i128,
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

    let mut investors = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let inv = Address::generate(&env);
        sac_admin.mint(&inv, &100_000_000i128);
        client.fund(&inv, &100_000_000i128);
        investors.push_back(inv);
    }

    // Page 2: start=3, limit=3 → should return items 3 and 4 only
    let page = client.get_investors(&3, &3);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap(), investors.get(3).unwrap());
    assert_eq!(page.get(1).unwrap(), investors.get(4).unwrap());
}

#[test]
fn get_investors_start_past_end_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_id);

    let escrow_id = env.register(crate::LiquifactEscrow, ());
    let client = crate::LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV-PG-PAST"),
        &sme,
        &200_000_000i128,
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

    let inv = Address::generate(&env);
    sac_admin.mint(&inv, &200_000_000i128);
    client.fund(&inv, &200_000_000i128);

    // Only 1 investor; start=5 is past the end
    let result = client.get_investors(&5, &10);
    assert_eq!(result.len(), 0);
}

// ── get_allowlisted_investors ─────────────────────────────────────────────────

fn setup_allowlist_escrow(env: &Env) -> (crate::LiquifactEscrowClient<'_>, Address, Address) {
    let client = super::deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "INV-AL-PG"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(env),
        &None,
        &Address::generate(env),
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
    (client, admin, sme)
}

#[test]
fn get_allowlisted_investors_empty_when_none_set() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_allowlist_escrow(&env);
    let result = client.get_allowlisted_investors(&0, &10);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_allowlisted_investors_zero_limit_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _sme) = setup_allowlist_escrow(&env);
    let inv = Address::generate(&env);
    client.set_investor_allowlisted(&inv, &true);
    let result = client.get_allowlisted_investors(&0, &0);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_allowlisted_investors_start_past_end_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _sme) = setup_allowlist_escrow(&env);
    let inv = Address::generate(&env);
    client.set_investor_allowlisted(&inv, &true);
    // Only 1 investor; start=5 is past the end
    let result = client.get_allowlisted_investors(&5, &10);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_allowlisted_investors_first_page() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_allowlist_escrow(&env);

    let mut addrs = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
        addrs.push_back(addr);
    }

    let page = client.get_allowlisted_investors(&0, &3);
    assert_eq!(page.len(), 3);
    assert_eq!(page.get(0).unwrap(), addrs.get(0).unwrap());
    assert_eq!(page.get(1).unwrap(), addrs.get(1).unwrap());
    assert_eq!(page.get(2).unwrap(), addrs.get(2).unwrap());
}

#[test]
fn get_allowlisted_investors_continuation_page() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_allowlist_escrow(&env);

    let mut addrs = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
        addrs.push_back(addr);
    }

    // Continuation: start=3, limit=3 → items 3 and 4
    let page = client.get_allowlisted_investors(&3, &3);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap(), addrs.get(3).unwrap());
    assert_eq!(page.get(1).unwrap(), addrs.get(4).unwrap());
}

#[test]
fn get_allowlisted_investors_excludes_revoked_addresses() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_allowlist_escrow(&env);

    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);
    let addr_c = Address::generate(&env);
    client.set_investor_allowlisted(&addr_a, &true);
    client.set_investor_allowlisted(&addr_b, &true);
    client.set_investor_allowlisted(&addr_c, &true);

    // Revoke addr_b
    client.set_investor_allowlisted(&addr_b, &false);

    // Full page scan should only return addr_a and addr_c
    let result = client.get_allowlisted_investors(&0, &10);
    assert_eq!(result.len(), 2);
    assert!(result.contains(&addr_a));
    assert!(result.contains(&addr_c));
    assert!(!result.contains(&addr_b));
}

// ── get_revoked_attestation_digests ───────────────────────────────────────────

fn setup_attestation_escrow(env: &Env) -> (crate::LiquifactEscrowClient<'_>, Address) {
    let client = super::deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "INV-ATT-PG"),
        &sme,
        &100_000_000_000i128,
        &800i64,
        &0u64,
        &Address::generate(env),
        &None,
        &Address::generate(env),
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
    (client, admin)
}

fn make_digest(seed: u8) -> soroban_sdk::BytesN<32> {
    let env = Env::default();
    soroban_sdk::BytesN::from_array(&env, &[seed; 32])
}

#[test]
fn get_revoked_attestation_digests_empty_log_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_attestation_escrow(&env);
    let result = client.get_revoked_attestation_digests(&0, &10);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_revoked_attestation_digests_zero_limit_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_attestation_escrow(&env);
    let digest = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    client.append_attestation_digest(&digest);
    client.revoke_attestation_digests(&soroban_sdk::vec![&env, 0u32]);
    let result = client.get_revoked_attestation_digests(&0, &0);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_revoked_attestation_digests_start_past_end_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_attestation_escrow(&env);
    let digest = soroban_sdk::BytesN::from_array(&env, &[2u8; 32]);
    client.append_attestation_digest(&digest);
    client.revoke_attestation_digests(&soroban_sdk::vec![&env, 0u32]);
    // Log has length 1, start=5 is past the end
    let result = client.get_revoked_attestation_digests(&5, &10);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_revoked_attestation_digests_no_revocations_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_attestation_escrow(&env);
    let digest = soroban_sdk::BytesN::from_array(&env, &[3u8; 32]);
    client.append_attestation_digest(&digest);
    // Entry exists but is not revoked
    let result = client.get_revoked_attestation_digests(&0, &10);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_revoked_attestation_digests_page_of_revoked_entries() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_attestation_escrow(&env);

    // Append 4 digests
    for seed in 0u8..4 {
        let digest = soroban_sdk::BytesN::from_array(&env, &[seed; 32]);
        client.append_attestation_digest(&digest);
    }

    // Revoke indices 1 and 3
    client.revoke_attestation_digests(&soroban_sdk::vec![&env, 1u32, 3u32]);

    // Full scan from start=0
    let result = client.get_revoked_attestation_digests(&0, &20);
    assert_eq!(result.len(), 2);
    assert_eq!(
        result.get(0).unwrap().digest,
        soroban_sdk::BytesN::from_array(&env, &[1u8; 32])
    );
    assert_eq!(
        result.get(1).unwrap().digest,
        soroban_sdk::BytesN::from_array(&env, &[3u8; 32])
    );
}

#[test]
fn get_revoked_attestation_digests_continuation_start() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_attestation_escrow(&env);

    // Append 4 digests and revoke all
    for seed in 0u8..4 {
        let digest = soroban_sdk::BytesN::from_array(&env, &[seed; 32]);
        client.append_attestation_digest(&digest);
    }
    client.revoke_attestation_digests(&soroban_sdk::vec![&env, 0u32, 1u32, 2u32, 3u32]);

    // Start at index 2 → should see digests at log positions 2 and 3
    let result = client.get_revoked_attestation_digests(&2, &10);
    assert_eq!(result.len(), 2);
    assert_eq!(
        result.get(0).unwrap().digest,
        soroban_sdk::BytesN::from_array(&env, &[2u8; 32])
    );
    assert_eq!(
        result.get(1).unwrap().digest,
        soroban_sdk::BytesN::from_array(&env, &[3u8; 32])
    );
}

// ── get_collateral_records ────────────────────────────────────────────────────

#[test]
fn get_collateral_records_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_attestation_escrow(&env);

    let result = client.get_collateral_records(&0, &10);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_collateral_records_page() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_attestation_escrow(&env);

    for i in 1..=4 {
        env.ledger().with_mut(|l| l.timestamp = (i as u64) * 100);
        client.record_sme_collateral_commitment(
            &soroban_sdk::Symbol::new(&env, "USDC"),
            &(i as i128),
        );
    }

    let result = client.get_collateral_records(&0, &2);
    assert_eq!(result.len(), 2);
    assert_eq!(result.get(0).unwrap().amount, 1);
    assert_eq!(result.get(1).unwrap().amount, 2);
}

#[test]
fn get_collateral_records_continuation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_attestation_escrow(&env);

    for i in 1..=4 {
        env.ledger().with_mut(|l| l.timestamp = (i as u64) * 100);
        client.record_sme_collateral_commitment(
            &soroban_sdk::Symbol::new(&env, "USDC"),
            &(i as i128),
        );
    }

    let result = client.get_collateral_records(&2, &5);
    assert_eq!(result.len(), 2);
    assert_eq!(result.get(0).unwrap().amount, 3);
    assert_eq!(result.get(1).unwrap().amount, 4);
}

#[test]
fn get_collateral_records_ceiling() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_attestation_escrow(&env);

    for i in 1..=55 {
        env.ledger().with_mut(|l| l.timestamp = (i as u64) * 100);
        client.record_sme_collateral_commitment(
            &soroban_sdk::Symbol::new(&env, "USDC"),
            &(i as i128),
        );
    }

    let result = client.get_collateral_records(&0, &100);
    assert_eq!(result.len(), 50);
}

// ── get_pause_records ──────────────────────────────────────────────────────────

// ── get_settlement_records ───────────────────────────────────────────────────

fn setup_settlement_escrow(
    env: &Env,
    invoice_id: &str,
    yield_bps: i64,
) -> (crate::LiquifactEscrowClient<'_>, Address, Address) {
    let client = super::deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let (token, treasury) = super::free_addresses(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, invoice_id),
        &sme,
        &100_000_000_000i128,
        &yield_bps,
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
    );
    (client, admin, sme)
}

#[test]
fn get_settlement_records_empty_before_settle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_settlement_escrow(&env, "SETL-PG-EMP", 800i64);

    let result = client.get_settlement_records(&0, &10);
    assert_eq!(result.len(), 0, "must be empty before any settle");
}

#[test]
fn get_settlement_records_zero_limit_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_settlement_escrow(&env, "SETL-PG-ZRL", 800i64);

    let investor = Address::generate(&env);
    client.fund(&investor, &100_000_000_000i128);
    client.settle();

    let result = client.get_settlement_records(&0, &0);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_settlement_records_start_past_end_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_settlement_escrow(&env, "SETL-PG-PST", 800i64);

    let investor = Address::generate(&env);
    client.fund(&investor, &100_000_000_000i128);
    client.settle();

    // Log has length 1, start=5 is past the end
    let result = client.get_settlement_records(&5, &10);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_settlement_records_single_record_after_settle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_settlement_escrow(&env, "SETL-PG-SGL", 500i64);

    let settle_ts: u64 = 42_000;
    env.ledger().with_mut(|l| l.timestamp = settle_ts);

    let investor = Address::generate(&env);
    client.fund(&investor, &100_000_000_000i128);
    client.settle();

    let result = client.get_settlement_records(&0, &10);
    assert_eq!(result.len(), 1, "one settlement record expected");

    let record = result.get(0).unwrap();
    assert_eq!(record.settled_at, settle_ts);
    assert_eq!(record.funded_amount, 100_000_000_000i128);
    assert_eq!(record.yield_bps, 500i64);
    assert_eq!(record.maturity, 0u64);

    // settle_pool = funded_amount + (funded_amount * yield_bps / 10_000)
    //            = 100_000_000_000 + (100_000_000_000 * 500 / 10_000)
    //            = 100_000_000_000 + 5_000_000_000
    //            = 105_000_000_000
    assert_eq!(record.settle_pool, 105_000_000_000i128);
}

#[test]
fn get_settlement_records_correct_settle_pool_with_max_yield() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sme) = setup_settlement_escrow(&env, "SETL-PG-YLD", 10_000i64);

    let investor = Address::generate(&env);
    client.fund(&investor, &500_000_000i128);
    client.settle();

    // settle_pool = 500_000_000 + (500_000_000 * 10_000 / 10_000)
    //            = 500_000_000 + 500_000_000
    //            = 1_000_000_000
    let result = client.get_settlement_records(&0, &10);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get(0).unwrap().settle_pool, 1_000_000_000i128);
}

// ── get_pause_records ──────────────────────────────────────────────────────────

fn push_pause_records(client: &crate::LiquifactEscrowClient<'_>, env: &Env, count: u32) {
    for _ in 0..count {
        env.ledger().with_mut(|l| l.timestamp += 1);
        client.set_paused(&true);
        env.ledger().with_mut(|l| l.timestamp += 1);
        client.set_paused(&false);
    }
}

#[test]
fn get_pause_records_empty_when_no_records() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = super::setup(&env);
    super::default_init(&client, &env, &admin, &sme);

    let result = client.get_pause_records(&0, &10);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_pause_records_zero_limit_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = super::setup(&env);
    super::default_init(&client, &env, &admin, &sme);

    push_pause_records(&client, &env, 3);

    let result = client.get_pause_records(&0, &0);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_pause_records_start_past_end_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = super::setup(&env);
    super::default_init(&client, &env, &admin, &sme);

    push_pause_records(&client, &env, 3);

    let result = client.get_pause_records(&5, &10);
    assert_eq!(result.len(), 0);
}

#[test]
fn get_pause_records_single_page() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = super::setup(&env);
    super::default_init(&client, &env, &admin, &sme);

    // Create 3 pause records
    push_pause_records(&client, &env, 3);

    let result = client.get_pause_records(&0, &10);
    assert_eq!(result.len(), 3);
    // Each record should have an activated_at timestamp (non-zero)
    for i in 0..3 {
        let record = result.get(i).unwrap();
        assert!(record.activated_at > 0, "record {i} missing activated_at");
    }
}

#[test]
fn get_pause_records_continuation_page() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = super::setup(&env);
    super::default_init(&client, &env, &admin, &sme);

    // Create 5 pause records
    push_pause_records(&client, &env, 5);

    // Page 1: first 3 records
    let page1 = client.get_pause_records(&0, &3);
    assert_eq!(page1.len(), 3);

    // Page 2: remaining 2 records
    let page2 = client.get_pause_records(&3, &3);
    assert_eq!(page2.len(), 2);

    // Verify no overlap and correct ordering
    let a = page1.get(2).unwrap().activated_at;
    let b = page2.get(0).unwrap().activated_at;
    assert!(a < b, "continuation records out of order");
}

#[test]
fn get_pause_records_ceiling_clamped() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = super::setup(&env);
    super::default_init(&client, &env, &admin, &sme);

    // Push more records than the ceiling
    let count = MAX_PAUSE_READ_PAGE + 10;
    push_pause_records(&client, &env, count);

    // Request well above the ceiling
    let result = client.get_pause_records(&0, &(MAX_PAUSE_READ_PAGE * 2));
    // Should be clamped to MAX_PAUSE_READ_PAGE, not the full requested amount
    assert_eq!(result.len(), MAX_PAUSE_READ_PAGE as u32);
}

#[test]
fn get_pause_records_ceiling_with_offset() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = super::setup(&env);
    super::default_init(&client, &env, &admin, &sme);

    let count = MAX_PAUSE_READ_PAGE + 10;
    push_pause_records(&client, &env, count);

    // Start past the ceiling, request more than ceiling
    let result = client.get_pause_records(&(MAX_PAUSE_READ_PAGE - 5), &(MAX_PAUSE_READ_PAGE * 2));
    // Should return at most 5 items (from offset to len, clamped by ceiling)
    assert_eq!(result.len(), 15);
}
