# Sol worker brief — durable forward receipts

## Outcome

Create a lifecycle record separate from the measurement stage's usage-only
`execution_receipt.rs`. The single forwarder Bash call stays foreground with
its existing 600000ms timeout. It starts one exact background companion job,
waits that exact job, and never declares completion from the Bash acknowledgement.

## Exact ownership

Sol changes only:

- new `loom/src/models/forward_receipt.rs` and `loom/src/models/mod.rs`;
- new `loom/src/commands/hook/forward_receipt.rs`,
  `loom/src/commands/hook/mod.rs`, `loom/src/cli/types_ops.rs`, and
  `loom/src/cli/dispatch.rs`;
- `loom-hooks/codex-forward.sh`, `loom-hooks/codex-forward-guard.sh`, and
  `loom-hooks/post-tool-use.sh`;
- new `loom/src/commands/usage/forward_join.rs`, its declaration in
  `loom/src/commands/usage/mod.rs`, and its call from `parse_all` in that same
  file after `transcript::parse`/agent-type assignment;
- focused tests for exactly those hook/model/usage files.

Terra owns `commands/subagents/**`, poll guard, and Codex doctrine. No shared
file is jointly owned. Sol does not alter `ledger.rs`, `render.rs`, or signals.

Sol owns `loom-hooks/tests/run-all.sh` in wave 1 and registers its new
codex-forward*/forward-receipt hook tests there.

## Grounded transport contract

- The guard's exact eight-word argv checker cannot inject an argument or env
  (`is_exact_forward_command`, `loom-hooks/codex-forward-guard.sh:106-120`;
  `parse_shell_words` is the quoting state machine before it); it authorizes
  only.
- The wrapper cannot write `.loom/work` through the worktree symlink
  (`loom-hooks/codex-forward.sh:32-37`); only the hook-side Rust writer persists.
- The companion lane is the only lane on Linux
  (`loom-hooks/codex-forward.sh:211`) and runs Codex under its fixed
  `workspace-write` sandbox inside the outer Bash sandbox; the direct lane
  (`--sandbox danger-full-access`, `codex-forward.sh:150-151`) is a macOS
  fallback that relies on the outer sandbox alone.
- A transcript associates an assistant tool use (`id`, name, input) with the
  user tool result's `tool_use_id` (`commands/usage/transcript.rs:128-154,
  237-284`). Use that observed identity, not a fictitious hook field.
- A natural harness background acknowledgement may expose
  `toolUseResult.backgroundTaskId` and a task-output path (observed
  `/home/dkaponis/.claude/projects/-home-dkaponis-src-loom/97bc4c4e-b8b8-4e3a-a028-cc756920c0e2.jsonl:133-134`).
  It is an optional transcript fallback, not a requirement or completion proof.

## Receipt model and authoritative state

`ForwardReceipt` has `schema:1`, `receipt_id`, parent session, safe agent ID,
tool-use ID, stage ID, Loom session ID, backend (`companion` or `direct`),
exact backend ID, state, observation timestamp, and optional exit code.
Companion observations also carry optional `codex_thread_id`, populated only
from that exact job's structured record. It is unknown until observed. Direct
mode's backend ID is already the Codex thread ID. Carry requested model/effort
as bounded diagnostics, never as join keys.
`receipt_id` is deterministic: SHA-256 of the domain-separated canonical UTF-8
tuple `loom.forward-receipt.v1\0parent_session_id\0agent_id\0tool_use_id\0stage_id\0loom_session_id`.
Reject empty/unsafe components; never derive identity from model, effort, time,
or a newest file.

The hook writer persists append-only start/terminal observations with a lock,
atomic/no-follow discipline. Its state record also carries a validated private
locator for the exact companion job record; it is not displayed. A start with a
different backend ID, duplicate conflicting state, malformed data, or a
terminal before start becomes `unknown`, never success.

Terminal `succeeded` requires authoritative evidence: the exact companion job
record says completed/done, or (direct mode only) the wrapper's own captured
`codex exec` process exited 0 after emitting its exact `thread.started` ID.
Model text is never terminal evidence. Failed/canceled are terminal but
unsuccessful; queued/running/missing evidence is nonterminal unknown.

## Wrapper and marker channel

The wrapper changes internally to `task --background --json`, parses only its
returned job ID, then runs `status <that-id> --wait --json` and `result
<that-id> --json`. It removes all newest-three/newest-rollout logic. Direct
mode runs `codex exec --json`, captures only its own `thread.started` ID, keeps
the current outer Seatbelt boundary and closed stdin, and never scans sessions.

Capture provider stdout/stderr in a private temporary file. The wrapper alone
writes a bounded prefix channel before releasing captured provider output:

```text
LOOM-FORWARD-START {"v":1,"backend":"companion","job_id":"task-..."}
LOOM-FORWARD-END {"v":1,"backend":"companion","job_id":"task-...","outcome":"succeeded","exit_code":0}
```

Decode only at most these two schema-validated lines before a fixed separator;
reject duplicates, extra prefix data, mismatched backend IDs, and marker-like
provider output after the separator. This prevents model-printed text from
being accepted as a wrapper event. The final stdout remains compact.

During a genuinely foreground run, the transcript's matching inline completed
tool result supplies this prefix after natural completion. If the harness
backgrounds that long Bash request, its observed task-output handle may expose
the START marker early; the hook adapter binds it to the same tool-use ID. No
early exact backend ID is promised when no acknowledgement is observable.

## Hook writer and usage attribution

`post-tool-use.sh` invokes `loom hook forward-receipt --transcript <path>`
after its existing heartbeat work. The hidden Rust command parses only the
matching forwarding Bash tool-use/result and its bounded marker channel; it
does not persist raw tool-result text or assume hook output fields. If data is
not visible, it writes no success record and leaves the job unknown.

`commands/usage/forward_join.rs` joins the Claude forwarding transcript through
its actual parent/agent/tool-use tuple. A Codex rollout does not have that tuple:
join its structured session/thread ID only to the receipt's observed exact
`codex_thread_id` (or direct backend thread ID). Validate unique scope across
all candidate receipts; absent/conflicting thread identity remains un-attributed.
Invoke this join for both provider-normalized rows and the legacy Claude parse
result, using the prior stage's actual entry points. It adds metadata without
changing token totals, summing usage receipts again, or attempting recovery.

Root selection preserves `usage_work_dir`'s current isolation: explicit project
uses only its own validated state root; no caller-root fallback. For `--all`,
leave lifecycle joins un-attributed unless the individual transcript's project
and state root can be independently proved from structured source metadata.
Do not reverse an ambiguous escaped project slug into a filesystem path or
choose the first project. An optional new `--forward-receipts-root` is a
read-only single-project test/import seam, distinct from usage-only
`--receipts-root`; reject it with `--all`, validate the declared project scope,
and never search a different root on missing data. Fixtures cover two projects
with matching-looking foreign receipts, explicit project, current project,
missing root and `--all`; cross-root joins must remain absent with diagnostics.

Place the reusable pure decoder/observation reader under the owned model helper
namespace `loom/src/models/forward_receipt/` (new), registered from
`forward_receipt.rs`; the hook command owns persistence and calls that pure
reader, while Terra's adapter imports only the pure reader. Add the namespace
to the stage's file ownership. The marker prefix ends at an explicit separator;
the START line is flushed immediately after the exact ID is known, END only
after authoritative completion. Bounded reads tolerate a still-incomplete
prefix as active/unknown, never as a missing-worker success.

The pure decoder validates observed task-output locations against the actual
harness hierarchy with no-follow, regular-file, ownership and byte limits;
tests inject roots. A marker ID alone cannot select an arbitrary path. Companion
locator resolution uses the wrapper's observed installed state root plus exact
job ID and validates the record's identity. If the terminal direct-process
evidence or companion state is unavailable, remain unknown; do not trust model text.

## Required: hook tests must not inherit a live session's identity

`codex-forward-guard-blocks-edit.sh`, `codex-forward-guard-quoting.sh`, and
`codex-forward-guard-bash-companion-only.sh` currently inherit
`LOOM_STAGE_ID`/`LOOM_SESSION_ID`/`LOOM_WORK_DIR` from the running stage
session, and the guard writes its ledger through them — a gate run inside any
session that also runs `bash loom-hooks/tests/run-all.sh` appends fake forward
records to that session's own live `.loom/work/subagents/<stage>/codex.jsonl`.
`run-all.sh` (or each `codex-forward*` test individually) must clear
`LOOM_STAGE_ID`, `LOOM_SESSION_ID`, `LOOM_WORK_DIR`, `LOOM_SESSION_TYPE`, and
`LOOM_MAIN_AGENT_PID` before invoking the guard, and point any state root the
guard writes through at a per-test tempdir instead. Add a regression that
runs the guard tests with those five variables set to a scratch work dir and
asserts no record lands outside the test's own tempdir.

## Focused tests

Test canonical receipt hashing, unsafe identity rejection, start/terminal
conflicts, prefix-channel spoofing, direct exit versus printed text, exact
companion completed/failed/canceled records, absent inline result, natural
background-handle observation, usage joins with same-model siblings, and the
live-identity-isolation regression above. The orchestrator alone runs
verification.
