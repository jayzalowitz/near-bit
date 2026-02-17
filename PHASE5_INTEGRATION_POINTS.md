# Phase 5: Critical Integration Points Identified

**Status**: Exploration Complete ✅  
**Date**: February 16, 2026

---

## Nearcore Architecture Map

### Transaction Flow
```
Transaction arrives
  ↓
nearcore/core/primitives/src/transaction.rs::SignedTransaction
  ├─ Contains: signer_id, signature, actions, block_hash
  └─ Methods: sign(), get_hash(), verify()
  ↓
Access Key Lookup
  ├─ nearcore/runtime/runtime/src/actions.rs
  ├─ Functions: get_access_key(), set_access_key()
  └─ Access key contains: nonce, permission (Full or FunctionCall)
  ↓
Signature Verification
  ├─ nearcore/core/primitives/src/transaction.rs::verify_transaction_signature()
  ├─ Input: transaction, public_keys
  └─ Verifies: signature.verify(hash, key)
  ↓
Action Execution
  └─ Transfer, FunctionCall, CreateAccount, etc.
```

### Critical Files to Modify

**1. nearcore/core/primitives/src/transaction.rs** (VERIFY INTEGRATION POINTS)
- Line 661: `verify_transaction_signature()` - verification function
- Line 352: `SignedTransaction` struct - transaction definition
- **Action**: Create wrapper function for Bitcoin address handling

**2. nearcore/runtime/runtime/src/actions.rs** (INTEGRATE ACCESS KEY AUTO-REGISTER)
- Line 101, 120: `get_access_key()` calls - where access keys are retrieved
- Line 110, 134: `set_access_key()` calls - where access keys are stored
- **Action**: Add Bitcoin address detection and auto-registration logic

**3. nearcore/core/crypto/src/bitcoin_utils.rs** (ALREADY CREATED ✅)
- `derive_bitcoin_address_from_pubkey()` function ready
- **Action**: Ensure public visibility for actions.rs to use

**4. nearcore/core/primitives-core/src/types.rs** (VERIFY ACCOUNT ID)
- Account ID parsing and validation
- **Action**: Ensure Bitcoin addresses are accepted as valid account IDs

---

## Integration Strategy

### Option A: Wrapper Function (RECOMMENDED)
Create a new function that handles both ED25519 and secp256k1:

```rust
pub fn verify_and_register_transaction_signature(
    transaction: &SignedTransaction,
    public_keys: &[PublicKey],
    state_update: &mut TrieUpdate,  // For auto-register
    account_id: &AccountId,
) -> Result<(bool, Option<PublicKey>), Error> {
    // Check if Bitcoin address
    if is_bitcoin_address(&transaction.signer_id) {
        // Perform signature recovery
        let recovered_pubkey = recover_secp256k1_signature(
            transaction.get_hash().as_ref(),
            &transaction.signature
        )?;
        
        // Verify address matches
        if derived_address != transaction.signer_id {
            return Ok((false, None));  // Signature doesn't match
        }
        
        // Auto-register access key if needed
        if get_access_key(state_update, account_id, &recovered_pubkey)?.is_none() {
            // First transaction - auto-register
            let access_key = AccessKey::full_access();
            set_access_key(state_update, account_id.clone(), recovered_pubkey.clone(), &access_key);
        }
        
        Ok((true, Some(recovered_pubkey)))
    } else {
        // Existing ED25519 flow
        let valid = verify_transaction_signature(transaction, public_keys);
        Ok((valid, None))
    }
}
```

### Option B: Conditional Logic in Actions
Add Bitcoin address check in the access key lookup function.

**Recommended**: Option A (cleaner, isolated logic)

---

## Helper Function Specifications

### 1. Bitcoin Address Detection
```rust
fn is_bitcoin_address(account_id: &AccountId) -> bool {
    let addr_str = account_id.as_str();
    addr_str.starts_with('1') ||  // P2PKH
    addr_str.starts_with('3') ||  // P2SH
    addr_str.starts_with("bc1")   // SegWit/Taproot
}
```

### 2. Signature Recovery Integration
```rust
fn recover_secp256k1_signature(
    message_hash: &[u8],
    signature: &Signature,  // 65-byte recoverable signature
) -> Result<(PublicKey, String), String> {
    // Extract secp256k1 signature
    if let Signature::SECP256K1(sig) = signature {
        let pubkey = sig.recover(&message_hash)?;  // Already in nearcore!
        let address = bitcoin_utils::derive_bitcoin_address_from_pubkey(&pubkey)?;
        Ok((PublicKey::SECP256K1(pubkey), address))
    } else {
        Err("Not a secp256k1 signature".to_string())
    }
}
```

### 3. Access Key Auto-Registration
```rust
fn auto_register_access_key_if_needed(
    state_update: &mut TrieUpdate,
    account_id: &AccountId,
    pubkey: &PublicKey,
) -> Result<bool, StorageError> {
    if get_access_key(state_update, account_id, pubkey)?.is_none() {
        // First transaction - register it
        let access_key = AccessKey::full_access();
        set_access_key(state_update, account_id.clone(), pubkey.clone(), &access_key);
        Ok(true)  // Was registered
    } else {
        Ok(false)  // Already had it
    }
}
```

---

## Integration Points Summary

| Component | Location | Change Required | Priority |
|-----------|----------|-----------------|----------|
| Bitcoin address detection | Helper | NEW | HIGH |
| Signature recovery integration | transaction.rs | WRAPPER | HIGH |
| Access key auto-registration | actions.rs | LOGIC | HIGH |
| Bitcoin address validation | near-account-id | VERIFY ✅ | LOW |
| Public key recovery | bitcoin_utils.rs | EXPORT ✅ | LOW |

---

## Implementation Roadmap

### Phase 5.1: Create Helper Functions (This Week)
- [ ] Implement `is_bitcoin_address()`
- [ ] Implement `recover_secp256k1_signature()`
- [ ] Implement `auto_register_access_key_if_needed()`
- [ ] Create wrapper function

### Phase 5.2: Integrate into Actions (Next Week)
- [ ] Hook wrapper into action execution
- [ ] Test with Bitcoin secp256k1 signatures
- [ ] Validate auto-registration

### Phase 5.3: Deploy Testnet (Following Week)
- [ ] Deploy single-node testnet
- [ ] Test end-to-end
- [ ] Performance validation

---

## Risk Assessment

**Low Risk**:
- ✅ Signature recovery already in nearcore
- ✅ Bitcoin address derivation already implemented
- ✅ Helper functions are self-contained

**Medium Risk**:
- ⚠️ Integration with existing flow must not break ED25519
- ⚠️ Access key auto-registration must happen before action execution

**Mitigation**:
- Keep all Bitcoin-specific logic isolated
- Comprehensive unit tests with both ED25519 and secp256k1
- Integration tests for access key registration flow

---

## Files Ready to Use

✅ `nearcore/core/crypto/src/bitcoin_utils.rs` - Address derivation  
✅ `nearcore/core/crypto/src/signature.rs` - Signature recovery already there  
✅ `nearcore/core/primitives/src/transaction.rs` - Transaction structures  
✅ `nearcore/runtime/runtime/src/actions.rs` - Access key management  
✅ `near-account-id` - Bitcoin address validation  

**Status**: All infrastructure in place, ready for integration.

---

## Next Steps

1. **Create helper functions** in new file: `nearcore/runtime/runtime/src/bitcoin_tx.rs`
2. **Modify transaction.rs** to export wrapper function
3. **Modify actions.rs** to call wrapper and auto-register keys
4. **Write unit tests** for all helper functions
5. **Deploy testnet** and validate

**Timeline**: Start immediately, complete within 1-2 weeks

