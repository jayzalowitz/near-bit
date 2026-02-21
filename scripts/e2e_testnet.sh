#!/usr/bin/env bash
set -euo pipefail
set +m

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_DIR="${ARTIFACT_DIR:-$ROOT_DIR/.context/e2e}"
WORK_DIR="$(mktemp -d /tmp/bitinfinity-e2e.XXXXXX)"
CHAIN_ID="bitinfinity-mainnet-e2e"
SATOSHI_ADDR="1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"
FUNDED_BALANCE_YOCTO="5000000000000000000000000" # 5 BIT
SEND_AMOUNT_RAW="0.1"
SEND_AMOUNT_1="1.0"
SEND_AMOUNT_2="0.25"
SEND_AMOUNT_PSBT="0.05"
SEND_AMOUNT_TOO_HIGH="999999.0"
MIN_EXPECTED_TOTAL_SENT="1.35"
MAX_EXPECTED_TOTAL_DEBIT="1.40"
NEAR_RPC_URL="${NEAR_RPC_URL:-http://127.0.0.1:3030}"
BTC_RPC_ADDR="${BTC_RPC_ADDR:-127.0.0.1:18332}"
BTC_RPC_AUTH_ADDR="${BTC_RPC_AUTH_ADDR:-127.0.0.1:18333}"
BTCRPC_AUTH_USER="${BTCRPC_AUTH_USER:-e2euser}"
BTCRPC_AUTH_PASS="${BTCRPC_AUTH_PASS:-e2epass}"

NEARD_BIN="${NEARD_BIN:-$ROOT_DIR/nearcore/target/release/neard}"
TOOLS_BIN="${TOOLS_BIN:-$ROOT_DIR/target/debug/bitinfinity-tools}"
NODE_BIN="${NODE_BIN:-$ROOT_DIR/target/debug/bitinfinity-neard}"
BTCRPC_BIN="${BTCRPC_BIN:-$ROOT_DIR/target/debug/bitinfinity-btcrpc}"
GENESIS_DIR="$WORK_DIR/genesis"
NODE_HOME="$WORK_DIR/home"
BTCRPC_HOME="$WORK_DIR/btcrpc-home"
BTCRPC_AUTH_HOME="$WORK_DIR/btcrpc-auth-home"
FUNDED_KEY_JSON="$WORK_DIR/funded-keypair.json"
EXTRA_RECORDS="$WORK_DIR/extra-records.json"
BTC_RECORDS="$WORK_DIR/generated-btc-records.json"
FUNDED_RECORD="$WORK_DIR/funded-record.json"

mkdir -p "$ARTIFACT_DIR"
mkdir -p "$BTCRPC_HOME"
mkdir -p "$BTCRPC_AUTH_HOME"

NODE_PID=""
BTCRPC_PID=""
BTCRPC_AUTH_PID=""
cleanup() {
  if [[ -n "$BTCRPC_AUTH_PID" ]] && kill -0 "$BTCRPC_AUTH_PID" 2>/dev/null; then
    kill "$BTCRPC_AUTH_PID" || true
    wait "$BTCRPC_AUTH_PID" || true
  fi
  if [[ -n "$BTCRPC_PID" ]] && kill -0 "$BTCRPC_PID" 2>/dev/null; then
    kill "$BTCRPC_PID" || true
    wait "$BTCRPC_PID" || true
  fi
  if [[ -n "$NODE_PID" ]] && kill -0 "$NODE_PID" 2>/dev/null; then
    kill "$NODE_PID" || true
    wait "$NODE_PID" || true
  fi
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

for cmd in cargo curl jq; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Missing required command: $cmd" >&2
    exit 1
  fi
done

if [[ ! -x "$NEARD_BIN" ]]; then
  echo "Missing neard binary at $NEARD_BIN" >&2
  echo "Set NEARD_BIN or build with: (cd nearcore && cargo build -p neard --release)" >&2
  exit 1
fi

btc_rpc_call() {
  local payload="$1"
  curl -s -H 'content-type: application/json' --data "$payload" "http://$BTC_RPC_ADDR/"
}

float_gt() {
  awk -v a="$1" -v b="$2" 'BEGIN { exit !(a > b) }'
}

float_lt() {
  awk -v a="$1" -v b="$2" 'BEGIN { exit !(a < b) }'
}

echo "[1/12] Building required binaries..."
cargo build -p bitinfinity-tools -p bitinfinity-neard -p bitinfinity-btcrpc >"$ARTIFACT_DIR/build.log" 2>&1

echo "[2/12] Generating funded Bitcoin keypair..."
"$TOOLS_BIN" generate-keypair --output "$FUNDED_KEY_JSON" >"$ARTIFACT_DIR/keygen.log" 2>&1
FUNDED_ADDR="$(jq -r '.bitcoin_address // empty' "$FUNDED_KEY_JSON")"
FUNDED_WIF="$(jq -r '.private_key_wif // empty' "$FUNDED_KEY_JSON")"
if [[ -z "$FUNDED_ADDR" || -z "$FUNDED_WIF" ]]; then
  echo "Failed to extract funded keypair from $FUNDED_KEY_JSON" >&2
  exit 1
fi

echo "[3/12] Generating synthetic genesis..."
"$TOOLS_BIN" generate-genesis \
  --testnet --num-accounts 10 --chain-id "$CHAIN_ID" --output-dir "$GENESIS_DIR" \
  >"$ARTIFACT_DIR/genesis.log" 2>&1

echo "[4/12] Creating extra funded account record..."
jq '[.records[] | select(.Account.account_id? and (.Account.account_id | test("^(1|3|bc1)")))]' \
  "$GENESIS_DIR/genesis.json" >"$BTC_RECORDS"

cat >"$FUNDED_RECORD" <<JSON
[
  {
    "Account": {
      "account_id": "$FUNDED_ADDR",
      "account": {
        "amount": "$FUNDED_BALANCE_YOCTO",
        "locked": "0",
        "code_hash": "11111111111111111111111111111111",
        "storage_usage": 0,
        "version": "V1"
      }
    }
  }
]
JSON

jq -s '.[0] + .[1]' "$BTC_RECORDS" "$FUNDED_RECORD" >"$EXTRA_RECORDS"

if [[ "$(jq 'length' "$EXTRA_RECORDS")" -lt 2 ]]; then
  echo "Merged records payload is unexpectedly small" >&2
  exit 1
fi

cat >"$ARTIFACT_DIR/extra_records_preview.json" <<JSON
[
  {
    "Account": {
      "account_id": "$FUNDED_ADDR"
    }
  }
]
JSON

echo "[5/12] Initializing node home..."
"$NODE_BIN" init \
  --home "$NODE_HOME" \
  --chain-id "$CHAIN_ID" \
  --account-id validator.bitinfinity \
  --genesis-records "$EXTRA_RECORDS" \
  --neard-bin "$NEARD_BIN" \
  >"$ARTIFACT_DIR/init.log" 2>&1

echo "[6/12] Starting bitinfinity-neard..."
(
  "$NODE_BIN" run --home "$NODE_HOME" --neard-bin "$NEARD_BIN" \
    >"$ARTIFACT_DIR/node.log" 2>&1 || true
) &
NODE_PID=$!

wait_for_near() {
  for _ in $(seq 1 60); do
    if curl -sf "$NEAR_RPC_URL/status" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

if ! wait_for_near; then
  echo "nearcore RPC did not become ready" >&2
  exit 1
fi

INITIAL_HEIGHT="$(curl -s "$NEAR_RPC_URL/status" | jq -r '.sync_info.latest_block_height // 0')"
sleep 5
LATER_HEIGHT="$(curl -s "$NEAR_RPC_URL/status" | jq -r '.sync_info.latest_block_height // 0')"
if [[ "$LATER_HEIGHT" -le "$INITIAL_HEIGHT" ]]; then
  echo "Warning: block height did not increase during warmup ($INITIAL_HEIGHT -> $LATER_HEIGHT)" >&2
fi

echo "[7/12] Starting bitinfinity-btcrpc..."
(
  HOME="$BTCRPC_HOME" BTC_RPC_NOAUTH=1 "$BTCRPC_BIN" \
    --near-rpc-url "$NEAR_RPC_URL" \
    --btc-rpc-addr "$BTC_RPC_ADDR" \
    >"$ARTIFACT_DIR/btcrpc.log" 2>&1 || true
) &
BTCRPC_PID=$!

wait_for_btcrpc() {
  local payload='{"jsonrpc":"2.0","id":"e2e","method":"getblockcount","params":[]}'
  for _ in $(seq 1 60); do
    if curl -sf -H 'content-type: application/json' --data "$payload" "http://$BTC_RPC_ADDR/" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

if ! wait_for_btcrpc; then
  echo "Bitcoin RPC bridge did not become ready" >&2
  exit 1
fi

echo "[8/12] Querying initial balances..."
cat >"$ARTIFACT_DIR/near_view_account_request.json" <<JSON
{"jsonrpc":"2.0","id":"e2e","method":"query","params":{"request_type":"view_account","finality":"final","account_id":"$SATOSHI_ADDR"}}
JSON
curl -s -H 'content-type: application/json' \
  --data @"$ARTIFACT_DIR/near_view_account_request.json" \
  "$NEAR_RPC_URL" >"$ARTIFACT_DIR/near_view_account_response.json"

cat >"$ARTIFACT_DIR/btc_getbalance_request.json" <<JSON
{"jsonrpc":"2.0","id":"e2e","method":"getbalance","params":["$SATOSHI_ADDR"]}
JSON
SATOSHI_BALANCE_BEFORE="$(curl -s -H 'content-type: application/json' \
  --data @"$ARTIFACT_DIR/btc_getbalance_request.json" \
  "http://$BTC_RPC_ADDR/" | tee "$ARTIFACT_DIR/btc_getbalance_before_response.json" | jq -r '.result // 0')"

FUNDED_BALANCE_BEFORE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"funded-before\",\"method\":\"getbalance\",\"params\":[\"$FUNDED_ADDR\"]}" \
  | tee "$ARTIFACT_DIR/btc_getbalance_funded_before_response.json" | jq -r '.result // 0')"

NEAR_AMOUNT="$(jq -r '.result.amount // "0"' "$ARTIFACT_DIR/near_view_account_response.json")"

if [[ "$NEAR_AMOUNT" == "0" ]]; then
  echo "NEAR account balance query returned 0 for $SATOSHI_ADDR" >&2
  exit 1
fi

if [[ "$FUNDED_BALANCE_BEFORE" == "0" || "$FUNDED_BALANCE_BEFORE" == "0.0" ]]; then
  echo "Funded test account balance is zero before send: $FUNDED_ADDR" >&2
  exit 1
fi

echo "[9/12] Importing key and validating wallet coin-control..."
IMPORT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"import\",\"method\":\"importprivkey\",\"params\":[\"$FUNDED_WIF\"]}" \
  | tee "$ARTIFACT_DIR/btc_importprivkey_response.json")"
if [[ "$(echo "$IMPORT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "importprivkey failed" >&2
  exit 1
fi

VALIDATE_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"validate-fund\",\"method\":\"validateaddress\",\"params\":[\"$FUNDED_ADDR\"]}" \
  | tee "$ARTIFACT_DIR/btc_validateaddress_response.json")"
if [[ "$(echo "$VALIDATE_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "validateaddress failed for funded address: $FUNDED_ADDR" >&2
  exit 1
fi
if [[ "$(echo "$VALIDATE_RESPONSE" | jq -r '.result.isvalid // false')" != "true" ]]; then
  echo "validateaddress reported funded address invalid: $FUNDED_ADDR" >&2
  exit 1
fi

ADDRESSINFO_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"addressinfo-fund\",\"method\":\"getaddressinfo\",\"params\":[\"$FUNDED_ADDR\"]}" \
  | tee "$ARTIFACT_DIR/btc_getaddressinfo_response.json")"
if [[ "$(echo "$ADDRESSINFO_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getaddressinfo failed for funded address: $FUNDED_ADDR" >&2
  exit 1
fi
if [[ "$(echo "$ADDRESSINFO_RESPONSE" | jq -r '.result.address // empty')" != "$FUNDED_ADDR" ]]; then
  echo "getaddressinfo returned unexpected address for funded key" >&2
  exit 1
fi

ADDRESSINFO_INVALID_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"addressinfo-invalid","method":"getaddressinfo","params":["not-a-bitcoin-address"]}' \
  | tee "$ARTIFACT_DIR/btc_getaddressinfo_invalid_response.json")"
ADDRESSINFO_INVALID_ERROR_CODE="$(echo "$ADDRESSINFO_INVALID_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$ADDRESSINFO_INVALID_ERROR_CODE" != "-5" ]]; then
  echo "getaddressinfo invalid-address path did not return -5 (got: $ADDRESSINFO_INVALID_ERROR_CODE)" >&2
  exit 1
fi

BESTBLOCK_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"bestblock","method":"getbestblockhash","params":[]}' \
  | tee "$ARTIFACT_DIR/btc_getbestblockhash_response.json")"
BEST_BLOCK_HASH="$(echo "$BESTBLOCK_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$BEST_BLOCK_HASH" ]]; then
  echo "getbestblockhash returned empty hash" >&2
  exit 1
fi

BLOCKHEADER_HEX_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"blockheader-hex\",\"method\":\"getblockheader\",\"params\":[\"$BEST_BLOCK_HASH\",false]}" \
  | tee "$ARTIFACT_DIR/btc_getblockheader_hex_response.json")"
HEADER_HEX="$(echo "$BLOCKHEADER_HEX_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$HEADER_HEX" || "${#HEADER_HEX}" -ne 160 ]]; then
  echo "getblockheader(verbose=false) returned invalid raw header length: ${#HEADER_HEX}" >&2
  exit 1
fi

BLOCKHEADER_UNKNOWN_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"blockheader-unknown","method":"getblockheader","params":["0000000000000000000000000000000000000000000000000000000000000000",true]}' \
  | tee "$ARTIFACT_DIR/btc_getblockheader_unknown_response.json")"
BLOCKHEADER_UNKNOWN_ERROR_CODE="$(echo "$BLOCKHEADER_UNKNOWN_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$BLOCKHEADER_UNKNOWN_ERROR_CODE" != "-5" ]]; then
  echo "getblockheader unknown-hash path did not return -5 (got: $BLOCKHEADER_UNKNOWN_ERROR_CODE)" >&2
  exit 1
fi

BLOCKHEADER_JSON_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"blockheader-json\",\"method\":\"getblockheader\",\"params\":[\"$BEST_BLOCK_HASH\",true]}" \
  | tee "$ARTIFACT_DIR/btc_getblockheader_json_response.json")"
if [[ "$(echo "$BLOCKHEADER_JSON_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getblockheader(verbose=true) failed for best block hash" >&2
  exit 1
fi
if [[ "$(echo "$BLOCKHEADER_JSON_RESPONSE" | jq -r '.result.hash // empty')" != "$BEST_BLOCK_HASH" ]]; then
  echo "getblockheader(verbose=true) returned unexpected hash" >&2
  exit 1
fi

SCANTXOUTSET_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"scan-fund\",\"method\":\"scantxoutset\",\"params\":[\"start\",[{\"desc\":\"addr($FUNDED_ADDR)\"}]]}" \
  | tee "$ARTIFACT_DIR/btc_scantxoutset_response.json")"
if [[ "$(echo "$SCANTXOUTSET_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "scantxoutset failed for funded address descriptor" >&2
  exit 1
fi
if [[ "$(echo "$SCANTXOUTSET_RESPONSE" | jq -r '.result.success // false')" != "true" ]]; then
  echo "scantxoutset did not report success" >&2
  exit 1
fi
SCANTXOUTSET_TXID="$(echo "$SCANTXOUTSET_RESPONSE" | jq -r '.result.unspents[0].txid // empty')"
if [[ -z "$SCANTXOUTSET_TXID" ]]; then
  echo "scantxoutset did not return a txid for funded descriptor" >&2
  exit 1
fi

SCANTXOUTSET_INVALID_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"scan-invalid","method":"scantxoutset","params":["nonsense",[]]}' \
  | tee "$ARTIFACT_DIR/btc_scantxoutset_invalid_response.json")"
SCANTXOUTSET_INVALID_ERROR_CODE="$(echo "$SCANTXOUTSET_INVALID_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$SCANTXOUTSET_INVALID_ERROR_CODE" != "-8" ]]; then
  echo "scantxoutset invalid-action path did not return -8 (got: $SCANTXOUTSET_INVALID_ERROR_CODE)" >&2
  exit 1
fi

SCANTXOUTSET_EMPTY_START_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"scan-empty-start","method":"scantxoutset","params":["start",[]]}' \
  | tee "$ARTIFACT_DIR/btc_scantxoutset_empty_start_response.json")"
SCANTXOUTSET_EMPTY_START_ERROR_CODE="$(echo "$SCANTXOUTSET_EMPTY_START_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$SCANTXOUTSET_EMPTY_START_ERROR_CODE" != "-8" ]]; then
  echo "scantxoutset empty-start path did not return -8 (got: $SCANTXOUTSET_EMPTY_START_ERROR_CODE)" >&2
  exit 1
fi

LISTUNSPENT_INVALID_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"listunspent-invalid-range\",\"method\":\"listunspent\",\"params\":[10,1,[\"$FUNDED_ADDR\"]]}" \
  | tee "$ARTIFACT_DIR/btc_listunspent_invalid_range_response.json")"
LISTUNSPENT_INVALID_ERROR_CODE="$(echo "$LISTUNSPENT_INVALID_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$LISTUNSPENT_INVALID_ERROR_CODE" != "-8" ]]; then
  echo "listunspent invalid-range path did not return -8 (got: $LISTUNSPENT_INVALID_ERROR_CODE)" >&2
  exit 1
fi

LISTUNSPENT_INVALID_ADDRS_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"listunspent-invalid-addresses","method":"listunspent","params":[1,9999999,[123]]}' \
  | tee "$ARTIFACT_DIR/btc_listunspent_invalid_addresses_response.json")"
LISTUNSPENT_INVALID_ADDRS_ERROR_CODE="$(echo "$LISTUNSPENT_INVALID_ADDRS_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$LISTUNSPENT_INVALID_ADDRS_ERROR_CODE" != "-32602" ]]; then
  echo "listunspent invalid-addresses path did not return -32602 (got: $LISTUNSPENT_INVALID_ADDRS_ERROR_CODE)" >&2
  exit 1
fi

LISTUNSPENT_BEFORE_LOCK="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"listunspent-before-lock\",\"method\":\"listunspent\",\"params\":[1,9999999,[\"$FUNDED_ADDR\"]]}" \
  | tee "$ARTIFACT_DIR/btc_listunspent_before_lock_response.json")"
LOCK_TXID="$(echo "$LISTUNSPENT_BEFORE_LOCK" | jq -r '.result[0].txid // empty')"
LOCK_VOUT="$(echo "$LISTUNSPENT_BEFORE_LOCK" | jq -r '.result[0].vout // empty')"
if [[ -z "$LOCK_TXID" || -z "$LOCK_VOUT" ]]; then
  echo "listunspent did not return a lockable UTXO for funded address: $FUNDED_ADDR" >&2
  exit 1
fi
if [[ "$SCANTXOUTSET_TXID" != "$LOCK_TXID" ]]; then
  echo "scantxoutset txid does not match stable listunspent txid ($SCANTXOUTSET_TXID != $LOCK_TXID)" >&2
  exit 1
fi

GETTX_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"gettransaction-synth\",\"method\":\"gettransaction\",\"params\":[\"$LOCK_TXID\"]}" \
  | tee "$ARTIFACT_DIR/btc_gettransaction_synthetic_response.json")"
if [[ "$(echo "$GETTX_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "gettransaction failed for synthetic listunspent txid: $LOCK_TXID" >&2
  exit 1
fi
if [[ "$(echo "$GETTX_RESPONSE" | jq -r '.result.txid // empty')" != "$LOCK_TXID" ]]; then
  echo "gettransaction returned unexpected txid for synthetic tx" >&2
  exit 1
fi

GETRAW_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getrawtransaction-synth\",\"method\":\"getrawtransaction\",\"params\":[\"$LOCK_TXID\",1]}" \
  | tee "$ARTIFACT_DIR/btc_getrawtransaction_synthetic_response.json")"
if [[ "$(echo "$GETRAW_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getrawtransaction failed for synthetic listunspent txid: $LOCK_TXID" >&2
  exit 1
fi
if [[ "$(echo "$GETRAW_RESPONSE" | jq -r '.result.txid // empty')" != "$LOCK_TXID" ]]; then
  echo "getrawtransaction returned unexpected txid for synthetic tx" >&2
  exit 1
fi

UNKNOWN_TXID="$(printf 'f%.0s' $(seq 1 64))"
GETTX_UNKNOWN_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"gettransaction-unknown\",\"method\":\"gettransaction\",\"params\":[\"$UNKNOWN_TXID\"]}" \
  | tee "$ARTIFACT_DIR/btc_gettransaction_unknown_response.json")"
GETTX_UNKNOWN_ERROR_CODE="$(echo "$GETTX_UNKNOWN_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$GETTX_UNKNOWN_ERROR_CODE" != "-5" ]]; then
  echo "gettransaction unknown-tx path did not return -5 (got: $GETTX_UNKNOWN_ERROR_CODE)" >&2
  exit 1
fi

GETRAW_UNKNOWN_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getrawtransaction-unknown\",\"method\":\"getrawtransaction\",\"params\":[\"$UNKNOWN_TXID\",1]}" \
  | tee "$ARTIFACT_DIR/btc_getrawtransaction_unknown_response.json")"
GETRAW_UNKNOWN_ERROR_CODE="$(echo "$GETRAW_UNKNOWN_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$GETRAW_UNKNOWN_ERROR_CODE" != "-5" ]]; then
  echo "getrawtransaction unknown-tx path did not return -5 (got: $GETRAW_UNKNOWN_ERROR_CODE)" >&2
  exit 1
fi

LOCK_MISSING_TXOS_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"lockunspent-missing-txos","method":"lockunspent","params":[false]}' \
  | tee "$ARTIFACT_DIR/btc_lockunspent_missing_txos_response.json")"
LOCK_MISSING_TXOS_ERROR_CODE="$(echo "$LOCK_MISSING_TXOS_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$LOCK_MISSING_TXOS_ERROR_CODE" != "-32602" ]]; then
  echo "lockunspent missing-txos path did not return -32602 (got: $LOCK_MISSING_TXOS_ERROR_CODE)" >&2
  exit 1
fi

LOCK_INVALID_TXID_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"lockunspent-invalid-txid","method":"lockunspent","params":[false,[{"txid":"abcd","vout":0}]]}' \
  | tee "$ARTIFACT_DIR/btc_lockunspent_invalid_txid_response.json")"
LOCK_INVALID_TXID_ERROR_CODE="$(echo "$LOCK_INVALID_TXID_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$LOCK_INVALID_TXID_ERROR_CODE" != "-8" ]]; then
  echo "lockunspent invalid-txid path did not return -8 (got: $LOCK_INVALID_TXID_ERROR_CODE)" >&2
  exit 1
fi

LOCK_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"lockunspent-lock\",\"method\":\"lockunspent\",\"params\":[false,[{\"txid\":\"$LOCK_TXID\",\"vout\":$LOCK_VOUT}]]}" \
  | tee "$ARTIFACT_DIR/btc_lockunspent_lock_response.json")"
if [[ "$(echo "$LOCK_RESPONSE" | jq -r '.result // false')" != "true" ]]; then
  echo "lockunspent(false, ...) failed for $LOCK_TXID:$LOCK_VOUT" >&2
  exit 1
fi

LISTLOCKED_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"listlockunspent-after-lock","method":"listlockunspent","params":[]}' \
  | tee "$ARTIFACT_DIR/btc_listlockunspent_after_lock_response.json")"
LOCKED_MATCH_COUNT="$(echo "$LISTLOCKED_RESPONSE" | jq --arg txid "$LOCK_TXID" --argjson vout "$LOCK_VOUT" '[.result[] | select(.txid == $txid and .vout == $vout)] | length')"
if [[ "$LOCKED_MATCH_COUNT" -lt 1 ]]; then
  echo "listlockunspent missing expected entry $LOCK_TXID:$LOCK_VOUT after lock" >&2
  exit 1
fi

LISTUNSPENT_WHILE_LOCKED="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"listunspent-while-locked\",\"method\":\"listunspent\",\"params\":[1,9999999,[\"$FUNDED_ADDR\"]]}" \
  | tee "$ARTIFACT_DIR/btc_listunspent_while_locked_response.json")"
LOCKED_VISIBLE_COUNT="$(echo "$LISTUNSPENT_WHILE_LOCKED" | jq --arg txid "$LOCK_TXID" --argjson vout "$LOCK_VOUT" '[.result[] | select(.txid == $txid and .vout == $vout)] | length')"
if [[ "$LOCKED_VISIBLE_COUNT" -ne 0 ]]; then
  echo "listunspent still returned locked UTXO $LOCK_TXID:$LOCK_VOUT" >&2
  exit 1
fi

WCF_WHILE_LOCKED_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"walletcreatefundedpsbt-while-locked\",\"method\":\"walletcreatefundedpsbt\",\"params\":[[],[{\"$SATOSHI_ADDR\":0.01}],0,{}]}" \
  | tee "$ARTIFACT_DIR/btc_walletcreatefundedpsbt_while_locked_response.json")"
WCF_WHILE_LOCKED_ERROR_CODE="$(echo "$WCF_WHILE_LOCKED_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$WCF_WHILE_LOCKED_ERROR_CODE" != "-4" ]]; then
  echo "walletcreatefundedpsbt while locked did not return -4 (got: $WCF_WHILE_LOCKED_ERROR_CODE)" >&2
  exit 1
fi

UNLOCK_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"lockunspent-unlock\",\"method\":\"lockunspent\",\"params\":[true,[{\"txid\":\"$LOCK_TXID\",\"vout\":$LOCK_VOUT}]]}" \
  | tee "$ARTIFACT_DIR/btc_lockunspent_unlock_response.json")"
if [[ "$(echo "$UNLOCK_RESPONSE" | jq -r '.result // false')" != "true" ]]; then
  echo "lockunspent(true, ...) failed for $LOCK_TXID:$LOCK_VOUT" >&2
  exit 1
fi

LISTLOCKED_AFTER_UNLOCK="$(btc_rpc_call '{"jsonrpc":"2.0","id":"listlockunspent-after-unlock","method":"listlockunspent","params":[]}' \
  | tee "$ARTIFACT_DIR/btc_listlockunspent_after_unlock_response.json")"
LOCKED_AFTER_UNLOCK_COUNT="$(echo "$LISTLOCKED_AFTER_UNLOCK" | jq --arg txid "$LOCK_TXID" --argjson vout "$LOCK_VOUT" '[.result[] | select(.txid == $txid and .vout == $vout)] | length')"
if [[ "$LOCKED_AFTER_UNLOCK_COUNT" -ne 0 ]]; then
  echo "listlockunspent still contains $LOCK_TXID:$LOCK_VOUT after unlock" >&2
  exit 1
fi

LISTUNSPENT_AFTER_UNLOCK="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"listunspent-after-unlock\",\"method\":\"listunspent\",\"params\":[1,9999999,[\"$FUNDED_ADDR\"]]}" \
  | tee "$ARTIFACT_DIR/btc_listunspent_after_unlock_response.json")"
UNLOCKED_VISIBLE_COUNT="$(echo "$LISTUNSPENT_AFTER_UNLOCK" | jq --arg txid "$LOCK_TXID" --argjson vout "$LOCK_VOUT" '[.result[] | select(.txid == $txid and .vout == $vout)] | length')"
if [[ "$UNLOCKED_VISIBLE_COUNT" -lt 1 ]]; then
  echo "listunspent did not restore unlocked UTXO $LOCK_TXID:$LOCK_VOUT" >&2
  exit 1
fi

LOCK_NUMERIC_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"lockunspent-lock-numeric\",\"method\":\"lockunspent\",\"params\":[0,[{\"txid\":\"$LOCK_TXID\",\"vout\":$LOCK_VOUT}]]}" \
  | tee "$ARTIFACT_DIR/btc_lockunspent_lock_numeric_response.json")"
LOCK_NUMERIC_RESULT="$(echo "$LOCK_NUMERIC_RESPONSE" | jq -r '.result // false')"
if [[ "$LOCK_NUMERIC_RESULT" != "true" ]]; then
  echo "lockunspent with numeric lock flag failed for $LOCK_TXID:$LOCK_VOUT" >&2
  exit 1
fi

UNLOCK_NUMERIC_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"lockunspent-unlock-numeric","method":"lockunspent","params":[1]}' \
  | tee "$ARTIFACT_DIR/btc_lockunspent_unlock_numeric_response.json")"
UNLOCK_NUMERIC_RESULT="$(echo "$UNLOCK_NUMERIC_RESPONSE" | jq -r '.result // false')"
if [[ "$UNLOCK_NUMERIC_RESULT" != "true" ]]; then
  echo "lockunspent with numeric unlock-all flag failed" >&2
  exit 1
fi

LISTLOCKED_AFTER_NUMERIC_UNLOCK="$(btc_rpc_call '{"jsonrpc":"2.0","id":"listlockunspent-after-numeric-unlock","method":"listlockunspent","params":[]}' \
  | tee "$ARTIFACT_DIR/btc_listlockunspent_after_numeric_unlock_response.json")"
LOCKED_AFTER_NUMERIC_UNLOCK_COUNT="$(echo "$LISTLOCKED_AFTER_NUMERIC_UNLOCK" | jq --arg txid "$LOCK_TXID" --argjson vout "$LOCK_VOUT" '[.result[] | select(.txid == $txid and .vout == $vout)] | length')"
if [[ "$LOCKED_AFTER_NUMERIC_UNLOCK_COUNT" -ne 0 ]]; then
  echo "listlockunspent still contains $LOCK_TXID:$LOCK_VOUT after numeric unlock-all" >&2
  exit 1
fi

echo "[10/12] Verifying PSBT flow, then sending via raw+wallet and sendtoaddress..."
CREATE_PSBT_INVALID_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"create-psbt-invalid-output\",\"method\":\"createpsbt\",\"params\":[[{\"txid\":\"$LOCK_TXID\",\"vout\":$LOCK_VOUT}],{\"$SATOSHI_ADDR\":\"bad-amount\"},0,true]}" \
  | tee "$ARTIFACT_DIR/btc_createpsbt_invalid_output_response.json")"
CREATE_PSBT_INVALID_ERROR_CODE="$(echo "$CREATE_PSBT_INVALID_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$CREATE_PSBT_INVALID_ERROR_CODE" != "-32602" ]]; then
  echo "createpsbt invalid-output path did not return -32602 (got: $CREATE_PSBT_INVALID_ERROR_CODE)" >&2
  exit 1
fi

CREATE_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"create-psbt\",\"method\":\"createpsbt\",\"params\":[[{\"txid\":\"$LOCK_TXID\",\"vout\":$LOCK_VOUT}],[{\"$SATOSHI_ADDR\":0.01}],0,true]}" \
  | tee "$ARTIFACT_DIR/btc_createpsbt_response.json")"
CREATED_PSBT="$(echo "$CREATE_PSBT_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$CREATED_PSBT" ]]; then
  echo "createpsbt returned empty result" >&2
  exit 1
fi
if [[ "$(echo "$CREATE_PSBT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "createpsbt failed" >&2
  exit 1
fi

CREATE_PSBT_OBJECT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"create-psbt-object-outputs\",\"method\":\"createpsbt\",\"params\":[[{\"txid\":\"$LOCK_TXID\",\"vout\":$LOCK_VOUT}],{\"$SATOSHI_ADDR\":0.003,\"$FUNDED_ADDR\":0.002},0,true]}" \
  | tee "$ARTIFACT_DIR/btc_createpsbt_object_outputs_response.json")"
CREATED_OBJECT_PSBT="$(echo "$CREATE_PSBT_OBJECT_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$CREATED_OBJECT_PSBT" ]]; then
  echo "createpsbt (object outputs) returned empty result" >&2
  exit 1
fi
if [[ "$(echo "$CREATE_PSBT_OBJECT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "createpsbt (object outputs) failed" >&2
  exit 1
fi
DECODE_OBJECT_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"decodepsbt-object-outputs\",\"method\":\"decodepsbt\",\"params\":[\"$CREATED_OBJECT_PSBT\"]}" \
  | tee "$ARTIFACT_DIR/btc_decodepsbt_object_outputs_response.json")"
OBJECT_PSBT_VOUT_COUNT="$(echo "$DECODE_OBJECT_PSBT_RESPONSE" | jq -r '.result.tx.vout | length')"
if [[ "$OBJECT_PSBT_VOUT_COUNT" -ne 2 ]]; then
  echo "createpsbt object outputs did not produce 2 outputs (got: $OBJECT_PSBT_VOUT_COUNT)" >&2
  exit 1
fi

WCF_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"walletcreatefundedpsbt\",\"method\":\"walletcreatefundedpsbt\",\"params\":[[],[{\"$SATOSHI_ADDR\":$SEND_AMOUNT_PSBT}],0,{}]}" \
  | tee "$ARTIFACT_DIR/btc_walletcreatefundedpsbt_response.json")"
FUNDED_PSBT="$(echo "$WCF_PSBT_RESPONSE" | jq -r '.result.psbt // empty')"
if [[ -z "$FUNDED_PSBT" ]]; then
  echo "walletcreatefundedpsbt returned empty psbt" >&2
  exit 1
fi
if [[ "$(echo "$WCF_PSBT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "walletcreatefundedpsbt failed" >&2
  exit 1
fi

WCF_PSBT_OBJECT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"walletcreatefundedpsbt-object-outputs\",\"method\":\"walletcreatefundedpsbt\",\"params\":[[],{\"$SATOSHI_ADDR\":$SEND_AMOUNT_PSBT},0,{}]}" \
  | tee "$ARTIFACT_DIR/btc_walletcreatefundedpsbt_object_outputs_response.json")"
FUNDED_OBJECT_PSBT="$(echo "$WCF_PSBT_OBJECT_RESPONSE" | jq -r '.result.psbt // empty')"
if [[ -z "$FUNDED_OBJECT_PSBT" ]]; then
  echo "walletcreatefundedpsbt (object outputs) returned empty psbt" >&2
  exit 1
fi
if [[ "$(echo "$WCF_PSBT_OBJECT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "walletcreatefundedpsbt (object outputs) failed" >&2
  exit 1
fi
DECODE_FUNDED_OBJECT_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"decodepsbt-funded-object-outputs\",\"method\":\"decodepsbt\",\"params\":[\"$FUNDED_OBJECT_PSBT\"]}" \
  | tee "$ARTIFACT_DIR/btc_decodepsbt_funded_object_outputs_response.json")"
FUNDED_OBJECT_PSBT_VOUT_COUNT="$(echo "$DECODE_FUNDED_OBJECT_PSBT_RESPONSE" | jq -r '.result.tx.vout | length')"
if [[ "$FUNDED_OBJECT_PSBT_VOUT_COUNT" -lt 1 ]]; then
  echo "walletcreatefundedpsbt object outputs decode had no outputs" >&2
  exit 1
fi

WCF_PSBT_INVALID_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"walletcreatefundedpsbt-invalid-output\",\"method\":\"walletcreatefundedpsbt\",\"params\":[[],{\"$SATOSHI_ADDR\":\"bad-amount\"},0,{}]}" \
  | tee "$ARTIFACT_DIR/btc_walletcreatefundedpsbt_invalid_output_response.json")"
WCF_PSBT_INVALID_ERROR_CODE="$(echo "$WCF_PSBT_INVALID_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$WCF_PSBT_INVALID_ERROR_CODE" != "-32602" ]]; then
  echo "walletcreatefundedpsbt invalid-output path did not return -32602 (got: $WCF_PSBT_INVALID_ERROR_CODE)" >&2
  exit 1
fi

WCF_PSBT_INSUFFICIENT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"walletcreatefundedpsbt-insufficient\",\"method\":\"walletcreatefundedpsbt\",\"params\":[[],[{\"$SATOSHI_ADDR\":999999.0}],0,{}]}" \
  | tee "$ARTIFACT_DIR/btc_walletcreatefundedpsbt_insufficient_response.json")"
WCF_PSBT_INSUFFICIENT_ERROR_CODE="$(echo "$WCF_PSBT_INSUFFICIENT_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$WCF_PSBT_INSUFFICIENT_ERROR_CODE" != "-4" ]]; then
  echo "walletcreatefundedpsbt insufficient-funds path did not return -4 (got: $WCF_PSBT_INSUFFICIENT_ERROR_CODE)" >&2
  exit 1
fi

DECODE_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"decodepsbt-funded\",\"method\":\"decodepsbt\",\"params\":[\"$FUNDED_PSBT\"]}" \
  | tee "$ARTIFACT_DIR/btc_decodepsbt_funded_response.json")"
if [[ "$(echo "$DECODE_PSBT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "decodepsbt failed for walletcreatefundedpsbt output" >&2
  exit 1
fi
if [[ "$(echo "$DECODE_PSBT_RESPONSE" | jq -r '.result.tx.vout | length')" -lt 1 ]]; then
  echo "decodepsbt reported no outputs for funded psbt" >&2
  exit 1
fi
FUNDED_PSBT_INPUT_TXID="$(echo "$DECODE_PSBT_RESPONSE" | jq -r '.result.tx.vin[0].txid // empty')"
if [[ "$FUNDED_PSBT_INPUT_TXID" != "$LOCK_TXID" ]]; then
  echo "walletcreatefundedpsbt used unstable/unexpected synthetic input txid ($FUNDED_PSBT_INPUT_TXID != $LOCK_TXID)" >&2
  exit 1
fi

ANALYZE_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"analyzepsbt-funded\",\"method\":\"analyzepsbt\",\"params\":[\"$FUNDED_PSBT\"]}" \
  | tee "$ARTIFACT_DIR/btc_analyzepsbt_funded_response.json")"
if [[ "$(echo "$ANALYZE_PSBT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "analyzepsbt failed for funded psbt" >&2
  exit 1
fi
ANALYZE_PSBT_NEXT="$(echo "$ANALYZE_PSBT_RESPONSE" | jq -r '.result.next // empty')"
if [[ "$ANALYZE_PSBT_NEXT" != "signer" ]]; then
  echo "analyzepsbt expected next=signer before signing, got: $ANALYZE_PSBT_NEXT" >&2
  exit 1
fi

FINALIZE_UNSIGNED_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"finalizepsbt-unsigned\",\"method\":\"finalizepsbt\",\"params\":[\"$FUNDED_PSBT\"]}" \
  | tee "$ARTIFACT_DIR/btc_finalizepsbt_unsigned_response.json")"
FINALIZE_UNSIGNED_HEX="$(echo "$FINALIZE_UNSIGNED_PSBT_RESPONSE" | jq -r '.result.hex // empty')"
FINALIZE_UNSIGNED_COMPLETE="$(echo "$FINALIZE_UNSIGNED_PSBT_RESPONSE" | jq -r '.result.complete')"
if [[ "$(echo "$FINALIZE_UNSIGNED_PSBT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "finalizepsbt failed for unsigned funded psbt" >&2
  exit 1
fi
if [[ "$FINALIZE_UNSIGNED_COMPLETE" != "false" ]]; then
  echo "finalizepsbt expected complete=false for unsigned funded psbt" >&2
  exit 1
fi
if [[ -n "$FINALIZE_UNSIGNED_HEX" ]]; then
  echo "finalizepsbt expected empty hex for unsigned funded psbt" >&2
  exit 1
fi

UTXOUPDATE_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"utxoupdatepsbt-funded\",\"method\":\"utxoupdatepsbt\",\"params\":[\"$FUNDED_PSBT\"]}" \
  | tee "$ARTIFACT_DIR/btc_utxoupdatepsbt_funded_response.json")"
UPDATED_PSBT="$(echo "$UTXOUPDATE_PSBT_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$UPDATED_PSBT" ]]; then
  echo "utxoupdatepsbt returned empty psbt" >&2
  exit 1
fi
if [[ "$(echo "$UTXOUPDATE_PSBT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "utxoupdatepsbt failed" >&2
  exit 1
fi

UTXOUPDATE_INVALID_PSBT_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"utxoupdatepsbt-invalid","method":"utxoupdatepsbt","params":["***not-base64***"]}' \
  | tee "$ARTIFACT_DIR/btc_utxoupdatepsbt_invalid_response.json")"
UTXOUPDATE_INVALID_ERROR_CODE="$(echo "$UTXOUPDATE_INVALID_PSBT_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$UTXOUPDATE_INVALID_ERROR_CODE" != "-22" ]]; then
  echo "utxoupdatepsbt invalid path did not return -22 (got: $UTXOUPDATE_INVALID_ERROR_CODE)" >&2
  exit 1
fi

WALLETPROCESS_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"walletprocesspsbt-funded\",\"method\":\"walletprocesspsbt\",\"params\":[\"$UPDATED_PSBT\"]}" \
  | tee "$ARTIFACT_DIR/btc_walletprocesspsbt_funded_response.json")"
SIGNED_PSBT="$(echo "$WALLETPROCESS_PSBT_RESPONSE" | jq -r '.result.psbt // empty')"
SIGNED_PSBT_COMPLETE="$(echo "$WALLETPROCESS_PSBT_RESPONSE" | jq -r '.result.complete // false')"
if [[ -z "$SIGNED_PSBT" ]]; then
  echo "walletprocesspsbt returned empty psbt" >&2
  exit 1
fi
if [[ "$(echo "$WALLETPROCESS_PSBT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "walletprocesspsbt failed" >&2
  exit 1
fi
if [[ "$SIGNED_PSBT_COMPLETE" != "true" ]]; then
  echo "walletprocesspsbt did not report complete=true" >&2
  exit 1
fi

DECODE_SIGNED_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"decodepsbt-signed\",\"method\":\"decodepsbt\",\"params\":[\"$SIGNED_PSBT\"]}" \
  | tee "$ARTIFACT_DIR/btc_decodepsbt_signed_response.json")"
if [[ "$(echo "$DECODE_SIGNED_PSBT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "decodepsbt failed for signed psbt" >&2
  exit 1
fi
SIGNED_PSBT_SIG_COUNT="$(echo "$DECODE_SIGNED_PSBT_RESPONSE" | jq '[.result.inputs[]? | (.partial_signatures // {}) | length] | add // 0')"
if [[ "$SIGNED_PSBT_SIG_COUNT" -lt 1 ]]; then
  echo "signed psbt decode showed no partial_signatures" >&2
  exit 1
fi

ANALYZE_SIGNED_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"analyzepsbt-signed\",\"method\":\"analyzepsbt\",\"params\":[\"$SIGNED_PSBT\"]}" \
  | tee "$ARTIFACT_DIR/btc_analyzepsbt_signed_response.json")"
if [[ "$(echo "$ANALYZE_SIGNED_PSBT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "analyzepsbt failed for signed psbt" >&2
  exit 1
fi
ANALYZE_SIGNED_NEXT="$(echo "$ANALYZE_SIGNED_PSBT_RESPONSE" | jq -r '.result.next // empty')"
ANALYZE_SIGNED_IS_FINAL="$(echo "$ANALYZE_SIGNED_PSBT_RESPONSE" | jq -r '.result.inputs[0].is_final // false')"
if [[ "$ANALYZE_SIGNED_NEXT" != "finalizer" ]]; then
  echo "analyzepsbt expected next=finalizer after signing, got: $ANALYZE_SIGNED_NEXT" >&2
  exit 1
fi
if [[ "$ANALYZE_SIGNED_IS_FINAL" != "true" ]]; then
  echo "analyzepsbt expected first signed input to be final" >&2
  exit 1
fi

COMBINE_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"combinepsbt-funded\",\"method\":\"combinepsbt\",\"params\":[[\"$FUNDED_PSBT\",\"$SIGNED_PSBT\"]]}" \
  | tee "$ARTIFACT_DIR/btc_combinepsbt_funded_response.json")"
COMBINED_PSBT="$(echo "$COMBINE_PSBT_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$COMBINED_PSBT" ]]; then
  echo "combinepsbt returned empty result" >&2
  exit 1
fi
if [[ "$(echo "$COMBINE_PSBT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "combinepsbt failed" >&2
  exit 1
fi

JOIN_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"joinpsbts-funded\",\"method\":\"joinpsbts\",\"params\":[[\"$FUNDED_PSBT\",\"$SIGNED_PSBT\"]]}" \
  | tee "$ARTIFACT_DIR/btc_joinpsbts_funded_response.json")"
JOINED_PSBT="$(echo "$JOIN_PSBT_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$JOINED_PSBT" ]]; then
  echo "joinpsbts returned empty result" >&2
  exit 1
fi
if [[ "$(echo "$JOIN_PSBT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "joinpsbts failed" >&2
  exit 1
fi
if [[ "$JOINED_PSBT" != "$COMBINED_PSBT" ]]; then
  echo "joinpsbts returned unexpected candidate versus combinepsbt" >&2
  exit 1
fi

COMBINE_MISMATCH_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"combinepsbt-mismatch\",\"method\":\"combinepsbt\",\"params\":[[\"$CREATED_PSBT\",\"$SIGNED_PSBT\"]]}" \
  | tee "$ARTIFACT_DIR/btc_combinepsbt_mismatch_response.json")"
COMBINE_MISMATCH_ERROR_CODE="$(echo "$COMBINE_MISMATCH_PSBT_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$COMBINE_MISMATCH_ERROR_CODE" != "-8" ]]; then
  echo "combinepsbt mismatched-transaction path did not return -8 (got: $COMBINE_MISMATCH_ERROR_CODE)" >&2
  exit 1
fi

JOIN_MISMATCH_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"joinpsbts-mismatch\",\"method\":\"joinpsbts\",\"params\":[[\"$CREATED_PSBT\",\"$SIGNED_PSBT\"]]}" \
  | tee "$ARTIFACT_DIR/btc_joinpsbts_mismatch_response.json")"
JOIN_MISMATCH_ERROR_CODE="$(echo "$JOIN_MISMATCH_PSBT_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$JOIN_MISMATCH_ERROR_CODE" != "-8" ]]; then
  echo "joinpsbts mismatched-transaction path did not return -8 (got: $JOIN_MISMATCH_ERROR_CODE)" >&2
  exit 1
fi

JOIN_INVALID_PSBT_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"joinpsbts-invalid","method":"joinpsbts","params":[["***not-base64***",""]]}' \
  | tee "$ARTIFACT_DIR/btc_joinpsbts_invalid_response.json")"
JOIN_INVALID_ERROR_CODE="$(echo "$JOIN_INVALID_PSBT_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$JOIN_INVALID_ERROR_CODE" != "-22" ]]; then
  echo "joinpsbts invalid-candidate path did not return -22 (got: $JOIN_INVALID_ERROR_CODE)" >&2
  exit 1
fi

FINALIZE_PSBT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"finalizepsbt-funded\",\"method\":\"finalizepsbt\",\"params\":[\"$COMBINED_PSBT\"]}" \
  | tee "$ARTIFACT_DIR/btc_finalizepsbt_funded_response.json")"
FINALIZED_PSBT_HEX="$(echo "$FINALIZE_PSBT_RESPONSE" | jq -r '.result.hex // empty')"
FINALIZED_PSBT_COMPLETE="$(echo "$FINALIZE_PSBT_RESPONSE" | jq -r '.result.complete // false')"
if [[ -z "$FINALIZED_PSBT_HEX" ]]; then
  echo "finalizepsbt returned empty hex for funded psbt" >&2
  exit 1
fi
if [[ "$(echo "$FINALIZE_PSBT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "finalizepsbt failed" >&2
  exit 1
fi
if [[ "$FINALIZED_PSBT_COMPLETE" != "true" ]]; then
  echo "finalizepsbt did not return complete=true" >&2
  exit 1
fi

CREATE_RAW_INVALID_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"create-raw-invalid-output\",\"method\":\"createrawtransaction\",\"params\":[[],{\"$SATOSHI_ADDR\":\"bad-amount\"}]}" \
  | tee "$ARTIFACT_DIR/btc_createrawtransaction_invalid_output_response.json")"
CREATE_RAW_INVALID_ERROR_CODE="$(echo "$CREATE_RAW_INVALID_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$CREATE_RAW_INVALID_ERROR_CODE" != "-32602" ]]; then
  echo "createrawtransaction invalid-output path did not return -32602 (got: $CREATE_RAW_INVALID_ERROR_CODE)" >&2
  exit 1
fi

CREATE_RAW_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"create-raw\",\"method\":\"createrawtransaction\",\"params\":[[],{\"$SATOSHI_ADDR\":$SEND_AMOUNT_RAW}]}" \
  | tee "$ARTIFACT_DIR/btc_createrawtransaction_response.json")"
RAW_INTENT_HEX="$(echo "$CREATE_RAW_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$RAW_INTENT_HEX" ]]; then
  echo "createrawtransaction returned empty result" >&2
  exit 1
fi
if [[ "$(echo "$CREATE_RAW_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "createrawtransaction failed" >&2
  exit 1
fi

SIGN_RAW_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"sign-raw\",\"method\":\"signrawtransactionwithwallet\",\"params\":[\"$RAW_INTENT_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_signrawtransactionwithwallet_response.json")"
SIGNED_RAW_HEX="$(echo "$SIGN_RAW_RESPONSE" | jq -r '.result.hex // empty')"
SIGNED_RAW_COMPLETE="$(echo "$SIGN_RAW_RESPONSE" | jq -r '.result.complete // false')"
if [[ "$(echo "$SIGN_RAW_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "signrawtransactionwithwallet failed" >&2
  exit 1
fi
if [[ -z "$SIGNED_RAW_HEX" || "$SIGNED_RAW_COMPLETE" != "true" ]]; then
  echo "signrawtransactionwithwallet did not return a complete signed payload" >&2
  exit 1
fi

INVALID_INTENT_HEX="$(jq -nc --arg addr "$SATOSHI_ADDR" '{outputs:[{address:$addr,amount:"not-a-number"}]}' | xxd -p -c 9999)"
SIGN_RAW_INVALID_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"sign-raw-invalid-intent\",\"method\":\"signrawtransactionwithwallet\",\"params\":[\"$INVALID_INTENT_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_signrawtransactionwithwallet_invalid_intent_response.json")"
SIGN_RAW_INVALID_ERROR_CODE="$(echo "$SIGN_RAW_INVALID_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$SIGN_RAW_INVALID_ERROR_CODE" != "-22" ]]; then
  echo "signrawtransactionwithwallet invalid-intent path did not return -22 (got: $SIGN_RAW_INVALID_ERROR_CODE)" >&2
  exit 1
fi

SEND_RAW_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"send-raw\",\"method\":\"sendrawtransaction\",\"params\":[\"$SIGNED_RAW_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_sendrawtransaction_response.json")"
RAW_TXID="$(echo "$SEND_RAW_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$RAW_TXID" ]]; then
  echo "sendrawtransaction returned no txid" >&2
  exit 1
fi
if [[ "$(echo "$SEND_RAW_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "sendrawtransaction failed" >&2
  exit 1
fi

SEND1_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"send1\",\"method\":\"sendtoaddress\",\"params\":[\"$SATOSHI_ADDR\",$SEND_AMOUNT_1]}" \
  | tee "$ARTIFACT_DIR/btc_sendtoaddress_1_response.json")"
TXID1="$(echo "$SEND1_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$TXID1" ]]; then
  echo "First sendtoaddress returned no txid" >&2
  exit 1
fi

SEND2_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"send2\",\"method\":\"sendtoaddress\",\"params\":[\"$SATOSHI_ADDR\",$SEND_AMOUNT_2]}" \
  | tee "$ARTIFACT_DIR/btc_sendtoaddress_2_response.json")"
TXID2="$(echo "$SEND2_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$TXID2" ]]; then
  echo "Second sendtoaddress returned no txid" >&2
  exit 1
fi

REPLAY_RAW_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"send-raw-replay\",\"method\":\"sendrawtransaction\",\"params\":[\"$SIGNED_RAW_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_sendrawtransaction_replay_response.json")"
RAW_REPLAY_ERROR="$(echo "$REPLAY_RAW_RESPONSE" | jq -r '.error.message // empty')"
RAW_REPLAY_RESULT="$(echo "$REPLAY_RAW_RESPONSE" | jq -r '.result // empty')"
if [[ -n "$RAW_REPLAY_ERROR" ]]; then
  RAW_REPLAY_MODE="rejected"
else
  if [[ "$RAW_REPLAY_RESULT" != "$RAW_TXID" ]]; then
    echo "Replay sendrawtransaction returned unexpected txid: $RAW_REPLAY_RESULT (expected $RAW_TXID)" >&2
    exit 1
  fi
  RAW_REPLAY_MODE="idempotent"
fi

LAST_RAW_HEX_CHAR="${SIGNED_RAW_HEX: -1}"
if [[ "$LAST_RAW_HEX_CHAR" == "0" ]]; then
  TAMPERED_LAST_CHAR="1"
else
  TAMPERED_LAST_CHAR="0"
fi
TAMPERED_SIGNED_RAW_HEX="${SIGNED_RAW_HEX%?}$TAMPERED_LAST_CHAR"
TAMPERED_SEND_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"send-raw-tampered\",\"method\":\"sendrawtransaction\",\"params\":[\"$TAMPERED_SIGNED_RAW_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_sendrawtransaction_tampered_response.json")"
if [[ "$(echo "$TAMPERED_SEND_RESPONSE" | jq -r '.error // empty')" == "" ]]; then
  echo "Tampered signed raw transaction unexpectedly accepted" >&2
  exit 1
fi

INSUFFICIENT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"send-insufficient\",\"method\":\"sendtoaddress\",\"params\":[\"$SATOSHI_ADDR\",$SEND_AMOUNT_TOO_HIGH]}" \
  | tee "$ARTIFACT_DIR/btc_sendtoaddress_insufficient_response.json")"
if [[ "$(echo "$INSUFFICIENT_RESPONSE" | jq -r '.error // empty')" == "" ]]; then
  echo "Insufficient-balance sendtoaddress unexpectedly succeeded" >&2
  exit 1
fi

echo "[11/12] Verifying transaction query methods..."
GETRAW_RAWTX_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getraw-rawtx\",\"method\":\"getrawtransaction\",\"params\":[\"$RAW_TXID\",false]}" \
  | tee "$ARTIFACT_DIR/btc_getrawtransaction_rawtx_response.json")"
if [[ "$(echo "$GETRAW_RAWTX_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getrawtransaction failed for raw-signed txid: $RAW_TXID" >&2
  exit 1
fi
if [[ -z "$(echo "$GETRAW_RAWTX_RESPONSE" | jq -r '.result // empty')" ]]; then
  echo "getrawtransaction returned empty hex for raw-signed transaction" >&2
  exit 1
fi

GETTX_RAWTX_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"gettx-rawtx\",\"method\":\"gettransaction\",\"params\":[\"$RAW_TXID\"]}" \
  | tee "$ARTIFACT_DIR/btc_gettransaction_rawtx_response.json")"
if [[ "$(echo "$GETTX_RAWTX_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "gettransaction failed for raw-signed txid: $RAW_TXID" >&2
  exit 1
fi
if [[ "$(echo "$GETTX_RAWTX_RESPONSE" | jq -r '.result.txid // empty')" != "$RAW_TXID" ]]; then
  echo "gettransaction returned unexpected txid for raw-signed transaction" >&2
  exit 1
fi

GETRAW_SENT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getraw-sent\",\"method\":\"getrawtransaction\",\"params\":[\"$TXID1\",true]}" \
  | tee "$ARTIFACT_DIR/btc_getrawtransaction_sent_response.json")"
if [[ "$(echo "$GETRAW_SENT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getrawtransaction failed for submitted txid: $TXID1" >&2
  exit 1
fi
if [[ "$(echo "$GETRAW_SENT_RESPONSE" | jq -r '.result.txid // empty')" != "$TXID1" ]]; then
  echo "getrawtransaction returned unexpected txid for submitted transaction" >&2
  exit 1
fi

GETTX_SENT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"gettx-sent\",\"method\":\"gettransaction\",\"params\":[\"$TXID1\"]}" \
  | tee "$ARTIFACT_DIR/btc_gettransaction_sent_response.json")"
if [[ "$(echo "$GETTX_SENT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "gettransaction failed for submitted txid: $TXID1" >&2
  exit 1
fi
if [[ "$(echo "$GETTX_SENT_RESPONSE" | jq -r '.result.txid // empty')" != "$TXID1" ]]; then
  echo "gettransaction returned unexpected txid for submitted transaction" >&2
  exit 1
fi

echo "[12/12] Verifying post-transaction balances and access key registration..."
FUNDED_BALANCE_AFTER="$FUNDED_BALANCE_BEFORE"
SATOSHI_BALANCE_AFTER="$SATOSHI_BALANCE_BEFORE"
for _ in $(seq 1 60); do
  FUNDED_BALANCE_AFTER="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"funded-after\",\"method\":\"getbalance\",\"params\":[\"$FUNDED_ADDR\"]}" \
    | tee "$ARTIFACT_DIR/btc_getbalance_funded_after_response.json" | jq -r '.result // 0')"
  SATOSHI_BALANCE_AFTER="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"satoshi-after\",\"method\":\"getbalance\",\"params\":[\"$SATOSHI_ADDR\"]}" \
    | tee "$ARTIFACT_DIR/btc_getbalance_after_response.json" | jq -r '.result // 0')"
  if float_lt "$FUNDED_BALANCE_AFTER" "$FUNDED_BALANCE_BEFORE" && float_gt "$SATOSHI_BALANCE_AFTER" "$SATOSHI_BALANCE_BEFORE"; then
    break
  fi
  sleep 1
done

if ! float_lt "$FUNDED_BALANCE_AFTER" "$FUNDED_BALANCE_BEFORE"; then
  echo "Funded sender balance did not decrease ($FUNDED_BALANCE_BEFORE -> $FUNDED_BALANCE_AFTER)" >&2
  exit 1
fi
if ! float_gt "$SATOSHI_BALANCE_AFTER" "$SATOSHI_BALANCE_BEFORE"; then
  echo "Receiver balance did not increase ($SATOSHI_BALANCE_BEFORE -> $SATOSHI_BALANCE_AFTER)" >&2
  exit 1
fi

FUNDED_DEBIT="$(awk -v before="$FUNDED_BALANCE_BEFORE" -v after="$FUNDED_BALANCE_AFTER" 'BEGIN { printf "%.8f", before - after }')"
if ! awk -v debit="$FUNDED_DEBIT" -v min="$MIN_EXPECTED_TOTAL_SENT" 'BEGIN { exit !(debit >= min) }'; then
  echo "Funded debit was below expected send total ($FUNDED_DEBIT < $MIN_EXPECTED_TOTAL_SENT)" >&2
  exit 1
fi
if ! awk -v debit="$FUNDED_DEBIT" -v max="$MAX_EXPECTED_TOTAL_DEBIT" 'BEGIN { exit !(debit <= max) }'; then
  echo "Funded debit exceeded expected max ($FUNDED_DEBIT > $MAX_EXPECTED_TOTAL_DEBIT); possible replay/double execution" >&2
  exit 1
fi

cat >"$ARTIFACT_DIR/near_access_key_list_request.json" <<JSON
{"jsonrpc":"2.0","id":"e2e","method":"query","params":{"request_type":"view_access_key_list","finality":"final","account_id":"$FUNDED_ADDR"}}
JSON
ACCESS_KEY_COUNT="$(curl -s -H 'content-type: application/json' \
  --data @"$ARTIFACT_DIR/near_access_key_list_request.json" \
  "$NEAR_RPC_URL" | tee "$ARTIFACT_DIR/near_access_key_list_response.json" | jq -r '.result.keys | length')"
if [[ "$ACCESS_KEY_COUNT" -lt 1 ]]; then
  echo "Expected at least one registered access key for funded account, found $ACCESS_KEY_COUNT" >&2
  exit 1
fi

echo "[auth] Verifying Bitcoin RPC auth behavior..."
(
  HOME="$BTCRPC_AUTH_HOME" \
  BTC_RPC_USER="$BTCRPC_AUTH_USER" \
  BTC_RPC_PASS="$BTCRPC_AUTH_PASS" \
  "$BTCRPC_BIN" \
    --near-rpc-url "$NEAR_RPC_URL" \
    --btc-rpc-addr "$BTC_RPC_AUTH_ADDR" \
    >"$ARTIFACT_DIR/btcrpc_auth.log" 2>&1 || true
) &
BTCRPC_AUTH_PID=$!

wait_for_btcrpc_auth() {
  local payload='{"jsonrpc":"2.0","id":"auth-ready","method":"getblockcount","params":[]}'
  for _ in $(seq 1 60); do
    if curl -sf -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$payload" "http://$BTC_RPC_AUTH_ADDR/" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

if ! wait_for_btcrpc_auth; then
  echo "Auth-enabled Bitcoin RPC bridge did not become ready" >&2
  exit 1
fi

AUTH_PAYLOAD='{"jsonrpc":"2.0","id":"auth-check","method":"getblockcount","params":[]}'
AUTH_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 without auth, got: $AUTH_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 with wrong auth, got: $AUTH_WRONG_CODE" >&2
  exit 1
fi

AUTH_OK_RESPONSE="$(curl -s -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/" | tee "$ARTIFACT_DIR/btc_auth_success_response.json")"
AUTH_OK_RESULT="$(echo "$AUTH_OK_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$AUTH_OK_RESULT" ]]; then
  echo "Expected successful authenticated RPC response" >&2
  exit 1
fi

cat >"$ARTIFACT_DIR/summary.txt" <<TXT
chain_id=$CHAIN_ID
near_rpc_url=$NEAR_RPC_URL
btc_rpc_addr=$BTC_RPC_ADDR
btc_rpc_auth_addr=$BTC_RPC_AUTH_ADDR
initial_height=$INITIAL_HEIGHT
later_height=$LATER_HEIGHT
height_increased=$([[ "$LATER_HEIGHT" -gt "$INITIAL_HEIGHT" ]] && echo true || echo false)
satoshi_address=$SATOSHI_ADDR
funded_address=$FUNDED_ADDR
txid_raw=$RAW_TXID
txid1=$TXID1
txid2=$TXID2
raw_replay_mode=$RAW_REPLAY_MODE
raw_replay_error=$RAW_REPLAY_ERROR
psbt_create_len=${#CREATED_PSBT}
psbt_create_object_vout_count=$OBJECT_PSBT_VOUT_COUNT
psbt_create_invalid_output_error_code=$CREATE_PSBT_INVALID_ERROR_CODE
psbt_funded_len=${#FUNDED_PSBT}
psbt_funded_object_vout_count=$FUNDED_OBJECT_PSBT_VOUT_COUNT
psbt_walletcreate_invalid_output_error_code=$WCF_PSBT_INVALID_ERROR_CODE
psbt_funded_input_txid=$FUNDED_PSBT_INPUT_TXID
psbt_walletcreate_insufficient_error_code=$WCF_PSBT_INSUFFICIENT_ERROR_CODE
psbt_analyze_next_unsigned=$ANALYZE_PSBT_NEXT
psbt_finalize_unsigned_complete=$FINALIZE_UNSIGNED_COMPLETE
psbt_finalize_unsigned_hex_len=${#FINALIZE_UNSIGNED_HEX}
psbt_utxoupdate_invalid_error_code=$UTXOUPDATE_INVALID_ERROR_CODE
psbt_signed_complete=$SIGNED_PSBT_COMPLETE
psbt_signed_sig_count=$SIGNED_PSBT_SIG_COUNT
psbt_analyze_next_signed=$ANALYZE_SIGNED_NEXT
psbt_analyze_signed_input0_final=$ANALYZE_SIGNED_IS_FINAL
psbt_join_len=${#JOINED_PSBT}
psbt_combine_mismatch_error_code=$COMBINE_MISMATCH_ERROR_CODE
psbt_join_mismatch_error_code=$JOIN_MISMATCH_ERROR_CODE
psbt_join_invalid_error_code=$JOIN_INVALID_ERROR_CODE
psbt_finalize_complete=$FINALIZED_PSBT_COMPLETE
psbt_final_hex_len=${#FINALIZED_PSBT_HEX}
createrawtransaction_invalid_output_error_code=$CREATE_RAW_INVALID_ERROR_CODE
signrawtransaction_invalid_intent_error_code=$SIGN_RAW_INVALID_ERROR_CODE
lock_txid=$LOCK_TXID
lock_vout=$LOCK_VOUT
lock_numeric_result=$LOCK_NUMERIC_RESULT
unlock_numeric_result=$UNLOCK_NUMERIC_RESULT
near_amount_yoctobit=$NEAR_AMOUNT
satoshi_balance_before=$SATOSHI_BALANCE_BEFORE
satoshi_balance_after=$SATOSHI_BALANCE_AFTER
funded_balance_before=$FUNDED_BALANCE_BEFORE
funded_balance_after=$FUNDED_BALANCE_AFTER
funded_debit=$FUNDED_DEBIT
walletcreate_while_locked_error_code=$WCF_WHILE_LOCKED_ERROR_CODE
access_key_count=$ACCESS_KEY_COUNT
getaddressinfo_invalid_error_code=$ADDRESSINFO_INVALID_ERROR_CODE
gettransaction_unknown_error_code=$GETTX_UNKNOWN_ERROR_CODE
getrawtransaction_unknown_error_code=$GETRAW_UNKNOWN_ERROR_CODE
getblockheader_unknown_error_code=$BLOCKHEADER_UNKNOWN_ERROR_CODE
scantxoutset_invalid_error_code=$SCANTXOUTSET_INVALID_ERROR_CODE
scantxoutset_empty_start_error_code=$SCANTXOUTSET_EMPTY_START_ERROR_CODE
scantxoutset_txid=$SCANTXOUTSET_TXID
listunspent_invalid_range_error_code=$LISTUNSPENT_INVALID_ERROR_CODE
listunspent_invalid_addresses_error_code=$LISTUNSPENT_INVALID_ADDRS_ERROR_CODE
lockunspent_missing_txos_error_code=$LOCK_MISSING_TXOS_ERROR_CODE
lockunspent_invalid_txid_error_code=$LOCK_INVALID_TXID_ERROR_CODE
auth_noauth_http_code=$AUTH_NOAUTH_CODE
auth_wrong_http_code=$AUTH_WRONG_CODE
auth_ok_result=$AUTH_OK_RESULT
node_log=$ARTIFACT_DIR/node.log
btcrpc_log=$ARTIFACT_DIR/btcrpc.log
btcrpc_auth_log=$ARTIFACT_DIR/btcrpc_auth.log
TXT

echo "E2E transaction flow succeeded. Artifacts written to $ARTIFACT_DIR"
