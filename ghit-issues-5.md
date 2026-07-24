---
type: Feature
title: "Deduplicate the four divergent is_settleable implementations into a single source of truth"
labels: type:refactor, area:settlement, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Deduplicate the four divergent is_settleable implementations into a single source of truth

### Description
[`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) currently declares `pub fn is_settleable(env: Env) -> bool` **four times** (around lines 1500, 1693, 2152, and 3342). Multiple co-existing definitions of the same public view are a maintenance hazard: a fix or rule change applied to one copy silently leaves the others stale, and integrators have no guarantee that the binary's exported `is_settleable` matches the documented logic. Settlement-readiness is a money-path predicate (it gates `settle()`), so divergence here is a correctness and audit risk.

This issue consolidates all four into a single authoritative implementation backed by one private helper, removing the redundant copies while preserving the exact externally-observable result.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Audit all four `is_settleable` bodies and document any behavioral differences between them before collapsing.
- Introduce one private helper (e.g. `fn settleable_now(env: &Env) -> bool`) that encodes the canonical rule: funded status, maturity reached, and no legal hold blocking settlement.
- Leave exactly one `pub fn is_settleable` exported, delegating to the helper; delete the rest.
- Confirm the WASM export surface is unchanged for clients (one symbol, same name and signature).
- Preserve every existing invariant: legal-hold gate, maturity gate, and funded-status gate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-dedup-is-settleable`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — collapse the four `is_settleable` definitions into one plus a shared private helper.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/settlement.rs`](contracts/escrow/src/tests/settlement.rs) — assert the single surviving view returns identical results across open / funded / matured / held / settled states.
  - **Add documentation:** note the canonical predicate in [`README.md`](README.md) and any settlement doc under `docs/`.
  - Include NatSpec-style doc comments (`///`) on the surviving entrypoint and the helper.
  - Validate security assumptions: no relaxation of the maturity or legal-hold gate during the merge.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: pre-maturity, exact-maturity, legal-hold-active, already-settled, and never-funded.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`refactor: collapse four divergent is_settleable copies into one canonical view`

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
title: "Resolve the conflicting get_sme_collateral_commitment return types into one stable view"
labels: type:refactor, area:collateral, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Resolve the conflicting get_sme_collateral_commitment return types into one stable view

### Description
[`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) defines `get_sme_collateral_commitment` twice with **two different return types** — one returns `Option<CollateralPledge>` (around line 1632) and another returns `Option<SmeCollateralCommitment>` (around line 2229). Two public getters with the same name but divergent return types is an ambiguous, client-breaking surface: SDK consumers cannot rely on a stable shape, and the `record_sme_collateral_commitment` writer stores `SmeCollateralCommitment`, so at least one getter may decode a foreign layout.

This issue picks the single correct type that matches what `record_sme_collateral_commitment` persists under `DataKey::SmeCollateralPledge`, removes the conflicting definition, and ensures one coherent collateral read API.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Determine which struct (`CollateralPledge` vs `SmeCollateralCommitment`) is actually written to `DataKey::SmeCollateralPledge` by `record_sme_collateral_commitment`.
- Keep exactly one `get_sme_collateral_commitment` whose return type matches the stored type; remove the other.
- If both structs are genuinely needed, document the mapping and provide an explicit conversion rather than two same-named getters.
- Update `clear_sme_collateral_commitment` and `EscrowSummary` references to use the unified type.
- Preserve the record-only semantics of the collateral metadata (no asset custody implied).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-unify-collateral-getter`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — remove the duplicate getter and align the return type with the stored struct.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/attestations.rs`](contracts/escrow/src/tests/attestations.rs) — record a commitment, read it back, and assert the decoded fields round-trip.
  - **Add documentation:** clarify the collateral read API in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the surviving getter.
  - Validate security assumptions: no decode panic when the stored layout differs from the getter's type.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: absent commitment, recorded commitment, replaced commitment, and cleared commitment.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`refactor: unify get_sme_collateral_commitment to a single stored-type-matched view`

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
title: "Emit a structured event from sweep_terminal_dust recording the swept amount and recipient"
labels: type:feature, area:treasury, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Emit a structured event from sweep_terminal_dust recording the swept amount and recipient

### Description
`sweep_terminal_dust(env, amount)` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) moves residual funding-token dust to the treasury after a terminal state, enforcing a liability floor — but it emits **no `contractevent`**. Every other state-changing entrypoint (`funded`, `sme_wd`, `inv_claim`, `refunded`, `fund_can`, etc.) publishes a structured event, so indexers and reconciliation tooling currently cannot observe treasury sweeps on-chain. This is a transparency gap on a privileged, fund-moving admin path.

This issue adds a dedicated `dust_swept` event carrying the effective swept amount, the treasury recipient, and the invoice id, consistent with the existing event catalog.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Define a new event struct (e.g. `DustSweptEvt`) with a unique `symbol_short!` topic name not already used elsewhere in `lib.rs`.
- Publish it at the end of `sweep_terminal_dust` with the effective amount actually transferred, the treasury address, and `invoice_id`.
- Ensure the event fires only on a successful sweep, after the balance-checked transfer, never on an error path.
- Keep the existing liability-floor enforcement (`SweepExceedsLiabilityFloor`) and all typed error returns unchanged.
- Update the events catalog documentation to include the new event.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-dust-swept-event`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the event struct and publish it inside `sweep_terminal_dust`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/external_calls.rs`](contracts/escrow/src/tests/external_calls.rs) — assert the event topic, amount, and recipient on a successful sweep and assert no event on the floor-exceeded failure.
  - **Add documentation:** add the event to the catalog in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the new event struct.
  - Validate security assumptions: the event payload reflects the real transferred amount, not the requested amount.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: zero-effective sweep, capped sweep, legal-hold-blocked sweep, and non-terminal status.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: emit dust_swept event from sweep_terminal_dust with amount and recipient`

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
title: "Add a claimable-payout preview view that reports an investor's pending unclaimed amount"
labels: type:feature, area:payouts, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a claimable-payout preview view that reports an investor's pending unclaimed amount

### Description
`compute_investor_payout(env, investor)` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) returns the gross pro-rata payout an investor is owed, but it does not account for whether that investor has **already claimed** (`DataKey::InvestorClaimed`) or whether the claim-not-before gate has elapsed. Front-ends must combine three separate calls (`compute_investor_payout`, `is_investor_claimed`, `get_investor_claim_not_before`) and re-implement the gating logic to show a correct "claimable now" figure, which is error-prone.

This issue adds a single read-only view that returns the amount an investor can claim **right now** — zero if already claimed, zero if the claim-not-before timestamp has not elapsed or the escrow is not settled, otherwise the computed payout.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_claimable_payout(env, investor) -> i128` returning `0` when: not settled, legal hold blocks claims, already claimed, or `now < InvestorClaimNotBefore`.
- Otherwise return the same figure `claim_investor_payout` would transfer, reusing `compute_investor_payout` internally — no duplicated math.
- Keep it a pure read (no storage writes, no auth) so it is safe to call from simulation.
- Document precisely how it differs from `compute_investor_payout` (gross vs net-of-gates).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-claimable-payout-view`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add `get_claimable_payout` delegating to the existing payout math and gates.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/settlement.rs`](contracts/escrow/src/tests/settlement.rs) — assert zero before settlement, zero after claim, zero before claim-not-before, and the full amount when claimable.
  - **Add documentation:** add the view to the read-API surface in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) describing every zero-returning condition.
  - Validate security assumptions: the preview never over-reports relative to what `claim_investor_payout` would actually transfer.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: unknown investor, already-claimed, gated, legal-hold-active, and fully claimable.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add get_claimable_payout preview view accounting for claim gates`

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
title: "Add an expiry deadline to pending admin proposals so stale handovers auto-invalidate"
labels: type:feature, area:admin, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add an expiry deadline to pending admin proposals so stale handovers auto-invalidate

### Description
`propose_admin(env, new_admin)` writes `DataKey::PendingAdmin` and `accept_admin(env)` consumes it, but the proposal **never expires**. A `PendingAdmin` written long ago can be accepted at any future time — including after the proposed key has been rotated, compromised, or forgotten — because there is no time bound on acceptance. The two-step handover protects against fat-fingering the wrong address, but a perpetually-valid pending proposal is a latent governance hazard.

This issue adds an optional expiry timestamp recorded at proposal time, after which `accept_admin` rejects the stale proposal with a typed error.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Persist a proposal expiry timestamp alongside `PendingAdmin` (new `DataKey` variant, append-only).
- Accept an optional validity window in `propose_admin` (or derive from a configurable default constant); store `now + window`.
- Make `accept_admin` check `now <= expiry` and return a new append-only typed error (e.g. `AdminProposalExpired`) otherwise.
- Have `cancel_pending_admin` clear the expiry alongside the proposal.
- Preserve the existing `adm_prop` / `admin` events and same-admin rejection (`NewAdminSameAsCurrent`).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-admin-proposal-expiry`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the expiry key, the proposal-time write, and the acceptance-time check.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/admin.rs`](contracts/escrow/src/tests/admin.rs) — assert acceptance before expiry succeeds, after expiry fails, and cancellation clears expiry.
  - **Add documentation:** update the admin-handover docs in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the new error and the expiry semantics.
  - Validate security assumptions: an expired proposal cannot be accepted and does not block a fresh proposal.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: accept exactly at expiry, accept one second past expiry, re-propose after expiry, and cancel before expiry.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add expiry deadline to pending admin proposals with typed expiry error`

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
title: "Add a reconciliation view comparing the live token balance against outstanding liabilities"
labels: type:feature, area:reconciliation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a reconciliation view comparing the live token balance against outstanding liabilities

### Description
The contract exposes `get_token_balance(env)` (the live SEP-41 balance held by the contract) and `get_distributed_principal(env)` (principal already returned), but offers no single view that tells an operator whether the contract is **solvent** for its remaining obligations. Reconciliation tooling must fetch the balance, the funded amount, the distributed principal, and the settlement state separately and compute the surplus/deficit off-chain, duplicating the exact liability arithmetic that `sweep_terminal_dust` already encodes internally.

This issue adds a read-only view returning the contract's reconciliation position: live balance, outstanding investor liability, and the resulting surplus (sweepable dust) or deficit.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a `ReconciliationView` struct (`token_balance`, `outstanding_liability`, `surplus`) and `get_reconciliation(env) -> ReconciliationView`.
- Compute `outstanding_liability` using the same formula `sweep_terminal_dust` relies on (`funded_amount - distributed_principal`, floored at zero), so the two never disagree.
- Use saturating/checked arithmetic; the view must never panic on extreme values.
- Keep it a pure read with no auth and no storage writes.
- Cross-reference `sweep_terminal_dust`'s liability floor in the doc comment.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-reconciliation-view`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the struct and `get_reconciliation` reusing the liability formula.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/external_calls.rs`](contracts/escrow/src/tests/external_calls.rs) — assert surplus equals sweepable dust before and after partial refunds.
  - **Add documentation:** add the view to the read-API surface and reconciliation guide in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the struct fields and the surplus definition.
  - Validate security assumptions: surplus never exceeds what `sweep_terminal_dust` would permit.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: zero balance, over-funded balance, fully-distributed, and partial-refund states.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add get_reconciliation view comparing token balance to outstanding liability`

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
title: "Add a batch attestation-digest revocation entrypoint for multiple indices in one call"
labels: type:feature, area:attestations, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a batch attestation-digest revocation entrypoint for multiple indices in one call

### Description
`revoke_attestation_digest(env, index)` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) revokes a single entry of the append-only attestation log per transaction. When a compliance event invalidates several digests at once (e.g. a superseded document bundle), an admin must submit one transaction per index, each paying its own fees and emitting a separate `att_rev` event, with no atomicity across the set.

This issue adds an admin-gated batch revocation entrypoint that revokes a bounded list of indices atomically, mirroring the existing single-revoke validation and the `fund_batch` bounded-batch pattern.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `revoke_attestation_digests(env, indices: Vec<u32>)` requiring admin auth.
- Reject empty and oversized batches with append-only typed errors (reuse the batch-bound pattern; cap at a sensible constant).
- Validate each index against the log length (`AttestationIndexOutOfRange`) and skip-or-fail clearly on already-revoked entries (`AttestationAlreadyRevoked`) — choose and document one policy.
- Emit one `att_rev` event per newly revoked index, preserving the existing single-revoke event shape.
- Preserve the original digests for auditability (revocation is a marker, not a delete).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-batch-attestation-revoke`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the batch entrypoint reusing the single-revoke validation helper.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/attestations.rs`](contracts/escrow/src/tests/attestations.rs) — assert atomic revocation, out-of-range rejection, duplicate-index handling, and empty/oversized batch errors.
  - **Add documentation:** update the attestation docs in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) describing batch bounds and the duplicate policy.
  - Validate security assumptions: only admin can revoke; a partial failure rolls back the whole batch.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: empty batch, oversized batch, out-of-range index, already-revoked index, and unauthorized caller.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add batch attestation-digest revocation entrypoint with bounded indices`

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
title: "Add a single-index attestation log reader returning the digest at a given position"
labels: type:feature, area:attestations, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a single-index attestation log reader returning the digest at a given position

### Description
`get_attestation_append_log(env)` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) returns the entire append-only digest vector. As the log grows toward `MAX_ATTESTATION_APPEND_ENTRIES`, a client that only needs one digest — for example to verify a single revoked entry surfaced by `is_attestation_revoked(index)` — must transfer and decode the whole vector. There is no targeted accessor for a single position.

This issue adds a bounded single-index reader returning the digest at a given index (with its revocation status), complementing the existing whole-log read.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_attestation_digest_at(env, index: u32) -> Option<BytesN<32>>` returning `None` for out-of-range indices.
- Optionally return a small struct pairing the digest with its `is_attestation_revoked` flag for one round-trip.
- Keep it a pure read with no auth and no writes.
- Reuse the same length/bounds logic the revoke path uses so behavior is consistent.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-attestation-digest-at`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the single-index reader.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/attestations.rs`](contracts/escrow/src/tests/attestations.rs) — assert in-range, out-of-range (`None`), and revoked-flag pairing.
  - **Add documentation:** add the view to the read-API surface in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the bounds behavior.
  - Validate security assumptions: no panic on `u32::MAX` index; consistent with whole-log indexing.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: empty log, first/last index, out-of-range, and revoked entry.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add get_attestation_digest_at single-index reader with revocation status`

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
title: "Add a count-of-attestations view exposing append-log length and remaining capacity"
labels: type:feature, area:attestations, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a count-of-attestations view exposing append-log length and remaining capacity

### Description
The append-only attestation log is bounded by `MAX_ATTESTATION_APPEND_ENTRIES` (32), and `append_attestation_digest` fails with `AttestationAppendLogCapacityReached` once full. But there is no cheap view telling a client how many digests have been appended or how much headroom remains — they must fetch the entire log via `get_attestation_append_log` and count, then know the constant out-of-band.

This issue adds a lightweight view returning the current length and the remaining capacity, so integrators can warn before the log fills.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_attestation_log_stats(env) -> (u32, u32)` (or a small named struct) returning `(used, remaining)` where `used + remaining == MAX_ATTESTATION_APPEND_ENTRIES`.
- Read the length without materializing/decoding the full vector where the SDK allows, or document why a full read is necessary.
- Keep it a pure read with no auth and no writes.
- Reference `MAX_ATTESTATION_APPEND_ENTRIES` and the capacity error in the doc comment.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-attestation-log-stats`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the stats view.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/attestations.rs`](contracts/escrow/src/tests/attestations.rs) — assert `used`/`remaining` after 0, several, and a full log near the cap.
  - **Add documentation:** add the view to the read-API surface in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) tying `remaining` to the capacity error.
  - Validate security assumptions: stats stay consistent across appends and the capacity boundary.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: empty log, partially filled, exactly full, and post-capacity-error state.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add get_attestation_log_stats view exposing used and remaining capacity`

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
title: "Add a paginated read API enumerating allowlisted investor addresses"
labels: type:feature, area:allowlist, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a paginated read API enumerating allowlisted investor addresses

### Description
The allowlist is queryable only one address at a time via `is_investor_allowlisted(env, investor)`. When `set_investors_allowlisted` has been used to admit many addresses, there is no way to enumerate the current allowlist on-chain — an operator or auditor cannot answer "who is currently allowed to fund?" without already knowing every candidate address. The investor-position API already established a paginated `get_investors(start, limit)` pattern; the allowlist lacks the equivalent.

This issue adds a paginated enumeration of currently-allowlisted addresses, mirroring the existing pagination convention.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Maintain an indexable record of allowlisted addresses as entries are toggled in `set_investor_allowlisted` / `set_investors_allowlisted` (append on grant; handle revoke without leaving stale `true` reads).
- Add `get_allowlisted_investors(env, start: u32, limit: u32) -> Vec<Address>` and a count view.
- Bound `limit` to prevent unbounded reads, consistent with `get_investors`.
- Ensure enumeration reflects the live `InvestorAllowlisted` truth (no addresses that have since been revoked).
- Preserve the `al_set` / `al_batch` events and `AllowlistActive` gate semantics.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-allowlist-enumeration`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the index storage and the paginated view.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test_allowlist_tests.rs`](contracts/escrow/src/test_allowlist_tests.rs) — assert pagination, post-revoke consistency, and limit bounds.
  - **Add documentation:** update the allowlist docs in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the pagination contract.
  - Validate security assumptions: revoked addresses never appear; reads cannot exhaust resources.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: empty allowlist, single page, multi-page, revoked-then-listed, and oversized limit.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add paginated get_allowlisted_investors enumeration with count view`

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
title: "Add an investor-count and remaining-slots view derived from the unique-investor cap"
labels: type:feature, area:funding, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add an investor-count and remaining-slots view derived from the unique-investor cap

### Description
`get_unique_funder_count(env)` returns how many distinct investors have funded, and `get_max_unique_investors_cap(env)` returns the optional cap, but there is no single view reporting how many investor **slots remain** before `UniqueInvestorCapReached` is hit. Front-ends gating a "closing soon" indicator must fetch both values and compute the difference, re-deriving the saturating logic the funding path uses.

This issue adds a view returning remaining unique-investor capacity (or unlimited when no cap is configured).

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_remaining_investor_slots(env) -> Option<u32>` returning `None` when no cap is configured and `Some(cap - count)` (saturating, floored at zero) otherwise.
- Reuse `UniqueFunderCount` and `MaxUniqueInvestorsCap` reads; no new storage.
- Keep it a pure read with no auth and no writes.
- Document the relationship to the `UniqueInvestorCapReached` error and to `lower_max_unique_investors`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-remaining-investor-slots`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the remaining-slots view.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/cap_validation.rs`](contracts/escrow/src/tests/cap_validation.rs) — assert `None` without a cap, exact remaining after funding, and zero at the cap.
  - **Add documentation:** add the view to the read-API surface in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the unlimited case.
  - Validate security assumptions: the view never goes negative and stays consistent after `lower_max_unique_investors`.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: no cap, cap not yet reached, cap exactly reached, and post-lower-cap.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add get_remaining_investor_slots view derived from the unique-investor cap`

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
title: "Add an admin entrypoint to update the legal-hold clear delay before a hold is requested"
labels: type:feature, area:compliance, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add an admin entrypoint to update the legal-hold clear delay before a hold is requested

### Description
The legal-hold clear delay (`DataKey::LegalHoldClearDelay`, read via `get_legal_hold_clear_delay`) is set at `init` and gates how long after `request_clear_legal_hold` an admin must wait before `set_legal_hold(false)` succeeds. There is no entrypoint to adjust this delay afterward, so an instance deployed with an inappropriate timelock (too short for compliance, or too long to be operable) is stuck with it for its lifetime.

This issue adds an admin-gated entrypoint to update the clear delay, with a guard preventing changes that would weaken an in-flight clear request.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `update_legal_hold_clear_delay(env, new_delay: u64)` requiring admin auth.
- Reject the update while a clear request is pending (`LegalHoldClearableAt` set) so an admin cannot shorten the timelock to bypass an active waiting period — use an append-only typed error.
- Apply the same upper-bound validation used for the init-time delay (consistent with any existing bound issue) to prevent an unclearable hold.
- Emit a structured event with the old and new delay.
- Preserve the `lh_req` / `legalhld` events and the two-phase clear semantics.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-update-legal-hold-delay`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the entrypoint, the pending-request guard, and the event.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/legal_hold.rs`](contracts/escrow/src/tests/legal_hold.rs) — assert update succeeds when idle, fails during a pending clear, and is admin-gated.
  - **Add documentation:** update the legal-hold docs in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the pending-request restriction.
  - Validate security assumptions: an in-flight clear cannot be accelerated by changing the delay.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: update while idle, update during pending clear, out-of-bound delay, and unauthorized caller.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add update_legal_hold_clear_delay guarded against in-flight clear requests`

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
title: "Add a cancel-pending-legal-hold-clear entrypoint to abort an in-flight clear request"
labels: type:feature, area:compliance, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a cancel-pending-legal-hold-clear entrypoint to abort an in-flight clear request

### Description
`request_clear_legal_hold(env)` writes `DataKey::LegalHoldClearableAt`, starting the timelock toward `set_legal_hold(false)`. If new compliance information arrives mid-window, an admin has no way to **cancel** the pending clear request — the only path is to wait out the delay and then re-assert the hold, leaving a window where the clear could be completed in error. There is a `cancel_pending_admin` for the admin handover but no equivalent for the legal-hold clear.

This issue adds an admin-gated entrypoint to cancel a pending clear request, resetting the timelock.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `cancel_clear_legal_hold(env)` requiring admin auth; remove `DataKey::LegalHoldClearableAt`.
- Return an append-only typed error when no clear request is pending.
- Emit a structured event (e.g. `lh_cancel`) with a unique symbol topic.
- The hold itself stays active; only the pending clear is aborted, so a fresh `request_clear_legal_hold` restarts the full delay.
- Preserve all existing legal-hold gates on funding, settlement, withdrawal, and claims.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-cancel-legal-hold-clear`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the cancel entrypoint and event.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/legal_hold.rs`](contracts/escrow/src/tests/legal_hold.rs) — assert cancel clears the pending state, a new request restarts the delay, and cancel without a pending request errors.
  - **Add documentation:** update the legal-hold lifecycle docs in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the restart-delay behavior.
  - Validate security assumptions: cancel cannot itself clear the hold or shorten a future delay.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: cancel with pending request, cancel without pending request, re-request after cancel, and unauthorized caller.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add cancel_clear_legal_hold to abort an in-flight clear request`

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
title: "Validate the new beneficiary differs from the current SME in rotate_beneficiary"
labels: type:enhancement, area:beneficiary, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Validate the new beneficiary differs from the current SME in rotate_beneficiary

### Description
`rotate_beneficiary(env, new_sme_address)` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) rotates the SME payout address under dual authorization. Unlike `propose_admin` — which rejects a no-op handover with `NewAdminSameAsCurrent` — `rotate_beneficiary` appears to accept a `new_sme_address` equal to the existing `sme_address`, emitting a `ben_rot` event for a rotation that changed nothing. This produces misleading audit history and wastes a dual-auth ceremony.

This issue adds a guard rejecting a no-op rotation with a typed error, matching the admin-handover precedent.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Compare `new_sme_address` against the current `escrow.sme_address` and reject equality with a new append-only typed error (e.g. `BeneficiaryUnchanged`).
- Place the check before any state write or event publish so a no-op rotation is fully rejected.
- Preserve the dual-authorization requirement (both current SME and admin) and the `ben_rot` event for genuine rotations.
- Keep error codes append-only to preserve SDK stability.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-beneficiary-unchanged-guard`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the equality guard and typed error to `rotate_beneficiary`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/admin.rs`](contracts/escrow/src/tests/admin.rs) — assert a same-address rotation errors and a genuine rotation still succeeds and emits `ben_rot`.
  - **Add documentation:** note the no-op rejection in the beneficiary-rotation docs in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the new error.
  - Validate security assumptions: dual-auth still enforced; no event on the rejected path.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: same-address rotation, genuine rotation, missing SME auth, and missing admin auth.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`enhancement: reject no-op beneficiary rotation with BeneficiaryUnchanged error`

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
title: "Reject a new funding target below the current funded amount in update_funding_target consistency tests"
labels: type:test, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Reject a new funding target below the current funded amount in update_funding_target consistency tests

### Description
`update_funding_target(env, new_target)` enforces `TargetNotPositive`, `TargetUpdateNotOpen`, and `TargetBelowFundedAmount`, and re-evaluates whether lowering the target now crosses the funded threshold. These transitions — especially a target update that immediately promotes an open escrow to funded and writes the `FundingCloseSnapshot` — are subtle and under-covered. There is no focused test asserting that lowering the target to exactly the funded amount triggers the funded transition and snapshot exactly once.

This issue adds a dedicated test module for `update_funding_target` covering the rejection bounds and the mid-update funded promotion.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert `TargetNotPositive` for `new_target <= 0`, `TargetBelowFundedAmount` when below current `funded_amount`, and `TargetUpdateNotOpen` when not open.
- Assert that lowering the target to exactly `funded_amount` promotes status to funded and writes `FundingCloseSnapshot` once (immutable thereafter).
- Assert the `fund_tgt` event fires with the correct old/new values.
- Assert a subsequent fund attempt after the promotion is rejected (`EscrowNotOpenForFunding`).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-update-funding-target-transitions`
- Implement changes
  - **Write code in:** no production change expected; if a gap is found, fix it in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/funding.rs`](contracts/escrow/src/tests/funding.rs) — the rejection matrix and the funded-promotion path.
  - **Add documentation:** note the target-lowering promotion rule in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on any helper added.
  - Validate security assumptions: the snapshot denominator is captured exactly once.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: zero/negative target, below-funded target, not-open status, exact-funded promotion, and post-promotion fund rejection.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: cover update_funding_target bounds and mid-update funded promotion`

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
title: "Add tests asserting refund emits the refunded event and increments distributed principal"
labels: type:test, area:refunds, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests asserting refund emits the refunded event and increments distributed principal

### Description
`refund(env, investor)` follows checks-effects-interactions: it zeroes the contribution, marks `InvestorRefunded`, increments `DistributedPrincipal`, performs a balance-checked token transfer, and publishes a `refunded` event. This path is central to the cancelled-escrow flow and feeds the `sweep_terminal_dust` liability floor, yet there is no focused test asserting the full set of state mutations and the event payload together, nor the double-refund guard.

This issue adds a dedicated refund test module verifying the event, the distributed-principal accounting, and idempotency.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert `refund` requires investor auth and only works when status is cancelled (`RefundNotCancelled` otherwise).
- Assert the `refunded` event carries the correct investor, invoice id, and amount.
- Assert `DistributedPrincipal` increases by exactly the refunded amount and `InvestorRefunded` becomes true.
- Assert a second refund of the same investor fails (`NoContributionToRefund`) — no double-spend.
- Assert the balance-checked transfer actually moved tokens out of the contract using a registered mock token.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-refund-event-and-accounting`
- Implement changes
  - **Write code in:** no production change expected; if a gap is found, fix it in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/funding.rs`](contracts/escrow/src/tests/funding.rs) — the full refund mutation and event assertions.
  - **Add documentation:** note the refund accounting in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on any test helper.
  - Validate security assumptions: no double-refund; distributed-principal stays consistent with the liability floor.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: refund when not cancelled, refund with zero contribution, double refund, and unauthorized caller.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: assert refund emits refunded event and increments distributed principal`

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
title: "Add tests for the inbound funding-token transfer balance-delta wrapper in external_calls"
labels: type:test, area:external-calls, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for the inbound funding-token transfer balance-delta wrapper in external_calls

### Description
[`contracts/escrow/src/external_calls.rs`](contracts/escrow/src/external_calls.rs) exposes `transfer_funding_token_inbound_with_balance_checks`, the inbound counterpart to the outbound wrapper, which guards against malicious or non-conforming SEP-41 tokens by asserting expected balance deltas on both sender and recipient. The outbound wrapper has dedicated safety tests, but the inbound wrapper's delta assertions (`SenderBalanceDeltaMismatch`, `RecipientBalanceDeltaMismatch`, underflow guards) are not directly exercised against a misbehaving mock token.

This issue adds focused tests for the inbound wrapper using a mock token that under- or over-reports balances.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Use a mock SEP-41 token whose `transfer`/`balance` can be configured to deviate from expected deltas.
- Assert the happy path moves exactly `amount` inbound and updates both balances correctly.
- Assert `SenderBalanceDeltaMismatch` and `RecipientBalanceDeltaMismatch` fire when the mock misreports.
- Assert the positive-amount guard (`TransferAmountNotPositive`) and the pre-transfer balance check.
- Cover the underflow guards (`SenderBalanceUnderflow`, `RecipientBalanceUnderflow`).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-inbound-transfer-balance-checks`
- Implement changes
  - **Write code in:** no production change expected; if a defect is found, fix it in [`contracts/escrow/src/external_calls.rs`](contracts/escrow/src/external_calls.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/external_calls_mocked.rs`](contracts/escrow/src/tests/external_calls_mocked.rs) — the inbound delta-mismatch and underflow assertions.
  - **Add documentation:** note the inbound wrapper guarantees in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the mock token configuration.
  - Validate security assumptions: a hostile token cannot trick the contract into recording an unbacked inbound transfer.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: zero amount, under-reporting token, over-reporting token, and underflow conditions.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: cover inbound funding-token transfer balance-delta wrapper against hostile tokens`

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
title: "Add tests for sweep_terminal_dust liability-floor enforcement after partial refunds"
labels: type:test, area:treasury, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for sweep_terminal_dust liability-floor enforcement after partial refunds

### Description
`sweep_terminal_dust(env, amount)` computes outstanding liability as `funded_amount - distributed_principal` and rejects any sweep that would dip below it (`SweepExceedsLiabilityFloor`). The interaction between partial refunds (which raise `DistributedPrincipal`) and the shrinking liability floor is the subtle part: after some investors refund but others have not, the sweepable surplus changes, and an off-by-one here would let the treasury drain funds still owed to investors. Existing property tests touch the floor, but a deterministic scenario walking refund-then-sweep steps is missing.

This issue adds a deterministic test sequence exercising the floor as refunds accumulate.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Build a cancelled escrow with several investors, refund a subset, and assert the maximum sweepable amount equals `balance - (funded_amount - distributed_principal)` at each step.
- Assert a sweep exceeding the floor by one unit fails with `SweepExceedsLiabilityFloor`.
- Assert sweeps respect `MAX_DUST_SWEEP_AMOUNT`, the positive-amount guard, and the terminal-status requirement.
- Assert the legal-hold gate (`LegalHoldBlocksTreasuryDustSweep`) blocks sweeping while held.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-dust-sweep-floor-after-refunds`
- Implement changes
  - **Write code in:** no production change expected; if a defect is found, fix it in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/external_calls.rs`](contracts/escrow/src/tests/external_calls.rs) — the refund-then-sweep floor sequence.
  - **Add documentation:** note the floor formula in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on any helper.
  - Validate security assumptions: the treasury can never sweep principal still owed to un-refunded investors.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: no refunds, partial refunds, all refunded, over-floor sweep, capped sweep, and held sweep.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: cover sweep_terminal_dust liability floor across partial-refund sequences`

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
title: "Add tests for unrevoke_attestation_digest reversing a revocation and its guards"
labels: type:test, area:attestations, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for unrevoke_attestation_digest reversing a revocation and its guards

### Description
`unrevoke_attestation_digest(env, index)` reverses an erroneous revocation, emitting `att_unrev`, and is the counterpart to `revoke_attestation_digest`. While revocation has coverage, the un-revoke path — including its admin gate, out-of-range rejection, and the requirement that the index actually be revoked before it can be un-revoked — lacks focused tests, leaving a privileged compliance-reversal entrypoint under-verified.

This issue adds dedicated tests for the un-revoke flow and its error conditions.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert un-revoke requires admin auth and flips `is_attestation_revoked(index)` back to false.
- Assert `att_unrev` fires with the correct index.
- Assert out-of-range indices reject with `AttestationIndexOutOfRange`.
- Assert un-revoking a non-revoked index is handled by a clear typed error or documented no-op (choose and assert one).
- Assert the underlying digest is preserved throughout revoke → unrevoke cycles.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-unrevoke-attestation`
- Implement changes
  - **Write code in:** no production change expected; if a gap is found, fix it in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/attestations.rs`](contracts/escrow/src/tests/attestations.rs) — the un-revoke success and guard matrix.
  - **Add documentation:** note the un-revoke semantics in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on any helper.
  - Validate security assumptions: only admin can un-revoke; digest integrity is preserved.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: un-revoke a revoked index, un-revoke a non-revoked index, out-of-range index, and unauthorized caller.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: cover unrevoke_attestation_digest reversal and guard conditions`

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
title: "Add tests for rotate_beneficiary dual-authorization and the post-rotation payout target"
labels: type:test, area:beneficiary, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for rotate_beneficiary dual-authorization and the post-rotation payout target

### Description
`rotate_beneficiary(env, new_sme_address)` changes who receives SME liquidity at `withdraw()`, under dual authorization. Beyond asserting both signatures are required, the critical end-to-end property is that after a successful rotation the **withdraw actually pays the new address** — a test that ties rotation to the downstream `sme_wd` transfer is missing, so a regression that updated the stored address but kept paying the old SME could slip through.

This issue adds an end-to-end test rotating the beneficiary and asserting the subsequent withdrawal routes funds to the new address.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert rotation requires both current SME and admin auth (missing either fails).
- After rotation, fund to target, settle/withdraw, and assert the token transfer credits the **new** SME address, not the old one.
- Assert the `ben_rot` event carries old and new addresses.
- Use a registered mock token to verify the real balance delta on withdrawal.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-rotate-beneficiary-e2e`
- Implement changes
  - **Write code in:** no production change expected; if a defect is found, fix it in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/integration.rs`](contracts/escrow/src/tests/integration.rs) — the rotate-then-withdraw end-to-end assertion.
  - **Add documentation:** note the rotation-to-payout linkage in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on any helper.
  - Validate security assumptions: withdrawal pays exactly the current stored beneficiary.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: missing SME auth, missing admin auth, withdrawal after rotation, and no rotation (baseline).
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: assert beneficiary rotation routes the subsequent withdrawal to the new SME`

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
title: "Add tests for cancel_funding state transition and the gate against cancelling a funded escrow"
labels: type:test, area:lifecycle, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for cancel_funding state transition and the gate against cancelling a funded escrow

### Description
`cancel_funding(env)` moves an open escrow to the cancelled state (4), emitting `fund_can` and unlocking the `refund` path for investors. Because cancellation is the gateway to investor refunds, the rules around **when** cancellation is permitted are safety-critical: cancelling an already-funded, settled, or withdrawn escrow must be impossible, or it could strip the SME of liquidity it is owed. A focused test asserting the allowed/forbidden source states is missing.

This issue adds tests for the cancellation transition matrix and the resulting refund unlock.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert `cancel_funding` succeeds only from the open state and emits `fund_can`.
- Assert it is rejected from funded, settled, withdrawn, and already-cancelled states with the appropriate typed error.
- Assert cancellation requires the correct authorization (admin-gated).
- Assert that after cancellation, `refund` becomes available and `withdraw`/`settle` remain blocked.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-cancel-funding-transitions`
- Implement changes
  - **Write code in:** no production change expected; if a gap is found, fix it in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/integration.rs`](contracts/escrow/src/tests/integration.rs) — the cancellation transition matrix and refund unlock.
  - **Add documentation:** note the cancellable source states in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on any helper.
  - Validate security assumptions: a funded SME cannot be deprived of liquidity by a late cancellation.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: cancel from open, funded, settled, withdrawn, and cancelled; unauthorized caller.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: cover cancel_funding transition matrix and the post-cancel refund unlock`

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
title: "Add tests for update_maturity_max_horizon bounds and its effect on later maturity updates"
labels: type:test, area:maturity, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for update_maturity_max_horizon bounds and its effect on later maturity updates

### Description
`update_maturity_max_horizon(env, new_horizon)` adjusts the ceiling (`DataKey::MaturityMaxHorizon`, default `DEFAULT_MATURITY_MAX_HORIZON_SECS`) that `update_maturity` validates against, emitting `mtry_max`. The interaction is subtle: lowering the horizon must not retroactively invalidate an already-set maturity, but it must constrain the **next** `update_maturity`. There is no focused test asserting that a lowered horizon rejects a subsequently-too-far maturity while leaving the current one intact.

This issue adds tests for the horizon update and its downstream effect on maturity validation.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert `update_maturity_max_horizon` is admin-gated and emits `mtry_max` with old/new values.
- Assert that after lowering the horizon, an `update_maturity` beyond `now + new_horizon` is rejected with the documented error.
- Assert an `update_maturity` within the new horizon still succeeds.
- Assert the default-horizon fallback applies when the key is unset.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-maturity-max-horizon`
- Implement changes
  - **Write code in:** no production change expected; if a gap is found, fix it in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/admin.rs`](contracts/escrow/src/tests/admin.rs) — the horizon bounds and downstream maturity-update assertions.
  - **Add documentation:** note the horizon-to-maturity relationship in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on any helper.
  - Validate security assumptions: lowering the horizon cannot strand a valid existing maturity.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: default fallback, lowered horizon rejecting a far maturity, within-horizon maturity, and unauthorized caller.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: cover update_maturity_max_horizon bounds and downstream maturity validation`

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
title: "Add tests for fund_with_commitment claim-lock interaction with maturity and settlement"
labels: type:test, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for fund_with_commitment claim-lock interaction with maturity and settlement

### Description
`fund_with_commitment` records a per-investor commitment lock (`DataKey::InvestorClaimNotBefore`) that delays `claim_investor_payout` beyond standard settlement, and rejects a tiered second deposit (`TieredSecondDeposit`) and a lock exceeding maturity (`CommitmentLockExceedsMaturity`). The interplay between the commitment lock, the settlement gate, and the maturity bound is intricate, and there is no focused test asserting that a committed investor can only claim after **both** settlement and their personal lock have elapsed.

This issue adds tests for the commitment-lock claim timing and its bounds.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert a committed investor's `claim_investor_payout` is blocked until `now >= InvestorClaimNotBefore` even after settlement (`InvestorCommitmentLockNotExpired`).
- Assert `TieredSecondDeposit` rejects a second commitment deposit from the same investor.
- Assert `CommitmentLockExceedsMaturity` rejects a lock past maturity.
- Assert the effective yield recorded reflects the committed tier (`get_effective_yield_bps`).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-commitment-lock-claim-timing`
- Implement changes
  - **Write code in:** no production change expected; if a gap is found, fix it in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/settlement.rs`](contracts/escrow/src/tests/settlement.rs) — the commitment-lock claim-timing assertions.
  - **Add documentation:** note the lock-vs-settlement timing in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on any helper.
  - Validate security assumptions: a committed investor cannot claim before their lock elapses.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: claim before lock, claim after lock, second commitment deposit, and lock past maturity.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: cover fund_with_commitment claim-lock timing against settlement and maturity`

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
title: "Add tests verifying get_remaining_funding_capacity tracks funded amount across deposits"
labels: type:test, area:funding, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests verifying get_remaining_funding_capacity tracks funded amount across deposits

### Description
`get_remaining_funding_capacity(env)` reports how much principal can still be accepted before the funding target is met. As deposits accumulate via `fund` and `fund_batch`, this view must shrink monotonically to exactly zero at the target and must never report negative capacity, even if a target update changes the denominator mid-flight. There is no focused test walking the capacity down across several deposits and a target update.

This issue adds tests verifying the capacity view stays consistent across the funding lifecycle.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert capacity equals `funding_target - funded_amount` (floored at zero) after each deposit.
- Assert capacity reaches exactly zero when the target is met and the escrow promotes to funded.
- Assert capacity recomputes correctly after `update_funding_target` raises or lowers the target while open.
- Assert capacity is never negative even at or beyond the target.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-remaining-funding-capacity`
- Implement changes
  - **Write code in:** no production change expected; if a gap is found, fix it in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/funding.rs`](contracts/escrow/src/tests/funding.rs) — the capacity-tracking assertions across deposits and a target update.
  - **Add documentation:** note the capacity formula in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on any helper.
  - Validate security assumptions: capacity cannot be exploited to over-fund past the target.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: single deposit, multiple deposits, target raise, target lower, and exact-target promotion.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: verify get_remaining_funding_capacity tracks funded amount across deposits`

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
title: "Add property-based tests asserting refunds never exceed total funded principal"
labels: type:test, area:properties, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add property-based tests asserting refunds never exceed total funded principal

### Description
In a cancelled escrow, the sum of all per-investor refunds must never exceed `funded_amount`, and `DistributedPrincipal` must converge to exactly the total contributed when every investor refunds. The existing proptest suite covers payout rounding and the dust floor, but there is no property asserting the global refund conservation invariant across arbitrary investor sets and refund orderings.

This issue adds a proptest invariant for refund conservation.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Generate arbitrary sets of investors with arbitrary contributions, fund, cancel, then refund in arbitrary order.
- Assert `sum(refunds) == sum(contributions)` once all investors refund, and `DistributedPrincipal` equals that sum.
- Assert no single refund exceeds the investor's recorded contribution and double-refund is impossible.
- Assert the contract token balance never goes negative during the refund sequence.
- Seed any discovered failing case into `escrow/proptest-regressions`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-refund-conservation-proptest`
- Implement changes
  - **Write code in:** no production change expected; if a defect is found, fix it in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/properties.rs`](contracts/escrow/src/tests/properties.rs) — the refund-conservation invariant.
  - **Add documentation:** note the conservation invariant in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the property and its assumptions.
  - Validate security assumptions: no ordering of refunds can over-distribute principal.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: single investor, many investors, reverse-order refunds, and partial refunds.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: add proptest invariant asserting refunds never exceed funded principal`

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
title: "Add property-based tests that the sum of investor payouts never exceeds the settled pool"
labels: type:test, area:properties, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add property-based tests that the sum of investor payouts never exceeds the settled pool

### Description
At settlement, the total claimable across all investors (principal plus yield via `compute_investor_payout`) must never exceed the settled pool the contract is obligated to distribute. Pro-rata rounding is already proptest-covered for a single investor, but there is no global property asserting that summing every investor's payout stays within the pool — the place where rounding dust could accumulate into an over-distribution.

This issue adds a proptest invariant bounding the aggregate payout by the settled pool.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Generate arbitrary investor sets and yield configs, fund to target, settle, and sum `compute_investor_payout` over all investors.
- Assert the aggregate is `<=` the settled pool (principal + total yield owed), with rounding bias favoring the contract, never the investors collectively.
- Assert the snapshot denominator (`FundingCloseSnapshot`) is used consistently for every investor's share.
- Seed any failing case into `escrow/proptest-regressions`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-aggregate-payout-bound-proptest`
- Implement changes
  - **Write code in:** no production change expected; if a defect is found, fix it in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/properties.rs`](contracts/escrow/src/tests/properties.rs) — the aggregate-payout bound invariant.
  - **Add documentation:** note the aggregate bound in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the property.
  - Validate security assumptions: collective payouts cannot exceed the pool due to rounding.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: equal contributions, skewed contributions, single investor, and many small investors.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: add proptest invariant bounding aggregate investor payouts by the settled pool`

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
title: "Add tests for the allowlist gate blocking non-allowlisted funders when active"
labels: type:test, area:allowlist, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for the allowlist gate blocking non-allowlisted funders when active

### Description
When `AllowlistActive` is true, both `fund` and `fund_with_commitment` must reject any investor not marked `InvestorAllowlisted`, returning `InvestorNotAllowlisted`. Toggling the allowlist on/off mid-funding and adding/removing addresses via `set_investor_allowlisted` / `set_investors_allowlisted` creates several gating combinations. The existing allowlist tests focus on the setters; a focused test asserting the funding gate itself across active/inactive and allowed/denied combinations is missing.

This issue adds tests for the funding-time allowlist enforcement.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert that with the allowlist active, a non-allowlisted `fund` and `fund_with_commitment` both fail with `InvestorNotAllowlisted`.
- Assert an allowlisted investor funds successfully while the gate is active.
- Assert disabling the allowlist (`set_allowlist_active(false)`) lets any investor fund.
- Assert revoking an investor mid-funding blocks their next deposit even if they previously contributed.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-allowlist-funding-gate`
- Implement changes
  - **Write code in:** no production change expected; if a gap is found, fix it in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test_allowlist_tests.rs`](contracts/escrow/src/test_allowlist_tests.rs) — the funding-gate enforcement matrix.
  - **Add documentation:** note the funding-time gate in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on any helper.
  - Validate security assumptions: a revoked investor cannot bypass the gate via a prior contribution.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: active+denied, active+allowed, inactive+any, and revoke-then-fund.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: cover allowlist funding gate across active/inactive and allowed/denied cases`

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
title: "Convert remaining panic-string guards in fund_batch into typed EscrowError codes"
labels: type:refactor, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Convert remaining panic-string guards in fund_batch into typed EscrowError codes

### Description
`fund_batch(env, entries)` already returns typed errors for empty (`FundingBatchEmpty`), oversized (`FundingBatchTooLarge`), and duplicate (`FundingBatchDuplicateInvestor`) batches, but the per-entry funding loop may still surface bare `panic!`/`assert!` strings for conditions that have no dedicated discriminant. Panic strings produce opaque host errors that SDK clients cannot match on, breaking the otherwise-typed error contract this contract maintains elsewhere.

This issue audits `fund_batch` and its inner `fund_impl` for any remaining panic-string guards and converts them to append-only typed `EscrowError` codes.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Enumerate every `panic!`, `assert!`, `unwrap`, and `expect` reachable from `fund_batch` and the shared funding helper.
- Replace each with an existing or new append-only `EscrowError` variant; do not reuse or renumber existing discriminants.
- Preserve atomicity: a failing entry rolls back the whole batch.
- Keep the `funded` promotion and `FundingCloseSnapshot` write behavior identical when the batch crosses the target.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-fund-batch-typed-errors`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — replace panic strings in the batch funding path with typed errors.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/funding.rs`](contracts/escrow/src/tests/funding.rs) — assert each converted condition returns the typed error, not a panic.
  - **Add documentation:** extend the error-code reference in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on each new error.
  - Validate security assumptions: batch atomicity is preserved on every error path.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: over-cap entry, below-floor entry, overflow entry, and a mid-batch failure rollback.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`refactor: convert fund_batch panic-string guards to typed EscrowError codes`

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
title: "Extract the repeated status-equality guard checks into a single shared helper"
labels: type:refactor, area:lifecycle, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Extract the repeated status-equality guard checks into a single shared helper

### Description
Across [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs), many entrypoints assert the escrow is in a specific status before proceeding — `refund` uses `guard_status_eq(&env, status, 4, RefundNotCancelled)`, and similar status checks appear in `settle`, `withdraw`, `cancel_funding`, `update_funding_target`, and others, each with its own inline comparison and error. This repeated `status == N → else Error` pattern is duplicated boilerplate that is easy to get subtly wrong (wrong status code, wrong error).

This issue consolidates the status-gate pattern behind one well-tested helper used consistently by every status-gated entrypoint.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Audit all status comparisons and route them through a single `guard_status_eq` (or a small family covering eq/one-of) helper.
- Ensure each call site passes the correct expected status and the correct typed error — no behavioral change.
- Add a `guard_status_in(&[..])` variant if multiple source states are valid (e.g. for views that accept funded-or-settled).
- Keep the status code legend (0 open, 1 funded, 2 settled, 3 withdrawn, 4 cancelled) documented at the helper.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-status-guard-helper`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — centralize status gating behind the helper.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/integration.rs`](contracts/escrow/src/tests/integration.rs) — assert each gated entrypoint still rejects the wrong status with the same error.
  - **Add documentation:** note the status legend and helper in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the helper(s).
  - Validate security assumptions: no entrypoint's gate is loosened by the refactor.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: each gated entrypoint called from a wrong status.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`refactor: centralize escrow status-equality guards into a shared helper`

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
title: "Extract repeated unique-funder-count increment logic into a shared funding helper"
labels: type:refactor, area:funding, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Extract repeated unique-funder-count increment logic into a shared funding helper

### Description
The logic that detects a brand-new investor (zero prior contribution), increments `DataKey::UniqueFunderCount`, and enforces `MaxUniqueInvestorsCap` lives inline in the funding path used by `fund`, `fund_with_commitment`, and `fund_batch`. Because all three deposit entrypoints must apply the same new-investor accounting, any divergence in how they update the unique-funder count is a correctness risk — for instance, a batch path that increments differently than the single path would corrupt the cap enforcement.

This issue extracts the unique-funder detection and increment into one shared helper used by every deposit path.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a private helper that, given an investor's prior and new contribution, decides whether they are new and updates `UniqueFunderCount` and the cap check atomically.
- Route `fund`, `fund_with_commitment`, and `fund_batch` through it.
- Preserve `UniqueInvestorCapReached` semantics and the exact count behavior (incremented once per distinct address, never on top-ups).
- No change to observable counts for any existing test.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-unique-funder-helper`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — extract and apply the shared helper.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/cap_validation.rs`](contracts/escrow/src/tests/cap_validation.rs) — assert counts agree across `fund`, `fund_with_commitment`, and `fund_batch`.
  - **Add documentation:** note the shared accounting in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the helper.
  - Validate security assumptions: the cap cannot be bypassed via any deposit path.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: first deposit, top-up, batch with mixed new/existing, and cap boundary.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`refactor: extract unique-funder-count increment into a shared funding helper`

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
title: "Deduplicate the repeated get_escrow definition into a single shared loader"
labels: type:refactor, area:storage, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Deduplicate the repeated get_escrow definition into a single shared loader

### Description
[`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) defines `pub fn get_escrow(env: Env) -> InvoiceEscrow` more than once (around lines 1373 and 1619). As with the duplicated `is_settleable` and `get_sme_collateral_commitment`, two copies of the primary state loader risk drifting — for example one bumping TTL and the other not — and confuse the exported surface. `get_escrow` is the most-called read in the contract, so a single canonical implementation matters.

This issue collapses the duplicate `get_escrow` definitions into one, backed by the existing internal escrow loader.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Identify both `get_escrow` bodies and confirm whether they differ (e.g. TTL bump, error on missing).
- Keep exactly one `pub fn get_escrow`, delegating to the canonical private loader used internally.
- Ensure a not-initialized read returns the documented typed error rather than panicking, consistently.
- No change to the returned `InvoiceEscrow` shape or values for existing callers.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-dedup-get-escrow`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — collapse the duplicate loaders.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/init.rs`](contracts/escrow/src/tests/init.rs) — assert the single loader returns identical state and the same not-initialized behavior.
  - **Add documentation:** note the canonical loader in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the surviving `get_escrow`.
  - Validate security assumptions: no behavioral divergence (TTL, error) between the removed and kept copies.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: pre-init read, post-init read, and read after a state transition.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`refactor: collapse duplicate get_escrow definitions into one canonical loader`

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
title: "Document the cancelled-escrow refund lifecycle and its interaction with dust sweeping"
labels: type:docs, area:refunds, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the cancelled-escrow refund lifecycle and its interaction with dust sweeping

### Description
The cancelled branch (status 4) unlocks `refund`, tracks `DistributedPrincipal`, and bounds `sweep_terminal_dust` by the outstanding liability (`funded_amount - distributed_principal`). These pieces are individually documented in code comments, but there is no single narrative explaining the end-to-end cancellation → refund → residual-sweep flow, how the liability floor protects un-refunded investors, and what an operator should do with leftover dust. Integrators handling cancellations currently have to reverse-engineer this from `lib.rs`.

This issue writes an end-to-end document for the cancellation and refund lifecycle.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Describe the transition into status 4 via `cancel_funding`, who may trigger it, and what it unlocks.
- Document `refund` (auth, idempotency, `DistributedPrincipal` accounting) and the `refunded` event.
- Explain the `sweep_terminal_dust` liability floor and why the treasury cannot sweep principal still owed.
- Include a worked example with multiple investors, partial refunds, and a final dust sweep.
- Add a sequence diagram of the cancellation → refund → sweep flow.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-cancellation-refund-lifecycle`
- Implement changes
  - **Write code in:** no production change; documentation only, cross-linking [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** if any doc example claims a behavior, back it with a test in [`contracts/escrow/src/tests/integration.rs`](contracts/escrow/src/tests/integration.rs).
  - **Add documentation:** add `docs/escrow-cancellation-refunds.md` and link it from [`README.md`](README.md).
  - Include NatSpec-style doc comments (`///`) where the doc references specific entrypoints.
  - Validate security assumptions: the doc accurately states the liability-floor protection.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: ensure any documented example matches actual contract behavior.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`docs: document the cancellation refund lifecycle and dust-sweep liability floor`

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
title: "Document the SEP-41 token-safety wrappers and the balance-delta threat model"
labels: type:docs, area:external-calls, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the SEP-41 token-safety wrappers and the balance-delta threat model

### Description
[`contracts/escrow/src/external_calls.rs`](contracts/escrow/src/external_calls.rs) wraps every funding-token movement in `transfer_funding_token_with_balance_checks` and `transfer_funding_token_inbound_with_balance_checks`, which assert pre/post balances and reject mismatches (`SenderBalanceDeltaMismatch`, `RecipientBalanceDeltaMismatch`, underflow guards). This is a deliberate defense against fee-on-transfer, rebasing, and otherwise non-conforming SEP-41 tokens, but the threat model and the guarantees these wrappers provide are not written down anywhere integrators or auditors can find.

This issue documents the token-safety wrappers, the threats they mitigate, and their limitations.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Explain each wrapper's pre-transfer check, post-transfer delta assertion, and the errors it can raise.
- Describe the threat model: fee-on-transfer, rebasing, reentrant, and lying tokens.
- State the residual assumptions (e.g. the contract trusts the configured token address set at init).
- Recommend the class of tokens safe to configure as the funding token.
- Cross-reference the error-code reference for every wrapper error.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-token-safety-wrappers`
- Implement changes
  - **Write code in:** no production change; documentation only, cross-linking [`contracts/escrow/src/external_calls.rs`](contracts/escrow/src/external_calls.rs).
  - **Write comprehensive tests in:** ensure each documented guarantee is backed by a test in [`contracts/escrow/src/tests/external_calls_mocked.rs`](contracts/escrow/src/tests/external_calls_mocked.rs).
  - **Add documentation:** add `docs/escrow-token-safety.md` and link it from [`README.md`](README.md).
  - Include NatSpec-style doc comments (`///`) where the doc references the wrappers.
  - Validate security assumptions: the documented guarantees match the implemented checks.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: confirm documented mitigations correspond to real test cases.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`docs: document SEP-41 token-safety wrappers and the balance-delta threat model`

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
title: "Document the schema version constant and the migrate/upgrade compatibility contract"
labels: type:docs, area:upgrade, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the schema version constant and the migrate/upgrade compatibility contract

### Description
The contract pins `SCHEMA_VERSION` (currently 6), writes it at `init`, exposes it via `get_version`, and gates `migrate(from_version)` with `MigrationVersionMismatch`, `AlreadyCurrentSchemaVersion`, and `NoMigrationPath`. The `DataKey` docs repeatedly warn "never delete or rename this variant" and "keep error codes append-only," but there is no consolidated document stating the versioning and compatibility rules a contributor must follow to safely evolve storage and ship a `migrate` path.

This issue documents the schema-version and upgrade compatibility contract.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Explain `SCHEMA_VERSION`, where it is written, and how `get_version` reflects it.
- Document the additive-only rules: append `DataKey` variants, never renumber or remove `EscrowError` discriminants, never break stored layouts.
- Describe the `migrate` flow and each of its typed errors, plus when a new migration path must be added.
- Reference the `upgrade(new_wasm_hash)` admin entrypoint and how it relates to schema migration.
- Provide a checklist for contributors changing persistent storage.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-schema-version-contract`
- Implement changes
  - **Write code in:** no production change; documentation only, cross-linking [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** ensure the documented migrate errors are asserted in [`contracts/escrow/src/tests/init.rs`](contracts/escrow/src/tests/init.rs).
  - **Add documentation:** add `docs/escrow-schema-versioning.md` and link it from [`README.md`](README.md).
  - Include NatSpec-style doc comments (`///`) tying `SCHEMA_VERSION` to the doc.
  - Validate security assumptions: the doc's additive-only rules match the code's invariants.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: confirm documented migrate errors fire on the stated version inputs.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`docs: document SCHEMA_VERSION and the migrate/upgrade compatibility contract`

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
title: "Add an admin entrypoint to raise the per-investor contribution cap while funding is open"
labels: type:feature, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add an admin entrypoint to raise the per-investor contribution cap while funding is open

### Description
`MaxPerInvestorCap` (read via `get_max_per_investor_cap`) is an immutable per-address principal ceiling set at `init` and enforced on every deposit (`InvestorContributionExceedsCap`). There is a `lower_max_unique_investors` to tighten the unique-investor count, but no symmetric entrypoint to **raise** the per-investor cap when an SME wants to admit a larger anchor investor mid-raise. Today the only option is to deploy a new escrow.

This issue adds a raise-only admin entrypoint for the per-investor cap, mirroring the safety posture of the existing cap-adjustment entrypoint.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `raise_max_per_investor(env, new_cap: i128)` requiring admin auth and only callable while the escrow is open.
- Enforce raise-only: reject a `new_cap` not strictly greater than the current cap with an append-only typed error; reject when no cap is configured.
- Persist the new cap to `MaxPerInvestorCap` and emit a structured event with old/new values.
- Preserve `InvestorContributionExceedsCap` enforcement against the updated cap on subsequent deposits.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-raise-per-investor-cap`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the raise-only entrypoint and event.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/cap_validation.rs`](contracts/escrow/src/tests/cap_validation.rs) — assert raise succeeds, lowering rejects, not-open rejects, and the new cap is enforced.
  - **Add documentation:** note the cap-raise entrypoint in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the raise-only restriction.
  - Validate security assumptions: the cap can only increase, never silently decrease, and only while open.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: raise above current, equal cap, lower cap, no cap configured, not-open status, and unauthorized caller.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add raise_max_per_investor raise-only cap entrypoint with event`

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
title: "Add an admin entrypoint to lower the minimum contribution floor while funding is open"
labels: type:feature, area:funding, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add an admin entrypoint to lower the minimum contribution floor while funding is open

### Description
`MinContributionFloor` (read via `get_min_contribution_floor`) is the minimum per-call deposit, set at `init` and enforced as `FundingBelowMinContribution`. If a raise is undersubscribed, an SME may want to admit smaller tickets to close the gap, but the floor is immutable today — there is no entrypoint to lower it, so the only path is redeploying the escrow.

This issue adds a lower-only admin entrypoint for the minimum contribution floor, consistent with the raise/lower-only safety pattern used elsewhere.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `lower_min_contribution_floor(env, new_floor: i128)` requiring admin auth and only callable while open.
- Enforce lower-only: reject a `new_floor` not strictly less than the current floor; reject a non-positive floor.
- Persist to `MinContributionFloor` and emit a structured event with old/new values.
- Preserve `FundingBelowMinContribution` enforcement against the updated floor and the floor-vs-amount relationship (`MinContributionExceedsAmount`).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-lower-min-contribution-floor`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the lower-only entrypoint and event.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/cap_validation.rs`](contracts/escrow/src/tests/cap_validation.rs) — assert lower succeeds, raising rejects, not-open rejects, and the new floor is enforced.
  - **Add documentation:** note the floor-lowering entrypoint in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the lower-only restriction.
  - Validate security assumptions: the floor can only decrease, never silently increase, and only while open.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: lower below current, equal floor, higher floor, non-positive floor, not-open status, and unauthorized caller.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add lower_min_contribution_floor lower-only entrypoint with event`

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
title: "Emit a structured event from accept_admin recording the completed handover"
labels: type:enhancement, area:admin, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Emit a structured event from accept_admin recording the completed handover

### Description
`propose_admin` emits `adm_prop`, `cancel_pending_admin` emits `adm_can`, and the deprecated `transfer_admin` shim emits `depr_xfer`, but the actual completion of a two-step handover in `accept_admin` emits the generic `admin` event. Indexers reconstructing governance history cannot cleanly distinguish a freshly-proposed-then-accepted handover from a one-shot legacy transfer, because the terminal `admin` event does not carry the prior admin or signal that it completed a two-step flow.

This issue enriches the `accept_admin` event to record both the outgoing and incoming admin, making the completed two-step handover unambiguous on-chain.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Extend the event published by `accept_admin` to carry the previous admin and the new admin (and the invoice id), without renaming or reusing another entrypoint's symbol.
- If reusing the `admin` symbol, ensure its payload is consistent wherever it is emitted, or introduce a dedicated `adm_acc` symbol unique across `lib.rs`.
- Preserve the `PendingAdmin` consumption and the no-pending-admin rejection path.
- Update the events catalog and the admin-handover docs.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-accept-admin-event`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — enrich the `accept_admin` event payload/symbol.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/admin.rs`](contracts/escrow/src/tests/admin.rs) — assert the event carries the correct prior and new admin on a completed handover.
  - **Add documentation:** update the events catalog and admin docs in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the event struct.
  - Validate security assumptions: the event reflects the real admin transition; no symbol collision with other events.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: accept with a pending admin, accept without one, and event-payload correctness.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`enhancement: enrich accept_admin event with outgoing and incoming admin`

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
title: "Add a tier-lookup helper exposing which yield tier a contribution amount would fall into"
labels: type:feature, area:yield, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a tier-lookup helper exposing which yield tier a contribution amount would fall into

### Description
When a tier table is configured (`DataKey::YieldTierTable`), `fund_with_commitment` selects an investor's effective yield based on their committed amount and lock, persisting it to `InvestorEffectiveYield`. But there is no read that lets a prospective investor see, before depositing, **which tier** a given amount/lock would qualify for. They must guess from the raw tier table (if even exposed) and re-implement the selection logic, which risks disagreeing with the on-chain rule.

This issue adds a pure view that returns the tier (yield bps and lock) a hypothetical contribution would receive, reusing the exact on-chain selection logic.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `preview_yield_tier(env, amount: i128, lock: u64) -> (i64, u64)` (effective yield bps, claim-not-before delta) or a small named struct.
- Reuse the same tier-selection helper `fund_with_commitment` uses internally — do not duplicate the selection rule.
- Return the base `yield_bps` when no tier table is configured.
- Keep it a pure read with no auth and no storage writes; safe for simulation.
- Document that the preview reflects the rule applied at first deposit only.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-preview-yield-tier`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the preview view delegating to the tier-selection helper.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/funding.rs`](contracts/escrow/src/tests/funding.rs) — assert the preview matches the yield actually recorded by `fund_with_commitment` for the same amount/lock.
  - **Add documentation:** add the view to the read-API surface and the tiered-yield docs in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the no-tier-table fallback.
  - Validate security assumptions: the preview never disagrees with the on-chain selection.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: no tier table, below first tier, exact tier boundary, top tier, and lock variations.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add preview_yield_tier view reusing the on-chain tier-selection rule`

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
title: "Add tests asserting bind_primary_attestation_hash is single-set and admin-gated"
labels: type:test, area:attestations, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests asserting bind_primary_attestation_hash is single-set and admin-gated

### Description
`bind_primary_attestation_hash(env, digest)` is documented as a **single-set** admin-only binding (`PrimaryAttestationAlreadyBound` on a second call), emitting `att_bind`, and is read back via `get_primary_attestation_hash`. As the canonical compliance-document anchor, its immutability-after-first-set guarantee is security-relevant, yet there is no focused test asserting the second-bind rejection, the admin gate, and the digest round-trip together.

This issue adds dedicated tests for the primary attestation binding.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert the first bind succeeds, emits `att_bind`, and `get_primary_attestation_hash` returns the exact digest.
- Assert a second bind fails with `PrimaryAttestationAlreadyBound` and does not overwrite the digest.
- Assert the binding requires admin auth (a non-admin caller is rejected).
- Assert `get_primary_attestation_hash` returns `None` before any bind.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-primary-attestation-binding`
- Implement changes
  - **Write code in:** no production change expected; if a gap is found, fix it in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/attestations.rs`](contracts/escrow/src/tests/attestations.rs) — the single-set, admin-gate, and round-trip assertions.
  - **Add documentation:** note the single-set guarantee in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on any helper.
  - Validate security assumptions: the primary digest is immutable after first bind.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: bind before/after, double bind, unauthorized caller, and pre-bind read.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`test: assert bind_primary_attestation_hash is single-set and admin-gated`

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
title: "Add an admin entrypoint to clear the off-chain registry reference and emit a rebind event"
labels: type:feature, area:registry, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add an admin entrypoint to clear the off-chain registry reference and emit a rebind event

### Description
`DataKey::RegistryRef` holds an optional indexer/registry contract id (read via `get_registry_ref`), described in the code as a hint only, not authority. A prior issue covers rebinding it to a new address, but there is no way to **clear** it back to `None` — for example when an indexer is decommissioned and the stale reference should not mislead consumers into trusting a dead contract. The reference can be set or changed but never unset.

This issue adds an admin-gated entrypoint to clear the registry reference and emit a structured rebind/clear event.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `clear_registry_ref(env)` requiring admin auth that removes `DataKey::RegistryRef`, making `get_registry_ref` return `None`.
- Emit a structured event (e.g. `reg_clr`) with a unique symbol topic distinct from existing event symbols.
- Make it a no-op-safe call: return a clear typed error or document an idempotent no-op when no reference is set.
- Reiterate in docs that the registry reference is a hint, never authority, so clearing it changes no money-path behavior.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-clear-registry-ref`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the clear entrypoint and event.
  - **Write comprehensive tests in:** [`contracts/escrow/src/tests/admin.rs`](contracts/escrow/src/tests/admin.rs) — assert clearing sets `None`, emits the event, is admin-gated, and handles the already-cleared case.
  - **Add documentation:** note the clear entrypoint and the hint-only nature in [`README.md`](README.md) and `docs/`.
  - Include NatSpec-style doc comments (`///`) on the no-reference behavior.
  - Validate security assumptions: clearing the reference affects no funding, settlement, or payout logic.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: clear when set, clear when unset, unauthorized caller, and event correctness.
- Include the full `cargo test` output and a short **security notes** section in the PR description.

### Example commit message
`feat: add clear_registry_ref admin entrypoint with a rebind/clear event`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
