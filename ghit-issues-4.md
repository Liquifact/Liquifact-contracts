---
type: Feature
title: "Fix the duplicate EscrowError discriminant shared by FundingDeadlinePassed and NoPendingAdmin"
labels: type:security, area:errors, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Resolve the colliding numeric code 163 between FundingDeadlinePassed and NoPendingAdmin

### Description
In the `EscrowError` enum in [`escrow/src/lib.rs`](escrow/src/lib.rs), two distinct variants are assigned the **same numeric discriminant**: `FundingDeadlinePassed = 163` and `NoPendingAdmin = 163`. The contract's documented SDK contract is that callers "branch on the numeric code rather than legacy panic strings", so two unrelated failures (a funding window that has closed versus an `accept_admin` with no pending successor) are indistinguishable to clients — a real correctness and observability bug. Because the codes are meant to be append-only and stable, simply renumbering one in place would itself break the policy unless done carefully.

This issue assigns a unique, append-only code to one of the colliding variants and proves every error code is distinct.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Keep `FundingDeadlinePassed` at its existing slot (it is referenced from `init`) and move `NoPendingAdmin` to a fresh unused discriminant in the admin-handover range (e.g. the 80s block alongside `NewAdminSameAsCurrent = 80`), or vice versa, choosing whichever minimizes churn against deployed instances.
- Add a compile-time or test-time assertion that no two `EscrowError` variants share a discriminant.
- Update [`docs/escrow-error-messages.md`](docs/escrow-error-messages.md) to reflect the corrected, collision-free table and note the historical collision in a migration note.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-dedupe-error-code-163`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — reassign the colliding discriminant.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — assert `accept_admin` with no pending admin and a deadline-passed `fund` raise distinct, correct codes; add a uniqueness test over all variants.
  - **Add documentation:** update [`docs/escrow-error-messages.md`](docs/escrow-error-messages.md).
  - Include NatSpec-style `///` comments on the reassigned variant explaining the history.
  - Validate security: no two codes collide; client branching is unambiguous.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: deadline-passed funding, accept-admin with no proposal, full-enum uniqueness check.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`fix: resolve duplicate EscrowError discriminant 163 with uniqueness test and docs`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Convert revoke_attestation_digest panic strings to typed EscrowError codes"
labels: type:security, area:errors, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Harden revoke_attestation_digest with stable typed errors

### Description
`revoke_attestation_digest` in [`escrow/src/lib.rs`](escrow/src/lib.rs) still validates with raw `assert!` panic strings — `"attestation index out of range"` and `"attestation already revoked at index"` — while every other attestation path (`bind_primary_attestation_hash`, `append_attestation_digest`) uses the append-only `EscrowError` enum with codes such as `PrimaryAttestationAlreadyBound` and `AttestationAppendLogCapacityReached`. This breaks the documented SDK contract that callers branch on numeric codes, leaving the revoke path inconsistent with its sibling entrypoints.

This issue replaces both asserts with typed errors.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add append-only `EscrowError` variants in the attestation range (alongside `50`/`51`), e.g. `AttestationIndexOutOfRange` and `AttestationAlreadyRevoked`; never renumber existing codes.
- Replace the two `assert!` calls in `revoke_attestation_digest` with `ensure(&env, cond, EscrowError::...)`, preserving exact behavior, guard ordering, and the `AttestationDigestRevoked` event.
- Keep admin authorization first; no behavior change beyond the revert type.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-revoke-attestation-typed-errors`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — new error variants and `ensure` calls.
  - **Write comprehensive tests in:** [`escrow/src/tests/attestations.rs`](escrow/src/tests/attestations.rs) — assert each typed error via `try_revoke_attestation_digest` (out-of-range index, double revoke), plus non-admin rejection.
  - **Add documentation:** update [`docs/escrow-error-messages.md`](docs/escrow-error-messages.md) and [`docs/escrow-attestations.md`](docs/escrow-attestations.md).
  - Include NatSpec-style `///` comments on the new variants and the entrypoint.
  - Validate security: identical revert conditions, stable numeric codes.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: index past log length, already-revoked index, valid revoke, non-admin caller.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`fix: replace revoke_attestation_digest panic strings with typed EscrowError codes and tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Disambiguate the al_set event symbol shared by single and batch allowlist writes"
labels: type:enhancement, area:events, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Give batch allowlist writes a distinct event topic from single writes

### Description
Both `set_investor_allowlisted` and the batch `set_investors_allowlisted` in [`escrow/src/lib.rs`](escrow/src/lib.rs) publish the `InvestorAllowlistChanged` event with the identical `symbol_short!("al_set")` name. The batch path's documented invariant is that "the end state and emitted events are identical to calling `set_investor_allowlisted` individually", which is correct for per-investor accounting, but it leaves indexers unable to distinguish a single administrative change from a bulk operation, and provides no batch-level marker (size, common `allowed` flag) for audit trails.

This issue adds a dedicated batch-level event while preserving the per-investor events.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Keep emitting one `InvestorAllowlistChanged` (`al_set`) per address from the batch path so the documented per-address invariant holds.
- Add a single additional `InvestorAllowlistBatchApplied` `#[contractevent]` (distinct symbol, e.g. `al_batch`) emitted once per `set_investors_allowlisted` call carrying `invoice_id`, batch size, and the common `allowed` flag.
- Keep `set_investor_allowlisted` unchanged; this is purely additive for indexers.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-allowlist-batch-event`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — new batch event and emission point.
  - **Write comprehensive tests in:** [`escrow/src/test_allowlist_tests.rs`](escrow/src/test_allowlist_tests.rs) — assert N per-investor events plus exactly one batch event with correct size/flag.
  - **Add documentation:** update [`docs/EVENT_SCHEMA.md`](docs/EVENT_SCHEMA.md) and [`docs/escrow-events.md`](docs/escrow-events.md).
  - Include NatSpec-style `///` comments on the new event.
  - Validate security: per-address invariant unchanged; single batch event.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: single-element batch, max-size batch, allow vs disallow flag.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add distinct batch allowlist event topic alongside per-investor events with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add an inbound funding-token transfer helper to external_calls with balance-delta checks"
labels: type:feature, area:token-safety, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement transfer_into_escrow_with_balance_checks for inbound custody

### Description
[`escrow/src/external_calls.rs`](escrow/src/external_calls.rs) exposes only `transfer_funding_token_with_balance_checks`, an **outbound** helper used by `refund` and `sweep_terminal_dust` to move tokens from the contract to a recipient with strict pre/post balance-delta conservation. There is no symmetric **inbound** helper that pulls tokens from an external payer into the contract while applying the same fee-on-transfer / rebasing / hook-token safe-fail invariants. Any future on-chain custody at `fund` (recording investor principal) must hand-roll the balance checks, duplicating subtle logic and risking divergence from the audited outbound path.

This issue adds a hardened inbound helper mirroring the outbound one.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `transfer_into_escrow_with_balance_checks(env, token, from, to_contract, amount)` that records the contract's pre-balance, calls `token::Client::transfer`, then asserts the recipient delta equals `amount` and the sender delta is non-positive, reusing the existing typed errors (`TransferAmountNotPositive`, `RecipientBalanceDeltaMismatch`, `SenderBalanceDeltaMismatch`, underflow guards).
- Do not wire it into `fund` in this issue (custody activation is tracked separately); deliver the audited primitive and its tests so callers can adopt it safely.
- Keep the outbound helper untouched; share constants/error codes where applicable.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-inbound-transfer-helper`
- Implement changes
  - **Write code in:** [`escrow/src/external_calls.rs`](escrow/src/external_calls.rs) — the inbound helper.
  - **Write comprehensive tests in:** [`escrow/src/tests/external_calls_mocked.rs`](escrow/src/tests/external_calls_mocked.rs) — adversarial tokens (fee-on-transfer under-credit, rebasing over-credit, no-op), zero/negative amount, and a happy-path delta assertion.
  - **Add documentation:** update [`docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`](docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md).
  - Include NatSpec-style `///` comments on the helper and its invariants.
  - Validate security: every non-compliant inbound path safe-fails; balance conservation holds.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: under-delivery, over-credit, no-op transfer, zero amount.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add inbound funding-token transfer helper with balance-delta checks and tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a paginated view enumerating revoked attestation indices"
labels: type:feature, area:read-api, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement get_revoked_attestation_indices for the audit chain

### Description
Attestation revocation in [`escrow/src/lib.rs`](escrow/src/lib.rs) is stored per index under `DataKey::AttestationRevoked(u32)` and is only queryable one index at a time via `is_attestation_revoked(index)`. There is no way to enumerate which entries in the bounded append-log (capped at `MAX_ATTESTATION_APPEND_ENTRIES = 32`) have been revoked — an indexer or auditor must probe all 32 slots individually, with no single authoritative on-chain answer.

This issue adds a read returning the set of revoked indices for the log.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_revoked_attestation_indices(env) -> Vec<u32>` that scans `0..get_attestation_append_log().len()` and collects indices where `DataKey::AttestationRevoked(i)` is set.
- Pure read, no auth, no mutation; bounded by `MAX_ATTESTATION_APPEND_ENTRIES`.
- Document that indices align with `get_attestation_append_log` ordering and that legacy instances with no revocations return an empty `Vec`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-revoked-attestation-view`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `get_revoked_attestation_indices` view.
  - **Write comprehensive tests in:** [`escrow/src/tests/attestations.rs`](escrow/src/tests/attestations.rs) — none revoked, some revoked, all revoked, ordering matches the log.
  - **Add documentation:** update [`docs/escrow-attestations.md`](docs/escrow-attestations.md) and [`docs/escrow-read-api.md`](docs/escrow-read-api.md).
  - Include NatSpec-style `///` comments on the view.
  - Validate security: pure read, bounded scan, no mutation.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: empty log, partial revocation, full revocation.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add get_revoked_attestation_indices read view with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add an un-revoke entrypoint to reverse an erroneous attestation revocation"
labels: type:feature, area:attestations, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement admin unrevoke_attestation_digest to clear a mistaken revocation

### Description
`revoke_attestation_digest` in [`escrow/src/lib.rs`](escrow/src/lib.rs) sets `DataKey::AttestationRevoked(index)` permanently, and there is **no way to undo it**. If an admin revokes the wrong index (a fat-finger on a 0-based index), the provenance entry is marked revoked forever even though the underlying digest was legitimate, polluting the audit chain that indexers surface.

This issue adds an admin-gated `unrevoke_attestation_digest(index)` that clears the flag and emits a dedicated event.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `unrevoke_attestation_digest(env, index: u32)` gated by admin auth; assert the index is in range and currently revoked (append-only typed errors, reusing/extending the attestation error range), then remove `DataKey::AttestationRevoked(index)`.
- Emit a new `AttestationDigestUnrevoked` `#[contractevent]` carrying `invoice_id` and `index`.
- Keep ADR-002 guard ordering: range/state checks then admin `require_auth` consistent with the existing revoke path.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-unrevoke-attestation`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `unrevoke_attestation_digest`, event, errors.
  - **Write comprehensive tests in:** [`escrow/src/tests/attestations.rs`](escrow/src/tests/attestations.rs) — revoke then unrevoke restores state, unrevoke-without-revoke rejection, out-of-range rejection, non-admin rejection.
  - **Add documentation:** update [`docs/escrow-attestations.md`](docs/escrow-attestations.md).
  - Include NatSpec-style `///` comments on the entrypoint and event.
  - Validate security: admin-only, idempotency, bounded index.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: unrevoke of a non-revoked index, out-of-range index, double unrevoke, non-admin caller.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add admin unrevoke_attestation_digest entrypoint with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add an admin entrypoint to cancel a pending admin handover proposal"
labels: type:feature, area:admin, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement cancel_pending_admin to withdraw an unaccepted handover

### Description
The two-step admin handover in [`escrow/src/lib.rs`](escrow/src/lib.rs) writes `DataKey::PendingAdmin` in `propose_admin` and only clears it when the successor calls `accept_admin`. There is **no way for the current admin to retract a proposal** once made — if the wrong successor was proposed, or the handover is abandoned, the pending key lingers and the proposed address can accept at any later time, which is a standing key-rotation risk.

This issue adds an admin-gated `cancel_pending_admin()` that clears the pending proposal.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `cancel_pending_admin(env)` gated via `load_escrow_require_admin`; require a pending admin to exist (reuse `NoPendingAdmin`), then remove `DataKey::PendingAdmin`.
- Emit a new `AdminProposalCancelled` `#[contractevent]` carrying `invoice_id` and the cancelled pending address.
- Keep `propose_admin`/`accept_admin` semantics unchanged; this only removes an unaccepted proposal.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-cancel-pending-admin`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `cancel_pending_admin`, event.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — propose then cancel clears `get_pending_admin`, accept-after-cancel fails, cancel-without-proposal rejection, non-admin rejection.
  - **Add documentation:** update [`docs/OPERATOR_RUNBOOK.md`](docs/OPERATOR_RUNBOOK.md) and the README entrypoint table.
  - Include NatSpec-style `///` comments on the entrypoint and event.
  - Validate security: admin-only, proposal cannot be accepted after cancel.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: cancel with no proposal, cancel then re-propose, accept blocked after cancel, non-admin caller.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add cancel_pending_admin entrypoint to retract an unaccepted handover with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a settled-coupon read view exposing the total pool owed at settlement"
labels: type:enhancement, area:read-api, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement get_settlement_pool returning principal plus base coupon

### Description
`compute_investor_payout` in [`escrow/src/lib.rs`](escrow/src/lib.rs) derives a per-investor `gross_payout` from the `FundingCloseSnapshot` and the documented formula `settle_pool = total_principal + coupon`, but the **aggregate** `settle_pool` (the total amount the SME must repay to fully satisfy all investors) is never exposed as a view. SME repayment tooling and dashboards must re-derive `total_principal × yield_bps / 10_000` off-chain, risking a rounding divergence from the on-chain math.

This issue adds an authoritative aggregate view of the settlement pool.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_settlement_pool(env) -> i128` returning `total_principal + floor(total_principal × yield_bps / 10_000)` computed from `DataKey::FundingCloseSnapshot` and the escrow's base `yield_bps`, using the same `checked_*` arithmetic and `ComputePayoutArithmeticOverflow` guard as `compute_investor_payout`.
- Return `0` when the snapshot is absent (escrow not yet funded), matching `compute_investor_payout` semantics.
- Document that this uses the escrow base yield (tier-specific effective yields are per-investor and reflected only in `compute_investor_payout`).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-settlement-pool-view`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `get_settlement_pool` view reusing the coupon math.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — pool equals principal+coupon, zero before snapshot, rounding floor, overflow guard.
  - **Add documentation:** update [`docs/escrow-pro-rata.md`](docs/escrow-pro-rata.md) and [`docs/escrow-read-api.md`](docs/escrow-read-api.md).
  - Include NatSpec-style `///` comments on the view.
  - Validate security: pure read, identical rounding to the payout formula, overflow-safe.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: zero yield, max yield, no snapshot, large principal near overflow.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add get_settlement_pool aggregate coupon view with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Emit a dedicated event when an admin handover is proposed via the deprecated transfer_admin shim"
labels: type:enhancement, area:admin, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Surface deprecated transfer_admin usage to indexers and operators

### Description
`transfer_admin` in [`escrow/src/lib.rs`](escrow/src/lib.rs) is a `#[deprecated]` shim that silently delegates to `propose_admin`, so the only on-chain signal of a call is the generic `AdminProposedEvent` — indistinguishable from a direct `propose_admin` call. Operators migrating off the legacy one-step API have no way to detect that integrations are still calling the deprecated path, so they cannot drive the deprecation to completion.

This issue makes deprecated-shim usage observable without changing the handover behavior.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Keep `transfer_admin` delegating to `propose_admin` (no behavior change to the two-step flow).
- Emit an additional `DeprecatedTransferAdminUsed` `#[contractevent]` carrying `invoice_id` and the proposed address, so indexers can flag legacy callers.
- Update the deprecation rustdoc to mention the observability event and the intended removal path.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-deprecated-transfer-admin-event`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — emit the new event from `transfer_admin`.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — `transfer_admin` emits both the proposal and the deprecation event; `propose_admin` emits only the proposal.
  - **Add documentation:** update [`docs/EVENT_SCHEMA.md`](docs/EVENT_SCHEMA.md) and [`docs/OPERATOR_RUNBOOK.md`](docs/OPERATOR_RUNBOOK.md).
  - Include NatSpec-style `///` comments on the new event.
  - Validate security: handover behavior unchanged; purely additive event.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: shim vs direct propose, same-address rejection still typed.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: emit deprecation event on transfer_admin shim usage with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a raise-only entrypoint to increase the unique-investor cap before funding closes"
labels: type:feature, area:investor-caps, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement raise_max_unique_investors as a counterpart to the existing lower-only setter

### Description
`lower_max_unique_investors` in [`escrow/src/lib.rs`](escrow/src/lib.rs) lets an admin only **lower** the `MaxUniqueInvestorsCap` while the escrow is open, with guards `NewCapNotLower` and `NewCapBelowCurrentFunderCount`. There is no symmetric way to **raise** the cap: if primary issuance attracts more demand than initially configured, the admin cannot widen participation without redeploying, even though the open state would otherwise permit it.

This issue adds a raise-only counterpart with parallel guards.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `raise_max_unique_investors(env, new_cap: u32)` gated via `load_escrow_require_admin`, allowed only while `status == 0`, requiring an existing cap (`NoInvestorCapConfigured`) and `new_cap > old_cap` (new append-only typed error `NewCapNotHigher`).
- Emit a new `MaxUniqueInvestorsCapRaised` `#[contractevent]` carrying `invoice_id`, `old_cap`, `new_cap` (parallel to `MaxUniqueInvestorsCapLowered`).
- Preserve all funding-cap enforcement in `fund_impl`; this only widens the ceiling pre-close.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-raise-unique-cap`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `raise_max_unique_investors`, event, error.
  - **Write comprehensive tests in:** [`escrow/src/tests/cap_validation.rs`](escrow/src/tests/cap_validation.rs) — raise accepted, equal/lower rejected, no-cap rejection, post-close rejection, more funders allowed after raise.
  - **Add documentation:** update [`docs/escrow-investor-caps.md`](docs/escrow-investor-caps.md).
  - Include NatSpec-style `///` comments on the entrypoint, event, and error.
  - Validate security: admin-only, open-state-only, raise-only monotonicity.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: raise from existing cap, equal cap rejected, no configured cap, status != open.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add raise_max_unique_investors entrypoint mirroring the lower-only setter with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Allow an admin to update the funding deadline while the escrow is open"
labels: type:feature, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement update_funding_deadline for the open funding window

### Description
The optional `funding_deadline` in [`escrow/src/lib.rs`](escrow/src/lib.rs) is validated and stored once at `init` (`DataKey::FundingDeadline`) and surfaced via `get_funding_deadline`/`is_funding_expired`, but it is **write-once** — there is no entrypoint to extend or set a deadline after deployment. An issuer who needs to extend a stalled raise, or who omitted a deadline at init, cannot adjust it without redeploying, unlike `update_funding_target` and `update_maturity` which are both adjustable while open.

This issue adds an admin-gated deadline update consistent with the other open-state setters.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `update_funding_deadline(env, new_deadline: Option<u64>)` gated via `load_escrow_require_admin`, allowed only while `status == 0`; `Some(d)` requires `d > now` (reuse the `init` validation / `FundingDeadlinePassed`), `None` clears the deadline.
- Emit a new `FundingDeadlineUpdated` `#[contractevent]` carrying `invoice_id`, prior, and new deadline.
- Preserve `is_funding_expired` semantics and the "no deadline" meaning of an absent key.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-update-funding-deadline`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `update_funding_deadline`, event.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — set from none, extend, clear, past-deadline rejection, post-close rejection, `is_funding_expired` reflects update (Ledger testutils).
  - **Add documentation:** update [`docs/escrow-lifecycle.md`](docs/escrow-lifecycle.md) and [`docs/escrow-ledger-time.md`](docs/escrow-ledger-time.md).
  - Include NatSpec-style `///` comments on the entrypoint and event.
  - Validate security: admin-only, open-state-only, deadline must be in the future.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: no prior deadline, extend, clear to none, deadline in the past, status != open.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add admin update_funding_deadline entrypoint for the open window with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a deposit-preview view that simulates a fund call without mutating state"
labels: type:feature, area:read-api, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement preview_fund to report whether a deposit would be accepted

### Description
A client sizing a deposit against `fund` in [`escrow/src/lib.rs`](escrow/src/lib.rs) must replicate every `fund_impl` precondition off-chain — status open, not legal-held, not deadline-expired, allowlist gate, min-contribution floor, per-investor cap, and unique-funder cap — to know whether a given `(investor, amount)` will succeed. This logic is duplicated in every front-end and drifts from the contract as guards evolve.

This issue adds a pure preview view that runs the same checks read-only and reports the outcome.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `preview_fund(env, investor: Address, amount: i128) -> u32` returning `0` for "would succeed" or the numeric `EscrowError` code that `fund` would raise first, evaluating the guards in the exact same order as `fund_impl`.
- Pure read: no auth, no state mutation; must not call `require_auth`.
- Document that this is advisory — `fund` remains the source of truth and can still revert under racing state changes.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-preview-fund`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `preview_fund` reusing the same guard predicates as `fund_impl`.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — each rejection reason returns its code, a valid deposit returns 0, ordering matches `fund` failures.
  - **Add documentation:** update [`docs/escrow-read-api.md`](docs/escrow-read-api.md).
  - Include NatSpec-style `///` comments on the view and the code mapping.
  - Validate security: pure read, no mutation, ordering matches enforcement.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: below floor, over per-investor cap, unique cap reached, not allowlisted, deadline passed, legal hold, closed status, valid deposit.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add preview_fund read view reporting the first fund guard failure with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Emit a settlement-coupon snapshot event carrying the computed pool at settle()"
labels: type:enhancement, area:events, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add the realized settlement pool to the settle() event payload

### Description
`settle()` in [`escrow/src/lib.rs`](escrow/src/lib.rs) flips `status` to 2 and emits `EscrowSettled` with `funded_amount`, `yield_bps`, and `maturity`, but it does **not** announce the computed `settle_pool` (principal plus coupon) that investors are collectively owed. Indexers must re-derive the coupon from `funded_amount × yield_bps / 10_000`, duplicating the on-chain rounding and risking divergence from `compute_investor_payout`.

This issue adds the realized pool to the settlement event additively.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Extend `EscrowSettled` (append-only field) with the computed `settle_pool` derived from the `FundingCloseSnapshot.total_principal` and base `yield_bps`, using the same `checked_*` arithmetic as `compute_investor_payout`.
- Keep existing topics and fields stable per the additive policy (ADR-007); compute the pool once during `settle`.
- If a `get_settlement_pool` view exists, reuse its math to guarantee identical rounding.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-settle-pool-event`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — extend `EscrowSettled` and populate it in `settle`.
  - **Write comprehensive tests in:** [`escrow/src/tests/settlement.rs`](escrow/src/tests/settlement.rs) — event pool equals principal+coupon, zero-yield case, rounding floor.
  - **Add documentation:** update [`docs/EVENT_SCHEMA.md`](docs/EVENT_SCHEMA.md) and [`docs/escrow-events.md`](docs/escrow-events.md).
  - Include NatSpec-style `///` comments on the new field.
  - Validate security: additive field, identical rounding, overflow-safe.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: zero yield, max yield, no-maturity escrow, large principal.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add realized settlement pool to EscrowSettled event payload with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a view exposing the contract's live funding-token balance for reconciliation"
labels: type:feature, area:read-api, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement get_token_balance to surface on-chain custody for audits

### Description
`sweep_terminal_dust` and `refund` in [`escrow/src/lib.rs`](escrow/src/lib.rs) read the contract's funding-token balance via `TokenClient::balance(this)` to enforce the liability floor and move funds, but there is **no public view** that returns this balance. Auditors reconciling on-chain custody against `funded_amount` and `distributed_principal` must construct a token client call themselves and know the funding-token address, with no single contract-level answer.

This issue adds a read returning the contract's current funding-token balance.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_token_balance(env) -> i128` reading the bound `DataKey::FundingToken` and returning `TokenClient::balance(env.current_contract_address())`; raise `FundingTokenNotSet` if uninitialized (matching the existing getter behavior).
- Pure read, no auth, no mutation.
- Document the reconciliation relationship: balance versus `funded_amount - distributed_principal` for cancelled escrows.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-token-balance-view`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `get_token_balance` view.
  - **Write comprehensive tests in:** [`escrow/src/tests/integration.rs`](escrow/src/tests/integration.rs) — register a SAC, mint to the contract, assert the view matches the token balance after refund/sweep.
  - **Add documentation:** update [`docs/escrow-read-api.md`](docs/escrow-read-api.md) and [`docs/adr/ADR-006-dust-sweep-and-token-safety.md`](docs/adr/ADR-006-dust-sweep-and-token-safety.md).
  - Include NatSpec-style `///` comments on the view.
  - Validate security: pure read, correct token resolution, no mutation.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: zero balance, post-mint balance, balance after a sweep, uninitialized escrow.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add get_token_balance reconciliation view with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Validate the invoice amount against a maximum bound at init to prevent overflow-prone configs"
labels: type:security, area:init-validation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Bound the init amount so downstream coupon math cannot overflow

### Description
`init` in [`escrow/src/lib.rs`](escrow/src/lib.rs) validates only `amount > 0` and `yield_bps` in `0..=10_000`, but places **no upper bound** on `amount`. Because `compute_investor_payout` later computes `total_principal × yield_bps` and `contribution × settle_pool` with `i128` checked arithmetic, an extreme `amount` near `i128::MAX` makes settlement-time payout computation revert with `ComputePayoutArithmeticOverflow` for every investor — funds become un-claimable through the on-chain view, discovered only after the escrow is fully funded.

This issue rejects implausibly large amounts at init so overflow is impossible by construction.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a `MAX_INVOICE_AMOUNT` constant chosen so `amount × 10_000` and `amount × settle_pool` cannot overflow `i128` for any valid yield; reject `amount > MAX_INVOICE_AMOUNT` at `init` with a new append-only typed error (e.g. `AmountExceedsMax`).
- Document the bound's derivation relative to the `compute_investor_payout` formula.
- Preserve all existing `init` validation and ordering; this is an additional guard.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-init-amount-bound`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — constant, validation, error.
  - **Write comprehensive tests in:** [`escrow/src/tests/init.rs`](escrow/src/tests/init.rs) — accept at-bound amount, reject above-bound, and assert a near-bound funded escrow's `compute_investor_payout` never overflows.
  - **Add documentation:** update [`docs/escrow-numeric-model.md`](docs/escrow-numeric-model.md).
  - Include NatSpec-style `///` comments on the constant and error.
  - Validate security: no overflow path reachable from any valid init.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: exactly at bound, one over bound, max yield with large amount.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`fix: bound init amount to prevent settlement payout overflow with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Bound the legal-hold clear delay at init to prevent an unclearable hold"
labels: type:security, area:legal-hold, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Validate legal_hold_clear_delay against an upper bound at init

### Description
`init` in [`escrow/src/lib.rs`](escrow/src/lib.rs) accepts an optional `legal_hold_clear_delay` and stores it in `DataKey::LegalHoldClearDelay` when `> 0`, with no upper-bound validation. The clear flow (`request_clear_legal_hold` then `set_legal_hold(false)`) gates on `now >= clearable_at`, where `clearable_at = now + delay` and the addition is overflow-guarded by `LegalHoldClearDelayOverflow`. A mistaken delay just below the overflow threshold (e.g. decades) makes a placed legal hold effectively permanent — funds frozen with no realistic clear path short of an admin handover.

This issue adds a sane upper bound on the clear delay at init.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a `MAX_LEGAL_HOLD_CLEAR_DELAY_SECS` constant and reject `delay > MAX_LEGAL_HOLD_CLEAR_DELAY_SECS` at `init` with a new append-only typed error (e.g. `LegalHoldClearDelayTooLarge`).
- Preserve the `delay == 0` / absent "immediate clear" semantics and the existing overflow guard.
- Reference the ledger-time trust model and the legal-hold timing in the docs.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-clear-delay-bound`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — constant, validation, error.
  - **Write comprehensive tests in:** [`escrow/src/tests/legal_hold.rs`](escrow/src/tests/legal_hold.rs) — accept zero/in-window delay, reject above-bound delay.
  - **Add documentation:** update [`docs/escrow-legal-hold.md`](docs/escrow-legal-hold.md) and [`docs/adr/ADR-004-legal-hold.md`](docs/adr/ADR-004-legal-hold.md).
  - Include NatSpec-style `///` comments on the constant and error.
  - Validate security: no permanently-unclearable hold from a bad delay.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: zero delay, exactly at bound, above bound.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`fix: bound legal-hold clear delay at init to prevent an unclearable hold with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Reject fund_batch entries containing duplicate investor addresses"
labels: type:security, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Guard fund_batch against intra-batch duplicate addresses

### Description
`fund_batch` in [`escrow/src/lib.rs`](escrow/src/lib.rs) applies per-entry funding validation and is bounded by `MAX_FUND_BATCH = 50`, but it does not reject a batch that lists the **same investor address twice**. Two entries for one address are processed as sequential deposits, which can silently bypass a caller's intent (a single intended deposit applied twice), interact confusingly with the per-investor cap mid-batch, and complicate auditing the unique-funder count transition. Whether duplicates are valid is undocumented and untested.

This issue makes the duplicate-handling contract explicit and safe.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Reject any batch containing a repeated address with a new append-only typed error (e.g. `FundingBatchDuplicateInvestor`) detected before any state mutation, so the batch is atomic and intent-preserving.
- Keep `MAX_FUND_BATCH` bounded so the duplicate scan stays within CPU limits.
- Document that repeat deposits for one investor must be separate single `fund` calls, matching the tiered second-deposit discipline.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-fund-batch-dedupe`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — duplicate detection and error.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — duplicate batch rejected with no partial state, unique batch succeeds, boundary at `MAX_FUND_BATCH`.
  - **Add documentation:** update [`docs/escrow-lifecycle.md`](docs/escrow-lifecycle.md) and the README entrypoint table.
  - Include NatSpec-style `///` comments on the guard and error.
  - Validate security: atomic rejection, no partial-state corruption, bounded scan.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: adjacent duplicates, non-adjacent duplicates, all-unique batch, single-element batch.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`fix: reject duplicate investor addresses in fund_batch with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Persist a settlement timestamp at settle() and expose it through a view"
labels: type:feature, area:settlement, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Record the ledger time of settlement for audit and claim accounting

### Description
`settle()` in [`escrow/src/lib.rs`](escrow/src/lib.rs) transitions `status` to 2 and emits `EscrowSettled`, but it does not **persist** the ledger timestamp at which settlement occurred. The `FundingCloseSnapshot` captures the funding-close moment, yet the settlement moment is recoverable only by replaying events. Claim accounting, dispute resolution, and reporting all need an authoritative on-chain "settled at" value that survives event pruning.

This issue stores the settlement timestamp and exposes it via a read.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- In `settle`, write a new `DataKey::SettledAt` (`u64`) with `env.ledger().timestamp()` once at the status 1→2 transition; do not overwrite if already set (settle is one-shot from status 1).
- Add `get_settled_at(env) -> Option<u64>` returning the stored timestamp, `None` before settlement.
- Keep the additive-key policy (ADR-007): legacy settled instances return `None`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-settled-at-timestamp`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `DataKey::SettledAt`, write in `settle`, `get_settled_at` view.
  - **Write comprehensive tests in:** [`escrow/src/tests/settlement.rs`](escrow/src/tests/settlement.rs) — timestamp recorded at settle, `None` before, value matches Ledger testutils time.
  - **Add documentation:** update [`docs/escrow-ledger-time.md`](docs/escrow-ledger-time.md) and [`docs/escrow-data-model.md`](docs/escrow-data-model.md).
  - Include NatSpec-style `///` comments on the key and view.
  - Validate security: write-once at settle, pure read, no mutation in the getter.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: pre-settle None, post-settle value, no-maturity vs maturity escrow.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: persist settlement timestamp and expose get_settled_at with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a batch refund entrypoint for cancelled escrows"
labels: type:feature, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement a bounded refund_batch for cancelled-escrow recovery

### Description
`refund` in [`escrow/src/lib.rs`](escrow/src/lib.rs) returns principal to one investor per transaction in a cancelled (status 4) escrow, zeroing the contribution and incrementing `DistributedPrincipal`. For a cancelled raise with many funders, a relayer must submit one transaction per address, mirroring the operational burden that motivated `fund_batch`. There is no bounded batch recovery path.

This issue adds a bounded `refund_batch` applying identical per-investor gates and the liability-floor accounting.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `refund_batch(env, investors: Vec<Address>)` bounded by a `MAX_REFUND_BATCH` constant (mirroring `MAX_FUND_BATCH`); reject empty/oversized batches with append-only typed errors.
- Each entry must pass the cancelled-status gate and `NoContributionToRefund`, require per-investor auth, perform the same checks-effects-interactions zeroing + transfer, and increment `DistributedPrincipal`; skip already-refunded entries without failing the whole batch.
- Emit one `InvestorRefundedEvt` per newly-refunded investor, preserving single-`refund` semantics.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-refund-batch`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `refund_batch`, `MAX_REFUND_BATCH`.
  - **Write comprehensive tests in:** [`escrow/src/tests/integration.rs`](escrow/src/tests/integration.rs) — batch equals N single refunds, skip-refunded, cap rejection, `DistributedPrincipal` accumulation, liability floor preserved.
  - **Add documentation:** update [`docs/escrow-lifecycle.md`](docs/escrow-lifecycle.md) and the README entrypoint table.
  - Include NatSpec-style `///` comments.
  - Validate security: per-investor auth, idempotency, bounded work, floor invariant.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: empty batch, oversized batch, mixed refunded/unrefunded, non-cancelled status.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add bounded refund_batch entrypoint for cancelled-escrow recovery with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Emit a structured event when an admin updates the maturity timestamp on a no-op change"
labels: type:enhancement, area:settlement, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Guard update_maturity against no-op writes and noisy events

### Description
`update_maturity` in [`escrow/src/lib.rs`](escrow/src/lib.rs) writes the new maturity and emits `MaturityUpdatedEvent` with `old_maturity` and `new_maturity` unconditionally — even when `new_maturity == old_maturity`. This produces a misleading "change" event and an unnecessary instance-storage write for a no-op call, polluting the audit trail that indexers consume and wasting a write on long-dated escrows.

This issue makes `update_maturity` reject (or skip) a no-op change, consistent with `propose_admin`'s `NewAdminSameAsCurrent` guard.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Reject `new_maturity == old_maturity` with a new append-only typed error (e.g. `MaturityUnchanged`), mirroring the no-op guard pattern used by `propose_admin` and `rotate_beneficiary` (`NewSmeSameAsCurrent`).
- Preserve admin auth, the open-state gate (`MaturityUpdateNotOpen`), and the event on a genuine change.
- Document the new guard alongside the other no-op guards in the auth/ADR docs.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-maturity-noop-guard`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — no-op guard and error.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — same-value rejected, genuine change emits the event, non-admin and closed-state still rejected.
  - **Add documentation:** update [`docs/escrow-error-messages.md`](docs/escrow-error-messages.md).
  - Include NatSpec-style `///` comments on the guard and error.
  - Validate security: no behavior change beyond the no-op rejection.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: equal maturity, increased maturity, decreased maturity, closed status.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: reject no-op update_maturity to avoid misleading events with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a view returning the effective yield bps an investor would receive at settlement"
labels: type:feature, area:read-api, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement get_effective_yield_bps resolving tier-or-base for an investor

### Description
`compute_investor_payout` in [`escrow/src/lib.rs`](escrow/src/lib.rs) internally resolves an investor's effective yield as `InvestorEffectiveYield(investor).unwrap_or(escrow.yield_bps)` — a tier-specific yield set at a tiered first deposit, or the escrow base yield otherwise. The standalone getter `get_investor_yield_bps` returns only the stored per-investor value (defaulting to a raw value), so callers cannot directly read the **resolved** effective rate the payout math will actually use without re-implementing the fallback.

This issue exposes the resolved effective yield as a dedicated view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_effective_yield_bps(env, investor: Address) -> i64` returning `InvestorEffectiveYield(investor)` when set, otherwise the escrow base `yield_bps`, matching the exact resolution in `compute_investor_payout`.
- Pure read, no auth, no mutation.
- Document the difference from `get_investor_yield_bps` (stored vs resolved) to avoid integrator confusion.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-effective-yield-view`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `get_effective_yield_bps` view.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — tiered investor returns tier yield, non-tiered returns base, unknown investor returns base, matches payout resolution.
  - **Add documentation:** update [`docs/escrow-read-api.md`](docs/escrow-read-api.md) and [`docs/adr/ADR-005-tiered-yield.md`](docs/adr/ADR-005-tiered-yield.md).
  - Include NatSpec-style `///` comments distinguishing stored vs resolved yield.
  - Validate security: pure read, identical resolution to payout math.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: tiered deposit, base-only deposit, non-participant, zero base yield.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add get_effective_yield_bps resolved-yield view with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a view reporting whether the funding target has been reached"
labels: type:enhancement, area:read-api, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement is_fully_funded surfacing the close condition

### Description
The open→funded transition in `fund_impl` ([`escrow/src/lib.rs`](escrow/src/lib.rs)) fires when `funded_amount >= funding_target`, but front-ends must read both fields (via `get_escrow`) and compare them client-side to know whether a raise has hit its target — re-deriving the exact close predicate. Over-funding past the target is permitted while open, so a naive equality check is wrong; the contract should expose the authoritative predicate.

This issue adds a pure `is_fully_funded` view returning the close condition.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `is_fully_funded(env) -> bool` returning `funded_amount >= funding_target` from the loaded escrow, matching the `fund_impl` close predicate exactly.
- Pure read, no auth, no mutation.
- Document that a `true` result before status flips to 1 cannot occur (the transition is atomic), so this is consistent with `status == 1` once funded.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-is-fully-funded-view`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `is_fully_funded` view.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — false when under target, true at exact target and when over-funded, consistency with status.
  - **Add documentation:** update [`docs/escrow-read-api.md`](docs/escrow-read-api.md).
  - Include NatSpec-style `///` comments on the view.
  - Validate security: pure read, predicate matches the close transition.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: unfunded, partial, exact, over-funded.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add is_fully_funded read view with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add tests asserting migrate() rejects every version path with the documented typed errors"
labels: type:test, area:upgradeability, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test the migrate() no-migration-path contract end to end

### Description
`migrate` in [`escrow/src/lib.rs`](escrow/src/lib.rs) intentionally implements **no** migration logic: every call requires admin auth, reads `DataKey::Version`, and terminates with one of `MigrationVersionMismatch`, `AlreadyCurrentSchemaVersion`, or `NoMigrationPath` against the current `SCHEMA_VERSION = 6`. This "explicit no-op" contract — and the deliberate auth-before-version-check ordering — has no dedicated coverage proving each branch and that no storage write ever occurs.

This issue adds an exhaustive `migrate` test suite.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert `migrate(from_version)` with a mismatching `from_version` raises `MigrationVersionMismatch`; with `from_version >= SCHEMA_VERSION` raises `AlreadyCurrentSchemaVersion`; with `from_version < SCHEMA_VERSION` (and matching stored version) raises `NoMigrationPath`.
- Assert a non-admin caller is rejected before any version check (auth-first ordering) via `mock_auths`.
- Assert `DataKey::Version` is unchanged after every failed call.
- No production change unless a real gap surfaces.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-migrate-paths`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — each typed-error branch via `try_migrate`, auth-first ordering, version immutability.
  - **Add documentation:** cross-link scenarios in [`docs/OPERATOR_RUNBOOK.md`](docs/OPERATOR_RUNBOOK.md).
  - Include NatSpec-style `///` comments on helpers.
  - Validate security: admin-gated, no storage mutation on any path.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: mismatching from_version, equal-to-current, below-current, non-admin caller.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add coverage for migrate no-migration-path branches and auth ordering`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add tests for the upgrade() entrypoint state preservation and admin gating"
labels: type:test, area:upgradeability, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test upgrade() WASM replacement preserves state and rejects non-admins

### Description
`upgrade(new_wasm_hash)` in [`escrow/src/lib.rs`](escrow/src/lib.rs) replaces the deployed WASM via the deployer after `load_escrow_require_admin`, with a documented guarantee that no persistent keys, escrow records, or balances are modified. This funds-adjacent code-replacement path needs dedicated coverage proving stored `DataKey::Escrow` and per-investor state survive an upgrade and that non-admins are rejected before the deployer call.

This issue adds an upgrade test suite using a second registered WASM.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Register a second test WASM, fund an escrow, call `upgrade`, then assert `get_escrow`, contributions, and snapshot are preserved post-upgrade.
- Assert a non-admin caller is rejected via `mock_auths` and the WASM is unchanged.
- Assert the `upgrade` event (`symbol_short!("upgrade")`) is emitted with the expected payload.
- No production change unless a real gap surfaces.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-upgrade-entrypoint`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — upgrade preserves state, non-admin rejection, event payload.
  - **Add documentation:** cross-link scenarios in [`docs/OPERATOR_RUNBOOK.md`](docs/OPERATOR_RUNBOOK.md).
  - Include NatSpec-style `///` comments on helpers.
  - Validate security: admin-only, state survives, event correctness.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: upgrade then read preserved state, unauthorized caller, repeated upgrade.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add upgrade entrypoint state-preservation and admin-gating coverage`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add tests for fund_batch equivalence, ordering, and the funded-target transition mid-batch"
labels: type:test, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test fund_batch matches N single funds and closes the target correctly

### Description
`fund_batch` in [`escrow/src/lib.rs`](escrow/src/lib.rs) applies per-entry `fund` validation across up to `MAX_FUND_BATCH = 50` entries with `FundingBatchEmpty`/`FundingBatchTooLarge` bounds, and must produce state identical to calling `fund` once per entry — including the open→funded status flip and the `FundingCloseSnapshot` write when the cumulative total crosses `funding_target` mid-batch. This equivalence and the mid-batch transition have no dedicated coverage.

This issue adds a focused `fund_batch` test suite.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert a batch of N entries yields the same `funded_amount`, per-investor contributions, and `UniqueFunderCount` as N sequential single `fund` calls.
- Assert the status flips to 1 and the snapshot is written exactly once when the running total crosses `funding_target` mid-batch.
- Assert `FundingBatchEmpty` and `FundingBatchTooLarge` at the boundaries, and per-investor cap/floor still enforced inside the batch.
- No production change unless a real gap surfaces.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-fund-batch-equivalence`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — equivalence, mid-batch close, bound rejections, cap/floor inside batch.
  - **Add documentation:** cross-link scenarios in [`docs/escrow-lifecycle.md`](docs/escrow-lifecycle.md).
  - Include NatSpec-style `///` comments on helpers.
  - Validate security: batch equals single-call semantics, single snapshot write.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: empty, exactly `MAX_FUND_BATCH`, one over, mid-batch target crossing, over-funding.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add fund_batch equivalence and mid-batch funded-transition coverage`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add tests for the funding deadline gate and is_funding_expired transitions"
labels: type:test, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test the funding_deadline enforcement across ledger time

### Description
The funding deadline in [`escrow/src/lib.rs`](escrow/src/lib.rs) is validated at `init` (`deadline > now`), stored in `DataKey::FundingDeadline`, surfaced by `get_funding_deadline`/`is_funding_expired`, and enforced in the fund path via `FundingDeadlinePassed`. The end-to-end behavior across the deadline boundary — fund accepted before, rejected after, and `is_funding_expired` flipping at the exact second — has no dedicated time-based coverage.

This issue adds deadline tests using the `Ledger` testutils to advance time.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert `init` rejects a deadline already in the past; accepts a future deadline.
- Assert `fund` succeeds before the deadline and is rejected with `FundingDeadlinePassed` after it, using Ledger testutils to advance `timestamp()`.
- Assert `is_funding_expired` is `false` before and `true` at/after the deadline; absent deadline means never expired.
- No production change unless a real gap surfaces.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-funding-deadline-gate`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — deadline boundary fund acceptance/rejection, `is_funding_expired` transitions, no-deadline case.
  - **Add documentation:** cross-link scenarios in [`docs/escrow-ledger-time.md`](docs/escrow-ledger-time.md).
  - Include NatSpec-style `///` comments on helpers.
  - Validate security: deadline cannot trap funded escrows; ledger-time semantics correct.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: no deadline, exactly at deadline, after deadline, init with past deadline.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add funding deadline gate and is_funding_expired transition coverage`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add tests asserting the permissionless bump_ttl extends all per-investor persistent keys"
labels: type:test, area:storage-ttl, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test bump_ttl extends instance and per-investor persistent TTLs

### Description
`bump_ttl(allowlisted)` in [`escrow/src/lib.rs`](escrow/src/lib.rs) is the permissionless TTL top-up: it extends instance storage and, per supplied address, extends `InvestorAllowlisted`, `InvestorContribution`, `InvestorEffectiveYield`, `InvestorClaimNotBefore`, and `InvestorClaimed` by `PERSISTENT_TTL_MIN_EXTENSION_LEDGERS`. This funds-safety keepalive — which prevents archival of live positions on long-dated escrows — has no dedicated test proving each per-investor key survives past the prior horizon.

This issue adds TTL-extension coverage using the Ledger testutils.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Fund investors, advance the ledger near the archival horizon, call `bump_ttl` with the funder set, and assert each per-investor persistent key (`InvestorContribution`, `InvestorEffectiveYield`, `InvestorClaimNotBefore`, `InvestorClaimed`, `InvestorAllowlisted`) remains readable past the prior TTL.
- Assert `bump_ttl` is callable by any address (permissionless) and mutates no other state.
- No production change unless a real gap surfaces.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-bump-ttl-coverage`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — per-key TTL survival, permissionless caller, no state change.
  - **Add documentation:** cross-link scenarios in [`docs/escrow-gas-storage-notes.md`](docs/escrow-gas-storage-notes.md) and [`docs/adr/ADR-007-storage-key-evolution.md`](docs/adr/ADR-007-storage-key-evolution.md).
  - Include NatSpec-style `///` comments on helpers.
  - Validate security: live positions are not archived; no unintended mutation.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: empty allowlisted vec, multiple funders, near-horizon advancement.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add bump_ttl per-investor persistent TTL extension coverage`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add tests for the two-step admin handover propose/accept lifecycle"
labels: type:test, area:admin, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test propose_admin and accept_admin authority transfer and guards

### Description
The two-step handover in [`escrow/src/lib.rs`](escrow/src/lib.rs) — `propose_admin` (current admin, `NewAdminSameAsCurrent` guard) writing `DataKey::PendingAdmin`, then `accept_admin` (successor auth, `NoPendingAdmin` guard) promoting and clearing the pending key — is the contract's key-rotation safety mechanism. It needs explicit coverage proving authority changes only after both parties authorize and that the old admin loses control afterward.

This issue adds an admin-handover lifecycle test suite.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert `propose_admin` requires current-admin auth, rejects `NewAdminSameAsCurrent`, and sets `get_pending_admin`.
- Assert `accept_admin` requires the pending address's auth, rejects `NoPendingAdmin` when none pending, promotes the admin, clears the pending key, and emits `AdminTransferredEvent`.
- Assert the old admin can no longer perform admin-gated actions and the new admin can.
- No production change unless a real gap surfaces.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-admin-handover-lifecycle`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — propose/accept flow, guards, old-admin lockout, new-admin authority.
  - **Add documentation:** cross-link scenarios in [`docs/OPERATOR_RUNBOOK.md`](docs/OPERATOR_RUNBOOK.md).
  - Include NatSpec-style `///` comments on helpers.
  - Validate security: authority changes only after dual authorization.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: same-address proposal, accept with no proposal, accept by wrong address, old-admin post-handover rejection.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add propose_admin/accept_admin handover lifecycle coverage`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add tests verifying every emitted contractevent uses a unique symbol topic name"
labels: type:test, area:events, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test that no two distinct events share a symbol_short topic

### Description
[`escrow/src/lib.rs`](escrow/src/lib.rs) emits many `#[contractevent]` types, each with a hand-written `symbol_short!(...)` name. Some names are reused intentionally (the single and batch allowlist writes both emit `al_set` for `InvestorAllowlistChanged`), but there is no test guarding against an **accidental** collision where two semantically different events share a topic — which would make them indistinguishable to indexers. Hand-maintained symbols are easy to duplicate by mistake as new events are added.

This issue adds an event-topic uniqueness/consistency test exercising every entrypoint.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Drive each event-emitting entrypoint in tests, capture emitted events via `env.events().all()`, and assert that each distinct event struct maps to its expected, intended symbol name.
- Explicitly document and assert the intentional `al_set` reuse between single and batch allowlist writes so it is a deliberate, tested exception rather than an accident.
- No production change unless an unintended collision surfaces (then file/fix separately).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-event-topic-uniqueness`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if an unintended collision surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — emitted-symbol assertions per entrypoint, documented reuse exception.
  - **Add documentation:** cross-link the symbol map in [`docs/EVENT_SCHEMA.md`](docs/EVENT_SCHEMA.md).
  - Include NatSpec-style `///` comments on helpers.
  - Validate security: indexer-facing topics are intentional and stable.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: events with Option fields, the intentional allowlist reuse, multi-topic events.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: assert event symbol topics are intentional and collision-free`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add proptest invariants for the sweep_terminal_dust liability floor in cancelled escrows"
labels: type:test, area:treasury, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test the dust-sweep liability-floor invariant under random refund sequences

### Description
`sweep_terminal_dust` in [`escrow/src/lib.rs`](escrow/src/lib.rs) enforces, for cancelled (status 4) escrows, that `balance - sweep_amt >= funded_amount - distributed_principal` so the treasury can never sweep principal still owed to investors who have not yet refunded. `distributed_principal` accumulates as each `refund` runs. This safety invariant — across arbitrary interleavings of refunds and sweep amounts — is not exercised by randomized tests.

This issue adds property tests asserting the floor holds for all orderings.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Generate random investor sets, contributions, partial-refund subsets, and sweep amounts; cancel the escrow; assert every accepted sweep keeps `balance_after >= funded_amount - distributed_principal`, and that over-large sweeps fail with `SweepExceedsLiabilityFloor`.
- Assert sweeps in settled (2) / withdrawn (3) states are unaffected by the floor (distributed_principal stays 0 there).
- Persist any counterexamples to [`escrow/proptest-regressions/test.txt`](escrow/proptest-regressions/test.txt).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-dust-floor-properties`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if an invariant violation is found.
  - **Write comprehensive tests in:** [`escrow/src/tests/properties.rs`](escrow/src/tests/properties.rs) — randomized refund/sweep sequences with floor assertions.
  - **Add documentation:** note the invariant in [`docs/adr/ADR-006-dust-sweep-and-token-safety.md`](docs/adr/ADR-006-dust-sweep-and-token-safety.md).
  - Include NatSpec-style `///` comments on generators.
  - Validate security: treasury can never sweep owed principal.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: no refunds, all refunded, partial refunds, sweep exactly at the floor, sweep over the floor.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add proptest invariants for the dust-sweep liability floor`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add tests for partial_settle early-funding promotion and snapshot capture"
labels: type:test, area:settlement, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test partial_settle dual-caller authorization and snapshot semantics

### Description
`partial_settle(caller)` in [`escrow/src/lib.rs`](escrow/src/lib.rs) lets either the SME or the admin promote an under-target open escrow to funded (status 1) early, writing the `FundingCloseSnapshot` if absent and emitting `EscrowPartialSettle`. It uniquely accepts two possible callers and is blocked by legal hold and a non-open status. This funds-routing-relevant early-close path needs coverage proving the caller set, the guards, and that the snapshot captures the current `funded_amount`.

This issue adds a `partial_settle` test suite.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert `partial_settle` succeeds for the SME and for the admin, and is rejected for any other caller via `mock_auths`.
- Assert it is blocked while a legal hold is active and when `status != 0`.
- Assert the snapshot is written exactly once (not overwritten if a prior snapshot exists) and captures the current `funded_amount` and `funding_target`; assert `compute_investor_payout` works against the early snapshot after settle.
- No production change unless a real gap surfaces.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-partial-settle-coverage`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/settlement.rs`](escrow/src/tests/settlement.rs) — SME/admin callers, wrong caller, hold/status guards, snapshot capture.
  - **Add documentation:** cross-link scenarios in [`docs/escrow-snapshot.md`](docs/escrow-snapshot.md).
  - Include NatSpec-style `///` comments on helpers.
  - Validate security: only SME/admin can early-close; snapshot write-once.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: SME caller, admin caller, third-party caller, hold active, non-open status, pre-existing snapshot.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add partial_settle dual-caller and snapshot-capture coverage`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add tests for the invoice_id string validation charset and length rules"
labels: type:test, area:init-validation, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test validate_invoice_id_string boundary and charset rejections

### Description
`init` in [`escrow/src/lib.rs`](escrow/src/lib.rs) routes its `invoice_id: String` through `validate_invoice_id_string`, which enforces a length bound (`InvoiceIdInvalidLength`) and a charset restriction (`InvoiceIdInvalidCharset`) before converting to the stored `Symbol`. These input-validation rules — the first line of defense against malformed identifiers that flow into every event topic — have no dedicated boundary coverage at the exact length limits and disallowed characters.

This issue adds focused invoice-id validation tests.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert `init` accepts a minimal valid id and an at-maximum-length id, and rejects an empty id and an over-length id with `InvoiceIdInvalidLength`.
- Assert rejection with `InvoiceIdInvalidCharset` for ids containing disallowed characters, and acceptance of every allowed character class.
- Assert the stored `invoice_id` symbol round-trips into `get_escrow` and event payloads.
- No production change unless a real gap surfaces.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-invoice-id-validation`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/init.rs`](escrow/src/tests/init.rs) — length boundaries, charset acceptance/rejection, round-trip.
  - **Add documentation:** cross-link the rules in [`docs/escrow-data-model.md`](docs/escrow-data-model.md).
  - Include NatSpec-style `///` comments on helpers.
  - Validate security: malformed identifiers cannot be initialized.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: empty, max length, over length, disallowed char, all allowed classes.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add invoice_id charset and length validation coverage`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add tests confirming claim_investor_payout idempotency under repeated and racing calls"
labels: type:test, area:settlement, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test the claim_investor_payout mark-before-emit idempotency contract

### Description
`claim_investor_payout` in [`escrow/src/lib.rs`](escrow/src/lib.rs) documents a precise guard order: legal-hold gate, `investor.require_auth()`, single contribution fetch (`NoContributionToClaim`), settled-status gate (`InvestorClaimNotSettled`), `not_before` lock (`InvestorCommitmentLockNotExpired`), then an idempotent early-return on the `InvestorClaimed` marker which is set **before** the event is emitted. This mark-before-emit no-double-emit property and the full guard ordering need explicit coverage.

This issue adds an idempotency and guard-ordering test suite for the claim path.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert a first claim emits `InvestorPayoutClaimed` and flips `is_investor_claimed`; a second claim is a silent no-op (no second event).
- Assert each guard rejects in the documented order: hold active, no contribution, unsettled escrow, unexpired commitment lock.
- Assert a non-investor signer is rejected via `mock_auths`.
- No production change unless a real gap surfaces.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-claim-idempotency`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/settlement.rs`](escrow/src/tests/settlement.rs) — single claim, double-claim no-op, each guard rejection, auth.
  - **Add documentation:** cross-link scenarios in [`docs/escrow-ledger-time.md`](docs/escrow-ledger-time.md).
  - Include NatSpec-style `///` comments on helpers.
  - Validate security: no double-emit, guard ordering matches docs.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: repeat claim, hold mid-flow, lock not expired, non-participant, wrong signer.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add claim_investor_payout idempotency and guard-ordering coverage`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Document the two-step admin handover and key-rotation recovery procedure"
labels: type:docs, area:admin, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document propose_admin/accept_admin handover and the deprecated shim

### Description
Admin authority in [`escrow/src/lib.rs`](escrow/src/lib.rs) rotates via a two-step `propose_admin` → `accept_admin` flow (with the `transfer_admin` shim now deprecated), and the admin is the only role that can clear a legal hold — making correct handover a funds-safety-critical operator procedure. The relationship between proposal, acceptance, the `PendingAdmin` key, the deprecation of `transfer_admin`, and the legal-hold recovery lever is not captured in one authoritative operator document.

This issue produces a complete, code-accurate admin-handover document.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Document `propose_admin` (current-admin auth, `NewAdminSameAsCurrent`), `accept_admin` (successor auth, `NoPendingAdmin`), and the `PendingAdmin` lifecycle and events.
- Document that `transfer_admin` is deprecated and only proposes; advise migrating to the explicit two-step calls.
- Tie the handover to the legal-hold recovery path (only the new admin can clear a persisted hold after handover).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-admin-handover`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only rustdoc corrections if inline docs drift.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — an anchoring test that the post-handover admin can clear a legal hold the old admin set.
  - **Add documentation:** expand [`docs/OPERATOR_RUNBOOK.md`](docs/OPERATOR_RUNBOOK.md) and reconcile with [`docs/adr/ADR-002-auth-boundaries.md`](docs/adr/ADR-002-auth-boundaries.md).
  - Include NatSpec-style `///` comments where clarified.
  - Validate security: documented flow matches enforced auth.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: handover then hold-clear by new admin, deprecated shim path.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`docs: document admin handover and key-rotation recovery with anchoring test`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Document the contract WASM upgrade procedure and additive-key compatibility rules"
labels: type:docs, area:upgradeability, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document upgrade() versus migrate() and the additive-key safety contract

### Description
[`escrow/src/lib.rs`](escrow/src/lib.rs) exposes both `upgrade(new_wasm_hash)` (admin-gated code replacement, preserving storage, not bumping `SCHEMA_VERSION`) and `migrate(from_version)` (currently a deliberate no-op that always errors). Their division of labor — when to upgrade, when to migrate, why upgrading to a WASM that reorders or removes `DataKey` variants corrupts state, and how `SCHEMA_VERSION = 6` and the additive-key policy (ADR-007) interact — is easy to misread and is a high-risk operator action.

This issue produces a precise upgrade/migration operator guide.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Document the `upgrade` authorization, state-preservation guarantee, the requirement to verify additive-only `DataKey` changes, and the post-upgrade `migrate` step if schema changes accompany the new WASM.
- Document `migrate`'s current "explicit no-op" behavior and its three typed-error branches, and the procedure to implement a real migration path.
- Cross-reference `SCHEMA_VERSION`, `DataKey::Version`, and ADR-007.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-upgrade-migration-guide`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only rustdoc corrections if inline docs drift.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — an anchoring test that state survives upgrade and migrate errors as documented.
  - **Add documentation:** expand [`docs/OPERATOR_RUNBOOK.md`](docs/OPERATOR_RUNBOOK.md) and reconcile with [`docs/adr/ADR-007-storage-key-evolution.md`](docs/adr/ADR-007-storage-key-evolution.md).
  - Include NatSpec-style `///` comments where clarified.
  - Validate security: documented behavior matches enforced rules.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: upgrade preserves state, migrate error branches, additive vs destructive key changes.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`docs: document upgrade vs migrate and additive-key compatibility with anchoring test`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Document the storage TTL and rent model including bump_ttl and write-time extensions"
labels: type:docs, area:storage-ttl, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the instance and persistent TTL strategy for long-dated escrows

### Description
[`escrow/src/lib.rs`](escrow/src/lib.rs) mixes instance storage (escrow, version, snapshot, caps, legal hold) and per-address persistent storage (`InvestorContribution`, `InvestorEffectiveYield`, `InvestorClaimNotBefore`, `InvestorClaimed`, `InvestorAllowlisted`), with the permissionless `bump_ttl` entrypoint extending both by `INSTANCE_TTL_MIN_EXTENSION_LEDGERS` / `PERSISTENT_TTL_MIN_EXTENSION_LEDGERS`. Under Soroban rent/archival, a long-dated escrow whose entries expire can default reads to zero/false — silently erasing positions or flipping the allowlist gate. This funds-safety-relevant model needs an authoritative document.

This issue produces a complete TTL/rent operator document.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Document which keys are instance vs persistent, their default-on-archival behavior, and the failure modes (defaulted contribution, allowlist falling to `false`).
- Document the `bump_ttl` keepalive (permissionless, extend-only) and the recommended cadence for long-dated escrows, plus which addresses to pass.
- Reference the constants and the ledger-time model.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-ttl-rent-model`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only rustdoc clarifications.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — an anchoring test showing a bumped key survives past the prior horizon.
  - **Add documentation:** expand [`docs/escrow-gas-storage-notes.md`](docs/escrow-gas-storage-notes.md) and reconcile with [`docs/adr/ADR-007-storage-key-evolution.md`](docs/adr/ADR-007-storage-key-evolution.md).
  - Include NatSpec-style `///` comments where clarified.
  - Validate security: documented archival behavior matches code defaults.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: instance vs persistent keys, defaulted reads, bump cadence.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`docs: document storage TTL and rent model with anchoring bump_ttl test`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Document the complete read-only view API surface for integrators"
labels: type:docs, area:read-api, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Catalog every get_/is_/has_ view with its return type and defaults

### Description
[`escrow/src/lib.rs`](escrow/src/lib.rs) exposes a large set of pure read views — `get_escrow`, `get_escrow_summary`, `get_funding_token`, `get_treasury`, `get_registry_ref`, `get_pending_admin`, `has_maturity_lock`, `get_funding_deadline`, `is_funding_expired`, `get_legal_hold`, `get_legal_hold_clear_delay`, `get_legal_hold_clearable_at`, `get_min_contribution_floor`, `get_max_unique_investors_cap`, `get_max_per_investor_cap`, `get_unique_funder_count`, `get_contribution`, `get_funding_close_snapshot`, `get_investor_yield_bps`, `get_investor_claim_not_before`, `get_sme_collateral_commitment`, `is_attestation_revoked`, `is_investor_claimed`, `is_allowlist_active`, `is_investor_allowlisted`, `compute_investor_payout`, `is_investor_refunded`, `get_distributed_principal`, and more. There is no single integrator-facing catalog of these views, their return types, and their default/absent semantics.

This issue produces a complete read-API reference.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Tabulate every public read view: signature, return type, what it returns when the underlying key is unset (default vs `Option::None` vs typed error), and whether it requires the escrow to be initialized.
- Note which views are `Option`-returning vs default-returning and the rationale.
- Keep consistent with the data-model and ADR-007 additive-key behavior for legacy instances.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-read-api-catalog`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only rustdoc additions on any undocumented view.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — an anchoring test asserting representative default/absent return values match the catalog.
  - **Add documentation:** expand [`docs/escrow-read-api.md`](docs/escrow-read-api.md); cross-link from [`README.md`](README.md).
  - Include NatSpec-style `///` comments on any newly-documented view.
  - Validate security: documented defaults match code behavior.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: unset Option keys, default-returning keys, pre-init reads.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`docs: catalog the complete read-only view API with anchoring test`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Document the investor allowlist model and its persistent-storage semantics"
labels: type:docs, area:allowlist, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document allowlist toggling, batch writes, and the fund-gate interaction

### Description
The allowlist controls in [`escrow/src/lib.rs`](escrow/src/lib.rs) — `set_allowlist_active`, `is_allowlist_active`, `set_investor_allowlisted`, the bounded batch `set_investors_allowlisted` (capped at `MAX_INVESTOR_ALLOWLIST_BATCH`), and `is_investor_allowlisted` — gate `fund` via `InvestorNotAllowlisted` only when the allowlist is active. Entries live in **persistent** per-address storage (so they have independent TTL and can be archived), and the gate defaults to disallowed on a missing entry. These nuances and the active/inactive interaction are not documented authoritatively.

This issue produces a complete allowlist document.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Document the active/inactive toggle, the per-address persistent storage and its TTL/archival implications (and how `bump_ttl` keeps entries alive), and the `fund`-gate behavior when active vs inactive.
- Document the batch bound (`MAX_INVESTOR_ALLOWLIST_BATCH`) and the equivalence-to-single-calls invariant.
- Reference the data model and storage-TTL notes.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-allowlist-model`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only rustdoc clarifications.
  - **Write comprehensive tests in:** [`escrow/src/test_allowlist_tests.rs`](escrow/src/test_allowlist_tests.rs) — anchoring tests: inactive allowlist permits any funder, active allowlist gates on entry, batch equals single calls.
  - **Add documentation:** add/expand a dedicated allowlist doc and cross-link from [`README.md`](README.md) and [`docs/escrow-data-model.md`](docs/escrow-data-model.md).
  - Include NatSpec-style `///` comments where clarified.
  - Validate security: documented gate behavior matches enforced rules.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: allowlist inactive, active with/without entry, batch toggling, archived entry default.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`docs: document the investor allowlist model and persistent-storage semantics with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Extract the repeated checked coupon and pro-rata arithmetic into a shared helper"
labels: type:refactor, area:settlement, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Refactor the compute_investor_payout overflow-guarded math into a reusable function

### Description
`compute_investor_payout` in [`escrow/src/lib.rs`](escrow/src/lib.rs) performs the canonical coupon and pro-rata arithmetic — `coupon = total_principal × yield / 10_000`, `settle_pool = total_principal + coupon`, `gross = contribution × settle_pool / total_principal` — with four `checked_*` calls each falling back to `fail(ComputePayoutArithmeticOverflow)`. As new views and events (aggregate pool, settlement event payloads) need the same numbers, re-deriving this arithmetic inline risks rounding and overflow-handling divergence from the audited path.

This issue extracts the math into one shared, overflow-safe helper.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a private `settlement_coupon(env, total_principal, yield_bps) -> i128` and/or `gross_payout(env, contribution, total_principal, yield_bps) -> i128` helper(s) using the existing `checked_*` + `ComputePayoutArithmeticOverflow` discipline.
- Refactor `compute_investor_payout` to call the helper(s) with identical results; no behavior or rounding change.
- This is a readability/correctness refactor enabling future reuse (aggregate pool view, settle event).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-payout-math-helper`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — coupon/pro-rata helper(s) and call-site replacement.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — helper results equal the prior inline computation across representative inputs; overflow still raises the typed error.
  - **Add documentation:** note the helper in [`docs/escrow-pro-rata.md`](docs/escrow-pro-rata.md).
  - Include NatSpec-style `///` comments on the helper(s).
  - Validate security: identical rounding and overflow behavior at every call site.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: zero yield, max yield, single investor, near-overflow inputs.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`refactor: extract shared overflow-safe coupon and pro-rata payout helpers with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Consolidate the repeated instance-storage get/set boilerplate behind typed key accessors"
labels: type:refactor, area:storage, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Refactor scattered env.storage().instance() reads into named helpers

### Description
Across [`escrow/src/lib.rs`](escrow/src/lib.rs), simple instance keys such as `DataKey::FundingDeadline`, `DataKey::DistributedPrincipal`, `DataKey::PendingAdmin`, `DataKey::LegalHoldClearDelay`, and `DataKey::SettledAt`-style flags are read with verbose `env.storage().instance().get(&DataKey::X).unwrap_or(default)` expressions repeated at every call site and getter. The repetition is error-prone (an inconsistent default in one place silently changes behavior) and obscures intent.

This issue introduces small typed accessor helpers for the most-repeated instance keys, with no behavior change.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add private accessors (e.g. `distributed_principal(&env) -> i128`, `pending_admin(&env) -> Option<Address>`, `funding_deadline(&env) -> Option<u64>`) returning the same defaults currently used inline.
- Replace the duplicated `get(...).unwrap_or(...)` reads at every site with the accessors; preserve identical defaults and behavior.
- Pure readability/maintainability refactor; no error-code or event changes.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-instance-key-accessors`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — accessors and call-site replacement.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — assert each accessor returns the documented default when unset and the stored value when set.
  - **Add documentation:** note the accessors in [`docs/escrow-data-model.md`](docs/escrow-data-model.md).
  - Include NatSpec-style `///` comments on the accessors.
  - Validate security: identical defaults and reads at every site.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: unset keys default correctly, set keys round-trip, pre-init reads.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`refactor: consolidate instance-storage reads behind typed accessors with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
