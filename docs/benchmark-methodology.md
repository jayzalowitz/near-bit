# Bitcoin Infinity TPS Benchmark Methodology

Status: methodology and runner automation implemented; full-duration and pilot benchmark results published below.

This document defines how throughput and latency claims are measured for Bitcoin Infinity.

## Scope

- Network: private testnet
- Consensus: Proof of Stake (nearcore runtime)
- Measurement unit: successfully finalized on-chain transactions
- Finality definition: transaction included and execution outcome available from RPC

## Reporting Rules

- Always report single-shard results separately from multi-shard aggregate results.
- Never publish a raw `>1M TPS` claim without test configuration and raw output.
- Include p50/p95/p99 RPC latency for transaction submission and confirmation.
- Include validator health metrics (missed blocks, epoch transitions, memory and CPU usage).

## Test Profiles

Run these profiles with identical transaction mix and fee settings:

1. Baseline: `1,000 TPS` for `60 minutes`
2. Stress: `10,000 TPS` for `30 minutes`
3. Peak stress: `50,000 TPS` for `10 minutes`

For each profile, publish:

- target TPS
- achieved TPS
- success rate
- finality latency p50/p95/p99
- validator missed-block rate
- RPC error rate

## Execution Runner

Use `scripts/benchmark/run_tps_profiles.sh` to run the methodology profiles and collect standardized artifacts.

Examples:

```bash
# Preview commands only
./scripts/benchmark/run_tps_profiles.sh --dry-run --skip-build --profile baseline

# Run full methodology suite
./scripts/benchmark/run_tps_profiles.sh --profile all
```

Default profile settings in the runner:

- `baseline`: `1000 TPS` for `3600s`
- `stress`: `10000 TPS` for `1800s`
- `peak`: `50000 TPS` for `600s`

The runner will:

- build benchmark binaries (unless `--skip-build`),
- write per-profile tx-generator schedules,
- run `transactions-generator` benchmark workloads,
- scrape NEAR metrics endpoint while the workload runs,
- emit per-profile and aggregate summary outputs, including shutdown diagnostics (`schedule_completed_from_log`, `signal_11_from_log`) and normalized status (`effective_run_status`).
- use controller-enabled schedules by default and gracefully terminate after schedule completion.
- apply timeout in two phases:
  - setup/startup timeout (`--startup-timeout`, default `900s`) before schedule starts
  - runtime timeout (`duration + --run-grace`) after schedule starts
- exit non-zero if any profile has non-zero `effective_run_status` (use `--allow-nonzero-run-status` to override while keeping diagnostics).

## Transaction Mix

- 90% simple value transfers
- 10% wallet/RPC compatibility operations (UTXO listing, raw tx lookups)
- All transactions signed using secp256k1 account flow

## Required Artifacts

- Testnet config used (epoch length, shard count, validator count)
- Load-generator source and commit hash
- Raw CSV/JSON output files
- Summary report with plots

Runner artifact layout:

- `artifacts/benchmarks/<timestamp>/git-commit.txt`
- `artifacts/benchmarks/<timestamp>/<profile>/neard.log`
- `artifacts/benchmarks/<timestamp>/<profile>/metrics.csv`
- `artifacts/benchmarks/<timestamp>/<profile>/tx-generator-settings.json`
- `artifacts/benchmarks/<timestamp>/<profile>/config.json`
- `artifacts/benchmarks/<timestamp>/<profile>/genesis.json`
- `artifacts/benchmarks/<timestamp>/<profile>/summary.json`
- `artifacts/benchmarks/<timestamp>/summary.json`
- `artifacts/benchmarks/<timestamp>/summary.csv`
- `artifacts/benchmarks/<timestamp>/summary.md`

## Publishing Format

When publishing benchmark results, use this exact wording pattern:

- `Single-shard measured throughput: X TPS`
- `Estimated aggregate throughput with N shards: Y TPS (assumes linear scaling; measured single-shard baseline above)`

This avoids overstating unmeasured performance.

## Full Methodology Results (2026-02-20)

Full profile suite command:
- `./scripts/benchmark/run_tps_profiles.sh --profile all --skip-build --startup-timeout 1200 --run-grace 180 --metrics-interval 5 --out-dir artifacts/benchmarks/full-methodology-20260220T191500Z`

Aggregate diagnostics:
- `nonzero_profile_count=0`
- `signal_11_profile_count=0`

Observed full-duration profile results:
- baseline (`1000 TPS`, `3600s`): `avg=997.768`, `peak=1345.953`, `final_success_metric=3596631`, `final_failed_metric=0`, `run_status=143`, `effective_run_status=0`, `schedule_completed_from_log=1`, `signal_11_from_log=0`
- stress (`10000 TPS`, `1800s`): `avg=9968.958`, `peak=12885.510`, `final_success_metric=17965779`, `final_failed_metric=0`, `run_status=143`, `effective_run_status=0`, `schedule_completed_from_log=1`, `signal_11_from_log=0`
- peak (`50000 TPS`, `600s`): `avg=17034.490`, `peak=19269.950`, `final_success_metric=10303437`, `final_failed_metric=0`, `run_status=143`, `effective_run_status=0`, `schedule_completed_from_log=1`, `signal_11_from_log=0`

Validator health snapshots (from `node_status` log lines):
- baseline: `bps_avg=1.640`, `bps_max=1.710`, `mem_min=77.1MB`, `mem_max=103.0MB`
- stress: `bps_avg=1.623`, `bps_max=1.700`, `mem_min=76.5MB`, `mem_max=103.0MB`
- peak: `bps_avg=1.088`, `bps_max=1.670`, `mem_min=76.8MB`, `mem_max=100.0MB`

Artifacts:
- `artifacts/benchmarks/full-methodology-20260220T191500Z/summary.json`
- `artifacts/benchmarks/full-methodology-20260220T191500Z/summary.csv`
- `artifacts/benchmarks/full-methodology-20260220T191500Z/summary.md`

## Release Candidate Benchmark Artifacts (2026-03-05)

Release-candidate verification command:
- `./scripts/benchmark/run_tps_profiles.sh --profile all --duration-override 20 --run-grace 45 --startup-timeout 600 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/release-candidate-20260305T170837Z`

Aggregate diagnostics:
- `git_commit=ad763faee947446daa00444cf8d5ce701ee8a449`
- `nonzero_profile_count=0`
- `signal_11_profile_count=0`

Observed release-candidate profile results:
- baseline (`1000 TPS`, `20s`): `avg=621.389`, `peak=889.315`, `final_success_metric=19020`, `final_failed_metric=0`, `run_status=143`, `effective_run_status=0`, `schedule_completed_from_log=1`, `signal_11_from_log=0`
- stress (`10000 TPS`, `20s`): `avg=4419.368`, `peak=6881.821`, `final_success_metric=134977`, `final_failed_metric=0`, `run_status=143`, `effective_run_status=0`, `schedule_completed_from_log=1`, `signal_11_from_log=0`
- peak (`50000 TPS`, `20s`): `avg=6343.463`, `peak=7867.829`, `final_success_metric=178933`, `final_failed_metric=0`, `run_status=143`, `effective_run_status=0`, `schedule_completed_from_log=1`, `signal_11_from_log=0`

Published raw artifacts (committed):
- `docs/benchmark-artifacts/release-candidate-20260305T170837Z/summary.json`
- `docs/benchmark-artifacts/release-candidate-20260305T170837Z/summary.csv`
- `docs/benchmark-artifacts/release-candidate-20260305T170837Z/summary.md`
- `docs/benchmark-artifacts/release-candidate-20260305T170837Z/baseline/neard.log`
- `docs/benchmark-artifacts/release-candidate-20260305T170837Z/stress/neard.log`
- `docs/benchmark-artifacts/release-candidate-20260305T170837Z/peak/neard.log`

## Historical Pilot Results (2026-02-20)

These are short-duration pilot runs used to validate runner behavior before the full-duration publication above.

Single-shard baseline pilot:
- command: `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 1000 --duration-override 60 --run-grace 45 --num-accounts 500 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/pilot-baseline-1000-20260220T180109Z`
- observed: `avg_tps_from_log=857.586`, `peak_tps_from_log=1035.614`, `final_success_metric=58856`, `final_failed_metric=0`
- artifact: `artifacts/benchmarks/pilot-baseline-1000-20260220T180109Z/summary.json`

Native-TPS multi-profile pilot (20s/profile):
- command: `./scripts/benchmark/run_tps_profiles.sh --profile all --duration-override 20 --run-grace 45 --num-accounts 500 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/pilot-all-native-tps-20260220T180236Z`
- baseline target `1000`: `avg=620.265`, `peak=886.428`
- stress target `10000`: `avg=6143.507`, `peak=8844.634`
- peak target `50000`: `avg=8935.701`, `peak=12648.842`
- failed metric remained `0` in all three profiles
- artifact: `artifacts/benchmarks/pilot-all-native-tps-20260220T180236Z/summary.json`

Extended high-load pilots (60s, pre-mitigation historical runs):
- stress command: `./scripts/benchmark/run_tps_profiles.sh --profile stress --tps-override 10000 --duration-override 60 --run-grace 60 --num-accounts 500 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/pilot-stress-10000-60s-20260220T181011Z`
- stress observed: `avg=8551.340`, `peak=10328.730`, `final_success_metric=595345`, `final_failed_metric=0`, `run_status=139`, `schedule_completed_from_log=1`, `signal_11_from_log=1`
- peak command: `./scripts/benchmark/run_tps_profiles.sh --profile peak --tps-override 50000 --duration-override 60 --run-grace 60 --num-accounts 500 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/pilot-peak-50000-60s-20260220T181137Z`
- peak observed: `avg=12353.138`, `peak=14985.768`, `final_success_metric=843928`, `final_failed_metric=0`, `run_status=139`, `schedule_completed_from_log=1`, `signal_11_from_log=1`
- artifacts:
  - `artifacts/benchmarks/pilot-stress-10000-60s-20260220T181011Z/summary.json`
  - `artifacts/benchmarks/pilot-peak-50000-60s-20260220T181137Z/summary.json`

Extended high-load pilots (60s, post-mitigation controller-mode runs):
- stress command: `./scripts/benchmark/run_tps_profiles.sh --profile stress --tps-override 10000 --duration-override 60 --run-grace 120 --startup-timeout 900 --num-accounts 500 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/post-fix-stress60-20260220T183159Z`
- stress observed: `avg=8331.414`, `peak=11059.288`, `final_success_metric=566478`, `final_failed_metric=0`, `run_status=143`, `effective_run_status=0`, `schedule_completed_from_log=1`, `signal_11_from_log=0`
- peak command: `./scripts/benchmark/run_tps_profiles.sh --profile peak --tps-override 50000 --duration-override 60 --run-grace 120 --startup-timeout 900 --num-accounts 500 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/post-fix2-peak60-20260220T183819Z`
- peak observed: `avg=8798.048`, `peak=11576.004`, `final_success_metric=619419`, `final_failed_metric=0`, `run_status=143`, `effective_run_status=0`, `schedule_completed_from_log=1`, `signal_11_from_log=0`
- artifacts:
  - `artifacts/benchmarks/post-fix-stress60-20260220T183159Z/summary.json`
  - `artifacts/benchmarks/post-fix2-peak60-20260220T183819Z/summary.json`

Note:
- The pilot runs are retained for historical comparison and regression tracking.
