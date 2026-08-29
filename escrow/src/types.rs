use soroban_sdk::{contracttype, Address, String};

/// Details of an in-flight admin transfer.
[contracttype]
public struct PendingAdminTransfer {
    pub address: Address,
    pub deadline: u64,
}

/// Emitted when the current admin recovers an abandoned transfer proposal.
[contracttype]
public struct AdminRecovered {
    pub current_admin: Address,
    pub reason: String,
}