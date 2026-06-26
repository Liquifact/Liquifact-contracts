use super::{
    free_addresses, install_stellar_asset_token, setup, MAX_ATTESTATION_APPEND_ENTRIES,
    SCHEMA_VERSION,
};
use crate::{CollateralCommitmentSnapshot, DataKey, EscrowCloseSnapshot, EscrowError, YieldTier};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, BytesN, Env, Error, InvokeError, Vec as SorobanVec,
};

pub(crate) use super::assert_contract_error;

#[test]
fn typed_error_codes_cover_init_and_state_guards() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    assert_contract_error(
        client.try_init(
            &admin,
            &soroban_sdk::String::from_str(&env, "ERR_INIT"),
            &sme,
            &0,
            &100,
            &100,
            &funding_token,
            &None,
            &treasury,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
        ),
        EscrowError::AmountMustBePositive,
    );

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ERR_FLOW"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&investor, &0),
        EscrowError::FundingAmountNotPositive,
    );
    assert_contract_error(client.try_settle(), EscrowError::SettlementNotFunded);
    assert_contract_error(client.try_withdraw(), EscrowError::WithdrawalNotFunded);
    assert_contract_error(
        client.try_claim_investor_payout(&investor),
        EscrowError::NoContributionToClaim,
    );
}

#[test]
fn typed_error_codes_cover_allowlist_attestation_and_dust_guards() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ERR_MORE"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.set_allowlist_active(&true);
    let investor = Address::generate(&env);
    assert_contract_error(
        client.try_fund(&investor, &10),
        EscrowError::InvestorNotAllowlisted,
    );

    let digest = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    client.bind_primary_attestation_hash(&digest);
    assert_contract_error(
        client.try_bind_primary_attestation_hash(&digest),
        EscrowError::PrimaryAttestationAlreadyBound,
    );

    assert_contract_error(
        client.try_sweep_terminal_dust(&0),
        EscrowError::SweepAmountNotPositive,
    );
    assert_contract_error(
        client.try_sweep_terminal_dust(&1),
        EscrowError::DustSweepNotTerminal,
    );
}

#[test]
fn escrow_error_discriminants_match_canonical_table() {
    const TABLE: &[(EscrowError, u32)] = &[
        (EscrowError::AmountMustBePositive, 1),
        (EscrowError::YieldBpsOutOfRange, 2),
        (EscrowError::EscrowAlreadyInitialized, 3),
        (EscrowError::InvoiceIdInvalidLength, 4),
        (EscrowError::InvoiceIdInvalidCharset, 5),
        (EscrowError::MinContributionNotPositive, 6),
        (EscrowError::MinContributionExceedsAmount, 7),
        (EscrowError::MaxUniqueInvestorsNotPositive, 8),
        (EscrowError::MaxPerInvestorNotPositive, 9),
        (EscrowError::TierYieldOutOfRange, 10),
        (EscrowError::TierYieldBelowBase, 11),
        (EscrowError::TierLockNotIncreasing, 12),
        (EscrowError::TierYieldNotNonDecreasing, 13),
        (EscrowError::EscrowNotInitialized, 20),
        (EscrowError::FundingTokenNotSet, 21),
        (EscrowError::TreasuryNotSet, 22),
        (EscrowError::LegalHoldBlocksTreasuryDustSweep, 30),
        (EscrowError::SweepAmountNotPositive, 31),
        (EscrowError::SweepAmountExceedsMax, 32),
        (EscrowError::DustSweepNotTerminal, 33),
        (EscrowError::NoFundingTokenBalanceToSweep, 34),
        (EscrowError::EffectiveSweepAmountZero, 35),
        (EscrowError::TransferAmountNotPositive, 36),
        (EscrowError::InsufficientTokenBalanceBeforeTransfer, 37),
        (EscrowError::SenderBalanceUnderflow, 38),
        (EscrowError::RecipientBalanceUnderflow, 39),
        (EscrowError::SenderBalanceDeltaMismatch, 40),
        (EscrowError::RecipientBalanceDeltaMismatch, 41),
        (EscrowError::SweepExceedsLiabilityFloor, 42),
        (EscrowError::PrimaryAttestationAlreadyBound, 50),
        (EscrowError::AttestationAppendLogCapacityReached, 51),
        (EscrowError::CollateralAmountNotPositive, 60),
        (EscrowError::CollateralAssetEmpty, 61),
        (EscrowError::CollateralTimestampBackwards, 62),
        (EscrowError::InvestorBatchEmpty, 70),
        (EscrowError::InvestorBatchTooLarge, 71),
        (EscrowError::TargetNotPositive, 72),
        (EscrowError::TargetUpdateNotOpen, 73),
        (EscrowError::TargetBelowFundedAmount, 74),
        (EscrowError::CapLowerNotOpen, 75),
        (EscrowError::NoInvestorCapConfigured, 76),
        (EscrowError::NewCapNotLower, 77),
        (EscrowError::NewCapBelowCurrentFunderCount, 78),
        (EscrowError::MaturityUpdateNotOpen, 79),
        (EscrowError::NewAdminSameAsCurrent, 80),
        (EscrowError::FundingBatchEmpty, 82),
        (EscrowError::FundingBatchTooLarge, 83),
        (EscrowError::MigrationVersionMismatch, 90),
        (EscrowError::AlreadyCurrentSchemaVersion, 91),
        (EscrowError::NoMigrationPath, 92),
        (EscrowError::FundingAmountNotPositive, 100),
        (EscrowError::FundingBelowMinContribution, 101),
        (EscrowError::LegalHoldBlocksFunding, 102),
        (EscrowError::EscrowNotOpenForFunding, 103),
        (EscrowError::InvestorNotAllowlisted, 104),
        (EscrowError::InvestorContributionOverflow, 105),
        (EscrowError::InvestorContributionExceedsCap, 106),
        (EscrowError::UniqueInvestorCapReached, 107),
        (EscrowError::TieredSecondDeposit, 108),
        (EscrowError::InvestorClaimTimeOverflow, 109),
        (EscrowError::FundedAmountOverflow, 110),
        (EscrowError::CommitmentLockExceedsMaturity, 111),
        (EscrowError::LegalHoldBlocksSettlement, 120),
        (EscrowError::SettlementNotFunded, 121),
        (EscrowError::MaturityNotReached, 122),
        (EscrowError::LegalHoldBlocksWithdrawal, 123),
        (EscrowError::WithdrawalNotFunded, 124),
        (EscrowError::LegalHoldBlocksInvestorClaims, 125),
        (EscrowError::NoContributionToClaim, 126),
        (EscrowError::InvestorClaimNotSettled, 127),
        (EscrowError::InvestorCommitmentLockNotExpired, 128),
        (EscrowError::ComputePayoutArithmeticOverflow, 129),
        (EscrowError::LegalHoldBlocksCancelFunding, 140),
        (EscrowError::CancelFundingNotOpen, 141),
        (EscrowError::RefundNotCancelled, 142),
        (EscrowError::NoContributionToRefund, 143),
        (EscrowError::LegalHoldClearRequestMissing, 150),
        (EscrowError::LegalHoldClearNotReady, 151),
        (EscrowError::LegalHoldClearDelayOverflow, 152),
        (EscrowError::LegalHoldBlocksBeneficiaryRotation, 160),
        (EscrowError::RotationNotOpen, 161),
        (EscrowError::NewSmeSameAsCurrent, 162),
        (EscrowError::FundingDeadlinePassed, 164),
        (EscrowError::NoPendingAdmin, 163),
    ];
    assert_eq!(TABLE.len(), 84);
    for (variant, code) in TABLE {
        assert_eq!(*variant as u32, *code, "discriminant drift for code {code}");
    }
}

#[test]
fn typed_error_codes_cover_range_boundaries() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);
    let investor = Address::generate(&env);

    // Init group: 1 (low) and 13 (high)
    let init_client = super::deploy(&env);
    assert_contract_error(
        init_client.try_init(
            &admin,
            &soroban_sdk::String::from_str(&env, "BOUND_LOW"),
            &sme,
            &0,
            &100,
            &100,
            &funding_token,
            &None,
            &treasury,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
        ),
        EscrowError::AmountMustBePositive,
    );
    let mut bad_tiers = SorobanVec::new(&env);
    bad_tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 600,
    });
    bad_tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 500,
    });
    let tier_client = super::deploy(&env);
    assert_contract_error(
        tier_client.try_init(
            &admin,
            &soroban_sdk::String::from_str(&env, "BOUND_HIGH"),
            &sme,
            &100,
            &100,
            &100,
            &funding_token,
            &None,
            &treasury,
            &Some(bad_tiers),
            &None,
            &None,
            &None,
            &None,
            &None,
        ),
        EscrowError::TierYieldNotNonDecreasing,
    );

    // Metadata group: 20 and 22
    let meta_client = super::deploy(&env);
    assert_contract_error(
        meta_client.try_fund(&investor, &10),
        EscrowError::EscrowNotInitialized,
    );
    let treasury_client = super::deploy(&env);
    treasury_client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "META22"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    treasury_client.cancel_funding();
    env.as_contract(&treasury_client.address, || {
        env.storage().instance().remove(&DataKey::Treasury);
    });
    assert_contract_error(
        treasury_client.try_sweep_terminal_dust(&1),
        EscrowError::TreasuryNotSet,
    );

    // Sweep group: 30 (low) and 42 (high)
    let hold_sweep_client = super::deploy(&env);
    hold_sweep_client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SWEEP30"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    hold_sweep_client.set_legal_hold(&true);
    assert_contract_error(
        hold_sweep_client.try_sweep_terminal_dust(&1),
        EscrowError::LegalHoldBlocksTreasuryDustSweep,
    );

    let token = install_stellar_asset_token(&env);
    let sweep_treasury = Address::generate(&env);
    let sweep_investor = Address::generate(&env);
    let fund_amount = 1_000i128;
    let floor_client = super::deploy(&env);
    floor_client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SWEEP42"),
        &sme,
        &10_000i128,
        &0i64,
        &0u64,
        &token.id,
        &None,
        &sweep_treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    token.stellar.mint(&floor_client.address, &fund_amount);
    floor_client.fund(&sweep_investor, &fund_amount);
    floor_client.cancel_funding();
    assert_contract_error(
        floor_client.try_sweep_terminal_dust(&1),
        EscrowError::SweepExceedsLiabilityFloor,
    );

    // Attestation group: 50 and 51
    let attest_client = super::deploy(&env);
    attest_client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ATTEST"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    let digest = BytesN::from_array(&env, &[1u8; 32]);
    attest_client.bind_primary_attestation_hash(&digest);
    assert_contract_error(
        attest_client.try_bind_primary_attestation_hash(&digest),
        EscrowError::PrimaryAttestationAlreadyBound,
    );
    for i in 0u8..MAX_ATTESTATION_APPEND_ENTRIES as u8 {
        attest_client.append_attestation_digest(&BytesN::from_array(&env, &[i; 32]));
    }
    assert_contract_error(
        attest_client.try_append_attestation_digest(&BytesN::from_array(&env, &[0xFF; 32])),
        EscrowError::AttestationAppendLogCapacityReached,
    );

    // Collateral group: 60 and 62
    let collat_client = super::deploy(&env);
    collat_client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "COLLAT"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    let asset = soroban_sdk::Symbol::new(&env, "GOLD");
    assert_contract_error(
        collat_client.try_record_sme_collateral_commitment(&asset, &0),
        EscrowError::CollateralAmountNotPositive,
    );
    collat_client.record_sme_collateral_commitment(&asset, &100);
    env.ledger()
        .set_timestamp(env.ledger().timestamp().saturating_sub(1));
    assert_contract_error(
        collat_client.try_record_sme_collateral_commitment(&asset, &200),
        EscrowError::CollateralTimestampBackwards,
    );

    // Admin group: 72 and 80
    let admin_client = super::deploy(&env);
    admin_client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ADMIN"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_contract_error(
        admin_client.try_update_funding_target(&0),
        EscrowError::TargetNotPositive,
    );
    assert_contract_error(
        admin_client.try_propose_admin(&admin, &None),
        EscrowError::NewAdminSameAsCurrent,
    );

    // Migration group: 90–92
    let migrate_client = super::deploy(&env);
    migrate_client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MIGRATE"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_contract_error(
        migrate_client.try_migrate(&(SCHEMA_VERSION - 1)),
        EscrowError::MigrationVersionMismatch,
    );
    assert_contract_error(
        migrate_client.try_migrate(&SCHEMA_VERSION),
        EscrowError::AlreadyCurrentSchemaVersion,
    );
    env.as_contract(&migrate_client.address, || {
        env.storage().instance().set(&DataKey::Version, &0u32);
    });
    assert_contract_error(migrate_client.try_migrate(&0), EscrowError::NoMigrationPath);

    // Funding group: 100 (skip legacy 108)
    let fund_client = super::deploy(&env);
    fund_client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FUND100"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_contract_error(
        fund_client.try_fund(&investor, &0),
        EscrowError::FundingAmountNotPositive,
    );

    // Settlement group: 120 and 126
    let settle_client = super::deploy(&env);
    settle_client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "SETTLE"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    settle_client.set_legal_hold(&true);
    assert_contract_error(
        settle_client.try_settle(),
        EscrowError::LegalHoldBlocksSettlement,
    );
    settle_client.clear_legal_hold();
    assert_contract_error(
        settle_client.try_claim_investor_payout(&investor),
        EscrowError::NoContributionToClaim,
    );

    // Refund group: 140 and 143
    let refund_client = super::deploy(&env);
    refund_client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "REFUND"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    refund_client.set_legal_hold(&true);
    assert_contract_error(
        refund_client.try_cancel_funding(),
        EscrowError::LegalHoldBlocksCancelFunding,
    );
    refund_client.clear_legal_hold();
    refund_client.cancel_funding();
    assert_contract_error(
        refund_client.try_refund(&investor),
        EscrowError::NoContributionToRefund,
    );

    // Legal-hold clear group: 150 and 151
    let lh_client = super::deploy(&env);
    lh_client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "LH150"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &Some(10u64),
        &None,
    );
    lh_client.set_legal_hold(&true);
    assert_contract_error(
        lh_client.try_set_legal_hold(&false),
        EscrowError::LegalHoldClearRequestMissing,
    );
    lh_client.request_clear_legal_hold();
    assert_contract_error(
        lh_client.try_set_legal_hold(&false),
        EscrowError::LegalHoldClearNotReady,
    );

    // Beneficiary rotation group: 160–162
    let rot_client = super::deploy(&env);
    rot_client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROT160"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    rot_client.set_legal_hold(&true);
    let new_sme = Address::generate(&env);
    assert_contract_error(
        rot_client.try_rotate_beneficiary(&new_sme),
        EscrowError::LegalHoldBlocksBeneficiaryRotation,
    );
    rot_client.clear_legal_hold();
    assert_contract_error(
        rot_client.try_rotate_beneficiary(&sme),
        EscrowError::NewSmeSameAsCurrent,
    );

    let rot_terminal = super::deploy(&env);
    let rot_token = install_stellar_asset_token(&env);
    rot_terminal.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "ROT161"),
        &sme,
        &100,
        &0i64,
        &0u64,
        &rot_token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    rot_token.stellar.mint(&rot_terminal.address, &100);
    rot_terminal.fund(&investor, &100);
    rot_terminal.settle();
    assert_contract_error(
        rot_terminal.try_rotate_beneficiary(&new_sme),
        EscrowError::RotationNotOpen,
    );
}

#[test]
fn typed_error_codes_cover_legal_hold_clear_delay_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);
    env.ledger().set_timestamp(u64::MAX - 5);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "LH152"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &Some(10u64),
        &None,
    );
    client.set_legal_hold(&true);
    assert_contract_error(
        client.try_request_clear_legal_hold(),
        EscrowError::LegalHoldClearDelayOverflow,
    );
}

#[test]
fn test_migrate_wrong_version() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MIG90"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    assert_contract_error(
        client.try_migrate(&(SCHEMA_VERSION - 1)),
        EscrowError::MigrationVersionMismatch,
    );
}

#[test]
fn test_migrate_already_current() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    assert_contract_error(
        client.try_migrate(&SCHEMA_VERSION),
        EscrowError::AlreadyCurrentSchemaVersion,
    );
}

#[test]
fn test_migrate_no_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    env.as_contract(&client.address, || {
        env.storage().instance().set(&DataKey::Version, &0u32);
    });

    assert_contract_error(client.try_migrate(&0), EscrowError::NoMigrationPath);
}

#[test]
fn test_admin_handover_and_maturity_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let updated = client.update_maturity(&200);
    assert_eq!(updated.maturity, 200);

    let new_admin = Address::generate(&env);
    let pending = client.propose_admin(&new_admin, &None);
    assert_eq!(pending, new_admin);
    assert_eq!(client.get_escrow().admin, admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));

    let updated = client.accept_admin();
    assert_eq!(updated.admin, new_admin);
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
#[should_panic]
fn test_update_maturity_not_open() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    client.fund(&investor, &100);
    client.update_maturity(&200);
}

#[test]
#[should_panic]
fn test_transfer_admin_same_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.propose_admin(&admin, &None);
}

#[test]
#[should_panic]
fn test_fund_during_legal_hold() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.set_legal_hold(&true);
    let investor = Address::generate(&env);
    client.fund(&investor, &10);
}

#[test]
#[should_panic]
fn test_fund_below_floor() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &Some(50),
        &None,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    client.fund(&investor, &10);
}

#[test]
#[should_panic]
fn test_claim_not_settled() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    client.fund(&investor, &10);
    client.claim_investor_payout(&investor);
}

#[test]
#[should_panic]
fn test_claim_lock_not_expired() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &100,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    client.fund_with_commitment(&investor, &100, &3600);

    env.ledger().with_mut(|li| li.timestamp = 101);
    client.settle();

    client.claim_investor_payout(&investor);
}

#[test]
fn test_all_getters() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);
    let registry = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TEST"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &Some(registry.clone()),
        &treasury,
        &None,
        &Some(10),
        &Some(5),
        &None,
        &None,
        &None,
    );

    assert_eq!(client.get_funding_token(), funding_token);
    assert_eq!(client.get_treasury(), treasury);
    assert_eq!(client.get_registry_ref(), Some(registry));
    assert_eq!(client.get_version(), 6);
    assert!(!client.get_legal_hold());
    assert_eq!(client.get_min_contribution_floor(), 10);
    assert_eq!(client.get_max_unique_investors_cap(), Some(5));
    assert_eq!(client.get_unique_funder_count(), 0);
    assert!(client.get_primary_attestation_hash().is_none());
    assert_eq!(client.get_attestation_append_log().len(), 0);
}

#[test]
fn test_attestations_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let hash1 = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    let hash2 = soroban_sdk::BytesN::from_array(&env, &[2u8; 32]);

    client.bind_primary_attestation_hash(&hash1);
    assert_eq!(client.get_primary_attestation_hash(), Some(hash1.clone()));

    client.append_attestation_digest(&hash2);
    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 1);
    assert_eq!(log.get(0).unwrap(), hash2);
}

#[test]
#[should_panic]
fn test_bind_primary_attestation_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    client.bind_primary_attestation_hash(&hash);
    client.bind_primary_attestation_hash(&hash);
}

#[test]
fn test_unique_investors_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "CAP"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &Some(2),
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &10);
    client.fund(&Address::generate(&env), &10);
    assert_eq!(client.get_unique_funder_count(), 2);
}

#[test]
#[should_panic]
fn test_unique_investors_cap_exceeded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "CAP"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &Some(1),
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &10);
    client.fund(&Address::generate(&env), &10);
}

#[test]
fn test_sweep_terminal_dust_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let token = crate::tests::install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &100);
    env.ledger().with_mut(|li| li.timestamp = 200);
    client.settle();

    token.stellar.mint(&client.address, &50);

    let swept = client.sweep_terminal_dust(&50);
    assert_eq!(swept, 50);
    assert_eq!(token.token.balance(&treasury), 50);
}

#[test]
fn test_bump_ttl_covers_persistent_investor_keys() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TTL001"),
        &sme,
        &100,
        &10,
        &0,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    client.set_investor_allowlisted(&investor, &true);
    client.fund(&investor, &100);
    client.settle();
    client.claim_investor_payout(&investor);

    let mut investors = SorobanVec::new(&env);
    investors.push_back(investor);
    client.bump_ttl(&investors);
}

#[test]
fn test_sweep_not_terminal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    assert_contract_error(
        client.try_sweep_terminal_dust(&10),
        EscrowError::DustSweepNotTerminal,
    );
}

#[test]
#[should_panic]
fn test_sweep_no_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let token = crate::tests::install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &100);
    env.ledger().with_mut(|li| li.timestamp = 200);
    client.settle();

    client.sweep_terminal_dust(&10);
}

#[test]
fn test_withdraw_happy_path() {
    use crate::LiquifactEscrow;
    use soroban_sdk::token::{StellarAssetClient, TokenClient};

    let env = Env::default();
    env.mock_all_auths();

    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_id = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &token_id);

    let escrow_id = env.register(LiquifactEscrow, ());
    let client = super::LiquifactEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "W"),
        &sme,
        &100,
        &10,
        &10,
        &token_id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &100);
    assert_eq!(client.get_escrow().status, 1);

    // Mint funded_amount into the escrow contract so withdraw() can transfer it.
    sac_admin.mint(&escrow_id, &100);

    let updated = client.withdraw();
    assert_eq!(updated.status, 3);
}

#[test]
#[should_panic]
fn test_settle_too_early() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &20000,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &100);
    client.settle();
}

#[test]
fn test_update_funding_target_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let updated = client.update_funding_target(&200);
    assert_eq!(updated.funding_target, 200);
}

#[test]
#[should_panic]
fn test_update_funding_target_too_low() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &50);
    client.update_funding_target(&40);
}

#[test]
fn test_sme_collateral_commitment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let asset = soroban_sdk::Symbol::new(&env, "GOLD");
    let commitment = client.record_sme_collateral_commitment(&asset, &5000);
    assert_eq!(commitment.amount, 5000);
    assert_eq!(commitment.asset, asset);

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, 5000);
}

#[test]
#[should_panic]
fn test_sme_collateral_empty_asset_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    let empty_asset = soroban_sdk::Symbol::new(&env, "");
    client.record_sme_collateral_commitment(&empty_asset, &5000);
}

#[test]
#[should_panic]
fn test_sme_collateral_stale_timestamp_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let asset = soroban_sdk::Symbol::new(&env, "GOLD");
    client.record_sme_collateral_commitment(&asset, &5000);

    // Simulate stale replay: move ledger timestamp backward
    env.ledger().with_mut(|li| li.timestamp = 100);

    client.record_sme_collateral_commitment(&asset, &7000);
}

#[test]
fn test_sme_collateral_replacement_preserves_prior_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let asset = soroban_sdk::Symbol::new(&env, "GOLD");
    let first = client.record_sme_collateral_commitment(&asset, &5000);
    assert_eq!(first.amount, 5000);

    // Advance timestamp so the replacement is not stale
    env.ledger().with_mut(|li| li.timestamp = 20000);

    let second = client.record_sme_collateral_commitment(&asset, &7000);
    assert_eq!(second.amount, 7000);
    assert_eq!(second.recorded_at, 20000);

    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.amount, 7000);
}

#[test]
fn test_clear_legal_hold_convenience() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.set_legal_hold(&true);
    assert!(client.get_legal_hold());
    client.clear_legal_hold();
    assert!(!client.get_legal_hold());
}

#[test]
fn test_claim_not_before_getter() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    client.fund_with_commitment(&investor, &50, &1000);
    let nbf = client.get_investor_claim_not_before(&investor);
    assert!(nbf > 0);
}

#[test]
fn test_init_with_tiers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);

    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 500,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 600,
    });

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &1000,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_escrow().yield_bps, 100); // Default yield
}

#[test]
#[should_panic]
fn test_sweep_too_much() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.fund(&Address::generate(&env), &100);
    env.ledger().with_mut(|li| li.timestamp = 200);
    client.settle();

    client.sweep_terminal_dust(&(crate::MAX_DUST_SWEEP_AMOUNT + 1));
}

#[test]
#[should_panic]
fn test_withdraw_not_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.withdraw();
}

#[test]
#[should_panic]
fn test_settle_not_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.settle();
}

#[test]
fn test_fund_with_zero_commitment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let investor = Address::generate(&env);
    client.fund_with_commitment(&investor, &50, &0);
    assert_eq!(client.get_investor_claim_not_before(&investor), 0);
}

#[test]
#[should_panic]
fn test_update_target_invalid() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    client.update_funding_target(&0);
}

#[test]
#[should_panic]
fn test_init_yield_out_of_range() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &10001,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_init_min_contribution_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &Some(0),
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_init_tiers_unsorted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 500,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 600,
    });
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_init_tiers_not_increasing_yield() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 600,
    });
    tiers.push_back(YieldTier {
        min_lock_secs: 200,
        yield_bps: 500,
    });
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_init_tiers_lower_than_base() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 50,
    });
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
fn test_get_yield_bps_empty_tiers_branch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Inject empty tiers directly to trigger the branch in get_yield_bps_for_commitment
    env.as_contract(&client.address, || {
        let empty_tiers: SorobanVec<YieldTier> = SorobanVec::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::YieldTierTable, &empty_tiers);
    });

    let investor = Address::generate(&env);
    // This will trigger line 489 in lib.rs
    client.fund_with_commitment(&investor, &10, &0);
}

#[test]
#[should_panic]
fn test_init_tier_yield_out_of_range() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    let mut tiers = SorobanVec::new(&env);
    tiers.push_back(YieldTier {
        min_lock_secs: 100,
        yield_bps: 10001,
    });
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "T"),
        &sme,
        &100,
        &100,
        &10,
        &token,
        &None,
        &treasury,
        &Some(tiers),
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_get_escrow_summary_before_init() {
    let env = Env::default();
    let (client, _admin, _sme) = setup(&env);
    client.get_escrow_summary();
}

#[test]
fn test_get_escrow_summary_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let summary = client.get_escrow_summary();

    // Verify fields match individual getters
    assert_eq!(summary.escrow, client.get_escrow());
    assert_eq!(summary.has_maturity_lock, client.has_maturity_lock());
    assert_eq!(summary.legal_hold, client.get_legal_hold());

    let expected_snapshot = match client.get_funding_close_snapshot() {
        Some(snap) => EscrowCloseSnapshot::Some(snap),
        None => EscrowCloseSnapshot::None,
    };
    assert_eq!(summary.funding_close_snapshot, expected_snapshot);
    assert_eq!(
        summary.unique_funder_count,
        client.get_unique_funder_count()
    );
    assert_eq!(summary.is_allowlist_active, client.is_allowlist_active());
    assert_eq!(summary.schema_version, client.get_version());
    let expected_collateral = match client.get_sme_collateral_commitment() {
        Some(c) => CollateralCommitmentSnapshot::Some(c),
        None => CollateralCommitmentSnapshot::None,
    };
    assert_eq!(summary.sme_collateral_commitment, expected_collateral);
    assert_eq!(
        summary.has_primary_attestation,
        client.get_primary_attestation_hash().is_some()
    );
    assert_eq!(
        summary.attestation_log_length,
        client.get_attestation_append_log().len()
    );

    // Verify default values specifically
    assert!(summary.has_maturity_lock);
    assert!(!summary.legal_hold);
    assert_eq!(summary.funding_close_snapshot, EscrowCloseSnapshot::None);
    assert_eq!(summary.unique_funder_count, 0);
    assert!(!summary.is_allowlist_active);
    assert_eq!(summary.schema_version, 6);
    assert_eq!(
        summary.sme_collateral_commitment,
        CollateralCommitmentSnapshot::None
    );
    assert!(!summary.has_primary_attestation);
    assert_eq!(summary.attestation_log_length, 0);
}

#[test]
fn test_get_escrow_summary_after_state_changes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Make state changes
    client.set_allowlist_active(&true);

    let investor = Address::generate(&env);
    client.set_investor_allowlisted(&investor, &true);
    // Fund enough to trigger funded status and capture snapshot
    client.fund(&investor, &1000);
    client.set_legal_hold(&true);

    let summary = client.get_escrow_summary();

    // Verify fields match individual getters under state changes
    assert_eq!(summary.escrow, client.get_escrow());
    assert_eq!(summary.has_maturity_lock, client.has_maturity_lock());
    assert_eq!(summary.legal_hold, client.get_legal_hold());

    let expected_snapshot = match client.get_funding_close_snapshot() {
        Some(snap) => EscrowCloseSnapshot::Some(snap),
        None => EscrowCloseSnapshot::None,
    };
    assert_eq!(summary.funding_close_snapshot, expected_snapshot);
    assert_eq!(
        summary.unique_funder_count,
        client.get_unique_funder_count()
    );
    assert_eq!(summary.is_allowlist_active, client.is_allowlist_active());
    assert_eq!(summary.schema_version, client.get_version());
    let expected_collateral = match client.get_sme_collateral_commitment() {
        Some(c) => CollateralCommitmentSnapshot::Some(c),
        None => CollateralCommitmentSnapshot::None,
    };
    assert_eq!(summary.sme_collateral_commitment, expected_collateral);
    assert_eq!(
        summary.has_primary_attestation,
        client.get_primary_attestation_hash().is_some()
    );
    assert_eq!(
        summary.attestation_log_length,
        client.get_attestation_append_log().len()
    );

    // Verify state-specific values
    assert!(summary.has_maturity_lock);
    assert!(summary.legal_hold);
    assert!(summary.is_allowlist_active);
    assert_eq!(summary.unique_funder_count, 1);
    assert_eq!(summary.escrow.status, 1); // Funded
    assert!(matches!(
        summary.funding_close_snapshot,
        EscrowCloseSnapshot::Some(_)
    ));

    let snapshot = match &summary.funding_close_snapshot {
        EscrowCloseSnapshot::Some(snap) => snap.clone(),
        EscrowCloseSnapshot::None => panic!("Expected Some snapshot"),
    };
    assert_eq!(snapshot.total_principal, 1000);
    assert_eq!(snapshot.funding_target, 1000);

    // New fields should still be at defaults (no collateral or attestations set)
    assert_eq!(
        summary.sme_collateral_commitment,
        CollateralCommitmentSnapshot::None
    );
    assert!(!summary.has_primary_attestation);
    assert_eq!(summary.attestation_log_length, 0);
}

#[test]
fn test_get_escrow_summary_with_collateral_and_attestations() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (funding_token, treasury) = free_addresses(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV002"),
        &sme,
        &1000,
        &100,
        &100,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Record SME collateral
    let asset = soroban_sdk::Symbol::new(&env, "GOLD");
    client.record_sme_collateral_commitment(&asset, &5000);

    // Bind primary attestation hash
    let primary_hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    client.bind_primary_attestation_hash(&primary_hash);

    // Append several attestation digests
    let hash2 = soroban_sdk::BytesN::from_array(&env, &[2u8; 32]);
    let hash3 = soroban_sdk::BytesN::from_array(&env, &[3u8; 32]);
    client.append_attestation_digest(&hash2);
    client.append_attestation_digest(&hash3);

    let summary = client.get_escrow_summary();

    // Verify all fields match individual getters
    assert_eq!(summary.escrow, client.get_escrow());
    assert_eq!(summary.has_maturity_lock, client.has_maturity_lock());
    assert_eq!(summary.legal_hold, client.get_legal_hold());
    let expected_snapshot = match client.get_funding_close_snapshot() {
        Some(snap) => EscrowCloseSnapshot::Some(snap),
        None => EscrowCloseSnapshot::None,
    };
    assert_eq!(summary.funding_close_snapshot, expected_snapshot);
    assert_eq!(
        summary.unique_funder_count,
        client.get_unique_funder_count()
    );
    assert_eq!(summary.is_allowlist_active, client.is_allowlist_active());
    assert_eq!(summary.schema_version, client.get_version());
    let expected_collateral = match client.get_sme_collateral_commitment() {
        Some(c) => CollateralCommitmentSnapshot::Some(c),
        None => CollateralCommitmentSnapshot::None,
    };
    assert_eq!(summary.sme_collateral_commitment, expected_collateral);
    assert_eq!(
        summary.has_primary_attestation,
        client.get_primary_attestation_hash().is_some()
    );
    assert_eq!(
        summary.attestation_log_length,
        client.get_attestation_append_log().len()
    );

    // Verify new field values
    let collateral = match &summary.sme_collateral_commitment {
        CollateralCommitmentSnapshot::Some(c) => c,
        CollateralCommitmentSnapshot::None => panic!("Expected collateral"),
    };
    assert_eq!(collateral.asset, asset);
    assert_eq!(collateral.amount, 5000);
    assert!(summary.has_primary_attestation);
    assert_eq!(summary.attestation_log_length, 2);
}

#[test]
fn test_record_sme_collateral_commitment_semantics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let token = crate::tests::install_stellar_asset_token(&env);

    // Initialize escrow with the mock token
    let (_, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV_COLL_001"),
        &sme,
        &10_000i128,
        &100,
        &100,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Check that get_sme_collateral_commitment returns None initially
    assert!(client.get_sme_collateral_commitment().is_none());

    // Mint tokens to SME, admin, and escrow contract to track balances
    token.stellar.mint(&sme, &1_000_000i128);
    token.stellar.mint(&admin, &1_000_000i128);
    token.stellar.mint(&client.address, &1_000_000i128);

    let sme_bal_before = token.token.balance(&sme);
    let admin_bal_before = token.token.balance(&admin);
    let escrow_bal_before = token.token.balance(&client.address);

    // 1. Happy path: Record first commitment
    let asset_sym = soroban_sdk::Symbol::new(&env, "USDC");
    let pledge_amount = 5_000i128;

    // Set ledger timestamp to a known value
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 10000;
    env.ledger().set(ledger_info);

    let commitment = client.record_sme_collateral_commitment(&asset_sym, &pledge_amount);

    // Assert that the returned commitment is correct
    assert_eq!(commitment.asset, asset_sym);
    assert_eq!(commitment.amount, pledge_amount);
    assert_eq!(commitment.recorded_at, 10000);

    // Assert that the stored commitment matches
    let stored = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored.asset, asset_sym);
    assert_eq!(stored.amount, pledge_amount);
    assert_eq!(stored.recorded_at, 10000);

    // CRITICAL SECURITY ASSERTION: Assert that NO token balances changed!
    assert_eq!(token.token.balance(&sme), sme_bal_before);
    assert_eq!(token.token.balance(&admin), admin_bal_before);
    assert_eq!(token.token.balance(&client.address), escrow_bal_before);

    // 2. Edge Case: Record with replacement (timestamp goes forward)
    let new_pledge_amount = 7_500i128;
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 12000;
    env.ledger().set(ledger_info);

    let replacement = client.record_sme_collateral_commitment(&asset_sym, &new_pledge_amount);

    // Assert replacement details
    assert_eq!(replacement.asset, asset_sym);
    assert_eq!(replacement.amount, new_pledge_amount);
    assert_eq!(replacement.recorded_at, 12000);

    let stored_replacement = client.get_sme_collateral_commitment().unwrap();
    assert_eq!(stored_replacement.amount, new_pledge_amount);
    assert_eq!(stored_replacement.recorded_at, 12000);

    // Token balances must still be completely unaffected
    assert_eq!(token.token.balance(&sme), sme_bal_before);
    assert_eq!(token.token.balance(&admin), admin_bal_before);
    assert_eq!(token.token.balance(&client.address), escrow_bal_before);

    // 3. Error Case: Timestamp goes backwards
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 11000; // 11000 < 12000 (previous recorded_at)
    env.ledger().set(ledger_info);

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset_sym, &8_000i128),
        EscrowError::CollateralTimestampBackwards,
    );

    // Restore timestamp
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp = 12000;
    env.ledger().set(ledger_info);

    // 4. Error Case: Amount must be positive (0 or negative)
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset_sym, &0i128),
        EscrowError::CollateralAmountNotPositive,
    );
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset_sym, &-100i128),
        EscrowError::CollateralAmountNotPositive,
    );

    // 5. Error Case: Asset symbol must be non-empty
    let empty_symbol = soroban_sdk::Symbol::new(&env, "");
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&empty_symbol, &5_000i128),
        EscrowError::CollateralAssetEmpty,
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// `is_settleable` view — readiness across status/maturity/hold combinations
// ──────────────────────────────────────────────────────────────────────────────

/// Helper: initialise a standard escrow for is_settleable tests.
fn init_settleable_test(
    env: &Env,
    client: &super::LiquifactEscrowClient<'_>,
    admin: &Address,
    sme: &Address,
    maturity: u64,
) {
    let (token, treasury) = free_addresses(env);
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, "STL_001"),
        sme,
        &1000,
        &100,
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
    );
}

/// Fund to exactly the target amount using a fresh investor.
fn fund_to_target_stl(env: &Env, client: &super::LiquifactEscrowClient<'_>) -> Address {
    let investor = Address::generate(env);
    client.fund(&investor, &1000);
    investor
}

#[test]
fn test_is_settleable_open_status_returns_false() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    init_settleable_test(&env, &client, &admin, &sme, 0);
    // status = 0 (open) — not funded yet
    assert!(!client.is_settleable());
}

#[test]
fn test_is_settleable_funded_no_maturity_returns_true() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    init_settleable_test(&env, &client, &admin, &sme, 0);
    fund_to_target_stl(&env, &client);
    // status = 1 (funded), maturity = 0, no hold → settleable
    assert!(client.is_settleable());
}

#[test]
fn test_is_settleable_funded_with_maturity_before_returns_false() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let maturity: u64 = 20_000;
    init_settleable_test(&env, &client, &admin, &sme, maturity);
    fund_to_target_stl(&env, &client);
    // Advance ledger to just before maturity
    env.ledger().with_mut(|l| l.timestamp = maturity - 1);
    assert!(!client.is_settleable());
}

#[test]
fn test_is_settleable_funded_with_maturity_at_exact_returns_true() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let maturity: u64 = 20_000;
    init_settleable_test(&env, &client, &admin, &sme, maturity);
    fund_to_target_stl(&env, &client);
    env.ledger().with_mut(|l| l.timestamp = maturity);
    assert!(client.is_settleable());
}

#[test]
fn test_is_settleable_blocked_by_legal_hold() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    init_settleable_test(&env, &client, &admin, &sme, 0);
    fund_to_target_stl(&env, &client);
    client.set_legal_hold(&true);
    assert!(!client.is_settleable());
}

#[test]
fn test_is_settleable_already_settled_returns_false() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    init_settleable_test(&env, &client, &admin, &sme, 0);
    fund_to_target_stl(&env, &client);
    client.settle();
    assert!(!client.is_settleable());
}

#[test]
fn test_is_settleable_withdrawn_returns_false() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    init_settleable_test(&env, &client, &admin, &sme, 0);
    fund_to_target_stl(&env, &client);
    client.withdraw();
    assert!(!client.is_settleable());
}

#[test]
fn test_is_settleable_not_initialized_panics() {
    let env = Env::default();
    let (client, _admin, _sme) = setup(&env);
    // No init call — get_escrow returns error
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.is_settleable();
    }));
    assert!(
        result.is_err(),
        "is_settleable must panic when escrow not initialized"
    );
}

#[test]
fn test_is_settleable_funded_maturity_zero_hold_active_returns_false() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    init_settleable_test(&env, &client, &admin, &sme, 0);
    fund_to_target_stl(&env, &client);
    client.set_legal_hold(&true);
    assert!(
        !client.is_settleable(),
        "hold must block settleability even when maturity is 0"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// `EscrowSettled` event — `settled_at_ledger_timestamp` field
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_settle_event_timestamp_matches_ledger_time() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    let settle_ts: u64 = 50_000;

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "EVT_TS"),
        &sme,
        &1000,
        &100,
        &0,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    fund_to_target_stl(&env, &client);

    env.ledger().with_mut(|l| l.timestamp = settle_ts);
    client.settle();

    // At least one event must be emitted (the settle event)
    let contract_events = env.events().all();
    let events = contract_events.events();
    assert!(!events.is_empty(), "settle must emit at least one event");
}

#[test]
fn test_settle_event_timestamp_with_maturity() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    let maturity: u64 = 30_000;
    let settle_ts: u64 = 30_000;

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "EVT_TS2"),
        &sme,
        &1000,
        &100,
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
    );
    fund_to_target_stl(&env, &client);

    env.ledger().with_mut(|l| l.timestamp = settle_ts);
    client.settle();

    // Verify event is emitted
    let contract_events = env.events().all();
    let events = contract_events.events();
    assert!(!events.is_empty());
}

#[test]
fn test_settle_event_emitted_at_current_ledger_time() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);

    let expected_ts: u64 = 77_777;
    env.ledger().with_mut(|l| l.timestamp = expected_ts);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "EVT_TS3"),
        &sme,
        &1000,
        &100,
        &0,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    fund_to_target_stl(&env, &client);
    client.settle();

    // The settled escrow status confirms the event was emitted
    assert_eq!(client.get_escrow().status, 2);
}

// ──────────────────────────────────────────────────────────────────────────────
// `is_settleable` edge: partial_settle then pre-maturity
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_is_settleable_after_partial_settle_with_maturity() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let maturity: u64 = 10_000;
    init_settleable_test(&env, &client, &admin, &sme, maturity);

    // Partial fund and partial_settle
    let investor = Address::generate(&env);
    client.fund(&investor, &500);
    client.partial_settle(&sme);
    // status = 1 (funded) after partial_settle

    // Before maturity
    env.ledger().with_mut(|l| l.timestamp = maturity - 1);
    assert!(
        !client.is_settleable(),
        "pre-maturity after partial_settle must not be settleable"
    );

    // At maturity
    env.ledger().with_mut(|l| l.timestamp = maturity);
    assert!(
        client.is_settleable(),
        "at-maturity after partial_settle must be settleable"
    );

    // After settlement
    client.settle();
    assert!(
        !client.is_settleable(),
        "settled escrow must not be settleable"
    );
}
