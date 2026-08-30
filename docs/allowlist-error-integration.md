# Allowlist Error Codes — SDK Integration Guide

This document supplements [`docs/allowlist-errors.md`](allowlist-errors.md) with concrete
typed-error handling patterns for every SDK that consumes Liquifact Escrow contract calls.

All codes are **stable and append-only** — never branch on panic-string text; always branch
on the numeric `ContractError(code)` value.

---

## Quick reference

| Code | Variant | Entrypoint(s) | Meaning |
|-----:|---------|---------------|---------|
| 70 | `InvestorBatchEmpty` | `set_investors_allowlisted` | Zero-length investor batch rejected |
| 71 | `InvestorBatchTooLarge` | `set_investors_allowlisted` | Batch exceeds `MAX_INVESTOR_ALLOWLIST_BATCH` (32) |
| 104 | `InvestorNotAllowlisted` | `fund`, `fund_with_commitment` | Allowlist gate active and investor absent |

---

## Handling patterns by SDK

### JavaScript / TypeScript (Stellar SDK)

```typescript
import { Contract, SorobanRpc, xdr } from '@stellar/stellar-sdk';

const ALLOWLIST_ERROR_CODES: Record<number, string> = {
  70: 'InvestorBatchEmpty',
  71: 'InvestorBatchTooLarge',
  104: 'InvestorNotAllowlisted',
};

function classifyAllowlistError(err: unknown): string | null {
  // Soroban contract errors are surfaced as simulation failures containing
  // a ContractError XDR value with a numeric code.
  if (err instanceof Error && err.message.includes('Error(Contract,')) {
    const match = err.message.match(/Error\(Contract,\s*#?(\d+)\)/);
    if (match) {
      const code = parseInt(match[1], 10);
      return ALLOWLIST_ERROR_CODES[code] ?? null;
    }
  }
  return null;
}

// Usage in a fund-gating flow:
async function fundEscrow(amount: bigint, investor: string): Promise<void> {
  try {
    await escrowContract.fund({ investor, amount }).simulate();
  } catch (err) {
    const variant = classifyAllowlistError(err);
    if (variant === 'InvestorNotAllowlisted') {
      throw new Error(
        `Investor ${investor} is not on the allowlist. Ask the admin to call set_investor_allowlisted.`
      );
    }
    throw err;
  }
}
```

### Python (Stellar Python SDK)

```python
from stellar_sdk.exceptions import SorobanRpcErrorResponse
import re

ALLOWLIST_CODES = {
    70: "InvestorBatchEmpty",
    71: "InvestorBatchTooLarge",
    104: "InvestorNotAllowlisted",
}

def classify_allowlist_error(err: Exception) -> str | None:
    match = re.search(r'Error\(Contract,\s*#?(\d+)\)', str(err))
    if match:
        return ALLOWLIST_CODES.get(int(match.group(1)))
    return None

def fund_escrow(client, investor: str, amount: int) -> dict:
    try:
        return client.fund(investor=investor, amount=amount)
    except Exception as err:
        variant = classify_allowlist_error(err)
        if variant == "InvestorNotAllowlisted":
            raise ValueError(
                f"Investor {investor} is not allowlisted. "
                "Call set_investor_allowlisted before funding."
            ) from err
        raise
```

### Rust integration tests (Soroban testutils)

```rust
use soroban_sdk::{Error, InvokeError};
use liquifact_escrow::EscrowError;

/// Assert that a `try_*` call returns the expected contract error code.
fn assert_allowlist_error<T, E: std::fmt::Debug>(
    result: Result<Result<T, E>, Result<Error, InvokeError>>,
    expected: EscrowError,
) {
    let code = expected as u32;
    match result {
        Err(Ok(err)) => assert_eq!(err, Error::from_contract_error(code)),
        Err(Err(InvokeError::Contract(c))) => assert_eq!(c, code),
        other => panic!("expected ContractError({code}), got {other:?}"),
    }
}

#[test]
fn investor_not_allowlisted_returns_code_104() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy_and_init(&env);
    let uninvited = Address::generate(&env);

    client.set_allowlist_active(&true);
    // uninvited not in allowlist
    assert_allowlist_error(
        client.try_fund(&uninvited, &1_000i128),
        EscrowError::InvestorNotAllowlisted, // code 104
    );
}

#[test]
fn batch_empty_returns_code_70() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy_and_init(&env);

    let v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    assert_allowlist_error(
        client.try_set_investors_allowlisted(&v, &true),
        EscrowError::InvestorBatchEmpty, // code 70
    );
}

#[test]
fn batch_too_large_returns_code_71() {
    use liquifact_escrow::MAX_INVESTOR_ALLOWLIST_BATCH;
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy_and_init(&env);

    let mut v: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..=(MAX_INVESTOR_ALLOWLIST_BATCH as usize) {
        v.push_back(Address::generate(&env));
    }
    assert_allowlist_error(
        client.try_set_investors_allowlisted(&v, &true),
        EscrowError::InvestorBatchTooLarge, // code 71
    );
}
```

---

## Decision tree for `fund` / `fund_with_commitment` callers

```
call fund(investor, amount)
         |
         v
  ContractError(104)? ─── yes ──► investor not on allowlist
         |                        → call set_investor_allowlisted(investor, true) [admin]
         | no                     → retry fund(investor, amount)
         v
  success or other error
```

## Decision tree for `set_investors_allowlisted` callers

```
call set_investors_allowlisted(batch, true)
         |
         v
  ContractError(70)? ─── yes ──► batch is empty; supply at least 1 investor
         |
  ContractError(71)? ─── yes ──► batch > 32; split into chunks of ≤ 32
         |
  success or other error
```

---

## Batch size limit constants

| Constant | Value | Meaning |
|----------|------:|---------|
| `MAX_INVESTOR_ALLOWLIST_BATCH` | 32 | Hard upper bound on `set_investors_allowlisted` per call |

Split batches larger than 32 into pages of ≤ 32 and submit one transaction per page.

---

## Stability guarantee

Codes 70, 71, and 104 are **append-only and will never be renumbered or reassigned**.
New allowlist-related failures will receive new codes. See
[`docs/escrow-error-messages.md`](escrow-error-messages.md) for the full contract-wide table.

## Cross-references

- [`docs/allowlist-errors.md`](allowlist-errors.md) — full error code reference with trigger conditions
- [`docs/allowlist.md`](allowlist.md) — allowlist feature overview
- [`docs/allowlist-auth.md`](allowlist-auth.md) — authorization rules
- [`docs/allowlist-states.md`](allowlist-states.md) — state machine
