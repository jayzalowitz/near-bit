# Quantum-Resistant Secondary Signing System Proposal

**Issue Type**: Feature Proposal
**Priority**: Medium-High
**Phase**: Post-Mainnet (Phase 6+)
**Impact**: Security

---

## Executive Summary

Implement a quantum-resistant secondary signing system for Bitcoin Infinity accounts as a proactive security measure against future quantum computing threats to ECDSA (secp256k1).

This proposal includes:
1. Optional quantum-resistant secondary key registration
2. "Quantum Bit" emergency activation mechanism
3. Account locking/migration flow for quantum-vulnerable accounts
4. Backwards compatibility with existing secp256k1 accounts

---

## Problem Statement

### Quantum Computing Threat
- Shor's algorithm can break ECDSA in polynomial time if quantum computers with sufficient qubits exist
- Current Bitcoin UTXO set uses secp256k1 (vulnerable to quantum attacks)
- Bitcoin Infinity forks Bitcoin's address space, inheriting the same vulnerability
- Need proactive migration path before quantum threat is imminent

### Current Vulnerability
- Bitcoin addresses: ~1.1M owned by Satoshi (Patoshi), ~21M total BTC in circulation
- Bitcoin Infinity mirrors this: ~501K BIT in test accounts
- If quantum computers become practical (estimated 10-15+ years), funds at risk
- No current migration mechanism in place

---

## Proposed Solution

### 1. Quantum-Resistant Secondary Key System

#### Registration Flow
Users can optionally register a quantum-resistant public key alongside their secp256k1 key:

```rust
struct AccessKey {
    secp256k1_key: PublicKey,  // Existing
    quantum_resistant_key: Option<QuantumResistantKey>,  // NEW
    permission: AccessKeyPermission,
}

enum QuantumResistantKey {
    Dilithium3,      // CRYSTALS-Dilithium (NIST standard)
    Falcon1024,      // Falcon (NIST standard)
    SPHINCS,         // Hash-based (ultra-conservative)
}
```

#### Key Registration Process
1. User generates quantum-resistant keypair locally
2. User signs registration transaction with secp256k1
3. Transaction includes quantum-resistant public key
4. Both keys stored in account state
5. User can now sign transactions with either key

#### Signing Options
Users can sign transactions with:
- **Secp256k1 only** (current behavior)
- **Quantum-resistant only** (if registered)
- **Both keys** (highest security, dual-signature)
- **Migrate to quantum-resistant** (secp256k1 deprecated for account)

### 2. "Quantum Bit" Emergency Activation

#### Emergency Protocol
When quantum threat becomes imminent:

1. **Supermajority Vote** (validator consensus)
   - Requires 2/3+ validator approval
   - Activated via governance transaction
   - Cannot be reversed (one-way switch)

2. **Quantum Bit Activation**
   - Sets global flag: `quantum_threat_detected = true`
   - Timestamp recorded: `quantum_activation_height`
   - Event broadcasted to all nodes

3. **Account Locking Grace Period**
   - Accounts without quantum-resistant keys: 30-day grace period
   - Can still transact normally during grace period
   - Warnings in RPC responses
   - Countdown in block headers

#### Account Lock Mechanism
After grace period expires:

```rust
// Check account security level
if quantum_bit_active && account.quantum_resistant_key.is_none() {
    // Account is in "quantum vulnerable" state
    return Err(InvalidTxError::QuantumVulnerableAccountLocked {
        account_id: account_id.clone(),
        grace_period_end: quantum_activation_height + GRACE_PERIOD_BLOCKS,
        recommendation: "Register quantum-resistant key to continue"
    });
}
```

#### What Remains Available
- **Query/view** account state (read-only)
- **Transfer out** funds to quantum-safe accounts
- **Register quantum-resistant key** (unlocks account)

#### What's Blocked
- **New transactions** from the account
- **Key operations** (add key, deploy contract, etc.)
- **Balance changes** except explicit transfers

### 3. Account Migration Path

#### Migration Options

**Option A: Quick Migration (Recommended)**
```bash
# 1. Register quantum-resistant key
bitinfinity-cli register-qr-key \
    --account <bitcoin-address> \
    --qr-key <quantum-resistant-pubkey> \
    --sign-with secp256k1

# 2. Make key the primary signer
bitinfinity-cli set-primary-key \
    --account <bitcoin-address> \
    --key-type dilithium3
```

**Option B: Conservative Migration**
```bash
# Keep both keys, require both signatures
bitinfinity-cli require-dual-signature \
    --account <bitcoin-address> \
    --secp256k1-key <existing> \
    --qr-key <quantum-resistant>
```

**Option C: Cold Storage Migration**
```bash
# Transfer all funds to new quantum-safe account
bitinfinity-cli transfer \
    --from <old-bitcoin-address> \
    --to <new-qr-address> \
    --amount max
```

### 4. Implementation Details

#### New Transaction Types
```rust
enum Action {
    // Existing actions...
    Transfer(TransferAction),
    DeployContract(DeployContractAction),
    
    // NEW: Quantum-resistant operations
    RegisterQuantumResistantKey(RegisterQRKeyAction),
    RotateToQuantumResistant(RotateToQRAction),
    RequireDualSignature(RequireDualSignatureAction),
}

struct RegisterQRKeyAction {
    quantum_key_algorithm: QuantumResistantAlgorithm,
    quantum_public_key: Vec<u8>,
}

struct RotateToQRAction {
    new_primary_algorithm: QuantumResistantAlgorithm,
    keep_secp256k1_backup: bool,
}
```

#### Signature Verification
```rust
pub fn verify_transaction_signature(
    tx: &SignedTransaction,
    account: &Account,
) -> Result<(), InvalidTxError> {
    // If quantum bit active and no QR key, reject
    if quantum_bit_active() && account.quantum_resistant_key.is_none() {
        return Err(InvalidTxError::QuantumVulnerableAccountLocked);
    }
    
    // Try secp256k1 signature
    if tx.signature.is_secp256k1() {
        return verify_secp256k1(&tx.signature, &account.secp256k1_key);
    }
    
    // Try quantum-resistant signature
    if tx.signature.is_quantum_resistant() {
        return verify_quantum_resistant(&tx.signature, &account.quantum_resistant_key)?;
    }
    
    // Try dual-signature
    if tx.signature.is_dual() {
        verify_secp256k1(&tx.signature.secp256k1_part, &account.secp256k1_key)?;
        verify_quantum_resistant(&tx.signature.qr_part, &account.quantum_resistant_key)?;
        return Ok(());
    }
    
    Err(InvalidTxError::InvalidSignature)
}
```

#### RPC Endpoints (New)

```bash
# Get account security status
curl -X POST http://127.0.0.1:3030 \
  -d '{
    "method": "query",
    "params": {
      "request_type": "view_account_security",
      "account_id": "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"
    }
  }' | jq .

# Response:
{
  "secp256k1_key_registered": true,
  "quantum_resistant_key_registered": false,
  "quantum_bit_active": false,
  "quantum_vulnerable": false,
  "grace_period_remaining_blocks": null,
  "recommendation": "Consider registering a quantum-resistant key"
}

# Check global quantum status
curl -X POST http://127.0.0.1:3030 \
  -d '{
    "method": "query",
    "params": {
      "request_type": "view_quantum_status"
    }
  }' | jq .

# Response:
{
  "quantum_bit_active": false,
  "quantum_activation_height": null,
  "vulnerable_accounts_count": 501000,
  "migrated_accounts_count": 1000,
  "grace_period_blocks": 216000
}
```

---

## NIST Post-Quantum Standards

### Recommended Algorithms

| Algorithm | Type | Security | Key Size | Speed | Notes |
|-----------|------|----------|----------|-------|-------|
| **Dilithium3** | Lattice | NIST Level 3 | 2.5KB pub | Fast | Recommended primary |
| **Falcon1024** | Lattice | NIST Level 5 | 1.8KB pub | Slower | Smaller keys |
| **SPHINCS** | Hash-based | NIST Level 3 | 32B pub | Slow | Most conservative |

**Recommendation**: Dilithium3 as primary (balance of speed and security)

---

## Governance & Activation

### Quantum Bit Governance
```rust
// Governance transaction to activate quantum bit
struct QuantumBitActivation {
    reason: String,  // "Quantum threat detected on 2037-03-15"
    effective_height: BlockHeight,
    grace_period_blocks: u64,  // ~30 days = 2,592,000 blocks
}

// Requires supermajority vote
// Validators vote via special vote transactions
// 2/3 + 1 validators must approve
// No reversal once activated
```

### Communication Strategy
- **Phase 1**: Announce feature (now)
- **Phase 2**: Testnet validation (3-6 months)
- **Phase 3**: Mainnet deployment (optional, user opt-in)
- **Phase 4**: Monitoring and education (continuous)
- **Phase 5**: Emergency activation (only if quantum threat imminent)

---

## Backwards Compatibility

### Existing Accounts
- ✅ Continue working indefinitely if quantum bit not activated
- ✅ Not required to migrate until grace period expires
- ✅ Can migrate at any time (no deadline until quantum bit active)
- ✅ All existing transactions remain valid

### Breaking Changes
- None during normal operation
- Only breaking change: Account locking after grace period (when quantum threat imminent)

---

## Testing Strategy

### Unit Tests
- Quantum key registration
- Dual-signature verification
- Account locking after grace period
- Migration transaction flows
- RPC endpoint responses

### Integration Tests
- Mainnet testnet with quantum-resistant accounts
- Mixed quantum-safe and vulnerable accounts
- Quantum bit activation and account locking
- Emergency migration under time pressure

### Stress Tests
- 1M+ accounts with quantum keys
- Signature verification performance (Dilithium slower than secp256k1)
- Grace period countdown and locking

---

## Performance Considerations

### Signature Size
| Algorithm | Signature Size | Overhead vs secp256k1 |
|-----------|---|---|
| secp256k1 | 65 bytes | - |
| Dilithium3 | 2,420 bytes | +3,630% |
| Falcon1024 | 690 bytes | +961% |

**Impact**: 
- Larger transactions (~2.4KB extra for Dilithium)
- More storage for blocks
- Slower signature verification (~10-50x slower)
- Manageable for most use cases (still <1MB per block average)

### Optimization
- Only verify QR signatures when needed
- Batch verification where possible
- Use optimized libraries (liboqs-rs)

---

## Implementation Timeline

### Phase 5.4 (Testnet): 2 weeks
- [ ] Implement quantum key registration
- [ ] Add RPC endpoints
- [ ] Testnet deployment with sample quantum keys
- [ ] Performance benchmarking

### Phase 5.5 (Hardening): 1 week
- [ ] Security audit of quantum key code
- [ ] Emergency activation mechanism testing
- [ ] Documentation and user guides

### Phase 6.0 (Mainnet): Deployment ready
- [ ] Optional feature available to all users
- [ ] Validators can activate quantum bit if needed
- [ ] Education campaign for users

### Phase 6+ (Future)
- [ ] Monitor quantum computing progress
- [ ] Activate quantum bit if threat becomes imminent
- [ ] Manage mass migration to quantum-safe keys

---

## Questions for Community

1. **Algorithm preference**: Dilithium3, Falcon1024, or multiple options?
2. **Dual-signature requirement**: Should quantum bit require both secp256k1 AND QR signature?
3. **Grace period**: 30 days optimal, or different duration?
4. **Backwards compatibility**: Break or maintain secp256k1 forever?
5. **User education**: What resources would help users migrate?

---

## References

- NIST Post-Quantum Cryptography: https://csrc.nist.gov/projects/post-quantum-cryptography/
- CRYSTALS-Dilithium: https://pq-crystals.org/dilithium/
- Falcon: https://falcon-sign.info/
- liboqs-rs: https://github.com/open-quantum-safe/liboqs-rust

---

## Acceptance Criteria

- [x] Design document complete
- [ ] Community consensus on algorithm choice
- [ ] Implementation plan approved
- [ ] Testnet deployment successful
- [ ] Security audit passed
- [ ] User documentation complete
- [ ] Ready for mainnet deployment

---

**Status**: PROPOSAL
**Next Step**: Community review and feedback

