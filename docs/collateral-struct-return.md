# Collateral Struct Return — `get_collateral_config()`

## Overview

The `get_collateral_config()` entrypoint returns a **named `CollateralConfig` struct** rather than a raw tuple. This document describes the struct layout, the motivation for the named return type, and how to integrate against it.

Source of truth: `escrow/src/lib.rs` and `escrow/src/types.rs`.

---

## Background

Earlier versions of the collateral read API returned an opaque positional tuple for the configuration snapshot. Positional tuples make call-site code fragile:

```
// old, opaque — which field is which?
let (limit, commitment) = client.collateral();
```

The struct return makes intent explicit:

```rust
let config = client.get_collateral_config();
let limit = config.collateral_limit;   // named, typed, self-documenting
let commitment = config.sme_commitment; // sealed enum — exhaustively matchable
```

---

## `CollateralConfig` Struct

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollateralConfig {
    /// Current ceiling for SME commitment amounts.
    /// Defaults to `MAX_INVOICE_AMOUNT` before any admin override.
    pub collateral_limit: i128,

    /// Snapshot of the current SME collateral commitment, if any.
    /// `None` before the SME has called `record_sme_collateral_commitment`,
    /// or after `clear_sme_collateral_commitment`.
    pub sme_commitment: CollateralCommitmentSnapshot,
}
```

### `CollateralCommitmentSnapshot` enum

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CollateralCommitmentSnapshot {
    None,
    Some(SmeCollateralCommitment),
}
```

This mirrors `Option<SmeCollateralCommitment>` but is an explicit `#[contracttype]` enum so that it can be serialised and returned from a Soroban entrypoint.

---

## Entrypoint Reference

### `get_collateral_config`

```rust
pub fn get_collateral_config(env: Env) -> CollateralConfig
```

| Property | Value |
|---|---|
| Auth required | None (read-only) |
| State required | None — returns defaults before `init` |
| Returns | `CollateralConfig { collateral_limit, sme_commitment }` |
| Idempotent | Yes |

#### Default values (before `init` or `set_collateral_limit`)

| Field | Default |
|---|---|
| `collateral_limit` | `MAX_INVOICE_AMOUNT` |
| `sme_commitment` | `CollateralCommitmentSnapshot::None` |

---

## Individual Getter Consistency Guarantee

`get_collateral_config()` is a **composite view** — it is always consistent with the pair of individual getters:

```rust
assert_eq!(config.collateral_limit, client.get_collateral_limit());
assert_eq!(
    config.sme_commitment,
    match client.get_sme_collateral_commitment() {
        Some(c) => CollateralCommitmentSnapshot::Some(c),
        None    => CollateralCommitmentSnapshot::None,
    }
);
```

The composite view is always computed from the same storage read path; there is no caching or staleness window.

---

## Integration Example

### JavaScript / TypeScript

```typescript
const config = await escrow.getCollateralConfig();

console.log("Collateral limit:", config.collateralLimit.toString());

if (config.smeCommitment.tag === "Some") {
  const c = config.smeCommitment.values[0];
  console.log(`Commitment: ${c.asset} × ${c.amount} at t=${c.recordedAt}`);
} else {
  console.log("No active collateral commitment.");
}
```

### Python (stellar-sdk)

```python
config = escrow.get_collateral_config()
limit = config.collateral_limit

if config.sme_commitment["tag"] == "Some":
    c = config.sme_commitment["values"][0]
    print(f"Commitment: {c['asset']} × {c['amount']}")
else:
    print("No commitment.")
```

### Rust integration test pattern

```rust
let config = client.get_collateral_config();
assert_eq!(config.collateral_limit, expected_limit);
match config.sme_commitment {
    CollateralCommitmentSnapshot::Some(c) => {
        assert_eq!(c.amount, expected_amount);
    }
    CollateralCommitmentSnapshot::None => {
        panic!("expected a commitment to be present");
    }
}
```

---

## Migration from Tuple Return

If your call site used a positional tuple:

```rust
// Before (tuple)
let (limit, commitment_opt) = client.collateral();

// After (named struct)
let config = client.get_collateral_config();
let limit = config.collateral_limit;
let commitment_opt = match config.sme_commitment {
    CollateralCommitmentSnapshot::Some(c) => Some(c),
    CollateralCommitmentSnapshot::None    => None,
};
```

---

## Covered by Tests

| Test | Location |
|---|---|
| Field presence and type access | `escrow/src/tests/collateral_struct_ret.rs` |
| Structural equality | `escrow/src/tests/collateral_struct_ret.rs` |
| `Some`/`None` transitions | `escrow/src/tests/collateral_struct_ret.rs` |
| Consistency with individual getters | `escrow/src/tests/collateral_struct_ret.rs` |
| Pre-init defaults | `escrow/src/tests/collateral_struct_ret.rs` |
| Boundary / limit updates | `escrow/src/tests/collateral_config_view.rs` |

---

## See Also

- [`docs/escrow-sme-collateral.md`](escrow-sme-collateral.md) — functional collateral model
- [`docs/collateral-auth.md`](collateral-auth.md) — authorization rules for mutating entrypoints
- [`docs/collateral-errors.md`](collateral-errors.md) — typed error reference
- [`docs/escrow-read-api.md`](escrow-read-api.md) — full read entrypoint inventory
