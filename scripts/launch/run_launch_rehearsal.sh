#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/launch/run_launch_rehearsal.sh [--mode smoke|full] [--include-fuzz] [--require-go] [--allow-dirty] [--checklist-file <path>] [--out-dir <path>]

Options:
  --mode <smoke|full>  Rehearsal mode passed to evidence generator. Default: full.
  --include-fuzz       Include fuzz smoke in readiness execution.
  --require-go         Enforce strict GO criteria from checklist.
  --allow-dirty        Allow running on dirty worktree (default: fail).
  --checklist-file     Checklist file path. Default: docs/mainnet-go-no-go-checklist.md
  --out-dir <path>     Rehearsal output root. Default: artifacts/launch-rehearsals
  -h, --help           Show this help text.
EOF
}

MODE="full"
INCLUDE_FUZZ=0
REQUIRE_GO=0
ALLOW_DIRTY=0
CHECKLIST_FILE="docs/mainnet-go-no-go-checklist.md"
OUT_ROOT="artifacts/launch-rehearsals"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      if [[ $# -lt 2 ]]; then
        echo "--mode requires a value (smoke|full)" >&2
        exit 1
      fi
      MODE="$2"
      shift 2
      ;;
    --include-fuzz)
      INCLUDE_FUZZ=1
      shift
      ;;
    --require-go)
      REQUIRE_GO=1
      shift
      ;;
    --allow-dirty)
      ALLOW_DIRTY=1
      shift
      ;;
    --checklist-file)
      if [[ $# -lt 2 ]]; then
        echo "--checklist-file requires a path value" >&2
        exit 1
      fi
      CHECKLIST_FILE="$2"
      shift 2
      ;;
    --out-dir)
      if [[ $# -lt 2 ]]; then
        echo "--out-dir requires a path value" >&2
        exit 1
      fi
      OUT_ROOT="$2"
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

if [[ "$MODE" != "smoke" && "$MODE" != "full" ]]; then
  echo "Invalid --mode value: $MODE (expected smoke or full)" >&2
  exit 1
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

require_cmd git
require_cmd jq
require_cmd find

if [[ ! -f "$CHECKLIST_FILE" ]]; then
  echo "Checklist file not found: $CHECKLIST_FILE" >&2
  exit 1
fi

timestamp="$(date -u +"%Y%m%dT%H%M%SZ")"
iso_timestamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
commit_sha="$(git rev-parse HEAD)"
short_sha="$(git rev-parse --short HEAD)"
branch_name="$(git rev-parse --abbrev-ref HEAD)"

rehearsal_dir="${OUT_ROOT}/${timestamp}-${short_sha}"
evidence_root="${rehearsal_dir}/evidence"
rehearsal_log="${rehearsal_dir}/rehearsal.log"
summary_json="${rehearsal_dir}/summary.json"
summary_md="${rehearsal_dir}/SUMMARY.md"

mkdir -p "$rehearsal_dir"

evidence_cmd=(
  ./scripts/launch/generate_evidence_bundle.sh
  --mode "$MODE"
  --checklist-file "$CHECKLIST_FILE"
  --out-dir "$evidence_root"
)
if [[ "$INCLUDE_FUZZ" -eq 1 ]]; then
  evidence_cmd+=(--include-fuzz)
fi
if [[ "$REQUIRE_GO" -eq 1 ]]; then
  evidence_cmd+=(--require-go)
fi
if [[ "$ALLOW_DIRTY" -eq 1 ]]; then
  evidence_cmd+=(--allow-dirty)
fi

echo "Starting launch rehearsal: ${rehearsal_dir}"
echo "Running: ${evidence_cmd[*]}"

set +e
"${evidence_cmd[@]}" 2>&1 | tee "$rehearsal_log"
rehearsal_exit_code=${PIPESTATUS[0]}
set -e

bundle_dir=""
if [[ -d "$evidence_root" ]]; then
  bundle_dir="$(find "$evidence_root" -mindepth 1 -maxdepth 1 -type d | sort | tail -n 1)"
fi
if [[ -z "$bundle_dir" ]]; then
  bundle_dir="$(awk -F': ' '/Launch evidence bundle complete: /{print $2}' "$rehearsal_log" | tail -n 1)"
fi

if [[ -z "$bundle_dir" || ! -d "$bundle_dir" ]]; then
  echo "Failed to resolve evidence bundle directory from rehearsal run." >&2
  echo "Rehearsal log: $rehearsal_log" >&2
  exit "${rehearsal_exit_code:-1}"
fi

metadata_json="${bundle_dir}/metadata.json"
checklist_report_json="${bundle_dir}/go-no-go-checklist-report.json"

gate_status="unknown"
gate_exit_code=-1
checklist_status="unknown"
checklist_exit_code=-1
checklist_todo=-1
checklist_invalid=-1
checklist_missing_signoff=-1

if [[ -f "$metadata_json" ]]; then
  gate_status="$(jq -r '.readiness_gate.status // "unknown"' "$metadata_json")"
  gate_exit_code="$(jq -r '.readiness_gate.exit_code // -1' "$metadata_json")"
  checklist_status="$(jq -r '.checklist.status // "unknown"' "$metadata_json")"
  checklist_exit_code="$(jq -r '.checklist.exit_code // -1' "$metadata_json")"
fi
if [[ -f "$checklist_report_json" ]]; then
  checklist_todo="$(jq -r '.totals.todo // -1' "$checklist_report_json")"
  checklist_invalid="$(jq -r '.totals.invalid // -1' "$checklist_report_json")"
  checklist_missing_signoff="$(jq -r '.totals.missing_signoff_fields // -1' "$checklist_report_json")"
fi

go_ready=false
if [[ "$gate_status" == "passed" && "$checklist_todo" -eq 0 && "$checklist_invalid" -eq 0 && "$checklist_missing_signoff" -eq 0 ]]; then
  go_ready=true
fi

overall_status="failed"
if [[ "$rehearsal_exit_code" -eq 0 ]]; then
  overall_status="passed"
fi

jq -n \
  --arg generated_at "$iso_timestamp" \
  --arg rehearsal_dir "$rehearsal_dir" \
  --arg evidence_bundle_dir "$bundle_dir" \
  --arg log_file "$rehearsal_log" \
  --arg commit_sha "$commit_sha" \
  --arg short_sha "$short_sha" \
  --arg branch "$branch_name" \
  --arg mode "$MODE" \
  --arg checklist_file "$CHECKLIST_FILE" \
  --arg overall_status "$overall_status" \
  --arg gate_status "$gate_status" \
  --arg checklist_status "$checklist_status" \
  --argjson rehearsal_exit_code "$rehearsal_exit_code" \
  --argjson gate_exit_code "$gate_exit_code" \
  --argjson checklist_exit_code "$checklist_exit_code" \
  --argjson include_fuzz "$INCLUDE_FUZZ" \
  --argjson require_go "$REQUIRE_GO" \
  --argjson allow_dirty "$ALLOW_DIRTY" \
  --argjson checklist_todo "$checklist_todo" \
  --argjson checklist_invalid "$checklist_invalid" \
  --argjson checklist_missing_signoff "$checklist_missing_signoff" \
  --argjson go_ready "$go_ready" \
  '{
    generated_at,
    rehearsal_dir,
    evidence_bundle_dir,
    log_file,
    git: {
      commit_sha,
      short_sha,
      branch
    },
    execution: {
      mode,
      include_fuzz,
      require_go,
      allow_dirty,
      checklist_file
    },
    result: {
      overall_status,
      rehearsal_exit_code,
      gate_status,
      gate_exit_code,
      checklist_status,
      checklist_exit_code,
      checklist_todo,
      checklist_invalid,
      checklist_missing_signoff,
      go_ready
    }
  }' > "$summary_json"

cat > "$summary_md" <<EOF
# Launch Rehearsal Summary

- generated_at: ${iso_timestamp}
- rehearsal_dir: ${rehearsal_dir}
- evidence_bundle_dir: ${bundle_dir}
- commit: ${commit_sha}
- branch: ${branch_name}
- mode: ${MODE}
- include_fuzz: ${INCLUDE_FUZZ}
- require_go: ${REQUIRE_GO}
- allow_dirty: ${ALLOW_DIRTY}
- checklist_file: ${CHECKLIST_FILE}
- overall_status: ${overall_status}
- rehearsal_exit_code: ${rehearsal_exit_code}
- gate_status: ${gate_status}
- gate_exit_code: ${gate_exit_code}
- checklist_status: ${checklist_status}
- checklist_exit_code: ${checklist_exit_code}
- checklist_todo: ${checklist_todo}
- checklist_invalid: ${checklist_invalid}
- checklist_missing_signoff: ${checklist_missing_signoff}
- go_ready: ${go_ready}

## Artifacts

- rehearsal.log
- summary.json
- evidence/...
EOF

echo
echo "Launch rehearsal complete: ${rehearsal_dir}"
echo "Summary: ${summary_md}"

if [[ "$rehearsal_exit_code" -ne 0 ]]; then
  exit "$rehearsal_exit_code"
fi
