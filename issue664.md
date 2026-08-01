Document the global pause switch and its precedence over the compliance legal hold
Description
The contract now carries two independent freeze mechanisms — set_paused/is_paused and the timelocked legal hold behind set_legal_hold, request_clear_legal_hold, and clear_legal_hold_after_delay. Operators have no single document explaining which one to reach for or what happens when both are active.

Requirements and context
Repository scope: Liquifact/Liquifact-contracts only.
Write docs/escrow-pause.md covering who can pause, which entrypoints are blocked, and how unpausing differs from clearing a hold.
State the precedence when both are active and which typed EscrowError surfaces in that case.
Cross-link docs/escrow-legal-hold.md and the OPERATOR_RUNBOOK.md incident procedure.
Suggested execution
Fork the repo and create a branch
git checkout -b docs/contracts-document-pause-switch
Write code in: escrow/src/lib.rs
Write comprehensive tests in: escrow/src/tests.rs
Add documentation: README / docs
Include NatSpec-style /// comments
Test and commit
Run cargo fmt --all -- --check, cargo build, cargo test
Cover edge cases and failure paths
Example commit message
docs(escrow): document the global pause switch and legal-hold precedence

Guidelines
Minimum 95 percent test coverage for impacted modules
Clear documentation
Timeframe: 96 hours.