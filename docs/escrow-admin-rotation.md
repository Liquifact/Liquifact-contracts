# Admin Rotation Guide — LiquiFact Escrow Contract

This document provides operational guidance for rotating the admin address on a LiquiFact escrow contract instance. The admin role controls governance-level operations including legal holds, maturity updates, funding target adjustments, and attestation binding.

---

## 🔐 Admin Rotation Checklist (Production)

Before executing an admin transfer, verify each item:

### 1. Verify Current Admin Identity
- [ ] Confirm the current admin address matches your controlled wallet/contract
- [ ] Check on-chain state: `client.get_escrow().admin`
- [ ] Cross-reference with off-chain governance records

### 2. Confirm New Admin Address Correctness
- [ ] Validate the new admin address character-by-character
- [ ] Test the address in a non-production environment first
- [ ] If using a multisig or governed contract, verify its deployment and configuration
- [ ] **CRITICAL:** An incorrect address results in **permanent loss of control**

### 3. Use Multisig Wallet (Recommended)
- [ ] Current admin should be a multisig or governed contract (not a single key)
- [ ] New admin should be a multisig or governed contract
- [ ] Ensure sufficient signers are available for both old and new admin addresses
- [ ] Document the multisig threshold and signer list off-chain

### 4. Coordinate Off-Chain Approvals
- [ ] Notify all stakeholders (governance council, risk team, SME)
- [ ] Obtain required approvals per your governance policy
- [ ] Log the rotation request with timestamp and rationale
- [ ] Schedule the transfer during a maintenance window if possible

### 5. Verify Transaction Before Execution
- [ ] Double-check the `new_admin` parameter in the transaction
- [ ] Simulate the transaction in a test environment
- [ ] Verify gas/fee estimates
- [ ] Confirm the transaction will call `transfer_admin(new_admin)` on the correct contract

### 6. Execute Transfer
```rust
client.transfer_admin(&new_admin);
```

### 7. Post-Transfer Verification
- [ ] Verify event emission: `AdminTransferredEvent` with correct `old_admin` and `new_admin`
- [ ] Confirm state update: `client.get_escrow().admin == new_admin`
- [ ] Test admin access with new admin address (e.g., call a non-destructive admin function)
- [ ] Notify stakeholders of successful rotation
- [ ] Update off-chain governance records and runbooks

---

## ⏳ Operational Best Practices

### Timelock Mechanisms
While this contract does not embed an on-chain timelock, production deployments should consider:

- **Off-chain timelock policy:** Require a waiting period (e.g., 24-48 hours) between approval and execution
- **Governance layer timelock:** If admin is a DAO or multisig, configure a proposal-to-execution delay
- **Monitoring window:** Alert on `AdminTransferredEvent` emission and verify within SLA

### Logging and Monitoring
- **Indexer requirement:** Track all `AdminTransferredEvent` emissions
- **Alerting:** Set up alerts for any admin change event
- **Audit trail:** Maintain an off-chain log of all admin rotations with:
  - Timestamp
  - Old admin address
  - New admin address
  - Approving parties
  - Rationale

### Frequency Restrictions
- **Limit rotation frequency:** Avoid frequent admin changes (recommended: ≤ 4 per year)
- **Document each rotation:** Maintain a change log in your governance system
- **Review access patterns:** Periodically audit who has access to the admin key/multisig

---

## ⚠️ Risks

### Critical Risks

#### 1. Incorrect Admin Address → Permanent Loss of Control
- **Impact:** HIGH — Contract becomes unmanageable
- **Mitigation:** 
  - Triple-check address before submission
  - Test with a small-value test contract first
  - Use address book / ENS-like naming if available
  - Implement a proposal-accept pattern in future versions

#### 2. Compromised Admin Key → Contract Takeover
- **Impact:** CRITICAL — Attacker can enable legal holds, change maturity, etc.
- **Mitigation:**
  - Use multisig or hardware wallets
  - Rotate keys proactively on a schedule
  - Monitor for unauthorized `AdminTransferredEvent` emissions
  - Maintain an incident response plan

#### 3. No-Op Transfers Hiding Operational Mistakes
- **Impact:** MEDIUM — Wasted gas, confusion in audit logs
- **Mitigation:**
  - The contract **rejects** `new_admin == current_admin` with a deterministic error
  - No event is emitted on no-op attempts
  - Review transaction logs to catch accidental submissions

### Operational Risks

#### 4. Uncoordinated Rotation
- **Impact:** HIGH — Multiple parties may attempt simultaneous transfers
- **Mitigation:** Coordinate via off-chain governance processes

#### 5. New Admin Unprepared
- **Impact:** MEDIUM — New admin cannot perform duties if keys/wallets not ready
- **Mitigation:** Verify new admin setup before executing transfer

---

## 🔍 Event Monitoring

### AdminTransferredEvent Schema

```rust
pub struct AdminTransferredEvent {
    #[topic]
    pub name: Symbol,          // "admin"
    #[topic]
    pub invoice_id: Symbol,    // Invoice identifier
    pub old_admin: Address,    // Previous admin address
    pub new_admin: Address,    // New admin address
}
```

### Indexer Requirements

Indexers and monitoring systems must:

1. **Listen for `AdminTransferredEvent`** on all deployed escrow contracts
2. **Extract payload fields:**
   - `old_admin`: The previous admin address
   - `new_admin`: The new admin address
   - `invoice_id`: Which escrow instance was affected
3. **Verify state consistency:**
   - After event emission, query `get_escrow().admin` to confirm it matches `new_admin`
   - Alert if state diverges from event (should not happen with correct implementation)
4. **Maintain an admin history table:**
   - Timestamp of rotation
   - Old admin → New admin mapping
   - Associated invoice_id

### Event Validation

- **Emitted ONLY on successful transfer** — no event on failure or no-op
- **Atomic emission** — event is emitted after state mutation in the same transaction
- **Deterministic payload** — `old_admin` is captured before mutation, `new_admin` after

### Example Monitoring Query (Pseudocode)

```sql
SELECT 
  event_timestamp,
  invoice_id,
  old_admin,
  new_admin
FROM AdminTransferredEvent
WHERE contract_address = '<escrow_contract>'
ORDER BY event_timestamp DESC;
```

---

## 🛡️ Security Requirements

The `transfer_admin` function enforces:

1. **Strict Authorization**
   - Only the current admin can call `transfer_admin`
   - Unauthorized calls panic with no state mutation or event emission

2. **No-Op Rejection**
   - `new_admin == current_admin` → panic with `"New admin must differ from current admin"`
   - No event emitted, no state mutated

3. **Atomic State Transition**
   - Admin address updated in single storage write
   - No partial state possible

4. **Deterministic Errors**
   - All failure paths panic with descriptive messages
   - No silent failures or ambiguous error codes

5. **Event Integrity**
   - Event emitted exactly once per successful transfer
   - Payload matches actual state change
   - No extra or malformed fields

---

## 🧠 Assumptions

### Role Scope

- **Admin controls governance functions only:**
  - Legal hold set/clear
  - Maturity updates (open state)
  - Funding target updates (open state)
  - Attestation binding
  - Admin transfer
  - Allowlist management

- **Admin does NOT control:**
  - Token economics (funding, settlement, payouts)
  - SME operations (withdrawal, collateral recording)
  - Investor operations (funding, claiming)

### External Calls

- Refer to [escrow/src/external_calls.rs](../escrow/src/external_calls.rs) for token transfer semantics
- Admin rotation does not interact with token contracts or external systems

### Trust Model

- The admin address is set at initialization and can only be changed via `transfer_admin`
- Production deployments should use a **governed contract or multisig** as admin
- This contract does not embed timelock or council multisig logic — those are off-chain concerns

---

## 📚 Related Documentation

- [ADR-004: Legal Hold](./adr/ADR-004-legal-hold.md) — Admin role in compliance operations
- [OPERATOR_RUNBOOK.md](./OPERATOR_RUNBOOK.md) — General operational procedures
- [escrow-lifecycle.md](./escrow-lifecycle.md) — State machine and role separation
- [glossary.md](./glossary.md) — Role definitions (Admin, SME, Investor)

---

## 📝 Change Log

| Date | Version | Summary |
|------|---------|---------|
| 2026-04-27 | 1.0 | Initial admin rotation guide with event schema and security checklist |
