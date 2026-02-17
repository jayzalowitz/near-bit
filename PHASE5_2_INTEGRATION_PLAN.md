# Phase 5.2: Integration into Transaction Verifier

**Date**: February 16, 2026
**Status**: Planning Complete | Implementation Ready
**Objective**: Hook bitcoin_tx helper functions into nearcore's transaction verification flow

---

## Integration Point Located

**File**: `nearcore/runtime/runtime/src/verifier.rs`
**Function**: `validate_verify_and_charge_transaction()` (line 1232)
**Signature**:
```rust
pub fn validate_verify_and_charge_transaction(
    config: &RuntimeConfig,
    state_update: &mut TrieUpdate,
    signed_tx: SignedTransaction,
    gas_price: Balance,
    block_height: Option<BlockHeight>,
    current_protocol_version: ProtocolVersion,
) -> Result<VerificationResult, InvalidTxError>
```

**Why This Function**:
- Called for every transaction verification
- Has access to mutable `state_update` (needed for access key registration)
- Called BEFORE charge/gas verification
- Perfect place to register access keys transparently

---

## Current Flow (Lines 1240-1279)

```
INPUT: SignedTransaction signed_tx

↓

Step 1 (line 1240): Validate structure
  validated_tx = validate_transaction(config, signed_tx, ...)
  └─ Checks: well-formed actions, nonce range, etc.

↓

Step 2 (line 1244): Get signer and access key
  (signer, access_key) = get_signer_and_access_key(state_update, &validated_tx)
  └─ Problem: Access key doesn't exist for Bitcoin addresses!
  └─ Returns: InvalidAccessKeyError::AccessKeyNotFound

↓

Step 3 (line 1248-1272): Check verdict
  verdict = verify_and_charge_*_tx_ephemeral(...)
  └─ Validates charge, nonce, balance, etc.

↓

Step 4 (line 1278): Apply changes
  set_tx_state_changes(state_update, &validated_tx, &signer, &access_key)
  └─ Stores final state

↓

OUTPUT: VerificationResult
```

---

## Problem: Access Key Missing for Bitcoin Addresses

**Genesis Setup**:
- Bitcoin address accounts created with balance
- NO access key (pubkey unknown from UTXO scriptPubKey)
- When user signs first transaction:
  - get_signer_and_access_key() tries to lookup access key
  - Returns AccessKeyNotFound error
  - Transaction FAILS
  - User sees error, doesn't understand why

**Current Error** (line 170-176):
```rust
let access_key = match get_access_key(state_update, signer_id, validated_tx.public_key())? {
    Some(access_key) => access_key,
    None => {
        return Err(InvalidTxError::InvalidAccessKeyError(
            InvalidAccessKeyError::AccessKeyNotFound { ... }
        ));
    }
};
```

---

## Solution: Bitcoin-Aware Access Key Lookup

### Strategy: Pre-Register Keys Before Standard Lookup

**New Flow**:
```
INPUT: SignedTransaction signed_tx

↓

Step 1: Validate structure (unchanged)
  validated_tx = validate_transaction(...)

↓

Step 2: [NEW] Try Bitcoin address recovery and auto-registration
  IF signer_id is a Bitcoin address:
    │
    ├─ recover_secp256k1_signature(message_hash, signature)
    │  └─ Recover pubkey from signature
    │
    ├─ Verify address matches:
    │  └─ IF recovered address ≠ claimed signer_id → FAIL
    │
    └─ auto_register_access_key_if_needed(state_update, signer_id, recovered_pubkey)
       └─ IF no key exists: register it (first tx)
       └─ IF key exists: skip (subsequent tx)

↓

Step 3: Get signer and access key (unchanged)
  (signer, access_key) = get_signer_and_access_key(state_update, &validated_tx)
  └─ Now access key is guaranteed to exist (either from genesis or just registered)

↓

Step 4-5: Standard verification and state application (unchanged)
```

---

## Implementation: Code Changes Required

### Change 1: Add Bitcoin Recovery Call Before get_signer_and_access_key()

**Location**: `verifier.rs` lines 1240-1244

**Before**:
```rust
let validated_tx = match validate_transaction(config, signed_tx, current_protocol_version) {
    Ok(validated_tx) => validated_tx,
    Err((err, _tx)) => return Err(err),
};
let (mut signer, mut access_key) = get_signer_and_access_key(state_update, &validated_tx)?;
```

**After**:
```rust
let validated_tx = match validate_transaction(config, signed_tx, current_protocol_version) {
    Ok(validated_tx) => validated_tx,
    Err((err, _tx)) => return Err(err),
};

// Bitcoin Infinity: Try to auto-register Bitcoin address access keys on first transaction
if bitcoin_tx::is_bitcoin_address(validated_tx.signer_id()) {
    let tx_hash = validated_tx.to_tx().get_hash();
    let result = bitcoin_tx::verify_and_register_bitcoin_transaction(
        validated_tx.signature(),
        &tx_hash,
        validated_tx.signer_id(),
        state_update,
    );

    match result {
        Ok((valid, recovered_pubkey)) => {
            if !valid {
                return Err(InvalidTxError::InvalidSignature);
            }
        }
        Err(e) => {
            return Err(InvalidTxError::InvalidSignature); // Signature recovery failed
        }
    }
}

let (mut signer, mut access_key) = get_signer_and_access_key(state_update, &validated_tx)?;
```

**Why This Works**:
- For Bitcoin addresses: auto-registers key if first tx
- For NEAR addresses: skipped entirely
- Standard access key lookup now always succeeds for Bitcoin addresses

---

## Implementation Details

### Type Requirements

The implementation uses types from our Phase 5.1 work:
- `bitcoin_tx::is_bitcoin_address()` - Already defined
- `bitcoin_tx::verify_and_register_bitcoin_transaction()` - Already defined
- `near_crypto::Signature` - Already available in verifier.rs
- `ValidatedTransaction::signature()` - Check if method exists
- `Transaction::get_hash()` - Check signature

### Possible Issue: signature() method on ValidatedTransaction

Need to verify that `ValidatedTransaction` has a method to get the signature.

Let me check ValidatedTransaction in primitives:
```rust
pub struct ValidatedTransaction {
    signed_tx: SignedTransaction,
    tx_hash: CryptoHash,
    // ...
}
```

The signed_tx contains the signature, so we can access it as:
```rust
validated_tx.signed_tx.signature
```

Or better, just use the original signed_tx before converting to validated:
```rust
let signed_tx_copy = signed_tx.clone(); // If not already consumed
```

Actually, looking at the code, signed_tx is moved into validate_transaction(), so we need to access it from validated_tx. Let me check what methods are available.

### Revised Implementation (v2)

Rather than trying to access internals, I can modify validate_verify_and_charge_transaction to capture the signature BEFORE validation:

```rust
pub fn validate_verify_and_charge_transaction(
    config: &RuntimeConfig,
    state_update: &mut TrieUpdate,
    signed_tx: SignedTransaction,
    gas_price: Balance,
    block_height: Option<BlockHeight>,
    current_protocol_version: ProtocolVersion,
) -> Result<VerificationResult, InvalidTxError> {
    // [NEW] Capture signature and signer before moving signed_tx
    let original_signature = signed_tx.signature.clone();
    let original_signer_id = signed_tx.signer_id().clone();

    let validated_tx = match validate_transaction(config, signed_tx, current_protocol_version) {
        Ok(validated_tx) => validated_tx,
        Err((err, _tx)) => return Err(err),
    };

    // [NEW] Bitcoin address recovery
    if bitcoin_tx::is_bitcoin_address(&original_signer_id) {
        let tx_hash = validated_tx.to_tx().get_hash();
        let result = bitcoin_tx::verify_and_register_bitcoin_transaction(
            &original_signature,
            &tx_hash,
            &original_signer_id,
            state_update,
        );

        match result {
            Ok((valid, _recovered_pubkey)) => {
                if !valid {
                    return Err(InvalidTxError::InvalidSignature);
                }
            }
            Err(_e) => {
                return Err(InvalidTxError::InvalidSignature);
            }
        }
    }

    // Continue with standard flow
    let (mut signer, mut access_key) = get_signer_and_access_key(state_update, &validated_tx)?;
    let transaction_cost = tx_cost(config, &validated_tx.to_tx(), gas_price)?;
    // ... rest unchanged
}
```

---

## Testing Strategy

### Unit Test: Bitcoin Address Recovery
**File**: `nearcore/runtime/runtime/src/verifier.rs` (in tests module)

```rust
#[test]
fn test_bitcoin_address_first_transaction_auto_registers_key() {
    // Setup
    let config = make_runtime_config();
    let mut state_update = make_test_trie_update();

    // Create account with Bitcoin address, no access key
    let bitcoin_addr = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".parse().unwrap();
    let account = Account::new(1_000_000, 0, 0, AccountContract::None);
    set_account(&mut state_update, bitcoin_addr.clone(), &account);

    // Create and sign transaction
    let signed_tx = create_bitcoin_signed_tx(&bitcoin_addr, ...);

    // Verify and charge
    let result = validate_verify_and_charge_transaction(
        &config,
        &mut state_update,
        signed_tx,
        1,
        None,
        PROTOCOL_VERSION,
    );

    // Assert success
    assert!(result.is_ok());

    // Verify access key was registered
    let stored_key = get_access_key(&state_update, &bitcoin_addr, &recovered_pubkey);
    assert!(stored_key.is_ok());
}
```

### Integration Test: Mixed Accounts
```rust
#[test]
fn test_mixed_bitcoin_and_near_accounts() {
    // Create both Bitcoin and NEAR-style accounts
    // Both with full access keys (Bitcoin recovered, NEAR pre-registered)
    // Send transactions from both
    // Verify both succeed
}
```

### End-to-End Test: Testnet
```bash
# 1. Generate testnet with Bitcoin addresses
sydney-tools generate-genesis --testnet --num-accounts 10

# 2. Extract private keys
# 3. Start neard with testnet genesis
# 4. Create Bitcoin secp256k1 signatures
# 5. Submit first transaction from Bitcoin address
# 6. Verify: transaction succeeds, balance transferred
# 7. Submit second transaction
# 8. Verify: uses cached access key, succeeds instantly
```

---

## Error Handling

### InvalidSignature Error
When Bitcoin signature recovery fails:
```rust
Err(InvalidTxError::InvalidSignature) {
    signer_id: bitcoin_address,
    transaction_hash: tx_hash,
}
```

**User sees**: "Signature verification failed"
**Actual cause**: Secp256k1 recovery failed (corrupt signature, wrong key, etc.)

### AccessKeyNotFound (should not happen)
If we reach get_signer_and_access_key and the key doesn't exist:
- For Bitcoin addresses: SHOULD NOT HAPPEN (auto-registered above)
- For NEAR addresses: existing behavior, already handled
- **Mitigation**: Double-check that auto-registration completed successfully

---

## Implementation Checklist

- [ ] Add `use bitcoin_tx` import in verifier.rs
- [ ] Capture signature and signer_id before validate_transaction
- [ ] Add Bitcoin address detection and recovery call
- [ ] Handle errors appropriately (return InvalidSignature)
- [ ] Test with synthetic Bitcoin transactions
- [ ] Verify first tx auto-registers key
- [ ] Verify subsequent txs use cached key
- [ ] Test with mixed Bitcoin + NEAR accounts
- [ ] Run existing verifier tests to ensure no regression

---

## Files to Modify

| File | Changes | Lines |
|------|---------|-------|
| `nearcore/runtime/runtime/src/verifier.rs` | Add Bitcoin recovery call in validate_verify_and_charge_transaction | 1244-1245 (new code inserted) |
| `nearcore/runtime/runtime/src/verifier.rs` | Add import for bitcoin_tx module | 1-50 |

---

## Success Criteria (Phase 5.2)

- [ ] Bitcoin addresses can be used as account IDs
- [ ] First transaction from Bitcoin address auto-registers key transparently
- [ ] Subsequent transactions use cached key (fast path)
- [ ] NEAR-style accounts continue to work unchanged
- [ ] All existing verifier tests pass
- [ ] New tests for Bitcoin address flow pass
- [ ] Code compiles without warnings
- [ ] No performance regression

---

## Timeline

**Phase 5.2**: 2-3 hours
- 30 min: Modify verifier.rs
- 1 hour: Write and run tests
- 30 min: Debug and fix issues
- 30 min: Document and commit

**Total Phase 5**: ~1 week after 5.1 + 5.2 completed

---

## Key Innovation: Invisible to User

From the user's perspective:
```
Bitcoin Wallet
    ↓
[User has BTC key] → [Signs transaction] → [Submits to Sydney]
                                              ↓
                                        Sydney validates
                                        Recovers pubkey
                                        Auto-registers key (transparent)
                                              ↓
                                        [First tx succeeds]
                                        Balance shows updated
```

No claiming, no bridging, no difference from NEAR wallets. Just works.

---

**Status**: Implementation plan complete, ready to code Phase 5.2
