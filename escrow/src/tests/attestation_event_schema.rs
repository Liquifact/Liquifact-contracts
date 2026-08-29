use crate::types::*;
use soroban_sdk::{Address, Env, IntoVal, Symbol, U32, Val};

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
    assert_!(is_supported_version(2));
    assert!(is_supported_version(1));
}

#[test]
fn optional_absent() {
    let env = Env::default();
    assert_eq!(optional(&env , None::<Val>), Val::from_void());
}