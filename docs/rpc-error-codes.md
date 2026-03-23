# Bitcoin Infinity RPC Error Code Reference

This document summarizes JSON-RPC error codes currently emitted by `bitinfinity-btcrpc`.

## Common Codes

| Code | Meaning | Typical Triggers |
| --- | --- | --- |
| `-32700` | Parse error | Invalid JSON body / malformed JSON-RPC envelope. |
| `-32601` | Method not found / intentionally unsupported | Unknown RPC method, or intentionally stubbed PoW/networking methods. |
| `-32602` | Invalid parameters | Missing required params, wrong types, invalid argument shapes. |
| `-32000` | Internal/runtime backend error | Node RPC failures, signing/key handling failures, timeout/internal execution errors. |

## Wallet and Transaction Codes

| Code | Meaning | Typical Triggers |
| --- | --- | --- |
| `-28` | Backend not ready/connected | Chain node unreachable or not fully available for queried data. |
| `-25` | Transaction submit/processing failed | Broadcast/submit failures, no valid payment output, tx execution submission error. |
| `-22` | Decode/format error | Invalid raw transaction, invalid PSBT payload/base64, malformed signed intent. |
| `-18` | Wallet not loaded | Wallet-scoped methods called after `unloadwallet`. |
| `-15` | Wallet encryption/lock state conflict | `walletlock` on unencrypted wallet, `encryptwallet` called when already encrypted. |
| `-14` | Wallet passphrase error | Wrong passphrase during encrypted wallet operations. |
| `-13` | Wallet locked | Signing/send methods invoked without unlocking via `walletpassphrase`. |
| `-8` | Invalid parameter range/value | Unsupported scan action, out-of-range values, empty wallet alias names. |
| `-6` | Insufficient funds / missing spendable key | Sender lacks funds or no usable local key entry exists. |
| `-5` | Not found / invalid identifier | Missing tx/block/address, invalid key/address input, mempool miss. |
| `-4` | Operation rejected by state/policy | Fee-bump unsupported, insufficient funds in specific flows, wallet persistence errors. |
| `-3` | Invalid amount | Non-positive/negative (depending on method), non-finite, overflowed, or sub-satoshi-precision amounts across send, PSBT/raw intent, and chain action/deposit/allowance flows. |
| `-1` | Generic unsupported operation | Explicitly disabled/not-implemented endpoints (e.g., unsupported wallet dump modes). |

## Notes

- Error-code behavior is compatibility-oriented but adapted for Bitcoin Infinity’s PoS/account-based architecture.
- For method-level behavior differences and stubs, see `docs/rpc-compatibility-matrix.md`.
