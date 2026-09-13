# W2 — Poll-resistant useful-progress liveness

Plan stage: owned waits, second worker after W1. Lane: Codex `gpt-5.6-sol`, effort `xhigh`, Bash timeout `600000` ms.

Separate heartbeat observation time from useful progress, make repeated `loom subagents` polling
guardable through the real tokenized command path, and update the single-source waiting doctrine for
W1's explicit owned wait. Workers NEVER spawn subagents.

The hook source root is `loom-hooks/`; edit only the paths below. Do not run git or write `.loom/`. Do not run the full test suite, formatter, linter, or
type checker. You may run one narrow check once if certain. Report changed files, assumptions, and
unresolved issues.

## Owned paths

Hook and doctrine paths:

- `loom-hooks/post-tool-use.sh`
- `loom-hooks/poll-guard.sh`
- `loom-hooks/session-start.sh`
- `loom-hooks/subagent-stop.sh`
- `loom-hooks/teammate-idle.sh` (created by worker-evidence W1)
- `loom-hooks/_progress-classification.sh` (new)
- `loom-hooks/_post-tool-heartbeat.sh` (new extraction)
- `loom-hooks/tests/progress-heartbeat.sh` (new)
- `loom-hooks/tests/poll-guard-subagent-waits.sh` (new)
- `loom-hooks/tests/run-all.sh` (current counterpart)
- `loom/src/fs/permissions/constants.rs`
- `loom/src/fs/permissions/tests/constants_tests.rs`
- `loom/src/fs/permissions/tests/hooks_tests.rs`
- `CLAUDE.md.template`
- `loom/src/orchestrator/signals/tests_doctrine_waiting.rs`
- `loom/src/orchestrator/signals/format/helpers.rs`

Rust liveness paths:

- `loom/src/orchestrator/monitor/heartbeat.rs`
- `loom/src/orchestrator/monitor/heartbeat_store.rs` (new extraction)
- `loom/src/orchestrator/monitor/mod.rs`
- `loom/src/orchestrator/monitor/detection.rs`
- `loom/src/orchestrator/monitor/events.rs`
- `loom/src/orchestrator/monitor/hung_latch.rs`
- `loom/src/orchestrator/monitor/heartbeat/tests.rs`
- `loom/src/orchestrator/monitor/tests/heartbeats.rs`
- `loom/src/orchestrator/monitor/tests/ceiling_retries.rs`
- `loom/src/orchestrator/core/event_handler.rs`
- `loom/src/orchestrator/core/heartbeat_apply.rs`
- `loom/src/fs/session_files/exact.rs`
- `loom/src/models/session/methods.rs`
- `loom/src/commands/status/data/heartbeat_facts.rs`
- `loom/src/commands/status/data/collector_activity_tests.rs`

## Heartbeat contract

Extend the backward-compatible heartbeat schema with `progress_at: Option<DateTime<Utc>>` and a
tagged `activity_kind: Progress | Observation`. `timestamp` remains the latest observed tool time so
context tokens, transcript path, ceiling enforcement, and operator visibility remain fresh. For a
legacy record lacking the new fields, `effective_progress_at()` returns its timestamp.

Extract heartbeat filesystem helpers from the 399-line `heartbeat.rs` into `heartbeat_store.rs` and
re-export existing public names, keeping both files under 400 lines. Extract the heartbeat write block
from the 499-line post-tool hook into `_post-tool-heartbeat.sh`; keep every edited file under 400 lines.
Embed/install both new sourced libraries through `LOOM_HOOKS` and pin that they are not registered as
standalone hooks.

`_progress-classification.sh` consumes the canonical token stream from `_common.sh`. A Bash call is
`Observation` only when every executable segment is a recognized status-only operation, including
`loom subagents list|harvest|watch`, `git status`, and the existing narrow read-only poll set. Handle
absolute `loom`, `command`, `env` assignments/`-u`, `timeout`, separators, and pipelines through the
existing tokenizer; any malformed, mixed, or unknown command is `Progress`. Non-Bash tools retain
progress semantics.

Session start seeds both timestamps. Subagent terminal evidence is progress; teammate idle is an
observation. On progress, write both timestamps as now. On observation, advance `timestamp` while carrying forward
the prior validated `progress_at`; if none exists, carry the prior legacy timestamp. Preserve the
existing heartbeat owner lock, exact Loom-session check, no-follow behavior, parent/subagent context
separation, resident-token measurement, and ceiling checks. Classification can change liveness fields
only; it must never suppress those metrics or checks.

Make `HeartbeatWatcher` judge stage and adjudicator stalls from `effective_progress_at`, while
reporting observation age separately. `HeartbeatUpdate` records whether useful progress advanced;
only that condition clears an existing hung latch. Pass the effective progress time through
`HeartbeatReceived` and the exact locked session update. `Session::last_active` advances monotonically
to useful progress only; status-only observations still update context/transcript facts. Status
heartbeat facts compute stale/working from useful progress and retain the latest activity string so an
operator can see that the fresh observation was only polling.

## Poll guard and doctrine

Count actual tokenized `loom subagents list|harvest|watch` invocations, including safe wrapper and
environment-prefix forms. Allow the first owned W1 watch. Repeated unchanged list/harvest calls warn,
then deny under the existing switch; repeated watch calls point to the active wait/`AlreadyWaiting`
result and are denied before another model-driven loop forms. Quoted prose, heredoc bodies, lookalike
commands, different subcommands, and a command segment that performs work do not match.

Replace Rule 6's frozen block in `CLAUDE.md.template` and its byte-identical test constant. It must say:
spawn workers, capture their IDs, run one background `loom subagents watch --worker ... --timeout
3600`, treat exit 0/2/3/4/5 distinctly, harvest terminal reports once, and never re-arm or poll. State
that timeout is a wait deadline, not proof of death, and that only exact authoritative terminal
evidence permits completion. Keep the block absent from generated stable prefixes. Update
`format_subagent_timeout_section` to reference the same single owned wait and remove its current advice
to issue another watch.

## Regression proof

Shell tests drive the installed hook assets with exact Bash payloads. Cover the T3-shaped 902-list
sequence without sleeping: the first call is allowed, thresholds warn/deny, wrapper/env forms normalize
to the same key, and prose/lookalikes stay allowed. Prove a legitimate single watch is allowed while a
repeat cannot create a second wait. Assert polling advances observation time and resident context but
does not advance progress; a real edit/build/worker-terminal event does.

Rust fake-clock tests prove continuous observations cannot keep a stale-progress session healthy,
cannot clear its hung latch, and still preserve changing context readings. Cover legacy heartbeat
fallback, monotonic session progress, reordered records, stale predecessor ownership, stage versus
judge files, status rendering, and a fresh real-progress transition that clears the latch exactly once.

Done means T3-style polling is bounded at the hook and runtime layers, useful progress drives stall
decisions, context-ceiling telemetry remains live, and every waiting-doctrine surface agrees with W1.
