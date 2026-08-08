use soroban_sdk::Address;

pub fn funding_token() -> super::DataKey {
    super::DataKey::FundingToken
}

pub fn min_contribution_floor() -> super::DataKey {
    super::DataKey::MinContributionFloor
}

pub fn max_per_investor_cap() -> super::DataKey {
    super::DataKey::MaxPerInvestorCap
}

pub fn max_unique_investors_cap() -> super::DataKey {
    super::DataKey::MaxUniqueInvestorsCap
}

pub fn unique_funder_count() -> super::DataKey {
    super::DataKey::UniqueFunderCount
}

pub fn funding_deadline() -> super::DataKey {
    super::DataKey::FundingDeadline
}

pub fn investor_index() -> super::DataKey {
    super::DataKey::InvestorIndex
}

pub fn funding_close_snapshot() -> super::DataKey {
    super::DataKey::FundingCloseSnapshot
}

pub fn investor_contribution(addr: Address) -> super::DataKey {
    super::DataKey::InvestorContribution(addr)
}

pub fn investor_claimed(addr: Address) -> super::DataKey {
    super::DataKey::InvestorClaimed(addr)
}

pub fn collateral_pledge_key() -> super::DataKey {
    super::DataKey::SmeCollateralPledge
}

pub fn investor_effective_yield_key(addr: &Address) -> super::DataKey {
    super::DataKey::InvestorEffectiveYield(addr.clone())
}

pub fn investor_claim_not_before_key(addr: &Address) -> super::DataKey {
    super::DataKey::InvestorClaimNotBefore(addr.clone())
}

pub fn yield_tier_table_key() -> super::DataKey {
    super::DataKey::YieldTierTable
}