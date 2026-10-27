# Codex units of stage planning-sandbox

Each unit is forwarded by the orchestrator through one `loom-codex-forwarder` subagent in the
FOREGROUND with the model flags in its block below (always `--effort high`) and an explicit Bash timeout of 600000 ms.
The unit text the orchestrator forwards is the block below, verbatim. Every unit reads
`doc/plans/briefs/feat-planner-06-session-sandbox/common.md` (the pinned public API) first.

Rules for every unit: write only the files named; run at most `cargo build` or
`cargo test --no-run` from `loom/` as a compile check; never run git; keep files under 360 lines
and functions under 45 (a 10% margin: `cargo fmt` grows them); double literal braces inside
`format!`; put CLI placeholders in rustdoc inside backticks. The orchestrator runs the proof
command and `git status --short` after each unit.

## C1 kind (wave 1)

- Model: `--model gpt-6.1-terra --effort high`.

- Owns: `loom/src/plan_session/kind.rs` (seeded with a purpose line).
- Reads: `loom/src/planning/slug.rs` (`plan_slug`).
- Entry points: `SessionKind`, `SessionKind::label`, `SessionKind::pressure_bundle`, `SessionDir`
  and its eleven path methods, exactly as common.md "plan_session::kind".
- Steps: 1. write the two types and their methods with doc comments; 2. add a `#[cfg(test)] mod tests`
  at the bottom with one test per method (labels, the bundle of the plan file `PLAN-x.md` under
  doc/plans is the directory `briefs/x` beside it, a non-plan file name gives None, every SessionDir path ends in the name
  common.md gives); 3. compile.
- Done: `cargo test --lib plan_session::kind::` passes.
- Traps: `SessionKind` serialises with `#[serde(tag = "kind", rename_all = "snake_case")]`, so
  `Pressure { plan }` is `{"kind":"pressure","plan":"..."}`; keep that exact attribute.

## C2 sandbox snapshot tests (wave 3, after W1)

- Model: `--model gpt-6.1-sol --effort high`.

- Owns: `loom/src/plan_session/sandbox/tests.rs`.
- Reads: `loom/src/plan_session/sandbox/mod.rs`, `filesystem.rs`, `permissions.rs`, `reads.rs`
  (W1 wrote them in this stage; codex `loom map` shows the base layer, so read the files).
- Entry points: `build_settings`, `write_session_sandbox`, `ensure_runtime_grants`.
- Steps: 1. a fixture helper building the tree of w1-settings.md step 7 in a `tempfile::TempDir`
  with a runtime from `crate::planning::runtime::runtime_paths_under`; 2. one whole-JSON snapshot test
  for the authoring kind comparing against a `serde_json::json!` value with the temp paths
  substituted; 3. the targeted tests w1-settings.md step 7 lists for files and grants (0600 modes,
  planted symlink refused, `ensure_runtime_grants` twice keeps the lock byte-identical).
- Done: `cargo test --lib plan_session::sandbox::tests` passes.
- Traps: every permission path is `//` plus the absolute path; the deny list includes the tool
  denies of common.md "Tool denies".

## C3 kind-specific settings tests (wave 3, after W1)

- Model: `--model gpt-6.1-sol --effort high`.

- Owns: `loom/src/plan_session/sandbox/tests_kinds.rs`.
- Reads: as C2, plus the fixture helper shape of C2 (write your own copy; do not import the C2 module).
- Entry points: `build_settings`, `assert_planning_settings`.
- Steps: 1. whole-JSON snapshots for the summary and pressure kinds; 2. the per-kind and per-key
  tests of w1-settings.md step 7 (tool denies per kind, the redirected-runtime `planning_root`
  deny, no allow under `doc/loom/`, the hooks object order, one refusal test per
  `assert_planning_settings` key); 3. compile.
- Done: `cargo test --lib plan_session::sandbox::tests_kinds` passes.
- Traps: pressure keeps `SendMessage` and `ListAgents` allowed (not in deny); summary has no
  `WebSearch` allow. Plan 11 later adds a top-level `teammateMode` key to the pressure snapshot here.

## C4 hook entry (wave 4, after W2)

- Model: `--model gpt-6.1-terra --effort high`.

- Owns: `loom/src/plan_session/hook_cli.rs`.
- Reads: `loom/src/plan_session/guard/mod.rs` (`run_pre_tool_use`, written by W2 in this stage).
- Entry points: `hook_cli::run(event: &str, session_dir: &Path, stdin: impl Read, stdout: impl Write) -> i32`.
- Steps: 1. `match event { "PreToolUse" => write the guard answer when Some, _ => nothing }`;
  2. return 0 always, ignoring stdout write errors without panicking; 3. a `#[cfg(test)] mod tests`
  with an unknown event (empty output, 0) and PreToolUse against a missing session dir (a deny line, 0).
- Done: `cargo test --lib plan_session::hook_cli` passes.

## C5 plan hook command (wave 4)

- Model: `--model gpt-6.1-terra --effort high`.

- Owns: `loom/src/commands/plan/hook.rs` (seeded).
- Entry points: `pub fn execute(event: &str, session_dir: &Path) -> anyhow::Result<()>`, which calls
  `crate::plan_session::hook_cli::run(event, session_dir, std::io::stdin().lock(), std::io::stdout().lock())`
  and returns Ok.
- Steps: 1. write the function with a doc comment saying Claude Code runs it for planning sessions;
  2. compile.
- Done: `cargo build` passes.

## C6 CLI variant (wave 4)

- Model: `--model gpt-6.1-terra --effort high`.

- Owns: `loom/src/cli/types_ops.rs`.
- Entry points: `PlanCommands`.
- Steps: 1. add `#[command(hide = true)] Hook { event: String, #[arg(long)] session_dir: std::path::PathBuf }`
  with a doc comment ("the hook entry Claude Code runs for planning sessions; not user-facing");
  2. compile.
- Done: `cargo build` passes. Traps: `event` is a positional `String`, never a `ValueEnum`.

## C7 CLI dispatch (wave 5, after C6)

- Model: `--model gpt-6.1-terra --effort high`.

- Owns: `loom/src/cli/dispatch.rs`.
- Entry points: `dispatch_plan` (about line 104 at `b0f91c00`; plans 1, 2 and 4 added arms since).
- Steps: 1. add the arm `PlanCommands::Hook { event, session_dir } => plan::hook::execute(&event, &session_dir),`;
  2. compile. The file must stay under 400 lines; if the arm would cross it, stop and report.
- Done: `cargo build` passes. Note: C6 changed `cli/types_ops.rs` in this stage (the `Hook` variant).

## C8 guard write-table tests (wave 4, after W2)

- Model: `--model gpt-6.1-sol --effort high`.

- Owns: `loom/src/plan_session/guard/tests.rs`.
- Reads: `loom/src/plan_session/guard/mod.rs`, `guard/paths.rs` (W2, this stage),
  `loom/src/plan_session/sandbox/mod.rs` (`write_session_sandbox`).
- Entry points: `run_pre_tool_use`.
- Steps: 1. the fixture helper (same tree as C2, own copy) writing `guard.json` through
  `write_session_sandbox`; 2. one test per write-table row and kind listed in w2-guard.md step 6
  (the write rows, the temp-dir rows, the pressure rows, the mid-session lock, FIFO and directory
  targets, relative paths, `..` escapes, oversize, invalid JSON, missing guard.json,
  `hook_event_name: "PostToolUse"`); 3. assert the exact deny JSON shape once and that no output
  ever contains `"allow"`.
- Done: `cargo test --lib plan_session::guard::tests` passes.

## C9 guard read tests (wave 4, after W2)

- Model: `--model gpt-6.1-sol --effort high`.

- Owns: `loom/src/plan_session/guard/tests_reads.rs`.
- Reads: as C8.
- Steps: 1. own fixture helper; 2. the read tests of w2-guard.md step 6 (credentials, runtime lock,
  project token, drafts allowed, source file allowed, `Grep` at home denied and at root allowed);
  3. the link and pseudo-path tests (`/proc/self/cwd/...`, `/dev/fd/0`, a symlink under `doc/` for
  Read and Grep, a symlinked ancestor of the root, a redirected runtime).
- Done: `cargo test --lib plan_session::guard::tests_reads` passes.

## C10 README entry (wave 3)

- Model: `--model gpt-6.1-terra --effort high`.

- Owns: `README.md` (repository root).
- Steps: 1. under `### Plan Commands`, add one entry for `loom plan hook <event> --session-dir <dir>`,
  matching the surrounding entry format: a hidden internal command that Claude Code runs for
  planning sessions (the PreToolUse path guard), not run by hand; 2. no usage example.
- Done: the orchestrator reads the diff.

## C11 loom-usage sentence (wave 3)

- Model: `--model gpt-6.1-terra --effort high`.

- Owns: `skills/loom-usage/SKILL.md`.
- Steps: 1. where `loom plan` subcommands are described (find them with `rg -n "loom plan"`), add
  one sentence that `loom plan hook` is internal and never run by hand.
- Done: the orchestrator reads the diff. No completions entry (the command is hidden).
