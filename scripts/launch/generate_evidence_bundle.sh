#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/launch/generate_evidence_bundle.sh [--mode smoke|full] [--include-fuzz] [--check-nightly-fuzz-health] [--nightly-fuzz-branch <name>] [--nightly-fuzz-workflow <name>] [--nightly-fuzz-window-days <n>] [--nightly-fuzz-min-runs <n>] [--nightly-fuzz-max-runs <n>] [--nightly-fuzz-job-pattern <regex>] [--nightly-fuzz-allow-in-progress] [--nightly-fuzz-fail-on-cancelled] [--check-snapshot-supply] [--snapshot-genesis <path>] [--snapshot-txoutsetinfo <path>] [--snapshot-tolerance-sats <n>] [--snapshot-json-out <path>] [--cargo-target-dir <path>] [--skip-issue1-goal-checks] [--skip-gate] [--require-go] [--checklist-file <path>] [--out-dir <path>]

Options:
  --mode <smoke|full>  Readiness gate mode to run. Default: full.
  --include-fuzz       Pass --include-fuzz to readiness gate.
  --check-nightly-fuzz-health  Enforce 7-day nightly fuzz health check in readiness gate.
  --nightly-fuzz-branch <name> Branch used by nightly fuzz health check. Default: main.
  --nightly-fuzz-workflow <name> Workflow name used by nightly fuzz health check. Default: Nightly Fuzz.
  --nightly-fuzz-window-days <n> Lookback window in days for nightly fuzz health. Default: 7.
  --nightly-fuzz-min-runs <n> Minimum required runs in lookback window. Default: 1.
  --nightly-fuzz-max-runs <n> Max runs fetched from GitHub API. Default: 200.
  --nightly-fuzz-job-pattern <regex> Evaluate only matching fuzz jobs (case-insensitive).
  --nightly-fuzz-allow-in-progress Do not fail when in-progress runs are present.
  --nightly-fuzz-fail-on-cancelled Treat cancelled runs/jobs as failures.
  --check-snapshot-supply Enforce gate #10 snapshot-vs-genesis reconciliation in readiness.
  --snapshot-genesis <path> Path to genesis.json used for snapshot reconciliation.
  --snapshot-txoutsetinfo <path> Path to bitcoin-cli gettxoutsetinfo JSON.
  --snapshot-tolerance-sats <n> Allowed satoshi diff for snapshot check. Default: 1.
  --snapshot-json-out <path> Copy snapshot-check JSON summary to this path.
  --cargo-target-dir <path> Cargo target directory for readiness/manifest commands.
                            Default: .context/cargo-target locally, target in CI.
  --skip-issue1-goal-checks Skip targeted Issue #1 goal validation tests in readiness gate.
  --skip-gate          Skip executing readiness gate and only snapshot metadata/docs.
  --require-go         Enforce GO criteria in checklist validation.
  --checklist-file     Checklist file path. Default: docs/mainnet-go-no-go-checklist.md
  --allow-dirty        Allow running on a dirty worktree (default: fail if dirty).
  --out-dir <path>     Output root for bundles. Default: artifacts/launch-readiness.
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
NIGHTLY_FUZZ_JOB_PATTERN=""
NIGHTLY_FUZZ_ALLOW_IN_PROGRESS=0
NIGHTLY_FUZZ_FAIL_ON_CANCELLED=0
CHECK_SNAPSHOT_SUPPLY=0
SNAPSHOT_GENESIS=""
SNAPSHOT_TXOUTSETINFO=""
SNAPSHOT_TOLERANCE_SATS=1
SNAPSHOT_JSON_OUT=""
CARGO_TARGET_DIR_OVERRIDE=""
SKIP_ISSUE1_GOAL_CHECKS=0
SKIP_GATE=0
REQUIRE_GO=0
ALLOW_DIRTY=0
OUT_ROOT="artifacts/launch-readiness"
CHECKLIST_FILE="docs/mainnet-go-no-go-checklist.md"

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
    --nightly-fuzz-job-pattern)
      if [[ $# -lt 2 ]]; then
        echo "--nightly-fuzz-job-pattern requires a value" >&2
        exit 1
      fi
      NIGHTLY_FUZZ_JOB_PATTERN="$2"
      shift 2
      ;;
    --nightly-fuzz-allow-in-progress)
      NIGHTLY_FUZZ_ALLOW_IN_PROGRESS=1
      shift
      ;;
    --nightly-fuzz-fail-on-cancelled)
      NIGHTLY_FUZZ_FAIL_ON_CANCELLED=1
      shift
      ;;
    --check-snapshot-supply)
      CHECK_SNAPSHOT_SUPPLY=1
      shift
      ;;
    --snapshot-genesis)
      if [[ $# -lt 2 ]]; then
        echo "--snapshot-genesis requires a path value" >&2
        exit 1
      fi
      SNAPSHOT_GENESIS="$2"
      shift 2
      ;;
    --snapshot-txoutsetinfo)
      if [[ $# -lt 2 ]]; then
        echo "--snapshot-txoutsetinfo requires a path value" >&2
        exit 1
      fi
      SNAPSHOT_TXOUTSETINFO="$2"
      shift 2
      ;;
    --snapshot-tolerance-sats)
      if [[ $# -lt 2 ]]; then
        echo "--snapshot-tolerance-sats requires a numeric value" >&2
        exit 1
      fi
      SNAPSHOT_TOLERANCE_SATS="$2"
      shift 2
      ;;
    --snapshot-json-out)
      if [[ $# -lt 2 ]]; then
        echo "--snapshot-json-out requires a path value" >&2
        exit 1
      fi
      SNAPSHOT_JSON_OUT="$2"
      shift 2
      ;;
    --cargo-target-dir)
      if [[ $# -lt 2 ]]; then
        echo "--cargo-target-dir requires a path value" >&2
        exit 1
      fi
      CARGO_TARGET_DIR_OVERRIDE="$2"
      shift 2
      ;;
    --skip-issue1-goal-checks)
      SKIP_ISSUE1_GOAL_CHECKS=1
      shift
      ;;
    --skip-gate)
      SKIP_GATE=1
      shift
      ;;
    --require-go)
      REQUIRE_GO=1
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
    --allow-dirty)
      ALLOW_DIRTY=1
      shift
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

if [[ ! "$SNAPSHOT_TOLERANCE_SATS" =~ ^[0-9]+$ ]]; then
  echo "--snapshot-tolerance-sats must be a non-negative integer." >&2
  exit 1
fi

if [[ "$MODE" != "smoke" && "$MODE" != "full" ]]; then
  echo "Invalid --mode value: $MODE (expected smoke or full)" >&2
  exit 1
fi

if [[ ! -f "$CHECKLIST_FILE" ]]; then
  echo "Checklist file not found: $CHECKLIST_FILE" >&2
  exit 1
fi

if [[ "$CHECK_SNAPSHOT_SUPPLY" -eq 1 ]]; then
  if [[ -z "$SNAPSHOT_GENESIS" || -z "$SNAPSHOT_TXOUTSETINFO" ]]; then
    echo "--check-snapshot-supply requires --snapshot-genesis and --snapshot-txoutsetinfo." >&2
    exit 1
  fi
  if [[ ! -f "$SNAPSHOT_GENESIS" ]]; then
    echo "Snapshot-check genesis file not found: $SNAPSHOT_GENESIS" >&2
    exit 1
  fi
  if [[ ! -f "$SNAPSHOT_TXOUTSETINFO" ]]; then
    echo "Snapshot-check txoutsetinfo file not found: $SNAPSHOT_TXOUTSETINFO" >&2
    exit 1
  fi
elif [[ -n "$SNAPSHOT_GENESIS" || -n "$SNAPSHOT_TXOUTSETINFO" || -n "$SNAPSHOT_JSON_OUT" ]]; then
  echo "Snapshot parameters provided without --check-snapshot-supply." >&2
  echo "Use --check-snapshot-supply to enforce gate #10 snapshot reconciliation." >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

resolve_cargo_target_dir() {
  if [[ -n "$CARGO_TARGET_DIR_OVERRIDE" ]]; then
    echo "$CARGO_TARGET_DIR_OVERRIDE"
    return
  fi
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    echo "$CARGO_TARGET_DIR"
    return
  fi
  if [[ "${CI:-}" == "true" ]]; then
    echo "$ROOT_DIR/target"
    return
  fi
  echo "$ROOT_DIR/.context/cargo-target"
}

CARGO_TARGET_DIR="$(resolve_cargo_target_dir)"
export CARGO_TARGET_DIR
mkdir -p "$CARGO_TARGET_DIR"

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Required command not found: $cmd" >&2
    exit 1
  fi
}

require_cmd git
require_cmd jq
require_cmd shasum
require_cmd rustc
require_cmd cargo
require_cmd bash

timestamp="$(date -u +"%Y%m%dT%H%M%SZ")"
iso_timestamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
commit_sha="$(git rev-parse HEAD)"
short_sha="$(git rev-parse --short HEAD)"
branch_name="$(git rev-parse --abbrev-ref HEAD)"
worktree_status="$(git status --porcelain)"

if [[ -n "$worktree_status" ]]; then
  worktree_dirty=true
else
  worktree_dirty=false
fi

if [[ "$worktree_dirty" == true && "$ALLOW_DIRTY" -eq 0 ]]; then
  echo "Refusing to generate launch evidence on a dirty worktree." >&2
  echo "Commit/stash changes or pass --allow-dirty for non-signoff runs." >&2
  exit 1
fi

bundle_dir="${OUT_ROOT}/${timestamp}-${short_sha}"
mkdir -p "$bundle_dir"

echo "Writing launch evidence bundle to: $bundle_dir"

jq -n \
  --arg generated_at "$iso_timestamp" \
  --arg timestamp "$timestamp" \
  --arg commit_sha "$commit_sha" \
  --arg short_sha "$short_sha" \
  --arg branch "$branch_name" \
  --arg mode "$MODE" \
  --argjson include_fuzz "$INCLUDE_FUZZ" \
  --argjson check_nightly_fuzz_health "$CHECK_NIGHTLY_FUZZ_HEALTH" \
  --arg nightly_fuzz_branch "$NIGHTLY_FUZZ_BRANCH" \
  --arg nightly_fuzz_workflow "$NIGHTLY_FUZZ_WORKFLOW" \
  --argjson nightly_fuzz_window_days "$NIGHTLY_FUZZ_WINDOW_DAYS" \
  --argjson nightly_fuzz_min_runs "$NIGHTLY_FUZZ_MIN_RUNS" \
  --argjson nightly_fuzz_max_runs "$NIGHTLY_FUZZ_MAX_RUNS" \
  --arg nightly_fuzz_job_pattern "$NIGHTLY_FUZZ_JOB_PATTERN" \
  --argjson nightly_fuzz_allow_in_progress "$NIGHTLY_FUZZ_ALLOW_IN_PROGRESS" \
  --argjson nightly_fuzz_fail_on_cancelled "$NIGHTLY_FUZZ_FAIL_ON_CANCELLED" \
  --argjson check_snapshot_supply "$CHECK_SNAPSHOT_SUPPLY" \
  --arg snapshot_genesis "$SNAPSHOT_GENESIS" \
  --arg snapshot_txoutsetinfo "$SNAPSHOT_TXOUTSETINFO" \
  --argjson snapshot_tolerance_sats "$SNAPSHOT_TOLERANCE_SATS" \
  --arg cargo_target_dir "$CARGO_TARGET_DIR" \
  --argjson skip_issue1_goal_checks "$SKIP_ISSUE1_GOAL_CHECKS" \
  --argjson skip_gate "$SKIP_GATE" \
  --argjson require_go "$REQUIRE_GO" \
  --argjson allow_dirty "$ALLOW_DIRTY" \
  --argjson worktree_dirty "$worktree_dirty" \
  --arg checklist_file "$CHECKLIST_FILE" \
  --arg rustc_version "$(rustc --version)" \
  --arg cargo_version "$(cargo --version)" \
  '{
    generated_at: $generated_at,
    timestamp: $timestamp,
    git: {
      commit_sha: $commit_sha,
      short_sha: $short_sha,
      branch: $branch,
      worktree_dirty: $worktree_dirty
    },
    readiness_gate: {
      mode: $mode,
      include_fuzz: $include_fuzz,
      check_nightly_fuzz_health: $check_nightly_fuzz_health,
      nightly_fuzz_branch: $nightly_fuzz_branch,
      nightly_fuzz_workflow: $nightly_fuzz_workflow,
      nightly_fuzz_window_days: $nightly_fuzz_window_days,
      nightly_fuzz_min_runs: $nightly_fuzz_min_runs,
      nightly_fuzz_max_runs: $nightly_fuzz_max_runs,
      nightly_fuzz_job_pattern: (if $nightly_fuzz_job_pattern == "" then null else $nightly_fuzz_job_pattern end),
      nightly_fuzz_allow_in_progress: ($nightly_fuzz_allow_in_progress == 1),
      nightly_fuzz_fail_on_cancelled: ($nightly_fuzz_fail_on_cancelled == 1),
      check_snapshot_supply: ($check_snapshot_supply == 1),
      snapshot_genesis: (if $snapshot_genesis == "" then null else $snapshot_genesis end),
      snapshot_txoutsetinfo: (if $snapshot_txoutsetinfo == "" then null else $snapshot_txoutsetinfo end),
      snapshot_tolerance_sats: $snapshot_tolerance_sats,
      cargo_target_dir: $cargo_target_dir,
      skip_issue1_goal_checks: ($skip_issue1_goal_checks == 1),
      skipped: $skip_gate
    },
    checklist: {
      file: $checklist_file,
      require_go: $require_go
    },
    execution: {
      allow_dirty: $allow_dirty
    },
    toolchain: {
      rustc_version: $rustc_version,
      cargo_version: $cargo_version
    }
  }' > "${bundle_dir}/metadata.json"

{
  echo "$worktree_status"
} > "${bundle_dir}/git-status.txt"

git show --no-patch --pretty=fuller "$commit_sha" > "${bundle_dir}/git-commit.txt"

copy_and_checksum() {
  local src="$1"
  local dst="$2"
  cp "$src" "$dst"
  shasum -a 256 "$dst" >> "${bundle_dir}/SHA256SUMS.txt"
}

copy_and_checksum "docs/launch-readiness-gates.md" "${bundle_dir}/launch-readiness-gates.md"
copy_and_checksum "docs/technical-whitepaper.md" "${bundle_dir}/technical-whitepaper.md"
copy_and_checksum "docs/communications-launch-plan.md" "${bundle_dir}/communications-launch-plan.md"
copy_and_checksum "docs/blog-what-is-bitcoin-infinity.md" "${bundle_dir}/blog-what-is-bitcoin-infinity.md"
copy_and_checksum "docs/blog-utxo-to-genesis-deep-dive.md" "${bundle_dir}/blog-utxo-to-genesis-deep-dive.md"
copy_and_checksum "docs/blog-patoshi-balance-floor-explainer.md" "${bundle_dir}/blog-patoshi-balance-floor-explainer.md"
checklist_basename="$(basename "$CHECKLIST_FILE")"
copy_and_checksum "$CHECKLIST_FILE" "${bundle_dir}/${checklist_basename}"
copy_and_checksum "docs/genesis-determinism-check.md" "${bundle_dir}/genesis-determinism-check.md"
copy_and_checksum "docs/snapshot-supply-reconciliation.md" "${bundle_dir}/snapshot-supply-reconciliation.md"
copy_and_checksum "docs/incident-communication-templates.md" "${bundle_dir}/incident-communication-templates.md"
copy_and_checksum "docs/incident-launch-pack.md" "${bundle_dir}/incident-launch-pack.md"
copy_and_checksum "docs/go-no-go-gate-update.md" "${bundle_dir}/go-no-go-gate-update.md"
copy_and_checksum "docs/go-no-go-signoff-prefill.md" "${bundle_dir}/go-no-go-signoff-prefill.md"
copy_and_checksum "docs/external-gate-packet.md" "${bundle_dir}/external-gate-packet.md"
copy_and_checksum ".github/workflows/ci.yml" "${bundle_dir}/ci.yml"
copy_and_checksum ".github/workflows/nightly-fuzz.yml" "${bundle_dir}/nightly-fuzz.yml"
copy_and_checksum ".github/workflows/launch-evidence.yml" "${bundle_dir}/launch-evidence.yml"
copy_and_checksum ".github/workflows/launch-rehearsal.yml" "${bundle_dir}/launch-rehearsal.yml"
copy_and_checksum ".github/workflows/release-manifest.yml" "${bundle_dir}/release-manifest.yml"
copy_and_checksum "scripts/launch/run_readiness_gate.sh" "${bundle_dir}/run_readiness_gate.sh"
copy_and_checksum "scripts/launch/check_go_no_go_checklist.sh" "${bundle_dir}/check_go_no_go_checklist.sh"
copy_and_checksum "scripts/launch/check_nightly_fuzz_health.sh" "${bundle_dir}/check_nightly_fuzz_health.sh"
copy_and_checksum "scripts/launch/check_issue1_core_goals.sh" "${bundle_dir}/check_issue1_core_goals.sh"
copy_and_checksum "scripts/launch/check_genesis_determinism.sh" "${bundle_dir}/check_genesis_determinism.sh"
copy_and_checksum "scripts/launch/check_snapshot_supply_reconciliation.sh" "${bundle_dir}/check_snapshot_supply_reconciliation.sh"
copy_and_checksum "scripts/launch/generate_incident_launch_pack.sh" "${bundle_dir}/generate_incident_launch_pack.sh"
copy_and_checksum "scripts/launch/generate_external_gate_packet.sh" "${bundle_dir}/generate_external_gate_packet.sh"
copy_and_checksum "scripts/launch/update_go_no_go_gate.sh" "${bundle_dir}/update_go_no_go_gate.sh"
copy_and_checksum "scripts/launch/prefill_go_no_go_signoff.sh" "${bundle_dir}/prefill_go_no_go_signoff.sh"
copy_and_checksum "scripts/launch/run_launch_rehearsal.sh" "${bundle_dir}/run_launch_rehearsal.sh"
copy_and_checksum "scripts/launch/generate_release_manifest.sh" "${bundle_dir}/generate_release_manifest.sh"

snapshot_bundle_json="${bundle_dir}/snapshot-supply-check.json"
if [[ "$CHECK_SNAPSHOT_SUPPLY" -eq 1 ]]; then
  snapshot_genesis_sha="$(shasum -a 256 "$SNAPSHOT_GENESIS" | awk '{print $1}')"
  snapshot_txoutsetinfo_sha="$(shasum -a 256 "$SNAPSHOT_TXOUTSETINFO" | awk '{print $1}')"
  {
    echo "snapshot_genesis_path=${SNAPSHOT_GENESIS}"
    echo "snapshot_genesis_sha256=${snapshot_genesis_sha}"
    echo "snapshot_txoutsetinfo_path=${SNAPSHOT_TXOUTSETINFO}"
    echo "snapshot_txoutsetinfo_sha256=${snapshot_txoutsetinfo_sha}"
    echo "snapshot_tolerance_sats=${SNAPSHOT_TOLERANCE_SATS}"
  } > "${bundle_dir}/snapshot-inputs.txt"
  shasum -a 256 "${bundle_dir}/snapshot-inputs.txt" >> "${bundle_dir}/SHA256SUMS.txt"
  copy_and_checksum "$SNAPSHOT_TXOUTSETINFO" "${bundle_dir}/snapshot-gettxoutsetinfo.json"
fi

gate_status="skipped"
gate_exit_code=0
if [[ "$SKIP_GATE" -eq 0 ]]; then
  gate_log="${bundle_dir}/readiness-gate.log"
  gate_cmd=(./scripts/launch/run_readiness_gate.sh "--${MODE}")
  gate_cmd+=(--skip-checklist)
  gate_cmd+=(--cargo-target-dir "$CARGO_TARGET_DIR")
  if [[ "$REQUIRE_GO" -eq 1 ]]; then
    gate_cmd+=(--require-go)
  fi
  if [[ "$INCLUDE_FUZZ" -eq 1 ]]; then
    gate_cmd+=(--include-fuzz)
  fi
  if [[ "$CHECK_NIGHTLY_FUZZ_HEALTH" -eq 1 ]]; then
    gate_cmd+=(
      --check-nightly-fuzz-health
      --nightly-fuzz-branch "$NIGHTLY_FUZZ_BRANCH"
      --nightly-fuzz-workflow "$NIGHTLY_FUZZ_WORKFLOW"
      --nightly-fuzz-window-days "$NIGHTLY_FUZZ_WINDOW_DAYS"
      --nightly-fuzz-min-runs "$NIGHTLY_FUZZ_MIN_RUNS"
      --nightly-fuzz-max-runs "$NIGHTLY_FUZZ_MAX_RUNS"
    )
    if [[ -n "$NIGHTLY_FUZZ_JOB_PATTERN" ]]; then
      gate_cmd+=(--nightly-fuzz-job-pattern "$NIGHTLY_FUZZ_JOB_PATTERN")
    fi
    if [[ "$NIGHTLY_FUZZ_ALLOW_IN_PROGRESS" -eq 1 ]]; then
      gate_cmd+=(--nightly-fuzz-allow-in-progress)
    fi
    if [[ "$NIGHTLY_FUZZ_FAIL_ON_CANCELLED" -eq 1 ]]; then
      gate_cmd+=(--nightly-fuzz-fail-on-cancelled)
    fi
  fi
  if [[ "$SKIP_ISSUE1_GOAL_CHECKS" -eq 1 ]]; then
    gate_cmd+=(--skip-issue1-goal-checks)
  fi
  if [[ "$CHECK_SNAPSHOT_SUPPLY" -eq 1 ]]; then
    gate_cmd+=(
      --check-snapshot-supply
      --snapshot-genesis "$SNAPSHOT_GENESIS"
      --snapshot-txoutsetinfo "$SNAPSHOT_TXOUTSETINFO"
      --snapshot-tolerance-sats "$SNAPSHOT_TOLERANCE_SATS"
      --snapshot-json-out "$snapshot_bundle_json"
    )
  fi

  echo "Running readiness gate: ${gate_cmd[*]}"
  set +e
  "${gate_cmd[@]}" 2>&1 | tee "$gate_log"
  gate_exit_code=${PIPESTATUS[0]}
  set -e
  if [[ "$gate_exit_code" -eq 0 ]]; then
    gate_status="passed"
  else
    gate_status="failed"
  fi
else
  echo "Skipping readiness gate execution (--skip-gate set)."
fi

if [[ "$CHECK_SNAPSHOT_SUPPLY" -eq 1 && -f "$snapshot_bundle_json" ]]; then
  shasum -a 256 "$snapshot_bundle_json" >> "${bundle_dir}/SHA256SUMS.txt"
  if [[ -n "$SNAPSHOT_JSON_OUT" ]]; then
    cp "$snapshot_bundle_json" "$SNAPSHOT_JSON_OUT"
  fi
fi

checklist_status="passed"
checklist_exit_code=0
checklist_log="${bundle_dir}/go-no-go-checklist-report.txt"
checklist_json="${bundle_dir}/go-no-go-checklist-report.json"
checklist_todo=-1
checklist_invalid=-1
checklist_missing_signoff=-1
checklist_invalid_signoff_format=-1
checklist_inconsistent_go_decision=-1
checklist_done_missing_owner=-1
checklist_done_missing_evidence=-1
checklist_done_missing_completed_date=-1
checklist_done_invalid_completed_date=-1
checklist_done_invalid_evidence_refs=-1
checklist_cmd=(bash ./scripts/launch/check_go_no_go_checklist.sh --file "$CHECKLIST_FILE" --json-out "$checklist_json")
if [[ "$REQUIRE_GO" -eq 1 ]]; then
  checklist_cmd+=(--require-go)
fi

echo "Running checklist validation: ${checklist_cmd[*]}"
set +e
"${checklist_cmd[@]}" 2>&1 | tee "$checklist_log"
checklist_exit_code=${PIPESTATUS[0]}
set -e
if [[ "$checklist_exit_code" -ne 0 ]]; then
  checklist_status="failed"
fi

if [[ -f "$checklist_json" ]]; then
  checklist_todo="$(jq -r '.totals.todo // -1' "$checklist_json")"
  checklist_invalid="$(jq -r '.totals.invalid // -1' "$checklist_json")"
  checklist_missing_signoff="$(jq -r '.totals.missing_signoff_fields // -1' "$checklist_json")"
  checklist_invalid_signoff_format="$(jq -r '.totals.invalid_signoff_format // -1' "$checklist_json")"
  checklist_inconsistent_go_decision="$(jq -r '.totals.inconsistent_go_decision // -1' "$checklist_json")"
  checklist_done_missing_owner="$(jq -r '.totals.done_missing_owner // -1' "$checklist_json")"
  checklist_done_missing_evidence="$(jq -r '.totals.done_missing_evidence // -1' "$checklist_json")"
  checklist_done_missing_completed_date="$(jq -r '.totals.done_missing_completed_date // -1' "$checklist_json")"
  checklist_done_invalid_completed_date="$(jq -r '.totals.done_invalid_completed_date // -1' "$checklist_json")"
  checklist_done_invalid_evidence_refs="$(jq -r '.totals.done_invalid_evidence_refs // -1' "$checklist_json")"
fi

jq \
  --arg gate_status "$gate_status" \
  --argjson gate_exit_code "$gate_exit_code" \
  --arg checklist_status "$checklist_status" \
  --argjson checklist_exit_code "$checklist_exit_code" \
  --argjson checklist_todo "$checklist_todo" \
  --argjson checklist_invalid "$checklist_invalid" \
  --argjson checklist_missing_signoff "$checklist_missing_signoff" \
  --argjson checklist_invalid_signoff_format "$checklist_invalid_signoff_format" \
  --argjson checklist_inconsistent_go_decision "$checklist_inconsistent_go_decision" \
  --argjson checklist_done_missing_owner "$checklist_done_missing_owner" \
  --argjson checklist_done_missing_evidence "$checklist_done_missing_evidence" \
  --argjson checklist_done_missing_completed_date "$checklist_done_missing_completed_date" \
  --argjson checklist_done_invalid_completed_date "$checklist_done_invalid_completed_date" \
  --argjson checklist_done_invalid_evidence_refs "$checklist_done_invalid_evidence_refs" \
  '.readiness_gate.status = $gate_status
   | .readiness_gate.exit_code = $gate_exit_code
   | .checklist.status = $checklist_status
   | .checklist.exit_code = $checklist_exit_code
   | .checklist.totals = {
      todo: $checklist_todo,
      invalid: $checklist_invalid,
      missing_signoff_fields: $checklist_missing_signoff,
      invalid_signoff_format: $checklist_invalid_signoff_format,
      inconsistent_go_decision: $checklist_inconsistent_go_decision,
      done_missing_owner: $checklist_done_missing_owner,
      done_missing_evidence: $checklist_done_missing_evidence,
      done_missing_completed_date: $checklist_done_missing_completed_date,
      done_invalid_completed_date: $checklist_done_invalid_completed_date,
      done_invalid_evidence_refs: $checklist_done_invalid_evidence_refs
    }' \
  "${bundle_dir}/metadata.json" > "${bundle_dir}/metadata.tmp.json"
mv "${bundle_dir}/metadata.tmp.json" "${bundle_dir}/metadata.json"

cat > "${bundle_dir}/SUMMARY.md" <<EOF
# Launch Evidence Bundle Summary

- generated_at: ${iso_timestamp}
- bundle_dir: ${bundle_dir}
- commit: ${commit_sha}
- branch: ${branch_name}
- worktree_dirty: ${worktree_dirty}
- readiness_gate_mode: ${MODE}
- readiness_gate_include_fuzz: ${INCLUDE_FUZZ}
- readiness_gate_check_nightly_fuzz_health: ${CHECK_NIGHTLY_FUZZ_HEALTH}
- readiness_gate_nightly_fuzz_branch: ${NIGHTLY_FUZZ_BRANCH}
- readiness_gate_nightly_fuzz_workflow: ${NIGHTLY_FUZZ_WORKFLOW}
- readiness_gate_nightly_fuzz_window_days: ${NIGHTLY_FUZZ_WINDOW_DAYS}
- readiness_gate_nightly_fuzz_min_runs: ${NIGHTLY_FUZZ_MIN_RUNS}
- readiness_gate_nightly_fuzz_max_runs: ${NIGHTLY_FUZZ_MAX_RUNS}
- readiness_gate_nightly_fuzz_job_pattern: ${NIGHTLY_FUZZ_JOB_PATTERN}
- readiness_gate_nightly_fuzz_allow_in_progress: ${NIGHTLY_FUZZ_ALLOW_IN_PROGRESS}
- readiness_gate_nightly_fuzz_fail_on_cancelled: ${NIGHTLY_FUZZ_FAIL_ON_CANCELLED}
- readiness_gate_check_snapshot_supply: ${CHECK_SNAPSHOT_SUPPLY}
- readiness_gate_snapshot_tolerance_sats: ${SNAPSHOT_TOLERANCE_SATS}
- cargo_target_dir: ${CARGO_TARGET_DIR}
- readiness_gate_skip_issue1_goal_checks: ${SKIP_ISSUE1_GOAL_CHECKS}
- readiness_gate_status: ${gate_status}
- readiness_gate_exit_code: ${gate_exit_code}
- checklist_file: ${CHECKLIST_FILE}
- checklist_require_go: ${REQUIRE_GO}
- checklist_status: ${checklist_status}
- checklist_exit_code: ${checklist_exit_code}
- checklist_todo: ${checklist_todo}
- checklist_invalid: ${checklist_invalid}
- checklist_missing_signoff: ${checklist_missing_signoff}
- checklist_invalid_signoff_format: ${checklist_invalid_signoff_format}
- checklist_inconsistent_go_decision: ${checklist_inconsistent_go_decision}
- checklist_done_missing_owner: ${checklist_done_missing_owner}
- checklist_done_missing_evidence: ${checklist_done_missing_evidence}
- checklist_done_missing_completed_date: ${checklist_done_missing_completed_date}
- checklist_done_invalid_completed_date: ${checklist_done_invalid_completed_date}
- checklist_done_invalid_evidence_refs: ${checklist_done_invalid_evidence_refs}

## Files

- metadata.json
- git-status.txt
- git-commit.txt
- SHA256SUMS.txt
- launch-readiness-gates.md
- technical-whitepaper.md
- communications-launch-plan.md
- blog-what-is-bitcoin-infinity.md
- blog-utxo-to-genesis-deep-dive.md
- blog-patoshi-balance-floor-explainer.md
- ${checklist_basename}
- genesis-determinism-check.md
- snapshot-supply-reconciliation.md
- incident-communication-templates.md
- incident-launch-pack.md
- go-no-go-gate-update.md
- go-no-go-signoff-prefill.md
- external-gate-packet.md
- ci.yml
- nightly-fuzz.yml
- launch-evidence.yml
- launch-rehearsal.yml
- release-manifest.yml
- run_readiness_gate.sh
- check_go_no_go_checklist.sh
- check_nightly_fuzz_health.sh
- check_issue1_core_goals.sh
- check_genesis_determinism.sh
- check_snapshot_supply_reconciliation.sh
- generate_incident_launch_pack.sh
- generate_external_gate_packet.sh
- update_go_no_go_gate.sh
- prefill_go_no_go_signoff.sh
- run_launch_rehearsal.sh
- generate_release_manifest.sh
- go-no-go-checklist-report.txt
- go-no-go-checklist-report.json
EOF

if [[ "$SKIP_GATE" -eq 0 ]]; then
  echo "- readiness-gate.log" >> "${bundle_dir}/SUMMARY.md"
fi
if [[ "$CHECK_SNAPSHOT_SUPPLY" -eq 1 ]]; then
  {
    echo "- snapshot-inputs.txt"
    echo "- snapshot-gettxoutsetinfo.json"
    echo "- snapshot-supply-check.json"
  } >> "${bundle_dir}/SUMMARY.md"
fi

echo
echo "Launch evidence bundle complete: ${bundle_dir}"

if [[ "$SKIP_GATE" -eq 0 && "$gate_exit_code" -ne 0 ]]; then
  echo "Readiness gate failed during evidence generation. See ${bundle_dir}/readiness-gate.log" >&2
  exit "$gate_exit_code"
fi

if [[ "$checklist_exit_code" -ne 0 ]]; then
  echo "Checklist validation failed during evidence generation. See ${bundle_dir}/go-no-go-checklist-report.txt" >&2
  exit "$checklist_exit_code"
fi
