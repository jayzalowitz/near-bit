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

echo "[1/10] Building required binaries..."
cargo build -p bitinfinity-tools -p bitinfinity-neard -p bitinfinity-btcrpc >"$ARTIFACT_DIR/build.log" 2>&1

echo "[2/10] Generating funded Bitcoin keypair..."
"$TOOLS_BIN" generate-keypair --output "$FUNDED_KEY_JSON" >"$ARTIFACT_DIR/keygen.log" 2>&1
FUNDED_ADDR="$(jq -r '.bitcoin_address // empty' "$FUNDED_KEY_JSON")"
FUNDED_WIF="$(jq -r '.private_key_wif // empty' "$FUNDED_KEY_JSON")"
if [[ -z "$FUNDED_ADDR" || -z "$FUNDED_WIF" ]]; then
  echo "Failed to extract funded keypair from $FUNDED_KEY_JSON" >&2
  exit 1
fi

echo "[3/10] Generating synthetic genesis..."
"$TOOLS_BIN" generate-genesis \
  --testnet --num-accounts 10 --chain-id "$CHAIN_ID" --output-dir "$GENESIS_DIR" \
  >"$ARTIFACT_DIR/genesis.log" 2>&1

echo "[4/10] Creating extra funded account record..."
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

echo "[5/10] Initializing node home..."
"$NODE_BIN" init \
  --home "$NODE_HOME" \
  --chain-id "$CHAIN_ID" \
  --account-id validator.bitinfinity \
  --genesis-records "$EXTRA_RECORDS" \
  --neard-bin "$NEARD_BIN" \
  >"$ARTIFACT_DIR/init.log" 2>&1

echo "[6/10] Starting bitinfinity-neard..."
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

echo "[7/10] Starting bitinfinity-btcrpc..."
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

echo "[8/10] Querying initial balances..."
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

echo "[9/10] Importing key and sending Bitcoin-signed transfers..."
IMPORT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"import\",\"method\":\"importprivkey\",\"params\":[\"$FUNDED_WIF\"]}" \
  | tee "$ARTIFACT_DIR/btc_importprivkey_response.json")"
if [[ "$(echo "$IMPORT_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "importprivkey failed" >&2
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

echo "[10/10] Verifying post-transaction balances and access key registration..."
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
