# Collateral authorization and access rules

The LiquiFact escrow contract lets the SME record and clear **off-chain collateral pledge metadata**. This document is the authoritative, code-accurate reference for **who may call what**, in which state, the **exact guard ordering**, and the **operator-facing rejection codes**.

> **Scope:** metadata only. These entrypoints do **not** transfer tokens, reserve balances, verify custody, create an on-chain lien, or block `settle` / `withdraw` / `claim_investor_payout`. See [`docs/escrow-sme-collateral.md`](escrow-sme-collateral.md) for the functional model.

Source of truth: `escrow/src/lib.rs` (`record_sme_collateral_commitment`, `clear_sme_collateral_commitment`, `get_sme_collateral_commitment`).

---

## Roles

| Role | Address | Collateral authority |
| --- | --- | --- |
| **SME** | `InvoiceEscrow::sme_address` | Sole writer — may `record` and `clear` |
| **Anyone** | — | May `get_sme_collateral_commitment` / read via `get_escrow_summary` (no auth) |
| **Admin** | `InvoiceEscrow::admin` | **No** collateral entrypoints — admin cannot record or clear pledges |

Auth for mutating calls is enforced via `load_escrow_require_sme`, which loads `DataKey::Escrow` and calls `sme_address.require_auth()` (see [`docs/adr/ADR-002-auth-boundaries.md`](adr/ADR-002-auth-boundaries.md)).

Failed `require_auth` surfaces as a **host authorization trap**, not a typed `EscrowError`.

---

## Entrypoints

### `record_sme_collateral_commitment`

```rust
pub fn record_sme_collateral_commitment(
    env: Env,
    asset: Symbol,
    amount: i128,
) -> SmeCollateralCommitment
```

- **Auth:** SME (`load_escrow_require_sme`)
- **Storage:** writes `DataKey::SmeCollateralPledge` (`SmeCollateralCommitment { asset, amount, recorded_at }`)
- **Event:** `CollateralRecordedEvt` (`coll_rec`) with `amount` and `prior_amount` (`0` on first record)
- **Idempotency:** a later successful call **overwrites** the stored pledge (replace, not append)

### `clear_sme_collateral_commitment`

```rust
pub fn clear_sme_collateral_commitment(env: Env)
```

- **Auth:** SME (`load_escrow_require_sme`), after an existence check
- **Storage:** removes `DataKey::SmeCollateralPledge`
- **Events:** publishes both `CollateralClearedEvt` and `CollateralCommitmentCleared` (topic `coll_clr`), carrying the retired `asset` / `amount` / `recorded_at`

### `get_sme_collateral_commitment`

```rust
pub fn get_sme_collateral_commitment(env: Env) -> Option<SmeCollateralCommitment>
```

- **Auth:** none (read-only)
- **Returns:** `Some(commitment)` when a pledge is stored; `None` when never recorded or already cleared

`get_escrow_summary` also surfaces the same optional pledge for dashboards — likewise ungated.

---

## Exact guard ordering (code-accurate)

### `record_sme_collateral_commitment`

Evaluated in this order:

1. **Amount gate** — `amount > 0` → else [`EscrowError::CollateralAmountNotPositive`](escrow-error-messages.md) (**60**)
2. **Asset gate** — `asset != Symbol::new(&env, "")` → else [`EscrowError::CollateralAssetEmpty`](escrow-error-messages.md) (**61**)
3. **SME authorization** — `load_escrow_require_sme`
   - missing escrow → [`EscrowError::EscrowNotInitialized`](escrow-error-messages.md) (**20**)
   - wrong / missing SME signature → host auth trap
4. **Monotonic timestamp (replace only)** — if a prior pledge exists, `ledger.timestamp() >= prior.recorded_at`
   - else [`EscrowError::CollateralTimestampBackwards`](escrow-error-messages.md) (**62**); prior storage is left unchanged
   - equal timestamps are **allowed** (`>=`, not strict `>`)
5. **Write + event** — set `SmeCollateralPledge`, emit `CollateralRecordedEvt`

### `clear_sme_collateral_commitment`

Evaluated in this order (ADR-002: informative errors before auth):

1. **Read-only existence check** — load `SmeCollateralPledge`; absent → [`EscrowError::NoCollateralToClear`](escrow-error-messages.md) (**169**) **before** any auth
2. **SME authorization** — `load_escrow_require_sme` (**20** / host auth trap as above)
3. **Mutation** — `remove` the pledge key; emit `CollateralClearedEvt` then `CollateralCommitmentCleared`

---

## Allowed states and orthogonal gates

Collateral mutations are **status-agnostic**. Neither `record` nor `clear` reads `InvoiceEscrow::status`.

| Gate | Applies to collateral? |
| --- | --- |
| Escrow status (`0` open / `1` funded / `2` settled / `3` withdrawn / `4` cancelled) | **No** — record/clear allowed in any status once initialized |
| Legal hold | **No** — explicitly ungated (see [`docs/escrow-legal-hold.md`](escrow-legal-hold.md)) |
| Operational pause | **No** — pause only blocks fund / settle / withdraw / claim |

After [`rotate_beneficiary`](ESCROW_BENEFICIARY_ROTATION.md), the **new** `sme_address` is the only address that can record or clear collateral.

---

## Operator-facing rejection codes

| Code | Variant | Entrypoint | Trigger |
| ---: | --- | --- | --- |
| 20 | `EscrowNotInitialized` | `record`, `clear` | Escrow storage missing |
| 60 | `CollateralAmountNotPositive` | `record` | `amount <= 0` |
| 61 | `CollateralAssetEmpty` | `record` | empty `asset` symbol |
| 62 | `CollateralTimestampBackwards` | `record` (replace) | `now < prior.recorded_at` |
| 169 | `NoCollateralToClear` | `clear` | no pledge stored |

Non-SME callers fail authorization at the host (not one of the codes above).

---

## What collateral does **not** do

- Move, lock, or reserve funding-token balances
- Prove off-chain custody or create an enforceable on-chain encumbrance
- Gate or delay `settle`, `withdraw`, or `claim_investor_payout`
- Require admin dual-control (contrast [`rotate_beneficiary`](ESCROW_BENEFICIARY_ROTATION.md))

Covered by tests such as `test_record_collateral_stored_and_does_not_block_settle` (`escrow/src/tests/admin.rs`) and `test_collateral_record_does_not_change_token_balances` (`escrow/src/tests/coverage.rs`).

---

## Worked example

Assumptions: escrow already `init`'d with SME `S`; ledger time `t0`; admin is irrelevant for these calls.

1. **Anyone** calls `get_sme_collateral_commitment` → `None` (no auth).
2. **Investor** (not SME) calls `record_sme_collateral_commitment("USDC", 5_000)` → **host auth trap**.
3. **SME `S`** calls `record_sme_collateral_commitment("USDC", 5_000)` at `t0` → stores pledge; event `prior_amount = 0`.
4. Ledger stays at `t0`. **SME `S`** replaces with `("USDC", 7_500)` → accepted (`now >= recorded_at`); event `prior_amount = 5_000`.
5. Operator rolls ledger **backward** below `t0` and SME retries replace → **`CollateralTimestampBackwards` (62)**; stored amount remains `7_500`.
6. Escrow is fully funded and **settled**. **SME `S`** still may `clear_sme_collateral_commitment` → removes pledge; dual `coll_clr` events fire. Settlement is unaffected by the earlier record.
7. **SME `S`** calls `clear` again → **`NoCollateralToClear` (169)** (existence check runs before auth).

---

## Events (indexer notes)

| Event | Name topic | Emitted by |
| --- | --- | --- |
| `CollateralRecordedEvt` | `coll_rec` | `record_sme_collateral_commitment` |
| `CollateralClearedEvt` | `coll_clr` | `clear_sme_collateral_commitment` |
| `CollateralCommitmentCleared` | `coll_clr` | `clear_sme_collateral_commitment` (second publish) |

Full topic/data layout: [`docs/EVENT_SCHEMA.md`](EVENT_SCHEMA.md). Treat `coll_rec` / `coll_clr` as **compliance/risk signals**, not proof of locked funds ([`docs/audit-handoff-escrow.md`](audit-handoff-escrow.md)).

---

## See also

- [`docs/escrow-sme-collateral.md`](escrow-sme-collateral.md) — functional overview and test matrix
- [`docs/adr/ADR-002-auth-boundaries.md`](adr/ADR-002-auth-boundaries.md) — role → entrypoint auth map
- [`docs/escrow-error-messages.md`](escrow-error-messages.md) — typed error reference
- [`docs/escrow-legal-hold.md`](escrow-legal-hold.md) — hold does not gate collateral
- [`docs/ESCROW_BENEFICIARY_ROTATION.md`](ESCROW_BENEFICIARY_ROTATION.md) — how SME identity (and thus collateral auth) can change
- [`docs/beneficiary.md`](beneficiary.md) — SME-gated entrypoint inventory
