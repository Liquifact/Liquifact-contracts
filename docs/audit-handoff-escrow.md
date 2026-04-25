# LiquiFact Escrow — Audit Handoff Bundle

**Contract:** `liquifact_escrow` (`escrow/src/lib.rs`)  
**Schema version:** 5 (`SCHEMA_VERSION`)  
**Soroban SDK:** 25.0  
**Stellar protocol:** SEP-41 token interface  

---

## 1. Contract Purpose

Single-invoice escrow. Holds investor stablecoin principal until a funding target is met, then allows the SME to withdraw or settle. After settlement, each investor records a claim marker; actual payout math happens off-chain using the on-chain snapshot and contribution data.

---

## 2. Lifecycle State Machine

```
         fund() / fund_with_commitment()
  [0: open] ──────────────────────────► [1: funded]
                 (funded_amount ≥ target)      │
                                               ├── settle() ──► [2: settled] ──► claim_investor_payout()
                                               └── withdraw() ─► [3: withdrawn]
```

| Status | Value | Terminal | Dust sweep allowed |
|--------|-------|----------|--------------------|
| open | 0 | No | No |
| funded | 1 | No | No |
| settled | 2 | Yes | Yes |
| withdrawn | 3 | Yes | Yes |

Transitions are **strictly forward**. No entrypoint moves status backward.

---

## 3. Invariants

These map directly to property tests in `escrow/src/test/properties.rs` and `test/init.rs`.

| ID | Name | Statement | Test(s) |
|----|------|-----------|---------|
| ESC-STA-001 | status_monotone | `status` never decreases; valid paths `0→1→2` or `0→1→3` | `prop_status_only_increases`, `test_withdraw_funded_then_cannot_settle` |
| ESC-FUND-001 | funded_amount_monotone | Each `fund` call adds a positive amount; `funded_amount` never decreases | `prop_funded_amount_non_decreasing` |
| ESC-FUND-002 | contribution_sum | `funded_amount == Σ contribution(investor)` across all investors while open | `test_contributions_sum_equals_funded_amount` |
| ESC-CLM-001 | investor_claim_once | `InvestorClaimed(investor)` set at most once; second call panics | `test_claim_investor_twice_panics` |
| ESC-ATT-001 | primary_attestation_single_set | `PrimaryAttestationHash` written once; rebind panics | `test_bind_primary_attestation_single_set_and_get`, `test_bind_primary_attestation_twice_panics` |
| ESC-ATT-002 | attestation_append_bounded | `len(AttestationAppendLog) ≤ MAX_ATTESTATION_APPEND_ENTRIES (32)` | `test_append_attestation_respects_max_length` |
| ESC-MIN-001 | min_contribution_per_call | If `min_floor > 0`, every `fund` amount `≥ min_floor` | `test_min_contribution_floor_rejects_below_and_accepts_equal` |
| ESC-CAP-001 | unique_funder_cap | If `MaxUniqueInvestorsCap = n`, at most `n` distinct investor addresses may contribute | `test_max_unique_investors_cap_enforced` |
| ESC-INI-001 | single_initialization | `DataKey::Escrow` written exactly once; second `init` panics | `test_double_init_panics` |
| ESC-IMM-001 | funding_token_immutable | `DataKey::FundingToken` set at init and never mutated | `test_init_stores_registry_some_and_getters` |
| ESC-IMM-002 | treasury_immutable | `DataKey::Treasury` set at init and never mutated | `test_init_stores_registry_some_and_getters` |
| ESC-SNAP-001 | snapshot_write_once | `FundingCloseSnapshot` written at first `status→1` transition; never overwritten | `test_funding_close_snapshot_set_on_fund` |
| ESC-YIELD-001 | tier_selection_immutable | `InvestorEffectiveYield(investor)` set on first deposit only; `fund_with_commitment` panics if investor already contributed | `test_fund_with_commitment_second_call_panics` |
| ESC-DUST-001 | dust_sweep_terminal_only | `sweep_terminal_dust` rejected when `status < 2` | `test_sweep_rejected_when_open` |
| ESC-DUST-002 | dust_sweep_capped | `sweep_terminal_dust` amount ≤ `MAX_DUST_SWEEP_AMOUNT` (100_000_000) | `test_sweep_rejects_amount_above_dust_cap` |

---

## 4. Trust Model

### 4.1 Role → Entrypoint Map

| Role | Stored at | Entrypoints authorized |
|------|-----------|------------------------|
| `admin` | `InvoiceEscrow::admin` (rotatable via `transfer_admin`) | `init`, `set_legal_hold`, `clear_legal_hold`, `update_maturity`, `update_funding_target`, `transfer_admin`, `migrate`, `bind_primary_attestation_hash`, `append_attestation_digest` |
| `sme_address` | `InvoiceEscrow::sme_address` (immutable) | `settle`, `withdraw`, `record_sme_collateral_commitment` |
| `investor` | per-call argument (verified via `require_auth`) | `fund`, `fund_with_commitment`, `claim_investor_payout` |
| `treasury` | `DataKey::Treasury` (immutable) | `sweep_terminal_dust` |

**No superuser path exists.** The admin cannot sweep dust unless it is also the treasury. A compromised investor key cannot settle, withdraw, or sweep.

### 4.2 Legal Hold Gate

When `DataKey::LegalHold == true`, the following entrypoints panic immediately:

- `fund` / `fund_with_commitment`
- `settle`
- `withdraw`
- `claim_investor_payout`
- `sweep_terminal_dust`

Read-only getters are never blocked. Only `admin` can set or clear the hold. There is **no timelock or automatic expiry** — production deployments must use a multisig or governed contract as `admin`.

### 4.3 Registry Non-Authority Model

`DataKey::RegistryRef` is an **optional, read-only, off-chain hint** stored at init. No on-chain logic in this contract calls or reads it after storage. Its presence **does not** constitute proof of registry membership. Callers must query the registry contract directly.

---

## 5. Function → Event → Off-chain Followup

| Function | Event struct | Topic symbol | Off-chain followup |
|----------|-------------|-------------|---------------------|
| `init` | `EscrowInitialized` | `escrow_ii` | Index `invoice_id`; register `funding_token` and `treasury` addresses; start monitoring |
| `fund` | `EscrowFunded` | `funded` | Update investor contribution ledger; check `status` field for funded transition |
| `fund_with_commitment` | `EscrowFunded` | `funded` | Same as `fund`; also record `investor_effective_yield_bps` and claim-lock timestamp |
| `settle` | `EscrowSettled` | `escrow_sd` | Trigger off-chain pro-rata payout calculation using `FundingCloseSnapshot` + per-investor `get_contribution` |
| `withdraw` | `SmeWithdrew` | `sme_wd` | Record SME liquidity event; update invoice status in off-chain ledger |
| `claim_investor_payout` | `InvestorPayoutClaimed` | `inv_claim` | Mark investor as paid in off-chain system; release hold on investor record |
| `set_legal_hold(true)` | `LegalHoldChanged` | `legalhld` | Alert compliance dashboard; suspend investor UI funding flows |
| `set_legal_hold(false)` / `clear_legal_hold` | `LegalHoldChanged` | `legalhld` | Resume operations; notify relevant parties |
| `update_maturity` | `MaturityUpdatedEvent` | `maturity` | Update off-chain settlement schedule; re-notify investors if material |
| `transfer_admin` | `AdminTransferredEvent` | `admin` | Update key registry and access control records |
| `update_funding_target` | `FundingTargetUpdated` | `fund_tgt` | Update off-chain target display; re-evaluate investor communications |
| `record_sme_collateral_commitment` | `CollateralRecordedEvt` | `coll_rec` | Store in compliance/risk system; **do not treat as enforced on-chain lock** |
| `sweep_terminal_dust` | `TreasuryDustSwept` | `dust_sw` | Reconcile treasury balance; log sweep amount and token address |
| `bind_primary_attestation_hash` | `PrimaryAttestationBound` | `att_bind` | Verify digest against known IPFS CID or document bundle; record binding in compliance system |
| `append_attestation_digest` | `AttestationDigestAppended` | `att_app` | Append to off-chain audit log with `index` for ordering |

---

## 6. Known Limitations and Out-of-Scope Items

### 6.1 Fee-on-Transfer / Non-Standard Tokens

`external_calls::transfer_funding_token_with_balance_checks` records pre/post balances and asserts exact delta equality on both sender and recipient. Fee-on-transfer, rebasing, or "hook" tokens will trigger a panic (safe failure). They are **not supported** and must be excluded by governance before deployment. Standard SEP-41 tokens with no side-effects are the only in-scope class.

### 6.2 Registry Hint — Not Authority

`RegistryRef` is metadata for off-chain indexers only. The contract never calls the registry on-chain. See §4.3 above.

### 6.3 Record-Only Collateral

`SmeCollateralCommitment` stores asset code, amount, and timestamp. It does **not** custody tokens, freeze assets, or trigger automated liquidation. A future version could enforce transfers, but that would require an explicit API change and must not reuse this record as proof of locked assets.

### 6.4 Claim Is a Marker Only

`claim_investor_payout` sets `InvestorClaimed(investor) = true` and emits an event. It **does not transfer tokens**. Actual payout is the responsibility of the integration layer using `FundingCloseSnapshot.total_principal` and `get_contribution(investor)` for pro-rata math. Integer rounding in off-chain division should be audited separately.

### 6.5 Ledger Time Trust

Maturity (`InvoiceEscrow::maturity`) and claim locks (`InvestorClaimNotBefore`) are compared against `Env::ledger().timestamp()` — validator-observed ledger time, not a wall-clock oracle. Simulated and live network timestamps may skew; boundaries are `>=` / `<` on integer seconds.

### 6.6 Legal Hold — No Automatic Expiry

There is no timelock on the hold. Indefinite fund lock is possible if `admin` is a single compromised key. Production deployments must set `admin` to a governed multisig with off-chain recovery procedures.

### 6.7 Unique Investor Cap — Sybil Resistance

`MaxUniqueInvestorsCap` limits distinct **chain accounts**, not real-world persons. It provides no Sybil resistance.

### 6.8 Schema Migration

The `migrate` function panics for all current `from_version` values below `SCHEMA_VERSION`. Changing `InvoiceEscrow` struct layout requires a coordinated migration or full redeploy. Additive instance keys (new `DataKey` variants) are backward-compatible; layout changes are not.

### 6.9 Token Economics — Out of Scope

Yield coupon calculation, off-chain interest accrual, and pro-rata rounding are entirely off-chain concerns. The contract stores `yield_bps` and the snapshot but performs no token arithmetic beyond the `calculate_principal_plus_yield` helper (pure integer, no custody).

---

## 7. Storage Key Reference

| `DataKey` variant | Type | Mutable after init | Notes |
|-------------------|------|--------------------|-------|
| `Escrow` | `InvoiceEscrow` | Yes (status, funded_amount, admin) | Rewritten atomically on every state change |
| `Version` | `u32` | No | Always `SCHEMA_VERSION` after init |
| `FundingToken` | `Address` | No | SEP-41 token; set once |
| `Treasury` | `Address` | No | Dust sweep recipient; set once |
| `RegistryRef` | `Address` | No | Optional; omitted when `None` at init |
| `LegalHold` | `bool` | Yes (admin only) | Absent = `false` |
| `MinContributionFloor` | `i128` | No | `0` = no floor |
| `MaxUniqueInvestorsCap` | `u32` | No | Optional; omitted when unlimited |
| `UniqueFunderCount` | `u32` | Yes | Incremented on first deposit per address |
| `YieldTierTable` | `Vec<YieldTier>` | No | Optional; omitted when no tiers |
| `FundingCloseSnapshot` | `FundingCloseSnapshot` | No | Written once at status→1; never overwritten |
| `InvestorContribution(addr)` | `i128` | Yes | Incremented per `fund` call |
| `InvestorEffectiveYield(addr)` | `i64` | No | Set on first deposit; immutable thereafter |
| `InvestorClaimNotBefore(addr)` | `u64` | No | `0` = no gate; set by `fund_with_commitment` |
| `InvestorClaimed(addr)` | `bool` | No (write-once) | Set to `true` by `claim_investor_payout` |
| `SmeCollateralPledge` | `SmeCollateralCommitment` | Yes (SME may replace) | Record-only; no token custody |
| `PrimaryAttestationHash` | `BytesN<32>` | No (write-once) | Single-set; rebind panics |
| `AttestationAppendLog` | `Vec<BytesN<32>>` | Append-only | Bounded by `MAX_ATTESTATION_APPEND_ENTRIES` |

---

## 8. Security Assumptions

1. **`admin` is a governed key.** Legal hold, attestation binding, maturity updates, and admin rotation are all gated by `admin`. A compromised single-key admin can freeze the escrow indefinitely.
2. **Funding token is standard SEP-41.** Fee-on-transfer or rebasing tokens will panic at the balance-check boundary, not silently corrupt state.
3. **Soroban single-writer model.** The host function runs to completion before any other call to the same contract. Classic EVM-style reentrancy is not possible; the pre/post balance check in `external_calls` is a defense-in-depth against non-compliant token behavior, not a reentrancy guard.
4. **Off-chain payout correctness is the integrator's responsibility.** The contract records the snapshot and contribution data; it does not enforce that investors receive correct amounts.
5. **Attestation digests are not verified on-chain.** The contract stores 32-byte blobs verbatim. Hash algorithm and canonical encoding are off-chain conventions that must be documented and agreed separately.

---

## 9. Test Coverage Summary

All 91 tests pass. CI enforces `cargo llvm-cov --features testutils --fail-under-lines 95`.

| File | Line coverage |
|------|--------------|
| `src/lib.rs` | ≥ 95% |
| `test/init.rs` | 100% |
| `test/funding.rs` | 100% |
| `test/settlement.rs` | 100% |
| `test/admin.rs` | 100% |
| `test/integration.rs` | 95% |
| `test/properties.rs` | 100% |
| **TOTAL** | **97.02%** |

Run locally:

```bash
cargo test -p liquifact_escrow
cargo llvm-cov --features testutils --fail-under-lines 95 --summary-only -p liquifact_escrow
```
