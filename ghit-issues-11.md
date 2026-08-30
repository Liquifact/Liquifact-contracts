---
type: Feature
title: "Add a read view returning the current count of active unique investors"
labels: type:feature, area:funding, stack:rust, stack:soroban, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose the active investor count

### Description
Callers must currently page the whole investor set to learn how many unique investors an escrow has. This issue adds an O(1) `get_investor_count` read view backed by the existing unique-investor counter.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `get_investor_count() -> u32` returning the stored unique-investor count without iterating.
- Read-only; must not panic before any funding (return 0).
- Reuse the counter already maintained during fund; do not recompute.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/funding-02-investor-count`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`escrow/src/test/`](escrow/src/test/) — zero before funding, increments once per unique investor, stable across repeat deposits.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: no investors, repeat deposit by same investor, at the unique cap.
- Include the full test output in the PR description.

### Example commit message
`feat(funding): add get_investor_count view`

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
title: "Add boundary tests for the lower-only protocol-fee setter"
labels: type:test, area:fees, stack:rust, stack:soroban, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover the fee lower-only invariant

### Description
The lower-only fee setter must accept a decrease and reject any increase, but the boundary is thinly tested. This issue adds focused tests.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add tests asserting: setting a strictly lower bps succeeds; equal bps and any higher bps are rejected with the typed error; non-admin is rejected.
- Assert exact typed `EscrowError` codes.
- Do not change contract logic unless a defect is found (note it).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/fees-01-lower-only`
- Implement changes
  - **Write comprehensive tests in:** [`escrow/src/test/`](escrow/src/test/).
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: equal bps rejected, one-below accepted, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`test(fees): cover lower-only fee setter boundary`

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
title: "Extract the maturity-reached check into a shared helper"
labels: type:refactor, area:settlement, stack:rust, stack:soroban, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate maturity gating

### Description
Several entrypoints repeat the same 'has maturity been reached' ledger-time comparison inline. This issue extracts a `maturity_reached` helper reused by all of them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a private helper computing maturity-reached from ledger time and route every gate through it.
- Behaviour unchanged; same inclusive/exclusive boundary as today.
- No ABI change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/settlement-01-maturity-helper`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`escrow/src/test/`](escrow/src/test/) — just-before, exactly-at, just-after maturity.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: exact maturity ledger, off-by-one boundary.
- Include the full test output in the PR description.

### Example commit message
`refactor(settlement): extract maturity_reached helper`

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
title: "Document the legal-hold clear-delay bound and its rationale"
labels: type:docs, area:compliance, stack:rust, stack:soroban, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document the legal-hold timing bound

### Description
The legal-hold clear delay is bounded at init to prevent an unclearable hold, but the bound and its reasoning are undocumented. This issue documents them.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `docs/legal-hold.md` describing the clear-delay parameter, its accepted range, the failure it prevents, and how the hold interacts with settlement.
- Cross-reference the relevant entrypoints with a worked example.
- Keep it accurate — read the init validation first.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/compliance-01-legal-hold`
- Implement changes
  - **Add documentation:** create `docs/legal-hold.md`.
- Test and commit

### Test and commit
- Run `cargo fmt` and `cargo test`.
- Cover edge cases: n/a — verify the bound against the init code.
- Include the full test output in the PR description.

### Example commit message
`docs(compliance): document legal-hold clear-delay bound`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
