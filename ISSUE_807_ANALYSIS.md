# Issue #807: Yield-Tier Bounds Validation - Analysis

## Step 1: Identification of Yield-Tier Entrypoints

After systematic search, there are **THREE main yield-tier entry points** that accept parameters:

### 1. `LiquifactEscrow::init()` - Primary initialization
   - **Location**: lib.rs, line 1810
   - **Parameters related to yield tiers**:
     - `yield_bps: i64` - base escrow yield in basis points
     - `yield_tiers: Option<Vec<YieldTier>>` - optional tier configuration
     - `protocol_fee_bps: Option<i64>` - protocol fee in basis points (NEW, added to validate)

### 2. `LiquifactEscrow::fund_with_commitment()` - Investor funding with tier commitment
   - **Location**: lib.rs, line 3904
   - **Parameters related to yield tiers**:
     - `committed_lock_secs: u64` - investor's lock duration for tier selection

### 3. `LiquifactEscrow::preview_yield_tier()` - Read-only yield tier preview
   - **Location**: lib.rs, line 2738
   - **Parameters**:
     - `amount: i128` - funding amount (currently unused in logic)
     - `lock: u64` - lock duration for tier selection

---

## Step 2: Current EscrowError Variants (Yield-Tier Related)

**Existing error codes that are ALREADY VALIDATED**:

1. **`TierYieldOutOfRange = 10`** - Tier yield_bps not in `0..=10_000`
2. **`TierYieldBelowBase = 11`** - Tier yield_bps < base yield_bps
3. **`TierLockNotIncreasing = 12`** - Tier min_lock_secs not strictly increasing
4. **`TierYieldNotNonDecreasing = 13`** - Tier yield_bps not non-decreasing
5. **`YieldBpsOutOfRange = 2`** - Base yield_bps not in `0..=10_000`
6. **`ProtocolFeeBpsOutOfRange = 215`** - Protocol fee not in `0..=10_000`

**Current validation location**: `validate_yield_tiers_table()` (lib.rs, line 1711)

---

## Step 3: Bounds Analysis Per Parameter

### Parameter Analysis: `init()` - yield_bps (base yield)

**Currently validated**: YES
- **Existing check**: `(0..=10_000).contains(&yield_bps)` at line 1838
- **Error code**: `YieldBpsOutOfRange = 2`
- **Derived reasoning**:
  - Basis points convention: 1 bps = 0.01% = 1/10,000
  - Maximum: 10,000 bps = 100% = valid full-amount yield
  - Minimum: 0 bps = 0% = valid zero yield (passive bond)
  - Used in: `compute_investor_payout()` (line 2640) → `coupon = principal * yield / 10_000`
  - Arithmetic: `i128::MAX * 10_000 / 10_000 = i128::MAX` ✓ no overflow
  - Arithmetic: `0 * yield / 10_000 = 0` ✓ safe
- **Status**: Already properly bounded

### Parameter Analysis: `init()` - protocol_fee_bps

**Currently validated**: YES
- **Existing check**: `(0..=10_000).contains(&protocol_fee_bps)` at line 1850
- **Error code**: `ProtocolFeeBpsOutOfRange = 215`
- **Derived reasoning**:
  - Same basis points convention as yield_bps
  - Maximum: 10,000 bps = 100% = route all SME payout to treasury
  - Minimum: 0 bps = 0% = no fee (default)
  - Used in: `withdraw()` (line 4225+) → fee calculation splits SME payout
  - Arithmetic: Same overflow safety as yield_bps
- **Status**: Already properly bounded

### Parameter Analysis: `init()` - yield_tiers table

**Currently validated**: YES (but at per-tier level)
- **Existing checks** in `validate_yield_tiers_table()`:
  1. Per-tier `yield_bps` in `0..=10_000` → `TierYieldOutOfRange = 10`
  2. Per-tier `yield_bps >= base_yield` → `TierYieldBelowBase = 11`
  3. `min_lock_secs` strictly increasing → `TierLockNotIncreasing = 12`
  4. `yield_bps` non-decreasing → `TierYieldNotNonDecreasing = 13`
- **Missing**: No validation on individual tier `min_lock_secs` range!
  - **Issue**: `min_lock_secs` is a `u64` (unsigned 64-bit)
  - **Min valid**: 0 (no minimum lock)
  - **Max valid**: `u64::MAX` (18,446,744,073,709,551,615 seconds ≈ 584 billion years)
  - **Actual risk**: `min_lock_secs` is only used for comparison in `effective_yield_for_commitment()` (line 1778)
    - `if committed_lock_secs >= t.min_lock_secs`
    - Pure comparison, no arithmetic, no overflow risk
  - **Conclusion**: `u64` range is inherently safe; no additional bounds needed
- **Status**: Already properly bounded

### Parameter Analysis: `fund_with_commitment()` - committed_lock_secs (u64)

**Currently PARTIALLY validated**:
- **Existing check**: `CommitmentLockExceedsMaturity = 111` at line 4126
  - Validates: `now + committed_lock_secs` must not exceed escrow maturity
- **Missing**: Validation that `committed_lock_secs` itself is reasonable
- **Analysis**:
  - **Min valid**: 0 seconds (no lock commitment)
  - **Max valid**: Should be bounded to prevent overflow when added to current timestamp
    - Safe maximum: `u64::MAX` (no arithmetic danger in effective_yield_for_commitment)
    - But logically: should not lock investor past escrow maturity
    - **Already checked**: `CommitmentLockExceedsMaturity` guard prevents `now + committed_lock_secs > maturity`
  - **Used in**: `effective_yield_for_commitment()` (line 1778) → pure comparison
  - **Used in**: Setting `InvestorClaimNotBefore` (line 4122) → `now + committed_lock_secs`
  - **Overflow check**: Line 4124 checks `checked_add` → already guarded with `InvestorClaimTimeOverflow`
- **Conclusion**: All arithmetic is already guarded; `u64` range is safe
- **Status**: Already properly bounded at critical points

### Parameter Analysis: `preview_yield_tier()` - amount (i128)

**Currently NOT validated**:
- **What it does**: Parameter is explicitly unused (line 2740: `let _ = amount`)
- **Why it exists**: Signature parity with `fund_with_commitment()` for API consistency
- **Risk**: Zero (not used in any logic)
- **Conclusion**: No validation needed
- **Status**: No validation required (intentionally unused)

### Parameter Analysis: `preview_yield_tier()` - lock (u64)

**Currently NOT validated** as standalone parameter:
- **What it does**: Passed to `effective_yield_for_commitment()` (line 2742)
- **Risk analysis**:
  - Used in comparison only: `committed_lock_secs >= t.min_lock_secs` (line 1778)
  - Returns tuple `(i64, u64)` with best yield and best_lock
  - No arithmetic, no overflow, pure comparison logic
- **Conclusion**: `u64::MAX` is safe; no additional bounds needed
- **Status**: No validation required (pure comparison logic)

---

## Step 4: Summary of Required Changes

### Parameters Already Validated (No Action Needed)
1. ✅ `init()::yield_bps` - validated to `0..=10_000`
2. ✅ `init()::protocol_fee_bps` - validated to `0..=10_000`
3. ✅ `init()::yield_tiers` table structure - all checks in place
4. ✅ `fund_with_commitment()::committed_lock_secs` - guarded by maturity overflow check
5. ✅ `preview_yield_tier()::lock` - used in pure comparison only

### Parameters NOT Validated (But Don't Need It)
1. ⚠️ `preview_yield_tier()::amount` - intentionally unused, no risk
2. ⚠️ `fund_with_commitment()::committed_lock_secs` - already guarded by maturity check

### Actual Gaps Found
**Issue**: Current validation in `validate_yield_tiers_table()` (called from `init()`) is GOOD.
However, there's no documentation on what makes a "valid" tier table.

**Missing Documentation**:
- The function `validate_yield_tiers_table()` lacks comprehensive rustdoc
- The error conditions could be better documented in the init() function docs
- No doc comments explaining the constraints

---

## Step 5: Recommendation for Issue #807

**STATUS**: Yield-tier bounds validation is **already comprehensive**. The existing code already:
- Validates base `yield_bps` to `0..=10_000`
- Validates protocol fee to `0..=10_000`
- Validates all tier `yield_bps` to `0..=10_000`
- Validates tier `yield_bps >= base_yield`
- Validates tier `min_lock_secs` strictly increasing
- Validates tier `yield_bps` non-decreasing
- Prevents `committed_lock_secs` from exceeding maturity
- Prevents overflow in investor claim-not-before calculations

**What's Missing**:
1. **Documentation**: The bounds are not explicitly documented in rustdoc
2. **Visibility**: Error codes are typed but constraints not clearly explained in function docs

**Action for Issue #807**:
1. Add comprehensive rustdoc to `init()` explaining all yield-tier validation rules
2. Add rustdoc to `fund_with_commitment()` explaining lock-secs constraints
3. Add rustdoc to `preview_yield_tier()` explaining parameters and bounds
4. Add rustdoc to `validate_yield_tiers_table()` (currently private, but should be explicit)
5. Optionally: Add validation tests to explicitly cover boundary cases

---

## Appendix: Derived Bounds (With Justification)

| Parameter | Function | Valid Range | Derivation | Error Code |
|-----------|----------|-------------|-----------|-----------|
| `yield_bps` | `init()` | `0..=10_000` | Basis points; 10_000 = 100%; math uses / 10_000 | `YieldBpsOutOfRange` |
| `protocol_fee_bps` | `init()` | `0..=10_000` | Basis points; same convention as yield_bps | `ProtocolFeeBpsOutOfRange` |
| Tier `yield_bps` | `init()` | `0..=10_000` | Basis points; >= base_yield | `TierYieldOutOfRange` |
| Tier `min_lock_secs` | `init()` | `0..=u64::MAX` | u64 natural range; only used in comparison (no arithmetic) | N/A (already safe) |
| `committed_lock_secs` | `fund_with_commitment()` | `0..=u64::MAX` | u64 natural range; guarded by maturity check | `CommitmentLockExceedsMaturity` |
| `lock` | `preview_yield_tier()` | `0..=u64::MAX` | u64 natural range; pure comparison logic | N/A (already safe) |
| `amount` | `preview_yield_tier()` | Any `i128` | Unused parameter; no validation needed | N/A |

