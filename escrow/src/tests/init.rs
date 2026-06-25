use super::*;
use crate::EscrowInitialized;
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
