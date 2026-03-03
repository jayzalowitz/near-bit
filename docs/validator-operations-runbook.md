# Validator Operations Runbook

This runbook is for operators running Bitcoin Infinity nodes in test or production-like environments.

## Node Lifecycle

### Initialize

```bash
cargo run -p bitinfinity-neard -- init --home ~/.bitinfinity
```

### Run

```bash
cargo run -p bitinfinity-neard -- run --home ~/.bitinfinity
```

### Configuration inspection

```bash
cargo run -p bitinfinity-neard -- config --home ~/.bitinfinity
```

## Health Checks

- process liveness and restart behavior
- RPC reachability
- block production/finality progression
- memory and CPU saturation trends
- error-rate spikes in adapter logs

## Pre-Deployment Checklist

1. build release binaries
2. run local CI parity checks
3. validate RPC smoke (wallet lifecycle + send/query)
4. validate benchmark runner dry-run
5. verify monitoring and log sinks

## Rollout Strategy

- canary node first
- validate chain progress and RPC correctness
- stagger remaining nodes
- avoid synchronized restarts across all validators

## Incident Categories

Use standardized public/internal messaging templates from [Incident Communication Templates](incident-communication-templates.md).

### Backend unavailable

Symptoms: repeated readiness errors (`-28`) from adapter clients.

Actions:

1. verify node process and RPC bindings
2. inspect recent deploy/config changes
3. restart only affected process first

### Wallet operation failures

Symptoms: lock-state and wallet-context errors (`-18`, `-13`, `-14`, `-15`).

Actions:

1. verify wallet context lifecycle in calling services
2. verify secret/passphrase source validity
3. reduce unlock window and retry sequence deterministically

### Throughput or latency degradation

Symptoms: TPS drop, elevated p95/p99 finality delay, queue growth.

Actions:

1. correlate with resource saturation
2. inspect mempool/pending transaction trends
3. compare against baseline profile artifacts
4. run controlled benchmark dry-run before config changes

## Post-Incident Requirements

- write timeline and impact summary
- record root cause and concrete remediation
- add regression tests or runbook checks where possible
- link artifacts and logs for future audits

## Operational Hygiene

- pin runtime/toolchain versions for releases
- keep benchmark artifacts by timestamp and commit
- avoid committing fuzz runtime outputs
- enforce least-privilege access for node hosts and secrets
