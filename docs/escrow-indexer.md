# Escrow Indexer Ops (Stellar/Soroban)

Minimal indexer strategy for `liquifact_escrow`: what to subscribe to as events, what to poll as
state keys, and how to handle stale-RPC vs chainhead disagreement.

---

## 1) Minimal architecture

Use **both** channels:

- **Event stream (primary timeline):** subscribe to contract events for lifecycle transitions and
  append-only activity.
- **Storage reads (state truth):** poll selected read endpoints to reconcile derived state and
  recover from gaps/restarts.

Recommended split:

- **Subscribe** for write activity (funding, settle, admin/legal-hold controls, claims, attestation updates).
- **Poll** for canonical snapshots and optional/hint fields (`get_escrow`, `get_registry_ref`, etc.).

---

## 2) What to subscribe to (contract events)

Contract event names (`symbol_short`) emitted by `escrow/src/lib.rs`:

- `escrow_ii` - escrow initialized
- `funded` - contribution recorded
- `escrow_sd` - settled
- `sme_wd` - SME withdraw
- `inv_claim` - investor payout claimed
- `dust_sw` - treasury dust sweep
- `legalhld` - legal hold changed
- `admin` - admin transferred
- `maturity` - maturity updated
- `fund_tgt` - funding target updated
- `coll_rec` - SME collateral commitment recorded
- `att_bind` - primary attestation hash bound
- `att_app` - attestation append-log updated
- `al_ena` - allowlist mode enabled/disabled
- `al_set` - investor allowlist membership set

See also: `docs/EVENT_SCHEMA.md`.

### RPC event filter baseline

- Filter by `contractId` = escrow contract id.
- Filter by topics where `topic[0]` is the event symbol above.
- Persist `(ledger, txHash, eventIndex)` as your idempotency cursor.

### Horizon/websocket note

If your stack uses Horizon streams/websocket wrappers, map them to Soroban contract event sources
for the same `contractId` and enforce the same idempotency key `(ledger, txHash, eventIndex)`.

---

## 3) What to poll (storage/read API)

Read endpoints and their backing keys (`docs/escrow-read-api.md`):

### Poll every cycle (canonical state)

- `get_escrow()` -> `DataKey::Escrow`
- `get_legal_hold()` -> `DataKey::LegalHold`
- `get_funding_close_snapshot()` -> `DataKey::FundingCloseSnapshot`
- `get_unique_funder_count()` -> `DataKey::UniqueFunderCount`
- `get_version()` -> `DataKey::Version`

### Poll on-demand (investor/account views)

- `get_contribution(investor)` -> `DataKey::InvestorContribution(investor)`
- `is_investor_claimed(investor)` -> `DataKey::InvestorClaimed(investor)`
- `get_investor_yield_bps(investor)` -> `DataKey::InvestorEffectiveYield(investor)` fallback to base
- `get_investor_claim_not_before(investor)` -> `DataKey::InvestorClaimNotBefore(investor)`

### Poll at startup + periodically (config/hints)

- `get_funding_token()` -> `DataKey::FundingToken` (immutable after init)
- `get_treasury()` -> `DataKey::Treasury` (immutable after init)
- `get_registry_ref()` -> `DataKey::RegistryRef` (optional hint only)
- `get_min_contribution_floor()` -> `DataKey::MinContributionFloor`
- `get_max_unique_investors_cap()` -> `DataKey::MaxUniqueInvestorsCap`
- `get_primary_attestation_hash()` -> `DataKey::PrimaryAttestationHash`
- `get_attestation_append_log()` -> `DataKey::AttestationAppendLog`
- `get_sme_collateral_commitment()` -> `DataKey::SmeCollateralPledge`

---

## 4) Registry hint caveat (`get_registry_ref`)

`get_registry_ref()` is a discoverability hint, not authority:

- `None` is valid and operational.
- A non-`None` address does **not** prove membership or compliance by itself.
- If you need registry guarantees, call and verify that registry contract directly.

Do not treat `RegistryRef` as a trust anchor in risk engines without independent verification.

---

## 5) Failure modes: stale RPC vs chainhead

### Symptom A: event subscription lags chainhead

- **Observation:** latest closed ledger (network) is higher than event stream cursor.
- **Risk:** UI appears stale; lifecycle transitions (e.g., `settled`) are delayed.
- **Action:** keep event cursor, backfill missing ledger range, then resume live stream.

### Symptom B: storage read is behind while events are newer

- **Observation:** event indicates change, but `get_escrow()` still returns prior snapshot.
- **Risk:** temporary read-after-write inconsistency across providers.
- **Action:** gate reconciliation on ledger number; retry read against same-or-newer ledger.

### Symptom C: reorg/rollback window

- **Observation:** previously seen event disappears from canonical chain.
- **Risk:** derived tables contain orphaned transitions.
- **Action:** store ledger/tx/event provenance; support rollback of non-finalized ledgers and replay.

### Symptom D: provider disagreement (RPC A vs RPC B)

- **Observation:** different latest ledgers or inconsistent event availability.
- **Risk:** duplicated or missing state transitions.
- **Action:** prefer a single writer provider per environment, periodically cross-check, and use
  deterministic idempotency keys to avoid double application.

---

## 6) Recommended reconciliation loop

1. Read latest finalized/accepted ledger checkpoint.
2. Consume events from last stored cursor to checkpoint.
3. Apply event-driven projections idempotently.
4. Poll canonical read endpoints (`get_escrow`, hold, snapshot, version).
5. If projection != polled canonical state, mark drift and re-sync from last good ledger.

---

## 7) Security notes for indexers

- Unsupported token economics (fee-on-transfer/rebasing/hook tokens) are out of scope for escrow
  accounting and are expected to fail closed at transfer balance checks in
  `escrow/src/external_calls.rs`.
- Indexers should surface these failures as integration alerts, not silently smooth them over.
- Treat attestation and collateral entries as metadata records unless/until on-chain enforcement is
  explicitly introduced by contract APIs.

