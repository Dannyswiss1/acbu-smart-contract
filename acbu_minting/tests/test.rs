#![cfg(test)]

use acbu_minting::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String as SorobanString};

#[test]
fn test_initialize() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let reserve_tracker = Address::generate(&env);
    let acbu_token = Address::generate(&env);
    let usdc_token = Address::generate(&env);
    let fee_rate = 300; // 0.3%

    let contract_id = env.register_contract(None, MintingContract);
    let client = MintingContractClient::new(&env, &contract_id);

    client.initialize(
        &admin,
        &oracle,
        &reserve_tracker,
        &acbu_token,
        &usdc_token,
        &fee_rate,
    );

    assert_eq!(client.get_fee_rate(), fee_rate);
    assert_eq!(client.is_paused(), false);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_initialize_twice() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let reserve_tracker = Address::generate(&env);
    let acbu_token = Address::generate(&env);
    let usdc_token = Address::generate(&env);
    let fee_rate = 300;

    let contract_id = env.register_contract(None, MintingContract);
    let client = MintingContractClient::new(&env, &contract_id);

    client.initialize(
        &admin,
        &oracle,
        &reserve_tracker,
        &acbu_token,
        &usdc_token,
        &fee_rate,
    );

    // Try to initialize again
    client.initialize(
        &admin,
        &oracle,
        &reserve_tracker,
        &acbu_token,
        &usdc_token,
        &fee_rate,
    );
}

#[test]
fn test_pause_unpause() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let reserve_tracker = Address::generate(&env);
    let acbu_token = Address::generate(&env);
    let usdc_token = Address::generate(&env);
    let fee_rate = 300;

    let contract_id = env.register_contract(None, MintingContract);
    let client = MintingContractClient::new(&env, &contract_id);

    client.initialize(
        &admin,
        &oracle,
        &reserve_tracker,
        &acbu_token,
        &usdc_token,
        &fee_rate,
    );

    assert_eq!(client.is_paused(), false);

    env.mock_all_auths();
    client.pause();
    assert_eq!(client.is_paused(), true);

    env.mock_all_auths();
    client.unpause();
    assert_eq!(client.is_paused(), false);
}

#[test]
fn test_set_fee_rate() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let reserve_tracker = Address::generate(&env);
    let acbu_token = Address::generate(&env);
    let usdc_token = Address::generate(&env);
    let fee_rate = 300;

    let contract_id = env.register_contract(None, MintingContract);
    let client = MintingContractClient::new(&env, &contract_id);

    client.initialize(
        &admin,
        &oracle,
        &reserve_tracker,
        &acbu_token,
        &usdc_token,
        &fee_rate,
    );

    let new_fee_rate = 500; // 0.5%
    env.mock_all_auths();
    client.set_fee_rate(&new_fee_rate);
    assert_eq!(client.get_fee_rate(), new_fee_rate);
}

// --- mint_from_fiat security tests ---

fn setup_client(env: &Env) -> (MintingContractClient, Address) {
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    let reserve_tracker = Address::generate(env);
    let acbu_token = Address::generate(env);
    let usdc_token = Address::generate(env);

    let contract_id = env.register_contract(None, MintingContract);
    let client = MintingContractClient::new(env, &contract_id);
    client.initialize(&admin, &oracle, &reserve_tracker, &acbu_token, &usdc_token, &300);
    (client, admin)
}

/// Returns (client, admin, acbu_token_address, usdc_token_address) with real SAC tokens
/// registered so cross-contract mint/transfer calls succeed in tests.
fn setup_client_with_tokens(env: &Env) -> (MintingContractClient, Address, Address, Address) {
    use soroban_sdk::testutils::Address as _;
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    let reserve_tracker = Address::generate(env);

    let acbu_token = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let usdc_token = env.register_stellar_asset_contract_v2(admin.clone()).address();

    let contract_id = env.register_contract(None, MintingContract);
    let client = MintingContractClient::new(env, &contract_id);
    client.initialize(&admin, &oracle, &reserve_tracker, &acbu_token, &usdc_token, &300);
    (client, admin, acbu_token, usdc_token)
}

#[test]
#[should_panic]
fn test_mint_from_fiat_recipient_cannot_self_mint() {
    // A non-admin address must not be able to trigger fiat-backed minting for themselves.
    // The contract requires admin authorization (require_auth) which will be absent for a
    // random, non-admin caller even when mock_all_auths is NOT used.
    let env = Env::default();
    let (client, _admin) = setup_client(&env);
    let recipient = Address::generate(&env);

    // No mock_all_auths: admin.require_auth() will fail because admin is not authorising.
    client.mint_from_fiat(
        &SorobanString::from_str(&env, "NGN"),
        &100_000_000_i128,
        &recipient,
        &SorobanString::from_str(&env, "FTX-001"),
    );
}

#[test]
#[should_panic(expected = "Invalid fintech_tx_id")]
fn test_mint_from_fiat_rejects_empty_tx_id() {
    let env = Env::default();
    let (client, admin) = setup_client(&env);
    let recipient = Address::generate(&env);

    // Mock admin authorization so we get past the auth check and reach input validation.
    env.mock_all_auths();
    client.mint_from_fiat(
        &SorobanString::from_str(&env, "NGN"),
        &100_000_000_i128,
        &recipient,
        &SorobanString::from_str(&env, ""), // empty tx_id
    );
    let _ = admin; // suppress unused warning
}

#[test]
#[should_panic(expected = "Invalid currency")]
fn test_mint_from_fiat_rejects_empty_currency() {
    let env = Env::default();
    let (client, admin) = setup_client(&env);
    let recipient = Address::generate(&env);

    env.mock_all_auths();
    client.mint_from_fiat(
        &SorobanString::from_str(&env, ""), // empty currency
        &100_000_000_i128,
        &recipient,
        &SorobanString::from_str(&env, "FTX-001"),
    );
    let _ = admin;
}

#[test]
#[should_panic(expected = "Transaction ID already processed")]
fn test_mint_from_fiat_rejects_duplicate_tx_id() {
    // The same fintech_tx_id must never produce a second mint (replay attack).
    let env = Env::default();
    let (client, admin, _acbu, _usdc) = setup_client_with_tokens(&env);
    let recipient = Address::generate(&env);

    // Allow all auths so the first call (including token mint) succeeds.
    env.mock_all_auths();

    client.mint_from_fiat(
        &SorobanString::from_str(&env, "NGN"),
        &100_000_000_i128,
        &recipient,
        &SorobanString::from_str(&env, "FTX-001"),
    );

    // Second call with the same fintech_tx_id must be rejected even with valid auth.
    client.mint_from_fiat(
        &SorobanString::from_str(&env, "NGN"),
        &100_000_000_i128,
        &recipient,
        &SorobanString::from_str(&env, "FTX-001"),
    );
    let _ = admin;
}
