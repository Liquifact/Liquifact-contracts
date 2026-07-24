# Escrow Read API

Complete catalog of all public read-only views on `LiquifactEscrow`. All functions are pure reads:
no state mutation, no authorization required unless specified otherwise.

**Integrator note:** Return types, defaults, and absent-key behavior documented for each view match
the on-chain implementation exactly. Off-chain tooling should use these views rather than
re-implementing storage reads to guarantee identical semantics.

---

## Index

**Core Escrow State:**
- [get_escrow](#get_escrow--invoiceescrow)
- [get_version](#get_version--u32)
- [get_escrow_summary](#get_escrow_summary--escrowsummary)

**Immutable Bindings:**
- [get_funding_token](#get_funding_token--address)
- [get_treasury](#get_treasury--address)
- [get_registry_ref](#get_registry_ref--optionaddress)

**Admin & Governance:**
- [get_pending_admin](#get_pending_admin--optionaddress)
- [get_pending_admin_expiry](#get_pending_admin_expiry--optionu64)
- [get_pending_admin_remaining_secs](#get_pending_admin_remaining_secs--optionu64)
- [get_legal_hold](#get_legal_hold--bool)
- [get_legal_hold_clear_delay](#get_legal_hold_clear_delay--u64)
- [get_legal_hold_clearable_at](#get_legal_hold_clearable_at--optionu64)

**Funding Constraints:**
- [get_funding_deadline](#get_funding_deadline--optionu64)
- [is_funding_expired](#is_funding_expired--bool)
- [get_min_contribution_floor](#get_min_contribution_floor--i128)
- [get_max_unique_investors_cap](#get_max_unique_investors_cap--optionu32)
- [get_remaining_investor_slots](#get_remaining_investor_slots--optionu32)
- [get_max_per_investor_cap](#get_max_per_investor_cap--optioni128)

**Maturity & Settlement:**
- [has_maturity_lock](#has_maturity_lock--bool)
- [get_funding_close_snapshot](#get_funding_close_snapshot--optionfundingclosesnapshot)

**Tier Lookup:**
- [preview_yield_tier](#preview_yield_tieramount-i128-lock-u64--i64-u64)

**Deposit Preview:**
- [preview_fund](#preview_fundenvinvestor-address-amount-i128--u32)

**Per-Investor State:**
- [get_contribution](#get_contributioninvestor-address--i128)
- [get_unique_funder_count](#get_unique_funder_count--u32)
- [get_investor_yield_bps](#get_investor_yield_bpsinvestor-address--i64)
- [get_investor_claim_not_before](#get_investor_claim_not_beforeinvestor-address--u64)
- [is_investor_claimed](#is_investor_claimedinvestor-address--bool)
- [is_investor_refunded](#is_investor_refundedinvestor-address--bool)
- [compute_investor_payout](#compute_investor_payoutinvestor-address--i128)
- [get_claimable_payout](#get_claimable_payoutinvestor-address--i128)
- [get_settlement_pool](#get_settlement_pool--i128)

**Attestations:**
- [get_primary_attestation_hash](#get_primary_attestation_hash--optionbytesn32)
- [get_attestation_append_log](#get_attestation_append_log--vecbytesn32)
- [get_attestation_log_stats](#get_attestation_log_stats--u32-u32)
- [is_attestation_revoked](#is_attestation_revokedindex-u32--bool)

**Collateral Metadata:**
- [get_sme_collateral_commitment](#get_sme_collateral_commitment--optionsmecollateralcommitment)

**Allowlist:**
- [is_allowlist_active](#is_allowlist_active--bool)
- [is_investor_allowlisted](#is_investor_allowlistedinvestor-address--bool)

**Distributed Principal:**
- [get_distributed_principal](#get_distributed_principal--i128)
- [get_reconciliation](#get_reconciliation--reconciliationview)

**Paginated Enumeration:**
- [Paginated Enumeration](#paginated-enumeration)

---

## Core Escrow State

### `get_escrow() → InvoiceEscrow`

**Storage key:** `DataKey::Escrow`  
**Signature:** `pub fn get_escrow(env: Env) -> InvoiceEscrow`

Returns the full escrow snapshot containing all core state fields.

**Requires initialization:** Yes — emits [`EscrowError::EscrowNotInitialized`] (code 20) if called before `init`.

**Return value:**
- `InvoiceEscrow` struct with fields: `invoice_id`, `admin`, `sme_address`, `amount`, `funding_target`, `funded_amount`, `yield_bps`, `maturity`, `status`.

---

### `get_version() → u32`

**Storage key:** `DataKey::Version`  
**Signature:** `pub fn get_version(env: Env) -> u32`

Returns the stored schema version written by `init` (see `SCHEMA_VERSION`).

**Requires initialization:** No  
**Default when absent:** `0`

**Return value:**
- `u32` schema version (current production: `6`).
- Returns `0` if called before `init`.

---

### `get_escrow_summary() → EscrowSummary`

**Signature:** `pub fn get_escrow_summary(env: Env) -> EscrowSummary`

Bundles multiple read-only values in a single host invocation, optimizing read latency and gas efficiency for off-chain indexers and frontend rendering.

**Requires initialization:** Yes — panics via `get_escrow` if escrow is not initialized.

**Return value:** `EscrowSummary` struct containing:
- `escrow: InvoiceEscrow` — Full escrow snapshot.
- `has_maturity_lock: bool` — True when `escrow.maturity > 0`.
- `legal_hold: bool` — True if compliance hold is active.
- `funding_close_snapshot: EscrowCloseSnapshot` — Custom option-like enum (`None` or `Some(FundingCloseSnapshot)`).
- `unique_funder_count: u32` — Distinct address count.
- `is_allowlist_active: bool` — Allowlist gate status.
- `schema_version: u32` — Contract schema version.
- `sme_collateral_commitment: CollateralCommitmentSnapshot` — Custom option-like enum (`None` or `Some(SmeCollateralCommitment)`).
- `has_primary_attestation: bool` — Primary attestation binding status.
- `attestation_log_length: u32` — Number of append-log entries.

---

## Immutable Bindings

### `get_funding_token() → Address`

**Storage key:** `DataKey::FundingToken`  
**Signature:** `pub fn get_funding_token(env: Env) -> Address`

Returns the SEP-41 token contract address bound to this escrow instance at `init`.

**Immutable:** Set once at `init`; cannot change after deploy.  
**Requires initialization:** Yes — emits [`EscrowError::FundingTokenNotSet`] (code 21) if called before `init`.

**Return value:**
- `Address` of the funding token contract.
- This is the only token that `sweep_terminal_dust` may transfer to the treasury.

---

### `get_treasury() → Address`

**Storage key:** `DataKey::Treasury`  
**Signature:** `pub fn get_treasury(env: Env) -> Address`

Returns the protocol treasury address that receives terminal dust sweeps.

**Immutable:** Set once at `init`; cannot change after deploy.  
**Requires initialization:** Yes — emits [`EscrowError::TreasuryNotSet`] (code 22) if called before `init`.

**Return value:**
- `Address` of the treasury.
- The treasury must authorize `sweep_terminal_dust`; the admin cannot sweep unless it is also the treasury.

---

### `get_registry_ref() → Option<Address>`

**Storage key:** `DataKey::RegistryRef`  
**Signature:** `pub fn get_registry_ref(env: Env) -> Option<Address>`

Returns the optional registry contract address supplied at `init`, or `None` when absent.

**Immutable:** Set once at `init`; cannot change after deploy.  
**Requires initialization:** No  
**Default when absent:** `None`

**Non-authority model:**
- `RegistryRef` is a **read-only discoverability hint** for off-chain indexers only.
- No on-chain logic in this contract reads or calls this address.
- Its presence **does not** prove registry membership; call the registry contract directly to verify.
- The key is omitted from instance storage entirely when `registry = None` at `init`.

**Return value:**
- `Some(Address)` when a registry was configured.
- `None` otherwise.

---

## Admin & Governance

### `get_pending_admin() → Option<Address>`

**Storage key:** `DataKey::PendingAdmin`  
**Signature:** `pub fn get_pending_admin(env: Env) -> Option<Address>`

Returns the proposed successor admin waiting for `accept_admin`, or `None` when no handover is in progress.

**Requires initialization:** No  
**Default when absent:** `None`

**Return value:**
- `Some(Address)` when a handover is pending.
- `None` when no `propose_admin` has been issued, or after a successful `accept_admin`.

### `get_pending_admin_expiry() → Option<u64>`

**Storage key:** `DataKey::PendingAdminExpiry`

**Signature:** `pub fn get_pending_admin_expiry(env: Env) -> Option<u64>`

Returns the absolute ledger timestamp recorded by `propose_admin`, or `None` when no expiry has been recorded.

**Requires initialization:** No

**Default when absent:** `None`

**Return value:**
- `Some(timestamp)` when a handover proposal with an expiry exists.
- `None` before `propose_admin`, after `accept_admin`, after `cancel_pending_admin`, or when no expiry key is present.

### `get_pending_admin_remaining_secs() → Option<u64>`

**Storage keys:** `DataKey::PendingAdmin`, `DataKey::PendingAdminExpiry`

**Signature:** `pub fn get_pending_admin_remaining_secs(env: Env) -> Option<u64>`

Returns the pending-admin proposal's remaining validity window computed against `Env::ledger().timestamp()`.

**Requires initialization:** No

**Default when absent:** `None`

**Return value:**
- `None` when no pending admin proposal is active.
- `Some(expiry - now)` while `now < expiry`.
- `Some(0)` when `now >= expiry`, using saturating arithmetic.

**Boundary parity with `accept_admin`:**
- At `now == expiry`, this view returns `Some(0)` and `accept_admin` still accepts the proposal.
- At `now > expiry`, this view still returns `Some(0)` and `accept_admin` rejects with `AdminProposalExpired`.
- Pure read: no authorization, no storage writes, no TTL bump.

---

## `get_remaining_funding_capacity() → i128`

**Storage key:** `DataKey::Escrow`

Returns the remaining funding capacity before the funding target is reached.

- **Calculation**: `funding_target.saturating_sub(funded_amount)` clamped at `0` (via `.max(0)`) so it never goes negative when over-funded.
- **Informational only**: This view is for frontend guidance. The `fund` method may still accept deposits that over-fund past the target while the escrow status is `0` (Open).
- **No authorization**: Pure read; no auth or signature required.
- **Complexity**:
  - Time Complexity: $O(1)$ read from storage.
  - Space Complexity: $O(1)$ in-memory calculation.
- Panics with `"Escrow not initialized"` before `init`.

---

## `get_version() → u32`

**Storage key:** `DataKey::LegalHold`  
**Signature:** `pub fn get_legal_hold(env: Env) -> bool`

Returns `true` when a compliance hold is active; blocks `settle`, `withdraw`, `claim_investor_payout`, `fund`, and `sweep_terminal_dust`.

**Requires initialization:** No  
**Default when absent:** `false`

---

## `is_fully_funded() → bool`

**Derived from:** `DataKey::Escrow` (`funded_amount`, `funding_target`)

Returns `true` when `funded_amount >= funding_target`.

### Purpose

Exposes the contract's authoritative funding-completion predicate as a pure read view so
frontends no longer need to reimplement the funding logic client-side. Frontends and
indexers should call this view instead of reading `get_escrow()` and comparing fields
manually, because this view exactly mirrors the predicate used internally by the funding
transition logic and is therefore guaranteed to stay in sync with any future changes.

### Return value

| Condition | Returns |
|-----------|---------|
| `funded_amount < funding_target` | `false` |
| `funded_amount == funding_target` | `true` |
| `funded_amount > funding_target` | `true` |

### Exact predicate

```text
funded_amount >= funding_target
```

This is identical to the condition in `fund_impl` that transitions `status` from `0`
(open) to `1` (funded).

### Atomicity note

A `true` result before the funded status transition cannot occur because the transition
is atomic: `funded_amount` is updated and `status` is set to `1` in the same storage
write within `fund_impl`. Consequently `is_fully_funded() == true` implies `status == 1`.

### Authorization

None — pure read; no auth required, no state mutation, no side effects.

---

## `get_legal_hold() → bool`

**Storage key:** `DataKey::LegalHoldClearDelay`  
**Signature:** `pub fn get_legal_hold_clear_delay(env: Env) -> u64`

Returns the configured minimum delay (in seconds) between `request_clear_legal_hold` and `set_legal_hold(false)`.

**Requires initialization:** No  
**Default when absent:** `0` (no delay enforced; hold can be cleared immediately)

---

### `get_legal_hold_clearable_at() → Option<u64>`

**Storage key:** `DataKey::LegalHoldClearableAt`  
**Signature:** `pub fn get_legal_hold_clearable_at(env: Env) -> Option<u64>`

Returns the earliest ledger timestamp at which a pending legal-hold clear may be applied, or `None` when no clear request has been recorded.

**Requires initialization:** No  
**Default when absent:** `None`

**Return value:**
- `Some(timestamp)` after `request_clear_legal_hold` is called.
- `None` when no request is pending (or after a successful clear removes the key).

---

## Funding Constraints

### `get_funding_deadline() → Option<u64>`

**Storage key:** `DataKey::FundingDeadline`  
**Signature:** `pub fn get_funding_deadline(env: Env) -> Option<u64>`

Returns the optional funding deadline (ledger timestamp). After this timestamp passes, `fund` calls are rejected.

**Requires initialization:** No  
**Default when absent:** `None` (no deadline — funding is open indefinitely)

**Return value:**
- `Some(timestamp)` when configured at `init`.
- `None` when no deadline was set.

---

### `is_funding_expired() → bool`

**Signature:** `pub fn is_funding_expired(env: Env) -> bool`

Returns `true` when a funding deadline is set **and** `Env::ledger().timestamp() > deadline`.

**Requires initialization:** No  
**Default when absent:** `false` (no deadline set → never expired)

**Logic:**
```
if FundingDeadline exists:
    return ledger.timestamp() > deadline
else:
    return false
```

---

### `get_min_contribution_floor() → i128`

**Storage key:** `DataKey::MinContributionFloor`  
**Signature:** `pub fn get_min_contribution_floor(env: Env) -> i128`

Returns the minimum per-call funding amount in token base units. Applies to every `fund` / `fund_with_commitment` call.

**Requires initialization:** No (but written as `0` at `init`)  
**Default when absent:** `0` (no extra floor beyond "amount must be positive")

**Notes:**
- The floor applies to **each individual deposit**, not to cumulative principal.
- Written as `0` even when unconfigured at `init`, so reads always succeed post-init.

---

### `get_max_unique_investors_cap() → Option<u32>`

**Storage key:** `DataKey::MaxUniqueInvestorsCap`  
**Signature:** `pub fn get_max_unique_investors_cap(env: Env) -> Option<u32>`

Returns the optional cap on distinct investor addresses. Reflects the current stored cap, including any reduction via `lower_max_unique_investors`.

**Requires initialization:** No  
**Default when absent:** `None` (unlimited investors)

**Return value:**
- `Some(u32)` when configured.
- `None` when no cap was set at `init`.

---

### `get_remaining_investor_slots() -> Option<u32>`

**Signature:** `pub fn get_remaining_investor_slots(env: Env) -> Option<u32>`

Returns the number of remaining investor slots before the `MaxUniqueInvestorsCap` is reached. This safely resolves the gap between the cap and the `get_unique_funder_count`. 

**Requires initialization:** No  
**Default when absent:** `None` (unlimited investors)

**Return value:**
- `None` when no cap is configured (i.e., the escrow accepts unlimited distinct investors).
- `Some(u32)` indicating the exact remaining capacity of new distinct investors. Calculated as `cap - unique_funder_count`. Floored at zero (saturating subtraction) ensuring it stays completely consistent and safe even if the cap is reduced via `lower_max_unique_investors`.

---

### `get_max_per_investor_cap() → Option<i128>`

**Storage key:** `DataKey::MaxPerInvestorCap`  
**Signature:** `pub fn get_max_per_investor_cap(env: Env) -> Option<i128>`

Returns the optional immutable cap on cumulative principal for a single investor address.

**Requires initialization:** No  
**Default when absent:** `None` (unlimited per-investor)

**Return value:**
- `Some(i128)` when configured at `init`.
- `None` when unconfigured.

---

## Maturity & Settlement

### `has_maturity_lock() → bool`

**Derived from:** `DataKey::Escrow.maturity`  
**Signature:** `pub fn has_maturity_lock(env: Env) -> bool`

Returns `true` when `InvoiceEscrow::maturity > 0` and `settle()` is gated by ledger time.

**Requires initialization:** Yes — calls `get_escrow` internally.

**Logic:**
```
return get_escrow().maturity > 0
```

**Return value:**
- `true` — settlement requires `Env::ledger().timestamp() >= maturity`.
- `false` — `maturity == 0`; no time lock, funded escrow can settle immediately.

---

### `get_funding_close_snapshot() → Option<FundingCloseSnapshot>`

**Storage key:** `DataKey::FundingCloseSnapshot`  
**Signature:** `pub fn get_funding_close_snapshot(env: Env) -> Option<FundingCloseSnapshot>`

Returns the pro-rata denominator snapshot captured exactly once when the escrow first transitioned from open (0) to funded (1).

**Requires initialization:** No  
**Default when absent:** `None` (escrow has not yet reached funded status)

**Immutable once written:** the snapshot is never updated after the status-0-to-1 transition.

**Return value:**
- `None` until the escrow reaches `status == 1`.
- `Some(FundingCloseSnapshot)` with fields:
  - `total_principal: i128` — `funded_amount` at close (includes over-funding past target).
  - `funding_target: i128` — Snapshot of target at close time.
  - `closed_at_ledger_timestamp: u64` — Ledger timestamp of the funding transition.
  - `closed_at_ledger_sequence: u32` — Ledger sequence at transition.

Historical alias of [`get_effective_yield_bps`](#get_effective_yield_bpsinvestor-address--i64) —
same return value, documented around the per-investor storage slot.

---

## `get_effective_yield_bps(investor: Address) → i64`

**Storage key:** `DataKey::InvestorEffectiveYield(investor)`, falling back to `DataKey::Escrow.yield_bps`

Returns the **resolved effective yield (bps)** the investor would receive at settlement — exactly the
rate `compute_investor_payout` applies when computing the coupon. The resolution is identical to the
payout math:

```text
effective_yield_bps = InvestorEffectiveYield(investor)   // tier locked at first deposit
                      .unwrap_or(escrow.yield_bps)        // else the escrow base yield
```

| Investor state | Returns |
| --- | --- |
| Tiered (funded via `fund_with_commitment`) | the tier `yield_bps` selected at first deposit |
| Base-only / non-tiered | the escrow base `yield_bps` |
| Unknown (never funded) | the escrow base `yield_bps` |

### Stored vs resolved

`DataKey::InvestorEffectiveYield` is the **stored** per-investor slot: present only after a tiered
first deposit, absent otherwise. This view returns the **resolved** value — the stored slot when
present, otherwise the base-yield fallback — so integrators read the same number the payout math uses
without re-implementing the `unwrap_or` fallback themselves.

`get_investor_yield_bps` returns the same value; prefer `get_effective_yield_bps` when the intent is
"the rate `compute_investor_payout` will actually apply."

---

## Tier Lookup

### `preview_yield_tier(amount: i128, lock: u64) → (i64, u64)`

**Signature:** `pub fn preview_yield_tier(env: Env, amount: i128, lock: u64) -> (i64, u64)`

Pure read — no auth, no storage writes, safe for simulation.

Returns `(effective_yield_bps, matched_lock_secs)` for a hypothetical first deposit of `amount`
with `lock` seconds of commitment, using the **exact same tier-selection rule** applied by
`fund_with_commitment`. This lets a prospective investor see which tier they would receive before
depositing, without re-implementing the selection logic.

The `amount` parameter mirrors the `fund_with_commitment` signature. In the current release, tier
selection is lock-only; `amount` is accepted for API parity and forward-compatibility.

**Return values:**

| Condition | `effective_yield_bps` | `matched_lock_secs` |
|---|---|---|
| No `YieldTierTable` configured | escrow base `yield_bps` | `0` |
| `lock == 0` | escrow base `yield_bps` | `0` |
| `lock` below every tier threshold | escrow base `yield_bps` | `0` |
| `lock >= min_lock_secs` of a tier | highest qualifying tier's `yield_bps` | that tier's `min_lock_secs` |

> **Note:** this preview reflects the rule applied at **first deposit only**. A follow-on
> `fund` call does not re-select a tier.

**Security note:** the preview is guaranteed to agree with `fund_with_commitment` because it delegates
to the same internal `effective_yield_for_commitment` helper — there is no separate selection path.

---

## Deposit Preview

### `preview_fund(env, investor: Address, amount: i128) → u32`

**Signature:** `pub fn preview_fund(env: Env, investor: Address, amount: i128) -> u32`

Pure-read preview: returns `0` if a [`LiquifactEscrow::fund`] call with the same `(investor, amount)`
would succeed on the current ledger, or the numeric [`EscrowError`] code of the **first** guard that
would reject it.

Guards are evaluated in the exact same order as [`LiquifactEscrow::fund_impl`] so the returned code
is always the first failure a real `fund` would encounter.

**Authorization:** None — pure read; no auth required, no state mutation, no side effects.

**Advisory only:** This view is a snapshot. Racing state changes (a concurrent fund, an admin
legal-hold toggle, or a deadline expiry on the next ledger) may cause a real `fund` to revert even
when this view returned `0`. Always handle the `fund` result; never treat a preview success as a
guarantee.

**Return values:**

| Return | Meaning |
|--------|---------|
| `0` | Deposit would be accepted (all guards pass) |
| `20` | Escrow not initialized — [`EscrowError::EscrowNotInitialized`] |
| `100` | Amount ≤ 0 — [`EscrowError::FundingAmountNotPositive`] |
| `101` | Below minimum contribution floor — [`EscrowError::FundingBelowMinContribution`] |
| `102` | Legal hold active — [`EscrowError::LegalHoldBlocksFunding`] |
| `103` | Escrow not in open status — [`EscrowError::EscrowNotOpenForFunding`] |
| `104` | Investor not allowlisted when allowlist is active — [`EscrowError::InvestorNotAllowlisted`] |
| `105` | Contribution would overflow i128 — [`EscrowError::InvestorContributionOverflow`] |
| `106` | Would exceed per-investor cap — [`EscrowError::InvestorContributionExceedsCap`] |
| `107` | New investor would exceed unique-investor cap — [`EscrowError::UniqueInvestorCapReached`] |
| `110` | Total funded amount would overflow — [`EscrowError::FundedAmountOverflow`] |
| `164` | Funding deadline passed — [`EscrowError::FundingDeadlinePassed`] |
| `210` | Operational pause active — [`EscrowError::PausedBlocksFunding`] |

**Guard ordering (matches `fund_impl`):**

1. Amount must be positive.
2. Amount must meet the minimum contribution floor (if configured).
3. Escrow must be initialized.
4. Operational pause must not be active.
5. Legal hold must not be active.
6. Escrow status must be open (`0`).
7. Funding deadline must not have passed (if configured).
8. Investor must be allowlisted (if allowlist is active).
9. Investor contribution must not overflow.
10. New contribution must not exceed the per-investor cap (if configured).
11. New investor must not exceed the unique-investor cap (if configured).
12. Total funded amount must not overflow.

**Code mapping:** Every guard above corresponds to the same-named check in
[`LiquifactEscrow::fund_impl`]. Search for the error variant name in `fund_impl` to
cross-reference the enforcement side. The preview runs the checks in the same linear
order so the first failure code reported here is exactly the first error `fund` itself
would emit.

---

## Per-Investor State

### `get_contribution(investor: Address) → i128`

**Storage key:** `DataKey::InvestorContribution(investor)` (persistent)  
**Signature:** `pub fn get_contribution(env: Env, investor: Address) -> i128`

Returns the cumulative principal contributed by `investor` in token base units.

**Requires initialization:** No  
**Default when absent:** `0` (never contributed)  
**Storage type:** Persistent (independent TTL per address; see ADR-007)

---

### `get_contributions(investors: Vec<Address>) → Vec<i128>`

**Storage key:** `DataKey::InvestorContribution(investor)` (persistent, one read per input)
**Signature:** `pub fn get_contributions(env: Env, investors: Vec<Address>) -> Vec<i128>`

Returns one contribution amount per supplied address, preserving input order. Unknown addresses
return `0`, matching `get_contribution`.

**Requires initialization:** No
**Default when absent:** `0` per address
**Batch bound:** `investors.len() <= MAX_INVESTOR_READ_BATCH` (50)
**Error:** `EscrowError::ContributionReadBatchTooLarge` when the input exceeds the bound
**Security note:** Pure read-only; performs no authorization, storage writes, or TTL extension.

---

### `get_unique_funder_count() → u32`

**Storage key:** `DataKey::UniqueFunderCount`  
**Signature:** `pub fn get_unique_funder_count(env: Env) -> u32`

Returns the count of distinct investor addresses with non-zero contributions. Initialized to `0` at `init`.

**Requires initialization:** No (but written as `0` at `init`)  
**Default when absent:** `0`

**Notes:** counts distinct chain accounts, not real-world persons (Sybil resistance is not a goal of this counter).

---

### `get_investor_yield_bps(investor: Address) → i64`

**Storage key:** `DataKey::InvestorEffectiveYield(investor)` (persistent)  
**Signature:** `pub fn get_investor_yield_bps(env: Env, investor: Address) -> i64`

Returns the effective annualized yield in basis points locked in at the investor's first deposit.

**Requires initialization:** Yes — reads `get_escrow()` for the base yield fallback.  
**Default when absent:** falls back to `InvoiceEscrow::yield_bps` (base yield for legacy / simple `fund` positions)  
**Storage type:** Persistent

**Return value:**
- Investor's tier-selected `yield_bps` when set via `fund_with_commitment`.
- Base `InvoiceEscrow::yield_bps` for simple `fund` deposits or pre-v2 positions.

---

## `get_distributed_principal() → i128`

**Storage key:** `DataKey::DistributedPrincipal`

Returns the total principal already returned to investors via [`LiquifactEscrow::refund`].

- Used by [`LiquifactEscrow::sweep_terminal_dust`] to compute outstanding liabilities.
- Absent ⇒ `0` (no refunds have occurred).

---

## `get_token_balance() → i128`

**Storage key:** None (reads [`DataKey::FundingToken`] and queries token contract)

Returns the contract's current funding-token balance for on-chain custody reconciliation.

- Emits [`EscrowError::FundingTokenNotSet`] if called before `init`.
- **Pure read** — no authorization required, no state mutation.

### Reconciliation relationship

Auditors can reconcile on-chain custody against recorded liabilities:

```
balance = get_token_balance()
funded_amount = get_escrow().funded_amount
distributed_principal = get_distributed_principal()

outstanding_liability = funded_amount - distributed_principal
excess_balance = balance - outstanding_liability  // tokens available for sweep

// After the cancelled escrow's liability is fully discharged (all refunds complete):
// balance == distributed_principal == funded_amount  (or less if partial sweep occurred)
```

This view surfaces the balance already consulted internally by [`LiquifactEscrow::sweep_terminal_dust`]
and [`LiquifactEscrow::withdraw`] for liability-floor enforcement.

---

## `get_reconciliation() → ReconciliationView`

**Storage keys:** reads `DataKey::Escrow`, `DataKey::DistributedPrincipal`, and
`DataKey::FundingToken` (then queries the token contract for the live balance).

Returns the contract's full reconciliation position in a single call, so operators
no longer have to fetch the balance, funded amount, distributed principal, and
settlement state separately and re-implement the liability arithmetic off-chain
(see the [Reconciliation relationship](#reconciliation-relationship) above).

```text
outstanding_liability = max(funded_amount - distributed_principal, 0)
surplus               = token_balance - outstanding_liability
```

`outstanding_liability` uses the **identical floor** that
[`LiquifactEscrow::sweep_terminal_dust`] enforces, so the view and the sweep guard
can never disagree. `surplus` is the sweepable dust when positive and a deficit
when negative.

### `ReconciliationView` fields

| Field | Type | Description |
|-------|------|-------------|
| `token_balance` | `i128` | Live SEP-41 funding-token balance held by the contract. |
| `outstanding_liability` | `i128` | Principal still owed to investors: `max(funded_amount - distributed_principal, 0)`. |
| `surplus` | `i128` | `token_balance - outstanding_liability`. Positive = sweepable surplus; negative = deficit. |

- **Pure read** — no authorization required, no state mutation.
- **Never panics on values** — all arithmetic is saturating.
- Emits [`EscrowError::EscrowNotInitialized`] / [`EscrowError::FundingTokenNotSet`]
  only when the escrow has not been initialized.

**Security note:** in settled (`2`) and withdrawn (`3`) states `distributed_principal`
is `0` by design, so `outstanding_liability` reflects the full `funded_amount` and the
reported `surplus` is never larger than what `sweep_terminal_dust` would actually
permit (that guard only applies the floor in the cancelled state `4`). The view is
therefore conservative and can never over-report sweepable funds.

---

### `is_investor_claimed(investor: Address) → bool`

**Storage key:** `DataKey::InvestorClaimed(investor)` (persistent)  
**Signature:** `pub fn is_investor_claimed(env: Env, investor: Address) -> bool`

Returns `true` when the investor has exercised `claim_investor_payout` after settlement.

**Requires initialization:** No  
**Default when absent:** `false`  
**Storage type:** Persistent

**Notes:** written once and never unset. A second `claim_investor_payout` call is a no-op (idempotent) rather than an error.

---

### `is_investor_refunded(investor: Address) → bool`

**Storage key:** `DataKey::InvestorRefunded(investor)`  
**Signature:** `pub fn is_investor_refunded(env: Env, investor: Address) -> bool`

Returns `true` when an investor's principal has been returned via `refund` in a cancelled (status 4) escrow.

**Requires initialization:** No  
**Default when absent:** `false`

**Notes:** written once; prevents double-refund. After `refund` succeeds, `get_contribution` for the same address returns `0`.

---

### `compute_investor_payout(investor: Address) → i128`

**Signature:** `pub fn compute_investor_payout(env: Env, investor: Address) → i128`

- `None` — Escrow is not yet funded; no close snapshot exists.
- `Some(FundingCloseSnapshot)` — The pro-rata denominator snapshot captured when the escrow first transitioned to **funded**.

---

### `get_settlement_pool() → i128`

**Storage keys:** `DataKey::FundingCloseSnapshot`, `DataKey::Escrow`  
**Signature:** `pub fn get_settlement_pool(env: Env) -> i128`

Returns the **total settlement pool** owed by the SME — the aggregate principal plus base-yield
coupon the SME must repay to fully satisfy all investors. Avoids rounding divergence that arises
when off-chain tooling re-derives the formula from raw snapshot fields.

#### Formula (floor / truncating integer division)

```text
coupon       = total_principal × yield_bps / 10_000  (floor)
settle_pool  = total_principal + coupon
```

Where `total_principal` is from `DataKey::FundingCloseSnapshot` and `yield_bps` is the
escrow base yield from `InvoiceEscrow::yield_bps`.

#### Yield note

Uses the escrow **base yield** only. Per-investor effective yields from `fund_with_commitment`
tier selection are reflected individually in `compute_investor_payout` but are **not** aggregated
here.

#### Return values

| Condition | Returns |
|-----------|---------|
| `DataKey::FundingCloseSnapshot` absent (escrow not yet funded) | `0` |
| `total_principal <= 0` (degenerate snapshot) | `0` |
| Normal funded state | `total_principal + floor(total_principal × yield_bps / 10_000)` |

#### Overflow safety

All multiplications use `i128::checked_mul`; all divisions use `i128::checked_div`. Emits
`EscrowError::ComputePayoutArithmeticOverflow` (code 129) on overflow.

#### Rounding invariant

Sum of all per-investor `compute_investor_payout` values is guaranteed ≤ `get_settlement_pool()`.
Any fractional residue is swept by `sweep_terminal_dust`.

#### Authorization

None — pure read; no auth required and no state mutation.

---

## `get_yield_tiers() → Vec<YieldTier>`

**Storage key:** `DataKey::YieldTierTable`

Returns the yield-tier ladder configured at `init`, or an empty `Vec` when no tiers were configured (base yield applies to all investors).

- **Immutable** — set once at `init`; the contract never mutates this key after initialization.
- **Order** — returned order matches the validated non-decreasing ordering enforced at `init`: `min_lock_secs` strictly increasing, `yield_bps` non-decreasing.
- **Empty vec** — returned for both "no tiers passed at init" and "legacy instance predating tier support"; callers must not treat an empty result as an error.
- **Pure read** — no auth required, no state mutation.

### `YieldTier` fields

| Field | Type | Description |
|-------|------|-------------|
| `min_lock_secs` | `u64` | Minimum `committed_lock_secs` an investor must pass to qualify for this tier |
| `yield_bps` | `i64` | Effective annualized yield in basis points for qualifying investors |

---

## Paginated Enumeration

`get_investors`, `get_allowlisted_investors`, and `get_revoked_attestation_digests` share one `(start, limit)` contract. Both args are `u32`; `start` is a zero-based index position and `limit` is clamped (never rejected) to a per-view maximum. See docs/escrow-indexer.md for how indexers poll these.

All three apply the same guard first:

```text
len = length of backing index/log   // absent key => 0
if start >= len || limit == 0 { return empty Vec }
```

So `start >= len` or `limit == 0` returns an empty `Vec` (never panics), and an absent backing key behaves as `len == 0`. All three are pure reads (no auth, no writes, no TTL bump). Every backing collection is append-only, so a given `start` keeps addressing the same position as new entries are appended at the tail; already-read pages stay stable.

| View | Backing key | Max limit | Page shape |
|------|-------------|-----------|------------|
| `get_investors` | `InvestorIndex` | `MAX_INVESTOR_READ_BATCH` = 50 | Unfiltered half-open slice |
| `get_allowlisted_investors` | `AllowlistIndex` | 50 | Half-open slice, then filtered |
| `get_revoked_attestation_digests` | `AttestationAppendLog` | `MAX_ATTESTATION_READ_PAGE` = 20 | Forward scan for matches |

**get_investors** returns `index[start .. min(start + min(limit,50), len)]` in first-funded order, one address per position. Detect the last page when the returned length is below your page size; otherwise call again with `start += page_size`.

**get_allowlisted_investors** walks the same window over `AllowlistIndex` but drops addresses whose live `InvestorAllowlisted(addr)` flag is no longer true (revoked entries stay in the index and are skipped at read time). A page may be shorter than the window, so a short or empty page is NOT end-of-list. Advance `start` by the page size each call and stop once `start >= len`. Note `get_allowlisted_investors_count` returns only the live count, not the index length.

**get_revoked_attestation_digests** scans forward from `start` collecting up to `min(limit,20)` revoked entries, skipping non-revoked ones; it stops on match count, not positions scanned, and does not report where it stopped. The log is capped at 32 entries, so `start = 0` then `start = 20` covers any instance. Read the log length from `get_attestation_log_stats` or `get_escrow_summary().attestation_log_length`; do not derive the next `start` from the returned length.

Worked loop (full investor enumeration, 50 at a time):

```text
start = 0
page_size = 50
loop {
    page = get_investors(start, page_size)
    process(page)
    if page.len() < page_size { break }   // last page
    start += page_size
}
```
