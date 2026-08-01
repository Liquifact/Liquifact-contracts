---
type: Feature
title: "Add a version/metadata view to funding"
labels: type:feature, area:funding, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Version funding

### Description
Callers can't query funding's deployed version/metadata. This issue adds a read-only view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning funding's version/metadata (e.g. schema version) without mutating storage.
- Return a sane default before init.
- Cover the value in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/funding-71-version`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: value after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(funding): add version view`

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
title: "Add boundary/fuzz-style tests for funding"
labels: type:test, area:funding, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Boundary-test funding

### Description
funding's numeric/length boundaries aren't exhaustively tested. This issue adds boundary cases.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests for funding at min, max, zero, and over-limit inputs asserting typed errors where expected.
- Keep runs bounded.
- Note any unguarded boundary found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/funding-71-boundary`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: min, max, zero, over-limit.
- Include the full test output in the PR description.

### Example commit message
`test(funding): add boundary tests`

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
title: "Extract funding validation into a helper"
labels: type:refactor, area:funding, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Helper for funding

### Description
funding repeats inline validation. This issue extracts a shared validation helper.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Extract funding's repeated validation into a helper returning a typed error; reuse it at each call site.
- Behaviour identical.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/funding-71-valhelper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same rejections, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(funding): extract validation helper`

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
title: "Add an upgrade-authorization check to funding"
labels: type:feature, area:funding, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Guard funding upgrade

### Description
funding's upgrade path lacks an explicit admin authorization check. This issue adds one.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Require admin authorization for funding's upgrade/migration entrypoint, rejecting others with the typed error.
- Emit an event on upgrade.
- Cover admin-allowed and non-admin-rejected in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/funding-72-upgradeauth`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: admin allowed, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(funding): guard upgrade authorization`

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
title: "Add a state-diagram note for funding"
labels: type:docs, area:funding, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Diagram funding states

### Description
funding's state machine isn't documented. This issue adds a state-diagram note.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/funding-states.md` with a diagram of funding's states and allowed transitions.
- Cross-reference the entrypoints enforcing them.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/funding-71-states`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(funding): add state diagram`

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
title: "Add a version/metadata view to settlement"
labels: type:feature, area:settlement, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Version settlement

### Description
Callers can't query settlement's deployed version/metadata. This issue adds a read-only view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning settlement's version/metadata (e.g. schema version) without mutating storage.
- Return a sane default before init.
- Cover the value in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/settlement-71-version`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: value after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(settlement): add version view`

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
title: "Add boundary/fuzz-style tests for settlement"
labels: type:test, area:settlement, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Boundary-test settlement

### Description
settlement's numeric/length boundaries aren't exhaustively tested. This issue adds boundary cases.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests for settlement at min, max, zero, and over-limit inputs asserting typed errors where expected.
- Keep runs bounded.
- Note any unguarded boundary found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/settlement-71-boundary`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: min, max, zero, over-limit.
- Include the full test output in the PR description.

### Example commit message
`test(settlement): add boundary tests`

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
title: "Extract settlement validation into a helper"
labels: type:refactor, area:settlement, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Helper for settlement

### Description
settlement repeats inline validation. This issue extracts a shared validation helper.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Extract settlement's repeated validation into a helper returning a typed error; reuse it at each call site.
- Behaviour identical.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/settlement-71-valhelper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same rejections, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(settlement): extract validation helper`

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
title: "Add an upgrade-authorization check to settlement"
labels: type:feature, area:settlement, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Guard settlement upgrade

### Description
settlement's upgrade path lacks an explicit admin authorization check. This issue adds one.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Require admin authorization for settlement's upgrade/migration entrypoint, rejecting others with the typed error.
- Emit an event on upgrade.
- Cover admin-allowed and non-admin-rejected in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/settlement-72-upgradeauth`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: admin allowed, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(settlement): guard upgrade authorization`

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
title: "Add a state-diagram note for settlement"
labels: type:docs, area:settlement, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Diagram settlement states

### Description
settlement's state machine isn't documented. This issue adds a state-diagram note.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/settlement-states.md` with a diagram of settlement's states and allowed transitions.
- Cross-reference the entrypoints enforcing them.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/settlement-71-states`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(settlement): add state diagram`

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
title: "Add a version/metadata view to attestation"
labels: type:feature, area:attestation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Version attestation

### Description
Callers can't query attestation's deployed version/metadata. This issue adds a read-only view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning attestation's version/metadata (e.g. schema version) without mutating storage.
- Return a sane default before init.
- Cover the value in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/attestation-71-version`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: value after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(attestation): add version view`

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
title: "Add boundary/fuzz-style tests for attestation"
labels: type:test, area:attestation, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Boundary-test attestation

### Description
attestation's numeric/length boundaries aren't exhaustively tested. This issue adds boundary cases.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests for attestation at min, max, zero, and over-limit inputs asserting typed errors where expected.
- Keep runs bounded.
- Note any unguarded boundary found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/attestation-71-boundary`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: min, max, zero, over-limit.
- Include the full test output in the PR description.

### Example commit message
`test(attestation): add boundary tests`

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
title: "Extract attestation validation into a helper"
labels: type:refactor, area:attestation, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Helper for attestation

### Description
attestation repeats inline validation. This issue extracts a shared validation helper.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Extract attestation's repeated validation into a helper returning a typed error; reuse it at each call site.
- Behaviour identical.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/attestation-71-valhelper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same rejections, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(attestation): extract validation helper`

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
title: "Add an upgrade-authorization check to attestation"
labels: type:feature, area:attestation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Guard attestation upgrade

### Description
attestation's upgrade path lacks an explicit admin authorization check. This issue adds one.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Require admin authorization for attestation's upgrade/migration entrypoint, rejecting others with the typed error.
- Emit an event on upgrade.
- Cover admin-allowed and non-admin-rejected in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/attestation-72-upgradeauth`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: admin allowed, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(attestation): guard upgrade authorization`

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
title: "Add a state-diagram note for attestation"
labels: type:docs, area:attestation, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Diagram attestation states

### Description
attestation's state machine isn't documented. This issue adds a state-diagram note.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/attestation-states.md` with a diagram of attestation's states and allowed transitions.
- Cross-reference the entrypoints enforcing them.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/attestation-71-states`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(attestation): add state diagram`

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
title: "Add a version/metadata view to collateral"
labels: type:feature, area:collateral, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Version collateral

### Description
Callers can't query collateral's deployed version/metadata. This issue adds a read-only view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning collateral's version/metadata (e.g. schema version) without mutating storage.
- Return a sane default before init.
- Cover the value in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/collateral-71-version`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: value after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(collateral): add version view`

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
title: "Add boundary/fuzz-style tests for collateral"
labels: type:test, area:collateral, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Boundary-test collateral

### Description
collateral's numeric/length boundaries aren't exhaustively tested. This issue adds boundary cases.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests for collateral at min, max, zero, and over-limit inputs asserting typed errors where expected.
- Keep runs bounded.
- Note any unguarded boundary found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/collateral-71-boundary`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: min, max, zero, over-limit.
- Include the full test output in the PR description.

### Example commit message
`test(collateral): add boundary tests`

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
title: "Extract collateral validation into a helper"
labels: type:refactor, area:collateral, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Helper for collateral

### Description
collateral repeats inline validation. This issue extracts a shared validation helper.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Extract collateral's repeated validation into a helper returning a typed error; reuse it at each call site.
- Behaviour identical.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/collateral-71-valhelper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same rejections, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(collateral): extract validation helper`

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
title: "Add an upgrade-authorization check to collateral"
labels: type:feature, area:collateral, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Guard collateral upgrade

### Description
collateral's upgrade path lacks an explicit admin authorization check. This issue adds one.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Require admin authorization for collateral's upgrade/migration entrypoint, rejecting others with the typed error.
- Emit an event on upgrade.
- Cover admin-allowed and non-admin-rejected in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/collateral-72-upgradeauth`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: admin allowed, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(collateral): guard upgrade authorization`

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
title: "Add a state-diagram note for collateral"
labels: type:docs, area:collateral, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Diagram collateral states

### Description
collateral's state machine isn't documented. This issue adds a state-diagram note.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/collateral-states.md` with a diagram of collateral's states and allowed transitions.
- Cross-reference the entrypoints enforcing them.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/collateral-71-states`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(collateral): add state diagram`

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
title: "Add a version/metadata view to yield-tier"
labels: type:feature, area:yield-tier, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Version yield-tier

### Description
Callers can't query yield-tier's deployed version/metadata. This issue adds a read-only view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning yield-tier's version/metadata (e.g. schema version) without mutating storage.
- Return a sane default before init.
- Cover the value in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/yield-tier-71-version`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: value after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(yield-tier): add version view`

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
title: "Add boundary/fuzz-style tests for yield-tier"
labels: type:test, area:yield-tier, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Boundary-test yield-tier

### Description
yield-tier's numeric/length boundaries aren't exhaustively tested. This issue adds boundary cases.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests for yield-tier at min, max, zero, and over-limit inputs asserting typed errors where expected.
- Keep runs bounded.
- Note any unguarded boundary found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/yield-tier-71-boundary`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: min, max, zero, over-limit.
- Include the full test output in the PR description.

### Example commit message
`test(yield-tier): add boundary tests`

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
title: "Extract yield-tier validation into a helper"
labels: type:refactor, area:yield-tier, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Helper for yield-tier

### Description
yield-tier repeats inline validation. This issue extracts a shared validation helper.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Extract yield-tier's repeated validation into a helper returning a typed error; reuse it at each call site.
- Behaviour identical.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/yield-tier-71-valhelper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same rejections, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(yield-tier): extract validation helper`

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
title: "Add an upgrade-authorization check to yield-tier"
labels: type:feature, area:yield-tier, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Guard yield-tier upgrade

### Description
yield-tier's upgrade path lacks an explicit admin authorization check. This issue adds one.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Require admin authorization for yield-tier's upgrade/migration entrypoint, rejecting others with the typed error.
- Emit an event on upgrade.
- Cover admin-allowed and non-admin-rejected in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/yield-tier-72-upgradeauth`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: admin allowed, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(yield-tier): guard upgrade authorization`

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
title: "Add a state-diagram note for yield-tier"
labels: type:docs, area:yield-tier, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Diagram yield-tier states

### Description
yield-tier's state machine isn't documented. This issue adds a state-diagram note.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/yield-tier-states.md` with a diagram of yield-tier's states and allowed transitions.
- Cross-reference the entrypoints enforcing them.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/yield-tier-71-states`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(yield-tier): add state diagram`

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
title: "Add a version/metadata view to allowlist"
labels: type:feature, area:allowlist, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Version allowlist

### Description
Callers can't query allowlist's deployed version/metadata. This issue adds a read-only view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning allowlist's version/metadata (e.g. schema version) without mutating storage.
- Return a sane default before init.
- Cover the value in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/allowlist-71-version`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: value after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(allowlist): add version view`

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
title: "Add boundary/fuzz-style tests for allowlist"
labels: type:test, area:allowlist, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Boundary-test allowlist

### Description
allowlist's numeric/length boundaries aren't exhaustively tested. This issue adds boundary cases.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests for allowlist at min, max, zero, and over-limit inputs asserting typed errors where expected.
- Keep runs bounded.
- Note any unguarded boundary found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/allowlist-71-boundary`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: min, max, zero, over-limit.
- Include the full test output in the PR description.

### Example commit message
`test(allowlist): add boundary tests`

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
title: "Extract allowlist validation into a helper"
labels: type:refactor, area:allowlist, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Helper for allowlist

### Description
allowlist repeats inline validation. This issue extracts a shared validation helper.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Extract allowlist's repeated validation into a helper returning a typed error; reuse it at each call site.
- Behaviour identical.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/allowlist-71-valhelper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same rejections, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(allowlist): extract validation helper`

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
title: "Add an upgrade-authorization check to allowlist"
labels: type:feature, area:allowlist, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Guard allowlist upgrade

### Description
allowlist's upgrade path lacks an explicit admin authorization check. This issue adds one.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Require admin authorization for allowlist's upgrade/migration entrypoint, rejecting others with the typed error.
- Emit an event on upgrade.
- Cover admin-allowed and non-admin-rejected in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/allowlist-72-upgradeauth`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: admin allowed, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(allowlist): guard upgrade authorization`

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
title: "Add a state-diagram note for allowlist"
labels: type:docs, area:allowlist, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Diagram allowlist states

### Description
allowlist's state machine isn't documented. This issue adds a state-diagram note.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/allowlist-states.md` with a diagram of allowlist's states and allowed transitions.
- Cross-reference the entrypoints enforcing them.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/allowlist-71-states`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(allowlist): add state diagram`

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
title: "Add a version/metadata view to fees"
labels: type:feature, area:fees, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Version fees

### Description
Callers can't query fees's deployed version/metadata. This issue adds a read-only view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning fees's version/metadata (e.g. schema version) without mutating storage.
- Return a sane default before init.
- Cover the value in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/fees-71-version`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: value after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(fees): add version view`

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
title: "Add boundary/fuzz-style tests for fees"
labels: type:test, area:fees, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Boundary-test fees

### Description
fees's numeric/length boundaries aren't exhaustively tested. This issue adds boundary cases.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests for fees at min, max, zero, and over-limit inputs asserting typed errors where expected.
- Keep runs bounded.
- Note any unguarded boundary found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/fees-71-boundary`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: min, max, zero, over-limit.
- Include the full test output in the PR description.

### Example commit message
`test(fees): add boundary tests`

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
title: "Extract fees validation into a helper"
labels: type:refactor, area:fees, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Helper for fees

### Description
fees repeats inline validation. This issue extracts a shared validation helper.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Extract fees's repeated validation into a helper returning a typed error; reuse it at each call site.
- Behaviour identical.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/fees-71-valhelper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same rejections, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(fees): extract validation helper`

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
title: "Add an upgrade-authorization check to fees"
labels: type:feature, area:fees, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Guard fees upgrade

### Description
fees's upgrade path lacks an explicit admin authorization check. This issue adds one.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Require admin authorization for fees's upgrade/migration entrypoint, rejecting others with the typed error.
- Emit an event on upgrade.
- Cover admin-allowed and non-admin-rejected in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/fees-72-upgradeauth`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: admin allowed, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(fees): guard upgrade authorization`

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
title: "Add a state-diagram note for fees"
labels: type:docs, area:fees, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Diagram fees states

### Description
fees's state machine isn't documented. This issue adds a state-diagram note.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/fees-states.md` with a diagram of fees's states and allowed transitions.
- Cross-reference the entrypoints enforcing them.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/fees-71-states`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(fees): add state diagram`

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
title: "Add a version/metadata view to pauser"
labels: type:feature, area:pauser, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Version pauser

### Description
Callers can't query pauser's deployed version/metadata. This issue adds a read-only view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning pauser's version/metadata (e.g. schema version) without mutating storage.
- Return a sane default before init.
- Cover the value in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/pauser-71-version`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: value after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(pauser): add version view`

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
title: "Add boundary/fuzz-style tests for pauser"
labels: type:test, area:pauser, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Boundary-test pauser

### Description
pauser's numeric/length boundaries aren't exhaustively tested. This issue adds boundary cases.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests for pauser at min, max, zero, and over-limit inputs asserting typed errors where expected.
- Keep runs bounded.
- Note any unguarded boundary found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/pauser-71-boundary`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: min, max, zero, over-limit.
- Include the full test output in the PR description.

### Example commit message
`test(pauser): add boundary tests`

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
title: "Extract pauser validation into a helper"
labels: type:refactor, area:pauser, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Helper for pauser

### Description
pauser repeats inline validation. This issue extracts a shared validation helper.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Extract pauser's repeated validation into a helper returning a typed error; reuse it at each call site.
- Behaviour identical.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/pauser-71-valhelper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same rejections, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(pauser): extract validation helper`

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
title: "Add an upgrade-authorization check to pauser"
labels: type:feature, area:pauser, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Guard pauser upgrade

### Description
pauser's upgrade path lacks an explicit admin authorization check. This issue adds one.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Require admin authorization for pauser's upgrade/migration entrypoint, rejecting others with the typed error.
- Emit an event on upgrade.
- Cover admin-allowed and non-admin-rejected in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/pauser-72-upgradeauth`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: admin allowed, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(pauser): guard upgrade authorization`

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
title: "Add a state-diagram note for pauser"
labels: type:docs, area:pauser, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Diagram pauser states

### Description
pauser's state machine isn't documented. This issue adds a state-diagram note.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/pauser-states.md` with a diagram of pauser's states and allowed transitions.
- Cross-reference the entrypoints enforcing them.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/pauser-71-states`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(pauser): add state diagram`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
