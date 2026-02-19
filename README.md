# Bitcoin Infinity

**Bitcoin Infinity** is a Layer 1 blockchain that combines Bitcoin's address space and 21M token supply with NEAR Protocol's execution engine — smart contracts, sharding, and sub-second finality.

Your existing Bitcoin private key is your Bitcoin Infinity private key. No migration. No claiming. No new wallet software required.

```
Bitcoin address:  1A1zP1eP5QGefi2DMPTfTL5SLmv7Divfna
Bitcoin Infinity: same address, same key, now with smart contracts
```

## What's different

| | Bitcoin | Bitcoin Infinity |
|--|---------|-----------------|
| Addresses | Bitcoin (P2PKH, P2WPKH, P2TR) | Same |
| Keys | secp256k1 | Same |
| Supply | 21,000,000 BTC | 21,000,000 BIT |
| Smallest unit | 1 satoshi | 1 finney (10⁻⁸ satoshi) |
| Consensus | Proof of Work | Proof of Stake (NEAR BFT) |
| Finality | ~60 min | ~1 second |
| Throughput | ~7 TPS | 1,000+ TPS per shard |
| Smart contracts | No | Yes (NEAR VM, Rust/JS) |
| Satoshi's coins | Spendable | Staking-only, floor-enforced |

## Repository structure

```
bitinfinity-btcrpc/     Bitcoin Core-compatible JSON-RPC proxy (204 methods)
bitinfinity-tools/      UTXO snapshot parser, genesis builder, Patoshi detector
bitinfinity-neard/      Custom nearcore binary with Bitcoin Infinity genesis
near-account-id/        Forked near-account-id: accepts Bitcoin addresses as account IDs
nearcore/               Forked nearcore: secp256k1 signature verification, BTC accounts
docs/                   GitHub Pages site
genesis-testnet/        Testnet genesis.json and config
```

## Quick start

See [QUICKSTART.md](QUICKSTART.md) for step-by-step instructions to:
- Generate a Bitcoin Infinity keypair
- Boot a local testnet node
- Connect Sparrow Wallet
- Send your first transaction

## Connect your Bitcoin wallet

Bitcoin Infinity exposes a Bitcoin Core-compatible RPC endpoint. Any wallet that supports a custom Bitcoin Core RPC server works out of the box.

```bash
# Start the RPC proxy (needs a running nearcore node)
cargo run -p bitinfinity-btcrpc

# In Sparrow Wallet: File → Preferences → Server → Bitcoin Core
# Host: 127.0.0.1  Port: 8332  Use SSL: No
```

## Build

```bash
# All three binaries
cargo build --release

# Individual
cargo build --release -p bitinfinity-btcrpc
cargo build --release -p bitinfinity-tools
cargo build --release -p bitinfinity-neard
```

## Test

```bash
cargo test --workspace
cargo test --manifest-path near-account-id/Cargo.toml
```

## Key design decisions

- **Bitcoin addresses as NEAR account IDs**: `near-account-id` accepts P2PKH, P2SH, P2WPKH, P2WSH, and P2TR addresses natively
- **secp256k1 in nearcore**: transactions signed with Bitcoin keys are verified via pubkey recovery + address derivation before any action executes
- **Patoshi balance floor**: Satoshi-era coinbase accounts are staking-only; the genesis balance is a permanent floor; excess above floor auto-sweeps to the Bitcoin Infinity Foundation each epoch
- **21M hard cap**: `MAX_SUPPLY = 21_000_000 * 10^24 yoctoBIT`; remaining emission distributed as staking rewards on the Bitcoin halving schedule, time-based not block-count-based
- **The finney**: 1 satoshi = 10⁸ finneys = 10¹⁶ yoctoBIT; named after Hal Finney

## Security

Found a vulnerability? Email security@bitcoininfinity.io or submit to the bug bounty program (link TBD). Do not open a public GitHub issue for security findings.

## Related issues

- [#2 — Quantum Resistance](https://github.com/infinitoshi/near-bit/issues/2)
- [#10 — Patoshi Balance Floor](https://github.com/infinitoshi/near-bit/issues/10)
- [#11 — Launch Plan](https://github.com/infinitoshi/near-bit/issues/11)

## License

MIT — see [LICENSE](LICENSE) or each crate's `Cargo.toml`.
The `nearcore/` subtree is Apache 2.0 per upstream NEAR Protocol.
