//! Bitcoin Infinity account management
//!
//! Manages accounts keyed by Bitcoin addresses with transparent access via signature recovery

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Represents a Bitcoin Infinity account state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    /// Bitcoin address (account ID)
    pub address: String,
    /// Account balance in yoctoBIT
    pub balance: u128,
    /// Transaction nonce (for ordering/deduplication)
    pub nonce: u64,
    /// Recovered public key (optional, populated after first tx)
    pub public_key: Option<String>,
}

impl Account {
    /// Create a new account with a Bitcoin address and initial balance
    pub fn new(address: String, balance: u128) -> Self {
        Account {
            address,
            balance,
            nonce: 0,
            public_key: None,
        }
    }

    /// Register the public key after signature recovery
    pub fn register_public_key(&mut self, public_key: String) {
        self.public_key = Some(public_key);
    }

    /// Get the account's current balance
    pub fn get_balance(&self) -> u128 {
        self.balance
    }

    /// Add balance (for receiving transfers or mining rewards)
    pub fn add_balance(&mut self, amount: u128) -> Result<(), String> {
        self.balance = self
            .balance
            .checked_add(amount)
            .ok_or("Balance overflow".to_string())?;
        Ok(())
    }

    /// Subtract balance (for sending transfers)
    pub fn subtract_balance(&mut self, amount: u128) -> Result<(), String> {
        if self.balance < amount {
            return Err(format!(
                "Insufficient balance: {} < {}",
                self.balance, amount
            ));
        }
        self.balance -= amount;
        Ok(())
    }

    /// Increment nonce (for transaction ordering)
    pub fn increment_nonce(&mut self) {
        self.nonce += 1;
    }
}

/// Manages all Bitcoin Infinity accounts
pub struct AccountManager {
    /// All accounts indexed by Bitcoin address
    accounts: BTreeMap<String, Account>,
}

impl AccountManager {
    /// Create a new account manager
    pub fn new() -> Self {
        AccountManager {
            accounts: BTreeMap::new(),
        }
    }

    /// Create a new account with initial balance
    pub fn create_account(&mut self, address: String, balance: u128) -> Result<(), String> {
        if self.accounts.contains_key(&address) {
            return Err(format!("Account {} already exists", address));
        }
        self.accounts
            .insert(address.clone(), Account::new(address, balance));
        Ok(())
    }

    /// Load accounts from genesis data (UTXO map)
    pub fn load_from_utxos(&mut self, utxos: &BTreeMap<String, u64>) -> Result<(), String> {
        for (address, satoshis) in utxos {
            let balance = *satoshis as u128 * 10u128.pow(16); // Convert to yoctoBIT
            self.create_account(address.clone(), balance)?;
        }
        Ok(())
    }

    /// Get an account (immutable)
    pub fn get_account(&self, address: &str) -> Option<&Account> {
        self.accounts.get(address)
    }

    /// Get an account (mutable)
    pub fn get_account_mut(&mut self, address: &str) -> Option<&mut Account> {
        self.accounts.get_mut(address)
    }

    /// Get account balance
    pub fn get_balance(&self, address: &str) -> Option<u128> {
        self.get_account(address).map(|a| a.balance)
    }

    /// Transfer balance from one account to another
    pub fn transfer(&mut self, from: &str, to: &str, amount: u128) -> Result<(), String> {
        // Verify both accounts exist
        if !self.accounts.contains_key(from) {
            return Err(format!("Sender account {} not found", from));
        }
        if !self.accounts.contains_key(to) {
            return Err(format!("Receiver account {} not found", to));
        }

        // Subtract from sender
        self.accounts
            .get_mut(from)
            .unwrap()
            .subtract_balance(amount)?;

        // Add to receiver
        self.accounts.get_mut(to).unwrap().add_balance(amount)?;

        Ok(())
    }

    /// Process a transaction (transfer + nonce increment)
    pub fn execute_transfer(&mut self, from: &str, to: &str, amount: u128) -> Result<(), String> {
        // Transfer the balance
        self.transfer(from, to, amount)?;

        // Increment sender's nonce
        if let Some(acc) = self.get_account_mut(from) {
            acc.increment_nonce();
        }

        Ok(())
    }

    /// Register a public key for an account (called after first successful signature recovery)
    pub fn register_public_key(&mut self, address: &str, public_key: String) -> Result<(), String> {
        self.get_account_mut(address)
            .ok_or_else(|| format!("Account {} not found", address))?
            .register_public_key(public_key);
        Ok(())
    }

    /// Get total supply (sum of all account balances)
    pub fn total_supply(&self) -> u128 {
        self.accounts.values().map(|a| a.balance).sum()
    }

    /// Get account count
    pub fn account_count(&self) -> usize {
        self.accounts.len()
    }

    /// Export all accounts for persistence
    pub fn export_accounts(&self) -> BTreeMap<String, Account> {
        self.accounts.clone()
    }
}

impl Default for AccountManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_creation() {
        let mut manager = AccountManager::new();
        let addr = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string();
        let balance = 50_000_000_000_000u128;

        manager.create_account(addr.clone(), balance).unwrap();
        assert_eq!(manager.get_balance(&addr), Some(balance));
    }

    #[test]
    fn test_transfer() {
        let mut manager = AccountManager::new();
        let addr1 = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string();
        let addr2 = "1FP5gk4z7mDdSb3m3YvUwFb1BDUvcLYe1F".to_string();
        let initial = 100_000_000_000_000u128;

        manager.create_account(addr1.clone(), initial).unwrap();
        manager.create_account(addr2.clone(), 0).unwrap();

        manager
            .transfer(&addr1, &addr2, 50_000_000_000_000)
            .unwrap();

        assert_eq!(manager.get_balance(&addr1), Some(50_000_000_000_000));
        assert_eq!(manager.get_balance(&addr2), Some(50_000_000_000_000));
    }

    #[test]
    fn test_insufficient_balance() {
        let mut manager = AccountManager::new();
        let addr1 = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string();
        let addr2 = "1FP5gk4z7mDdSb3m3YvUwFb1BDUvcLYe1F".to_string();

        manager.create_account(addr1.clone(), 100).unwrap();
        manager.create_account(addr2.clone(), 0).unwrap();

        let result = manager.transfer(&addr1, &addr2, 200);
        assert!(result.is_err());
        assert_eq!(manager.get_balance(&addr1), Some(100)); // Balance unchanged
    }

    #[test]
    fn test_load_from_utxos() {
        let mut manager = AccountManager::new();
        let mut utxos = BTreeMap::new();
        utxos.insert(
            "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string(),
            50_000_000_000_000u64,
        );
        utxos.insert(
            "1FP5gk4z7mDdSb3m3YvUwFb1BDUvcLYe1F".to_string(),
            25_000_000_000u64,
        );

        manager.load_from_utxos(&utxos).unwrap();

        assert_eq!(manager.account_count(), 2);
        assert_eq!(
            manager.get_balance("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"),
            Some(50_000_000_000_000u128 * 10u128.pow(16))
        );
    }

    #[test]
    fn test_nonce_increment() {
        let mut manager = AccountManager::new();
        let addr = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string();

        manager.create_account(addr.clone(), 1000).unwrap();
        let account = manager.get_account(&addr).unwrap();
        assert_eq!(account.nonce, 0);

        manager.execute_transfer(&addr, &addr, 0).unwrap();
        let account = manager.get_account(&addr).unwrap();
        assert_eq!(account.nonce, 1);
    }
}
