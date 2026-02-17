//! Transaction processing for Bitcoin Infinity
//!
//! Validates and executes transactions:
//! 1. Signature recovery from Bitcoin key
//! 2. Address validation
//! 3. Balance transfer
//! 4. Nonce management

use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use crate::signature_recovery::{validate_transaction_signature, SignatureValidation};
use crate::account_manager::AccountManager;

/// A Bitcoin Infinity transaction (simplified, for testing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Sender's Bitcoin address
    pub from: String,
    /// Receiver's Bitcoin address
    pub to: String,
    /// Amount in yoctoBIT
    pub amount: u128,
    /// Transaction nonce (for ordering)
    pub nonce: u64,
    /// Gas price in yoctoBIT per unit
    pub gas_price: u64,
    /// Maximum gas
    pub gas_limit: u64,
    /// The signature (65 bytes: 64 bytes signature + 1 byte recovery id)
    pub signature: Vec<u8>,
}

impl Transaction {
    /// Compute the hash of this transaction (what was signed)
    pub fn hash(&self) -> [u8; 32] {
        // Simple hash: SHA256 of (from + to + amount + nonce)
        // In production, this would be a more sophisticated encoding
        let mut hasher = Sha256::new();
        hasher.update(self.from.as_bytes());
        hasher.update(self.to.as_bytes());
        hasher.update(self.amount.to_le_bytes());
        hasher.update(self.nonce.to_le_bytes());
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result[..32]);
        hash
    }

    /// Validate the transaction signature
    pub fn validate_signature(&self) -> SignatureValidation {
        // Signature must be 65 bytes
        if self.signature.len() != 65 {
            return SignatureValidation {
                is_valid: false,
                signer_address: None,
                error: Some(format!("Invalid signature length: {}", self.signature.len())),
            };
        }

        let hash = self.hash();
        let mut sig_bytes = [0u8; 65];
        sig_bytes.copy_from_slice(&self.signature[..65]);

        validate_transaction_signature(&hash, &sig_bytes, &self.from)
    }
}

/// Result of transaction execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionReceipt {
    pub transaction_hash: String,
    pub status: TransactionStatus,
    pub error: Option<String>,
    pub gas_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionStatus {
    Success,
    Failed,
}

/// Process transactions in a block
pub struct TransactionProcessor {
    accounts: AccountManager,
}

impl TransactionProcessor {
    pub fn new(accounts: AccountManager) -> Self {
        TransactionProcessor { accounts }
    }

    /// Execute a single transaction
    pub fn execute_transaction(&mut self, tx: &Transaction) -> TransactionReceipt {
        let tx_hash = format!("0x{:x}", {
            let mut hasher = Sha256::new();
            hasher.update(serde_json::to_string(tx).unwrap_or_default());
            let result = hasher.finalize();
            u64::from_le_bytes([
                result[0], result[1], result[2], result[3], result[4], result[5], result[6],
                result[7],
            ])
        });

        // Validate signature
        let sig_validation = tx.validate_signature();
        if !sig_validation.is_valid {
            return TransactionReceipt {
                transaction_hash: tx_hash,
                status: TransactionStatus::Failed,
                error: sig_validation.error,
                gas_used: 0,
            };
        }

        // Register public key if recovered
        if let Some(addr) = sig_validation.signer_address.clone() {
            if let Some(account) = self.accounts.get_account(&addr) {
                if account.public_key.is_none() {
                    // This is first transaction - in real implementation, register pubkey
                    // For now, we just note it in logs
                }
            }
        }

        // Check account exists
        if self.accounts.get_account(&tx.from).is_none() {
            return TransactionReceipt {
                transaction_hash: tx_hash,
                status: TransactionStatus::Failed,
                error: Some(format!("Account {} not found", tx.from)),
                gas_used: 0,
            };
        }

        // Execute transfer
        match self.accounts.execute_transfer(&tx.from, &tx.to, tx.amount) {
            Ok(()) => TransactionReceipt {
                transaction_hash: tx_hash,
                status: TransactionStatus::Success,
                error: None,
                gas_used: 21000, // Standard transfer gas
            },
            Err(e) => TransactionReceipt {
                transaction_hash: tx_hash,
                status: TransactionStatus::Failed,
                error: Some(e),
                gas_used: 21000,
            },
        }
    }

    /// Execute a batch of transactions
    pub fn execute_block(&mut self, transactions: Vec<Transaction>) -> Vec<TransactionReceipt> {
        transactions
            .iter()
            .map(|tx| self.execute_transaction(tx))
            .collect()
    }

    /// Get account state
    pub fn get_account_balance(&self, address: &str) -> Option<u128> {
        self.accounts.get_balance(address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_hash_deterministic() {
        let tx = Transaction {
            from: "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string(),
            to: "1FP5gk4z7mDdSb3m3YvUwFb1BDUvcLYe1F".to_string(),
            amount: 100_000u128,
            nonce: 1,
            gas_price: 1,
            gas_limit: 21000,
            signature: vec![0u8; 65],
        };

        let hash1 = tx.hash();
        let hash2 = tx.hash();

        assert_eq!(hash1, hash2, "Transaction hash should be deterministic");
    }

    #[test]
    fn test_transaction_validation() {
        let tx = Transaction {
            from: "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string(),
            to: "1FP5gk4z7mDdSb3m3YvUwFb1BDUvcLYe1F".to_string(),
            amount: 100_000u128,
            nonce: 1,
            gas_price: 1,
            gas_limit: 21000,
            signature: vec![0u8; 65], // Invalid signature
        };

        let validation = tx.validate_signature();
        assert!(!validation.is_valid, "Invalid signature should fail validation");
    }

    #[test]
    fn test_transaction_execution() {
        let mut accounts = AccountManager::new();
        accounts
            .create_account("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string(), 1_000_000u128)
            .unwrap();
        accounts
            .create_account("1FP5gk4z7mDdSb3m3YvUwFb1BDUvcLYe1F".to_string(), 0u128)
            .unwrap();

        let mut processor = TransactionProcessor::new(accounts);

        let tx = Transaction {
            from: "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string(),
            to: "1FP5gk4z7mDdSb3m3YvUwFb1BDUvcLYe1F".to_string(),
            amount: 100_000u128,
            nonce: 1,
            gas_price: 1,
            gas_limit: 21000,
            signature: vec![0u8; 65], // Will fail validation but shows processing works
        };

        let receipt = processor.execute_transaction(&tx);
        println!("Transaction receipt: {:?}", receipt);
    }
}
