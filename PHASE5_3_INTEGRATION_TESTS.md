# Phase 5.3: Integration Tests & Testnet Deployment

**Date**: February 16-17, 2026
**Status**: Planning & Implementation in Progress
**Objective**: Validate Bitcoin address support end-to-end

---

## Test Strategy Overview

### Test Levels

1. **Unit Tests** (Phase 5.1) ✅
   - Individual function behavior
   - 4 tests for `is_bitcoin_address()`

2. **Integration Tests** (Phase 5.3) - THIS PHASE
   - Bitcoin address transaction flow
   - Signature recovery + registration
   - Mixed Bitcoin + NEAR accounts
   - Error handling

3. **Testnet Tests** (Phase 5.3)
   - Single-node network deployment
   - Real block production
   - Transaction execution
   - State consistency

4. **End-to-End Tests** (Phase 5.3)
   - Bitcoin wallet → Sydney transaction
   - Balance verification
   - Multi-transaction flow

---

## Integration Test Suite

### Test 1: Bitcoin Address First Transaction Auto-Registration

**Objective**: Verify that first transaction from a Bitcoin address auto-registers the access key

**Setup**:
```rust
// Create account with Bitcoin address, no access key (like genesis)
let bitcoin_addr = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".parse().unwrap();
let account = Account::new(1_000_000_000_000, 0, 0, AccountContract::None);
set_account(&mut state_update, bitcoin_addr.clone(), &account);

// Create secp256k1 signed transaction
let private_key = create_test_secp256k1_key();
let signed_tx = create_signed_transaction(
    &bitcoin_addr,
    &private_key,
    vec![Action::Transfer(TransferAction { deposit: 1_000_000 })],
);
```

**Execution**:
```rust
let result = validate_verify_and_charge_transaction(
    &config,
    &mut state_update,
    signed_tx,
    1,  // gas_price
    None,
    PROTOCOL_VERSION,
);
```

**Expectations**:
- ✅ Transaction validates successfully
- ✅ Signature recovery succeeds
- ✅ Address derivation matches sender
- ✅ Access key is registered (check with `get_access_key()`)
- ✅ Balance is updated

**What It Tests**:
- `is_bitcoin_address()` correctly identifies Bitcoin accounts
- `recover_secp256k1_signature()` recovers public key
- `auto_register_access_key_if_needed()` registers key on first tx

---

### Test 2: Bitcoin Address Subsequent Transaction Uses Cached Key

**Objective**: Verify that second transaction uses cached access key (fast path)

**Setup**: Start where Test 1 ends (access key already registered)

**Execution**: Send another transaction from the same Bitcoin address

**Expectations**:
- ✅ Transaction validates instantly (uses cached key, no recovery)
- ✅ No signature recovery overhead
- ✅ Balance updated correctly
- ✅ Nonce incremented

**What It Tests**:
- Fast path performance after first transaction
- Access key lookup works correctly
- Standard NEAR verification flow

---

### Test 3: Invalid Bitcoin Signature Rejected

**Objective**: Verify that signatures not matching the Bitcoin address are rejected

**Setup**:
```rust
// Create Bitcoin address
let bitcoin_addr_1 = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".parse().unwrap();

// Create DIFFERENT Bitcoin address
let bitcoin_addr_2 = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNb".parse().unwrap();

// Create transaction claiming to be from address 1
// But sign with address 2's private key
let private_key_2 = create_test_secp256k1_key();
let signed_tx = create_signed_transaction(
    &bitcoin_addr_1,  // Claimed sender
    &private_key_2,   // But signed with key from address 2
    vec![Action::Transfer(...)],
);
```

**Execution**:
```rust
let result = validate_verify_and_charge_transaction(
    &config,
    &mut state_update,
    signed_tx,
    1,
    None,
    PROTOCOL_VERSION,
);
```

**Expectations**:
- ✅ Transaction rejected with `InvalidSignature` error
- ✅ Signature recovery succeeds but address mismatch detected
- ✅ No state changes applied
- ✅ No access key registered

**What It Tests**:
- Signature verification catches mismatches
- Address derivation is correct
- Security: can't forge transactions

---

### Test 4: Corrupt Signature Handling

**Objective**: Verify that invalid signatures are handled gracefully

**Setup**:
```rust
// Create transaction with intentionally corrupt signature
let corrupt_sig = Signature::SECP256K1(
    // Invalid secp256k1 signature
);
```

**Execution**: Try to validate transaction with corrupt signature

**Expectations**:
- ✅ Signature recovery fails
- ✅ Returns `InvalidSignature` error
- ✅ Transaction rejected
- ✅ No state corruption

**What It Tests**:
- Error handling in signature recovery
- No panics on invalid data
- Graceful failure

---

### Test 5: Mixed Bitcoin and NEAR Accounts

**Objective**: Verify both account types work in same transaction flow

**Setup**:
```rust
// Create both Bitcoin and NEAR accounts
let bitcoin_addr = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".parse().unwrap();
let near_addr = "alice.near".parse().unwrap();

// Both with balances
set_account(&mut state_update, bitcoin_addr.clone(), &bitcoin_account);
set_account(&mut state_update, near_addr.clone(), &near_account);

// Pre-register NEAR access key (as normal)
set_access_key(&mut state_update, near_addr.clone(), &near_pubkey, &access_key);
```

**Execution**:
```rust
// Test 1: Bitcoin address (signature recovery)
let result_btc = validate_verify_and_charge_transaction(
    &config, &mut state_update, bitcoin_signed_tx, 1, None, PROTOCOL_VERSION
);

// Test 2: NEAR address (standard path)
let result_near = validate_verify_and_charge_transaction(
    &config, &mut state_update, near_signed_tx, 1, None, PROTOCOL_VERSION
);
```

**Expectations**:
- ✅ Bitcoin address validates via signature recovery
- ✅ NEAR address validates via standard path
- ✅ Both succeed independently
- ✅ No interference between account types

**What It Tests**:
- Bitcoin path and NEAR path work simultaneously
- No crosstalk or conflicts
- Both transaction types succeed
- Architecture design is sound

---

### Test 6: Access Key Nonce Increment

**Objective**: Verify that nonce is properly incremented across multiple transactions

**Setup**: Same as Test 2 (subsequent transaction)

**Execution**:
```rust
// Send 3 transactions in sequence
for i in 1..=3 {
    let result = validate_verify_and_charge_transaction(
        &config, &mut state_update, signed_tx_i, 1, None, PROTOCOL_VERSION
    );
    assert!(result.is_ok());

    // Get access key and check nonce
    let access_key = get_access_key(&state_update, &bitcoin_addr, &pubkey)?;
    assert_eq!(access_key.nonce, i);  // Nonce incremented
}
```

**Expectations**:
- ✅ Nonce increments after each transaction
- ✅ Future transactions with old nonce are rejected
- ✅ Proper ordering enforced
- ✅ No replay attacks possible

**What It Tests**:
- Nonce management works correctly
- Transaction ordering is enforced
- Security against replays

---

## Test Implementation Code Template

```rust
#[cfg(test)]
mod bitcoin_address_tests {
    use super::*;
    use near_primitives::test_utils::*;

    fn setup_bitcoin_account(
        state_update: &mut TrieUpdate,
        bitcoin_addr: &AccountId,
        balance: Balance,
    ) {
        let account = Account::new(balance, 0, 0, AccountContract::None);
        set_account(state_update, bitcoin_addr.clone(), &account);
    }

    fn create_bitcoin_signed_tx(
        signer_id: &AccountId,
        private_key: &SecretKey,
        nonce: u64,
    ) -> SignedTransaction {
        let tx = Transaction {
            signer_id: signer_id.clone(),
            public_key: private_key.public_key(),
            nonce,
            receiver_id: "receiver.near".parse().unwrap(),
            block_hash: CryptoHash::default(),
            actions: vec![Action::Transfer(TransferAction { deposit: 1_000 })],
        };
        SignedTransaction::new(
            Signature::SECP256K1(private_key.sign(tx.get_hash().as_ref())),
            tx,
        )
    }

    #[test]
    fn test_bitcoin_first_tx_auto_registers_key() {
        let config = make_runtime_config();
        let mut state_update = make_test_trie_update();

        let bitcoin_addr = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".parse().unwrap();
        setup_bitcoin_account(&mut state_update, &bitcoin_addr, 1_000_000_000_000);

        let private_key = create_test_secp256k1_key();
        let signed_tx = create_bitcoin_signed_tx(&bitcoin_addr, &private_key, 1);

        // Validate transaction
        let result = validate_verify_and_charge_transaction(
            &config,
            &mut state_update,
            signed_tx.clone(),
            1,
            None,
            PROTOCOL_VERSION,
        );

        // Check success
        assert!(result.is_ok());

        // Verify access key was registered
        let recovered_pubkey = get_recovered_pubkey_from_tx(&signed_tx).unwrap();
        let access_key = get_access_key(&state_update, &bitcoin_addr, &recovered_pubkey);
        assert!(access_key.is_ok());
        assert!(access_key.unwrap().is_some());
    }

    #[test]
    fn test_bitcoin_invalid_signature_rejected() {
        let config = make_runtime_config();
        let mut state_update = make_test_trie_update();

        let bitcoin_addr = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".parse().unwrap();
        setup_bitcoin_account(&mut state_update, &bitcoin_addr, 1_000_000_000_000);

        // Create signature from DIFFERENT key
        let wrong_key = create_test_secp256k1_key();
        let signed_tx = create_bitcoin_signed_tx(&bitcoin_addr, &wrong_key, 1);

        let result = validate_verify_and_charge_transaction(
            &config,
            &mut state_update,
            signed_tx,
            1,
            None,
            PROTOCOL_VERSION,
        );

        // Should fail with InvalidSignature
        assert!(matches!(result, Err(InvalidTxError::InvalidSignature)));
    }
}
```

---

## Testnet Deployment Strategy

### Phase 5.3a: Unit & Integration Tests (2 hours)
```bash
cd nearcore/runtime/runtime
cargo test bitcoin_address_tests -- --nocapture
```

### Phase 5.3b: Testnet Genesis (30 min)
```bash
# Generate testnet with 10 Bitcoin addresses
cargo run -p sydney-tools -- generate-genesis \
    --testnet \
    --num-accounts 10 \
    --output-dir ./genesis-testnet/

# Extract private keys for testing
extract_bitcoin_private_keys.sh ./genesis-testnet/ > test_keys.txt
```

### Phase 5.3c: Single-Node Testnet Deployment (1 hour)
```bash
# Initialize testnet
cargo run -p bitinfinity-neard -- init \
    --home ~/.sydney-testnet \
    --chain-id sydney-testnet

# Copy genesis from generation step
cp ./genesis-testnet/genesis.json ~/.sydney-testnet/

# Start node
cargo run -p bitinfinity-neard -- run \
    --home ~/.sydney-testnet
```

### Phase 5.3d: End-to-End Testing (1 hour)
```bash
# In separate terminal, run tests
cargo run --example bitcoin_testnet_validation -- \
    --keys ./test_keys.txt \
    --rpc http://127.0.0.1:3030
```

Expected output:
```
[✓] Bitcoin address 1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa connected
[✓] First transaction sent (auto-registration)
[✓] Access key registered transparently
[✓] Balance updated: 999999 SYD
[✓] Second transaction sent (cached key)
[✓] Transaction confirmed immediately
[✓] All 10 test addresses working
[✓] Mixed Bitcoin+NEAR accounts working
SUCCESS: Bitcoin Infinity testnet validation complete
```

---

## Success Criteria (Phase 5.3)

- [ ] All 6 integration tests pass
- [ ] Bitcoin address signature recovery works
- [ ] First transaction auto-registration transparent
- [ ] Subsequent transactions use cached keys
- [ ] Invalid signatures properly rejected
- [ ] Mixed account types work simultaneously
- [ ] Testnet deploys and runs stably
- [ ] Single-node network produces blocks
- [ ] Bitcoin addresses can send transactions
- [ ] No regressions in NEAR-style accounts
- [ ] All code compiles clean
- [ ] No memory leaks or panics

---

## Timeline

| Task | Duration | Status |
|------|----------|--------|
| Write integration tests | 1-2 hours | ⏳ NEXT |
| Run tests locally | 30 min | ⏳ AFTER TESTS |
| Generate testnet genesis | 30 min | ⏳ AFTER TESTS |
| Deploy single-node testnet | 1 hour | ⏳ AFTER GENESIS |
| End-to-end validation | 1 hour | ⏳ AFTER DEPLOY |
| **Total Phase 5.3** | **~4-5 hours** | ⏳ IN PROGRESS |

---

## Key Validation Points

1. **Signature Recovery Works**: Can we extract pubkey from Bitcoin signature?
2. **Address Matching**: Does derived address match claimed sender?
3. **Key Registration**: Is first transaction transparent (no user action)?
4. **Performance**: Is second transaction fast (uses cached key)?
5. **Mixed Accounts**: Do Bitcoin + NEAR accounts coexist?
6. **Block Production**: Does testnet produce blocks normally?
7. **State Consistency**: Are balances and nonces correct?
8. **Error Handling**: Are invalid transactions rejected gracefully?

---

## Next Immediate Actions

1. ✅ Phase 5.1: Helper functions - COMPLETE
2. ✅ Phase 5.2: Verifier integration - COMPLETE
3. ⏳ Phase 5.3a: Write integration tests - **START NOW**
4. ⏳ Phase 5.3b: Generate testnet
5. ⏳ Phase 5.3c: Deploy single-node testnet
6. ⏳ Phase 5.3d: End-to-end validation

---

**Phase 5.3 begins now. Testing and deployment ahead.**
