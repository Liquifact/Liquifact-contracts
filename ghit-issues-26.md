---
type: Feature
title: "Add a read view exposing funding configuration"
labels: type:feature, area:funding, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose funding config

### Description
Callers can't read the current funding configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning the funding configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/funding-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(funding): add config read view`

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
title: "Add tests for funding event topics and payloads"
labels: type:test, area:funding, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test funding events

### Description
funding's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests capturing funding's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/funding-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(funding): cover event topics/payloads`

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
title: "Return a typed struct from funding instead of a tuple"
labels: type:refactor, area:funding, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type funding return

### Description
funding returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Replace funding's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/funding-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(funding): return a typed struct`

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
title: "Add an admin setter to update funding parameters within bounds"
labels: type:feature, area:funding, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure funding

### Description
funding parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add an admin-guarded setter for the funding parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/funding-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(funding): add admin parameter setter`

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
title: "Document funding error codes and their meanings"
labels: type:docs, area:funding, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document funding errors

### Description
funding's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/funding-errors.md` listing each funding EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/funding-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(funding): document error codes`

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
title: "Add a read view exposing settlement configuration"
labels: type:feature, area:settlement, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose settlement config

### Description
Callers can't read the current settlement configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning the settlement configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/settlement-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(settlement): add config read view`

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
title: "Add tests for settlement event topics and payloads"
labels: type:test, area:settlement, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test settlement events

### Description
settlement's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests capturing settlement's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/settlement-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(settlement): cover event topics/payloads`

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
title: "Return a typed struct from settlement instead of a tuple"
labels: type:refactor, area:settlement, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type settlement return

### Description
settlement returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Replace settlement's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/settlement-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(settlement): return a typed struct`

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
title: "Add an admin setter to update settlement parameters within bounds"
labels: type:feature, area:settlement, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure settlement

### Description
settlement parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add an admin-guarded setter for the settlement parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/settlement-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(settlement): add admin parameter setter`

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
title: "Document settlement error codes and their meanings"
labels: type:docs, area:settlement, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document settlement errors

### Description
settlement's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/settlement-errors.md` listing each settlement EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/settlement-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(settlement): document error codes`

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
title: "Add a read view exposing attestation configuration"
labels: type:feature, area:attestation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose attestation config

### Description
Callers can't read the current attestation configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning the attestation configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/attestation-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(attestation): add config read view`

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
title: "Add tests for attestation event topics and payloads"
labels: type:test, area:attestation, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test attestation events

### Description
attestation's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests capturing attestation's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/attestation-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(attestation): cover event topics/payloads`

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
title: "Return a typed struct from attestation instead of a tuple"
labels: type:refactor, area:attestation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type attestation return

### Description
attestation returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Replace attestation's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/attestation-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(attestation): return a typed struct`

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
title: "Add an admin setter to update attestation parameters within bounds"
labels: type:feature, area:attestation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure attestation

### Description
attestation parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add an admin-guarded setter for the attestation parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/attestation-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(attestation): add admin parameter setter`

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
title: "Document attestation error codes and their meanings"
labels: type:docs, area:attestation, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document attestation errors

### Description
attestation's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/attestation-errors.md` listing each attestation EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/attestation-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(attestation): document error codes`

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
title: "Add a read view exposing collateral configuration"
labels: type:feature, area:collateral, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose collateral config

### Description
Callers can't read the current collateral configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning the collateral configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/collateral-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(collateral): add config read view`

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
title: "Add tests for collateral event topics and payloads"
labels: type:test, area:collateral, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test collateral events

### Description
collateral's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests capturing collateral's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/collateral-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(collateral): cover event topics/payloads`

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
title: "Return a typed struct from collateral instead of a tuple"
labels: type:refactor, area:collateral, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type collateral return

### Description
collateral returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Replace collateral's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/collateral-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(collateral): return a typed struct`

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
title: "Add an admin setter to update collateral parameters within bounds"
labels: type:feature, area:collateral, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure collateral

### Description
collateral parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add an admin-guarded setter for the collateral parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/collateral-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(collateral): add admin parameter setter`

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
title: "Document collateral error codes and their meanings"
labels: type:docs, area:collateral, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document collateral errors

### Description
collateral's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/collateral-errors.md` listing each collateral EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/collateral-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(collateral): document error codes`

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
title: "Add a read view exposing yield-tier configuration"
labels: type:feature, area:yield-tier, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose yield-tier config

### Description
Callers can't read the current yield-tier configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning the yield-tier configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/yield-tier-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(yield-tier): add config read view`

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
title: "Add tests for yield-tier event topics and payloads"
labels: type:test, area:yield-tier, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test yield-tier events

### Description
yield-tier's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests capturing yield-tier's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/yield-tier-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(yield-tier): cover event topics/payloads`

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
title: "Return a typed struct from yield-tier instead of a tuple"
labels: type:refactor, area:yield-tier, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type yield-tier return

### Description
yield-tier returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Replace yield-tier's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/yield-tier-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(yield-tier): return a typed struct`

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
title: "Add an admin setter to update yield-tier parameters within bounds"
labels: type:feature, area:yield-tier, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure yield-tier

### Description
yield-tier parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add an admin-guarded setter for the yield-tier parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/yield-tier-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(yield-tier): add admin parameter setter`

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
title: "Document yield-tier error codes and their meanings"
labels: type:docs, area:yield-tier, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document yield-tier errors

### Description
yield-tier's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/yield-tier-errors.md` listing each yield-tier EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/yield-tier-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(yield-tier): document error codes`

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
title: "Add a read view exposing allowlist configuration"
labels: type:feature, area:allowlist, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose allowlist config

### Description
Callers can't read the current allowlist configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning the allowlist configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/allowlist-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(allowlist): add config read view`

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
title: "Add tests for allowlist event topics and payloads"
labels: type:test, area:allowlist, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test allowlist events

### Description
allowlist's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests capturing allowlist's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/allowlist-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(allowlist): cover event topics/payloads`

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
title: "Return a typed struct from allowlist instead of a tuple"
labels: type:refactor, area:allowlist, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type allowlist return

### Description
allowlist returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Replace allowlist's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/allowlist-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(allowlist): return a typed struct`

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
title: "Add an admin setter to update allowlist parameters within bounds"
labels: type:feature, area:allowlist, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure allowlist

### Description
allowlist parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add an admin-guarded setter for the allowlist parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/allowlist-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(allowlist): add admin parameter setter`

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
title: "Document allowlist error codes and their meanings"
labels: type:docs, area:allowlist, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document allowlist errors

### Description
allowlist's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/allowlist-errors.md` listing each allowlist EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/allowlist-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(allowlist): document error codes`

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
title: "Add a read view exposing fees configuration"
labels: type:feature, area:fees, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose fees config

### Description
Callers can't read the current fees configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a read-only view returning the fees configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/fees-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(fees): add config read view`

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
title: "Add tests for fees event topics and payloads"
labels: type:test, area:fees, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test fees events

### Description
fees's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests capturing fees's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/fees-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(fees): cover event topics/payloads`

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
title: "Return a typed struct from fees instead of a tuple"
labels: type:refactor, area:fees, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type fees return

### Description
fees returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Replace fees's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/fees-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(fees): return a typed struct`

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
title: "Add an admin setter to update fees parameters within bounds"
labels: type:feature, area:fees, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure fees

### Description
fees parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add an admin-guarded setter for the fees parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/fees-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(fees): add admin parameter setter`

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
title: "Document fees error codes and their meanings"
labels: type:docs, area:fees, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document fees errors

### Description
fees's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/fees-errors.md` listing each fees EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/fees-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(fees): document error codes`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
