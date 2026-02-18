//! Bitcoin Address Support for Bitcoin Infinity Chain
//!
//! This module provides support for Bitcoin address-based accounts and secp256k1 signature
//! recovery, enabling users to sign transactions with their Bitcoin private keys.
//!
//! Phase 5.1 Implementation: Helper Functions for Transaction Validation Integration

use near_crypto::{PublicKey, Signature};
use near_primitives::types::AccountId;
use near_primitives::account::AccessKey;
use near_store::{StorageError, TrieUpdate, get_access_key, set_access_key};

/// Detects if an account ID is a Bitcoin address (as opposed to NEAR-style).
///
/// Bitcoin addresses come in several formats:
/// - P2PKH (legacy): Starts with '1', 25-34 characters
/// - P2SH (multisig): Starts with '3', 34 characters
/// - P2WPKH (SegWit): Starts with 'bc1q', 42 characters
/// - P2WSH (SegWit 32B): Starts with 'bc1q', 62 characters
/// - P2TR (Taproot): Starts with 'bc1p', 62 characters
///
/// # Arguments
/// * `account_id` - The account ID string to check
///
/// # Returns
/// `true` if this appears to be a Bitcoin address, `false` otherwise
pub fn is_bitcoin_address(account_id: &AccountId) -> bool {
    use near_primitives_core::account::id::AccountType;
    matches!(account_id.get_account_type(), AccountType::BtcImplicitAccount)
}

/// Recovers a secp256k1 public key from a signature.
///
/// This is the core mechanism for Bitcoin address account access: when a user signs with their
/// Bitcoin private key, we recover the public key from the signature, derive the Bitcoin address,
/// and verify it matches the claimed sender. This happens transparently on the first transaction.
///
/// The signature must be a 65-byte recoverable ECDSA signature (r, s, recovery_id).
///
/// # Arguments
/// * `message_hash` - The 32-byte message hash that was signed
/// * `signature` - The signature object (must be SECP256K1 variant)
///
/// # Returns
/// * `Ok((pubkey, bitcoin_address))` - Recovered public key and derived Bitcoin address
/// * `Err(message)` - If signature is not secp256k1 or recovery fails
pub fn recover_secp256k1_signature(
    message_hash: &[u8],
    signature: &Signature,
) -> Result<(PublicKey, String), String> {
    // Only secp256k1 signatures support public key recovery
    match signature {
        Signature::SECP256K1(sig) => {
            // Convert message hash slice to fixed-size 32-byte array
            let hash_array: [u8; 32] = message_hash.try_into()
                .map_err(|_| "Message hash must be exactly 32 bytes".to_string())?;

            // Use nearcore's built-in recovery method
            let recovered_pubkey = sig
                .recover(hash_array)
                .map_err(|e| format!("Failed to recover public key: {}", e))?;

            // Derive all possible Bitcoin address formats from the recovered public key
            // Returns both P2PKH and P2WPKH addresses for matching
            let addresses = near_crypto::bitcoin_utils::derive_all_bitcoin_addresses(&recovered_pubkey);

            // Return the P2PKH address as primary (most common), but callers should
            // check all formats via derive_all_bitcoin_addresses
            let bitcoin_address = addresses.into_iter().next()
                .unwrap_or_default();

            Ok((PublicKey::SECP256K1(recovered_pubkey), bitcoin_address))
        }
        _ => {
            Err("Signature is not secp256k1; cannot recover public key".to_string())
        }
    }
}

/// Automatically registers an access key if not already present.
///
/// This function is called transparently when processing the first transaction from a Bitcoin
/// address account. The recovered public key is stored as a FullAccess access key, allowing
/// subsequent transactions to skip recovery and use standard access key lookup.
///
/// From the user's perspective, there is no difference between the first and subsequent
/// transactions - they just sign and send. This registration happens invisibly.
///
/// # Arguments
/// * `state_update` - The trie update to write to
/// * `account_id` - The account (Bitcoin address) to register the key for
/// * `pubkey` - The recovered public key
///
/// # Returns
/// * `Ok(true)` - Key was newly registered (first transaction)
/// * `Ok(false)` - Key already existed
/// * `Err(StorageError)` - Storage error during lookup or write
pub fn auto_register_access_key_if_needed(
    state_update: &mut TrieUpdate,
    account_id: &AccountId,
    pubkey: &PublicKey,
) -> Result<bool, StorageError> {
    // Check if access key already exists
    match get_access_key(state_update, account_id, pubkey)? {
        Some(_) => {
            // Access key already registered, skip
            Ok(false)
        }
        None => {
            // First transaction from this Bitcoin address account
            // Register the recovered public key as a full access key
            let access_key = AccessKey::full_access();
            set_access_key(state_update, account_id.clone(), pubkey.clone(), &access_key);
            Ok(true)
        }
    }
}

/// Wrapper for verifying and registering Bitcoin transactions.
///
/// This combines signature verification and access key registration into a single step.
/// For Bitcoin addresses, it recovers the public key, verifies the signature, and registers
/// the access key if needed (on first transaction).
///
/// # Arguments
/// * `tx_signature` - The transaction signature
/// * `message_hash` - The 32-byte message hash
/// * `signer_id` - The claimed signer (account ID)
/// * `state_update` - The trie update for potential access key registration
///
/// # Returns
/// * `Ok((valid, Some(pubkey)))` - For Bitcoin addresses: (always true if matches, recovered pubkey)
/// * `Ok((true, None))` - For NEAR addresses: pass through to standard verification
/// * `Err(message)` - If verification fails
pub fn verify_and_register_bitcoin_transaction(
    tx_signature: &Signature,
    message_hash: &[u8],
    signer_id: &AccountId,
    state_update: &mut TrieUpdate,
) -> Result<(bool, Option<PublicKey>), String> {
    // Check if this is a Bitcoin address account
    if is_bitcoin_address(signer_id) {
        // Try to recover the public key from the secp256k1 signature
        let (recovered_pubkey, _primary_address) = recover_secp256k1_signature(message_hash, tx_signature)?;

        // Try all address derivation formats (P2PKH, P2WPKH) to match the signer
        let secp_key = match &recovered_pubkey {
            PublicKey::SECP256K1(k) => k,
            _ => return Err("Expected secp256k1 public key".to_string()),
        };
        let all_addresses = near_crypto::bitcoin_utils::derive_all_bitcoin_addresses(secp_key);

        let signer_str = signer_id.as_str();
        let matched = all_addresses.iter().any(|addr| addr == signer_str);
        if !matched {
            return Ok((false, None)); // Signature doesn't match claimed sender in any format
        }

        // Auto-register the access key if this is the first transaction
        // Note: This may fail with StorageError, but we propagate it as a String for now
        let _ = auto_register_access_key_if_needed(state_update, signer_id, &recovered_pubkey)
            .map_err(|e| format!("Failed to register access key: {}", e))?;

        // Transaction is valid, return the recovered pubkey
        Ok((true, Some(recovered_pubkey)))
    } else {
        // For non-Bitcoin addresses, use standard ED25519 verification
        // (This is handled by existing nearcore code)
        Ok((true, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitcoin_address_detection() {
        // Bech32 SegWit (lowercase, valid as NEAR AccountId)
        let bech32: AccountId = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4".parse().unwrap();
        assert!(is_bitcoin_address(&bech32));

        // Bech32 Taproot
        let taproot: AccountId = "bc1pqw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4".parse().unwrap();
        assert!(is_bitcoin_address(&taproot));

        // P2PKH-style (lowercased — real P2PKH uses mixed case base58check,
        // but NEAR AccountId requires lowercase. Full base58check support
        // requires modifying near-account-id.)
        let p2pkh: AccountId = "1a1zp1ep5qgefi2dmptftl5slmv7divfna".parse().unwrap();
        assert!(is_bitcoin_address(&p2pkh));

        // P2SH-style (lowercased)
        let p2sh: AccountId = "3j98t1wpez73cnmyviecrnyiwrnqrhwnly".parse().unwrap();
        assert!(is_bitcoin_address(&p2sh));

        // NEAR-style address
        let near_address: AccountId = "alice.near".parse().unwrap();
        assert!(!is_bitcoin_address(&near_address));

        // Hex NEAR implicit account
        let near_implicit: AccountId = "0123456789abcdef0123456789abcdef".parse().unwrap();
        assert!(!is_bitcoin_address(&near_implicit));
    }

    #[test]
    fn test_is_bitcoin_address_edge_cases() {
        // Address starting with number other than 1 or 3
        let not_btc: AccountId = "2a1zp1ep5qgefi2dmptftl5slmv7divfna".parse().unwrap();
        assert!(!is_bitcoin_address(&not_btc));

        // Address starting with 'bc' but not 'bc1'
        let not_btc2: AccountId = "bcqw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4".parse().unwrap();
        assert!(!is_bitcoin_address(&not_btc2));

        // Short non-Bitcoin account
        let unknown: AccountId = "xx".parse().unwrap();
        assert!(!is_bitcoin_address(&unknown));
    }
}
