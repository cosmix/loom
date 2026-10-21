# Usage Evidence

> Usage reasons, scope, trials

## Completeness

Each provider report carries `completeness` (`commands/usage/completeness.rs::assess`): `measured`,
or `partial` with reason codes in a fixed order: `missing-roots`, `unreadable-files`,
`malformed-rows` (unparseable lines), `unreadable-session-records`, `missed-session-transcripts`
(a session record names an in-window transcript the report did not read), `unknown-usage-rows`,
`ambiguous-rows`, `codex-scope-unavailable`, `malformed-forward-receipts` (a damaged or truncated
receipt line, Codex only: it can hide a project thread), `forward-thread-missing` and
`forward-receipt-unthreaded`. The first four and the receipt reasons are the evidence-loss
reasons. `fallback_coverage_rows` is a coverage count and `missing_timestamp_rows` counts every row
type without a timestamp, so neither is a reason; a usage row without a timestamp is dropped
without the report reading partial
([concern](../concerns/token-accounting-and-proof-defects.md#delivery-proof-and-quota-continuity-defects)).

Session-record inspection (`missed_claude_transcripts`) canonicalizes the work dir once, because
from a stage worktree `.loom/work` is a symlink and the bounded record read opens its root without
following links
([mistake](../mistakes/completion-runner-blocks.md#a-symlinked-work-dir-made-every-session-record-read-as-unreadable)).
Per-record diagnostics print to stderr through `inline_safe`.

## Codex Scope

The Codex store is shared by every project on the machine, so a Codex row counts for a project only
when a forward receipt of that project names its thread (`codex_scope.rs`, `CodexScope`); every
other row is foreign and dropped. Without readable receipts the scope is `Unavailable` and the
report reads `codex-scope-unavailable`; `--all` is unscoped. A receipt's `observed_at` is its
PostToolUse instant, an end time, so rows are windowed by their own timestamps and a thread is
checked for missing rows only when its receipt lies in `[since + MAX_FORWARD_SECS (600), until]`
(`forward-thread-missing`); `forward-receipt-unthreaded` counts receipts without a thread id
observed in `[since, until]`. `from_receipt_threads` builds a scope from receipt threads without a
`ForwardJoin`. Scope and session-record completeness read only the current `.loom/work`;
`loom init` archives and removes it (`commands/init/cleanup.rs:177-187`), so a window before the
last init reads 0 Codex tokens.

## Run Evidence

`loom usage --run-evidence <declaration.json> --project <dir> --since <rfc3339> --until <rfc3339>`
(`run_evidence.rs`, `run_evidence_stages.rs`) writes one run's `RunEvidence`, the input
`--compare` pairs. The declaration (at most 1 MiB) supplies the identities the tooling cannot
measure; token vectors, retries, fixes, the critical path and open review findings are measured.
The stage set is every stage that started inside the window; a stage straddling either bound
aborts the producer, and stages are sorted by id before that check. The producer refuses in-scope
usage from a provider the declaration does not assign, and an assigned provider reading `partial`
marks the run's provenance partial. Finding severity maps `critical`→critical, `major`→high,
`minor`→medium, anything else→low. Produced evidence carries `notification_latency_ms: null`. The
flag conflicts with `--compare`, `--all`, `--provider`, `--stage` and `--plan`, and requires
`--project`, `--since` and `--until`. The procedure that uses it:
[Live Paired Trials](#live-paired-trials).

## Live Paired Trials

For a change that needs live evidence (frozen replays of later plans, or a question the offline
comparison cannot settle). Live trials consume subscription quota, so each needs operator
scheduling and authority, and runs from an ordinary shell with no other Claude or Codex session
open: a stage session cannot run `loom run`, and its own consumption would count as concurrent use
(`comparison_quota.rs`, `unrelated_use`). No stage runs this procedure.

1. **One clone per run.** Each run gets a new clone at `<experiment-root>/<intervention>-<arm>-<run>`,
   outside this repository, so no two runs share a `~/.claude/projects` slug. Reset the clone's
   checked-out branch to the pinned revision and use it as the plan's base branch; no branch is
   created. Write the same `.loom/config.toml` into every clone of an experiment (`.loom/` is
   gitignored): no intervention dimension covers configuration, so its digest is part of the
   environment identity, and an intervention that is a configuration change ships as a revision
   that changes the default.
2. **Shared install, one arm at a time.** A clone isolates only `.loom/work` and
   `.loom/config.toml`. The `loom` binary, the hooks under `~/.claude/hooks/loom/`, the agents,
   the skills and `~/.claude/CLAUDE.md` are shared, so arms run one at a time, and before each arm
   `./dev-install.sh` installs that arm's revision from its clone. The declaration's
   `environment_id` is a digest of what must not vary between arms: host and OS, the `claude` and
   `codex` CLI versions, `~/.loom/config.toml`, the clones' shared `.loom/config.toml`, and
   `~/.claude/settings.json` unless `./dev-install.sh` rewrites it (then its digest is
   provenance). It never includes the installed binary's revision or the installed assets: those
   come from the arm's revision, the declared `implementation_revision`, and `--compare` requires
   `environment_id` equal (`incomparable-environment`), so an identity carrying them makes every
   implementation-change pair incomparable. Record the installed binary's revision and the
   installed assets' digests per run in `<run>/provenance.json` beside the run's `RunEvidence`, and
   check the binary's revision equals the declared `implementation_revision` before the run starts.
3. **Freeze the inputs.** Fix the workload fixtures, the source-input revision and digest, the
   acceptance contract, the required commands, the review dimensions, and the providers, models
   and effort. Name the one intervention; the manifest lists one changed dimension, because an
   empty `changed_dimensions` is invalid.
4. **Cover the outcomes.** Include successful work, expected failures and review-heavy work, with
   the same checks and independent review in both arms. A clone at a new path starts cold; a warm
   start is a second run in the same clone inside the first run's cache TTL, started only after
   the first run's evidence is produced and saved (step 7). Its window must not overlap the first
   run's: the producer takes a run's stages by window, so the first run's stage records in the
   same `.loom/work` stay out of the second run's evidence.
5. **Interleave and keep everything.** Interleave baseline and candidate order across
   repetitions, and keep every outcome, including failures, retries, fixes and incomplete runs.
   A missing measurement is unknown, never zero.
6. **Measure only canonical rows.** Only `measured-canonical` rows of `loom usage --provider all
   --since <start> --until <end> --json` are measurements. `duplicate-exact`,
   `ambiguous-conflict`, `fallback-coverage`, `synthetic` and `unknown-usage` rows, and every
   absent dimension, go into the artifact as missing. A clone isolates Claude slugs only; Codex
   rollouts share `~/.codex/sessions`, so copy each run's rollouts out at the end of the run (they
   are not retained), and rely on forward-receipt scoping to keep another arm's threads out.
7. **Produce the evidence at once.** Run `loom usage --run-evidence <declaration.json> --project
   <clone> --since <start> --until <end>` as soon as the run ends, before the next run starts in
   any clone, and keep the file: it is the run's evidence and is never produced again later,
   because review rounds and dispute rulings recorded afterwards change `open_findings`. The
   declaration names every provider the run used; the producer refuses a run with in-scope usage
   from an undeclared provider. The independent reviewer replaces the local finding ids
   (`<stage>:F-<round>-<k>`) with cross-arm ids and keeps the severity mapping (critical, major,
   minor and unspecified map to critical, high, medium and low). Assemble the pairs into an
   artifact that declares `"token_rule": "net-total"` and a `"latency_tolerance_ms"` (produced
   evidence carries `notification_latency_ms: null` and needs the tolerance to reach
   `supported-candidate`), fix the pair count and the stopping rule before the first pair, and run
   every retained artifact through `loom usage --compare <artifact> --json`. Any new defect,
   omitted requirement, weakened check or review, failed semantic completion, or latency
   regression beyond the tolerance rejects the candidate, whatever the average. A rejected
   candidate is reverted through the operator's ordinary deployment procedure, and its evidence
   is kept.
8. **Reading the verdict.** The net-total rule needs at least 3 pairs per artifact (fewer read
   `insufficient-pairs`) and judges the mean difference against 2 standard errors
   ([Offline Comparison](token-accounting-and-receipts.md#offline-comparison-loom-usage---compare)).
   A check that asks for a median computes it from `.pairs[].baseline_provider_tokens` and
   `.pairs[].candidate_provider_tokens` of the `--compare --json` report. Quantities
   `RunEvidence` does not carry are read from `loom usage --project <clone> --since <start>
   --until <end> --json` over the same window: API calls from
   `.provider_ledger.providers[].coverage.measured_responses`, cache read and cache write from the
   provider token vectors, tokens before the first edit from `.lifecycle.orientation`. A window
   opened before the clone's last `loom init` cannot be measured: Codex scope and session-record
   completeness read only the current `.loom/work`.
