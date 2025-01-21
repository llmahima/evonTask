use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::{caller, time};
use ic_cdk_macros::*;
use std::collections::HashMap;
use std::cell::RefCell;

#[derive(CandidType, Deserialize, Clone, Debug)]
struct Allowance {
    amount: u64,
    expires_at: u64,
}

#[derive(CandidType, Deserialize)]
struct TokenState {
    balances: HashMap<Principal, u64>,
    allowances: HashMap<Principal, HashMap<Principal, Allowance>>,
    total_supply: u64,
    owner: Principal,
    minters: Vec<Principal>,
    paused: bool,
    transaction_count: HashMap<Principal, u64>,  // For rate limiting
    last_transaction_time: HashMap<Principal, u64>,
}

#[derive(CandidType, Debug)]
enum Error {
    InsufficientBalance,
    InsufficientAllowance,
    Unauthorized,
    ExpiredAllowance,
    ContractPaused,
    RateLimitExceeded,
    InvalidAmount,
}

type Result<T> = std::result::Result<T, Error>;

thread_local! {
    static STATE: RefCell<TokenState> = RefCell::new(TokenState {
        balances: HashMap::new(),
        allowances: HashMap::new(),
        total_supply: 0,
        owner: Principal::anonymous(),
        minters: Vec::new(),
        paused: false,
        transaction_count: HashMap::new(),
        last_transaction_time: HashMap::new(),
    });
}

// Security constants
const MAX_TRANSACTIONS_PER_HOUR: u64 = 100;
const TRANSACTION_WINDOW: u64 = 3600_000_000_000; // 1 hour in nanoseconds
const MAX_MINT_AMOUNT: u64 = 1_000_000;

#[init]
fn init() {
    let caller = caller();
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.owner = caller;
        state.minters.push(caller);
        state.total_supply = 1_000_000;
        state.balances.insert(caller, 1_000_000);
    });
}

// Owner-only function to add minters
#[update]
fn add_minter(minter: Principal) -> Result<()> {
    require_owner()?;
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if !state.minters.contains(&minter) {
            state.minters.push(minter);
        }
    });
    Ok(())
}

// Minting functionality
#[update]
fn mint(to: Principal, amount: u64) -> Result<()> {
    if amount > MAX_MINT_AMOUNT {
        return Err(Error::InvalidAmount);
    }
    
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if !state.minters.contains(&caller()) {
            return Err(Error::Unauthorized);
        }
        
        let current_balance = state.balances.get(&to).unwrap_or(&0);
        state.balances.insert(to, current_balance + amount);
        state.total_supply += amount;
        Ok(())
    })
}

// Enhanced transfer with rate limiting and security checks
#[update]
fn transfer(to: Principal, amount: u64) -> Result<()> {
    let caller = caller();
    check_rate_limit(&caller)?;
    
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.paused {
            return Err(Error::ContractPaused);
        }
        
        let from_balance = state.balances.get(&caller).unwrap_or(&0);
        if *from_balance < amount {
            return Err(Error::InsufficientBalance);
        }
        
        // Update balances
        state.balances.insert(caller, from_balance - amount);
        let to_balance = state.balances.get(&to).unwrap_or(&0);
        state.balances.insert(to, to_balance + amount);
        
        // Update rate limiting data
        update_transaction_count(&mut state, &caller);
        
        Ok(())
    })
}

// Allowance functionality
#[update]
fn approve(spender: Principal, amount: u64, duration: u64) -> Result<()> {
    let caller = caller();
    
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let expires_at = time() + duration;
        
        let allowance = Allowance {
            amount,
            expires_at,
        };
        
        state.allowances
            .entry(caller)
            .or_insert_with(HashMap::new)
            .insert(spender, allowance);
        
        Ok(())
    })
}

#[update]
fn transfer_from(from: Principal, to: Principal, amount: u64) -> Result<()> {
    let caller = caller();
    check_rate_limit(&caller)?;
    
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.paused {
            return Err(Error::ContractPaused);
        }
        
        // Check allowance
        let allowance = state.allowances
            .get(&from)
            .and_then(|inner_map| inner_map.get(&caller))
            .ok_or(Error::InsufficientAllowance)?;
            
        if time() > allowance.expires_at {
            return Err(Error::ExpiredAllowance);
        }
        
        if allowance.amount < amount {
            return Err(Error::InsufficientAllowance);
        }
        
        // Check balance
        let from_balance = state.balances.get(&from).unwrap_or(&0);
        if *from_balance < amount {
            return Err(Error::InsufficientBalance);
        }
        
        // Update balances
        state.balances.insert(from, from_balance - amount);
        let to_balance = state.balances.get(&to).unwrap_or(&0);
        state.balances.insert(to, to_balance + amount);
        
        // Update allowance
        if let Some(inner_map) = state.allowances.get_mut(&from) {
            if let Some(allowance) = inner_map.get_mut(&caller) {
                allowance.amount -= amount;
            }
        }
        
        // Update rate limiting data
        update_transaction_count(&mut state, &caller);
        
        Ok(())
    })
}

// Emergency pause functionality
#[update]
fn pause() -> Result<()> {
    require_owner()?;
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.paused = true;
    });
    Ok(())
}

#[update]
fn unpause() -> Result<()> {
    require_owner()?;
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.paused = false;
    });
    Ok(())
}

// Helper functions
fn require_owner() -> Result<()> {
    STATE.with(|state| {
        let state = state.borrow();
        if caller() != state.owner {
            return Err(Error::Unauthorized);
        }
        Ok(())
    })
}

fn check_rate_limit(user: &Principal) -> Result<()> {
    STATE.with(|state| {
        let state = state.borrow();
        let current_time = time();
        
        if let Some(last_time) = state.last_transaction_time.get(user) {
            if current_time - last_time < TRANSACTION_WINDOW {
                if let Some(count) = state.transaction_count.get(user) {
                    if *count >= MAX_TRANSACTIONS_PER_HOUR {
                        return Err(Error::RateLimitExceeded);
                    }
                }
            }
        }
        Ok(())
    })
}

fn update_transaction_count(state: &mut TokenState, user: &Principal) {
    let current_time = time();
    let last_time = state.last_transaction_time.get(user).unwrap_or(&0);
    
    if current_time - last_time >= TRANSACTION_WINDOW {
        state.transaction_count.insert(*user, 1);
    } else {
        let count = state.transaction_count.get(user).unwrap_or(&0) + 1;
        state.transaction_count.insert(*user, count);
    }
    
    state.last_transaction_time.insert(*user, current_time);
}

// Query functions
#[query]
fn balance_of(account: Principal) -> u64 {
    STATE.with(|state| {
        let state = state.borrow();
        *state.balances.get(&account).unwrap_or(&0)
    })
}

#[query]
fn allowance(owner: Principal, spender: Principal) -> u64 {
    STATE.with(|state| {
        let state = state.borrow();
        state.allowances
            .get(&owner)
            .and_then(|inner_map| inner_map.get(&spender))
            .map_or(0, |allowance| {
                if time() > allowance.expires_at {
                    0
                } else {
                    allowance.amount
                }
            })
    })
}

#[query]
fn total_supply() -> u64 {
    STATE.with(|state| {
        let state = state.borrow();
        state.total_supply
    })
}

// Tests
#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_principal(id: u8) -> Principal {
        Principal::from_slice(&[id; 29])
    }
    
    #[test]
    fn test_init() {
        init();
        STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.total_supply, 1_000_000);
            assert!(state.minters.contains(&caller()));
        });
    }
    
    #[test]
    fn test_minting() {
        init();
        let recipient = create_test_principal(1);
        assert!(mint(recipient, 1000).is_ok());
        assert_eq!(balance_of(recipient), 1000);
    }
    
    #[test]
    fn test_transfer() {
        init();
        let recipient = create_test_principal(1);
        assert!(transfer(recipient, 100).is_ok());
        assert_eq!(balance_of(recipient), 100);
    }
    
    #[test]
    fn test_allowances() {
        init();
        let spender = create_test_principal(1);
        let recipient = create_test_principal(2);
        
        // Approve spending
        assert!(approve(spender, 100, 3600_000_000_000).is_ok());
        assert_eq!(allowance(caller(), spender), 100);
        
        // Test transfer_from
        assert!(transfer_from(caller(), recipient, 50).is_ok());
        assert_eq!(balance_of(recipient), 50);
        assert_eq!(allowance(caller(), spender), 50);
    }
    
    #[test]
    fn test_rate_limiting() {
        init();
        let recipient = create_test_principal(1);
        
        // Perform MAX_TRANSACTIONS_PER_HOUR + 1 transfers
        for _ in 0..MAX_TRANSACTIONS_PER_HOUR {
            assert!(transfer(recipient, 1).is_ok());
        }
        
        // The next transfer should fail due to rate limiting
        assert!(matches!(transfer(recipient, 1), Err(Error::RateLimitExceeded)));
    }
    
    #[test]
    fn test_pause_functionality() {
        init();
        let recipient = create_test_principal(1);
        
        // Test pausing
        assert!(pause().is_ok());
        assert!(matches!(transfer(recipient, 100), Err(Error::ContractPaused)));
        
        // Test unpausing
        assert!(unpause().is_ok());
        assert!(transfer(recipient, 100).is_ok());
    }
}