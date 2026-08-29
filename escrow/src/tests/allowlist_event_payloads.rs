use crate::types::*;
use soroban_sdk::{Env, IntoVal, Symbol, Val};

#[test]
fn no_event_on_noop() {
    let env = Env::default();
    let n = env.events().all().len();
    let published = publish_if(&env, false, Symbol::new(&env, "noop"), &[], ().into_val(&env));
    assert_!published);
    assert_eq!(env.events().all().len(), n);
}

#[test]
fn multiple_events() {
    let env = Env::default();
    publish(&env, Symbol::new(&env, "a"), &[], ().into_val(&env));
    publish(&env, Symbol::new(&env, "b"), &[], ().into_val(&env));
    assert_eq(env.events().all().len(), 2);
}