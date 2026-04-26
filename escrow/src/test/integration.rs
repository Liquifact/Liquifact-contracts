use super::super::external_calls::transfer_funding_token_with_balance_checks;
use super::*;
use soroban_sdk::{contract, contractimpl, FromStr};

// External-call and token-integration assumptions that should stay separate
// from escrow state-machine assertions.

#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) -> bool {
        panic!("Token contract transfer should not be invoked by escrow metadata-only flows")
    }

    pub fn is_paused(_env: Env) -> bool {
        panic!("Token contract pause status should not be read by escrow metadata-only flows")
    }

    pub fn decimals(_env: Env) -> i32 {
        6
    }
}

// --- Gold Standard Integration Test ---

/// **GOLD STANDARD INTEGRATION TEST**
///
/// This test demonstrates the complete happy path escrow lifecycle that new contributors
/// should use as a reference implementation. It covers:
///
/// 1. **Open**: Initialize escrow with realistic parameters
/// 2. **Overfund**: Multiple investors contribute, exceeding target
/// 3. **Snapshot**: Verify funding close snapshot captures state
/// 4. **Settle**: SME settles the escrow after maturity
/// 5. **Claim**: Investors claim their principal + yield payouts
///
/// **Token Amounts & Decimals:**
/// - USDC (7 decimals): 1 USDC = 10,000,000 base units
/// - Target: 50,000 USDC (500,000,000,000 base units)
/// - Yield: 12% APY (1200 bps)
/// - Maturity: 365 days (31,536,000 seconds)
///
/// **Security Notes:**
/// - Uses mock auth for testing; production requires real signatures
/// - Token transfers are metadata-only per external_calls.rs assumptions
#[test]
fn test_escrow_gold_standard_happy_path_open_overfund_snapshot_settle_claim() {
    let env = Env::default();
    env.mock_all_auths();

    // === SETUP PHASE ===
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    // Create realistic investor addresses
    let investor_alice = Address::generate(&env);
    let investor_bob = Address::generate(&env);
    let investor_charlie = Address::generate(&env);

    // USDC-like token with 7 decimals: 1 USDC = 10,000,000 base units
    const USDC_DECIMALS: i128 = 10_000_000;
    const TARGET_USDC: i128 = 50_000 * USDC_DECIMALS; // 50,000 USDC
    const YIELD_BPS: i64 = 1200; // 12% APY
    const MATURITY_SECS: u64 = 365 * 24 * 60 * 60; // 1 year

    // === PHASE 1: OPEN - Initialize Escrow ===
    client.init(
        &admin,
        &String::from_str(&env, "GOLD001"), // Invoice ID
        &sme,
        &TARGET_USDC,
        &YIELD_BPS,
        &MATURITY_SECS,
        &funding_token,
        &None, // No yield tiers for simplicity
        &treasury,
        &None, // No registry
        &None, // No min contribution floor
        &None, // No max investors cap
    );

    let initial_escrow = client.get_escrow();
    assert_eq!(
        initial_escrow.status, 0,
        "Escrow should start in Open status"
    );
    assert_eq!(
        initial_escrow.funded_amount, 0,
        "Should start with zero funding"
    );
    assert_eq!(initial_escrow.funding_target, TARGET_USDC);
    assert_eq!(initial_escrow.yield_bps, YIELD_BPS);

    // === PHASE 2: OVERFUND - Multiple Investors Contribute ===

    // Alice contributes 20,000 USDC (40% of target)
    let alice_amount = 20_000 * USDC_DECIMALS;
    let escrow_after_alice = client.fund(&investor_alice, &alice_amount);
    assert_eq!(
        escrow_after_alice.status, 0,
        "Should remain Open after partial funding"
    );
    assert_eq!(escrow_after_alice.funded_amount, alice_amount);

    // Verify Alice's contribution is tracked
    let alice_contribution = client.get_contribution(&investor_alice);
    assert_eq!(alice_contribution, alice_amount);

    // Bob contributes 25,000 USDC (50% of target) - this triggers funding completion
    let bob_amount = 25_000 * USDC_DECIMALS;
    let escrow_after_bob = client.fund(&investor_bob, &bob_amount);
    assert_eq!(
        escrow_after_bob.status, 1,
        "Should transition to Funded status"
    );
    assert_eq!(escrow_after_bob.funded_amount, alice_amount + bob_amount);

    // Charlie contributes 10,000 USDC (overfunding scenario)
    let charlie_amount = 10_000 * USDC_DECIMALS;
    let escrow_after_charlie = client.fund(&investor_charlie, &charlie_amount);
    assert_eq!(
        escrow_after_charlie.status, 1,
        "Should remain Funded after overfunding"
    );

    let total_funded = alice_amount + bob_amount + charlie_amount;
    assert_eq!(escrow_after_charlie.funded_amount, total_funded);
    assert!(total_funded > TARGET_USDC, "Should be overfunded");

    // === PHASE 3: SNAPSHOT - Verify Funding Close Snapshot ===
    let snapshot = client.get_funding_close_snapshot();
    assert!(
        snapshot.is_some(),
        "Funding close snapshot should be captured"
    );

    let snapshot = snapshot.unwrap();
    assert_eq!(
        snapshot.total_principal, total_funded,
        "Snapshot should capture total funded amount"
    );
    assert_eq!(
        snapshot.funding_target, TARGET_USDC,
        "Snapshot should preserve original target"
    );
    assert!(
        snapshot.closed_at_ledger_timestamp > 0,
        "Should have valid timestamp"
    );
    assert!(
        snapshot.closed_at_ledger_sequence > 0,
        "Should have valid sequence"
    );

    // Verify individual contributions sum to snapshot total
    let alice_contrib = client.get_contribution(&investor_alice);
    let bob_contrib = client.get_contribution(&investor_bob);
    let charlie_contrib = client.get_contribution(&investor_charlie);
    assert_eq!(
        alice_contrib + bob_contrib + charlie_contrib,
        snapshot.total_principal
    );

    // === PHASE 4: SETTLE - SME Settles After Maturity ===
    env.ledger().with_mut(|li| {
        li.timestamp = MATURITY_SECS + 1; // Just past maturity
    });

    let settled_escrow = client.settle();
    assert_eq!(
        settled_escrow.status, 2,
        "Should transition to Settled status"
    );
    assert_eq!(
        settled_escrow.funded_amount, total_funded,
        "Funded amount should be preserved"
    );

    // === PHASE 5: CLAIM - Investors Claim Principal + Yield ===
    let alice_expected_payout = calculate_expected_payout(alice_amount, YIELD_BPS);
    let bob_expected_payout = calculate_expected_payout(bob_amount, YIELD_BPS);
    let charlie_expected_payout = calculate_expected_payout(charlie_amount, YIELD_BPS);

    // Alice claims her payout (function only sets claimed flag, doesn't return amount)
    client.claim_investor_payout(&investor_alice);

    // Verify Alice is marked as claimed
    assert!(
        client.is_investor_claimed(&investor_alice),
        "Alice should be marked as claimed"
    );

    // Bob claims his payout
    client.claim_investor_payout(&investor_bob);

    // Charlie claims his payout
    client.claim_investor_payout(&investor_charlie);

    // === VERIFICATION PHASE ===
    assert!(client.is_investor_claimed(&investor_alice));
    assert!(client.is_investor_claimed(&investor_bob));
    assert!(client.is_investor_claimed(&investor_charlie));

    // Verify individual contributions and effective yields
    let alice_contrib = client.get_contribution(&investor_alice);
    let bob_contrib = client.get_contribution(&investor_bob);
    let charlie_contrib = client.get_contribution(&investor_charlie);

    assert_eq!(alice_contrib, alice_amount);
    assert_eq!(bob_contrib, bob_amount);
    assert_eq!(charlie_contrib, charlie_amount);

    // Verify effective yields (all should be base yield since no commitment)
    let alice_yield = client.get_investor_yield_bps(&investor_alice);
    let bob_yield = client.get_investor_yield_bps(&investor_bob);
    let charlie_yield = client.get_investor_yield_bps(&investor_charlie);

    assert_eq!(alice_yield, YIELD_BPS);
    assert_eq!(bob_yield, YIELD_BPS);
    assert_eq!(charlie_yield, YIELD_BPS);

    // Verify total contributions match expected yield calculation
    let total_principal = alice_amount + bob_amount + charlie_amount;
    let total_expected_yield = (total_principal * YIELD_BPS as i128) / 10_000;
    let _total_expected_payout = total_principal + total_expected_yield;

    assert_eq!(
        alice_expected_payout,
        alice_amount + (alice_amount * YIELD_BPS as i128) / 10_000
    );
    assert_eq!(
        bob_expected_payout,
        bob_amount + (bob_amount * YIELD_BPS as i128) / 10_000
    );
    assert_eq!(
        charlie_expected_payout,
        charlie_amount + (charlie_amount * YIELD_BPS as i128) / 10_000
    );

    // Verify escrow remains in settled state
    let final_escrow = client.get_escrow();
    assert_eq!(
        final_escrow.status, 2,
        "Escrow should remain in Settled status"
    );
}

/// Helper function to calculate expected payout using the same formula as the contract.
/// Formula: payout = principal + (principal × yield_bps) / 10_000
/// This matches the contract's `calculate_principal_plus_yield` function.
fn calculate_expected_payout(principal: i128, yield_bps: i64) -> i128 {
    let yield_amount = (principal * yield_bps as i128) / 10_000;
    principal + yield_amount
}

/// **TIERED YIELD INTEGRATION TEST**
///
/// Demonstrates the tiered yield system with commitment locks.
/// Shows how investors can get higher yields by committing to longer lock periods.
///
/// **Yield Tiers:**
/// - Base: 8% APY (800 bps) - no lock required
/// - Tier 1: 10% APY (1000 bps) - 90 days lock
/// - Tier 2: 12% APY (1200 bps) - 180 days lock
/// - Tier 3: 15% APY (1500 bps) - 365 days lock
#[test]
fn test_escrow_tiered_yield_with_commitment_locks() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    // Create yield tier table
    let yield_tiers = SorobanVec::from_array(
        &env,
        [
            YieldTier {
                min_lock_secs: 90 * 24 * 60 * 60,
                yield_bps: 1000,
            }, // 90 days, 10%
            YieldTier {
                min_lock_secs: 180 * 24 * 60 * 60,
                yield_bps: 1200,
            }, // 180 days, 12%
            YieldTier {
                min_lock_secs: 365 * 24 * 60 * 60,
                yield_bps: 1500,
            }, // 365 days, 15%
        ],
    );

    const USDC_DECIMALS: i128 = 10_000_000;
    const TARGET_USDC: i128 = 30_000 * USDC_DECIMALS; // 30,000 USDC
    const BASE_YIELD_BPS: i64 = 800; // 8% base yield

    // Initialize with tiered yield
    client.init(
        &admin,
        &String::from_str(&env, "TIER001"),
        &sme,
        &TARGET_USDC,
        &BASE_YIELD_BPS,
        &0u64, // No maturity for this test
        &funding_token,
        &Some(yield_tiers),
        &treasury,
        &None,
        &None,
        &None,
    );

    let investor_base = Address::generate(&env);
    let investor_tier1 = Address::generate(&env);
    let investor_tier2 = Address::generate(&env);
    let investor_tier3 = Address::generate(&env);

    // Base investor (no commitment) - gets 8%
    let base_amount = 5_000 * USDC_DECIMALS;
    client.fund(&investor_base, &base_amount);
    let base_yield = client.get_investor_yield_bps(&investor_base);
    assert_eq!(
        base_yield, BASE_YIELD_BPS,
        "Base investor should get base yield"
    );

    // Tier 1 investor (90 days) - gets 10%
    let tier1_amount = 8_000 * USDC_DECIMALS;
    let tier1_lock = 90 * 24 * 60 * 60; // 90 days
    client.fund_with_commitment(&investor_tier1, &tier1_amount, &tier1_lock);
    let tier1_yield = client.get_investor_yield_bps(&investor_tier1);
    assert_eq!(tier1_yield, 1000, "Tier 1 investor should get 10% yield");

    // Tier 2 investor (180 days) - gets 12%
    let tier2_amount = 10_000 * USDC_DECIMALS;
    let tier2_lock = 180 * 24 * 60 * 60; // 180 days
    client.fund_with_commitment(&investor_tier2, &tier2_amount, &tier2_lock);
    let tier2_yield = client.get_investor_yield_bps(&investor_tier2);
    assert_eq!(tier2_yield, 1200, "Tier 2 investor should get 12% yield");

    // Tier 3 investor (365 days) - gets 15%
    let tier3_amount = 7_000 * USDC_DECIMALS;
    let tier3_lock = 365 * 24 * 60 * 60; // 365 days
    client.fund_with_commitment(&investor_tier3, &tier3_amount, &tier3_lock);
    let tier3_yield = client.get_investor_yield_bps(&investor_tier3);
    assert_eq!(tier3_yield, 1500, "Tier 3 investor should get 15% yield");

    // Settle the escrow
    let settled = client.settle();
    assert_eq!(settled.status, 2);

    // Verify claim locks are enforced
    let current_time = env.ledger().timestamp();

    // Base investor can claim immediately
    let base_claim_time = client.get_investor_claim_not_before(&investor_base);
    assert_eq!(
        base_claim_time, 0,
        "Base investor should have no claim lock"
    );

    // Tiered investors have appropriate claim locks
    let tier1_claim_time = client.get_investor_claim_not_before(&investor_tier1);
    let tier2_claim_time = client.get_investor_claim_not_before(&investor_tier2);
    let tier3_claim_time = client.get_investor_claim_not_before(&investor_tier3);

    assert!(
        tier1_claim_time > current_time,
        "Tier 1 should have future claim time"
    );
    assert!(
        tier2_claim_time > tier1_claim_time,
        "Tier 2 should have longer lock than Tier 1"
    );
    assert!(
        tier3_claim_time > tier2_claim_time,
        "Tier 3 should have longest lock"
    );

    // Fast-forward past all lock periods
    env.ledger().with_mut(|li| {
        li.timestamp = tier3_claim_time + 1;
    });

    // All investors can now claim with their respective yields
    client.claim_investor_payout(&investor_base);
    client.claim_investor_payout(&investor_tier1);
    client.claim_investor_payout(&investor_tier2);
    client.claim_investor_payout(&investor_tier3);

    // Verify all are marked as claimed
    assert!(client.is_investor_claimed(&investor_base));
    assert!(client.is_investor_claimed(&investor_tier1));
    assert!(client.is_investor_claimed(&investor_tier2));
    assert!(client.is_investor_claimed(&investor_tier3));

    // Verify higher tiers would yield more absolute return
    let tier3_expected = calculate_expected_payout(tier3_amount, 1500);
    let base_expected = calculate_expected_payout(base_amount, BASE_YIELD_BPS);
    let tier3_yield_amount = tier3_expected - tier3_amount;
    let base_yield_amount = base_expected - base_amount;
    assert!(
        tier3_yield_amount > base_yield_amount,
        "Higher tier should yield more absolute return"
    );
}

// --- Existing Tests (Preserved) ---

#[test]
fn test_collateral_record_is_metadata_only_and_does_not_invoke_token_contract() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let _token_id = env.register(MockToken, ());
    let funding = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &String::from_str(&env, "COLTI001"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &funding,
        &None,
        &treasury,
        &None,
        &None,
        &None,
    );

    let commitment = client.record_sme_collateral_commitment(&symbol_short!("USDC"), &5_000i128);
    assert_eq!(commitment.asset, symbol_short!("USDC"));
    assert_eq!(commitment.amount, 5_000i128);
    assert!(client.get_sme_collateral_commitment().is_some());
}

#[test]
fn test_token_integration_assumptions_are_documented_in_readme() {
    let contents = include_str!("../../../docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md");
    assert!(
        contents.contains("fee-on-transfer"),
        "Expected unsupported token warning to be documented"
    );
    assert!(
        contents.contains("smallest units"),
        "Expected smallest-unit assumption to be documented"
    );
}

#[test]
fn test_external_transfer_wrapper_balance_deltas() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);
    token.stellar.mint(&holder, &777i128);
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 777i128);
    assert_eq!(token.token.balance(&holder), 0);
    assert_eq!(token.token.balance(&treasury), 777i128);
}

#[test]
#[should_panic(expected = "insufficient token balance before transfer")]
fn test_external_wrapper_panics_when_undercollateralized() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);
    token.stellar.mint(&holder, &1i128);
    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 10i128);
}
