# Bitcoin Infinity - Working Implementation Plan

## Overview
Creating a fully functional L1 blockchain based on NEAR Protocol that accepts Bitcoin addresses as account IDs and uses secp256k1 keys for user accounts.

## Completed ✅
- Bitcoin address validation (P2PKH, P2SH, P2WPKH, P2WSH, P2TR)
- secp256k1 keypair generation with Bitcoin address derivation
- Testnet genesis builder (synthetic UTXO data → genesis config)
- nearcore added as git subtree

## Current Phase: Getting a Working Testnet

### Strategy
1. Use nearcore's neard binary with custom chain config
2. Apply targeted modifications to support Bitcoin addresses
3. Build a single-node testnet that accepts Bitcoin-signed transactions

### Key Files to Modify in nearcore

#### 1. core/crypto/src/signature.rs
- Add secp256k1 signature verification
- Implement public key recovery from secp256k1 signatures
- Keep ed25519 for validator block production

#### 2. core/primitives/src/account.rs
- Allow Bitcoin addresses as account IDs
- Remove strict NEAR account ID validation that would reject Bitcoin addresses

#### 3. core/chain-configs/src/genesis_config.rs
- Add Bitcoin Infinity genesis parameters
- Set chain_id to "bitinfinity-mainnet"
- Configure token denomination as BIT with proper conversion

#### 4. runtime/runtime/src/verifier.rs
- Accept secp256k1 signatures for transaction signing
- Implement signature recovery for Bitcoin address verification
- Transparent access key registration on first transaction

### Build Steps

```bash
# 1. Compile nearcore crates with Bitcoin Infinity config
cd nearcore
cargo build --release -p neard

# 2. Initialize node with Bitcoin Infinity genesis
neard init --chain-id bitinfinity-mainnet \
    --account-id validator.bitinfinity \
    --fast-init

# 3. Load our genesis data
# Copy genesis_config.json and records.json from bitinfinity-tools

# 4. Run single-node testnet
neard run --home ~/.bitinfinity/
```

### Testnet Workflow
1. User generates Bitcoin keypair: `bitinfinity-tools keygen`
2. User gets Bitcoin address and private key
3. User can sign transactions with their Bitcoin key
4. Transactions are verified using signature recovery
5. Balances reflect Bitcoin address allocations from genesis

## Implementation Order

### Phase 1: Minimal Working Testnet (This week)
- [ ] Create Bitcoin Infinity fork of key nearcore crates
- [ ] Modify signature validation to accept secp256k1
- [ ] Get single-node testnet running
- [ ] Test transaction signing with Bitcoin keys

### Phase 2: Full Features (When Bitcoin sync completes)
- [ ] Real UTXO snapshot parsing
- [ ] Patoshi coin identification and reassignment
- [ ] Bitcoin RPC compatibility layer
- [ ] Multi-node validator network

### Phase 3: Production (Future)
- [ ] Mainnet launch with real Bitcoin UTXO state
- [ ] Full P2P network
- [ ] Cross-chain security guarantees
- [ ] Ecosystem tooling and RPC endpoints

## Architecture Notes

### Dual-Key System
- **User Accounts**: Bitcoin secp256k1 keys
  - Sign transactions with existing Bitcoin wallets
  - Bitcoin addresses are account IDs
  - Private key = Bitcoin private key

- **Validators**: NEAR ed25519 keys
  - Block production and VRF
  - Different keys from user accounts
  - Unchanged from NEAR Protocol

### Signature Verification Flow
```
User signs with secp256k1 private key
    ↓
Transaction includes signature + signer_id (Bitcoin address)
    ↓
Chain recovers public key from signature
    ↓
Derives Bitcoin address from public key
    ↓
Compares to signer_id → Signature valid ✓
    ↓
Transparently stores public key as access key for future txs
```

No extra steps for users - just sign and send with their Bitcoin key.

## Success Criteria

- [ ] Single-node testnet running with Bitcoin Infinity configuration
- [ ] Can import Bitcoin private key and derive address
- [ ] Can sign transaction with Bitcoin key and submit to chain
- [ ] Transaction validates and balance updates correctly
- [ ] Subsequent transactions use stored access key (faster validation)
- [ ] Chain produces blocks and processes transactions
- [ ] JSON-RPC returns data in NEAR format but with Bitcoin addresses

## Testing

```bash
# Generate keypair
bitinfinity-tools keygen > keypair.json

# Create transaction with Bitcoin key
# (using Bitcoin transaction library to sign)

# Submit to node
curl -X POST http://localhost:3030 \
  -H "Content-Type: application/json" \
  -d @transaction.json

# Check balance
curl -X POST http://localhost:3030 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": "dontcare",
    "method": "query",
    "params": {
      "request_type": "view_account",
      "account_id": "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa",
      "finality": "final"
    }
  }'
```

## Bitcoin Core Status
- Current: 57.6% synced (750K / 937K blocks)
- ETA: ~2-3 hours remaining
- Once complete: Can generate real genesis from Bitcoin UTXO state
