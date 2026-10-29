# A2: run state, commit of `doc/` changes, starting a run, the dashboard child, the run strip

Dispatch: `loom-codex-forwarder`, `--model gpt-6.1-sol --effort high`, foreground, Bash timeout 600000 ms. The orchestrator partitions this territory into ordered file-sized forwards before dispatch to fit the 540 s deadline; workers never delegate. All test/proof commands below are for the orchestrator; workers run compile-only checks, no git, and never write under `.loom/`.

SEED: every file you own already exists as a one-line placeholder (the orchestrator created it so the crate compiles at every wave); replace it whole.

LANE: codex units write `changes.rs` (CB3, `split_changes`), `strip.rs` (CB2, `build_strip` and the strip thread) and `dashboard_parse.rs` (CB1, `parse_dashboard_startup`, `strip_token`) in parallel with you; see `doc/plans/briefs/feat-planner-11-execution-handoff/app-handoff/codex-units.md`. Your `dashboard.rs` re-exports `parse_dashboard_startup` (`pub use super::dashboard_parse::parse_dashboard_startup;`) and your `install.rs` calls `strip::spawn_strip` (CF6's `mod.rs` declares every module). The sections below on those files are their specification.

Stage `app-handoff` of `doc/plans/PLAN-feat-planner-11-execution-handoff.md`. Tier: codex sol/high (`loom-codex-forwarder`). Never run git yourself, never spawn subagents. Orchestrator proof at
the end: `cargo test --lib plan_server::run::` from `loom/`. Frozen contract files
`tests/plan_handoff_contracts.rs` and `tests/plan_handoff_contracts/**` are READ-ONLY.

## Files you own (relative to `loom/`)

- `src/plan_server/run/routes.rs` (`pub fn all() -> Vec<RouteSpec>`, your routes plus A3's) and
  `run/install.rs` (`pub fn run(ctx: &Arc<PlanContext>)`, which calls `strip::spawn_strip`); codex unit
  CF6 owns `run/mod.rs`, which declares every file of this module and delegates to these two; A3's `src/plan_server/run/abandon.rs` exposes
  `pub(super) fn routes() -> Vec<RouteSpec>` and `pub(crate) fn view(ctx: &PlanContext) -> Option<AbandonView>`;
  append its routes in `routes()`)
- `src/plan_server/run/state.rs`, `commit.rs`, `start.rs`, `child.rs`, `dashboard.rs`,
  `tests.rs` (new)
- `src/commands/init/work_state.rs` (widen `holds_orchestration_state` to `pub(crate)`)
- `src/commands/init/mod.rs` (one line: `pub(crate) use work_state::holds_orchestration_state;`;
  stage `pressure-pipeline` already added `pub use plan_setup::preflight_plan;` there)

## Spec

`doc/product/section_11.md` lines 200-368 ("Starting a run" through "While plans run").

## `GET /api/run/state` (Viewer) → `RunStateView` (`state.rs`)

- `workspace` and `offer` (section 11 "The workspace's state"): `WorkDir::new(root)`; absent
  `.loom/work` → `Absent`/`Start`. Present but `!holds_orchestration_state` → `Absent`. Else the
  plan path from `crate::fs::plan_lifecycle::get_plan_source_path(&work_dir)`: file name
  `PLAN-` → `Initialized`/`StartInitialized`; `IN_PROGRESS-` with
  `crate::daemon::DaemonServer::is_running(work_dir.root())` → `Running`/`NoneRunning`; without
  → `Stopped`/`Resume`; `DONE-` → `Done`/`StartNext`. `active_plan` = that path, repo-relative.
- `plans`: every written plan in section 01 order (plan 1 `Project` reading order: metaplan
  heading order, then unlisted by file name), `listed` false for unlisted; `preselected` = the
  first (the initialized plan when `Initialized`).
- `branch`: `crate::git::branch::operations::current_branch(root)`.
- `doc_changes` / `blocking_changes` (`changes.rs`): one `git status --porcelain=v1 -z
  --untracked-files=all` through `crate::git::runner::run_git` (reading only; hooks are
  irrelevant to status). Paths under `doc/` (all statuses, untracked included) go to
  `doc_changes`; outside `doc/`, only tracked modified or staged paths (not `??`) go to
  `blocking_changes` (section 11 "What loom requires": untracked files are allowed). Exclude
  `doc/loom/summaries/**/.*.tmp-*` (a crashed seal's temp file; plan 4 sweeps them under its
  store lock) from `doc_changes`. Parse `-z`
  records (rename records carry two paths: the destination counts; the source is listed too
  when it lies under `doc/`). Handle hundreds of paths: no `head`, no line cap.
- `backends`: `[Tmux, Native]`, `Native` omitted when plan 2's headless detection says the
  server runs headless (`plan_server::browser::EnvView::current()` with `decide(&env, false)`); `default_backend: Tmux`.
- `clean_notice`: workspace `Done` or `Initialized` with another plan chosen (the browser
  decides the latter; send true for `Done`).
- `abandon`: A3's `abandon::view(ctx)` when the workspace is `Running` or `Stopped`.
- `dashboard_url`: the dashboard child's URL WITHOUT its `token` query parameter. The tokenized
  URL goes only in the dashboard route's response to the authoring holder
  (`ctx.authoring().is_holder`), never in a broadcast or a viewer's response.
- `busy`: the job running now, if any. One job at a time per server (`Mutex<Option<RunJob>>`
  in an extension struct `RunJobs` you put in `ctx.extensions()` at `install()`; A3 uses it too).

## `POST /api/run/commit` (Holder) (`commit.rs`)

Body `CommitRequest`. Refuse (400 `invalid_paths`) unless every path is in the CURRENT
`doc_changes` (recompute; never trust the browser) and starts with `doc/`; refuse an empty
message or one over 2,000 bytes; refuse while read-only. Run, in the project root, with stdin
null and the server's environment: `git add -- <paths...>` then `git commit -m <message>`,
both through `std::process::Command::new("git")` directly, exactly
`git -c core.quotepath=off --literal-pathspecs add -- <paths...>` then
`git --literal-pathspecs commit -m <message> -- <paths...>` (a pathspec on the commit gives
`--only` semantics: the commit holds exactly the ticked paths, never something else already
staged; `--literal-pathspecs` stops `doc/plans/*` acting as a glob). NOT `crate::git::runner::run_git`:
it prepends `NO_HOOKS_ARGS` (`src/git/runner.rs:116-125`), and section 11 requires the user's
hooks and signing. Never pass `--no-verify`, never push. Stream both commands' output as
`RunOutput { job: Commit, step: "git add" | "git commit", .. }` frames and end with a `done`
frame carrying the exit code; a failure stops the sequence and leaves the output.

## `POST /api/run/start` (Holder) (`start.rs`, `child.rs`)

Body `RunStartRequest`. Recompute the state; refuse 409 `blocked` when `blocking_changes` is
non-empty, `not_written` when `plan` is not a written plan, `running` when the workspace is
`Running`, `needs_clean` when the workspace is `Done` or `Initialized` for another plan and
`clean` is false. Then run, in order, `<current_exe> init <plan> --backend <tmux|native>
[--clean]` and `<current_exe> run` (`std::env::current_exe()`, the running `loom`), project
root as cwd, the server's environment, stdin null; stream each line as `RunOutput { job: Start,
step: "loom init" | "loom run" }`; the first non-zero exit stops and leaves its output. Add no
checks beyond these (section 11 "Start": loom's commands are the authority).

`child.rs`: `pub(super) fn run_streamed(cmd: Command, on_line: impl FnMut(OutputStream, &str)) -> std::io::Result<ExitStatus>`:
stdout and stderr each read on its own thread line by line (no pipe left undrained; a child
printing 1 MiB must not block), lines over 4,096 bytes cut on a char boundary,
`context::untrusted::inline_safe` is NOT applied here (the browser renders text, never HTML);
returns the exit status. Bound each child at 30 minutes (kill on expiry, exit code `-1`).

## Dashboard child (`dashboard.rs`)

`POST /api/run/dashboard` (Viewer may request; only the holder's response carries the token):
the first call starts `<current_exe> status --web --host <ctx.bind().local.ip()>` plus `--terminals` when the
run's backend is tmux (read `.loom/work/config.toml` `[terminal] backend` through
`crate::fs::work_dir::read_terminal_config`, else the user default), cwd the project root,
stdout piped. Parse the URL with `pub(crate) fn parse_dashboard_startup(lines: &[&str]) -> Option<String>`
over the format plan 2 pins (`commands/status/web/tests/startup.rs`): loopback first line
`loom dashboard: http://<addr>/...  (...)`; remote first line `loom dashboard listening on ...`
followed by `  bootstrap from this machine at: http://...` lines (prefer `this machine`). Read at
most 20 lines within 10 s. Keep the child for the server's lifetime (a `ShutdownHook { name:
"dashboard", drain: kill, abort: kill }`); a second call returns the same URL. Over SSH, the
response carries `plan_server::startup::tunnel_hint(port, hostname)` for the dashboard's port. Add a test pinning the parser
against plan 2's exact strings and one against `print_startup`'s real output (spawn
`print_startup` through the test helper plan 2's startup test uses, if it exposes one).

## Run strip (`strip.rs`)

At `install()` start a thread: every 5 s, when `.loom/work` exists, `WorkDir::new(root)`,
`crate::commands::status::data::collect_status_data(&work_dir)` and
`crate::commands::status::render::attention_entries(&data.stages)` (check exact module paths with
`loom map --find-all collect_status_data`); build `RunStrip { plan: get_plan_source_path, daemon_running,
merged: data.merge.merged.len(), total: data.progress.total, attention: entries.len(),
dashboard_url /* tokenless */ }` and broadcast `PlanMessage::RunStrip` only when it differs from the last one
sent; with no `.loom/work`, broadcast one `RunStrip` with `plan: None` and stop sending until
it appears. A read error is logged once with `lifecycle::line` and the strip keeps its last
value. The thread exits when `ctx.is_stopping()`.

## Tests (`tests.rs`)

State table: each `.loom/work` row of section 11 on fixture workspaces in a `TempDir` git repo
(a `config.toml` with `[plan] source_path` pointing at `PLAN-`, `IN_PROGRESS-` and `DONE-`
files; the daemon-running case uses whatever test seam `DaemonServer::is_running` offers, else
a live lock record as `src/daemon/server/core.rs` tests do); changes split with a modified file
outside `doc/` (blocking), an untracked file outside `doc/` (not blocking), and 300 changed
paths under `doc/`; commit refuses a path outside the current list; start refuses each blocking
condition; `run_streamed` on a child printing 1 MiB to both streams; the dashboard parser.

Also, each its own test: the `Running` state (daemon running) gives `NoneRunning`; `plans`
order is metaplan heading order then unlisted plans by file name, with `preselected` the first
and the initialized plan when `Initialized`; `backends` omits `Native` when headless;
`clean_notice` true for `Done`; `start` with a fake `loom` (a `#!/bin/sh` script passed in place
of `current_exe` through a `loom_bin: &Path` parameter on the sequence function) runs `init
<plan> --backend tmux` then `run`, passes `--clean` only when asked, stops at a failing `init`
without running `run`, and keeps the failing step's output lines; a commit failure (hook exits
1) starts nothing after it; the dashboard route returns the URL with `?token=` to the holder and
without it to a non-holder, starts one child for two calls, and its shutdown hook kills the
child; `build_strip(work_dir: &WorkDir, dashboard_url: Option<&str>) -> anyhow::Result<RunStrip>`
(pure, `pub`, strips any `token` query parameter from `dashboard_url`; the frozen contract
`run_strip_carries_no_token` calls it) on a fixture `.loom/work` with two merged stages of three and one stage needing
attention gives `merged: 2, total: 3, attention: 1`.

Every file under 400 lines, functions under 50.

## Added (fix round)

- `start` and A3's abandon children run in their own process group
  (`std::os::unix::process::CommandExt::process_group(0)`), so the server's first Ctrl-C does
  not reach `loom init`, `loom run`, `loom stop` or `loom clean` mid-step; the shutdown drain
  waits for a running job to finish instead of killing it.
- Tests: a staged file outside `doc/` survives a commit staged and absent from `HEAD`; a ticked
  `doc/a*` commits only itself; `.section_03.md.tmp-41822` under `doc/loom/summaries/` is not in
  `doc_changes`; a viewer's `GET /api/run/state` and every broadcast `RunStrip` carry no
  `token=`; the holder's dashboard response does.
- `RunStateView.finish_abandon: Option<String>` (the stranded plan path, from A3's
  `stranded_plan(root, ctx.runtime())`) when the workspace is `Absent` and an `IN_PROGRESS-` plan exists.
