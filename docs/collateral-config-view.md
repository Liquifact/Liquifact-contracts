# Collateral Configuration View — `get_collateral_config()`

## Overview

The `get_collateral_config()` entrypoint exposes a **read-only snapshot** of the collateral subsystem configuration. It returns a `CollateralConfig` struct containing the collateral ceiling and the current SME commitment, and is safe to call at any time — including before `init` — returning sensible defaults.

Source of truth: `escrow/src/lib.rs`.

---

## Why a Config View?

Callers previously had to read two separate storage keys (`get_collateral_limit()` and `get_sme_collateral_commitment()`) and merge the results client-side. This is error-prone:

- A storage update between the two reads can produce an inconsistent snapshot.
- Each call incurs separate ledger I/O.

`get_collateral_config()` reads both values in a single atomic call, returning a self-consistent struct.

---

## Entrypoint Signature

```rust
pub fn get_collateral_config(env: Env) -> CollateralConfig
```

| Property | Value |
|---|---|
| Auth required | **None** — ungated read |
| Escrow state required | **None** — valid before `init` |
| Idempotent | Yes |
| Emits event | No |

---

## Return Type: `CollateralConfig`

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollateralConfig {
    /// Maximum amount an SME may commit as collateral.
    /// Defaults to `MAX_INVOICE_AMOUNT` before any admin override via `set_collateral_limit`.
    pub collateral_limit: i128,

    /// Current SME collateral commitment, if any.
    /// `CollateralCommitmentSnapshot::None` before the first `record_sme_collateral_commitment`,
    /// or after `clear_sme_collateral_commitment`.
    pub sme_commitment: CollateralCommitmentSnapshot,
}
```

### `CollateralCommitmentSnapshot`

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CollateralCommitmentSnapshot {
    None,
    Some(SmeCollateralCommitment),
}
```

### `SmeCollateralCommitment`

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmeCollateralCommitment {
    pub asset: Symbol,
    pub amount: i128,
    pub recorded_at: u64,
}
```

---

## Default Values

| Field | Before `init` | After `init`, no mutations |
|---|---|---|
| `collateral_limit` | `MAX_INVOICE_AMOUNT` | `MAX_INVOICE_AMOUNT` |
| `sme_commitment` | `CollateralCommitmentSnapshot::None` | `CollateralCommitmentSnapshot::None` |

Calling `get_collateral_config()` before `init` does **not** panic or return an error. This allows dashboards and explorers to read the config at any time.

---

## State Transitions That Affect the Config

| Action | Effect on `CollateralConfig` |
|---|---|
| Contract deployed (pre-`init`) | Both fields at defaults |
| `init(...)` called | Fields remain at defaults unless overridden |
| `set_collateral_limit(limit)` | `collateral_limit` updated; `sme_commitment` unchanged |
| `record_sme_collateral_commitment(asset, amount)` | `sme_commitment` becomes `Some(...)` |
| Subsequent `record_sme_collateral_commitment(...)` | `sme_commitment.Some.amount` updated (replace, not append) |
| `clear_sme_collateral_commitment()` | `sme_commitment` reverts to `None` |

---

## Relationship to Individual Getters

`get_collateral_config()` is **always consistent** with the pair:

```text
config.collateral_limit  == get_collateral_limit()
config.sme_commitment    == get_sme_collateral_commitment()  (lifted to CollateralCommitmentSnapshot)
```

For any single call, both sides are read from the same ledger snapshot. Use the composite view in preference to the individual getters when you need both values together.

---

## Integration Examples

### JavaScript / TypeScript

```typescript
const config = await escrow.getCollateralConfig();

console.log("Collateral ceiling:", config.collateralLimit.toString());

switch (config.smeCommitment.tag) {
  case "Some":
    const { asset, amount, recordedAt } = config.smeCommitment.values[0];
    console.log(`Commitment: ${asset} × ${amount} (recorded at ledger ts ${recordedAt})`);
    break;
  case "None":
    console.log("No active commitment.");
    break;
}
```

### Python (stellar-sdk)

```python
config = escrow.get_collateral_config()

print(f"Limit: {config.collateral_limit}")

if config.sme_commitment["tag"] == "Some":
    c = config.sme_commitment["values"][0]
    print(f"Commitment: {c['asset']} × {c['amount']} at t={c['recorded_at']}")
else:
    print("No commitment.")
```

### Rust test helper

```rust
fn assert_no_commitment(client: &LiquifactEscrowClient<'_>) {
    let cfg = client.get_collateral_config();
    assert_eq!(cfg.sme_commitment, CollateralCommitmentSnapshot::None);
}

fn assert_commitment_amount(client: &LiquifactEscrowClient<'_>, expected: i128) {
    let cfg = client.get_collateral_config();
    match cfg.sme_commitment {
        CollateralCommitmentSnapshot::Some(c) => assert_eq!(c.amount, expected),
        CollateralCommitmentSnapshot::None => panic!("no commitment"),
    }
}
```

---

## Authorization and State Rules

`get_collateral_config()` is **fully ungated**:

- Requires no signer.
- Can be called in any escrow status (open, funded, settled, withdrawn, cancelled).
- Is not gated by legal hold or operational pause.
- Works identically before and after `init`.

---

## Covered by Tests

| Scenario | File |
|---|---|
| Defaults before `init` | `escrow/src/tests/collateral_config_view.rs` |
| Defaults after `init` | `escrow/src/tests/collateral_config_view.rs` |
| Consistency with individual getters | `escrow/src/tests/collateral_config_view.rs` |
| Boundary — `i128::MAX` limit | `escrow/src/tests/collateral_config_view.rs` |
| `Some`/`None` transitions | `escrow/src/tests/collateral_struct_ret.rs` |
| Multiple limit updates | `escrow/src/tests/collateral_boundary_tests.rs` |

---

## See Also

- [`docs/collateral-auth.md`](collateral-auth.md) — authorization rules for mutating entrypoints
- [`docs/collateral-errors.md`](collateral-errors.md) — typed error codes for collateral operations
- [`docs/collateral-struct-return.md`](collateral-struct-return.md) — named struct return rationale
- [`docs/escrow-read-api.md`](escrow-read-api.md) — full read entrypoint inventory
- [`docs/escrow-sme-collateral.md`](escrow-sme-collateral.md) — collateral functional model
