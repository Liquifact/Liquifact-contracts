---
type: Feature
title: "Add an extend-only entrypoint to lengthen the funding deadline while the escrow is open"
labels: type:feature, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add an extend-only entrypoint to lengthen the funding deadline while the escrow is open

### Description
The escrow exposes [`get_funding_deadline`](escrow/src/lib.rs) and [`is_funding_expired`](escrow/src/lib.rs), and an under-funded escrow becomes cancellable once the deadline lapses. However, there is no admin path to **push the deadline out** when an SME needs more time to attract investors — the only options today are to let it expire or cancel and re-deploy. This forces a destructive teardown for what should be a routine schedule adjustment.

This issue adds an `extend_funding_deadline(new_deadline)` admin entrypoint that may only move the deadline **forward** (never shorten it), is gated to the open funding window, and emits a structured event so integrators can re-sync their off-chain timers.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `extend_funding_deadline(new_deadline: u64)` gated by admin `require_auth` via the existing admin loader, callable only while the escrow status is open (not funded, settled, or cancelled).
- Reject any `new_deadline` that is less than or equal to the current stored deadline with a new typed `EscrowError` variant (e.g. `FundingDeadlineNotExtended`), keeping error codes **append-only**.
- Respect the maturity bound: reject a `new_deadline` at or beyond the maturity timestamp so the funding window cannot swallow settlement.
- Emit a structured event carrying `invoice_id`, the old deadline, and the new deadline.
- Bump the persistent/instance TTL for any touched entry.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-extend-funding-deadline`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `extend_funding_deadline`, the new typed error, and the deadline-extended event struct.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — assert forward-only moves, rejection of equal/earlier deadlines, the maturity-bound guard, the status gate, admin auth, and the emitted event payload.
  - **Add documentation:** update [`README.md`](README.md) and the funding-lifecycle docs to describe deadline extension and its bounds.
  - Include NatSpec-style doc comments (`///`) on the new entrypoint, matching the existing style in `lib.rs`.
  - Validate security assumptions: no shortening, no extension past maturity, admin-only, correct status gating.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: equal deadline, earlier deadline, deadline past maturity, non-admin caller, funded/settled/cancelled status.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add extend-only funding deadline entrypoint with bounds, event, tests and docs`

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
title: "Add a read view exposing the pending-admin proposal's remaining validity seconds"
labels: type:feature, area:admin, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a read view exposing the pending-admin proposal's remaining validity seconds

### Description
The two-step handover stores a `PendingAdminExpiry` timestamp set by [`propose_admin`](escrow/src/lib.rs), and [`get_pending_admin_expiry`](escrow/src/lib.rs) returns that **absolute** timestamp. Integrators and dashboards must independently fetch the current ledger time and subtract to learn how long a successor has left to call [`accept_admin`](escrow/src/lib.rs) — duplicated, error-prone arithmetic that drifts from the contract's own inclusive-bound semantics.

This issue adds a `get_pending_admin_remaining_secs()` view that returns `Some(seconds_left)` computed against `env.ledger().timestamp()` using the **same inclusive comparison** as `accept_admin`, `Some(0)` when expired-on-this-ledger, and `None` when no proposal is active.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_pending_admin_remaining_secs() -> Option<u64>` reading `DataKey::PendingAdmin` and `DataKey::PendingAdminExpiry`.
- Return `None` if no pending admin is set; `Some(0)` if `now >= expiry`; otherwise `Some(expiry - now)` using saturating arithmetic.
- Match the inclusive expiry semantics documented on `accept_admin` exactly so the view never reports time remaining on a proposal `accept_admin` would reject.
- Pure read-only: no auth, no storage writes, no TTL bump.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-pending-admin-remaining-view`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `get_pending_admin_remaining_secs`.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — assert `None` with no proposal, a positive remaining value, the `Some(0)` boundary exactly at expiry, and consistency with what `accept_admin` accepts/rejects.
  - **Add documentation:** update the admin-handover docs and the integrator view-surface reference.
  - Include NatSpec-style doc comments (`///`) on the new view.
  - Validate security assumptions: read-only, no state mutation, boundary parity with `accept_admin`.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: no proposal, exactly-at-expiry, one second before expiry, far-future expiry.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add pending-admin remaining-validity read view with expiry-parity tests and docs`

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
title: "Add a batch contribution read view returning many investors' funded amounts in one call"
labels: type:feature, area:views, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a batch contribution read view returning many investors' funded amounts in one call

### Description
[`get_contribution(investor)`](escrow/src/lib.rs) returns a single investor's funded principal, and [`get_investors(start, limit)`](escrow/src/lib.rs) paginates addresses. To render a cap table, an integrator must call `get_contribution` once per address — N round-trips for N investors. There is no way to fetch a page of `(address, amount)` pairs together, which is wasteful for indexers reconstructing the book.

This issue adds `get_contributions(addresses: Vec<Address>) -> Vec<i128>` that resolves each supplied address to its recorded contribution (zero for non-funders) in a single read call, mirroring the input order.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_contributions(addresses: Vec<Address>) -> Vec<i128>` returning one amount per input address, in order, reusing the same storage path as `get_contribution`.
- Return `0` for any address with no recorded contribution (never panic on unknown addresses).
- Bound the input length with a constant (e.g. reuse the existing pagination ceiling) and reject oversized batches with a typed error to avoid unbounded reads.
- Pure read-only: no auth, no writes, no TTL bump.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-batch-contribution-view`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `get_contributions`.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — assert order preservation, zero for unknown addresses, equivalence with per-address `get_contribution`, the empty-input case, and the over-cap rejection.
  - **Add documentation:** extend the integrator view-surface reference with the batch read.
  - Include NatSpec-style doc comments (`///`) on the new view.
  - Validate security assumptions: bounded reads, no mutation, no panic on unknown input.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: empty vector, mix of funders and non-funders, duplicate addresses, batch at and above the cap.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add bounded batch contribution read view with order and equivalence tests`

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
title: "Emit a structured event from clear_sme_collateral_commitment recording the cleared digest"
labels: type:enhancement, area:events, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Emit a structured event from clear_sme_collateral_commitment recording the cleared digest

### Description
[`record_sme_collateral_commitment`](escrow/src/lib.rs) writes commitment metadata and [`clear_sme_collateral_commitment`](escrow/src/lib.rs) removes it, but the **clear** path emits no event. An off-chain indexer tracking collateral state therefore sees the commitment appear (if recording emits) but cannot observe its removal on-chain, breaking auditability of the metadata-only collateral lifecycle.

This issue adds a structured `contractevent` from `clear_sme_collateral_commitment` carrying `invoice_id` and the digest/reference that was cleared, so the full record → clear cycle is reconstructable from the event log alone.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Read the existing `SmeCollateralCommitment` before clearing so its identifying field (digest or reference) can be included in the event.
- Emit a new event struct with a **unique** symbol topic distinct from every existing topic, carrying `invoice_id` and the cleared commitment's identifier.
- Preserve the existing auth gate and the no-op/absent-commitment behavior (decide and document whether clearing an absent commitment is a no-op or a typed error, without changing the current success semantics).
- Keep the event additive so existing consumers are unaffected.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-collateral-clear-event`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — new event struct + publish in `clear_sme_collateral_commitment`.
  - **Write comprehensive tests in:** [`escrow/src/tests/attestations.rs`](escrow/src/tests/attestations.rs) — assert the event fires with the correct payload after a recorded commitment, the topic is unique, and the absent-commitment path behaves as documented.
  - **Add documentation:** update the collateral-commitment docs and reconcile the events catalog.
  - Include NatSpec-style doc comments (`///`) noting the new event.
  - Validate security assumptions: no metadata leakage beyond the existing record, unchanged auth.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: clear after record, clear with no commitment present, unauthorized caller.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: emit structured event on collateral commitment clear with tests and catalog update`

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
title: "Add a paginated read view enumerating only the currently revoked attestation digests"
labels: type:feature, area:attestations, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a paginated read view enumerating only the currently revoked attestation digests

### Description
The append-log exposes [`get_attestation_append_log`](escrow/src/lib.rs), [`get_attestation_digest_at`](escrow/src/lib.rs), and [`is_attestation_revoked`](escrow/src/lib.rs). To list which digests are *currently revoked*, an integrator must read the whole log, then probe `is_attestation_revoked` for every index — O(N) round-trips and no way to page. There is a paginated view of revoked *indices* already, but none returning the **digests themselves** in revoked state.

This issue adds `get_revoked_attestation_digests(start, limit) -> Vec<AttestationDigestInfo>` that pages over the log and returns only entries whose index is currently revoked, so an auditor can fetch revoked digests directly.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_revoked_attestation_digests(start: u32, limit: u32) -> Vec<AttestationDigestInfo>` reusing the same `AttestationDigestInfo` shape returned by `get_attestation_digest_at`.
- Page over the append-log honoring `start`/`limit`, and skip indices that are not currently revoked (respecting any prior `unrevoke_attestation_digest`).
- Bound `limit` with the existing pagination ceiling; return an empty `Vec` past the end rather than panicking.
- Pure read-only: no auth, no writes, no TTL bump.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-revoked-digests-view`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `get_revoked_attestation_digests`.
  - **Write comprehensive tests in:** [`escrow/src/tests/attestations.rs`](escrow/src/tests/attestations.rs) — assert only revoked entries appear, pagination boundaries, exclusion of un-revoked entries, empty result past the end, and the over-cap guard.
  - **Add documentation:** extend the attestation docs and integrator view-surface reference.
  - Include NatSpec-style doc comments (`///`) on the new view.
  - Validate security assumptions: bounded reads, no mutation, correct interaction with un-revoke.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: no revocations, mixed revoked/un-revoked, page past end, limit above the cap.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add paginated revoked-attestation-digests read view with boundary tests`

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
title: "Validate the proposed admin differs from any active pending admin in propose_admin"
labels: type:security, area:admin, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Validate the proposed admin differs from any active pending admin in propose_admin

### Description
[`propose_admin`](escrow/src/lib.rs) rejects a proposal where `new_admin == current_admin` (`NewAdminSameAsCurrent`), but it does **not** check the new proposal against any **already-pending** proposal. Re-proposing the identical pending address silently resets `PendingAdminExpiry` to a fresh window with no event distinction from a genuine change, and silently overwrites a different in-flight successor. Both cases deserve explicit, observable handling.

This issue tightens `propose_admin` so re-proposing the exact pending address is rejected as a no-op typed error, and replacing a *different* pending proposal emits a distinct supersede signal in the event payload — making handover races auditable.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- In `propose_admin`, read any existing `DataKey::PendingAdmin`; if it equals `new_admin`, reject with a new typed error (e.g. `PendingAdminUnchanged`), keeping codes **append-only**.
- When a *different* pending proposal exists and is being replaced, include the superseded address in the emitted `AdminProposedEvent` (or a companion event) so observers can detect the overwrite.
- Preserve the existing `NewAdminSameAsCurrent` guard, admin `require_auth`, and expiry-window behavior for genuinely new proposals.
- Do not weaken the ability to legitimately change the pending successor.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-propose-admin-pending-guard`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — the new guard, error variant, and event field in `propose_admin`.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — assert rejection of an identical re-proposal, the supersede event on a changed proposal, the preserved same-as-current guard, and expiry refresh semantics.
  - **Add documentation:** update the two-step handover docs to describe the new guard and supersede signal.
  - Include NatSpec-style doc comments (`///`) describing the new error path.
  - Validate security assumptions: no silent overwrite, observable supersede, admin-only.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: identical re-proposal, different successor replacement, same-as-current, non-admin caller, expiry refresh.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`fix: reject identical pending re-proposal and signal supersede in propose_admin with tests`

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
title: "Reject zero-amount entries in fund_batch before mutating any escrow state"
labels: type:security, area:funding, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Reject zero-amount entries in fund_batch before mutating any escrow state

### Description
[`fund_batch`](escrow/src/lib.rs) records multiple `(Address, i128)` contributions in one call. Single-entry [`fund`](escrow/src/lib.rs) rejects non-positive amounts via `AmountMustBePositive`, but the batch path must apply the **same** positivity check to **every** entry **before** any storage write, or a malformed batch can partially mutate the unique-funder count and per-investor map before failing — leaving the escrow in an inconsistent half-applied state.

This issue adds an up-front validation pass over the whole batch so any non-positive (or otherwise invalid) entry fails the entire call atomically, with no partial state change.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Validate **all** entries in `fund_batch` (positivity, and reuse existing per-entry floor/cap checks) in a first pass before performing any write or counter increment.
- On any invalid entry, return the existing typed error (`AmountMustBePositive` or the relevant cap/floor error) without having mutated contributions, the unique-funder count, or the funded total.
- Preserve all existing per-entry semantics for valid batches (ordering, funded-target promotion mid-batch, allowlist gating).
- Do not introduce a new error code if an existing one already describes the condition.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-fund-batch-prevalidate`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — the pre-validation pass in `fund_batch`.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — assert that a batch containing a zero/negative entry leaves contributions, funder count, and funded total unchanged, alongside happy-path equivalence with sequential `fund` calls.
  - **Add documentation:** note the atomic all-or-nothing validation in the batch funding docs.
  - Include NatSpec-style doc comments (`///`) clarifying the atomicity guarantee.
  - Validate security assumptions: no partial mutation, atomic rejection, preserved valid-batch behavior.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: zero entry, negative entry, invalid entry at first/middle/last position, all-valid batch.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`fix: pre-validate all fund_batch entries to guarantee atomic all-or-nothing recording`

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
title: "Add a settlement-readiness view bundling settleable, legal-hold, and maturity state in one call"
labels: type:feature, area:views, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a settlement-readiness view bundling settleable, legal-hold, and maturity state in one call

### Description
Deciding whether [`settle`](escrow/src/lib.rs) will succeed today requires an integrator to stitch together [`is_settleable`](escrow/src/lib.rs), [`get_legal_hold`](escrow/src/lib.rs), [`has_maturity_lock`](escrow/src/lib.rs), and the maturity timestamp across multiple calls — and to replicate the contract's own precedence rules between them. That logic drifts out of sync with the contract and produces confusing UIs ("settleable" but blocked by a legal hold).

This issue adds a single `get_settlement_readiness()` view returning a struct that bundles the settleable flag, legal-hold state, maturity-reached state, and a single derived "ready now" boolean computed with the contract's authoritative precedence.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a `#[contracttype]` result (e.g. `SettlementReadiness`) with fields: `is_settleable`, `legal_hold_active`, `maturity_reached`, and `ready_now`.
- Compute `ready_now` using the **same** gating precedence `settle`/`partial_settle` apply, so a `true` value reliably predicts a successful settle on the current ledger.
- Reuse the existing single-source-of-truth `is_settleable` logic; do not duplicate its rule.
- Pure read-only: no auth, no writes, no TTL bump.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-settlement-readiness-view`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — the `SettlementReadiness` type and `get_settlement_readiness`.
  - **Write comprehensive tests in:** [`escrow/src/tests/settlement.rs`](escrow/src/tests/settlement.rs) — assert each field, and that `ready_now == true` exactly when a subsequent `settle` succeeds (and `false` when it would fail due to hold/maturity).
  - **Add documentation:** add a settlement-readiness section to the docs and the view-surface reference.
  - Include NatSpec-style doc comments (`///`) describing the precedence used.
  - Validate security assumptions: read-only, no drift from settle's own gating.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: pre-maturity, post-maturity, active legal hold, funded-but-held, ready state.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add bundled settlement-readiness view with settle-parity tests and docs`

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
title: "Add an admin entrypoint to raise the maturity-max-horizon ceiling with a forward-only guard"
labels: type:enhancement, area:maturity, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add an admin entrypoint to raise the maturity-max-horizon ceiling with a forward-only guard

### Description
[`update_maturity_max_horizon`](escrow/src/lib.rs) sets the ceiling that [`update_maturity`](escrow/src/lib.rs) must respect, exposed via [`get_maturity_max_horizon`](escrow/src/lib.rs). The current setter accepts arbitrary new horizons. For a "term-extension only" governance policy there is no entrypoint that **guarantees** the horizon can only be raised, never lowered — important because lowering a horizon below an already-set maturity creates a confusing invalid configuration.

This issue adds a dedicated `raise_maturity_max_horizon(new_horizon)` that rejects any value not strictly greater than the current horizon, providing a monotonic, policy-safe lever distinct from the general setter.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `raise_maturity_max_horizon(new_horizon: u64) -> u64` gated by admin `require_auth`.
- Reject any `new_horizon` not strictly greater than the current stored horizon with a new typed error (e.g. `HorizonNotRaised`), keeping codes **append-only**.
- Emit a structured event carrying `invoice_id`, the old horizon, and the new horizon.
- Leave the existing `update_maturity_max_horizon` behavior unchanged; this is an additional, stricter entrypoint.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-raise-maturity-horizon`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `raise_maturity_max_horizon`, the new error, and the event.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — assert forward-only enforcement, rejection of equal/lower horizons, admin auth, the event payload, and that a later `update_maturity` can use the raised ceiling.
  - **Add documentation:** update the maturity/term docs describing the monotonic raise lever.
  - Include NatSpec-style doc comments (`///`) on the new entrypoint.
  - Validate security assumptions: strictly increasing, admin-only, no interaction with active maturity locks.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: equal horizon, lower horizon, non-admin caller, subsequent maturity update at the new ceiling.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add monotonic raise_maturity_max_horizon entrypoint with event, tests and docs`

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
title: "Add tests asserting propose_admin expiry-window override and the default-window fallback"
labels: type:test, area:admin, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests asserting propose_admin expiry-window override and the default-window fallback

### Description
[`propose_admin`](escrow/src/lib.rs) takes an `Option<u64>` validity window: `Some(w)` sets expiry to `now + w`, while `None` falls back to `DEFAULT_ADMIN_PROPOSAL_VALIDITY_SECS`. The downstream gate in [`accept_admin`](escrow/src/lib.rs) enforces this expiry inclusively. The override-vs-default branching and its exact inclusive boundary are exactly the kind of governance logic that needs locked-in regression coverage, and there is no test pinning both branches and the boundary.

This issue adds focused tests for the explicit window, the default fallback, and the inclusive accept boundary.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert `propose_admin(.., Some(w))` stores `now + w` exactly via `get_pending_admin_expiry`.
- Assert `propose_admin(.., None)` stores `now + DEFAULT_ADMIN_PROPOSAL_VALIDITY_SECS`.
- Assert `accept_admin` succeeds at `timestamp == expiry` (inclusive) and fails one second later with `AdminProposalExpired`.
- Use the existing test harness ledger-time controls; no production code changes unless a bug is found (document it if so).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-propose-admin-window`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a defect is uncovered; otherwise no production change.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — the override, default-fallback, and inclusive-boundary cases.
  - **Add documentation:** note the validity-window semantics in the handover docs if under-specified.
  - Include NatSpec-style doc comments (`///`) on any new test helpers.
  - Validate security assumptions: expiry cannot be bypassed at or past the boundary.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: explicit window, default window, accept exactly at expiry, accept one second past expiry.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: cover propose_admin window override, default fallback, and inclusive accept boundary`

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
title: "Add tests for clear_registry_ref clearing the reference and emitting the rebind event"
labels: type:test, area:registry, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for clear_registry_ref clearing the reference and emitting the rebind event

### Description
The off-chain registry pointer is managed by [`rebind_registry_ref`](escrow/src/lib.rs), [`clear_registry_ref`](escrow/src/lib.rs), and read via [`get_registry_ref`](escrow/src/lib.rs). The clear path nulls the `Option<Address>` reference and should be admin-gated and observable, but there is no test asserting the post-clear `None` read, the admin gate, and the emitted rebind/clear event together.

This issue adds dedicated tests for the clear path so the registry-pointer lifecycle is fully regression-covered.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- After a `rebind_registry_ref(Some(addr))`, assert `clear_registry_ref` makes `get_registry_ref` return `None`.
- Assert `clear_registry_ref` requires admin auth (unauthorized caller fails).
- Assert the documented clear/rebind event fires with the correct `invoice_id` and previous reference (if carried).
- Assert clearing an already-empty reference behaves as documented (no-op or typed error) without corrupting state.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-clear-registry-ref`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a defect is uncovered.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — the clear path, auth gate, event assertion, and already-empty case.
  - **Add documentation:** reconcile the registry-pointer section and events catalog if gaps surface.
  - Include NatSpec-style doc comments (`///`) on any new test helpers.
  - Validate security assumptions: admin-only clear, observable event, no stale pointer.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: clear after rebind, unauthorized clear, double clear, event payload.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: cover clear_registry_ref state reset, admin gate, and rebind event`

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
title: "Add tests for preview_yield_tier matching the tier actually assigned by fund_with_commitment"
labels: type:test, area:yield, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for preview_yield_tier matching the tier actually assigned by fund_with_commitment

### Description
[`preview_yield_tier(amount, lock)`](escrow/src/lib.rs) returns the `(yield_bps, claim_lock)` an investor *would* receive, and [`fund_with_commitment`](escrow/src/lib.rs) assigns the real tier on deposit (surfaced later via [`get_investor_yield_bps`](escrow/src/lib.rs) and [`get_investor_claim_not_before`](escrow/src/lib.rs)). If the preview and the actual assignment ever diverge at tier boundaries, investors are misled. No test currently pins preview-vs-actual equivalence across the tier table.

This issue adds equivalence tests asserting that, for amounts/locks spanning every tier boundary, `preview_yield_tier` exactly matches what `fund_with_commitment` later records.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- For each tier in [`get_yield_tiers`](escrow/src/lib.rs), test amounts/locks at the boundary (just below, exactly at, just above) and assert the preview equals the post-fund `get_investor_yield_bps`/`get_investor_claim_not_before`.
- Include the base/no-tier case (amount below the first tier threshold).
- Use distinct investors per case to avoid second-deposit interactions; no production change unless a divergence is found (document it).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-preview-vs-actual-tier`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a preview/actual divergence is uncovered.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — boundary parity across the full tier table plus the base case.
  - **Add documentation:** clarify the preview/assignment contract in the yield docs.
  - Include NatSpec-style doc comments (`///`) on any new test helpers.
  - Validate security assumptions: investors are never quoted a tier different from the one assigned.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: below-first-tier, each boundary triple, highest tier, zero lock.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: assert preview_yield_tier matches fund_with_commitment assignment across all boundaries`

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
title: "Add tests for lower_max_unique_investors rejecting a cap below the current funder count"
labels: type:test, area:caps, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for lower_max_unique_investors rejecting a cap below the current funder count

### Description
[`lower_max_unique_investors(new_cap)`](escrow/src/lib.rs) tightens the unique-investor ceiling, which feeds [`get_max_unique_investors_cap`](escrow/src/lib.rs) and [`get_remaining_investor_slots`](escrow/src/lib.rs). Lowering the cap **below** the count already recorded by [`get_unique_funder_count`](escrow/src/lib.rs) would create an inconsistent state where remaining slots underflows or the invariant "count <= cap" is violated. There is no test pinning this boundary.

This issue adds tests asserting the lower-only semantics, the floor at the current funder count, and the resulting remaining-slots value.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Fund N distinct investors, then assert `lower_max_unique_investors(N)` succeeds (cap exactly at count → zero remaining slots) and `lower_max_unique_investors(N-1)` is rejected with the appropriate typed error.
- Assert raising via this entrypoint is rejected (it is lower-only) and admin auth is enforced.
- Assert `get_remaining_investor_slots` is consistent after each successful lowering.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-lower-unique-cap-floor`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a defect is uncovered.
  - **Write comprehensive tests in:** [`escrow/src/tests/cap_validation.rs`](escrow/src/tests/cap_validation.rs) — the at-count, below-count, raise-attempt, and auth cases.
  - **Add documentation:** clarify the lower-only cap semantics in the caps docs.
  - Include NatSpec-style doc comments (`///`) on any new test helpers.
  - Validate security assumptions: cap can never drop below recorded funders, no slot underflow.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: cap == count, cap == count-1, raise attempt, non-admin caller, remaining-slots consistency.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: assert lower_max_unique_investors floors at funder count and rejects raises`

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
title: "Add property-based tests that get_remaining_investor_slots never underflows across fund flows"
labels: type:test, area:caps, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add property-based tests that get_remaining_investor_slots never underflows across fund flows

### Description
[`get_remaining_investor_slots`](escrow/src/lib.rs) derives remaining capacity from the unique-investor cap minus [`get_unique_funder_count`](escrow/src/lib.rs). Under arbitrary interleavings of [`fund`](escrow/src/lib.rs), [`fund_batch`](escrow/src/lib.rs), repeated deposits by the same investor, and cap lowering via [`lower_max_unique_investors`](escrow/src/lib.rs), this derived value must always satisfy `0 <= remaining` and `count + remaining == cap`. There is no property-based coverage asserting this invariant holds for every sequence.

This issue adds proptest-style invariants over randomized funding/cap sequences.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Generate randomized sequences of single/batch funds (including repeat funders) and valid cap lowerings.
- After each operation, assert `get_remaining_investor_slots` is `None` when no cap is set, else `Some(r)` with `r >= 0` and `count + r == cap`.
- Assert repeat deposits by an existing investor never decrement remaining slots (count is unique-based).
- Follow the existing proptest harness conventions in the properties test module.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-remaining-slots-proptest`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if an invariant violation is uncovered.
  - **Write comprehensive tests in:** [`escrow/src/tests/properties.rs`](escrow/src/tests/properties.rs) — the randomized invariants.
  - **Add documentation:** state the slots invariant in the caps docs.
  - Include NatSpec-style doc comments (`///`) on any new generators.
  - Validate security assumptions: no underflow, exact count+remaining==cap conservation.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: no-cap escrow, cap exactly hit, repeat funders, cap lowered mid-sequence.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: add property-based invariants for remaining-investor-slots conservation and non-underflow`

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
title: "Extract the repeated open-funding-window status gate into a single shared guard helper"
labels: type:refactor, area:internals, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Extract the repeated open-funding-window status gate into a single shared guard helper

### Description
Several entrypoints — [`fund`](escrow/src/lib.rs), [`fund_with_commitment`](escrow/src/lib.rs), [`fund_batch`](escrow/src/lib.rs), [`update_funding_target`](escrow/src/lib.rs), [`lower_max_unique_investors`](escrow/src/lib.rs), and [`lower_min_contribution_floor`](escrow/src/lib.rs) — independently re-check that the escrow is in the open funding window (not funded/settled/cancelled). These open inline `ensure(..)` checks duplicate the same condition and typed error across call sites, so adding a new open-window operation risks an inconsistent gate.

This issue consolidates the check into one private helper (e.g. `require_funding_open(&env, &escrow)`) reused by every open-window entrypoint, with no behavior change.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a private helper that asserts the escrow is in the open funding window and returns the existing typed error otherwise.
- Replace the inline status gates in all listed entrypoints with calls to the helper, preserving the exact error codes and ordering relative to auth checks.
- Pure refactor: no public signature, error code, or event change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-funding-open-guard`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — the `require_funding_open` helper and call-site substitutions.
  - **Write comprehensive tests in:** [`escrow/src/tests/integration_status_guards.rs`](escrow/src/tests/integration_status_guards.rs) — assert each entrypoint still rejects from funded/settled/cancelled with the same error after the refactor.
  - **Add documentation:** note the shared guard in the internals docs.
  - Include NatSpec-style doc comments (`///`) on the helper.
  - Validate security assumptions: no gate weakened or reordered relative to auth.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: each entrypoint from each non-open status, plus the happy open-window path.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`refactor: consolidate open-funding-window status gate into a shared guard helper`

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
title: "Replace the magic-number default-balance literal in the mock token with a named constant"
labels: type:refactor, area:testutils, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Replace the magic-number default-balance literal in the mock token with a named constant

### Description
The in-crate mock token's [`balance`](escrow/src/lib.rs) and [`transfer`](escrow/src/lib.rs) functions both hard-code the literal `100_000_000_000_000i128` as the default starting balance for any unseen address. The value is duplicated across the two functions, undocumented, and easy to desynchronize — if one is changed and the other is not, transfers silently compute against mismatched starting balances, corrupting test expectations.

This issue hoists the literal into a single named `const` (e.g. `MOCK_TOKEN_DEFAULT_BALANCE`) with a doc comment, referenced from both functions, so the default is defined once.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Define one `const MOCK_TOKEN_DEFAULT_BALANCE: i128` (test/testutils scope) documenting why this magnitude is chosen.
- Replace both literal occurrences in `balance` and `transfer` with the constant.
- Pure refactor confined to test/testutils code: no production behavior, no public API change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-mock-token-default-const`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — the new constant and the two substitutions in the mock token.
  - **Write comprehensive tests in:** [`escrow/src/tests/external_calls_mocked.rs`](escrow/src/tests/external_calls_mocked.rs) — assert an unseen address reports the constant and that a transfer between two unseen addresses produces the expected symmetric deltas around it.
  - **Add documentation:** note the testutils default-balance constant in the test-harness docs.
  - Include NatSpec-style doc comments (`///`) on the constant.
  - Validate security assumptions: confined to test code, no production path affected.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: unseen sender, unseen recipient, both unseen, repeated transfers.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`refactor: hoist mock-token default balance into a single named constant`

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
title: "Document the off-chain registry-reference pointer lifecycle and its rebind and clear semantics"
labels: type:docs, area:registry, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the off-chain registry-reference pointer lifecycle and its rebind and clear semantics

### Description
The contract stores an optional pointer to an off-chain registry, mutated by [`rebind_registry_ref`](escrow/src/lib.rs) and [`clear_registry_ref`](escrow/src/lib.rs) and read via [`get_registry_ref`](escrow/src/lib.rs). What this pointer *means*, what trust it does and does not confer, who may change it, and how integrators should treat a `None` value are undocumented — leaving integrators to guess whether the on-chain escrow depends on the registry for any settlement-critical decision.

This issue documents the registry-reference pointer's full lifecycle and its explicit non-authority over on-chain funds.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Describe the pointer's purpose, the `Option<Address>` set/clear states, and the admin-only mutation path with its emitted event.
- State explicitly that the pointer is a **reference only** and confers no control over escrow funds, settlement, or auth — it does not gate any value-moving entrypoint.
- Document how integrators should interpret `None` (unbound) vs `Some(addr)` and the rebind event for off-chain re-sync.
- Cross-link the relevant entrypoints and the events catalog.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-registry-ref-lifecycle`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only NatSpec `///` clarifications on the three entrypoints if wording is thin.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — a doc-backing test asserting that binding/clearing the registry does not change any settlement or funding outcome.
  - **Add documentation:** add a registry-reference section to [`README.md`](README.md) and the docs tree describing the lifecycle and non-authority guarantee.
  - Include NatSpec-style doc comments (`///`) reflecting the documented semantics.
  - Validate security assumptions: documentation matches code — registry pointer never gates funds.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: ensure the doc-backing test confirms no fund-flow dependency on the pointer.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`docs: document registry-reference pointer lifecycle and its non-authority over escrow funds`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
