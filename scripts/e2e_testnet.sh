#!/usr/bin/env bash
set -euo pipefail
set +m

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_DIR="${ARTIFACT_DIR:-$ROOT_DIR/.context/e2e}"
WORK_DIR="$(mktemp -d /tmp/bitinfinity-e2e.XXXXXX)"
CHAIN_ID="bitinfinity-mainnet-e2e"
SATOSHI_ADDR="1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"
FUNDED_BALANCE_YOCTO="5000000000000000000000000" # 5 BIT
SEND_AMOUNT_1="1.0"
SEND_AMOUNT_2="0.25"
NEAR_RPC_URL="${NEAR_RPC_URL:-http://127.0.0.1:3030}"
BTC_RPC_ADDR="${BTC_RPC_ADDR:-127.0.0.1:18332}"

NEARD_BIN="${NEARD_BIN:-$ROOT_DIR/nearcore/target/release/neard}"
TOOLS_BIN="${TOOLS_BIN:-$ROOT_DIR/target/debug/bitinfinity-tools}"
NODE_BIN="${NODE_BIN:-$ROOT_DIR/target/debug/bitinfinity-neard}"
BTCRPC_BIN="${BTCRPC_BIN:-$ROOT_DIR/target/debug/bitinfinity-btcrpc}"
GENESIS_DIR="$WORK_DIR/genesis"
NODE_HOME="$WORK_DIR/home"
BTCRPC_HOME="$WORK_DIR/btcrpc-home"
FUNDED_KEY_JSON="$WORK_DIR/funded-keypair.json"
EXTRA_RECORDS="$WORK_DIR/extra-records.json"
BTC_RECORDS="$WORK_DIR/generated-btc-records.json"
FUNDED_RECORD="$WORK_DIR/funded-record.json"

mkdir -p "$ARTIFACT_DIR"
mkdir -p "$BTCRPC_HOME"

NODE_PID=""
BTCRPC_PID=""
cleanup() {
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

LISTUNSPENT_BEFORE_LOCK="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"listunspent-before-lock\",\"method\":\"listunspent\",\"params\":[1,9999999,[\"$FUNDED_ADDR\"]]}" \
  | tee "$ARTIFACT_DIR/btc_listunspent_before_lock_response.json")"
LOCK_TXID="$(echo "$LISTUNSPENT_BEFORE_LOCK" | jq -r '.result[0].txid // empty')"
LOCK_VOUT="$(echo "$LISTUNSPENT_BEFORE_LOCK" | jq -r '.result[0].vout // empty')"
if [[ -z "$LOCK_TXID" || -z "$LOCK_VOUT" ]]; then
  echo "listunspent did not return a lockable UTXO for funded address: $FUNDED_ADDR" >&2
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

echo "[10/12] Sending Bitcoin-signed transfers..."
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

echo "[11/12] Verifying transaction query methods..."
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

cat >"$ARTIFACT_DIR/summary.txt" <<TXT
chain_id=$CHAIN_ID
near_rpc_url=$NEAR_RPC_URL
btc_rpc_addr=$BTC_RPC_ADDR
initial_height=$INITIAL_HEIGHT
later_height=$LATER_HEIGHT
height_increased=$([[ "$LATER_HEIGHT" -gt "$INITIAL_HEIGHT" ]] && echo true || echo false)
satoshi_address=$SATOSHI_ADDR
funded_address=$FUNDED_ADDR
txid1=$TXID1
txid2=$TXID2
lock_txid=$LOCK_TXID
lock_vout=$LOCK_VOUT
near_amount_yoctobit=$NEAR_AMOUNT
satoshi_balance_before=$SATOSHI_BALANCE_BEFORE
satoshi_balance_after=$SATOSHI_BALANCE_AFTER
funded_balance_before=$FUNDED_BALANCE_BEFORE
funded_balance_after=$FUNDED_BALANCE_AFTER
access_key_count=$ACCESS_KEY_COUNT
node_log=$ARTIFACT_DIR/node.log
btcrpc_log=$ARTIFACT_DIR/btcrpc.log
TXT

echo "E2E transaction flow succeeded. Artifacts written to $ARTIFACT_DIR"
