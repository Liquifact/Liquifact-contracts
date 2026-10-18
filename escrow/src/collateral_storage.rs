//! Centralised, concurrency-hardened storage for the SME collateral subsystem.
//!
//! # Why this module exists
//!
//! The collateral lifecycle used to be open-coded at the entrypoints: each function
//! read and wrote [`DataKey::SmeCollateralPledge`] / [`DataKey::CollateralLimit`]
//! inline, with the ordering of the timestamp check, the configured-limit check and the
//! write left implicit. That is fragile under repeated or re-entrant execution — a
//! retried/duplicated commitment, or a second mutation interleaved before the first has
//! returned, could observe or persist a partially-applied state. This module makes the
//! whole lifecycle go through one place with explicit, testable guarantees.
//!
//! # Execution model (why this is safe on Soroban)
//!
//! Soroban executes one invocation at a time and commits all storage writes of a
//! succeeded invocation as a single atomic unit: if the invocation panics or returns an
//! error, **none** of its writes are visible. "Concurrent execution" therefore does not
//! mean parallel threads stomping on cells; it means:
//!
//! * **Re-entrancy** — the contract can be re-entered (for example via a token or
//!   callback) before the outer call has returned, mutating the same collateral cell
//!   twice in one invocation.
//! * **Retries / duplicates** — an at-least-once client can submit the same commitment
//!   twice.
//! * **Timing boundaries** — a replacement must be a pure function of
//!   `env.ledger().timestamp()` so every validator agrees.
//!
//! The guarantees below are aimed exactly at those.
//!
//! # Invariants
//!
//! 1. **Exclusive mutation.** Every mutating operation runs under
//!    [`CollateralStorageKey::MutationLock`]. A second mutation while the lock is held
//!    fails fast with [`EscrowError::ConcurrentMutation`] and changes nothing.
//! 2. **Monotone timestamps.** A replacement is rejected with
//!    [`EscrowError::CollateralTimestampBackwards`] if `now < prior.recorded_at`; a
//!    commitment's `recorded_at` is therefore non-decreasing.
//! 3. **Idempotent retry.** Re-submitting the *same* `(asset, amount)` when a commitment
//!    already exists is a deterministic no-op ([`RecordOutcome::Unchanged`]): the stored
//!    `recorded_at` is preserved and the caller can suppress a duplicate event.
//! 4. **Bounded amount.** A commitment is rejected with
//!    [`EscrowError::CollateralLimitExceeded`] when `amount` exceeds the stored ceiling;
//!    the ceiling itself is bounded by [`MAX_INVOICE_AMOUNT`].
//! 5. **Clear is once-only.** Clearing returns the removed commitment or
//!    [`EscrowError::NoCollateralToClear`]; it can never double-remove.
//! 6. **Pure reads.** The `version` / `limit` / `commitment` views never mutate storage.
//!    In particular the schema version is never touched by a collateral mutation, so the
//!    version view is stable across the whole lifecycle.
//! 7. **No leaked lock.** The lock is released by RAII on every normal return path; a
//!    panic unwinds the invocation and rolls the lock write back too, so a transaction can
//!    never strand it.
//!
//! # Backward compatibility
//!
//! The persisted collateral layout ([`DataKey::SmeCollateralPledge`],
//! [`DataKey::CollateralLimit`]) and every public signature are unchanged. The only new
//! cell is the additive [`CollateralStorageKey::MutationLock`] flag (ADR-007): absent ⇒
//! unlocked, so instances written before this change behave identically. No `migrate`
//! call is required and [`crate::SCHEMA_VERSION`] is not bumped.

use crate::{DataKey, EscrowError, SmeCollateralCommitment, MAX_INVOICE_AMOUNT};
use soroban_sdk::{contracttype, Env, Symbol};

/// Additive instance-storage keys owned by the collateral storage layer.
///
/// Kept separate from [`DataKey`] so the lock is an implementation detail of this module
/// and can never be confused with a persisted domain value.
#[contracttype]
#[derive(Clone)]
pub enum CollateralStorageKey {
    /// Exclusive mutation lock held while a collateral transition is in flight.
    /// Absent / `false` ⇒ no mutation in progress. Used to reject re-entrant or
    /// overlapping writes with [`EscrowError::ConcurrentMutation`].
    ///
    /// **Additive key (ADR-007):** absent on instances predating this guard, which read as
    /// unlocked and behave identically.
    MutationLock,
}

/// Transient in-memory view of the collateral cells.
///
/// Deliberately **not** a `#[contracttype]`: it is never persisted as a single value. It
/// exists only so a mutation can read the commitment and ceiling coherently before writing.
#[derive(Clone, Debug, PartialEq)]
pub struct CollateralState {
    /// Currently recorded commitment, if any.
    pub commitment: Option<SmeCollateralCommitment>,
    /// Configured ceiling; defaults to [`MAX_INVOICE_AMOUNT`] when unset.
    pub limit: i128,
}

/// Result of a commitment record attempt.
///
/// Lets the caller distinguish a freshly applied write (which should emit
/// `CollateralRecordedEvt`) from an idempotent retry (which must not re-emit).
#[derive(Clone, Debug, PartialEq)]
pub enum RecordOutcome {
    /// A new commitment was written. `prior_amount` is the amount it replaced, or `0`.
    Applied {
        prior_amount: i128,
        commitment: SmeCollateralCommitment,
    },
    /// The stored `(asset, amount)` already matched, so nothing changed and no event
    /// should be emitted. `commitment` is the existing record, with its original
    /// `recorded_at` preserved.
    Unchanged { commitment: SmeCollateralCommitment },
}

/// RAII guard for [`CollateralStorageKey::MutationLock`].
///
/// The lock is cleared on `Drop`, so every normal early return releases it. A panic
/// unwinds the whole invocation and Soroban rolls the storage write back anyway, so a
/// transaction can never strand the lock.
struct MutationGuard<'a> {
    env: &'a Env,
}

impl Drop for MutationGuard<'_> {
    fn drop(&mut self) {
        self.env
            .storage()
            .instance()
            .remove(&CollateralStorageKey::MutationLock);
    }
}

/// Acquire the exclusive collateral mutation lock, or fail if another mutation is
/// already in flight.
///
/// This is the single gate that serialises collateral writes. It is intentionally
/// conservative: a re-entrant call fails fast with a typed, diagnosable error rather than
/// racing the outer mutation.
fn acquire_mutation_lock(env: &Env) -> Result<MutationGuard<'_>, EscrowError> {
    let held: bool = env
        .storage()
        .instance()
        .get(&CollateralStorageKey::MutationLock)
        .unwrap_or(false);
    if held {
        return Err(EscrowError::ConcurrentMutation);
    }
    env.storage()
        .instance()
        .set(&CollateralStorageKey::MutationLock, &true);
    Ok(MutationGuard { env })
}

/// Read the commitment cell. Never writes. Absent ⇒ `None`.
pub(crate) fn commitment(env: &Env) -> Option<SmeCollateralCommitment> {
    env.storage().instance().get(&DataKey::SmeCollateralPledge)
}

/// Read the configured collateral ceiling. Never writes.
///
/// Absent ⇒ [`MAX_INVOICE_AMOUNT`] (additive key, ADR-007).
pub(crate) fn limit(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::CollateralLimit)
        .unwrap_or(MAX_INVOICE_AMOUNT)
}

/// Read both collateral cells into an in-memory state. Never writes.
fn read_state(env: &Env) -> CollateralState {
    CollateralState {
        commitment: commitment(env),
        limit: limit(env),
    }
}

/// Collateral-subsystem schema version.
///
/// Pure, single-key read of the contract-wide [`DataKey::Version`]. The collateral
/// lifecycle never writes this key, so the value is stable across every collateral
/// mutation and identical to [`crate::LiquifactEscrow::get_version`].
pub(crate) fn version(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::Version).unwrap_or(0)
}

/// Validate an asset symbol and amount against the per-item rules.
fn validate(env: &Env, asset: &Symbol, amount: i128) -> Result<(), EscrowError> {
    if amount <= 0 {
        return Err(EscrowError::CollateralAmountNotPositive);
    }
    if asset == &Symbol::new(env, "") {
        return Err(EscrowError::CollateralAssetEmpty);
    }
    Ok(())
}

/// Record or replace the SME collateral commitment under the mutation lock.
///
/// Authorisation is **not** performed here; the caller must have already required the
/// SME's signature. This split lets the storage invariants be unit-tested without a
/// contract invocation frame.
///
/// # Guarantees
/// * Validates `amount > 0` and a non-empty `asset` before any mutation.
/// * Rejects `amount > limit` with [`EscrowError::CollateralLimitExceeded`].
/// * Rejects a replacement timestamped before the stored record with
///   [`EscrowError::CollateralTimestampBackwards`].
/// * Re-submitting the same `(asset, amount)` is an idempotent no-op
///   ([`RecordOutcome::Unchanged`]); the stored `recorded_at` is preserved.
/// * Runs the whole read-modify-write under the mutation lock.
pub(crate) fn apply_record(
    env: &Env,
    asset: &Symbol,
    amount: i128,
) -> Result<RecordOutcome, EscrowError> {
    validate(env, asset, amount)?;

    // Serialise the read-modify-write against re-entrant / overlapping calls.
    let _guard = acquire_mutation_lock(env)?;

    let state = read_state(env);

    if amount > state.limit {
        return Err(EscrowError::CollateralLimitExceeded);
    }

    let now = env.ledger().timestamp();

    if let Some(ref existing) = state.commitment {
        if now < existing.recorded_at {
            return Err(EscrowError::CollateralTimestampBackwards);
        }
        // Idempotent retry: the stored record already matches what this call would write,
        // so preserve its original `recorded_at` and report `Unchanged`.
        if existing.asset == asset.clone() && existing.amount == amount {
            return Ok(RecordOutcome::Unchanged {
                commitment: existing.clone(),
            });
        }
    }

    let prior_amount = state.commitment.as_ref().map(|c| c.amount).unwrap_or(0);
    let commitment = SmeCollateralCommitment {
        asset: asset.clone(),
        amount,
        recorded_at: now,
    };
    env.storage()
        .instance()
        .set(&DataKey::SmeCollateralPledge, &commitment);

    Ok(RecordOutcome::Applied {
        prior_amount,
        commitment,
    })
}

/// Remove the SME collateral commitment under the mutation lock.
///
/// Authorisation is **not** performed here; the caller must have already required the
/// SME's signature.
///
/// Returns the removed commitment so the caller can emit a retirement event, or
/// [`EscrowError::NoCollateralToClear`] when nothing is stored. The lock-protected read
/// means a racing second clear cannot double-remove.
pub(crate) fn apply_clear(env: &Env) -> Result<SmeCollateralCommitment, EscrowError> {
    let _guard = acquire_mutation_lock(env)?;
    match commitment(env) {
        Some(existing) => {
            env.storage()
                .instance()
                .remove(&DataKey::SmeCollateralPledge);
            Ok(existing)
        }
        None => Err(EscrowError::NoCollateralToClear),
    }
}

/// Set the collateral ceiling under the mutation lock.
///
/// Authorisation is **not** performed here; the caller must have already required the
/// admin's signature.
///
/// # Errors
/// * [`EscrowError::CollateralLimitNotPositive`] when `new_limit <= 0`.
/// * [`EscrowError::CollateralLimitExceedsMax`] when `new_limit > MAX_INVOICE_AMOUNT`.
///
/// Validation runs before the lock is taken, so an invalid value can never strand it.
pub(crate) fn apply_set_limit(env: &Env, new_limit: i128) -> Result<(), EscrowError> {
    if new_limit <= 0 {
        return Err(EscrowError::CollateralLimitNotPositive);
    }
    if new_limit > MAX_INVOICE_AMOUNT {
        return Err(EscrowError::CollateralLimitExceedsMax);
    }

    let _guard = acquire_mutation_lock(env)?;
    env.storage()
        .instance()
        .set(&DataKey::CollateralLimit, &new_limit);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Ledger as _;
    use soroban_sdk::{contract, contractimpl, Env, Symbol};

    /// Minimal registered contract so the tests get a valid contract frame for
    /// instance-storage access (`env.as_contract`).
    #[contract]
    pub struct Harness;

    #[contractimpl]
    impl Harness {
        pub fn noop() {}
    }

    /// Run `f` inside a fresh environment **and** a registered contract frame, with the
    /// ledger timestamp set to `ts`.
    fn with_harness<R>(ts: u64, f: impl FnOnce(&Env) -> R) -> R {
        let env = Env::default();
        env.ledger().with_mut(|l| l.timestamp = ts);
        let id = env.register(Harness, ());
        env.as_contract(&id, || f(&env))
    }

    fn set_time(env: &Env, ts: u64) {
        env.ledger().with_mut(|l| l.timestamp = ts);
    }

    fn asset(env: &Env, s: &str) -> Symbol {
        Symbol::new(env, s)
    }

    fn lock_held(env: &Env) -> bool {
        env.storage()
            .instance()
            .get::<CollateralStorageKey, bool>(&CollateralStorageKey::MutationLock)
            .unwrap_or(false)
    }

    // --- success -----------------------------------------------------------

    #[test]
    fn record_stores_commitment_and_releases_lock() {
        with_harness(1_000, |env| {
            let a = asset(env, "GOLD");
            let outcome = apply_record(env, &a, 500).unwrap();
            assert_eq!(
                outcome,
                RecordOutcome::Applied {
                    prior_amount: 0,
                    commitment: SmeCollateralCommitment {
                        asset: a.clone(),
                        amount: 500,
                        recorded_at: 1_000,
                    },
                }
            );
            assert_eq!(commitment(env).unwrap().amount, 500);
            // Guard must not leak across mutations.
            assert!(!lock_held(env));
        });
    }

    #[test]
    fn clear_removes_and_returns_prior() {
        with_harness(1_000, |env| {
            apply_record(env, &asset(env, "GOLD"), 500).unwrap();
            let cleared = apply_clear(env).unwrap();
            assert_eq!(cleared.asset, asset(env, "GOLD"));
            assert_eq!(cleared.amount, 500);
            assert!(commitment(env).is_none());
            assert!(!lock_held(env));
        });
    }

    #[test]
    fn set_limit_updates_and_releases_lock() {
        with_harness(1_000, |env| {
            apply_set_limit(env, 123_456).unwrap();
            assert_eq!(limit(env), 123_456);
            assert!(!lock_held(env));
        });
    }

    // --- rejection ---------------------------------------------------------

    #[test]
    fn record_rejects_non_positive_and_empty_asset() {
        with_harness(1_000, |env| {
            assert_eq!(
                apply_record(env, &asset(env, "GOLD"), 0),
                Err(EscrowError::CollateralAmountNotPositive)
            );
            assert_eq!(
                apply_record(env, &asset(env, ""), 10),
                Err(EscrowError::CollateralAssetEmpty)
            );
            // Failed validation must not leave the lock held.
            assert!(!lock_held(env));
        });
    }

    #[test]
    fn record_rejects_backwards_timestamp() {
        with_harness(1_000, |env| {
            apply_record(env, &asset(env, "GOLD"), 500).unwrap();
            set_time(env, 999);
            assert_eq!(
                apply_record(env, &asset(env, "SILVER"), 600),
                Err(EscrowError::CollateralTimestampBackwards)
            );
            assert!(!lock_held(env));
        });
    }

    #[test]
    fn clear_without_commitment_is_rejected() {
        with_harness(1_000, |env| {
            assert_eq!(apply_clear(env), Err(EscrowError::NoCollateralToClear));
            assert!(!lock_held(env));
        });
    }

    #[test]
    fn set_limit_rejects_non_positive_and_over_max() {
        with_harness(1_000, |env| {
            assert_eq!(
                apply_set_limit(env, 0),
                Err(EscrowError::CollateralLimitNotPositive)
            );
            assert_eq!(
                apply_set_limit(env, MAX_INVOICE_AMOUNT + 1),
                Err(EscrowError::CollateralLimitExceedsMax)
            );
            assert!(!lock_held(env));
        });
    }

    // --- idempotent retries / duplicates -----------------------------------

    #[test]
    fn re_recording_identical_commitment_is_idempotent() {
        with_harness(1_000, |env| {
            let a = asset(env, "GOLD");
            let first = apply_record(env, &a, 500).unwrap();
            assert!(matches!(first, RecordOutcome::Applied { .. }));

            // At-least-once retry at a later ledger time must converge to the same
            // state, not overwrite `recorded_at` or emit a second effect.
            set_time(env, 2_000);
            let retry = apply_record(env, &a, 500).unwrap();
            assert_eq!(
                retry,
                RecordOutcome::Unchanged {
                    commitment: SmeCollateralCommitment {
                        asset: a.clone(),
                        amount: 500,
                        recorded_at: 1_000,
                    },
                }
            );
            assert_eq!(commitment(env).unwrap().recorded_at, 1_000);
            assert!(!lock_held(env));
        });
    }

    // --- racing / re-entrant execution -------------------------------------

    #[test]
    fn reentrant_mutation_is_rejected_without_side_effects() {
        with_harness(1_000, |env| {
            // Simulate a mutation already in flight by holding the lock.
            env.storage()
                .instance()
                .set(&CollateralStorageKey::MutationLock, &true);

            assert_eq!(
                apply_record(env, &asset(env, "GOLD"), 500),
                Err(EscrowError::ConcurrentMutation)
            );
            assert_eq!(apply_clear(env), Err(EscrowError::ConcurrentMutation));
            assert_eq!(
                apply_set_limit(env, 10),
                Err(EscrowError::ConcurrentMutation)
            );
            // The re-entrant attempts changed nothing.
            assert!(commitment(env).is_none());
            assert_eq!(limit(env), MAX_INVOICE_AMOUNT);
        });
    }

    #[test]
    fn concurrent_distinct_records_serialise_to_last_writer() {
        with_harness(1_000, |env| {
            // First mutation completes and releases the lock, then a second distinct
            // record applies cleanly and replaces the commitment.
            apply_record(env, &asset(env, "GOLD"), 500).unwrap();
            let second = apply_record(env, &asset(env, "SILVER"), 700).unwrap();
            assert_eq!(
                second,
                RecordOutcome::Applied {
                    prior_amount: 500,
                    commitment: SmeCollateralCommitment {
                        asset: asset(env, "SILVER"),
                        amount: 700,
                        recorded_at: 1_000,
                    },
                }
            );
            assert_eq!(commitment(env).unwrap().asset, asset(env, "SILVER"));
            assert!(!lock_held(env));
        });
    }

    // --- timing boundaries --------------------------------------------------

    #[test]
    fn replacement_at_exact_same_timestamp_succeeds() {
        with_harness(1_000, |env| {
            apply_record(env, &asset(env, "GOLD"), 500).unwrap();
            // `now >= prior.recorded_at` is inclusive.
            let out = apply_record(env, &asset(env, "SILVER"), 700).unwrap();
            assert!(matches!(out, RecordOutcome::Applied { .. }));
            assert_eq!(commitment(env).unwrap().asset, asset(env, "SILVER"));
        });
    }

    #[test]
    fn amount_exactly_at_limit_is_accepted() {
        with_harness(1_000, |env| {
            apply_set_limit(env, 1_000).unwrap();
            assert!(apply_record(env, &asset(env, "GOLD"), 1_000).is_ok());
        });
    }

    #[test]
    fn amount_above_limit_is_rejected() {
        with_harness(1_000, |env| {
            apply_set_limit(env, 1_000).unwrap();
            assert_eq!(
                apply_record(env, &asset(env, "GOLD"), 1_001),
                Err(EscrowError::CollateralLimitExceeded)
            );
            assert!(commitment(env).is_none());
        });
    }

    // --- pure views ---------------------------------------------------------

    #[test]
    fn version_is_absent_before_init_and_never_changed_by_collateral() {
        with_harness(1_000, |env| {
            assert_eq!(version(env), 0);

            apply_set_limit(env, 1_000).unwrap();
            apply_record(env, &asset(env, "GOLD"), 500).unwrap();
            apply_clear(env).unwrap();

            // The collateral lifecycle never writes `DataKey::Version`.
            assert_eq!(version(env), 0);
        });
    }

    #[test]
    fn version_reflects_the_stored_schema_version() {
        with_harness(1_000, |env| {
            env.storage()
                .instance()
                .set(&DataKey::Version, &crate::SCHEMA_VERSION);
            assert_eq!(version(env), crate::SCHEMA_VERSION);
        });
    }
}
