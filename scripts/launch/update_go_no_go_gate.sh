#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/launch/update_go_no_go_gate.sh --gate <id> --status <todo|done> [options]

Options:
  --file <path>          Checklist file path. Default: docs/mainnet-go-no-go-checklist.md
  --gate <id>            Numeric gate id to update (1..N).
  --status <todo|done>   Target gate status.
  --owner <value>        Required for --status done; rejected for --status todo.
  --evidence <value>     Required for --status done; rejected for --status todo.
                         Accepts comma/semicolon-separated refs (repo paths or http(s) URLs).
  --completed-date <v>   Required for --status done; rejected for --status todo.
                         Format: YYYY-MM-DD or YYYY-MM-DDTHH:MM:SSZ.
  -h, --help             Show this help text.
EOF
}

CHECKLIST_FILE="docs/mainnet-go-no-go-checklist.md"
GATE_ID=""
STATUS=""
OWNER=""
EVIDENCE=""
COMPLETED_DATE=""

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
    --gate)
      if [[ $# -lt 2 ]]; then
        echo "--gate requires a numeric value" >&2
        exit 1
      fi
      GATE_ID="$2"
      shift 2
      ;;
    --status)
      if [[ $# -lt 2 ]]; then
        echo "--status requires a value (todo|done)" >&2
        exit 1
      fi
      STATUS="$2"
      shift 2
      ;;
    --owner)
      if [[ $# -lt 2 ]]; then
        echo "--owner requires a value" >&2
        exit 1
      fi
      OWNER="$2"
      shift 2
      ;;
    --evidence)
      if [[ $# -lt 2 ]]; then
        echo "--evidence requires a value" >&2
        exit 1
      fi
      EVIDENCE="$2"
      shift 2
      ;;
    --completed-date)
      if [[ $# -lt 2 ]]; then
        echo "--completed-date requires a value" >&2
        exit 1
      fi
      COMPLETED_DATE="$2"
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

trim() {
  local value="$1"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  printf '%s' "$value"
}

normalize_evidence_ref() {
  local value="$1"
  local normalized="$value"

  if [[ "$normalized" == \[*\]\(*\) ]]; then
    local link_target="${normalized#*](}"
    if [[ "$link_target" != "$normalized" && "$link_target" == *")" ]]; then
      normalized="${link_target%)}"
    fi
  fi
  if [[ "$normalized" == \`*\` ]]; then
    normalized="${normalized#\`}"
    normalized="${normalized%\`}"
  fi
  printf '%s' "$normalized"
}

resolve_local_evidence_candidate() {
  local value="$1"
  local candidate="$value"
  if [[ "$candidate" =~ ^(.+)#L[0-9]+(C[0-9]+)?$ ]]; then
    candidate="${BASH_REMATCH[1]}"
  fi
  if [[ "$candidate" =~ ^(.+):[0-9]+(:[0-9]+)?$ ]]; then
    candidate="${BASH_REMATCH[1]}"
  fi
  printf '%s' "$candidate"
}

is_valid_evidence_ref() {
  local ref="$1"
  local normalized
  local candidate

  normalized="$(normalize_evidence_ref "$ref")"
  if [[ "$normalized" =~ ^https?:// ]]; then
    return 0
  fi
  candidate="$(resolve_local_evidence_candidate "$normalized")"
  if [[ -f "$candidate" ]]; then
    return 0
  fi
  return 1
}

validate_done_inputs() {
  local owner_trimmed
  local evidence_trimmed
  local completed_trimmed
  local evidence_entries
  local evidence_ref

  owner_trimmed="$(trim "$OWNER")"
  evidence_trimmed="$(trim "$EVIDENCE")"
  completed_trimmed="$(trim "$COMPLETED_DATE")"

  if [[ -z "$owner_trimmed" ]]; then
    echo "--owner is required when --status done" >&2
    exit 1
  fi
  if [[ -z "$evidence_trimmed" ]]; then
    echo "--evidence is required when --status done" >&2
    exit 1
  fi
  if [[ -z "$completed_trimmed" ]]; then
    echo "--completed-date is required when --status done" >&2
    exit 1
  fi
  if [[ ! "$completed_trimmed" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}([T][0-9]{2}:[0-9]{2}:[0-9]{2}Z)?$ ]]; then
    echo "Invalid --completed-date format: $completed_trimmed" >&2
    echo "Expected YYYY-MM-DD or YYYY-MM-DDTHH:MM:SSZ" >&2
    exit 1
  fi

  evidence_entries="${evidence_trimmed//;/,}"
  IFS=',' read -r -a evidence_refs <<< "$evidence_entries"
  for evidence_ref in "${evidence_refs[@]}"; do
    evidence_ref="$(trim "$evidence_ref")"
    if [[ -z "$evidence_ref" ]]; then
      continue
    fi
    if ! is_valid_evidence_ref "$evidence_ref"; then
      echo "Invalid evidence reference: $evidence_ref" >&2
      echo "Use resolvable repo paths or http(s) URLs." >&2
      exit 1
    fi
  done
}

if [[ ! -f "$CHECKLIST_FILE" ]]; then
  echo "Checklist file not found: $CHECKLIST_FILE" >&2
  exit 1
fi
if [[ ! "$GATE_ID" =~ ^[0-9]+$ ]]; then
  echo "--gate must be a non-negative integer: $GATE_ID" >&2
  exit 1
fi

STATUS="$(echo "$STATUS" | tr '[:upper:]' '[:lower:]')"
if [[ "$STATUS" != "todo" && "$STATUS" != "done" ]]; then
  echo "--status must be todo or done: $STATUS" >&2
  exit 1
fi

if ! command -v awk >/dev/null 2>&1; then
  echo "Required command not found: awk" >&2
  exit 1
fi
if ! command -v mktemp >/dev/null 2>&1; then
  echo "Required command not found: mktemp" >&2
  exit 1
fi

if [[ "$STATUS" == "done" ]]; then
  validate_done_inputs
else
  if [[ -n "$(trim "$OWNER")" || -n "$(trim "$EVIDENCE")" || -n "$(trim "$COMPLETED_DATE")" ]]; then
    echo "--owner/--evidence/--completed-date are only valid with --status done" >&2
    exit 1
  fi
fi

tmp_file="$(mktemp "${TMPDIR:-/tmp}/go-no-go-gate-update.XXXXXX")"

set +e
awk \
  -F'|' \
  -v gate_id="$GATE_ID" \
  -v status="$STATUS" \
  -v owner_value="$OWNER" \
  -v evidence_value="$EVIDENCE" \
  -v completed_value="$COMPLETED_DATE" \
  '
    BEGIN {
      updated = 0
    }
    $0 ~ /^\|[[:space:]]*[0-9]+[[:space:]]*\|/ {
      row_id = $2
      gate_name = $3
      gsub(/^[ \t]+|[ \t]+$/, "", row_id)
      gsub(/^[ \t]+|[ \t]+$/, "", gate_name)
      if (row_id == gate_id) {
        if (status == "done") {
          printf "| %s | %s | %s | done | %s | %s |\n", row_id, gate_name, owner_value, evidence_value, completed_value
        } else {
          printf "| %s | %s |  | todo |  |  |\n", row_id, gate_name
        }
        updated = 1
        next
      }
    }
    {
      print
    }
    END {
      if (updated == 0) {
        exit 3
      }
    }
  ' "$CHECKLIST_FILE" > "$tmp_file"
awk_exit_code=$?
set -e

if [[ "$awk_exit_code" -eq 3 ]]; then
  rm -f "$tmp_file"
  echo "Gate id not found in checklist: $GATE_ID" >&2
  exit 1
fi
if [[ "$awk_exit_code" -ne 0 ]]; then
  rm -f "$tmp_file"
  echo "Failed to update checklist row for gate id: $GATE_ID" >&2
  exit "$awk_exit_code"
fi

mv "$tmp_file" "$CHECKLIST_FILE"

if [[ "$STATUS" == "done" ]]; then
  echo "Updated gate ${GATE_ID} to done:"
  echo "  owner=${OWNER}"
  echo "  evidence=${EVIDENCE}"
  echo "  completed_date=${COMPLETED_DATE}"
else
  echo "Updated gate ${GATE_ID} to todo (owner/evidence/completed date cleared)."
fi
