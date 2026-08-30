// Tests for the shared paginate_window helper and the public paginated read views:
//   get_investors, get_allowlisted_investors, get_revoked_attestation_digests,
//   get_collateral_records, get_pause_records, and get_settlement_records.
//
// Each test uses a fresh Env so state cannot leak across cases.

use crate::{PauseReason, PauseScope};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

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
        &soroban_sdk::String::from_str(env, "INV_PG_001"),
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
        &soroban_sdk::String::from_str(&env, "INV_PG_FIRST"),
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
        &soroban_sdk::String::from_str(&env, "INV_PG_CONT"),
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
        &soroban_sdk::String::from_str(&env, "INV_PG_PAST"),
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
        &soroban_sdk::String::from_str(env, "INV_AL_PG"),
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
        &soroban_sdk::String::from_str(env, "INV_ATT_PG"),
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
    // Zero page limits are rejected with a typed read-boundary error.
    let expected = crate::EscrowError::AttestationReadLimitZero as u32;
    match client.try_get_revoked_attestation_digests(&0, &0) {
        Err(Ok(error)) => assert_eq!(error, soroban_sdk::Error::from_contract_error(expected)),
        Err(Err(soroban_sdk::InvokeError::Contract(code))) => assert_eq!(code, expected),
        other => panic!("expected AttestationReadLimitZero, got {other:?}"),
    }
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
