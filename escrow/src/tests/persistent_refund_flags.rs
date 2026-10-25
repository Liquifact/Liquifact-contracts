//! Tests for issue #1541: per-investor refund flags moved from instance storage to
//! persistent storage.
//!
//! Covers the acceptance criteria from the issue:
//! - After refunding 20 investors, instance storage has no `InvestorRefunded` keys and
//!   persistent storage has all 20 (inspected via `env.as_contract`).
//! - A legacy instance `InvestorRefunded` entry seeded via `env.as_contract` is honoured
//!   by `is_investor_refunded` (returns `true`) and `refund_batch` (skips the investor),
//!   and is migrated to persistent storage on the read path.
//! - `bump_ttl` and `batch_bump_ttl` extend the TTL of the persistent `InvestorRefunded`
//!   key without panicking when other per-investor keys are absent.

use super::*;
use crate::DataKey;
use soroban_sdk::testutils::storage::Persistent as _;

/// Set up a cancelled escrow with `n` funded investors using a real Stellar asset token.
///
/// Each investor is funded with `per_investor` base units. The funding target is set well
/// above `n * per_investor` so the escrow stays in status 0 (open) after funding, allowing
/// `cancel_funding` to transition it to status 4 (cancelled) for refunds.
///
/// Returns the client, the contract address, and the vector of investor addresses in
/// funding order.
fn setup_cancelled_with_investors<'a>(
    env: &'a Env,
    n: u32,
    per_investor: i128,
    invoice_id: &str,
) -> (LiquifactEscrowClient<'a>, Address, SorobanVec<Address>) {
    let (client, admin, sme) = setup(env);
    let token = install_stellar_asset_token(env);
    let treasury = Address::generate(env);
    // Target well above total contributions so status stays 0 (open) after funding.
    let target = per_investor.checked_mul(n as i128).unwrap() * 100;

    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &target,
        &800i64,
        &0u64,
        &token.id,
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

    let mut investors = SorobanVec::new(env);
    for _ in 0..n {
        let inv = Address::generate(env);
        token.stellar.mint(&inv, &per_investor);
        client.fund(&inv, &per_investor);
        investors.push_back(inv);
    }

    client.cancel_funding();
    let contract_addr = client.address.clone();
    (client, contract_addr, investors)
}

// ── Acceptance criterion 1: 20 refunds leave no instance keys, all in persistent ──

#[test]
fn refund_writes_persistent_not_instance_for_20_investors() {
    let env = Env::default();
    // 20 fund + 20 refund operations each perform a balance-checked SEP-41 transfer,
    // exceeding the default Env compute budget and resource limits. Relax both so
    // this storage-placement test measures behavior, not metering (same approach used
    // by the MAX_FUND_BATCH upper-bound test in tests/funding.rs).
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let n = 20u32;
    let per_investor = 1_000_000i128;
    let (client, contract_addr, investors) =
        setup_cancelled_with_investors(&env, n, per_investor, "PREFUND20");

    // Refund all 20 investors in a single batch.
    client.refund_batch(&investors);

    // Inspect storage directly via env.as_contract: instance must have no
    // InvestorRefunded keys, persistent must have all 20.
    for i in 0..n {
        let inv = investors.get(i).unwrap();
        let instance_has = env.as_contract(&contract_addr, || {
            env.storage()
                .instance()
                .has(&DataKey::InvestorRefunded(inv.clone()))
        });
        assert!(
            !instance_has,
            "instance storage still holds InvestorRefunded for investor index {}",
            i
        );
        let persistent_has = env.as_contract(&contract_addr, || {
            env.storage()
                .persistent()
                .has(&DataKey::InvestorRefunded(inv.clone()))
        });
        assert!(
            persistent_has,
            "persistent storage missing InvestorRefunded for investor index {}",
            i
        );
    }

    // Every investor is reported as refunded through the public API too.
    for i in 0..n {
        let inv = investors.get(i).unwrap();
        assert!(
            client.is_investor_refunded(&inv),
            "investor {} not refunded",
            i
        );
    }
}

// ── Acceptance criterion 2: legacy instance entry is honoured and migrated ───────

#[test]
fn is_investor_refunded_reads_legacy_instance_entry_and_migrates_it() {
    let env = Env::default();
    let (client, contract_addr, investors) =
        setup_cancelled_with_investors(&env, 1, 1_000_000i128, "PREFLEG");
    let investor = investors.get(0).unwrap();

    // Seed a legacy instance InvestorRefunded entry (pre-#1541 storage location).
    env.as_contract(&contract_addr, || {
        env.storage()
            .instance()
            .set(&DataKey::InvestorRefunded(investor.clone()), &true);
    });

    // Before the read: instance has the legacy entry, persistent does not.
    let instance_before = env.as_contract(&contract_addr, || {
        env.storage()
            .instance()
            .has(&DataKey::InvestorRefunded(investor.clone()))
    });
    assert!(instance_before, "legacy instance entry should be present");
    let persistent_before = env.as_contract(&contract_addr, || {
        env.storage()
            .persistent()
            .has(&DataKey::InvestorRefunded(investor.clone()))
    });
    assert!(
        !persistent_before,
        "persistent entry should not exist before migration"
    );

    // The public read returns true via the legacy instance entry.
    assert!(
        client.is_investor_refunded(&investor),
        "is_investor_refunded must honour the legacy instance entry"
    );

    // After the read: the legacy instance entry is migrated to persistent storage.
    let instance_after = env.as_contract(&contract_addr, || {
        env.storage()
            .instance()
            .has(&DataKey::InvestorRefunded(investor.clone()))
    });
    assert!(
        !instance_after,
        "legacy instance entry must be removed after migration"
    );
    let persistent_after = env.as_contract(&contract_addr, || {
        env.storage()
            .persistent()
            .has(&DataKey::InvestorRefunded(investor.clone()))
    });
    assert!(
        persistent_after,
        "persistent entry must be written after migration"
    );

    // A second read hits persistent storage directly (no instance entry to migrate).
    assert!(client.is_investor_refunded(&investor));
}

#[test]
fn refund_batch_skips_investor_with_legacy_instance_refund_flag() {
    let env = Env::default();
    let (client, contract_addr, investors) =
        setup_cancelled_with_investors(&env, 1, 1_000_000i128, "PREFSKIP");
    let investor = investors.get(0).unwrap();

    // Seed a legacy instance InvestorRefunded entry. The investor still has a
    // non-zero contribution because only the refund flag was seeded, not a real refund.
    env.as_contract(&contract_addr, || {
        env.storage()
            .instance()
            .set(&DataKey::InvestorRefunded(investor.clone()), &true);
    });

    let contribution_before = client.get_contribution(&investor);
    assert_eq!(contribution_before, 1_000_000i128);
    let distributed_before = client.get_distributed_principal();
    assert_eq!(distributed_before, 0i128);

    // refund_batch should skip this investor because investor_refunded() returns true
    // via the legacy instance entry.
    let mut batch = SorobanVec::new(&env);
    batch.push_back(investor.clone());
    client.refund_batch(&batch);

    // Contribution unchanged (refund skipped) and distributed_principal stays at 0.
    assert_eq!(
        client.get_contribution(&investor),
        1_000_000i128,
        "contribution must be unchanged when refund is skipped"
    );
    assert_eq!(
        client.get_distributed_principal(),
        0i128,
        "distributed_principal must not move when refund is skipped"
    );

    // The legacy entry was migrated to persistent storage during the skip check.
    let instance_has = env.as_contract(&contract_addr, || {
        env.storage()
            .instance()
            .has(&DataKey::InvestorRefunded(investor.clone()))
    });
    assert!(!instance_has, "legacy instance entry must be migrated away");
    let persistent_has = env.as_contract(&contract_addr, || {
        env.storage()
            .persistent()
            .has(&DataKey::InvestorRefunded(investor.clone()))
    });
    assert!(
        persistent_has,
        "persistent entry must exist after migration"
    );
}

// ── Double-refund safety through the legacy/persistent split ─────────────────────

#[test]
fn cannot_double_refund_through_legacy_persistent_split() {
    // A refunded investor cannot be refunded twice: the per-investor refund flag is
    // checked (persistent first, then legacy instance) before refund_impl zeroes the
    // contribution. Even if a legacy instance entry exists, investor_refunded() migrates
    // it to persistent and returns true, so refund_batch skips the investor.
    let env = Env::default();
    let (client, contract_addr, investors) =
        setup_cancelled_with_investors(&env, 1, 1_000_000i128, "PREFDBL");
    let investor = investors.get(0).unwrap();

    // First refund succeeds and writes the persistent flag.
    client.refund(&investor);
    assert!(client.is_investor_refunded(&investor));
    assert_eq!(client.get_contribution(&investor), 0i128);
    assert_eq!(client.get_distributed_principal(), 1_000_000i128);

    // Inject a legacy instance entry to simulate a stale pre-#1541 flag.
    env.as_contract(&contract_addr, || {
        env.storage()
            .instance()
            .set(&DataKey::InvestorRefunded(investor.clone()), &true);
    });

    // refund_batch skips the investor (both persistent and legacy say refunded).
    let mut batch = SorobanVec::new(&env);
    batch.push_back(investor.clone());
    client.refund_batch(&batch);

    // No second payout: distributed_principal unchanged, contribution still zero.
    assert_eq!(client.get_distributed_principal(), 1_000_000i128);
    assert_eq!(client.get_contribution(&investor), 0i128);

    // The legacy instance entry was migrated away during the skip check.
    let instance_has = env.as_contract(&contract_addr, || {
        env.storage()
            .instance()
            .has(&DataKey::InvestorRefunded(investor.clone()))
    });
    assert!(!instance_has);
}

// ── TTL extension covers the persistent refund flag ──────────────────────────────

#[test]
fn batch_bump_ttl_extends_investor_refunded_ttl() {
    let env = Env::default();
    let (client, contract_addr, investors) =
        setup_cancelled_with_investors(&env, 1, 1_000_000i128, "PREFBBT");
    let investor = investors.get(0).unwrap();

    // Refund so InvestorRefunded is written to persistent storage.
    client.refund(&investor);

    let key = DataKey::InvestorRefunded(investor.clone());
    let ttl_before = env.as_contract(&contract_addr, || env.storage().persistent().get_ttl(&key));

    // Advance the ledger so the TTL decreases.
    let mut ledger = env.ledger().get();
    ledger.sequence_number += 500;
    env.ledger().set(ledger);

    let ttl_mid = env.as_contract(&contract_addr, || env.storage().persistent().get_ttl(&key));
    assert!(
        ttl_mid < ttl_before,
        "TTL should decrease after ledger advance ({} -> {})",
        ttl_before,
        ttl_mid
    );

    // batch_bump_ttl must extend the InvestorRefunded TTL.
    let mut keys = SorobanVec::new(&env);
    keys.push_back(DataKey::InvestorRefunded(investor.clone()));
    client.batch_bump_ttl(&keys);

    let ttl_after = env.as_contract(&contract_addr, || env.storage().persistent().get_ttl(&key));
    assert!(
        ttl_after > ttl_mid,
        "TTL must be extended by batch_bump_ttl ({} -> {})",
        ttl_mid,
        ttl_after
    );
}

#[test]
fn bump_ttl_extends_investor_refunded_ttl_when_refunded() {
    let env = Env::default();
    let (client, _contract_addr, investors) =
        setup_cancelled_with_investors(&env, 1, 1_000_000i128, "PREFBTTL");
    let investor = investors.get(0).unwrap();

    // Allowlist the investor so the first bump_ttl loop (which extends
    // InvestorAllowlisted unconditionally) does not panic on an absent key.
    client.set_investor_allowlisted(&investor, &true);

    // Refund so InvestorRefunded exists in persistent storage.
    client.refund(&investor);

    // bump_ttl must not panic even though InvestorClaimed is absent for this
    // investor (the escrow was cancelled, not settled, so no claim occurred).
    let mut addrs = SorobanVec::new(&env);
    addrs.push_back(investor.clone());
    client.bump_ttl(&addrs);
}
