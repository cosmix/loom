---
name: loom-code-review
description: Comprehensive code review covering correctness, maintainability, performance, security, and best practices. Use for PR reviews, pre-merge audits, architecture and design critique, and actionable reviewer feedback.
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

## Overview

Review a change for correctness, security, performance, and maintainability, then produce prioritized, actionable feedback. Optimize signal: gate on what breaks users; comment (don't gate) on the rest.

## Review vs. adversarial security audit

Different jobs — don't conflate:

- **Code review (this skill):** holistic, author-empathetic. Reviews the *diff and its blast radius* against stated intent across four lenses. Assumes good faith; catches the bugs a careful peer catches.
- **Security audit (`/loom-security-audit`, `/loom-threat-model`):** attacker mindset, threat model, whole-attack-surface. Assumes hostile input everywhere.

For auth, crypto, payments, deserialization, or anything touching a trust boundary: do the code review AND trigger a security audit. A passing code review is not a security sign-off.

## Severity taxonomy

Label every comment. Only the first two gate the merge.

| Severity           | Gate? | Meaning                                                                            |
| ------------------ | ----- | --------------------------------------------------------------------------------- |
| **BLOCKER**        | ✅     | Security hole, data loss, crash, corruption. Merge is unsafe.                      |
| **SILENT_FAILURE** | ✅     | Exit 0 but the operation actually failed (sandbox-blocked download, partial fetch, stale cache, swallowed error). Always investigate before merge. |
| **CRITICAL**       | ✅     | Logic error / real bug that will bite in normal use.                              |
| **MAJOR**          | ❌     | Maintainability, tech debt, missing test for a real path. Fix soon.               |
| **MINOR / nit**    | ❌     | Style, naming, micro-optimization. Prefix with `nit:` so the author can skip it.  |

**Approve-with-comments discipline:** if nothing is BLOCKER/CRITICAL, approve and leave the MAJOR/MINOR notes as non-blocking. Don't hold a PR hostage over nits or personal style. Blocking on taste is the top reviewer anti-pattern — it trains authors to ignore you.

## Method

1. **Read intent first.** PR description, linked issue, commit messages. Review against what the change *claims* to do; flag scope creep separately from bugs.
2. **Review the diff AND its blast radius.** A hunk is not self-contained. For every changed symbol, `rg` its callers and callees — a signature change, a new early-return, a changed default, or a widened type ripples outward. Bugs hide at the seams the diff doesn't show.
3. **Four lenses per hunk:** correctness → security → performance → maintainability (below).
4. **Missing-tests / missing-error-path pass** (separate sweep — easy to skip).
5. **Verdict:** approve / approve-with-comments / request-changes, each comment severity-tagged.

### Blast-radius checklist

- [ ] Callers of every changed function/signature updated (search, don't assume)
- [ ] Callees: are new preconditions actually guaranteed by callers?
- [ ] Changed default value / enum variant / error type — who relied on the old one?
- [ ] Concurrency: new shared state, lock ordering, `await` points holding a guard?
- [ ] Public API / serialized format / DB schema change — back-compat and consumers?
- [ ] Tests, docs, and types updated alongside behavior?

### Four lenses

- **Correctness:** edge cases (empty, null, zero, negative, overflow, unicode, TZ/DST), off-by-one, error paths, resource cleanup (files/locks/connections on *every* return incl. early ones), idempotency/retry safety, race conditions. Trace the unhappy path, not just the happy one.
- **Security:** input validation at the boundary, injection (SQL/command/path/SSRF/XSS), authz on every sensitive op, secrets not logged, crypto-grade randomness. (Deep dive → security audit.)
- **Performance:** algorithmic complexity, N+1 queries, unnecessary allocation/clone in hot paths, missing pagination/streaming for unbounded data, `O(n)`-in-a-loop membership checks (want a set/map). Don't nitpick micro-perf off the hot path.
- **Maintainability:** does the abstraction fit the problem? single responsibility, honest naming, no copy-paste of logic that will drift, no dead/speculative code, comments explain *why* not *what*.

### Missing tests & error paths

The most common real defect in a passing PR: an untested error path. Ask:

- [ ] New branch / early-return / `catch` with no test exercising it?
- [ ] External call (network/db/fs) — what happens on timeout, 500, empty result?
- [ ] Does a returned `Result`/`Err`/rejected promise get handled, or silently dropped?
- [ ] New public function without a test for its failure mode, not just success?

## Loom orchestration review

For code produced by loom stages, add these (they catch the "compiles + tests pass but doesn't work" class):

- **Silent failure (BLOCKER):** exit 0 with error/warning on stderr; sandbox blocked a download but stage completed; external dep referenced but not installed/reachable.
- **Wiring (CRITICAL):** feature compiles and tests pass but is never imported / registered / mounted / reachable by a real user. Verify: is the module imported at the entry point? command/route registered? event handler connected? DI binding present? Can a user actually invoke it? (See `/loom-wiring-test`.)
- **Dependency reality:** package actually installed (not just in manifest); data file actually downloaded (not just referenced); endpoint actually reachable (not just configured).

## Domain quick-checklists

Compressed — expand the relevant one only when the diff touches that domain.

- **Infra/IaC:** no hardcoded secrets; least-privilege IAM; state backend secured; resource limits/requests (k8s); non-root, minimal base image, no secrets in layers (Docker); rollback path (CI/CD).
- **Data pipeline:** schema validation; null/dup handling; idempotent + exactly-once where claimed; dead-letter queue; partitioning/batching; storage lifecycle for cost.
- **ML:** seed set for reproducibility; train/val/test split has no leakage; data + model versioned; drift monitoring and rollback in prod; bias/fairness on training data.

## Reviewer output format

Group by severity, cite `file:line`, state issue → impact → fix. Keep it scannable.

```markdown
# Review: auth/login.py

## BLOCKER
- **L45 SQL injection** — `f"... WHERE email = '{email}'"` interpolates user input.
  Impact: arbitrary SQL. Fix: `cursor.execute("... WHERE email = %s", (email,))`.

## MAJOR
- **L112 unchecked None** — `send_email(user.email, ...)` after `.first()` can `None`-deref
  when the user doesn't exist. Add a guard returning 404.

## nit
- **L23** `GetUser` → `get_user` (snake_case).

## Good
- Clean validation split (L30-40); solid test coverage on the happy path.
```

Two tiny wrong→right patterns worth internalizing:

```python
# perf: O(n·m) — membership check rebuilds nothing but scans a list each pass
if user.id in active_ids:      # list → O(n) per lookup
# → hoist once:
active = set(active_ids)       # O(1) per lookup
if user.id in active:
```

## Author-empathy phrasing

Same fact, better delivery — critique the code, ask don't command, give the reason:

- ❌ "This is wrong." → ✅ "This deref crashes when `user` is None (L112) — guard it?"
- ❌ "Why didn't you use a set?" → ✅ "A `set` here makes this O(1) per lookup; worth it on the hot path."
- ❌ "Bad naming." → ✅ "nit: `data` is vague — `active_orders`?"

Acknowledge good work explicitly; it makes the blocking comments land.

## Verify before done

- [ ] Read the PR intent; scope creep flagged separately from bugs
- [ ] Every changed symbol's callers/callees checked (blast radius, not just the hunk)
- [ ] All four lenses applied; unhappy paths traced
- [ ] Missing-test / error-path sweep done
- [ ] Loom: wiring + silent-failure + dependency-reality checks (if orchestrated code)
- [ ] Every comment severity-tagged; nits marked `nit:`; verdict matches (only BLOCKER/CRITICAL gate)
- [ ] Security-sensitive surface → security audit triggered, not just reviewed
