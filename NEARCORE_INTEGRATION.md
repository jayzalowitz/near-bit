# Bitcoin Infinity - Nearcore Integration Guide

## Overview

Bitcoin Infinity modifies NEAR Protocol to:
1. Use secp256k1 keys for user accounts (Bitcoin-compatible)
2. Maintain ed25519 keys for validators (Doomslug consensus requirement)
3. Support Bitcoin addresses as account IDs
4. Implement transparent account access via signature recovery

## Key Changes Applied

### 1. Default Key Type (✅ COMPLETE)
**File**: `nearcore/core/crypto/src/signature.rs`
- `split_key_type_data()` now defaults to SECP256K1 instead of ED25519
- **Impact**: Any key without explicit type prefix is treated as secp256k1

**File**: `nearcore/core/crypto/src/signer.rs`
- `EmptySigner::public_key()` now returns SECP256K1
- **Impact**: Test signers use secp256k1 by default

### 2. Bitcoin Address Utilities (✅ COMPLETE)
**File**: `nearcore/core/crypto/src/bitcoin_utils.rs` (NEW)
- `derive_bitcoin_address_from_pubkey()`: Converts secp256k1 pubkey → Bitcoin P2PKH address
- Process: compress → SHA256 → RIPEMD160 → Base58Check
- **Impact**: Can recover Bitcoin addresses from recovered secp256k1 public keys

**File**: `nearcore/core/crypto/Cargo.toml` (✅ UPDATED)
- Added `ripemd.workspace = true`
- Added `sha2.workspace = true`

### 3. Bitcoin Address Validation (✅ IN WORKSPACE)
**File**: `near-account-id/src/lib.rs`
- Already supports all Bitcoin address types
- `get_account_type()` detects Bitcoin addresses
- `validate_bitcoin_address()` validates all formats
- **Impact**: Account IDs can be Bitcoin addresses

## Next Steps - Transaction Validation Integration

### Step 1: Modify Transaction Verifier
**File**: `nearcore/runtime/runtime/src/verifier.rs`

Need to add signature recovery logic when verifying secp256k1 transactions:

```rust
// When processing a signed transaction with secp256k1 signature:
1. Extract signature from transaction
2. Hash the transaction data (use existing hash function)
3. Call signature.recover(tx_hash) to get public key
4. Derive Bitcoin address from pubkey using bitcoin_utils
5. Compare derived address with transaction.signer_id
6. If match → signature is valid, automatically store pubkey as access key
7. If mismatch → reject transaction
```

### Step 2: Automatic Access Key Registration
**File**: `nearcore/runtime/runtime/src/actions.rs`

On first transaction from a Bitcoin address:
1. Signature recovery reveals the public key
2. Automatically create and store an AccessKey for the account
3. Future transactions can use cached key (faster verification)
4. **User-facing**: No difference - it's all automatic

### Step 3: Dual-Key Architecture Documentation
**File**: `nearcore/core/crypto/src/key_conversion.rs`

No code changes needed - just document that:
- User accounts: secp256k1 (Bitcoin addresses)
- Validator keys: ed25519 (NEAR consensus)
- `is_valid_staking_key()` remains ed25519-only
- No overlap between two systems

## Implementation Status

- [x] Default key type changed to SECP256K1
- [x] Bitcoin address utilities implemented
- [x] Bitcoin address validation available
- [x] Secp256k1 signature recovery already in nearcore
- [ ] Transaction verifier integration
- [ ] Access key registration on first tx
- [ ] Testing with secp256k1-signed transactions
- [ ] Single-node testnet validation

## Testing Strategy

1. **Unit tests**: Bitcoin address derivation from known keys
2. **Integration tests**: 
   - Create account with Bitcoin address
   - Sign transaction with secp256k1 key
   - Verify transaction is accepted
   - Check access key was registered
3. **Testnet validation**:
   - Start single-node testnet
   - Submit transactions signed with Bitcoin keys
   - Query account state

## Critical Files Summary

| File | Change | Status |
|------|--------|--------|
| signature.rs | Default → SECP256K1 | ✅ Done |
| signer.rs | EmptySigner → SECP256K1 | ✅ Done |
| bitcoin_utils.rs | NEW: address derivation | ✅ Done |
| Cargo.toml | Add sha2, ripemd | ✅ Done |
| verifier.rs | Signature recovery integration | ⏳ Next |
| actions.rs | Auto access key registration | ⏳ Next |
| key_conversion.rs | Document dual-key | ⏳ Next |

## Building Bitcoin Infinity

```bash
# From nearcore directory
cargo build --release -p neard

# Run testnet node
./target/release/neard init --home ~/.bitinfinity-testnet
./target/release/neard run --home ~/.bitinfinity-testnet
```

## Key Design Decisions

1. **Secp256k1 as default**: Enables Bitcoin key compatibility without explicit prefixes
2. **Signature recovery on demand**: Public keys only stored after first transaction (saves genesis space)
3. **Bitcoin address format in UTXO**: Account balances keyed by Bitcoin address string
4. **Dual-key isolation**: Validators use ed25519, users use secp256k1, no crossover

