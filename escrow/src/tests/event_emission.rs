//! Event-emission contract for every state-mutating escrow entrypoint.
//!
//! Off-chain indexers (`docs/escrow-indexer.md`) reconstruct state from `topic[0]` of
//! contract events (the stable event-name topic emitted by `#[contractevent]`).
//! A mutating entrypoint that writes storage without publishing an event silently
//! breaks that reconciliation. This module makes the guarantee executable:
//!
//! - [`CASES`] is a hand-maintained table of `(group, entrypoint, expected topic,
//!   setup, call)`. Every case runs in a fresh `Env`, snapshots `env.events().all()`,
//!   invokes exactly one mutating entrypoint via `call`, then requires the count to
//!   grow by at least one and an event with the expected `topic[0]` to be present.
//! - [`EXEMPT`] lists every non-mutating (or intentionally event-free) `pub fn`
//!   with a reason string.
//! - `all_pub_fns_are_hand_maintained` and `every_pub_fn_is_mapped_or_exempt`
//!   parse the real `lib.rs` source at compile time and fail when a new `pub fn`
//!   is added without being added to this table or to the exemption list.
//!
//! Note on the Soroban test host: events are cleared at the start of every
//! top-level invocation. The runner therefore performs a read-only
//! `get_version()` call after setup, takes the "before" snapshot (normally
//! empty), and then invokes the mutator under test.

use super::{
    deploy, deploy_with_id, install_stellar_asset_token, LiquifactEscrowClient, StellarTestToken,
};
use crate::{
    FeeSchedule, LiquifactEscrow, PauseReason, PauseScope, DEFAULT_MATURITY_MAX_HORIZON_SECS,
    MAX_MATURITY_MAX_HORIZON_SECS, MAX_PAUSE_TOGGLE_LIMIT, MIN_ADMIN_PROPOSAL_VALIDITY_SECS,
    MIN_MATURITY_MAX_HORIZON_SECS, MIN_PAUSE_MAX_DURATION_SECS, MIN_PAUSE_TOGGLE_WINDOW_SECS,
    UPGRADE_TIMELOCK_SECS,
};
use soroban_sdk::xdr::{ContractEventBody, ScVal};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    vec, Address, Bytes, BytesN, Env, String, Symbol, Vec as SorobanVec,
};

const AMOUNT: i128 = 1_000_000;
const YIELD_BPS: i64 = 400;
const COUPON: i128 = AMOUNT * (YIELD_BPS as i128) / 10_000;
/// Minimal WASM module with the `contractenvmetav0` custom section the Soroban
/// host requires before it will swap contract code. The section payload is
/// XDR `ScEnvMetaEntry::ScEnvMetaKindInterfaceVersion { protocol: 25,
/// pre_release: 0 }`.
const MINIMAL_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, // magic
    0x01, 0x00, 0x00, 0x00, // version
    0x00, // custom section id
    0x1e, // payload length: 1 + 17 name bytes + 12 XDR bytes
    0x11, b'c', b'o', b'n', b't', b'r', b'a', b'c', b't', b'e', b'n', b'v', b'm', b'e', b't', b'a',
    b'v', b'0', // "contractenvmetav0"
    0x00, 0x00, 0x00, 0x00, // ScEnvMetaKindInterfaceVersion discriminant
    0x00, 0x00, 0x00, 0x19, // protocol 25
    0x00, 0x00, 0x00, 0x00, // pre-release 0
];

// ── Harness ──────────────────────────────────────────────────────────────────

struct Harness<'a> {
    client: LiquifactEscrowClient<'a>,
    id: Address,
    admin: Address,
    sme: Address,
    treasury: Address,
    token_id: Address,
    token: Option<StellarTestToken<'a>>,
    investor: Address,
    investor2: Address,
    other: Address,
    digest: BytesN<32>,
    digest2: BytesN<32>,
    reference: BytesN<32>,
    hash: BytesN<32>,
}

#[derive(Clone, Copy)]
struct InitOpts {
    amount: i128,
    yield_bps: i64,
    maturity: u64,
    min_contribution: Option<i128>,
    max_unique_investors: Option<u32>,
    max_per_investor: Option<i128>,
    legal_hold_clear_delay: Option<u64>,
    maturity_max_horizon: Option<u64>,
    funding_deadline: Option<u64>,
    allowlist_active: Option<bool>,
    protocol_fee_bps: Option<i64>,
}

impl Default for InitOpts {
    fn default() -> Self {
        Self {
            amount: AMOUNT,
            yield_bps: YIELD_BPS,
            maturity: 0,
            min_contribution: None,
            max_unique_investors: None,
            max_per_investor: None,
            legal_hold_clear_delay: None,
            maturity_max_horizon: None,
            funding_deadline: None,
            allowlist_active: None,
            protocol_fee_bps: None,
        }
    }
}

fn new_harness<'a>(env: &'a Env, real_token: bool) -> Harness<'a> {
    env.mock_all_auths();
    let token = if real_token {
        Some(install_stellar_asset_token(env))
    } else {
        None
    };
    let (id, client) = deploy_with_id(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);
    let token_id = token
        .as_ref()
        .map(|t| t.id.clone())
        .unwrap_or_else(|| Address::generate(env));
    Harness {
        client,
        id,
        admin,
        sme,
        treasury,
        token_id,
        token,
        investor: Address::generate(env),
        investor2: Address::generate(env),
        other: Address::generate(env),
        digest: BytesN::<32>::from_array(env, &[1u8; 32]),
        digest2: BytesN::<32>::from_array(env, &[2u8; 32]),
        reference: BytesN::<32>::from_array(env, &[3u8; 32]),
        hash: BytesN::<32>::from_array(env, &[4u8; 32]),
    }
}

fn initialize(h: &Harness<'_>, env: &Env, opts: InitOpts) {
    h.client.init(
        &h.admin,
        &String::from_str(env, "EVTCOV"),
        &h.sme,
        &opts.amount,
        &opts.yield_bps,
        &opts.maturity,
        &h.token_id,
        &None,
        &h.treasury,
        &None,
        &opts.min_contribution,
        &opts.max_unique_investors,
        &opts.max_per_investor,
        &opts.legal_hold_clear_delay,
        &opts.maturity_max_horizon,
        &opts.funding_deadline,
        &opts.allowlist_active,
        &opts.protocol_fee_bps,
    );
}

fn schedule(activation_ledger: u32, fee_bps: u32) -> FeeSchedule {
    FeeSchedule {
        fee_bps,
        min_fee_bps: 0,
        max_fee_bps: 10_000,
        activation_ledger,
    }
}

fn upload_wasm(env: &Env) -> BytesN<32> {
    env.deployer()
        .upload_contract_wasm(Bytes::from_slice(env, MINIMAL_WASM))
}

// ── Setup functions ──────────────────────────────────────────────────────────

fn setup_uninitialized(env: &Env) -> Harness<'_> {
    new_harness(env, false)
}

fn setup_plain(env: &Env) -> Harness<'_> {
    let h = new_harness(env, false);
    initialize(&h, env, InitOpts::default());
    h
}

fn setup_capped(env: &Env) -> Harness<'_> {
    let h = new_harness(env, false);
    initialize(
        &h,
        env,
        InitOpts {
            min_contribution: Some(100),
            max_unique_investors: Some(10),
            max_per_investor: Some(1_000),
            ..InitOpts::default()
        },
    );
    h
}

fn setup_with_deadline(env: &Env) -> Harness<'_> {
    let h = new_harness(env, false);
    initialize(
        &h,
        env,
        InitOpts {
            funding_deadline: Some(1_000),
            ..InitOpts::default()
        },
    );
    h
}

fn setup_partial_funded(env: &Env) -> Harness<'_> {
    let h = setup_plain(env);
    h.client.fund(&h.investor, &1_000);
    h
}

fn setup_expired_funding(env: &Env) -> Harness<'_> {
    // `setup_with_deadline` uses a deadline of 1_000; move past it with some principal in.
    let h = setup_with_deadline(env);
    h.client.fund(&h.investor, &1_000);
    env.ledger().set_timestamp(1_001);
    h
}

fn setup_real_funded(env: &Env) -> Harness<'_> {
    let h = new_harness(env, true);
    initialize(&h, env, InitOpts::default());
    h.token.as_ref().unwrap().stellar.mint(&h.investor, &AMOUNT);
    h.client.fund(&h.investor, &AMOUNT);
    // Pre-fund the coupon so `settle` needs no SME shortfall transfer.
    h.token.as_ref().unwrap().stellar.mint(&h.sme, &COUPON);
    h
}

fn setup_real_settled(env: &Env) -> Harness<'_> {
    let h = setup_real_funded(env);
    h.client.settle();
    h
}

fn setup_real_withdrawn(env: &Env) -> Harness<'_> {
    let h = setup_real_funded(env);
    h.client.withdraw();
    h
}

fn setup_real_cancelled(env: &Env) -> Harness<'_> {
    // Cancel while the escrow is still open (partially funded); a fully funded
    // escrow is status 1 and `cancel_funding` correctly rejects it.
    let h = new_harness(env, true);
    initialize(&h, env, InitOpts::default());
    h.token.as_ref().unwrap().stellar.mint(&h.investor, &1_000);
    h.client.fund(&h.investor, &1_000);
    h.client.cancel_funding();
    h
}

fn setup_hold(env: &Env) -> Harness<'_> {
    let h = setup_plain(env);
    h.client.set_legal_hold(&true);
    h
}

fn setup_clear_requested(env: &Env) -> Harness<'_> {
    let h = setup_hold(env);
    h.client.request_clear_legal_hold();
    h
}

fn setup_fee_pending(env: &Env) -> Harness<'_> {
    let h = setup_plain(env);
    h.client.submit_fee_schedule(&schedule(101, 100));
    h
}

fn setup_fee_activatable(env: &Env) -> Harness<'_> {
    let h = setup_fee_pending(env);
    env.ledger().set_sequence_number(101);
    h
}

fn setup_pauser(env: &Env) -> Harness<'_> {
    let h = setup_plain(env);
    h.client.set_pauser(&Some(h.other.clone()));
    h
}

fn setup_attestor(env: &Env) -> Harness<'_> {
    let h = setup_plain(env);
    h.client.set_attestor(&Some(h.other.clone()));
    h
}

fn setup_attestation(env: &Env) -> Harness<'_> {
    let h = setup_plain(env);
    h.client.append_attestation_digest(&h.digest.clone());
    h
}

fn setup_attestation_revoked(env: &Env) -> Harness<'_> {
    let h = setup_attestation(env);
    h.client.revoke_attestation_digest(&0);
    h
}

fn setup_collateral(env: &Env) -> Harness<'_> {
    let h = setup_plain(env);
    h.client
        .record_sme_collateral_commitment(&symbol_short!("USDC"), &50_000);
    h
}

fn setup_admin_proposed(env: &Env) -> Harness<'_> {
    let h = setup_plain(env);
    h.client.propose_admin(&h.other.clone(), &None);
    h
}

fn setup_admin_expired(env: &Env) -> Harness<'_> {
    let h = setup_plain(env);
    h.client
        .propose_admin(&h.other.clone(), &Some(MIN_ADMIN_PROPOSAL_VALIDITY_SECS));
    env.ledger()
        .set_timestamp(MIN_ADMIN_PROPOSAL_VALIDITY_SECS + 1);
    h
}

fn setup_upgrade_proposed(env: &Env) -> Harness<'_> {
    let mut h = setup_plain(env);
    h.hash = upload_wasm(env);
    let eta = h.client.propose_upgrade(&h.hash.clone());
    env.ledger().set_timestamp(eta);
    h
}

fn setup_upgrade_cancelable(env: &Env) -> Harness<'_> {
    let h = setup_plain(env);
    h.client.propose_upgrade(&h.hash.clone());
    h
}

fn setup_callback(env: &Env) -> Harness<'_> {
    let h = setup_plain(env);
    h.client.register_callback(&h.other.clone(), &1);
    h
}

// ── Call functions (one mutating entrypoint each) ────────────────────────────

type SetupFn = for<'a> fn(&'a Env) -> Harness<'a>;
type CallFn = for<'a> fn(&'a Env, &mut Harness<'a>);

fn call_init(env: &Env, h: &mut Harness<'_>) {
    initialize(h, env, InitOpts::default());
}

fn call_rebind_registry(env: &Env, h: &mut Harness<'_>) {
    h.client.rebind_registry_ref(&Some(h.other.clone()));
    let _ = env;
}

fn call_clear_registry(_env: &Env, h: &mut Harness<'_>) {
    h.client.clear_registry_ref();
}

fn call_rotate_beneficiary(_env: &Env, h: &mut Harness<'_>) {
    h.client.rotate_beneficiary(&h.other.clone());
}

fn call_sweep_dust(env: &Env, h: &mut Harness<'_>) {
    h.token
        .as_ref()
        .unwrap()
        .stellar
        .mint(&h.id.clone(), &1_000);
    h.client.sweep_terminal_dust(&1_000);
    let _ = env;
}

fn call_propose_admin(_env: &Env, h: &mut Harness<'_>) {
    h.client.propose_admin(&h.other.clone(), &None);
}

fn call_accept_admin(_env: &Env, h: &mut Harness<'_>) {
    h.client.accept_admin();
}

fn call_transfer_admin(_env: &Env, h: &mut Harness<'_>) {
    #[allow(deprecated)]
    h.client.transfer_admin(&h.other.clone());
}

fn call_cancel_admin(_env: &Env, h: &mut Harness<'_>) {
    h.client.cancel_pending_admin();
}

fn call_recover_admin(env: &Env, h: &mut Harness<'_>) {
    h.client
        .recover_admin(&String::from_str(env, "proposal expired"));
}

fn call_propose_upgrade(_env: &Env, h: &mut Harness<'_>) {
    h.client.propose_upgrade(&h.hash.clone());
}

fn call_execute_upgrade(_env: &Env, h: &mut Harness<'_>) {
    h.client.execute_upgrade();
}

fn call_cancel_upgrade(_env: &Env, h: &mut Harness<'_>) {
    h.client.cancel_upgrade();
}

fn call_upgrade(_env: &Env, h: &mut Harness<'_>) {
    #[allow(deprecated)]
    h.client.upgrade(&h.hash.clone());
}

fn call_register_callback(_env: &Env, h: &mut Harness<'_>) {
    h.client.register_callback(&h.other.clone(), &1);
}

fn call_execute_callback(_env: &Env, h: &mut Harness<'_>) {
    h.client.execute_callback(&1, &h.other.clone(), &1);
}

fn call_submit_fee_schedule(env: &Env, h: &mut Harness<'_>) {
    h.client
        .submit_fee_schedule(&schedule(env.ledger().sequence() as u32 + 1, 100));
}

fn call_activate_fee_schedule(_env: &Env, h: &mut Harness<'_>) {
    assert!(h.client.activate_fee_schedule());
}

fn call_cancel_fee_schedule(_env: &Env, h: &mut Harness<'_>) {
    h.client.cancel_pending_fee_schedule();
}

fn call_set_protocol_fee(_env: &Env, h: &mut Harness<'_>) {
    h.client.set_protocol_fee_bps(&100);
}

fn call_set_performance_fee(_env: &Env, h: &mut Harness<'_>) {
    h.client.set_performance_fee_bps(&1_000);
}

fn call_set_partial_close_bps(_env: &Env, h: &mut Harness<'_>) {
    h.client.set_partial_close_min_bps(&5_000);
}

fn call_set_paused(_env: &Env, h: &mut Harness<'_>) {
    h.client
        .set_paused(&true, &PauseScope::All, &PauseReason::Incident);
}

fn call_pause_as_pauser(_env: &Env, h: &mut Harness<'_>) {
    h.client
        .pause_as_pauser(&h.other.clone(), &PauseScope::All, &PauseReason::Incident);
}

fn call_set_pauser(_env: &Env, h: &mut Harness<'_>) {
    h.client.set_pauser(&Some(h.other.clone()));
}

fn call_set_pause_max(_env: &Env, h: &mut Harness<'_>) {
    h.client
        .set_pause_max_duration(&MIN_PAUSE_MAX_DURATION_SECS);
}

fn call_set_pause_cooldown(_env: &Env, h: &mut Harness<'_>) {
    h.client.set_pause_cooldown(&MIN_PAUSE_TOGGLE_WINDOW_SECS);
}

fn call_set_pause_rate_limit(_env: &Env, h: &mut Harness<'_>) {
    h.client
        .set_pause_rate_limit(&MAX_PAUSE_TOGGLE_LIMIT, &MIN_PAUSE_TOGGLE_WINDOW_SECS);
}

fn call_set_hold(_env: &Env, h: &mut Harness<'_>) {
    h.client.set_legal_hold(&true);
}

fn call_request_clear_hold(_env: &Env, h: &mut Harness<'_>) {
    h.client.request_clear_legal_hold();
}

fn call_clear_hold(_env: &Env, h: &mut Harness<'_>) {
    h.client.clear_legal_hold();
}

fn call_clear_hold_after_delay(_env: &Env, h: &mut Harness<'_>) {
    h.client.clear_legal_hold_after_delay();
}

fn call_cancel_clear_hold(_env: &Env, h: &mut Harness<'_>) {
    h.client.cancel_clear_legal_hold();
}

fn call_set_allowlist_active(_env: &Env, h: &mut Harness<'_>) {
    h.client.set_allowlist_active(&true);
}

fn call_set_investor_allowlisted(_env: &Env, h: &mut Harness<'_>) {
    h.client
        .set_investor_allowlisted(&h.investor.clone(), &true);
}

fn call_set_investors_allowlisted(env: &Env, h: &mut Harness<'_>) {
    h.client
        .set_investors_allowlisted(&vec![env, h.investor.clone(), h.investor2.clone()], &true);
}

fn call_set_investor_frozen(_env: &Env, h: &mut Harness<'_>) {
    h.client.set_investor_frozen(&h.investor.clone(), &true, &7);
}

fn call_set_investor_expiry(_env: &Env, h: &mut Harness<'_>) {
    h.client
        .set_investor_allowlist_expiry(&h.investor.clone(), &1_000);
}

fn call_set_investors_expiry(env: &Env, h: &mut Harness<'_>) {
    h.client.set_investors_allowlist_expiry(
        &vec![env, h.investor.clone(), h.investor2.clone()],
        &1_000,
    );
}

fn call_bind_attestation(_env: &Env, h: &mut Harness<'_>) {
    h.client.bind_primary_attestation_hash(&h.digest.clone());
}

fn call_require_attestation(_env: &Env, h: &mut Harness<'_>) {
    h.client.set_require_attest_withdraw(&true);
}

fn call_set_attestor(_env: &Env, h: &mut Harness<'_>) {
    h.client.set_attestor(&Some(h.other.clone()));
}

fn call_append_attestation(_env: &Env, h: &mut Harness<'_>) {
    h.client.append_attestation_digest(&h.digest.clone());
}

fn call_append_attestation_as(_env: &Env, h: &mut Harness<'_>) {
    h.client
        .append_attestation_digest_as(&h.other.clone(), &h.digest2.clone());
}

fn call_revoke_attestation(_env: &Env, h: &mut Harness<'_>) {
    h.client.revoke_attestation_digest(&0);
}

fn call_revoke_attestations(env: &Env, h: &mut Harness<'_>) {
    h.client.revoke_attestation_digests(&vec![env, 0u32]);
}

fn call_unrevoke_attestation(_env: &Env, h: &mut Harness<'_>) {
    h.client.unrevoke_attestation_digest(&0);
}

fn call_record_collateral(_env: &Env, h: &mut Harness<'_>) {
    h.client
        .record_sme_collateral_commitment(&symbol_short!("USDC"), &50_000);
}

fn call_batch_record_collateral(env: &Env, h: &mut Harness<'_>) {
    h.client.batch_record_collateral(&vec![
        env,
        (symbol_short!("USDC"), 10_000i128),
        (symbol_short!("USDC"), 20_000i128),
    ]);
}

fn call_clear_collateral(_env: &Env, h: &mut Harness<'_>) {
    h.client.clear_sme_collateral_commitment();
}

fn call_fund(_env: &Env, h: &mut Harness<'_>) {
    h.client.fund(&h.investor.clone(), &1_000);
}

fn call_fund_with_commitment(_env: &Env, h: &mut Harness<'_>) {
    h.client
        .fund_with_commitment(&h.investor.clone(), &1_000, &0);
}

fn call_fund_with_reference(_env: &Env, h: &mut Harness<'_>) {
    h.client
        .fund_with_reference(&h.investor.clone(), &1_000, &h.reference.clone());
}

fn call_fund_batch(env: &Env, h: &mut Harness<'_>) {
    h.client.fund_batch(&vec![
        env,
        (h.investor.clone(), 1_000i128),
        (h.investor2.clone(), 1_000i128),
    ]);
}

fn call_cancel_funding(_env: &Env, h: &mut Harness<'_>) {
    h.client.cancel_funding();
}

fn call_expire_funding(_env: &Env, h: &mut Harness<'_>) {
    h.client.expire_funding();
}

fn call_refund(_env: &Env, h: &mut Harness<'_>) {
    h.client.refund(&h.investor.clone());
}

fn call_refund_batch(env: &Env, h: &mut Harness<'_>) {
    h.client.refund_batch(&vec![env, h.investor.clone()]);
}

fn call_unfund(_env: &Env, h: &mut Harness<'_>) {
    h.client.unfund(&h.investor.clone(), &500);
}

fn call_update_target(_env: &Env, h: &mut Harness<'_>) {
    h.client.update_funding_target(&(AMOUNT / 2));
}

fn call_lower_unique(_env: &Env, h: &mut Harness<'_>) {
    h.client.lower_max_unique_investors(&5);
}

fn call_raise_unique(_env: &Env, h: &mut Harness<'_>) {
    h.client.raise_max_unique_investors(&20);
}

fn call_lower_floor(_env: &Env, h: &mut Harness<'_>) {
    h.client.lower_min_contribution_floor(&50);
}

fn call_raise_per_investor(_env: &Env, h: &mut Harness<'_>) {
    h.client.raise_max_per_investor(&2_000_000);
}

fn call_update_maturity(_env: &Env, h: &mut Harness<'_>) {
    h.client.update_maturity(&1_000);
}

fn call_update_yield(_env: &Env, h: &mut Harness<'_>) {
    h.client.update_yield_bps(&500);
}

fn call_extend_deadline(_env: &Env, h: &mut Harness<'_>) {
    h.client.extend_funding_deadline(&2_000);
}

fn call_update_horizon(_env: &Env, h: &mut Harness<'_>) {
    h.client
        .update_maturity_max_horizon(&MIN_MATURITY_MAX_HORIZON_SECS);
}

fn call_raise_horizon(_env: &Env, h: &mut Harness<'_>) {
    h.client
        .raise_maturity_max_horizon(&MAX_MATURITY_MAX_HORIZON_SECS);
}

fn call_partial_settle(_env: &Env, h: &mut Harness<'_>) {
    h.client.partial_settle(&h.sme.clone());
}

fn call_settle(_env: &Env, h: &mut Harness<'_>) {
    h.client.settle();
}

fn call_settle_batch(env: &Env, h: &mut Harness<'_>) {
    let caller = deploy(env);
    caller.settle_batch(&vec![env, h.id.clone()]);
}

fn call_repay(env: &Env, h: &mut Harness<'_>) {
    let pool = h.client.get_settlement_pool();
    h.token
        .as_ref()
        .unwrap()
        .stellar
        .mint(&h.other.clone(), &pool);
    h.client.repay(&h.other.clone(), &pool);
    let _ = env;
}

fn call_withdraw(_env: &Env, h: &mut Harness<'_>) {
    h.client.withdraw();
}

fn call_claim(_env: &Env, h: &mut Harness<'_>) {
    h.client.claim_investor_payout(&h.investor.clone());
}

fn call_push_payouts(env: &Env, h: &mut Harness<'_>) {
    h.client
        .push_investor_payouts(&vec![env, h.investor.clone()]);
}

// ── The (entrypoint, setup, call) coverage table ─────────────────────────────

struct Case {
    group: &'static str,
    entrypoint: &'static str,
    topic: &'static str,
    setup: SetupFn,
    call: CallFn,
}

const CASES: &[Case] = &[
    // admin / lifecycle
    Case {
        group: "admin",
        entrypoint: "init",
        topic: "escrow_ii",
        setup: setup_uninitialized,
        call: call_init,
    },
    Case {
        group: "admin",
        entrypoint: "rebind_registry_ref",
        topic: "reg_rebind",
        setup: setup_plain,
        call: call_rebind_registry,
    },
    Case {
        group: "admin",
        entrypoint: "clear_registry_ref",
        topic: "reg_rebind",
        setup: setup_plain,
        call: call_clear_registry,
    },
    Case {
        group: "admin",
        entrypoint: "rotate_beneficiary",
        topic: "ben_rot",
        setup: setup_plain,
        call: call_rotate_beneficiary,
    },
    Case {
        group: "admin",
        entrypoint: "sweep_terminal_dust",
        topic: "dust_sw",
        setup: setup_real_withdrawn,
        call: call_sweep_dust,
    },
    Case {
        group: "admin",
        entrypoint: "propose_admin",
        topic: "adm_prop",
        setup: setup_plain,
        call: call_propose_admin,
    },
    Case {
        group: "admin",
        entrypoint: "accept_admin",
        topic: "adm_acc",
        setup: setup_admin_proposed,
        call: call_accept_admin,
    },
    Case {
        group: "admin",
        entrypoint: "transfer_admin",
        topic: "depr_xfer",
        setup: setup_plain,
        call: call_transfer_admin,
    },
    Case {
        group: "admin",
        entrypoint: "cancel_pending_admin",
        topic: "adm_can",
        setup: setup_admin_proposed,
        call: call_cancel_admin,
    },
    Case {
        group: "admin",
        entrypoint: "recover_admin",
        topic: "adm_rec",
        setup: setup_admin_expired,
        call: call_recover_admin,
    },
    Case {
        group: "admin",
        entrypoint: "propose_upgrade",
        topic: "upg_prop",
        setup: setup_plain,
        call: call_propose_upgrade,
    },
    Case {
        group: "admin",
        entrypoint: "execute_upgrade",
        topic: "upgrade",
        setup: setup_upgrade_proposed,
        call: call_execute_upgrade,
    },
    Case {
        group: "admin",
        entrypoint: "cancel_upgrade",
        topic: "upg_cncl",
        setup: setup_upgrade_cancelable,
        call: call_cancel_upgrade,
    },
    Case {
        group: "admin",
        entrypoint: "upgrade",
        topic: "depr_upg",
        setup: setup_plain,
        call: call_upgrade,
    },
    Case {
        group: "admin",
        entrypoint: "register_callback",
        topic: "cb_reg",
        setup: setup_plain,
        call: call_register_callback,
    },
    Case {
        group: "admin",
        entrypoint: "execute_callback",
        topic: "cb_exec",
        setup: setup_callback,
        call: call_execute_callback,
    },
    // fee
    Case {
        group: "fee",
        entrypoint: "submit_fee_schedule",
        topic: "fee_sub",
        setup: setup_plain,
        call: call_submit_fee_schedule,
    },
    Case {
        group: "fee",
        entrypoint: "activate_fee_schedule",
        topic: "fee_act",
        setup: setup_fee_activatable,
        call: call_activate_fee_schedule,
    },
    Case {
        group: "fee",
        entrypoint: "cancel_pending_fee_schedule",
        topic: "fee_can",
        setup: setup_fee_pending,
        call: call_cancel_fee_schedule,
    },
    Case {
        group: "fee",
        entrypoint: "set_protocol_fee_bps",
        topic: "fee_upd",
        setup: setup_plain,
        call: call_set_protocol_fee,
    },
    Case {
        group: "fee",
        entrypoint: "set_performance_fee_bps",
        topic: "perf_upd",
        setup: setup_plain,
        call: call_set_performance_fee,
    },
    Case {
        group: "fee",
        entrypoint: "set_partial_close_min_bps",
        topic: "part_bps",
        setup: setup_plain,
        call: call_set_partial_close_bps,
    },
    // pause / legal hold
    Case {
        group: "pause",
        entrypoint: "set_paused",
        topic: "paused",
        setup: setup_plain,
        call: call_set_paused,
    },
    Case {
        group: "pause",
        entrypoint: "pause_as_pauser",
        topic: "paused",
        setup: setup_pauser,
        call: call_pause_as_pauser,
    },
    Case {
        group: "pause",
        entrypoint: "set_pauser",
        topic: "pauser",
        setup: setup_plain,
        call: call_set_pauser,
    },
    Case {
        group: "pause",
        entrypoint: "set_pause_max_duration",
        topic: "pausemax",
        setup: setup_plain,
        call: call_set_pause_max,
    },
    Case {
        group: "pause",
        entrypoint: "set_pause_cooldown",
        topic: "pausecd",
        setup: setup_plain,
        call: call_set_pause_cooldown,
    },
    Case {
        group: "pause",
        entrypoint: "set_pause_rate_limit",
        topic: "pause_rl",
        setup: setup_plain,
        call: call_set_pause_rate_limit,
    },
    Case {
        group: "pause",
        entrypoint: "set_legal_hold",
        topic: "legalhld",
        setup: setup_plain,
        call: call_set_hold,
    },
    Case {
        group: "pause",
        entrypoint: "request_clear_legal_hold",
        topic: "lh_req",
        setup: setup_hold,
        call: call_request_clear_hold,
    },
    Case {
        group: "pause",
        entrypoint: "clear_legal_hold",
        topic: "legalhld",
        setup: setup_hold,
        call: call_clear_hold,
    },
    Case {
        group: "pause",
        entrypoint: "clear_legal_hold_after_delay",
        topic: "legalhld",
        setup: setup_clear_requested,
        call: call_clear_hold_after_delay,
    },
    Case {
        group: "pause",
        entrypoint: "cancel_clear_legal_hold",
        topic: "lh_cancel",
        setup: setup_clear_requested,
        call: call_cancel_clear_hold,
    },
    // allowlist
    Case {
        group: "allowlist",
        entrypoint: "set_allowlist_active",
        topic: "al_ena",
        setup: setup_plain,
        call: call_set_allowlist_active,
    },
    Case {
        group: "allowlist",
        entrypoint: "set_investor_allowlisted",
        topic: "al_set",
        setup: setup_plain,
        call: call_set_investor_allowlisted,
    },
    Case {
        group: "allowlist",
        entrypoint: "set_investors_allowlisted",
        topic: "al_set",
        setup: setup_plain,
        call: call_set_investors_allowlisted,
    },
    Case {
        group: "allowlist",
        entrypoint: "set_investor_frozen",
        topic: "inv_frz",
        setup: setup_plain,
        call: call_set_investor_frozen,
    },
    Case {
        group: "allowlist",
        entrypoint: "set_investor_allowlist_expiry",
        topic: "al_exp",
        setup: setup_plain,
        call: call_set_investor_expiry,
    },
    Case {
        group: "allowlist",
        entrypoint: "set_investors_allowlist_expiry",
        topic: "al_exp",
        setup: setup_plain,
        call: call_set_investors_expiry,
    },
    // attestation
    Case {
        group: "attestation",
        entrypoint: "bind_primary_attestation_hash",
        topic: "att_bind",
        setup: setup_plain,
        call: call_bind_attestation,
    },
    Case {
        group: "attestation",
        entrypoint: "set_require_attest_withdraw",
        topic: "att_req",
        setup: setup_plain,
        call: call_require_attestation,
    },
    Case {
        group: "attestation",
        entrypoint: "set_attestor",
        topic: "att_role",
        setup: setup_plain,
        call: call_set_attestor,
    },
    Case {
        group: "attestation",
        entrypoint: "append_attestation_digest",
        topic: "att_app",
        setup: setup_plain,
        call: call_append_attestation,
    },
    Case {
        group: "attestation",
        entrypoint: "append_attestation_digest_as",
        topic: "att_app",
        setup: setup_attestor,
        call: call_append_attestation_as,
    },
    Case {
        group: "attestation",
        entrypoint: "revoke_attestation_digest",
        topic: "att_rev",
        setup: setup_attestation,
        call: call_revoke_attestation,
    },
    Case {
        group: "attestation",
        entrypoint: "revoke_attestation_digests",
        topic: "att_rev",
        setup: setup_attestation,
        call: call_revoke_attestations,
    },
    Case {
        group: "attestation",
        entrypoint: "unrevoke_attestation_digest",
        topic: "att_unrev",
        setup: setup_attestation_revoked,
        call: call_unrevoke_attestation,
    },
    // collateral
    Case {
        group: "collateral",
        entrypoint: "record_sme_collateral_commitment",
        topic: "coll_rec",
        setup: setup_plain,
        call: call_record_collateral,
    },
    Case {
        group: "collateral",
        entrypoint: "batch_record_collateral",
        topic: "coll_rec",
        setup: setup_plain,
        call: call_batch_record_collateral,
    },
    Case {
        group: "collateral",
        entrypoint: "clear_sme_collateral_commitment",
        topic: "coll_clr",
        setup: setup_collateral,
        call: call_clear_collateral,
    },
    // funding
    Case {
        group: "funding",
        entrypoint: "fund",
        topic: "funded",
        setup: setup_plain,
        call: call_fund,
    },
    Case {
        group: "funding",
        entrypoint: "fund_with_commitment",
        topic: "funded",
        setup: setup_plain,
        call: call_fund_with_commitment,
    },
    Case {
        group: "funding",
        entrypoint: "fund_with_reference",
        topic: "fund_ref",
        setup: setup_plain,
        call: call_fund_with_reference,
    },
    Case {
        group: "funding",
        entrypoint: "fund_batch",
        topic: "funded",
        setup: setup_plain,
        call: call_fund_batch,
    },
    Case {
        group: "funding",
        entrypoint: "cancel_funding",
        topic: "fund_can",
        setup: setup_partial_funded,
        call: call_cancel_funding,
    },
    Case {
        group: "funding",
        entrypoint: "expire_funding",
        topic: "fund_exp",
        setup: setup_expired_funding,
        call: call_expire_funding,
    },
    Case {
        group: "funding",
        entrypoint: "refund",
        topic: "refunded",
        setup: setup_real_cancelled,
        call: call_refund,
    },
    Case {
        group: "funding",
        entrypoint: "refund_batch",
        topic: "refund_bt",
        setup: setup_real_cancelled,
        call: call_refund_batch,
    },
    Case {
        group: "funding",
        entrypoint: "unfund",
        topic: "unfunded",
        setup: setup_partial_funded,
        call: call_unfund,
    },
    Case {
        group: "funding",
        entrypoint: "update_funding_target",
        topic: "fund_tgt",
        setup: setup_plain,
        call: call_update_target,
    },
    Case {
        group: "funding",
        entrypoint: "lower_max_unique_investors",
        topic: "inv_cap",
        setup: setup_capped,
        call: call_lower_unique,
    },
    Case {
        group: "funding",
        entrypoint: "raise_max_unique_investors",
        topic: "raise_cap",
        setup: setup_capped,
        call: call_raise_unique,
    },
    Case {
        group: "funding",
        entrypoint: "lower_min_contribution_floor",
        topic: "floor_lo",
        setup: setup_capped,
        call: call_lower_floor,
    },
    Case {
        group: "funding",
        entrypoint: "raise_max_per_investor",
        topic: "inv_cap",
        setup: setup_capped,
        call: call_raise_per_investor,
    },
    Case {
        group: "funding",
        entrypoint: "update_maturity",
        topic: "maturity",
        setup: setup_plain,
        call: call_update_maturity,
    },
    Case {
        group: "funding",
        entrypoint: "update_yield_bps",
        topic: "yld_upd",
        setup: setup_plain,
        call: call_update_yield,
    },
    Case {
        group: "funding",
        entrypoint: "extend_funding_deadline",
        topic: "fund_ext",
        setup: setup_with_deadline,
        call: call_extend_deadline,
    },
    Case {
        group: "funding",
        entrypoint: "update_maturity_max_horizon",
        topic: "mtry_max",
        setup: setup_plain,
        call: call_update_horizon,
    },
    Case {
        group: "funding",
        entrypoint: "raise_maturity_max_horizon",
        topic: "mtry_rse",
        setup: setup_plain,
        call: call_raise_horizon,
    },
    // settlement
    Case {
        group: "settlement",
        entrypoint: "partial_settle",
        topic: "part_set",
        setup: setup_partial_funded,
        call: call_partial_settle,
    },
    Case {
        group: "settlement",
        entrypoint: "settle",
        topic: "escrow_sd",
        setup: setup_real_funded,
        call: call_settle,
    },
    Case {
        group: "settlement",
        entrypoint: "settle_batch",
        topic: "stl_batch",
        setup: setup_real_funded,
        call: call_settle_batch,
    },
    Case {
        group: "settlement",
        entrypoint: "repay",
        topic: "repaid",
        setup: setup_real_withdrawn,
        call: call_repay,
    },
    Case {
        group: "settlement",
        entrypoint: "withdraw",
        topic: "sme_wd",
        setup: setup_real_funded,
        call: call_withdraw,
    },
    Case {
        group: "settlement",
        entrypoint: "claim_investor_payout",
        topic: "inv_claim",
        setup: setup_real_settled,
        call: call_claim,
    },
    Case {
        group: "settlement",
        entrypoint: "push_investor_payouts",
        topic: "inv_claim",
        setup: setup_real_settled,
        call: call_push_payouts,
    },
];

// ── Exemption list ───────────────────────────────────────────────────────────

const VIEW: &str = "pure view/getter: no storage writes and no indexer-visible transition";
const MOCK: &str = "test-only DefaultMockToken helper contract, not a LiquifactEscrow entrypoint";

const EXEMPT: &[(&str, &str)] = &[
    ("get_active_fee_schedule", VIEW),
    ("get_pending_fee_schedule", VIEW),
    ("get_previous_fee_schedule", VIEW),
    ("get_pause_state", VIEW),
    ("get_token_balance", VIEW),
    ("get_escrow", VIEW),
    ("get_beneficiary", VIEW),
    ("get_remaining_funding_capacity", VIEW),
    ("get_funding_token", VIEW),
    ("get_treasury", VIEW),
    ("get_registry_ref", VIEW),
    ("get_pending_admin", VIEW),
    ("get_pending_admin_expiry", VIEW),
    ("get_pending_admin_remaining_secs", VIEW),
    ("has_maturity_lock", VIEW),
    ("get_version", VIEW),
    ("get_funding_deadline", VIEW),
    ("is_funding_expired", VIEW),
    ("get_legal_hold", VIEW),
    ("is_paused", VIEW),
    ("get_legal_hold_clear_delay", VIEW),
    ("get_legal_hold_clearable_at", VIEW),
    ("get_min_contribution_floor", VIEW),
    ("get_protocol_fee_bps", VIEW),
    ("get_performance_fee_bps", VIEW),
    ("get_max_unique_investors_cap", VIEW),
    ("get_max_per_investor_cap", VIEW),
    ("get_unique_funder_count", VIEW),
    ("get_escrow_summary", VIEW),
    ("get_primary_attestation_hash", VIEW),
    ("get_withdraw_requirements", VIEW),
    ("get_attestation_author", VIEW),
    ("get_attestation_append_log", VIEW),
    ("get_attestation_digest_at", VIEW),
    ("get_contribution", VIEW),
    ("get_contributions", VIEW),
    ("get_investors", VIEW),
    ("get_funding_records", VIEW),
    ("get_funding_close_snapshot", VIEW),
    ("get_settled_at", VIEW),
    ("get_investor_yield_bps", VIEW),
    ("get_investor_claim_not_before", VIEW),
    ("get_yield_tiers", VIEW),
    ("get_yield_tiers_page", VIEW),
    ("preview_yield_tier", VIEW),
    ("get_sme_collateral_commitment", VIEW),
    ("is_attestation_revoked", VIEW),
    ("get_revoked_attestation_digests", VIEW),
    ("get_attestation_digests", VIEW),
    ("is_investor_claimed", VIEW),
    ("is_settleable", VIEW),
    ("get_settlement_readiness", VIEW),
    ("get_settlement_config", VIEW),
    ("get_pauser", VIEW),
    ("get_pause_max_duration", VIEW),
    ("get_pause_cooldown", VIEW),
    ("get_pause_rate_limit", VIEW),
    ("get_paused_at", VIEW),
    ("is_allowlist_active", VIEW),
    ("is_investor_allowlisted", VIEW),
    ("get_investor_freeze", VIEW),
    ("get_allowlisted_investors", VIEW),
    ("get_allowlisted_investors_count", VIEW),
    ("get_investor_allowlist_expiry", VIEW),
    ("is_investor_allowlist_valid", VIEW),
    ("get_pending_upgrade", VIEW),
    ("get_reference_owner", VIEW),
    ("get_partial_close_min_bps", VIEW),
    ("preview_claim_split", VIEW),
    ("get_claimable_payout", VIEW),
    ("get_investor_position", VIEW),
    ("get_investor_positions", VIEW),
    ("compute_investor_payout", VIEW),
    ("get_settlement_pool", VIEW),
    ("get_repayment_status", VIEW),
    ("get_settlement_shortfall", VIEW),
    ("get_funding_deadline_extensions", VIEW),
    ("get_maturity_max_horizon", VIEW),
    ("get_remaining_investor_slots", VIEW),
    ("get_storage_limit", VIEW),
    ("is_investor_refunded", VIEW),
    ("get_distributed_principal", VIEW),
    ("get_reconciliation", VIEW),
    ("check_core_invariants", VIEW),
    ("audit_investor_page", VIEW),
    ("get_callback", VIEW),
    ("get_callback_nonce", VIEW),
    ("is_callback_consumed", VIEW),
    (
        "migrate",
        "all branches return typed errors in the current schema; no successful state mutation exists",
    ),
    (
        "set_storage_limit",
        "internal TTL/operational tuning only; deliberately outside the indexer event contract",
    ),
    (
        "bump_ttl",
        "TTL maintenance only; no business state or indexer-visible transition",
    ),
    (
        "batch_bump_ttl",
        "TTL maintenance only; no business state or indexer-visible transition",
    ),
    ("balance", MOCK),
    ("transfer", MOCK),
];

/// Hand-maintained inventory of `pub fn`s in `escrow/src/lib.rs`. Adding a new
/// public function without updating this list fails `all_pub_fns_are_hand_maintained`.
const ALL_PUB_FNS: &[&str] = &[
    "submit_fee_schedule",
    "activate_fee_schedule",
    "cancel_pending_fee_schedule",
    "get_active_fee_schedule",
    "get_pending_fee_schedule",
    "get_previous_fee_schedule",
    "get_pause_state",
    "get_token_balance",
    "init",
    "get_escrow",
    "get_beneficiary",
    "get_remaining_funding_capacity",
    "get_funding_token",
    "get_treasury",
    "get_registry_ref",
    "rebind_registry_ref",
    "clear_registry_ref",
    "get_pending_admin",
    "get_pending_admin_expiry",
    "get_pending_admin_remaining_secs",
    "has_maturity_lock",
    "sweep_terminal_dust",
    "rotate_beneficiary",
    "get_version",
    "get_funding_deadline",
    "is_funding_expired",
    "get_legal_hold",
    "is_paused",
    "get_legal_hold_clear_delay",
    "get_legal_hold_clearable_at",
    "get_min_contribution_floor",
    "get_protocol_fee_bps",
    "set_protocol_fee_bps",
    "set_performance_fee_bps",
    "get_performance_fee_bps",
    "get_max_unique_investors_cap",
    "get_max_per_investor_cap",
    "get_unique_funder_count",
    "get_escrow_summary",
    "bind_primary_attestation_hash",
    "get_primary_attestation_hash",
    "set_require_attest_withdraw",
    "get_withdraw_requirements",
    "set_attestor",
    "append_attestation_digest",
    "append_attestation_digest_as",
    "get_attestation_author",
    "get_attestation_append_log",
    "get_attestation_digest_at",
    "get_contribution",
    "get_contributions",
    "get_investors",
    "get_funding_records",
    "get_funding_close_snapshot",
    "get_settled_at",
    "get_investor_yield_bps",
    "get_investor_claim_not_before",
    "get_yield_tiers",
    "get_yield_tiers_page",
    "preview_yield_tier",
    "get_sme_collateral_commitment",
    "clear_sme_collateral_commitment",
    "revoke_attestation_digest",
    "revoke_attestation_digests",
    "is_attestation_revoked",
    "get_revoked_attestation_digests",
    "get_attestation_digests",
    "unrevoke_attestation_digest",
    "is_investor_claimed",
    "is_settleable",
    "get_settlement_readiness",
    "get_settlement_config",
    "record_sme_collateral_commitment",
    "batch_record_collateral",
    "set_paused",
    "set_pauser",
    "get_pauser",
    "pause_as_pauser",
    "set_pause_max_duration",
    "get_pause_max_duration",
    "set_pause_cooldown",
    "get_pause_cooldown",
    "set_pause_rate_limit",
    "get_pause_rate_limit",
    "get_paused_at",
    "set_legal_hold",
    "request_clear_legal_hold",
    "set_allowlist_active",
    "is_allowlist_active",
    "set_investor_allowlisted",
    "set_investors_allowlisted",
    "is_investor_allowlisted",
    "set_investor_frozen",
    "get_investor_freeze",
    "get_allowlisted_investors",
    "get_allowlisted_investors_count",
    "set_investor_allowlist_expiry",
    "set_investors_allowlist_expiry",
    "get_investor_allowlist_expiry",
    "is_investor_allowlist_valid",
    "clear_legal_hold",
    "clear_legal_hold_after_delay",
    "cancel_clear_legal_hold",
    "update_funding_target",
    "lower_max_unique_investors",
    "raise_max_unique_investors",
    "lower_min_contribution_floor",
    "raise_max_per_investor",
    "migrate",
    "propose_upgrade",
    "execute_upgrade",
    "cancel_upgrade",
    "get_pending_upgrade",
    "upgrade",
    "fund",
    "fund_with_commitment",
    "fund_with_reference",
    "get_reference_owner",
    "fund_batch",
    "partial_settle",
    "set_partial_close_min_bps",
    "get_partial_close_min_bps",
    "settle",
    "settle_batch",
    "repay",
    "withdraw",
    "preview_claim_split",
    "claim_investor_payout",
    "push_investor_payouts",
    "get_claimable_payout",
    "get_investor_position",
    "get_investor_positions",
    "compute_investor_payout",
    "get_settlement_pool",
    "get_repayment_status",
    "get_settlement_shortfall",
    "update_maturity",
    "update_yield_bps",
    "extend_funding_deadline",
    "get_funding_deadline_extensions",
    "get_maturity_max_horizon",
    "get_remaining_investor_slots",
    "update_maturity_max_horizon",
    "raise_maturity_max_horizon",
    "get_storage_limit",
    "set_storage_limit",
    "bump_ttl",
    "batch_bump_ttl",
    "propose_admin",
    "accept_admin",
    "transfer_admin",
    "cancel_pending_admin",
    "recover_admin",
    "cancel_funding",
    "expire_funding",
    "refund",
    "refund_batch",
    "unfund",
    "is_investor_refunded",
    "get_distributed_principal",
    "get_reconciliation",
    "check_core_invariants",
    "audit_investor_page",
    "register_callback",
    "execute_callback",
    "get_callback",
    "get_callback_nonce",
    "is_callback_consumed",
    "balance",
    "transfer",
];

// ── Helpers ──────────────────────────────────────────────────────────────────

const SOURCE: &str = include_str!("../lib.rs");

fn source_pub_fns() -> std::vec::Vec<std::string::String> {
    let mut names = std::vec::Vec::new();
    for line in SOURCE.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("pub fn ") {
            let name: std::string::String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

fn has_topic_symbol(event: &soroban_sdk::xdr::ContractEvent, expected: &str) -> bool {
    // `#[contractevent]` prepends the event struct name as topic[0], followed by
    // every field marked `#[topic]`. The stable indexer name (for example
    // `escrow_ii`) is one of those symbols, so match anywhere in the topic list.
    let ContractEventBody::V0(body) = &event.body;
    body.topics.iter().any(|topic| {
        matches!(topic, ScVal::Symbol(symbol) if symbol.0.as_vec().as_slice() == expected.as_bytes())
    })
}

fn run_case(case: &Case) {
    let env = Env::default();
    let mut harness = (case.setup)(&env);

    // Clear setup events (the host clears on every top-level invocation anyway)
    // and take a stable pre-call snapshot.
    harness.client.get_version();
    let before = env.events().all().events().len();

    (case.call)(&env, &mut harness);

    let after = env.events().all();
    let events = after.events();
    assert!(
        events.len() > before,
        "{} emitted no event: before={before}, after={}",
        case.entrypoint,
        events.len()
    );
    assert!(
        events[before..]
            .iter()
            .any(|event| has_topic_symbol(event, case.topic)),
        "{} emitted no event with topic symbol {:?}; events={events:?}",
        case.entrypoint,
        case.topic
    );
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn every_mutating_entrypoint_emits_its_expected_event() {
    assert!(CASES.len() >= 60, "coverage table unexpectedly small");
    for case in CASES {
        run_case(case);
    }
}

#[test]
fn all_eight_groups_are_covered() {
    for group in [
        "admin",
        "fee",
        "pause",
        "allowlist",
        "attestation",
        "collateral",
        "funding",
        "settlement",
    ] {
        assert!(
            CASES.iter().any(|case| case.group == group),
            "missing event-emission cases for group {group}"
        );
    }
}

#[test]
fn all_pub_fns_are_hand_maintained() {
    let mut parsed = source_pub_fns();
    parsed.sort();
    let mut known: std::vec::Vec<std::string::String> =
        ALL_PUB_FNS.iter().map(|name| (*name).to_string()).collect();
    known.sort();
    assert_eq!(
        parsed, known,
        "new pub fn in lib.rs: update ALL_PUB_FNS and either CASES or EXEMPT"
    );
}

#[test]
fn every_pub_fn_is_mapped_or_exempt() {
    let mut seen = std::collections::HashSet::new();
    for name in ALL_PUB_FNS {
        assert!(seen.insert(*name), "duplicate ALL_PUB_FNS entry: {name}");
        let mapped = CASES.iter().any(|case| case.entrypoint == *name);
        let exempt = EXEMPT
            .iter()
            .any(|(entry, reason)| *entry == *name && !reason.trim().is_empty());
        assert!(
            mapped || exempt,
            "pub fn {name} is neither in CASES nor justified in EXEMPT"
        );
        assert!(
            !(mapped && exempt),
            "pub fn {name} is both an event case and exempt"
        );
    }
    for (name, reason) in EXEMPT {
        assert!(
            ALL_PUB_FNS.contains(name),
            "stale exemption for unknown pub fn {name}"
        );
        assert!(
            !reason.trim().is_empty(),
            "empty exemption reason for {name}"
        );
    }
    let mut case_names = std::collections::HashSet::new();
    for case in CASES {
        assert!(
            case_names.insert(case.entrypoint),
            "duplicate CASES entry: {}",
            case.entrypoint
        );
        assert!(
            !case.topic.is_empty(),
            "empty expected topic: {}",
            case.entrypoint
        );
    }
}
