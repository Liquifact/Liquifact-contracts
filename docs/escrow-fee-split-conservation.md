# Escrow protocol fee split conservation

This note documents the accounting identity that governs the protocol fee leg of
`withdraw()`, and the property tests that enforce it.

Reference issue: #663.

## Formula

The fee is a floored (truncating) basis-point share of the disbursed principal:

```text
fee     = funded_amount * protocol_fee_bps / 10_000   (integer division, floor)
sme_net = funded_amount - fee
```

`protocol_fee_bps` is optional at `init` time. When it is absent or `0`, no fee
is charged and no fee record is written.

## Invariants

| Invariant | Meaning |
| --- | --- |
| `fee + sme_net == funded_amount` | Exact conservation. The split never creates or destroys value. |
| `fee >= 0` | The fee leg is never negative for any valid rate. |
| `fee <= funded_amount` | The fee never exceeds the principal, including at the `10_000` bps endpoint. |
| residue stays with the SME | Because `fee` is floored, any rounding remainder is retained by `sme_net`. |
| treasury delta `== fee` | The treasury token balance moves by exactly the fee leg. |
| SME delta `== sme_net` | The SME token balance moves by exactly the net leg. |
| one fee record per non-zero fee | A zero fee writes no `FeeRecord`. |

## Endpoints

| `protocol_fee_bps` | `fee` | `sme_net` |
| --- | --- | --- |
| `0` | `0` | `funded_amount` |
| `10_000` | `funded_amount` | `0` |

Both endpoints are covered by dedicated deterministic tests in addition to the
generated cases.

## Where the tests live

`escrow/src/tests/fee_split_proptest.rs`

| Test | Kind | Asserts |
| --- | --- | --- |
| `prop_fee_plus_sme_net_equals_disbursed_principal` | proptest | conservation, non-negativity, upper bound, and treasury/SME balance deltas after `withdraw()` |
| `prop_fee_record_matches_computed_fee_leg` | proptest | the `FeeRecord` amount and treasury agree with the computed fee leg; a zero fee writes nothing |
| `fee_split_endpoint_zero_bps_gives_sme_everything` | unit | `0` bps endpoint |
| `fee_split_endpoint_max_bps_gives_treasury_everything` | unit | `10_000` bps endpoint |
| `fee_split_rounding_residue_stays_with_sme` | unit | flooring residue direction |
| `fee_split_minimum_principal_floors_fee_to_zero` | unit | smallest valid principal |

The generators sweep `funded_amount` across the valid positive range and
`protocol_fee_bps` across the whole `0..=10_000` range, endpoints included.

Related documents: `docs/escrow-numeric-model.md`, `docs/fees-states.md`,
`docs/fees-errors.md`, `docs/fees-auth.md`.
