# Escrow Dependency Policy (`soroban-sdk` and host compatibility)

This policy defines how maintainers track, evaluate, and roll out Soroban dependency updates for
`liquifact_escrow`.

## 1) Version pin strategy

- Pin `soroban-sdk` and related Soroban crates in `Cargo.toml` to an explicit compatible series.
- Keep `Cargo.lock` committed so CI and reviewers execute the same resolved dependency graph.
- Avoid broad, implicit upgrades in feature branches unrelated to dependency maintenance.

## 2) Update cadence

### Weekly checks (lightweight)

- Review upstream Soroban/Stellar release notes for:
  - host behavior changes
  - VM/runtime breaking changes
  - deprecations affecting contract APIs or testutils
- Run dependency audit commands and open a tracking issue for any actionable findings.

### Monthly checks (full maintenance cycle)

- Evaluate patch/minor updates for `soroban-sdk` and direct transitive risk dependencies.
- Create a dedicated dependency-update branch.
- Run full escrow validation suite (`fmt`, `clippy`, `test`, `llvm-cov` policy gate).
- Record migration impact (if any) on contract behavior, events, or storage assumptions.

## 3) Regression testing for contract upgrades

For every dependency bump candidate:

1. Run existing CI gate commands from `README.md`.
2. Re-run scenario-critical tests:
   - funding to funded transition
   - settlement and claim path
   - legal hold gating
   - dust sweep guards
3. Validate event compatibility for indexers (`docs/EVENT_SCHEMA.md` expectations).
4. Confirm no accidental storage schema drift unless intentionally planned.

If behavior changes are detected, document them in the PR and propose explicit migration/redeploy
guidance before merge.

## 4) Tracking upstream breaking host changes

Maintainers should track Soroban host/runtime notes as first-class input for release risk:

- Monitor upstream release notes and advisories on each weekly check.
- Flag any host-level semantic change that could affect:
  - auth boundaries
  - event shape/order assumptions
  - token call behavior and balance-delta checks
  - ledger timestamp/sequence assumptions in tests

Open a dependency-risk issue immediately when uncertain impact exists, even before code changes.

## 5) Emergency bump process (security advisory or critical regression)

When a high-severity advisory or breakage is announced:

1. Open an incident issue (`severity`, affected versions, suspected blast radius).
2. Create emergency branch (example: `hotfix/deps-soroban-<version>`).
3. Apply minimal dependency bump and lockfile update.
4. Run mandatory checks:
   - `cargo fmt --all -- --check`
   - `cargo clippy -p liquifact_escrow -- -D warnings`
   - `cargo test -p liquifact_escrow`
   - `cargo llvm-cov --features testutils --fail-under-lines 95 --summary-only -p liquifact_escrow`
5. Perform merge dry-run against `upstream/main`.
6. Open PR with:
   - advisory reference
   - risk assessment
   - rollback plan
   - explicit note on token-economics assumptions remaining out of scope per
     `escrow/src/external_calls.rs`.

## 6) Scope boundaries

- This policy governs dependency update process and verification, not token-economics support
  expansion.
- Unsupported token models (fee-on-transfer/rebasing/hook behavior) remain out of scope unless
  explicitly accepted in a separate ADR and implementation PR.
