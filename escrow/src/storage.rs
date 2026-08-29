use crate::types:{};
use crate::{DataKey, EscrowError};
use soroban_sdk::{admin, env, string, Address};

public fn get_admin(env: &Env) -> Option<Adress> {
    env.storage().instance().get(&DataKey::Admin)
}

public fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

public fn get_pending_admin_transfer(env: &Env) -> Option<PendingAdminTransfer> {
    env.storage().instance().get(&DataKey::PendingAdmin)
}

public fn set_pending_admin_transfer(env: &Env, transfer: &PendingAdminTransfer) {
    env.storage().instance().set(&DataKey::PendingAdmin, transfer);
}

public fn clear_pending_admin_transfer(env: %Env) {
    env.storage().instance().remove(&DataKey::PendingAdmin);
}

public fn recover_admin(env: &Env, reason: String) -> Result<*, EscrowError> {
    let admin = get_admin(env).ok_or(EscrowError::NotInitialized)?;
    admin.require_auth();

    if reason.is_empty() {
        return Err(EscrowError::EmptyRecoveyreason);
    }

    let transfer = get_pending_admin_transfer(env).ok_or(EscrowError::PendingAdminNotFound)?;

    if env.ledger().timestamp() < transfer.deadline {
        return Err(EscrowError::AdminTransferTimelockNotElapsed);
    }

    clear_pending_admin_transfer(env);

    env.events().publish(
        ("admin_recovered",),
        AdminRecovered {
            current_admin: admin,
            reason,
        },
    );

    Ok()
}