# Escrow Contract Error Messages

This document contains reference codes for typed `EscrowError` values emitted by the LiquiFact escrow smart contract.

## Settlement & Bounds Errors

| Error Name | Code | Description |
|---|---|---|
| `SettlementAmountInvalid` | 100 | The settlement amount is non-positive or exceeds remaining unsettled principal. |
| `MaturityNotReached` | 101 | Settlement attempted before contract maturity timestamp. |
| `EscrowNotInFundedState` | 102 | Operation requires the escrow to be in `Funded` status. |
| `WithdrawAmountInvalid` | 103 | The withdrawal amount is non-positive or exceeds available funded balance. |