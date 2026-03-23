#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: ./scripts/launch/prefill_go_no_go_signoff.sh \
  --release-commit <sha> \
  --genesis-hash <sha256> \
  --launch-window-start <YYYY-MM-DDTHH:MM:SSZ> \
  --launch-window-end <YYYY-MM-DDTHH:MM:SSZ> \
  --final-decision <GO|NO-GO> \
  --approvers <comma-separated-names> \
  [--allow-go] \
  [--decision-timestamp <YYYY-MM-DDTHH:MM:SSZ>] \
  [--file <path>]

Options:
  --release-commit <sha>         Required. 7-40 char hex commit SHA.
  --genesis-hash <sha256>        Required. 64-char hex hash.
  --launch-window-start <ts>     Required. RFC3339 UTC timestamp.
  --launch-window-end <ts>       Required. RFC3339 UTC timestamp.
  --final-decision <GO|NO-GO>    Required.
  --approvers <names>            Required. Free-form approver list.
  --allow-go                     Optional safety override required when final decision is GO.
  --decision-timestamp <ts>      Optional. RFC3339 UTC. Default: now.
  --file <path>                  Checklist file path. Default: docs/mainnet-go-no-go-checklist.md
  -h, --help                     Show this help text.
USAGE
}

CHECKLIST_FILE="docs/mainnet-go-no-go-checklist.md"
RELEASE_COMMIT=""
GENESIS_HASH=""
LAUNCH_WINDOW_START=""
LAUNCH_WINDOW_END=""
FINAL_DECISION=""
APPROVERS=""
DECISION_TIMESTAMP=""
ALLOW_GO=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release-commit)
      if [[ $# -lt 2 ]]; then
        echo "--release-commit requires a value" >&2
        exit 1
      fi
      RELEASE_COMMIT="$2"
      shift 2
      ;;
    --genesis-hash)
      if [[ $# -lt 2 ]]; then
        echo "--genesis-hash requires a value" >&2
        exit 1
      fi
      GENESIS_HASH="$2"
      shift 2
      ;;
    --launch-window-start)
      if [[ $# -lt 2 ]]; then
        echo "--launch-window-start requires a value" >&2
        exit 1
      fi
      LAUNCH_WINDOW_START="$2"
      shift 2
      ;;
    --launch-window-end)
      if [[ $# -lt 2 ]]; then
        echo "--launch-window-end requires a value" >&2
        exit 1
      fi
      LAUNCH_WINDOW_END="$2"
      shift 2
      ;;
    --final-decision)
      if [[ $# -lt 2 ]]; then
        echo "--final-decision requires a value" >&2
        exit 1
      fi
      FINAL_DECISION="$2"
      shift 2
      ;;
    --approvers)
      if [[ $# -lt 2 ]]; then
        echo "--approvers requires a value" >&2
        exit 1
      fi
      APPROVERS="$2"
      shift 2
      ;;
    --decision-timestamp)
      if [[ $# -lt 2 ]]; then
        echo "--decision-timestamp requires a value" >&2
        exit 1
      fi
      DECISION_TIMESTAMP="$2"
      shift 2
      ;;
    --allow-go)
      ALLOW_GO=1
      shift
      ;;
    --file)
      if [[ $# -lt 2 ]]; then
        echo "--file requires a path value" >&2
        exit 1
      fi
      CHECKLIST_FILE="$2"
      shift 2
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

require_non_empty() {
  local value="$1"
  local flag="$2"
  if [[ -z "${value// }" ]]; then
    echo "$flag is required." >&2
    exit 1
  fi
}

require_non_empty "$RELEASE_COMMIT" "--release-commit"
require_non_empty "$GENESIS_HASH" "--genesis-hash"
require_non_empty "$LAUNCH_WINDOW_START" "--launch-window-start"
require_non_empty "$LAUNCH_WINDOW_END" "--launch-window-end"
require_non_empty "$FINAL_DECISION" "--final-decision"
require_non_empty "$APPROVERS" "--approvers"

if [[ ! -f "$CHECKLIST_FILE" ]]; then
  echo "Checklist file not found: $CHECKLIST_FILE" >&2
  exit 1
fi

if [[ ! "$RELEASE_COMMIT" =~ ^[0-9a-fA-F]{7,40}$ ]]; then
  echo "--release-commit must be 7-40 hex characters." >&2
  exit 1
fi

if [[ ! "$GENESIS_HASH" =~ ^[0-9a-fA-F]{64}$ ]]; then
  echo "--genesis-hash must be 64 hex characters." >&2
  exit 1
fi

if [[ ! "$LAUNCH_WINDOW_START" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]]; then
  echo "--launch-window-start must be RFC3339 UTC (YYYY-MM-DDTHH:MM:SSZ)." >&2
  exit 1
fi

if [[ ! "$LAUNCH_WINDOW_END" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]]; then
  echo "--launch-window-end must be RFC3339 UTC (YYYY-MM-DDTHH:MM:SSZ)." >&2
  exit 1
fi

normalized_decision="$(echo "$FINAL_DECISION" | tr '[:lower:]' '[:upper:]' | xargs)"
if [[ "$normalized_decision" != "GO" && "$normalized_decision" != "NO-GO" ]]; then
  echo "--final-decision must be GO or NO-GO." >&2
  exit 1
fi

if [[ "$normalized_decision" == "GO" && "$ALLOW_GO" -ne 1 ]]; then
  echo "Refusing to set final decision GO without explicit override." >&2
  echo "Re-run with --allow-go after completing final launch review." >&2
  exit 1
fi

if [[ -z "$DECISION_TIMESTAMP" ]]; then
  DECISION_TIMESTAMP="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
fi

if [[ ! "$DECISION_TIMESTAMP" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]]; then
  echo "--decision-timestamp must be RFC3339 UTC (YYYY-MM-DDTHH:MM:SSZ)." >&2
  exit 1
fi

launch_window_value="${LAUNCH_WINDOW_START} to ${LAUNCH_WINDOW_END}"

tmp_file="$(mktemp)"
awk \
  -v release_commit="$RELEASE_COMMIT" \
  -v genesis_hash="$GENESIS_HASH" \
  -v launch_window="$launch_window_value" \
  -v final_decision="$normalized_decision" \
  -v decision_timestamp="$DECISION_TIMESTAMP" \
  -v approvers="$APPROVERS" \
  '{
    if ($0 ~ /^- Release candidate commit:/) {
      print "- Release candidate commit: " release_commit
      next
    }
    if ($0 ~ /^- Proposed genesis hash:/) {
      print "- Proposed genesis hash: " genesis_hash
      next
    }
    if ($0 ~ /^- Planned launch window \(UTC\):/) {
      print "- Planned launch window (UTC): " launch_window
      next
    }
    if ($0 ~ /^- Final decision:/) {
      print "- Final decision: " final_decision
      next
    }
    if ($0 ~ /^- Decision timestamp \(UTC\):/) {
      print "- Decision timestamp (UTC): " decision_timestamp
      next
    }
    if ($0 ~ /^- Signoff approvers:/) {
      print "- Signoff approvers: " approvers
      next
    }
    print $0
  }' "$CHECKLIST_FILE" > "$tmp_file"

mv "$tmp_file" "$CHECKLIST_FILE"

echo "Updated signoff block in: $CHECKLIST_FILE"
echo "Release candidate commit: $RELEASE_COMMIT"
echo "Proposed genesis hash: $GENESIS_HASH"
echo "Planned launch window (UTC): $launch_window_value"
echo "Final decision: $normalized_decision"
echo "Decision timestamp (UTC): $DECISION_TIMESTAMP"
echo "Signoff approvers: $APPROVERS"
