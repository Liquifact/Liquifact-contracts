// No `#![allow(dead_code)]` — every constructor is the required call-site path.
// Suppressing that lint was masking gaps where callers built `DataKey` inline instead
// of routing through here. Callers must use these functions; direct `DataKey::*` literals
// in entrypoints are a code-smell and will produce dead-code warnings for the helper.
//
//! Centralized constructors for all storage keys used by the LiquiFact escrow contract.
//!
//! # Purpose
//!
//! All persistent and instance-storage keys are defined here as variants of [`DataKey`].
//! Typed constructor functions are provided for every key family so that call sites never
//! build a [`DataKey`] inline — reducing the risk of typos, discriminant drift between
//! modules, and copy-paste errors when a new key needs to be added.
//!
//! ## Invariants enforced here
//!
//! 1. **Address liveness** — per-investor key constructors call [`Address::require_auth`] is
//!    *not* the right guard here (that belongs in entrypoints). Instead, the constructors are
//!    the single place to enforce that an `Address` argument is structurally non-trivial.
//!    In Soroban, `Address` is always a 32-byte value; there is no null/zero sentinel. The
//!    constructors therefore document the caller's obligation in doc-comments and do not add
//!    a runtime check that would change cost semantics. Callers that receive `investor: Address`
//!    from a signed transaction already have the network-layer guarantee.
//!
//! 2. **Nonce overflow** — `callback_context` receives a `u64` nonce that must never wrap.
//!    The nonce is incremented with `checked_add` in `register_callback`; once it saturates
//!    the host panics before calling this constructor. The constructor adds a debug-mode
//!    assertion so tests catch any path that bypasses the checked increment.
//!
//! 3. **Additive-key policy (ADR-007)** — Adding a new variant is **backward-compatible**
//!    when the new key is read with `.unwrap_or(default)` and its absence does not change
//!    existing entrypoint semantics. Renaming a variant, changing its XDR discriminant, or
//!    altering the stored type of an existing key is **breaking** and requires a `migrate`
//!    path or a full redeploy.
//!
//! ## Storage tier annotations
//!
//! Each constructor's doc-comment marks the storage tier it targets so reviewers can audit
//! TTL/archival handling without cross-referencing every call site:
//!
//! - `[INSTANCE]` — `env.storage().instance()` — evicts when the contract itself is
//!   evicted; suitable for per-escrow configuration that is always needed together.
//! - `[PERSISTENT]` — `env.storage().persistent()` — per-entry TTL; must be bumped
//!   alongside each investor interaction to prevent archival of active participants.
//!
//! ## Collateral keys
//!
//! The collateral pledge key family is managed by [`collateral_pledge_key`]. All three
//! collateral entrypoints (`record_sme_collateral_commitment`, `clear_sme_collateral_commitment`,
//! `get_sme_collateral_commitment`) call this function instead of constructing
//! `DataKey::SmeCollateralPledge` inline. This ensures any future rename or split of the
//! collateral key cannot diverge across call sites.

use crate::DataKey;
use soroban_sdk::Address;

// ---------------------------------------------------------------------------
// Invariant helpers
// ---------------------------------------------------------------------------

/// Asserts that `nonce` is not the sentinel value `0`.
///
/// Nonce `0` is the "never registered" default returned by `get_callback_nonce` on a fresh
/// escrow. Storing a `CallbackContext` under nonce `0` would make `is_callback_consumed(0)`
/// return `true` for an unconsumed or non-existent entry — a silent correctness bug.
///
/// This is a `debug_assert` rather than a `panic_with_error` because:
/// - The nonce is produced exclusively by `register_callback` via `checked_add(1, ...)`.
///   Any path reaching `0` is a programming error, not a user-facing input error.
/// - In production WASM builds, `debug_assert` compiles to nothing, keeping the happy-path
///   cost identical to the pre-invariant code.
#[inline(always)]
fn assert_nonce_nonzero(nonce: u64) {
    debug_assert!(
        nonce != 0,
        "callback_context: nonce 0 is reserved as the unregistered sentinel; \
         register_callback must use checked_add starting from 0, \
         so the first valid nonce is 1"
    );
}

// ---------------------------------------------------------------------------
// Per-investor persistent keys  [PERSISTENT]
// ---------------------------------------------------------------------------
//
// These four keys form a logical group: they are always written together on the
// first deposit leg and read together during claim/refund/settle. They share the
// same `Address` discriminant so that their TTL bumps can be batched in a single
// `extend_ttl_for_activity` call (see `lib.rs`).
//
// Invariant: the `investor` address must be the same across all four calls for a
// given participant. Mismatched keys silently orphan data — the constructors accept
// any `Address` but callers must pass the same value they received from the signed
// transaction.

/// `[PERSISTENT]` Per-investor cumulative principal recorded by `fund` / `fund_with_commitment`
/// / `fund_batch`.
///
/// **Default on read:** `0` (no contribution). **Write rule:** written atomically with
/// [`DataKey::InvestorEffectiveYield`] and [`DataKey::InvestorClaimNotBefore`] on the first
/// deposit; updated-in-place on follow-on deposits.
///
/// **Invariant:** value is always `>= 0`. Zero is the "no contribution" sentinel; a stored
/// zero is semantically equivalent to absent (but is written during refund for idempotency).
pub(crate) fn investor_contribution(investor: Address) -> DataKey {
    DataKey::InvestorContribution(investor)
}

/// `[PERSISTENT]` Per-investor effective yield in basis points selected on the investor's
/// **first** deposit.
///
/// **Default on read:** absent → falls back to `InvoiceEscrow::yield_bps`. **Write rule:**
/// written once on the first deposit; never mutated thereafter.
///
/// **Invariant:** once written, the value is immutable for the lifetime of the escrow.
/// Changing it after `status >= 1` (funded) would silently alter the investor's payout.
pub(crate) fn investor_effective_yield(investor: Address) -> DataKey {
    DataKey::InvestorEffectiveYield(investor)
}

/// `[PERSISTENT]` Per-investor minimum ledger timestamp before `claim_investor_payout`
/// (`0` = no extra claim gate).
///
/// **Default on read:** `0`. **Write rule:** written on the first deposit alongside
/// [`investor_effective_yield`]; `0` for plain `fund`, `now + lock_secs` for
/// `fund_with_commitment`.
///
/// **Invariant:** `claim_not_before <= maturity` is validated at deposit time by
/// `EscrowError::CommitmentLockExceedsMaturity`. A value of `0` always allows an immediate
/// claim (subject only to settlement status).
pub(crate) fn investor_claim_not_before(investor: Address) -> DataKey {
    DataKey::InvestorClaimNotBefore(investor)
}

/// `[PERSISTENT]` Write-once claimed-payout marker; set to `true` when an investor exercises
/// their payout after settlement.
///
/// **Default on read:** `false`. **Write rule:** written exactly once in `claim_investor_payout`
/// **before** the SEP-41 transfer (checks-effects-interactions). A second read of `true` causes
/// the entrypoint to return early without re-emitting or re-transferring.
///
/// **Invariant:** once set to `true`, this value must never be reset to `false`. Any code path
/// that clears this marker reintroduces the double-claim vulnerability.
pub(crate) fn investor_claimed(investor: Address) -> DataKey {
    DataKey::InvestorClaimed(investor)
}

// ---------------------------------------------------------------------------
// Instance-storage configuration keys  [INSTANCE]
// ---------------------------------------------------------------------------

/// `[INSTANCE]` Minimum per-call contribution floor in token base units (`0` = no floor).
///
/// **Default on read:** `0` (no floor enforced). Always written — even the `0` default — at
/// `init` so reads never branch on absence.
pub(crate) fn min_contribution_floor() -> DataKey {
    DataKey::MinContributionFloor
}

/// `[INSTANCE]` Optional cap on the number of distinct investor addresses that may contribute.
///
/// **Default on read:** absent → unlimited. Checked against [`unique_funder_count`] on each
/// new investor.
pub(crate) fn max_unique_investors_cap() -> DataKey {
    DataKey::MaxUniqueInvestorsCap
}

/// `[INSTANCE]` Optional cap on total principal for a single investor address.
///
/// **Default on read:** absent → unlimited. Checked against `InvestorContribution` on every
/// deposit call.
pub(crate) fn max_per_investor_cap() -> DataKey {
    DataKey::MaxPerInvestorCap
}

/// `[INSTANCE]` Monotonically increasing count of distinct investor addresses.
///
/// **Default on read:** `0`. Written as `0` at `init`; incremented by `1` (never decremented)
/// each time a **new** investor makes their first deposit.
///
/// **Invariant:** `unique_funder_count <= max_unique_investors_cap` (when the cap is set).
/// `unique_funder_count <= MAX_UNIQUE_INVESTORS` unconditionally (issue #1229).
pub(crate) fn unique_funder_count() -> DataKey {
    DataKey::UniqueFunderCount
}

/// `[INSTANCE]` Ordered list of all investor addresses used by paginated views.
///
/// **Default on read:** absent → empty list. Append-only: addresses are pushed when an
/// investor first contributes and never removed, even on refund.
///
/// **Invariant:** every address in the list has a corresponding [`investor_contribution`] entry
/// (which may be `0` after a refund). Length equals [`unique_funder_count`].
pub(crate) fn investor_index() -> DataKey {
    DataKey::InvestorIndex
}

/// `[INSTANCE]` Optional funding deadline as a ledger timestamp (absent = no deadline).
///
/// **Default on read:** absent → no deadline enforced. When present, `fund` rejects calls
/// where `env.ledger().timestamp() > deadline`.
pub(crate) fn funding_deadline() -> DataKey {
    DataKey::FundingDeadline
}

/// `[INSTANCE]` Write-once pro-rata snapshot captured when escrow status first becomes funded.
///
/// **Default on read:** absent → escrow not yet funded. Written once; the write is guarded by
/// `!env.storage().instance().has(&keys::funding_close_snapshot())` so it cannot be
/// overwritten even if `fund_impl` is re-entered.
///
/// **Invariant:** once written, `total_principal`, `target`, `ledger_timestamp`, and
/// `ledger_sequence` are immutable for the lifetime of the escrow.
pub(crate) fn funding_close_snapshot() -> DataKey {
    DataKey::FundingCloseSnapshot
}

/// `[INSTANCE]` Immutable SEP-41 funding token address, set exactly once at `init`.
///
/// **Invariant:** written before any investor interaction; cannot be changed after `init`
/// completes. All token transfers in this contract address the value stored here.
pub(crate) fn funding_token() -> DataKey {
    DataKey::FundingToken
}

/// `[INSTANCE]` Immutable decimal scale (number of significant decimal places) of the
/// SEP-41 funding token, set once at `init` when `token_decimals` is supplied.
///
/// **Default on read:** absent → scale validation is skipped for backward compatibility
/// (additive key, ADR-007). When present, `fund` rejects amounts that are not representable
/// exactly at the token's precision.
///
/// **Invariant:** once written, the value is immutable. Changing the decimal scale after
/// investors have contributed would make existing balances uninterpretable.
pub(crate) fn funding_token_scale() -> DataKey {
    DataKey::FundingTokenScale
}

// ---------------------------------------------------------------------------
// Callback keys  [INSTANCE]
// ---------------------------------------------------------------------------

/// `[INSTANCE]` Monotonically increasing invocation nonce counter for cross-contract callbacks.
///
/// **Default on read:** `0`. Incremented with `checked_add` before each `register_callback`
/// call; the first live nonce is therefore `1`. The value `0` must never be stored under a
/// [`callback_context`] key (see [`assert_nonce_nonzero`]).
///
/// **Invariant:** strictly increasing; never decremented, never reset.
pub(crate) fn callback_nonce() -> DataKey {
    DataKey::CallbackNonce
}

/// `[INSTANCE]` Pending callback context keyed by invocation nonce.
///
/// **Precondition:** `nonce` must be `>= 1`. Nonce `0` is the unregistered sentinel value
/// returned by [`callback_nonce`] for a fresh escrow. Storing a context under nonce `0` would
/// make `is_callback_consumed(0)` return `true` for a non-existent entry, corrupting the
/// consumed/pending distinction.
///
/// **Invariant:** the context stored here binds exactly one `(origin, nonce, phase)` triple;
/// it is set to `consumed = true` before returning from `execute_callback` and never reset.
///
/// # Panics (debug builds only)
///
/// Panics in debug builds if `nonce == 0`.
pub(crate) fn callback_context(nonce: u64) -> DataKey {
    assert_nonce_nonzero(nonce);
    DataKey::CallbackContext(nonce)
}

// ---------------------------------------------------------------------------
// Running-total and release-tracking keys  [INSTANCE]
// ---------------------------------------------------------------------------

/// `[INSTANCE]` Running total of principal released to the SME via `release`.
///
/// **Default on read:** `0` (nothing released). Incremented atomically with each successful
/// `release` transfer; never decremented.
///
/// **Invariant:** `released_amount <= funded_amount`. Any increment that would push
/// `released_amount` above `funded_amount` must be rejected before the storage write.
pub(crate) fn released_amount() -> DataKey {
    DataKey::ReleasedAmount
}

// ---------------------------------------------------------------------------
// Collateral key
// ---------------------------------------------------------------------------

/// `[INSTANCE]` Optional SME collateral commitment metadata (record-only — not an on-chain
/// asset lock).
///
/// Absent when no commitment has been recorded; replaceable by the SME via
/// `record_sme_collateral_commitment`.
///
/// **Invariant (documentation only):** the value stored here represents *self-reported*
/// metadata from the SME. It does not create a lien, freeze a balance, move tokens, or
/// constitute an enforceable on-chain claim. Risk teams and indexers must verify supporting
/// evidence outside this contract.
pub(crate) fn collateral_pledge_key() -> DataKey {
    DataKey::SmeCollateralPledge
}
