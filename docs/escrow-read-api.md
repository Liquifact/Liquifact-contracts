# Escrow Read API

Soroban read-only entry points on `LiquifactEscrow`. All functions take `env: Env` and are
view-only (no state mutation, no auth required).

---

## `get_funding_token() → Address`

**Storage key:** `DataKey::FundingToken`

Returns the SEP-41 token contract address bound to this escrow instance at `init`.

- **Immutable** — set once at `init`; cannot change after deploy.
- Panics with `"Funding token not set"` if called before `init`.
- This is the only token that `sweep_terminal_dust` may transfer to the treasury.

---

## `get_treasury() → Address`

**Storage key:** `DataKey::Treasury`

Returns the protocol treasury address that receives terminal dust sweeps.

- **Immutable** — set once at `init`; cannot change after deploy.
- Panics with `"Treasury not set"` if called before `init`.
- The treasury must authorize `sweep_terminal_dust`; the admin cannot sweep unless it is also the treasury.

---

## `get_registry_ref() → Option<Address>`

**Storage key:** `DataKey::RegistryRef`

Returns the optional registry contract address supplied at `init`, or `None` when absent.

### Non-authority model

`RegistryRef` is a **read-only discoverability hint** for off-chain indexers only.

- No on-chain logic in this contract reads or calls this address.
- Its presence **does not** prove registry membership; call the registry contract directly to
  verify any on-chain claim.
- `None` is a valid, fully operational state — registry integration is optional.
- The key is omitted from instance storage entirely when `registry = None` at `init`, so
  `get_registry_ref()` on an uninitialized contract also returns `None`.

---

## `get_escrow() → InvoiceEscrow`

**Storage key:** `DataKey::Escrow`

Returns the full escrow snapshot. Panics with `"Escrow not initialized"` before `init`.

---

## `get_version() → u32`

**Storage key:** `DataKey::Version`

Returns the current schema version (`SCHEMA_VERSION`). Returns `0` before `init`.

---

## `get_legal_hold() → bool`

**Storage key:** `DataKey::LegalHold`

Returns `true` when a compliance hold is active. Defaults to `false` when the key is absent.

---

## `get_min_contribution_floor() → i128`

**Storage key:** `DataKey::MinContributionFloor`

Returns the per-call funding floor in token base units. `0` means no extra floor.

---

## `get_max_unique_investors_cap() → Option<u32>`

**Storage key:** `DataKey::MaxUniqueInvestorsCap`

Returns the optional cap on distinct investor addresses. `None` means unlimited.

---

## `get_unique_funder_count() → u32`

**Storage key:** `DataKey::UniqueFunderCount`

Returns the count of distinct addresses that have contributed principal. Initialized to `0` at `init`.

---

## `get_contribution(investor: Address) → i128`

**Storage key:** `DataKey::InvestorContribution(investor)`

Returns the cumulative principal contributed by `investor`. `0` when absent.

---

## `get_funding_close_snapshot() → Option<FundingCloseSnapshot>`

**Storage key:** `DataKey::FundingCloseSnapshot`

Returns the pro-rata denominator snapshot captured when the escrow first became **funded** (status 1).
`None` until that transition. Immutable once written.

---

## `get_investor_yield_bps(investor: Address) → i64`

**Storage key:** `DataKey::InvestorEffectiveYield(investor)`

Returns the effective annualized yield (bps) locked in at the investor's first deposit.
Falls back to `InvoiceEscrow::yield_bps` when the key is absent (legacy positions).

---

## `get_investor_claim_not_before(investor: Address) → u64`

**Storage key:** `DataKey::InvestorClaimNotBefore(investor)`

Returns the earliest ledger timestamp at which the investor may call `claim_investor_payout`.
`0` means no extra gate beyond settled status.

---

## `get_sme_collateral_commitment() → Option<SmeCollateralCommitment>`

**Storage key:** `DataKey::SmeCollateralPledge`

Returns the SME collateral pledge metadata, or `None` when never recorded.

**Record-only:** this is not an enforced on-chain asset lock.

---

## `is_investor_claimed(investor: Address) → bool`

**Storage key:** `DataKey::InvestorClaimed(investor)`

Returns `true` when the investor has exercised `claim_investor_payout`. Defaults to `false`.

---

## `get_primary_attestation_hash() → Option<BytesN<32>>`

**Storage key:** `DataKey::PrimaryAttestationHash`

Returns the single-set 32-byte attestation digest, or `None` when unbound.

---

## `get_attestation_append_log() → Vec<BytesN<32>>`

**Storage key:** `DataKey::AttestationAppendLog`

Returns the append-only audit chain of digests. Returns an empty `Vec` when no entries exist.
Bounded by `MAX_ATTESTATION_APPEND_ENTRIES`.
