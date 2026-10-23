# Code Review: Loom Observability & Hook Improvements

**Plan:** PLAN-loom-observability-and-hooks | **Generated:** 2026-05-12 16:33 UTC

## Summary

This plan adds human-readable container log streaming to `loom container logs` with format options, demotes the tool-use hook from blocking to advisory JSON warnings, and introduces structured tool-event tracking that enables stuck-agent detection with visibility in `loom status` output. It defers context-velocity monitoring to a follow-up plan and establishes the foundation for future observability improvements through persistent soft-signal storage.

## Changes by Stage

### Hooks Warn-Mode + Tool Event Tracking (hooks-warn-and-track)

**Status:** completed  
**Purpose:** Demote prefer-modern-tools.sh from block to inline-warning (JSON
additionalContext); teach post-tool-use.sh to append structured rows to
.work/tool-events.jsonl; add ToolEvent Rust schema with public reader
helpers.

Use parallel subagents and skills to maximize performance.

HOOK SOURCE-OF-TRUTH:
  loom/src/fs/permissions/constants.rs uses include_str! to embed the
  hooks/*.sh files at compile time. The .sh files are the only source.
  Do NOT edit constants.rs.

SUBAGENT A - hooks/prefer-modern-tools.sh:

- Change `exit 2` to `exit 0`
- Replace stderr nudge with stdout JSON using the pattern from
    hooks/skill-trigger.sh:284-289:
      jq -nc --arg ctx "LOOM_HOOK_WARN: Prefer rg/fd over grep/find. ..." \
        '{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
- Only emit warning when tool_name == "Bash" AND the command actually
    invokes a bare grep/find (re-use existing_common.sh detection to
    avoid false-positives on heredocs, echoes, quoted matches)

SUBAGENT A - hooks/post-tool-use.sh:

- Handle BOTH .tool_result.*and .tool_response.* via jq // fallback;
    do NOT do live-discovery
- After the existing heartbeat update, append one JSONL row to
    ${LOOM_WORK_DIR}/tool-events.jsonl
- Row schema:
      ts, tool, is_error, session_id, stage_id (always populated)
      exit, output_bytes, output_head, output_tail (null if payload
        lacks the data — do NOT fail the hook)
- Use head -c 200 / tail -c 200 for truncation; pipe through
    iconv -c -t UTF-8 if available
- Use jq -nc to build the JSON; if jq is unavailable, bail silently
    (must not break heartbeat)

SUBAGENT A - hooks/tests/ (new directory):
  Create these shell tests with deterministic JSON stdin:
    prefer-modern-tools-grep.sh:  expects JSON additionalContext with LOOM_HOOK_WARN
    prefer-modern-tools-find.sh:  same shape for find command
    prefer-modern-tools-rg.sh:    expects empty stdout (no warning)
    prefer-modern-tools-quoted.sh: command="echo \"use grep\"", expects no warning
    post-tool-use-tool-event.sh:  exports temp LOOM_WORK_DIR, feeds fixture
      JSON, asserts heartbeat AND tool-events.jsonl rows parseable
  Wire into hooks/tests/run-all.sh driver

SUBAGENT B - loom/src/hooks/events.rs:
  Add alongside HookEventLog (lines 16-29):

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolEvent {
        pub ts: String,
        pub tool: String,
        pub is_error: bool,
        pub session_id: String,
        pub stage_id: String,
        #[serde(default)]
        pub exit: Option<i32>,
        #[serde(default)]
        pub output_bytes: Option<u64>,
        #[serde(default)]
        pub output_head: Option<String>,
        #[serde(default)]
        pub output_tail: Option<String>,
    }

    pub fn read_tool_events(work_dir: &Path) -> io::Result<Vec<ToolEvent>>;
    pub fn tail_tool_events(work_dir: &Path, n: usize) -> io::Result<Vec<ToolEvent>>;

  Reader skips blank lines, tolerates malformed lines (warn via tracing).

  Unit tests in #[cfg(test)] mod tests:
    - parses minimal row (only required fields)
    - parses full row (all best-effort fields)
    - tail returns correct slice
    - missing file returns Ok(empty)
    - malformed line is skipped

SUBAGENT B - loom/src/hooks/mod.rs:
  Current: `mod events;` (private). Change to `pub mod events;` OR add
    pub use events::{ToolEvent, read_tool_events, tail_tool_events};
  Either way, stage 4 must be able to import via
    use crate::hooks::events::tail_tool_events;

SUBAGENT FILE ASSIGNMENTS:
  Subagent A - Hook Scripts (loom-software-engineer):
    Files Owned: hooks/prefer-modern-tools.sh, hooks/post-tool-use.sh,
      hooks/tests/ (new directory)
    Files Read-Only: hooks/_common.sh, hooks/skill-trigger.sh
  Subagent B - Rust Schema + Module Export (loom-software-engineer):
    Files Owned: loom/src/hooks/events.rs, loom/src/hooks/mod.rs
    Files Read-Only: loom/src/hooks/validators.rs, loom/src/hooks/config.rs

NO FILE OVERLAP between subagents confirmed.

SUBAGENT MEMORY: loom memory only. Record mistakes, decisions, surprises.
Never use loom knowledge update. Never use Claude Code auto-memory.

#### Files Changed

No changes recorded.

#### Key Decisions

- used pub mod events; plus re-export in mod.rs *(makes crate::hooks::events::tail_tool_events accessible while keeping flat import via crate::hooks::ToolEvent)*

#### Notes

- gotcha: cargo test hooks::events appears to return 0 tests when tailing output — the lib binary results appear FIRST and the integration binaries LAST. Use head -20 not tail to see lib test results

### Container Logs Streaming Formatter (log-formatter)

**Status:** completed  
**Purpose:** Re-architect `loom container logs` from exec() to spawn-with-stdout-pipe so
loom owns the stream, then format it. Default --format human, opt-in
--format json preserves the raw byte stream. Add --show-thinking and
--verbose flags. Update dispatch and completions.

Use parallel subagents and skills to maximize performance.

STREAMING REDESIGN (subagent A):
  Current src/commands/container/logs.rs:230-244 calls
  Command::new(...).args(...).exec() which replaces the loom process. The
  new path:
    - --format json: spawn the runtime logs command, copy stdout bytes to
      loom stdout verbatim, propagate exit code. No reserialization.
    - --format human: spawn with piped stdout, read line-by-line via
      BufReader, parse each line as a stream-json event, render via the
      new log_format module. On a malformed line, render a single
      diagnostic and continue. On EOF, exit. On --follow, keep reading.

STREAM-JSON EVENT CATALOG (subagent A reference):
  Each line is JSON with top-level "type":
    - assistant: message.content[] has blocks of type text|tool_use|thinking
    - user: message.content[] has tool_result blocks (with content +
      is_error). May ALSO carry tool_use_result field at message level
      with same bytes — render once.
    - system: meta events (suppressed by default)
    - rate_limit_event: rate-limit info (suppressed by default)
    - any other: render "[unknown event: <type>]" diagnostic

RENDER RULES:
  Default human format:
    - Assistant text -> prose + thin separator
    - Assistant tool_use -> one line "-> ToolName(arg=truncated)"
      (string values longer than 80 chars truncated to 77 + "...")
    - User tool_result -> one line "<- ok (N bytes)" or
      "<- error: <first line>"; if body begins "PreToolUse:" label as
      "[hook block]"; if body contains "LOOM_HOOK_WARN:" label as
      "[hook warn]"
    - Assistant thinking -> suppressed (with --show-thinking: show first
      line prefixed "[thinking] ")
    - system / rate_limit_event -> suppressed (with --verbose: footer
      summary)
    - Duplicated tool_use_result with same bytes as tool_result block -> discard

CLI SHAPE (subagent B):
  loom container logs <stage_id> [--format human|json] [--show-thinking]
    [--verbose] [--follow|-f] [--tail N]
  Update src/cli/types.rs ContainerCommands::Logs struct.
  Update src/cli/dispatch.rs to pass new fields to logs::execute.
  Update src/completions/dynamic/commands.rs to include new flags.

FIXTURES (subagent C):
  Create loom/tests/fixtures/stream-json/ with one .jsonl per scenario
  and a matching .expected.txt for the human render. Required fixtures:
    assistant-text.jsonl
    tool-use-and-result.jsonl
    tool-use-with-dup-result.jsonl  (dedup test)
    thinking-blocks.jsonl
    hook-block.jsonl
    hook-warn.jsonl
    rate-limit.jsonl
    malformed.jsonl  (mix of valid and malformed lines)
    unknown-event.jsonl

  Tests in tests/log_format_test.rs compare formatter output to .expected.txt
  via assert_eq! (no new crate dependencies).

SUBAGENT FILE ASSIGNMENTS:
  Subagent A - Streaming + Formatter (loom-software-engineer):
    Files Owned:
      src/commands/container/logs.rs
      src/commands/container/log_format.rs (new)
      src/commands/container/mod.rs
    Files Read-Only:
      src/orchestrator/terminal/container/mod.rs (verify stream-json invocation)
  Subagent B - CLI Wiring (loom-software-engineer):
    Files Owned:
      src/cli/types.rs
      src/cli/dispatch.rs
      src/completions/dynamic/commands.rs
    Files Read-Only:
      src/commands/container/logs.rs
  Subagent C - Fixtures + Tests (loom-software-engineer):
    Files Owned:
      tests/log_format_test.rs (new)
      tests/fixtures/stream-json/ (new directory with .jsonl + .expected.txt)
    Files Read-Only:
      src/commands/container/log_format.rs

NO FILE OVERLAP between subagents confirmed.

SUBAGENT MEMORY (ALL subagents): loom memory only. Record mistakes,
decisions, surprises. Never use loom knowledge update. Never use Claude
Code auto-memory.

#### Files Changed

No changes recorded.

#### Key Decisions

- used loom::commands:: path (not crate::) in types.rs for LogFormat *(types.rs is compiled as part of a binary crate that imports the loom library — crate:: would refer to the binary crate root which doesn't expose commands. loom:: is the correct path to the library.)*

#### Notes

- found: exec() removal from logs.rs requires removing 'use std::os::unix::process::CommandExt' import — subagent A correctly handled this, B had noted the missing import which A already removed
- found: cargo test <filter> only matches test function names, not file names. Use cargo test --test <filename> to run all tests in a specific integration test file.

### Stuck Detection + Soft-Signal Store (stuck-detection)

**Status:** completed  
**Purpose:** Add stuck-agent detection to the orchestrator monitor with a durable
soft-signal store so static loom status (not just live TUI) can show
the flag. Context velocity is deferred to a future plan.

Use parallel subagents and skills to maximize performance.

SHARED API CONTRACT (all subagents build to these exact shapes):

  // src/orchestrator/monitor/tool_analysis.rs
  pub const STUCK_WINDOW_SECS: u64 = 60;
  pub const STUCK_MIN_EVENTS: u32 = 5;
  pub const STUCK_FAILURE_RATIO: f32 = 0.80;

  pub struct ToolAnalysis {
      pub recent_events: usize,
      pub recent_failure_count: u32,
      pub recent_failure_ratio: f32,
      pub last_event_age_secs: u64,
  }

  impl ToolAnalysis {
      pub fn is_possibly_stuck(&self) -> bool {
          self.recent_failure_count >= STUCK_MIN_EVENTS
              && self.recent_failure_ratio >= STUCK_FAILURE_RATIO
      }
  }

  pub fn analyze_session(work_dir: &Path, session_id: &str) -> io::Result<ToolAnalysis>;

  // src/orchestrator/monitor/soft_signals.rs
  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
  #[serde(tag = "kind", rename_all = "snake_case")]
  pub enum SoftSignal {
      PossiblyStuck {
          session_id: String,
          stage_id: String,
          recent_events: usize,
          failure_count: u32,
          failure_ratio: f32,
          emitted_at: String,
          expires_at: String,
      },
  }

  pub const DECAY_WINDOW_SECS: u64 = 120;

  pub fn append(work_dir: &Path, sig: &SoftSignal) -> io::Result<()>;
  pub fn read_active(work_dir: &Path, now: SystemTime) -> io::Result<Vec<SoftSignal>>;
  pub fn read_active_for_session(
      work_dir: &Path, now: SystemTime, session_id: &str
  ) -> io::Result<Vec<SoftSignal>>;

SOFT-SIGNAL STORE SEMANTICS:

- Append-only JSONL at ${LOOM_WORK_DIR}/monitor/soft-signals.jsonl
- File auto-created on first append (mkdir -p parent)
- read_active filters out rows where now >= expires_at
- Detection does NOT re-emit while a non-expired row already exists for
    the same (session_id, kind) — this is the "once-per-crossing" mirror
    of the existing last_budget_exceeded HashMap pattern at detection.rs:29

SUBAGENT A - src/orchestrator/monitor/tool_analysis.rs:

- Use hooks::events::tail_tool_events(work_dir, 50)
- Filter to events within STUCK_WINDOW_SECS of newest event ts
- is_failure_shaped = event.is_error OR event.output_bytes == Some(0).
    Treat output_bytes == None as not-a-failure (payload may lack it).
- Compute the ToolAnalysis struct
- Unit tests: empty file, single event, all-failure window, mixed above
    ratio, mixed below ratio

SUBAGENT B - src/orchestrator/monitor/soft_signals.rs:

- Implement append, read_active, read_active_for_session
- Decay filter on read
- Unit tests: append+read round-trip, decay filtering, missing file
    returns empty, malformed lines skipped

SUBAGENT B - src/orchestrator/monitor/detection.rs:

- In the existing tick loop (detection.rs:127-192), per running session:
  - Skip if non-expired PossiblyStuck already in store for this session
  - Call tool_analysis::analyze_session
  - If analysis.is_possibly_stuck(), build SoftSignal::PossiblyStuck and
      call soft_signals::append
  - Also emit MonitorEvent::PossiblyStuck for live consumers
- Mirror the existing last_budget_exceeded HashMap pattern for
    in-memory dedupe; persistent dedupe is via the store's expiry

SUBAGENT B - src/orchestrator/monitor/events.rs:
  Add new variant:
    PossiblyStuck {
        session_id: SessionId,
        recent_events: usize,
        failure_count: u32,
        failure_ratio: f32,
    }

SUBAGENT B - handlers.rs + event_handler.rs:
  Add a real match arm (log at INFO with session/stage context) for the new
  variant; do NOT use a catch-all wildcard.

SUBAGENT C - Status surfacing across THREE render paths:
  The status command has data model, static graph, compact, daemon
  protocol, and TUI renderer files. Each must learn the new flag.

  Files (with the actual current layout):
    src/commands/status/data/mod.rs       - extend StatusData/StageSummary
      with is_possibly_stuck: bool
    src/commands/status/data/collector.rs - call
      soft_signals::read_active_for_session per session at gather time
    src/commands/status/render/graph.rs   - static render appends [stuck?]
      annotation to the per-stage row (around line 64)
    src/commands/status/render/compact.rs - compact output includes a
      "stuck" token (around line 11)
    src/daemon/protocol.rs                - StageInfo gains
      is_possibly_stuck: bool (around line 123)
    src/commands/status/ui/tui/renderer.rs - TUI per-stage row shows the
      flag (around line 65)

  Match existing flag rendering style (color, spacing). Do NOT change
  column layout.

TESTS:

- cargo test orchestrator::monitor::tool_analysis
- cargo test orchestrator::monitor::soft_signals
- cargo test status_soft_signals (new integration test): write a
    PossiblyStuck row to a temp work dir, build StatusData via the
    collector, assert is_possibly_stuck == true for the matching session
- render/graph_tests.rs snapshot: render a stage row with the flag set,
    assert output contains "stuck?"

SUBAGENT FILE ASSIGNMENTS:
  Subagent A - Tool Analysis (loom-software-engineer):
    Files Owned:
      src/orchestrator/monitor/tool_analysis.rs (new)
      src/orchestrator/monitor/mod.rs (add pub mod tool_analysis)
    Files Read-Only: src/hooks/events.rs
  Subagent B - Store + Detection + Events (loom-software-engineer):
    Files Owned:
      src/orchestrator/monitor/soft_signals.rs (new)
      src/orchestrator/monitor/detection.rs
      src/orchestrator/monitor/events.rs
      src/orchestrator/monitor/handlers.rs
      src/orchestrator/core/event_handler.rs
      src/orchestrator/monitor/mod.rs (add pub mod soft_signals — coordinate
        with subagent A who also edits mod.rs; both append-only edits, distinct
        lines)
    Files Read-Only:
      src/orchestrator/monitor/tool_analysis.rs
      src/orchestrator/monitor/heartbeat.rs
  Subagent C - Status Surfacing (loom-software-engineer):
    Files Owned:
      src/commands/status/data/mod.rs
      src/commands/status/data/collector.rs
      src/commands/status/render/graph.rs
      src/commands/status/render/compact.rs
      src/daemon/protocol.rs
      src/commands/status/ui/tui/renderer.rs
    Files Read-Only:
      src/orchestrator/monitor/soft_signals.rs
      src/orchestrator/monitor/events.rs

MOD.RS COORDINATION:
  Subagents A and B both need to add a "pub mod ..." line to
  src/orchestrator/monitor/mod.rs. This is the only shared file. Stage
  author resolved by having the main agent (not subagents) make both edits
  in one pass after subagents finish, OR sequence the subagents such that
  A finishes first and B reads A's mod.rs edit before adding its own line.
  Prefer: main agent applies both mod.rs edits after collecting subagent
  output.

SUBAGENT MEMORY: loom memory only. Record mistakes, decisions, surprises.
Never use loom knowledge update. Never use Claude Code auto-memory.

#### Files Changed

No changes recorded.

#### Key Decisions

- made soft_signals pub mod instead of pub(crate) *(acceptance criteria rg check uses literal 'pub mod soft_signals' so pub(crate) fails the pattern match)*

#### Notes

- gotcha: cargo test filter 'orchestrator::monitor::tool_analysis' shows '0 passed, N filtered out' in the output TAIL because the lib binary result scrolls past tail -15 — always pipe through rg 'test result' to see all binary results

### Integration Verification (integration-verify)

**Status:** completed  
**Purpose:** Full build, lints, code review, end-to-end smoke of the five active
scenarios. Context velocity is explicitly deferred — do NOT test for it.

Use parallel subagents and skills to maximize performance.

Never use Claude Code auto-memory.

CONTEXT GATHERING:

- Read doc/plans/IN_PROGRESS-PLAN-loom-observability-and-hooks.md
- loom memory show --all
- Read doc/loom/knowledge/*.md

BUILD & TEST (zero tolerance — fix ALL warnings/errors):

- (cd loom && cargo test)
- (cd loom && cargo clippy --all-targets -- -D warnings)
- (cd loom && cargo build --release)
- bash hooks/tests/run-all.sh

CODE REVIEW (parallel loom-code-reviewer subagents):

- Architecture: module boundaries, soft_signals API, status data-model
  changes
- Test coverage: are fixtures realistic, do tests exercise both success
  and failure paths
- Security: path traversal in JSONL parsers, unbounded memory growth in
  the soft-signal store, hook stdin handling

FUNCTIONAL VERIFICATION (FIVE scenarios; document each in loom memory):

1. FORMATTER DEFAULT HUMAN. Build a recorded transcript fixture; run
   `./target/debug/loom container logs <stage_id>` (positional). Confirm
   assistant text rendered as prose, tool-call summaries one-liners,
   tool-result lines deduped (no double rendering), thinking blocks absent.

2. FORMATTER --format json. Same input. Output is byte-identical to fixture.

3. FORMATTER --show-thinking. Thinking blocks appear with [thinking]
   prefix.

4. HOOK WARN. Run:
     printf '%s\n' '{"tool_name":"Bash","tool_input":{"command":"grep -r foo ."}}' \
       | bash hooks/prefer-modern-tools.sh
   Exit code is 0. Stdout is valid JSON. jq filter
     .hookSpecificOutput.additionalContext | contains("LOOM_HOOK_WARN")
   returns true.

5. STUCK FLAG. Manually append 6 rows of
     {"is_error":true,"output_bytes":0,"tool":"Bash","session_id":"S",
      "stage_id":"X","ts":"<recent ISO>"}
   to .work/tool-events.jsonl for a synthetic running session.
   Run `loom status`. Confirm `stuck?` annotation appears for that
   session. Confirm `.work/monitor/soft-signals.jsonl` exists and
   contains a PossiblyStuck row for session S.

Fix any issue found rather than report it. Record discoveries to loom
memory so knowledge-distill can curate them.

#### Files Changed

- loom/src/models/stage/types.rs - is_possibly_stuck now #[serde(skip)] (was #[serde(default)] which still serialized the false/true value to every saved stage file). Field is purely derived at read time.
- loom/src/daemon/server/status.rs - daemon's collect_status now derives is_possibly_stuck via has_active_stuck_signal(work_dir, session_id) reading soft-signals.jsonl. Fixes the live/TUI status path which previously always reported false because the orchestrator's PossiblyStuck event handler never wrote the field.
- hooks/post-tool-use.sh - replaced 'echo $OUTPUT_TEXT | wc -c' with 'printf %s'. echo appends a newline, so empty tool output was recorded as output_bytes=1 instead of 0, breaking the failure-shape heuristic in tool_analysis.rs:101 for the real-data path.
- loom/src/commands/container/log_format.rs - added sanitize_preview() helper, applied to '[malformed line: ...]' and '[unknown event: ...]' previews. Prevents an adversarial container log producer from injecting ANSI/control sequences into the operator's terminal via 'loom container logs'.
- loom/src/orchestrator/monitor/tests.rs + daemon/server/status.rs + commands/container/log_format.rs - added tests: test_possibly_stuck_emits_once_and_persists_signal (closes detection-dedup test gap flagged by reviewer); test_possibly_stuck_not_emitted_for_only_old_events; test_is_possibly_stuck_derived_from_soft_signals; test_is_possibly_stuck_never_persisted_to_stage_yaml; malformed_preview_strips_control_chars; missing_type_preview_strips_control_chars. Also new hooks/tests/post-tool-use-empty-output.sh regression test.

#### Key Decisions

- Validate git_user_name/git_user_email at init AND read boundaries, drop invalid values to None *(Security review flagged that operator-supplied or hand-edited config.toml could contain newlines/control chars that podman accepts in -e values but produce malformed git commit objects. Added validate_git_identity() in plan/schema/execution.rs (rejects empty, >256 bytes, or any char.is_control()), wired into commands/init/execute.rs::sanitize_git_identity (prints yellow \! warning) and fs/work_dir.rs::read_project_execution (silent scrub since init already warned). Fail-closed to None matches existing 'partial identity = inject nothing' semantics.)*

#### Notes

- found: loom stage complete's run_aggregated_wiring_reverification (loom/src/commands/stage/complete.rs:663) resolved every prior stage's wiring patterns against the integration-verify stage's verification_dir, ignoring each originating stage's working_dir. Caused 10 false 'Wiring source file missing' errors on this stage because prior stages used working_dir=loom while integration-verify uses working_dir=. Fixed by joining verification_dir + stage_def.working_dir.
- found: scenario-5 cannot be exercised end-to-end without the daemon running. The detection→soft-signal→status pipeline is verifiable in three pieces: tool_analysis::analyze_session (returns is_possibly_stuck on the 6-failing-events fixture), soft_signals::append (writes the row), and either commands::status::data::collector OR daemon::server::status (renders [stuck?] when the signal is active). New test_possibly_stuck_emits_once_and_persists_signal closes the full-loop test gap.
- found: scenarios 1-3 (formatter human / json / show-thinking) are best verified via the existing tests/log_format_test.rs fixtures rather than end-to-end CLI invocation, since 'loom container logs' requires a live container runtime and the fixture-driven integration tests already exercise format_stream end-to-end with the exact options the CLI plumbs through.
- bug: Stage::is_possibly_stuck in models/stage/types.rs uses only #[serde(default)] but doc-comment claims 'not persisted to stage files'. serde_yaml::to_string in verify/transitions/serialization.rs:33 emits the field on every save. Either add #[serde(skip)] OR remove field entirely (architecture review recommends remove + route via StageInfo computed in daemon/server/status.rs).
- bug: hooks/post-tool-use.sh:81 uses 'echo "$OUTPUT_TEXT" | wc -c' which appends a trailing newline. Empty tool output is recorded as output_bytes=1, never 0. This silently breaks the output_bytes==0 failure heuristic in tool_analysis.rs:101 for the real-data path (synthetic fixture tests force the value to 0). Fix: use printf '%s' instead of echo.
- found: scenario-5 verified end-to-end. Static loom status renders [stuck?] when soft-signals.jsonl has an active PossiblyStuck row for a running session+stage. Pipeline: tool-events.jsonl → analyze_session → soft_signals::append → read_active_for_session in build_stage_summary. Live/TUI path is BROKEN: orchestrator's PossiblyStuck event handler does not update Stage.is_possibly_stuck (event_handler.rs:159 only eprintln) so daemon/server/status.rs forwards a value that's always false.
- found: podman rm -f loom-nonexistent-name exits 0 on podman 4.9.3 (Linux) — verified live. This is the core assumption underpinning preemptive_remove_existing's best-effort design. Same assumption now codified as smoke_rm_f_missing_container_exits_zero in tests/container_smoke.rs.
- gap: testing review flagged that the spawn-failure rollback chain in orchestrator/core/stage_executor.rs (remove_container_on_failure + git::remove_worktree + git::delete_branch + try_mark_blocked) has zero direct test coverage. The headline retry-after-failure scenario is unverified end-to-end at the integration level. Mitigations in place: (a) preemptive_remove_existing_is_infallible unit test covers the rm -f preamble; (b) the new smoke_rm_f_missing_container_exits_zero validates podman's exit-0 contract on missing containers; (c) the wiring check confirms remove_worktree|delete_branch patterns exist in stage_executor.rs. Future work: add a unit-test seam that injects a failing TerminalBackend::spawn_session and asserts each rollback helper was called.
- found: loom/tests/container_smoke.rs adds 3 podman-gated integration tests (rm -f exit code, GIT_* env propagation, in-container commit-filter.sh blocks Co-Authored-By trailer). Pattern: each test calls podman_available() / pick_smoke_image() / installed_hooks_dir() and prints a 'skipping — ...' eprintln + early return rather than asserting. This keeps cargo test green on macOS/CI without podman while still running automatically on Linux dev machines. Verified pass against podman 4.9.3 with localhost/loom/base:python-5ed48c54 image.
- gotcha: reviewer flagged a 'HIGH native regression' in hooks/generator.rs claiming the backend match introduced double-firing of global hooks on native worktrees. False alarm — git show 9454a4b~1 proves configure_loom_hooks(obj) was already being called unconditionally on the native worktree before the branch was added. The branch commit only added the container arm; native semantics unchanged. Detection rule: when a reviewer asserts behavior change, verify against the actual diff (git show <commit>~1 vs <commit>), not against the stage description's framing.

### Knowledge Distillation (knowledge-distill)

**Status:** executing  
**Purpose:** Curate stage memories into permanent knowledge; update user-facing
documentation for the new flags and the tool-event log + soft-signal
store conventions.

Never use Claude Code auto-memory.

CONTEXT GATHERING:

- Read doc/plans/IN_PROGRESS-PLAN-loom-observability-and-hooks.md
- loom memory show --all
- Read doc/loom/knowledge/*.md

MEMORY CURATION (loom knowledge update):

- "## Container Logs / Transcript Format" in architecture.md — list
  stream-json event types, the content vs tool_use_result duplication,
  the rate_limit_event shape
- "## Tool Event Log" in entry-points.md — pointers to
  hooks/post-tool-use.sh + loom/src/hooks/events.rs ToolEvent
- "## Soft Signals" in architecture.md — the
  .work/monitor/soft-signals.jsonl store, decay window, schema
- "## Hook Output Contract" in conventions.md — exit 0 + JSON
  hookSpecificOutput for warnings; exit 2 for blocks
- "## Deferred: Context Velocity" in concerns.md — rationale
  (context_percent: null today; needs stream-json usage extraction)

DOCUMENTATION UPDATE:

- README.md: short "Container Logs" usage section with --format,
  --show-thinking, --verbose
- Only update relevant sections — do NOT rewrite entire files

#### Files Changed

No changes recorded.

#### Key Decisions

No decisions recorded.

#### Notes

- found: heartbeat context_percent is always null today. post-tool-use.sh writes 'context_percent: null' to heartbeat JSON because context % extraction from stream-json requires parsing the runtime logs JSONL, which the hook doesn't do. This is the Context Velocity deferred concern - needs stream-json usage extraction to populate.
- found: soft-signals decay window is 120 seconds (DECAY_WINDOW_SECS=120 in soft_signals.rs). Stuck detection requires: recent_failure_count >= 5 (STUCK_MIN_EVENTS) AND failure_ratio >= 0.80 (STUCK_FAILURE_RATIO), computed over a 60-second rolling window (STUCK_WINDOW_SECS=60) from tool-events.jsonl. PossiblyStuck is the only SoftSignal variant currently.
- found: loom container logs flags are --format (human|json), --show-thinking, --verbose. --format=human (default) parses stream-json JSONL from runtime logs and renders human-readable. --format=json passes raw bytes through. --show-thinking shows [thinking] prefix lines. --verbose appends suppressed-event footer. LogFormat enum in commands/container/log_format.rs.
- found: tool-events.jsonl lives at .work/tool-events.jsonl (top-level, not .work/hooks/). hooks/events.jsonl (.work/hooks/events.jsonl) is the hook lifecycle log (session-start, pre-compact etc.). Two distinct files. ToolEvent struct in hooks/events.rs is written by post-tool-use.sh; HookEventLog also in hooks/events.rs is for session lifecycle hooks.

## Open Questions

No open questions.
