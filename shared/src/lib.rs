use soroban_sdk::{contracttype, Address, String as SorobanString, Symbol};

/// Currency code type (e.g., "NGN", "KES", "RWF")
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrencyCode(pub SorobanString);

impl CurrencyCode {
    pub fn new(code: &str) -> Self {
        CurrencyCode(SorobanString::from_str(code))
    }
}

/// Rate data structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct RateData {
    pub currency: CurrencyCode,
    pub rate_usd: i128,        // Rate in 7 decimals (e.g., 0.0012345 = 12345)
    pub timestamp: u64,
    pub sources: soroban_sdk::Vec<i128>, // Source rates for median calculation
}

/// Reserve data structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct ReserveData {
    pub currency: CurrencyCode,
    pub amount: i128,           // Reserve amount in native currency
    pub value_usd: i128,        // Value in USD (7 decimals)
    pub timestamp: u64,
}

/// Account details for withdrawals
#[contracttype]
#[derive(Clone, Debug)]
pub struct AccountDetails {
    pub account_number: SorobanString,
    pub bank_code: SorobanString,
    pub account_name: SorobanString,
    pub currency: CurrencyCode,
}

/// Mint event data
#[contracttype]
#[derive(Clone, Debug)]
pub struct MintEvent {
    pub transaction_id: SorobanString,
    pub user: Address,
    pub usdc_amount: i128,
    pub acbu_amount: i128,
    pub fee: i128,
    pub rate: i128,
    pub timestamp: u64,
}

/// Burn event data
#[contracttype]
#[derive(Clone, Debug)]
pub struct BurnEvent {
    pub transaction_id: SorobanString,
    pub user: Address,
    pub acbu_amount: i128,
    pub local_amount: i128,
    pub currency: CurrencyCode,
    pub fee: i128,
    pub rate: i128,
    pub timestamp: u64,
}

/// Rate update event data
#[contracttype]
#[derive(Clone, Debug)]
pub struct RateUpdateEvent {
    pub currency: CurrencyCode,
    pub rate: i128,
    pub timestamp: u64,
    pub validators: soroban_sdk::Vec<Address>,
}

/// Error types
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractError {
    Unauthorized,
    Paused,
    InvalidAmount,
    InvalidRate,
    InsufficientReserves,
    RateLimitExceeded,
    InvalidCurrency,
    OracleError,
    ReserveError,
}

/// Constants
pub const BASIS_POINTS: i128 = 10_000;
pub const DECIMALS: i128 = 10_000_000; // 7 decimals
pub const MIN_MINT_AMOUNT: i128 = 10_000_000; // 10 USDC (7 decimals)
pub const MAX_MINT_AMOUNT: i128 = 1_000_000_000_000; // 1M USDC (7 decimals)
pub const MIN_BURN_AMOUNT: i128 = 10_000_000; // 10 ACBU (7 decimals)
pub const UPDATE_INTERVAL_SECONDS: u64 = 21_600; // 6 hours
pub const EMERGENCY_THRESHOLD_BPS: i128 = 500; // 5% deviation threshold
pub const OUTLIER_THRESHOLD_BPS: i128 = 300; // 3% deviation for outlier detection

/// Utility functions
pub fn calculate_fee(amount: i128, fee_rate_bps: i128) -> i128 {
    (amount * fee_rate_bps) / BASIS_POINTS
}

pub fn calculate_amount_after_fee(amount: i128, fee_rate_bps: i128) -> i128 {
    amount - calculate_fee(amount, fee_rate_bps)
}

/// Calculate median of sorted values
pub fn median(values: &[i128]) -> Option<i128> {
    if values.is_empty() {
        return None;
    }

    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        let lower = nth_smallest(values, mid - 1)?;
        let upper = nth_smallest(values, mid)?;
        Some((lower + upper) / 2)
    } else {
        nth_smallest(values, mid)
    }
}

fn nth_smallest(values: &[i128], target_index: usize) -> Option<i128> {
    for &candidate in values {
        let mut less_than = 0usize;
        let mut equal_to = 0usize;

        for &value in values {
            if value < candidate {
                less_than += 1;
            } else if value == candidate {
                equal_to += 1;
            }
        }

        if less_than <= target_index && target_index < less_than + equal_to {
            return Some(candidate);
        }
    }

    None
}

/// Calculate percentage deviation
pub fn calculate_deviation(value1: i128, value2: i128) -> i128 {
    if value2 == 0 {
        return i128::MAX;
    }
    let diff = if value1 > value2 {
        value1 - value2
    } else {
        value2 - value1
    };
    (diff * BASIS_POINTS) / value2
}
