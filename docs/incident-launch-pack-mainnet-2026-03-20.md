# Incident Communication Pack (Launch Window Prefill)

Generated at (UTC): 2026-03-16T21:19:24Z
Release candidate: 4f16c7d42
Launch window (UTC): 2026-03-20T18:00:00Z to 2026-03-20T22:00:00Z
Status page: https://status.bitcoininfinity.io
Coordination channel: #validators-bridge
Incident ID prefix: LAUNCH

Use this pack to satisfy go/no-go gate #13 by pre-filling launch-window constants before incidents happen.
Replace remaining angle-bracket placeholders with incident-specific values when sending an update.

## 1) Investigating Issue

Title: Investigating issue affecting <component>

Timestamp (UTC): 2026-03-20T18:00:00Z
Severity: <P0|P1|P2|P3>
Affected component(s): <rpc|validator|wallet flow|explorer|other>
Current impact: <one-sentence impact statement>

We are investigating an issue affecting <component>. Users may experience <symptom>.
Current mitigation status: <none|partial mitigation active>.
Next update ETA: <minutes>.

Tracking:
- Status page: https://status.bitcoininfinity.io
- Incident ID: LAUNCH-001
- Build/version: 4f16c7d42

## 2) Security Halt / Do Not Transact

Title: Security advisory: pause transactions while emergency response is active

Timestamp (UTC): 2026-03-20T18:00:00Z
Severity: P0
Scope: <mainnet|testnet|specific subsystem>

A security issue has been identified in <component>. We are coordinating a fix with validator operators.
Do not submit new transactions until further notice.

Immediate actions in progress:
1. Validator coordination and containment.
2. Patch validation on isolated environment.
3. Public follow-up with remediation steps.

Next update ETA: <minutes>.
Status page: https://status.bitcoininfinity.io
Incident ID: LAUNCH-002

## 3) Resolution / Recovery Complete

Title: Incident resolved: normal operations restored

Timestamp (UTC): 2026-03-20T22:00:00Z
Severity: <P0|P1|P2|P3>
Affected component(s): <component list>

The incident affecting <component> has been resolved. Normal operations have resumed.

What happened:
- <short factual summary>

What we changed:
- <fix 1>
- <fix 2>

Verification completed:
- <health check 1>
- <health check 2>

Post-incident report ETA: <date/time>.
Status page: https://status.bitcoininfinity.io
Incident ID: LAUNCH-003

## 4) Emergency Upgrade Request (Validators)

Title: Emergency upgrade required: version 4f16c7d42

Timestamp (UTC): 2026-03-20T18:00:00Z
Severity: <P0|P1>
Upgrade deadline (UTC): 2026-03-20T22:00:00Z

Validators: upgrade to 4f16c7d42 by the deadline above.
Reason: <one-sentence reason>.

Upgrade instructions:
1. Pull release/tag: 4f16c7d42.
2. Verify artifact checksum/signature: <checksum instructions>.
3. Restart node using standard rollout (canary first).
4. Confirm post-upgrade health: block progression, RPC availability, validator status.

Compatibility notes:
- Minimum compatible version: <version>
- Unsafe versions: <version list>

Coordination channel: #validators-bridge
Status page: https://status.bitcoininfinity.io
Incident ID: LAUNCH-004
