# Sparrow-Compatible Testnet Send/Receive/PSBT Walkthrough

Last updated: March 5, 2026.

This document records a reproducible testnet validation of the PSBT + send/receive workflow that Sparrow uses via Bitcoin Core-compatible RPC methods.

Validation command:

```bash
./scripts/e2e_testnet.sh
```

Execution timestamp (UTC): `2026-03-05T17:16:43Z`  
Release candidate commit under test: `bab1bd9d21459fa2070d970d006eaa97151e46ad`

## Workflow Coverage

Validated RPC flow (Sparrow-compatible lifecycle):

1. Create PSBT: `createpsbt`, `walletcreatefundedpsbt`
2. Decode/analyze PSBT: `decodepsbt`, `analyzepsbt`
3. Sign PSBT: `walletprocesspsbt`
4. Finalize to raw hex: `finalizepsbt`
5. Broadcast finalized transaction: `sendrawtransaction`
6. Send to destination address directly: `sendtoaddress`
7. Verify receive result via balance delta checks: `getbalance`

## Key Results

From `summary.txt` in the artifact bundle:

- `psbt_create_len=128`
- `psbt_funded_len=128`
- `psbt_signed_complete=true`
- `psbt_finalize_complete=true`
- `psbt_final_hex_len=170`
- `txid_raw=8db372ef4b00c8fb7041b7511234bd16acb0183fc132074b5eec558b24c402fa`
- `txid1=98ee21fa0be389adc6211bbc467e9f47655f6ff620605917e8af1f66a169f5a1`
- `txid2=a9f50733a774fbd72a98ab002eac0bea43b3e785febe099ea2f41ee17356dca0`
- `satoshi_balance_before=500000.0`
- `satoshi_balance_after=500001.35`
- `funded_balance_before=5.0`
- `funded_balance_after=3.64755609`
- `funded_debit=1.35244391`

Auth checks for Sparrow-critical methods also passed:

- `auth_psbt_noauth_http_code=401`
- `auth_psbt_wrong_http_code=401`
- `auth_psbt_ok_result_len=128`
- `auth_walletcreatefundedpsbt_ok_http_code=200`
- `auth_walletprocesspsbt_ok_http_code=200`
- `auth_finalizepsbt_ok_http_code=200`
- `auth_sendraw_ok_http_code=200`
- `auth_sendtoaddress_ok_http_code=200`

## Published Artifacts

Evidence bundle:

- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/summary.txt`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_createpsbt_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_walletcreatefundedpsbt_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_walletprocesspsbt_funded_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_finalizepsbt_funded_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_sendrawtransaction_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_sendtoaddress_1_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_sendtoaddress_2_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_getbalance_before_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_getbalance_after_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_getbalance_funded_before_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_getbalance_funded_after_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_auth_createpsbt_success_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_auth_walletcreatefundedpsbt_success_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_auth_walletprocesspsbt_success_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_auth_finalizepsbt_success_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_auth_sendrawtransaction_success_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/btc_auth_sendtoaddress_success_response.json`
- `docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/git-commit.txt`
