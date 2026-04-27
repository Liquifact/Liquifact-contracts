# Escrow Gas / Storage Notes

## Soroban cost model

Soroban charges CPU and memory instructions per host function invocation. Instance storage reads
(`env.storage().instance().get(...)`) each consume a measurable instruction budget. Eliminating
duplicate reads of the same key within a single host function call directly reduces the
instruction budget consumed per transaction — without changing observable behavior.

No benchmarking harness exists in this repository. `cargo llvm-cov` measures line coverage only.
Cost savings below are reasoned from the Soroban host cost schedule, not measured wall-clock time.

---

## Audit table — `escrow/src/lib.rs` (issue #185)

| Function | Reads before | Reads after | Change | Rationale |
|---|---|---|---|---|
| `fund_impl` (new investor path) | `UniqueFunderCount` read twice: once for cap check, once for increment | Once: hoisted into `cur_funder_count` local, reused for both uses | **−1 read** on every new-investor call | Single read covers cap assertion and increment write |
| `fund_impl` (event field) | `InvestorEffectiveYield` read after write to populate event | Captured in `investor_effective_yield_bps` local at write time | **−1 read** on every `fund` / `fund_with_commitment` call | Value is known at write time; post-write read is redundant |
| `fund_impl` (returning investor, `simple_fund`) | `InvestorEffectiveYield` not read (yield set on first deposit only) | Read once for event field on returning-investor path | No net change — read was already absent on first-deposit path; added on returning path to populate event correctly |  |
| `fund_impl` (`legal_hold_active` order) | Hold check after escrow read | Unchanged — escrow read is always needed for `yield_bps` and `status`; hoisting hold before escrow would not reduce reads | No change | Comment added explaining order |
| `sweep_terminal_dust` | `DataKey::Escrow`, `DataKey::Treasury`, `DataKey::FundingToken` — each read once | Same | No change — reads are non-redundant | Each key is distinct and read exactly once |
| `get_investor_yield_bps` | `DataKey::Escrow` + `DataKey::InvestorEffectiveYield` | Same | No change — reads are non-redundant | Escrow read is required for the base yield fallback; doc comment added to inform callers |
| All other `get_*` getters | Single read per key | Same | No change — reads are non-redundant | Each getter reads exactly one key |
| All `env.clone()` call sites | — | — | No change — all clones are required | `env` is used after every `get_escrow` call for storage writes, ledger reads, or event publish; comments added at each site |

---

## `env.clone()` audit

`Env` is a reference-counted handle to the Soroban host. Cloning it is cheap (pointer copy).
Every `Self::get_escrow(env.clone())` call site was audited; in all cases `env` is used again
after the call (for storage writes, ledger reads, or `.publish(&env)`), so the clone is
required. A comment was added at each site documenting this decision.

---

## Net savings per `fund` call (new investor)

- **−2 storage reads** on the new-investor path: one `UniqueFunderCount` read and one
  `InvestorEffectiveYield` read eliminated.
- **−1 storage read** on the returning-investor path: one `InvestorEffectiveYield` post-write
  read eliminated (replaced by a local variable); one read added to fetch the existing yield
  for the event field — net zero on this path.

These are micro-optimizations. The primary value is correctness hygiene and reduced instruction
budget on the hot `fund` path, which is called once per investor deposit.
