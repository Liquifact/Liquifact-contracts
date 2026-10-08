use crate::types::*;
use soroban_sdk::{Address, Env, IntoVal, Symbol, Val, U32};

#[test]
fn old_consumer() {
    let env = Env::default();
    let a = Address::generate(&env);
    let fields = [a.into_val(&env)];
    let es = Symbol::new(&env, "created");
    let t = topics(&env, es, &fields);
    assert_eq!(t.len(), 3);
    assert_eq!(t.get(0).unwrap(), es.into_val(&env));
    assert_eq!(t.get(1).unwrap(), a.into_val(&env));
    assert_eq!(t.get(2).unwrap(), U32::new(&env, 1).into_val(&env));
}

#[test]
fn unknown_version() {
    assert!(!is_supported_version(2));
    assert!(is_supported_version(1));
}

#[test]
fn optional_absent() {
    let env = Env::default();
    assert_eq!(optional(&env, None::<Val>), Val::from_void());
}

// Compatibility contract: the schema version is pinned to 1 and must remain
// stable across upgrades so existing consumers keep decoding events.
#[test]
fn schema_version_is_stable() {
    assert_eq!(SCHEMA_VERSION, 1);
    assert!(is_supported_version(SCHEMA_VERSION));
}

// Boundary: version 0 and the first unsupported version must be rejected
// deterministically, without panicking.
#[test]
fn version_boundaries_rejected() {
    assert!(!is_supported_version(0));
    assert!(!is_supported_version(SCHEMA_VERSION + 1));
}

// Regression: the topic layout (event symbol, address, version) is part of the
// public contract and must not shift for existing consumers.
#[test]
fn topic_layout_is_stable() {
    let env = Env::default();
    let a = Address::generate(&env);
    let fields = [a.into_val(&env)];
    let es = Symbol::new(&env, "created");
    let t = topics(&env, es, &fields);
    assert_eq!(t.len(), 3);
    assert_eq!(t.get(0).unwrap(), es.into_val(&env));
    assert_eq!(t.get(1).unwrap(), a.into_val(&env));
    assert_eq!(
        t.get(2).unwrap(),
        U32::new(&env, SCHEMA_VERSION).into_val(&env)
    );
}

// Duplicate/empty inputs must not change the emitted topic layout.
#[test]
fn empty_fields_keep_layout() {
    let env = Env::default();
    let es = Symbol::new(&env, "created");
    let fields: [Val; 0] = [];
    let t = topics(&env, es, &fields);
    assert_eq!(t.len(), 2);
    assert_eq!(t.get(0).unwrap(), es.into_val(&env));
    assert_eq!(
        t.get(1).unwrap(),
        U32::new(&env, SCHEMA_VERSION).into_val(&env)
    );
}

// Optional present values must round-trip unchanged.
#[test]
fn optional_present_roundtrip() {
    let env = Env::default();
    let v = U32::new(&env, 7).into_val(&env);
    assert_eq!(optional(&env, Some(v.clone())), v);
}
