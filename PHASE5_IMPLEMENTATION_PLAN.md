# Phase 5: Transaction Validation Integration - Implementation Plan

**Phase**: 5 (Critical Integration)  
**Duration**: 1-2 weeks  
**Objective**: Enable Bitcoin Infinity testnet with signature recovery

---

## Architecture: How Transactions Will Work

```
Bitcoin User
  ↓ (owns Bitcoin private key)
  Creates Bitcoin Infinity transaction
  ↓
  Signs with secp256k1 (same Bitcoin key)
  ↓
  Submits to chain (signer_id = Bitcoin address)
  ↓
  [Transaction Verification]
  ├─ Extract signature (65 bytes)
  ├─ Hash transaction data
  ├─ Call Secp256K1Signature::recover()
  ├─ Recover public key from signature
  ├─ Derive Bitcoin address from pubkey (using bitcoin_utils)
  ├─ Compare with signer_id
  ├─ If FIRST transaction:
  │   └─ Auto-register pubkey as access key
  └─ Execute transaction
  ↓
  Transaction succeeds, account state updated
```

**User Experience**: Completely transparent. User just signs and sends.

---

## Implementation Steps

### Step 1: Find Transaction Verification Entry Points

**Locations to check**:
1. `nearcore/chain/chain/` - Block and transaction processing
2. `nearcore/runtime/runtime/` - Runtime execution
3. `nearcore/core/primitives/` - Transaction structures

**What to look for**:
- Where `SignedTransaction` is processed
- Where public key validation happens
- Where access keys are checked/registered

### Step 2: Create Signature Verification Helper

**File**: New or existing verifier module

```rust
pub fn verify_secp256k1_transaction(
    transaction: &SignedTransaction,
) -> Result<TransactionVerification> {
    // 1. Extract signature
    let sig_bytes: [u8; 65] = extract_signature(&transaction)?;
    
    // 2. Hash transaction
    let tx_hash = hash_transaction(&transaction)?;
    
    // 3. Recover public key
    let pubkey = Secp256K1Signature(sig_bytes).recover(&tx_hash)?;
    
    // 4. Derive address
    let recovered_address = bitcoin_utils::derive_bitcoin_address_from_pubkey(&pubkey)?;
    
    // 5. Verify match
    if recovered_address != transaction.signer_id.to_string() {
        return Err(TransactionError::SignatureMismatch);
    }
    
    Ok(TransactionVerification {
        valid: true,
        pubkey: pubkey,
        address: recovered_address,
    })
}
```

### Step 3: Integrate Into Access Key Verification

**Current Flow**:
1. Get access key for account
2. Verify signature with public key

**New Flow** (for secp256k1 and Bitcoin addresses):
1. Check if account is Bitcoin address (starts with '1', '3', 'bc1')
2. If first transaction (no access keys):
   - Perform signature recovery
   - Derive address
   - Verify match
   - Create access key with recovered pubkey
3. If subsequent transaction:
   - Use cached access key (existing flow)

### Step 4: Auto-Register Access Key

**Location**: Where transaction actions are executed

```rust
// After signature verification succeeds on first tx:
if account.get_access_keys().is_empty() && is_bitcoin_address(&signer_id) {
    // Store the recovered public key as access key
    let access_key = AccessKey {
        nonce: 0,
        permission: AccessKeyPermission::FullAccess,
        public_key: recovered_pubkey,
    };
    account.add_access_key(access_key);
}
```

**Effect**: Invisible to user, but subsequent transactions use cached key (faster)

---

## Files to Modify

### Critical Files

1. **nearcore/core/crypto/src/bitcoin_utils.rs**
   - ✅ Already created
   - Export `derive_bitcoin_address_from_pubkey()`

2. **nearcore/core/crypto/src/signature.rs**
   - ✅ Already has `Secp256K1Signature::recover()`
   - Verify it's properly accessible

3. **Transaction Verifier** (TBD - find location)
   - Add `verify_secp256k1_transaction()`
   - Hook into existing validation

4. **Access Key Management** (TBD - find location)
   - Add auto-register logic for Bitcoin addresses
   - Only on first transaction

5. **Account ID Validation** (TBD - verify)
   - Use `near-account-id` for Bitcoin address detection
   - Already implemented

### Helper Files to Create (If Needed)

1. **Bitcoin-specific transaction verifier**
   - Encapsulate all Bitcoin-specific logic
   - Keep mainline code clean

---

## Integration Points

### 1. Transaction Deserialization

When a transaction arrives:
```rust
let tx: SignedTransaction = ...;
if is_secp256k1_signed(&tx) {
    verify_secp256k1_transaction(&tx)?;
} else {
    // Existing ed25519 verification
    verify_ed25519_transaction(&tx)?;
}
```

### 2. Access Key Lookup

When checking access permissions:
```rust
if is_bitcoin_address(&account_id) && account.access_keys.is_empty() {
    // Perform signature recovery instead of access key lookup
    recover_and_verify_secp256k1(&tx)?;
} else {
    // Existing access key flow
    verify_with_access_key(&account, &tx)?;
}
```

### 3. Account Creation

When account is first used:
```rust
if is_bitcoin_address(&account_id) {
    // Create with no access keys (will be auto-registered on first tx)
    create_bitcoin_account(&account_id, &balance);
} else {
    // Existing flow
    create_near_account(&account_id, &balance);
}
```

---

## Testing Strategy

### Unit Tests

1. **Signature Recovery**
   - Test vector with known Bitcoin key/address
   - Verify recovered address matches

2. **Address Derivation**
   - Test vectors for all address types
   - Ensure matches Bitcoin standard

3. **Transaction Verification**
   - Valid signature, matching address → pass
   - Valid signature, wrong address → fail
   - Invalid signature → fail

### Integration Tests

1. **Genesis with Bitcoin Addresses**
   - Create genesis with 10 Bitcoin addresses
   - Load into testnet
   - Verify balances readable

2. **First Transaction** (Auto-Register)
   - Create account with Bitcoin address
   - Sign transaction with Bitcoin key
   - Submit and verify:
     - Transaction executes
     - Access key is registered
     - Balance updates

3. **Subsequent Transactions** (Cached Key)
   - Sign second transaction
   - Verify uses cached key (faster path)
   - Balance updates correctly

4. **Address Mismatch**
   - Sign with different key
   - Verify transaction rejected
   - Error message correct

### Testnet Validation

1. **Full Flow**
   - Initialize testnet with Bitcoin addresses
   - Sign multiple transactions with Bitcoin keys
   - Verify all execute correctly
   - Query final balances

2. **Performance**
   - Measure first transaction time (with recovery)
   - Measure subsequent transaction time (cached)
   - Log gas usage

---

## Success Criteria

- [ ] Transaction with Bitcoin secp256k1 signature accepted
- [ ] Signature recovery produces correct public key
- [ ] Derived address matches Bitcoin address account ID
- [ ] Access key auto-registered on first transaction
- [ ] Subsequent transactions use cached key
- [ ] All testnet transactions execute with Bitcoin keys
- [ ] Performance acceptable (first tx <100ms, subsequent <10ms)
- [ ] All edge cases handled (wrong address, invalid sig, etc.)

---

## Known Challenges

### Challenge 1: Finding Verification Entry Points
**Solution**: Explore nearcore structure, trace transaction flow from gossip → validation

### Challenge 2: Minimal Changes to Mainline Code
**Goal**: Keep Bitcoin-specific logic isolated, don't break existing ed25519 flow
**Solution**: Create Bitcoin-specific functions, use flag/type detection to route

### Challenge 3: Access Key Auto-Registration
**Goal**: Transparent, no user action needed
**Solution**: Hook into account creation and first transaction execution

### Challenge 4: Testing Without Full Testnet
**Goal**: Validate logic before full deployment
**Solution**: Unit tests with known vectors, integration tests with mocked structures

---

## Rollout Plan

### Week 1: Implementation
- [ ] Locate transaction verification entry points
- [ ] Create signature recovery integration function
- [ ] Implement address matching logic
- [ ] Write unit tests

### Week 2: Integration & Testing
- [ ] Integrate into transaction validator
- [ ] Implement auto-register logic
- [ ] Run integration tests
- [ ] Fix any issues

### Week 3: Testnet Deployment
- [ ] Deploy single-node testnet
- [ ] Test end-to-end with Bitcoin keys
- [ ] Performance validation
- [ ] Documentation updates

---

## Deliverables

1. **Code**
   - Transaction verifier with signature recovery
   - Auto-registration logic
   - All tests passing

2. **Documentation**
   - How signature recovery works
   - Transaction flow diagrams
   - Testnet deployment guide

3. **Testnet**
   - Single-node Bitcoin Infinity testnet
   - Working with Bitcoin addresses
   - Accepts secp256k1-signed transactions

---

## Timeline

| Week | Task | Status |
|------|------|--------|
| This | Explore + Plan ✅ | Done |
| Next | Implement verifier | 🔄 |
| Next | Integrate + test | 🔄 |
| Next | Testnet deploy | ⏳ |

**Target Completion**: Early to mid March 2026

---

## What Comes After Phase 5

- Bitcoin Core sync completion (running)
- Real UTXO snapshot processing
- Mainnet genesis generation
- Multi-node validator setup
- Mainnet launch

**Total Timeline to Mainnet**: 3-4 weeks from now

