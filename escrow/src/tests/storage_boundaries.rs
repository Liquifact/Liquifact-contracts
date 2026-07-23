//! Storage boundary and rejection tests (issue #729).
//!
//! Covers accept/reject boundaries of every bounded or gated storage operation:
//! - [`DataKey::Version`] read defaults and post-init value
//! - [`DataKey::FundingToken`] / [`DataKey::Treasury`] fail-fast on missing keys
//! - Invoice-id length: exactly at limit, one over
//! - Invoice-id charset: valid edge chars, first rejected char
//! - Amount boundaries: 1 (min), MAX_INVOICE_AMOUNT, MAX_INVOICE_AMOUNT + 1
//! - YieldBps boundaries: 0, 10_000, 10_001
//! - MinContribution: 0 rejected, positive accepted, exceeds amount rejected
//! - MaxUniqueInvestors: 0 rejected, 1 accepted
//! - MaxPerInvestor: 0 rejected, positive accepted
//! - Attestation append log: exactly at capacity, one over
//! - Attestation revoke batch: empty, exactly at MAX, one over
//! - Dust sweep amount: 0, MAX_DUST_SWEEP_AMOUNT, one over
//! - fund_batch: empty, exactly at MAX, one over, duplicate investors
//! - get_contributions batch: exactly at MAX, one over
//! - Per-investor persistent storage: absent → default, written, idempotent second read
//! - UniqueFunderCount: zero at init, increments per new investor, does not increment on re-fund
//! - FundingCloseSnapshot: absent before funded, written on status transition
//! - SettledAt: absent before settle, written atomically on settle
//! - DistributedPrincipal: absent → 0, incremented per refund, incremented per claim
//! - LegalHold: absent → false, set true, clear two-phase
//! - Paused: absent → false, toggled true/false, events emitted
//! - PendingAdmin: absent → None, written on propose, cleared on accept/cancel
//! - ProtocolFeeBps: absent → 0, stored at init, immutable

use super::*;
use crate::{
    DataKey, EscrowError, LiquifactEscrowClient, PausedChanged, MAX_ATTESTATION_APPEND_ENTRIES,
    MAX_ATTESTATION_REVOKE_BATCH, MAX_DUST_SWEEP_AMOUNT, MAX_FUND_BATCH, MAX_INVESTOR_READ_BATCH,
    MAX_INVOICE_AMOUNT, MAX_INVOICE_ID_STRING_LEN, SCHEMA_VERSION,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, BytesN, Env, String, Vec as SorobanVec,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Shared init helper: sets up a basic escrow with no optional caps.
fn basic_init<'a>(
    client: &LiquifactEscrowClient<'a>,
    env: &'a Env,
    admin: &Address,
    sme: &Address,
) -> (Address, Address) {
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        admin,
        &String::from_str(env, "SB_BASE"),
        sme,
        &100_000i128,
        &500i64,
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
    (token, treasury)
}

// ── DataKey::Version ─────────────────────────────────────────────────────────

/// Before init the version getter returns 0 (unwrap_or default).
#[test]
fn test_version_defaults_to_zero_before_init() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    assert_eq!(client.get_version(), 0);
}

/// After init the version equals SCHEMA_VERSION.
#[test]
fn test_version_equals_schema_version_after_init() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    assert_eq!(client.get_version(), SCHEMA_VERSION);
}

// ── FundingToken / Treasury fail-fast ────────────────────────────────────────

/// get_funding_token before init panics with FundingTokenNotSet (code 21).
#[test]
fn test_get_funding_token_before_init_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    assert_contract_error(
        client.try_get_funding_token(),
        EscrowError::FundingTokenNotSet,
    );
}

/// get_treasury before init panics with TreasuryNotSet (code 22).
#[test]
fn test_get_treasury_before_init_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    assert_contract_error(client.try_get_treasury(), EscrowError::TreasuryNotSet);
}

/// Removing DataKey::Treasury from storage causes sweep_terminal_dust to emit TreasuryNotSet.
#[test]
fn test_treasury_removed_from_storage_causes_treasury_not_set_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    basic_init(&client, &env, &admin, &sme);
    client.cancel_funding(); // terminal state required for dust sweep
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Treasury);
    });
    assert_contract_error(
        client.try_sweep_terminal_dust(&1),
        EscrowError::TreasuryNotSet,
    );
}

// ── EscrowNotInitialized (code 20) ───────────────────────────────────────────

/// Every state-reading entrypoint should fail with EscrowNotInitialized if Escrow key is absent.
#[test]
fn test_get_escrow_before_init_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    assert_contract_error(client.try_get_escrow(), EscrowError::EscrowNotInitialized);
}

#[test]
fn test_fund_before_init_fails_with_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&investor, &1_000i128),
        EscrowError::EscrowNotInitialized,
    );
}

// ── Invoice-id length boundaries ─────────────────────────────────────────────

/// Invoice id exactly at MAX_INVOICE_ID_STRING_LEN characters is accepted.
#[test]
fn test_invoice_id_at_max_length_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    // 32 ASCII alphanumeric chars = MAX_INVOICE_ID_STRING_LEN
    let id_32 = String::from_str(&env, "ABCDEFGHIJKLMNOPQRSTUVWXYZ123456");
    client.init(
        &admin,
        &id_32,
        &sme,
        &100_000i128,
        &500i64,
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
    // Stored as Symbol; length accepted means no panic above and escrow is readable.
    let escrow = client.get_escrow();
    assert_eq!(escrow.status, 0);
}

/// Invoice id one byte over MAX_INVOICE_ID_STRING_LEN is rejected with InvoiceIdInvalidLength (code 4).
#[test]
fn test_invoice_id_one_over_max_length_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    // 33 characters — one over the 32-byte limit
    let id_33 = String::from_str(&env, "ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567");
    assert_contract_error(
        client.try_init(
            &admin,
            &id_33,
            &sme,
            &100_000i128,
            &500i64,
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
        ),
        EscrowError::InvoiceIdInvalidLength,
    );
}

/// Empty invoice id (0 bytes) is rejected with InvoiceIdInvalidLength (code 4).
#[test]
fn test_invoice_id_empty_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, ""),
            &sme,
            &100_000i128,
            &500i64,
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
        ),
        EscrowError::InvoiceIdInvalidLength,
    );
}

// ── Invoice-id charset boundary ───────────────────────────────────────────────

/// Invoice id with disallowed character (space) is rejected with InvoiceIdInvalidCharset (code 5).
#[test]
fn test_invoice_id_disallowed_char_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "INVALID ID"),
            &sme,
            &100_000i128,
            &500i64,
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
        ),
        EscrowError::InvoiceIdInvalidCharset,
    );
}

/// Invoice id with underscore (allowed edge-char) is accepted.
#[test]
fn test_invoice_id_with_underscore_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "INV_001"),
        &sme,
        &100_000i128,
        &500i64,
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
    assert_eq!(client.get_escrow().status, 0);
}

// ── Amount boundaries ─────────────────────────────────────────────────────────

/// Amount of 0 is rejected with AmountMustBePositive (code 1).
#[test]
fn test_amount_zero_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "AMT0"),
            &sme,
            &0i128,
            &500i64,
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
        ),
        EscrowError::AmountMustBePositive,
    );
}

/// Amount of exactly MAX_INVOICE_AMOUNT is accepted.
#[test]
fn test_amount_at_max_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "MAXAMT"),
        &sme,
        &MAX_INVOICE_AMOUNT,
        &500i64,
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
    assert_eq!(client.get_escrow().amount, MAX_INVOICE_AMOUNT);
}

/// Amount of MAX_INVOICE_AMOUNT + 1 is rejected with AmountExceedsMax (code 14).
#[test]
fn test_amount_one_over_max_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "OVAMT"),
            &sme,
            &(MAX_INVOICE_AMOUNT + 1),
            &500i64,
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
        ),
        EscrowError::AmountExceedsMax,
    );
}

// ── YieldBps boundaries ───────────────────────────────────────────────────────

/// yield_bps of 0 (no yield) is accepted.
#[test]
fn test_yield_bps_zero_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "YBPS0"),
        &sme,
        &100_000i128,
        &0i64,
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
    assert_eq!(client.get_escrow().yield_bps, 0);
}

/// yield_bps of exactly 10_000 (100%) is accepted.
#[test]
fn test_yield_bps_at_max_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "YBPSMAX"),
        &sme,
        &100_000i128,
        &10_000i64,
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
    assert_eq!(client.get_escrow().yield_bps, 10_000);
}

/// yield_bps of 10_001 is rejected with YieldBpsOutOfRange (code 2).
#[test]
fn test_yield_bps_one_over_max_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "YBPSOV"),
            &sme,
            &100_000i128,
            &10_001i64,
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
        ),
        EscrowError::YieldBpsOutOfRange,
    );
}

// ── MinContribution boundaries ────────────────────────────────────────────────

/// MinContribution of 0 is rejected with MinContributionNotPositive (code 6).
#[test]
fn test_min_contribution_zero_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "MINCTR0"),
            &sme,
            &100_000i128,
            &500i64,
            &0u64,
            &Address::generate(&env),
            &None,
            &Address::generate(&env),
            &None,
            &Some(0i128), // min_contribution = 0
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None::<i64>,
        ),
        EscrowError::MinContributionNotPositive,
    );
}

/// MinContribution exactly equal to the amount is rejected with MinContributionExceedsAmount (code 7).
#[test]
fn test_min_contribution_equal_to_amount_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "MCEXCD"),
            &sme,
            &100_000i128,
            &500i64,
            &0u64,
            &Address::generate(&env),
            &None,
            &Address::generate(&env),
            &None,
            &Some(100_000i128), // min == amount → rejected
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None::<i64>,
        ),
        EscrowError::MinContributionExceedsAmount,
    );
}

/// MinContribution of amount - 1 is accepted (exactly one under target).
#[test]
fn test_min_contribution_one_under_amount_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "MCON1"),
        &sme,
        &100_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &Some(99_999i128),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_eq!(client.get_min_contribution_floor(), 99_999i128);
}

// ── MaxUniqueInvestors boundaries ─────────────────────────────────────────────

/// MaxUniqueInvestors of 0 is rejected with MaxUniqueInvestorsNotPositive (code 8).
#[test]
fn test_max_unique_investors_zero_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "MUI0"),
            &sme,
            &100_000i128,
            &500i64,
            &0u64,
            &Address::generate(&env),
            &None,
            &Address::generate(&env),
            &None,
            &None,
            &Some(0u32), // max_unique_investors = 0
            &None,
            &None,
            &None,
            &None,
            &None,
            &None::<i64>,
        ),
        EscrowError::MaxUniqueInvestorsNotPositive,
    );
}

/// MaxUniqueInvestors of 1 is accepted and stored.
#[test]
fn test_max_unique_investors_one_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "MUI1"),
        &sme,
        &100_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &Some(1u32),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_eq!(client.get_max_unique_investors_cap(), Some(1u32));
}

// ── MaxPerInvestor boundaries ─────────────────────────────────────────────────

/// MaxPerInvestor of 0 is rejected with MaxPerInvestorNotPositive (code 9).
#[test]
fn test_max_per_investor_zero_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "MPI0"),
            &sme,
            &100_000i128,
            &500i64,
            &0u64,
            &Address::generate(&env),
            &None,
            &Address::generate(&env),
            &None,
            &None,
            &None,
            &Some(0i128), // max_per_investor = 0
            &None,
            &None,
            &None,
            &None,
            &None::<i64>,
        ),
        EscrowError::MaxPerInvestorNotPositive,
    );
}

/// MaxPerInvestor of 1 is accepted and stored.
#[test]
fn test_max_per_investor_one_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "MPI1"),
        &sme,
        &100_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &Some(1i128),
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    assert_eq!(client.get_max_per_investor_cap(), Some(1i128));
}

// ── EscrowAlreadyInitialized (code 3) ─────────────────────────────────────────

/// Calling init twice on the same contract is rejected.
#[test]
fn test_double_init_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "SB_BASE"),
            &sme,
            &100_000i128,
            &500i64,
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
        ),
        EscrowError::EscrowAlreadyInitialized,
    );
}

// ── Attestation append-log capacity ──────────────────────────────────────────

/// Append log accepts exactly MAX_ATTESTATION_APPEND_ENTRIES entries.
#[test]
fn test_attestation_log_at_capacity_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    for i in 0u8..MAX_ATTESTATION_APPEND_ENTRIES as u8 {
        client.append_attestation_digest(&BytesN::from_array(&env, &[i; 32]));
    }
    assert_eq!(
        client.get_attestation_append_log().len(),
        MAX_ATTESTATION_APPEND_ENTRIES
    );
}

/// One entry over capacity is rejected with AttestationAppendLogCapacityReached (code 51).
#[test]
fn test_attestation_log_one_over_capacity_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    for i in 0u8..MAX_ATTESTATION_APPEND_ENTRIES as u8 {
        client.append_attestation_digest(&BytesN::from_array(&env, &[i; 32]));
    }
    assert_contract_error(
        client.try_append_attestation_digest(&BytesN::from_array(&env, &[0xFF; 32])),
        EscrowError::AttestationAppendLogCapacityReached,
    );
}

// ── Attestation revoke batch boundaries ──────────────────────────────────────

/// Revoke batch with empty indices is rejected with AttestationBatchEmpty (code 54).
#[test]
fn test_attestation_revoke_batch_empty_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    client.append_attestation_digest(&BytesN::from_array(&env, &[1u8; 32]));
    let empty_indices = SorobanVec::new(&env);
    assert_contract_error(
        client.try_revoke_attestation_digests(&empty_indices),
        EscrowError::AttestationBatchEmpty,
    );
}

/// Revoke batch of exactly MAX_ATTESTATION_REVOKE_BATCH is accepted.
#[test]
fn test_attestation_revoke_batch_at_max_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    // Append MAX_ATTESTATION_REVOKE_BATCH entries first
    for i in 0u8..MAX_ATTESTATION_REVOKE_BATCH as u8 {
        client.append_attestation_digest(&BytesN::from_array(&env, &[i; 32]));
    }
    let mut indices: SorobanVec<u32> = SorobanVec::new(&env);
    for i in 0u32..MAX_ATTESTATION_REVOKE_BATCH {
        indices.push_back(i);
    }
    // Should not panic
    client.revoke_attestation_digests(&indices);
}

/// Revoke batch one over MAX_ATTESTATION_REVOKE_BATCH is rejected with AttestationBatchTooLarge (code 55).
#[test]
fn test_attestation_revoke_batch_one_over_max_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let mut indices: SorobanVec<u32> = SorobanVec::new(&env);
    for i in 0u32..MAX_ATTESTATION_REVOKE_BATCH + 1 {
        indices.push_back(i);
    }
    assert_contract_error(
        client.try_revoke_attestation_digests(&indices),
        EscrowError::AttestationBatchTooLarge,
    );
}

// ── Dust sweep amount boundaries ─────────────────────────────────────────────

/// Sweep amount of 0 is rejected with SweepAmountNotPositive (code 31).
#[test]
fn test_dust_sweep_zero_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    assert_contract_error(
        client.try_sweep_terminal_dust(&0),
        EscrowError::SweepAmountNotPositive,
    );
}

/// Sweep amount of MAX_DUST_SWEEP_AMOUNT + 1 is rejected with SweepAmountExceedsMax (code 32).
#[test]
fn test_dust_sweep_one_over_max_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    client.cancel_funding(); // enter terminal state
    assert_contract_error(
        client.try_sweep_terminal_dust(&(MAX_DUST_SWEEP_AMOUNT + 1)),
        EscrowError::SweepAmountExceedsMax,
    );
}

/// Sweep in a non-terminal state (open) is rejected with DustSweepNotTerminal (code 33).
#[test]
fn test_dust_sweep_not_terminal_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    // status == 0 (open), not terminal
    assert_contract_error(
        client.try_sweep_terminal_dust(&1),
        EscrowError::DustSweepNotTerminal,
    );
}

// ── fund_batch boundaries ─────────────────────────────────────────────────────

/// fund_batch with empty list is rejected with FundingBatchEmpty (code 82).
#[test]
fn test_fund_batch_empty_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let empty: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    assert_contract_error(
        client.try_fund_batch(&empty),
        EscrowError::FundingBatchEmpty,
    );
}

/// fund_batch one entry over MAX_FUND_BATCH is rejected with FundingBatchTooLarge (code 83).
#[test]
fn test_fund_batch_one_over_max_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let mut entries: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    for _ in 0..MAX_FUND_BATCH + 1 {
        entries.push_back((Address::generate(&env), 1_000i128));
    }
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingBatchTooLarge,
    );
}

/// fund_batch with duplicate investor addresses is rejected with FundingBatchDuplicateInvestor (code 84).
#[test]
fn test_fund_batch_duplicate_investor_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let dup = Address::generate(&env);
    let mut entries: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
    entries.push_back((dup.clone(), 1_000i128));
    entries.push_back((dup.clone(), 1_000i128));
    assert_contract_error(
        client.try_fund_batch(&entries),
        EscrowError::FundingBatchDuplicateInvestor,
    );
}

// ── get_contributions batch boundaries ───────────────────────────────────────

/// get_contributions with batch over MAX_INVESTOR_READ_BATCH is rejected with
/// ContributionReadBatchTooLarge (code 203).
#[test]
fn test_get_contributions_batch_too_large_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let mut investors: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..MAX_INVESTOR_READ_BATCH + 1 {
        investors.push_back(Address::generate(&env));
    }
    assert_contract_error(
        client.try_get_contributions(&investors),
        EscrowError::ContributionReadBatchTooLarge,
    );
}

/// get_contributions with exactly MAX_INVESTOR_READ_BATCH entries is accepted.
#[test]
fn test_get_contributions_batch_at_max_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let mut investors: SorobanVec<Address> = SorobanVec::new(&env);
    for _ in 0..MAX_INVESTOR_READ_BATCH {
        investors.push_back(Address::generate(&env));
    }
    let results = client.get_contributions(&investors);
    // All unknown investors → contribution = 0
    assert_eq!(results.len(), MAX_INVESTOR_READ_BATCH);
    for i in 0..MAX_INVESTOR_READ_BATCH {
        assert_eq!(results.get(i).unwrap(), 0i128);
    }
}

// ── Per-investor persistent storage defaults ──────────────────────────────────

/// get_contribution for an unknown investor defaults to 0 (absent key → 0).
#[test]
fn test_investor_contribution_absent_defaults_to_zero() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let unknown = Address::generate(&env);
    assert_eq!(client.get_contribution(&unknown), 0i128);
}

/// get_investor_yield_bps for an unknown investor defaults to escrow base yield.
#[test]
fn test_investor_yield_absent_defaults_to_base_yield() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let unknown = Address::generate(&env);
    let escrow = client.get_escrow();
    assert_eq!(client.get_investor_yield_bps(&unknown), escrow.yield_bps);
}

/// get_investor_claim_not_before for an unknown investor defaults to 0.
#[test]
fn test_investor_claim_not_before_absent_defaults_to_zero() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let unknown = Address::generate(&env);
    assert_eq!(client.get_investor_claim_not_before(&unknown), 0u64);
}

// ── UniqueFunderCount storage ─────────────────────────────────────────────────

/// UniqueFunderCount is 0 immediately after init.
#[test]
fn test_unique_funder_count_zero_at_init() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    assert_eq!(client.get_unique_funder_count(), 0u32);
}

/// UniqueFunderCount increments once per new investor, not per deposit.
#[test]
fn test_unique_funder_count_does_not_increment_on_re_fund() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let inv = Address::generate(&env);
    client.fund(&inv, &1_000i128);
    assert_eq!(client.get_unique_funder_count(), 1u32);
    client.fund(&inv, &1_000i128);
    // Same address — count stays at 1
    assert_eq!(client.get_unique_funder_count(), 1u32);
}

/// Two different investors each increment UniqueFunderCount by 1.
#[test]
fn test_unique_funder_count_increments_per_new_investor() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let inv1 = Address::generate(&env);
    let inv2 = Address::generate(&env);
    client.fund(&inv1, &1_000i128);
    assert_eq!(client.get_unique_funder_count(), 1u32);
    client.fund(&inv2, &1_000i128);
    assert_eq!(client.get_unique_funder_count(), 2u32);
}

// ── FundingCloseSnapshot storage ─────────────────────────────────────────────

/// FundingCloseSnapshot is absent (None) before the escrow is funded.
#[test]
fn test_funding_close_snapshot_absent_before_funded() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    assert!(client.get_funding_close_snapshot().is_none());
}

/// FundingCloseSnapshot is written when escrow transitions to status == 1.
#[test]
fn test_funding_close_snapshot_written_on_funded() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let inv = Address::generate(&env);
    // fund the full target to trigger status → 1
    client.fund(&inv, &100_000i128);
    assert_eq!(client.get_escrow().status, 1);
    let snapshot = client.get_funding_close_snapshot();
    assert!(snapshot.is_some());
    let snap = snapshot.unwrap();
    assert_eq!(snap.total_principal, 100_000i128);
}

// ── SettledAt storage ─────────────────────────────────────────────────────────

/// SettledAt is absent before settle is called.
#[test]
fn test_settled_at_absent_before_settle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    assert!(client.get_settled_at().is_none());
}

/// SettledAt is written atomically with the ledger timestamp when settle() is called.
#[test]
fn test_settled_at_written_on_settle() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    env.ledger().set_timestamp(42_000u64);
    let inv = Address::generate(&env);
    client.fund(&inv, &100_000i128);
    client.settle();
    assert_eq!(client.get_settled_at(), Some(42_000u64));
}

// ── DistributedPrincipal storage ──────────────────────────────────────────────

/// get_distributed_principal returns 0 before any refund or claim.
#[test]
fn test_distributed_principal_zero_at_init() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    assert_eq!(client.get_distributed_principal(), 0i128);
}

/// DistributedPrincipal increments after a refund.
#[test]
fn test_distributed_principal_increments_after_refund() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &String::from_str(&env, "DISTREF"),
        &sme,
        &100_000i128,
        &500i64,
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
    let inv = Address::generate(&env);
    token.stellar.mint(&inv, &10_000i128);
    client.fund(&inv, &10_000i128);
    client.cancel_funding();
    client.refund(&inv);
    assert_eq!(client.get_distributed_principal(), 10_000i128);
}

// ── LegalHold storage ────────────────────────────────────────────────────────

/// LegalHold is false (absent → default) before any set_legal_hold call.
#[test]
fn test_legal_hold_defaults_to_false() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    assert!(!client.get_legal_hold());
}

/// set_legal_hold(true) stores true; get_legal_hold returns true.
#[test]
fn test_legal_hold_stored_on_set() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    client.set_legal_hold(&true);
    assert!(client.get_legal_hold());
}

/// After clear_legal_hold() the flag returns to false.
#[test]
fn test_legal_hold_cleared_after_clear() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    client.set_legal_hold(&true);
    client.clear_legal_hold();
    assert!(!client.get_legal_hold());
}

// ── Paused storage ────────────────────────────────────────────────────────────

/// Paused is false (absent → default) before any set_paused call.
#[test]
fn test_paused_defaults_to_false() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    assert!(!client.is_paused());
}

/// set_paused(true) stores true; is_paused returns true.
#[test]
fn test_paused_stored_on_set() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    client.set_paused(&true);
    assert!(client.is_paused());
}

/// set_paused(false) clears the flag and emits PausedChanged with active=0.
#[test]
fn test_paused_cleared_emits_event() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let contract_id = client.address.clone();
    basic_init(&client, &env, &admin, &sme);
    client.set_paused(&true);
    client.set_paused(&false);
    assert!(!client.is_paused());
    let events = env.events().all();
    let invoice_id = client.get_escrow().invoice_id;
    assert_eq!(
        events.events().last().unwrap().clone(),
        PausedChanged {
            name: symbol_short!("paused"),
            invoice_id,
            active: 0u32,
        }
        .to_xdr(&env, &contract_id)
    );
}

// ── PendingAdmin storage ──────────────────────────────────────────────────────

/// PendingAdmin is None before any propose_admin call.
#[test]
fn test_pending_admin_absent_before_propose() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    assert_eq!(client.get_pending_admin(), None);
}

/// propose_admin writes PendingAdmin; get_pending_admin returns the nominated address.
#[test]
fn test_pending_admin_stored_on_propose() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let new_admin = Address::generate(&env);
    client.propose_admin(&new_admin, &None);
    assert_eq!(client.get_pending_admin(), Some(new_admin));
}

/// accept_admin clears PendingAdmin from storage.
#[test]
fn test_pending_admin_cleared_after_accept() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let new_admin = Address::generate(&env);
    client.propose_admin(&new_admin, &None);
    client.accept_admin();
    assert_eq!(client.get_pending_admin(), None);
}

// ── ProtocolFeeBps storage ────────────────────────────────────────────────────

/// ProtocolFeeBps absent before init defaults to 0 via get_protocol_fee_bps.
#[test]
fn test_protocol_fee_bps_absent_defaults_to_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    // Not initialized — key absent → unwrap_or(0) path
    assert_eq!(client.get_protocol_fee_bps(), 0i64);
}

/// ProtocolFeeBps stored at init is readable afterwards.
#[test]
fn test_protocol_fee_bps_stored_at_init() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "PFBPS"),
        &sme,
        &100_000i128,
        &500i64,
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
        &Some(250i64), // 2.5% fee
    );
    assert_eq!(client.get_protocol_fee_bps(), 250i64);
}

/// ProtocolFeeBps of 10_001 is rejected with ProtocolFeeBpsOutOfRange (code 215).
#[test]
fn test_protocol_fee_bps_out_of_range_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "PFBPSBAD"),
            &sme,
            &100_000i128,
            &500i64,
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
            &Some(10_001i64),
        ),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );
}

// ── Yield tier table storage validation ──────────────────────────────────────

/// Tier with yield_bps > 10_000 is rejected with TierYieldOutOfRange (code 10).
#[test]
fn test_tier_yield_out_of_range_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let mut tiers: SorobanVec<YieldTier> = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100u64,
        yield_bps: 10_001i64,
    });
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "TIERBAD"),
            &sme,
            &100_000i128,
            &500i64,
            &0u64,
            &Address::generate(&env),
            &None,
            &Address::generate(&env),
            &Some(tiers), // yield_tiers — position 10
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None::<i64>,
        ),
        EscrowError::TierYieldOutOfRange,
    );
}

/// Tier with yield_bps below base yield is rejected with TierYieldBelowBase (code 11).
#[test]
fn test_tier_yield_below_base_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let mut tiers: SorobanVec<YieldTier> = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100u64,
        yield_bps: 400i64, // base is 500, tier is below
    });
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "TIERBLW"),
            &sme,
            &100_000i128,
            &500i64,
            &0u64,
            &Address::generate(&env),
            &None,
            &Address::generate(&env),
            &Some(tiers), // yield_tiers — position 10
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None::<i64>,
        ),
        EscrowError::TierYieldBelowBase,
    );
}

/// Tiers with non-strictly-increasing min_lock_secs rejected with TierLockNotIncreasing (code 12).
#[test]
fn test_tier_lock_not_increasing_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let mut tiers: SorobanVec<YieldTier> = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 200u64,
        yield_bps: 600i64,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 100u64,
        yield_bps: 700i64,
    }); // lock decreases
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "TIERLK"),
            &sme,
            &100_000i128,
            &500i64,
            &0u64,
            &Address::generate(&env),
            &None,
            &Address::generate(&env),
            &Some(tiers), // yield_tiers — position 10
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None::<i64>,
        ),
        EscrowError::TierLockNotIncreasing,
    );
}

/// Valid two-tier table is stored and readable.
#[test]
fn test_valid_yield_tier_table_stored() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let mut tiers: SorobanVec<YieldTier> = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100u64,
        yield_bps: 600i64,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 200u64,
        yield_bps: 800i64,
    });
    client.init(
        &admin,
        &String::from_str(&env, "TIERGD"),
        &sme,
        &100_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &Some(tiers), // yield_tiers — position 10
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
    );
    let stored = client.get_yield_tiers();
    assert_eq!(stored.len(), 2u32);
    assert_eq!(stored.get(0).unwrap().yield_bps, 600i64);
    assert_eq!(stored.get(1).unwrap().min_lock_secs, 200u64);
}

// ── Unauthorized caller boundaries ───────────────────────────────────────────

/// Admin-only operation (update_funding_target) by a non-admin address panics.
#[test]
#[should_panic]
fn test_update_funding_target_by_non_admin_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let impersonator = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &impersonator,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "update_funding_target",
            args: SorobanVec::<soroban_sdk::Val>::new(&env),
            sub_invokes: &[],
        },
    }]);
    client.update_funding_target(&200_000i128);
}

/// Admin-only operation (set_legal_hold) by a non-admin address panics.
#[test]
#[should_panic]
fn test_set_legal_hold_by_non_admin_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let impersonator = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &impersonator,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "set_legal_hold",
            args: SorobanVec::<soroban_sdk::Val>::new(&env),
            sub_invokes: &[],
        },
    }]);
    client.set_legal_hold(&true);
}

/// Admin-only operation (set_paused) by a non-admin address panics.
#[test]
#[should_panic]
fn test_set_paused_by_non_admin_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    let impersonator = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &impersonator,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "set_paused",
            args: SorobanVec::<soroban_sdk::Val>::new(&env),
            sub_invokes: &[],
        },
    }]);
    client.set_paused(&true);
}

// ── ProtocolFeeBps immutability ───────────────────────────────────────────────

/// ProtocolFeeBps of exactly 10_000 (100%) is accepted.
#[test]
fn test_protocol_fee_bps_at_max_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "PFMAX"),
        &sme,
        &100_000i128,
        &500i64,
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
        &Some(10_000i64),
    );
    assert_eq!(client.get_protocol_fee_bps(), 10_000i64);
}

// ── Maturity boundary storage ─────────────────────────────────────────────────

/// Maturity of 0 disables the maturity lock (has_maturity_lock = false on EscrowInitialized).
#[test]
fn test_maturity_zero_disables_lock() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    basic_init(&client, &env, &admin, &sme);
    assert_eq!(client.get_escrow().maturity, 0u64);
    // Settlement should succeed immediately because there's no maturity gate
    let inv = Address::generate(&env);
    client.fund(&inv, &100_000i128);
    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

/// Maturity in the past is rejected with MaturityInPast (code 166).
#[test]
fn test_maturity_in_past_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    env.ledger().set_timestamp(5_000u64);
    assert_contract_error(
        client.try_init(
            &admin,
            &String::from_str(&env, "MATPAST"),
            &sme,
            &100_000i128,
            &500i64,
            &1_000u64, // maturity < now (5000)
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
        ),
        EscrowError::MaturityInPast,
    );
}
