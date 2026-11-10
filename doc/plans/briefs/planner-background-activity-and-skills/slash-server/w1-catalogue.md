# W1: the command catalogue, the side process and `enabledPlugins` (step B2)

Read `common.md` in this directory first. You are a `loom-senior-software-engineer`. You run in wave 1 beside W5 (markdown only). Decisions B-D1 and B-D2 of the plan are the specification.

## Files owned

`src/plan_session/commands/**` (new: `mod.rs`, `builtins.rs`, `list.rs`, `side.rs`, your tests), `src/plan_session/mod.rs` (the `pub mod commands;` line), `src/plan_session/launch/script.rs`, `src/plan_session/sandbox/mod.rs`, `src/plan_session/sandbox/tests.rs`, `src/plan_session/sandbox/tests_kinds_fixture.rs`.

Frozen, read only: `src/plan_session/commands/contract_tests.rs` (three contracts) and `src/plan_session/sandbox/plugins_contract_tests.rs` (one). Read only too: `tests/plan_session_bridge_runtime_contracts.rs` and its `launch.rs` (they run `launch_script` with `sh`; its output must stay byte-identical), and the probe harness `tests/plan_session_bridge_e2e/probes/side.rs`, `side_run.rs` (the P2 launch, as evidence).

## What exists (verified 2026-11-10)

- `launch/script.rs:12-49` `launch_script(spec, settings, env)` renders `exec env -i <environment words> <claude> --session-id <id> --name <display name> --settings <settings> --setting-sources user,project --model <m> --effort <e> --permission-mode <mode> --plugin-dir <dir> --strict-mcp-config --append-system-prompt-file <brief> [<opening>]`, every word shell-escaped, behind `cd <project>` and the `OwnerWatch` hook line; `:51-97` `environment_words` pins `HOME`, `LOOM_PLAN_STATE`, `LOOM_PLAN_SESSION` and drops `LOOM_STAGE_ID`, `LOOM_SESSION_ID`, `LOOM_WORK_DIR` and the parent tmux variables. The `script.rs` tests build a `LaunchSpec` with `spec(base)` (`:117-`).
- `sandbox/mod.rs:136-167` `build_settings` builds the settings object (`sandbox`, `permissions`, `worktree`, `hooks`, and `teammateMode` for pressure and team scopes); `assert_planning_settings` (`sandbox/assert.rs`) has no key allowlist. `sandbox/tests.rs` `authoring_settings_match_whole_json_snapshot` and `tests_kinds_fixture.rs` `expected_common_for` pin the whole JSON (sibling plan `PLAN-live-test-lane.md` H1 also owns those two files; whichever plan merges second re-reads them).
- The `initialize` answer: 117 rows `{name, description, argumentHint, aliases?, builtin?}`; `builtin: true` on 55 (19 bundled skills, 36 native commands, `probe-notes.md` "Built-in rows" names both groups); source markers in the description: a `(<plugin>)` prefix with a `plugin:name` name, a `(user)` suffix, a `(claude.ai sync)` suffix, `(dynamic workflow)` on `deep-research` (each suffix after one space); no row carries a source or a command type.
- P2 side process: argv as `common.md` says, stdin one `{"type":"control_request","request_id":"<id>","request":{"subtype":"initialize"}}` line, answer in 383 ms as a `control_response` whose `.response.response.commands` is the row list, a `system` `commands_changed` line with the whole list about 3 s after a skill file changed, no `system/init` without a turn, no model request.

## Steps

1. **Argv builder.** In `launch/script.rs`, factor `claude_argv(spec, settings) -> Vec<OsString>` (the flag words from `claude_bin` through `--append-system-prompt-file <brief>`, without the argv opening, which `launch_script` appends itself) and `claude_env(spec, env) -> Vec<(OsString, OsString)>` (the filtered and pinned environment of `environment_words`, unescaped). `launch_script` escapes and renders them; its output does not change by a byte.
2. **`builtins.rs`.** The native table: one row per native command of 2.1.321 (the 36 of the headless list and the 21 missing from it), each `{name, aliases, description, argument_hint, class, reason}` per `common.md`'s class table. Rows in the headless list copy the fixture's description, aliases and argument hint verbatim; the 21 others get a short description in plain words. A comment above the table names Claude Code 2.1.321 and `probe-notes.md` "Built-in rows" as its source, and says a later version's new native command reads as a bundled skill until the table learns it (the plan's "Parity limit").
3. **`list.rs`.** `CommandTable` and its types (`common.md`). `from_initialize(rows, generation)`: a row whose name is a native row is replaced by the native row; any other `builtin: true` row is `Bundled`; otherwise `Plugin` (a `(<plugin>)` description prefix and a name with `:`), `Personal` (a `(user)` suffix), `Account` (a `(claude.ai sync)` suffix), else `Skill`; strip the marker and the space before it from the description (and `(dynamic workflow)`); every non-native row is `Run`. Rows without a string `name` are skipped. Cap every stored string (names 200 characters, descriptions 1,000, hints 200; the mapper's `cap_chars` pattern). `lookup` takes a name without its slash and matches a name or an alias. `loading()` and `unavailable(reason)` hold the native rows only.
4. **`side.rs`.**
   - `side_argv(session_argv, side_settings)`: drop `--session-id`, `--name`, `--append-system-prompt-file` and the word after each; replace the word after `--settings` with `side_settings`; append `-p`, `--input-format`, `stream-json`, `--output-format`, `stream-json`, `--verbose`, `--no-session-persistence`.
   - `side_settings(session)`: a copy without `hooks`, with `"disableAllHooks": true`; every other key (the sandbox block, `permissions`, `enabledPlugins`) kept.
   - `SideProcess::start(spec, deadline, on_table)`: write `spec.settings` to `spec.settings_path` (0600, through the crate's safe write helpers, refusing a symlink at the path); spawn `spec.program` with `spec.args`, `env_clear()` then exactly `spec.env`, `spec.cwd`, stdin and stdout piped, stderr to null; write the one `initialize` line and keep stdin open; read stdout line by line on a named thread (`side-commands`), bounded as `common.md` says; on the `control_response` whose `request_id` matches, call `on_table(Arc::new(CommandTable::from_initialize(rows, 1)))`; on each later `system` `commands_changed`, `from_initialize(rows, generation + 1)`; on the deadline, a cap, EOF before the answer or a malformed answer, kill, `wait`, delete the settings file and call `on_table(Arc::new(CommandTable::unavailable(<why>)))` once. `stop()` and `Drop` kill, wait and delete the file; neither blocks longer than the kill and reap. `pub const STARTUP_DEADLINE: Duration = Duration::from_secs(20);`.
5. **`enabledPlugins`.** `build_settings` adds the top-level `"enabledPlugins": {"loom-ui@skills-dir": false}` for every kind (B-D2: the planner shows background work itself; `PLAN-loom-ui-mods.md` Decision 4 gives the `<name>@skills-dir` id). Add the key to the expected JSON of `authoring_settings_match_whole_json_snapshot` and `expected_common_for`; no other assertion changes.
6. **Tests** (yours, in `src/plan_session/commands/tests.rs` and `tests_side.rs`, declared from `commands/mod.rs`):
   - every one of the 57 native names is in the table once, with one class and, for Terminal only, one of the four reasons; `lookup` finds `reset`, `new`, `cost` and `stats` through their canonical rows;
   - the source of every fixture row (the `commands_changed` line's extra row, `probe-added`, reads as `Personal`, as the other `(user)` rows do);
   - a fake `claude` sh script that answers `initialize` with line 1 of `probe-notes/p2-side-initialize.side-loom-ui-disabled.jsonl` (its `request_id` rewritten to the one sent) and then prints line 2: `on_table` sees generation 1 with 117 rows' entries, then generation 2; the process got exactly one stdin line; `stop()` kills and reaps it and deletes the settings file;
   - an output cap: a fake that prints one 2 MiB line is killed and reaped and the table is `Unavailable`;
   - EOF before the answer gives `Unavailable` once.
   Fake scripts live in a TempDir; reap is checked with `nix::sys::signal::kill(pid, None)` failing with `ESRCH`.
7. **Run** `cargo test --lib plan_session::commands:: plan_session::sandbox:: plan_session::launch` and `cargo test --test plan_session_bridge_runtime_contracts` once ready.

## Done when

The four frozen contracts and your tests pass, `launch_script`'s output is unchanged (the runtime contracts and `script.rs` tests pass), and every new file is at most 400 lines with functions at most 50.

## Traps

- The side process must never get a positional prompt: with `-p` a prompt is a user message and a model request. `claude_argv` leaves the opening out.
- `env_clear()` first: the plan server's own environment (its `LOOM_*` variables) must not reach the side process; `claude_env` already drops the stage identity.
- The reader thread outlives nothing: `stop()` and drop end it, and a panic in it must not leave the child running.
