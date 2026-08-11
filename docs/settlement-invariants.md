# Settlement Invariants

This document records the invariants that must always hold for settlement in the Liquifact escrow contract, and where each is enforced.

## 1. Settlement State Gating
Settlement can only occur when the escrow is in a valid state and not blocked by operational controls.

*   **Funded Status Required:** `settle` requires the escrow `status` to be exactly `1` (Funded). It cannot be called when Open (0), Settled (2), Withdrawn (3), or Cancelled (4).
    *   *Enforced in:* `settle` entrypoint via `guard_status_eq(&env, escrow.status, 1, EscrowError::SettlementNotFunded)`.
*   **Maturity Gate:** If the escrow has a non-zero `maturity` timestamp, settlement is blocked until the ledger timestamp is greater than or equal to `maturity`.
    *   *Enforced in:* `settle` entrypoint via `ensure(&env, now >= escrow.maturity, EscrowError::MaturityNotReached)`.
*   **Legal Hold Block:** Settlement is blocked if a legal hold is active (`DataKey::LegalHold`).
    *   *Enforced in:* `settle` and `partial_settle` entrypoints via `guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksSettlement)` and `EscrowError::LegalHoldBlocksPartialSettle`.
*   **Operational Pause Block:** Settlement is blocked if the protocol is paused (`DataKey::Paused`).
    *   *Enforced in:* `settle` entrypoint via `ensure(&env, !Self::paused_active(&env), EscrowError::PausedBlocksSettlement)`.
*   **SME Authorization:** Only the SME can trigger the main `settle` entrypoint.
    *   *Enforced in:* `settle` entrypoint via `Self::load_escrow_require_sme(&env)`.

## 2. Partial Settlement Gating
`partial_settle` acts as an early funding close to transition the escrow into a settleable state.

*   **Open Status Required:** `partial_settle` requires the escrow `status` to be `0` (Open).
    *   *Enforced in:* `partial_settle` entrypoint via `guard_status_eq(&env, escrow.status, 0, EscrowError::PartialSettleNotOpen)`.
*   **Authorization:** Must be called by either the SME or the Admin.
    *   *Enforced in:* `partial_settle` entrypoint via `ensure(&env, caller == escrow.sme_address || caller == escrow.admin, EscrowError::PartialSettleUnauthorizedCaller)`.
*   **Legal Hold Block:** Partial settlement is blocked if a legal hold is active.
    *   *Enforced in:* `partial_settle` entrypoint via `guard_not_legal_hold(&env, EscrowError::LegalHoldBlocksPartialSettle)`.

## 3. Storage and State Transitions
When a settlement or partial settlement occurs, specific storage invariants must be maintained.

*   **Settled Status:** Successful `settle` unconditionally transitions the escrow `status` to `2` (Settled).
*   **Early Funded Status:** Successful `partial_settle` unconditionally transitions the escrow `status` to `1` (Funded).
*   **SettledAt is Single-Write:** `DataKey::SettledAt` is written exactly once during `settle`. It cannot be overwritten because the state machine prevents returning to `status == 1` after `settle`.
    *   *Enforced in:* `settle` logic and state machine constraints.
*   **FundingCloseSnapshot:** `partial_settle` writes `DataKey::FundingCloseSnapshot` if it is not already present, ensuring the snapshot exists for settlement calculations.
*   **TTL Extension:** `settle` bumps the instance storage TTL to ensure the escrow remains rent-free and accessible post-settlement.
    *   *Enforced in:* implicitly managed through Soroban storage lifetime rules when the instance is updated.

## 4. Pre-Settlement Exclusivity
Certain operations are strictly prohibited once the escrow moves out of the pre-settlement phase.

*   **Beneficiary Rotation:** The SME beneficiary can only be rotated while the escrow is in a pre-settlement state (`0` or `1`).
    *   *Enforced in:* `rotate_beneficiary` via `ensure(&env, is_pre_settlement_status(escrow.status), EscrowError::RotationNotOpen)`.
*   **Investor Claims:** Investors cannot claim payouts until *after* settlement.
    *   *Enforced in:* `claim_investor_payout` via `EscrowError::InvestorClaimNotSettled`.

## 5. Single Source of Truth
*   **Readiness Mirrors Gate:** The read-only view `get_settlement_readiness` and `is_settleable` use the exact same underlying logic (`Self::settleable_now`) as the `settle` entrypoint to ensure off-chain queries never drift from on-chain enforcement.
    *   *Enforced in:* `settleable_now` internal helper.
