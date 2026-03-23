# Monitoring, Alerting, and Status Page Drill (Launch Window 2026-03-10)

Date (UTC): 2026-03-05
Owner: launch-readiness

## Simulated Failure Scenario

Scenario: RPC backend unavailable event causing client-facing `-28` readiness errors.

## Drill Timeline (UTC)

- 2026-03-05T17:30:00Z: Failure injected in rehearsal environment.
- 2026-03-05T17:31:40Z: Alert fired to validator channel.
- 2026-03-05T17:33:10Z: Status-page incident entry published.
- 2026-03-05T17:39:00Z: Recovery validated and status-page incident resolved.

## Outcomes

- Time to detect: 100 seconds
- Time to public status update: 190 seconds
- Time to recovery validation: 540 seconds

## Evidence

- `docs/external-gate-artifacts/mainnet-2026-03-10/monitoring-drill-timeline.md`
- `docs/incident-launch-pack-mainnet-2026-03-10.md`
- `docs/validator-operations-runbook.md`

## Signoff

- Operations lead: launch-readiness
- Approval timestamp (UTC): 2026-03-05T17:47:30Z
