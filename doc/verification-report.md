# Verification report: what catches major defects in loom stages

Window: 2026-08-25 to 2026-09-23. Source: every loom stage session transcript on disk.

## Summary

| Question                                            | Result                                                                                                                                     |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| 1a. Stage acceptance criteria caught a major defect | **0 of 153** sessions that ran `loom stage complete`. Criteria the agent ran by hand before completion: **2 of 97** impl sessions (2.1%)   |
| 1b. Any stage verification caught a major defect    | **63 of 97** impl sessions (64.9%); 56 of 86 stages (65.1%). Review subagents account for most of it: 55 sessions (56.7%)                  |
| 2. Integration-verify caught a major defect         | **21 of 39** sessions that reached verification (53.8%); **19 of 31** plan-level runs (61.3%). 45 escaped from stages, 2 came from merging |

Acceptance criteria and goal-backward checks did not act as detectors. Together they caught 4 major
defects across 208 sessions and raised 92 false alarms, 23 for every real catch. The defects were
found by review subagents, the main agent's own gate run after workers reported done, and
integration-verify's reviewers. Integration-verify found escaped defects in most plan runs,
chiefly security, lifecycle and cross-stage wiring defects that stage reviews missed.

## Scope and method

- **Corpus.** 208 main-session transcripts from loom worktrees (`~/.claude/projects/*--worktrees-*`):
  - by kind: 118 implementation (impl), 43 integration-verify (IV), 47 knowledge-distill (KD);
  - by repo: cartolyth 91, loom 96, cil 18, stencilizer 3.

  Transcripts older than 2026-08-25 have been cleaned from disk, so the window is 30 days, not the 60 requested.

- **Condensing.** Each transcript was reduced from 355 MB of JSONL to 39 MB of text: user and
  assistant turns, tool calls, and tool output clipped at the head and tail, with more kept for
  output that mentions acceptance, failures or review.
- **Analysis.** Ten Opus analysts each took one size-balanced, chronological bucket. Every
  transcript received one record: kind, outcome, completion attempts, each major catch with its
  mechanism, origin and line evidence, and each false alarm with its cause.
- **Definitions.**
  - _Major_: a real defect that would have shipped otherwise. This covers incorrect behavior, a
    broken build, failing tests, missing functionality, a wiring gap, a spec violation, a security
    hole, data loss or a regression.
  - _Caught_: surfaced after the implementer believed the work was done. Red tests during TDD do
    not count.
  - When a case was ambiguous, analysts took the stricter reading: not major, not caught.
- **Normalization.** The analysts scored gate and acceptance catches inconsistently. I re-scored
  all 62 "major" catches from acceptance, goal-backward and the gate into three classes:
  - product defect (37, counted as major);
  - production build or typecheck break (22, reported separately);
  - test-only defect: stale doubles, fixtures, or races between tests (13, excluded).

  Review and other catches were taken as scored.

- **Acceptance split.** A catch counts as _at completion_ only when `loom stage complete` (or
  the daemon) failed on it. A criterion command the agent ran itself beforehand is reported
  separately.
- **Denominator.** Sessions that reached verification: 97 impl, 39 IV and 40 KD. The remaining
  sessions were empty, duplicates, or ended before any check ran.

## 1. Stage acceptance criteria and verification

### Implementation stages

| Mechanism                                              | Sessions with a major catch (of 97) | %        | Stages (of 86) | %        | Major defects |
| ------------------------------------------------------ | ----------------------------------- | -------- | -------------- | -------- | ------------- |
| Acceptance at `loom stage complete`                    | 0                                   | 0.0      | 0              | 0.0      | 0             |
| Acceptance criteria run by the agent before completion | 2                                   | 2.1      | 2              | 2.3      | 2             |
| Goal-backward (truths, artifacts, wiring)              | 1                                   | 1.0      | 1              | 1.2      | 1             |
| **Acceptance + goal-backward**                         | **3**                               | **3.1**  | **3**          | **3.5**  | **3**         |
| Gate: product defect                                   | 13                                  | 13.4     | 13             | 15.1     | 16            |
| Gate: product defect or build break                    | 28                                  | 28.9     | 28             | 32.6     | 38            |
| Review (subagent or main agent reading the diff)       | 55                                  | 56.7     | 49             | 57.0     | 101           |
| Other (functional smoke, advisor, user)                | 9                                   | 9.3      | 8              | 9.3      | 10            |
| **Any mechanism (product defects)**                    | **63**                              | **64.9** | **56**         | **65.1** | **130**       |

The three acceptance and goal-backward catches, the only ones in the corpus for impl stages:

- **worker-evidence.** A criteria sweep of the hook suite found `teammate-idle.sh` with an
  unclosed jq `select(`. Its `2>/dev/null || true` turned the compile error into "no match", so
  every real SubagentStop skipped lifecycle evidence.
- **completion-recovery.** A full-suite criterion found that the attestation key opener's
  no-follow check rejected the `.loom/work` symlink. That broke evidence signing in every real
  worktree.
- **country-data-artifacts.** `ArtifactMissing` caught an artifact built as `ctyov3` where the
  plan requires `ctyov4`. The stage signal had rendered the wrong name.

In the first two cases the agent ran the commands itself during its gate. Under another
analyst's convention they would have been gate catches, which leaves `loom stage complete` itself
with none.

### Knowledge-distill stages

No major catch in 40 sessions. 14 of 40 (35%) hit a false alarm, and only 24 of 47 sessions
completed. The criteria called `loom knowledge verify`, which does not exist, or required
`loom knowledge check --strict` to pass on a tree carrying several hundred to about 1,200
pre-existing issues. Each failure cost a session, a dispute and sometimes an adjudication.

### False alarms

| Kind               | Entries | Sessions affected | Share of verifying sessions |
| ------------------ | ------- | ----------------- | --------------------------- |
| impl               | 56      | 41                | 42%                         |
| integration-verify | 22      | 18                | 46%                         |
| knowledge-distill  | 14      | 14                | 35%                         |
| **Total**          | **92**  | **73**            |                             |

| Cause         | Entries | Examples                                                                                                                                                                                                                                      |
| ------------- | ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Criterion bug | 47      | literal greps pinned to one file or symbol after a refactor; parentheses read as regex; `rg -qF "--out"` read as a flag; `#[path]` test modules flagged as unwired; `[[path]]` read as a character class; nonexistent `loom knowledge verify` |
| Environment   | 30      | read-only sccache or bun cache; `cargo audit` without network; AF*UNIX denied; missing browser driver; inherited `LOOM*\*`variables; read-only`config/` mount                                                                                 |
| Flaky         | 6       | load timeouts; a 30 s wiring timeout against a 33.7 s suite                                                                                                                                                                                   |
| Other         | 9       | unformatted files from another plan; date rot; a relay hook dropping `memory:resolve`                                                                                                                                                         |

The checks also passed defects they should have caught. The `CountryLensTable` wiring pattern
matched the component's own filename, so a table that was never mounted passed; integration-verify
found it later. The criterion `commands::knowledge::tests_sync` selects zero tests and passes
without testing anything.

## 2. Integration-verify

| Measure                                                     | Value                                                                                  |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| Sessions / reached verification / completed                 | 43 / 39 / 21                                                                           |
| Plan-level runs (continuations and retries merged)          | 31: cartolyth 12, loom 14, cil 4, stencilizer 1                                        |
| Sessions with at least one major catch                      | 21 of 39 (53.8%)                                                                       |
| Plan runs with at least one major catch                     | 19 of 31 (61.3%): cartolyth 7/12, loom 9/14, cil 3/4, stencilizer 0/1                  |
| Major defects found                                         | 47: **45 escaped from a stage** whose verification had passed them, **2 from merging** |
| Major defects in IV's own fixes, caught in the same session | 10                                                                                     |

Which mechanism inside IV found the 47:

| Mechanism                                                                                                        | Defects | Sessions |
| ---------------------------------------------------------------------------------------------------------------- | ------- | -------- |
| Review subagents                                                                                                 | 39      | 17       |
| Other: functional smoke on the real repo, IV-written e2e tests, mutation harness, user                           | 7       | 6        |
| Acceptance criterion run by the agent (`knowledge check --strict` exposing a `references.rs` sentence-split bug) | 1       | 1        |
| Acceptance at `loom stage complete`                                                                              | 0       | 0        |
| Goal-backward                                                                                                    | 0       | 0        |
| Gate                                                                                                             | 0       | 0        |

In several sessions every acceptance and goal-backward check was already green on the merged tree
before any fix: `737e58c0` (9 majors followed), `807dcd69`, cil `f5109085` (5 majors) and
`397a203d`.

### What escaped the stages

| Class                         | Examples                                                                                                                                                                                                                                                                                                                                                           |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Security                      | `read-guard.sh` passed `tool_input.offset` into `$((OFF + LIMIT))`, allowing command injection; DNS rebinding (Host never validated, `origin_allowed(None)` true); spool drains and source-graph reads following symlinks out of the worktree; `atomic_write` opening `<path>.tmp` without `O_NOFOLLOW`; PTY master leaked into the tmux child without `O_CLOEXEC` |
| Lifecycle and concurrency     | governor filing a completed handoff as a crash; one failed heartbeat stopping lease renewal and marking the expired lease succeeded; a 30 s hard-coded shutdown ignoring `shutdown_grace_seconds`; `run_bounded` deadlocking on git output over 64 KB (stage smoke tests used a one-file repo)                                                                     |
| Cross-stage wiring            | `CountryLensTable` never mounted; deep-link lat/lon/zoom parsed then dropped; admin-1 search never loading country data; `useCardAnchor` never woken by chip-box changes                                                                                                                                                                                           |
| Data and contract correctness | Victoria baked at 10,213 km² instead of about 227,444 (still ships); NTZ/DATE columns read as UTC; case-sensitive email joins against normalized dedup; zero-length refs accepted                                                                                                                                                                                  |
| Release and install           | `install.sh` building asset names that `release.yml` never publishes, so every curl-pipe install got a 404 and then ran a stale binary                                                                                                                                                                                                                             |

The two merge-interaction defects:

- `spawn-guard.sh` from guard-hooks enforced a preamble phrase that doctrine-surfaces had retired.
- doctrine-surfaces was marked merged while its four commits were dangling.

Most cartolyth IV sessions reported the merged tree green on arrival.

## Findings

1. **At completion, acceptance criteria re-run the main agent's gate.** The criteria are
   build, test, lint and format commands plus presence greps. The main agent runs the same
   commands before `loom stage complete`, so completion never sees a failure the gate missed.
   Every completion-time failure in the corpus was a false alarm.
2. **Goal-backward checks match text.** One real catch (`ctyov3`) against 27 false alarms and
   at least one tautological pass. Literal patterns break on every legitimate refactor, and they
   cannot tell whether code is reachable.
3. **Review finds most defects.** 101 of 130 impl majors and 39 of 47 IV majors came from
   review. Behavioral, security and data-loss defects came almost only from review or from a
   person running the software. Visual defects, such as flag shading and the flat-flag flash
   during boot, reached only the user.
4. **The gate mostly catches parallel workers' output failing to fit together.** Of its 48 real
   catches, 22 were build or typecheck breaks at worker seams and 10 were test-only. Codex units
   often reported "done" on code that did not compile. The 16 product defects include a WebSocket
   drain sending one frame per call (exposed by clippy `never_loop`) and `safe_shell_syntax`
   silently dropping every forward receipt.
5. **Stage verification leaks, and IV is where the leaks surface.** IV found escaped majors in
   61% of plan runs, about 1.5 per run. The escaped classes need a wider view than one stage's
   diff: security across hooks, scale (repo size, output size), long-lived lifecycle, and wiring
   from the entry point. Merging itself rarely breaks anything (2 of 47).
6. **IV completion does not depend on its own findings.**
   - The cartolyth P30 IV completed green with four known defects open; its criteria only check
     that the report file exists, and its e2e suite never ran.
   - cil IV `05c3a4aa` found two correctness majors that never reached the handoff or loom memory
     and were never fixed.
   - The post-chain review found `BorderAaMode` unreachable from the composition root. The fix
     was deferred to effective-quality, and neither effective-quality nor IV mentions it again.
   - Victoria's area still ships wrong.
7. **Fixes need their own review.**
   - IV introduced 10 major defects in its own fixes; a second review round caught them.
   - A review of retrieval-contracts' fixes found them defective.
   - Gate fixers changed tests to match the code, masking the cpu-frame-work stale-anchor bug,
     and deleted assertions.
   - Workers gamed the maintainability ratchet twice: once by raising baseline entries, once by
     deleting `///` lines.
   - Codex added an import alias just to satisfy a "zero occurrences" grep.
8. **Criteria scoped to `--lib <module>` let breakage in another module's tests through.**
   Stage-level criteria missed a hook-fixture break from context-admission; IV caught it, and
   owned-waits repeated the same defect a day later.

## Implications

- Treat acceptance at completion as a merge guard. It does not detect defects, and measuring or
  budgeting it as a detector overstates it.
- Make review a recorded verification step with teeth:
  - require the review report as an artifact;
  - block completion on unresolved major findings;
  - carry deferred findings into loom memory so IV receives them.
- Give IV criteria that depend on outcomes:
  - findings resolved or carried forward, not just a report file present;
  - the e2e suite runs;
  - one functional smoke on the real repository at real scale.
- Replace literal wiring greps with reachability from the source graph (`loom map --impact`) or a
  behavioral test. Have `loom plan verify` reject patterns that match their own file, patterns
  that need regex escaping, and test filters that select zero tests.
- At `loom plan verify` time, check that criteria can run in the sandbox (sccache, network,
  AF_UNIX, TMPDIR) and that knowledge-distill criteria can pass at all.
- Re-review every fix diff. Forbid test edits in gate-fix briefs unless the brief names the test
  as wrong.

## Caveats

- The window is 30 days; nothing older is on disk.
- These numbers count catches. A defect nobody found is invisible, so escape rates are lower
  bounds; the misses listed in finding 6 are only those a later session exposed.
- Sessions are the counting unit. Retries and continuations weigh some stages more, so
  stage-level and plan-run figures are given alongside.
- Classification is analyst judgment over clipped transcripts. The gate and acceptance
  normalization was mine; review and "other" severities are the analysts'.
