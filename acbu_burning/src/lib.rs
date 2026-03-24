#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String as SorobanString,
    Symbol, Vec,
};

use shared::{
    calculate_amount_after_fee, calculate_fee, AccountDetails, BurnEvent, CurrencyCode,
    MIN_BURN_AMOUNT, BASIS_POINTS, DECIMALS,
};

mod shared {
    pub use shared::*;
}

mod token {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32-unknown-unknown/release/soroban_token_contract.wasm",
        sha256 = "0x0000000000000000000000000000000000000000000000000000000000000000"
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataKey {
    pub admin: Symbol,
    pub oracle: Symbol,
    pub reserve_tracker: Symbol,
    pub acbu_token: Symbol,
    pub withdrawal_processor: Symbol,
    pub fee_rate: Symbol,
    pub paused: Symbol,
    pub min_burn_amount: Symbol,
}

const DATA_KEY: DataKey = DataKey {
    admin: symbol_short!("ADMIN"),
    oracle: symbol_short!("ORACLE"),
    reserve_tracker: symbol_short!("RES_TRK"),
    acbu_token: symbol_short!("ACBU_TKN"),
    withdrawal_processor: symbol_short!("WD_PROC"),
    fee_rate: symbol_short!("FEE_RATE"),
    paused: symbol_short!("PAUSED"),
    min_burn_amount: symbol_short!("MIN_BURN"),
};

#[contract]
pub struct BurningContract;

#[contractimpl]
impl BurningContract {
    /// Initialize the burning contract
    pub fn initialize(
        env: Env,
        admin: Address,
        oracle: Address,
        reserve_tracker: Address,
        acbu_token: Address,
        withdrawal_processor: Address,
        fee_rate_bps: i128,
    ) {
        // Check if already initialized
        if env.storage().instance().has(&DATA_KEY.admin) {
            panic!("Contract already initialized");
        }

        // Validate inputs
        if fee_rate_bps < 0 || fee_rate_bps > BASIS_POINTS {
            panic!("Invalid fee rate");
        }

        // Store configuration
        env.storage().instance().set(&DATA_KEY.admin, &admin);
        env.storage().instance().set(&DATA_KEY.oracle, &oracle);
        env.storage().instance().set(&DATA_KEY.reserve_tracker, &reserve_tracker);
        env.storage().instance().set(&DATA_KEY.acbu_token, &acbu_token);
        env.storage().instance().set(&DATA_KEY.withdrawal_processor, &withdrawal_processor);
        env.storage().instance().set(&DATA_KEY.fee_rate, &fee_rate_bps);
        env.storage().instance().set(&DATA_KEY.paused, &false);
        env.storage().instance().set(&DATA_KEY.min_burn_amount, &MIN_BURN_AMOUNT);
    }

    /// Burn ACBU for single currency redemption
    pub fn burn_for_currency(
        env: Env,
        acbu_amount: i128,
        currency: SorobanString,
        recipient_account: AccountDetails,
    ) -> i128 {
        Self::check_paused(&env);
        let caller = env.invoker();

        // Validate amount
        let min_amount = env.storage().instance().get(&DATA_KEY.min_burn_amount).unwrap();
        if acbu_amount < min_amount {
            panic!("Invalid burn amount");
        }

        // Get contract addresses
        let oracle = env.storage().instance().get(&DATA_KEY.oracle).unwrap();
        let reserve_tracker = env.storage().instance().get(&DATA_KEY.reserve_tracker).unwrap();
        let acbu_token = env.storage().instance().get(&DATA_KEY.acbu_token).unwrap();
        let fee_rate = env.storage().instance().get(&DATA_KEY.fee_rate).unwrap();

        // Get currency/USD rate from oracle
        // Note: In production, this would call the oracle contract
        let currency_code = CurrencyCode(currency.clone());
        let currency_rate = DECIMALS; // 1:1 with USD initially

        // Verify reserve availability
        // Note: In production, this would call the reserve tracker contract
        // For now, we'll skip the check

        // Calculate local currency amount
        // ACBU amount -> USD value -> Local currency
        let acbu_after_fee = calculate_amount_after_fee(acbu_amount, fee_rate);
        let usd_value = (acbu_after_fee * DECIMALS) / DECIMALS; // Assuming 1:1 ACBU:USD
        let local_amount = (usd_value * DECIMALS) / currency_rate;

        // Burn ACBU from caller
        let acbu_client = token::Client::new(&env, &acbu_token);
        acbu_client.burn(&caller, &acbu_amount);

        // Calculate fee
        let fee = calculate_fee(acbu_amount, fee_rate);

        // Emit BurnEvent
        let tx_id = SorobanString::from_str(&format!("burn_{}", env.ledger().sequence()));
        let burn_event = BurnEvent {
            transaction_id: tx_id,
            user: caller.clone(),
            acbu_amount,
            local_amount,
            currency: currency_code,
            fee,
            rate: currency_rate,
            timestamp: env.ledger().timestamp(),
        };
        env.events().publish((symbol_short!("burn"), caller), burn_event);

        local_amount
    }

    /// Burn ACBU for basket redemption (proportional)
    pub fn burn_for_basket(
        env: Env,
        acbu_amount: i128,
        recipient_accounts: Vec<AccountDetails>,
    ) -> Vec<i128> {
        Self::check_paused(&env);
        let caller = env.invoker();

        // Validate amount
        let min_amount = env.storage().instance().get(&DATA_KEY.min_burn_amount).unwrap();
        if acbu_amount < min_amount {
            panic!("Invalid burn amount");
        }

        if recipient_accounts.is_empty() {
            panic!("No recipient accounts provided");
        }

        // Get contract addresses
        let acbu_token = env.storage().instance().get(&DATA_KEY.acbu_token).unwrap();
        let fee_rate = env.storage().instance().get(&DATA_KEY.fee_rate).unwrap();

        // Calculate amounts per currency based on basket weights
        // For MVP: Equal distribution
        let acbu_after_fee = calculate_amount_after_fee(acbu_amount, fee_rate);
        let amount_per_account = acbu_after_fee / (recipient_accounts.len() as i128);

        // Burn ACBU from caller
        let acbu_client = token::Client::new(&env, &acbu_token);
        acbu_client.burn(&caller, &acbu_amount);

        // Calculate local amounts for each currency
        let mut local_amounts = Vec::new(&env);
        for account in recipient_accounts.iter() {
            // Get currency rate
            let currency_rate = DECIMALS; // 1:1 with USD initially
            let usd_value = (amount_per_account * DECIMALS) / DECIMALS;
            let local_amount = (usd_value * DECIMALS) / currency_rate;
            local_amounts.push_back(local_amount);

            // Emit BurnEvent for each currency
            let tx_id = SorobanString::from_str(&format!(
                "burn_basket_{}_{}",
                env.ledger().sequence(),
                account.currency.0
            ));
            let burn_event = BurnEvent {
                transaction_id: tx_id,
                user: caller.clone(),
                acbu_amount: amount_per_account,
                local_amount,
                currency: account.currency.clone(),
                fee: calculate_fee(amount_per_account, fee_rate),
                rate: currency_rate,
                timestamp: env.ledger().timestamp(),
            };
            env.events().publish((symbol_short!("burn"), caller.clone()), burn_event);
        }

        local_amounts
    }

    /// Pause the contract (admin only)
    pub fn pause(env: Env) {
        Self::check_admin(&env);
        env.storage().instance().set(&DATA_KEY.paused, &true);
    }

    /// Unpause the contract (admin only)
    pub fn unpause(env: Env) {
        Self::check_admin(&env);
        env.storage().instance().set(&DATA_KEY.paused, &false);
    }

    /// Set fee rate (admin only)
    pub fn set_fee_rate(env: Env, fee_rate_bps: i128) {
        Self::check_admin(&env);
        if fee_rate_bps < 0 || fee_rate_bps > BASIS_POINTS {
            panic!("Invalid fee rate");
        }
        env.storage().instance().set(&DATA_KEY.fee_rate, &fee_rate_bps);
    }

    /// Get current fee rate
    pub fn get_fee_rate(env: Env) -> i128 {
        env.storage().instance().get(&DATA_KEY.fee_rate).unwrap()
    }

    /// Check if contract is paused
    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&DATA_KEY.paused).unwrap_or(false)
    }

    // Private helper functions
    fn check_paused(env: &Env) {
        let paused: bool = env
            .storage()
            .instance()
            .get(&DATA_KEY.paused)
            .unwrap_or(false);
        if paused {
            panic!("Contract is paused");
        }
    }

    fn check_admin(env: &Env) {
        let admin: Address = env.storage().instance().get(&DATA_KEY.admin).unwrap();
        if admin != env.invoker() {
            panic!("Unauthorized: admin only");
        }
    }
}
