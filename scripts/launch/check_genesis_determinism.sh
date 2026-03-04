#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/launch/check_genesis_determinism.sh [--testnet] [--num-accounts <n>] [--utxo-snapshot <path>] [--patoshi-csv <path> --satoshi-address <btc-addr>] [--chain-id <id>] [--genesis-time <rfc3339>] [--validator-account <id>] [--validator-key <key>] [--json-out <path>] [--keep-tmp]

Purpose:
  Validate launch gate #9 by generating genesis twice with identical inputs and
  asserting the resulting genesis.json SHA256 hashes are identical.

Modes:
  --testnet            Use synthetic deterministic fixture input (default).
  --utxo-snapshot      Use a real Bitcoin dumptxoutset snapshot (mainnet mode).

Options:
  --num-accounts <n>   Synthetic account count for --testnet mode. Default: 100.
  --patoshi-csv <path> Optional Patoshi CSV for --utxo-snapshot mode.
  --satoshi-address <addr> Required when --patoshi-csv is provided to avoid
                           nondeterministic auto-key generation.
  --chain-id <id>      Chain ID passed to genesis generation. Default: bitinfinity-mainnet.
  --genesis-time <ts>  Canonical genesis timestamp (RFC3339). Default: 2026-01-01T00:00:00Z.
  --validator-account <id> Validator account ID. Default: validator.bitinfinity.
  --validator-key <key> Validator ed25519 key used by genesis tool.
  --json-out <path>    Write machine-readable summary JSON.
  --keep-tmp           Keep temporary run directory on success.
  -h, --help           Show this help text.
EOF
}

MODE="testnet"
NUM_ACCOUNTS=100
UTXO_SNAPSHOT=""
PATOSHI_CSV=""
SATOSHI_ADDRESS=""
CHAIN_ID="bitinfinity-mainnet"
GENESIS_TIME="2026-01-01T00:00:00Z"
VALIDATOR_ACCOUNT="validator.bitinfinity"
VALIDATOR_KEY="ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp"
JSON_OUT=""
KEEP_TMP=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --testnet)
      MODE="testnet"
      shift
      ;;
    --utxo-snapshot)
      if [[ $# -lt 2 ]]; then
        echo "--utxo-snapshot requires a path value" >&2
        exit 1
      fi
      MODE="snapshot"
      UTXO_SNAPSHOT="$2"
      shift 2
      ;;
    --patoshi-csv)
      if [[ $# -lt 2 ]]; then
        echo "--patoshi-csv requires a path value" >&2
        exit 1
      fi
      PATOSHI_CSV="$2"
      shift 2
      ;;
    --satoshi-address)
      if [[ $# -lt 2 ]]; then
        echo "--satoshi-address requires a value" >&2
        exit 1
      fi
      SATOSHI_ADDRESS="$2"
      shift 2
      ;;
    --num-accounts)
      if [[ $# -lt 2 ]]; then
        echo "--num-accounts requires a numeric value" >&2
        exit 1
      fi
      NUM_ACCOUNTS="$2"
      shift 2
      ;;
    --chain-id)
      if [[ $# -lt 2 ]]; then
        echo "--chain-id requires a value" >&2
        exit 1
      fi
      CHAIN_ID="$2"
      shift 2
      ;;
    --genesis-time)
      if [[ $# -lt 2 ]]; then
        echo "--genesis-time requires an RFC3339 value" >&2
        exit 1
      fi
      GENESIS_TIME="$2"
      shift 2
      ;;
    --validator-account)
      if [[ $# -lt 2 ]]; then
        echo "--validator-account requires a value" >&2
        exit 1
      fi
      VALIDATOR_ACCOUNT="$2"
      shift 2
      ;;
    --validator-key)
      if [[ $# -lt 2 ]]; then
        echo "--validator-key requires a value" >&2
        exit 1
      fi
      VALIDATOR_KEY="$2"
      shift 2
      ;;
    --json-out)
      if [[ $# -lt 2 ]]; then
        echo "--json-out requires a path value" >&2
        exit 1
      fi
      JSON_OUT="$2"
      shift 2
      ;;
    --keep-tmp)
      KEEP_TMP=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ ! "$NUM_ACCOUNTS" =~ ^[0-9]+$ ]] || [[ "$NUM_ACCOUNTS" -eq 0 ]]; then
  echo "--num-accounts must be a positive integer." >&2
  exit 1
fi

if [[ "$MODE" == "snapshot" ]]; then
  if [[ -z "$UTXO_SNAPSHOT" ]]; then
    echo "--utxo-snapshot is required in snapshot mode." >&2
    exit 1
  fi
  if [[ ! -f "$UTXO_SNAPSHOT" ]]; then
    echo "UTXO snapshot not found: $UTXO_SNAPSHOT" >&2
    exit 1
  fi
else
  if [[ -n "$PATOSHI_CSV" || -n "$SATOSHI_ADDRESS" ]]; then
    echo "--patoshi-csv/--satoshi-address are only valid with --utxo-snapshot mode." >&2
    exit 1
  fi
fi

if [[ -n "$PATOSHI_CSV" || -n "$SATOSHI_ADDRESS" ]]; then
  if [[ -z "$PATOSHI_CSV" || -z "$SATOSHI_ADDRESS" ]]; then
    echo "When using Patoshi reassignment, both --patoshi-csv and --satoshi-address are required." >&2
    echo "This avoids nondeterministic auto-keypair generation." >&2
    exit 1
  fi
  if [[ ! -f "$PATOSHI_CSV" ]]; then
    echo "Patoshi CSV not found: $PATOSHI_CSV" >&2
    exit 1
  fi
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Required command not found: $cmd" >&2
    exit 1
  fi
}

sha256_file() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    echo "Missing SHA256 tool: install sha256sum or shasum" >&2
    exit 1
  fi
}

require_cmd cargo
require_cmd jq
require_cmd mktemp
require_cmd awk

run_once() {
  local output_dir="$1"
  local log_file="$2"
  local -a cmd=(
    cargo run -q -p bitinfinity-tools -- generate-genesis
    --output-dir "$output_dir"
    --chain-id "$CHAIN_ID"
    --genesis-time "$GENESIS_TIME"
    --validator-account "$VALIDATOR_ACCOUNT"
    --validator-key "$VALIDATOR_KEY"
  )

  if [[ "$MODE" == "testnet" ]]; then
    cmd+=(--testnet --num-accounts "$NUM_ACCOUNTS")
  else
    cmd+=(--utxo-snapshot "$UTXO_SNAPSHOT")
    if [[ -n "$PATOSHI_CSV" ]]; then
      cmd+=(--patoshi-csv "$PATOSHI_CSV" --satoshi-address "$SATOSHI_ADDRESS")
    fi
  fi

  echo "Running: ${cmd[*]}"
  "${cmd[@]}" >"$log_file" 2>&1
}

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/genesis-determinism.XXXXXX")"
run1_dir="${tmp_root}/run1"
run2_dir="${tmp_root}/run2"
run1_log="${tmp_root}/run1.log"
run2_log="${tmp_root}/run2.log"
run1_genesis="${run1_dir}/genesis.json"
run2_genesis="${run2_dir}/genesis.json"

run_once "$run1_dir" "$run1_log"
run_once "$run2_dir" "$run2_log"

if [[ ! -f "$run1_genesis" || ! -f "$run2_genesis" ]]; then
  echo "Missing genesis output after deterministic check run." >&2
  echo "run1 log: $run1_log" >&2
  echo "run2 log: $run2_log" >&2
  echo "tmp dir: $tmp_root" >&2
  exit 1
fi

hash1="$(sha256_file "$run1_genesis")"
hash2="$(sha256_file "$run2_genesis")"

genesis_time="$(jq -r '.genesis_time' "$run1_genesis")"
total_supply="$(jq -r '.total_supply' "$run1_genesis")"
record_count="$(jq '.records | length' "$run1_genesis")"
account_record_count="$(jq '[.records[] | .Account? | select(.)] | length' "$run1_genesis")"
data_record_count="$(jq '[.records[] | .Data? | select(.)] | length' "$run1_genesis")"

if [[ -n "$JSON_OUT" ]]; then
  jq -n \
    --arg mode "$MODE" \
    --arg chain_id "$CHAIN_ID" \
    --arg genesis_time "$genesis_time" \
    --arg total_supply "$total_supply" \
    --arg hash1 "$hash1" \
    --arg hash2 "$hash2" \
    --argjson deterministic "$([[ "$hash1" == "$hash2" ]] && echo true || echo false)" \
    --argjson records "$record_count" \
    --argjson account_records "$account_record_count" \
    --argjson data_records "$data_record_count" \
    --arg run1_log "$run1_log" \
    --arg run2_log "$run2_log" \
    --arg tmp_dir "$tmp_root" \
    '{
      mode: $mode,
      chain_id: $chain_id,
      genesis_time: $genesis_time,
      total_supply: $total_supply,
      records: $records,
      account_records: $account_records,
      data_records: $data_records,
      hash_run1: $hash1,
      hash_run2: $hash2,
      deterministic: $deterministic,
      logs: {
        run1: $run1_log,
        run2: $run2_log
      },
      tmp_dir: $tmp_dir
    }' > "$JSON_OUT"
fi

if [[ "$hash1" != "$hash2" ]]; then
  echo "Genesis determinism check failed: hashes differ." >&2
  echo "run1 hash: $hash1" >&2
  echo "run2 hash: $hash2" >&2
  echo "run1: $run1_genesis" >&2
  echo "run2: $run2_genesis" >&2
  if command -v diff >/dev/null 2>&1; then
    diff -u "$run1_genesis" "$run2_genesis" > "${tmp_root}/genesis.diff" || true
    echo "diff: ${tmp_root}/genesis.diff" >&2
  fi
  echo "tmp dir kept for inspection: $tmp_root" >&2
  exit 1
fi

echo "Genesis determinism check passed at $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
echo "mode: $MODE"
echo "chain_id: $CHAIN_ID"
echo "genesis_time: $genesis_time"
echo "records: $record_count (account=$account_record_count, data=$data_record_count)"
echo "total_supply: $total_supply"
echo "sha256: $hash1"

if [[ "$KEEP_TMP" -eq 1 ]]; then
  echo "tmp dir kept: $tmp_root"
else
  rm -rf "$tmp_root"
fi
