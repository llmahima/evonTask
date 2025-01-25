use ic_cdk_macros::{init, update, query};
use candid::CandidType;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
struct Account {
    name: String,
    balance: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
struct Transaction {
    from: String,
    to: String,
    amount: u64,
    timestamp: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
struct TokenState {
    accounts: HashMap<String, Account>,
    total_supply: u64,
    transactions: Vec<Transaction>,
}

thread_local! {
    static TOKEN_STATE: std::cell::RefCell<TokenState> = std::cell::RefCell::new(TokenState {
        accounts: HashMap::new(),
        total_supply: 0,
        transactions: Vec::new(),
    });
}

#[init]
fn init() {
    TOKEN_STATE.with(|state| {
        let mut token_state = state.borrow_mut();
        token_state.total_supply = 0;
    });
}

#[update]
fn create_account(name: String, initial_balance: u64) -> Result<String, String> {
    TOKEN_STATE.with(|state| {
        let mut token_state = state.borrow_mut();
        
        if token_state.accounts.contains_key(&name) {
            return Err("Account already exists".to_string());
        }
        
        let account = Account {
            name: name.clone(),
            balance: initial_balance,
        };
        
        token_state.accounts.insert(name.clone(), account);
        token_state.total_supply += initial_balance;
        
        Ok(format!("Account created for {} with balance {}", name, initial_balance))
    })
}

#[update]
fn send_token(from: String, to: String, amount: u64) -> Result<String, String> {
    TOKEN_STATE.with(|state| {
        let mut token_state = state.borrow_mut();
        
        let sender = token_state.accounts.get_mut(&from)
            .ok_or_else(|| "Sender account not found".to_string())?;
        
        if sender.balance < amount {
            return Err("Insufficient balance".to_string());
        }
        
        sender.balance -= amount;
        
        let recipient = token_state.accounts.get_mut(&to)
            .ok_or_else(|| "Recipient account not found".to_string())?;
        recipient.balance += amount;
        
        let transaction = Transaction {
            from: from.clone(),
            to: to.clone(),
            amount,
            timestamp: ic_cdk::api::time(),
        };
        
        token_state.transactions.push(transaction);
        
        Ok(format!("Sent {} tokens from {} to {}", amount, from, to))
    })
}

#[query]
fn get_balance(name: String) -> Result<u64, String> {
    TOKEN_STATE.with(|state| {
        let token_state = state.borrow();
        token_state.accounts.get(&name)
            .map(|account| account.balance)
            .ok_or_else(|| "Account not found".to_string())
    })
}

#[query]
fn get_total_supply() -> u64 {
    TOKEN_STATE.with(|state| {
        state.borrow().total_supply
    })
}

#[query]
fn get_transaction_history(name: String) -> Vec<Transaction> {
    TOKEN_STATE.with(|state| {
        let token_state = state.borrow();
        token_state.transactions.iter()
            .filter(|tx| tx.from == name || tx.to == name)
            .cloned()
            .collect()
    })
}

// Export the Candid interface
ic_cdk::export_candid!();