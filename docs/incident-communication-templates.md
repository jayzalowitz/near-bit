# Incident Communication Templates

Use these templates for public status updates and validator/operator coordination. Keep messages factual, timestamped, and version-specific.

## 1) Investigating Issue

Use when impact is confirmed but root cause is still under investigation.

```text
Title: Investigating issue affecting <component>

Timestamp (UTC): <YYYY-MM-DDTHH:MM:SSZ>
Severity: <P0|P1|P2|P3>
Affected component(s): <rpc|validator|wallet flow|explorer|other>
Current impact: <one-sentence impact statement>

We are investigating an issue affecting <component>. Users may experience <symptom>.
Current mitigation status: <none|partial mitigation active>.
Next update ETA: <minutes>.

Tracking:
- Status page: <url>
- Incident ID: <id>
- Build/version: <git-sha-or-tag>
```

## 2) Security Halt / Do Not Transact

Use only for high-confidence security incidents requiring immediate transaction pause.

```text
Title: Security advisory: pause transactions while emergency response is active

Timestamp (UTC): <YYYY-MM-DDTHH:MM:SSZ>
Severity: P0
Scope: <mainnet|testnet|specific subsystem>

A security issue has been identified in <component>. We are coordinating a fix with validator operators.
Do not submit new transactions until further notice.

Immediate actions in progress:
1. Validator coordination and containment.
2. Patch validation on isolated environment.
3. Public follow-up with remediation steps.

Next update ETA: <minutes>.
Status page: <url>
Incident ID: <id>
```

## 3) Resolution / Recovery Complete

Use after service restoration and verification checks complete.

```text
Title: Incident resolved: normal operations restored

Timestamp (UTC): <YYYY-MM-DDTHH:MM:SSZ>
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
Status page: <url>
Incident ID: <id>
```

## 4) Emergency Upgrade Request (Validators)

Use for urgent protocol/runtime/security updates requiring fast operator action.

```text
Title: Emergency upgrade required: version <X.Y.Z>

Timestamp (UTC): <YYYY-MM-DDTHH:MM:SSZ>
Severity: <P0|P1>
Upgrade deadline (UTC): <YYYY-MM-DDTHH:MM:SSZ>

Validators: upgrade to <X.Y.Z> by the deadline above.
Reason: <one-sentence reason>.

Upgrade instructions:
1. Pull release/tag: <tag-or-commit>.
2. Verify artifact checksum/signature: <checksum instructions>.
3. Restart node using standard rollout (canary first).
4. Confirm post-upgrade health: block progression, RPC availability, validator status.

Compatibility notes:
- Minimum compatible version: <version>
- Unsafe versions: <version list>

Coordination channel: <url/channel>
Status page: <url>
Incident ID: <id>
```

## Usage Rules

1. Always include UTC timestamps and incident ID.
2. Do not speculate on cause before confirmation.
3. Keep update cadence explicit (`next update ETA`).
4. Link every public message to a canonical status page entry.
