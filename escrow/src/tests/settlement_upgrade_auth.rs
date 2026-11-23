//! Upgrade entrypoints must reject missing or non-admin authorization without
//! creating, replacing, executing, or cancelling a pending upgrade.

use super::{default_init, setup, LiquifactEscrowClient};
use soroban_sdk::{
    testutils::{Ledger as _, MockAuth, MockAuthInvoke},
    xdr::{ScErrorCode, ScErrorType},
    BytesN, Env, Error, IntoVal, InvokeError,
};
use std::fmt::Debug;

#[derive(Clone, Copy, Debug)]
enum UpgradeCall {
    Propose,
    Execute,
    Cancel,
    Legacy,
}

impl UpgradeCall {
    fn name(self) -> &'static str {
        match self {
            Self::Propose => "propose_upgrade",
            Self::Execute => "execute_upgrade",
            Self::Cancel => "cancel_upgrade",
            Self::Legacy => "upgrade",
        }
    }

    fn assert_rejected(self, client: &LiquifactEscrowClient<'_>, hash: &BytesN<32>) {
        match self {
            Self::Propose => assert_auth_error(client.try_propose_upgrade(hash)),
            Self::Execute => assert_auth_error(client.try_execute_upgrade()),
            Self::Cancel => assert_auth_error(client.try_cancel_upgrade()),
            Self::Legacy => assert_auth_error(client.try_upgrade(hash)),
        }
    }
}

fn assert_auth_error<T: Debug, E: Debug>(result: Result<Result<T, E>, Result<Error, InvokeError>>) {
    // try_call normalizes host errors to Context/InvalidAction. Contract errors
    // such as NoPendingUpgrade are preserved and must not satisfy this check.
    match result {
        Err(Ok(error)) => assert_eq!(
            error,
            Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction),
            "expected a host authorization rejection, got {error:?}"
        ),
        other => panic!("expected a host authorization rejection, got {other:?}"),
    }
}

fn assert_upgrade_auth_matrix(call: UpgradeCall) {
    for has_pending in [false, true] {
        for use_non_admin_auth in [false, true] {
            let env = Env::default();
            let (client, admin, sme) = setup(&env);
            default_init(&client, &env, &admin, &sme);

            if has_pending {
                // Mature the authorized proposal so timing cannot mask an auth failure.
                let eta = client.propose_upgrade(&BytesN::from_array(&env, &[1; 32]));
                env.ledger().set_timestamp(eta);
            }
            let pending_before = client.get_pending_upgrade();
            assert_eq!(pending_before.is_some(), has_pending);
            let new_hash = BytesN::from_array(&env, &[2; 32]);

            if use_non_admin_auth {
                // The SME is a legitimate escrow role, but cannot administer upgrades.
                let args = match call {
                    UpgradeCall::Propose | UpgradeCall::Legacy => {
                        (new_hash.clone(),).into_val(&env)
                    }
                    UpgradeCall::Execute | UpgradeCall::Cancel => soroban_sdk::vec![&env],
                };
                env.mock_auths(&[MockAuth {
                    address: &sme,
                    invoke: &MockAuthInvoke {
                        contract: &client.address,
                        fn_name: call.name(),
                        args,
                        sub_invokes: &[],
                    },
                }]);
            } else {
                // Clear setup's blanket mocks and enforce real host auth with no entries.
                env.set_auths(&[]);
            }

            // A missing-proposal contract error must not accidentally count as an
            // authorization rejection when testing execute or cancel.
            call.assert_rejected(&client, &new_hash);
            assert_eq!(
                client.get_pending_upgrade(),
                pending_before,
                "{call:?} changed pending upgrade after a rejected call"
            );
        }
    }
}

#[test]
fn propose_upgrade_requires_admin_auth() {
    assert_upgrade_auth_matrix(UpgradeCall::Propose);
}

#[test]
fn execute_upgrade_requires_admin_auth() {
    assert_upgrade_auth_matrix(UpgradeCall::Execute);
}

#[test]
fn cancel_upgrade_requires_admin_auth() {
    assert_upgrade_auth_matrix(UpgradeCall::Cancel);
}

#[test]
fn legacy_upgrade_requires_admin_auth() {
    assert_upgrade_auth_matrix(UpgradeCall::Legacy);
}
