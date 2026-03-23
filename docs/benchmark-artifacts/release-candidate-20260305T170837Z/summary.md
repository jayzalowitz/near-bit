# Bitcoin Infinity Benchmark Run (20260305T170837Z)

- generated_at_utc: 2026-03-05T17:10:35Z
- git_commit: ad763faee947446daa00444cf8d5ce701ee8a449
- nonzero_profile_count: 0
- signal_11_profile_count: 0
- methodology: docs/benchmark-methodology.md

## Profiles

| profile | target TPS | duration (s) | controller enabled | run status | effective status | timed out | timeout phase | run launched | schedule started | avg TPS (log) | peak TPS (log) | final success metric | final failed metric | schedule completed | signal 11 |
|---|---:|---:|---|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| baseline | 1000 | 20 | true | 143 | 0 | 0 | n/a | 1 | 1 | 621.389 | 889.315 | 19020 | 0 | 1 | 0 |
| peak | 50000 | 20 | true | 143 | 0 | 0 | n/a | 1 | 1 | 6343.463 | 7867.829 | 178933 | 0 | 1 | 0 |
| stress | 10000 | 20 | true | 143 | 0 | 0 | n/a | 1 | 1 | 4419.368 | 6881.821 | 134977 | 0 | 1 | 0 |

Raw artifacts:
- summary json: `summary.json`
- summary csv: `summary.csv`
- per-profile logs/metrics/config under profile subdirectories
