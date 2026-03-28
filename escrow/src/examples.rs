//! # End-to-End Integration Examples
//!
//! This module provides realistic usage patterns for the LiquiFact Escrow contract.
//! These examples are intended for backend integrators and client-side test suites.

#[cfg(test)]
mod examples {
    use crate::{LiquifactEscrow, LiquifactEscrowClient};
    use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};

    /// Full Lifecycle Example: From Initialization to Investor Payout
    ///
    /// This example demonstrates the standard "Happy Path" flow:
    /// 1. Admin creates the escrow for an invoice.
    /// 2. Multiple investors fund the invoice.
    /// 3. SME (Business) withdraws the funded principal for working capital.
    /// 4. Buyer pays the invoice (Settlement).
    /// 5. Investors claim their principal plus interest.
    #[test]
    pub fn example_full_lifecycle_flow() {
        let env = Env::default();
        env.mock_all_auths();

        // 1. Setup Identities
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        let _buyer = Address::generate(&env); // Buyer is off-chain, but SME/Admin auths for them
        let investor_a = Address::generate(&env);
        let investor_b = Address::generate(&env);

        // 2. Deploy Contract
        let contract_id = env.register(LiquifactEscrow, ());
        let client = LiquifactEscrowClient::new(&env, &contract_id);

        // 3. Initialize Escrow (Admin Action)
        // Invoice ID: INV001, Amount: 1,000,000, Yield: 10% (1000 bps), Maturity: 30 days
        let amount = 1_000_000i128;
        let yield_bps = 1000i64;
        let maturity = env.ledger().timestamp() + (30 * 24 * 60 * 60);

        client.init(
            &admin,
            &symbol_short!("INV001"),
            &sme,
            &amount,
            &yield_bps,
            &maturity,
        );

        println!("Escrow initialized for INV001");

        // 4. Funding (Investor Action)
        // Investor A funds 60%
        client.fund(&investor_a, &600_000);
        // Investor B funds 40%
        client.fund(&investor_b, &400_000);

        let escrow = client.get_escrow();
        assert_eq!(escrow.status, 1); // Status 1 = Fully Funded
        println!("Escrow fully funded. Total: {}", escrow.funded_amount);

        // 5. Principal Withdrawal (SME Action)
        // Once funded, the SME can withdraw the principal to pay their suppliers.
        let withdrawn = client.withdraw();
        assert_eq!(withdrawn, 1_000_000);
        println!("SME withdrawn principal for working capital.");

        // 6. Settlement (SME/Admin Action)
        // After 30 days, the buyer pays the invoice.
        // Total Payout = Principal + (Principal * 10%) = 1,100,000
        let total_yield = (amount * yield_bps as i128) / 10000;
        let total_payout = amount + total_yield;

        client.settle(&total_payout);

        let escrow_settled = client.get_escrow();
        assert_eq!(escrow_settled.status, 2); // Status 2 = Settled
        println!("Invoice settled by buyer. Yield accrued.");

        // 7. Claiming Payouts (Investor Action)
        // Investor A claims: 600k principal + 60k interest = 660,000
        let payout_a = client.claim(&investor_a);
        assert_eq!(payout_a, 660_000);

        // Investor B claims: 400k principal + 40k interest = 440,000
        let payout_b = client.claim(&investor_b);
        assert_eq!(payout_b, 440_000);

        println!("Investors successfully claimed their payouts.");
    }

    /// Error Handling Example: Double-Claim Protection
    ///
    /// This demonstrates why security integrators must ensure exactly one claim per investor.
    #[test]
    #[should_panic(expected = "Payout already claimed")]
    pub fn example_error_double_claim() {
        let env = Env::default();
        env.mock_all_auths();
        let investor = Address::generate(&env);

        // ... (setup and settle escrow)
        let contract_id = env.register(LiquifactEscrow, ());
        let client = LiquifactEscrowClient::new(&env, &contract_id);
        client.init(
            &Address::generate(&env),
            &symbol_short!("INVERR"),
            &Address::generate(&env),
            &1000,
            &0,
            &1000,
        );
        client.fund(&investor, &1000);
        client.settle(&1000);

        // First claim succeeds
        client.claim(&investor);

        // Second claim MUST fail to prevent drainage
        client.claim(&investor);
    }
}
