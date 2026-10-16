---
name: loom-code-review
description: Comprehensive code review covering correctness, maintainability, performance, security, and best practices.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Bash
triggers:
  - review
  - code review
  - PR review
  - pull request
  - check code
  - audit code
  - feedback
  - approve
  - request changes
  - comment
  - suggestion
  - LGTM
  - nit
  - blocker
  - code quality
  - best practice
  - architecture review
  - design review
  - security review
  - infra review
---

# Code Review

Review the requested change and its affected behavior. Prioritize correctness, security,
expected load, useful tests and maintainability. Give concrete findings and the smallest
adequate repairs; disclose consequential gaps in what you checked.

## Method

1. Read intent and locate the affected flow: entry points, callers/callees, state, tests,
   fixtures and config. Use source-graph queries where available, then targeted reads.
2. Trace success and failure paths beyond the diff. Confirm new preconditions are enforced
   and changed defaults, signatures or formats reach every consumer.
3. Establish expected load from repository/deployment evidence. Check concurrency and resource
   growth against it; state assumptions that affect a finding.
4. Apply the four lenses below, including missing tests and error handling.
5. Reconfirm each finding: the caller exists, the input/state is possible, and the cited code
   produces the consequence. Include a concrete scenario or violated requirement and a repair.

For a requested repository audit, map entry points and data flows first, then inspect the
highest-risk paths deeply. Report unexamined areas; do not page every file.

## Four lenses

- **Correctness:** boundary values, parsing, cleanup on early returns, idempotency, retry safety,
  races, lock ordering and compatibility. Follow errors through callers.
- **Security:** validation at trust boundaries, authorization, injection, secret exposure and
  data-loss prevention. Preserve these and accessibility when simplifying. For substantive
  auth/crypto/payment or trust-boundary changes, use `loom-security-audit` or `loom-threat-model`
  through `loom-skills`; a routine review does not establish security assurance.
- **Performance:** unbounded state, query-per-item patterns, repeated work and allocation on
  hot paths. Ground scale claims in expected inputs and deployment; speculative scale is a suggestion.
- **Maintainability:** remove unnecessary options, wrappers and duplication. Prefer an existing
  project facility, then standard/platform facilities or installed dependencies. Name the reuse
  target and verify its contract. Search callers, config and dynamic references before declaring
  code unused. Preserve project boundaries; avoid abstraction for hypothetical consumers.

## Tests and evidence

Changed risky logic needs a test that fails on a plausible wrong implementation: parsers,
branches, data writes, money, security, concurrency and bug fixes deserve particular attention.
Prefer extending existing tests. Trivial edits need no ceremonial tests. Inspect failure
assertions and external-call timeout/error handling; a passing suite alone proves neither.

Follow the task's verification boundary. A read-only review can inspect tests but must not
claim they ran. Stage implementers keep their one-check limit; the assigned verifier owns the gate.

## Loom stages

- **Silent failure:** investigate exit 0 with failure evidence on stderr, partial downloads,
  sandbox denials, swallowed errors and stale evidence.
- **Wiring:** follow the real entry point to the changed behavior. A declaration, import or
  isolated unit test does not prove registration, mounting, dispatch or execution.
- **Dependencies:** confirm required packages/data/services exist; a manifest or config entry
  alone does not establish availability.
- **Review protocol:** the `loom-code-reviewer` agent's `loom-review` schema governs stage
  output. Every recorded finding blocks completion regardless of severity; suggestions do not.
  Classify taste or unproven concerns as suggestions, and account for all open finding IDs.

## Findings and verdict

Outside the stage protocol, use these severities:

| Severity | Gate? | Evidence |
| --- | --- | --- |
| BLOCKER | Yes | Security exposure, data loss, crash or corruption |
| SILENT_FAILURE | Yes | Reported success despite an actual failure |
| CRITICAL | Yes | Concrete incorrect behavior in a reachable scenario |
| MAJOR | No | Substantiated maintainability debt or a missing test for a real risk |
| MINOR / nit | No | Optional style or small optimization |

Lead with findings ordered by impact; omit empty groups. For each, give `file:line`, the
trigger, wrong outcome or violated requirement, and the smallest adequate fix. Distinguish
established defects from assumptions. A known limitation needs a finding when current
requirements exceed it; otherwise retain its limit and revisit condition.

End with approve / approve-with-comments / request-changes and material unchecked areas.
Do not manufacture nits, praise, line-savings estimates or a larger redesign to fill a report.
In a Loom stage, put this human-readable material before the required final `loom-review` block.
