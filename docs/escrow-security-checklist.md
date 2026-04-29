# Escrow Security Checklist

> Issue #167 — Soroban security research.
> Scope: `escrow/src/lib.rs` and `escrow/src/external_calls.rs`.
> Schema version: [`SCHEMA_VERSION`] = 5.

---

## 1. Authentication Matrix

Every state-mutating entrypoint and the identity required to authorize it.

| Entrypoint | Required signer | Auth call site | Notes |
|---|---|---|---|
| `init` | `admin` (caller-supplied) | `admin.require_auth()` | One-time; panics if escrow exists |
| `transfer_admin` | current `escrow.admin` | `escrow.admin.require_auth()` | `new_admin` must differ; overwrites stored admin |
| `update_maturity` | `escrow.admin` | `escrow.admin.require_auth()` | Only in `status == 0` |
| `update_funding_target` | `escrow.admin` | `escrow.admin.require_auth()` | Only in `status == 0`; `new_target >= funded_amount` |
| `set_legal_hold` / `clear_legal_hold` | `escrow.admin` | `escrow.admin.require_auth()` | No timelock; no multisig enforced on-chain |
| `set_allowlist_active` | `escrow.admin` | `escrow.admin.require_auth()` | Enables/disables `AllowlistActive` gate |
| `set_investor_allowlisted` | `escrow.admin` | `escrow.admin.require_auth()` | Writes to **persistent** storage (see §5.4) |
| `bind_primary_attestation_hash` | `escrow.admin` | `escrow.admin.require_auth()` | Single-set; second call panics |
| `append_attestation_digest` | `escrow.admin` | `escrow.admin.require_auth()` | Bounded at `MAX_ATTESTATION_APPEND_ENTRIES` = 32 |
| `fund` | `investor` (caller-supplied) | `investor.require_auth()` | `status == 0`; allowlist-gated when active |
| `fund_with_commitment` | `investor` (caller-supplied) | `investor.require_auth()` | First deposit only (`prev == 0`); sets claim lock |
| `record_sme_collateral_commitment` | `escrow.sme_address` | `escrow.sme_address.require_auth()` | Ledger record only; no token transfer |
| `settle` | `escrow.sme_address` | `escrow.sme_address.require_auth()` | `status == 1`; optional maturity gate |
| `withdraw` | `escrow.sme_address` | `escrow.sme_address.require_auth()` | `status == 1`; sets `status = 3` |
| `claim_investor_payout` | `investor` (caller-supplied) | `investor.require_auth()` | `status == 2`; contribution > 0; claim-lock gate |
| `sweep_terminal_dust` | `treasury` | `treasury.require_auth()` | `status == 2 or 3`; amount ≤ `MAX_DUST_SWEEP_AMOUNT` |
| `migrate` | **none** | *(no `require_auth`)* | **Always panics** on all current paths — safe now, dangerous if logic is added without adding an auth guard (see §5.1) |

### Read-only entrypoints

All `get_*` and `is_*` functions carry no `require_auth`. They expose full escrow state to any caller. This is intentional for indexers and off-chain tooling; treat all stored data as public.

---

## 2. Trusted Addresses

### 2.1 Admin (`InvoiceEscrow::admin`)

- Set at `init`; mutable only via `transfer_admin` (requires current admin signature).
- Controls: hold activation, allowlist, attestation binding, maturity, funding target, schema migration (future).
- **Risk**: a single EOA admin can indefinitely freeze funds via `set_legal_hold`. Production deployments **must** use a governed contract or multisig at this address. There is no on-chain escape hatch if the admin key is lost or malicious.

### 2.2 SME (`InvoiceEscrow::sme_address`)

- Immutable after `init`; cannot be rotated.
- Controls: settlement finalization (`settle`), liquidity pull (`withdraw`), collateral record (`record_sme_collateral_commitment`).
- **Risk**: if `sme_address` is compromised while the escrow is funded, the SME can call `withdraw` (status → 3) before maturity, pulling all principal. No admin veto exists on `withdraw`.

### 2.3 Treasury (`DataKey::Treasury`)

- Set once at `init`; immutable.
- Only permitted actor for `sweep_terminal_dust`.
- Receives at most `MAX_DUST_SWEEP_AMOUNT` = 100,000,000 base units per call, only in terminal states.
- **Risk**: if treasury address is a contract, confirm it cannot re-enter or redirect the transfer during the SEP-41 call. Soroban host-function atomicity makes interleaved re-entry impossible, but the treasury contract receiving the transfer could call back into unrelated contracts after the balance check.

### 2.4 Funding Token (`DataKey::FundingToken`)

- Set once at `init`; immutable.
- Treated as a compliant SEP-41 token for all balance-delta checks.
- See §4 for threat model.

### 2.5 Registry (`DataKey::RegistryRef`)

- Optional; written as a hint at `init`.
- **No on-chain authority.** The contract never reads or verifies this address after storage. Off-chain indexers must not treat its presence as proof of current registry membership.

---

## 3. Invariants

These conditions must hold at every ledger boundary. A violation indicates either a contract bug or an adversarial token.

### I-1: Contribution accounting

```
funded_amount == Σ InvestorContribution(addr) for all addr with a non-zero entry
```

Maintained in `fund_impl`: `escrow.funded_amount` is incremented by `amount` atomically with `InvestorContribution(investor) += amount` in the same host function. No other entrypoint modifies `funded_amount` or individual contributions.

**Caveat**: `fund_impl` does **not** pull tokens from the investor. The contract records the commitment but does not enforce that the caller's token balance decreased. Actual token custody is the responsibility of the integration layer. See §5.2.

### I-2: Status monotonicity

```
status ∈ {0, 1, 2, 3}
0 → 1 → 2  (settle path)
0 → 1 → 3  (withdraw path)
```

No entrypoint decrements or resets `status`. Once `status == 2` or `status == 3`, no further state transitions are possible.

### I-3: FundingCloseSnapshot is write-once

Written inside `fund_impl` the first time `funded_amount >= funding_target` triggers status → 1. Guarded by `if !env.storage().instance().has(&DataKey::FundingCloseSnapshot)`. Never overwritten. Immutable after first set.

### I-4: InvestorClaimed is write-once

`InvestorClaimed(addr)` is set to `true` in `claim_investor_payout`. The function returns early on a second call (idempotent). Never reset to `false`.

### I-5: PrimaryAttestationHash is single-set

`bind_primary_attestation_hash` panics if `DataKey::PrimaryAttestationHash` already exists. Cannot be replaced or cleared.

### I-6: UniqueFunderCount monotonically increases

Incremented once per new investor (`prev == 0`) in `fund_impl`. Never decremented. Bounded above by `MaxUniqueInvestorsCap` when set.

### I-7: AttestationAppendLog bounded

`log.len() < MAX_ATTESTATION_APPEND_ENTRIES` (32) is enforced before each append. Panics at capacity.

### I-8: Immutable init keys

`FundingToken`, `Treasury`, and `YieldTierTable` are written once at `init` and never overwritten by any entrypoint. Confirmed by absence of any `.set(&DataKey::FundingToken, ...)` after the init block.

### I-9: funded_amount ≥ 0

`fund_impl` only accepts positive `amount` and uses `checked_add` (panics on overflow). No entrypoint subtracts from `funded_amount`. The stored value can only grow.

### I-10: Yield tier ladder non-decreasing

Enforced in `validate_yield_tiers_table` at `init`: each tier must have `yield_bps >= previous.yield_bps` and `min_lock_secs > previous.min_lock_secs`. Base `yield_bps` is a lower bound. Immutable after init.

---

## 4. SEP-41 Trust Boundaries and Breakpoints

The only on-chain token interaction is in `external_calls::transfer_funding_token_with_balance_checks`, called exclusively from `sweep_terminal_dust`.

### 4.1 Fee-on-transfer tokens

`transfer_funding_token_with_balance_checks` measures `from_before`, `treasury_before`, calls `token.transfer(...)`, then asserts:

```rust
spent == amount    // sender delta
received == amount // recipient delta
```

A fee-on-transfer token delivers `amount - fee` to the recipient. The recipient-delta assertion panics. **Effect**: `sweep_terminal_dust` is permanently broken for this escrow instance. Tokens remain locked. Governance must redeploy.

### 4.2 Rebasing tokens

If the token contract modifies balances between the pre-transfer read and the post-transfer read (e.g. yield accrual mid-call), either delta assertion may fail. Same outcome as §4.1.

### 4.3 Balance-query manipulation

A malicious token could return a fabricated value from `balance()`. If the token returns `from_before < amount`, the insufficient-balance assert fires before the transfer. If it manipulates `from_after` to fake a smaller spend or `treasury_after` to fake a larger receipt, the delta assertions catch it.

The checks are necessary but assume the token does not lie about both the pre- and post-transfer balances in a coordinated way. A token that lies consistently (same fake delta on both sides) can pass the checks. Governance allowlists are the correct defense; the contract cannot eliminate this assumption.

### 4.4 Soroban execution model and reentrancy

Soroban executes cross-contract calls synchronously to completion within the host function. A token contract cannot interleave execution mid-transfer into the escrow's own entrypoints. Classic EVM reentrancy is not applicable.

A token contract can, however, call **other** contracts during `transfer`. If those contracts interact with this escrow in a separate transaction, no in-flight state is exposed. The pre/post balance pattern remains the correct defense for correctness.

### 4.5 Token contract upgrade post-init

`FundingToken` is an `Address`. If the token contract at that address is upgraded to a malicious implementation after escrow `init`, all subsequent `balance()` and `transfer()` calls go to the upgraded code. The escrow cannot detect this. Governance must monitor the token contract's upgrade authority and treat any upgrade as a potential invalidation of this escrow's token assumptions.

### 4.6 fund() and withdraw() do not transfer tokens

`fund_impl` records investor contributions and updates `funded_amount`. **It does not call the token contract.** Actual tokens must arrive at the escrow contract address via a separate SEP-41 `transfer` call by the investor. If an investor calls `fund()` without first transferring tokens, the accounting diverges from the actual token balance. The contract treats the caller's signed commitment as the source of truth.

`withdraw()` and `settle()` change status but do not move tokens. `claim_investor_payout()` sets a claimed flag and emits an event. The actual token disbursement to the SME or investors is off-chain or via a separate orchestrating contract. **The contract is an accounting ledger, not a token custodian for payouts.**

This creates a window where `funded_amount` > actual token balance (unfunded commitments) or actual token balance > `funded_amount` (stray transfers, airdrops). `sweep_terminal_dust` addresses the latter in terminal states, capped at `MAX_DUST_SWEEP_AMOUNT`.

---

## 5. Assumptions and Risks

### 5.1 `migrate()` has no auth guard

`migrate` performs no `require_auth()` check. In the current implementation every code path panics before any storage write, so there is no exploitable consequence. **If a future developer adds migration logic before the panic branches, the function becomes callable by any account.** Before implementing a migration path, add `escrow.admin.require_auth()` as the first statement.

### 5.2 Accounting-custody decoupling

`funded_amount` is a commitment record, not a token balance assertion. Integrations that custody principal on-chain must enforce that token transfers and `fund()` calls are atomic (e.g., via an outer orchestrator contract or a Soroban transaction with both operations). Without this, `funded_amount` can overstate actual holdings, and `sweep_terminal_dust` could drain funds that were never genuinely deposited.

### 5.3 Legal hold has no on-chain expiry

`LegalHold` is a boolean toggled exclusively by `escrow.admin`. There is no timelock, no council override, and no programmatic expiry. A compromised or malicious admin can freeze `settle`, `withdraw`, `claim_investor_payout`, and `sweep_terminal_dust` indefinitely. Off-chain governance (multisig rotation, emergency procedures) is the only recovery path. Document the admin governance model before deploying to production.

### 5.4 Storage type mismatch: AllowlistActive vs. InvestorAllowlisted

`AllowlistActive` is stored in **instance** storage. `InvestorAllowlisted(addr)` entries are stored in **persistent** storage. These have different TTL semantics under Soroban's rent model. If instance storage expires and is not extended, `AllowlistActive` returns `false` (default via `unwrap_or`), silently disabling the allowlist gate even if persistent allowlist entries remain. Operators must extend instance storage TTL together with persistent storage TTL.

### 5.5 Ledger timestamp trust

`settle`, `claim_investor_payout`, and `fund_with_commitment` (claim lock calculation) rely on `env.ledger().timestamp()`. This is validator-observed ledger close time, not a wall-clock oracle. Boundaries are `>=` / `<` comparisons on integer seconds. There is measurable skew between simulated environments (Futurenet, local) and mainnet. Do not assume sub-minute maturity precision.

### 5.6 Admin key loss is unrecoverable

There is no guardian, recovery address, or protocol DAO escape hatch. If `escrow.admin` is lost: legal holds cannot be cleared, attestation digests cannot be appended, and future schema migrations cannot be authorized (once implemented). `sme_address` is also immutable. Key management for both addresses is a deployment prerequisite, not a contract-level guarantee.

### 5.7 Over-funding is intentional and unbound above the target

`fund_impl` permits `funded_amount` to exceed `funding_target`. The `FundingCloseSnapshot` records the actual `funded_amount` at close (including overflow). Off-chain pro-rata calculations must use `snapshot.total_principal`, not `snapshot.funding_target`. Misuse of the funding target as the denominator underestimates each investor's share.

### 5.8 Collateral pledge is non-custodial

`record_sme_collateral_commitment` writes a `SmeCollateralCommitment` struct. It does not transfer, lock, or encumber any on-chain asset. The record cannot trigger automated liquidation. Do not use the presence of a collateral record as on-chain proof of asset lock without a separate enforcement contract.

### 5.9 InvestorClaimed is an event marker, not a payment proof

`claim_investor_payout` marks an investor as claimed and emits `InvestorPayoutClaimed`. It does not transfer tokens. Off-chain systems that release principal or yield based on this event must implement their own idempotency and replay guards. A re-emitted or replayed event must not trigger a second disbursement.
