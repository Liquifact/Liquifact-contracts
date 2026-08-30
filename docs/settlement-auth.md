# LiquiFact Escrow Settlement Authorization Reference

This document describes the authorization rules governing settlement-related entrypoints in the LiquiFact Escrow contract. It covers who may call each function, in which escrow state, and which errors are returned when preconditions fail.

---

## Status Definitions

| Status Value | State Name | Description |
|--------------|------------|-------------|
| `0` | **Open** | Escrow initialized; accepts investor contributions. |
| `1` | **Funded** | Target funding met (`funded_amount >= funding_target`). SME may withdraw or settle. |
| `2` | **Settled** | SME has finalized settlement. Payout claims are unlocked. |
| `3` | **Withdrawn** | SME has withdrawn the funded liquidity. Terminal state. |
| `4` | **Cancelled** | Admin aborted funding. Investor refunds are unlocked. |

---

## Roles

| Role | Source | Authorization Mechanism |
|------|--------|------------------------|
| **Admin** | `InvoiceEscrow::admin` | `load_escrow_require_admin` → `admin.require_auth()` |
| **SME** | `InvoiceEscrow::sme_address` | `load_escrow_require_sme` → `sme_address.require_auth()` |
| **Investor** | `DataKey::InvestorContribution` | `investor.require_auth()` (self-auth) |
| **Treasury** | `DataKey::Treasury` | `treasury.require_auth()` |

---

## Settlement-Related Entrypoints

### 1. `partial_settle(env: Env, caller: Address) -> InvoiceEscrow`

Closes funding early for an under-funded invoice, transitioning the escrow to a settleable state (status `1`).

| Aspect | Detail |
|--------|--------|
| **Authorized caller** | `sme_address` **or** `admin` |
| **Auth check** | `caller.require_auth()` — explicit address passed as argument |
| **Legal-hold gate** | Blocked if `LegalHold == true` → `LegalHoldBlocksPartialSettle (201)` |
| **Status precondition** | `status == 0` (Open) → `PartialSettleNotOpen (202)` if not |
| **Unauthorized caller** | `PartialSettleUnauthorizedCaller (200)` |
| **State transition** | `0 → 1` (writes `FundingCloseSnapshot` if not present) |
| **Operational pause** | **Not checked** (partial settle is not pause-gated) |

**Guard ordering:**
1. `caller.require_auth()`
2. `guard_not_legal_hold(..., LegalHoldBlocksPartialSettle)`
3. Load escrow (no auth on load)
4. `ensure(caller == sme_address || caller == admin, PartialSettleUnauthorizedCaller)`
5. `guard_status_eq(..., 0, PartialSettleNotOpen)`

---

### 2. `settle(env: Env) -> InvoiceEscrow`

Finalizes the escrow after maturity (when configured), transitioning to `status = 2` (Settled) and unlocking investor payout claims.

| Aspect | Detail |
|--------|--------|
| **Authorized caller** | `sme_address` (only) |
| **Auth check** | `load_escrow_require_sme` → `sme_address.require_auth()` |
| **Legal-hold gate** | Blocked if `LegalHold == true` → `LegalHoldBlocksSettlement (120)` |
| **Operational pause gate** | Blocked if `Paused == true` → `PausedBlocksSettlement (211)` |
| **Status precondition** | `status != 2` → `EscrowAlreadySettled (236)` (once-only guard); then `status == 1` (Funded) → `SettlementNotFunded (121)` if not |
| **Maturity gate** | If `maturity > 0`, requires `ledger.timestamp() >= maturity` → `MaturityNotReached (122)` |
| **State transition** | `1 → 2` (writes `SettledAt` ledger timestamp, emits `EscrowSettled` event) |
| **Settle pool computation** | `settle_pool = funded_amount + (funded_amount * yield_bps / 10_000)` (floor) |

**Guard ordering:**
1. `ensure(!paused_active, PausedBlocksSettlement)`
2. `guard_not_legal_hold(..., LegalHoldBlocksSettlement)`
3. `load_escrow_require_sme` (loads escrow + `sme_address.require_auth()`)
4. `ensure(status != 2, EscrowAlreadySettled)` — once-only guard; a re-entrant/replayed second settle is rejected here
5. `ensure(status == 1, SettlementNotFunded)`
6. `if maturity > 0 { ensure(now >= maturity, MaturityNotReached) }`

---

### 3. `withdraw(env: Env) -> InvoiceEscrow`

SME pulls funded liquidity, net of the immutable protocol fee, transitioning to `status = 3` (Withdrawn).

| Aspect | Detail |
|--------|--------|
| **Authorized caller** | `sme_address` (only) |
| **Auth check** | `load_escrow_require_sme` → `sme_address.require_auth()` |
| **Legal-hold gate** | Blocked if `LegalHold == true` → `LegalHoldBlocksWithdrawal (123)` |
| **Operational pause gate** | Blocked if `Paused == true` → `PausedBlocksWithdrawal (212)` |
| **Status precondition** | `status == 1` (Funded) → `WithdrawalNotFunded (124)` if not |
| **Balance check** | Contract must hold `>= funded_amount` → `InsufficientContractBalance (165)` |
| **State transition** | `1 → 3` (updates `DistributedPrincipal`, transfers fee to treasury + net to SME) |
| **Fee split** | `fee = funded_amount * fee_bps / 10_000` (floor), `sme_payout = funded_amount - fee` |

**Guard ordering:**
1. `ensure(!paused_active, PausedBlocksWithdrawal)`
2. `guard_not_legal_hold(..., LegalHoldBlocksWithdrawal)`
3. `load_escrow_require_sme`
4. `guard_status_eq(..., 1, WithdrawalNotFunded)`
5. Balance sufficiency check
6. Fee/net computation (checked arithmetic)

---

### 4. `claim_investor_payout(env: Env, investor: Address)`

Investor claims their pro-rata gross payout after settlement. Idempotent — second call is a no-op.

| Aspect | Detail |
|--------|--------|
| **Authorized caller** | `investor` (self-auth) |
| **Auth check** | `investor.require_auth()` |
| **Legal-hold gate** | Blocked if `LegalHold == true` → `LegalHoldBlocksInvestorClaims (125)` |
| **Operational pause gate** | Blocked if `Paused == true` → `PausedBlocksInvestorClaims (213)` |
| **Status precondition** | `status == 2` (Settled) → `InvestorClaimNotSettled (127)` if not |
| **Contribution check** | `InvestorContribution > 0` → `NoContributionToClaim (126)` |
| **Commitment lock** | `ledger.timestamp() >= InvestorClaimNotBefore` → `InvestorCommitmentLockNotExpired (128)` |
| **Idempotency** | Returns early if `InvestorClaimed == true` (no re-emit) |
| **State mutation** | Sets `InvestorClaimed = true` **before** transfer (checks-effects-interactions) |

**Guard ordering:**
1. `ensure(!paused_active, PausedBlocksInvestorClaims)`
2. `guard_not_legal_hold(..., LegalHoldBlocksInvestorClaims)`
3. `investor.require_auth()`
4. Load contribution, `ensure(contribution > 0, NoContributionToClaim)`
5. Load escrow, `guard_status_eq(..., 2, InvestorClaimNotSettled)`
6. `ensure(now >= not_before, InvestorCommitmentLockNotExpired)`
7. Early-return if already claimed
8. Compute payout, `ensure(payout > 0, PayoutZero)`
9. `set_persistent_investor_claimed(true)` — **before** transfer
10. Token transfer to investor

---

### 5. `cancel_funding(env: Env) -> InvoiceEscrow`

Admin aborts the funding round, transitioning to `status = 4` (Cancelled) and unlocking investor refunds.

| Aspect | Detail |
|--------|--------|
| **Authorized caller** | `admin` (only) |
| **Auth check** | `load_escrow_require_admin` → `admin.require_auth()` |
| **Legal-hold gate** | Blocked if `LegalHold == true` → `LegalHoldBlocksCancelFunding (140)` |
| **Status precondition** | `status == 0` (Open) → `CancelFundingNotOpen (141)` if not |
| **State transition** | `0 → 4` (emits `FundingCancelled` event) |
| **Operational pause** | **Not checked** (cancel is not pause-gated) |

**Guard ordering:**
1. `guard_not_legal_hold(..., LegalHoldBlocksCancelFunding)`
2. `load_escrow_require_admin`
3. `guard_status_eq(..., 0, CancelFundingNotOpen)`

---

### 6. `refund(env: Env, investor: Address)`

Investor reclaims their recorded principal when escrow is cancelled.

| Aspect | Detail |
|--------|--------|
| **Authorized caller** | `investor` (self-auth) |
| **Auth check** | `investor.require_auth()` |
| **Legal-hold gate** | **Not checked** (refund is not hold-gated) |
| **Operational pause** | **Not checked** |
| **Status precondition** | `status == 4` (Cancelled) → `RefundNotCancelled (142)` if not |
| **Contribution check** | `InvestorContribution > 0` → `NoContributionToRefund (143)` |
| **Idempotency** | Contribution zeroed **before** transfer (checks-effects-interactions) |
| **Accounting** | Increments `DistributedPrincipal` by refunded amount |

**Guard ordering:**
1. `investor.require_auth()`
2. Load escrow, `guard_status_eq(..., 4, RefundNotCancelled)`
3. Load contribution, `ensure(amount > 0, NoContributionToRefund)`
4. Zero contribution, set `InvestorRefunded = true`
5. Update `DistributedPrincipal`
6. Token transfer to investor

---

### 7. `sweep_terminal_dust(env: Env, amount: i128) -> i128`

Treasury sweeps residual funding-token balance from a terminal escrow (status 2, 3, or 4).

| Aspect | Detail |
|--------|--------|
| **Authorized caller** | `treasury` (only) |
| **Auth check** | `treasury.require_auth()` (explicit check after loading escrow) |
| **Legal-hold gate** | Blocked if `LegalHold == true` → `LegalHoldBlocksTreasuryDustSweep (30)` |
| **Status precondition** | `is_terminal_status(status)` (2, 3, or 4) → `DustSweepNotTerminal (33)` |
| **Amount bounds** | `0 < amount <= MAX_DUST_SWEEP_AMOUNT` → `SweepAmountNotPositive (31)` / `SweepAmountExceedsMax (32)` |
| **Balance check** | Contract must hold `> 0` funding token → `NoFundingTokenBalanceToSweep (34)` |
| **Liability floor (cancelled only)** | `balance - sweep_amt >= funded_amount - distributed_principal` → `SweepExceedsLiabilityFloor (42)` |
| **Effective amount** | `min(amount, balance)` → `EffectiveSweepAmountZero (35)` if zero |

**Guard ordering:**
1. `guard_not_legal_hold(..., LegalHoldBlocksTreasuryDustSweep)`
2. `ensure(amount > 0, SweepAmountNotPositive)`
3. `ensure(amount <= MAX_DUST_SWEEP_AMOUNT, SweepAmountExceedsMax)`
4. Load escrow, `ensure(is_terminal_status(status), DustSweepNotTerminal)`
5. `treasury.require_auth()`
6. Load token, check balance > 0
7. Compute `sweep_amt = amount.min(balance)`, `ensure(sweep_amt > 0, EffectiveSweepAmountZero)`
8. If `status == 4`, enforce liability floor
9. Token transfer to treasury

---

## Summary Matrix: Settlement-Related Entrypoints

| Entrypoint | Role | Auth Source | Legal Hold | Pause | Status Gate | State Transition |
|------------|------|-------------|------------|-------|-------------|------------------|
| `partial_settle` | SME \|\| Admin | `caller` arg | ✅ 201 | ❌ | `status == 0` | `0 → 1` |
| `settle` | SME | `sme_address` | ✅ 120 | ✅ 211 | `status == 1` + maturity | `1 → 2` |
| `withdraw` | SME | `sme_address` | ✅ 123 | ✅ 212 | `status == 1` + balance | `1 → 3` |
| `claim_investor_payout` | Investor | `investor` arg | ✅ 125 | ✅ 213 | `status == 2` + lock | `2 → 2` (claimed) |
| `cancel_funding` | Admin | `admin` | ✅ 140 | ❌ | `status == 0` | `0 → 4` |
| `refund` | Investor | `investor` arg | ❌ | ❌ | `status == 4` | `4 → 4` (refunded) |
| `sweep_terminal_dust` | Treasury | `treasury` | ✅ 30 | ❌ | `is_terminal_status` | `2/3/4 → 2/3/4` |

---

## Error Codes Reference (Settlement-Related)

| Code | Constant | Trigger |
|------|----------|---------|
| 120 | `LegalHoldBlocksSettlement` | `settle()` with active legal hold |
| 121 | `SettlementNotFunded` | `settle()` when `status != 1` and `status != 2` |
| 236 | `EscrowAlreadySettled` | `settle()` on an already-settled escrow (`status == 2`) |
| 122 | `MaturityNotReached` | `settle()` before `maturity` timestamp |
| 123 | `LegalHoldBlocksWithdrawal` | `withdraw()` with active legal hold |
| 124 | `WithdrawalNotFunded` | `withdraw()` when `status != 1` |
| 125 | `LegalHoldBlocksInvestorClaims` | `claim_investor_payout()` with active legal hold |
| 126 | `NoContributionToClaim` | `claim_investor_payout()` with zero contribution |
| 127 | `InvestorClaimNotSettled` | `claim_investor_payout()` when `status != 2` |
| 128 | `InvestorCommitmentLockNotExpired` | `claim_investor_payout()` before `not_before` |
| 129 | `ComputePayoutArithmeticOverflow` | Payout math overflow |
| 140 | `LegalHoldBlocksCancelFunding` | `cancel_funding()` with active legal hold |
| 141 | `CancelFundingNotOpen` | `cancel_funding()` when `status != 0` |
| 142 | `RefundNotCancelled` | `refund()` when `status != 4` |
| 143 | `NoContributionToRefund` | `refund()` with zero contribution |
| 165 | `InsufficientContractBalance` | `withdraw()` contract balance < `funded_amount` |
| 200 | `PartialSettleUnauthorizedCaller` | `partial_settle()` by non-SME/non-Admin |
| 201 | `LegalHoldBlocksPartialSettle` | `partial_settle()` with active legal hold |
| 202 | `PartialSettleNotOpen` | `partial_settle()` when `status != 0` |
| 211 | `PausedBlocksSettlement` | `settle()` when paused |
| 212 | `PausedBlocksWithdrawal` | `withdraw()` when paused |
| 213 | `PausedBlocksInvestorClaims` | `claim_investor_payout()` when paused |
| 30 | `LegalHoldBlocksTreasuryDustSweep` | `sweep_terminal_dust()` with active legal hold |
| 33 | `DustSweepNotTerminal` | `sweep_terminal_dust()` on non-terminal status |
| 42 | `SweepExceedsLiabilityFloor` | Sweep would leave balance < outstanding refunds |

---

## Worked Example: Full Settlement Flow

### Setup
```rust
// 1. Admin initializes escrow
let admin = Address::generate(&env);
let sme = Address::generate(&env);
let treasury = Address::generate(&env);
let token = Address::generate(&env);

client.init(
    &admin,
    &String::from_str(&env, "INV-001"),
    &sme,
    &1_000_000i128,  // funding_target
    &500i64,         // yield_bps (5%)
    &0u64,           // maturity (0 = no time lock)
    &token,
    &None,
    &treasury,
    &None, &None, &None, &None, &None, &None, &None, &None::<i64>
);
```

### Investor Funds
```rust
// 2. Investor contributes
let investor = Address::generate(&env);
token_client.mint(&investor, &1_000_000i128);
client.fund(&investor, &1_000_000i128);
// status transitions 0 → 1 (Funded), FundingCloseSnapshot written
```

### Settlement
```rust
// 3. SME calls settle (no maturity lock)
client.settle();  // requires sme auth
// status transitions 1 → 2 (Settled)
// EscrowSettled event emitted with settle_pool = 1_050_000 (1_000_000 + 50_000 coupon)
```

### Investor Claims Payout
```rust
// 4. Investor claims their pro-rata share
client.claim_investor_payout(&investor);  // requires investor auth
// InvestorPayoutClaimed event emitted
// InvestorClaimed flag set to true
```

### Alternative: SME Withdraws Instead
```rust
// 3b. SME withdraws liquidity (alternative to settle)
client.withdraw();  // requires sme auth
// status transitions 1 → 3 (Withdrawn)
// SmeWithdrew event emitted with amount = funded_amount - fee
```

### Alternative: Admin Cancels (before funding complete)
```rust
// 3c. Admin cancels before funding target met
client.cancel_funding();  // requires admin auth
// status transitions 0 → 4 (Cancelled)
// FundingCancelled event emitted

// 4c. Investors refund
client.refund(&investor);  // requires investor auth
```

---

## Key Invariants

1. **Mutual exclusivity of SME terminal paths**: From `status == 1`, only one of `settle()` or `withdraw()` can succeed. After either, `status != 1` so the other is blocked.

2. **Legal hold is a global block**: When `LegalHold == true`, all settlement-related entrypoints except `refund()` are blocked.

3. **Operational pause is narrower**: Pause blocks `settle`, `withdraw`, `claim_investor_payout`, and `fund` — but **not** `partial_settle`, `cancel_funding`, `refund`, or `sweep_terminal_dust`.

4. **Admin vs SME authority**: 
   - Admin: `cancel_funding`, `partial_settle`, `set_legal_hold`, `set_paused`, `sweep_terminal_dust` (via treasury)
   - SME: `partial_settle`, `settle`, `withdraw`, `rotate_beneficiary`

5. **Checks-effects-interactions**: All fund-moving operations (`withdraw`, `claim_investor_payout`, `refund`, `sweep_terminal_dust`) update state **before** external token transfers.

6. **Idempotency**: `claim_investor_payout` and `refund` are idempotent — second call with same parameters is a no-op (for claim) or fails with typed error (for refund with zero contribution).

---

## Cross-References

- State machine: [`docs/STATE_MACHINE_IMPLEMENTATION.md`](STATE_MACHINE_IMPLEMENTATION.md)
- Events: [`docs/escrow-events.md`](escrow-events.md)
- Errors: [`docs/escrow-error-messages.md`](escrow-error-messages.md)
- Legal hold: [`docs/escrow-legal-hold.md`](escrow-legal-hold.md)
- Cancellation & refunds: [`docs/escrow-cancellation-refunds.md`](escrow-cancellation-refunds.md)
