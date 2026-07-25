# Fees Model

This document describes the Liquifact escrow's protocol fee model, its invariants, and the entrypoints that interact with it.

## Overview

The protocol fee is an **immutable** split applied at SME withdrawal time. A percentage of the funded principal is routed to a treasury address, while the remainder goes to the SME. The fee is configured at escrow initialization and cannot be changed thereafter.

## Data Model

### Storage Keys

| Key | Type | Immutability | Description |
|-----|------|--------------|-------------|
| `DataKey::ProtocolFeeBps` | `i64` | Immutable | Protocol fee in basis points (0..=10_000). Default: 0 (no fee). |
| `DataKey::Treasury` | `Address` | Immutable | Treasury address that receives fees and dust sweeps. |

### Fee Calculation

At `withdraw` time, the funded principal is split using integer arithmetic:

```text
fee        = funded_amount * protocol_fee_bps / 10_000   (floor, checked)
sme_payout = funded_amount - fee                          (checked)
```

- **Division is floor**: any residue below one 10,000th stays with the SME (never over-charges the treasury).
- **Arithmetic is checked**: overflow/underflow panics with typed errors.
- **Conservation invariant**: `sme_payout + fee == funded_amount` for every withdrawal.

### Basis Points Range

- Valid range: `0..=10_000` (0% to 100%).
- `0` = no fee (full `funded_amount` goes to SME, no treasury transfer).
- `10_000` = 100% fee (entire `funded_amount` goes to treasury, SME receives nothing).

## Invariants

1. **Immutability**: `ProtocolFeeBps` and `Treasury` are set once at `init` and never mutated.
2. **Conservation**: The sum of SME payout and treasury fee always equals the gross `funded_amount`.
3. **Floor rounding**: Rounding residue always favors the SME, never the treasury.
4. **Zero-fee path**: When `protocol_fee_bps == 0`, the treasury transfer is skipped entirely (preserves legacy gas profile).
5. **Legacy compatibility**: Instances predating `ProtocolFeeBps` read as `0` (additive-key default), matching pre-fee behavior.

## Entrypoints

### `LiquifactEscrow::init`

**Sets the fee model (immutable).**

```rust
pub fn init(
    env: Env,
    admin: Address,
    invoice_id: String,
    sme_address: Address,
    amount: i128,
    yield_bps: i64,
    maturity: u64,
    funding_token: Address,
    registry: Option<Address>,
    treasury: Address,
    yield_tiers: Option<Vec<YieldTier>>,
    min_contribution: Option<i128>,
    max_unique_investors: Option<u32>,
    max_per_investor: Option<i128>,
    legal_hold_clear_delay: Option<u64>,
    maturity_max_horizon: Option<u64>,
    funding_deadline: Option<u64>,
    allowlist_active: Option<bool>,
    protocol_fee_bps: Option<i64>,
) -> InvoiceEscrow
```

**Validation:**
- `protocol_fee_bps` must be in `0..=10_000` → `EscrowError::ProtocolFeeBpsOutOfRange` (code 215).
- Default value is `0` when `None` is passed.

**Storage writes:**
- `DataKey::ProtocolFeeBps ← protocol_fee_bps` (always written, even when `0`).
- `DataKey::Treasury ← treasury`.

### `LiquifactEscrow::withdraw`

**Applies the fee split and transfers tokens.**

```rust
pub fn withdraw(env: Env) -> InvoiceEscrow
```

**Guard ordering (canonical):**
1. Operational pause check (read-only).
2. Legal hold check (read-only).
3. `sme_address.require_auth()`.
4. Status check (must be `1` = funded).
5. Contract balance sufficiency check (`balance >= funded_amount`).
6. Fee/net computation (checked arithmetic).
7. Status transition to `3` (withdrawn).
8. Token transfers (fee → treasury, net → SME).
9. Event emission.

**Fee computation:**
```rust
let fee_bps: i64 = env.storage().instance().get(&DataKey::ProtocolFeeBps).unwrap_or(0);
let fee: i128 = amount
    .checked_mul(fee_bps as i128)
    .and_then(|scaled| scaled.checked_div(10_000))
    .unwrap_or_else(|| fail(&env, EscrowError::WithdrawFeeArithmeticOverflow));
let net: i128 = amount
    .checked_sub(fee)
    .unwrap_or_else(|| fail(&env, EscrowError::WithdrawNetArithmeticUnderflow));
```

**Token transfers:**
- Treasury transfer skipped when `fee == 0` (optimization for zero-fee case).
- SME transfer skipped when `net == 0` (degenerate `fee_bps == _000` case).
- Both transfers use SEP-41 balance-delta verification via `external_calls::transfer_funding_token_with_balance_checks`.

**Errors:**
- `EscrowError::WithdrawFeeArithmeticOverflow` (216) — `funded_amount * fee_bps` overflowed.
- `EscrowError::WithdrawNetArithmeticUnderflow` (217) — `funded_amount - fee` underflowed (unreachable for in-range `fee_bps`).

**Event emitted:**
```rust
SmeWithdrew {
    name: symbol_short!("sme_wd"),
    invoice_id: escrow.invoice_id.clone(),
    amount: net,        // SME receives
    recipient: sme,
    fee,                // Treasury receives
}
```

### `LiquifactEscrow::sweep_terminal_dust`

**Transfers dust to treasury (separate from fee flow).**

```rust
pub fn sweep_terminal_dust(env: Env, amount: i128) -> i128
```

This entrypoint moves stray token balances to the treasury in terminal states (settled, withdrawn, cancelled). It is **not** part of the fee split logic but shares the same treasury recipient.

**Key differences from fee flow:**
- Callable in terminal states only (status ∈ {2, 3, 4}).
- Capped by `MAX_DUST_SWEEP_AMOUNT` (100,000,000 base units).
- Respects liability floor: `balance - sweep_amt >= funded_amount - distributed_principal`.
- Blocked by legal hold.

**Event emitted:**
```rust
TreasuryDustSwept {
    name: symbol_short!("dust_sw"),
    invoice_id: escrow.invoice_id.clone(),
    recipient: treasury,
    funding_token: token_addr,
    amount: sweep_amt,
}
```

### Read-only Getters

#### `LiquifactEscrow::get_protocol_fee_bps`

```rust
pub fn get_protocol_fee_bps(env: Env) -> i64
```

Returns the stored protocol fee in basis points. Reads `0` for instances predating the key (additive-key default).

#### `LiquifactEscrow::get_treasury`

```rust
pub fn get_treasury(env: Env) -> Address
```

Returns the immutable treasury address. Panics with `EscrowError::TreasuryNotSet` (22) if called before init.

## Worked Example

### Scenario

- Invoice target: `1,000,000` USDC (6 decimals)
- Funded amount: `1,000,000` USDC (exact target)
- Protocol fee: `250` bps (2.5%)
- Treasury: `0xTREASURY...`
- SME: `0xSME...`

### Initialization

```rust
client.init(
    &admin,
    &"INV-001".into(),
    &sme,
    &1_000_000i128,
    &500i64,              // yield_bps
    &maturity_timestamp,
    &usdc_token,
    &None,                // registry
    &treasury,
    &None,                // yield_tiers
    &None,                // min_contribution
    &None,                // max_unique_investors
    &None,                // max_per_investor
    &None,                // legal_hold_clear_delay
    &None,                // maturity_max_horizon
    &None,                // funding_deadline
    &None,                // allowlist_active
    &Some(250i64),        // protocol_fee_bps = 2.5%
);
```

Storage after init:
- `DataKey::ProtocolFeeBps = 250`
- `DataKey::Treasury = 0xTREASURY...`

### Withdrawal

When the SME calls `withdraw`:

```rust
// Fee computation
fee = 1_000_000 * 250 / 10_000
    = 250,000,000 / 10_000
    = 25,000  (floor division)

sme_payout = 1_000,000 - 25,000
           = 975,000
```

**Token transfers:**
- Treasury receives: `25,000` USDC
- SME receives: `975,000` USDC

**Conservation check:**
```
975,000 + 25,000 = 1,000,000 ✓
```

**Event emitted:**
```rust
SmeWithdrew {
    invoice_id: "INV-001",
    amount: 975_000,
    recipient: 0xSME...,
    fee: 25_000,
}
```

### Edge Cases

#### Zero fee (`protocol_fee_bps = 0`)

```rust
fee = 1_000,000 * 0 / 10_000 = 0
sme_payout = 1_000,000 - 0 = 1_000,000
```

- Treasury transfer skipped (gas optimization).
- SME receives full `funded_amount`.
- Event shows `fee = 0`.

#### 100% fee (`protocol_fee_bps = 10_000`)

```rust
fee = 1_000,000 * 10_000 / 10_000 = 1_000,000
sme_payout = 1_000,000 - 1_000,000 = 0
```

- SME transfer skipped (net is zero).
- Treasury receives full `funded_amount`.
- Event shows `amount = 0`, `fee = 1_000,000`.

#### Rounding residue

With `funded_amount = 1,000,001` and `protocol_fee_bps = 250`:

```rust
fee = 1,000,001 * 250 / 10_000
    = 250,000,250 / 10_000
    = 25,000  (floor, remainder = 250)

sme_payout = 1,000,001 - 25,000
           = 975,001
```

- Treasury receives `25,000` (short by 0.025 USDC due to floor).
- SME receives `975,001` (includes the 0.025 residue).
- Conservation: `975,001 + 25,000 = 1,000,001` ✓

## Cross-Reference to Source Code

### Fee storage and validation
- `DataKey::ProtocolFeeBps`: [`escrow/src/lib.rs:877`](../escrow/src/lib.rs#L877)
- `DataKey::Treasury`: [`escrow/src/lib.rs:789`](../escrow/src/lib.rs#L789)
- Init validation: [`escrow/src/lib.rs:1824-1828`](../escrow/src/lib.rs#L1824-L1828)
- Init storage write: [`escrow/src/lib.rs:1874-1877`](../escrow/src/lib.rs#L1874-L1877)

### Fee computation and withdrawal
- Withdraw entrypoint: [`escrow/src/lib.rs:4348`](../escrow/src/lib.rs#L4348)
- Fee computation: [`escrow/src/lib.rs:4368-4379`](../escrow/src/lib.rs#L4368-L4379)
- Treasury transfer: [`escrow/src/lib.rs:4417-4426`](../escrow/src/lib.rs#L4417-L4426)
- SME transfer: [`escrow/src/lib.rs:4427-4435`](../escrow/src/lib.rs#L4427-L4435)
- Event emission: [`escrow/src/lib.rs:4437-4444`](../escrow/src/lib.rs#L4437-L4444)

### Read-only getters
- `get_protocol_fee_bps`: [`escrow/src/lib.rs:2343-2348`](../escrow/src/lib.rs#L2343-L2348)
- `get_treasury`: [`escrow/src/lib.rs:1988-1990`](../escrow/src/lib.rs#L1988-L1990)

### Dust sweep (treasury interaction)
- `sweep_terminal_dust`: [`escrow/src/lib.rs:2121`](../escrow/src/lib.rs#L2121)
- Treasury dust sweep event: [`escrow/src/lib.rs:1451`](../escrow/src/lib.rs#L1451)

### Error codes
- `ProtocolFeeBpsOutOfRange`: [`escrow/src/lib.rs:560`](../escrow/src/lib.rs#L560)
- `WithdrawFeeArithmeticOverflow`: [`escrow/src/lib.rs:562`](../escrow/src/lib.rs#L562)
- `WithdrawNetArithmeticUnderflow`: [`escrow/src/lib.rs:564`](../escrow/src/lib.rs#L564)
- `TreasuryNotSet`: [`escrow/src/lib.rs:317`](../escrow/src/lib.rs#L317)

### Events
- `SmeWithdrew`: [`escrow/src/lib.rs:1372`](../escrow/src/lib.rs#L1372)
- `TreasuryDustSwept`: [`escrow/src/lib.rs:1451`](../escrow/src/lib.rs#L1451)

## Testing

Fee-related tests are located in:
- `escrow/src/tests/init.rs` — init validation and getter tests
- `escrow/src/tests/external_calls.rs` — dust sweep liability floor tests
- `escrow/src/tests/legal_hold.rs` — legal hold blocking treasury operations
- `escrow/src/tests/integration.rs` — integration tests with fee expectations

Key test coverage:
- `test_get_treasury_before_init_fails_with_typed_error` — verifies `TreasuryNotSet`
- `test_get_treasury_after_init_succeeds` — verifies treasury persistence
- Liability floor tests in `external_calls.rs` — ensure dust sweeps respect outstanding liabilities
- Legal hold tests in `legal_hold.rs` — ensure holds block both fee transfers and dust sweeps

## Security Considerations

1. **Immutability**: The fee rate and treasury address cannot be changed after deployment. This prevents fee manipulation attacks.
2. **Floor rounding**: Rounding always favors the SME, preventing the protocol from silently extracting value via rounding errors.
3. **Checked arithmetic**: All fee computations use checked arithmetic to prevent overflow/underflow attacks.
4. **Balance verification**: Token transfers use SEP-41 balance-delta checks to reject fee-on-transfer or malicious tokens.
5. **Legal hold**: Both fee transfers and dust sweeps are blocked by legal holds, ensuring compliance gates apply to all treasury movements.
6. **Liability floor**: Dust sweeps cannot pull investor principal; the liability floor ensures `balance - sweep >= outstanding_liability`.

## Related Documentation

- [Escrow Numeric Model](./escrow-numeric-model.md) — integer arithmetic and overflow bounds
- [Escrow Token Integration Checklist](./ESCROW_TOKEN_INTEGRATION_CHECKLIST.md) — token safety and fee-on-transfer rejection
- [Escrow Legal Hold](./escrow-legal-hold.md) — compliance hold behavior
- [Operator Runbook](./OPERATOR_RUNBOOK.md) — operational guidance
