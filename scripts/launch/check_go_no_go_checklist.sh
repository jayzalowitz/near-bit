#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/launch/check_go_no_go_checklist.sh [--file <path>] [--require-go] [--json-out <path>]

Options:
  --file <path>  Checklist file path. Default: docs/mainnet-go-no-go-checklist.md
  --require-go   Exit non-zero unless every gate status is "done" and signoff fields are populated.
  --json-out     Write machine-readable summary JSON to the specified file path.
  -h, --help     Show this help text.
EOF
}

CHECKLIST_FILE="docs/mainnet-go-no-go-checklist.md"
REQUIRE_GO=0
JSON_OUT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --file)
      if [[ $# -lt 2 ]]; then
        echo "--file requires a path value" >&2
        exit 1
      fi
      CHECKLIST_FILE="$2"
      shift 2
      ;;
    --require-go)
      REQUIRE_GO=1
      shift
      ;;
    --json-out)
      if [[ $# -lt 2 ]]; then
        echo "--json-out requires a path value" >&2
        exit 1
      fi
      JSON_OUT="$2"
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

if [[ ! -f "$CHECKLIST_FILE" ]]; then
  echo "Checklist file not found: $CHECKLIST_FILE" >&2
  exit 1
fi

if ! command -v awk >/dev/null 2>&1; then
  echo "Required command not found: awk" >&2
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "Required command not found: jq" >&2
  exit 1
fi

total_gates=0
done_gates=0
todo_gates=0
invalid_status=0

declare -a pending_lines
declare -a invalid_lines

while IFS=$'\t' read -r gate_id gate_name gate_status; do
  total_gates=$((total_gates + 1))
  normalized_status="$(echo "$gate_status" | tr '[:upper:]' '[:lower:]' | xargs)"
  if [[ "$normalized_status" == "done" ]]; then
    done_gates=$((done_gates + 1))
  elif [[ "$normalized_status" == "todo" ]]; then
    todo_gates=$((todo_gates + 1))
    pending_lines+=("${gate_id}: ${gate_name}")
  else
    invalid_status=$((invalid_status + 1))
    invalid_lines+=("${gate_id}: ${gate_name} [status=${gate_status}]")
  fi
done < <(
  awk -F'|' '
    $0 ~ /^\|[[:space:]]*[0-9]+[[:space:]]*\|/ {
      id=$2; gate=$3; status=$5;
      gsub(/^[ \t]+|[ \t]+$/, "", id);
      gsub(/^[ \t]+|[ \t]+$/, "", gate);
      gsub(/^[ \t]+|[ \t]+$/, "", status);
      printf "%s\t%s\t%s\n", id, gate, status;
    }
  ' "$CHECKLIST_FILE"
)

if [[ "$total_gates" -eq 0 ]]; then
  echo "No gate rows found in checklist: $CHECKLIST_FILE" >&2
  exit 1
fi

required_signoff_fields=(
  "Release candidate commit:"
  "Proposed genesis hash:"
  "Planned launch window (UTC):"
  "Final decision:"
  "Decision timestamp (UTC):"
  "Signoff approvers:"
)

missing_signoff=0
declare -a missing_signoff_lines
for field in "${required_signoff_fields[@]}"; do
  raw_line="$(
    awk -v needle="$field" '
      {
        line = $0
        sub(/^[[:space:]]*-[[:space:]]*/, "", line)
        if (index(line, needle) == 1) {
          print line
          exit
        }
      }
    ' "$CHECKLIST_FILE"
  )"
  value="${raw_line#"$field"}"
  value="$(echo "$value" | xargs)"
  if [[ -z "$raw_line" || -z "${value// }" ]]; then
    missing_signoff=$((missing_signoff + 1))
    missing_signoff_lines+=("$field")
  fi
done

echo "Checklist summary: file=${CHECKLIST_FILE}"
echo "Total gates:   ${total_gates}"
echo "Done gates:    ${done_gates}"
echo "Todo gates:    ${todo_gates}"
echo "Invalid gates: ${invalid_status}"
echo "Missing signoff fields: ${missing_signoff}"

if [[ "$todo_gates" -gt 0 ]]; then
  echo
  echo "Pending gates:"
  for line in "${pending_lines[@]-}"; do
    if [[ -z "$line" ]]; then
      continue
    fi
    echo "  - ${line}"
  done
fi

if [[ "$invalid_status" -gt 0 ]]; then
  echo
  echo "Invalid gate statuses:"
  for line in "${invalid_lines[@]-}"; do
    if [[ -z "$line" ]]; then
      continue
    fi
    echo "  - ${line}"
  done
fi

if [[ "$missing_signoff" -gt 0 ]]; then
  echo
  echo "Missing signoff fields:"
  for line in "${missing_signoff_lines[@]-}"; do
    if [[ -z "$line" ]]; then
      continue
    fi
    echo "  - ${line}"
  done
fi

if [[ -n "$JSON_OUT" ]]; then
  pending_json="$(printf '%s\n' "${pending_lines[@]-}" | jq -R -s 'split("\n") | map(select(length > 0))')"
  invalid_json="$(printf '%s\n' "${invalid_lines[@]-}" | jq -R -s 'split("\n") | map(select(length > 0))')"
  missing_json="$(printf '%s\n' "${missing_signoff_lines[@]-}" | jq -R -s 'split("\n") | map(select(length > 0))')"

  jq -n \
    --arg file "$CHECKLIST_FILE" \
    --argjson require_go "$REQUIRE_GO" \
    --argjson total_gates "$total_gates" \
    --argjson done_gates "$done_gates" \
    --argjson todo_gates "$todo_gates" \
    --argjson invalid_gates "$invalid_status" \
    --argjson missing_signoff_fields "$missing_signoff" \
    --argjson pending "$pending_json" \
    --argjson invalid "$invalid_json" \
    --argjson missing "$missing_json" \
    '{
      checklist_file: $file,
      require_go: $require_go,
      totals: {
        gates: $total_gates,
        done: $done_gates,
        todo: $todo_gates,
        invalid: $invalid_gates,
        missing_signoff_fields: $missing_signoff_fields
      },
      pending_gates: ($pending // []),
      invalid_gates: ($invalid // []),
      missing_signoff: ($missing // [])
    }' > "$JSON_OUT"
fi

if [[ "$REQUIRE_GO" -eq 1 ]]; then
  if [[ "$todo_gates" -gt 0 || "$invalid_status" -gt 0 || "$missing_signoff" -gt 0 ]]; then
    echo
    echo "GO criteria not met (--require-go)." >&2
    exit 1
  fi
fi
