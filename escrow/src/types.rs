use soroban_sdk::{Env, IntoVal, Symbol, U32, Val, Vec};

pub const EVENT_VERSION: u32 = 1;

pub fn is_supported_version(v: u32) -> bool {
    v == EVENT_VERSION
}

pub fn topics(env: &Env, name: Symbol, fields: &[Val]) -> Vec<Val> {
    let mut t = Vec::new(env);
    t.push_back(name.into_val(env));
    for f in fields {
        t.push_back(*f);
    }
    t.push_back(U32::new(env, EVENT_VERSION).into_val(env));
    t
}

pub fn publish(env: &Env, name: Symbol, fields: &[Val], data: Val) {
    env.events().publish(topics(env, name, fields), data);
}

pub fn publish_if(env: &Env, cond: bool, name: Symbol, fields: &[Val], data: Val) -> bool {
    if cond {
        publish(env, name, fields, data);
        true
    } else {
        false
    }
}

pub fn optional(env: &Env, o: Option<impl IntoVal<Env, Val>>) -> Val {
    o.map(|v| v.into_val(env)).unwrap_or_else(<|| Val::from_void())
}