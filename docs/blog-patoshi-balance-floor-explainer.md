# Blog Draft: Patoshi Balance Floor, Governance Constraints, and Launch Reality

Status: Draft for technical and policy review

## Context

Patoshi-related balances are one of the highest-risk policy and implementation topics in the project. This post explains what is currently implemented in-repo, what is still external, and how launch controls treat both.

## What the Mechanism Is Trying to Do

The Patoshi policy direction in issue #11 is intended to:

1. apply explicit constraints to identified Patoshi-linked balances
2. encode guard behavior in runtime paths rather than informal promises
3. keep enforcement and unlock behavior testable and reviewable

## What Is In-Repo Today

Repository work includes:

1. Patoshi-aware tooling support in genesis and verification flows
2. test coverage in `bitinfinity-tools` around Patoshi processing paths
3. launch documentation and readiness tracking that treats Patoshi controls as explicit gates

Current readiness scripts verify related code and launch artifacts, but they do not substitute for completed legal and governance signoff.

## What Is Not Solved by Code Alone

Several Patoshi topics are external blockers by definition:

1. legal classification and jurisdictional risk treatment
2. governance publication for treasury/foundation control
3. validator/operator agreement around enforcement behavior

These remain tracked as external launch blockers in `launch-readiness-gates.md`.

## Why We Separate Technical and External Gates

Conflating these domains causes false confidence. Technical CI success does not imply policy readiness.

The launch model therefore separates:

1. repository-verifiable controls
2. external governance/legal/compliance controls

This keeps the engineering status honest and reduces ambiguity in go/no-go decisions.

## Engineering Requirements for Patoshi-Sensitive Changes

Any Patoshi-sensitive code path should meet:

1. deterministic test coverage for allowed and rejected behavior
2. explicit error semantics for blocked actions
3. no hidden policy toggles without traceable configuration
4. evidence capture in launch bundles where behavior affects go/no-go

## Communications Requirements

Public messaging should avoid claims that imply policy completion before external gates are signed:

1. describe implemented controls as implemented
2. describe legal/governance items as pending when pending
3. publish exact references to status docs and checklists

## Recommended Review Sequence

Before launch signoff, review in this order:

1. `technical-whitepaper.md`
2. `security-and-threat-model.md`
3. `tokenomics-and-governance.md`
4. `launch-readiness-gates.md`
5. `mainnet-go-no-go-checklist.md`

## Closing

Patoshi handling is not a marketing claim. It is a combined technical, legal, and governance control surface. The project treats it that way by design: test what can be tested in code, and track what cannot be solved in code as explicit blockers.
