---
type: Feature
title: "Add a lower-only admin entrypoint to reduce the protocol fee bps after init"
labels: type:feature, area:protocol-fee, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add a lower-only admin entrypoint to reduce the protocol fee bps after init

### Description
`protocol_fee_bps` is written once in `init()` under `DataKey::ProtocolFeeBps` and is read by `withdraw()` to size the treasury cut, but no entrypoint can ever change it. Add `lower_protocol_fee_bps(new_bps)` so an admin can only ever reduce the fee, mirroring the forward-only guard style already used by `lower_min_contribution_floor` and `lower_max_unique_investors`.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Reject any `new_bps` greater than or equal to the value returned by `get_protocol_fee_bps()` with a typed `EscrowError`, and reject values outside `0..=10_000`.
- Gate the call on admin `require_auth()` and emit a structured `#[contractevent]` carrying the old and new bps.
- Allow the reduction only before the escrow reaches `withdrawn`, so an already-disbursed escrow cannot be retroactively re-priced.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-lower-protocol-fee`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(escrow): add lower-only protocol fee bps update entrypoint`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add a protocol-fee preview view reporting the treasury cut and SME net before withdraw"
labels: type:feature, area:read-api, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add a protocol-fee preview view reporting the treasury cut and SME net before withdraw

### Description
`withdraw()` computes `fee = funded_amount * protocol_fee_bps / 10_000` inline and splits the disbursement between the treasury and the SME, but integrators cannot see that split until the transaction lands. Add a `preview_protocol_fee()` view returning both legs using the same checked arithmetic path.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Return a struct (or tuple) with the computed fee and the SME net amount, derived from `funded_amount` and `DataKey::ProtocolFeeBps`.
- Reuse the exact rounding used in `withdraw()` so the preview never disagrees with the executed split by even one stroop.
- Keep the view read-only and callable in any status, returning zeros when `funded_amount` is zero.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-protocol-fee-preview`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(escrow): add preview_protocol_fee read view`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add a batch claimable-payout view returning many investors' pending amounts in one call"
labels: type:feature, area:read-api, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add a batch claimable-payout view returning many investors' pending amounts in one call

### Description
`get_claimable_payout(investor)` answers for a single address, forcing indexers to issue one simulation per investor across a large cap table. Add `get_claimable_payouts(investors: Vec<Address>) -> Vec<i128>` that mirrors the batching already provided by `get_contributions`.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Preserve per-entry ordering so index `i` of the output corresponds to index `i` of the input.
- Bound the input length with a `MAX_*_BATCH` constant and a typed error, matching the batch guards used by `set_investors_allowlisted`.
- Return `0` for unknown investors and for investors already flagged by `is_investor_claimed`, exactly as the single-address view does.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-batch-claimable-payouts`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(escrow): add get_claimable_payouts batch read view`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Extend EscrowSummary with the pause flag and the configured protocol fee bps"
labels: type:enhancement, area:read-api, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Extend EscrowSummary with the pause flag and the configured protocol fee bps

### Description
`get_escrow_summary()` aggregates the fields an indexer needs in one round-trip, but it omits the global pause switch read by `is_paused()` and the fee read by `get_protocol_fee_bps()`. Clients must therefore make two extra calls to render accurate escrow state.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add `paused: bool` and `protocol_fee_bps: i64` to the `EscrowSummary` struct as additive fields.
- Populate them from the same storage keys the standalone views use so the summary can never drift.
- Update `docs/escrow-read-api.md` and add a test asserting the summary fields track `set_paused` and the init-time fee.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-summary-pause-fee`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(escrow): extend EscrowSummary with pause and protocol fee fields`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Emit a dedicated protocol-fee event from withdraw recording the treasury cut"
labels: type:enhancement, area:events, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Emit a dedicated protocol-fee event from withdraw recording the treasury cut

### Description
`withdraw()` transfers the protocol fee to `DataKey::Treasury` and the remainder to the SME, but the fee leg is not separately observable on-chain. Accounting integrations have to re-derive it from `funded_amount` and the fee bps rather than reading a topic.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a `#[contractevent]` struct carrying the fee amount, the effective bps, the treasury address, and the SME net amount.
- Emit it from `withdraw()` only when the computed fee is non-zero, keeping the zero-fee path byte-identical to today.
- Use a `symbol_short!` topic that collides with no existing emitter and register it in `docs/EVENT_SCHEMA.md`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-protocol-fee-event`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(escrow): emit protocol fee event from withdraw`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Bound the bump_ttl allowlisted vector length to cap per-call storage work"
labels: type:security, area:storage-ttl, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Bound the bump_ttl allowlisted vector length to cap per-call storage work

### Description
`bump_ttl(env, allowlisted: Vec<Address>)` is permissionless and iterates the caller-supplied vector to extend per-investor persistent keys, but the vector length is unbounded. A caller can pass an arbitrarily large list and drive the instruction and storage-access budget to the ledger limit on every invocation.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Introduce a `MAX_BUMP_TTL_BATCH` constant and reject longer inputs with a typed `EscrowError`, matching `MAX_INVESTOR_ALLOWLIST_BATCH` and `MAX_ATTESTATION_REVOKE_BATCH`.
- Keep the entrypoint permissionless — the cap is the mitigation, not an auth gate.
- Add tests at the cap boundary, one entry over the cap, and with an empty vector.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-cap-bump-ttl-batch`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`fix(escrow): bound bump_ttl batch length with a typed error`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Disambiguate the att_rev event symbol shared by single and batch attestation revocation"
labels: type:security, area:events, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Disambiguate the att_rev event symbol shared by single and batch attestation revocation

### Description
`revoke_attestation_digest` and `revoke_attestation_digests` both emit events under `symbol_short!("att_rev")`. Indexers subscribing to that topic cannot distinguish a single targeted revocation from an entry inside an operator batch, which weakens the compliance audit trail these digests exist to provide.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Give the batch emitter its own topic (for example `att_revb`) or add a discriminating field to the event payload.
- Verify no other emitter reuses the chosen symbol before landing the change.
- Add a test asserting the two entrypoints publish distinguishable topics, and update `docs/escrow-attestations.md`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-att-rev-symbol`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`fix(escrow): disambiguate single and batch attestation revocation event topics`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Collapse the duplicated coll_clr event emitted twice by clear_sme_collateral_commitment"
labels: type:refactor, area:events, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Collapse the duplicated coll_clr event emitted twice by clear_sme_collateral_commitment

### Description
`clear_sme_collateral_commitment()` publishes two separate events that both use `symbol_short!("coll_clr")`. Downstream consumers see the same logical clear twice per call and cannot tell which payload is authoritative.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Determine which of the two emissions is the intended record and remove or rename the redundant one.
- Preserve every field currently observable to indexers so the change is additive from a consumer's point of view.
- Add a test asserting exactly one `coll_clr` event is published per successful clear.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-coll-clr-duplicate`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`refactor(escrow): remove duplicate coll_clr event emission`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Extract the repeated start and limit pagination bounds logic into a shared helper"
labels: type:refactor, area:read-api, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Extract the repeated start and limit pagination bounds logic into a shared helper

### Description
`get_investors`, `get_allowlisted_investors`, and `get_revoked_attestation_digests` each re-implement the same `start`/`limit` window clamping against a backing length. The duplicated arithmetic is the kind of code where one path silently drifts into an off-by-one or an underflow.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a private helper returning the resolved `(start, end)` window for a given collection length, with saturating arithmetic.
- Rewrite all three paginated views to call it, leaving their public signatures and return types unchanged.
- Add unit tests for `start` past the end, zero `limit`, and a `limit` exceeding the remaining items.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-pagination-helper`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`refactor(escrow): centralize paginated view window clamping`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add tests for the global pause switch gating fund, settle, withdraw, and claim"
labels: type:test, area:pause, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add tests for the global pause switch gating fund, settle, withdraw, and claim

### Description
`set_paused()` writes the pause flag surfaced by `is_paused()`, but there is no test matrix proving the flag actually blocks the value-moving entrypoints. Without it a refactor could drop a pause check from `fund`, `settle`, `withdraw`, or `claim_investor_payout` and no test would fail.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert each value-moving entrypoint fails with the expected typed `EscrowError` while paused, and succeeds after unpausing.
- Assert read-only views such as `get_escrow` and `get_escrow_summary` remain callable while paused.
- Assert `set_paused` is admin-gated and that a redundant no-op call is handled as documented.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-pause-gating-tests`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`test(escrow): add pause switch gating matrix`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add tests for get_reconciliation balance-versus-liability accounting across the lifecycle"
labels: type:test, area:reconciliation, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add tests for get_reconciliation balance-versus-liability accounting across the lifecycle

### Description
`get_reconciliation()` returns a `ReconciliationView` comparing the live funding-token balance against outstanding liabilities, and `sweep_terminal_dust` relies on the same liability floor. The view has no dedicated test coverage across the states it is meant to describe.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Cover a fresh escrow, a partially funded escrow, a fully funded escrow, and post-settlement after some investors have claimed.
- Assert the view's liability figure tracks `funded_amount` minus `get_distributed_principal()` at every step.
- Include a cancelled-and-partially-refunded escrow and assert the surplus reported matches what `sweep_terminal_dust` would permit.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-reconciliation-tests`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`test(escrow): cover get_reconciliation across the escrow lifecycle`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add tests for get_settlement_readiness field-by-field against maturity and legal hold"
labels: type:test, area:settlement, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add tests for get_settlement_readiness field-by-field against maturity and legal hold

### Description
`get_settlement_readiness()` bundles settleability, legal-hold state, and maturity into a single `SettlementReadiness` struct, but nothing asserts each field independently against the underlying predicates such as `is_settleable()` and `get_legal_hold()`.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Assert every field matches its standalone view across pre-maturity, post-maturity, held, and already-settled escrows.
- Cover the zero-maturity and `has_maturity_lock()` false configurations explicitly.
- Assert the struct never reports settleable while a legal hold is active.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-settlement-readiness-tests`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`test(escrow): assert settlement readiness fields against source predicates`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add proptests asserting the protocol fee plus the SME net always equals the disbursed principal"
labels: type:test, area:protocol-fee, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add proptests asserting the protocol fee plus the SME net always equals the disbursed principal

### Description
`withdraw()` floors the protocol fee and leaves the rounding residue with the SME, so no value should ever be created or lost in the split. This conservation property is asserted only by hand-picked examples today, not across the input space.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Generate random `funded_amount` and `protocol_fee_bps` pairs across the full valid ranges including the `0` and `10_000` endpoints.
- Assert `fee + sme_net == funded_amount` exactly, that `fee` is never negative, and that `fee` never exceeds `funded_amount`.
- Assert the treasury and SME balance deltas after `withdraw()` match the computed legs.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-fee-split-proptest`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`test(escrow): add proptest for protocol fee split conservation`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Document the global pause switch and its precedence over the compliance legal hold"
labels: type:docs, area:pause, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Document the global pause switch and its precedence over the compliance legal hold

### Description
The contract now carries two independent freeze mechanisms — `set_paused`/`is_paused` and the timelocked legal hold behind `set_legal_hold`, `request_clear_legal_hold`, and `clear_legal_hold_after_delay`. Operators have no single document explaining which one to reach for or what happens when both are active.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Write `docs/escrow-pause.md` covering who can pause, which entrypoints are blocked, and how unpausing differs from clearing a hold.
- State the precedence when both are active and which typed `EscrowError` surfaces in that case.
- Cross-link `docs/escrow-legal-hold.md` and the `OPERATOR_RUNBOOK.md` incident procedure.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-document-pause-switch`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`docs(escrow): document the global pause switch and legal-hold precedence`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Document the pagination contract shared by every start and limit read view"
labels: type:docs, area:read-api, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Document the pagination contract shared by every start and limit read view

### Description
`get_investors`, `get_allowlisted_investors`, and `get_revoked_attestation_digests` all take `(start, limit)` but their exact semantics — clamping, ordering stability, and behavior when `start` is past the end — are not written down anywhere. Indexer authors currently have to read `lib.rs` to page safely.

### Requirements and context
- **Repository scope:** Liquifact/Liquifact-contracts only.
- Add a pagination section to `docs/escrow-read-api.md` specifying the half-open window, clamping rules, and the maximum `limit` honored.
- State whether ordering is stable across ledger writes and how a caller should detect the final page.
- Include a worked paging loop example and cross-link `docs/escrow-indexer.md`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-document-pagination-contract`
- **Write code in:** `escrow/src/lib.rs`
- **Write comprehensive tests in:** `escrow/src/tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`docs(escrow): document the shared start/limit pagination contract`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the Liquifact community on Discord:** https://discord.gg/JrGPH4V3
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
