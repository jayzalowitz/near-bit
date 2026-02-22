#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NEARCORE_DIR="${ROOT_DIR}/nearcore"
TX_GEN_DIR="${NEARCORE_DIR}/benchmarks/transactions-generator"
ARTIFACT_ROOT="${ROOT_DIR}/artifacts/benchmarks"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
RUN_DIR="${ARTIFACT_ROOT}/${TIMESTAMP}"

LOGLEVEL="info,transaction-generator=info,stats=info"
METRICS_URL="http://127.0.0.1:4040/metrics"
METRICS_INTERVAL_S=5
NUM_ACCOUNTS=500
DRY_RUN=0
SKIP_BUILD=0
PROFILE_SELECTION="all"
TPS_OVERRIDE=""
DURATION_OVERRIDE_S=""
RUN_GRACE_S=90
STARTUP_TIMEOUT_S=900
ALLOW_NONZERO_RUN_STATUS=0
ENABLE_CONTROLLER=1

profile_tps() {
  case "$1" in
    baseline) echo "1000" ;;
    stress) echo "10000" ;;
    peak) echo "50000" ;;
    *)
      echo "error: unknown profile '$1'" >&2
      exit 1
      ;;
  esac
}

profile_duration_s() {
  case "$1" in
    baseline) echo "3600" ;;
    stress) echo "1800" ;;
    peak) echo "600" ;;
    *)
      echo "error: unknown profile '$1'" >&2
      exit 1
      ;;
  esac
}

print_usage() {
  cat <<'EOF'
Usage: scripts/benchmark/run_tps_profiles.sh [options]

Runs benchmark profiles defined in docs/benchmark-methodology.md using
nearcore/benchmarks/transactions-generator and captures reproducible artifacts.

Options:
  --profile <baseline|stress|peak|all>   Profile to run (default: all)
  --tps-override <n>                      Override target TPS for all selected profiles
  --duration-override <seconds>           Override duration for all selected profiles
  --num-accounts <n>                      Number of generated sender accounts (default: 500)
  --run-grace <seconds>                   Extra runtime window before forced shutdown (default: 90)
  --startup-timeout <seconds>             Max wait for setup/schedule start before forced shutdown (default: 900)
  --disable-controller                    Disable tx-generator controller loop (default: enabled)
  --allow-nonzero-run-status              Do not fail script when one or more profiles exit non-zero
  --metrics-interval <seconds>            Metrics polling interval (default: 5)
  --out-dir <path>                        Output directory (default: artifacts/benchmarks/<timestamp>)
  --loglevel <rust_log>                   RUST_LOG for neard run-localnet
  --skip-build                            Skip neard/synth-bm build step
  --dry-run                               Print commands but do not execute benchmark workload
  -h, --help                              Show this help

Examples:
  scripts/benchmark/run_tps_profiles.sh --profile baseline
  scripts/benchmark/run_tps_profiles.sh --profile baseline --duration-override 60
  scripts/benchmark/run_tps_profiles.sh --profile baseline --duration-override 30 --run-grace 30
  scripts/benchmark/run_tps_profiles.sh --profile all --metrics-interval 2
  scripts/benchmark/run_tps_profiles.sh --dry-run --profile stress
EOF
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: missing required command: $1" >&2
    exit 1
  fi
}

json_number_or_null() {
  local value="$1"
  if [[ -z "${value}" ]]; then
    echo "null"
  else
    echo "${value}"
  fi
}

metric_delta_or_empty() {
  local final="$1"
  local baseline="$2"
  local baseline_num delta
  if ! [[ "${final}" =~ ^[0-9]+$ ]]; then
    echo ""
    return
  fi
  baseline_num=0
  if [[ "${baseline}" =~ ^[0-9]+$ ]]; then
    baseline_num="${baseline}"
  fi
  delta="$((final - baseline_num))"
  if [[ "${delta}" -lt 0 ]]; then
    delta=0
  fi
  echo "${delta}"
}

run_cmd() {
  if [[ "${DRY_RUN}" -eq 1 ]]; then
    echo "[dry-run] $*"
  else
    eval "$@"
  fi
}

collect_metrics_loop() {
  local watched_pid="$1"
  local output_csv="$2"
  echo "timestamp_utc,tx_processed_success_total,tx_processed_failed_total" >"${output_csv}"
  while kill -0 "${watched_pid}" >/dev/null 2>&1; do
    local now success failed metrics
    now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    metrics="$(curl -fsS "${METRICS_URL}" 2>/dev/null || true)"
    if [[ -n "${metrics}" ]]; then
      success="$(awk '/^near_transaction_processed_successfully_total / {print $2; exit}' <<<"${metrics}")"
      failed="$(awk '/^near_transaction_processed_failed_total / {print $2; exit}' <<<"${metrics}")"
    else
      success=""
      failed=""
    fi
    echo "${now},${success:-},${failed:-}" >>"${output_csv}"
    sleep "${METRICS_INTERVAL_S}"
  done
}

terminate_run_process() {
  local pid="$1"
  local sig="${2:-TERM}"
  pkill "-${sig}" -P "${pid}" >/dev/null 2>&1 || true
  kill "-${sig}" "${pid}" >/dev/null 2>&1 || true
}

apply_unlimit_tuning() {
  local near_home="${TX_GEN_DIR}/.near"
  local genesis_file="${near_home}/genesis.json"
  local config_file="${near_home}/config.json"

  run_cmd "jq '.gas_limit=20000000000000000' \"${genesis_file}\" > \"${near_home}/genesis.tmp.json\" && mv \"${near_home}/genesis.tmp.json\" \"${genesis_file}\""
  # jq may serialize very large integers as scientific notation; force canonical integer literal.
  run_cmd "perl -0pi -e 's/\"gas_limit\"\\s*:\\s*2e\\+16/\"gas_limit\": 20000000000000000/g' \"${genesis_file}\""
  run_cmd "jq '.view_client_threads=8 | .store.load_mem_tries_for_tracked_shards=true | .produce_chunk_add_transactions_time_limit={\"secs\":0,\"nanos\":500000000}' \"${config_file}\" > \"${near_home}/config.tmp.json\" && mv \"${near_home}/config.tmp.json\" \"${config_file}\""
}

enable_tx_generator() {
  local near_home="${TX_GEN_DIR}/.near"
  local near_config_file="${near_home}/config.json"
  local near_accounts_path="${TX_GEN_DIR}/user-data"
  local settings_in="${TX_GEN_DIR}/tx-generator-settings.json.in"
  local settings_file="${TX_GEN_DIR}/tx-generator-settings.json"
  local merged_settings="${near_home}/tx-generator-settings.tmp.json"

  if [[ "${DRY_RUN}" -eq 1 ]]; then
    run_cmd "if [[ ! -f \"${settings_file}\" ]]; then cp -f \"${settings_in}\" \"${settings_file}\"; fi"
  fi
  if [[ "${DRY_RUN}" -eq 0 ]] && [[ ! -f "${settings_file}" ]]; then
    cp -f "${settings_in}" "${settings_file}"
  fi
  run_cmd "jq --arg accounts_path \"${near_accounts_path}\" '.tx_generator.accounts_path = \$accounts_path | del(.tx_generator.receiver_accounts_path)' \"${settings_file}\" > \"${merged_settings}\""
  run_cmd "jq -s '.[0] * .[1]' \"${near_config_file}\" \"${merged_settings}\" > \"${near_home}/config.tmp.json\" && mv \"${near_home}/config.tmp.json\" \"${near_config_file}\""
  run_cmd "rm -f \"${merged_settings}\""
}

prepare_schedule_file() {
  local profile="$1"
  local schedule_tps="$2"
  local schedule_duration="$3"
  if [[ "${ENABLE_CONTROLLER}" -eq 1 ]]; then
    jq -n \
      --argjson tps "${schedule_tps}" \
      --argjson duration_s "${schedule_duration}" \
      '{
        tx_generator: {
          transaction_type: "NativeToken",
          schedule: [
            { tps: $tps, duration_s: $duration_s }
          ],
          controller: {
            target_block_production_time_s: 1.5,
            bps_filter_window_length: 6,
            gain_proportional: 300.0,
            gain_integral: 0,
            gain_derivative: 0.0,
            block_pause_threshold_ms: 3000
          }
        }
      }' >"${TX_GEN_DIR}/tx-generator-settings.json"
  else
    jq -n \
      --argjson tps "${schedule_tps}" \
      --argjson duration_s "${schedule_duration}" \
      '{
        tx_generator: {
          transaction_type: "NativeToken",
          schedule: [
            { tps: $tps, duration_s: $duration_s }
          ],
          controller: null
        }
      }' >"${TX_GEN_DIR}/tx-generator-settings.json"
  fi
  cp "${TX_GEN_DIR}/tx-generator-settings.json" "${RUN_DIR}/${profile}/tx-generator-settings.json"
}

ensure_binaries() {
  if [[ "${SKIP_BUILD}" -eq 1 ]]; then
    if [[ "${DRY_RUN}" -eq 0 ]]; then
      if [[ ! -x "${TX_GEN_DIR}/neard" ]]; then
        echo "error: expected executable ${TX_GEN_DIR}/neard when using --skip-build" >&2
        echo "hint: rerun without --skip-build to build and link neard with tx_generator support." >&2
        exit 1
      fi
      if ! grep -a -E -q 'starting the static load schedule|completed running the schedule|tx generator idle: no schedule provided' "${TX_GEN_DIR}/neard"; then
        echo "error: ${TX_GEN_DIR}/neard does not appear to include tx_generator benchmark markers." >&2
        echo "hint: rerun without --skip-build to rebuild neard with --features tx_generator." >&2
        exit 1
      fi
    fi
    return
  fi
  require_cmd cargo
  local neard_bin synth_bm_manifest
  neard_bin="${NEARCORE_DIR}/target/release/neard"
  synth_bm_manifest="${NEARCORE_DIR}/benchmarks/synth-bm/Cargo.toml"

  run_cmd "cargo build --release --manifest-path \"${NEARCORE_DIR}/Cargo.toml\" -p neard --features tx_generator"
  run_cmd "cargo build --release --manifest-path \"${synth_bm_manifest}\""
  run_cmd "ln -sf \"${neard_bin}\" \"${TX_GEN_DIR}/neard\""
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --profile)
        PROFILE_SELECTION="${2:-}"
        shift 2
        ;;
      --tps-override)
        TPS_OVERRIDE="${2:-}"
        shift 2
        ;;
      --duration-override)
        DURATION_OVERRIDE_S="${2:-}"
        shift 2
        ;;
      --num-accounts)
        NUM_ACCOUNTS="${2:-}"
        shift 2
        ;;
      --run-grace)
        RUN_GRACE_S="${2:-}"
        shift 2
        ;;
      --startup-timeout)
        STARTUP_TIMEOUT_S="${2:-}"
        shift 2
        ;;
      --disable-controller)
        ENABLE_CONTROLLER=0
        shift
        ;;
      --allow-nonzero-run-status)
        ALLOW_NONZERO_RUN_STATUS=1
        shift
        ;;
      --metrics-interval)
        METRICS_INTERVAL_S="${2:-}"
        shift 2
        ;;
      --out-dir)
        RUN_DIR="${2:-}"
        shift 2
        ;;
      --loglevel)
        LOGLEVEL="${2:-}"
        shift 2
        ;;
      --skip-build)
        SKIP_BUILD=1
        shift
        ;;
      --dry-run)
        DRY_RUN=1
        shift
        ;;
      -h|--help)
        print_usage
        exit 0
        ;;
      *)
        echo "error: unknown option: $1" >&2
        print_usage
        exit 1
        ;;
    esac
  done
}

validate_numeric_overrides() {
  if [[ -n "${TPS_OVERRIDE}" ]] && ! [[ "${TPS_OVERRIDE}" =~ ^[0-9]+$ ]]; then
    echo "error: --tps-override must be an integer" >&2
    exit 1
  fi
  if [[ -n "${DURATION_OVERRIDE_S}" ]] && ! [[ "${DURATION_OVERRIDE_S}" =~ ^[0-9]+$ ]]; then
    echo "error: --duration-override must be an integer number of seconds" >&2
    exit 1
  fi
  if [[ -n "${NUM_ACCOUNTS}" ]] && ! [[ "${NUM_ACCOUNTS}" =~ ^[0-9]+$ ]]; then
    echo "error: --num-accounts must be an integer" >&2
    exit 1
  fi
  if ! [[ "${METRICS_INTERVAL_S}" =~ ^[0-9]+$ ]]; then
    echo "error: --metrics-interval must be an integer number of seconds" >&2
    exit 1
  fi
  if ! [[ "${RUN_GRACE_S}" =~ ^[0-9]+$ ]]; then
    echo "error: --run-grace must be an integer number of seconds" >&2
    exit 1
  fi
  if ! [[ "${STARTUP_TIMEOUT_S}" =~ ^[0-9]+$ ]]; then
    echo "error: --startup-timeout must be an integer number of seconds" >&2
    exit 1
  fi
}

select_profiles() {
  local profiles=()
  if [[ "${PROFILE_SELECTION}" == "all" ]]; then
    profiles=(baseline stress peak)
  else
    case "${PROFILE_SELECTION}" in
      baseline|stress|peak)
        profiles=("${PROFILE_SELECTION}")
        ;;
      *)
        echo "error: invalid profile '${PROFILE_SELECTION}'" >&2
        exit 1
        ;;
    esac
  fi
  echo "${profiles[@]}"
}

main() {
  parse_args "$@"
  validate_numeric_overrides

  require_cmd jq
  if [[ "${DRY_RUN}" -eq 0 ]]; then
    require_cmd just
    require_cmd curl
    require_cmd awk
    require_cmd sed
    require_cmd grep
    require_cmd pkill
    require_cmd tee
    require_cmd perl
  fi

  mkdir -p "${RUN_DIR}"
  local git_commit
  git_commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
  printf '%s\n' "${git_commit}" >"${RUN_DIR}/git-commit.txt"

  ensure_binaries

  local profiles
  # shellcheck disable=SC2207
  profiles=($(select_profiles))

  for profile in "${profiles[@]}"; do
    local tps duration profile_dir log_file metrics_file start_ts end_ts
    local avg_rate peak_rate final_processed final_failed
    local final_success_metric final_failed_metric final_success_metric_raw final_failed_metric_raw
    local metrics_baseline_success metrics_baseline_failed metrics_baseline_captured
    local schedule_completed signal_11 effective_run_status
    local run_status

    tps="$(profile_tps "${profile}")"
    duration="$(profile_duration_s "${profile}")"
    if [[ -n "${TPS_OVERRIDE}" ]]; then
      tps="${TPS_OVERRIDE}"
    fi
    if [[ -n "${DURATION_OVERRIDE_S}" ]]; then
      duration="${DURATION_OVERRIDE_S}"
    fi
    profile_dir="${RUN_DIR}/${profile}"
    log_file="${profile_dir}/neard.log"
    metrics_file="${profile_dir}/metrics.csv"
    mkdir -p "${profile_dir}"

    prepare_schedule_file "${profile}" "${tps}" "${duration}"

    if [[ "${DRY_RUN}" -eq 1 ]]; then
      echo "[dry-run] profile=${profile} tps=${tps} duration_s=${duration}"
      run_cmd "cd \"${TX_GEN_DIR}\" && just init-localnet"
      apply_unlimit_tuning
      run_cmd "cd \"${TX_GEN_DIR}\" && RUST_LOG=\"info,transaction-generator=off\" just create-accounts \"${NUM_ACCOUNTS}\""
      enable_tx_generator
      run_cmd "cd \"${TX_GEN_DIR}\" && just run-localnet \"${LOGLEVEL}\""
      continue
    fi

    start_ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    (
      cd "${TX_GEN_DIR}"
      just init-localnet
      apply_unlimit_tuning
      RUST_LOG="info,transaction-generator=off" just create-accounts "${NUM_ACCOUNTS}"
      enable_tx_generator
      just run-localnet "${LOGLEVEL}"
    ) > >(tee "${log_file}") 2>&1 &
    local run_pid="$!"

    collect_metrics_loop "${run_pid}" "${metrics_file}" &
    local metrics_pid="$!"

    local max_wait_s runtime_elapsed_s startup_elapsed_s timed_out timeout_phase schedule_stop_requested schedule_started run_localnet_started
    max_wait_s="$((duration + RUN_GRACE_S))"
    runtime_elapsed_s=0
    startup_elapsed_s=0
    timed_out=0
    timeout_phase=""
    metrics_baseline_success=""
    metrics_baseline_failed=""
    metrics_baseline_captured=0
    schedule_stop_requested=0
    schedule_started=0
    run_localnet_started=0
    while kill -0 "${run_pid}" >/dev/null 2>&1; do
      if [[ -f "${log_file}" ]]; then
        if [[ "${run_localnet_started}" -eq 0 ]] && grep -E -q 'RUST_LOG=.*--home .* run$' "${log_file}"; then
          run_localnet_started=1
        fi
        if [[ "${run_localnet_started}" -eq 1 ]] && [[ "${metrics_baseline_captured}" -eq 0 ]] && [[ -f "${metrics_file}" ]]; then
          metrics_baseline_success="$(awk -F, 'NR>1 && $2!="" {v=$2} END {print v}' "${metrics_file}")"
          metrics_baseline_failed="$(awk -F, 'NR>1 && $3!="" {v=$3} END {print v}' "${metrics_file}")"
          metrics_baseline_captured=1
        fi

        if [[ "${run_localnet_started}" -eq 1 ]]; then
          if [[ "${schedule_started}" -eq 0 ]] && grep -E -q 'started schedule=|starting the static load schedule' "${log_file}"; then
            schedule_started=1
          fi
          if [[ "${schedule_started}" -eq 0 ]] && [[ "${ENABLE_CONTROLLER}" -eq 0 ]]; then
            # Controller-disabled runs may not emit tx-generator schedule logs.
            # Treat run-localnet launch as runtime start to avoid counting setup.
            schedule_started=1
          fi
        fi
      fi

      if [[ "${schedule_started}" -eq 0 ]]; then
        startup_elapsed_s="$((startup_elapsed_s + 1))"
        if [[ "${startup_elapsed_s}" -ge "${STARTUP_TIMEOUT_S}" ]]; then
          timed_out=1
          timeout_phase="startup"
          break
        fi
      else
        if [[ "${ENABLE_CONTROLLER}" -eq 1 ]] && [[ "${schedule_stop_requested}" -eq 0 ]] && [[ -f "${log_file}" ]] && grep -q 'completed running the schedule' "${log_file}"; then
          schedule_stop_requested=1
          terminate_run_process "${run_pid}" "TERM"
        fi

        runtime_elapsed_s="$((runtime_elapsed_s + 1))"
        if [[ "${runtime_elapsed_s}" -ge "${max_wait_s}" ]]; then
          timed_out=1
          timeout_phase="runtime"
          break
        fi
      fi
      sleep 1
    done
    if kill -0 "${run_pid}" >/dev/null 2>&1; then
      timed_out=1
      if [[ -z "${timeout_phase}" ]]; then
        timeout_phase="unknown"
      fi
      terminate_run_process "${run_pid}" "TERM"
      sleep 2
    fi
    if kill -0 "${run_pid}" >/dev/null 2>&1; then
      terminate_run_process "${run_pid}" "KILL"
    fi

    set +e
    wait "${run_pid}"
    run_status="$?"
    set -e

    kill "${metrics_pid}" >/dev/null 2>&1 || true
    wait "${metrics_pid}" 2>/dev/null || true
    end_ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    cp "${TX_GEN_DIR}/.near/config.json" "${profile_dir}/config.json"
    cp "${TX_GEN_DIR}/.near/genesis.json" "${profile_dir}/genesis.json"

    local rate_stats
    rate_stats="$(awk '
      index($0, "rate=") {
        split($0, a, "rate=")
        split(a[2], b, /[^0-9.]/)
        if (b[1] != "") {
          v = b[1] + 0
          sum += v
          n += 1
          if (n == 1 || v > max) {
            max = v
          }
        }
      }
      END {
        if (n > 0) {
          printf "%.3f %.3f", sum / n, max
        }
      }
    ' "${log_file}")"
    avg_rate="$(awk '{print $1}' <<<"${rate_stats}")"
    peak_rate="$(awk '{print $2}' <<<"${rate_stats}")"
    final_processed="$(awk '
      index($0, "included_in_chunk: ") {
        split($0, a, "included_in_chunk: ")
        split(a[2], b, /[^0-9]/)
        if (b[1] != "") {
          v = b[1]
        }
      }
      END {
        if (v != "") {
          print v
        }
      }
    ' "${log_file}")"
    final_failed="$(awk '
      index($0, "failed: ") {
        split($0, a, "failed: ")
        split(a[2], b, /[^0-9]/)
        if (b[1] != "") {
          v = b[1]
        }
      }
      END {
        if (v != "") {
          print v
        }
      }
    ' "${log_file}")"
    final_success_metric_raw="$(awk -F, 'NR>1 && $2!="" {v=$2} END {print v}' "${metrics_file}")"
    final_failed_metric_raw="$(awk -F, 'NR>1 && $3!="" {v=$3} END {print v}' "${metrics_file}")"
    final_success_metric="$(metric_delta_or_empty "${final_success_metric_raw}" "${metrics_baseline_success}")"
    final_failed_metric="$(metric_delta_or_empty "${final_failed_metric_raw}" "${metrics_baseline_failed}")"
    schedule_completed=0
    signal_11=0
    if grep -q 'completed running the schedule' "${log_file}"; then
      schedule_completed=1
    fi
    if grep -E -q 'signal 11|SIGSEGV' "${log_file}"; then
      signal_11=1
    fi
    effective_run_status="${run_status}"
    if [[ "${run_status}" -eq 143 ]] && [[ "${schedule_completed}" -eq 1 ]] && [[ "${timed_out}" -eq 0 ]] && [[ "${signal_11}" -eq 0 ]]; then
      effective_run_status=0
    fi

    jq -n \
      --arg profile "${profile}" \
      --arg start_ts "${start_ts}" \
      --arg end_ts "${end_ts}" \
      --argjson target_tps "${tps}" \
      --argjson target_duration_s "${duration}" \
      --argjson controller_enabled "${ENABLE_CONTROLLER}" \
      --argjson run_status "${run_status}" \
      --argjson effective_run_status "${effective_run_status}" \
      --argjson timed_out "${timed_out}" \
      --arg timeout_phase "${timeout_phase}" \
      --argjson schedule_started "${schedule_started}" \
      --argjson run_localnet_started "${run_localnet_started}" \
      --argjson avg_tps "$(json_number_or_null "${avg_rate}")" \
      --argjson peak_tps "$(json_number_or_null "${peak_rate}")" \
      --argjson final_processed_log "$(json_number_or_null "${final_processed}")" \
      --argjson final_failed_log "$(json_number_or_null "${final_failed}")" \
      --argjson baseline_success_metric "$(json_number_or_null "${metrics_baseline_success}")" \
      --argjson baseline_failed_metric "$(json_number_or_null "${metrics_baseline_failed}")" \
      --argjson final_success_metric_raw "$(json_number_or_null "${final_success_metric_raw}")" \
      --argjson final_failed_metric_raw "$(json_number_or_null "${final_failed_metric_raw}")" \
      --argjson final_success_metric "$(json_number_or_null "${final_success_metric}")" \
      --argjson final_failed_metric "$(json_number_or_null "${final_failed_metric}")" \
      --argjson schedule_completed "${schedule_completed}" \
      --argjson signal_11 "${signal_11}" \
      '{
        profile: $profile,
        start_ts_utc: $start_ts,
        end_ts_utc: $end_ts,
        target_tps: $target_tps,
        target_duration_s: $target_duration_s,
        controller_enabled: ($controller_enabled == 1),
        run_status: $run_status,
        effective_run_status: $effective_run_status,
        timed_out: $timed_out,
        timeout_phase: (if $timed_out == 1 then $timeout_phase else null end),
        run_localnet_started_from_log: $run_localnet_started,
        schedule_started_from_log: $schedule_started,
        observed: {
          avg_tps_from_log: $avg_tps,
          peak_tps_from_log: $peak_tps,
          final_processed_from_log: $final_processed_log,
          final_failed_from_log: $final_failed_log,
          pre_run_success_metric_baseline: $baseline_success_metric,
          pre_run_failed_metric_baseline: $baseline_failed_metric,
          final_success_metric_raw: $final_success_metric_raw,
          final_failed_metric_raw: $final_failed_metric_raw,
          final_success_metric: $final_success_metric,
          final_failed_metric: $final_failed_metric,
          schedule_completed_from_log: $schedule_completed,
          signal_11_from_log: $signal_11
        }
      }' >"${profile_dir}/summary.json"
  done

  if [[ "${DRY_RUN}" -eq 1 ]]; then
    echo "dry-run complete. no benchmark workload executed."
    exit 0
  fi

  local summary_files=()
  while IFS= read -r summary_file; do
    summary_files+=("${summary_file}")
  done < <(find "${RUN_DIR}" -mindepth 2 -maxdepth 2 -name summary.json -type f | sort)

  if [[ "${#summary_files[@]}" -eq 0 ]]; then
    echo "error: no profile summary files were generated in ${RUN_DIR}" >&2
    exit 1
  fi

  jq -s \
    --arg generated_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --arg git_commit "${git_commit}" \
    '{
      generated_at_utc: $generated_at,
      git_commit: $git_commit,
      nonzero_profile_count: ([.[] | select(.effective_run_status != 0)] | length),
      signal_11_profile_count: ([.[] | select(.observed.signal_11_from_log == 1)] | length),
      profiles: .
    }' \
    "${summary_files[@]}" >"${RUN_DIR}/summary.json"

  {
    echo "profile,target_tps,target_duration_s,controller_enabled,run_status,effective_run_status,timed_out,timeout_phase,run_localnet_started_from_log,schedule_started_from_log,avg_tps_from_log,peak_tps_from_log,final_success_metric,final_failed_metric,schedule_completed_from_log,signal_11_from_log"
    find "${RUN_DIR}" -mindepth 2 -maxdepth 2 -name summary.json -type f | sort | while read -r summary_file; do
      jq -r '[.profile, .target_tps, .target_duration_s, .controller_enabled, .run_status, .effective_run_status, .timed_out, (.timeout_phase // "n/a"), .run_localnet_started_from_log, .schedule_started_from_log, .observed.avg_tps_from_log, .observed.peak_tps_from_log, .observed.final_success_metric, .observed.final_failed_metric, .observed.schedule_completed_from_log, .observed.signal_11_from_log] | @csv' "${summary_file}"
    done
  } >"${RUN_DIR}/summary.csv"

  {
    echo "# Bitcoin Infinity Benchmark Run (${TIMESTAMP})"
    echo
    echo "- generated_at_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "- git_commit: ${git_commit}"
    echo "- nonzero_profile_count: $(jq -r '.nonzero_profile_count' "${RUN_DIR}/summary.json")"
    echo "- signal_11_profile_count: $(jq -r '.signal_11_profile_count' "${RUN_DIR}/summary.json")"
    echo "- methodology: docs/benchmark-methodology.md"
    echo
    echo "## Profiles"
    echo
    echo "| profile | target TPS | duration (s) | controller enabled | run status | effective status | timed out | timeout phase | run launched | schedule started | avg TPS (log) | peak TPS (log) | final success metric | final failed metric | schedule completed | signal 11 |"
    echo "|---|---:|---:|---|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|"
    find "${RUN_DIR}" -mindepth 2 -maxdepth 2 -name summary.json -type f | sort | while read -r summary_file; do
      jq -r '"| \(.profile) | \(.target_tps) | \(.target_duration_s) | \(.controller_enabled) | \(.run_status) | \(.effective_run_status) | \(.timed_out) | \(.timeout_phase // "n/a") | \(.run_localnet_started_from_log) | \(.schedule_started_from_log) | \(.observed.avg_tps_from_log // "n/a") | \(.observed.peak_tps_from_log // "n/a") | \(.observed.final_success_metric // "n/a") | \(.observed.final_failed_metric // "n/a") | \(.observed.schedule_completed_from_log // "n/a") | \(.observed.signal_11_from_log // "n/a") |"' "${summary_file}"
    done
    echo
    echo "Raw artifacts:"
    echo "- summary json: \`summary.json\`"
    echo "- summary csv: \`summary.csv\`"
    echo "- per-profile logs/metrics/config under profile subdirectories"
  } >"${RUN_DIR}/summary.md"

  echo "benchmark artifacts written to: ${RUN_DIR}"

  local nonzero_profile_count
  nonzero_profile_count="$(jq -r '.nonzero_profile_count' "${RUN_DIR}/summary.json")"
  if [[ "${nonzero_profile_count}" -gt 0 ]] && [[ "${ALLOW_NONZERO_RUN_STATUS}" -eq 0 ]]; then
    echo "error: ${nonzero_profile_count} profile(s) exited with non-zero run_status (see ${RUN_DIR}/summary.json)." >&2
    echo "hint: rerun with --allow-nonzero-run-status to keep a zero script exit code while retaining diagnostics." >&2
    exit 2
  fi
}

main "$@"
