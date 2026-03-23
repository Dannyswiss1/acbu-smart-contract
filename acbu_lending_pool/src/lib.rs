#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol,
};

// Storage Keys

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataKey {
    pub admin:          Symbol,
    pub acbu_token:     Symbol,
    pub fee_rate:       Symbol,
    pub paused:         Symbol,
    pub pool_liquidity: Symbol,
    pub loan_counter:   Symbol,
}

const DATA_KEY: DataKey = DataKey {
    admin:          symbol_short!("ADMIN"),
    acbu_token:     symbol_short!("ACBU_TKN"),
    fee_rate:       symbol_short!("FEE_RATE"),
    paused:         symbol_short!("PAUSED"),
    pool_liquidity: symbol_short!("POOL_LIQ"),
    loan_counter:   symbol_short!("LOAN_CTR"),
};

// Domain Types

/// A single loan record persisted for the life of the loan.
///
/// Stored in `persistent` storage (not `temporary`) because loans can span
/// weeks or months — well beyond the TTL of temporary ledger entries.
///
/// The `repaid` flag is set to `true` on repayment rather than deleting the
/// entry, preserving the on-chain audit trail.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Loan {
    pub id:              u64,
    pub borrower:        Address,
    pub principal:       i128, // ACBU amount, 7-decimal fixed-point
    pub interest_bps:    i128, // flat rate agreed at creation, in basis points
    pub term_seconds:    u64,  // intended duration; informational, not enforced on-chain
    pub start_timestamp: u64,  // ledger timestamp at loan creation
    pub repaid:          bool,
}

// Events

#[contracttype]
#[derive(Clone, Debug)]
pub struct LoanCreatedEvent {
    pub lender:       Address, // always the pool contract address
    pub borrower:     Address,
    pub amount:       i128,
    pub interest_bps: i128,
    pub term_seconds: u64,
    pub timestamp:    u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct RepaymentEvent {
    pub borrower:  Address,
    pub amount:    i128, // total repaid: principal + interest
    pub timestamp: u64,
}

// Contract

#[contract]
pub struct LendingPool;

#[contractimpl]
impl LendingPool {

    // Lifecycle

    /// Initialize the lending pool contract. Can only be called once.
    pub fn initialize(env: Env, admin: Address, acbu_token: Address, fee_rate_bps: i128) {
        if env.storage().instance().has(&DATA_KEY.admin) {
            panic!("Contract already initialized");
        }
        if fee_rate_bps < 0 || fee_rate_bps > 10_000 {
            panic!("Invalid fee rate");
        }
        env.storage().instance().set(&DATA_KEY.admin,          &admin);
        env.storage().instance().set(&DATA_KEY.acbu_token,     &acbu_token);
        env.storage().instance().set(&DATA_KEY.fee_rate,       &fee_rate_bps);
        env.storage().instance().set(&DATA_KEY.paused,         &false);
        env.storage().instance().set(&DATA_KEY.pool_liquidity, &0i128);
        env.storage().instance().set(&DATA_KEY.loan_counter,   &0u64);
    }

    // Lender Interface

    /// Deposit ACBU into the pool (lender supplies liquidity).
    /// Returns the lender's updated balance.
    pub fn deposit(env: Env, lender: Address, amount: i128) -> Result<i128, soroban_sdk::Error> {
        let paused: bool = env.storage().instance().get(&DATA_KEY.paused).unwrap_or(false);
        if paused {
            return Err(soroban_sdk::Error::from_contract_error(2001));
        }
        if amount <= 0 {
            return Err(soroban_sdk::Error::from_contract_error(2002));
        }

        let acbu: Address = env.storage().instance().get(&DATA_KEY.acbu_token).unwrap().unwrap();
        soroban_sdk::token::Client::new(&env, &acbu)
            .transfer(&lender, &env.current_contract_address(), &amount);

        let existing: i128 = env.storage().temporary().get(&lender).unwrap_or(0);
        env.storage().temporary().set(&lender, &(existing + amount));

        let liq: i128 = env.storage().instance().get(&DATA_KEY.pool_liquidity).unwrap_or(0);
        env.storage().instance().set(&DATA_KEY.pool_liquidity, &(liq + amount));

        Ok(existing + amount)
    }

    /// Withdraw ACBU from the pool.
    ///
    /// Fails if the requested amount exceeds either the lender's deposited
    /// balance **or** the pool's currently available liquidity (i.e. funds not
    /// deployed as outstanding loans).
    pub fn withdraw(env: Env, lender: Address, amount: i128) -> Result<(), soroban_sdk::Error> {
        let paused: bool = env.storage().instance().get(&DATA_KEY.paused).unwrap_or(false);
        if paused {
            return Err(soroban_sdk::Error::from_contract_error(2001));
        }
        if amount <= 0 {
            return Err(soroban_sdk::Error::from_contract_error(2002));
        }

        let balance: i128 = env
            .storage()
            .temporary()
            .get(&lender)
            .ok_or(soroban_sdk::Error::from_contract_error(2003))?;

        if balance < amount {
            return Err(soroban_sdk::Error::from_contract_error(2004));
        }

        // Secondary guard: some portion of the pool may be deployed as loans.
        let liq: i128 = env.storage().instance().get(&DATA_KEY.pool_liquidity).unwrap_or(0);
        if liq < amount {
            return Err(soroban_sdk::Error::from_contract_error(2004));
        }

        env.storage().temporary().set(&lender, &(balance - amount));
        env.storage().instance().set(&DATA_KEY.pool_liquidity, &(liq - amount));

        let acbu: Address = env.storage().instance().get(&DATA_KEY.acbu_token).unwrap().unwrap();
        soroban_sdk::token::Client::new(&env, &acbu)
            .transfer(&env.current_contract_address(), &lender, &amount);

        Ok(())
    }

    /// Returns the lender's deposited balance.
    pub fn get_balance(env: Env, lender: Address) -> i128 {
        env.storage().temporary().get(&lender).unwrap_or(0)
    }

    // Borrower Interface

    /// Request a loan from the pool.
    ///
    /// - `amount`       — principal in ACBU (7-decimal fixed-point, must be > 0)
    /// - `interest_bps` — flat interest rate in basis points [0, 10_000]
    /// - `term_seconds` — agreed loan duration in seconds (must be > 0)
    ///
    /// Returns the unique `loan_id`, which the borrower must pass to `repay`.
    /// Emits `LoanCreatedEvent`.
    pub fn borrow(
        env: Env,
        borrower: Address,
        amount: i128,
        interest_bps: i128,
        term_seconds: u64,
    ) -> Result<u64, soroban_sdk::Error> {
        let paused: bool = env.storage().instance().get(&DATA_KEY.paused).unwrap_or(false);
        if paused {
            return Err(soroban_sdk::Error::from_contract_error(2001));
        }
        if amount <= 0 {
            return Err(soroban_sdk::Error::from_contract_error(2002));
        }
        if interest_bps < 0 || interest_bps > 10_000 {
            return Err(soroban_sdk::Error::from_contract_error(2005));
        }
        if term_seconds == 0 {
            return Err(soroban_sdk::Error::from_contract_error(2006));
        }

        borrower.require_auth();

        let liq: i128 = env.storage().instance().get(&DATA_KEY.pool_liquidity).unwrap_or(0);
        if liq < amount {
            return Err(soroban_sdk::Error::from_contract_error(2007));
        }

        // Assign loan ID.
        let loan_id: u64 = env.storage().instance().get(&DATA_KEY.loan_counter).unwrap_or(0) + 1;
        env.storage().instance().set(&DATA_KEY.loan_counter, &loan_id);

        let now = env.ledger().timestamp();

        // Persist loan record before transferring tokens so state is consistent
        // if the token call reverts.
        let loan = Loan {
            id: loan_id,
            borrower: borrower.clone(),
            principal: amount,
            interest_bps,
            term_seconds,
            start_timestamp: now,
            repaid: false,
        };
        env.storage().persistent().set(&(symbol_short!("LOAN"), loan_id), &loan);

        env.storage().instance().set(&DATA_KEY.pool_liquidity, &(liq - amount));

        let acbu: Address = env.storage().instance().get(&DATA_KEY.acbu_token).unwrap().unwrap();
        soroban_sdk::token::Client::new(&env, &acbu)
            .transfer(&env.current_contract_address(), &borrower, &amount);

        env.events().publish(
            (symbol_short!("borrow"), borrower.clone()),
            LoanCreatedEvent {
                lender: env.current_contract_address(),
                borrower,
                amount,
                interest_bps,
                term_seconds,
                timestamp: now,
            },
        );

        Ok(loan_id)
    }

    /// Repay an outstanding loan.
    ///
    /// The caller must be the original borrower. Total transferred is:
    ///   `principal + (principal * interest_bps / 10_000)`
    ///
    /// Interest is fixed at loan creation and does not change with repayment
    /// timing. Emits `RepaymentEvent`.
    pub fn repay(env: Env, loan_id: u64) -> Result<(), soroban_sdk::Error> {
        let paused: bool = env.storage().instance().get(&DATA_KEY.paused).unwrap_or(false);
        if paused {
            return Err(soroban_sdk::Error::from_contract_error(2001));
        }

        let loan: Loan = env
            .storage()
            .persistent()
            .get(&(symbol_short!("LOAN"), loan_id))
            .ok_or(soroban_sdk::Error::from_contract_error(2008))?;

        if loan.repaid {
            return Err(soroban_sdk::Error::from_contract_error(2009));
        }

        // Auth is checked against the stored borrower address, not the
        // transaction sender, so it cannot be bypassed by a third party.
        loan.borrower.require_auth();

        let interest = (loan.principal * loan.interest_bps) / 10_000;
        let total_repayment = loan.principal + interest;

        // Mark repaid and update pool before token transfer (reentrancy safety).
        let mut settled = loan.clone();
        settled.repaid = true;
        env.storage().persistent().set(&(symbol_short!("LOAN"), loan_id), &settled);

        let liq: i128 = env.storage().instance().get(&DATA_KEY.pool_liquidity).unwrap_or(0);
        env.storage().instance().set(&DATA_KEY.pool_liquidity, &(liq + total_repayment));

        let acbu: Address = env.storage().instance().get(&DATA_KEY.acbu_token).unwrap().unwrap();
        soroban_sdk::token::Client::new(&env, &acbu)
            .transfer(&loan.borrower, &env.current_contract_address(), &total_repayment);

        env.events().publish(
            (symbol_short!("repay"), loan.borrower.clone()),
            RepaymentEvent {
                borrower:  loan.borrower,
                amount:    total_repayment,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    // Queries

    /// Returns the full loan record for a given ID, or `None` if not found.
    pub fn get_loan(env: Env, loan_id: u64) -> Option<Loan> {
        env.storage().persistent().get(&(symbol_short!("LOAN"), loan_id))
    }

    /// Returns the total ACBU currently available for lending.
    pub fn get_pool_liquidity(env: Env) -> i128 {
        env.storage().instance().get(&DATA_KEY.pool_liquidity).unwrap_or(0)
    }

    // Admin

    pub fn pause(env: Env) -> Result<(), soroban_sdk::Error> {
        let admin: Address = env.storage().instance().get(&DATA_KEY.admin).unwrap().unwrap();
        admin.require_auth();
        env.storage().instance().set(&DATA_KEY.paused, &true);
        Ok(())
    }

    pub fn unpause(env: Env) -> Result<(), soroban_sdk::Error> {
        let admin: Address = env.storage().instance().get(&DATA_KEY.admin).unwrap().unwrap();
        admin.require_auth();
        env.storage().instance().set(&DATA_KEY.paused, &false);
        Ok(())
    }
}
