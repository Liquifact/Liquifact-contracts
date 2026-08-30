---
type: Feature
title: "Add a paginated read view enumerating active allowlisted addresses with their yield tier"
labels: type:feature, area:allowlist, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose active allowlist entries through a bounded, paginated view

### Description
The escrow contract in [`escrow/src/lib.rs`](escrow/src/lib.rs) records allowlisted investor addresses and their yield tier, but there is no read-only way to enumerate them — callers can only probe membership one address at a time. Off-chain UIs and reconciliation tooling need to page through the current allowlist. This issue adds a `get_allowlist_page(start, limit)` view returning `(address, tier)` pairs using the same start/limit pagination contract already used by the other read views.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a `get_allowlist_page` entrypoint to [`escrow/src/lib.rs`](escrow/src/lib.rs) returning a `Vec` of a small `AllowlistEntry` struct `{ investor: Address, tier: u32 }`.
- Reuse the existing start/limit bounds helper (the same one shared by the other paginated views); do not re-implement bounds checking.
- The view must be read-only (no storage writes) and must not panic on an empty allowlist — return an empty `Vec`.
- Keep the per-call result length bounded by the existing pagination limit ceiling so storage work stays capped.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/allowlist-01-paginated-view`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — the new view and the `AllowlistEntry` type.
  - **Write comprehensive tests in:** [`escrow/src/test/`](escrow/src/test/) — empty allowlist, single page, multi-page continuation, and limit-ceiling clamping.
  - Add rustdoc to the new entrypoint describing the pagination contract.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: empty set, exact-page boundary, over-limit request clamped.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat(allowlist): add paginated get_allowlist_page view`

### Guidelines
- **Minimum 95 percent test coverage** for the impacted module.
- Clear, reviewer-focused rustdoc.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add tests for the funding-deadline expiry path that makes an under-funded escrow cancellable"
labels: type:test, area:funding, stack:rust, stack:soroban, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover the funding-deadline expiry and cancellation lifecycle

### Description
The escrow supports a funding deadline after which an under-funded escrow can expire and become cancellable, but the transition is thinly tested. This issue adds focused tests around the deadline boundary and the resulting cancellable state in [`escrow/src/lib.rs`](escrow/src/lib.rs).

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests under [`escrow/src/test/`](escrow/src/test/) asserting: funding before the deadline succeeds; a fund attempt at/after the deadline is rejected with the correct typed `EscrowError`; an under-funded, expired escrow can be cancelled and investors made whole.
- Use the test ledger time helpers to advance to just-before and just-after the deadline; assert exact boundary behaviour (inclusive/exclusive) rather than approximate.
- Do not modify contract logic unless a test uncovers a real defect — if so, note it clearly in the PR.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/funding-01-deadline-expiry`
- Implement changes
  - **Write comprehensive tests in:** [`escrow/src/test/`](escrow/src/test/).
  - Add helper builders only if they reduce duplication across the new cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: exactly-at-deadline, one ledger past, refund correctness after cancel.
- Include the full `cargo test` output in the PR description.

### Example commit message
`test(funding): cover funding-deadline expiry and cancellation`

### Guidelines
- **Minimum 95 percent test coverage** for the impacted module.
- Clear, reviewer-focused test names.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Extract protocol-fee bps validation into a single checked helper reused by init and the lower-only setter"
labels: type:refactor, area:fees, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate protocol-fee bps bounds checking

### Description
Protocol-fee basis points are validated in more than one place (at `init` and in the lower-only fee setter), with the same bound repeated inline. This issue extracts a single `validate_fee_bps` helper in [`escrow/src/lib.rs`](escrow/src/lib.rs) so the ceiling lives in exactly one spot and both call sites share it.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a private `validate_fee_bps(bps: u32) -> Result<(), EscrowError>` (or a panic-to-typed-error equivalent matching the surrounding style) and call it from every place that currently bounds fee bps.
- Behaviour must be unchanged: the same inputs accepted/rejected with the same typed `EscrowError`.
- No new storage, no ABI change beyond the internal refactor.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/fees-01-validate-helper`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`escrow/src/test/`](escrow/src/test/) — boundary bps accepted, one-over rejected, from both entrypoints.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Confirm no behaviour change via the existing fee tests plus the new boundary cases.
- Include the full `cargo test` output in the PR description.

### Example commit message
`refactor(fees): extract shared validate_fee_bps helper`

### Guidelines
- **Minimum 95 percent test coverage** for the impacted module.
- Clear, reviewer-focused rustdoc.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Document the yield-tier selection algorithm and its rounding rules"
labels: type:docs, area:yield, stack:rust, stack:soroban, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document how a contribution maps to a yield tier

### Description
The contract selects a yield tier for each contribution from a configured tier table, but the selection algorithm and its rounding/boundary rules are not documented, making it hard for investors and reviewers to predict the tier a given amount receives. This issue adds a focused doc.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/yield-tiers.md` describing: the tier-table shape, how a contribution amount selects a tier, the boundary rule (which side a threshold belongs to), and any rounding applied to the resulting rate.
- Cross-reference the relevant entrypoints in [`escrow/src/lib.rs`](escrow/src/lib.rs) with a worked numeric example per tier boundary.
- Keep it accurate to the current code — read the selection logic before writing.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/yield-01-tier-selection`
- Implement changes
  - **Add documentation:** create `docs/yield-tiers.md`.
- Test and commit

### Test and commit
- Run `cargo fmt` and `cargo test` to confirm nothing else drifted.
- Verify each documented example against a quick unit assertion if practical.
- Note in the PR how you validated the boundary behaviour.

### Example commit message
`docs(yield): document tier selection and rounding`

### Guidelines
- Clear, reviewer-focused documentation with worked examples.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add an admin batch-bump-ttl entrypoint with a bounded key-set cap"
labels: type:feature, area:storage, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Let an admin extend TTL for a bounded set of storage keys in one call

### Description
Keeping escrow storage alive requires bumping TTL, and today that is done per-key. This issue adds an admin-only `batch_bump_ttl` entrypoint in [`escrow/src/lib.rs`](escrow/src/lib.rs) that accepts a bounded vector of keys and bumps each, capping the per-call length so storage work stays predictable (mirroring the existing bounded-vector guard used elsewhere).

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `batch_bump_ttl(admin, keys)` requiring admin auth; reject when `keys.len()` exceeds the configured per-call ceiling with a typed `EscrowError`.
- Reuse the existing bounded-vector length guard rather than introducing a new constant if one already exists.
- Emit no funds movement; this is storage maintenance only. Bump both instance and the relevant persistent entries as appropriate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/storage-01-batch-bump-ttl`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`escrow/src/test/`](escrow/src/test/) — auth required, over-cap rejected, TTL actually extended for each key.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: empty key set, at-cap accepted, over-cap rejected, non-admin rejected.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat(storage): add bounded admin batch_bump_ttl entrypoint`

### Guidelines
- **Minimum 95 percent test coverage** for the impacted module.
- Clear, reviewer-focused rustdoc.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
