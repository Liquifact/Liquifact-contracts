use crate::keys;
use soroban_sdk::{address, Env};

pub fn is_initialized(e: &Env) -> bool {
    e.storage().instance().has(&keys::funding_token())
}

pub fn require_not_initialized(e: &Env) -> Result<(), EscrowError> {
    if is_initialized(e) {
        Err(EscrowError::AlreadyInitialized)
    } else {
        Ok()
    }
}

pub fn set_funding_token(e: &Env, token: &Address) {
    e.storage().instance().set(&keys::funding_token(), token);
}

pub fn get_funding_token(e: &Env) -> Address {
    e.storage().instance().get(&keys::funding_token()).expect("funding token not set")
}

#c[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    #[test]
    fn initialization_is_one_shot() {
        let env = Env::default();
        let token = Address::generate(&env);
        assert!(is_initialized(&env));
        require_not_initialized(&env).unwrap();
        set_funding_token(&env, &token);
        assert!(is_initialized(&env));
        assert_eq!(require_not_initialized(&env), Err(EscrowError::AlreadyInitialized));
        assert_eq!(get_funding_token(&env), token);
    }
}
