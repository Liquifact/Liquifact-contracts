# Architecture Decision Records

Key design decisions for the LiquiFact escrow contract.

| ADR | Title | Status |
|-----|-------|--------|
| [ADR-001](ADR-001-state-model.md) | Escrow State Model | Accepted |
| [ADR-002](ADR-002-auth-boundaries.md) | Authorization Boundaries | Accepted |
| [ADR-003](ADR-003-settlement-flow.md) | Settlement Flow | Accepted |
| [ADR-004](ADR-004-legal-hold.md) | Legal / Compliance Hold | Accepted |
| [ADR-005](ADR-005-tiered-yield.md) | Optional Tiered Yield and Commitment Locks | Accepted |
| [ADR-006](ADR-006-dust-sweep-and-token-safety.md) | Treasury Dust Sweep and Token Safety | Accepted |
| [ADR-007](ADR-007-storage-key-evolution.md) | Storage Key Evolution and Additive-Key Policy | Accepted |

Each ADR links directly to the relevant sections of [`escrow/src/lib.rs`](../../escrow/src/lib.rs).
