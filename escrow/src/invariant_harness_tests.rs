//! Invariant test harness for escrow balance conservation (issue #1227).
//!
//! Individual tests already assert on isolated calls (a `fund` increments `funded_amount`,
//! a `settle` flips `status`, ...), but nothing previously proved that a full lifecycle keeps
//! the underlying SEP-41 token balance consistent with what was actually deposited, released,
//! or refunded. [`Harness`] closes that gap: it wraps every state-mutating entrypoint, tracks a
//! ground-truth ledger of deposits/payouts from the values the entrypoints themselves report,
//! and re-checks conservation against the **real** token balance (via this crate's
//! [`super::DefaultMockToken`] stand-in, not the contract's own internal counters) after every
//! single call. A violation panics immediately, naming the operation that broke it, so a
//! regression here always points at the first bad state transition rather than an eventual
//! symptom several calls later.
//!
//! Like [`super::settlement_guard_tests`] and [`super::callback_binding_tests`], this module
//! deliberately does **not** depend on the crate's `tests/` module tree, which is currently
//! disabled pending reconciliation with the lib API (see the note near the end of `lib.rs`).
//! It drives the public [`LiquifactEscrow`] surface directly so it can actually run today.
//!
//! ## Conservation law under test
//!
//! For any single escrow instance, at any point after [`LiquifactEscrow::init`]:
//!
//! ```text
//! token_balance(contract) - baseline == total_deposited - total_paid_out
//! ```
//!
//! `total_deposited` is the sum of every amount successfully passed to [`LiquifactEscrow::fund`]
//! / [`LiquifactEscrow::fund_with_commitment`]. `total_paid_out` is the sum of every amount that
//! actually left the contract via [`LiquifactEscrow::unfund`], [`LiquifactEscrow::refund`] /
//! [`LiquifactEscrow::refund_batch`], [`LiquifactEscrow::withdraw`] (fee + net, gross
//! `funded_amount`), or [`LiquifactEscrow::claim_investor_payout`]. `baseline` exists only
//! because of a test-infra quirk documented on [`Harness::drain_phantom_dust`]; it is `0` for
//! entrypoints that check the escrow's raw balance directly.
//!
//! ## A known, intentional reconciliation gap (not a regression here)
//!
//! [`LiquifactEscrow::get_reconciliation`] and [`LiquifactEscrow::get_distributed_principal`]
//! only advance `DataKey::DistributedPrincipal` from [`LiquifactEscrow::withdraw`] and
//! [`LiquifactEscrow::refund`] — **not** from [`LiquifactEscrow::claim_investor_payout`] (see
//! `settle`/`claim_investor_payout` in `lib.rs`: the settle+claim path is documented as an
//! off-chain-disbursement model). `full_release_via_settle_and_claim_conserves_balance` below
//! asserts this explicitly so a future change to that behavior is a deliberate, reviewed
//! decision rather than a silent drift: the *real* token-balance invariant this harness checks
//! still holds throughout, even though the contract's own liability view does not fall to zero
//! after every investor has claimed.

use soroban_sdk::{
    testutils::Address as _, token::TokenClient, Address, Env, MuxedAddress, String, Vec,
};

use super::{
    DefaultMockToken, InvoiceEscrow, LiquifactEscrow, LiquifactEscrowClient, SettlementResult,
};

/// Reusable invariant-checking wrapper around one deployed [`LiquifactEscrow`] instance.
///
/// Every method here calls the matching contract entrypoint, updates the running
/// deposit/payout ledger from values the entrypoint itself reports (never assumed by the
/// caller), and immediately re-asserts conservation. A test author who drives the contract
/// exclusively through a [`Harness`] gets the invariant checked after every step for free.
struct Harness<'a> {
    env: &'a Env,
    client: LiquifactEscrowClient<'a>,
    contract_id: Address,
    token: Address,
    admin: Address,
    sme: Address,
    treasury: Address,
    total_deposited: i128,
    total_paid_out: i128,
    /// Raw token balance of `contract_id` immediately after `init`, before any transfer.
    /// See [`Harness::drain_phantom_dust`] for why this is not simply `0`.
    baseline_balance: i128,
}

/// Deploys a fresh [`LiquifactEscrow`], binds it to a freshly registered
/// [`DefaultMockToken`], and returns a [`Harness`] ready to drive its lifecycle.
///
/// All auths are mocked (via `mock_all_auths_allowing_non_root_auth`, a strict superset of
/// `mock_all_auths` that also covers cross-contract batch entrypoints) so tests can focus on
/// the balance invariant rather than signature plumbing. Optional `init` parameters not
/// exercised by any edge case in this module (registry, yield tiers, contribution/investor
/// caps, legal-hold clear delay, custom maturity horizon, funding deadline, allowlist) are
/// fixed at their defaults; `amount`, `yield_bps`, `maturity`, and `protocol_fee_bps` are the
/// only knobs the scenarios below need.
fn deploy<'a>(
    env: &'a Env,
    invoice_id: &str,
    amount: i128,
    yield_bps: i64,
    maturity: u64,
    protocol_fee_bps: Option<i64>,
) -> Harness<'a> {
    env.mock_all_auths_allowing_non_root_auth();

    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let treasury = Address::generate(env);
    let token = Address::generate(env);

    // Pre-register the mock SEP-41 token so `fund`/`withdraw`/`refund`/`claim_investor_payout`
    // perform real, balance-delta-checked transfers instead of relying on the lazy
    // `register_mock_token_if_needed` path inside `fund_impl`.
    env.register_at(&token, DefaultMockToken, ());

    client.init(
        &admin,
        &String::from_str(env, invoice_id),
        &sme,
        &amount,
        &yield_bps,
        &maturity,
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
        &protocol_fee_bps,
    );

    let baseline_balance = TokenClient::new(env, &token).balance(&contract_id);

    Harness {
        env,
        client,
        contract_id,
        token,
        admin,
        sme,
        treasury,
        total_deposited: 0,
        total_paid_out: 0,
        baseline_balance,
    }
}

impl<'a> Harness<'a> {
    fn token_balance(&self) -> i128 {
        TokenClient::new(self.env, &self.token).balance(&self.contract_id)
    }

    /// Live balance movement relative to [`Harness::baseline_balance`] — what the harness
    /// expects to equal `total_deposited - total_paid_out` at every checkpoint.
    fn outstanding(&self) -> i128 {
        self.token_balance() - self.baseline_balance
    }

    /// Asserts the conservation law holds right now, naming `step` in the failure message so a
    /// broken invariant always identifies the first operation that violated it.
    fn assert_conserved(&self, step: &str) {
        let observed = self.outstanding();
        let expected = self.total_deposited - self.total_paid_out;
        assert_eq!(
            observed, expected,
            "balance conservation violated after `{step}`: contract balance moved by \
             {observed} relative to baseline, but total_deposited({}) - total_paid_out({}) \
             = {expected}",
            self.total_deposited, self.total_paid_out,
        );
    }

    fn fund(&mut self, investor: &Address, amount: i128) {
        self.client.fund(investor, &amount);
        self.total_deposited += amount;
        self.assert_conserved("fund");
    }

    fn unfund(&mut self, investor: &Address, amount: i128) {
        self.client.unfund(investor, &amount);
        self.total_paid_out += amount;
        self.assert_conserved("unfund");
    }

    fn partial_settle(&mut self, caller: &Address) -> InvoiceEscrow {
        let escrow = self.client.partial_settle(caller);
        self.assert_conserved("partial_settle");
        escrow
    }

    fn settle(&mut self) -> SettlementResult {
        let result = self.client.settle();
        self.assert_conserved("settle");
        result
    }

    fn claim(&mut self, investor: &Address) {
        let claimable = self.client.get_claimable_payout(investor);
        self.client.claim_investor_payout(investor);
        self.total_paid_out += claimable;
        self.assert_conserved("claim_investor_payout");
    }

    fn cancel_funding(&mut self) -> InvoiceEscrow {
        let escrow = self.client.cancel_funding();
        self.assert_conserved("cancel_funding");
        escrow
    }

    fn refund(&mut self, investor: &Address) {
        let amount = self.client.get_contribution(investor);
        self.client.refund(investor);
        self.total_paid_out += amount;
        self.assert_conserved("refund");
    }

    fn refund_batch(&mut self, investors: &Vec<Address>) {
        let mut total = 0i128;
        for investor in investors.iter() {
            if !self.client.is_investor_refunded(&investor) {
                total += self.client.get_contribution(&investor);
            }
        }
        self.client.refund_batch(investors);
        self.total_paid_out += total;
        self.assert_conserved("refund_batch");
    }

    fn withdraw(&mut self) -> InvoiceEscrow {
        let gross = self.client.get_escrow().funded_amount;
        let escrow = self.client.withdraw();
        self.total_paid_out += gross;
        self.assert_conserved("withdraw");
        escrow
    }

    fn set_legal_hold(&mut self, active: bool) {
        self.client.set_legal_hold(&active);
        self.assert_conserved("set_legal_hold");
    }

    fn close_escrow(&mut self) {
        self.client.close_escrow();
        self.assert_conserved("close_escrow");
    }

    /// Sweeps this harness's contract down to a **real** zero raw token balance.
    ///
    /// [`DefaultMockToken`] lazily materialises [`super::MOCK_TOKEN_DEFAULT_BALANCE`] for any
    /// address the first time it is queried or credited, so a freshly deployed escrow's *raw*
    /// balance is never actually `0` even once every tracked deposit has been paid back out —
    /// the phantom default rides along underneath every transfer. Entry points that inspect the
    /// raw balance directly (e.g. [`LiquifactEscrow::close_escrow`]'s `balance > 0` guard) need
    /// that phantom balance swept away before they can be exercised at all.
    ///
    /// Only valid once every tracked deposit has already been paid back out — draining a
    /// harness that still owes money would silently hide a real conservation bug, so this
    /// asserts the invariant is already exactly zero first.
    fn drain_phantom_dust(&mut self) {
        assert_eq!(
            self.total_deposited, self.total_paid_out,
            "drain_phantom_dust called before every tracked deposit was paid back out"
        );
        self.assert_conserved("drain_phantom_dust (precondition)");

        let raw = self.token_balance();
        if raw > 0 {
            let sink = Address::generate(self.env);
            TokenClient::new(self.env, &self.token).transfer(
                &self.contract_id,
                MuxedAddress::from(sink),
                &raw,
            );
        }
        self.baseline_balance = 0;
        self.assert_conserved("drain_phantom_dust");
    }
}

/// Edge case — **deposit then release**: investors fund to target, the escrow settles, and
/// every investor claims. The real token balance must land back at baseline.
#[test]
fn full_release_via_settle_and_claim_conserves_balance() {
    let env = Env::default();
    let mut h = deploy(&env, "INV_FULL_RLS", 1_000, 0, 0, None);

    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);

    h.fund(&investor_a, 600);
    h.fund(&investor_b, 400);
    assert_eq!(h.client.get_escrow().status, 1, "target reached: funded");

    h.settle();
    assert_eq!(h.client.get_escrow().status, 2);

    h.claim(&investor_a);
    h.claim(&investor_b);
    assert_eq!(
        h.outstanding(),
        0,
        "every investor claimed: balance back at baseline"
    );

    // A second claim for an already-paid investor is a documented no-op — it must not move
    // money twice.
    h.claim(&investor_a);
    assert_eq!(h.outstanding(), 0);

    // Documented gap (see module docs): the settle/claim path never advances
    // `DistributedPrincipal`, so the contract's own liability view still reports the full
    // gross `funded_amount` as outstanding even though the real balance has fully reconciled.
    assert_eq!(h.client.get_distributed_principal(), 0);
    let recon = h.client.get_reconciliation();
    assert_eq!(recon.outstanding_liability, 1_000);
    assert_eq!(recon.token_balance, h.token_balance());
}

/// Edge case — **deposit then refund**: investors fund below target, the admin cancels
/// funding, and a batch refund returns every contribution.
#[test]
fn cancel_then_refund_batch_conserves_balance() {
    let env = Env::default();
    let mut h = deploy(&env, "INV_REFUND", 1_000, 0, 0, None);

    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);

    h.fund(&investor_a, 700);
    h.fund(&investor_b, 200);
    assert_eq!(h.client.get_escrow().status, 0, "below target: still open");

    h.cancel_funding();
    assert_eq!(h.client.get_escrow().status, 4);

    let investors = soroban_sdk::vec![&env, investor_a.clone(), investor_b.clone()];
    h.refund_batch(&investors);
    assert_eq!(
        h.outstanding(),
        0,
        "both investors refunded: balance back at baseline"
    );

    // Idempotent replay: every entry is already refunded, so this is a total no-op.
    h.refund_batch(&investors);
    assert_eq!(h.outstanding(), 0);

    // A second single-investor refund attempt must fail (no contribution left) and must not
    // disturb the ledger.
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        h.client.refund(&investor_a);
    }));
    assert!(
        res.is_err(),
        "refunding an already-refunded investor must be rejected"
    );
    h.assert_conserved("rejected re-refund");
}

/// Edge case — **partial release sequence**: an early `partial_settle` closes funding before
/// target is reached, then investors claim one at a time. The invariant must hold at every
/// intermediate state, not only once the whole batch is done.
#[test]
fn partial_settle_then_staggered_claims_conserve_balance_at_every_step() {
    let env = Env::default();
    let mut h = deploy(&env, "INV_PARTIAL", 1_000, 0, 0, None);

    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);

    h.fund(&investor_a, 300);
    h.fund(&investor_b, 200);
    assert_eq!(
        h.client.get_escrow().status,
        0,
        "500 < target 1000: still open"
    );

    let admin = h.admin.clone();
    h.partial_settle(&admin);
    assert_eq!(h.client.get_escrow().status, 1, "closed early at 500");
    assert_eq!(h.client.get_escrow().funded_amount, 500);

    h.settle();
    assert_eq!(h.client.get_escrow().status, 2);

    // Only B claims first: the invariant must hold on this partially-released intermediate
    // state, with A's 300 still sitting in the contract.
    h.claim(&investor_b);
    assert_eq!(h.outstanding(), 300, "A has not claimed yet");

    h.claim(&investor_a);
    assert_eq!(h.outstanding(), 0, "both investors have now claimed");
}

/// Edge case — **dispute and closure**: a legal hold (this contract's compliance-hold /
/// "dispute" primitive) blocks claims and blocks `close_escrow` while active; once cleared and
/// every investor has claimed, the escrow can be closed exactly once.
#[test]
fn dispute_then_closure_conserves_balance() {
    let env = Env::default();
    let mut h = deploy(&env, "INV_DISPUTE", 1_000, 0, 0, None);

    let investor = Address::generate(&env);
    h.fund(&investor, 1_000);
    h.settle();

    h.set_legal_hold(true);

    // Blocked while the hold ("dispute") is active.
    let claim_blocked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        h.client.claim_investor_payout(&investor);
    }));
    assert!(
        claim_blocked.is_err(),
        "claim must be blocked during an active legal hold"
    );
    h.assert_conserved("blocked claim during dispute");

    let close_blocked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        h.client.close_escrow();
    }));
    assert!(
        close_blocked.is_err(),
        "close_escrow must reject an active dispute"
    );
    h.assert_conserved("blocked close_escrow during dispute");

    h.client.clear_legal_hold();
    h.assert_conserved("clear_legal_hold");

    h.claim(&investor);
    assert_eq!(
        h.outstanding(),
        0,
        "dispute resolved and investor paid in full"
    );

    h.drain_phantom_dust();
    assert_eq!(h.token_balance(), 0);

    h.close_escrow();
    assert!(h.client.get_closure_metadata().is_some());

    let reclose_blocked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        h.client.close_escrow();
    }));
    assert!(reclose_blocked.is_err(), "close_escrow must be one-shot");
    h.assert_conserved("rejected second close_escrow");
}

/// Edge case — **multiple escrows**: three independent instances, each driven through a
/// different terminal path (settle+claim, cancel+refund, on-chain SME withdraw with a protocol
/// fee), interleaved. Every instance's balance conservation is independent of the others.
#[test]
fn multiple_escrows_conserve_balance_independently() {
    let env = Env::default();

    let mut settled = deploy(&env, "INV_MULTI_STL", 1_000, 0, 0, None);
    let mut cancelled = deploy(&env, "INV_MULTI_CAN", 1_000, 0, 0, None);
    let mut withdrawn = deploy(&env, "INV_MULTI_WD", 2_000, 0, 0, Some(500)); // 5% protocol fee

    let inv_settled = Address::generate(&env);
    let inv_cancelled = Address::generate(&env);

    // Interleave calls across instances: an operation on one must never move another's balance.
    settled.fund(&inv_settled, 1_000);
    cancelled.fund(&inv_cancelled, 500);
    withdrawn.fund(&Address::generate(&env), 2_000);

    assert_eq!(settled.outstanding(), 1_000);
    assert_eq!(cancelled.outstanding(), 500);
    assert_eq!(withdrawn.outstanding(), 2_000);

    settled.settle();
    cancelled.cancel_funding();
    let sme_before = TokenClient::new(&env, &withdrawn.token).balance(&withdrawn.sme);
    let treasury_before = TokenClient::new(&env, &withdrawn.token).balance(&withdrawn.treasury);
    withdrawn.withdraw();

    settled.claim(&inv_settled);
    cancelled.refund(&inv_cancelled);

    assert_eq!(settled.outstanding(), 0, "settled instance fully released");
    assert_eq!(
        cancelled.outstanding(),
        0,
        "cancelled instance fully refunded"
    );
    assert_eq!(
        withdrawn.outstanding(),
        0,
        "withdrawn instance fully disbursed"
    );

    // The on-chain SME disbursement path actually splits funds: 5% to treasury, 95% to the SME.
    let sme_after = TokenClient::new(&env, &withdrawn.token).balance(&withdrawn.sme);
    let treasury_after = TokenClient::new(&env, &withdrawn.token).balance(&withdrawn.treasury);
    assert_eq!(
        sme_after - sme_before,
        1_900,
        "SME net payout: 2000 - 5% fee"
    );
    assert_eq!(
        treasury_after - treasury_before,
        100,
        "treasury fee: 5% of 2000"
    );

    // Final independence check: none of the three instances' invariants were disturbed by
    // driving the other two.
    settled.assert_conserved("final check");
    cancelled.assert_conserved("final check");
    withdrawn.assert_conserved("final check");
}

/// Full lifecycle coverage for [`LiquifactEscrow::unfund`]: an investor may pull back part of
/// an in-flight (still-open) contribution without the escrow ever funding or settling.
#[test]
fn unfund_while_open_conserves_balance() {
    let env = Env::default();
    let mut h = deploy(&env, "INV_UNFUND", 1_000, 0, 0, None);

    let investor = Address::generate(&env);
    h.fund(&investor, 800);
    h.unfund(&investor, 300);
    assert_eq!(h.client.get_contribution(&investor), 500);
    assert_eq!(
        h.client.get_escrow().status,
        0,
        "still open: never reached target"
    );
    assert_eq!(h.outstanding(), 500);

    h.unfund(&investor, 500);
    assert_eq!(h.client.get_contribution(&investor), 0);
    assert_eq!(
        h.outstanding(),
        0,
        "full principal pulled back before funding ever closed"
    );
}

/// The harness itself must fail loudly and name the offending step — this is what makes
/// [`Harness::assert_conserved`] useful as a regression signal rather than a silent pass.
#[test]
#[should_panic(expected = "balance conservation violated after `synthetic-ledger-corruption`")]
fn harness_reports_the_first_violating_operation_by_name() {
    let env = Env::default();
    let mut h = deploy(&env, "INV_SELFTEST", 1_000, 0, 0, None);

    let investor = Address::generate(&env);
    h.fund(&investor, 1_000);

    // Simulate a bookkeeping bug: record a deposit that never actually happened.
    h.total_deposited += 1;
    h.assert_conserved("synthetic-ledger-corruption");
}
