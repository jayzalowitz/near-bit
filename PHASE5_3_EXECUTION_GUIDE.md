# Phase 5.3: Practical Execution Guide

**Status**: Ready to Execute
**Estimated Time**: 4-5 hours total
**Goal**: Validate all Bitcoin address functionality before mainnet

---

## Quick Start Checklist

- [ ] Verify compilation status
- [ ] Run integration tests
- [ ] Generate testnet genesis
- [ ] Deploy single-node testnet
- [ ] Run end-to-end validation
- [ ] Document results

---

## Step 1: Verify Compilation (15 min)

### Check Core Crypto Module
```bash
cd nearcore/core/crypto
cargo check
# Expected: ✅ Clean compilation in ~4 seconds
```

### Check Runtime Module
```bash
cd nearcore/runtime/runtime
cargo check
# Expected: ✅ Clean compilation (first time ~10-15 min, then cached)
# Look for: bitcoin_tx module compiles without errors
```

### If Compilation Fails
Check for:
- Missing imports in bitcoin_tx.rs or verifier.rs
- Type mismatches (Signature vs PublicKey)
- Module not properly declared in lib.rs

---

## Step 2: Run Unit Tests (30 min)

### Tests from Phase 5.1

```bash
cd nearcore/runtime/runtime
cargo test bitcoin_tx::tests -- --nocapture

# Expected output:
# test bitcoin_tx::tests::test_bitcoin_address_detection ... ok
# test bitcoin_tx::tests::test_is_bitcoin_address_edge_cases ... ok
```

### If Tests Fail
- Check bitcoin_address format strings
- Verify starts_with() logic
- Ensure AccountId parsing works

---

## Step 3: Generate Testnet Genesis (30 min)

### Create 10 Bitcoin Addresses with Known Keys

```bash
cd bitinfinity-tools

# Generate genesis with 10 synthetic Bitcoin addresses
cargo run --release -- generate-genesis \
    --testnet \
    --num-accounts 10 \
    --output-dir ./genesis-output/

# Expected output:
# Generated 10 accounts with Bitcoin addresses
# Total balance: 10,000,000 SYD
# Config: genesis-output/genesis.json
# Records: genesis-output/records.json
```

### Verify Genesis Files

```bash
ls -lh genesis-output/
# Expected:
# -rw-r--r-- genesis.json (2-5 KB)
# -rw-r--r-- records.json (10-50 KB)

# Check genesis structure
jq '.chain_id' genesis-output/genesis.json
# Expected: "bitinfinity-testnet"

jq '.genesis_height' genesis-output/genesis.json
# Expected: 0

jq '.records | length' genesis-output/records.json
# Expected: 10 (or more if validators included)
```

### Extract Bitcoin Addresses for Testing

```bash
# Parse addresses from records
jq '.[] | select(.Account) | .Account.account_id' \
    genesis-output/records.json > addresses.txt

# Verify format
head -5 addresses.txt
# Expected output (Bitcoin addresses):
# "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"
# "1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2"
# etc.
```

---

## Step 4: Initialize Single-Node Testnet (1 hour)

### Create Testnet Home Directory

```bash
mkdir -p ~/.bitinfinity-testnet
cd ~/.bitinfinity-testnet

# Copy genesis files
cp ../bitinfinity-tools/genesis-output/genesis.json ./
cp ../bitinfinity-tools/genesis-output/records.json ./
```

### Initialize Node Configuration

```bash
cargo run -p bitinfinity-neard -- init \
    --home ~/.bitinfinity-testnet \
    --chain-id bitinfinity-testnet

# Expected: Creates directory structure:
# ~/.bitinfinity-testnet/
#   ├── config.json
#   ├── validator_key.json
#   ├── node_key.json
#   └── data/
```

### Verify Initialization

```bash
# Check that config was created
ls -la ~/.bitinfinity-testnet/
# Expected: config.json and key files present

# Check config contents
jq '.chain_id' ~/.bitinfinity-testnet/config.json
# Expected: "bitinfinity-testnet"

jq '.consensus.min_block_production_delay' ~/.bitinfinity-testnet/config.json
# Expected: 400 (milliseconds, 0.4 seconds)
```

---

## Step 5: Start Node (1-2 hours)

### Launch Node

```bash
cargo run -p bitinfinity-neard -- run \
    --home ~/.bitinfinity-testnet \
    2>&1 | tee node.log

# Watch for these messages in output:
# [INFO] Starting node...
# [INFO] Opening database at /path/to/data
# [INFO] Starting RPC server at 127.0.0.1:3030
# [INFO] Started node
```

### In Separate Terminal, Monitor Progress

```bash
# Watch logs in real-time
tail -f node.log | grep -E "Block|Block producer|finality"

# Expected output after 5-10 seconds:
# Block 0 produced
# Block 1 produced
# ... (blocks produced ~every 1 second)
```

### Check RPC Server Status

```bash
# Test if RPC is responding
curl -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": "test",
    "method": "status",
    "params": []
  }' | jq .

# Expected output:
# {
#   "result": {
#     "chain_id": "bitinfinity-testnet",
#     "sync_info": {
#       "latest_block_height": 10,
#       ...
#     }
#   }
# }
```

---

## Step 6: End-to-End Transaction Test (1 hour)

### Test 1: Query Account Balance

```bash
# Get balance of first Bitcoin address
ADDR="1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"

curl -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": \"test\",
    \"method\": \"query\",
    \"params\": {
      \"request_type\": \"view_account\",
      \"account_id\": \"$ADDR\",
      \"block_id\": \"final\"
    }
  }" | jq '.result.amount'

# Expected: 1000000000000000000000000 (1M SYD in yoctosyd)
```

### Test 2: Create Bitcoin Signature

```bash
# Create a simple transfer transaction
# This would require:
# 1. Get nonce from account
# 2. Create transaction struct
# 3. Sign with secp256k1 private key
# 4. Submit signed transaction

# Script example (pseudo-code):
#!/bin/bash
ADDR="1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"
PRIVATE_KEY="5KgRdvR..." # From genesis generation

# Get current nonce
NONCE=$(curl -s -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"id\":\"1\",\"method\":\"query\",\"params\":{...}}" \
  | jq '.result.nonce')

# Create transaction JSON
cat > tx.json <<EOF
{
  "signer_id": "$ADDR",
  "public_key": "secp256k1:...",
  "nonce": $((NONCE + 1)),
  "receiver_id": "$ADDR",
  "actions": [
    {
      "type": "Transfer",
      "params": {
        "deposit": "1000000"
      }
    }
  ]
}
EOF

# Sign with Bitcoin private key
# (This requires a tool like bitcoinjs-lib or secp256k1)
SIGNATURE=$(sign_with_secp256k1.sh tx.json $PRIVATE_KEY)

# Submit signed transaction
curl -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": \"send_tx\",
    \"method\": \"broadcast_tx_commit\",
    \"params\": [\"$SIGNATURE\"]
  }" | jq .

# Expected: Transaction receipt with status "SuccessReceiptId"
```

### Test 3: Verify Auto-Registration

```bash
# After first transaction, check that access key was registered
curl -X POST http://127.0.0.1:3030 \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": \"test\",
    \"method\": \"query\",
    \"params\": {
      \"request_type\": \"view_access_key\",
      \"account_id\": \"$ADDR\",
      \"public_key\": \"secp256k1:...\",
      \"block_id\": \"final\"
    }
  }" | jq '.result.permission'

# Expected output:
# "FullAccess" (or similar showing full access permission)
```

### Test 4: Second Transaction (Cache Test)

```bash
# Send another transaction from same address
# This should use cached access key (no signature recovery overhead)

# Monitor in node.log:
grep -i "signature recovery" node.log | tail -1
# Should show recovery only on first tx, not on second
```

---

## Step 7: Validation & Success Criteria

### Success Checklist

- [ ] Node starts without errors
- [ ] Blocks produce normally (~1 per second)
- [ ] RPC responds to queries
- [ ] Account balances visible
- [ ] First Bitcoin transaction succeeds
- [ ] Access key auto-registered
- [ ] Second transaction is instant (cached key)
- [ ] No panics or crashes
- [ ] State consistent across queries

### Troubleshooting

| Issue | Solution |
|-------|----------|
| Compilation fails | Run `cargo clean`, check imports |
| Node won't start | Check config.json, data directory permissions |
| RPC not responding | Verify port 3030 is open, check logs for errors |
| Transaction fails | Verify signature format, account exists, balance sufficient |
| Key not registering | Check bitcoin_tx.rs integration in verifier.rs |

---

## Performance Benchmarks

Expected results after Phase 5.3:

| Metric | Expected | Status |
|--------|----------|--------|
| Block production time | ~1 second | ✅ |
| First Bitcoin tx overhead | ~50μs (signature recovery) | ✅ |
| Subsequent tx overhead | <1μs (cached key) | ✅ |
| RPC query response | <100ms | ✅ |
| State consistency | 100% | ✅ |

---

## Documentation & Results

### After Successful Testnet

Create `PHASE5_3_RESULTS.md` with:
1. Screenshots of node running
2. Transaction hashes
3. Block production logs
4. Performance metrics
5. Any issues encountered and fixes

---

## Timeline Tracking

```
Start Time: [Now]
Est. Duration: 4-5 hours

Checkpoint 1: Compilation + Tests (1 hour)
Checkpoint 2: Genesis + Node Init (1.5 hours)
Checkpoint 3: Node Running (30 min)
Checkpoint 4: Transactions Working (1.5 hours)
Checkpoint 5: Documentation (30 min)

Total: ~5 hours
```

---

## Next After Phase 5.3

Once Phase 5.3 is complete:
1. ✅ Bitcoin address transactions proven to work
2. ✅ Signature recovery and auto-registration verified
3. ✅ Testnet stable and producing blocks
4. Ready for: **Bitcoin Core sync completion → Mainnet genesis**

---

## Commands Reference

```bash
# Start over fresh
rm -rf ~/.bitinfinity-testnet
rm -rf nearcore/runtime/runtime/target

# Kill node if stuck
killall neard 2>/dev/null || true

# Check node status
curl -s http://127.0.0.1:3030/status | jq '.result.sync_info.latest_block_height'

# Get full logs
cat ~/.bitinfinity-testnet/logs/* 2>/dev/null | tail -100

# Reset and try again
./reset_testnet.sh
```

---

**Ready to proceed. Phase 5.3 execution begins.**
