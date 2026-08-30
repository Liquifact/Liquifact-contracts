# Fees State Machine Documentation

This document describes the state machine, allowed state transitions, entrypoint enforcement, and invariants governing protocol fees in `LiquifactEscrow`.

---

## Overview & Scope

"Fees" in this contract refers exclusively to the immutable `protocol_fee_bps` split applied to the SME principal during `LiquifactEscrow::withdraw`. Fee rates are set once at contract initialization (`init`) and cannot be modified afterwards.

---

## State Diagram

```mermaid
stateDiagram-v2
    [*] --> Unconfigured : Contract Deployment

    Unconfigured --> Configured : init(protocol_fee_bps)
    
    note right of Configured
        Fee state is IMMUTABLE.
        Stored under DataKey::ProtocolFeeBps
    end note

    state Configured {
        [*] --> Idle : Escrow Active / Awaiting SME Withdrawal
        
        Idle --> FeeCalculation : withdraw()
        FeeCalculation --> DisburseTreasury : protocol_fee_bps > 0
        FeeCalculation --> DirectDisburse : protocol_fee_bps == 0
        
        DisburseTreasury --> WithdrawalComplete : Route fee to Treasury & net payout to SME
        DirectDisburse --> WithdrawalComplete : Transfer full principal to SME
        
        WithdrawalComplete --> Idle : Escrow Active / Closed
    }
| Current State | Description | Trigger / Entrypoint | Next Allowed State |
| :--- | :--- | :--- | :--- |
| **`Unconfigured`** | Contract is deployed but uninitialized. `DataKey::ProtocolFeeBps` is unset in storage. | Contract Deployment | `Configured` |
| **`Configured`** | `protocol_fee_bps` (0 <= bps <= 10,000) is stored immutably under `DataKey::ProtocolFeeBps`. | `LiquifactEscrow::init` | **Immutable** (No fee updates allowed) |
| **`Idle`** | Contract initialized and operational; waiting for SME withdrawal. | `init` complete | `FeeCalculation` |
| **`FeeCalculation`** | Calculates fee split and SME payout floor division. | `LiquifactEscrow::withdraw` | `DisburseTreasury` or `DirectDisburse` |
| **`WithdrawalComplete`** | Transfers executed according to the computed fee split. | Settlement completion | `Idle` |

---

## Entrypoints & State Enforcement

### 1. `LiquifactEscrow::init`
* **Auth Guard:** `admin.require_auth()`
* **Parameter:** `protocol_fee_bps: Option<i64>` (defaults `None` to `0`)
* **Validation:** Enforces 0 <= protocol_fee_bps <= 10,000.
* **Rejection:** Values outside this range revert with `EscrowError::ProtocolFeeBpsOutOfRange` (**215**).
* **Storage Write:** Saves `protocol_fee_bps` immutably under `DataKey::ProtocolFeeBps`.

### 2. `LiquifactEscrow::withdraw`
* **Auth Guard:** `sme_address.require_auth()`
* **State Operations:**
  1. Reads `protocol_fee_bps` from `DataKey::ProtocolFeeBps`.
  2. Computes the protocol fee and SME net payout:
     * `fee = floor(funded_amount * protocol_fee_bps / 10_000)`
     * `sme_payout = funded_amount - fee`
  3. Transfers `fee` to `DataKey::Treasury` and `sme_payout` to `InvoiceEscrow::sme_address`.
* **Conservation Invariant:** Strictly guarantees `sme_payout + fee == funded_amount`.
