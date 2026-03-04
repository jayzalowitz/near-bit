#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/launch/run_launch_rehearsal.sh [--mode smoke|full] [--include-fuzz] [--check-nightly-fuzz-health] [--nightly-fuzz-branch <name>] [--nightly-fuzz-workflow <name>] [--nightly-fuzz-window-days <n>] [--nightly-fuzz-min-runs <n>] [--nightly-fuzz-max-runs <n>] [--nightly-fuzz-allow-in-progress] [--skip-issue1-goal-checks] [--require-go] [--include-release-manifest|--skip-release-manifest] [--release-manifest-skip-build] [--operator <name>] [--allow-dirty] [--checklist-file <path>] [--out-dir <path>]

Options:
  --mode <smoke|full>  Rehearsal mode passed to evidence generator. Default: full.
  --include-fuzz       Include fuzz smoke in readiness execution.
  --check-nightly-fuzz-health  Enforce 7-day nightly fuzz health gate.
  --nightly-fuzz-branch <name> Branch used by nightly fuzz health check. Default: main.
  --nightly-fuzz-workflow <name> Workflow name used by nightly fuzz health check. Default: Nightly Fuzz.
  --nightly-fuzz-window-days <n> Lookback window in days for nightly fuzz health. Default: 7.
  --nightly-fuzz-min-runs <n> Minimum required runs in lookback window. Default: 1.
  --nightly-fuzz-max-runs <n> Max runs fetched from GitHub API. Default: 200.
  --nightly-fuzz-allow-in-progress Do not fail when in-progress runs are present.
  --skip-issue1-goal-checks Skip targeted Issue #1 goal validation tests in readiness gate.
  --require-go         Enforce strict GO criteria from checklist.
  --include-release-manifest  Generate release artifact manifest during rehearsal.
                              Default: enabled for --mode full, disabled for --mode smoke.
  --skip-release-manifest     Skip release artifact manifest generation.
  --release-manifest-skip-build  Skip release build when generating manifest.
  --operator <name>    Operator/signoff owner for rehearsal metadata.
                       Default: git user.name, else $USER, else "unknown".
  --allow-dirty        Allow running on dirty worktree (default: fail).
  --checklist-file     Checklist file path. Default: docs/mainnet-go-no-go-checklist.md
  --out-dir <path>     Rehearsal output root. Default: artifacts/launch-rehearsals
  -h, --help           Show this help text.
EOF
}

MODE="full"
INCLUDE_FUZZ=0
CHECK_NIGHTLY_FUZZ_HEALTH=0
NIGHTLY_FUZZ_BRANCH="main"
NIGHTLY_FUZZ_WORKFLOW="Nightly Fuzz"
NIGHTLY_FUZZ_WINDOW_DAYS=7
NIGHTLY_FUZZ_MIN_RUNS=1
NIGHTLY_FUZZ_MAX_RUNS=200
NIGHTLY_FUZZ_ALLOW_IN_PROGRESS=0
SKIP_ISSUE1_GOAL_CHECKS=0
REQUIRE_GO=0
INCLUDE_RELEASE_MANIFEST=-1
RELEASE_MANIFEST_SKIP_BUILD=0
OPERATOR=""
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
    --check-nightly-fuzz-health)
      CHECK_NIGHTLY_FUZZ_HEALTH=1
      shift
      ;;
    --nightly-fuzz-branch)
      if [[ $# -lt 2 ]]; then
        echo "--nightly-fuzz-branch requires a value" >&2
        exit 1
      fi
      NIGHTLY_FUZZ_BRANCH="$2"
      shift 2
      ;;
    --nightly-fuzz-workflow)
      if [[ $# -lt 2 ]]; then
        echo "--nightly-fuzz-workflow requires a value" >&2
        exit 1
      fi
      NIGHTLY_FUZZ_WORKFLOW="$2"
      shift 2
      ;;
    --nightly-fuzz-window-days)
      if [[ $# -lt 2 ]]; then
        echo "--nightly-fuzz-window-days requires a numeric value" >&2
        exit 1
      fi
      NIGHTLY_FUZZ_WINDOW_DAYS="$2"
      shift 2
      ;;
    --nightly-fuzz-min-runs)
      if [[ $# -lt 2 ]]; then
        echo "--nightly-fuzz-min-runs requires a numeric value" >&2
        exit 1
      fi
      NIGHTLY_FUZZ_MIN_RUNS="$2"
      shift 2
      ;;
    --nightly-fuzz-max-runs)
      if [[ $# -lt 2 ]]; then
        echo "--nightly-fuzz-max-runs requires a numeric value" >&2
        exit 1
      fi
      NIGHTLY_FUZZ_MAX_RUNS="$2"
      shift 2
      ;;
    --nightly-fuzz-allow-in-progress)
      NIGHTLY_FUZZ_ALLOW_IN_PROGRESS=1
      shift
      ;;
    --skip-issue1-goal-checks)
      SKIP_ISSUE1_GOAL_CHECKS=1
      shift
      ;;
    --require-go)
      REQUIRE_GO=1
      shift
      ;;
    --include-release-manifest)
      INCLUDE_RELEASE_MANIFEST=1
      shift
      ;;
    --skip-release-manifest)
      INCLUDE_RELEASE_MANIFEST=0
      shift
      ;;
    --release-manifest-skip-build)
      RELEASE_MANIFEST_SKIP_BUILD=1
      shift
      ;;
    --operator)
      if [[ $# -lt 2 ]]; then
        echo "--operator requires a value" >&2
        exit 1
      fi
      OPERATOR="$2"
      shift 2
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

for num in "$NIGHTLY_FUZZ_WINDOW_DAYS" "$NIGHTLY_FUZZ_MIN_RUNS" "$NIGHTLY_FUZZ_MAX_RUNS"; do
  if [[ ! "$num" =~ ^[0-9]+$ ]]; then
    echo "Nightly fuzz numeric options must be non-negative integers." >&2
    exit 1
  fi
done

if [[ "$MODE" != "smoke" && "$MODE" != "full" ]]; then
  echo "Invalid --mode value: $MODE (expected smoke or full)" >&2
  exit 1
fi

if [[ "$INCLUDE_RELEASE_MANIFEST" -lt 0 ]]; then
  if [[ "$MODE" == "full" ]]; then
    INCLUDE_RELEASE_MANIFEST=1
  else
    INCLUDE_RELEASE_MANIFEST=0
  fi
fi

if [[ "$INCLUDE_RELEASE_MANIFEST" -eq 0 && "$RELEASE_MANIFEST_SKIP_BUILD" -eq 1 ]]; then
  echo "--release-manifest-skip-build cannot be used when release manifest generation is disabled." >&2
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
require_cmd mktemp

restore_tracked_target_changes() {
  local -a dirty_target_files=()
  local file=""

  while IFS= read -r file; do
    if [[ -n "$file" ]]; then
      dirty_target_files+=("$file")
    fi
  done < <(git diff --name-only -- target)

  if [[ "${#dirty_target_files[@]}" -eq 0 ]]; then
    return
  fi

  echo "Restoring tracked target/ files before strict release manifest run."
  git restore --worktree -- "${dirty_target_files[@]}"
}

if [[ ! -f "$CHECKLIST_FILE" ]]; then
  echo "Checklist file not found: $CHECKLIST_FILE" >&2
  exit 1
fi

if [[ -z "$OPERATOR" ]]; then
  OPERATOR="$(git config user.name 2>/dev/null || true)"
fi
if [[ -z "$OPERATOR" ]]; then
  OPERATOR="${USER:-unknown}"
fi
if [[ -z "${OPERATOR// }" ]]; then
  OPERATOR="unknown"
fi

timestamp="$(date -u +"%Y%m%dT%H%M%SZ")"
iso_timestamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
commit_sha="$(git rev-parse HEAD)"
short_sha="$(git rev-parse --short HEAD)"
branch_name="$(git rev-parse --abbrev-ref HEAD)"

rehearsal_dir="${OUT_ROOT}/${timestamp}-${short_sha}"
evidence_root="${rehearsal_dir}/evidence"
rehearsal_log="${rehearsal_dir}/rehearsal.log"
release_manifest_root="${rehearsal_dir}/release-manifests"
release_manifest_log="${rehearsal_dir}/release-manifest.log"
summary_json="${rehearsal_dir}/summary.json"
summary_md="${rehearsal_dir}/SUMMARY.md"

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/launch-rehearsal.XXXXXX")"
tmp_evidence_root="${tmp_root}/evidence"
tmp_rehearsal_log="${tmp_root}/rehearsal.log"
tmp_release_manifest_root="${tmp_root}/release-manifests"
tmp_release_manifest_log="${tmp_root}/release-manifest.log"

evidence_cmd=(
  ./scripts/launch/generate_evidence_bundle.sh
  --mode "$MODE"
  --checklist-file "$CHECKLIST_FILE"
  --out-dir "$tmp_evidence_root"
)
if [[ "$INCLUDE_FUZZ" -eq 1 ]]; then
  evidence_cmd+=(--include-fuzz)
fi
if [[ "$CHECK_NIGHTLY_FUZZ_HEALTH" -eq 1 ]]; then
  evidence_cmd+=(
    --check-nightly-fuzz-health
    --nightly-fuzz-branch "$NIGHTLY_FUZZ_BRANCH"
    --nightly-fuzz-workflow "$NIGHTLY_FUZZ_WORKFLOW"
    --nightly-fuzz-window-days "$NIGHTLY_FUZZ_WINDOW_DAYS"
    --nightly-fuzz-min-runs "$NIGHTLY_FUZZ_MIN_RUNS"
    --nightly-fuzz-max-runs "$NIGHTLY_FUZZ_MAX_RUNS"
  )
  if [[ "$NIGHTLY_FUZZ_ALLOW_IN_PROGRESS" -eq 1 ]]; then
    evidence_cmd+=(--nightly-fuzz-allow-in-progress)
  fi
fi
if [[ "$SKIP_ISSUE1_GOAL_CHECKS" -eq 1 ]]; then
  evidence_cmd+=(--skip-issue1-goal-checks)
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
"${evidence_cmd[@]}" 2>&1 | tee "$tmp_rehearsal_log"
rehearsal_exit_code=${PIPESTATUS[0]}
set -e

bundle_dir=""
if [[ -d "$tmp_evidence_root" ]]; then
  bundle_dir="$(find "$tmp_evidence_root" -mindepth 1 -maxdepth 1 -type d | sort | tail -n 1)"
fi
if [[ -z "$bundle_dir" ]]; then
  bundle_dir="$(awk -F': ' '/Launch evidence bundle complete: /{print $2}' "$tmp_rehearsal_log" | tail -n 1)"
fi

if [[ -z "$bundle_dir" || ! -d "$bundle_dir" ]]; then
  mkdir -p "$rehearsal_dir"
  if [[ -f "$tmp_rehearsal_log" ]]; then
    cp "$tmp_rehearsal_log" "$rehearsal_log"
  fi
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
release_manifest_status="skipped"
release_manifest_exit_code=0
release_manifest_dir=""

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

if [[ "$INCLUDE_RELEASE_MANIFEST" -eq 1 ]]; then
  if [[ "$ALLOW_DIRTY" -eq 0 ]]; then
    restore_tracked_target_changes
  fi

  release_manifest_cmd=(
    ./scripts/launch/generate_release_manifest.sh
    --out-dir "$tmp_release_manifest_root"
  )
  if [[ "$RELEASE_MANIFEST_SKIP_BUILD" -eq 1 ]]; then
    release_manifest_cmd+=(--skip-build)
  fi
  if [[ "$ALLOW_DIRTY" -eq 1 ]]; then
    release_manifest_cmd+=(--allow-dirty)
  fi

  echo "Running release manifest: ${release_manifest_cmd[*]}"

  set +e
  "${release_manifest_cmd[@]}" 2>&1 | tee "$tmp_release_manifest_log"
  release_manifest_exit_code=${PIPESTATUS[0]}
  set -e

  if [[ "$release_manifest_exit_code" -eq 0 ]]; then
    release_manifest_status="passed"
  else
    release_manifest_status="failed"
  fi

  if [[ -d "$tmp_release_manifest_root" ]]; then
    release_manifest_dir="$(find "$tmp_release_manifest_root" -mindepth 1 -maxdepth 1 -type d | sort | tail -n 1)"
  fi
  if [[ -z "$release_manifest_dir" && -f "$tmp_release_manifest_log" ]]; then
    release_manifest_dir="$(awk -F': ' '/Release artifact manifest complete: /{print $2}' "$tmp_release_manifest_log" | tail -n 1)"
  fi
fi

go_ready=false
if [[ "$gate_status" == "passed" && "$checklist_todo" -eq 0 && "$checklist_invalid" -eq 0 && "$checklist_missing_signoff" -eq 0 ]]; then
  go_ready=true
fi

overall_status="passed"
overall_exit_code=0
if [[ "$rehearsal_exit_code" -ne 0 ]]; then
  overall_status="failed"
  overall_exit_code="$rehearsal_exit_code"
fi
if [[ "$INCLUDE_RELEASE_MANIFEST" -eq 1 && "$release_manifest_exit_code" -ne 0 ]]; then
  overall_status="failed"
  if [[ "$overall_exit_code" -eq 0 ]]; then
    overall_exit_code="$release_manifest_exit_code"
  fi
fi

mkdir -p "$evidence_root"
cp -R "$tmp_evidence_root"/. "$evidence_root"/
cp "$tmp_rehearsal_log" "$rehearsal_log"
bundle_dir="${evidence_root}/$(basename "$bundle_dir")"

if [[ "$INCLUDE_RELEASE_MANIFEST" -eq 1 ]]; then
  mkdir -p "$release_manifest_root"
  if [[ -d "$tmp_release_manifest_root" ]]; then
    cp -R "$tmp_release_manifest_root"/. "$release_manifest_root"/
  fi
  if [[ -f "$tmp_release_manifest_log" ]]; then
    cp "$tmp_release_manifest_log" "$release_manifest_log"
  fi
  if [[ -n "$release_manifest_dir" ]]; then
    release_manifest_dir="${release_manifest_root}/$(basename "$release_manifest_dir")"
  fi
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
  --arg operator "$OPERATOR" \
  --arg nightly_fuzz_branch "$NIGHTLY_FUZZ_BRANCH" \
  --arg nightly_fuzz_workflow "$NIGHTLY_FUZZ_WORKFLOW" \
  --arg checklist_file "$CHECKLIST_FILE" \
  --arg overall_status "$overall_status" \
  --arg gate_status "$gate_status" \
  --arg checklist_status "$checklist_status" \
  --argjson rehearsal_exit_code "$rehearsal_exit_code" \
  --argjson gate_exit_code "$gate_exit_code" \
  --argjson checklist_exit_code "$checklist_exit_code" \
  --argjson include_fuzz "$INCLUDE_FUZZ" \
  --argjson check_nightly_fuzz_health "$CHECK_NIGHTLY_FUZZ_HEALTH" \
  --argjson nightly_fuzz_window_days "$NIGHTLY_FUZZ_WINDOW_DAYS" \
  --argjson nightly_fuzz_min_runs "$NIGHTLY_FUZZ_MIN_RUNS" \
  --argjson nightly_fuzz_max_runs "$NIGHTLY_FUZZ_MAX_RUNS" \
  --argjson nightly_fuzz_allow_in_progress "$NIGHTLY_FUZZ_ALLOW_IN_PROGRESS" \
  --argjson skip_issue1_goal_checks "$SKIP_ISSUE1_GOAL_CHECKS" \
  --argjson require_go "$REQUIRE_GO" \
  --argjson include_release_manifest "$INCLUDE_RELEASE_MANIFEST" \
  --argjson release_manifest_skip_build "$RELEASE_MANIFEST_SKIP_BUILD" \
  --argjson allow_dirty "$ALLOW_DIRTY" \
  --argjson checklist_todo "$checklist_todo" \
  --argjson checklist_invalid "$checklist_invalid" \
  --argjson checklist_missing_signoff "$checklist_missing_signoff" \
  --arg release_manifest_status "$release_manifest_status" \
  --argjson release_manifest_exit_code "$release_manifest_exit_code" \
  --arg release_manifest_dir "$release_manifest_dir" \
  --arg release_manifest_log "$release_manifest_log" \
  --argjson go_ready "$go_ready" \
  '{
    generated_at: $generated_at,
    rehearsal_dir: $rehearsal_dir,
    evidence_bundle_dir: $evidence_bundle_dir,
    log_file: $log_file,
    git: {
      commit_sha: $commit_sha,
      short_sha: $short_sha,
      branch: $branch
    },
    execution: {
      mode: $mode,
      operator: $operator,
      include_fuzz: $include_fuzz,
      check_nightly_fuzz_health: $check_nightly_fuzz_health,
      nightly_fuzz_branch: $nightly_fuzz_branch,
      nightly_fuzz_workflow: $nightly_fuzz_workflow,
      nightly_fuzz_window_days: $nightly_fuzz_window_days,
      nightly_fuzz_min_runs: $nightly_fuzz_min_runs,
      nightly_fuzz_max_runs: $nightly_fuzz_max_runs,
      nightly_fuzz_allow_in_progress: ($nightly_fuzz_allow_in_progress == 1),
      skip_issue1_goal_checks: ($skip_issue1_goal_checks == 1),
      require_go: $require_go,
      include_release_manifest: $include_release_manifest,
      release_manifest_skip_build: $release_manifest_skip_build,
      allow_dirty: $allow_dirty,
      checklist_file: $checklist_file
    },
    release_manifest: {
      status: $release_manifest_status,
      exit_code: $release_manifest_exit_code,
      manifest_dir: $release_manifest_dir,
      log_file: $release_manifest_log
    },
    result: {
      overall_status: $overall_status,
      rehearsal_exit_code: $rehearsal_exit_code,
      gate_status: $gate_status,
      gate_exit_code: $gate_exit_code,
      checklist_status: $checklist_status,
      checklist_exit_code: $checklist_exit_code,
      checklist_todo: $checklist_todo,
      checklist_invalid: $checklist_invalid,
      checklist_missing_signoff: $checklist_missing_signoff,
      go_ready: $go_ready
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
- operator: ${OPERATOR}
- include_fuzz: ${INCLUDE_FUZZ}
- check_nightly_fuzz_health: ${CHECK_NIGHTLY_FUZZ_HEALTH}
- nightly_fuzz_branch: ${NIGHTLY_FUZZ_BRANCH}
- nightly_fuzz_workflow: ${NIGHTLY_FUZZ_WORKFLOW}
- nightly_fuzz_window_days: ${NIGHTLY_FUZZ_WINDOW_DAYS}
- nightly_fuzz_min_runs: ${NIGHTLY_FUZZ_MIN_RUNS}
- nightly_fuzz_max_runs: ${NIGHTLY_FUZZ_MAX_RUNS}
- nightly_fuzz_allow_in_progress: ${NIGHTLY_FUZZ_ALLOW_IN_PROGRESS}
- skip_issue1_goal_checks: ${SKIP_ISSUE1_GOAL_CHECKS}
- require_go: ${REQUIRE_GO}
- include_release_manifest: ${INCLUDE_RELEASE_MANIFEST}
- release_manifest_skip_build: ${RELEASE_MANIFEST_SKIP_BUILD}
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
- release_manifest_status: ${release_manifest_status}
- release_manifest_exit_code: ${release_manifest_exit_code}
- release_manifest_dir: ${release_manifest_dir}
- go_ready: ${go_ready}

## Artifacts

- rehearsal.log
- summary.json
- evidence/...
EOF

if [[ "$INCLUDE_RELEASE_MANIFEST" -eq 1 ]]; then
  {
    echo "- release-manifest.log"
    echo "- release-manifests/..."
  } >> "$summary_md"
fi

echo
echo "Launch rehearsal complete: ${rehearsal_dir}"
echo "Summary: ${summary_md}"

if [[ "$overall_exit_code" -ne 0 ]]; then
  exit "$overall_exit_code"
fi
