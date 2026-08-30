---
type: Feature
title: "Add a clear_sme_collateral_commitment entrypoint to release recorded collateral metadata"
labels: type:feature, area:collateral, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement clear_sme_collateral_commitment to retire stale collateral metadata

### Description
`record_sme_collateral_commitment` in [`escrow/src/lib.rs`](escrow/src/lib.rs) writes a metadata-only `DataKey::SmeCollateralPledge` and emits `CollateralRecordedEvt`, but there is **no way to remove it**. Once recorded, the commitment lingers in storage and is surfaced by `get_sme_collateral_commitment` forever, even after the underlying pledge is released off-chain — so indexers and dashboards report stale collateral on a settled or cancelled invoice.

This issue adds an SME-authorized `clear_sme_collateral_commitment()` that removes the pledge entry and emits a dedicated retirement event, mirroring the existing record path.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `clear_sme_collateral_commitment(env) -> ()` that loads the escrow via `load_escrow_require_sme`, asserts a commitment exists (append-only typed error `NoCollateralToClear`), removes `DataKey::SmeCollateralPledge`, and emits a new `CollateralClearedEvt` `#[contractevent]` carrying `invoice_id` and the prior amount.
- Preserve the metadata-only semantics documented on `record_sme_collateral_commitment`: no token movement, no balance reservation.
- Keep guard ordering consistent with ADR-002 (read-only existence check, then `require_auth`, then the storage remove and event).
- Do not renumber existing `EscrowError` codes; append the new variant.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-clear-sme-collateral`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `clear_sme_collateral_commitment`, `CollateralClearedEvt`, `NoCollateralToClear`.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — record then clear, clear-without-record rejection, non-SME caller rejection, event payload.
  - **Add documentation:** update [`docs/escrow-sme-collateral.md`](docs/escrow-sme-collateral.md) and the README entrypoint table.
  - Include NatSpec-style `///` comments on the new entrypoint and event.
  - Validate security: SME-only auth, no token movement, idempotent removal.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: clear with no prior commitment, wrong caller, clear after settle/cancel.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add clear_sme_collateral_commitment entrypoint to retire collateral metadata with tests`

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
title: "Expose the configured yield-tier table through a read-only view"
labels: type:feature, area:read-api, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement get_yield_tiers read view for the stored tier table

### Description
`init` in [`escrow/src/lib.rs`](escrow/src/lib.rs) persists an optional `DataKey::YieldTierTable` (validated by `validate_yield_tiers_table`) and `fund_with_commitment` consumes it via `effective_yield_for_commitment`, but there is **no public getter** for the tier table. Investors deciding which `committed_lock_secs` to pick, and dashboards rendering the tier ladder, cannot read the on-chain tiers — they must reconstruct them from the `EscrowInitialized` event or off-chain config.

This issue adds a pure `get_yield_tiers(env) -> Vec<YieldTier>` read returning the stored table (empty when none was configured).

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_yield_tiers(env) -> Vec<YieldTier>` reading `DataKey::YieldTierTable`, returning an empty `Vec` when unset (matching the `init` "empty tiers not stored" behavior).
- Pure read: no auth, no state change; consistent with the other `get_*` views.
- Document that the returned order matches the validated non-decreasing tier ordering enforced at `init`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-get-yield-tiers`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `get_yield_tiers` view.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — table round-trips through init, empty when no tiers, ordering preserved.
  - **Add documentation:** update [`docs/escrow-read-api.md`](docs/escrow-read-api.md) and [ADR-005](docs/adr/ADR-005-tiered-yield.md).
  - Include NatSpec-style `///` comments on the view.
  - Validate security: pure read, no auth, no mutation.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: no tiers, single tier, multi-tier ordering, legacy instance.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add get_yield_tiers read view for the configured tier table with tests`

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
title: "Add a funding deadline so under-funded escrows can expire and become cancellable"
labels: type:feature, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement an optional funding deadline gating fund() and unblocking cancel

### Description
The escrow in [`escrow/src/lib.rs`](escrow/src/lib.rs) has no time limit on the open (status 0) funding window: `fund_impl` accepts deposits indefinitely while `status == 0`, and `cancel_funding` requires the **admin** to act manually. There is no on-chain signal that a primary issuance has stalled, so investors' principal can sit in an open escrow with no automatic recovery trigger.

This issue adds an optional `funding_deadline` (ledger timestamp) configured at `init`: after it passes, new `fund` calls are rejected and the escrow is eligible for cancellation/refund recovery.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add an `Option<u64> funding_deadline` parameter to `init`, validated (0/absent ⇒ no deadline; otherwise must be `> now`), stored under a new `DataKey::FundingDeadline`.
- In `fund_impl`, after the status/legal-hold checks, reject deposits when a deadline is set and `now > deadline` with an append-only typed error `FundingDeadlinePassed`.
- Add a pure `get_funding_deadline(env) -> Option<u64>` view and an `is_funding_expired(env) -> bool` helper.
- Preserve the `funding_deadline == 0` "no deadline" semantics; do not affect already-funded (status 1) escrows.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-funding-deadline`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `DataKey::FundingDeadline`, init param/validation, fund gate, views, error.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — fund before/after deadline, no-deadline default, `is_funding_expired` transitions using `Ledger` testutils.
  - **Add documentation:** update [`docs/escrow-lifecycle.md`](docs/escrow-lifecycle.md) and [`docs/escrow-ledger-time.md`](docs/escrow-ledger-time.md).
  - Include NatSpec-style `///` comments on the new param, views, and error.
  - Validate security: deadline cannot retroactively trap funded escrows; ledger-time trust model documented.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: no deadline, exactly at deadline, after deadline, funded before deadline.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add optional funding deadline gating fund and recovery with tests and docs`

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
title: "Add an admin entrypoint to rebind the off-chain registry reference"
labels: type:feature, area:admin, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement set_registry_ref so the registry pointer can be corrected post-init

### Description
`init` in [`escrow/src/lib.rs`](escrow/src/lib.rs) optionally stores `DataKey::RegistryRef`, surfaced by `get_registry_ref`, but the pointer is **write-once at init** — there is no entrypoint to update it. If the off-chain registry contract is redeployed or the address was set incorrectly, the escrow points at a stale registry for its entire life with no recovery short of redeploying the whole escrow.

This issue adds an admin-gated `set_registry_ref(new_registry: Option<Address>)` so the reference can be corrected or cleared.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `set_registry_ref(env, new_registry: Option<Address>)` gated via `load_escrow_require_admin`; `Some` sets `DataKey::RegistryRef`, `None` removes it.
- Emit a new `RegistryRefUpdated` `#[contractevent]` carrying `invoice_id`, the prior registry (if any), and the new value for indexers.
- The registry is an informational pointer only; document that rebinding does not migrate or revalidate any registry-side state.
- Keep ADR-002 guard ordering: load escrow + admin `require_auth` before the storage write.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-set-registry-ref`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `set_registry_ref`, `RegistryRefUpdated` event.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — set, clear, non-admin rejection, `get_registry_ref` reflects update, event payload.
  - **Add documentation:** update [`docs/escrow-data-model.md`](docs/escrow-data-model.md) and the README entrypoint table.
  - Include NatSpec-style `///` comments on the entrypoint and event.
  - Validate security: admin-only, no impact on funds or status.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: set from none, overwrite existing, clear to none, non-admin caller.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add admin set_registry_ref entrypoint to rebind the registry pointer with tests`

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
title: "Add a remaining-funding-capacity read view for the open funding window"
labels: type:enhancement, area:read-api, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement get_remaining_funding_capacity to surface headroom to the target

### Description
Front-ends sizing a deposit must currently read `InvoiceEscrow::funding_target` and `funded_amount` separately from `get_escrow`/`get_escrow_summary` ([`escrow/src/lib.rs`](escrow/src/lib.rs)) and subtract them client-side, re-deriving the saturating semantics. There is no single on-chain view that answers "how much more can be funded before the target is reached?", and over-funding past the target is permitted, which clients frequently mishandle.

This issue adds a pure `get_remaining_funding_capacity(env) -> i128` view returning `max(0, funding_target - funded_amount)`.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_remaining_funding_capacity(env) -> i128` returning `funding_target.saturating_sub(funded_amount)` clamped at `0` so it never goes negative when over-funded.
- Pure read, no auth, no mutation; reuse the loaded escrow.
- Document that this is informational only — `fund` may still accept deposits that over-fund past the target while `status == 0`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-remaining-capacity-view`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `get_remaining_funding_capacity` view.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — capacity at zero/partial/exact/over-funded states.
  - **Add documentation:** update [`docs/escrow-read-api.md`](docs/escrow-read-api.md).
  - Include NatSpec-style `///` comments on the view.
  - Validate security: clamped non-negative, no mutation.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: unfunded, partially funded, exactly funded, over-funded.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add get_remaining_funding_capacity read view with tests`

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
title: "Extend EscrowSummary with collateral commitment and attestation status"
labels: type:enhancement, area:read-api, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Enrich get_escrow_summary with collateral and attestation fields

### Description
`get_escrow_summary` in [`escrow/src/lib.rs`](escrow/src/lib.rs) bundles core state (escrow, legal hold, snapshot, funder count, allowlist flag, schema version) into a single `EscrowSummary` host call, but it **omits** two metadata families that callers must fetch separately: the SME collateral commitment (`get_sme_collateral_commitment`) and the attestation binding (`get_primary_attestation_hash` / append-log length). Dashboards therefore make three extra round-trips for a complete view.

This issue extends `EscrowSummary` (additively) with collateral presence/amount and attestation status so one call returns the full picture.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add fields to `EscrowSummary`: an optional collateral commitment (`Option<SmeCollateralCommitment>`), whether a primary attestation hash is bound (`bool`), and the attestation append-log length (`u32`).
- Populate them in `get_escrow_summary` by reusing existing getters; keep the existing fields and their order stable per the additive-key policy (ADR-007).
- Pure read, no auth; ensure legacy instances with no collateral/attestation return the unset/zero defaults.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-summary-collateral-attestation`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — extend `EscrowSummary` and `get_escrow_summary`.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — summary with/without collateral and attestations, log-length accuracy.
  - **Add documentation:** update [`docs/escrow-read-api.md`](docs/escrow-read-api.md) and [`docs/escrow-data-model.md`](docs/escrow-data-model.md).
  - Include NatSpec-style `///` comments on the new fields.
  - Validate security: pure read, defaults for legacy state, stable field ordering.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: no collateral, recorded collateral, no attestation, bound + appended attestations.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: extend EscrowSummary with collateral and attestation status with tests`

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
title: "Make the dust-sweep per-call ceiling an admin-configurable parameter"
labels: type:feature, area:treasury, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement a configurable dust-sweep cap overriding MAX_DUST_SWEEP_AMOUNT

### Description
`sweep_terminal_dust` in [`escrow/src/lib.rs`](escrow/src/lib.rs) hard-caps each sweep at the compile-time constant `MAX_DUST_SWEEP_AMOUNT` (100_000_000 base units), rejecting larger requests with `SweepAmountExceedsMax`. For high-decimal tokens or large rounding residues this fixed ceiling can be too small, forcing many repeated sweeps; for low-value tokens it may be looser than desired. The cap cannot be tuned per deployment.

This issue adds an optional admin-configured override stored at `init`/via an admin setter, falling back to `MAX_DUST_SWEEP_AMOUNT` when unset.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `DataKey::MaxDustSweepOverride` (`i128`); add an admin entrypoint `set_max_dust_sweep(env, cap: i128)` validated to `cap > 0`, gated via `load_escrow_require_admin`, emitting a `MaxDustSweepUpdated` event.
- In `sweep_terminal_dust`, use the override when present, otherwise `MAX_DUST_SWEEP_AMOUNT`; keep the liability-floor invariant unchanged.
- Add a `get_max_dust_sweep(env) -> i128` view returning the effective cap.
- Append any new `EscrowError` codes; never renumber.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-configurable-dust-cap`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — override key, `set_max_dust_sweep`, getter, sweep cap logic, event.
  - **Write comprehensive tests in:** [`escrow/src/tests/integration.rs`](escrow/src/tests/integration.rs) — default cap, raised cap allows larger sweep, lowered cap rejects, non-admin setter rejection, liability floor still holds.
  - **Add documentation:** update [`docs/escrow-gas-storage-notes.md`](docs/escrow-gas-storage-notes.md) and [ADR-006](docs/adr/ADR-006-dust-sweep-and-token-safety.md).
  - Include NatSpec-style `///` comments on the setter, getter, and event.
  - Validate security: admin-only override, positive bound, floor invariant preserved.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: unset default, exactly at override, above override, non-admin caller, floor interaction.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: add admin-configurable dust-sweep cap overriding the compile-time max with tests`

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
title: "Add tests for the attestation bind and bounded append-log flow"
labels: type:test, area:attestations, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test bind_primary_attestation_hash and append_attestation_digest end to end

### Description
`bind_primary_attestation_hash` and `append_attestation_digest` in [`escrow/src/lib.rs`](escrow/src/lib.rs) implement a write-once primary hash plus a bounded append-only audit chain (capped at `MAX_ATTESTATION_APPEND_ENTRIES = 32`), with typed errors `PrimaryAttestationAlreadyBound` and `AttestationAppendLogCapacityReached`. These admin-gated funds-adjacent provenance writes need dedicated coverage in [`escrow/src/tests/attestations.rs`](escrow/src/tests/attestations.rs) to prove write-once, capacity, indexing, and auth boundaries.

This issue adds an exhaustive attestation test suite.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert `bind_primary_attestation_hash` succeeds once and rejects a second bind with `PrimaryAttestationAlreadyBound`; `get_primary_attestation_hash` reflects the bound value.
- Assert `append_attestation_digest` appends in order, increments the index in `AttestationDigestAppended`, and rejects the 33rd entry with `AttestationAppendLogCapacityReached`; `get_attestation_append_log` returns the full ordered vector.
- Assert both entrypoints reject non-admin callers via `mock_auths`.
- No production change unless a real gap surfaces (then file/fix separately).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-attestation-flow`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/attestations.rs`](escrow/src/tests/attestations.rs) — bind, re-bind rejection, append ordering, capacity, auth.
  - **Add documentation:** cross-link scenarios in [`docs/escrow-attestations.md`](docs/escrow-attestations.md).
  - Include NatSpec-style `///` comments on shared helpers.
  - Validate security: write-once primary, bounded log growth, admin-only.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: empty log, full log boundary, double bind, non-admin caller.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add coverage for attestation bind and bounded append-log flow`

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
title: "Add tests for the SME collateral commitment record and replace path"
labels: type:test, area:collateral, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test record_sme_collateral_commitment validation and replacement semantics

### Description
`record_sme_collateral_commitment` in [`escrow/src/lib.rs`](escrow/src/lib.rs) is a metadata-only write with non-trivial validation: positive amount (`CollateralAmountNotPositive`), non-empty asset symbol (`CollateralAssetEmpty`), monotonic `recorded_at` on replacement (`CollateralTimestampBackwards`), SME-only auth, and a `CollateralRecordedEvt` carrying the prior amount. This path has no dedicated coverage proving each validation branch and the replace-overwrite behavior.

This issue adds a focused collateral-commitment test suite.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert a first record succeeds and `get_sme_collateral_commitment` returns the asset/amount/timestamp; the event's `prior_amount` is `0`.
- Assert replacement overwrites and emits the prior amount; assert a backwards ledger timestamp is rejected with `CollateralTimestampBackwards` using `Ledger` testutils.
- Assert rejection of zero/negative amount and empty asset symbol, and that a non-SME caller is rejected.
- Confirm the metadata-only invariant: no token balance changes occur.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-collateral-commitment`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — record, replace, validation rejections, auth, no token movement.
  - **Add documentation:** cross-link scenarios in [`docs/escrow-sme-collateral.md`](docs/escrow-sme-collateral.md).
  - Include NatSpec-style `///` comments on helpers.
  - Validate security: SME-only, validation completeness, metadata-only.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: zero amount, empty asset, backwards timestamp, replace, non-SME caller.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add coverage for SME collateral commitment record and replace path`

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
title: "Add dual-authorization tests for the beneficiary rotation entrypoint"
labels: type:test, area:beneficiary-rotation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test rotate_beneficiary dual SME-plus-admin authorization and state gates

### Description
`rotate_beneficiary` in [`escrow/src/lib.rs`](escrow/src/lib.rs) is a funds-routing-critical entrypoint: it changes `sme_address` (the withdrawal recipient) and uniquely requires **both** the outgoing SME and the admin to authorize, only in pre-settlement states (status 0 or 1), with a no-op guard (`NewSmeSameAsCurrent`), a state guard (`RotationNotOpen`), and a legal-hold gate. This dual-auth path has no dedicated test asserting that a single signer is insufficient.

This issue adds a rotation test suite covering both signers, the guards, and the emitted event.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert rotation succeeds only when **both** SME and admin authorize; assert it fails with only SME, only admin, or neither (via `mock_auths`).
- Assert `BeneficiaryRotated` carries the correct prior/new SME and that a subsequent `withdraw` would route to the new beneficiary.
- Assert guards: `NewSmeSameAsCurrent` (no-op), `RotationNotOpen` (settled/withdrawn/cancelled), and `LegalHoldBlocksBeneficiaryRotation` while a hold is active.
- No production change unless a guard gap surfaces.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-beneficiary-rotation`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — dual-auth matrix, guards, event, post-rotation withdraw target.
  - **Add documentation:** cross-link scenarios in [`docs/ESCROW_BENEFICIARY_ROTATION.md`](docs/ESCROW_BENEFICIARY_ROTATION.md).
  - Include NatSpec-style `///` comments on helpers.
  - Validate security: both signers required; rotation blocked post-settlement and under hold.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: missing one signer, same-address no-op, wrong status, legal hold active.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add dual-auth and guard tests for rotate_beneficiary`

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
title: "Add boundary tests for min-contribution floor and the per-investor and unique caps"
labels: type:test, area:investor-caps, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test the funding cap and floor boundaries enforced in fund_impl

### Description
`fund_impl` in [`escrow/src/lib.rs`](escrow/src/lib.rs) enforces three independent limits: a per-call minimum (`MinContributionFloor` ⇒ `FundingBelowMinContribution`), a cumulative per-investor cap (`MaxPerInvestorCap` ⇒ `InvestorContributionExceedsCap`), and a distinct-funder cap (`MaxUniqueInvestorsCap` ⇒ `UniqueInvestorCapReached`), plus their `init`-time validation (`MinContributionNotPositive`, `MinContributionExceedsAmount`, `MaxPerInvestorNotPositive`, `MaxUniqueInvestorsNotPositive`). These boundary conditions need exhaustive coverage at the exact limit values.

This issue adds boundary tests for each floor/cap, including the interaction with follow-on deposits.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Floor: deposit below floor rejected, exactly at floor accepted, follow-on below floor still rejected (floor applies per call).
- Per-investor cap: cumulative deposits exactly at cap accepted, one over rejected (across multiple `fund` calls).
- Unique cap: distinct funders up to the cap accepted, the next new funder rejected, while follow-on deposits from existing funders still succeed.
- Init validation: assert each `init`-time rejection for non-positive/over-amount configurations.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-caps-and-floor-boundaries`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/cap_validation.rs`](escrow/src/tests/cap_validation.rs) — floor, per-investor cap, unique cap, init validation boundaries.
  - **Add documentation:** cross-link scenarios in [`docs/escrow-investor-caps.md`](docs/escrow-investor-caps.md).
  - Include NatSpec-style `///` comments on helpers.
  - Validate security: caps and floor are inclusive/exclusive exactly as documented.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: exact floor, exact per-investor cap, exact unique cap, follow-on deposits, init validation failures.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`test: add boundary tests for min-contribution floor and investor caps`

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
title: "Convert the tiered second-deposit panic in fund_with_commitment to a typed error"
labels: type:security, area:errors, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Harden fund_with_commitment with a typed error for follow-on tiered deposits

### Description
`fund_impl` (reached via `fund_with_commitment`) in [`escrow/src/lib.rs`](escrow/src/lib.rs) still uses a raw `assert!` with a panic string — `"Additional principal after a tiered first deposit must use fund(), not fund_with_commitment()"` — when an investor with an existing contribution (`prev != 0`) calls the commitment path again. Every other funding guard uses the append-only `EscrowError` enum, and the contract's SDK contract is that callers "branch on the numeric code rather than legacy panic strings". This one assert breaks that discipline on a funding-critical path.

This issue replaces the assert with a typed error.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add an append-only `EscrowError` variant (e.g. `TieredSecondDepositNotAllowed`); never renumber existing codes.
- Replace the `assert!(prev == 0, ...)` in the tiered branch with `ensure(&env, prev == 0, EscrowError::TieredSecondDepositNotAllowed)`.
- Preserve exact behavior and guard ordering — only the revert type changes; `fund()` follow-on deposits remain unaffected.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-tiered-second-deposit-typed-error`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — new error variant and `ensure` call.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — assert the typed error via `try_fund_with_commitment` after a prior deposit; assert `fund()` follow-on still works.
  - **Add documentation:** update [`docs/escrow-error-messages.md`](docs/escrow-error-messages.md) and [ADR-005](docs/adr/ADR-005-tiered-yield.md).
  - Include NatSpec-style `///` comments on the new variant.
  - Validate security: identical revert condition, stable numeric codes.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: tiered first deposit then tiered second (rejected), tiered first then `fund` (accepted).
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`fix: replace tiered second-deposit panic with typed EscrowError in fund_with_commitment with tests`

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
title: "Bound the commitment lock so an investor claim cannot be locked past maturity"
labels: type:security, area:tiered-yield, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Validate committed_lock_secs against settlement maturity in fund_with_commitment

### Description
In `fund_impl` ([`escrow/src/lib.rs`](escrow/src/lib.rs)) a tiered deposit derives `InvestorClaimNotBefore = now + committed_lock_secs`, enforced later in `claim_investor_payout` via `InvestorCommitmentLockNotExpired`. The lock is only checked for arithmetic overflow (`InvestorClaimTimeOverflow`); it is **not** bounded relative to the escrow's `maturity`. A tier lock longer than the maturity window means a settled escrow (status 2) holds an investor's payout claim hostage past the point where principal is due — funds the investor is entitled to are unclaimable until the lock expires.

This issue rejects, at deposit time, any commitment lock that would push the claim time beyond settlement maturity.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- When `committed_lock_secs > 0` and the escrow has a maturity lock (`maturity > 0`), reject the deposit if `now + committed_lock_secs > maturity` with a new append-only `EscrowError` (e.g. `CommitmentLockExceedsMaturity`).
- Preserve the `committed_lock_secs == 0` (no lock) and `maturity == 0` (no maturity lock) semantics — only constrain when both are set.
- Keep the existing `InvestorClaimTimeOverflow` overflow guard; this is an additional, narrower bound.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-commitment-lock-bound`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — bound check in the tiered branch, new error variant.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — lock within maturity accepted, lock past maturity rejected, no-maturity escrow unaffected, zero-lock unaffected, using `Ledger` testutils.
  - **Add documentation:** update [ADR-005](docs/adr/ADR-005-tiered-yield.md) and [`docs/escrow-legal-hold.md`](docs/escrow-legal-hold.md) cross-reference for claim timing.
  - Include NatSpec-style `///` comments on the new bound and error.
  - Validate security: no payout can be locked beyond the funds-due maturity.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: lock exactly at maturity, lock one second past maturity, no maturity, zero lock.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`fix: bound commitment lock to settlement maturity in fund_with_commitment with tests`

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
title: "Document the SME collateral commitment model and its metadata-only guarantees"
labels: type:docs, area:collateral, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document record_sme_collateral_commitment semantics and limitations

### Description
`record_sme_collateral_commitment` in [`escrow/src/lib.rs`](escrow/src/lib.rs) carries an important and easily-misread guarantee: it is **metadata-only** — it writes `DataKey::SmeCollateralPledge` and emits `CollateralRecordedEvt` but does **not** transfer tokens, reserve balances, verify custody, create an on-chain encumbrance, or block any flow. Misreading this as an enforced lien is a material risk for integrators. The existing [`docs/escrow-sme-collateral.md`](docs/escrow-sme-collateral.md) should be made authoritative and code-accurate.

This issue produces a complete, code-accurate collateral-commitment document.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Document the SME-only auth, the validation rules (positive amount, non-empty asset symbol, monotonic `recorded_at` on replace), and replacement semantics with the `prior_amount` event field.
- Prominently state the metadata-only limitations and contrast with the on-chain custody flows so integrators do not treat it as an enforced encumbrance.
- Reference the `SmeCollateralCommitment` struct fields and the `CollateralRecordedEvt` topic/payload.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-collateral-model`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only rustdoc clarifications if the inline comment drifts from docs.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — a test asserting no token-balance change accompanies a record, anchoring the metadata-only claim.
  - **Add documentation:** rewrite/expand [`docs/escrow-sme-collateral.md`](docs/escrow-sme-collateral.md); cross-link from [`README.md`](README.md).
  - Include NatSpec-style `///` comments where clarified.
  - Validate security: documented behavior matches enforced rules.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: record with/without replacement, asset symbol formatting, anchoring no-balance-change test.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`docs: document SME collateral commitment metadata-only model with anchoring test`

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
title: "Document the beneficiary rotation flow and its dual-authorization requirement"
labels: type:docs, area:beneficiary-rotation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document rotate_beneficiary dual-auth, state gates, and disbursement impact

### Description
`rotate_beneficiary` in [`escrow/src/lib.rs`](escrow/src/lib.rs) changes the SME withdrawal recipient and is the only entrypoint requiring **both** the outgoing SME and the admin to sign, restricted to pre-settlement states (status 0/1), with a no-op guard and a legal-hold gate. Because it redirects where funded principal is eventually disbursed, its authorization model and timing constraints need a precise, code-accurate operator-facing document; [`docs/ESCROW_BENEFICIARY_ROTATION.md`](docs/ESCROW_BENEFICIARY_ROTATION.md) should be the authoritative reference.

This issue produces a complete rotation document.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Document the dual SME+admin `require_auth` requirement, why both are needed, and the exact guard ordering (legal hold, status, no-op, dual auth).
- Document the allowed states (open/funded only) and the rejection codes `RotationNotOpen`, `NewSmeSameAsCurrent`, `LegalHoldBlocksBeneficiaryRotation`.
- Explain the downstream effect on `withdraw` (funds route to the new `sme_address`) and the `BeneficiaryRotated` event for indexers.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-beneficiary-rotation`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only rustdoc corrections if inline docs drift.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — a test asserting post-rotation withdrawal target matches the new SME, anchoring the doc.
  - **Add documentation:** rewrite/expand [`docs/ESCROW_BENEFICIARY_ROTATION.md`](docs/ESCROW_BENEFICIARY_ROTATION.md); reconcile with [ADR-002](docs/adr/ADR-002-auth-boundaries.md).
  - Include NatSpec-style `///` comments where clarified.
  - Validate security: documented dual-auth matches enforced auth.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: rotation in open vs funded, blocked post-settlement, hold active, post-rotation withdraw target.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`docs: document beneficiary rotation dual-auth flow with anchoring test`

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
title: "Extract the repeated legal-hold and terminal-status gate checks into shared guard helpers"
labels: type:refactor, area:guards, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Refactor duplicated legal-hold and status-gate checks into named helpers

### Description
The legal-hold gate `ensure(&env, !Self::legal_hold_active(&env), EscrowError::LegalHoldBlocks*)` is repeated across `fund_impl`, `settle`, `withdraw`, `claim_investor_payout`, `cancel_funding`, `rotate_beneficiary`, and `sweep_terminal_dust` in [`escrow/src/lib.rs`](escrow/src/lib.rs), each with a different error variant. Likewise the terminal-state check (`status == 2 || status == 3 || status == 4`) and the open-state check (`status == 0`) recur verbatim. The repeated, hand-written gates are error-prone — a future entrypoint can omit the hold check or mis-pick a status.

This issue extracts small named guard helpers parameterized by the error code, with no behavior change.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add private helpers, e.g. `guard_not_legal_hold(&env, err: EscrowError)`, `is_terminal_status(status: u32) -> bool`, and `guard_status_eq(&env, escrow_status, expected, err)`.
- Replace the inline checks at each call site, preserving the exact error variant and ADR-002 guard ordering (legal-hold/status checks before `require_auth` where they already are).
- No new errors, no behavior change; this is a readability/safety refactor only.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-shared-gate-helpers`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — guard helpers and call-site replacement.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — assert each refactored entrypoint still emits the same legal-hold/status error as before.
  - **Add documentation:** note the helpers in [ADR-002](docs/adr/ADR-002-auth-boundaries.md) and [`docs/escrow-security-checklist.md`](docs/escrow-security-checklist.md).
  - Include NatSpec-style `///` comments on the helpers.
  - Validate security: identical gate conditions and error codes at every site.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: legal-hold-blocked path per entrypoint, terminal vs non-terminal status, open-state guard.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`refactor: extract shared legal-hold and status-gate helpers with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.