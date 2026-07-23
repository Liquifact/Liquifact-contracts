---
type: Feature
title: "Move investor principal on-chain during fund() via SEP-41 token transfers"
labels: type:feature, area:funding, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement on-chain principal custody in fund() via SEP-41 token transfers

### Description
Today `fund_impl` in [`escrow/src/lib.rs`](escrow/src/lib.rs) records `DataKey::InvestorContribution` and increments `funded_amount`, but it **never moves tokens** — investor principal is only an accounting record while the bound `DataKey::FundingToken` sits unused for inflows. This means the contract's real token balance can diverge from `funded_amount`, and the `refund`/`sweep_terminal_dust` liability-floor math assumes funds are actually custodied on-chain.

This issue closes that gap: `fund` and `fund_with_commitment` must pull `amount` of the bound funding token from the investor into the contract atomically with the contribution write, so custody is real and balances reconcile.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- In `fund_impl`, after `investor.require_auth()` and all validation, transfer `amount` of `DataKey::FundingToken` from `investor` to `env.current_contract_address()` using a new inbound helper in [`escrow/src/external_calls.rs`](escrow/src/external_calls.rs) with strict pre/post balance-delta checks (mirror `transfer_funding_token_with_balance_checks`).
- Preserve canonical guard ordering (ADR-002): read-only checks, then `require_auth`, then storage writes and the token transfer last.
- Keep `funded_amount` and the contract's real token balance reconciled so `refund` and `sweep_terminal_dust`'s `funded_amount - distributed_principal` invariant remains sound.
- Add typed errors (append-only in `EscrowError`) for inbound transfer failures; do not reuse numeric codes.
- Bump persistent TTL for the investor's `DataKey::InvestorContribution` entry since it now backs a real balance.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-01-fund-onchain-custody`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — inbound transfer call inside `fund_impl`; new `EscrowError` variants.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — register a Stellar Asset Contract, mint to investors, assert balance deltas, insufficient-balance failure, and reconciliation with `funded_amount`.
  - **Add documentation:** update [`README.md`](README.md) and [`docs/escrow-lifecycle.md`](docs/escrow-lifecycle.md) to describe real inbound custody.
  - Include NatSpec-style `///` comments on the new helper and updated `fund` docs.
  - Validate security assumptions: no double-credit, auth on `investor`, balance conservation, no fund lock-up.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: zero/negative amount, under-funded investor wallet, fee-on-transfer rejection, paused (legal hold), allowlist gate, cap violations.
- Include full `cargo test` output and a short **security notes** section in the PR.

### Example commit message
`feat: custody investor principal on-chain in fund via SEP-41 transfers with tests and docs`

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
title: "Disburse funded liquidity to the SME on withdraw() via real token transfer"
labels: type:feature, area:settlement, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement on-chain SME disbursement in withdraw()

### Description
`withdraw()` in [`escrow/src/lib.rs`](escrow/src/lib.rs) flips `status` to 3 and emits `SmeWithdrew` with `funded_amount`, but it is purely an **accounting record** — no funding token ever reaches `escrow.sme_address`. If `fund` custodies principal on-chain, the SME has no trustless way to actually pull it.

This issue makes `withdraw` move `funded_amount` of the bound `DataKey::FundingToken` from the contract to `sme_address`, atomically with the status transition, so SME disbursement is final and auditable.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- After `escrow.sme_address.require_auth()` and the `status == 1` check, transfer `funded_amount` to `sme_address` via `external_calls::transfer_funding_token_with_balance_checks` in [`escrow/src/external_calls.rs`](escrow/src/external_calls.rs).
- Preserve the legal-hold gate and forward-only status transition; keep `SmeWithdrew` event but extend its payload with the recipient.
- Add a typed error path for insufficient contract balance (append-only `EscrowError` code).
- Coordinate with the dust-sweep liability floor so withdrawn principal is consistent with `distributed_principal` accounting.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-02-withdraw-onchain-disbursement`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — transfer call inside `withdraw`.
  - **Write comprehensive tests in:** [`escrow/src/tests/integration.rs`](escrow/src/tests/integration.rs) — SAC token, assert SME balance delta equals `funded_amount`, legal-hold block, wrong-status rejection.
  - **Add documentation:** update [`docs/ESCROW_SME_WITHDRAWAL.MD`](docs/ESCROW_SME_WITHDRAWAL.MD) and [`README.md`](README.md).
  - Include NatSpec-style `///` comments on the updated `withdraw`.
  - Validate security: auth, balance conservation, no double-withdraw.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: legal hold active, non-funded state, repeated withdraw, insufficient balance.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`feat: disburse funded liquidity to SME on withdraw via token transfer with tests`

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
title: "Settle investor payouts on-chain so claim_investor_payout() transfers tokens"
labels: type:feature, area:settlement, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement on-chain investor payouts in claim_investor_payout()

### Description
`claim_investor_payout()` in [`escrow/src/lib.rs`](escrow/src/lib.rs) only writes the `DataKey::InvestorClaimed` marker and emits `InvestorPayoutClaimed` — it never transfers the pro-rata payout that `compute_investor_payout()` already computes. Investors record a claim but receive nothing on-chain.

This issue wires `compute_investor_payout` into `claim_investor_payout` so a settled escrow pays the investor their gross payout in the bound funding token, atomically with marking the claim.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- After the settled-status and `not_before` gates, compute the payout via `Self::compute_investor_payout`, then transfer it from the contract to `investor` using [`escrow/src/external_calls.rs`](escrow/src/external_calls.rs).
- Keep the idempotency invariant: mark `InvestorClaimed` before transfer, early-return on repeat claims, no double-pay.
- Add typed errors (append-only) for a zero-computed payout or insufficient contract balance.
- Document the relationship between the dust-sweep liability floor, distributed payouts, and rounding residue.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-03-claim-onchain-payout`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — transfer inside `claim_investor_payout`.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — multi-investor pro-rata payouts, rounding residue, double-claim no-op, legal-hold block.
  - **Add documentation:** update [`docs/escrow-pro-rata.md`](docs/escrow-pro-rata.md) and [`README.md`](README.md).
  - Include NatSpec-style `///` comments on the updated entrypoint.
  - Validate security: checks-effects-interactions ordering, no double-spend, balance conservation.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: zero contribution, unsettled escrow, locked claim, repeated claim, insufficient balance.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`feat: pay investors on-chain in claim_investor_payout using pro-rata math with tests`

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
title: "Add a contract WASM upgrade entrypoint guarded by admin authorization"
labels: type:feature, area:upgradeability, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement an admin-gated upgrade() entrypoint for contract WASM replacement

### Description
The contract in [`escrow/src/lib.rs`](escrow/src/lib.rs) exposes `migrate()` for schema-version bookkeeping but has **no way to replace the deployed WASM**. The README upgrade policy says "redeploy required" for layout changes, which strands long-lived escrows whose `admin`, holds, and balances cannot be moved to a fixed binary.

This issue adds an `upgrade(new_wasm_hash: BytesN<32>)` entrypoint that calls `env.deployer().update_current_contract_wasm(...)` under current-admin authorization, enabling in-place code fixes while preserving stored state.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `upgrade(env, new_wasm_hash: BytesN<32>)`; require `Self::get_escrow(env).admin.require_auth()` before the deployer call (matches the `migrate` auth-first ordering).
- Emit a new `ContractUpgraded` `#[contractevent]` carrying `invoice_id` and the new hash for indexers.
- Document interaction with `SCHEMA_VERSION` / `DataKey::Version` and the additive-key policy (ADR-007).
- Do not change existing stored layout; the entrypoint only swaps code.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-04-wasm-upgrade-entrypoint`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `upgrade` entrypoint and `ContractUpgraded` event.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — register a second WASM, upgrade, assert state survives and unauthorized callers are rejected.
  - **Add documentation:** update [`docs/OPERATOR_RUNBOOK.md`](docs/OPERATOR_RUNBOOK.md) and the README upgrade policy.
  - Include NatSpec-style `///` comments documenting admin gating and risks.
  - Validate security: only current admin can upgrade; no state wipe.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: unauthorized caller, upgrade then read preserved `DataKey::Escrow`.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`feat: add admin-gated upgrade entrypoint for contract wasm replacement with tests`

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
title: "Batch fund recording to onboard multiple investor contributions in one call"
labels: type:feature, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement a bounded batch funding entrypoint

### Description
`fund` in [`escrow/src/lib.rs`](escrow/src/lib.rs) records exactly one investor per call, while `set_investors_allowlisted` already demonstrates a bounded-batch pattern (capped at `MAX_INVESTOR_ALLOWLIST_BATCH`). High-volume primary issuance has no equivalent for funding, forcing one transaction per investor.

This issue adds `fund_batch(entries: Vec<(Address, i128)>)` that applies the same per-entry validation, caps, and accounting as `fund`, with a bounded batch size and per-entry authorization.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a `MAX_FUND_BATCH` constant mirroring `MAX_INVESTOR_ALLOWLIST_BATCH`; reject empty and oversized batches with typed errors (append-only).
- Each entry must satisfy all existing `fund_impl` invariants (per-investor cap, unique-investor cap, min-contribution floor, allowlist, status); require auth per investor address.
- Emit one `EscrowFunded` event per entry, identical to single `fund` semantics.
- Ensure the funded-target snapshot transition fires correctly mid-batch.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-05-fund-batch`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `fund_batch` and `MAX_FUND_BATCH`.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — batch equals N single funds, cap rejection, mid-batch funded transition.
  - **Add documentation:** update [`README.md`](README.md) entrypoint table and [`docs/escrow-lifecycle.md`](docs/escrow-lifecycle.md).
  - Include NatSpec-style `///` comments.
  - Validate security: bounded CPU/storage, per-investor auth, no partial-state corruption.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: empty batch, oversized batch, duplicate addresses, cap boundary.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`feat: add bounded fund_batch entrypoint for multi-investor funding with tests`

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
title: "Add a maturity-based settlement readiness view and settlement timestamp event"
labels: type:enhancement, area:settlement, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add maturity-window readiness to settle() and surface it on-chain

### Description
`settle()` in [`escrow/src/lib.rs`](escrow/src/lib.rs) enforces `now >= maturity` but emits only `EscrowSettled` with no signal about *when* an escrow became settleable, and there is no read-only entrypoint to ask "is this escrow settleable now?". Indexers and SME tooling must re-derive the maturity comparison off-chain.

This issue adds a `is_settleable(env) -> bool` view and a `settled_at_ledger_timestamp` field on the settlement path so the maturity window is observable on-chain.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `is_settleable(env) -> bool` returning `status == 1 && (maturity == 0 || now >= maturity) && !legal_hold`.
- Extend `EscrowSettled` (append-only field) with the ledger timestamp at settlement; keep existing topics stable.
- Reuse `Env::ledger().timestamp()` semantics from [`docs/escrow-ledger-time.md`](docs/escrow-ledger-time.md).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-06-settlement-readiness`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `is_settleable` view, extended event.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — readiness across status/maturity/hold combinations, event field assertion.
  - **Add documentation:** update [`docs/EVENT_SCHEMA.md`](docs/EVENT_SCHEMA.md) and [`docs/escrow-events.md`](docs/escrow-events.md).
  - Include NatSpec-style `///` comments.
  - Validate security: pure read, no auth, no state change.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: no-maturity escrow, pre-maturity, post-maturity, hold active.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`feat: add is_settleable view and settlement timestamp event field with tests`

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
title: "Allow investors to revoke a contribution while the escrow is still open"
labels: type:feature, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement investor-initiated contribution withdrawal in the open state

### Description
Once an investor calls `fund` in [`escrow/src/lib.rs`](escrow/src/lib.rs), there is no way to back out before the escrow is funded — `refund` only works in the cancelled state (status 4). An investor who funds and then changes their mind while the invoice is still `open` (status 0) is stuck.

This issue adds `unfund(investor, amount)` allowing partial or full contribution withdrawal while `status == 0`, decrementing `funded_amount`, `InvestorContribution`, and (when zeroed) `UniqueFunderCount`.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Allow only while `status == 0`; require `investor.require_auth()`; reject when a legal hold is active.
- Decrement `InvestorContribution`, `funded_amount`, and decrement `UniqueFunderCount` if contribution reaches zero; never underflow (use `checked_sub`).
- If on-chain custody is enabled, return tokens via [`escrow/src/external_calls.rs`](escrow/src/external_calls.rs); otherwise update accounting only and document the dependency.
- Add typed errors (append-only) for over-withdrawal and wrong status.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-07-investor-unfund`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `unfund` entrypoint and `EscrowUnfunded` event.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — partial/full unfund, funder-count decrement, status guard.
  - **Add documentation:** update [`docs/escrow-lifecycle.md`](docs/escrow-lifecycle.md).
  - Include NatSpec-style `///` comments.
  - Validate security: no underflow, no unfund after funded, auth.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: over-withdrawal, full exit resets funder count, hold active, funded state rejection.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`feat: add investor unfund entrypoint for open-state contribution withdrawal with tests`

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
title: "Add a paginated read API for enumerating investor positions"
labels: type:feature, area:read-api, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement an on-chain investor registry with paginated reads

### Description
Per-investor data in [`escrow/src/lib.rs`](escrow/src/lib.rs) lives under address-keyed persistent entries (`DataKey::InvestorContribution(Address)`), which are not enumerable — `get_contribution` requires the caller to already know each address. Indexers and dashboards cannot list all funders from the contract, and `UniqueFunderCount` gives only a count.

This issue records funder addresses in a bounded append-only `DataKey::InvestorIndex` vector at first deposit and adds a paginated `get_investors(start, limit)` read.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- On first deposit (`prev == 0`) in `fund_impl`, append the investor to a bounded `DataKey::InvestorIndex`, consistent with `MaxUniqueInvestorsCap` and `UniqueFunderCount`.
- Add `get_investors(env, start: u32, limit: u32) -> Vec<Address>` with a bounded `limit`; pure read, no auth.
- Document the additive-key compatibility (ADR-007): legacy instances return an empty index.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-08-investor-index`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `DataKey::InvestorIndex`, append in `fund_impl`, `get_investors` view.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — index population, pagination bounds, no duplicate on repeat fund.
  - **Add documentation:** update [`docs/escrow-read-api.md`](docs/escrow-read-api.md) and [`docs/escrow-data-model.md`](docs/escrow-data-model.md).
  - Include NatSpec-style `///` comments.
  - Validate security: bounded growth, no duplicate entries.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: empty index, single funder, pagination past end, repeated deposits.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`feat: add bounded investor index with paginated get_investors read api and tests`

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
title: "Emit a structured lifecycle event when the escrow first reaches the funded state"
labels: type:enhancement, area:events, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a dedicated FundingClosed event at the open to funded transition

### Description
In `fund_impl` ([`escrow/src/lib.rs`](escrow/src/lib.rs)) the escrow silently flips `status` from 0 to 1 and writes `DataKey::FundingCloseSnapshot`, but the only signal is the generic `EscrowFunded` event with `status: 1` — indexers must diff sequential events to detect the *moment of close*. The pro-rata denominator is captured here yet never announced as a distinct event.

This issue emits a dedicated `FundingClosed` event carrying the snapshot fields exactly once at the transition.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Define a `FundingClosed` `#[contractevent]` with `invoice_id`, `total_principal`, `funding_target`, `closed_at_ledger_timestamp`, `closed_at_ledger_sequence`.
- Emit it only inside the `status == 0 && funded_amount >= funding_target` branch, alongside the snapshot write, and also in `partial_settle` where the snapshot is written.
- Keep `EscrowFunded` unchanged; this is additive.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-09-funding-closed-event`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `FundingClosed` event and emission points.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — event emitted exactly once, fields match snapshot, also on `partial_settle`.
  - **Add documentation:** update [`docs/EVENT_SCHEMA.md`](docs/EVENT_SCHEMA.md) and [`docs/escrow-events.md`](docs/escrow-events.md).
  - Include NatSpec-style `///` comments.
  - Validate security: single emission, no over-funding double-fire.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: exact-target close, over-funding close, partial-settle close.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`feat: emit dedicated FundingClosed event at open-to-funded transition with tests`

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
title: "Support a configurable protocol fee deducted at SME withdrawal"
labels: type:feature, area:settlement, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement an immutable protocol fee on SME disbursement

### Description
The escrow in [`escrow/src/lib.rs`](escrow/src/lib.rs) routes all funded principal to the SME at withdrawal with no protocol revenue mechanism, even though a `Treasury` address is already bound at `init`. There is no way for LiquiFact to capture a basis-point fee on disbursed liquidity.

This issue adds an immutable `protocol_fee_bps` configured at `init` that, on withdrawal, splits `funded_amount` into an SME payout and a treasury fee.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add an optional `protocol_fee_bps: Option<i64>` parameter to `init`, validated to `0..=10_000`, stored under a new `DataKey::ProtocolFeeBps` (default 0).
- In `withdraw`, compute `fee = funded_amount * fee_bps / 10_000` (floor, checked) and route it to `DataKey::Treasury` with the remainder to `sme_address`.
- Extend `SmeWithdrew` (append-only) with the fee amount; add typed overflow errors.
- This depends on on-chain disbursement; document the interaction.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-10-protocol-fee`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — fee storage, split logic in `withdraw`.
  - **Write comprehensive tests in:** [`escrow/src/tests/integration.rs`](escrow/src/tests/integration.rs) — fee math, zero-fee default, rounding, treasury delta.
  - **Add documentation:** update [`docs/escrow-numeric-model.md`](docs/escrow-numeric-model.md) and [`README.md`](README.md).
  - Include NatSpec-style `///` comments.
  - Validate security: overflow safety, fee bounds, conservation `sme + fee == funded_amount`.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: 0 bps, max bps, rounding residue, large `funded_amount`.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`feat: add immutable protocol fee split on SME withdrawal with tests and docs`

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
title: "Add a batch investor payout claim entrypoint for settled escrows"
labels: type:feature, area:settlement, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement a bounded batch claim path for investor payouts

### Description
`claim_investor_payout` in [`escrow/src/lib.rs`](escrow/src/lib.rs) processes one investor per transaction. For a settled escrow with many funders, distributing payouts requires one transaction per address, which is operationally heavy for a relayer or keeper.

This issue adds a bounded `claim_payouts_batch(investors: Vec<Address>)` that applies identical per-investor gates and idempotency, mirroring the `set_investors_allowlisted` batch pattern.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a `MAX_CLAIM_BATCH` constant; reject empty/oversized batches with typed errors.
- Each entry must pass the settled-status gate, `not_before` lock, idempotency check, and (if payouts are on-chain) the transfer; require auth per investor.
- Skip already-claimed entries without failing the whole batch; emit one `InvestorPayoutClaimed` per newly-claimed investor.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-11-claim-batch`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `claim_payouts_batch` and `MAX_CLAIM_BATCH`.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — batch equals N single claims, skip-claimed, cap rejection.
  - **Add documentation:** update [`README.md`](README.md) entrypoint table.
  - Include NatSpec-style `///` comments.
  - Validate security: per-investor auth, idempotency, bounded work.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: empty batch, oversized, mixed claimed/unclaimed, locked claims.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`feat: add bounded batch investor payout claim entrypoint with tests`

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
title: "Add a global pause switch independent of the compliance legal hold"
labels: type:feature, area:admin, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement an operational pause distinct from legal hold

### Description
The only circuit breaker in [`escrow/src/lib.rs`](escrow/src/lib.rs) is `DataKey::LegalHold`, which carries compliance semantics and a two-phase clear delay. There is no lightweight *operational* pause for incident response (e.g. a suspected token bug) that an admin can toggle without the legal-hold ceremony.

This issue adds an admin-controlled `DataKey::Paused` flag that gates `fund`, `settle`, `withdraw`, and `claim_investor_payout`, orthogonal to legal hold.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `set_paused(active: bool)` (admin auth) and `is_paused()` view; add a `PausedChanged` event.
- Add the pause gate to risk-bearing entrypoints as a read-only precondition before `require_auth`, consistent with ADR-002 ordering.
- Add typed errors (append-only) for each paused entrypoint; keep legal-hold logic untouched.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-12-operational-pause`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — pause flag, gate, events, errors.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — pause blocks each gated entrypoint, unpause restores, independence from legal hold.
  - **Add documentation:** update [`docs/escrow-security-checklist.md`](docs/escrow-security-checklist.md) and [`README.md`](README.md).
  - Include NatSpec-style `///` comments.
  - Validate security: admin-only toggle, gate ordering before auth.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: paused + legal hold both active, unauthorized toggle, each gated entrypoint.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`feat: add admin operational pause switch independent of legal hold with tests`

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
title: "Convert panic-string guards in partial_settle to typed EscrowError codes"
labels: type:security, area:errors, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Harden partial_settle() with stable typed errors

### Description
`partial_settle` in [`escrow/src/lib.rs`](escrow/src/lib.rs) uses raw `assert!` with panic strings ("Legal hold blocks partial settlement", "Unauthorized caller for partial settlement", "Escrow must be in Open state for partial settlement") instead of the project's append-only `EscrowError` enum. This breaks the documented client-SDK contract that callers "branch on the numeric code rather than legacy panic strings".

This issue replaces those asserts with typed errors so `partial_settle` matches every other entrypoint.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add append-only `EscrowError` variants (e.g. `LegalHoldBlocksPartialSettle`, `UnauthorizedPartialSettle`, `PartialSettleNotOpen`); never renumber existing codes.
- Replace each `assert!` in `partial_settle` with `ensure(&env, cond, EscrowError::...)`.
- Preserve guard ordering and the `EscrowPartialSettle` event; no behavior change beyond error type.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-13-partial-settle-typed-errors`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — new error variants and `ensure` calls.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — assert each typed error via `try_partial_settle`.
  - **Add documentation:** update [`docs/escrow-error-messages.md`](docs/escrow-error-messages.md).
  - Include NatSpec-style `///` comments on the new error variants.
  - Validate security: identical revert conditions, stable numeric codes.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: hold active, wrong caller, non-open status.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`fix: replace partial_settle panic strings with typed EscrowError codes and tests`

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
title: "Replace panic-string asserts in admin and allowlist batch paths with typed errors"
labels: type:security, area:errors, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Harden accept_admin, lower_max_unique_investors, and batch allowlist with typed errors

### Description
Several admin paths in [`escrow/src/lib.rs`](escrow/src/lib.rs) still panic with raw strings: `accept_admin` panics "No pending admin", `lower_max_unique_investors` uses four `assert!`/`panic!` messages, and `set_investors_allowlisted` asserts on batch bounds. This is inconsistent with the contract's append-only `EscrowError` discipline and forces SDKs to parse strings.

This issue migrates all of these to typed errors while preserving exact semantics.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add append-only `EscrowError` variants (e.g. `NoPendingAdmin`; reuse `CapLowerNotOpen`/`NewCapNotLower`/`NewCapBelowCurrentFunderCount` where already defined, otherwise add allowlist `BatchEmpty`/`BatchTooLarge` analogues) and never renumber.
- Replace asserts in `accept_admin`, `lower_max_unique_investors`, and `set_investors_allowlisted` with `ensure(...)`.
- Keep return values, events, and ordering identical.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-14-admin-typed-errors`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — error variants and `ensure` calls.
  - **Write comprehensive tests in:** [`escrow/src/test_allowlist_tests.rs`](escrow/src/test_allowlist_tests.rs) and [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — assert typed errors for each path.
  - **Add documentation:** update [`docs/escrow-error-messages.md`](docs/escrow-error-messages.md).
  - Include NatSpec-style `///` comments on the new error variants.
  - Validate security: stable codes, identical revert conditions.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: accept with no pending admin, cap not open, cap not lower, batch empty/too-large.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`fix: convert admin and allowlist batch panics to typed EscrowError codes with tests`

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
title: "Bump persistent TTL for per-investor keys inside fund and claim flows"
labels: type:security, area:storage-ttl, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Harden per-investor persistent entries against TTL expiry

### Description
`fund_impl` and `claim_investor_payout` in [`escrow/src/lib.rs`](escrow/src/lib.rs) write per-investor persistent keys (`InvestorContribution`, `InvestorEffectiveYield`, `InvestorClaimNotBefore`, `InvestorClaimed`) but rely entirely on the permissionless `bump_ttl` entrypoint to keep them alive. For a long-dated escrow, an investor's contribution entry can be archived before settlement, defaulting reads to zero and silently erasing their position.

This issue extends persistent TTL at write time inside the funding and claim paths using the existing `PERSISTENT_TTL_MIN_EXTENSION_LEDGERS` horizon.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- In the per-investor setters in `fund_impl` (and the claim marker write), call `env.storage().persistent().extend_ttl(...)` for the keys just written.
- Use the documented `PERSISTENT_TTL_MIN_EXTENSION_LEDGERS` constant; do not shorten any TTL (extend is monotonic).
- Keep `bump_ttl` as the permissionless top-up; this adds defense-in-depth at write time.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-15-per-investor-ttl-bump`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `extend_ttl` calls in the persistent setters / fund path.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — assert entries persist past the prior horizon using `Ledger` testutils.
  - **Add documentation:** update [`docs/escrow-gas-storage-notes.md`](docs/escrow-gas-storage-notes.md) and [ADR-007](docs/adr/ADR-007-storage-key-evolution.md).
  - Include NatSpec-style `///` comments.
  - Validate security: no premature archival of live positions.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: repeat deposits re-bump, claim marker bump, long-dated maturity.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`fix: extend per-investor persistent TTL at write time in fund and claim flows with tests`

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
title: "Enforce maturity bounds at init so settlement cannot be locked forever"
labels: type:security, area:init-validation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Harden init() and update_maturity() against unreachable maturity timestamps

### Description
`init` and `update_maturity` in [`escrow/src/lib.rs`](escrow/src/lib.rs) accept any `u64` maturity with no upper-bound or relative-to-now validation. A mistaken value (e.g. far beyond any plausible ledger time) silently locks `settle()` and the claim path forever, since both gate on `now >= maturity` with no escape other than legal-hold gymnastics.

This issue adds bounded validation: maturity must be either 0 (no lock) or within a sane window relative to `Env::ledger().timestamp()`.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a `MAX_MATURITY_HORIZON_SECS` constant and reject `maturity > now + horizon` with a new typed error (append-only).
- Apply the same validation in `init` and `update_maturity`; preserve the `maturity == 0` "no lock" semantics.
- Reference the ledger-time trust model in [`docs/escrow-ledger-time.md`](docs/escrow-ledger-time.md).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-16-maturity-bounds`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — constant, validation, error.
  - **Write comprehensive tests in:** [`escrow/src/tests/init.rs`](escrow/src/tests/init.rs) — accept zero/in-window, reject far-future.
  - **Add documentation:** update [`docs/escrow-ledger-time.md`](docs/escrow-ledger-time.md).
  - Include NatSpec-style `///` comments.
  - Validate security: no permanent settlement lock from bad input.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: maturity 0, exactly at horizon, beyond horizon, update path.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`fix: bound maturity timestamps at init and update_maturity to prevent settlement lock with tests`

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
title: "Add an authorization audit test matrix for every role-gated entrypoint"
labels: type:security, area:auth, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test the authorization boundary of every state-mutating entrypoint

### Description
ADR-002 in [`docs/adr/ADR-002-auth-boundaries.md`](docs/adr/ADR-002-auth-boundaries.md) defines per-role auth (admin, SME, investor, treasury), and each entrypoint in [`escrow/src/lib.rs`](escrow/src/lib.rs) calls `require_auth()` for one bound role. There is no single test matrix that asserts each entrypoint rejects the *wrong* signer and that no entrypoint silently mutates state before `require_auth`.

This issue adds an exhaustive negative-authorization test matrix.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- For each mutating entrypoint (`init`, `fund`, `settle`, `withdraw`, `claim_investor_payout`, `set_legal_hold`, `sweep_terminal_dust`, `propose_admin`, `accept_admin`, `cancel_funding`, `refund`, allowlist setters, `update_*`, attestation writes), assert the call fails without the correct signer using `env.mock_auths` toggling.
- Assert the read-only preconditions and ordering from the module rustdoc "Authorization guard ordering".
- No production code change unless a missing guard is discovered (then file/fix separately).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-17-auth-matrix`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a guard gap is found.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — the full negative-auth matrix.
  - **Add documentation:** cross-link results in [`docs/escrow-security-checklist.md`](docs/escrow-security-checklist.md).
  - Include NatSpec-style `///` comments on test helpers.
  - Validate security: every role boundary is covered.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: wrong signer per entrypoint, no signer, treasury vs admin on sweep.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`test: add exhaustive negative-authorization matrix for all role-gated entrypoints`

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
title: "Repair and re-enable the disabled settlement test module"
labels: type:test, area:settlement, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test settlement by fixing and re-enabling tests/settlement.rs

### Description
The settlement test module is **disabled** in [`escrow/src/tests.rs`](escrow/src/tests.rs): `// mod settlement;` is commented out with a note that [`escrow/src/tests/settlement.rs`](escrow/src/tests/settlement.rs) "has interleaved fragments left behind by overlapping PR merges (#290..#301) that produced six unbalanced brace points." The most critical state transitions (`settle`, `withdraw`, claims, maturity boundaries, dust sweep) therefore have no compiled coverage.

This issue repairs the broken fragments and re-enables the module.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Fix the unbalanced braces and interleaved fragments in [`escrow/src/tests/settlement.rs`](escrow/src/tests/settlement.rs) so it compiles cleanly.
- Re-enable `mod settlement;` in [`escrow/src/tests.rs`](escrow/src/tests.rs).
- Cover `settle` (status/maturity/hold), `withdraw`, `claim_investor_payout` (lock + idempotency), and `sweep_terminal_dust` terminal-state and liability-floor cases.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-18-reenable-settlement-tests`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a real bug surfaces while fixing tests.
  - **Write comprehensive tests in:** [`escrow/src/tests/settlement.rs`](escrow/src/tests/settlement.rs) — repaired and expanded suite.
  - **Add documentation:** update the test-organization table in [`README.md`](README.md).
  - Include NatSpec-style `///` comments on shared helpers.
  - Validate security: settlement invariants are actually exercised.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: pre/at/post maturity, hold-blocked settle, double withdraw, dust floor.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`test: repair and re-enable settlement test module with expanded coverage`

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
title: "Add property-based invariants for funding accounting and unique-funder count"
labels: type:test, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test funding invariants with proptest in properties.rs

### Description
[`escrow/src/tests/properties.rs`](escrow/src/tests/properties.rs) already hosts proptest-based invariants, but the core funding accounting in `fund_impl` ([`escrow/src/lib.rs`](escrow/src/lib.rs)) — `funded_amount == sum of contributions`, `UniqueFunderCount == distinct funders`, and cap monotonicity — lacks randomized coverage.

This issue adds property tests over random funding sequences to assert these invariants hold for all orderings.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Generate randomized sequences of `fund`/`fund_with_commitment` across multiple investors and amounts.
- Assert: sum of `get_contribution` equals `funded_amount`; `get_unique_funder_count` equals distinct funders; per-investor and unique caps are never exceeded; status flips to 1 exactly when `funded_amount >= funding_target`.
- Persist any discovered counterexamples to [`escrow/proptest-regressions/test.txt`](escrow/proptest-regressions/test.txt).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-19-funding-properties`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if an invariant violation is found.
  - **Write comprehensive tests in:** [`escrow/src/tests/properties.rs`](escrow/src/tests/properties.rs) — new proptest cases.
  - **Add documentation:** note invariants in [`docs/escrow-numeric-model.md`](docs/escrow-numeric-model.md).
  - Include NatSpec-style `///` comments on generators.
  - Validate security: accounting conservation under all orderings.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: cap boundaries, overflow inputs, repeated funders.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`test: add proptest invariants for funding accounting and unique funder count`

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
title: "Add proptest coverage for compute_investor_payout pro-rata rounding"
labels: type:test, area:settlement, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test pro-rata payout conservation and rounding invariants

### Description
`compute_investor_payout` in [`escrow/src/lib.rs`](escrow/src/lib.rs) implements floor integer division for coupon and pro-rata share, documenting the invariant that the sum over all investors is `<= total_principal + coupon` with residue swept as dust. That conservation invariant is not exercised by randomized tests.

This issue adds property tests asserting the documented rounding and conservation bounds.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Generate random investor sets, contributions, and yield bps; fund to close; assert `sum(compute_investor_payout) <= total_principal + coupon` and that residue is non-negative.
- Assert non-participants return 0 and overflow inputs raise `ComputePayoutArithmeticOverflow`.
- Reference the formula in [`docs/escrow-pro-rata.md`](docs/escrow-pro-rata.md).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-20-payout-properties`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a rounding bug is found.
  - **Write comprehensive tests in:** [`escrow/src/tests/properties.rs`](escrow/src/tests/properties.rs) — payout conservation properties.
  - **Add documentation:** clarify rounding edge cases in [`docs/escrow-pro-rata.md`](docs/escrow-pro-rata.md).
  - Include NatSpec-style `///` comments.
  - Validate security: no over-distribution beyond pool.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: single investor, equal splits, prime denominators, zero yield, max yield.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`test: add proptest conservation and rounding invariants for compute_investor_payout`

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
title: "Add end-to-end lifecycle tests for the cancel and refund path"
labels: type:test, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test the cancel_funding to refund lifecycle and liability floor

### Description
`cancel_funding` and `refund` in [`escrow/src/lib.rs`](escrow/src/lib.rs) implement the cancelled-escrow recovery path, with `refund` tracking `DataKey::DistributedPrincipal` that `sweep_terminal_dust` uses to enforce the liability floor. This multi-step interaction needs dedicated end-to-end coverage to prove the floor holds after partial refunds.

This issue adds lifecycle tests covering fund, cancel, refund, sweep with the liability invariant.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Set up a SAC token, fund multiple investors, cancel, refund a subset, then attempt `sweep_terminal_dust` and assert it respects `balance - sweep >= funded_amount - distributed_principal`.
- Assert double-refund fails with `NoContributionToRefund`, `is_investor_refunded` flips, and `DistributedPrincipal` accumulates.
- Reference [`docs/adr/ADR-006-dust-sweep-and-token-safety.md`](docs/adr/ADR-006-dust-sweep-and-token-safety.md).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-21-cancel-refund-lifecycle`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a bug surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/integration.rs`](escrow/src/tests/integration.rs) — full cancel/refund/sweep lifecycle.
  - **Add documentation:** cross-link scenarios in [`docs/escrow-lifecycle.md`](docs/escrow-lifecycle.md).
  - Include NatSpec-style `///` comments.
  - Validate security: liability floor never violated after partial refunds.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: partial refunds, full refunds, sweep attempt before/after refunds, legal hold.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`test: add cancel_funding to refund lifecycle tests with liability floor assertions`

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
title: "Add legal-hold two-phase clear timing and recovery tests"
labels: type:test, area:legal-hold, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test the legal-hold request/clear delay and admin-recovery flow

### Description
The legal-hold clear flow in [`escrow/src/lib.rs`](escrow/src/lib.rs) spans `request_clear_legal_hold`, the `LegalHoldClearDelay`, and `set_legal_hold(false)` with typed errors `LegalHoldClearRequestMissing` and `LegalHoldClearNotReady`. The documented recovery lever (`propose_admin`, `accept_admin`, `clear_legal_hold`) is a critical funds-safety path that needs explicit timing coverage in [`escrow/src/tests/legal_hold.rs`](escrow/src/tests/legal_hold.rs).

This issue adds tests for the delay window and the admin-handover recovery scenario.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert clearing without a prior request fails; clearing before `clearable_at` fails; clearing at/after succeeds.
- Assert the recovery path: hold active, propose+accept new admin, new admin clears the hold.
- Assert holds block `settle`, `withdraw`, and `claim_investor_payout` while active.
- Reference [`docs/escrow-legal-hold.md`](docs/escrow-legal-hold.md) and [ADR-004](docs/adr/ADR-004-legal-hold.md).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-22-legal-hold-timing`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only if a bug surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/legal_hold.rs`](escrow/src/tests/legal_hold.rs) — timing and recovery cases.
  - **Add documentation:** clarify timing examples in [`docs/escrow-legal-hold.md`](docs/escrow-legal-hold.md).
  - Include NatSpec-style `///` comments.
  - Validate security: clear delay cannot be bypassed; recovery requires admin authority.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: zero delay, nonzero delay, missing request, admin handover mid-hold.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`test: add legal-hold two-phase clear timing and admin recovery tests`

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
title: "Add SEP-41 token-safety tests for the external_calls balance-delta wrapper"
labels: type:test, area:token-safety, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Test the balance-delta invariants in external_calls.rs against non-compliant tokens

### Description
`transfer_funding_token_with_balance_checks` in [`escrow/src/external_calls.rs`](escrow/src/external_calls.rs) enforces strict pre/post balance conservation to safe-fail on fee-on-transfer, rebasing, and hook tokens. The existing mocked tests in [`escrow/src/tests/external_calls_mocked.rs`](escrow/src/tests/external_calls_mocked.rs) should be extended to prove each delta-mismatch error path triggers.

This issue adds adversarial-token tests for every typed error the wrapper can emit.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Build mock tokens that under-deliver (fee-on-transfer), over-credit (rebasing), and leave balances unchanged; assert `SenderBalanceDeltaMismatch` / `RecipientBalanceDeltaMismatch`.
- Assert `TransferAmountNotPositive` and `InsufficientTokenBalanceBeforeTransfer` paths.
- Keep production code unchanged unless a real gap is found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-23-token-safety-wrapper`
- Implement changes
  - **Write code in:** [`escrow/src/external_calls.rs`](escrow/src/external_calls.rs) — only if a gap surfaces.
  - **Write comprehensive tests in:** [`escrow/src/tests/external_calls_mocked.rs`](escrow/src/tests/external_calls_mocked.rs) — adversarial token scenarios.
  - **Add documentation:** update [`docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md`](docs/ESCROW_TOKEN_INTEGRATION_CHECKLIST.md).
  - Include NatSpec-style `///` comments on the mock tokens.
  - Validate security: every non-compliant token path safe-fails.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: fee-on-transfer, rebasing, no-op transfer, zero amount, insufficient balance.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`test: add adversarial SEP-41 token-safety tests for external_calls balance-delta wrapper`

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
title: "Document the complete escrow state machine including the cancelled branch"
labels: type:docs, area:state-machine, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the full status transition diagram and guards

### Description
`InvoiceEscrow::status` in [`escrow/src/lib.rs`](escrow/src/lib.rs) spans 0=open, 1=funded, 2=settled, 3=withdrawn, 4=cancelled, but the README entrypoint table and [ADR-001](docs/adr/ADR-001-state-model.md) describe statuses 0–3 and omit the cancelled (4) branch added with `cancel_funding`/`refund`. The state machine doc is out of date relative to the code.

This issue updates the state-machine documentation to cover all five states, every transition, and each guard.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Document each transition (entrypoint, required role, legal-hold gate, status precondition) for `fund`, `partial_settle`, `settle`, `withdraw`, `cancel_funding`, `refund`.
- Update [`docs/STATE_MACHINE_IMPLEMENTATION.md`](docs/STATE_MACHINE_IMPLEMENTATION.md) and reconcile [ADR-001](docs/adr/ADR-001-state-model.md) to include status 4.
- Add a Mermaid diagram; ensure the README entrypoint table is consistent.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-24-state-machine`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only doc-comment corrections if the inline `status` comment is stale.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — a test asserting illegal transitions are rejected, anchoring the docs.
  - **Add documentation:** [`docs/STATE_MACHINE_IMPLEMENTATION.md`](docs/STATE_MACHINE_IMPLEMENTATION.md), [ADR-001](docs/adr/ADR-001-state-model.md), [`README.md`](README.md).
  - Include NatSpec-style `///` comments where the inline status doc is updated.
  - Validate security: documented guards match code.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: each illegal transition rejected (e.g. settle from open, refund from funded).
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`docs: document full escrow state machine including cancelled branch with anchoring test`

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
title: "Reconcile the events catalog with all emitted contractevents"
labels: type:docs, area:events, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document every contractevent and its topics for indexers

### Description
[`escrow/src/lib.rs`](escrow/src/lib.rs) defines many `#[contractevent]` types (`EscrowInitialized`, `EscrowFunded`, `EscrowPartialSettle`, `EscrowSettled`, `SmeWithdrew`, `InvestorPayoutClaimed`, `FundingCancelled`, `InvestorRefundedEvt`, `TreasuryDustSwept`, attestation and allowlist events, etc.), but [`docs/EVENT_SCHEMA.md`](docs/EVENT_SCHEMA.md) and [`docs/escrow-events.md`](docs/escrow-events.md) may not list every event, its `#[topic]` fields, and the `symbol_short!` name emitted.

This issue produces a complete, code-accurate events catalog.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Enumerate every `#[contractevent]` in `lib.rs`: event struct, emitted `name` symbol, topic fields, and payload fields.
- Note which entrypoint emits each event and under what status transition.
- Keep [`docs/escrow-indexer.md`](docs/escrow-indexer.md) consistent with the catalog.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-25-events-catalog`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only doc-comment fixes if an event's rustdoc is wrong.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — assert emitted event names/topics match the documented catalog.
  - **Add documentation:** [`docs/EVENT_SCHEMA.md`](docs/EVENT_SCHEMA.md), [`docs/escrow-events.md`](docs/escrow-events.md), [`docs/escrow-indexer.md`](docs/escrow-indexer.md).
  - Include NatSpec-style `///` comments where event rustdoc is corrected.
  - Validate security: documented topics match emitted topics.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: events with `Option` fields, multi-topic events.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`docs: reconcile events catalog with all emitted contractevents and add assertion test`

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
title: "Write a complete error-code reference mapping every EscrowError variant"
labels: type:docs, area:errors, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the full EscrowError code table for client SDKs

### Description
The `EscrowError` enum in [`escrow/src/lib.rs`](escrow/src/lib.rs) carries roughly 60 append-only numeric codes across grouped ranges (init, terminal, sweep, attestation, collateral, funding, settlement, refund, legal-hold). The contract explicitly tells SDKs to "branch on the numeric code", yet [`docs/escrow-error-messages.md`](docs/escrow-error-messages.md) may not document every code with its trigger and recommended client handling.

This issue produces a complete, code-accurate error reference.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Tabulate every variant: numeric code, name, emitting entrypoint(s), trigger condition, and recommended client action.
- Document the range-grouping convention and the append-only / no-renumber policy.
- Cross-link from [`README.md`](README.md) security notes.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-26-error-reference`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only rustdoc additions on undocumented variants.
  - **Write comprehensive tests in:** [`escrow/src/tests/coverage.rs`](escrow/src/tests/coverage.rs) — a test asserting representative codes match documented numbers.
  - **Add documentation:** [`docs/escrow-error-messages.md`](docs/escrow-error-messages.md), [`README.md`](README.md).
  - Include NatSpec-style `///` comments on any newly-documented variant.
  - Validate security: documented numbers match the `#[repr(u32)]` discriminants.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: boundary codes per range group.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`docs: add complete EscrowError code reference with anchoring test`

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
title: "Document the tiered-yield and commitment-lock model with worked examples"
labels: type:docs, area:tiered-yield, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document tier selection, first-deposit discipline, and claim locks

### Description
The tiered-yield logic in [`escrow/src/lib.rs`](escrow/src/lib.rs) (`validate_yield_tiers_table`, `effective_yield_for_commitment`, `fund_with_commitment`) enforces non-decreasing tiers, first-deposit-only tier selection, and a `not_before` claim lock derived from `committed_lock_secs`. The interaction between tier matching, the `TieredSecondDeposit` rule, and claim-time gating deserves a worked, example-driven explanation beyond [ADR-005](docs/adr/ADR-005-tiered-yield.md).

This issue expands the tiered-yield documentation with concrete numeric examples.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Walk through tier-table validation rules and rejection cases (`TierYieldBelowBase`, `TierLockNotIncreasing`, `TierYieldNotNonDecreasing`).
- Show first deposit via `fund_with_commitment` selecting an effective yield, and why follow-on principal must use `fund`.
- Explain `InvestorClaimNotBefore` derivation and its enforcement in `claim_investor_payout`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-27-tiered-yield`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — only rustdoc clarifications.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — tests matching each documented example.
  - **Add documentation:** [ADR-005](docs/adr/ADR-005-tiered-yield.md), [`docs/escrow-numeric-model.md`](docs/escrow-numeric-model.md).
  - Include NatSpec-style `///` comments where clarified.
  - Validate security: documented behavior matches enforced rules.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: no tiers, single tier, max-lock tier, tiered second-deposit rejection.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`docs: expand tiered-yield and commitment-lock model with worked examples and tests`

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
title: "Extract the funded-close snapshot write into a single shared helper"
labels: type:refactor, area:funding, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Refactor duplicated FundingCloseSnapshot write logic

### Description
The `FundingCloseSnapshot` write block is duplicated in [`escrow/src/lib.rs`](escrow/src/lib.rs): once in `fund_impl` (open→funded transition) and again in `partial_settle`. Both construct the same struct with `total_principal`, `funding_target`, ledger timestamp and sequence, gated by `!has(&DataKey::FundingCloseSnapshot)`. Divergence between the two copies is a latent correctness risk for the pro-rata denominator.

This issue extracts a single private helper used by both call sites.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a private `write_funding_close_snapshot_if_absent(&env, &escrow)` helper and call it from both `fund_impl` and `partial_settle`.
- Preserve write-once immutability and identical field values; no behavior change.
- Keep the snapshot's documented immutability invariant intact.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-28-snapshot-helper`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — extract helper, replace both call sites.
  - **Write comprehensive tests in:** [`escrow/src/tests/funding.rs`](escrow/src/tests/funding.rs) — assert identical snapshot from both paths, write-once on over-funding.
  - **Add documentation:** note the shared helper in [`docs/escrow-snapshot.md`](docs/escrow-snapshot.md).
  - Include NatSpec-style `///` comments on the helper.
  - Validate security: snapshot remains write-once and immutable.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: fund close, partial-settle close, no double-write on over-funding.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`refactor: extract shared funding-close snapshot helper used by fund and partial_settle`

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
title: "Consolidate repeated escrow-read and admin-auth boilerplate into helpers"
labels: type:refactor, area:admin, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Refactor the get_escrow plus admin.require_auth pattern into a guard helper

### Description
Nearly every admin entrypoint in [`escrow/src/lib.rs`](escrow/src/lib.rs) (`set_legal_hold`, `set_allowlist_active`, `set_investor_allowlisted`, `update_funding_target`, `update_maturity`, `propose_admin`, `bind_primary_attestation_hash`, `append_attestation_digest`, `cancel_funding`, `migrate`) opens with the same `let escrow = Self::get_escrow(env.clone()); escrow.admin.require_auth();` boilerplate. The repetition is error-prone — a future entrypoint could forget the auth call.

This issue introduces a small helper that reads the escrow and enforces admin authorization in one place.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a private `require_admin(&env) -> InvoiceEscrow` helper returning the loaded escrow after `admin.require_auth()`.
- Replace the duplicated pattern at each admin call site without changing behavior or guard ordering (ADR-002).
- Keep SME/investor/treasury auth sites unchanged (different roles).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-29-admin-guard-helper`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — `require_admin` helper and call-site replacement.
  - **Write comprehensive tests in:** [`escrow/src/tests/admin.rs`](escrow/src/tests/admin.rs) — assert every refactored entrypoint still rejects non-admin callers.
  - **Add documentation:** note the helper in [ADR-002](docs/adr/ADR-002-auth-boundaries.md).
  - Include NatSpec-style `///` comments on the helper.
  - Validate security: identical auth boundary at every site.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: non-admin rejection per refactored entrypoint, post-handover admin.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`refactor: consolidate admin auth boilerplate into a require_admin helper with tests`

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
title: "Centralize repeated funding-token and treasury storage reads"
labels: type:refactor, area:storage, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Refactor duplicated FundingToken and Treasury reads into typed accessors

### Description
The `DataKey::FundingToken` read with `unwrap_or_else(|| fail(&env, EscrowError::FundingTokenNotSet))` is repeated across `sweep_terminal_dust`, `refund`, and the getter in [`escrow/src/lib.rs`](escrow/src/lib.rs), and the `Treasury` read is similarly duplicated. As more entrypoints move funds on-chain, this pattern will spread, risking inconsistent error handling.

This issue centralizes these reads behind private accessors that already raise the correct typed error.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add private `funding_token_or_fail(&env) -> Address` and `treasury_or_fail(&env) -> Address` helpers.
- Replace the inline `unwrap_or_else(... fail ...)` reads in `sweep_terminal_dust`, `refund`, and getters with the helpers; preserve identical error codes.
- No behavior change; this is a readability/maintainability refactor.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-30-token-treasury-accessors`
- Implement changes
  - **Write code in:** [`escrow/src/lib.rs`](escrow/src/lib.rs) — accessors and call-site replacement.
  - **Write comprehensive tests in:** [`escrow/src/tests/init.rs`](escrow/src/tests/init.rs) — assert `FundingTokenNotSet`/`TreasuryNotSet` still raised pre-init via the accessors.
  - **Add documentation:** note the accessors in [`docs/escrow-data-model.md`](docs/escrow-data-model.md).
  - Include NatSpec-style `///` comments on the accessors.
  - Validate security: identical typed errors and immutability semantics.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: pre-init reads, post-init reads, sweep and refund paths.
- Include full `cargo test` output and a **security notes** section in the PR.

### Example commit message
`refactor: centralize funding-token and treasury storage reads behind typed accessors with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord for questions, reviews, and faster merges:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.