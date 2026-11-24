use super::super::{LiquifactEscrow, LiquifactEscrowClient, YieldTier};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec as SorobanVec};

fn setup(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    let id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &id);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    (client, admin, sme)
}

#[test]
fn test_yield_tier_config_defaults_before_init() {
    let env = Env::default();
    let (client, _admin, _sme) = setup(&env);

    let cfg = client.get_yield_tier_config();

    assert_eq!(cfg.base_yield_bps, 0);
    assert!(cfg.tiers.is_empty());
}

#[test]
fn test_yield_tier_config_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);

    let mut tiers: SorobanVec<YieldTier> = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 86_400,
        yield_bps: 1000,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 604_800,
        yield_bps: 1200,
    });

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INVTier"),
        &sme,
        &10_000i128,
        &800i64, // base_yield_bps
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &Some(tiers.clone()),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );

    let cfg = client.get_yield_tier_config();

    assert_eq!(cfg.base_yield_bps, 800);
    assert_eq!(cfg.tiers.len(), 2);
    assert_eq!(cfg.tiers.get_unchecked(0), tiers.get_unchecked(0));
    assert_eq!(cfg.tiers.get_unchecked(1), tiers.get_unchecked(1));
    assert_eq!(cfg.tiers, client.get_yield_tiers());
}

#[test]
fn test_yield_tier_config_after_init_without_tiers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INVBase"),
        &sme,
        &10_000i128,
        &750i64, // base_yield_bps
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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

    let cfg = client.get_yield_tier_config();

    assert_eq!(cfg.base_yield_bps, 750);
    assert!(cfg.tiers.is_empty());
    assert_eq!(cfg.tiers, client.get_yield_tiers());
}
