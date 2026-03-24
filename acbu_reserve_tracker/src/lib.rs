#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, Symbol,
};

use shared::{CurrencyCode, ReserveData, DECIMALS, BASIS_POINTS};

mod shared {
    pub use shared::*;
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataKey {
    pub admin: Symbol,
    pub acbu_token: Symbol,
    pub reserves: Symbol,
    pub min_ratio: Symbol,
    pub target_ratio: Symbol,
}

const DATA_KEY: DataKey = DataKey {
    admin: symbol_short!("ADMIN"),
    acbu_token: symbol_short!("ACBU_TKN"),
    reserves: symbol_short!("RESERVES"),
    min_ratio: symbol_short!("MIN_RATIO"),
    target_ratio: symbol_short!("TGT_RATIO"),
};

#[contract]
pub struct ReserveTrackerContract;

#[contractimpl]
impl ReserveTrackerContract {
    /// Initialize the reserve tracker contract
    pub fn initialize(
        env: Env,
        admin: Address,
        acbu_token: Address,
        min_ratio_bps: i128,  // Minimum ratio in basis points (e.g., 10200 = 102%)
        target_ratio_bps: i128, // Target ratio in basis points (e.g., 10500 = 105%)
    ) {
        // Check if already initialized
        if env.storage().instance().has(&DATA_KEY.admin) {
            panic!("Contract already initialized");
        }

        // Validate inputs
        if min_ratio_bps < BASIS_POINTS || target_ratio_bps < min_ratio_bps {
            panic!("Invalid ratio configuration");
        }

        // Store configuration
        env.storage().instance().set(&DATA_KEY.admin, &admin);
        env.storage().instance().set(&DATA_KEY.acbu_token, &acbu_token);
        env.storage().instance().set(&DATA_KEY.min_ratio, &min_ratio_bps);
        env.storage().instance().set(&DATA_KEY.target_ratio, &target_ratio_bps);

        // Initialize reserves map
        let reserves: Map<CurrencyCode, ReserveData> = Map::new(&env);
        env.storage().instance().set(&DATA_KEY.reserves, &reserves);
    }

    /// Update reserve for a currency (backend function)
    pub fn update_reserve(
        env: Env,
        currency: CurrencyCode,
        amount: i128,
        value_usd: i128,
    ) {
        // Only admin (backend) can update reserves
        Self::check_admin(&env);

        let current_time = env.ledger().timestamp();

        // Create reserve data
        let reserve_data = ReserveData {
            currency: currency.clone(),
            amount,
            value_usd,
            timestamp: current_time,
        };

        // Update reserves map
        let mut reserves: Map<CurrencyCode, ReserveData> =
            env.storage().instance().get(&DATA_KEY.reserves).unwrap_or(Map::new(&env));
        reserves.set(currency, reserve_data);
        env.storage().instance().set(&DATA_KEY.reserves, &reserves);
    }

    /// Get reserve data for a currency
    pub fn get_reserve(env: Env, currency: CurrencyCode) -> ReserveData {
        let reserves: Map<CurrencyCode, ReserveData> =
            env.storage().instance().get(&DATA_KEY.reserves).unwrap_or(Map::new(&env));
        reserves
            .get(&currency)
            .unwrap_or_else(|| panic!("Reserve not found for currency"))
    }

    /// Verify reserves meet overcollateralization requirements
    pub fn verify_reserves(env: Env) -> bool {
        let reserves: Map<CurrencyCode, ReserveData> =
            env.storage().instance().get(&DATA_KEY.reserves).unwrap_or(Map::new(&env));
        let min_ratio: i128 = env.storage().instance().get(&DATA_KEY.min_ratio).unwrap();

        // Get total reserve value in USD
        let total_reserve_value = Self::get_total_reserve_value_internal(&env);

        // Get total ACBU supply
        let acbu_token: Address = env.storage().instance().get(&DATA_KEY.acbu_token).unwrap();
        let acbu_client = soroban_sdk::token::Client::new(&env, &acbu_token);
        let total_supply = acbu_client.balance(&env.current_contract_address());

        if total_supply == 0 {
            return true; // No supply means no reserves needed
        }

        // Calculate ratio (reserve value / supply)
        // Both are in 7 decimals, so ratio is direct
        let ratio = (total_reserve_value * BASIS_POINTS) / total_supply;

        ratio >= min_ratio
    }

    /// Get total reserve value in USD
    pub fn get_total_reserve_value(env: Env) -> i128 {
        Self::get_total_reserve_value_internal(&env)
    }

    /// Get minimum required ratio
    pub fn get_min_ratio(env: Env) -> i128 {
        env.storage().instance().get(&DATA_KEY.min_ratio).unwrap()
    }

    /// Get target ratio
    pub fn get_target_ratio(env: Env) -> i128 {
        env.storage().instance().get(&DATA_KEY.target_ratio).unwrap()
    }

    // Private helper functions
    fn get_total_reserve_value_internal(env: &Env) -> i128 {
        let reserves: Map<CurrencyCode, ReserveData> =
            env.storage().instance().get(&DATA_KEY.reserves).unwrap_or(Map::new(env));

        let mut total = 0i128;
        for (_currency, reserve_data) in reserves.iter() {
            total += reserve_data.value_usd;
        }
        total
    }

    fn check_admin(env: &Env) {
        let admin: Address = env.storage().instance().get(&DATA_KEY.admin).unwrap();
        if admin != env.invoker() {
            panic!("Unauthorized: admin only");
        }
    }
}
