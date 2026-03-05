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
NEAR_NETWORK_PORT="${NEAR_NETWORK_PORT:-24567}"

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

for cmd in cargo curl jq lsof; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Missing required command: $cmd" >&2
    exit 1
  fi
done

extract_port_from_url() {
  local url_no_scheme="${1#*://}"
  local host_port="${url_no_scheme%%/*}"
  echo "${host_port##*:}"
}

validate_port_number() {
  local port="$1"
  [[ "$port" =~ ^[0-9]+$ ]] && ((port >= 1 && port <= 65535))
}

assert_port_available() {
  local port="$1"
  local label="$2"
  local listeners

  listeners="$(lsof -n -P -iTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)"
  if [[ -n "$listeners" ]]; then
    echo "$label port $port is already in use; stop the conflicting process or set an alternate port env var." >&2
    echo "$listeners" >&2
    exit 1
  fi
}

NEAR_RPC_PORT="$(extract_port_from_url "$NEAR_RPC_URL")"
BTC_RPC_PORT="${BTC_RPC_ADDR##*:}"
BTC_RPC_AUTH_PORT="${BTC_RPC_AUTH_ADDR##*:}"

for port_label in \
  "NEAR_RPC_PORT:$NEAR_RPC_PORT" \
  "NEAR_NETWORK_PORT:$NEAR_NETWORK_PORT" \
  "BTC_RPC_PORT:$BTC_RPC_PORT" \
  "BTC_RPC_AUTH_PORT:$BTC_RPC_AUTH_PORT"; do
  label="${port_label%%:*}"
  port="${port_label##*:}"
  if ! validate_port_number "$port"; then
    echo "Invalid $label value: $port" >&2
    exit 1
  fi
done

assert_port_available "$NEAR_RPC_PORT" "NEAR RPC"
assert_port_available "$NEAR_NETWORK_PORT" "NEAR network"
assert_port_available "$BTC_RPC_PORT" "Bitcoin RPC"
assert_port_available "$BTC_RPC_AUTH_PORT" "Bitcoin RPC auth"

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
"$NODE_BIN" run --home "$NODE_HOME" --neard-bin "$NEARD_BIN" \
  >"$ARTIFACT_DIR/node.log" 2>&1 &
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
HOME="$BTCRPC_HOME" BTC_RPC_NOAUTH=1 "$BTCRPC_BIN" \
  --near-rpc-url "$NEAR_RPC_URL" \
  --btc-rpc-addr "$BTC_RPC_ADDR" \
  >"$ARTIFACT_DIR/btcrpc.log" 2>&1 &
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

BLOCKCHAININFO_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"blockchaininfo","method":"getblockchaininfo","params":[]}' \
  | tee "$ARTIFACT_DIR/btc_getblockchaininfo_response.json")"
if [[ "$(echo "$BLOCKCHAININFO_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getblockchaininfo failed" >&2
  exit 1
fi
QUANTUM_ENFORCEMENT_ACTIVE="$(echo "$BLOCKCHAININFO_RESPONSE" | jq -r 'if (.result | has("quantum_enforcement_active")) then (.result.quantum_enforcement_active | tostring) else empty end')"
if [[ "$QUANTUM_ENFORCEMENT_ACTIVE" != "false" ]]; then
  echo "getblockchaininfo quantum_enforcement_active expected false (got: $QUANTUM_ENFORCEMENT_ACTIVE)" >&2
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

QKEY1_HEX="1111111111111111111111111111111111111111111111111111111111111111"
QKEY2_HEX="2222222222222222222222222222222222222222222222222222222222222222"
QKEY3_HEX="3333333333333333333333333333333333333333333333333333333333333333"
QKEY4_HEX="4444444444444444444444444444444444444444444444444444444444444444"
QKEY_LEGACY_ADDR="$(printf '%s' "$FUNDED_ADDR" | tr '[:upper:]' '[:lower:]')"

QKEY_LIST_INITIAL_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-list-initial\",\"method\":\"listquantumkeys\",\"params\":[\"$FUNDED_ADDR\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_list_initial_response.json")"
QKEY_INITIAL_COUNT="$(echo "$QKEY_LIST_INITIAL_RESPONSE" | jq -r '.result.quantum_keys | length')"
if [[ "$(echo "$QKEY_LIST_INITIAL_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "listquantumkeys initial query failed" >&2
  exit 1
fi

QKEY_INVALID_ADDR_ADD_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-invalid-address-add\",\"method\":\"addquantumkey\",\"params\":[\"not-a-bitcoin-address\",\"dilithium3\",\"$QKEY1_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_invalid_address_add_response.json")"
QKEY_INVALID_ADDR_ADD_ERROR_CODE="$(echo "$QKEY_INVALID_ADDR_ADD_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$QKEY_INVALID_ADDR_ADD_ERROR_CODE" != "-5" ]]; then
  echo "addquantumkey invalid-address path did not return -5 (got: $QKEY_INVALID_ADDR_ADD_ERROR_CODE)" >&2
  exit 1
fi

QKEY_INVALID_ADDR_LIST_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"quantum-invalid-address-list","method":"listquantumkeys","params":["not-a-bitcoin-address"]}' \
  | tee "$ARTIFACT_DIR/btc_quantum_invalid_address_list_response.json")"
QKEY_INVALID_ADDR_LIST_ERROR_CODE="$(echo "$QKEY_INVALID_ADDR_LIST_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$QKEY_INVALID_ADDR_LIST_ERROR_CODE" != "-5" ]]; then
  echo "listquantumkeys invalid-address path did not return -5 (got: $QKEY_INVALID_ADDR_LIST_ERROR_CODE)" >&2
  exit 1
fi

QKEY_INVALID_ADDR_REMOVE_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-invalid-address-remove\",\"method\":\"removequantumkey\",\"params\":[\"not-a-bitcoin-address\",\"dilithium3\",\"$QKEY1_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_invalid_address_remove_response.json")"
QKEY_INVALID_ADDR_REMOVE_ERROR_CODE="$(echo "$QKEY_INVALID_ADDR_REMOVE_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$QKEY_INVALID_ADDR_REMOVE_ERROR_CODE" != "-5" ]]; then
  echo "removequantumkey invalid-address path did not return -5 (got: $QKEY_INVALID_ADDR_REMOVE_ERROR_CODE)" >&2
  exit 1
fi

QKEY_ADD1_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-add-1\",\"method\":\"addquantumkey\",\"params\":[\"$QKEY_LEGACY_ADDR\",\"dilithium3\",\"$QKEY1_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_add1_response.json")"
if [[ "$(echo "$QKEY_ADD1_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "addquantumkey failed for key 1" >&2
  exit 1
fi

QKEY_LIST_AFTER_ALIAS_ADD_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-list-after-alias-add\",\"method\":\"listquantumkeys\",\"params\":[\"$FUNDED_ADDR\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_list_after_alias_add_response.json")"
QKEY_AFTER_ALIAS_ADD_COUNT="$(echo "$QKEY_LIST_AFTER_ALIAS_ADD_RESPONSE" | jq -r '.result.quantum_keys | length')"
if [[ "$(echo "$QKEY_LIST_AFTER_ALIAS_ADD_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "listquantumkeys canonical query failed after lowercase-alias add" >&2
  exit 1
fi
if [[ "$QKEY_AFTER_ALIAS_ADD_COUNT" -ne 1 ]]; then
  echo "listquantumkeys canonical query expected 1 key after lowercase-alias add (got: $QKEY_AFTER_ALIAS_ADD_COUNT)" >&2
  exit 1
fi

QKEY_DUPLICATE_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-duplicate\",\"method\":\"addquantumkey\",\"params\":[\"$FUNDED_ADDR\",\"dilithium3\",\"$QKEY1_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_duplicate_response.json")"
QKEY_DUPLICATE_ERROR_CODE="$(echo "$QKEY_DUPLICATE_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$QKEY_DUPLICATE_ERROR_CODE" != "-32602" ]]; then
  echo "addquantumkey duplicate path did not return -32602 (got: $QKEY_DUPLICATE_ERROR_CODE)" >&2
  exit 1
fi

QKEY_INVALID_TYPE_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-invalid-type\",\"method\":\"addquantumkey\",\"params\":[\"$FUNDED_ADDR\",\"not-a-type\",\"$QKEY2_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_invalid_type_response.json")"
QKEY_INVALID_TYPE_ERROR_CODE="$(echo "$QKEY_INVALID_TYPE_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$QKEY_INVALID_TYPE_ERROR_CODE" != "-32602" ]]; then
  echo "addquantumkey invalid-keytype path did not return -32602 (got: $QKEY_INVALID_TYPE_ERROR_CODE)" >&2
  exit 1
fi

QKEY_INVALID_HEX_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"quantum-invalid-hex","method":"addquantumkey","params":["'"$FUNDED_ADDR"'","falcon512","not-hex"]}' \
  | tee "$ARTIFACT_DIR/btc_quantum_invalid_hex_response.json")"
QKEY_INVALID_HEX_ERROR_CODE="$(echo "$QKEY_INVALID_HEX_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$QKEY_INVALID_HEX_ERROR_CODE" != "-32602" ]]; then
  echo "addquantumkey invalid-pubkey-hex path did not return -32602 (got: $QKEY_INVALID_HEX_ERROR_CODE)" >&2
  exit 1
fi

QKEY_ADD2_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-add-2\",\"method\":\"addquantumkey\",\"params\":[\"$FUNDED_ADDR\",\"falcon512\",\"$QKEY2_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_add2_response.json")"
if [[ "$(echo "$QKEY_ADD2_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "addquantumkey failed for key 2" >&2
  exit 1
fi

QKEY_ADD3_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-add-3\",\"method\":\"addquantumkey\",\"params\":[\"$FUNDED_ADDR\",\"sphincsplus\",\"$QKEY3_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_add3_response.json")"
if [[ "$(echo "$QKEY_ADD3_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "addquantumkey failed for key 3" >&2
  exit 1
fi

QKEY_MAX_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-max\",\"method\":\"addquantumkey\",\"params\":[\"$FUNDED_ADDR\",\"dilithium3\",\"$QKEY4_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_max_response.json")"
QKEY_MAX_ERROR_CODE="$(echo "$QKEY_MAX_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$QKEY_MAX_ERROR_CODE" != "-32602" ]]; then
  echo "addquantumkey max-keys path did not return -32602 (got: $QKEY_MAX_ERROR_CODE)" >&2
  exit 1
fi

QKEY_LIST_ADDED_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-list-added\",\"method\":\"listquantumkeys\",\"params\":[\"$FUNDED_ADDR\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_list_added_response.json")"
QKEY_ADDED_COUNT="$(echo "$QKEY_LIST_ADDED_RESPONSE" | jq -r '.result.quantum_keys | length')"
if [[ "$(echo "$QKEY_LIST_ADDED_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "listquantumkeys after add failed" >&2
  exit 1
fi
if [[ "$QKEY_ADDED_COUNT" -ne 3 ]]; then
  echo "listquantumkeys expected 3 keys after add (got: $QKEY_ADDED_COUNT)" >&2
  exit 1
fi

QKEY_LIST_ALIAS_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-list-alias\",\"method\":\"listquantumkeys\",\"params\":[\"$QKEY_LEGACY_ADDR\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_list_alias_response.json")"
QKEY_ALIAS_COUNT="$(echo "$QKEY_LIST_ALIAS_RESPONSE" | jq -r '.result.quantum_keys | length')"
if [[ "$(echo "$QKEY_LIST_ALIAS_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "listquantumkeys lowercase-alias query failed" >&2
  exit 1
fi
if [[ "$QKEY_ALIAS_COUNT" -ne "$QKEY_ADDED_COUNT" ]]; then
  echo "listquantumkeys lowercase-alias expected $QKEY_ADDED_COUNT keys (got: $QKEY_ALIAS_COUNT)" >&2
  exit 1
fi

QKEY_REMOVE_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-remove\",\"method\":\"removequantumkey\",\"params\":[\"$QKEY_LEGACY_ADDR\",\"falcon512\",\"$QKEY2_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_remove_response.json")"
if [[ "$(echo "$QKEY_REMOVE_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "removequantumkey failed for existing key" >&2
  exit 1
fi

QKEY_REMOVE_INVALID_TYPE_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-remove-invalid-type\",\"method\":\"removequantumkey\",\"params\":[\"$FUNDED_ADDR\",\"not-a-type\",\"$QKEY2_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_remove_invalid_type_response.json")"
QKEY_REMOVE_INVALID_TYPE_ERROR_CODE="$(echo "$QKEY_REMOVE_INVALID_TYPE_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$QKEY_REMOVE_INVALID_TYPE_ERROR_CODE" != "-32602" ]]; then
  echo "removequantumkey invalid-keytype path did not return -32602 (got: $QKEY_REMOVE_INVALID_TYPE_ERROR_CODE)" >&2
  exit 1
fi

QKEY_REMOVE_INVALID_HEX_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"quantum-remove-invalid-hex","method":"removequantumkey","params":["'"$FUNDED_ADDR"'","falcon512","not-hex"]}' \
  | tee "$ARTIFACT_DIR/btc_quantum_remove_invalid_hex_response.json")"
QKEY_REMOVE_INVALID_HEX_ERROR_CODE="$(echo "$QKEY_REMOVE_INVALID_HEX_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$QKEY_REMOVE_INVALID_HEX_ERROR_CODE" != "-32602" ]]; then
  echo "removequantumkey invalid-pubkey-hex path did not return -32602 (got: $QKEY_REMOVE_INVALID_HEX_ERROR_CODE)" >&2
  exit 1
fi

QKEY_REMOVE_MISSING_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-remove-missing\",\"method\":\"removequantumkey\",\"params\":[\"$FUNDED_ADDR\",\"falcon512\",\"$QKEY2_HEX\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_remove_missing_response.json")"
QKEY_REMOVE_MISSING_ERROR_CODE="$(echo "$QKEY_REMOVE_MISSING_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$QKEY_REMOVE_MISSING_ERROR_CODE" != "-32602" ]]; then
  echo "removequantumkey missing-key path did not return -32602 (got: $QKEY_REMOVE_MISSING_ERROR_CODE)" >&2
  exit 1
fi

QKEY_LIST_REMOVED_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-list-removed\",\"method\":\"listquantumkeys\",\"params\":[\"$FUNDED_ADDR\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_list_removed_response.json")"
QKEY_REMOVED_COUNT="$(echo "$QKEY_LIST_REMOVED_RESPONSE" | jq -r '.result.quantum_keys | length')"
if [[ "$(echo "$QKEY_LIST_REMOVED_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "listquantumkeys after remove failed" >&2
  exit 1
fi
if [[ "$QKEY_REMOVED_COUNT" -ne 2 ]]; then
  echo "listquantumkeys expected 2 keys after remove (got: $QKEY_REMOVED_COUNT)" >&2
  exit 1
fi

QKEY_REGISTRY_PATH="$BTCRPC_HOME/.bitinfinity/quantum_keys.json"
if [[ ! -f "$QKEY_REGISTRY_PATH" ]]; then
  echo "quantum key registry file not found at $QKEY_REGISTRY_PATH" >&2
  exit 1
fi
QKEY_REGISTRY_COUNT="$(jq -r --arg addr "$FUNDED_ADDR" '.[$addr] | if type == "array" then length else 0 end' "$QKEY_REGISTRY_PATH")"
if [[ "$QKEY_REGISTRY_COUNT" -ne 2 ]]; then
  echo "quantum key registry file expected 2 persisted keys for funded address (got: $QKEY_REGISTRY_COUNT)" >&2
  exit 1
fi
QKEY_REGISTRY_ALIAS_COUNT="$(jq -r --arg addr "$QKEY_LEGACY_ADDR" '.[$addr] | if type == "array" then length else 0 end' "$QKEY_REGISTRY_PATH")"
if [[ "$QKEY_REGISTRY_ALIAS_COUNT" -ne "$QKEY_REGISTRY_COUNT" ]]; then
  echo "quantum key registry file expected lowercase alias count to match canonical ($QKEY_REGISTRY_ALIAS_COUNT != $QKEY_REGISTRY_COUNT)" >&2
  exit 1
fi

# Restart btcrpc and ensure persisted quantum keys are reloaded.
if kill -0 "$BTCRPC_PID" 2>/dev/null; then
  kill "$BTCRPC_PID" 2>/dev/null || true
  wait "$BTCRPC_PID" 2>/dev/null || true
fi

HOME="$BTCRPC_HOME" BTC_RPC_NOAUTH=1 "$BTCRPC_BIN" \
  --near-rpc-url "$NEAR_RPC_URL" \
  --btc-rpc-addr "$BTC_RPC_ADDR" \
  >"$ARTIFACT_DIR/btcrpc.log" 2>&1 &
BTCRPC_PID=$!

if ! wait_for_btcrpc; then
  echo "Bitcoin RPC bridge did not become ready after restart" >&2
  exit 1
fi

QKEY_LIST_RESTART_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-list-restart\",\"method\":\"listquantumkeys\",\"params\":[\"$FUNDED_ADDR\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_list_restart_response.json")"
QKEY_RESTART_COUNT="$(echo "$QKEY_LIST_RESTART_RESPONSE" | jq -r '.result.quantum_keys | length')"
if [[ "$(echo "$QKEY_LIST_RESTART_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "listquantumkeys after btcrpc restart failed" >&2
  exit 1
fi
if [[ "$QKEY_RESTART_COUNT" -ne 2 ]]; then
  echo "listquantumkeys expected 2 keys after btcrpc restart (got: $QKEY_RESTART_COUNT)" >&2
  exit 1
fi

QKEY_LIST_RESTART_ALIAS_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"quantum-list-restart-alias\",\"method\":\"listquantumkeys\",\"params\":[\"$QKEY_LEGACY_ADDR\"]}" \
  | tee "$ARTIFACT_DIR/btc_quantum_list_restart_alias_response.json")"
QKEY_RESTART_ALIAS_COUNT="$(echo "$QKEY_LIST_RESTART_ALIAS_RESPONSE" | jq -r '.result.quantum_keys | length')"
if [[ "$(echo "$QKEY_LIST_RESTART_ALIAS_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "listquantumkeys lowercase-alias query failed after btcrpc restart" >&2
  exit 1
fi
if [[ "$QKEY_RESTART_ALIAS_COUNT" -ne "$QKEY_RESTART_COUNT" ]]; then
  echo "listquantumkeys lowercase-alias expected $QKEY_RESTART_COUNT keys after restart (got: $QKEY_RESTART_ALIAS_COUNT)" >&2
  exit 1
fi

BESTBLOCK_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"bestblock","method":"getbestblockhash","params":[]}' \
  | tee "$ARTIFACT_DIR/btc_getbestblockhash_response.json")"
BEST_BLOCK_HASH="$(echo "$BESTBLOCK_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$BEST_BLOCK_HASH" ]]; then
  echo "getbestblockhash returned empty hash" >&2
  exit 1
fi

GETBLOCKHASH_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getblockhash-initial\",\"method\":\"getblockhash\",\"params\":[$INITIAL_HEIGHT]}" \
  | tee "$ARTIFACT_DIR/btc_getblockhash_initial_response.json")"
BLOCK_HASH_INITIAL="$(echo "$GETBLOCKHASH_RESPONSE" | jq -r '.result // empty')"
if [[ "$(echo "$GETBLOCKHASH_RESPONSE" | jq -r '.error // empty')" != "" || -z "$BLOCK_HASH_INITIAL" ]]; then
  echo "getblockhash failed for initial height $INITIAL_HEIGHT" >&2
  exit 1
fi

GETBLOCKHASH_INVALID_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"getblockhash-invalid","method":"getblockhash","params":[999999999]}' \
  | tee "$ARTIFACT_DIR/btc_getblockhash_invalid_response.json")"
GETBLOCKHASH_INVALID_ERROR_CODE="$(echo "$GETBLOCKHASH_INVALID_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$GETBLOCKHASH_INVALID_ERROR_CODE" != "-8" ]]; then
  echo "getblockhash out-of-range path did not return -8 (got: $GETBLOCKHASH_INVALID_ERROR_CODE)" >&2
  exit 1
fi

GETBLOCK_UNKNOWN_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"getblock-unknown","method":"getblock","params":["0000000000000000000000000000000000000000000000000000000000000000",1]}' \
  | tee "$ARTIFACT_DIR/btc_getblock_unknown_response.json")"
GETBLOCK_UNKNOWN_ERROR_CODE="$(echo "$GETBLOCK_UNKNOWN_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$GETBLOCK_UNKNOWN_ERROR_CODE" != "-5" ]]; then
  echo "getblock unknown-hash path did not return -5 (got: $GETBLOCK_UNKNOWN_ERROR_CODE)" >&2
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

MININGINFO_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"mininginfo","method":"getmininginfo","params":[]}' \
  | tee "$ARTIFACT_DIR/btc_getmininginfo_response.json")"
if [[ "$(echo "$MININGINFO_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getmininginfo failed" >&2
  exit 1
fi
MININGINFO_CONSENSUS="$(echo "$MININGINFO_RESPONSE" | jq -r '.result.consensus // empty')"
MININGINFO_HASHPS="$(echo "$MININGINFO_RESPONSE" | jq -r '.result.networkhashps // empty')"
if [[ "$MININGINFO_CONSENSUS" != "proof-of-stake" ]]; then
  echo "getmininginfo consensus mismatch (got: $MININGINFO_CONSENSUS)" >&2
  exit 1
fi
if [[ "$MININGINFO_HASHPS" != "0" ]]; then
  echo "getmininginfo networkhashps expected 0 for PoS (got: $MININGINFO_HASHPS)" >&2
  exit 1
fi

GETBLOCKTEMPLATE_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"getblocktemplate-stub","method":"getblocktemplate","params":[]}' \
  | tee "$ARTIFACT_DIR/btc_getblocktemplate_stub_response.json")"
GETBLOCKTEMPLATE_ERROR_CODE="$(echo "$GETBLOCKTEMPLATE_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$GETBLOCKTEMPLATE_ERROR_CODE" != "-32601" ]]; then
  echo "getblocktemplate stub path did not return -32601 (got: $GETBLOCKTEMPLATE_ERROR_CODE)" >&2
  exit 1
fi

GENERATE_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"generate-stub","method":"generate","params":[1]}' \
  | tee "$ARTIFACT_DIR/btc_generate_stub_response.json")"
GENERATE_ERROR_CODE="$(echo "$GENERATE_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$GENERATE_ERROR_CODE" != "-32601" ]]; then
  echo "generate stub path did not return -32601 (got: $GENERATE_ERROR_CODE)" >&2
  exit 1
fi

GENERATETOADDRESS_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"generatetoaddress-stub\",\"method\":\"generatetoaddress\",\"params\":[1,\"$FUNDED_ADDR\"]}" \
  | tee "$ARTIFACT_DIR/btc_generatetoaddress_stub_response.json")"
GENERATETOADDRESS_ERROR_CODE="$(echo "$GENERATETOADDRESS_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$GENERATETOADDRESS_ERROR_CODE" != "-32601" ]]; then
  echo "generatetoaddress stub path did not return -32601 (got: $GENERATETOADDRESS_ERROR_CODE)" >&2
  exit 1
fi

GENERATETODESCRIPTOR_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"generatetodescriptor-stub\",\"method\":\"generatetodescriptor\",\"params\":[1,\"addr($FUNDED_ADDR)\"]}" \
  | tee "$ARTIFACT_DIR/btc_generatetodescriptor_stub_response.json")"
GENERATETODESCRIPTOR_ERROR_CODE="$(echo "$GENERATETODESCRIPTOR_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$GENERATETODESCRIPTOR_ERROR_CODE" != "-32601" ]]; then
  echo "generatetodescriptor stub path did not return -32601 (got: $GENERATETODESCRIPTOR_ERROR_CODE)" >&2
  exit 1
fi

ADDNODE_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"addnode-stub","method":"addnode","params":["127.0.0.1:8333","onetry"]}' \
  | tee "$ARTIFACT_DIR/btc_addnode_stub_response.json")"
ADDNODE_ERROR_CODE="$(echo "$ADDNODE_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$ADDNODE_ERROR_CODE" != "-32601" ]]; then
  echo "addnode stub path did not return -32601 (got: $ADDNODE_ERROR_CODE)" >&2
  exit 1
fi

DISCONNECTNODE_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"disconnectnode-stub","method":"disconnectnode","params":["127.0.0.1:8333"]}' \
  | tee "$ARTIFACT_DIR/btc_disconnectnode_stub_response.json")"
DISCONNECTNODE_ERROR_CODE="$(echo "$DISCONNECTNODE_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$DISCONNECTNODE_ERROR_CODE" != "-32601" ]]; then
  echo "disconnectnode stub path did not return -32601 (got: $DISCONNECTNODE_ERROR_CODE)" >&2
  exit 1
fi

ONETRY_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"onetry-stub","method":"onetry","params":["127.0.0.1:8333"]}' \
  | tee "$ARTIFACT_DIR/btc_onetry_stub_response.json")"
ONETRY_ERROR_CODE="$(echo "$ONETRY_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$ONETRY_ERROR_CODE" != "-32601" ]]; then
  echo "onetry stub path did not return -32601 (got: $ONETRY_ERROR_CODE)" >&2
  exit 1
fi

GETBLOCK_V0_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getblock-v0\",\"method\":\"getblock\",\"params\":[\"$BEST_BLOCK_HASH\",0]}" \
  | tee "$ARTIFACT_DIR/btc_getblock_v0_response.json")"
GETBLOCK_V0_HEX="$(echo "$GETBLOCK_V0_RESPONSE" | jq -r '.result // empty')"
if [[ "$(echo "$GETBLOCK_V0_RESPONSE" | jq -r '.error // empty')" != "" || -z "$GETBLOCK_V0_HEX" ]]; then
  echo "getblock verbosity=0 failed or returned empty result" >&2
  exit 1
fi

GETBLOCK_BOOL_FALSE_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getblock-bool-false\",\"method\":\"getblock\",\"params\":[\"$BEST_BLOCK_HASH\",false]}" \
  | tee "$ARTIFACT_DIR/btc_getblock_bool_false_response.json")"
GETBLOCK_BOOL_FALSE_TYPE="$(echo "$GETBLOCK_BOOL_FALSE_RESPONSE" | jq -r '.result | type // empty')"
if [[ "$(echo "$GETBLOCK_BOOL_FALSE_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getblock verbosity=false failed" >&2
  exit 1
fi
if [[ "$GETBLOCK_BOOL_FALSE_TYPE" != "string" ]]; then
  echo "getblock verbosity=false expected string result (got: $GETBLOCK_BOOL_FALSE_TYPE)" >&2
  exit 1
fi

GETBLOCK_V1_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getblock-v1\",\"method\":\"getblock\",\"params\":[\"$BEST_BLOCK_HASH\",1]}" \
  | tee "$ARTIFACT_DIR/btc_getblock_v1_response.json")"
if [[ "$(echo "$GETBLOCK_V1_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getblock verbosity=1 failed" >&2
  exit 1
fi
if [[ "$(echo "$GETBLOCK_V1_RESPONSE" | jq -r '.result.hash // empty')" != "$BEST_BLOCK_HASH" ]]; then
  echo "getblock verbosity=1 returned unexpected hash" >&2
  exit 1
fi

GETBLOCK_BOOL_TRUE_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getblock-bool-true\",\"method\":\"getblock\",\"params\":[\"$BEST_BLOCK_HASH\",true]}" \
  | tee "$ARTIFACT_DIR/btc_getblock_bool_true_response.json")"
GETBLOCK_BOOL_TRUE_HASH="$(echo "$GETBLOCK_BOOL_TRUE_RESPONSE" | jq -r '.result.hash // empty')"
if [[ "$(echo "$GETBLOCK_BOOL_TRUE_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getblock verbosity=true failed" >&2
  exit 1
fi
if [[ "$GETBLOCK_BOOL_TRUE_HASH" != "$BEST_BLOCK_HASH" ]]; then
  echo "getblock verbosity=true returned unexpected hash" >&2
  exit 1
fi

GETBLOCK_V2_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getblock-v2\",\"method\":\"getblock\",\"params\":[\"$BEST_BLOCK_HASH\",2]}" \
  | tee "$ARTIFACT_DIR/btc_getblock_v2_response.json")"
GETBLOCK_V2_TX_TYPE="$(echo "$GETBLOCK_V2_RESPONSE" | jq -r '.result.tx | type // empty')"
if [[ "$(echo "$GETBLOCK_V2_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getblock verbosity=2 failed" >&2
  exit 1
fi
if [[ "$GETBLOCK_V2_TX_TYPE" != "array" ]]; then
  echo "getblock verbosity=2 expected tx array (got: $GETBLOCK_V2_TX_TYPE)" >&2
  exit 1
fi

GETBLOCKSTATS_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getblockstats\",\"method\":\"getblockstats\",\"params\":[$INITIAL_HEIGHT]}" \
  | tee "$ARTIFACT_DIR/btc_getblockstats_response.json")"
GETBLOCKSTATS_HEIGHT="$(echo "$GETBLOCKSTATS_RESPONSE" | jq -r '.result.height // empty')"
if [[ "$(echo "$GETBLOCKSTATS_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getblockstats failed for initial height $INITIAL_HEIGHT" >&2
  exit 1
fi
if [[ "$GETBLOCKSTATS_HEIGHT" != "$INITIAL_HEIGHT" ]]; then
  echo "getblockstats returned unexpected height (got: $GETBLOCKSTATS_HEIGHT expected: $INITIAL_HEIGHT)" >&2
  exit 1
fi

GETBLOCKSTATS_INVALID_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"getblockstats-invalid","method":"getblockstats","params":[999999999]}' \
  | tee "$ARTIFACT_DIR/btc_getblockstats_invalid_response.json")"
GETBLOCKSTATS_INVALID_ERROR_CODE="$(echo "$GETBLOCKSTATS_INVALID_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$GETBLOCKSTATS_INVALID_ERROR_CODE" != "-5" ]]; then
  echo "getblockstats out-of-range path did not return -5 (got: $GETBLOCKSTATS_INVALID_ERROR_CODE)" >&2
  exit 1
fi

GETCHAINTIPS_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"getchaintips","method":"getchaintips","params":[]}' \
  | tee "$ARTIFACT_DIR/btc_getchaintips_response.json")"
GETCHAINTIPS_STATUS="$(echo "$GETCHAINTIPS_RESPONSE" | jq -r '.result[0].status // empty')"
if [[ "$(echo "$GETCHAINTIPS_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getchaintips failed" >&2
  exit 1
fi
if [[ "$GETCHAINTIPS_STATUS" != "active" ]]; then
  echo "getchaintips expected active status (got: $GETCHAINTIPS_STATUS)" >&2
  exit 1
fi

GETRAWMEMPOOL_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"getrawmempool","method":"getrawmempool","params":[false]}' \
  | tee "$ARTIFACT_DIR/btc_getrawmempool_response.json")"
GETRAWMEMPOOL_TYPE="$(echo "$GETRAWMEMPOOL_RESPONSE" | jq -r '.result | type // empty')"
if [[ "$(echo "$GETRAWMEMPOOL_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getrawmempool(verbose=false) failed" >&2
  exit 1
fi
if [[ "$GETRAWMEMPOOL_TYPE" != "array" ]]; then
  echo "getrawmempool(verbose=false) expected array result (got: $GETRAWMEMPOOL_TYPE)" >&2
  exit 1
fi

GETRAWMEMPOOL_VERBOSE_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"getrawmempool-verbose","method":"getrawmempool","params":[true]}' \
  | tee "$ARTIFACT_DIR/btc_getrawmempool_verbose_response.json")"
GETRAWMEMPOOL_VERBOSE_TYPE="$(echo "$GETRAWMEMPOOL_VERBOSE_RESPONSE" | jq -r '.result | type // empty')"
if [[ "$(echo "$GETRAWMEMPOOL_VERBOSE_RESPONSE" | jq -r '.error // empty')" != "" ]]; then
  echo "getrawmempool(verbose=true) failed" >&2
  exit 1
fi
if [[ "$GETRAWMEMPOOL_VERBOSE_TYPE" != "object" ]]; then
  echo "getrawmempool(verbose=true) expected object result (got: $GETRAWMEMPOOL_VERBOSE_TYPE)" >&2
  exit 1
fi

MEMPOOL_UNKNOWN_TXID="ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
GETMEMPOOLENTRY_UNKNOWN_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getmempoolentry-unknown\",\"method\":\"getmempoolentry\",\"params\":[\"$MEMPOOL_UNKNOWN_TXID\"]}" \
  | tee "$ARTIFACT_DIR/btc_getmempoolentry_unknown_response.json")"
GETMEMPOOLENTRY_UNKNOWN_ERROR_CODE="$(echo "$GETMEMPOOLENTRY_UNKNOWN_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$GETMEMPOOLENTRY_UNKNOWN_ERROR_CODE" != "-5" ]]; then
  echo "getmempoolentry unknown-tx path did not return -5 (got: $GETMEMPOOLENTRY_UNKNOWN_ERROR_CODE)" >&2
  exit 1
fi

GETMEMPOOLANCESTORS_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getmempoolancestors\",\"method\":\"getmempoolancestors\",\"params\":[\"$MEMPOOL_UNKNOWN_TXID\"]}" \
  | tee "$ARTIFACT_DIR/btc_getmempoolancestors_response.json")"
GETMEMPOOLANCESTORS_ERROR_CODE="$(echo "$GETMEMPOOLANCESTORS_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$GETMEMPOOLANCESTORS_ERROR_CODE" != "-5" ]]; then
  echo "getmempoolancestors unknown-tx path did not return -5 (got: $GETMEMPOOLANCESTORS_ERROR_CODE)" >&2
  exit 1
fi

GETMEMPOOLDESCENDANTS_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"getmempooldescendants\",\"method\":\"getmempooldescendants\",\"params\":[\"$MEMPOOL_UNKNOWN_TXID\"]}" \
  | tee "$ARTIFACT_DIR/btc_getmempooldescendants_response.json")"
GETMEMPOOLDESCENDANTS_ERROR_CODE="$(echo "$GETMEMPOOLDESCENDANTS_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$GETMEMPOOLDESCENDANTS_ERROR_CODE" != "-5" ]]; then
  echo "getmempooldescendants unknown-tx path did not return -5 (got: $GETMEMPOOLDESCENDANTS_ERROR_CODE)" >&2
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

CREATE_PSBT_EMPTY_DEST_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"create-psbt-empty-destination\",\"method\":\"createpsbt\",\"params\":[[{\"txid\":\"$LOCK_TXID\",\"vout\":$LOCK_VOUT}],{},0,true]}" \
  | tee "$ARTIFACT_DIR/btc_createpsbt_empty_destination_response.json")"
CREATE_PSBT_EMPTY_DEST_ERROR_CODE="$(echo "$CREATE_PSBT_EMPTY_DEST_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$CREATE_PSBT_EMPTY_DEST_ERROR_CODE" != "-32602" ]]; then
  echo "createpsbt empty-destination path did not return -32602 (got: $CREATE_PSBT_EMPTY_DEST_ERROR_CODE)" >&2
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

WCF_PSBT_EMPTY_DEST_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"walletcreatefundedpsbt-empty-destination","method":"walletcreatefundedpsbt","params":[[],{},0,{}]}' \
  | tee "$ARTIFACT_DIR/btc_walletcreatefundedpsbt_empty_destination_response.json")"
WCF_PSBT_EMPTY_DEST_ERROR_CODE="$(echo "$WCF_PSBT_EMPTY_DEST_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$WCF_PSBT_EMPTY_DEST_ERROR_CODE" != "-32602" ]]; then
  echo "walletcreatefundedpsbt empty-destination path did not return -32602 (got: $WCF_PSBT_EMPTY_DEST_ERROR_CODE)" >&2
  exit 1
fi

WCF_PSBT_INSUFFICIENT_RESPONSE="$(btc_rpc_call "{\"jsonrpc\":\"2.0\",\"id\":\"walletcreatefundedpsbt-insufficient\",\"method\":\"walletcreatefundedpsbt\",\"params\":[[],[{\"$SATOSHI_ADDR\":999999.0}],0,{}]}" \
  | tee "$ARTIFACT_DIR/btc_walletcreatefundedpsbt_insufficient_response.json")"
WCF_PSBT_INSUFFICIENT_ERROR_CODE="$(echo "$WCF_PSBT_INSUFFICIENT_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$WCF_PSBT_INSUFFICIENT_ERROR_CODE" != "-4" ]]; then
  echo "walletcreatefundedpsbt insufficient-funds path did not return -4 (got: $WCF_PSBT_INSUFFICIENT_ERROR_CODE)" >&2
  exit 1
fi

DECODE_INVALID_PSBT_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"decodepsbt-invalid","method":"decodepsbt","params":["***not-base64***"]}' \
  | tee "$ARTIFACT_DIR/btc_decodepsbt_invalid_response.json")"
DECODE_INVALID_ERROR_CODE="$(echo "$DECODE_INVALID_PSBT_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$DECODE_INVALID_ERROR_CODE" != "-22" ]]; then
  echo "decodepsbt invalid path did not return -22 (got: $DECODE_INVALID_ERROR_CODE)" >&2
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

ANALYZE_INVALID_PSBT_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"analyzepsbt-invalid","method":"analyzepsbt","params":["***not-base64***"]}' \
  | tee "$ARTIFACT_DIR/btc_analyzepsbt_invalid_response.json")"
ANALYZE_INVALID_ERROR_CODE="$(echo "$ANALYZE_INVALID_PSBT_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$ANALYZE_INVALID_ERROR_CODE" != "-22" ]]; then
  echo "analyzepsbt invalid path did not return -22 (got: $ANALYZE_INVALID_ERROR_CODE)" >&2
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

FINALIZE_INVALID_PSBT_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"finalizepsbt-invalid","method":"finalizepsbt","params":["***not-base64***"]}' \
  | tee "$ARTIFACT_DIR/btc_finalizepsbt_invalid_response.json")"
FINALIZE_INVALID_ERROR_CODE="$(echo "$FINALIZE_INVALID_PSBT_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$FINALIZE_INVALID_ERROR_CODE" != "-22" ]]; then
  echo "finalizepsbt invalid path did not return -22 (got: $FINALIZE_INVALID_ERROR_CODE)" >&2
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

WALLETPROCESS_INVALID_PSBT_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"walletprocesspsbt-invalid","method":"walletprocesspsbt","params":["***not-base64***"]}' \
  | tee "$ARTIFACT_DIR/btc_walletprocesspsbt_invalid_response.json")"
WALLETPROCESS_INVALID_ERROR_CODE="$(echo "$WALLETPROCESS_INVALID_PSBT_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$WALLETPROCESS_INVALID_ERROR_CODE" != "-22" ]]; then
  echo "walletprocesspsbt invalid path did not return -22 (got: $WALLETPROCESS_INVALID_ERROR_CODE)" >&2
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

COMBINE_INVALID_PSBT_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"combinepsbt-invalid","method":"combinepsbt","params":[["***not-base64***",""]]}' \
  | tee "$ARTIFACT_DIR/btc_combinepsbt_invalid_response.json")"
COMBINE_INVALID_ERROR_CODE="$(echo "$COMBINE_INVALID_PSBT_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$COMBINE_INVALID_ERROR_CODE" != "-22" ]]; then
  echo "combinepsbt invalid-candidate path did not return -22 (got: $COMBINE_INVALID_ERROR_CODE)" >&2
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

CREATE_RAW_EMPTY_DEST_RESPONSE="$(btc_rpc_call '{"jsonrpc":"2.0","id":"create-raw-empty-destination","method":"createrawtransaction","params":[[],{}]}' \
  | tee "$ARTIFACT_DIR/btc_createrawtransaction_empty_destination_response.json")"
CREATE_RAW_EMPTY_DEST_ERROR_CODE="$(echo "$CREATE_RAW_EMPTY_DEST_RESPONSE" | jq -r '.error.code // empty')"
if [[ "$CREATE_RAW_EMPTY_DEST_ERROR_CODE" != "-32602" ]]; then
  echo "createrawtransaction empty-destination path did not return -32602 (got: $CREATE_RAW_EMPTY_DEST_ERROR_CODE)" >&2
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
HOME="$BTCRPC_AUTH_HOME" \
BTC_RPC_USER="$BTCRPC_AUTH_USER" \
BTC_RPC_PASS="$BTCRPC_AUTH_PASS" \
"$BTCRPC_BIN" \
  --near-rpc-url "$NEAR_RPC_URL" \
  --btc-rpc-addr "$BTC_RPC_AUTH_ADDR" \
  >"$ARTIFACT_DIR/btcrpc_auth.log" 2>&1 &
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

AUTH_GETBLOCKHEADER_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-getblockheader\",\"method\":\"getblockheader\",\"params\":[\"$BEST_BLOCK_HASH\",true]}"
AUTH_GETBLOCKHEADER_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETBLOCKHEADER_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKHEADER_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getblockheader without auth, got: $AUTH_GETBLOCKHEADER_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETBLOCKHEADER_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETBLOCKHEADER_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKHEADER_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getblockheader with wrong auth, got: $AUTH_GETBLOCKHEADER_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETBLOCKHEADER_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getblockheader_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETBLOCKHEADER_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKHEADER_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getblockheader, got: $AUTH_GETBLOCKHEADER_OK_CODE" >&2
  exit 1
fi
AUTH_GETBLOCKHEADER_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getblockheader_success_response.json")"
if [[ "$AUTH_GETBLOCKHEADER_OK_ID" != "auth-getblockheader" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getblockheader" >&2
  exit 1
fi

AUTH_GETBESTBLOCKHASH_PAYLOAD='{"jsonrpc":"2.0","id":"auth-getbestblockhash","method":"getbestblockhash","params":[]}'
AUTH_GETBESTBLOCKHASH_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETBESTBLOCKHASH_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBESTBLOCKHASH_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getbestblockhash without auth, got: $AUTH_GETBESTBLOCKHASH_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETBESTBLOCKHASH_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETBESTBLOCKHASH_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBESTBLOCKHASH_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getbestblockhash with wrong auth, got: $AUTH_GETBESTBLOCKHASH_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETBESTBLOCKHASH_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getbestblockhash_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETBESTBLOCKHASH_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBESTBLOCKHASH_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getbestblockhash, got: $AUTH_GETBESTBLOCKHASH_OK_CODE" >&2
  exit 1
fi
AUTH_GETBESTBLOCKHASH_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getbestblockhash_success_response.json")"
if [[ "$AUTH_GETBESTBLOCKHASH_OK_ID" != "auth-getbestblockhash" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getbestblockhash" >&2
  exit 1
fi

AUTH_GETBLOCKHASH_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-getblockhash\",\"method\":\"getblockhash\",\"params\":[$INITIAL_HEIGHT]}"
AUTH_GETBLOCKHASH_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETBLOCKHASH_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKHASH_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getblockhash without auth, got: $AUTH_GETBLOCKHASH_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETBLOCKHASH_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETBLOCKHASH_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKHASH_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getblockhash with wrong auth, got: $AUTH_GETBLOCKHASH_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETBLOCKHASH_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getblockhash_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETBLOCKHASH_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKHASH_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getblockhash, got: $AUTH_GETBLOCKHASH_OK_CODE" >&2
  exit 1
fi
AUTH_GETBLOCKHASH_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getblockhash_success_response.json")"
if [[ "$AUTH_GETBLOCKHASH_OK_ID" != "auth-getblockhash" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getblockhash" >&2
  exit 1
fi

AUTH_GETBLOCKCHAININFO_PAYLOAD='{"jsonrpc":"2.0","id":"auth-getblockchaininfo","method":"getblockchaininfo","params":[]}'
AUTH_GETBLOCKCHAININFO_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETBLOCKCHAININFO_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKCHAININFO_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getblockchaininfo without auth, got: $AUTH_GETBLOCKCHAININFO_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETBLOCKCHAININFO_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETBLOCKCHAININFO_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKCHAININFO_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getblockchaininfo with wrong auth, got: $AUTH_GETBLOCKCHAININFO_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETBLOCKCHAININFO_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getblockchaininfo_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETBLOCKCHAININFO_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKCHAININFO_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getblockchaininfo, got: $AUTH_GETBLOCKCHAININFO_OK_CODE" >&2
  exit 1
fi
AUTH_GETBLOCKCHAININFO_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getblockchaininfo_success_response.json")"
if [[ "$AUTH_GETBLOCKCHAININFO_OK_ID" != "auth-getblockchaininfo" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getblockchaininfo" >&2
  exit 1
fi

AUTH_GETMININGINFO_PAYLOAD='{"jsonrpc":"2.0","id":"auth-getmininginfo","method":"getmininginfo","params":[]}'
AUTH_GETMININGINFO_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETMININGINFO_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETMININGINFO_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getmininginfo without auth, got: $AUTH_GETMININGINFO_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETMININGINFO_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETMININGINFO_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETMININGINFO_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getmininginfo with wrong auth, got: $AUTH_GETMININGINFO_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETMININGINFO_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getmininginfo_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETMININGINFO_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETMININGINFO_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getmininginfo, got: $AUTH_GETMININGINFO_OK_CODE" >&2
  exit 1
fi
AUTH_GETMININGINFO_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getmininginfo_success_response.json")"
if [[ "$AUTH_GETMININGINFO_OK_ID" != "auth-getmininginfo" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getmininginfo" >&2
  exit 1
fi

AUTH_GETBLOCKTEMPLATE_PAYLOAD='{"jsonrpc":"2.0","id":"auth-getblocktemplate","method":"getblocktemplate","params":[]}'
AUTH_GETBLOCKTEMPLATE_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETBLOCKTEMPLATE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKTEMPLATE_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getblocktemplate without auth, got: $AUTH_GETBLOCKTEMPLATE_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETBLOCKTEMPLATE_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETBLOCKTEMPLATE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKTEMPLATE_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getblocktemplate with wrong auth, got: $AUTH_GETBLOCKTEMPLATE_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETBLOCKTEMPLATE_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getblocktemplate_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETBLOCKTEMPLATE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKTEMPLATE_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getblocktemplate, got: $AUTH_GETBLOCKTEMPLATE_OK_CODE" >&2
  exit 1
fi
AUTH_GETBLOCKTEMPLATE_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getblocktemplate_success_response.json")"
if [[ "$AUTH_GETBLOCKTEMPLATE_OK_ID" != "auth-getblocktemplate" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getblocktemplate" >&2
  exit 1
fi

AUTH_GENERATE_PAYLOAD='{"jsonrpc":"2.0","id":"auth-generate","method":"generate","params":[1]}'
AUTH_GENERATE_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GENERATE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GENERATE_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for generate without auth, got: $AUTH_GENERATE_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GENERATE_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GENERATE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GENERATE_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for generate with wrong auth, got: $AUTH_GENERATE_WRONG_CODE" >&2
  exit 1
fi

AUTH_GENERATE_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_generate_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GENERATE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GENERATE_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated generate, got: $AUTH_GENERATE_OK_CODE" >&2
  exit 1
fi
AUTH_GENERATE_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_generate_success_response.json")"
if [[ "$AUTH_GENERATE_OK_ID" != "auth-generate" ]]; then
  echo "Expected structured JSON-RPC response for authenticated generate" >&2
  exit 1
fi

AUTH_GENERATETOADDRESS_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-generatetoaddress\",\"method\":\"generatetoaddress\",\"params\":[1,\"$FUNDED_ADDR\"]}"
AUTH_GENERATETOADDRESS_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GENERATETOADDRESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GENERATETOADDRESS_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for generatetoaddress without auth, got: $AUTH_GENERATETOADDRESS_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GENERATETOADDRESS_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GENERATETOADDRESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GENERATETOADDRESS_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for generatetoaddress with wrong auth, got: $AUTH_GENERATETOADDRESS_WRONG_CODE" >&2
  exit 1
fi

AUTH_GENERATETOADDRESS_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_generatetoaddress_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GENERATETOADDRESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GENERATETOADDRESS_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated generatetoaddress, got: $AUTH_GENERATETOADDRESS_OK_CODE" >&2
  exit 1
fi
AUTH_GENERATETOADDRESS_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_generatetoaddress_success_response.json")"
if [[ "$AUTH_GENERATETOADDRESS_OK_ID" != "auth-generatetoaddress" ]]; then
  echo "Expected structured JSON-RPC response for authenticated generatetoaddress" >&2
  exit 1
fi

AUTH_GENERATETODESCRIPTOR_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-generatetodescriptor\",\"method\":\"generatetodescriptor\",\"params\":[1,\"addr($FUNDED_ADDR)\"]}"
AUTH_GENERATETODESCRIPTOR_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GENERATETODESCRIPTOR_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GENERATETODESCRIPTOR_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for generatetodescriptor without auth, got: $AUTH_GENERATETODESCRIPTOR_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GENERATETODESCRIPTOR_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GENERATETODESCRIPTOR_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GENERATETODESCRIPTOR_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for generatetodescriptor with wrong auth, got: $AUTH_GENERATETODESCRIPTOR_WRONG_CODE" >&2
  exit 1
fi

AUTH_GENERATETODESCRIPTOR_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_generatetodescriptor_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GENERATETODESCRIPTOR_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GENERATETODESCRIPTOR_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated generatetodescriptor, got: $AUTH_GENERATETODESCRIPTOR_OK_CODE" >&2
  exit 1
fi
AUTH_GENERATETODESCRIPTOR_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_generatetodescriptor_success_response.json")"
if [[ "$AUTH_GENERATETODESCRIPTOR_OK_ID" != "auth-generatetodescriptor" ]]; then
  echo "Expected structured JSON-RPC response for authenticated generatetodescriptor" >&2
  exit 1
fi

AUTH_ADDNODE_PAYLOAD='{"jsonrpc":"2.0","id":"auth-addnode","method":"addnode","params":["127.0.0.1:8333","onetry"]}'
AUTH_ADDNODE_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_ADDNODE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_ADDNODE_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for addnode without auth, got: $AUTH_ADDNODE_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_ADDNODE_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_ADDNODE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_ADDNODE_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for addnode with wrong auth, got: $AUTH_ADDNODE_WRONG_CODE" >&2
  exit 1
fi

AUTH_ADDNODE_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_addnode_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_ADDNODE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_ADDNODE_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated addnode, got: $AUTH_ADDNODE_OK_CODE" >&2
  exit 1
fi
AUTH_ADDNODE_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_addnode_success_response.json")"
if [[ "$AUTH_ADDNODE_OK_ID" != "auth-addnode" ]]; then
  echo "Expected structured JSON-RPC response for authenticated addnode" >&2
  exit 1
fi

AUTH_DISCONNECTNODE_PAYLOAD='{"jsonrpc":"2.0","id":"auth-disconnectnode","method":"disconnectnode","params":["127.0.0.1:8333"]}'
AUTH_DISCONNECTNODE_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_DISCONNECTNODE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_DISCONNECTNODE_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for disconnectnode without auth, got: $AUTH_DISCONNECTNODE_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_DISCONNECTNODE_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_DISCONNECTNODE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_DISCONNECTNODE_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for disconnectnode with wrong auth, got: $AUTH_DISCONNECTNODE_WRONG_CODE" >&2
  exit 1
fi

AUTH_DISCONNECTNODE_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_disconnectnode_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_DISCONNECTNODE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_DISCONNECTNODE_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated disconnectnode, got: $AUTH_DISCONNECTNODE_OK_CODE" >&2
  exit 1
fi
AUTH_DISCONNECTNODE_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_disconnectnode_success_response.json")"
if [[ "$AUTH_DISCONNECTNODE_OK_ID" != "auth-disconnectnode" ]]; then
  echo "Expected structured JSON-RPC response for authenticated disconnectnode" >&2
  exit 1
fi

AUTH_ONETRY_PAYLOAD='{"jsonrpc":"2.0","id":"auth-onetry","method":"onetry","params":["127.0.0.1:8333"]}'
AUTH_ONETRY_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_ONETRY_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_ONETRY_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for onetry without auth, got: $AUTH_ONETRY_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_ONETRY_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_ONETRY_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_ONETRY_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for onetry with wrong auth, got: $AUTH_ONETRY_WRONG_CODE" >&2
  exit 1
fi

AUTH_ONETRY_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_onetry_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_ONETRY_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_ONETRY_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated onetry, got: $AUTH_ONETRY_OK_CODE" >&2
  exit 1
fi
AUTH_ONETRY_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_onetry_success_response.json")"
if [[ "$AUTH_ONETRY_OK_ID" != "auth-onetry" ]]; then
  echo "Expected structured JSON-RPC response for authenticated onetry" >&2
  exit 1
fi

AUTH_GETBLOCK_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-getblock\",\"method\":\"getblock\",\"params\":[\"$BEST_BLOCK_HASH\",1]}"
AUTH_GETBLOCK_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETBLOCK_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCK_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getblock without auth, got: $AUTH_GETBLOCK_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETBLOCK_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETBLOCK_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCK_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getblock with wrong auth, got: $AUTH_GETBLOCK_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETBLOCK_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getblock_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETBLOCK_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCK_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getblock, got: $AUTH_GETBLOCK_OK_CODE" >&2
  exit 1
fi
AUTH_GETBLOCK_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getblock_success_response.json")"
if [[ "$AUTH_GETBLOCK_OK_ID" != "auth-getblock" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getblock" >&2
  exit 1
fi

AUTH_GETBLOCKSTATS_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-getblockstats\",\"method\":\"getblockstats\",\"params\":[$INITIAL_HEIGHT]}"
AUTH_GETBLOCKSTATS_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETBLOCKSTATS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKSTATS_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getblockstats without auth, got: $AUTH_GETBLOCKSTATS_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETBLOCKSTATS_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETBLOCKSTATS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKSTATS_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getblockstats with wrong auth, got: $AUTH_GETBLOCKSTATS_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETBLOCKSTATS_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getblockstats_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETBLOCKSTATS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBLOCKSTATS_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getblockstats, got: $AUTH_GETBLOCKSTATS_OK_CODE" >&2
  exit 1
fi
AUTH_GETBLOCKSTATS_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getblockstats_success_response.json")"
if [[ "$AUTH_GETBLOCKSTATS_OK_ID" != "auth-getblockstats" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getblockstats" >&2
  exit 1
fi

AUTH_GETCHAINTIPS_PAYLOAD='{"jsonrpc":"2.0","id":"auth-getchaintips","method":"getchaintips","params":[]}'
AUTH_GETCHAINTIPS_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETCHAINTIPS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETCHAINTIPS_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getchaintips without auth, got: $AUTH_GETCHAINTIPS_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETCHAINTIPS_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETCHAINTIPS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETCHAINTIPS_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getchaintips with wrong auth, got: $AUTH_GETCHAINTIPS_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETCHAINTIPS_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getchaintips_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETCHAINTIPS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETCHAINTIPS_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getchaintips, got: $AUTH_GETCHAINTIPS_OK_CODE" >&2
  exit 1
fi
AUTH_GETCHAINTIPS_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getchaintips_success_response.json")"
if [[ "$AUTH_GETCHAINTIPS_OK_ID" != "auth-getchaintips" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getchaintips" >&2
  exit 1
fi

AUTH_GETRAWMEMPOOL_PAYLOAD='{"jsonrpc":"2.0","id":"auth-getrawmempool","method":"getrawmempool","params":[false]}'
AUTH_GETRAWMEMPOOL_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETRAWMEMPOOL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETRAWMEMPOOL_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getrawmempool without auth, got: $AUTH_GETRAWMEMPOOL_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETRAWMEMPOOL_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETRAWMEMPOOL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETRAWMEMPOOL_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getrawmempool with wrong auth, got: $AUTH_GETRAWMEMPOOL_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETRAWMEMPOOL_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getrawmempool_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETRAWMEMPOOL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETRAWMEMPOOL_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getrawmempool, got: $AUTH_GETRAWMEMPOOL_OK_CODE" >&2
  exit 1
fi
AUTH_GETRAWMEMPOOL_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getrawmempool_success_response.json")"
if [[ "$AUTH_GETRAWMEMPOOL_OK_ID" != "auth-getrawmempool" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getrawmempool" >&2
  exit 1
fi

AUTH_GETMEMPOOLENTRY_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-getmempoolentry\",\"method\":\"getmempoolentry\",\"params\":[\"$MEMPOOL_UNKNOWN_TXID\"]}"
AUTH_GETMEMPOOLENTRY_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETMEMPOOLENTRY_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETMEMPOOLENTRY_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getmempoolentry without auth, got: $AUTH_GETMEMPOOLENTRY_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETMEMPOOLENTRY_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETMEMPOOLENTRY_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETMEMPOOLENTRY_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getmempoolentry with wrong auth, got: $AUTH_GETMEMPOOLENTRY_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETMEMPOOLENTRY_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getmempoolentry_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETMEMPOOLENTRY_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETMEMPOOLENTRY_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getmempoolentry, got: $AUTH_GETMEMPOOLENTRY_OK_CODE" >&2
  exit 1
fi
AUTH_GETMEMPOOLENTRY_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getmempoolentry_success_response.json")"
if [[ "$AUTH_GETMEMPOOLENTRY_OK_ID" != "auth-getmempoolentry" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getmempoolentry" >&2
  exit 1
fi

AUTH_GETMEMPOOLANCESTORS_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-getmempoolancestors\",\"method\":\"getmempoolancestors\",\"params\":[\"$MEMPOOL_UNKNOWN_TXID\"]}"
AUTH_GETMEMPOOLANCESTORS_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETMEMPOOLANCESTORS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETMEMPOOLANCESTORS_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getmempoolancestors without auth, got: $AUTH_GETMEMPOOLANCESTORS_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETMEMPOOLANCESTORS_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETMEMPOOLANCESTORS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETMEMPOOLANCESTORS_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getmempoolancestors with wrong auth, got: $AUTH_GETMEMPOOLANCESTORS_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETMEMPOOLANCESTORS_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getmempoolancestors_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETMEMPOOLANCESTORS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETMEMPOOLANCESTORS_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getmempoolancestors, got: $AUTH_GETMEMPOOLANCESTORS_OK_CODE" >&2
  exit 1
fi
AUTH_GETMEMPOOLANCESTORS_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getmempoolancestors_success_response.json")"
if [[ "$AUTH_GETMEMPOOLANCESTORS_OK_ID" != "auth-getmempoolancestors" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getmempoolancestors" >&2
  exit 1
fi

AUTH_GETMEMPOOLDESCENDANTS_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-getmempooldescendants\",\"method\":\"getmempooldescendants\",\"params\":[\"$MEMPOOL_UNKNOWN_TXID\"]}"
AUTH_GETMEMPOOLDESCENDANTS_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETMEMPOOLDESCENDANTS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETMEMPOOLDESCENDANTS_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getmempooldescendants without auth, got: $AUTH_GETMEMPOOLDESCENDANTS_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETMEMPOOLDESCENDANTS_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETMEMPOOLDESCENDANTS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETMEMPOOLDESCENDANTS_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getmempooldescendants with wrong auth, got: $AUTH_GETMEMPOOLDESCENDANTS_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETMEMPOOLDESCENDANTS_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getmempooldescendants_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETMEMPOOLDESCENDANTS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETMEMPOOLDESCENDANTS_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getmempooldescendants, got: $AUTH_GETMEMPOOLDESCENDANTS_OK_CODE" >&2
  exit 1
fi
AUTH_GETMEMPOOLDESCENDANTS_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getmempooldescendants_success_response.json")"
if [[ "$AUTH_GETMEMPOOLDESCENDANTS_OK_ID" != "auth-getmempooldescendants" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getmempooldescendants" >&2
  exit 1
fi

AUTH_GETADDRESSINFO_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-getaddressinfo\",\"method\":\"getaddressinfo\",\"params\":[\"$FUNDED_ADDR\"]}"
AUTH_GETADDRESSINFO_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETADDRESSINFO_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETADDRESSINFO_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getaddressinfo without auth, got: $AUTH_GETADDRESSINFO_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETADDRESSINFO_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETADDRESSINFO_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETADDRESSINFO_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getaddressinfo with wrong auth, got: $AUTH_GETADDRESSINFO_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETADDRESSINFO_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getaddressinfo_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETADDRESSINFO_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETADDRESSINFO_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getaddressinfo, got: $AUTH_GETADDRESSINFO_OK_CODE" >&2
  exit 1
fi
AUTH_GETADDRESSINFO_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getaddressinfo_success_response.json")"
if [[ "$AUTH_GETADDRESSINFO_OK_ID" != "auth-getaddressinfo" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getaddressinfo" >&2
  exit 1
fi

AUTH_VALIDATEADDRESS_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-validateaddress\",\"method\":\"validateaddress\",\"params\":[\"$FUNDED_ADDR\"]}"
AUTH_VALIDATEADDRESS_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_VALIDATEADDRESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_VALIDATEADDRESS_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for validateaddress without auth, got: $AUTH_VALIDATEADDRESS_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_VALIDATEADDRESS_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_VALIDATEADDRESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_VALIDATEADDRESS_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for validateaddress with wrong auth, got: $AUTH_VALIDATEADDRESS_WRONG_CODE" >&2
  exit 1
fi

AUTH_VALIDATEADDRESS_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_validateaddress_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_VALIDATEADDRESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_VALIDATEADDRESS_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated validateaddress, got: $AUTH_VALIDATEADDRESS_OK_CODE" >&2
  exit 1
fi
AUTH_VALIDATEADDRESS_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_validateaddress_success_response.json")"
if [[ "$AUTH_VALIDATEADDRESS_OK_ID" != "auth-validateaddress" ]]; then
  echo "Expected structured JSON-RPC response for authenticated validateaddress" >&2
  exit 1
fi

AUTH_SCANTXOUTSET_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-scantxoutset\",\"method\":\"scantxoutset\",\"params\":[\"start\",[{\"desc\":\"addr($FUNDED_ADDR)\"}]]}"
AUTH_SCANTXOUTSET_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_SCANTXOUTSET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SCANTXOUTSET_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for scantxoutset without auth, got: $AUTH_SCANTXOUTSET_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_SCANTXOUTSET_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_SCANTXOUTSET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SCANTXOUTSET_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for scantxoutset with wrong auth, got: $AUTH_SCANTXOUTSET_WRONG_CODE" >&2
  exit 1
fi

AUTH_SCANTXOUTSET_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_scantxoutset_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_SCANTXOUTSET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SCANTXOUTSET_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated scantxoutset, got: $AUTH_SCANTXOUTSET_OK_CODE" >&2
  exit 1
fi
AUTH_SCANTXOUTSET_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_scantxoutset_success_response.json")"
if [[ "$AUTH_SCANTXOUTSET_OK_ID" != "auth-scantxoutset" ]]; then
  echo "Expected structured JSON-RPC response for authenticated scantxoutset" >&2
  exit 1
fi

AUTH_CREATERAWTX_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-createrawtransaction\",\"method\":\"createrawtransaction\",\"params\":[[],{\"$SATOSHI_ADDR\":0.0001}]}"
AUTH_CREATERAWTX_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_CREATERAWTX_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_CREATERAWTX_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for createrawtransaction without auth, got: $AUTH_CREATERAWTX_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_CREATERAWTX_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_CREATERAWTX_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_CREATERAWTX_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for createrawtransaction with wrong auth, got: $AUTH_CREATERAWTX_WRONG_CODE" >&2
  exit 1
fi

AUTH_CREATERAWTX_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_createrawtransaction_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_CREATERAWTX_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_CREATERAWTX_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated createrawtransaction, got: $AUTH_CREATERAWTX_OK_CODE" >&2
  exit 1
fi
AUTH_CREATERAWTX_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_createrawtransaction_success_response.json")"
if [[ "$AUTH_CREATERAWTX_OK_ID" != "auth-createrawtransaction" ]]; then
  echo "Expected structured JSON-RPC response for authenticated createrawtransaction" >&2
  exit 1
fi

AUTH_GETBALANCE_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-getbalance\",\"method\":\"getbalance\",\"params\":[\"$FUNDED_ADDR\"]}"
AUTH_GETBALANCE_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETBALANCE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBALANCE_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getbalance without auth, got: $AUTH_GETBALANCE_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETBALANCE_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETBALANCE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBALANCE_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getbalance with wrong auth, got: $AUTH_GETBALANCE_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETBALANCE_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getbalance_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETBALANCE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBALANCE_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getbalance, got: $AUTH_GETBALANCE_OK_CODE" >&2
  exit 1
fi
AUTH_GETBALANCE_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getbalance_success_response.json")"
if [[ "$AUTH_GETBALANCE_OK_ID" != "auth-getbalance" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getbalance" >&2
  exit 1
fi

AUTH_GETBALANCES_PAYLOAD='{"jsonrpc":"2.0","id":"auth-getbalances","method":"getbalances","params":[]}'
AUTH_GETBALANCES_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETBALANCES_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBALANCES_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getbalances without auth, got: $AUTH_GETBALANCES_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETBALANCES_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETBALANCES_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBALANCES_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getbalances with wrong auth, got: $AUTH_GETBALANCES_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETBALANCES_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getbalances_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETBALANCES_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETBALANCES_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getbalances, got: $AUTH_GETBALANCES_OK_CODE" >&2
  exit 1
fi
AUTH_GETBALANCES_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getbalances_success_response.json")"
if [[ "$AUTH_GETBALANCES_OK_ID" != "auth-getbalances" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getbalances" >&2
  exit 1
fi

AUTH_GETTRANSACTION_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-gettransaction\",\"method\":\"gettransaction\",\"params\":[\"$TXID1\"]}"
AUTH_GETTRANSACTION_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETTRANSACTION_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETTRANSACTION_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for gettransaction without auth, got: $AUTH_GETTRANSACTION_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETTRANSACTION_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETTRANSACTION_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETTRANSACTION_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for gettransaction with wrong auth, got: $AUTH_GETTRANSACTION_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETTRANSACTION_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_gettransaction_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETTRANSACTION_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETTRANSACTION_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated gettransaction, got: $AUTH_GETTRANSACTION_OK_CODE" >&2
  exit 1
fi
AUTH_GETTRANSACTION_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_gettransaction_success_response.json")"
if [[ "$AUTH_GETTRANSACTION_OK_ID" != "auth-gettransaction" ]]; then
  echo "Expected structured JSON-RPC response for authenticated gettransaction" >&2
  exit 1
fi

AUTH_GETRAWTRANSACTION_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-getrawtransaction\",\"method\":\"getrawtransaction\",\"params\":[\"$TXID1\",true]}"
AUTH_GETRAWTRANSACTION_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETRAWTRANSACTION_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETRAWTRANSACTION_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getrawtransaction without auth, got: $AUTH_GETRAWTRANSACTION_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETRAWTRANSACTION_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETRAWTRANSACTION_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETRAWTRANSACTION_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getrawtransaction with wrong auth, got: $AUTH_GETRAWTRANSACTION_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETRAWTRANSACTION_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getrawtransaction_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETRAWTRANSACTION_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETRAWTRANSACTION_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getrawtransaction, got: $AUTH_GETRAWTRANSACTION_OK_CODE" >&2
  exit 1
fi
AUTH_GETRAWTRANSACTION_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getrawtransaction_success_response.json")"
if [[ "$AUTH_GETRAWTRANSACTION_OK_ID" != "auth-getrawtransaction" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getrawtransaction" >&2
  exit 1
fi

AUTH_PSBT_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-psbt\",\"method\":\"createpsbt\",\"params\":[[{\"txid\":\"$LOCK_TXID\",\"vout\":$LOCK_VOUT}],[{\"$SATOSHI_ADDR\":0.0001}],0,true]}"
AUTH_PSBT_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_PSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_PSBT_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for createpsbt without auth, got: $AUTH_PSBT_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_PSBT_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_PSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_PSBT_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for createpsbt with wrong auth, got: $AUTH_PSBT_WRONG_CODE" >&2
  exit 1
fi

AUTH_PSBT_OK_RESPONSE="$(curl -s -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_PSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/" | tee "$ARTIFACT_DIR/btc_auth_createpsbt_success_response.json")"
AUTH_PSBT_OK_RESULT="$(echo "$AUTH_PSBT_OK_RESPONSE" | jq -r '.result // empty')"
if [[ -z "$AUTH_PSBT_OK_RESULT" ]]; then
  echo "Expected successful authenticated createpsbt response" >&2
  exit 1
fi

AUTH_SENDRAW_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-sendraw\",\"method\":\"sendrawtransaction\",\"params\":[\"$SIGNED_RAW_HEX\"]}"
AUTH_SENDRAW_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_SENDRAW_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SENDRAW_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for sendrawtransaction without auth, got: $AUTH_SENDRAW_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_SENDRAW_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_SENDRAW_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SENDRAW_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for sendrawtransaction with wrong auth, got: $AUTH_SENDRAW_WRONG_CODE" >&2
  exit 1
fi

AUTH_SENDRAW_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_sendrawtransaction_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_SENDRAW_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SENDRAW_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated sendrawtransaction, got: $AUTH_SENDRAW_OK_CODE" >&2
  exit 1
fi

AUTH_SIGNRAW_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-signraw\",\"method\":\"signrawtransactionwithwallet\",\"params\":[\"$RAW_INTENT_HEX\"]}"
AUTH_SIGNRAW_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_SIGNRAW_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SIGNRAW_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for signrawtransactionwithwallet without auth, got: $AUTH_SIGNRAW_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_SIGNRAW_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_SIGNRAW_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SIGNRAW_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for signrawtransactionwithwallet with wrong auth, got: $AUTH_SIGNRAW_WRONG_CODE" >&2
  exit 1
fi

AUTH_SIGNRAW_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_signrawtransactionwithwallet_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_SIGNRAW_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SIGNRAW_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated signrawtransactionwithwallet, got: $AUTH_SIGNRAW_OK_CODE" >&2
  exit 1
fi
AUTH_SIGNRAW_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_signrawtransactionwithwallet_success_response.json")"
if [[ "$AUTH_SIGNRAW_OK_ID" != "auth-signraw" ]]; then
  echo "Expected structured JSON-RPC response for authenticated signrawtransactionwithwallet" >&2
  exit 1
fi

AUTH_SENDTOADDR_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-sendtoaddress\",\"method\":\"sendtoaddress\",\"params\":[\"$SATOSHI_ADDR\",0.0001]}"
AUTH_SENDTOADDR_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_SENDTOADDR_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SENDTOADDR_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for sendtoaddress without auth, got: $AUTH_SENDTOADDR_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_SENDTOADDR_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_SENDTOADDR_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SENDTOADDR_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for sendtoaddress with wrong auth, got: $AUTH_SENDTOADDR_WRONG_CODE" >&2
  exit 1
fi

AUTH_SENDTOADDR_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_sendtoaddress_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_SENDTOADDR_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SENDTOADDR_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated sendtoaddress, got: $AUTH_SENDTOADDR_OK_CODE" >&2
  exit 1
fi
AUTH_SENDTOADDR_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_sendtoaddress_success_response.json")"
if [[ "$AUTH_SENDTOADDR_OK_ID" != "auth-sendtoaddress" ]]; then
  echo "Expected structured JSON-RPC response for authenticated sendtoaddress" >&2
  exit 1
fi

AUTH_LOCKUNSPENT_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-lockunspent\",\"method\":\"lockunspent\",\"params\":[false,[{\"txid\":\"$LOCK_TXID\",\"vout\":$LOCK_VOUT}]]}"
AUTH_LOCKUNSPENT_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_LOCKUNSPENT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LOCKUNSPENT_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for lockunspent without auth, got: $AUTH_LOCKUNSPENT_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_LOCKUNSPENT_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_LOCKUNSPENT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LOCKUNSPENT_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for lockunspent with wrong auth, got: $AUTH_LOCKUNSPENT_WRONG_CODE" >&2
  exit 1
fi

AUTH_LOCKUNSPENT_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_lockunspent_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_LOCKUNSPENT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LOCKUNSPENT_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated lockunspent, got: $AUTH_LOCKUNSPENT_OK_CODE" >&2
  exit 1
fi
AUTH_LOCKUNSPENT_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_lockunspent_success_response.json")"
if [[ "$AUTH_LOCKUNSPENT_OK_ID" != "auth-lockunspent" ]]; then
  echo "Expected structured JSON-RPC response for authenticated lockunspent" >&2
  exit 1
fi

AUTH_LISTLOCKUNSPENT_PAYLOAD='{"jsonrpc":"2.0","id":"auth-listlockunspent","method":"listlockunspent","params":[]}'
AUTH_LISTLOCKUNSPENT_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_LISTLOCKUNSPENT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTLOCKUNSPENT_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listlockunspent without auth, got: $AUTH_LISTLOCKUNSPENT_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_LISTLOCKUNSPENT_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_LISTLOCKUNSPENT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTLOCKUNSPENT_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listlockunspent with wrong auth, got: $AUTH_LISTLOCKUNSPENT_WRONG_CODE" >&2
  exit 1
fi

AUTH_LISTLOCKUNSPENT_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_listlockunspent_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_LISTLOCKUNSPENT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTLOCKUNSPENT_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated listlockunspent, got: $AUTH_LISTLOCKUNSPENT_OK_CODE" >&2
  exit 1
fi
AUTH_LISTLOCKUNSPENT_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_listlockunspent_success_response.json")"
if [[ "$AUTH_LISTLOCKUNSPENT_OK_ID" != "auth-listlockunspent" ]]; then
  echo "Expected structured JSON-RPC response for authenticated listlockunspent" >&2
  exit 1
fi

AUTH_WALLETLOCK_PAYLOAD='{"jsonrpc":"2.0","id":"auth-walletlock","method":"walletlock","params":[]}'
AUTH_WALLETLOCK_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_WALLETLOCK_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETLOCK_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for walletlock without auth, got: $AUTH_WALLETLOCK_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_WALLETLOCK_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_WALLETLOCK_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETLOCK_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for walletlock with wrong auth, got: $AUTH_WALLETLOCK_WRONG_CODE" >&2
  exit 1
fi

AUTH_WALLETLOCK_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_walletlock_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_WALLETLOCK_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETLOCK_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated walletlock, got: $AUTH_WALLETLOCK_OK_CODE" >&2
  exit 1
fi
AUTH_WALLETLOCK_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_walletlock_success_response.json")"
if [[ "$AUTH_WALLETLOCK_OK_ID" != "auth-walletlock" ]]; then
  echo "Expected structured JSON-RPC response for authenticated walletlock" >&2
  exit 1
fi

AUTH_WALLETPASSPHRASE_PAYLOAD='{"jsonrpc":"2.0","id":"auth-walletpassphrase","method":"walletpassphrase","params":["test-passphrase",10]}'
AUTH_WALLETPASSPHRASE_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_WALLETPASSPHRASE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETPASSPHRASE_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for walletpassphrase without auth, got: $AUTH_WALLETPASSPHRASE_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_WALLETPASSPHRASE_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_WALLETPASSPHRASE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETPASSPHRASE_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for walletpassphrase with wrong auth, got: $AUTH_WALLETPASSPHRASE_WRONG_CODE" >&2
  exit 1
fi

AUTH_WALLETPASSPHRASE_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_walletpassphrase_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_WALLETPASSPHRASE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETPASSPHRASE_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated walletpassphrase, got: $AUTH_WALLETPASSPHRASE_OK_CODE" >&2
  exit 1
fi
AUTH_WALLETPASSPHRASE_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_walletpassphrase_success_response.json")"
if [[ "$AUTH_WALLETPASSPHRASE_OK_ID" != "auth-walletpassphrase" ]]; then
  echo "Expected structured JSON-RPC response for authenticated walletpassphrase" >&2
  exit 1
fi

AUTH_WALLETPASSPHRASECHANGE_PAYLOAD='{"jsonrpc":"2.0","id":"auth-walletpassphrasechange","method":"walletpassphrasechange","params":["wrong-old-passphrase","new-passphrase"]}'
AUTH_WALLETPASSPHRASECHANGE_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_WALLETPASSPHRASECHANGE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETPASSPHRASECHANGE_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for walletpassphrasechange without auth, got: $AUTH_WALLETPASSPHRASECHANGE_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_WALLETPASSPHRASECHANGE_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_WALLETPASSPHRASECHANGE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETPASSPHRASECHANGE_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for walletpassphrasechange with wrong auth, got: $AUTH_WALLETPASSPHRASECHANGE_WRONG_CODE" >&2
  exit 1
fi

AUTH_WALLETPASSPHRASECHANGE_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_walletpassphrasechange_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_WALLETPASSPHRASECHANGE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETPASSPHRASECHANGE_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated walletpassphrasechange, got: $AUTH_WALLETPASSPHRASECHANGE_OK_CODE" >&2
  exit 1
fi
AUTH_WALLETPASSPHRASECHANGE_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_walletpassphrasechange_success_response.json")"
if [[ "$AUTH_WALLETPASSPHRASECHANGE_OK_ID" != "auth-walletpassphrasechange" ]]; then
  echo "Expected structured JSON-RPC response for authenticated walletpassphrasechange" >&2
  exit 1
fi

AUTH_ENCRYPTWALLET_PAYLOAD='{"jsonrpc":"2.0","id":"auth-encryptwallet","method":"encryptwallet","params":[]}'
AUTH_ENCRYPTWALLET_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_ENCRYPTWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_ENCRYPTWALLET_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for encryptwallet without auth, got: $AUTH_ENCRYPTWALLET_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_ENCRYPTWALLET_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_ENCRYPTWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_ENCRYPTWALLET_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for encryptwallet with wrong auth, got: $AUTH_ENCRYPTWALLET_WRONG_CODE" >&2
  exit 1
fi

AUTH_ENCRYPTWALLET_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_encryptwallet_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_ENCRYPTWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_ENCRYPTWALLET_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated encryptwallet, got: $AUTH_ENCRYPTWALLET_OK_CODE" >&2
  exit 1
fi
AUTH_ENCRYPTWALLET_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_encryptwallet_success_response.json")"
if [[ "$AUTH_ENCRYPTWALLET_OK_ID" != "auth-encryptwallet" ]]; then
  echo "Expected structured JSON-RPC response for authenticated encryptwallet" >&2
  exit 1
fi

AUTH_CREATEWALLET_PAYLOAD='{"jsonrpc":"2.0","id":"auth-createwallet","method":"createwallet","params":["auth-wallet"]}'
AUTH_CREATEWALLET_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_CREATEWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_CREATEWALLET_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for createwallet without auth, got: $AUTH_CREATEWALLET_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_CREATEWALLET_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_CREATEWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_CREATEWALLET_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for createwallet with wrong auth, got: $AUTH_CREATEWALLET_WRONG_CODE" >&2
  exit 1
fi

AUTH_CREATEWALLET_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_createwallet_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_CREATEWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_CREATEWALLET_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated createwallet, got: $AUTH_CREATEWALLET_OK_CODE" >&2
  exit 1
fi
AUTH_CREATEWALLET_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_createwallet_success_response.json")"
if [[ "$AUTH_CREATEWALLET_OK_ID" != "auth-createwallet" ]]; then
  echo "Expected structured JSON-RPC response for authenticated createwallet" >&2
  exit 1
fi

AUTH_LOADWALLET_PAYLOAD='{"jsonrpc":"2.0","id":"auth-loadwallet","method":"loadwallet","params":["auth-wallet"]}'
AUTH_LOADWALLET_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_LOADWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LOADWALLET_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for loadwallet without auth, got: $AUTH_LOADWALLET_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_LOADWALLET_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_LOADWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LOADWALLET_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for loadwallet with wrong auth, got: $AUTH_LOADWALLET_WRONG_CODE" >&2
  exit 1
fi

AUTH_LOADWALLET_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_loadwallet_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_LOADWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LOADWALLET_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated loadwallet, got: $AUTH_LOADWALLET_OK_CODE" >&2
  exit 1
fi
AUTH_LOADWALLET_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_loadwallet_success_response.json")"
if [[ "$AUTH_LOADWALLET_OK_ID" != "auth-loadwallet" ]]; then
  echo "Expected structured JSON-RPC response for authenticated loadwallet" >&2
  exit 1
fi

AUTH_UNLOADWALLET_PAYLOAD='{"jsonrpc":"2.0","id":"auth-unloadwallet","method":"unloadwallet","params":["auth-wallet"]}'
AUTH_UNLOADWALLET_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_UNLOADWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_UNLOADWALLET_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for unloadwallet without auth, got: $AUTH_UNLOADWALLET_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_UNLOADWALLET_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_UNLOADWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_UNLOADWALLET_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for unloadwallet with wrong auth, got: $AUTH_UNLOADWALLET_WRONG_CODE" >&2
  exit 1
fi

AUTH_UNLOADWALLET_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_unloadwallet_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_UNLOADWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_UNLOADWALLET_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated unloadwallet, got: $AUTH_UNLOADWALLET_OK_CODE" >&2
  exit 1
fi
AUTH_UNLOADWALLET_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_unloadwallet_success_response.json")"
if [[ "$AUTH_UNLOADWALLET_OK_ID" != "auth-unloadwallet" ]]; then
  echo "Expected structured JSON-RPC response for authenticated unloadwallet" >&2
  exit 1
fi

AUTH_DUMPPRIVKEY_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-dumpprivkey\",\"method\":\"dumpprivkey\",\"params\":[\"$FUNDED_ADDR\"]}"
AUTH_DUMPPRIVKEY_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_DUMPPRIVKEY_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_DUMPPRIVKEY_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for dumpprivkey without auth, got: $AUTH_DUMPPRIVKEY_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_DUMPPRIVKEY_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_DUMPPRIVKEY_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_DUMPPRIVKEY_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for dumpprivkey with wrong auth, got: $AUTH_DUMPPRIVKEY_WRONG_CODE" >&2
  exit 1
fi

AUTH_DUMPPRIVKEY_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_dumpprivkey_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_DUMPPRIVKEY_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_DUMPPRIVKEY_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated dumpprivkey, got: $AUTH_DUMPPRIVKEY_OK_CODE" >&2
  exit 1
fi
AUTH_DUMPPRIVKEY_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_dumpprivkey_success_response.json")"
if [[ "$AUTH_DUMPPRIVKEY_OK_ID" != "auth-dumpprivkey" ]]; then
  echo "Expected structured JSON-RPC response for authenticated dumpprivkey" >&2
  exit 1
fi

AUTH_IMPORTPRIVKEY_PAYLOAD='{"jsonrpc":"2.0","id":"auth-importprivkey","method":"importprivkey","params":["not-a-valid-wif","auth-label",false]}'
AUTH_IMPORTPRIVKEY_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_IMPORTPRIVKEY_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_IMPORTPRIVKEY_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for importprivkey without auth, got: $AUTH_IMPORTPRIVKEY_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_IMPORTPRIVKEY_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_IMPORTPRIVKEY_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_IMPORTPRIVKEY_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for importprivkey with wrong auth, got: $AUTH_IMPORTPRIVKEY_WRONG_CODE" >&2
  exit 1
fi

AUTH_IMPORTPRIVKEY_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_importprivkey_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_IMPORTPRIVKEY_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_IMPORTPRIVKEY_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated importprivkey, got: $AUTH_IMPORTPRIVKEY_OK_CODE" >&2
  exit 1
fi
AUTH_IMPORTPRIVKEY_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_importprivkey_success_response.json")"
if [[ "$AUTH_IMPORTPRIVKEY_OK_ID" != "auth-importprivkey" ]]; then
  echo "Expected structured JSON-RPC response for authenticated importprivkey" >&2
  exit 1
fi

AUTH_IMPORTADDRESS_PAYLOAD='{"jsonrpc":"2.0","id":"auth-importaddress","method":"importaddress","params":[]}'
AUTH_IMPORTADDRESS_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_IMPORTADDRESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_IMPORTADDRESS_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for importaddress without auth, got: $AUTH_IMPORTADDRESS_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_IMPORTADDRESS_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_IMPORTADDRESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_IMPORTADDRESS_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for importaddress with wrong auth, got: $AUTH_IMPORTADDRESS_WRONG_CODE" >&2
  exit 1
fi

AUTH_IMPORTADDRESS_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_importaddress_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_IMPORTADDRESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_IMPORTADDRESS_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated importaddress, got: $AUTH_IMPORTADDRESS_OK_CODE" >&2
  exit 1
fi
AUTH_IMPORTADDRESS_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_importaddress_success_response.json")"
if [[ "$AUTH_IMPORTADDRESS_OK_ID" != "auth-importaddress" ]]; then
  echo "Expected structured JSON-RPC response for authenticated importaddress" >&2
  exit 1
fi

AUTH_BACKUPWALLET_PAYLOAD='{"jsonrpc":"2.0","id":"auth-backupwallet","method":"backupwallet","params":[]}'
AUTH_BACKUPWALLET_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_BACKUPWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_BACKUPWALLET_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for backupwallet without auth, got: $AUTH_BACKUPWALLET_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_BACKUPWALLET_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_BACKUPWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_BACKUPWALLET_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for backupwallet with wrong auth, got: $AUTH_BACKUPWALLET_WRONG_CODE" >&2
  exit 1
fi

AUTH_BACKUPWALLET_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_backupwallet_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_BACKUPWALLET_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_BACKUPWALLET_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated backupwallet, got: $AUTH_BACKUPWALLET_OK_CODE" >&2
  exit 1
fi
AUTH_BACKUPWALLET_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_backupwallet_success_response.json")"
if [[ "$AUTH_BACKUPWALLET_OK_ID" != "auth-backupwallet" ]]; then
  echo "Expected structured JSON-RPC response for authenticated backupwallet" >&2
  exit 1
fi

AUTH_SETTXFEE_PAYLOAD='{"jsonrpc":"2.0","id":"auth-settxfee","method":"settxfee","params":[0.00001]}'
AUTH_SETTXFEE_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_SETTXFEE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SETTXFEE_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for settxfee without auth, got: $AUTH_SETTXFEE_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_SETTXFEE_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_SETTXFEE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SETTXFEE_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for settxfee with wrong auth, got: $AUTH_SETTXFEE_WRONG_CODE" >&2
  exit 1
fi

AUTH_SETTXFEE_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_settxfee_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_SETTXFEE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SETTXFEE_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated settxfee, got: $AUTH_SETTXFEE_OK_CODE" >&2
  exit 1
fi
AUTH_SETTXFEE_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_settxfee_success_response.json")"
if [[ "$AUTH_SETTXFEE_OK_ID" != "auth-settxfee" ]]; then
  echo "Expected structured JSON-RPC response for authenticated settxfee" >&2
  exit 1
fi

AUTH_KEYPOOLREFILL_PAYLOAD='{"jsonrpc":"2.0","id":"auth-keypoolrefill","method":"keypoolrefill","params":[100]}'
AUTH_KEYPOOLREFILL_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_KEYPOOLREFILL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_KEYPOOLREFILL_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for keypoolrefill without auth, got: $AUTH_KEYPOOLREFILL_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_KEYPOOLREFILL_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_KEYPOOLREFILL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_KEYPOOLREFILL_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for keypoolrefill with wrong auth, got: $AUTH_KEYPOOLREFILL_WRONG_CODE" >&2
  exit 1
fi

AUTH_KEYPOOLREFILL_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_keypoolrefill_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_KEYPOOLREFILL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_KEYPOOLREFILL_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated keypoolrefill, got: $AUTH_KEYPOOLREFILL_OK_CODE" >&2
  exit 1
fi
AUTH_KEYPOOLREFILL_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_keypoolrefill_success_response.json")"
if [[ "$AUTH_KEYPOOLREFILL_OK_ID" != "auth-keypoolrefill" ]]; then
  echo "Expected structured JSON-RPC response for authenticated keypoolrefill" >&2
  exit 1
fi

AUTH_SIGNMESSAGE_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-signmessage\",\"method\":\"signmessage\",\"params\":[\"$FUNDED_ADDR\",\"auth-check-message\"]}"
AUTH_SIGNMESSAGE_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_SIGNMESSAGE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SIGNMESSAGE_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for signmessage without auth, got: $AUTH_SIGNMESSAGE_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_SIGNMESSAGE_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_SIGNMESSAGE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SIGNMESSAGE_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for signmessage with wrong auth, got: $AUTH_SIGNMESSAGE_WRONG_CODE" >&2
  exit 1
fi

AUTH_SIGNMESSAGE_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_signmessage_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_SIGNMESSAGE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SIGNMESSAGE_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated signmessage, got: $AUTH_SIGNMESSAGE_OK_CODE" >&2
  exit 1
fi
AUTH_SIGNMESSAGE_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_signmessage_success_response.json")"
if [[ "$AUTH_SIGNMESSAGE_OK_ID" != "auth-signmessage" ]]; then
  echo "Expected structured JSON-RPC response for authenticated signmessage" >&2
  exit 1
fi

AUTH_VERIFYMESSAGE_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-verifymessage\",\"method\":\"verifymessage\",\"params\":[\"$FUNDED_ADDR\",\"bad-signature\",\"auth-check-message\"]}"
AUTH_VERIFYMESSAGE_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_VERIFYMESSAGE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_VERIFYMESSAGE_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for verifymessage without auth, got: $AUTH_VERIFYMESSAGE_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_VERIFYMESSAGE_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_VERIFYMESSAGE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_VERIFYMESSAGE_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for verifymessage with wrong auth, got: $AUTH_VERIFYMESSAGE_WRONG_CODE" >&2
  exit 1
fi

AUTH_VERIFYMESSAGE_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_verifymessage_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_VERIFYMESSAGE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_VERIFYMESSAGE_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated verifymessage, got: $AUTH_VERIFYMESSAGE_OK_CODE" >&2
  exit 1
fi
AUTH_VERIFYMESSAGE_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_verifymessage_success_response.json")"
if [[ "$AUTH_VERIFYMESSAGE_OK_ID" != "auth-verifymessage" ]]; then
  echo "Expected structured JSON-RPC response for authenticated verifymessage" >&2
  exit 1
fi

AUTH_GETNEWADDRESS_PAYLOAD='{"jsonrpc":"2.0","id":"auth-getnewaddress","method":"getnewaddress","params":[]}'
AUTH_GETNEWADDRESS_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETNEWADDRESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETNEWADDRESS_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getnewaddress without auth, got: $AUTH_GETNEWADDRESS_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETNEWADDRESS_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETNEWADDRESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETNEWADDRESS_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getnewaddress with wrong auth, got: $AUTH_GETNEWADDRESS_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETNEWADDRESS_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getnewaddress_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETNEWADDRESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETNEWADDRESS_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getnewaddress, got: $AUTH_GETNEWADDRESS_OK_CODE" >&2
  exit 1
fi
AUTH_GETNEWADDRESS_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getnewaddress_success_response.json")"
if [[ "$AUTH_GETNEWADDRESS_OK_ID" != "auth-getnewaddress" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getnewaddress" >&2
  exit 1
fi

AUTH_SETLABEL_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-setlabel\",\"method\":\"setlabel\",\"params\":[\"$FUNDED_ADDR\",\"auth-label\"]}"
AUTH_SETLABEL_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_SETLABEL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SETLABEL_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for setlabel without auth, got: $AUTH_SETLABEL_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_SETLABEL_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_SETLABEL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SETLABEL_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for setlabel with wrong auth, got: $AUTH_SETLABEL_WRONG_CODE" >&2
  exit 1
fi

AUTH_SETLABEL_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_setlabel_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_SETLABEL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_SETLABEL_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated setlabel, got: $AUTH_SETLABEL_OK_CODE" >&2
  exit 1
fi
AUTH_SETLABEL_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_setlabel_success_response.json")"
if [[ "$AUTH_SETLABEL_OK_ID" != "auth-setlabel" ]]; then
  echo "Expected structured JSON-RPC response for authenticated setlabel" >&2
  exit 1
fi

AUTH_GETRAWCHANGEADDR_PAYLOAD='{"jsonrpc":"2.0","id":"auth-getrawchangeaddress","method":"getrawchangeaddress","params":[]}'
AUTH_GETRAWCHANGEADDR_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETRAWCHANGEADDR_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETRAWCHANGEADDR_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getrawchangeaddress without auth, got: $AUTH_GETRAWCHANGEADDR_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETRAWCHANGEADDR_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETRAWCHANGEADDR_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETRAWCHANGEADDR_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getrawchangeaddress with wrong auth, got: $AUTH_GETRAWCHANGEADDR_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETRAWCHANGEADDR_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getrawchangeaddress_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETRAWCHANGEADDR_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETRAWCHANGEADDR_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getrawchangeaddress, got: $AUTH_GETRAWCHANGEADDR_OK_CODE" >&2
  exit 1
fi
AUTH_GETRAWCHANGEADDR_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getrawchangeaddress_success_response.json")"
if [[ "$AUTH_GETRAWCHANGEADDR_OK_ID" != "auth-getrawchangeaddress" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getrawchangeaddress" >&2
  exit 1
fi

AUTH_LISTLABELS_PAYLOAD='{"jsonrpc":"2.0","id":"auth-listlabels","method":"listlabels","params":[]}'
AUTH_LISTLABELS_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_LISTLABELS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTLABELS_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listlabels without auth, got: $AUTH_LISTLABELS_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_LISTLABELS_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_LISTLABELS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTLABELS_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listlabels with wrong auth, got: $AUTH_LISTLABELS_WRONG_CODE" >&2
  exit 1
fi

AUTH_LISTLABELS_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_listlabels_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_LISTLABELS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTLABELS_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated listlabels, got: $AUTH_LISTLABELS_OK_CODE" >&2
  exit 1
fi
AUTH_LISTLABELS_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_listlabels_success_response.json")"
if [[ "$AUTH_LISTLABELS_OK_ID" != "auth-listlabels" ]]; then
  echo "Expected structured JSON-RPC response for authenticated listlabels" >&2
  exit 1
fi

AUTH_GETADDRBYLABEL_PAYLOAD='{"jsonrpc":"2.0","id":"auth-getaddressesbylabel","method":"getaddressesbylabel","params":["auth-label"]}'
AUTH_GETADDRBYLABEL_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETADDRBYLABEL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETADDRBYLABEL_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getaddressesbylabel without auth, got: $AUTH_GETADDRBYLABEL_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETADDRBYLABEL_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETADDRBYLABEL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETADDRBYLABEL_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getaddressesbylabel with wrong auth, got: $AUTH_GETADDRBYLABEL_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETADDRBYLABEL_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getaddressesbylabel_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETADDRBYLABEL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETADDRBYLABEL_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getaddressesbylabel, got: $AUTH_GETADDRBYLABEL_OK_CODE" >&2
  exit 1
fi
AUTH_GETADDRBYLABEL_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getaddressesbylabel_success_response.json")"
if [[ "$AUTH_GETADDRBYLABEL_OK_ID" != "auth-getaddressesbylabel" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getaddressesbylabel" >&2
  exit 1
fi

AUTH_GETRECVBYLABEL_PAYLOAD='{"jsonrpc":"2.0","id":"auth-getreceivedbylabel","method":"getreceivedbylabel","params":["auth-label"]}'
AUTH_GETRECVBYLABEL_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETRECVBYLABEL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETRECVBYLABEL_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getreceivedbylabel without auth, got: $AUTH_GETRECVBYLABEL_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETRECVBYLABEL_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETRECVBYLABEL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETRECVBYLABEL_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getreceivedbylabel with wrong auth, got: $AUTH_GETRECVBYLABEL_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETRECVBYLABEL_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getreceivedbylabel_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETRECVBYLABEL_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETRECVBYLABEL_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getreceivedbylabel, got: $AUTH_GETRECVBYLABEL_OK_CODE" >&2
  exit 1
fi
AUTH_GETRECVBYLABEL_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getreceivedbylabel_success_response.json")"
if [[ "$AUTH_GETRECVBYLABEL_OK_ID" != "auth-getreceivedbylabel" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getreceivedbylabel" >&2
  exit 1
fi

AUTH_GETWALLETINFO_PAYLOAD='{"jsonrpc":"2.0","id":"auth-getwalletinfo","method":"getwalletinfo","params":[]}'
AUTH_GETWALLETINFO_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_GETWALLETINFO_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETWALLETINFO_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getwalletinfo without auth, got: $AUTH_GETWALLETINFO_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_GETWALLETINFO_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_GETWALLETINFO_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETWALLETINFO_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for getwalletinfo with wrong auth, got: $AUTH_GETWALLETINFO_WRONG_CODE" >&2
  exit 1
fi

AUTH_GETWALLETINFO_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_getwalletinfo_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_GETWALLETINFO_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_GETWALLETINFO_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated getwalletinfo, got: $AUTH_GETWALLETINFO_OK_CODE" >&2
  exit 1
fi
AUTH_GETWALLETINFO_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_getwalletinfo_success_response.json")"
if [[ "$AUTH_GETWALLETINFO_OK_ID" != "auth-getwalletinfo" ]]; then
  echo "Expected structured JSON-RPC response for authenticated getwalletinfo" >&2
  exit 1
fi

AUTH_LISTADDRGROUPINGS_PAYLOAD='{"jsonrpc":"2.0","id":"auth-listaddressgroupings","method":"listaddressgroupings","params":[]}'
AUTH_LISTADDRGROUPINGS_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_LISTADDRGROUPINGS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTADDRGROUPINGS_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listaddressgroupings without auth, got: $AUTH_LISTADDRGROUPINGS_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_LISTADDRGROUPINGS_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_LISTADDRGROUPINGS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTADDRGROUPINGS_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listaddressgroupings with wrong auth, got: $AUTH_LISTADDRGROUPINGS_WRONG_CODE" >&2
  exit 1
fi

AUTH_LISTADDRGROUPINGS_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_listaddressgroupings_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_LISTADDRGROUPINGS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTADDRGROUPINGS_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated listaddressgroupings, got: $AUTH_LISTADDRGROUPINGS_OK_CODE" >&2
  exit 1
fi
AUTH_LISTADDRGROUPINGS_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_listaddressgroupings_success_response.json")"
if [[ "$AUTH_LISTADDRGROUPINGS_OK_ID" != "auth-listaddressgroupings" ]]; then
  echo "Expected structured JSON-RPC response for authenticated listaddressgroupings" >&2
  exit 1
fi

AUTH_LISTRECVBYADDR_PAYLOAD='{"jsonrpc":"2.0","id":"auth-listreceivedbyaddress","method":"listreceivedbyaddress","params":[]}'
AUTH_LISTRECVBYADDR_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_LISTRECVBYADDR_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTRECVBYADDR_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listreceivedbyaddress without auth, got: $AUTH_LISTRECVBYADDR_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_LISTRECVBYADDR_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_LISTRECVBYADDR_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTRECVBYADDR_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listreceivedbyaddress with wrong auth, got: $AUTH_LISTRECVBYADDR_WRONG_CODE" >&2
  exit 1
fi

AUTH_LISTRECVBYADDR_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_listreceivedbyaddress_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_LISTRECVBYADDR_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTRECVBYADDR_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated listreceivedbyaddress, got: $AUTH_LISTRECVBYADDR_OK_CODE" >&2
  exit 1
fi
AUTH_LISTRECVBYADDR_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_listreceivedbyaddress_success_response.json")"
if [[ "$AUTH_LISTRECVBYADDR_OK_ID" != "auth-listreceivedbyaddress" ]]; then
  echo "Expected structured JSON-RPC response for authenticated listreceivedbyaddress" >&2
  exit 1
fi

AUTH_LISTUNSPENT_PAYLOAD='{"jsonrpc":"2.0","id":"auth-listunspent","method":"listunspent","params":[]}'
AUTH_LISTUNSPENT_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_LISTUNSPENT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTUNSPENT_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listunspent without auth, got: $AUTH_LISTUNSPENT_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_LISTUNSPENT_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_LISTUNSPENT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTUNSPENT_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listunspent with wrong auth, got: $AUTH_LISTUNSPENT_WRONG_CODE" >&2
  exit 1
fi

AUTH_LISTUNSPENT_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_listunspent_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_LISTUNSPENT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTUNSPENT_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated listunspent, got: $AUTH_LISTUNSPENT_OK_CODE" >&2
  exit 1
fi
AUTH_LISTUNSPENT_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_listunspent_success_response.json")"
if [[ "$AUTH_LISTUNSPENT_OK_ID" != "auth-listunspent" ]]; then
  echo "Expected structured JSON-RPC response for authenticated listunspent" >&2
  exit 1
fi

AUTH_LISTTRANSACTIONS_PAYLOAD='{"jsonrpc":"2.0","id":"auth-listtransactions","method":"listtransactions","params":[]}'
AUTH_LISTTRANSACTIONS_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_LISTTRANSACTIONS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTTRANSACTIONS_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listtransactions without auth, got: $AUTH_LISTTRANSACTIONS_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_LISTTRANSACTIONS_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_LISTTRANSACTIONS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTTRANSACTIONS_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listtransactions with wrong auth, got: $AUTH_LISTTRANSACTIONS_WRONG_CODE" >&2
  exit 1
fi

AUTH_LISTTRANSACTIONS_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_listtransactions_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_LISTTRANSACTIONS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTTRANSACTIONS_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated listtransactions, got: $AUTH_LISTTRANSACTIONS_OK_CODE" >&2
  exit 1
fi
AUTH_LISTTRANSACTIONS_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_listtransactions_success_response.json")"
if [[ "$AUTH_LISTTRANSACTIONS_OK_ID" != "auth-listtransactions" ]]; then
  echo "Expected structured JSON-RPC response for authenticated listtransactions" >&2
  exit 1
fi

AUTH_LISTSINCEBLOCK_PAYLOAD='{"jsonrpc":"2.0","id":"auth-listsinceblock","method":"listsinceblock","params":[]}'
AUTH_LISTSINCEBLOCK_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_LISTSINCEBLOCK_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTSINCEBLOCK_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listsinceblock without auth, got: $AUTH_LISTSINCEBLOCK_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_LISTSINCEBLOCK_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_LISTSINCEBLOCK_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTSINCEBLOCK_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listsinceblock with wrong auth, got: $AUTH_LISTSINCEBLOCK_WRONG_CODE" >&2
  exit 1
fi

AUTH_LISTSINCEBLOCK_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_listsinceblock_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_LISTSINCEBLOCK_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTSINCEBLOCK_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated listsinceblock, got: $AUTH_LISTSINCEBLOCK_OK_CODE" >&2
  exit 1
fi
AUTH_LISTSINCEBLOCK_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_listsinceblock_success_response.json")"
if [[ "$AUTH_LISTSINCEBLOCK_OK_ID" != "auth-listsinceblock" ]]; then
  echo "Expected structured JSON-RPC response for authenticated listsinceblock" >&2
  exit 1
fi

AUTH_WALLETPROCESS_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-walletprocesspsbt\",\"method\":\"walletprocesspsbt\",\"params\":[\"$FUNDED_PSBT\"]}"
AUTH_WALLETPROCESS_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_WALLETPROCESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETPROCESS_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for walletprocesspsbt without auth, got: $AUTH_WALLETPROCESS_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_WALLETPROCESS_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_WALLETPROCESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETPROCESS_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for walletprocesspsbt with wrong auth, got: $AUTH_WALLETPROCESS_WRONG_CODE" >&2
  exit 1
fi

AUTH_WALLETPROCESS_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_walletprocesspsbt_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_WALLETPROCESS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETPROCESS_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated walletprocesspsbt, got: $AUTH_WALLETPROCESS_OK_CODE" >&2
  exit 1
fi
AUTH_WALLETPROCESS_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_walletprocesspsbt_success_response.json")"
if [[ "$AUTH_WALLETPROCESS_OK_ID" != "auth-walletprocesspsbt" ]]; then
  echo "Expected structured JSON-RPC response for authenticated walletprocesspsbt" >&2
  exit 1
fi

AUTH_DECODEPSBT_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-decodepsbt\",\"method\":\"decodepsbt\",\"params\":[\"$FUNDED_PSBT\"]}"
AUTH_DECODEPSBT_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_DECODEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_DECODEPSBT_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for decodepsbt without auth, got: $AUTH_DECODEPSBT_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_DECODEPSBT_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_DECODEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_DECODEPSBT_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for decodepsbt with wrong auth, got: $AUTH_DECODEPSBT_WRONG_CODE" >&2
  exit 1
fi

AUTH_DECODEPSBT_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_decodepsbt_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_DECODEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_DECODEPSBT_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated decodepsbt, got: $AUTH_DECODEPSBT_OK_CODE" >&2
  exit 1
fi
AUTH_DECODEPSBT_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_decodepsbt_success_response.json")"
if [[ "$AUTH_DECODEPSBT_OK_ID" != "auth-decodepsbt" ]]; then
  echo "Expected structured JSON-RPC response for authenticated decodepsbt" >&2
  exit 1
fi

AUTH_ANALYZEPSBT_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-analyzepsbt\",\"method\":\"analyzepsbt\",\"params\":[\"$FUNDED_PSBT\"]}"
AUTH_ANALYZEPSBT_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_ANALYZEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_ANALYZEPSBT_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for analyzepsbt without auth, got: $AUTH_ANALYZEPSBT_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_ANALYZEPSBT_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_ANALYZEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_ANALYZEPSBT_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for analyzepsbt with wrong auth, got: $AUTH_ANALYZEPSBT_WRONG_CODE" >&2
  exit 1
fi

AUTH_ANALYZEPSBT_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_analyzepsbt_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_ANALYZEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_ANALYZEPSBT_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated analyzepsbt, got: $AUTH_ANALYZEPSBT_OK_CODE" >&2
  exit 1
fi
AUTH_ANALYZEPSBT_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_analyzepsbt_success_response.json")"
if [[ "$AUTH_ANALYZEPSBT_OK_ID" != "auth-analyzepsbt" ]]; then
  echo "Expected structured JSON-RPC response for authenticated analyzepsbt" >&2
  exit 1
fi

AUTH_FINALIZEPSBT_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-finalizepsbt\",\"method\":\"finalizepsbt\",\"params\":[\"$FUNDED_PSBT\"]}"
AUTH_FINALIZEPSBT_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_FINALIZEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_FINALIZEPSBT_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for finalizepsbt without auth, got: $AUTH_FINALIZEPSBT_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_FINALIZEPSBT_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_FINALIZEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_FINALIZEPSBT_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for finalizepsbt with wrong auth, got: $AUTH_FINALIZEPSBT_WRONG_CODE" >&2
  exit 1
fi

AUTH_FINALIZEPSBT_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_finalizepsbt_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_FINALIZEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_FINALIZEPSBT_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated finalizepsbt, got: $AUTH_FINALIZEPSBT_OK_CODE" >&2
  exit 1
fi
AUTH_FINALIZEPSBT_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_finalizepsbt_success_response.json")"
if [[ "$AUTH_FINALIZEPSBT_OK_ID" != "auth-finalizepsbt" ]]; then
  echo "Expected structured JSON-RPC response for authenticated finalizepsbt" >&2
  exit 1
fi

AUTH_UTXOUPDATEPSBT_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-utxoupdatepsbt\",\"method\":\"utxoupdatepsbt\",\"params\":[\"$FUNDED_PSBT\"]}"
AUTH_UTXOUPDATEPSBT_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_UTXOUPDATEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_UTXOUPDATEPSBT_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for utxoupdatepsbt without auth, got: $AUTH_UTXOUPDATEPSBT_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_UTXOUPDATEPSBT_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_UTXOUPDATEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_UTXOUPDATEPSBT_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for utxoupdatepsbt with wrong auth, got: $AUTH_UTXOUPDATEPSBT_WRONG_CODE" >&2
  exit 1
fi

AUTH_UTXOUPDATEPSBT_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_utxoupdatepsbt_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_UTXOUPDATEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_UTXOUPDATEPSBT_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated utxoupdatepsbt, got: $AUTH_UTXOUPDATEPSBT_OK_CODE" >&2
  exit 1
fi
AUTH_UTXOUPDATEPSBT_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_utxoupdatepsbt_success_response.json")"
if [[ "$AUTH_UTXOUPDATEPSBT_OK_ID" != "auth-utxoupdatepsbt" ]]; then
  echo "Expected structured JSON-RPC response for authenticated utxoupdatepsbt" >&2
  exit 1
fi

AUTH_COMBINEPSBT_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-combinepsbt\",\"method\":\"combinepsbt\",\"params\":[[\"$FUNDED_PSBT\",\"$SIGNED_PSBT\"]]}"
AUTH_COMBINEPSBT_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_COMBINEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_COMBINEPSBT_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for combinepsbt without auth, got: $AUTH_COMBINEPSBT_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_COMBINEPSBT_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_COMBINEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_COMBINEPSBT_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for combinepsbt with wrong auth, got: $AUTH_COMBINEPSBT_WRONG_CODE" >&2
  exit 1
fi

AUTH_COMBINEPSBT_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_combinepsbt_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_COMBINEPSBT_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_COMBINEPSBT_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated combinepsbt, got: $AUTH_COMBINEPSBT_OK_CODE" >&2
  exit 1
fi
AUTH_COMBINEPSBT_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_combinepsbt_success_response.json")"
if [[ "$AUTH_COMBINEPSBT_OK_ID" != "auth-combinepsbt" ]]; then
  echo "Expected structured JSON-RPC response for authenticated combinepsbt" >&2
  exit 1
fi

AUTH_JOINPSBTS_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-joinpsbts\",\"method\":\"joinpsbts\",\"params\":[[\"$FUNDED_PSBT\",\"$SIGNED_PSBT\"]]}"
AUTH_JOINPSBTS_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_JOINPSBTS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_JOINPSBTS_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for joinpsbts without auth, got: $AUTH_JOINPSBTS_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_JOINPSBTS_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_JOINPSBTS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_JOINPSBTS_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for joinpsbts with wrong auth, got: $AUTH_JOINPSBTS_WRONG_CODE" >&2
  exit 1
fi

AUTH_JOINPSBTS_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_joinpsbts_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_JOINPSBTS_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_JOINPSBTS_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated joinpsbts, got: $AUTH_JOINPSBTS_OK_CODE" >&2
  exit 1
fi
AUTH_JOINPSBTS_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_joinpsbts_success_response.json")"
if [[ "$AUTH_JOINPSBTS_OK_ID" != "auth-joinpsbts" ]]; then
  echo "Expected structured JSON-RPC response for authenticated joinpsbts" >&2
  exit 1
fi

AUTH_WALLETCREATE_PAYLOAD="{\"jsonrpc\":\"2.0\",\"id\":\"auth-walletcreatefundedpsbt\",\"method\":\"walletcreatefundedpsbt\",\"params\":[[],[{\"$SATOSHI_ADDR\":0.0001}],0,{}]}"
AUTH_WALLETCREATE_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_WALLETCREATE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETCREATE_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for walletcreatefundedpsbt without auth, got: $AUTH_WALLETCREATE_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_WALLETCREATE_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_WALLETCREATE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETCREATE_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for walletcreatefundedpsbt with wrong auth, got: $AUTH_WALLETCREATE_WRONG_CODE" >&2
  exit 1
fi

AUTH_WALLETCREATE_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_walletcreatefundedpsbt_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_WALLETCREATE_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_WALLETCREATE_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated walletcreatefundedpsbt, got: $AUTH_WALLETCREATE_OK_CODE" >&2
  exit 1
fi
AUTH_WALLETCREATE_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_walletcreatefundedpsbt_success_response.json")"
if [[ "$AUTH_WALLETCREATE_OK_ID" != "auth-walletcreatefundedpsbt" ]]; then
  echo "Expected structured JSON-RPC response for authenticated walletcreatefundedpsbt" >&2
  exit 1
fi

AUTH_QUANTUM_PAYLOAD='{"jsonrpc":"2.0","id":"auth-addquantumkey","method":"addquantumkey","params":["'"$FUNDED_ADDR"'","dilithium3","aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}'
AUTH_QUANTUM_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_QUANTUM_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_QUANTUM_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for addquantumkey without auth, got: $AUTH_QUANTUM_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_QUANTUM_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_QUANTUM_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_QUANTUM_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for addquantumkey with wrong auth, got: $AUTH_QUANTUM_WRONG_CODE" >&2
  exit 1
fi

AUTH_QUANTUM_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_addquantumkey_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_QUANTUM_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_QUANTUM_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated addquantumkey, got: $AUTH_QUANTUM_OK_CODE" >&2
  exit 1
fi
AUTH_QUANTUM_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_addquantumkey_success_response.json")"
if [[ "$AUTH_QUANTUM_OK_ID" != "auth-addquantumkey" ]]; then
  echo "Expected structured JSON-RPC response for authenticated addquantumkey" >&2
  exit 1
fi

AUTH_REMOVEQUANTUM_PAYLOAD='{"jsonrpc":"2.0","id":"auth-removequantumkey","method":"removequantumkey","params":["'"$FUNDED_ADDR"'","falcon512","ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"]}'
AUTH_REMOVEQUANTUM_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_REMOVEQUANTUM_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_REMOVEQUANTUM_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for removequantumkey without auth, got: $AUTH_REMOVEQUANTUM_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_REMOVEQUANTUM_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_REMOVEQUANTUM_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_REMOVEQUANTUM_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for removequantumkey with wrong auth, got: $AUTH_REMOVEQUANTUM_WRONG_CODE" >&2
  exit 1
fi

AUTH_REMOVEQUANTUM_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_removequantumkey_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_REMOVEQUANTUM_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_REMOVEQUANTUM_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated removequantumkey, got: $AUTH_REMOVEQUANTUM_OK_CODE" >&2
  exit 1
fi
AUTH_REMOVEQUANTUM_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_removequantumkey_success_response.json")"
if [[ "$AUTH_REMOVEQUANTUM_OK_ID" != "auth-removequantumkey" ]]; then
  echo "Expected structured JSON-RPC response for authenticated removequantumkey" >&2
  exit 1
fi

AUTH_LISTQUANTUM_PAYLOAD='{"jsonrpc":"2.0","id":"auth-listquantumkeys","method":"listquantumkeys","params":["'"$FUNDED_ADDR"'"]}'
AUTH_LISTQUANTUM_NOAUTH_CODE="$(curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data "$AUTH_LISTQUANTUM_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTQUANTUM_NOAUTH_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listquantumkeys without auth, got: $AUTH_LISTQUANTUM_NOAUTH_CODE" >&2
  exit 1
fi

AUTH_LISTQUANTUM_WRONG_CODE="$(curl -s -o /dev/null -w '%{http_code}' -u "wrong:creds" -H 'content-type: application/json' --data "$AUTH_LISTQUANTUM_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTQUANTUM_WRONG_CODE" != "401" ]]; then
  echo "Expected HTTP 401 for listquantumkeys with wrong auth, got: $AUTH_LISTQUANTUM_WRONG_CODE" >&2
  exit 1
fi

AUTH_LISTQUANTUM_OK_CODE="$(curl -s -o "$ARTIFACT_DIR/btc_auth_listquantumkeys_success_response.json" -w '%{http_code}' -u "$BTCRPC_AUTH_USER:$BTCRPC_AUTH_PASS" -H 'content-type: application/json' --data "$AUTH_LISTQUANTUM_PAYLOAD" "http://$BTC_RPC_AUTH_ADDR/")"
if [[ "$AUTH_LISTQUANTUM_OK_CODE" != "200" ]]; then
  echo "Expected HTTP 200 for authenticated listquantumkeys, got: $AUTH_LISTQUANTUM_OK_CODE" >&2
  exit 1
fi
AUTH_LISTQUANTUM_OK_ID="$(jq -r '.id // empty' "$ARTIFACT_DIR/btc_auth_listquantumkeys_success_response.json")"
if [[ "$AUTH_LISTQUANTUM_OK_ID" != "auth-listquantumkeys" ]]; then
  echo "Expected structured JSON-RPC response for authenticated listquantumkeys" >&2
  exit 1
fi

cat >"$ARTIFACT_DIR/summary.txt" <<TXT
chain_id=$CHAIN_ID
near_rpc_url=$NEAR_RPC_URL
btc_rpc_addr=$BTC_RPC_ADDR
btc_rpc_auth_addr=$BTC_RPC_AUTH_ADDR
quantum_enforcement_active=$QUANTUM_ENFORCEMENT_ACTIVE
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
psbt_create_empty_destination_error_code=$CREATE_PSBT_EMPTY_DEST_ERROR_CODE
psbt_funded_len=${#FUNDED_PSBT}
psbt_funded_object_vout_count=$FUNDED_OBJECT_PSBT_VOUT_COUNT
psbt_walletcreate_invalid_output_error_code=$WCF_PSBT_INVALID_ERROR_CODE
psbt_walletcreate_empty_destination_error_code=$WCF_PSBT_EMPTY_DEST_ERROR_CODE
psbt_funded_input_txid=$FUNDED_PSBT_INPUT_TXID
psbt_walletcreate_insufficient_error_code=$WCF_PSBT_INSUFFICIENT_ERROR_CODE
psbt_decode_invalid_error_code=$DECODE_INVALID_ERROR_CODE
psbt_analyze_next_unsigned=$ANALYZE_PSBT_NEXT
psbt_analyze_invalid_error_code=$ANALYZE_INVALID_ERROR_CODE
psbt_finalize_invalid_error_code=$FINALIZE_INVALID_ERROR_CODE
psbt_finalize_unsigned_complete=$FINALIZE_UNSIGNED_COMPLETE
psbt_finalize_unsigned_hex_len=${#FINALIZE_UNSIGNED_HEX}
psbt_utxoupdate_invalid_error_code=$UTXOUPDATE_INVALID_ERROR_CODE
psbt_walletprocess_invalid_error_code=$WALLETPROCESS_INVALID_ERROR_CODE
psbt_signed_complete=$SIGNED_PSBT_COMPLETE
psbt_signed_sig_count=$SIGNED_PSBT_SIG_COUNT
psbt_analyze_next_signed=$ANALYZE_SIGNED_NEXT
psbt_analyze_signed_input0_final=$ANALYZE_SIGNED_IS_FINAL
psbt_join_len=${#JOINED_PSBT}
psbt_combine_mismatch_error_code=$COMBINE_MISMATCH_ERROR_CODE
psbt_combine_invalid_error_code=$COMBINE_INVALID_ERROR_CODE
psbt_join_mismatch_error_code=$JOIN_MISMATCH_ERROR_CODE
psbt_join_invalid_error_code=$JOIN_INVALID_ERROR_CODE
psbt_finalize_complete=$FINALIZED_PSBT_COMPLETE
psbt_final_hex_len=${#FINALIZED_PSBT_HEX}
createrawtransaction_invalid_output_error_code=$CREATE_RAW_INVALID_ERROR_CODE
createrawtransaction_empty_destination_error_code=$CREATE_RAW_EMPTY_DEST_ERROR_CODE
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
quantum_initial_count=$QKEY_INITIAL_COUNT
quantum_after_alias_add_count=$QKEY_AFTER_ALIAS_ADD_COUNT
quantum_after_add_count=$QKEY_ADDED_COUNT
quantum_alias_count=$QKEY_ALIAS_COUNT
quantum_after_remove_count=$QKEY_REMOVED_COUNT
quantum_duplicate_error_code=$QKEY_DUPLICATE_ERROR_CODE
quantum_invalid_type_error_code=$QKEY_INVALID_TYPE_ERROR_CODE
quantum_invalid_hex_error_code=$QKEY_INVALID_HEX_ERROR_CODE
quantum_invalid_address_add_error_code=$QKEY_INVALID_ADDR_ADD_ERROR_CODE
quantum_invalid_address_list_error_code=$QKEY_INVALID_ADDR_LIST_ERROR_CODE
quantum_invalid_address_remove_error_code=$QKEY_INVALID_ADDR_REMOVE_ERROR_CODE
quantum_max_keys_error_code=$QKEY_MAX_ERROR_CODE
quantum_remove_invalid_type_error_code=$QKEY_REMOVE_INVALID_TYPE_ERROR_CODE
quantum_remove_invalid_hex_error_code=$QKEY_REMOVE_INVALID_HEX_ERROR_CODE
quantum_remove_missing_error_code=$QKEY_REMOVE_MISSING_ERROR_CODE
quantum_registry_count=$QKEY_REGISTRY_COUNT
quantum_registry_alias_count=$QKEY_REGISTRY_ALIAS_COUNT
quantum_after_restart_count=$QKEY_RESTART_COUNT
quantum_after_restart_alias_count=$QKEY_RESTART_ALIAS_COUNT
quantum_registry_path=$QKEY_REGISTRY_PATH
gettransaction_unknown_error_code=$GETTX_UNKNOWN_ERROR_CODE
getrawtransaction_unknown_error_code=$GETRAW_UNKNOWN_ERROR_CODE
getblockhash_initial_len=${#BLOCK_HASH_INITIAL}
getblockhash_invalid_error_code=$GETBLOCKHASH_INVALID_ERROR_CODE
getblock_unknown_error_code=$GETBLOCK_UNKNOWN_ERROR_CODE
getblockheader_unknown_error_code=$BLOCKHEADER_UNKNOWN_ERROR_CODE
getmininginfo_consensus=$MININGINFO_CONSENSUS
getmininginfo_networkhashps=$MININGINFO_HASHPS
getblocktemplate_error_code=$GETBLOCKTEMPLATE_ERROR_CODE
generate_error_code=$GENERATE_ERROR_CODE
generatetoaddress_error_code=$GENERATETOADDRESS_ERROR_CODE
generatetodescriptor_error_code=$GENERATETODESCRIPTOR_ERROR_CODE
addnode_error_code=$ADDNODE_ERROR_CODE
disconnectnode_error_code=$DISCONNECTNODE_ERROR_CODE
onetry_error_code=$ONETRY_ERROR_CODE
getblock_v0_hex_len=${#GETBLOCK_V0_HEX}
getblock_bool_false_type=$GETBLOCK_BOOL_FALSE_TYPE
getblock_bool_true_hash=$GETBLOCK_BOOL_TRUE_HASH
getblock_v2_tx_type=$GETBLOCK_V2_TX_TYPE
getblockstats_height=$GETBLOCKSTATS_HEIGHT
getblockstats_invalid_error_code=$GETBLOCKSTATS_INVALID_ERROR_CODE
getchaintips_status=$GETCHAINTIPS_STATUS
getrawmempool_type=$GETRAWMEMPOOL_TYPE
getrawmempool_verbose_type=$GETRAWMEMPOOL_VERBOSE_TYPE
getmempoolentry_unknown_error_code=$GETMEMPOOLENTRY_UNKNOWN_ERROR_CODE
getmempoolancestors_unknown_error_code=$GETMEMPOOLANCESTORS_ERROR_CODE
getmempooldescendants_unknown_error_code=$GETMEMPOOLDESCENDANTS_ERROR_CODE
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
auth_getblockheader_noauth_http_code=$AUTH_GETBLOCKHEADER_NOAUTH_CODE
auth_getblockheader_wrong_http_code=$AUTH_GETBLOCKHEADER_WRONG_CODE
auth_getblockheader_ok_http_code=$AUTH_GETBLOCKHEADER_OK_CODE
auth_getbestblockhash_noauth_http_code=$AUTH_GETBESTBLOCKHASH_NOAUTH_CODE
auth_getbestblockhash_wrong_http_code=$AUTH_GETBESTBLOCKHASH_WRONG_CODE
auth_getbestblockhash_ok_http_code=$AUTH_GETBESTBLOCKHASH_OK_CODE
auth_getblockhash_noauth_http_code=$AUTH_GETBLOCKHASH_NOAUTH_CODE
auth_getblockhash_wrong_http_code=$AUTH_GETBLOCKHASH_WRONG_CODE
auth_getblockhash_ok_http_code=$AUTH_GETBLOCKHASH_OK_CODE
auth_getblockchaininfo_noauth_http_code=$AUTH_GETBLOCKCHAININFO_NOAUTH_CODE
auth_getblockchaininfo_wrong_http_code=$AUTH_GETBLOCKCHAININFO_WRONG_CODE
auth_getblockchaininfo_ok_http_code=$AUTH_GETBLOCKCHAININFO_OK_CODE
auth_getmininginfo_noauth_http_code=$AUTH_GETMININGINFO_NOAUTH_CODE
auth_getmininginfo_wrong_http_code=$AUTH_GETMININGINFO_WRONG_CODE
auth_getmininginfo_ok_http_code=$AUTH_GETMININGINFO_OK_CODE
auth_getblocktemplate_noauth_http_code=$AUTH_GETBLOCKTEMPLATE_NOAUTH_CODE
auth_getblocktemplate_wrong_http_code=$AUTH_GETBLOCKTEMPLATE_WRONG_CODE
auth_getblocktemplate_ok_http_code=$AUTH_GETBLOCKTEMPLATE_OK_CODE
auth_generate_noauth_http_code=$AUTH_GENERATE_NOAUTH_CODE
auth_generate_wrong_http_code=$AUTH_GENERATE_WRONG_CODE
auth_generate_ok_http_code=$AUTH_GENERATE_OK_CODE
auth_generatetoaddress_noauth_http_code=$AUTH_GENERATETOADDRESS_NOAUTH_CODE
auth_generatetoaddress_wrong_http_code=$AUTH_GENERATETOADDRESS_WRONG_CODE
auth_generatetoaddress_ok_http_code=$AUTH_GENERATETOADDRESS_OK_CODE
auth_generatetodescriptor_noauth_http_code=$AUTH_GENERATETODESCRIPTOR_NOAUTH_CODE
auth_generatetodescriptor_wrong_http_code=$AUTH_GENERATETODESCRIPTOR_WRONG_CODE
auth_generatetodescriptor_ok_http_code=$AUTH_GENERATETODESCRIPTOR_OK_CODE
auth_addnode_noauth_http_code=$AUTH_ADDNODE_NOAUTH_CODE
auth_addnode_wrong_http_code=$AUTH_ADDNODE_WRONG_CODE
auth_addnode_ok_http_code=$AUTH_ADDNODE_OK_CODE
auth_disconnectnode_noauth_http_code=$AUTH_DISCONNECTNODE_NOAUTH_CODE
auth_disconnectnode_wrong_http_code=$AUTH_DISCONNECTNODE_WRONG_CODE
auth_disconnectnode_ok_http_code=$AUTH_DISCONNECTNODE_OK_CODE
auth_onetry_noauth_http_code=$AUTH_ONETRY_NOAUTH_CODE
auth_onetry_wrong_http_code=$AUTH_ONETRY_WRONG_CODE
auth_onetry_ok_http_code=$AUTH_ONETRY_OK_CODE
auth_getblock_noauth_http_code=$AUTH_GETBLOCK_NOAUTH_CODE
auth_getblock_wrong_http_code=$AUTH_GETBLOCK_WRONG_CODE
auth_getblock_ok_http_code=$AUTH_GETBLOCK_OK_CODE
auth_getblockstats_noauth_http_code=$AUTH_GETBLOCKSTATS_NOAUTH_CODE
auth_getblockstats_wrong_http_code=$AUTH_GETBLOCKSTATS_WRONG_CODE
auth_getblockstats_ok_http_code=$AUTH_GETBLOCKSTATS_OK_CODE
auth_getchaintips_noauth_http_code=$AUTH_GETCHAINTIPS_NOAUTH_CODE
auth_getchaintips_wrong_http_code=$AUTH_GETCHAINTIPS_WRONG_CODE
auth_getchaintips_ok_http_code=$AUTH_GETCHAINTIPS_OK_CODE
auth_getrawmempool_noauth_http_code=$AUTH_GETRAWMEMPOOL_NOAUTH_CODE
auth_getrawmempool_wrong_http_code=$AUTH_GETRAWMEMPOOL_WRONG_CODE
auth_getrawmempool_ok_http_code=$AUTH_GETRAWMEMPOOL_OK_CODE
auth_getmempoolentry_noauth_http_code=$AUTH_GETMEMPOOLENTRY_NOAUTH_CODE
auth_getmempoolentry_wrong_http_code=$AUTH_GETMEMPOOLENTRY_WRONG_CODE
auth_getmempoolentry_ok_http_code=$AUTH_GETMEMPOOLENTRY_OK_CODE
auth_getmempoolancestors_noauth_http_code=$AUTH_GETMEMPOOLANCESTORS_NOAUTH_CODE
auth_getmempoolancestors_wrong_http_code=$AUTH_GETMEMPOOLANCESTORS_WRONG_CODE
auth_getmempoolancestors_ok_http_code=$AUTH_GETMEMPOOLANCESTORS_OK_CODE
auth_getmempooldescendants_noauth_http_code=$AUTH_GETMEMPOOLDESCENDANTS_NOAUTH_CODE
auth_getmempooldescendants_wrong_http_code=$AUTH_GETMEMPOOLDESCENDANTS_WRONG_CODE
auth_getmempooldescendants_ok_http_code=$AUTH_GETMEMPOOLDESCENDANTS_OK_CODE
auth_getaddressinfo_noauth_http_code=$AUTH_GETADDRESSINFO_NOAUTH_CODE
auth_getaddressinfo_wrong_http_code=$AUTH_GETADDRESSINFO_WRONG_CODE
auth_getaddressinfo_ok_http_code=$AUTH_GETADDRESSINFO_OK_CODE
auth_validateaddress_noauth_http_code=$AUTH_VALIDATEADDRESS_NOAUTH_CODE
auth_validateaddress_wrong_http_code=$AUTH_VALIDATEADDRESS_WRONG_CODE
auth_validateaddress_ok_http_code=$AUTH_VALIDATEADDRESS_OK_CODE
auth_scantxoutset_noauth_http_code=$AUTH_SCANTXOUTSET_NOAUTH_CODE
auth_scantxoutset_wrong_http_code=$AUTH_SCANTXOUTSET_WRONG_CODE
auth_scantxoutset_ok_http_code=$AUTH_SCANTXOUTSET_OK_CODE
auth_createrawtransaction_noauth_http_code=$AUTH_CREATERAWTX_NOAUTH_CODE
auth_createrawtransaction_wrong_http_code=$AUTH_CREATERAWTX_WRONG_CODE
auth_createrawtransaction_ok_http_code=$AUTH_CREATERAWTX_OK_CODE
auth_getbalance_noauth_http_code=$AUTH_GETBALANCE_NOAUTH_CODE
auth_getbalance_wrong_http_code=$AUTH_GETBALANCE_WRONG_CODE
auth_getbalance_ok_http_code=$AUTH_GETBALANCE_OK_CODE
auth_getbalances_noauth_http_code=$AUTH_GETBALANCES_NOAUTH_CODE
auth_getbalances_wrong_http_code=$AUTH_GETBALANCES_WRONG_CODE
auth_getbalances_ok_http_code=$AUTH_GETBALANCES_OK_CODE
auth_gettransaction_noauth_http_code=$AUTH_GETTRANSACTION_NOAUTH_CODE
auth_gettransaction_wrong_http_code=$AUTH_GETTRANSACTION_WRONG_CODE
auth_gettransaction_ok_http_code=$AUTH_GETTRANSACTION_OK_CODE
auth_getrawtransaction_noauth_http_code=$AUTH_GETRAWTRANSACTION_NOAUTH_CODE
auth_getrawtransaction_wrong_http_code=$AUTH_GETRAWTRANSACTION_WRONG_CODE
auth_getrawtransaction_ok_http_code=$AUTH_GETRAWTRANSACTION_OK_CODE
auth_psbt_noauth_http_code=$AUTH_PSBT_NOAUTH_CODE
auth_psbt_wrong_http_code=$AUTH_PSBT_WRONG_CODE
auth_psbt_ok_result_len=${#AUTH_PSBT_OK_RESULT}
auth_sendraw_noauth_http_code=$AUTH_SENDRAW_NOAUTH_CODE
auth_sendraw_wrong_http_code=$AUTH_SENDRAW_WRONG_CODE
auth_sendraw_ok_http_code=$AUTH_SENDRAW_OK_CODE
auth_signraw_noauth_http_code=$AUTH_SIGNRAW_NOAUTH_CODE
auth_signraw_wrong_http_code=$AUTH_SIGNRAW_WRONG_CODE
auth_signraw_ok_http_code=$AUTH_SIGNRAW_OK_CODE
auth_sendtoaddress_noauth_http_code=$AUTH_SENDTOADDR_NOAUTH_CODE
auth_sendtoaddress_wrong_http_code=$AUTH_SENDTOADDR_WRONG_CODE
auth_sendtoaddress_ok_http_code=$AUTH_SENDTOADDR_OK_CODE
auth_lockunspent_noauth_http_code=$AUTH_LOCKUNSPENT_NOAUTH_CODE
auth_lockunspent_wrong_http_code=$AUTH_LOCKUNSPENT_WRONG_CODE
auth_lockunspent_ok_http_code=$AUTH_LOCKUNSPENT_OK_CODE
auth_listlockunspent_noauth_http_code=$AUTH_LISTLOCKUNSPENT_NOAUTH_CODE
auth_listlockunspent_wrong_http_code=$AUTH_LISTLOCKUNSPENT_WRONG_CODE
auth_listlockunspent_ok_http_code=$AUTH_LISTLOCKUNSPENT_OK_CODE
auth_walletlock_noauth_http_code=$AUTH_WALLETLOCK_NOAUTH_CODE
auth_walletlock_wrong_http_code=$AUTH_WALLETLOCK_WRONG_CODE
auth_walletlock_ok_http_code=$AUTH_WALLETLOCK_OK_CODE
auth_walletpassphrase_noauth_http_code=$AUTH_WALLETPASSPHRASE_NOAUTH_CODE
auth_walletpassphrase_wrong_http_code=$AUTH_WALLETPASSPHRASE_WRONG_CODE
auth_walletpassphrase_ok_http_code=$AUTH_WALLETPASSPHRASE_OK_CODE
auth_walletpassphrasechange_noauth_http_code=$AUTH_WALLETPASSPHRASECHANGE_NOAUTH_CODE
auth_walletpassphrasechange_wrong_http_code=$AUTH_WALLETPASSPHRASECHANGE_WRONG_CODE
auth_walletpassphrasechange_ok_http_code=$AUTH_WALLETPASSPHRASECHANGE_OK_CODE
auth_encryptwallet_noauth_http_code=$AUTH_ENCRYPTWALLET_NOAUTH_CODE
auth_encryptwallet_wrong_http_code=$AUTH_ENCRYPTWALLET_WRONG_CODE
auth_encryptwallet_ok_http_code=$AUTH_ENCRYPTWALLET_OK_CODE
auth_createwallet_noauth_http_code=$AUTH_CREATEWALLET_NOAUTH_CODE
auth_createwallet_wrong_http_code=$AUTH_CREATEWALLET_WRONG_CODE
auth_createwallet_ok_http_code=$AUTH_CREATEWALLET_OK_CODE
auth_loadwallet_noauth_http_code=$AUTH_LOADWALLET_NOAUTH_CODE
auth_loadwallet_wrong_http_code=$AUTH_LOADWALLET_WRONG_CODE
auth_loadwallet_ok_http_code=$AUTH_LOADWALLET_OK_CODE
auth_unloadwallet_noauth_http_code=$AUTH_UNLOADWALLET_NOAUTH_CODE
auth_unloadwallet_wrong_http_code=$AUTH_UNLOADWALLET_WRONG_CODE
auth_unloadwallet_ok_http_code=$AUTH_UNLOADWALLET_OK_CODE
auth_dumpprivkey_noauth_http_code=$AUTH_DUMPPRIVKEY_NOAUTH_CODE
auth_dumpprivkey_wrong_http_code=$AUTH_DUMPPRIVKEY_WRONG_CODE
auth_dumpprivkey_ok_http_code=$AUTH_DUMPPRIVKEY_OK_CODE
auth_importprivkey_noauth_http_code=$AUTH_IMPORTPRIVKEY_NOAUTH_CODE
auth_importprivkey_wrong_http_code=$AUTH_IMPORTPRIVKEY_WRONG_CODE
auth_importprivkey_ok_http_code=$AUTH_IMPORTPRIVKEY_OK_CODE
auth_importaddress_noauth_http_code=$AUTH_IMPORTADDRESS_NOAUTH_CODE
auth_importaddress_wrong_http_code=$AUTH_IMPORTADDRESS_WRONG_CODE
auth_importaddress_ok_http_code=$AUTH_IMPORTADDRESS_OK_CODE
auth_backupwallet_noauth_http_code=$AUTH_BACKUPWALLET_NOAUTH_CODE
auth_backupwallet_wrong_http_code=$AUTH_BACKUPWALLET_WRONG_CODE
auth_backupwallet_ok_http_code=$AUTH_BACKUPWALLET_OK_CODE
auth_settxfee_noauth_http_code=$AUTH_SETTXFEE_NOAUTH_CODE
auth_settxfee_wrong_http_code=$AUTH_SETTXFEE_WRONG_CODE
auth_settxfee_ok_http_code=$AUTH_SETTXFEE_OK_CODE
auth_keypoolrefill_noauth_http_code=$AUTH_KEYPOOLREFILL_NOAUTH_CODE
auth_keypoolrefill_wrong_http_code=$AUTH_KEYPOOLREFILL_WRONG_CODE
auth_keypoolrefill_ok_http_code=$AUTH_KEYPOOLREFILL_OK_CODE
auth_signmessage_noauth_http_code=$AUTH_SIGNMESSAGE_NOAUTH_CODE
auth_signmessage_wrong_http_code=$AUTH_SIGNMESSAGE_WRONG_CODE
auth_signmessage_ok_http_code=$AUTH_SIGNMESSAGE_OK_CODE
auth_verifymessage_noauth_http_code=$AUTH_VERIFYMESSAGE_NOAUTH_CODE
auth_verifymessage_wrong_http_code=$AUTH_VERIFYMESSAGE_WRONG_CODE
auth_verifymessage_ok_http_code=$AUTH_VERIFYMESSAGE_OK_CODE
auth_getnewaddress_noauth_http_code=$AUTH_GETNEWADDRESS_NOAUTH_CODE
auth_getnewaddress_wrong_http_code=$AUTH_GETNEWADDRESS_WRONG_CODE
auth_getnewaddress_ok_http_code=$AUTH_GETNEWADDRESS_OK_CODE
auth_setlabel_noauth_http_code=$AUTH_SETLABEL_NOAUTH_CODE
auth_setlabel_wrong_http_code=$AUTH_SETLABEL_WRONG_CODE
auth_setlabel_ok_http_code=$AUTH_SETLABEL_OK_CODE
auth_getrawchangeaddress_noauth_http_code=$AUTH_GETRAWCHANGEADDR_NOAUTH_CODE
auth_getrawchangeaddress_wrong_http_code=$AUTH_GETRAWCHANGEADDR_WRONG_CODE
auth_getrawchangeaddress_ok_http_code=$AUTH_GETRAWCHANGEADDR_OK_CODE
auth_listlabels_noauth_http_code=$AUTH_LISTLABELS_NOAUTH_CODE
auth_listlabels_wrong_http_code=$AUTH_LISTLABELS_WRONG_CODE
auth_listlabels_ok_http_code=$AUTH_LISTLABELS_OK_CODE
auth_getaddressesbylabel_noauth_http_code=$AUTH_GETADDRBYLABEL_NOAUTH_CODE
auth_getaddressesbylabel_wrong_http_code=$AUTH_GETADDRBYLABEL_WRONG_CODE
auth_getaddressesbylabel_ok_http_code=$AUTH_GETADDRBYLABEL_OK_CODE
auth_getreceivedbylabel_noauth_http_code=$AUTH_GETRECVBYLABEL_NOAUTH_CODE
auth_getreceivedbylabel_wrong_http_code=$AUTH_GETRECVBYLABEL_WRONG_CODE
auth_getreceivedbylabel_ok_http_code=$AUTH_GETRECVBYLABEL_OK_CODE
auth_getwalletinfo_noauth_http_code=$AUTH_GETWALLETINFO_NOAUTH_CODE
auth_getwalletinfo_wrong_http_code=$AUTH_GETWALLETINFO_WRONG_CODE
auth_getwalletinfo_ok_http_code=$AUTH_GETWALLETINFO_OK_CODE
auth_listaddressgroupings_noauth_http_code=$AUTH_LISTADDRGROUPINGS_NOAUTH_CODE
auth_listaddressgroupings_wrong_http_code=$AUTH_LISTADDRGROUPINGS_WRONG_CODE
auth_listaddressgroupings_ok_http_code=$AUTH_LISTADDRGROUPINGS_OK_CODE
auth_listreceivedbyaddress_noauth_http_code=$AUTH_LISTRECVBYADDR_NOAUTH_CODE
auth_listreceivedbyaddress_wrong_http_code=$AUTH_LISTRECVBYADDR_WRONG_CODE
auth_listreceivedbyaddress_ok_http_code=$AUTH_LISTRECVBYADDR_OK_CODE
auth_listunspent_noauth_http_code=$AUTH_LISTUNSPENT_NOAUTH_CODE
auth_listunspent_wrong_http_code=$AUTH_LISTUNSPENT_WRONG_CODE
auth_listunspent_ok_http_code=$AUTH_LISTUNSPENT_OK_CODE
auth_listtransactions_noauth_http_code=$AUTH_LISTTRANSACTIONS_NOAUTH_CODE
auth_listtransactions_wrong_http_code=$AUTH_LISTTRANSACTIONS_WRONG_CODE
auth_listtransactions_ok_http_code=$AUTH_LISTTRANSACTIONS_OK_CODE
auth_listsinceblock_noauth_http_code=$AUTH_LISTSINCEBLOCK_NOAUTH_CODE
auth_listsinceblock_wrong_http_code=$AUTH_LISTSINCEBLOCK_WRONG_CODE
auth_listsinceblock_ok_http_code=$AUTH_LISTSINCEBLOCK_OK_CODE
auth_walletprocesspsbt_noauth_http_code=$AUTH_WALLETPROCESS_NOAUTH_CODE
auth_walletprocesspsbt_wrong_http_code=$AUTH_WALLETPROCESS_WRONG_CODE
auth_walletprocesspsbt_ok_http_code=$AUTH_WALLETPROCESS_OK_CODE
auth_decodepsbt_noauth_http_code=$AUTH_DECODEPSBT_NOAUTH_CODE
auth_decodepsbt_wrong_http_code=$AUTH_DECODEPSBT_WRONG_CODE
auth_decodepsbt_ok_http_code=$AUTH_DECODEPSBT_OK_CODE
auth_analyzepsbt_noauth_http_code=$AUTH_ANALYZEPSBT_NOAUTH_CODE
auth_analyzepsbt_wrong_http_code=$AUTH_ANALYZEPSBT_WRONG_CODE
auth_analyzepsbt_ok_http_code=$AUTH_ANALYZEPSBT_OK_CODE
auth_finalizepsbt_noauth_http_code=$AUTH_FINALIZEPSBT_NOAUTH_CODE
auth_finalizepsbt_wrong_http_code=$AUTH_FINALIZEPSBT_WRONG_CODE
auth_finalizepsbt_ok_http_code=$AUTH_FINALIZEPSBT_OK_CODE
auth_utxoupdatepsbt_noauth_http_code=$AUTH_UTXOUPDATEPSBT_NOAUTH_CODE
auth_utxoupdatepsbt_wrong_http_code=$AUTH_UTXOUPDATEPSBT_WRONG_CODE
auth_utxoupdatepsbt_ok_http_code=$AUTH_UTXOUPDATEPSBT_OK_CODE
auth_combinepsbt_noauth_http_code=$AUTH_COMBINEPSBT_NOAUTH_CODE
auth_combinepsbt_wrong_http_code=$AUTH_COMBINEPSBT_WRONG_CODE
auth_combinepsbt_ok_http_code=$AUTH_COMBINEPSBT_OK_CODE
auth_joinpsbts_noauth_http_code=$AUTH_JOINPSBTS_NOAUTH_CODE
auth_joinpsbts_wrong_http_code=$AUTH_JOINPSBTS_WRONG_CODE
auth_joinpsbts_ok_http_code=$AUTH_JOINPSBTS_OK_CODE
auth_walletcreatefundedpsbt_noauth_http_code=$AUTH_WALLETCREATE_NOAUTH_CODE
auth_walletcreatefundedpsbt_wrong_http_code=$AUTH_WALLETCREATE_WRONG_CODE
auth_walletcreatefundedpsbt_ok_http_code=$AUTH_WALLETCREATE_OK_CODE
auth_addquantumkey_noauth_http_code=$AUTH_QUANTUM_NOAUTH_CODE
auth_addquantumkey_wrong_http_code=$AUTH_QUANTUM_WRONG_CODE
auth_addquantumkey_ok_http_code=$AUTH_QUANTUM_OK_CODE
auth_removequantumkey_noauth_http_code=$AUTH_REMOVEQUANTUM_NOAUTH_CODE
auth_removequantumkey_wrong_http_code=$AUTH_REMOVEQUANTUM_WRONG_CODE
auth_removequantumkey_ok_http_code=$AUTH_REMOVEQUANTUM_OK_CODE
auth_listquantumkeys_noauth_http_code=$AUTH_LISTQUANTUM_NOAUTH_CODE
auth_listquantumkeys_wrong_http_code=$AUTH_LISTQUANTUM_WRONG_CODE
auth_listquantumkeys_ok_http_code=$AUTH_LISTQUANTUM_OK_CODE
node_log=$ARTIFACT_DIR/node.log
btcrpc_log=$ARTIFACT_DIR/btcrpc.log
btcrpc_auth_log=$ARTIFACT_DIR/btcrpc_auth.log
TXT

echo "E2E transaction flow succeeded. Artifacts written to $ARTIFACT_DIR"
