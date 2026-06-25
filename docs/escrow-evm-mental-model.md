# EVM Escrow Patterns vs. Soroban Invoice Escrow

This document provides a comparative overview for teams familiar with Ethereum Virtual Machine (EVM) escrow patterns, particularly OpenZeppelin's `Escrow.sol`, when transitioning to Soroban's invoice escrow implementation in the LiquiFact contracts. It highlights mappings, differences, and key considerations for cross-chain integrations.

## Overview

EVM escrows typically hold funds until predefined conditions are met, with simple deposit/withdraw mechanics. Soroban's invoice escrow extends this with structured invoice funding, settlement flows, legal holds, and Stellar-specific account/auth models. Both enforce token conservation but differ in execution environment, authorization, and state management.

## Key Comparisons

### 1. Account and Authorization Model

| Aspect | EVM (OpenZeppelin Escrow) | Soroban (LiquiFact Invoice Escrow) |
|--------|---------------------------|-----------------------------------|
| **Account Types** | EOAs (externally owned accounts) and contracts | Stellar accounts (public keys) and contracts |
| **Authorization** | `msg.sender` for transaction origin | `env.invoker()` or `require_auth()` for authenticated callers |
| **Admin Control** | Contract deployer or assigned admin | Immutable `admin` address set at init |
| **Beneficiary** | Designated address for withdrawals | SME (small/medium enterprise) address for invoice settlement |

**Mapping Note**: EVM's `msg.sender` maps to Soroban's authenticated invoker. Soroban requires explicit auth for state changes, unlike EVM's implicit sender checks.

### 2. Token Handling

| Aspect | EVM (ERC-20) | Soroban (SEP-41) |
|--------|--------------|------------------|
| **Standard** | ERC-20 interface | SEP-41 token contract |
| **Transfers** | `transfer` or `transferFrom` | `TokenClient::transfer` with balance checks |
| **Conservation** | Assumed; no built-in checks | Strict pre/post balance deltas enforced |
| **Fee-on-Transfer** | Out of scope (may break assumptions) | Explicitly out of scope; causes panics |

**Mapping Note**: Both use token contracts, but Soroban adds mandatory balance verification to prevent malicious tokens. EVM integrations should account for this when porting.

### 3. Escrow Mechanics

| Aspect | EVM Escrow | Soroban Invoice Escrow |
|--------|------------|-------------------------|
| **Deposit/Funding** | Unrestricted deposits to escrow | Structured funding with targets, minimums, and caps |
| **Conditions** | Custom logic for release | Status-based: open → funded → settled/withdrawn |
| **Withdrawal** | Beneficiary calls withdraw | SME withdraws after funding target met |
| **Settlement** | Not standardized | Explicit settle after maturity |
| **Legal Holds** | Not native | Admin-set holds block transitions |

**Mapping Note**: EVM's simple escrow maps to Soroban's "funded" status. Soroban's additional states (settled, withdrawn) and legal holds provide more governance but require different integration patterns.

### 4. State and Persistence

| Aspect | EVM | Soroban |
|--------|-----|---------|
| **Storage** | Contract storage slots | Instance storage with schema versioning |
| **Upgrades** | Proxy patterns or redeployment | Migration functions or redeployment |
| **Time** | Block timestamps | Ledger timestamps |
| **Events** | Emitted logs | Contract events |

**Mapping Note**: Soroban's instance storage loads fully per invocation, unlike EVM's partial loads. Ledger time replaces block time for maturity checks.

### 5. Execution and Reentrancy

| Aspect | EVM | Soroban |
|--------|-----|---------|
| **Reentrancy** | Possible; mitigated with checks | Not possible; calls complete before resumption |
| **Gas/Resource** | Gas limits | CPU/memory limits |
| **Cross-Contract** | Direct calls | Host functions |

**Mapping Note**: Soroban's non-reentrant model simplifies some security concerns but requires different async patterns for complex flows.

## Migration Considerations

- **Auth Boundaries**: Replace `msg.sender` checks with `require_auth(env.invoker())`.
- **Token Safety**: Add balance delta assertions around all transfers.
- **State Transitions**: Implement status enums instead of boolean flags.
- **Time Handling**: Use `env.ledger().timestamp()` for time-based logic.
- **Admin Roles**: Use immutable addresses instead of mutable roles.

## Out-of-Scope

- Token economics beyond standard ERC-20/SEP-41 (e.g., fee-on-transfer).
- Complex yield or attestation features unique to Soroban.
- Off-chain governance integrations.

This comparison assumes standard OpenZeppelin patterns. For custom EVM escrows, additional mappings may be needed.</content>
<parameter name="filePath">/workspaces/Liquifact-contracts/docs/escrow-evm-mental-model.md