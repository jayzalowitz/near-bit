# Protocol Limits and Parameters

Bitcoin Infinity runs a PoS execution runtime (protocol version 84). All execution limits, gas parameters, and contract constraints are documented below.

## Throughput and Scaling

The underlying runtime has been benchmarked at 1,000,000+ TPS using stateless validation across 70 shards (~14,800 TPS per shard). Three independent runs sustained 1M TPS for approximately one hour each, with peaks of 1,029,497 / 1,037,334 / 1,037,495 TPS on Google Cloud C4D hardware at ~$900/month total.

Key caveats:
- The 1M TPS benchmark used native token transfers only (no smart contract execution).
- Bitcoin Infinity testnet currently operates 1 shard.
- Per-shard throughput for simple transfers: ~14,800 TPS.
- Bitcoin Infinity measured single-shard throughput is documented separately in [benchmark-methodology.md](benchmark-methodology.md).

Throughput scales approximately linearly with shard count under the sharding protocol. The protocol supports dynamic resharding to add shards as demand grows.

## Genesis and Consensus

| Parameter | Value | Notes |
|---|---|---|
| Protocol version | 84 | |
| Gas limit per chunk | 1,000,000,000,000,000 (1 Pgas) | 1,000 Tgas |
| Min gas price | 100,000,000 yocto | |
| Max gas price | 10,000,000,000,000,000,000,000 yocto | |
| Transaction validity period | 100 blocks | |
| Epoch length | 500 blocks | Testnet setting |
| Block producer kickout threshold | 90% | |
| Chunk producer kickout threshold | 90% | |
| Chunk validator only kickout threshold | 80% | |
| Protocol upgrade stake threshold | 4/5 (80%) | |
| Gas price adjustment rate | 1/100 (1%) | |
| Protocol reward rate | 1/10 (10%) | |
| Max inflation rate | 1/20 (5%) | Annual |
| Num blocks per year | 31,536,000 | ~1 second block time |
| Fishermen threshold | 10 BIT-equivalent | |

## Validator Seats

| Parameter | Value |
|---|---|
| Block producer seats | 1 (testnet single-validator) |
| Chunk-only producer seats | 300 |
| Minimum validators per shard | 1 |
| Target validator mandates per shard | 68 |
| Minimum stake divisor | 10 |
| Minimum stake ratio | 1/6,250 |
| Max kickout stake percentage | 100% |

## Transaction and Receipt Limits

| Parameter | Value |
|---|---|
| Max transaction size | 1,572,864 bytes (1.5 MiB) |
| Max receipt size | 4,194,304 bytes (4 MiB) |
| Max actions per receipt | 100 |
| Max number of input data dependencies | 128 |
| Max promises per function call action | 1,024 |
| Max total prepaid gas | 300,000,000,000,000 (300 Tgas) |

## Gas Execution Limits

| Parameter | Value |
|---|---|
| Max gas burnt per receipt | 300,000,000,000,000 (300 Tgas) |
| Max gas burnt (view calls) | 200,000,000,000,000 (200 Tgas) |
| Max total prepaid gas per transaction | 300,000,000,000,000 (300 Tgas) |

## Smart Contract Limits

| Parameter | Value |
|---|---|
| Max contract size | 4,194,304 bytes (4 MiB) |
| Max arguments length | 4,194,304 bytes (4 MiB) |
| Max length of returned data | 4,194,304 bytes (4 MiB) |
| Max length of storage key | 2,048 bytes |
| Max length of storage value | 4,194,304 bytes (4 MiB) |

## Wasm VM Limits

| Parameter | Value |
|---|---|
| Max stack height | 262,144 |
| Initial memory pages | 1,024 |
| Max memory pages | 2,048 |
| Registers memory limit | 1,073,741,824 bytes (1 GiB) |
| Max register size | 104,857,600 bytes (100 MiB) |
| Max number of registers | 100 |

## Contract Structure Limits

| Parameter | Value |
|---|---|
| Max functions per contract | 10,000 |
| Max locals per contract | 1,000,000 |
| Max tables per contract | 1 |
| Max elements per contract table | 10,000 |

## Logging and Method Limits

| Parameter | Value |
|---|---|
| Max number of logs | 100 |
| Max total log length | 16,384 bytes |
| Max method name length | 256 bytes |
| Max total bytes for method names | 2,000 bytes |

## Storage Costs

| Parameter | Value |
|---|---|
| Storage cost per byte | 10^19 yocto (0.00001 BIT) |
| Storage bytes per account | 100 bytes |
| Storage extra bytes per record | 40 bytes |

## Account Limits

| Parameter | Value |
|---|---|
| Min allowed top-level account length | 65 characters |
| Account ID validity rules version | 2 |

Note: Bitcoin Infinity extends account validation to accept Bitcoin address formats (P2PKH, P2SH, P2WPKH, P2WSH, P2TR) via the forked `near-account-id` crate.

## Congestion Control (Protocol v84)

| Parameter | Value |
|---|---|
| Max congestion incoming gas | 400 Pgas |
| Max congestion outgoing gas | 10 Pgas |
| Max congestion memory consumption | 1,000,000,000 bytes (1 GB) |
| Max congestion missed chunks | 125 |
| Max outgoing gas | 300 Pgas |
| Min outgoing gas | 1 Pgas |
| Allowed shard outgoing gas | 1 Pgas |
| Max tx gas | 500 Tgas |
| Min tx gas | 20 Tgas |
| Reject tx congestion threshold | 0.8 (80%) |
| Outgoing receipts usual size limit | 102,400 bytes (100 KiB) |
| Outgoing receipts big size limit | 4,718,592 bytes (4.5 MiB) |

## Yield and Async

| Parameter | Value |
|---|---|
| Yield timeout | 200 blocks |
| Max yield payload size | 1,024 KiB |

## State Witness Limits (Protocol v84)

| Parameter | Value |
|---|---|
| Main storage proof size soft limit | 4,000,000 bytes |
| Combined transactions size limit | 4,194,304 bytes (4 MiB) |
| New transactions validation state size soft limit | 572,864 bytes |
| Per-receipt storage proof size limit | 4,000,000 bytes |

## Testnet Network Config

From `genesis-testnet/config.json`:

| Parameter | Value |
|---|---|
| RPC payload max size | 10,485,760 bytes (10 MiB) |
| Max peers | 40 |
| Minimum outbound peers | 5 |
| Ideal connections | 30–35 |
| Handshake timeout | 20 seconds |
| Ban window | 10,800 seconds (3 hours) |
| Peer expiration | 604,800 seconds (7 days) |

## Source

These values are derived from:
- `genesis-testnet/genesis.json` (protocol version 84 genesis config)
- `nearcore/core/parameters/res/runtime_configs/parameters.yaml` (base runtime config)
- `nearcore/core/parameters/src/snapshots/near_parameters__config_store__tests__84.json.snap` (protocol v84 snapshot)
- `nearcore/core/chain-configs/src/lib.rs` (chain default constants)
- `genesis-testnet/config.json` (network/RPC config)
