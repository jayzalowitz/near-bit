# Launch Communications Plan

This guide defines the repository-backed communications assets required for launch readiness.

## Scope

This plan covers:

1. prelaunch communications artifacts
2. public testnet launch announcement sequencing
3. mainnet launch communication controls
4. ownership and review workflow

This plan does not replace legal review, security incident templates, or go/no-go signoff.

## Required Prelaunch Assets

Before public launch announcements, publish or finalize:

1. `technical-whitepaper.md`
2. `blog-what-is-bitcoin-infinity.md`
3. `blog-utxo-to-genesis-deep-dive.md`
4. `blog-patoshi-balance-floor-explainer.md`
5. `launch-readiness-gates.md`
6. `mainnet-go-no-go-checklist.md`

## Audience and Message Boundaries

### Primary audiences

1. Bitcoin holders evaluating key continuity and trust assumptions
2. wallet and exchange integrators evaluating RPC and operational compatibility
3. validator operators evaluating launch process and runbook maturity

### Message constraints

1. Do not claim unmeasured aggregate throughput.
2. Do not publish unresolved security claims as complete.
3. Do not publish legal interpretations as final before counsel signoff.
4. Always link technical claims to source docs and reproducible commands.

## Prelaunch Publication Sequence

Use this sequence to avoid fragmented messaging:

1. Publish whitepaper baseline
2. Publish architecture and genesis deep-dive blog
3. Publish Patoshi policy and mechanism explainer
4. Publish launch readiness status with explicit external blockers
5. Publish public testnet announcement with known limitations

Recommended spacing: 24 to 72 hours between major technical posts.

## Public Testnet Announcement Checklist

Before publishing "testnet live":

1. launch readiness smoke gate is green on target commit
2. evidence bundle generated and archived
3. wallet integration path documented
4. known unsupported RPC methods listed
5. bug reporting path is active and monitored

Minimum announcement content:

1. tested wallet path(s)
2. supported RPC scope and caveats
3. issue tracker location
4. current launch blocker list for mainnet

## Mainnet Window Communications Checklist

Within the final launch window:

1. publish planned window in UTC
2. publish candidate commit SHA and artifact references
3. publish final go/no-go decision with approvers
4. publish first epoch stabilization status after launch

Post-launch follow-ups:

1. 24-hour status note
2. 7-day stabilization summary
3. first retrospective with open issues and owner assignments

## Ownership and Review

Suggested ownership model:

1. Protocol lead: technical accuracy and compatibility statements
2. Operations lead: launch window, validator readiness, and incident routing
3. Security lead: vulnerability and risk statements
4. Legal lead: token and policy disclaimers

Approval policy:

1. any launch-impacting communication requires at least two reviewers
2. high-risk statements (security, legal, token policy) require domain-owner signoff

## Repository Integration

This document is intended to be:

1. required by launch readiness doc checks
2. captured in launch evidence bundles
3. linked from documentation hub and site index

## Related Documents

1. `technical-whitepaper.md`
2. `incident-communication-templates.md`
3. `launch-readiness-gates.md`
4. `mainnet-go-no-go-checklist.md`
5. `launch-evidence-bundle.md`
6. `launch-rehearsal.md`
