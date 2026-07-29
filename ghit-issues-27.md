---
type: Feature
title: "Add a batch variant of the funding entrypoint"
labels: type:feature, area:funding, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Batch funding

### Description
Callers must invoke funding once per item, wasting fees. This issue adds a bounded batch entrypoint.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a batch funding entrypoint processing a bounded vec atomically (all-or-nothing) with the same per-item checks.
- Reject over-limit batches with a typed error.
- Cover batch success, partial-invalid rejection, and over-limit.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/funding-61-batch`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: batch ok, one invalid rolls back, over-limit rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(funding): add batch entrypoint`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add authorization negative-path tests for funding"
labels: type:test, area:funding, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Auth-test funding

### Description
funding's authorization rejections aren't fully tested. This issue adds negative-path coverage.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests asserting funding rejects unauthorized callers with the typed error across each guarded entrypoint.
- Cover admin-only and owner-only paths.
- No behaviour change unless a gap is found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/funding-61-authneg`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: non-admin rejected, non-owner rejected.
- Include the full test output in the PR description.

### Example commit message
`test(funding): cover auth negative paths`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Extract funding storage keys into a keys module"
labels: type:refactor, area:funding, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Centralize funding keys

### Description
funding constructs storage keys inline, risking drift. This issue centralizes them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Move funding storage-key construction into a single keys module and reference it everywhere.
- Identical key layout; no migration needed.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/funding-61-keys`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same keys, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(funding): centralize storage keys`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Emit an event on funding state changes"
labels: type:feature, area:funding, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Event on funding

### Description
funding state changes are silent on-chain. This issue emits an event so indexers can react.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Emit a documented event whenever funding state changes, with the relevant fields.
- No duplicate emissions.
- Cover topic and payload in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/funding-62-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: event emitted once, payload fields correct.
- Include the full test output in the PR description.

### Example commit message
`feat(funding): emit state-change event`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add an invariants note for funding"
labels: type:docs, area:funding, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document funding invariants

### Description
funding's invariants (what must always hold) are undocumented. This issue records them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/funding-invariants.md` listing the funding invariants and where each is enforced.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/funding-61-invariants`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(funding): document invariants`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add a batch variant of the settlement entrypoint"
labels: type:feature, area:settlement, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Batch settlement

### Description
Callers must invoke settlement once per item, wasting fees. This issue adds a bounded batch entrypoint.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a batch settlement entrypoint processing a bounded vec atomically (all-or-nothing) with the same per-item checks.
- Reject over-limit batches with a typed error.
- Cover batch success, partial-invalid rejection, and over-limit.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/settlement-61-batch`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: batch ok, one invalid rolls back, over-limit rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(settlement): add batch entrypoint`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add authorization negative-path tests for settlement"
labels: type:test, area:settlement, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Auth-test settlement

### Description
settlement's authorization rejections aren't fully tested. This issue adds negative-path coverage.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests asserting settlement rejects unauthorized callers with the typed error across each guarded entrypoint.
- Cover admin-only and owner-only paths.
- No behaviour change unless a gap is found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/settlement-61-authneg`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: non-admin rejected, non-owner rejected.
- Include the full test output in the PR description.

### Example commit message
`test(settlement): cover auth negative paths`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Extract settlement storage keys into a keys module"
labels: type:refactor, area:settlement, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Centralize settlement keys

### Description
settlement constructs storage keys inline, risking drift. This issue centralizes them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Move settlement storage-key construction into a single keys module and reference it everywhere.
- Identical key layout; no migration needed.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/settlement-61-keys`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same keys, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(settlement): centralize storage keys`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Emit an event on settlement state changes"
labels: type:feature, area:settlement, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Event on settlement

### Description
settlement state changes are silent on-chain. This issue emits an event so indexers can react.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Emit a documented event whenever settlement state changes, with the relevant fields.
- No duplicate emissions.
- Cover topic and payload in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/settlement-62-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: event emitted once, payload fields correct.
- Include the full test output in the PR description.

### Example commit message
`feat(settlement): emit state-change event`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add an invariants note for settlement"
labels: type:docs, area:settlement, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document settlement invariants

### Description
settlement's invariants (what must always hold) are undocumented. This issue records them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/settlement-invariants.md` listing the settlement invariants and where each is enforced.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/settlement-61-invariants`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(settlement): document invariants`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add a batch variant of the attestation entrypoint"
labels: type:feature, area:attestation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Batch attestation

### Description
Callers must invoke attestation once per item, wasting fees. This issue adds a bounded batch entrypoint.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a batch attestation entrypoint processing a bounded vec atomically (all-or-nothing) with the same per-item checks.
- Reject over-limit batches with a typed error.
- Cover batch success, partial-invalid rejection, and over-limit.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/attestation-61-batch`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: batch ok, one invalid rolls back, over-limit rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(attestation): add batch entrypoint`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add authorization negative-path tests for attestation"
labels: type:test, area:attestation, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Auth-test attestation

### Description
attestation's authorization rejections aren't fully tested. This issue adds negative-path coverage.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests asserting attestation rejects unauthorized callers with the typed error across each guarded entrypoint.
- Cover admin-only and owner-only paths.
- No behaviour change unless a gap is found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/attestation-61-authneg`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: non-admin rejected, non-owner rejected.
- Include the full test output in the PR description.

### Example commit message
`test(attestation): cover auth negative paths`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Extract attestation storage keys into a keys module"
labels: type:refactor, area:attestation, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Centralize attestation keys

### Description
attestation constructs storage keys inline, risking drift. This issue centralizes them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Move attestation storage-key construction into a single keys module and reference it everywhere.
- Identical key layout; no migration needed.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/attestation-61-keys`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same keys, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(attestation): centralize storage keys`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Emit an event on attestation state changes"
labels: type:feature, area:attestation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Event on attestation

### Description
attestation state changes are silent on-chain. This issue emits an event so indexers can react.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Emit a documented event whenever attestation state changes, with the relevant fields.
- No duplicate emissions.
- Cover topic and payload in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/attestation-62-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: event emitted once, payload fields correct.
- Include the full test output in the PR description.

### Example commit message
`feat(attestation): emit state-change event`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add an invariants note for attestation"
labels: type:docs, area:attestation, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document attestation invariants

### Description
attestation's invariants (what must always hold) are undocumented. This issue records them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/attestation-invariants.md` listing the attestation invariants and where each is enforced.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/attestation-61-invariants`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(attestation): document invariants`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add a batch variant of the collateral entrypoint"
labels: type:feature, area:collateral, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Batch collateral

### Description
Callers must invoke collateral once per item, wasting fees. This issue adds a bounded batch entrypoint.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a batch collateral entrypoint processing a bounded vec atomically (all-or-nothing) with the same per-item checks.
- Reject over-limit batches with a typed error.
- Cover batch success, partial-invalid rejection, and over-limit.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/collateral-61-batch`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: batch ok, one invalid rolls back, over-limit rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(collateral): add batch entrypoint`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add authorization negative-path tests for collateral"
labels: type:test, area:collateral, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Auth-test collateral

### Description
collateral's authorization rejections aren't fully tested. This issue adds negative-path coverage.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests asserting collateral rejects unauthorized callers with the typed error across each guarded entrypoint.
- Cover admin-only and owner-only paths.
- No behaviour change unless a gap is found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/collateral-61-authneg`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: non-admin rejected, non-owner rejected.
- Include the full test output in the PR description.

### Example commit message
`test(collateral): cover auth negative paths`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Extract collateral storage keys into a keys module"
labels: type:refactor, area:collateral, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Centralize collateral keys

### Description
collateral constructs storage keys inline, risking drift. This issue centralizes them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Move collateral storage-key construction into a single keys module and reference it everywhere.
- Identical key layout; no migration needed.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/collateral-61-keys`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same keys, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(collateral): centralize storage keys`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Emit an event on collateral state changes"
labels: type:feature, area:collateral, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Event on collateral

### Description
collateral state changes are silent on-chain. This issue emits an event so indexers can react.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Emit a documented event whenever collateral state changes, with the relevant fields.
- No duplicate emissions.
- Cover topic and payload in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/collateral-62-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: event emitted once, payload fields correct.
- Include the full test output in the PR description.

### Example commit message
`feat(collateral): emit state-change event`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add an invariants note for collateral"
labels: type:docs, area:collateral, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document collateral invariants

### Description
collateral's invariants (what must always hold) are undocumented. This issue records them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/collateral-invariants.md` listing the collateral invariants and where each is enforced.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/collateral-61-invariants`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(collateral): document invariants`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add a batch variant of the yield-tier entrypoint"
labels: type:feature, area:yield-tier, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Batch yield-tier

### Description
Callers must invoke yield-tier once per item, wasting fees. This issue adds a bounded batch entrypoint.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a batch yield-tier entrypoint processing a bounded vec atomically (all-or-nothing) with the same per-item checks.
- Reject over-limit batches with a typed error.
- Cover batch success, partial-invalid rejection, and over-limit.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/yield-tier-61-batch`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: batch ok, one invalid rolls back, over-limit rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(yield-tier): add batch entrypoint`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add authorization negative-path tests for yield-tier"
labels: type:test, area:yield-tier, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Auth-test yield-tier

### Description
yield-tier's authorization rejections aren't fully tested. This issue adds negative-path coverage.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests asserting yield-tier rejects unauthorized callers with the typed error across each guarded entrypoint.
- Cover admin-only and owner-only paths.
- No behaviour change unless a gap is found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/yield-tier-61-authneg`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: non-admin rejected, non-owner rejected.
- Include the full test output in the PR description.

### Example commit message
`test(yield-tier): cover auth negative paths`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Extract yield-tier storage keys into a keys module"
labels: type:refactor, area:yield-tier, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Centralize yield-tier keys

### Description
yield-tier constructs storage keys inline, risking drift. This issue centralizes them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Move yield-tier storage-key construction into a single keys module and reference it everywhere.
- Identical key layout; no migration needed.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/yield-tier-61-keys`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same keys, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(yield-tier): centralize storage keys`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Emit an event on yield-tier state changes"
labels: type:feature, area:yield-tier, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Event on yield-tier

### Description
yield-tier state changes are silent on-chain. This issue emits an event so indexers can react.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Emit a documented event whenever yield-tier state changes, with the relevant fields.
- No duplicate emissions.
- Cover topic and payload in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/yield-tier-62-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: event emitted once, payload fields correct.
- Include the full test output in the PR description.

### Example commit message
`feat(yield-tier): emit state-change event`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add an invariants note for yield-tier"
labels: type:docs, area:yield-tier, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document yield-tier invariants

### Description
yield-tier's invariants (what must always hold) are undocumented. This issue records them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/yield-tier-invariants.md` listing the yield-tier invariants and where each is enforced.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/yield-tier-61-invariants`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(yield-tier): document invariants`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add a batch variant of the allowlist entrypoint"
labels: type:feature, area:allowlist, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Batch allowlist

### Description
Callers must invoke allowlist once per item, wasting fees. This issue adds a bounded batch entrypoint.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a batch allowlist entrypoint processing a bounded vec atomically (all-or-nothing) with the same per-item checks.
- Reject over-limit batches with a typed error.
- Cover batch success, partial-invalid rejection, and over-limit.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/allowlist-61-batch`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: batch ok, one invalid rolls back, over-limit rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(allowlist): add batch entrypoint`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add authorization negative-path tests for allowlist"
labels: type:test, area:allowlist, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Auth-test allowlist

### Description
allowlist's authorization rejections aren't fully tested. This issue adds negative-path coverage.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests asserting allowlist rejects unauthorized callers with the typed error across each guarded entrypoint.
- Cover admin-only and owner-only paths.
- No behaviour change unless a gap is found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/allowlist-61-authneg`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: non-admin rejected, non-owner rejected.
- Include the full test output in the PR description.

### Example commit message
`test(allowlist): cover auth negative paths`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Extract allowlist storage keys into a keys module"
labels: type:refactor, area:allowlist, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Centralize allowlist keys

### Description
allowlist constructs storage keys inline, risking drift. This issue centralizes them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Move allowlist storage-key construction into a single keys module and reference it everywhere.
- Identical key layout; no migration needed.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/allowlist-61-keys`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same keys, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(allowlist): centralize storage keys`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Emit an event on allowlist state changes"
labels: type:feature, area:allowlist, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Event on allowlist

### Description
allowlist state changes are silent on-chain. This issue emits an event so indexers can react.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Emit a documented event whenever allowlist state changes, with the relevant fields.
- No duplicate emissions.
- Cover topic and payload in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/allowlist-62-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: event emitted once, payload fields correct.
- Include the full test output in the PR description.

### Example commit message
`feat(allowlist): emit state-change event`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add an invariants note for allowlist"
labels: type:docs, area:allowlist, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document allowlist invariants

### Description
allowlist's invariants (what must always hold) are undocumented. This issue records them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/allowlist-invariants.md` listing the allowlist invariants and where each is enforced.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/allowlist-61-invariants`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(allowlist): document invariants`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
