# Escrow Token Integration Security Checklist

This checklist describes the supported assumptions and explicit unsupported token behaviors for integrations that use the LiquiFact escrow contract with cross-contract token assets.

## Supported token assumptions

- Amounts are recorded in the escrow contract as raw smallest units using `i128`.
  - Integration layers must convert external human-readable amounts into smallest units before calling `fund`.
  - Do not rely on asset decimals inside the escrow contract; the contract stores integer amounts only.
- The escrow contract does not itself perform token transfers or custody assets.
  - `record_sme_collateral_commitment` stores SME-reported metadata only and does not lock assets, verify custody, or create an enforceable on-chain claim.
  - Token movement must be handled separately by the integration layer.
- The contract uses strong signer authorization for state changes (`require_auth(...)` for admin, SME, and investor roles).
- Token asset identity should be established by token contract ID or audited registry, not by symbol alone.

## Integration-layer responsibilities

- Validate the token contract before use:
  - confirm the contract ID or hash is expected and audited
  - confirm the token contract is not paused, frozen, or blacklisted
  - confirm the token implements standard transfer semantics without hidden fees
- Normalize decimals outside the contract:
  - convert human-facing amounts into the token’s smallest unit
  - reject tokens with nonstandard decimals or dynamic fractional behavior
- Protect against malicious tokens:
  - do not integrate with fee-on-transfer or deflationary transfer tokens
  - do not integrate with tokens that have reentrant hooks or unexpected callback behavior
  - do not assume token contract invariants beyond the audited interface
- Use separate transfer preflight logic or atomic transfer flows to ensure on-chain escrow state matches actual token movement.

## Explicit unsupported token behavior warnings

The escrow contract and its documented assumptions do not support direct integration with the following token behaviors:

- Fee-on-transfer or deflationary tokens
- Paused, frozen, or blacklisted token contracts
- Nonstandard transfer semantics or callback-based reentrancy
- Dynamic decimals, fractional units outside integer smallest-unit semantics
- Malicious token contracts that alter balances in unexpected ways or change transfer metadata

## Terminal dust sweep (`sweep_terminal_dust`)

- The escrow uses [`escrow/src/external_calls.rs`](../escrow/src/external_calls.rs) to assert **exact** sender/recipient balance deltas for the configured **funding** token.
- Integrations must still treat **fee-on-transfer** and other non-standard tokens as **unsupported**; such tokens can cause the sweep to panic when deltas do not match `amount`.

## Why this matters

Because the contract only records numeric state and collateral metadata (aside from the guarded dust sweep transfer path), token integration security is enforced by the surrounding application or bridge logic.

- The escrow contract is safe for algebraic accounting of on-chain amounts.
- The integration layer must reject unsupported token patterns before calling escrow entrypoints.
- The collateral commitment record is not an on-chain asset lock and should not be treated as proof of custody; see [`escrow-sme-collateral.md`](escrow-sme-collateral.md).
