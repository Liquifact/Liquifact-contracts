# Collateral Error Integration Guide

This guide is the SDK integration companion to [`docs/collateral-errors.md`](collateral-errors.md). It provides:

- A quick-reference table with all collateral-related `EscrowError` codes
- Language-specific typed-error patterns (JavaScript/TypeScript, Python, Rust)
- Decision trees for the two mutating entrypoints
- Common anti-patterns and how to avoid them

For the raw code-to-variant mapping, see [`docs/collateral-errors.md`](collateral-errors.md). For auth rules, see [`docs/collateral-auth.md`](collateral-auth.md).

---

## Quick Reference

| Code | Variant | Entrypoint(s) | Trigger summary |
|---:|---|---|---|
| 20 | `EscrowNotInitialized` | `record`, `clear` | Escrow storage absent |
| 60 | `CollateralAmountNotPositive` | `record` | `amount <= 0` |
| 61 | `CollateralAssetEmpty` | `record` | empty `asset` symbol |
| 62 | `CollateralTimestampBackwards` | `record` (replace) | `ledger.timestamp() < prior.recorded_at` |
| 169 | `NoCollateralToClear` | `clear` | no pledge stored (checked before auth) |

> **Note:** Codes 60–62 require successful SME auth before they can be raised (except the amount/asset gates which fire first). Code 169 fires **before** auth so callers get an informative error even without a signature.

Non-SME callers will hit a **host authorization trap** (not a typed code) for `record` and `clear`.

---

## Guard Ordering

Knowing the exact guard order lets you diagnose the *first* error that will fire:

### `record_sme_collateral_commitment(asset, amount)`

```
1. amount > 0                     → else CollateralAmountNotPositive (60)
2. asset != "" (non-empty symbol) → else CollateralAssetEmpty (61)
3. SME require_auth               → else host trap (no typed code)
4. escrow initialized             → else EscrowNotInitialized (20)
5. timestamp monotonic (replace)  → else CollateralTimestampBackwards (62)
6. write + event
```

### `clear_sme_collateral_commitment()`

```
1. pledge exists in storage       → else NoCollateralToClear (169)
2. SME require_auth               → else host trap
3. escrow initialized             → else EscrowNotInitialized (20)
4. remove + event
```

---

## JavaScript / TypeScript Integration

### Typed error classifier

```typescript
export const COLLATERAL_ERRORS = {
  EscrowNotInitialized:          20,
  CollateralAmountNotPositive:   60,
  CollateralAssetEmpty:          61,
  CollateralTimestampBackwards:  62,
  NoCollateralToClear:          169,
} as const;

export type CollateralErrorCode =
  (typeof COLLATERAL_ERRORS)[keyof typeof COLLATERAL_ERRORS];

function parseContractError(err: unknown): number | null {
  const msg = String(err);
  const match = msg.match(/contract error (\d+)/i)
             ?? msg.match(/Error\(Contract, #(\d+)\)/);
  return match ? parseInt(match[1], 10) : null;
}

export function classifyCollateralError(
  err: unknown
): keyof typeof COLLATERAL_ERRORS | "UNKNOWN" | "AUTH_TRAP" {
  const code = parseContractError(err);
  if (code === null) return "AUTH_TRAP";
  for (const [name, c] of Object.entries(COLLATERAL_ERRORS)) {
    if (c === code) return name as keyof typeof COLLATERAL_ERRORS;
  }
  return "UNKNOWN";
}
```

### `recordSmeCollateral` with typed error handling

```typescript
async function recordSmeCollateral(
  escrow: LiquifactEscrowClient,
  asset: string,
  amount: bigint
): Promise<SmeCollateralCommitment> {
  try {
    return await escrow.recordSmeCollateralCommitment({ asset, amount });
  } catch (err) {
    switch (classifyCollateralError(err)) {
      case "CollateralAmountNotPositive":
        throw new Error(`amount must be > 0, got ${amount}`);
      case "CollateralAssetEmpty":
        throw new Error("asset symbol must not be empty");
      case "EscrowNotInitialized":
        throw new Error("escrow not yet initialized — call init() first");
      case "CollateralTimestampBackwards":
        throw new Error(
          "ledger time is behind the stored commitment timestamp; " +
          "wait for ledger to advance"
        );
      case "AUTH_TRAP":
        throw new Error("caller is not the configured SME address");
      default:
        throw err;
    }
  }
}
```

### `clearSmeCollateral` with typed error handling

```typescript
async function clearSmeCollateral(
  escrow: LiquifactEscrowClient
): Promise<void> {
  try {
    await escrow.clearSmeCollateralCommitment();
  } catch (err) {
    switch (classifyCollateralError(err)) {
      case "NoCollateralToClear":
        // idempotent: nothing to clear is not an error for most callers
        return;
      case "EscrowNotInitialized":
        throw new Error("escrow not yet initialized");
      case "AUTH_TRAP":
        throw new Error("caller is not the configured SME address");
      default:
        throw err;
    }
  }
}
```

---

## Python Integration

```python
import re

COLLATERAL_ERRORS = {
    20:  "EscrowNotInitialized",
    60:  "CollateralAmountNotPositive",
    61:  "CollateralAssetEmpty",
    62:  "CollateralTimestampBackwards",
    169: "NoCollateralToClear",
}

def classify_collateral_error(exc: Exception) -> str:
    msg = str(exc)
    m = re.search(r"contract error (\d+)", msg, re.I) or \
        re.search(r"Error\(Contract, #(\d+)\)", msg)
    if not m:
        return "AUTH_TRAP"
    code = int(m.group(1))
    return COLLATERAL_ERRORS.get(code, f"UNKNOWN({code})")

def record_sme_collateral(escrow, asset: str, amount: int):
    try:
        return escrow.record_sme_collateral_commitment(asset=asset, amount=amount)
    except Exception as e:
        kind = classify_collateral_error(e)
        if kind == "CollateralAmountNotPositive":
            raise ValueError(f"amount must be > 0, got {amount}")
        elif kind == "CollateralAssetEmpty":
            raise ValueError("asset symbol must not be empty")
        elif kind == "EscrowNotInitialized":
            raise RuntimeError("escrow not initialized")
        elif kind == "CollateralTimestampBackwards":
            raise RuntimeError("ledger timestamp is behind stored commitment")
        elif kind == "AUTH_TRAP":
            raise PermissionError("caller is not the configured SME address")
        raise
```

---

## Rust Integration

```rust
fn assert_collateral_error(
    result: Result<Result<impl std::fmt::Debug, impl std::fmt::Debug>,
                   Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
    expected: EscrowError,
) {
    let code = expected as u32;
    match result {
        Err(Ok(e)) => assert_eq!(e, soroban_sdk::Error::from_contract_error(code)),
        Err(Err(soroban_sdk::InvokeError::Contract(c))) => assert_eq!(c, code),
        other => panic!("expected ContractError({code}), got {other:?}"),
    }
}

// Usage examples:

#[test]
fn reject_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    // ... setup ...
    let asset = Symbol::new(&env, "USDC");
    assert_collateral_error(
        client.try_record_sme_collateral_commitment(&asset, &0i128),
        EscrowError::CollateralAmountNotPositive,
    );
}

#[test]
fn reject_empty_asset() {
    let env = Env::default();
    env.mock_all_auths();
    // ... setup ...
    let empty = Symbol::new(&env, "");
    assert_collateral_error(
        client.try_record_sme_collateral_commitment(&empty, &1_000i128),
        EscrowError::CollateralAssetEmpty,
    );
}

#[test]
fn clear_with_no_commitment_returns_typed_error_before_auth() {
    let env = Env::default();
    // No mock_all_auths — auth should NOT be reached.
    // ... setup ...
    assert_collateral_error(
        client.try_clear_sme_collateral_commitment(),
        EscrowError::NoCollateralToClear,
    );
}
```

---

## Decision Trees

### Should I call `record_sme_collateral_commitment`?

```
Do I have the SME key available?  ──No──▶  Don't call — will host-trap.
         │
        Yes
         │
         ▼
Is the escrow initialized?  ──No──▶  Call init() first → then record.
         │
        Yes
         │
         ▼
Is amount > 0 and asset non-empty?  ──No──▶  Fix inputs.
         │
        Yes
         │
         ▼
Is this replacing an existing commitment?  ──No──▶  Safe to call.
         │
        Yes
         │
         ▼
Is ledger.timestamp() >= prior.recorded_at?  ──No──▶  Wait for ledger to advance.
         │
        Yes
         │
         ▼
   Call record_sme_collateral_commitment()
```

### Should I call `clear_sme_collateral_commitment`?

```
Is there an active commitment?  ──No──▶  NoCollateralToClear (169). Skip or handle as no-op.
  (check get_sme_collateral_commitment() first)
         │
        Yes
         │
         ▼
Do I have the SME key?  ──No──▶  Don't call — will host-trap.
         │
        Yes
         │
         ▼
   Call clear_sme_collateral_commitment()
```

---

## Anti-Patterns

| Anti-pattern | Problem | Fix |
|---|---|---|
| Calling `clear` without checking for commitment | Gets `NoCollateralToClear` on every cold call | Call `get_sme_collateral_commitment()` first; skip clear if `None` |
| Passing `amount = 0` to `record` | `CollateralAmountNotPositive` | Validate `amount > 0` client-side |
| Passing an empty `asset` symbol | `CollateralAssetEmpty` | Validate non-empty before calling |
| Retrying after `CollateralTimestampBackwards` immediately | Same error | Wait for ledger to advance past `prior.recorded_at` |
| Non-SME calling `record` / `clear` | Host auth trap — no typed code | Always sign with the escrow's SME address |

---

## Stability Guarantee

All error codes listed here are **stable and append-only**. Codes will not be reassigned. New codes may be added in future versions but existing codes will not change semantics.

---

## See Also

- [`docs/collateral-errors.md`](collateral-errors.md) — raw code reference
- [`docs/collateral-auth.md`](collateral-auth.md) — authorization rules and guard ordering
- [`docs/collateral-config-view.md`](collateral-config-view.md) — read-only config view
- [`docs/escrow-error-messages.md`](escrow-error-messages.md) — full contract error registry
