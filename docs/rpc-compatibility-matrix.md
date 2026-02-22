# Bitcoin Infinity RPC Compatibility Matrix

This matrix tracks Bitcoin Core RPC compatibility for the Issue #11 Tier 1/2/3 method sets, plus intentional PoS-chain stubs.

Status legend:
- `Core-like`: behaves close to Bitcoin Core semantics.
- `Adapted`: implemented, but semantics are adapted for account-based/PoS NEAR internals.
- `Intentional stub`: explicitly unsupported by design, with descriptive error messaging.

## Tier 1 (wallet-critical)

| Method | Status | Notes |
| --- | --- | --- |
| `getblockheader` | Core-like | Implemented with header lookup and Bitcoin-like response shape. |
| `gettransaction` | Adapted | Implemented over tx cache + NEAR status model. |
| `listunspent` | Adapted | Implemented via synthetic UTXO model over account balances. |
| `lockunspent` / `listlockunspent` | Core-like | Implemented wallet lock-state tracking. |
| `getaddressinfo` | Adapted | Implemented with Bitcoin-style fields and chain-specific internals. |
| `scantxoutset` | Adapted | Implemented against address/account state, returns scan summary. |
| `getrawtransaction` | Adapted | Implemented via cache + decode pathways. |
| `createrawtransaction` | Adapted | Implemented for Bitcoin-style construction in BIT workflow. |
| `signrawtransactionwithwallet` | Adapted | Implemented with wallet unlock enforcement and NEAR signing bridge. |

## Tier 2 (PSBT/HWW workflow)

| Method | Status | Notes |
| --- | --- | --- |
| `createpsbt` | Adapted | Implemented PSBT construction path. |
| `walletprocesspsbt` | Adapted | Implemented; now enforces unlocked wallet (`-13` when locked). |
| `finalizepsbt` | Adapted | Implemented with signed-input validation. |
| `decodepsbt` | Adapted | Implemented decode/inspection response. |
| `walletcreatefundedpsbt` | Adapted | Implemented funding + PSBT creation flow. |
| `analyzepsbt` | Adapted | Implemented signer/complete analysis reporting. |
| `combinepsbt` | Adapted | Implemented merge and mismatch guards. |
| `utxoupdatepsbt` | Adapted | Implemented update/roundtrip handling. |

## Wallet lifecycle/control

| Method | Status | Notes |
| --- | --- | --- |
| `listwallets` | Adapted | Implemented with active loaded-wallet visibility. |
| `loadwallet` | Adapted | Implemented alias load semantics in single-keystore mode. |
| `unloadwallet` | Adapted | Implemented unload semantics with lock/passphrase clearing. |
| `createwallet` | Adapted | Implemented alias create/load semantics (virtual wallet aliases). |
| `listwalletdir` | Adapted | Implemented active alias listing. |
| `getwalletinfo` | Adapted | Implemented wallet-loaded gating and dynamic wallet name. |

## Tier 3 (explorer/monitoring)

| Method | Status | Notes |
| --- | --- | --- |
| `getblock` | Adapted | Implemented with verbosity options mapped to NEAR-backed data. |
| `getblockstats` | Adapted | Implemented per-block statistics surface. |
| `getchaintips` | Adapted | Implemented chain-tip reporting from node status. |
| `getmempoolentry` | Adapted | Implemented; now pending-only (`-5` for non-pending entries). |
| `getmempoolancestors` | Adapted | Implemented pending graph traversal with transitive lookup + verbose mode. |
| `getmempooldescendants` | Adapted | Implemented pending graph traversal with transitive lookup + verbose mode. |
| `getrawmempool` | Adapted | Implemented pending tx listing (verbose and non-verbose). |

## Intentional PoS/architecture stubs

| Method(s) | Status | Rationale |
| --- | --- | --- |
| `generate`, `generatetoaddress`, `generatetodescriptor` | Intentional stub | Bitcoin Infinity is PoS; no CPU mining RPC support. |
| `getblocktemplate`, `generateblock`, `submitblock` | Intentional stub | No PoW template/block-mining workflow in NEAR consensus. |
| `addnode`, `disconnectnode`, `onetry` | Intentional stub | Peer management delegated to nearcore networking. |
| `getblockfilter` | Intentional stub | Compact block filter support not provided in current architecture. |

## Current caveats

- Mempool methods are backed by pending tx cache semantics, not a native Bitcoin mempool.
- Wallet-scoped RPCs consistently return `-18` when no wallet is loaded.
- Wallet aliases are virtual and map to a single underlying keystore backend.
- Several wallet/UTXO responses are synthetic adapters over account-based state.
- Compatibility is continuously enforced through auth-coverage checks, targeted unit tests, and CI/nightly fuzz pipelines.
