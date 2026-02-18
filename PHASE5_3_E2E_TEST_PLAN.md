# Phase 5.3: End-to-End Bitcoin Address Transaction Testing

## Overview
This document describes the detailed end-to-end testing procedure to validate Bitcoin address support in Bitcoin Infinity.

**Objective**: Verify that a Bitcoin address can send transactions on Bitcoin Infinity using secp256k1 signatures without any pre-registration or claiming.

---

## Test Prerequisites

Before starting tests, verify:
1. ✅ nearcore/runtime/runtime compiles
2. ✅ bitcoin_utils unit tests pass  
3. ✅ Testnet genesis generated (10 Bitcoin addresses)
4. ✅ Testnet initialized (~/.bitinfinity-testnet/)
5. ✅ Testnet node running (RPC on 127.0.0.1:3030)

---

## Test 1: Query Account Balance (RPC Test)

**Purpose**: Verify RPC endpoint is responding and can query Bitcoin address accounts.

**Steps**:
```bash
# Extract first Bitcoin address from genesis
ADDR=$(jq -r '.[0].account_id' genesis-testnet/records.json)
echo "Testing address: $ADDR"

# Query balance via RPC
curl -s -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": \"test_balance\",
    \"method\": \"query\",
    \"params\": {
      \"request_type\": \"view_account\",
      \"account_id\": \"$ADDR\",
      \"block_id\": \"final\"
    }
  }" | jq '.result.amount'
```

**Expected Output**:
- Numeric balance in yoctosyd
- Example: `500000000000000000000000000000`

**Success Criteria**:
- ✅ RPC responds without error
- ✅ Account exists with non-zero balance
- ✅ Balance matches genesis value

---

## Test 2: Create Secp256k1 Signature

**Purpose**: Generate a valid secp256k1 signature for a transaction.

### 2a. Generate Secp256k1 Private Key
```bash
# Create a new secp256k1 keypair for testing
cat > /tmp/keygen.js << 'JSEOF'
const secp256k1 = require('secp256k1');
const crypto = require('crypto');

// Generate random private key
const privateKey = crypto.randomBytes(32);
const publicKey = secp256k1.publicKeyCreate(privateKey);

// For simplicity, use a test key (NOT for production)
const testPrivateKey = Buffer.from(
  '0000000000000000000000000000000000000000000000000000000000000001',
  'hex'
);
const testPublicKey = secp256k1.publicKeyCreate(testPrivateKey);

console.log('Private Key (hex):', testPrivateKey.toString('hex'));
console.log('Public Key (compressed):', testPublicKey.toString('hex'));
JSEOF

# Run with Node.js (if installed)
# node /tmp/keygen.js
```

### 2b. Manually Sign a Transaction (Pseudo-code)
```javascript
// Transaction structure
const tx = {
  signer_id: "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa",
  public_key: "<recovered_from_signature>",
  nonce: 1,
  receiver_id: "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa",  // Send to self
  block_hash: "<latest_block_hash>",
  actions: [
    {
      type: "Transfer",
      params: {
        deposit: "1000000"  // 0.000001 SYD
      }
    }
  ]
};

// Sign the transaction
const txHash = sha256(JSON.stringify(tx));
const signature = secp256k1.sign(txHash, privateKey);
const signedTx = {
  signature: signature,  // 65-byte recoverable signature
  transaction: tx
};
```

**Expected Output**:
- 65-byte secp256k1 recoverable signature
- Transaction hash
- Serialized SignedTransaction

---

## Test 3: Submit Transaction to Testnet

**Purpose**: Send a Bitcoin-signed transaction to the Bitcoin Infinity testnet.

```bash
# Get latest block hash
BLOCK_HASH=$(curl -s -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":"1","method":"status","params":[]}' \
  | jq -r '.result.sync_info.latest_block_hash')

echo "Latest block hash: $BLOCK_HASH"

# Submit signed transaction
SIGNED_TX=$(cat /tmp/signed_tx.json)  # Pre-signed Bitcoin transaction

curl -s -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": \"send_tx\",
    \"method\": \"broadcast_tx_commit\",
    \"params\": [\"$SIGNED_TX\"]
  }" | jq '.result'
```

**Expected Output**:
```json
{
  "status": {
    "SuccessReceiptId": "<receipt_id>"
  },
  "transaction": {
    "hash": "<tx_hash>",
    "signer_id": "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa",
    ...
  },
  "receipts": [...]
}
```

**Success Criteria**:
- ✅ Transaction accepted (SuccessReceiptId present)
- ✅ Transaction hash returned
- ✅ Signer is the Bitcoin address
- ✅ No signature errors

---

## Test 4: Verify Access Key Auto-Registration

**Purpose**: Confirm that the first transaction transparently registered the access key.

```bash
# Query access key after first transaction
ADDR="1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"

# Try to get access keys for this account
curl -s -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": \"test_key\",
    \"method\": \"query\",
    \"params\": {
      \"request_type\": \"view_access_key_list\",
      \"account_id\": \"$ADDR\",
      \"block_id\": \"final\"
    }
  }" | jq '.result.keys'
```

**Expected Output**:
```json
[
  {
    "public_key": "secp256k1:<recovered_pubkey>",
    "access_key": {
      "nonce": 0,
      "permission": "FullAccess"
    }
  }
]
```

**Success Criteria**:
- ✅ Access key list is NOT empty
- ✅ Public key recovered from signature
- ✅ Permission is FullAccess
- ✅ Key automatically registered (no manual action required)

---

## Test 5: Verify Balance Update

**Purpose**: Confirm that the balance was debited for gas and transfer amount.

```bash
# Query balance after transaction
curl -s -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": \"test_balance_after\",
    \"method\": \"query\",
    \"params\": {
      \"request_type\": \"view_account\",
      \"account_id\": \"$ADDR\",
      \"block_id\": \"final\"
    }
  }" | jq '.result | {amount: .amount, nonce: .nonce}'
```

**Expected Output**:
```json
{
  "amount": "499999998000000000000000000000",
  "nonce": 1
}
```

**Success Criteria**:
- ✅ Balance decreased (from 500000000000000000000000000000)
- ✅ Amount deducted: transfer (1000000) + gas fees (~2000000)
- ✅ Nonce incremented to 1

---

## Test 6: Cached Key Test (Second Transaction)

**Purpose**: Verify that subsequent transactions use the cached key (fast path).

```bash
# Send second transaction
SIGNED_TX2=$(cat /tmp/signed_tx2.json)  # Pre-signed, nonce=2

time curl -s -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": \"send_tx2\",
    \"method\": \"broadcast_tx_commit\",
    \"params\": [\"$SIGNED_TX2\"]
  }" | jq '.result.status'
```

**Expected Output**:
- Transaction succeeds instantly
- Response time: < 100ms (vs ~50μs for signature recovery overhead on first tx)
- No signature recovery messages in logs

**Success Criteria**:
- ✅ Transaction accepted
- ✅ Faster than first transaction
- ✅ Nonce properly incremented to 2

---

## Test 7: Invalid Signature Rejection

**Purpose**: Verify that signatures not matching the Bitcoin address are rejected.

```bash
# Create transaction claiming to be from Address A
# But sign with private key from Address B

FAKE_SIGNED_TX=$(cat /tmp/invalid_signature.json)

curl -s -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": \"send_invalid\",
    \"method\": \"broadcast_tx_commit\",
    \"params\": [\"$FAKE_SIGNED_TX\"]
  }" | jq '.error'
```

**Expected Output**:
```json
{
  "code": -32000,
  "message": "...",
  "data": "InvalidSignature"
}
```

**Success Criteria**:
- ✅ Transaction rejected
- ✅ Error indicates InvalidSignature
- ✅ No state changes applied

---

## Test 8: Mixed Account Types (Bitcoin + NEAR)

**Purpose**: Verify that Bitcoin and NEAR accounts can coexist and transact.

```bash
# Prepare two addresses: one Bitcoin, one NEAR
BTC_ADDR="1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"
NEAR_ADDR="test.near"

# Bitcoin transaction
bitcoin_result=$(curl -s -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d "{...bitcoin_signed_tx...}")

# NEAR transaction (uses ED25519)
near_result=$(curl -s -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d "{...near_signed_tx...}")

echo "Bitcoin tx: $(echo $bitcoin_result | jq '.result.status')"
echo "NEAR tx: $(echo $near_result | jq '.result.status')"
```

**Expected Output**:
- Both transactions succeed
- No interference between account types
- Bitcoin path uses secp256k1, NEAR path uses ed25519

**Success Criteria**:
- ✅ Both transaction types work
- ✅ Independent of each other
- ✅ No crosstalk or conflicts

---

## Test 9: Block Finality Verification

**Purpose**: Verify that transactions are finalized and in consensus.

```bash
# Get transaction after finality
TX_HASH="<hash_from_test_3>"

sleep 5  # Wait for finality (typically 1-2 blocks)

curl -s -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": \"get_tx\",
    \"method\": \"tx\",
    \"params\": [\"$TX_HASH\", true]
  }" | jq '.result.final_execution_status'
```

**Expected Output**:
```
"SuccessValue"
```

**Success Criteria**:
- ✅ Transaction appears in finalized blocks
- ✅ Execution status is success
- ✅ No state changes after finality

---

## Performance Benchmarks

Expected metrics from Phase 5.3:

| Operation | Expected Time | Notes |
|-----------|---------------|-------|
| First Bitcoin tx (signature recovery) | ~50μs overhead | Recoverable signature |
| Subsequent Bitcoin tx (cached key) | <1μs overhead | Fast path |
| Block production | ~1 second | Standard NEAR finality |
| RPC query response | <100ms | Account state lookup |
| Transaction finality | ~1-2 blocks | Doomslug consensus |

---

## Troubleshooting

### Transaction Rejected: "InvalidSignature"
- **Cause**: Signature doesn't match account
- **Fix**: Verify private key used for signing matches account
- **Check**: Public key recovery should match account_id

### Transaction Rejected: "AccountDoesNotExist"
- **Cause**: Account not in genesis
- **Fix**: Use address from genesis-testnet/records.json
- **Check**: `jq '.[].account_id' genesis-testnet/records.json`

### RPC Not Responding
- **Cause**: Node not running or RPC not initialized
- **Fix**: Start node: `cargo run -p bitinfinity-neard -- run --home ~/.bitinfinity-testnet`
- **Check**: `curl http://127.0.0.1:3030/status | jq .`

### Access Key Not Found
- **Cause**: First transaction hasn't been processed yet
- **Fix**: Wait for block to finalize
- **Check**: Monitor logs: `tail -f ~/.bitinfinity-testnet/logs/*`

---

## Success Summary

All tests passed when:
1. ✅ Bitcoin addresses work as account IDs
2. ✅ Signature recovery succeeds
3. ✅ Address matching prevents forgery
4. ✅ First transaction transparently registers key
5. ✅ Subsequent transactions use cached key
6. ✅ Balance updates correctly
7. ✅ Invalid signatures are rejected
8. ✅ Mixed account types coexist
9. ✅ Transactions finalize consistently

**Result**: Bitcoin Infinity is ready for multi-validator testnet (Phase 5.4)

