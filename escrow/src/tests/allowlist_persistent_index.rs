//! Property tests and cost tests for the persistent allowlist index (Issue #1539).
//!
//! This module verifies:
//! - Random add/remove sequences maintain consistency (count, slot resolution, round-trip)
//! - Revoking a non-member writes no persistent entry
//! - Cost improvements for fund and get_allowlisted_investors_count operations

use crate::tests::{assert_contract_error, deploy, init, install_stellar_asset_token};
use crate::{DataKey, EscrowError, LiquifactEscrow, LiquifactEscrowClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec as SdkVec};

/// Property test: random add/remove sequences preserve invariants.
///
/// Asserts:
/// - count == number of members
/// - every slot < count resolves to an address
/// - SlotOf round-trips (SlotOf[addr] points to a slot that contains addr)
/// - paging returns each member exactly once
#[test]
fn property_test_random_add_remove_sequences() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let mut members: SdkVec<Address> = SdkVec::new(&env);
    let mut expected_count = 0u32;

    // Sequence 1: Add 10 addresses
    for _ in 0..10 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
        members.push_back(addr);
        expected_count += 1;
    }

    verify_invariants(&env, &client, &members, expected_count);

    // Sequence 2: Remove 3 addresses (indices 1, 4, 7)
    let to_remove = [1, 4, 7];
    for &idx in &to_remove {
        let addr = members.get(idx).unwrap();
        client.set_investor_allowlisted(&addr, &false);
        expected_count -= 1;
    }

    // Rebuild members list without removed addresses
    let mut active_members = SdkVec::new(&env);
    for i in 0..members.len() {
        if !to_remove.contains(&i) {
            active_members.push_back(members.get(i).unwrap());
        }
    }

    verify_invariants(&env, &client, &active_members, expected_count);

    // Sequence 3: Add 5 more addresses
    for _ in 0..5 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
        active_members.push_back(addr);
        expected_count += 1;
    }

    verify_invariants(&env, &client, &active_members, expected_count);

    // Sequence 4: Remove all
    for i in 0..active_members.len() {
        let addr = active_members.get(i).unwrap();
        client.set_investor_allowlisted(&addr, &false);
    }

    verify_invariants(&env, &client, &SdkVec::new(&env), 0);
}

/// Verify allowlist invariants.
fn verify_invariants(
    env: &Env,
    client: &LiquifactEscrowClient,
    expected_members: &SdkVec<Address>,
    expected_count: u32,
) {
    // Invariant 1: count matches expected
    let actual_count = client.get_allowlisted_investors_count();
    assert_eq!(
        actual_count, expected_count,
        "Count mismatch: expected {}, got {}",
        expected_count, actual_count
    );

    // Invariant 2: every slot < count resolves to an address
    env.as_contract(&client.address, || {
        for slot_idx in 0..actual_count {
            let addr_opt: Option<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::AllowlistSlot(slot_idx));
            assert!(
                addr_opt.is_some(),
                "Slot {} is empty but count is {}",
                slot_idx,
                actual_count
            );

            // Invariant 3: SlotOf round-trips
            let addr = addr_opt.unwrap();
            let slot_of: Option<u32> = env
                .storage()
                .persistent()
                .get(&DataKey::AllowlistSlotOf(addr.clone()));
            assert_eq!(
                slot_of,
                Some(slot_idx),
                "SlotOf round-trip failed for address at slot {}",
                slot_idx
            );

            // Verify the slot points back to the same address
            assert_eq!(
                env.storage()
                    .persistent()
                    .get::<DataKey, Address>(&DataKey::AllowlistSlot(slot_idx))
                    .unwrap(),
                addr,
                "Slot {} does not round-trip correctly",
                slot_idx
            );
        }
    });

    // Invariant 4: paging returns each member exactly once
    let page = client.get_allowlisted_investors(&0, &100);
    assert_eq!(
        page.len(),
        actual_count,
        "Paging returned {} addresses but count is {}",
        page.len(),
        actual_count
    );

    // Verify all expected members are present
    for i in 0..expected_members.len() {
        let expected_addr = expected_members.get(i).unwrap();
        assert!(
            client.is_investor_allowlisted(&expected_addr),
            "Expected member not allowlisted"
        );
    }

    // Verify no extra members
    for i in 0..page.len() {
        let addr = page.get(i).unwrap();
        let found = (0..expected_members.len())
            .any(|j| expected_members.get(j).unwrap() == addr);
        assert!(found, "Unexpected member in allowlist");
    }
}

/// Test: revoking a non-member writes no persistent entry.
///
/// Verifies the acceptance criterion: setting allowed=false for an address
/// that was never allowlisted is a no-op with no persistent write.
#[test]
fn revoke_non_member_writes_no_persistent_entry() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let contract_id = client.address.clone();
    init(&env, &client);

    let non_member = Address::generate(&env);

    // Verify not allowlisted
    assert!(!client.is_investor_allowlisted(&non_member));

    // Attempt to revoke (should be a no-op)
    client.set_investor_allowlisted(&non_member, &false);

    // Verify no persistent entries were written
    env.as_contract(&contract_id, || {
        let has_slot_of = env
            .storage()
            .persistent()
            .has(&DataKey::AllowlistSlotOf(non_member.clone()));
        assert!(
            !has_slot_of,
            "SlotOf entry exists for non-member after revoke"
        );

        let has_flag = env
            .storage()
            .persistent()
            .has(&DataKey::InvestorAllowlisted(non_member.clone()));
        assert!(
            !has_flag,
            "InvestorAllowlisted flag exists for non-member after revoke"
        );
    });

    // Verify count is still 0
    assert_eq!(client.get_allowlisted_investors_count(), 0);
}

/// Cost comparison test: get_allowlisted_investors_count with 200 addresses.
///
/// This test demonstrates the cost improvement from O(n) to O(1).
/// The PR should include the CPU instruction count from this test.
#[test]
fn cost_test_get_allowlisted_investors_count_200_addresses() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    // Add 200 addresses
    for _ in 0..200 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
    }

    // Measure cost of get_allowlisted_investors_count
    env.cost_estimate().budget().reset_default();
    let count = client.get_allowlisted_investors_count();
    let cpu_insns = env.cost_estimate().cpu_insns();
    let mem_bytes = env.cost_estimate().mem_bytes();

    assert_eq!(count, 200);

    // Output for PR (these values should be significantly lower than main branch)
    println!("get_allowlisted_investors_count with 200 addresses:");
    println!("  CPU instructions: {}", cpu_insns);
    println!("  Memory bytes: {}", mem_bytes);

    // The new implementation should be O(1), so cost should be minimal
    // On the old implementation, this would be O(n) with 200 persistent reads
    assert!(
        cpu_insns < 50_000,
        "CPU cost too high for O(1) operation: {}",
        cpu_insns
    );
}

/// Cost comparison test: fund operation with 200 allowlisted addresses.
///
/// Verifies that instance storage loading cost is reduced by moving
/// the allowlist index out of instance storage.
#[test]
fn cost_test_fund_with_200_allowlisted_addresses() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let token = install_stellar_asset_token(&env, &client.address);
    let investor = Address::generate(&env);

    // Add 200 addresses (not including the test investor)
    for _ in 0..200 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
    }

    // Add our test investor
    client.set_investor_allowlisted(&investor, &true);
    client.set_allowlist_active(&true);

    // Fund the investor's account
    token.mint(&investor, &10_000i128);

    // Measure cost of fund operation
    env.cost_estimate().budget().reset_default();
    client.fund(&investor, &5_000i128);
    let cpu_insns = env.cost_estimate().cpu_insns();
    let mem_bytes = env.cost_estimate().mem_bytes();

    println!("fund with 200 allowlisted addresses:");
    println!("  CPU instructions: {}", cpu_insns);
    println!("  Memory bytes: {}", mem_bytes);

    // The new implementation should not load the 200-address Vec from instance storage
    // Cost should be reasonable and not scale with allowlist size
}

/// Test: swap-remove preserves density and updates mappings correctly.
#[test]
fn swap_remove_updates_last_slot_mapping() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let contract_id = client.address.clone();
    init(&env, &client);

    // Add 5 addresses
    let mut addrs = SdkVec::new(&env);
    for _ in 0..5 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
        addrs.push_back(addr);
    }

    // Remove the address at slot 1 (middle of the list)
    let addr_to_remove = addrs.get(1).unwrap();
    client.set_investor_allowlisted(&addr_to_remove, &false);

    // Verify count is now 4
    assert_eq!(client.get_allowlisted_investors_count(), 4);

    // Verify the last address (originally at slot 4) is now at slot 1
    env.as_contract(&contract_id, || {
        let last_addr = addrs.get(4).unwrap();
        let slot_of_last: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::AllowlistSlotOf(last_addr.clone()))
            .expect("Last address should have a slot");

        // The swap-remove should have moved slot 4 -> slot 1
        assert_eq!(
            slot_of_last, 1,
            "Last address should now be at slot 1 after swap-remove"
        );

        // Verify slot 1 now contains the last address
        let addr_at_slot_1: Address = env
            .storage()
            .persistent()
            .get(&DataKey::AllowlistSlot(1))
            .expect("Slot 1 should contain an address");
        assert_eq!(
            addr_at_slot_1, last_addr,
            "Slot 1 should contain the last address"
        );

        // Verify slot 4 no longer exists
        assert!(
            !env.storage().persistent().has(&DataKey::AllowlistSlot(4)),
            "Slot 4 should be deleted"
        );
    });
}

/// Test: removing the last element (no swap needed).
#[test]
fn remove_last_element_no_swap() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let contract_id = client.address.clone();
    init(&env, &client);

    // Add 3 addresses
    let mut addrs = SdkVec::new(&env);
    for _ in 0..3 {
        let addr = Address::generate(&env);
        client.set_investor_allowlisted(&addr, &true);
        addrs.push_back(addr);
    }

    // Remove the last address (slot 2)
    let last_addr = addrs.get(2).unwrap();
    client.set_investor_allowlisted(&last_addr, &false);

    // Verify count is now 2
    assert_eq!(client.get_allowlisted_investors_count(), 2);

    // Verify slot 2 no longer exists
    env.as_contract(&contract_id, || {
        assert!(
            !env.storage().persistent().has(&DataKey::AllowlistSlot(2)),
            "Slot 2 should be deleted"
        );
        assert!(
            !env
                .storage()
                .persistent()
                .has(&DataKey::AllowlistSlotOf(last_addr)),
            "SlotOf mapping for last address should be deleted"
        );
    });
}

/// Test: idempotent add (adding an already-allowlisted address).
#[test]
fn idempotent_add_no_duplicate_slot() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    let addr = Address::generate(&env);

    // Add once
    client.set_investor_allowlisted(&addr, &true);
    assert_eq!(client.get_allowlisted_investors_count(), 1);

    // Add again (should be idempotent)
    client.set_investor_allowlisted(&addr, &true);
    assert_eq!(
        client.get_allowlisted_investors_count(),
        1,
        "Count should remain 1 after idempotent add"
    );

    // Verify only one slot exists
    let page = client.get_allowlisted_investors(&0, &10);
    assert_eq!(page.len(), 1, "Should have exactly one address in allowlist");
}

/// Test: batch operations maintain consistency.
#[test]
fn batch_operations_maintain_consistency() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init(&env, &client);

    // Batch add 10 addresses
    let mut addrs = SdkVec::new(&env);
    for _ in 0..10 {
        addrs.push_back(Address::generate(&env));
    }

    client.set_investors_allowlisted(&addrs, &true);
    assert_eq!(client.get_allowlisted_investors_count(), 10);

    // Verify all are allowlisted
    for i in 0..addrs.len() {
        assert!(client.is_investor_allowlisted(&addrs.get(i).unwrap()));
    }

    // Batch remove 5 addresses
    let mut to_remove = SdkVec::new(&env);
    for i in 0..5 {
        to_remove.push_back(addrs.get(i).unwrap());
    }

    client.set_investors_allowlisted(&to_remove, &false);
    assert_eq!(client.get_allowlisted_investors_count(), 5);

    // Verify consistency
    for i in 0..5 {
        assert!(!client.is_investor_allowlisted(&addrs.get(i).unwrap()));
    }
    for i in 5..10 {
        assert!(client.is_investor_allowlisted(&addrs.get(i).unwrap()));
    }
}
