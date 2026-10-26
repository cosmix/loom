# Plan: Session bridge

Implements: section_07

## Overview

The `loom plan` server owns interactive Claude Code sessions: it starts each one in its own tmux
server under plan 6's sandbox settings, reads its transcript into chat messages, learns the states
the transcript does not carry from a hook event spool, answers permission requests through answer
files the session cannot write, delivers the user's messages into the pane only while the session
is idle, interrupts, ends sessions with a final write, and recovers sessions a dead server left
running. Browsers receive one chat model, whole on connect and as deltas after, and only the
authoring holder's connection may command it.

Product spec: `doc/product/section_07.md` (whole), with section 10's attachment block ("What the
session receives"), section 12's delivery and self-approval rules, and the model fields section 08
renders. Metaplan entry 7 of `doc/plans/metaplan-planner-ui.md`. The design and every public
type are in `doc/plans/briefs/feat-planner-07-session-bridge/common.md`.

## Goals

- `Bridge::start_session` is the one way plans 8, 9, 10 and 11 start a session (authoring,
  summary, pressure); `BridgeEvent`s tell them what happened.
- State honesty: subagent events never move the session state; a main transcript record after the
  last `Stop` means working; liveness and signals go through `ProcessIdentity`, never
  `tmux has-session`.
- Input reaches the pane only while Idle, sanitised, as a bracketed paste, confirmed by
  `UserPromptSubmit` within 10 seconds; no keystroke ever answers a dialog.
- A session never waits on a dialog nobody sees: every permission request is answered (allow,
  deny, or deny with a reason) before Claude Code's one-hour hook timeout.
- The chat model is rebuilt from transcripts only; nothing is resumed across servers.
- Non-goals: guidance text, opening and closing messages, the plugin (plan 8; plan 7 ships fixed
  ones behind a replaceable provider), the chat UI and its zod schema (plan 9), summary
  scheduling (plan 10), pressure runs and teammate mode (plan 11).

## Decisions

1. **Modules.** `loom::plan_session` (created by plan 6) gains `model`, `attachments`, `input`,
   `transcript`, `spool`, `permission`, `state` (pure, stage `bridge-core`) and `bridge`, `launch`,
   `tmux_io`, `tmux_host` (stage `bridge-runtime`); the server side is `loom/src/plan_server/chat/`
   plus `plan_server/wire/chat.rs`, following plan 2's extension rule (one variant per wire enum,
   one dispatch arm, one `install_all` line).
2. **Reuse.** tmux through the lane's free functions (`spawn_in_tmux`, which already applies
   `PRESENTATION_OPTIONS`; `teardown_socket` and `tmux_control_command` widened to `pub(crate)`
   with no line change in the ledger-sensitive `tmux/mod.rs`); `list_loom_sockets` skips `plan-`
   beside `view-`. Transcripts through `tokens::tail::read_lines` (widened) and
   `provider_data::claude::parse`; `tokens::claude_line::parse_line` extracts usage only, so the
   record mapper is new code. Liveness through `process::identity`. Settings through plan 6's
   `write_session_sandbox` with the nine spool `HookSpec`s. Locked appends through
   `fs::safe_fs::safe_append`.
3. **Input delivery is new code** (`tmux_io.rs`): no loom code sends keys to a pane today.
   `load-buffer -b loom-input -` takes the text on stdin through `std::io::pipe` with a writer
   thread (100 KB exceeds a pipe buffer), then `paste-buffer -p -d`, then `send-keys Enter`; never
   `load-buffer -w`.
4. **The opening message is pasted, never an argument.** The launch line carries no positional
   prompt: `/proc/<pid>/cmdline` is readable by other local users, and an argv word would skip
   sanitising and the 100 KB check. After the first `SessionStart { startup }` the bridge delivers
   the opening message through the normal delivery path (section 07's "the first turn needs no
   input delivery" changes; the wording is filed with the lead).
5. **Claude Code behaviour nobody has observed here** (a Stop after an interrupt, the `/compact`
   event order, the double-Escape rewind picker, hook payload field names) is handled for every
   possibility: an interrupt record closes the turn itself, a later Stop is ignored, one Escape per
   turn, `/compact` is confirmed by any of its three signals. The operator run asserts what the
   installed Claude Code does and knowledge-distill records it.
6. **All host calls run on the driver thread.** Commands mutate state and return at once;
   listeners run outside the bridge lock. A wedged tmux call stalls one tick, never a WebSocket.
7. **The server answers before the hook times out.** The server denies a pending request at
   received + 3590 s, the hook gives up at 3595 s, Claude Code's registered timeout is 3600 s: a
   hook killed by its timeout is a non-blocking error after which Claude Code would show its own
   dialog in a pane nobody watches.
8. **Turn numbering is the mapper's**, opened by `UserPromptSubmit` (or adopted when the user
   record arrives first); pins are keyed `<session id>/t<n>` so plan 9's Restore finds them.
9. **Plan 7 owns the attachment block** (format, parse, server-side resolve) as one module shared
   with plan 9, by agreement with `frontend`; plan 9 owns the model's zod schema. The wire
   fixtures are JSON data under `web/src/plan/api/fixtures/`; no TypeScript or bundle changes in
   this plan, so no web gate runs: nothing under `web/` imports the fixtures until plan 9's schema
   test, and the bundle is unchanged.
10. **Guidance is replaceable.** `plan_server::chat::guidance::GuidanceSlot` holds a
   `GuidanceProvider`; plan 7's `FixedGuidance` writes a fixed brief and a minimal plugin; plan 8
   swaps its provider in. The closing message is read from the slot at delivery time.
11. **The operator end-to-end run is a person's step.** It needs a logged-in Claude Code and
   network, which no stage sandbox has. Integration-verify runs everything else, then blocks with
   the exact command for the operator, and records the result in the plan's overview through a
   memory the distill stage curates.
12. **No knowledge-bootstrap.** The tmux, hook, transcript and sandbox knowledge this plan uses
    exists (`mistakes/tmux-backend.md`, `mistakes/spurious-waiting-for-input.md`,
    `architecture/execution-containment.md`); seams are cited by symbol and line in the briefs.
13. **Gate per stage** as in plan 6: module filters, contract targets, maintainability, clippy and
    fmt per standard stage; the metaplan's full pre-push gate in integration-verify.

## Execution Diagram

```mermaid
graph LR
    bridge-core --> bridge-runtime
    bridge-runtime --> integration-verify
    integration-verify --> knowledge-distill
```

## Stages

### 1. `bridge-core`: model, attachments, transcript mapper, spool, permission hook, state machine

Pure modules and the hook commands, no tmux and no server. Lanes are `codex` first: twelve
codex `gpt-6.1-terra` units at effort high write the model and attachment types, the wire enums,
the four wire fixtures (formatted with oxfmt by the orchestrator and checked by a criterion) and
their parity tests, the spool, and the tool-summary and usage helpers; Claude keeps the honesty
rules, the delivery boundary and the parse contract: W2 (opus) the state machine, the blocking
permission hook, the hook entry and the input sanitising and send checks, W3 (opus) the transcript mapper core and
its recorded-transcript fixtures, W1 (sonnet) the attachment block, whose round trips converge on
fixtures. Four waves.
Necessity: Q3, the mapper and the state machine are the honesty rules every later piece trusts;
their contracts freeze and pass before a driver is built on them, so a runtime bug cannot mask a
mapper bug; Q4, the two stages together (six workers, two review rounds) would pass 500,000
tokens.

Risk checklist and contracts: untrusted input (`attachments-round-trip`,
`attachments-cut-on-char-boundary`, `sanitize-strips-terminal-escapes`,
`send-refuses-slash-except-compact`, `write-answer-refuses-path-ids`), external data correctness
(the captured record shapes: `mapper-one-message-per-id`, `mapper-skips-noise-and-duplicates`,
`mapper-interrupt-ends-turn`, `mapper-edit-result-counts-patch`), lifecycle
(`permission-denies-when-session-goes`, `permission-summary-denied-at-once`), the honesty rules
(`state-record-after-stop-is-working`, `state-subagents-never-move-state`,
`spool-ignores-mismatched-event`). Reachability is the next stage's.

### 2. `bridge-runtime`: launch, tmux input, the bridge, server wiring, recovery

Lanes are `claude` first: W4 (opus, xhigh) writes the `Bridge` against a fake host; then W5
(opus) the launch and tmux input with their real-tmux tests, and W6 (opus) the server wiring, the
chat commands and the end-to-end tests. Seven codex units write the authoring leases, the launch
script text, the tmux host adapter, the planning-socket skip, the fixed guidance, the turn pins
and the dispatch tests.
Necessity: Q1 on `bridge-core` (its frozen types must be merged first).

Risk checklist and contracts: lifecycle and concurrency (`bridge-queues-while-working`,
`bridge-interrupt-returns-queue`, `bridge-final-write-waits-for-idle`, `bridge-final-write-bounded`,
`bridge-pressure-ends-on-marker`, `bridge-death-is-failed`), destructive paths
(`recovery-leaves-unverifiable`, `teardown-failure-keeps-record`), untrusted input in the launch
line (`launch-script-escapes-and-drops-stage-env`), configuration of shared tooling
(`socket-list-skips-planning`); reachability through the wiring checks on `install_all` and the
dispatch arm, and the recovery test that drives real tmux.

### Integration Verification

The metaplan's full gate, a security review against section 12 (pane input,
answer files, self-approval channels, launch environment), an architecture review against the
ledger, the recovery test on real tmux where the host allows it, then an operator block for the
Claude Code end-to-end run.

### Knowledge Distillation

Records the bridge as a tier-2 topic, the operator run's result, and any hook payload or
transcript shape the stages found to differ from what the briefs captured.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  ratchet_files:
    - loom/maintainability-baseline.txt
    - doc/loom/knowledge/check-baseline.txt
  provision:
    - working_dir: "web"
      command: "test ! -e .npmrc && test ! -L .npmrc && bun install --frozen-lockfile --ignore-scripts --backend=copyfile --config=/dev/null"
  sandbox:
    enabled: true
    auto_allow: true
    filesystem:
      deny_read: ["~/.ssh/**", "~/.aws/**", "~/.config/gcloud/**", "~/.gnupg/**"]
    network:
      allowed_domains: ["crates.io", "index.crates.io", "static.crates.io", "registry.npmjs.org"]
      allow_local_binding: false
      allow_unix_sockets: []
  stages:
    - id: bridge-core
      name: "Bridge core: chat model, attachments, transcript mapper, spool, permission hook, state machine"
      summary: "Adds the pure halves of the session bridge: the chat model sent to browsers with its wire fixtures, the attachment block, a mapper from Claude Code transcripts to chat items, the hook event spool and blocking permission hook behind loom plan hook, and the session state machine with its honesty rules."
      stage_type: standard
      skills: ["loom-rust", "loom-security-audit"]
      implementers: ["codex", "claude"]
      subagent_timeout_secs: 1200
      working_dir: "loom"
      dependencies: []
      before_stage:
        - command: "rg -qF 'pub fn write_session_sandbox' src/plan_session/sandbox/mod.rs"
          description: "plan 6's settings builder is merged"
        - command: "rg -qF 'pub enum PlanMessage' src/plan_server/wire/message.rs"
          description: "plan 2's planning wire model is merged"
      after_stage:
        - command: "rg -qF 'pub mod transcript' src/plan_session/mod.rs"
          description: "the transcript mapper is declared"
      description: |
        Build stage 1 of doc/plans/PLAN-feat-planner-07-session-bridge.md (doc/plans/IN_PROGRESS-PLAN-feat-planner-07-session-bridge.md once loom run starts).
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is loom/ (contracts run as cargo test <name> -- --exact from it). YAML paths and the worker table are package-relative; brief paths are repo-relative.
        Every worker reads doc/plans/briefs/feat-planner-07-session-bridge/common.md first (the design, every public type, the captured transcript record shapes, the size and safety rules), then its own brief.
        PRECONDITION: plans 1 to 6 are merged. Confirm with loom map --find-all for write_session_sandbox, GuardPolicy, LockedSet, PlanMessage, ClientMessage, Project before spawning; a missing one is loom stage block naming it.
        Territories are DISJOINT. Workers NEVER spawn subagents. Spawn by agent type with the fixed prompt plus "Your brief: <path>. Read it in full before anything else." Wait with one background loom subagents watch.
        SEED (the orchestrator, before wave 1; mechanical, purpose lines and visibility only): in src/plan_session/mod.rs (plan 6's) add `pub mod model; pub mod attachments; pub mod input; pub mod transcript; pub mod spool; pub mod permission; pub mod state;` and create each module root (src/plan_session/model/mod.rs, attachments/mod.rs, input.rs, transcript/mod.rs, spool.rs, permission.rs, state.rs) holding only its //! purpose line; in transcript/mod.rs also declare `mod records; mod tools; mod usage; #[cfg(test)] mod tests;` with purpose-line files; in attachments/mod.rs declare `mod format; mod parse; mod resolve; #[cfg(test)] mod tests;` and in model/mod.rs `#[cfg(test)] mod tests;`, each with a purpose-line file (format.rs, parse.rs, resolve.rs, attachments/tests.rs and model/tests.rs; the seed, not C1 or C2, creates them so no unit writes a file another row owns); in src/plan_server/wire/mod.rs add `pub mod chat;` and `#[cfg(test)] mod tests_chat;` with purpose-line files; in src/tokens/tail.rs and src/tokens/mod.rs widen `read_lines`, `stat`, `Line`, `FileStat`, `MAX_LINE_BYTES` and `mod tail` from pub(super)/private to pub(crate) (no line-count change). Each seeded file is then owned by exactly one worker or unit.
        LANES (the user's decision; do not ask): codex units C1-C3 and C5-C13 go through loom-codex-forwarder subagents spawned in the FOREGROUND, at most 6 at once over disjoint files, each with --model gpt-6.1-terra --effort high and an explicit Bash timeout of 600000 ms; the forwarded text is the unit's block in doc/plans/briefs/feat-planner-07-session-bridge/bridge-core/codex-units.md, verbatim. Never use codex:codex-rescue. Codex units never run git and run compile checks only: after EACH unit run its proof command and git status --short yourself, and route a failure to a fresh unit (re-split if it timed out) or, after one failed retry, to a sonnet loom-software-engineer. The 6-forward cap counts codex only; Claude workers of the same wave run beside them.
        Claude keeps the state machine, the permission hook, hook_cli and the input sanitising and send checks at the pane-delivery boundary (W2, opus), the transcript mapper core (W3, opus) and the attachment block (W1, sonnet: its round trips converge on fixtures).
        WAVE 1: C1. WAVE 2: C2, C3, C5 (codex). WAVE 3 in ONE message: W1, W2, W3 (Claude) and C6, C7, C8, C9, C10, C11 (codex); after C7-C10 run `cd ../web && node_modules/.bin/oxfmt src/plan/api/fixtures/chat-snapshot.json src/plan/api/fixtures/chat-deltas.json src/plan/api/fixtures/chat-commands.json src/plan/api/fixtures/chat-frames.json` yourself (codex does not format web files). WAVE 4: C12, C13 (codex); after W1 returns add the three `pub use` re-exports to attachments/mod.rs (a small change).
        AFTER THE WAVES: rustfmt --check --edition 2021 tests/plan_session_bridge_core_contracts.rs (a frozen file the formatter would rewrite fails completion; a failure is a dispute-contract, never an edit); then cargo fmt --all; then cargo test --test maintainability; route any failure to a fresh worker of the owning territory; then one loom check.
        LEDGER: no row of maintainability-baseline.txt changes; tokens/tail.rs and tokens/mod.rs get visibility changes only.
        MUTATION: before the final review round, for each contract apply the wrong implementation its rejects: text names, confirm the contract fails, restore from a saved copy (never git checkout), and record loom memory note "mutation: <contract id> red under <mutation>".

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | W1 | Attachment block | sonnet | src/plan_session/attachments/format.rs; src/plan_session/attachments/parse.rs; src/plan_session/attachments/resolve.rs; src/plan_session/attachments/tests.rs; tests/fixtures/plan_session/attachments/** | common.md | doc/plans/briefs/feat-planner-07-session-bridge/bridge-core/w1-model-attachments.md |
        | W2 | Permission hook, state machine, hook entry, input checks | opus | src/plan_session/permission.rs; src/plan_session/permission_tests.rs; src/plan_session/state.rs; src/plan_session/state_tests.rs; src/plan_session/hook_cli.rs; src/plan_session/input.rs | common.md | doc/plans/briefs/feat-planner-07-session-bridge/bridge-core/w2-spool-permission-state.md |
        | W3 | Transcript mapper core | opus | src/plan_session/transcript/mod.rs; src/plan_session/transcript/records.rs; src/plan_session/transcript/tests.rs; tests/fixtures/plan_session/transcripts/** | common.md | doc/plans/briefs/feat-planner-07-session-bridge/bridge-core/w3-transcript.md |
        | C1-C2 | Codex units C1, C2 (one forward per file): attachment and model types | codex terra | src/plan_session/attachments/mod.rs; src/plan_session/model/mod.rs | common.md | doc/plans/briefs/feat-planner-07-session-bridge/bridge-core/codex-units.md |
        | C3-C5 | Codex units C3, C5 (one forward per file and its test): spool, tool summaries | codex terra | src/plan_session/spool.rs; src/plan_session/spool_tests.rs; src/plan_session/transcript/tools.rs; src/plan_session/transcript/tests_tools.rs | common.md | doc/plans/briefs/feat-planner-07-session-bridge/bridge-core/codex-units.md |
        | C6-C11 | Codex units C6-C11 (one forward per file): wire enums, the four wire fixtures, usage | codex terra | src/plan_server/wire/chat.rs; ../web/src/plan/api/fixtures/chat-snapshot.json; ../web/src/plan/api/fixtures/chat-deltas.json; ../web/src/plan/api/fixtures/chat-commands.json; ../web/src/plan/api/fixtures/chat-frames.json; src/plan_session/transcript/usage.rs | common.md | doc/plans/briefs/feat-planner-07-session-bridge/bridge-core/codex-units.md |
        | C12-C13 | Codex units C12, C13 (one forward per file): wire parity and model tests | codex terra | src/plan_server/wire/tests_chat.rs; src/plan_session/model/tests.rs | common.md | doc/plans/briefs/feat-planner-07-session-bridge/bridge-core/codex-units.md |

        CONTRACT SURFACE (the contract session writes tests/plan_session_bridge_core_contracts.rs and tests/plan_session_bridge_core_contracts/support.rs from this text before any code; every contract is a top-level #[test] fn of at most 50 lines, each file at most 400 lines, rustfmt-clean; transcript lines are written inline from the record shapes quoted in doc/plans/briefs/feat-planner-07-session-bridge/common.md "Transcript mapping"; temp dirs from tempfile::Builder::new().prefix("loom-plan-bridge-").tempdir_in(std::env::temp_dir())):
        - loom::plan_session::attachments::{format_message(text: &str, attachments: &[Attachment]) -> String, parse_message(delivered: &str) -> (String, Vec<Attachment>), Attachment { kind, path, anchor, lines: Option<(u32, u32)>, stage, summary_sentence, rests_on: Vec<String>, quote, changed_since_selected: bool, locked: Option<LockedTag> }, AttachmentKind::{Source, Summary}, LockedTag::{Running, Done, Retired, Knowledge, Brief}}
        - loom::plan_session::input::{sanitize(&str) -> String, check_send(&str) -> Result<(), InputRefusal>, check_delivered(&str) -> Result<(), InputRefusal>}
        - loom::plan_session::transcript::Mapper::{new(project_root: PathBuf), begin_turn(&mut self, at: DateTime<Utc>) -> (String, Vec<ChatDelta>), end_turn(&mut self) -> Vec<ChatDelta>, ingest_main(&mut self, line: &str) -> Ingest { deltas: Vec<ChatDelta>, marks: Vec<RecordMark> }, turns(&self) -> &[Turn], changed_by_tool(&self, turn_id: &str) -> Vec<String>}
        - loom::plan_session::model::{Turn { id, tokens: TokenCounts { input, cache_write, cache_read, output }, items: Vec<Item>, .. }, Item::{User, Thinking, Text, Tool { status, added, removed, .. }, Permission, Notice}, ToolStatus::{Pending, Done, Error, Denied, NotFinished}, SessionState::{Starting, Idle, Working, Permission, Ending, Ended, Failed}, ChatDelta}
        - loom::plan_session::state::{StateMachine::new() -> StateMachine, StateMachine::apply(&mut self, Input<'_>) -> Option<SessionState>, StateMachine::state(&self) -> SessionState, StateMachine::subagents_running(&self) -> u32, StateMachine::pending_requests(&self) -> &BTreeSet<String>, StateMachine::activity(&self) -> Activity::{Idle, Working, Permission}, Input::{Spool(&SpoolEvent), Record { at, main }, RequestResolved { request_id }, Interrupted { at }, AgentsKilled { at }, SelfStartedTurn { at }, ProcessGone, SpawnFailed, EndRequested, PermissionAnswered { request_id }, PermissionExpired { request_id }, CompactDelivered}}
        - loom::plan_session::spool::{SpoolEvent { received_at, event, session_id, transcript_path, request_id, payload }, SpoolReader::new(path: PathBuf), SpoolReader::read_new(&mut self) -> Vec<SpoolEvent>}
        - loom::plan_session::hook_cli::run(event: &str, session_dir: &Path, stdin: impl Read, stdout: impl Write) -> i32
        - loom::plan_session::permission::{write_answer(session_dir: &Path, request_id: &str, answer: &Answer) -> anyhow::Result<()>, Answer { behavior: Behavior::{Allow, Deny}, message, reason: Option<DenyReason> }}
        - loom::plan_session::guard::GuardPolicy { schema: 1, kind: SessionKind, project_root, runtime_dir, session_dir, home } serialised with serde_json into <session dir>/guard.json (plan 6, merged)
        The contracts are red at freeze because these modules hold only their purpose lines.

        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); NEVER loom knowledge (implementation stage); NEVER Claude Code auto-memory.
      files:
        - "src/plan_session/mod.rs"
        - "src/plan_session/model/**"
        - "src/plan_session/attachments/**"
        - "src/plan_session/input.rs"
        - "src/plan_session/transcript/**"
        - "src/plan_session/spool.rs"
        - "src/plan_session/spool_tests.rs"
        - "src/plan_session/permission.rs"
        - "src/plan_session/permission_tests.rs"
        - "src/plan_session/state.rs"
        - "src/plan_session/state_tests.rs"
        - "src/plan_session/hook_cli.rs"
        - "src/plan_server/wire/chat.rs"
        - "src/plan_server/wire/tests_chat.rs"
        - "src/plan_server/wire/mod.rs"
        - "src/tokens/tail.rs"
        - "src/tokens/mod.rs"
        - "tests/fixtures/plan_session/**"
        - "tests/plan_session_bridge_core_contracts.rs"
        - "tests/plan_session_bridge_core_contracts/**"
        - "../web/src/plan/api/fixtures/chat-snapshot.json"
        - "../web/src/plan/api/fixtures/chat-deltas.json"
        - "../web/src/plan/api/fixtures/chat-commands.json"
        - "../web/src/plan/api/fixtures/chat-frames.json"
      harness: ["tests/plan_session_bridge_core_contracts/**"]
      acceptance:
        - "cargo build"
        - "cargo test --lib plan_session::"
        - "cargo test --lib plan_server::wire::"
        - "cargo test --lib tokens::"
        - "cargo test --test plan_session_bridge_core_contracts"
        - "cd ../web && node_modules/.bin/oxfmt --check src/plan/api/fixtures/chat-snapshot.json src/plan/api/fixtures/chat-deltas.json src/plan/api/fixtures/chat-commands.json src/plan/api/fixtures/chat-frames.json"
        - "cargo test --test maintainability"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
      artifacts:
        - "src/plan_session/model/mod.rs"
        - "src/plan_session/attachments/format.rs"
        - "src/plan_session/attachments/parse.rs"
        - "src/plan_session/transcript/mod.rs"
        - "src/plan_session/spool.rs"
        - "src/plan_session/permission.rs"
        - "src/plan_session/state.rs"
      wiring:
        - source: "src/plan_session/hook_cli.rs"
          pattern: "\"PermissionRequest\" =>"
          literal: true
          description: "loom plan hook routes PermissionRequest to the blocking permission hook"
        - source: "src/plan_session/hook_cli.rs"
          pattern: "append_event("
          literal: true
          description: "loom plan hook appends spool events"
        - source: "src/plan_session/transcript/mod.rs"
          pattern: "parse_message("
          literal: true
          description: "the mapper rebuilds attachment chips from user records"
      contracts:
        - id: attachments-round-trip
          file: tests/plan_session_bridge_core_contracts.rs
          test: attachments_block_round_trips_with_an_escaped_literal_tag
          scenario: "user text with a line that is exactly <loom-attachments> and a CRLF, plus three attachments (source with lines 40-44, a stage address stage:integration-verify with locked Done and changed_since_selected, a summary sentence with two rests-on addresses); format_message then parse_message"
          rejects: "a parser that drops the escaped line, keeps the backslash, loses lines, rests-on, locked or changed-since-selected, or mistakes the user's literal tag line for the block"
        - id: attachments-cut-on-char-boundary
          file: tests/plan_session_bridge_core_contracts.rs
          test: quote_over_2000_chars_is_cut_at_a_line_boundary_without_splitting_a_char
          scenario: "a quote of 60 lines of 45 characters each where every line ends in a multi-byte character (é, 中, an emoji); format_message with one source attachment"
          rejects: "a byte-index cut that panics or splits a character, or a cut mid-line: the quoted lines must total at most 2,000 characters, end at a line boundary, and be followed by the line > […N more characters; read the source] with the exact remaining count"
        - id: sanitize-strips-terminal-escapes
          file: tests/plan_session_bridge_core_contracts.rs
          test: sanitize_removes_c0_c1_and_del_but_keeps_newline_and_tab
          scenario: "sanitize on a text holding ESC [ 2 0 1 ~ (the bracketed-paste end), U+009B, DEL, NUL, BEL, a CRLF pair, a lone CR, a tab, a newline and non-ASCII letters"
          rejects: "a sanitizer that strips only ESC or only ASCII controls: the C1 character, DEL, NUL and BEL must be gone, CRLF must become LF, the lone CR removed, tab, newline and letters kept"
        - id: send-refuses-slash-except-compact
          file: tests/plan_session_bridge_core_contracts.rs
          test: check_send_refuses_slash_commands_except_compact
          scenario: "check_send on '/clear', '  /exit', '/compact', '/compact keep the plan', '/compactx', 'path /tmp is fine' and check_delivered on a 100,001-byte text"
          rejects: "a check that refuses every leading slash, accepts /compactx, ignores leading whitespace, or measures characters instead of bytes"
        - id: mapper-one-message-per-id
          file: tests/plan_session_bridge_core_contracts.rs
          test: assistant_records_sharing_a_message_id_count_usage_once
          scenario: "after begin_turn, three assistant records with one message.id (a thinking block, a text block, a tool_use block), each carrying usage, the last with output_tokens 920; ingest_main each"
          rejects: "a mapper that sums usage per record: the turn's tokens equal the last record's usage alone, and the turn holds exactly one thinking, one text and one pending tool item"
        - id: mapper-skips-noise-and-duplicates
          file: tests/plan_session_bridge_core_contracts.rs
          test: noise_records_and_repeated_uuids_produce_no_items
          scenario: "records of types attachment, file-history-snapshot, mode, ai-title, queue-operation, a user isMeta record, a <local-command-caveat> user record, a user record with origin.kind task-notification, a system stop_hook_summary, then one human user record ingested twice"
          rejects: "a mapper that errors on unknown types, turns meta or notification records into user messages, or duplicates an item on a repeated uuid: exactly one user item results"
        - id: mapper-interrupt-ends-turn
          file: tests/plan_session_bridge_core_contracts.rs
          test: interrupt_record_marks_pending_tools_not_finished
          scenario: "a tool_use record with no result, then a user record whose content is the text block [Request interrupted by user]"
          rejects: "a mapper that leaves the tool pending or renders the interrupt as a user message: the tool is not_finished, a notice item is added, and the turn is closed"
        - id: mapper-edit-result-counts-patch
          file: tests/plan_session_bridge_core_contracts.rs
          test: edit_result_completes_the_tool_and_records_the_changed_path
          scenario: "an Edit tool_use for <root>/doc/product/x.md then its tool_result user record with toolUseResult.filePath and a structuredPatch of lines [' a', '-b', '+c', '+d']"
          rejects: "a mapper that ignores toolUseResult: the tool item is done with added 2 and removed 1, and changed_by_tool lists doc/product/x.md relative to the root"
        - id: state-record-after-stop-is-working
          file: tests/plan_session_bridge_core_contracts.rs
          test: a_main_record_after_stop_means_working
          scenario: "SessionStart startup, UserPromptSubmit, Stop received at t1, then Record { at: t1 + 1 s, main: true }, then a Notification idle_prompt received at t1 + 0.5 s"
          rejects: "a machine that trusts the spool alone and reports Idle: the state is Working after the record and stays Working after the earlier-stamped idle_prompt"
        - id: state-subagents-never-move-state
          file: tests/plan_session_bridge_core_contracts.rs
          test: subagent_events_and_records_never_move_the_state
          scenario: "an Idle machine receives SubagentStart, Record { main: false } after the last Stop, SubagentStop, and a second SubagentStop"
          rejects: "a machine that moves to Working on subagent activity or lets the running count go negative: the state stays Idle and subagents_running ends at 0"
        - id: state-permission-set
          file: tests/plan_session_bridge_core_contracts.rs
          test: two_pending_requests_keep_permission_until_both_are_answered
          scenario: "a Working machine receives PermissionRequest r1 and r2; PermissionAnswered r1; PermissionAnswered r2; then on a fresh Idle machine PermissionAnswered r9"
          rejects: "a single-slot Permission state: after r1 the state is still Permission, after r2 Working; the answer for the unknown r9 leaves the Idle machine Idle"
        - id: state-subagent-count-resets
          file: tests/plan_session_bridge_core_contracts.rs
          test: an_interrupt_or_agents_killed_resets_the_running_subagent_count
          scenario: "two SubagentStart events with no SubagentStop, then Interrupted; on a second machine two SubagentStart events then AgentsKilled"
          rejects: "a count that only SubagentStop decrements, which holds every final write to its bound: subagents_running is 0 after the interrupt and after AgentsKilled"
        - id: spool-ignores-mismatched-event
          file: tests/plan_session_bridge_core_contracts.rs
          test: hook_with_a_mismatched_event_name_appends_nothing
          scenario: "hook_cli::run(\"Stop\", dir, payload with hook_event_name SubagentStop) and hook_cli::run(\"Stop\", dir, payload with hook_event_name Stop); SpoolReader over events.jsonl"
          rejects: "a hook that spools whatever arrives: exactly one Stop event is read back, and both runs return 0 with empty stdout"
        - id: permission-denies-when-session-goes
          file: tests/plan_session_bridge_core_contracts.rs
          test: permission_hook_denies_when_the_session_directory_goes_away
          scenario: "an authoring guard.json and answers/ dir; hook_cli::run(\"PermissionRequest\", ..) on a thread; once the spool line appears, remove answers/"
          rejects: "a hook that keeps waiting (up to an hour) or exits silently: within 2 s it prints a PermissionRequest deny decision and returns 0"
        - id: permission-summary-denied-at-once
          file: tests/plan_session_bridge_core_contracts.rs
          test: summary_session_permission_requests_are_denied_at_once_and_logged
          scenario: "a guard.json whose kind is Summary; hook_cli::run(\"PermissionRequest\", ..) with a Bash payload"
          rejects: "a hook that waits for an answer for summary sessions or denies without logging: it returns at once with a deny decision, and the spool holds the request marked auto_denied summary_session"
        - id: write-answer-refuses-path-ids
          file: tests/plan_session_bridge_core_contracts.rs
          test: write_answer_refuses_a_request_id_that_is_not_a_uuid
          scenario: "write_answer with request ids '../escape', 'a/b', '' and a valid uuid v4"
          rejects: "a writer that joins the id into a path: the first three fail and create no file anywhere, the uuid writes answers/<id>.json with the answer's fields"

    - id: bridge-runtime
      name: "Bridge runtime: launch, tmux input, the bridge, server wiring, recovery"
      summary: "Starts planning sessions in their own tmux servers, drives them through the bridge (queue, delivery confirmation, permissions, interrupt, final write, recovery after a server crash), and wires the chat model and commands into the planning server."
      stage_type: standard
      skills: ["loom-rust", "loom-concurrency", "loom-security-audit"]
      implementers: ["claude", "codex"]
      sandbox:
        network:
          allowed_domains: ["crates.io", "index.crates.io", "static.crates.io", "registry.npmjs.org"]
          allow_local_binding: true
          allow_unix_sockets: []
      subagent_timeout_secs: 1200
      working_dir: "loom"
      dependencies: ["bridge-core"]
      before_stage:
        - command: "rg -qF 'pub struct Mapper' src/plan_session/transcript/mod.rs"
          description: "bridge-core is merged"
      after_stage:
        - command: "rg -qF 'chat::install' src/plan_server/routes.rs"
          description: "the chat module is installed by the planning server"
      description: |
        Build stage 2 of doc/plans/PLAN-feat-planner-07-session-bridge.md (doc/plans/IN_PROGRESS-PLAN-feat-planner-07-session-bridge.md once loom run starts).
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is loom/ (contracts run as cargo test <name> -- --exact from it). YAML paths and the worker table are package-relative; brief paths are repo-relative.
        Every worker reads doc/plans/briefs/feat-planner-07-session-bridge/common.md first (the Bridge API, LaunchSpec, the guidance seam, the wire enums, the rules), then its own brief.
        Territories are DISJOINT. Workers NEVER spawn subagents. Spawn by agent type with the fixed prompt plus "Your brief: <path>. Read it in full before anything else." Wait with one background loom subagents watch.
        SEED (the orchestrator, before wave 1; mechanical, purpose lines only): in src/plan_session/mod.rs add `pub mod bridge; pub mod launch; pub mod tmux_io; pub mod tmux_host;` and create src/plan_session/bridge/mod.rs, launch.rs (with `pub mod script;`), launch/script.rs, tmux_io.rs, tmux_host.rs holding only their //! purpose lines; create src/plan_server/chat/mod.rs with `pub mod lease; pub mod guidance; pub mod pins; #[cfg(test)] mod tests; #[cfg(test)] mod tests_dispatch;` and purpose-line files for those five, and declare `chat` where plan 2 declares route modules (W6 owns chat/mod.rs and that line afterwards).
        LANES (the user's decision; do not ask): codex units C14-C20 go through loom-codex-forwarder subagents spawned in the FOREGROUND, at most 6 at once over disjoint files, each with --model gpt-6.1-terra --effort high and an explicit Bash timeout of 600000 ms; the forwarded text is the unit's block in doc/plans/briefs/feat-planner-07-session-bridge/bridge-runtime/codex-units.md, verbatim. Never use codex:codex-rescue. Codex units never run git and run compile checks only: after EACH unit run its proof command and git status --short yourself, and route a failure to a fresh unit (re-split if it timed out) or, after one failed retry, to a sonnet loom-software-engineer. The 6-forward cap counts codex only; Claude workers of the same wave run beside them.
        Claude keeps the Bridge (W4: state, queue, delivery decisions, recovery), the launch and tmux input with their real-tmux tests (W5; codex cannot run tmux) and the server wiring and e2e tests (W6).
        WAVE 1: W4 alone (opus, spawned with effort xhigh: concurrent state, timers, recovery) and C14 (codex). WAVE 2 after W4 returned, in ONE message: W5 and W6 (Claude; W6 waits and rebuilds while W5 or codex files do not compile yet, never edits them) and C15, C17, C18, C19 (codex). WAVE 3: C16 after W5, C20 after W6.
        AFTER THE WAVES: rustfmt --check --edition 2021 tests/plan_session_bridge_runtime_contracts.rs; then cargo fmt --all; then cargo test --test maintainability; route any failure to a fresh worker of the owning territory; then one loom check.
        LEDGER: no row of maintainability-baseline.txt changes; orchestrator/terminal/tmux/mod.rs gets visibility changes only; tmux/socket.rs stays under 400 lines.
        RECORD: whether the real-tmux tests (launch round trip, recovery) ran or skipped on the stage host: loom memory note "tmux-tests: ran|skipped (<reason>)".
        MUTATION: before the final review round, for each contract apply the wrong implementation its rejects: text names, confirm the contract fails, restore from a saved copy (never git checkout), and record loom memory note "mutation: <contract id> red under <mutation>".

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | W4 | The Bridge | opus/xhigh | src/plan_session/bridge/mod.rs; src/plan_session/bridge/host.rs; src/plan_session/bridge/session.rs; src/plan_session/bridge/driver.rs; src/plan_session/bridge/commands.rs; src/plan_session/bridge/ending.rs; src/plan_session/bridge/registry.rs; src/plan_session/bridge/fake_host.rs; src/plan_session/bridge/tests.rs; src/plan_session/bridge/tests_queue.rs; src/plan_session/bridge/tests_permission.rs; src/plan_session/bridge/tests_end.rs; src/plan_session/bridge/tests_recovery.rs | common.md | doc/plans/briefs/feat-planner-07-session-bridge/bridge-runtime/w4-bridge.md |
        | W5 | Launch and tmux input | opus | src/plan_session/launch.rs; src/plan_session/tmux_io.rs; src/plan_session/launch_tests.rs; src/plan_session/tmux_io_tests.rs; src/orchestrator/terminal/tmux/mod.rs | common.md | doc/plans/briefs/feat-planner-07-session-bridge/bridge-runtime/w5-launch-tmux.md |
        | W6 | Server wiring, chat commands, e2e tests | opus | src/plan_server/chat/mod.rs; src/plan_server/chat/dispatch.rs; src/plan_server/chat/tests.rs; src/plan_server/wire/message.rs; src/plan_server/ws/dispatch.rs; src/plan_server/routes.rs; src/plan_server/mod.rs; tests/plan_session_bridge_e2e.rs | common.md | doc/plans/briefs/feat-planner-07-session-bridge/bridge-runtime/w6-server-chat.md |
        | C14-C17 | Codex units C14, C15, C16, C17 (one forward per file): leases, launch script, tmux host, socket skip | codex terra | src/plan_server/chat/lease.rs; src/plan_session/launch/script.rs; src/plan_session/tmux_host.rs; src/orchestrator/terminal/tmux/socket.rs | common.md | doc/plans/briefs/feat-planner-07-session-bridge/bridge-runtime/codex-units.md |
        | C18-C20 | Codex units C18, C19, C20 (one forward per file): fixed guidance, turn pins, dispatch tests | codex terra | src/plan_server/chat/guidance.rs; src/plan_server/chat/pins.rs; src/plan_server/chat/tests_dispatch.rs | common.md | doc/plans/briefs/feat-planner-07-session-bridge/bridge-runtime/codex-units.md |

        CONTRACT SURFACE (the contract session writes tests/plan_session_bridge_runtime_contracts.rs and tests/plan_session_bridge_runtime_contracts/support.rs, the latter holding a ScriptedHost: a SessionHost that records every call, reports identities the test sets, reports a socket as present or absent, and can be told to fail teardown, and a RecordingPins: a TurnPins that records each begin_turn and end_turn key; every contract is a top-level #[test] fn of at most 50 lines; the test drives a session by appending spool lines (the JSON line form of common.md "Spool") to <session dir>/events.jsonl and transcript lines to the transcript path a SessionStart line names, and waits for effects with a bounded poll of at most 5 s):
        - loom::plan_session::bridge::{Bridge::new(config: BridgeConfig, host: Arc<dyn SessionHost>, pins: Option<Arc<dyn TurnPins>>) -> Arc<Bridge>, Bridge::run(self: &Arc<Self>, stop: Arc<AtomicBool>) -> JoinHandle<()>, start_session(&self, StartRequest) -> Result<String, BridgeError>, send(&self, id: &str, text: String, attachments: Vec<Attachment>) -> Result<SendOutcome, BridgeError>, interrupt(&self, id: &str) -> Result<Vec<String>, BridgeError>, end(&self, id: &str, mode: EndMode, cause: EndCause) -> Result<Vec<String>, BridgeError>, answer_permission(&self, id: &str, request_id: &str, allow: bool, note: Option<String>) -> Result<(), BridgeError>, record_child(&self, id: &str, label: &str, identity: ProcessIdentity) -> Result<(), BridgeError>, release_session(&self, id: &str) -> Result<(), BridgeError>, sessions(&self) -> Vec<(String, SessionKind, SessionState)>, subscribe(&self, Box<dyn Fn(&BridgeEvent) + Send + Sync>), chat_snapshot(&self, for_holder: bool) -> ChatSnapshot, recover(&self) -> RecoveryReport}
        - BridgeConfig { project_root, runtime: RuntimePaths, loom_bin, claude_bin, home, closing_message: Arc<dyn Fn() -> String + Send + Sync>, delivery_timeout, final_write_bound, permission_deny_after, identity_interval, tick: Duration }; StartRequest { kind: SessionKind, guidance: Box<dyn FnOnce(&SessionDir) -> anyhow::Result<LaunchGuidance> + Send>, opening_origin: Option<Origin>, offer: Option<String>, extra_env: Vec<(String, String)> }; LaunchGuidance { brief_path, plugin_dir, opening_message, model, effort, display_name, opening_on_argv: bool }; SendOutcome::{Delivering { item_id }, Queued { queue_id }}; EndMode::{FinalWrite, Teardown}; EndCause::{Holder, GraceExpired, Draining, Caller} (Holder and Caller keep permission requests answerable; GraceExpired and Draining deny them at once); BridgeEvent::{Started, State, TurnStarted, TurnEnded, SubagentStopped, Ended { id, kind, outcome }}; EndOutcome::{Closed, Marker, TornDown, Failed(String)}; RecoveryReport { closed: Vec<RecoveredSession>, torn_down: Vec<RecoveredSession>, left_alone: Vec<(String, String, String)> }; RecoveredSession { id, kind, children }
        - loom::plan_session::bridge::host::{SessionHost { spawn(&self, &LaunchSpec) -> anyhow::Result<Spawned>, deliver(&self, socket, target, text) -> anyhow::Result<()>, interrupt(&self, socket, target) -> anyhow::Result<()>, teardown(&self, socket) -> anyhow::Result<()>, identity(&self, ProcessIdentity) -> IdentityStatus }, LaunchSpec { id, kind, dir, socket, tmux_session, guidance, extra_env, opening_argv: Option<String>, project_root, runtime, loom_bin, claude_bin, home }, Spawned { identity: ProcessIdentity }}
        - loom::plan_session::bridge::registry::{SessionRecord { schema, id, kind, socket, tmux_session, pid, start_time: Option<u64>, transcript_path, started_at, children: Vec<ChildRecord> }, write_record(&SessionDir, &SessionRecord) -> anyhow::Result<()>}
        - loom::plan_session::launch::script::launch_script(spec: &LaunchSpec, settings: &Path, env: &[(OsString, OsString)]) -> String
        - loom::orchestrator::terminal::tmux::list_loom_sockets(work_dir: &Path) -> Vec<LoomSocket> (reads $TMUX_TMPDIR/tmux-<uid>/; pin TMUX_TMPDIR to a temp dir, #[serial])
        - loom::process::identity::{ProcessIdentity { pid, start_time }, IdentityStatus::{VerifiedAlive, Dead, Unverifiable}}; loom::planning::runtime::runtime_paths_under(loom_dir, root)
        The contracts are red at freeze because the bridge modules hold only their purpose lines.

        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); NEVER loom knowledge (implementation stage); NEVER Claude Code auto-memory.
      files:
        - "src/plan_session/mod.rs"
        - "src/plan_session/bridge/**"
        - "src/plan_session/launch.rs"
        - "src/plan_session/launch/**"
        - "src/plan_session/tmux_io.rs"
        - "src/plan_session/tmux_host.rs"
        - "src/plan_session/launch_tests.rs"
        - "src/plan_session/tmux_io_tests.rs"
        - "src/orchestrator/terminal/tmux/mod.rs"
        - "src/orchestrator/terminal/tmux/socket.rs"
        - "src/plan_server/chat/**"
        - "src/plan_server/wire/message.rs"
        - "src/plan_server/ws/dispatch.rs"
        - "src/plan_server/routes.rs"
        - "src/plan_server/mod.rs"
        - "tests/plan_session_bridge_e2e.rs"
        - "tests/plan_session_bridge_runtime_contracts.rs"
        - "tests/plan_session_bridge_runtime_contracts/**"
      harness: ["tests/plan_session_bridge_runtime_contracts/**"]
      acceptance:
        - "cargo build"
        - "cargo test --lib plan_session::"
        - "cargo test --lib plan_server::"
        - "cargo test --lib orchestrator::terminal::tmux::"
        - "cargo test --test plan_session_bridge_runtime_contracts"
        - "cargo test --test plan_session_bridge_e2e"
        - "cargo test --test maintainability"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
      artifacts:
        - "src/plan_session/bridge/mod.rs"
        - "src/plan_session/bridge/driver.rs"
        - "src/plan_session/bridge/ending.rs"
        - "src/plan_session/launch.rs"
        - "src/plan_session/tmux_io.rs"
        - "src/plan_server/chat/mod.rs"
        - "src/plan_server/chat/dispatch.rs"
        - "tests/plan_session_bridge_e2e.rs"
      wiring:
        - source: "src/plan_server/routes.rs"
          pattern: "chat::install("
          literal: true
          description: "the planning server installs the chat bridge at startup"
        - source: "src/plan_server/ws/dispatch.rs"
          pattern: "ClientMessage::Chat("
          literal: true
          description: "chat commands from the browser reach the bridge"
        - source: "src/plan_server/chat/mod.rs"
          pattern: "on_open("
          literal: true
          description: "every new WebSocket gets the chat snapshot"
        - source: "src/plan_server/chat/mod.rs"
          pattern: "shutdown().register("
          literal: true
          description: "Ctrl-C drains sessions with a final write"
        - source: "src/plan_session/tmux_host.rs"
          pattern: "paste_text("
          literal: true
          description: "delivery goes through the bracketed-paste path"
        - source: "src/orchestrator/terminal/tmux/socket.rs"
          pattern: "starts_with(\"plan-\")"
          literal: true
          description: "loom init and loom clean skip planning sockets"
      contracts:
        - id: bridge-queues-while-working
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: a_message_sent_while_working_is_delivered_at_the_next_idle
          scenario: "a started session made Idle by a SessionStart startup line; send A (delivered), a UserPromptSubmit line, send B while Working; then a Stop line"
          rejects: "a bridge that types into the pane while the session works: the host records exactly one deliver before the Stop line and the second deliver (B) only after it, and send B returned Queued"
        - id: bridge-interrupt-returns-queue
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: interrupt_returns_queued_texts_and_delivers_nothing_at_the_next_idle
          scenario: "a Working session (one open turn) with two queued messages and a pending PermissionRequest spool line, a recording TurnPins; interrupt; then the interrupt transcript record and NO Stop line"
          rejects: "an interrupt that delivers the queue at the Idle it causes, leaves the permission pending, or waits for a Stop that Claude Code does not send: interrupt returns both texts, an answers/<id>.json deny with reason interrupt exists, the host recorded one interrupt and no further deliver, a TurnEnded event arrives and pins.end_turn was called exactly once"
        - id: bridge-interrupt-only-while-working
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: interrupt_is_refused_while_idle_and_presses_once_per_turn
          scenario: "an Idle session with one queued message: interrupt; then UserPromptSubmit (Working) and interrupt twice in a row"
          rejects: "an interrupt that sends Escape to an idle prompt (Claude Code's rewind picker on a double Escape): the Idle interrupt returns NotRunning, makes no host call and returns no texts; the two Working interrupts record exactly one host interrupt"
        - id: bridge-ending-returns-queue
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: end_returns_the_queue_and_delivers_only_the_closing_message
          scenario: "a Working session with queued message B; end(FinalWrite, EndCause::Holder); then send C; then a Stop line"
          rejects: "an end that delivers B (or C) before or after the closing text: end returns [B], send C returns NotRunning, and after the Stop the only deliver is the closing text"
        - id: bridge-parallel-tools-keep-request
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: another_tools_result_does_not_close_a_pending_request
          scenario: "one assistant message with two tool_use records, Read A and Bash B; a PermissionRequest line for Bash B with B's input; A's tool_result record 3 s later; then answer_permission for B (allow); separately, a Stop line on a session with a pending request"
          rejects: "a clear-on-any-later-record heuristic: after A's result B is still pending and the session in Permission, the allow is accepted and writes answers/<B>.json with behavior allow; after the Stop the cleared request has a deny answer file with reason session_ended"
        - id: bridge-grace-end-denies-pending
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: grace_end_denies_pending_and_new_requests_so_the_closing_write_runs
          scenario: "a session in Permission with one pending request; end(FinalWrite, EndCause::GraceExpired); then a second PermissionRequest line during the closing turn"
          rejects: "a final write that waits on answers nobody can give: both requests get deny answer files (reason holder) at once and the closing text is delivered without waiting for the bound"
        - id: bridge-final-write-waits-for-idle
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: end_delivers_the_closing_message_only_after_idle_and_tears_down_after_stop
          scenario: "a Working session with one SubagentStart line; end(FinalWrite); then a Stop line, a SubagentStop line and a second Stop line; closing_message returns 'CLOSE'"
          rejects: "an end that pastes the closing message mid-turn or tears down while a subagent runs: 'CLOSE' is delivered only after the first Stop, and teardown happens only after the second Stop with no subagent running"
        - id: bridge-final-write-bounded
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: end_tears_down_at_the_bound_without_a_stop
          scenario: "final_write_bound 1 s; an Idle session; end(FinalWrite) and no further spool line"
          rejects: "an end that waits forever for a Stop: teardown is recorded within 3 s and an Ended event follows"
        - id: bridge-pressure-ends-on-marker
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: a_pressure_session_ends_when_its_marker_appears_without_a_closing_message
          scenario: "a Pressure session made Idle; touch <session dir>/done/complete; also call end(FinalWrite) on a second pressure session"
          rejects: "a pressure session that gets the closing message or ignores the marker: the first ends with EndOutcome::Marker and no deliver of the closing text, the second is torn down without one"
        - id: bridge-death-is-failed
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: a_process_gone_outside_ending_fails_the_session_and_denies_pending_requests
          scenario: "an Idle session with a pending PermissionRequest; the host's identity starts returning Dead"
          rejects: "a bridge that restarts the session or reports Ended: the state is Failed, an Ended event carries EndOutcome::Failed, and the request's answer is a deny with reason session_ended"
        - id: recovery-leaves-unverifiable
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: recovery_leaves_an_unverifiable_record_and_its_directory_alone
          scenario: "two session records written with write_record before Bridge::new: one whose identity the host reports Unverifiable, one Dead"
          rejects: "a sweep that destroys on missing evidence: the unverifiable one is in left_alone with its socket, its directory and record still exist, and teardown was never called with its socket; the dead one is in torn_down (by id) and its directory is gone"
        - id: teardown-failure-keeps-record
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: a_failed_teardown_keeps_the_session_record_and_directory
          scenario: "the host's teardown returns an error; end(Teardown, EndCause::Caller) on a started authoring session; then start_session for a new authoring session"
          rejects: "a teardown that removes the record after a failed kill, or one that leaves the session Ending forever: session.json and the directory remain, the state is Failed, and the new start succeeds"
        - id: marker-end-keeps-children
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: a_marker_end_keeps_the_recorded_child_and_its_log_until_release
          scenario: "a Pressure session with a child recorded by record_child (a sleep process the test spawned) and a codex.log file in its directory; touch the marker; then release_session"
          rejects: "a marker end that terminates the Codex child or deletes codex.log: after Ended { Marker } the child is alive and codex.log readable; after release_session the child is gone and the directory removed"
        - id: provisional-record-recovered
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: recovery_tears_down_a_provisional_record_whose_socket_exists
          scenario: "a session directory holding a provisional record (pid None) whose socket the ScriptedHost reports present, and a second session directory with no record at all"
          rejects: "a recovery that sees only complete records and leaves a live session nobody can find: the provisional one is torn down (teardown called with its socket, Ended emitted), the record-less directory is in left_alone"
        - id: recovery-denies-pending-permission
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: recovery_denies_an_unanswered_request_before_the_final_write
          scenario: "a VerifiedAlive authoring record whose spool ends with a PermissionRequest and no answer file; recover()"
          rejects: "a recovery whose final write waits on a card nobody sees: an answers/<id>.json deny with reason server_stopping is written, then the closing text is delivered"
        - id: launch-script-escapes-and-drops-stage-env
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: launch_script_escapes_the_opening_message_and_drops_stage_variables
          scenario: "launch_script with a display name holding a single quote, $(touch pwned), a newline and a leading dash, a distinctive opening message OPENING-7f3a in the guidance, and an env list that includes LOOM_STAGE_ID, LOOM_SESSION_ID and LOOM_WORK_DIR; the script run with sh against a fake claude that writes its argv and environment as JSON"
          rejects: "unescaped interpolation, the opening message on the command line (world-readable through /proc), or an inherited stage identity: the fake claude receives the name as one argument byte-for-byte, no pwned file exists, neither the script text nor the argv contains OPENING-7f3a, LOOM_PLAN_SESSION and LOOM_PLAN_STATE are set and the three stage variables are absent"
        - id: socket-list-skips-planning
          file: tests/plan_session_bridge_runtime_contracts.rs
          test: list_loom_sockets_skips_planning_sockets
          scenario: "TMUX_TMPDIR pinned to a temp dir holding tmux-<uid>/loom-plan-3f9a0c41b7e2 and tmux-<uid>/loom-session-abc; list_loom_sockets on a temp work dir"
          rejects: "a listing that reports planning sockets as unattributed stage sockets: only the loom-session-abc socket is listed"

    - id: integration-verify
      name: "Integration Verification"
      summary: "Confirms the session bridge works on the merged tree: full suite, a security review against section 12, the recovery test on real tmux, then the operator's end-to-end run with Claude Code."
      stage_type: integration-verify
      skills: ["loom-rust", "loom-security-audit"]
      working_dir: "."
      dependencies: ["bridge-runtime"]
      description: |
        Final verification of doc/plans/PLAN-feat-planner-07-session-bridge.md (doc/plans/IN_PROGRESS-PLAN-feat-planner-07-session-bridge.md while running). Verify FUNCTIONAL INTEGRATION, not only tests passing. NEVER Claude Code auto-memory.
        Use parallel subagents and skills to maximize performance.
        CONTEXT: read the plan, doc/plans/briefs/feat-planner-07-session-bridge/common.md, loom memory show --all, and the knowledge sections the plan cites. working_dir is the repository root.
        BUILD AND TEST (zero tolerance; nothing is pre-existing): every acceptance command below, through one loom check.
        FUNCTIONAL (from this session's shell; it cannot bind a port or create a tmux socket, so the server and tmux paths are proven by the tests and the operator run): pipe a hand-written PermissionRequest payload into cargo run -q --manifest-path loom/Cargo.toml -- plan hook PermissionRequest --session-dir <a scratch session dir holding a Summary guard.json> and confirm the deny decision and the auto_denied spool line. Record the tmux-tests memory of bridge-runtime (the real-tmux tests skip in every sandbox; the operator run below is where they run).
        CODE REVIEW: parallel loom-code-reviewer subagents. Security, judged against doc/product/section_12.md "No way for a session to approve itself", "Input delivered to the pane", "Request limits", the threat-model rows for planning sessions and "A document with escape sequences", and section 07 "Permission prompts": answer files written only by the server, atomically, under ids that are uuids; the session cannot reach answers/, the tmux socket or the server port; every pasted text sanitised and bounded; no load-buffer -w; launch.sh escapes every argument and never carries LOOM_STAGE_ID, LOOM_SESSION_ID or LOOM_WORK_DIR; commands refused from every connection but the holder's; host-side opens of session-writable paths through safe_read/safe_fs; every destructive step verifies ProcessIdentity and does nothing on missing evidence. Concurrency: no host call or listener under the bridge lock; no lost delivery or double delivery across the queue, interrupt and end paths; the driver survives a panicking listener. Architecture: every API in doc/plans/briefs/feat-planner-07-session-bridge/common.md exists with the published signature (plans 8 to 11 bind to it); plan 2's extension rule kept (one variant, one arm, one install line). Test coverage: the contradiction cases of section 07's state rules, the mutation notes. Fix or dispute every finding; never defer one.
        SUGGESTIONS: weigh every pending reviewer suggestion the signal lists; resolve each implemented one with loom memory resolve <id> --outcome implemented --reason <what changed>.
        OPERATOR RUN (last, after the gate is green and every finding is closed): the Claude Code end-to-end test needs a logged-in Claude Code and network, and the real-tmux tests need Unix sockets, which no sandbox here has. File loom stage block integration-verify "Operator: from the repository root, with tmux and a logged-in Claude Code, run LOOM_TEST_REQUIRE_SANDBOX_FREE=1 cargo test --manifest-path loom/Cargo.toml --test plan_session_bridge_e2e -- --include-ignored --nocapture && LOOM_TEST_REQUIRE_SANDBOX_FREE=1 cargo test --manifest-path loom/Cargo.toml --lib plan_session:: && LOOM_TEST_REQUIRE_SANDBOX_FREE=1 cargo test --manifest-path loom/Cargo.toml --lib orchestrator::terminal::tmux::, then record loom memory note \"e2e: <pass|fail>, <claude --version>, <tmux -V>, interrupt Stop <seen|not seen>, /compact order <...>, TMPDIR <...>, <what happened>\" and loom stage retry integration-verify". A skip is a failure under that variable. On retry, read that memory; a fail is a finding to fix like any other.
        Record discoveries to loom memory for knowledge-distill, including any knowledge file the tree contradicts: loom memory note "stale-knowledge: ...".
      acceptance:
        - "cargo build --manifest-path loom/Cargo.toml"
        - command: "cargo test --manifest-path loom/Cargo.toml --all-targets --no-fail-fast"
          timeout_secs: 3600
        - command: "cargo test --manifest-path loom/Cargo.toml --doc --workspace --all-features -- --test-threads=1"
          timeout_secs: 1200
        - "cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings"
        - "cargo fmt --manifest-path loom/Cargo.toml --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path loom/Cargo.toml --workspace --all-features --no-deps"
        - "printf '{\"hook_event_name\":\"SubagentStop\"}' | cargo run -q --manifest-path loom/Cargo.toml -- plan hook Stop --session-dir loom/target/no-such-session"
      wiring:
        - source: "loom/src/plan_server/routes.rs"
          pattern: "chat::install("
          literal: true
          description: "the planning server installs the chat bridge"
        - source: "loom/src/plan_session/hook_cli.rs"
          pattern: "\"PermissionRequest\" =>"
          literal: true
          description: "the permission hook is reachable from loom plan hook"

    - id: knowledge-distill
      name: "Knowledge Distillation"
      summary: "Records the session bridge (spool, permission answers, state rules, delivery, recovery, the chat model) and the operator run's result in the knowledge base."
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      description: |
        Curate all stage memories into permanent knowledge; update user docs. NEVER Claude Code auto-memory.
        SINGLE-AGENT: do NOT spawn subagents; memories are compact summaries; keep code spot-reads narrow.
        START with loom memory pending --group; read the plan and the knowledge sections it touches.
        CORRECTIONS FIRST: apply every stale-knowledge memory in place with loom knowledge replace-section <file> "<heading>" "<body>", never loom knowledge update.
        Then curate: a new tier-2 topic architecture/planning-session-bridge (sessions in their own tmux servers with loom-plan- sockets, the session directory, the nine spool events and the event-name check, the permission answer files and the 3590/3595/3600 s ordering, the state machine and its two honesty rules, delivery only while Idle with the bracketed paste and the 10 s confirmation, interrupt and the returned queue, the final write and its bound, recovery and its do-not-destroy rule, the chat model and its deltas, the guidance slot), with a 2-4 line summary and link in architecture.md, and loom knowledge annotate architecture/planning-session-bridge --blurb "Planning bridge: tmux, spool, chat model". Add to mistakes/tmux-backend.md or mistakes/spurious-waiting-for-input.md only what the stages actually hit. Record the transcript record shapes and hook payload fields the stages confirmed or found different (memories tagged e2e:, tmux-tests:, or naming a payload field) in the topic, current truth only. Add the operator e2e result (from the e2e: memory) to the topic's verification note. TIER ROUTING as usual.
        DOCS: README.md gets one sentence under the loom plan section that sessions run in tmux servers named loom-plan-<id> and can be watched read-only with tmux -L <socket> attach -r (section 07 Naming); skip if plan 2's section already says it.
        INDEX BUDGET: doc/loom/knowledge/INDEX.md must stay at or under 15,000 bytes after this stage's writes (plan 1 trims it to that). Keep every blurb you write under 50 characters; shorten the longest existing blurbs with loom knowledge annotate <category>/<slug> --blurb "<shorter>" if a write pushes it over; never delete a topic to save bytes; never run loom knowledge check --write-baseline to absorb an INDEX size issue.
        SUGGESTIONS: record every unimplemented reviewer suggestion in concerns or its topic, then resolve it promoted, merged or discarded.
        RECEIPTS: every memory taken into knowledge gets loom memory resolve <id> --outcome promoted|merged|discarded|deferred right after the write that used it; finish with loom memory pending --strict and resolve whatever it lists.
        LAST, if this stage removed structural issues, ratchet the baseline: loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
        - "loom memory pending --strict"
        - 'test "$(wc -c < doc/loom/knowledge/INDEX.md)" -le 15000'
        - "test -s doc/loom/knowledge/architecture/planning-session-bridge.md"
        - "rg -qF 'planning-session-bridge' doc/loom/knowledge/architecture.md"
      files: ["doc/loom/knowledge/**", "README.md"]
```

<!-- END loom METADATA -->
