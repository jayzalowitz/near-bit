# Bitcoin Infinity Quick Start Guide

## What is Bitcoin Infinity?

Bitcoin Infinity is a new L1 blockchain that combines:
- **NEAR Protocol's execution engine** - Smart contracts, sharding, 1-second finality
- **Bitcoin's address space** - Your Bitcoin private key is your Bitcoin Infinity private key
- **Bitcoin's token count** - Same ~21M BIT token supply, with Satoshi's coins reassigned to a new key

Your existing Bitcoin wallet is your Bitcoin Infinity wallet. No migration, no claiming, no new software.

## Getting Started

### 1. Generate a Bitcoin Infinity Keypair

```bash
cargo run -p bitinfinity-tools -- keygen
```

Output:
```
Private key (WIF): 5KgRdvRMZFRaNsREy7KytsfAAc3rkmYPKdsun4SzmWUhDDZbxFR
Bitcoin address:   15ZZYBGDAdhh9otivXvJoE3YaFGE42uiQ2
```

The Bitcoin address is your **account ID** on Bitcoin Infinity. Use the private key to sign transactions.

### 2. Initialize a Testnet Node

```bash
cargo run -p bitinfinity-neard -- init --home ~/.bitinfinity
```

This creates the directory structure and prepares your node.

### 3. Generate Testnet Genesis (Synthetic Data)

```bash
cargo run -p bitinfinity-tools -- generate-genesis \
  --testnet \
  --num-accounts 100 \
  --chain-id bitinfinity-testnet \
  --output-dir ~/.bitinfinity/genesis
```

This creates:
- `genesis_config.json` - Chain configuration
- `records.json` - Account balances from synthetic UTXOs

### 4. Run Your Node

```bash
cargo run -p bitinfinity-neard -- run --home ~/.bitinfinity
```

Your node will start on:
- JSON-RPC: `http://localhost:3030`
- P2P Network: `localhost:24567`

### 5. Query Your Account Balance

```bash
curl -X POST http://localhost:3030 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": "dontcare",
    "method": "query",
    "params": {
      "request_type": "view_account",
      "account_id": "15ZZYBGDAdhh9otivXvJoE3YaFGE42uiQ2",
      "finality": "final"
    }
  }'
```

## Architecture

### Account IDs = Bitcoin Addresses

Any valid Bitcoin address works as an account ID:

| Type | Format | Example |
|------|--------|---------|
| P2PKH (Legacy) | Starts with '1' | `1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa` |
| P2SH (Multisig) | Starts with '3' | `3J98t1WpEZ73CNmYviecrnyiWrnqRhWNLy` |
| P2WPKH (SegWit) | Starts with 'bc1q' | `bc1qw508d6qejxtdg4y5r3z...` |
| P2TR (Taproot) | Starts with 'bc1p' | `bc1pxxx...` |

### Signing Transactions with Your Bitcoin Key

Bitcoin Infinity transactions are signed with your Bitcoin private key using secp256k1 ECDSA:

```bash
# Using any Bitcoin library (bitcoinjs-lib, rust-bitcoin, etc.)
const bitcoin = require('bitcoinjs-lib');

const keyPair = bitcoin.ECPair.fromPrivateKeyBuffer(Buffer.from(privateKey, 'hex'));
const signature = keyPair.sign(transactionHash);
```

The signature is verified on-chain using public key recovery - the same mechanism Bitcoin uses.

### Transparent Account Access

First transaction from a new address:
1. User signs with their Bitcoin key
2. Chain recovers the public key from the signature
3. Verifies it matches the account ID (Bitcoin address)
4. **Automatically stores** the public key as an access key
5. Subsequent transactions use the cached key (faster)

From the user's perspective, there's zero difference - transactions just work.

## Token Denomination

Bitcoin Infinity uses the same token model as NEAR:

- **1 BIT** = 10^24 yoctoBIT (smallest unit)
- **1 Satoshi** = 10^16 yoctoBIT (from Bitcoin conversion)

So if you had 1 BTC on Bitcoin, you have 100,000,000 satoshis, which equals:
- 100,000,000 × 10^16 = 10^24 yoctoBIT = **1 BIT** ✓

## Testnet vs Mainnet

### Testnet (Now)
- Synthetic UTXO data from `bitinfinity-tools generate-genesis --testnet`
- Fast setup for development
- Any Bitcoin address works
- No real value

### Mainnet (When Bitcoin Core Syncs)
- Real Bitcoin UTXO snapshot via `bitcoin-cli dumptxoutset`
- Satoshi's ~1.1M BTC reassigned to a new key (holder's key)
- Full network with validators
- Real value

## Next Steps

1. **Run the testnet** - Get a single-node network running
2. **Send transactions** - Sign with your Bitcoin key
3. **Deploy smart contracts** - Same NEAR smart contracts, but with Bitcoin addresses
4. **Set up validators** - Run a multi-node network with consensus
5. **Connect Bitcoin RPC** - Use existing Bitcoin wallets without changes

## Development Tools

### bitinfinity-tools
Genesis and key generation for Bitcoin Infinity.

```bash
# Generate keypair
cargo run -p bitinfinity-tools -- keygen

# Generate testnet genesis
cargo run -p bitinfinity-tools -- generate-genesis --testnet

# Generate mainnet genesis (when Bitcoin Core syncs)
cargo run -p bitinfinity-tools -- generate-genesis \
  --utxo-snapshot ~/.bitcoin/utxo_snapshot.dat \
  --patoshi-csv ./patoshi_addresses.csv
```

### bitinfinity-neard
Node binary for running Bitcoin Infinity.

```bash
# Initialize node
cargo run -p bitinfinity-neard -- init --home ~/.bitinfinity

# Run node
cargo run -p bitinfinity-neard -- run --home ~/.bitinfinity

# View configuration
cargo run -p bitinfinity-neard -- config --home ~/.bitinfinity
```

### bitinfinity-btcrpc
Bitcoin Core-compatible JSON-RPC proxy (204 methods) for wallet integration.

```bash
# Start the RPC proxy (needs a running nearcore node)
cargo run -p bitinfinity-btcrpc

# Existing Bitcoin wallets work by changing endpoint to localhost:8332
bitcoin-cli -rpcconnect=127.0.0.1 -rpcport=8332 getbalance
```

## FAQ

**Q: Do I need to create a new wallet?**
No. Your existing Bitcoin wallet works. Just import your private key into Bitcoin Infinity tooling.

**Q: What if I lose my private key?**
Same as Bitcoin - it's gone. Bitcoin Infinity doesn't custodize keys.

**Q: Can I move funds back to Bitcoin?**
Not directly. Bitcoin Infinity is a separate blockchain. Cross-chain bridges could be built, but they're not in scope for launch.

**Q: When is mainnet?**
When Bitcoin Core finishes syncing, we parse the real UTXO set and launch mainnet with Satoshi's coins reassigned. See `docs/launch-readiness-gates.md` for current gate status.

**Q: How fast is Bitcoin Infinity?**
1-second block time, instant finality via Doomslug consensus (same as NEAR).

**Q: Can it handle Bitcoin script?**
No, Bitcoin Infinity runs NEAR smart contracts. You get the full contract programming model (Rust/JavaScript in WASM), not Bitcoin's script.

## Resources

- **Bitcoin Infinity Plan**: `bitcoin-infinity.md`
- **NEAR Documentation**: https://docs.near.org
- **Bitcoin Address Validation**: Uses secp256k1 with Base58Check/Bech32 validation
- **Signature Recovery**: ECDSA public key recovery from secp256k1 signatures
