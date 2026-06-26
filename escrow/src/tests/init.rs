use super::*;
use crate::{EscrowInitialized, DEFAULT_MATURITY_MAX_HORIZON_SECS};
use proptest::prelude::*;
extern crate std;
use std::format;

// Initialization, getters, invoice-id validation, and init-shaped cost baselines.

#[test]
fn test_init_stores_escrow() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let escrow = client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_eq!(escrow.invoice_id, symbol_short!("INV001"));
    assert_eq!(escrow.admin, admin);
    assert_eq!(escrow.sme_address, sme);
    assert_eq!(escrow.amount, TARGET);
    assert_eq!(escrow.funding_target, TARGET);
    assert_eq!(escrow.funded_amount, 0);
    assert_eq!(escrow.yield_bps, 800);
    assert_eq!(escrow.maturity, 1000);
    assert_eq!(escrow.status, 0);
}

#[test]
fn test_init_stores_keyed_invoice_and_lists_it() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let escrow = client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV001"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    let got = client.get_escrow();
    assert_eq!(got, escrow);
}

#[test]
fn test_init_requires_admin_auth() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INVB"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert!(
        env.auths().iter().any(|(addr, _)| *addr == admin),
        "admin auth was not recorded for init"
    );
}

#[test]
fn test_migrate_requires_admin_auth_before_version_checks() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    env.mock_auths(&[]);
    let result = client.try_migrate(&99u32);

    assert!(
        result.is_err(),
        "migrate should reject an unauthenticated call"
    );
    assert!(
        !matches!(
            result,
            Err(Err(soroban_sdk::InvokeError::Contract(code)))
                if code == EscrowError::MigrationVersionMismatch as u32
        ),
        "migrate reached version checks before admin auth"
    );
}

#[test]
fn test_init_unauthorized_panics() {
    let env = Env::default();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.init(
            &admin,
            &soroban_sdk::String::from_str(&env, "INV001"),
            &sme,
            &1_000i128,
            &800i64,
            &1000u64,
            &Address::generate(&env),
            &None,
            &Address::generate(&env),
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
        );
    }));
    assert!(result.is_err(), "Expected panic without auth");
}

#[test]
#[should_panic]
fn test_double_init_panics() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);
    default_init(&client, &env, &admin, &sme);
}

#[test]
#[should_panic]
fn test_get_escrow_uninitialized_panics() {
    let env = Env::default();
    let client = deploy(&env);
    client.get_escrow();
}

#[test]
fn test_cost_baseline_init() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV100"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
fn test_cost_baseline_init_zero_maturity() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV101"),
        &sme,
        &TARGET,
        &800i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
fn test_cost_baseline_init_max_amount() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV102"),
        &sme,
        &i128::MAX,
        &800i64,
        &1000u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_init_invoice_id_empty_string_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (t, tr) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, ""),
        &sme,
        &1000i128,
        &500i64,
        &0u64,
        &t,
        &None,
        &tr,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_init_invoice_id_whitespace_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (t, tr) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV BAD"),
        &sme,
        &1000i128,
        &500i64,
        &0u64,
        &t,
        &None,
        &tr,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_init_invoice_id_too_long_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (t, tr) = free_addresses(&env);
    let thirty_three = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456";
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, thirty_three),
        &sme,
        &1000i128,
        &500i64,
        &0u64,
        &t,
        &None,
        &tr,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_init_invoice_id_bad_charset_hyphen_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (t, tr) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV-DASH"),
        &sme,
        &1000i128,
        &500i64,
        &0u64,
        &t,
        &None,
        &tr,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_init_invoice_id_non_ascii_multibyte_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let (admin, sme) = (Address::generate(&env), Address::generate(&env));
    let (t, tr) = free_addresses(&env);
    // "INV-💩" contains multi-byte UTF-8
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INV_💩"),
        &sme,
        &1000i128,
        &500i64,
        &0u64,
        &t,
        &None,
        &tr,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_init_invoice_id_embedded_null_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let (admin, sme) = (Address::generate(&env), Address::generate(&env));
    let (t, tr) = free_addresses(&env);

    // Create a string with an embedded null byte in the middle of valid chars
    let mut bytes = [b'A'; 10];
    bytes[5] = 0;
    let s = soroban_sdk::String::from_bytes(&env, &bytes[..]);

    client.init(
        &admin, &s, &sme, &1000i128, &500i64, &0u64, &t, &None, &tr, &None, &None, &None, &None,
        &None, &None,
    );
}

#[test]
fn test_init_stores_registry_some_and_getters() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let reg = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "REG001"),
        &sme,
        &5000i128,
        &100i64,
        &0u64,
        &token,
        &Some(reg.clone()),
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_registry_ref(), Some(reg));
    assert_eq!(client.get_funding_token(), token);
    assert_eq!(client.get_treasury(), treasury);
}

// --- min_contribution_floor init wiring ---

/// Floor stored and readable when a positive value is supplied at init.
#[test]
fn test_init_min_contribution_floor_stored() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FLOOR01"),
        &sme,
        &10_000i128,
        &500i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &Some(1_000i128),
        &None,
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_min_contribution_floor(), 1_000i128);
}

/// Floor defaults to 0 when `min_contribution` is `None`.
#[test]
fn test_init_min_contribution_floor_defaults_to_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FLOOR02"),
        &sme,
        &10_000i128,
        &500i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_min_contribution_floor(), 0i128);
}

/// `min_contribution = Some(0)` is rejected — the value must be positive when supplied.
#[test]
#[should_panic]
fn test_init_min_contribution_zero_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FLOOR03"),
        &sme,
        &10_000i128,
        &500i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &Some(0i128),
        &None,
        &None,
        &None,
        &None,
    );
}

/// `min_contribution` exceeding the invoice amount is rejected.
#[test]
#[should_panic]
fn test_init_min_contribution_exceeds_amount_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FLOOR04"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &Some(1_001i128),
        &None,
        &None,
        &None,
        &None,
    );
}

/// Floor equal to the invoice amount is the boundary — must be accepted.
#[test]
fn test_init_min_contribution_equal_to_amount_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (tok, tre) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FLOOR05"),
        &sme,
        &5_000i128,
        &500i64,
        &0u64,
        &tok,
        &None,
        &tre,
        &None,
        &Some(5_000i128),
        &None,
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_min_contribution_floor(), 5_000i128);
}

#[test]
fn test_get_funding_token_before_init_fails_with_typed_error() {
    let env = Env::default();
    let client = deploy(&env);
    assert_contract_error(
        client.try_get_funding_token(),
        EscrowError::FundingTokenNotSet,
    );
}

#[test]
fn test_get_treasury_before_init_fails_with_typed_error() {
    let env = Env::default();
    let client = deploy(&env);
    assert_contract_error(client.try_get_treasury(), EscrowError::TreasuryNotSet);
}

#[test]
fn test_get_funding_token_after_init_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "FT001"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_funding_token(), token);
}

#[test]
fn test_get_treasury_after_init_succeeds() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "TR001"),
        &sme,
        &TARGET,
        &800i64,
        &1000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_treasury(), treasury);
}

#[test]
fn test_get_registry_ref_before_init_returns_none() {
    let env = Env::default();
    let client = deploy(&env);
    assert_eq!(client.get_registry_ref(), None);
}

#[test]
fn test_init_registry_none_roundtrip() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "REG002"),
        &sme,
        &5000i128,
        &100i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_registry_ref(), None);
}

#[test]
fn test_init_escrow_initialized_event_includes_bound_refs() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let contract_id = client.address.clone();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let registry = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INIT_EVT"),
        &sme,
        &5_000i128,
        &100i64,
        &1_000u64,
        &token,
        &Some(registry.clone()),
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    assert_eq!(
        env.events().all(),
        std::vec![EscrowInitialized {
            name: symbol_short!("escrow_ii"),
            escrow: client.get_escrow(),
            funding_token: token,
            treasury,
            registry: Some(registry),
            has_maturity_lock: true,
        }
        .to_xdr(&env, &contract_id)]
    );
}

#[test]
fn test_init_escrow_initialized_event_registry_none() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let contract_id = client.address.clone();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "INIT_EVT2"),
        &sme,
        &5_000i128,
        &100i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    assert_eq!(
        env.events().all(),
        std::vec![EscrowInitialized {
            name: symbol_short!("escrow_ii"),
            escrow: client.get_escrow(),
            funding_token: token,
            treasury,
            registry: None,
            has_maturity_lock: false,
        }
        .to_xdr(&env, &contract_id)]
    );
}

// ---------------------------------------------------------------------------
// invoice_id boundary and charset fuzz/parameterized tests
// ---------------------------------------------------------------------------

/// Helper: attempt init with the given invoice_id string; returns Err on panic.
fn try_init_with_id(env: &Env, id: &str) -> Result<(), ()> {
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let (t, tr) = free_addresses(env);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.init(
            &admin,
            &String::from_str(env, id),
            &sme,
            &1_000i128,
            &500i64,
            &0u64,
            &t,
            &None,
            &tr,
            &None,
            &None,
            &None,
            &None,
            &None,
            &None,
        );
    }));
    result.map(|_| ()).map_err(|_| ())
}

// --- length boundary ---

/// Length 1 is the minimum valid length.
#[test]
fn test_invoice_id_length_1_accepted() {
    let env = Env::default();
    assert!(try_init_with_id(&env, "A").is_ok());
}

/// Length 32 is the maximum valid length (MAX_INVOICE_ID_STRING_LEN).
#[test]
fn test_invoice_id_length_32_accepted() {
    let env = Env::default();
    // 32 chars, all valid
    assert!(try_init_with_id(&env, "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345").is_ok());
}

/// Length 33 is one over the limit and must be rejected.
#[test]
#[should_panic]
fn test_invoice_id_length_33_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (t, tr) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456"), // 33 chars
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &t,
        &None,
        &tr,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

// --- charset: valid characters ---

/// All three character classes (upper, lower, digit, underscore) are accepted.
#[test]
fn test_invoice_id_all_valid_char_classes_accepted() {
    let env = Env::default();
    assert!(try_init_with_id(&env, "Az0_").is_ok());
}

/// Underscore-only string is valid.
#[test]
fn test_invoice_id_underscore_only_accepted() {
    let env = Env::default();
    assert!(try_init_with_id(&env, "_").is_ok());
}

/// Digits-only string is valid.
#[test]
fn test_invoice_id_digits_only_accepted() {
    let env = Env::default();
    assert!(try_init_with_id(&env, "0123456789").is_ok());
}

// --- charset: illegal characters (parameterized) ---

/// Every character outside [A-Za-z0-9_] must be rejected with the charset panic message.
/// This covers common punctuation, operators, whitespace, and non-ASCII bytes.
#[test]
fn test_invoice_id_illegal_chars_all_rejected() {
    // Characters that are NOT in [A-Za-z0-9_] — representative set covering
    // punctuation, operators, whitespace, and boundary ASCII values.
    let illegal: &[&str] = &[
        "INV-DASH",  // hyphen
        "INV.DOT",   // period
        "INV@AT",    // @
        "INV!BANG",  // !
        "INV#HASH",  // #
        "INV$DOLL",  // $
        "INV%PCT",   // %
        "INV^CARET", // ^
        "INV&AMP",   // &
        "INV*STAR",  // *
        "INV(PAR",   // (
        "INV)PAR",   // )
        "INV+PLUS",  // +
        "INV=EQ",    // =
        "INV[BRK",   // [
        "INV]BRK",   // ]
        "INV{BRC",   // {
        "INV}BRC",   // }
        "INV|PIPE",  // |
        "INV;SEMI",  // ;
        "INV:COL",   // :
        "INV'QUOT",  // '
        "INV,COM",   // ,
        "INV<LT",    // <
        "INV>GT",    // >
        "INV?QM",    // ?
        "INV/SL",    // /
        "INV BAD",   // space
        "INV\tTAB",  // tab
    ];

    for &id in illegal {
        let env = Env::default();
        let result = try_init_with_id(&env, id);
        assert!(
            result.is_err(),
            "expected panic for illegal invoice_id {:?} but init succeeded",
            id
        );
    }
}

/// A single illegal character at the start of an otherwise valid string is caught.
#[test]
fn test_invoice_id_illegal_char_at_start_rejected() {
    let env = Env::default();
    assert!(try_init_with_id(&env, "-LEADING").is_err());
}

/// A single illegal character at the end of an otherwise valid string is caught.
#[test]
fn test_invoice_id_illegal_char_at_end_rejected() {
    let env = Env::default();
    assert!(try_init_with_id(&env, "TRAILING-").is_err());
}

/// A single illegal character in the middle of an otherwise valid string is caught.
#[test]
fn test_invoice_id_illegal_char_in_middle_rejected() {
    let env = Env::default();
    assert!(try_init_with_id(&env, "MID.DLE").is_err());
}

// --- proptest: random valid strings always succeed ---

proptest! {
    /// Any string composed entirely of [A-Za-z0-9_] with length 1..=32 must be accepted.
    #[test]
    fn prop_valid_invoice_id_always_accepted(
        s in "[A-Za-z0-9_]{1,32}"
    ) {
        let env = Env::default();
        prop_assert!(
            try_init_with_id(&env, &s).is_ok(),
            "valid invoice_id {:?} was rejected",
            s
        );
    }

    /// Any string with at least one character outside [A-Za-z0-9_] (length 1..=32) must panic.
    #[test]
    fn prop_invalid_charset_invoice_id_always_rejected(
        // valid prefix + one illegal char + optional valid suffix, total ≤ 32
        prefix in "[A-Za-z0-9_]{0,15}",
        bad_char in "[^A-Za-z0-9_]",
        suffix in "[A-Za-z0-9_]{0,15}",
    ) {
        let combined = format!("{}{}{}", prefix, bad_char, suffix);
        // Only test if the combined string fits within the length limit so we isolate
        // the charset rejection rather than the length rejection.
        prop_assume!(combined.len() >= 1 && combined.len() <= 32);
        let env = Env::default();
        prop_assert!(
            try_init_with_id(&env, &combined).is_err(),
            "expected rejection for invoice_id with illegal char: {:?}",
            combined
        );
    }

    /// Strings longer than MAX_INVOICE_ID_STRING_LEN (32) must always be rejected.
    #[test]
    fn prop_too_long_invoice_id_always_rejected(
        s in "[A-Za-z0-9_]{33,64}"
    ) {
        let env = Env::default();
        prop_assert!(
            try_init_with_id(&env, &s).is_err(),
            "expected rejection for too-long invoice_id (len={}): {:?}",
            s.len(),
            s
        );
    }
}

// ── DataKey default-on-absence verification (docs/escrow-data-model.md) ──────

#[test]
fn datakey_defaults_on_fresh_init() {
    // Verifies that every key documented as "absent ⇒ default" actually returns
    // the documented default on a freshly initialised escrow with no optional
    // configuration supplied.
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    default_init(&client, &env, &admin, &sme);

    // Absent ⇒ false
    assert!(!client.get_legal_hold());
    assert!(!client.is_investor_allowlisted(&investor));
    assert!(!client.is_allowlist_active());
    assert!(!client.is_investor_refunded(&investor));

    // Absent ⇒ 0
    assert_eq!(client.get_contribution(&investor), 0i128);
    assert_eq!(client.get_min_contribution_floor(), 0i128);
    assert_eq!(client.get_unique_funder_count(), 0u32);
    assert_eq!(client.get_distributed_principal(), 0i128);

    // Optional caps absent ⇒ None
    assert!(client.get_max_unique_investors_cap().is_none());
    assert!(client.get_max_per_investor_cap().is_none());

    // FundingCloseSnapshot absent until funded
    assert!(client.get_funding_close_snapshot().is_none());

    // Version written at init
    assert_eq!(client.get_version(), crate::SCHEMA_VERSION);
}

#[test]
fn datakey_distributed_principal_starts_at_zero_and_increments_on_refund() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let investor = Address::generate(&env);
    let token = install_stellar_asset_token(&env);
    let treasury = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "DKTEST1"),
        &sme,
        &1_000i128,
        &0i64,
        &0u64,
        &token.id,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    assert_eq!(client.get_distributed_principal(), 0i128);

    token.stellar.mint(&client.address, &500i128);
    client.fund(&investor, &500i128);
    client.cancel_funding();

    assert_eq!(client.get_distributed_principal(), 0i128);

    client.refund(&investor);
    assert_eq!(client.get_distributed_principal(), 500i128);
}

// ── Maturity bounds (init path) ─────────────────────────────────────────

#[test]
fn test_init_maturity_zero_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MAT00"),
        &sme,
        &1000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_escrow().maturity, 0);
}

#[test]
fn test_init_maturity_within_horizon_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    env.ledger().set_timestamp(1000);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MAT01"),
        &sme,
        &1000i128,
        &800i64,
        &2000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_escrow().maturity, 2000);
}

#[test]
fn test_init_maturity_at_horizon_boundary_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    let now = 1000u64;
    env.ledger().set_timestamp(now);
    let at_boundary = now + DEFAULT_MATURITY_MAX_HORIZON_SECS;
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MAT02"),
        &sme,
        &1000i128,
        &800i64,
        &at_boundary,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    assert_eq!(client.get_escrow().maturity, at_boundary);
}

#[test]
#[should_panic(expected = "MaturityExceedsMaxHorizon")]
fn test_init_maturity_beyond_horizon_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    env.ledger().set_timestamp(1000);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MAT03"),
        &sme,
        &1000i128,
        &800i64,
        &(1000u64 + DEFAULT_MATURITY_MAX_HORIZON_SECS + 1),
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
#[should_panic(expected = "MaturityInPast")]
fn test_init_maturity_in_past_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    env.ledger().set_timestamp(2000);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MAT04"),
        &sme,
        &1000i128,
        &800i64,
        &1000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
}

#[test]
fn test_init_with_custom_horizon_used() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    let short_horizon = 3600u64; // 1 hour
    env.ledger().set_timestamp(1000);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MAT05"),
        &sme,
        &1000i128,
        &800i64,
        &3000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &Some(short_horizon),
    );
    assert_eq!(client.get_maturity_max_horizon(), short_horizon);
    assert_eq!(client.get_escrow().maturity, 3000);
}

// ── Maturity bounds (update_maturity path) ──────────────────────────────

#[test]
fn test_update_maturity_zero_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    env.ledger().set_timestamp(1000);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MATU0"),
        &sme,
        &1000i128,
        &800i64,
        &5000u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    let updated = client.update_maturity(&0u64);
    assert_eq!(updated.maturity, 0);
}

#[test]
fn test_update_maturity_within_horizon_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    env.ledger().set_timestamp(1000);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MATU1"),
        &sme,
        &1000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    let updated = client.update_maturity(&2000u64);
    assert_eq!(updated.maturity, 2000);
}

#[test]
fn test_update_maturity_at_horizon_boundary_accepted() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    let now = 1000u64;
    env.ledger().set_timestamp(now);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MATU2"),
        &sme,
        &1000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    let at_boundary = now + DEFAULT_MATURITY_MAX_HORIZON_SECS;
    let updated = client.update_maturity(&at_boundary);
    assert_eq!(updated.maturity, at_boundary);
}

#[test]
#[should_panic(expected = "MaturityExceedsMaxHorizon")]
fn test_update_maturity_beyond_horizon_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    env.ledger().set_timestamp(1000);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MATU3"),
        &sme,
        &1000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    client.update_maturity(&(1000u64 + DEFAULT_MATURITY_MAX_HORIZON_SECS + 1));
}

#[test]
#[should_panic(expected = "MaturityInPast")]
fn test_update_maturity_in_past_rejected() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    env.ledger().set_timestamp(5000);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MATU4"),
        &sme,
        &1000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    client.update_maturity(&1000u64);
}

// ── update_maturity_max_horizon ─────────────────────────────────────────

#[test]
fn test_update_maturity_max_horizon_by_admin() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    env.ledger().set_timestamp(1000);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "HOR01"),
        &sme,
        &1000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    // Default horizon is DEFAULT_MATURITY_MAX_HORIZON_SECS
    assert_eq!(
        client.get_maturity_max_horizon(),
        DEFAULT_MATURITY_MAX_HORIZON_SECS
    );
    // Update to a shorter horizon
    let new_horizon = 7200u64; // 2 hours
    let result = client.update_maturity_max_horizon(&new_horizon);
    assert_eq!(result, new_horizon);
    assert_eq!(client.get_maturity_max_horizon(), new_horizon);
    // Update_maturity now uses the new tighter horizon
    client.update_maturity(&(1000u64 + 3600u64)); // 1h from now — within 2h horizon
}

#[test]
#[should_panic(expected = "MaturityExceedsMaxHorizon")]
fn test_update_maturity_honors_reduced_horizon() {
    let env = Env::default();
    let (client, admin, sme) = setup(&env);
    let (token, treasury) = free_addresses(&env);
    env.ledger().set_timestamp(1000);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "HOR02"),
        &sme,
        &1000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    client.update_maturity_max_horizon(&3600u64); // 1 hour
    client.update_maturity(&(1000u64 + 7200u64)); // 2 hours — exceeds new 1h horizon
}

// ---------------------------------------------------------------------------
// invoice_id integration tests — typed-error boundaries, charset coverage,
// round-trip storage, and event-payload fidelity.
// ---------------------------------------------------------------------------

/// Helper: attempt init with the given invoice_id string; returns the typed
/// try_ result so callers can assert on specific error codes.
fn try_init_with_id_typed(
    env: &Env,
    id: &str,
) -> Result<
    Result<InvoiceEscrow, soroban_sdk::Error>,
    Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
> {
    env.mock_all_auths();
    let client = deploy(env);
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let (t, tr) = free_addresses(env);
    client.try_init(
        &admin,
        &soroban_sdk::String::from_str(env, id),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &t,
        &None,
        &tr,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    )
}

// ── length: minimum boundary (exactly 1 character) ──────────────────────

/// A single-character invoice_id sits at the minimum boundary and must be
/// accepted without error.
#[test]
fn test_invoice_id_min_length_boundary_accepted() {
    let env = Env::default();
    let result = try_init_with_id_typed(&env, "A");
    assert!(
        result.is_ok() && result.unwrap().is_ok(),
        "invoice_id of length 1 (minimum) must be accepted"
    );
}

/// A 32-character invoice_id sits at the maximum boundary
/// (MAX_INVOICE_ID_STRING_LEN = 32) and must be accepted without error.
#[test]
fn test_invoice_id_max_length_boundary_accepted() {
    let env = Env::default();
    // Exactly 32 chars, all within [A-Za-z0-9_].
    let id = "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345";
    assert_eq!(id.len(), 32, "pre-condition: id must be exactly 32 chars");
    let result = try_init_with_id_typed(&env, id);
    assert!(
        result.is_ok() && result.unwrap().is_ok(),
        "invoice_id of length 32 (maximum) must be accepted"
    );
}

// ── length: InvoiceIdInvalidLength on empty and over-max ────────────────

/// An empty invoice_id must be rejected with `InvoiceIdInvalidLength`.
#[test]
fn test_invoice_id_empty_rejects_with_invalid_length() {
    let env = Env::default();
    let result = try_init_with_id_typed(&env, "");
    assert_contract_error(result, EscrowError::InvoiceIdInvalidLength);
}

/// An invoice_id of 33 characters (one over the maximum) must be rejected
/// with `InvoiceIdInvalidLength`.
#[test]
fn test_invoice_id_over_max_rejects_with_invalid_length() {
    let env = Env::default();
    // 33 chars — all valid charset, so only the length check fires.
    let id = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456";
    assert_eq!(id.len(), 33, "pre-condition: id must be exactly 33 chars");
    let result = try_init_with_id_typed(&env, id);
    assert_contract_error(result, EscrowError::InvoiceIdInvalidLength);
}

// ── charset: all allowed character classes accepted ─────────────────────

/// An invoice_id containing uppercase letters, lowercase letters, digits,
/// and underscores (all four allowed classes) must be accepted.
#[test]
fn test_invoice_id_all_allowed_char_classes_each_accepted() {
    // Upper-only
    let env = Env::default();
    assert!(
        try_init_with_id_typed(&env, "UPPER").is_ok_and(|r| r.is_ok()),
        "uppercase-only id must be accepted"
    );

    // Lower-only
    let env = Env::default();
    assert!(
        try_init_with_id_typed(&env, "lower").is_ok_and(|r| r.is_ok()),
        "lowercase-only id must be accepted"
    );

    // Digits-only
    let env = Env::default();
    assert!(
        try_init_with_id_typed(&env, "0123456789").is_ok_and(|r| r.is_ok()),
        "digits-only id must be accepted"
    );

    // Underscore-only
    let env = Env::default();
    assert!(
        try_init_with_id_typed(&env, "_").is_ok_and(|r| r.is_ok()),
        "underscore-only id must be accepted"
    );

    // Mixed — one character from each class
    let env = Env::default();
    assert!(
        try_init_with_id_typed(&env, "Aa0_").is_ok_and(|r| r.is_ok()),
        "mixed valid chars id must be accepted"
    );
}

// ── charset: InvoiceIdInvalidCharset on disallowed characters ────────────

/// A space character anywhere in the invoice_id must be rejected with
/// `InvoiceIdInvalidCharset`.
#[test]
fn test_invoice_id_space_rejects_with_invalid_charset() {
    let env = Env::default();
    let result = try_init_with_id_typed(&env, "INV SPACE");
    assert_contract_error(result, EscrowError::InvoiceIdInvalidCharset);
}

/// A hyphen (`-`) in the invoice_id must be rejected with
/// `InvoiceIdInvalidCharset`.
#[test]
fn test_invoice_id_hyphen_rejects_with_invalid_charset() {
    let env = Env::default();
    let result = try_init_with_id_typed(&env, "INV-HYPHEN");
    assert_contract_error(result, EscrowError::InvoiceIdInvalidCharset);
}

/// A period (`.`) in the invoice_id must be rejected with
/// `InvoiceIdInvalidCharset`.
#[test]
fn test_invoice_id_period_rejects_with_invalid_charset() {
    let env = Env::default();
    let result = try_init_with_id_typed(&env, "INV.DOT");
    assert_contract_error(result, EscrowError::InvoiceIdInvalidCharset);
}

/// An `@` symbol in the invoice_id must be rejected with
/// `InvoiceIdInvalidCharset`.
#[test]
fn test_invoice_id_at_symbol_rejects_with_invalid_charset() {
    let env = Env::default();
    let result = try_init_with_id_typed(&env, "INV@AT");
    assert_contract_error(result, EscrowError::InvoiceIdInvalidCharset);
}

/// A tab character in the invoice_id must be rejected with
/// `InvoiceIdInvalidCharset`.
#[test]
fn test_invoice_id_tab_rejects_with_invalid_charset() {
    let env = Env::default();
    let result = try_init_with_id_typed(&env, "INV\tTAB");
    assert_contract_error(result, EscrowError::InvoiceIdInvalidCharset);
}

/// A `+` character in the invoice_id must be rejected with
/// `InvoiceIdInvalidCharset`.
#[test]
fn test_invoice_id_plus_rejects_with_invalid_charset() {
    let env = Env::default();
    let result = try_init_with_id_typed(&env, "INV+PLUS");
    assert_contract_error(result, EscrowError::InvoiceIdInvalidCharset);
}

/// A slash (`/`) in the invoice_id must be rejected with
/// `InvoiceIdInvalidCharset`.
#[test]
fn test_invoice_id_slash_rejects_with_invalid_charset() {
    let env = Env::default();
    let result = try_init_with_id_typed(&env, "INV/SLASH");
    assert_contract_error(result, EscrowError::InvoiceIdInvalidCharset);
}

/// An illegal character at the very start of an otherwise valid string
/// must still be caught and rejected with `InvoiceIdInvalidCharset`.
#[test]
fn test_invoice_id_illegal_char_at_position_zero_rejects_with_invalid_charset() {
    let env = Env::default();
    let result = try_init_with_id_typed(&env, "!LEADING");
    assert_contract_error(result, EscrowError::InvoiceIdInvalidCharset);
}

/// An illegal character at the very end of an otherwise valid string
/// must still be caught and rejected with `InvoiceIdInvalidCharset`.
#[test]
fn test_invoice_id_illegal_char_at_last_position_rejects_with_invalid_charset() {
    let env = Env::default();
    let result = try_init_with_id_typed(&env, "TRAILING!");
    assert_contract_error(result, EscrowError::InvoiceIdInvalidCharset);
}

/// An illegal character in the interior of an otherwise valid string
/// must still be caught and rejected with `InvoiceIdInvalidCharset`.
#[test]
fn test_invoice_id_illegal_char_in_interior_rejects_with_invalid_charset() {
    let env = Env::default();
    let result = try_init_with_id_typed(&env, "MID!DLE");
    assert_contract_error(result, EscrowError::InvoiceIdInvalidCharset);
}

// ── round-trip: stored invoice_id matches the value fetched via get_escrow ──

/// The `invoice_id` stored during `init` must round-trip perfectly when the
/// escrow is later fetched via `get_escrow`.  The stored `Symbol` value must
/// equal the `Symbol` constructed from the same string.
#[test]
fn test_invoice_id_roundtrips_via_get_escrow_at_min_length() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);

    // Minimum length: 1 character.
    let id = "Z";
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, id),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let escrow = client.get_escrow();
    assert_eq!(
        escrow.invoice_id,
        Symbol::new(&env, id),
        "invoice_id fetched via get_escrow must match the value supplied at init (min length)"
    );
}

/// The `invoice_id` stored during `init` must round-trip perfectly when the
/// escrow is later fetched via `get_escrow`, for an id at the maximum allowed
/// length (32 characters).
#[test]
fn test_invoice_id_roundtrips_via_get_escrow_at_max_length() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);

    // Maximum length: 32 characters, all allowed classes present.
    let id = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcd01";
    assert_eq!(id.len(), 32, "pre-condition: id must be exactly 32 chars");
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, id),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let escrow = client.get_escrow();
    assert_eq!(
        escrow.invoice_id,
        Symbol::new(&env, id),
        "invoice_id fetched via get_escrow must match the value supplied at init (max length)"
    );
}

/// The value returned by `init` and the value returned by a subsequent
/// `get_escrow` call must agree on `invoice_id`.
#[test]
fn test_invoice_id_init_return_value_matches_get_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let (token, treasury) = free_addresses(&env);

    let id = "INV_RT_001";
    let returned = client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, id),
        &sme,
        &50_000i128,
        &1_000i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );
    let fetched = client.get_escrow();

    assert_eq!(
        returned.invoice_id, fetched.invoice_id,
        "invoice_id in init return value must equal invoice_id in get_escrow"
    );
    assert_eq!(
        returned.invoice_id,
        Symbol::new(&env, id),
        "invoice_id must equal Symbol constructed from the original string"
    );
    // The entire escrow structs must also be equal.
    assert_eq!(
        returned, fetched,
        "InvoiceEscrow returned by init must equal the one stored and returned by get_escrow"
    );
}

// ── event payload: invoice_id in EscrowInitialized matches get_escrow ───

/// After a successful `init`, the `EscrowInitialized` event must be emitted
/// exactly once and its embedded `escrow.invoice_id` must match the value
/// returned by `get_escrow`.
#[test]
fn test_invoice_id_matches_escrow_initialized_event_payload() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let contract_id = client.address.clone();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);

    let id = "EVT_CHECK_1";
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, id),
        &sme,
        &5_000i128,
        &200i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let escrow = client.get_escrow();

    // Exactly one event must be emitted.
    let all_events = env.events().all();
    assert_eq!(
        all_events.events().len(),
        1,
        "exactly one event must be emitted by init"
    );

    // The event must match the EscrowInitialized structure with the
    // correct invoice_id inside the embedded escrow.
    let expected = EscrowInitialized {
        name: symbol_short!("escrow_ii"),
        escrow: escrow.clone(),
        funding_token: token,
        treasury,
        registry: None,
        has_maturity_lock: false,
    };
    assert_eq!(
        all_events,
        std::vec![expected.to_xdr(&env, &contract_id)],
        "EscrowInitialized event payload must embed the stored escrow including invoice_id"
    );

    // The invoice_id inside the event payload's escrow must equal the
    // Symbol built from the original string.
    assert_eq!(
        escrow.invoice_id,
        Symbol::new(&env, id),
        "event escrow.invoice_id must round-trip to the original string"
    );
}

/// Variant: init with a registry present; the event payload's embedded
/// `escrow.invoice_id` must still match `get_escrow` and the original string.
#[test]
fn test_invoice_id_matches_event_payload_with_registry_present() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let contract_id = client.address.clone();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let registry = Address::generate(&env);

    let id = "EVT_REG_001";
    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, id),
        &sme,
        &7_500i128,
        &300i64,
        &1_000u64,
        &token,
        &Some(registry.clone()),
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    let escrow = client.get_escrow();

    let all_events = env.events().all();
    assert_eq!(
        all_events.events().len(),
        1,
        "exactly one event must be emitted by init"
    );

    let expected = EscrowInitialized {
        name: symbol_short!("escrow_ii"),
        escrow: escrow.clone(),
        funding_token: token,
        treasury,
        registry: Some(registry),
        has_maturity_lock: true,
    };
    assert_eq!(
        all_events,
        std::vec![expected.to_xdr(&env, &contract_id)],
        "EscrowInitialized event payload must match stored escrow when registry is present"
    );

    assert_eq!(
        escrow.invoice_id,
        Symbol::new(&env, id),
        "event escrow.invoice_id must round-trip to the original string (registry present)"
    );
}
