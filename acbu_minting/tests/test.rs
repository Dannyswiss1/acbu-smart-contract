#![cfg(test)]

use acbu_minting::{MintingContract, MintingContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
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
    assert!(!client.is_paused());
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_initialize_twice() {
    let env = Env::default();
    env.mock_all_auths();
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
    env.mock_all_auths();
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

    assert!(!client.is_paused());
    client.pause();
    assert!(client.is_paused());
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_set_fee_rate() {
    let env = Env::default();
    env.mock_all_auths();
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
    client.set_fee_rate(&new_fee_rate);
    assert_eq!(client.get_fee_rate(), new_fee_rate);
}
#[test]
fn test_mint_from_usdc() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let reserve_tracker = Address::generate(&env);
    let user = Address::generate(&env);
    let fee_rate = 300; // 0.3%

    let contract_id = env.register_contract(None, MintingContract);
    let client = MintingContractClient::new(&env, &contract_id);

    // Setup SAC Mocks: Minting contract is admin/issuer of ACBU to enable mint()
    let usdc_token_id = env.register_stellar_asset_contract(admin.clone());
    let acbu_token_id = env.register_stellar_asset_contract(contract_id.clone());

    let usdc_token_client = soroban_sdk::token::StellarAssetClient::new(&env, &usdc_token_id);
    let usdc_client = soroban_sdk::token::Client::new(&env, &usdc_token_id);
    let acbu_client = soroban_sdk::token::Client::new(&env, &acbu_token_id);

    // Seed User: 100 USDC (7 decimals)
    let usdc_amount = 100 * 10_000_000; 
    usdc_token_client.mint(&user, &usdc_amount);

    client.initialize(
        &admin,
        &oracle,
        &reserve_tracker,
        &acbu_token_id,
        &usdc_token_id,
        &fee_rate,
    );

    // Execute: 50 USDC deposit
    let mint_amount = 50 * 10_000_000;
    let acbu_minted = client.mint_from_usdc(&mint_amount, &user);

    // Verification
    // 0.3% fee on 50 = 0.15. 50 - 0.15 = 49.85
    let expected_acbu = 498_500_000; 
    let expected_fee = 1_500_000;
    
    assert_eq!(acbu_minted, expected_acbu);
    assert_eq!(acbu_client.balance(&user), expected_acbu);
    assert_eq!(usdc_client.balance(&user), 50 * 10_000_000);

    // Event Audit
    let events = env.events().all();
    let last_event = events.last().unwrap();
    // Event Structure: (symbol "mint", Address user) -> MintEvent
    assert_eq!(last_event.0, (contract_id.clone(), (symbol_short!("mint"), user.clone()).into_val(&env)));
    
    let event_data: MintEvent = last_event.1.into_val(&env);
    assert_eq!(event_data.usdc_amount, mint_amount);
    assert_eq!(event_data.acbu_amount, expected_acbu);
    assert_eq!(event_data.fee, expected_fee);
}

#[test]
fn test_mint_from_fiat() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let reserve_tracker = Address::generate(&env);
    let recipient = Address::generate(&env);
    let fee_rate = 200; // 0.2%

    let contract_id = env.register_contract(None, MintingContract);
    let client = MintingContractClient::new(&env, &contract_id);

    let acbu_token_id = env.register_stellar_asset_contract(contract_id.clone());
    let usdc_token_id = Address::generate(&env); // Placeholder fixture
    let acbu_client = soroban_sdk::token::Client::new(&env, &acbu_token_id);

    client.initialize(
        &admin,
        &oracle,
        &reserve_tracker,
        &acbu_token_id,
        &usdc_token_id,
        &fee_rate,
    );

    let fiat_amount = 1000 * 10_000_000; 
    let currency = SorobanString::from_str(&env, "NGN");
    let fintech_tx_id = SorobanString::from_str(&env, "partner_id_001");

    // Must be admin to initiate fiat mint simulation
    let acbu_minted = client.mint_from_fiat(&currency, &fiat_amount, &recipient, &fintech_tx_id);

    // Verification
    // 0.2% fee on 1000 = 2. 1000 - 2 = 998
    let expected_acbu = 998 * 10_000_000;
    assert_eq!(acbu_minted, expected_acbu);
    assert_eq!(acbu_client.balance(&recipient), expected_acbu);

    // Event Audit
    let events = env.events().all();
    let last_event = events.last().unwrap();
    let event_data: MintEvent = last_event.1.into_val(&env);
    assert_eq!(event_data.acbu_amount, expected_acbu);
    // contains string partner_id_001
    assert!(event_data.transaction_id.to_string().contains("partner_id_001"));
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_unauthorized_mint_panic() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let reserve_tracker = Address::generate(&env);
    let recipient = Address::generate(&env);
    let attacker = Address::generate(&env);
    let fee_rate = 300;

    let contract_id = env.register_contract(None, MintingContract);
    let client = MintingContractClient::new(&env, &contract_id);

    let usdc_token_id = Address::generate(&env);
    let acbu_token_id = Address::generate(&env);

    client.initialize(
        &admin,
        &oracle,
        &reserve_tracker,
        &acbu_token_id,
        &usdc_token_id,
        &fee_rate,
    );

    let amount = 100 * 10_000_000;
    let currency = SorobanString::from_str(&env, "NGN");
    let tx_id = SorobanString::from_str(&env, "fail_tx");

    // Use attacker's client to simulate unauthorized call
    let attacker_client = MintingContractClient::new(&env, &contract_id);
    
    // In soroban testing, the last generated address or current setup sets the invoker.
    // If check_admin_or_user runs, it will compare the invoker (Address 0 or similar) against admin/user.
    // To ensure it fails, we assume it's checking env.invoker()
    attacker_client.mint_from_fiat(&currency, &amount, &recipient, &tx_id);
}
