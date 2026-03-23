# Issue #1 Core-Goal Check

This guide documents the targeted verification command for [issue #1](https://github.com/infinitoshi/near-bit/issues/1) launch-critical goals.

## Purpose

`check_issue1_core_goals.sh` runs the two suites that directly validate:

1. Bitcoin addresses accepted as account IDs
2. Patoshi reassignment and genesis-tooling integrity

## Command

```bash
./scripts/launch/check_issue1_core_goals.sh
```

Current checks:

- `cargo test --manifest-path near-account-id/Cargo.toml`
- `cargo test -p bitinfinity-tools`

## Integration with Launch Gates

`run_readiness_gate.sh` runs this check by default:

```bash
./scripts/launch/run_readiness_gate.sh --smoke
```

For fast local iteration only (not launch signoff), you can skip it:

```bash
./scripts/launch/run_readiness_gate.sh --smoke --skip-issue1-goal-checks
```

The same opt-out flag is passed through by:

- `./scripts/launch/generate_evidence_bundle.sh --skip-issue1-goal-checks`
- `./scripts/launch/run_launch_rehearsal.sh --skip-issue1-goal-checks`
