//! Bitcoin Address Support for Bitcoin Infinity Chain
//!
//! This module provides support for Bitcoin address-based accounts and secp256k1 signature
//! recovery, enabling users to sign transactions with their Bitcoin private keys.
//!
//! Phase 5.1 Implementation: Helper Functions for Transaction Validation Integration

use borsh::{BorshDeserialize, BorshSerialize};
use near_crypto::{PublicKey, Signature};
use near_primitives::account::AccessKey;
use near_primitives::errors::{ActionsValidationError, InvalidTxError};
use near_primitives::transaction::Action;
use near_primitives::trie_key::TrieKey;
use near_primitives::types::{AccountId, Balance, EpochHeight};
use near_primitives::version::ProtocolVersion;
use near_store::{StorageError, TrieAccess, TrieUpdate, get, get_access_key, set, set_access_key};

/// Hard-coded foundation account used by the Patoshi floor guard.
pub const FOUNDATION_ACCOUNT_ID: &str = "near";

/// Contract data key used to store a Patoshi record on an account.
pub const PATOSHI_RECORD_DATA_KEY: &[u8] = b"bitinfinity:patoshi:v1";

/// Contract data key storing the Borsh-encoded Patoshi account index (`Vec<String>`).
pub const PATOSHI_INDEX_DATA_KEY: &[u8] = b"bitinfinity:patoshi:index:v1";

/// Epoch delay before a successful unlock request becomes active.
pub const PATOSHI_UNLOCK_DELAY_EPOCHS: EpochHeight = 14;

/// Runtime state stored for Patoshi-locked accounts.
#[derive(Debug, Clone, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PatoshiRecord {
    /// Immutable floor in yoctoBIT.
    pub genesis_balance: Balance,
    /// `true` while the account is locked.
    pub is_locked: bool,
    /// Optional epoch height after which lock is lifted.
    pub unlock_epoch: Option<EpochHeight>,
}

pub fn foundation_account_id() -> AccountId {
    FOUNDATION_ACCOUNT_ID.parse().expect("FOUNDATION_ACCOUNT_ID must be a valid AccountId")
}

fn patoshi_record_trie_key(account_id: &AccountId) -> TrieKey {
    TrieKey::ContractData { account_id: account_id.clone(), key: PATOSHI_RECORD_DATA_KEY.to_vec() }
}

fn patoshi_index_trie_key() -> TrieKey {
    TrieKey::ContractData {
        account_id: foundation_account_id(),
        key: PATOSHI_INDEX_DATA_KEY.to_vec(),
    }
}

pub fn get_patoshi_record(
    trie: &dyn TrieAccess,
    account_id: &AccountId,
) -> Result<Option<PatoshiRecord>, StorageError> {
    get(trie, &patoshi_record_trie_key(account_id))
}

pub fn set_patoshi_record(
    state_update: &mut TrieUpdate,
    account_id: &AccountId,
    record: &PatoshiRecord,
) {
    set(state_update, patoshi_record_trie_key(account_id), record);
}

/// Reads the Patoshi account index from foundation contract-data.
///
/// Returns an empty vec if the index is absent (legacy genesis compatibility).
pub fn get_patoshi_index(trie: &dyn TrieAccess) -> Result<Vec<AccountId>, StorageError> {
    let Some(accounts): Option<Vec<String>> = get(trie, &patoshi_index_trie_key())? else {
        return Ok(Vec::new());
    };

    accounts
        .into_iter()
        .map(|account_id| {
            account_id.parse().map_err(|error| {
                StorageError::StorageInconsistentState(format!(
                    "Invalid account in Patoshi index `{}`: {}",
                    account_id, error
                ))
            })
        })
        .collect()
}

pub fn is_patoshi_locked(record: &PatoshiRecord, epoch_height: EpochHeight) -> bool {
    if !record.is_locked {
        return false;
    }
    match record.unlock_epoch {
        Some(unlock_epoch) => epoch_height < unlock_epoch,
        None => true,
    }
}

/// Computes how much Patoshi excess can be swept from liquid balance this epoch.
///
/// Returns `(sweep_amount, unswept_excess)`.
pub fn compute_patoshi_sweep(
    liquid_balance: Balance,
    locked_balance: Balance,
    genesis_balance: Balance,
) -> (Balance, Balance) {
    let total_balance = liquid_balance.saturating_add(locked_balance);
    let excess = total_balance.saturating_sub(genesis_balance);
    let sweep_amount = liquid_balance.min(excess);
    let unswept_excess = excess.saturating_sub(sweep_amount);
    (sweep_amount, unswept_excess)
}

fn action_name(action: &Action) -> &'static str {
    match action {
        Action::CreateAccount(_) => "CreateAccount",
        Action::DeployContract(_) => "DeployContract",
        Action::FunctionCall(_) => "FunctionCall",
        Action::Transfer(_) => "Transfer",
        Action::Stake(_) => "Stake",
        Action::AddKey(_) => "AddKey",
        Action::DeleteKey(_) => "DeleteKey",
        Action::DeleteAccount(_) => "DeleteAccount",
        Action::Delegate(_) => "Delegate",
        Action::DeployGlobalContract(_) => "DeployGlobalContract",
        Action::UseGlobalContract(_) => "UseGlobalContract",
        Action::TransferToGasKey(_) => "TransferToGasKey",
        Action::WithdrawFromGasKey(_) => "WithdrawFromGasKey",
        Action::DeterministicStateInit(_) => "DeterministicStateInit",
    }
}

fn patoshi_validation_error(
    reason: impl Into<String>,
    protocol_version: ProtocolVersion,
) -> InvalidTxError {
    InvalidTxError::ActionsValidation(ActionsValidationError::UnsupportedProtocolFeature {
        protocol_feature: reason.into(),
        version: protocol_version,
    })
}

/// Validate transaction-level Patoshi restrictions for a locked account.
pub fn validate_locked_patoshi_transaction(
    signer_id: &AccountId,
    receiver_id: &AccountId,
    actions: &[Action],
    post_tx_total_balance: Balance,
    epoch_height: EpochHeight,
    protocol_version: ProtocolVersion,
    record: &PatoshiRecord,
) -> Result<(), InvalidTxError> {
    if !is_patoshi_locked(record, epoch_height) {
        return Ok(());
    }

    let foundation_id = foundation_account_id();
    for action in actions {
        match action {
            // Stake/unstake stays on the sender account and is always allowed.
            Action::Stake(_) => {
                if receiver_id != signer_id {
                    return Err(patoshi_validation_error(
                        "patoshi_locked_stake_requires_self_receiver",
                        protocol_version,
                    ));
                }
            }
            // Transfers are only allowed to the foundation.
            Action::Transfer(_) => {
                if receiver_id != &foundation_id {
                    return Err(patoshi_validation_error(
                        "patoshi_locked_transfer_requires_foundation_receiver",
                        protocol_version,
                    ));
                }
            }
            _ => {
                return Err(patoshi_validation_error(
                    format!("patoshi_locked_action_blocked:{}", action_name(action)),
                    protocol_version,
                ));
            }
        }
    }

    if post_tx_total_balance < record.genesis_balance {
        return Err(patoshi_validation_error("patoshi_below_genesis_floor", protocol_version));
    }

    Ok(())
}

/// Canonical on-chain unlock trigger for locked Patoshi accounts.
///
/// The trigger is a single zero-value `Transfer` action to the foundation account.
pub fn is_patoshi_unlock_trigger(receiver_id: &AccountId, actions: &[Action]) -> bool {
    if receiver_id != &foundation_account_id() || actions.len() != 1 {
        return false;
    }
    matches!(actions.first(), Some(Action::Transfer(transfer)) if transfer.deposit.is_zero())
}

/// If `actions` contain a valid unlock trigger for a currently locked account,
/// schedule unlock by setting `unlock_epoch = current_epoch + PATOSHI_UNLOCK_DELAY_EPOCHS`.
///
/// Returns `Some(unlock_epoch)` when scheduling happened in this call, otherwise `None`.
pub fn maybe_schedule_patoshi_unlock(
    record: &mut PatoshiRecord,
    receiver_id: &AccountId,
    actions: &[Action],
    current_epoch: EpochHeight,
) -> Option<EpochHeight> {
    if !is_patoshi_locked(record, current_epoch) || record.unlock_epoch.is_some() {
        return None;
    }
    if !is_patoshi_unlock_trigger(receiver_id, actions) {
        return None;
    }

    let unlock_epoch = current_epoch.saturating_add(PATOSHI_UNLOCK_DELAY_EPOCHS);
    record.unlock_epoch = Some(unlock_epoch);
    Some(unlock_epoch)
}

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
            let hash_array: [u8; 32] = message_hash
                .try_into()
                .map_err(|_| "Message hash must be exactly 32 bytes".to_string())?;

            // Use nearcore's built-in recovery method
            let recovered_pubkey = sig
                .recover(hash_array)
                .map_err(|e| format!("Failed to recover public key: {}", e))?;

            // Derive all possible Bitcoin address formats from the recovered public key
            // Returns both P2PKH and P2WPKH addresses for matching
            let addresses =
                near_crypto::bitcoin_utils::derive_all_bitcoin_addresses(&recovered_pubkey);

            // Return the P2PKH address as primary (most common), but callers should
            // check all formats via derive_all_bitcoin_addresses
            let bitcoin_address = addresses.into_iter().next().unwrap_or_default();

            Ok((PublicKey::SECP256K1(recovered_pubkey), bitcoin_address))
        }
        _ => Err("Signature is not secp256k1; cannot recover public key".to_string()),
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
        let (recovered_pubkey, _primary_address) =
            recover_secp256k1_signature(message_hash, tx_signature)?;

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

    fn yocto(amount: u128) -> Balance {
        Balance::from_yoctonear(amount)
    }

    #[test]
    fn test_bitcoin_address_detection() {
        // Bech32 SegWit
        let bech32: AccountId = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4".parse().unwrap();
        assert!(is_bitcoin_address(&bech32));

        // Bech32 Taproot
        let taproot: AccountId =
            "bc1p2wsldez5mud2yam29q22wgfh9439spgduvct83k3pm50fcxa5dps59h4z5".parse().unwrap();
        assert!(is_bitcoin_address(&taproot));

        // Canonical P2PKH Base58Check
        let p2pkh: AccountId = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".parse().unwrap();
        assert!(is_bitcoin_address(&p2pkh));

        // Canonical P2SH Base58Check
        let p2sh: AccountId = "3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy".parse().unwrap();
        assert!(is_bitcoin_address(&p2sh));

        // Legacy lowercased compatibility path
        let p2pkh_legacy: AccountId = "1a1zp1ep5qgefi2dmptftl5slmv7divfna".parse().unwrap();
        assert!(is_bitcoin_address(&p2pkh_legacy));

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

    #[test]
    fn test_patoshi_lock_state_transition() {
        let locked =
            PatoshiRecord { genesis_balance: yocto(100), is_locked: true, unlock_epoch: None };
        assert!(is_patoshi_locked(&locked, 1));

        let unlocking =
            PatoshiRecord { genesis_balance: yocto(100), is_locked: true, unlock_epoch: Some(10) };
        assert!(is_patoshi_locked(&unlocking, 9));
        assert!(!is_patoshi_locked(&unlocking, 10));

        let unlocked =
            PatoshiRecord { genesis_balance: yocto(100), is_locked: false, unlock_epoch: None };
        assert!(!is_patoshi_locked(&unlocked, 1));
    }

    #[test]
    fn test_locked_patoshi_transfer_to_foundation_is_allowed() {
        use near_primitives::transaction::TransferAction;

        let signer: AccountId = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".parse().unwrap();
        let receiver = foundation_account_id();
        let actions = vec![Action::Transfer(TransferAction { deposit: yocto(10) })];
        let record =
            PatoshiRecord { genesis_balance: yocto(100), is_locked: true, unlock_epoch: None };

        let result = validate_locked_patoshi_transaction(
            &signer,
            &receiver,
            &actions,
            yocto(100),
            1,
            0,
            &record,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_locked_patoshi_transfer_to_non_foundation_is_rejected() {
        use near_primitives::transaction::TransferAction;

        let signer: AccountId = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".parse().unwrap();
        let receiver: AccountId = "bob.near".parse().unwrap();
        let actions = vec![Action::Transfer(TransferAction { deposit: yocto(10) })];
        let record =
            PatoshiRecord { genesis_balance: yocto(100), is_locked: true, unlock_epoch: None };

        let result = validate_locked_patoshi_transaction(
            &signer,
            &receiver,
            &actions,
            yocto(100),
            1,
            0,
            &record,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_locked_patoshi_rejects_non_transfer_stake_actions() {
        use near_primitives::action::FunctionCallAction;
        use near_primitives::types::Gas;

        let signer: AccountId = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".parse().unwrap();
        let receiver: AccountId = "bob.near".parse().unwrap();
        let actions = vec![Action::FunctionCall(Box::new(FunctionCallAction {
            method_name: "foo".to_string(),
            args: vec![],
            gas: Gas::from_gas(1),
            deposit: yocto(0),
        }))];
        let record =
            PatoshiRecord { genesis_balance: yocto(100), is_locked: true, unlock_epoch: None };

        let result = validate_locked_patoshi_transaction(
            &signer,
            &receiver,
            &actions,
            yocto(100),
            1,
            0,
            &record,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_locked_patoshi_floor_violation_is_rejected() {
        use near_primitives::transaction::TransferAction;

        let signer: AccountId = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".parse().unwrap();
        let receiver = foundation_account_id();
        let actions = vec![Action::Transfer(TransferAction { deposit: yocto(10) })];
        let record =
            PatoshiRecord { genesis_balance: yocto(100), is_locked: true, unlock_epoch: None };

        let result = validate_locked_patoshi_transaction(
            &signer,
            &receiver,
            &actions,
            yocto(99),
            1,
            0,
            &record,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_patoshi_unlock_trigger_requires_zero_transfer_to_foundation() {
        use near_primitives::transaction::TransferAction;

        let foundation = foundation_account_id();
        let non_foundation: AccountId = "bob.near".parse().unwrap();
        let zero_transfer = vec![Action::Transfer(TransferAction { deposit: yocto(0) })];
        let non_zero_transfer = vec![Action::Transfer(TransferAction { deposit: yocto(1) })];

        assert!(is_patoshi_unlock_trigger(&foundation, &zero_transfer));
        assert!(!is_patoshi_unlock_trigger(&foundation, &non_zero_transfer));
        assert!(!is_patoshi_unlock_trigger(&non_foundation, &zero_transfer));
    }

    #[test]
    fn test_maybe_schedule_patoshi_unlock_sets_unlock_epoch_once() {
        use near_primitives::transaction::TransferAction;

        let foundation = foundation_account_id();
        let actions = vec![Action::Transfer(TransferAction { deposit: yocto(0) })];
        let mut record =
            PatoshiRecord { genesis_balance: yocto(100), is_locked: true, unlock_epoch: None };

        let scheduled = maybe_schedule_patoshi_unlock(&mut record, &foundation, &actions, 10);
        assert_eq!(scheduled, Some(10 + PATOSHI_UNLOCK_DELAY_EPOCHS));
        assert_eq!(record.unlock_epoch, Some(10 + PATOSHI_UNLOCK_DELAY_EPOCHS));

        // A second request while already scheduled should be a no-op.
        let scheduled_again = maybe_schedule_patoshi_unlock(&mut record, &foundation, &actions, 11);
        assert_eq!(scheduled_again, None);
        assert_eq!(record.unlock_epoch, Some(10 + PATOSHI_UNLOCK_DELAY_EPOCHS));
    }

    #[test]
    fn test_compute_patoshi_sweep_sweeps_excess_from_liquid_balance() {
        // Total = 1_100, floor = 1_000 => excess = 100. Liquid has enough.
        let (sweep, unswept) = compute_patoshi_sweep(yocto(100), yocto(1_000), yocto(1_000));
        assert_eq!(sweep, yocto(100));
        assert_eq!(unswept, yocto(0));
    }

    #[test]
    fn test_compute_patoshi_sweep_reports_unswept_locked_excess() {
        // Total = 1_100, floor = 1_000 => excess = 100. Liquid only has 10.
        let (sweep, unswept) = compute_patoshi_sweep(yocto(10), yocto(1_090), yocto(1_000));
        assert_eq!(sweep, yocto(10));
        assert_eq!(unswept, yocto(90));
    }

    #[test]
    fn test_compute_patoshi_sweep_no_excess() {
        let (sweep, unswept) = compute_patoshi_sweep(yocto(0), yocto(1_000), yocto(1_000));
        assert_eq!(sweep, yocto(0));
        assert_eq!(unswept, yocto(0));
    }
}
