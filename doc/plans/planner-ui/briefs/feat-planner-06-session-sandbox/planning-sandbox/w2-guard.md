# W2: the PreToolUse guard and `loom plan hook`

Read `doc/plans/briefs/feat-planner-06-session-sandbox/common.md` first ("The guard" is your specification).
You run in wave 3, after W1 has returned: `SessionKind`, `SessionDir` and
`plan_session::sandbox::reads::read_denies` exist. Lanes: codex units C4-C9 (`codex-units.md` in
this directory) write `hook_cli.rs`, `commands/plan/hook.rs`, the CLI variant and dispatch arm,
and the two guard test files from steps 4-6 below, after you return; you own the guard core only.

## Files you own (write only these)

- `loom/src/plan_session/guard/mod.rs` (`GuardPolicy`, `Decision`, `load_policy`, `decide`,
  `render_deny`, `run_pre_tool_use`)
- `loom/src/plan_session/guard/paths.rs` (tool-input path extraction, lexical normalisation,
  read-deny matching)
(Steps 4-6 below are the specification codex units C4-C9 implement; the seed declared
`#[cfg(test)] mod tests; #[cfg(test)] mod tests_reads;` in `guard/mod.rs` and created both files;
keep those lines and do not write the files.)

## Steps

1. `guard/mod.rs`: types exactly as common.md. `load_policy` reads `guard.json` with
   `crate::fs::safe_read::read_to_string_bounded(session_dir, Path::new("guard.json"), 64 * 1024)`
   and rejects `schema != 1`. `run_pre_tool_use` reads stdin with
   `stdin.take(MAX_STDIN_BYTES + 1)` (`MAX_STDIN_BYTES = 1024 * 1024`, as
   `commands/hook/worker_brief.rs:21,97-105`), parses JSON, applies step 1 of common.md's
   decision list (mismatched `hook_event_name` -> `None`), loads the policy, calls `decide`, and
   maps `Deny` to `Some(render_deny(..))`. Every error path is `Some(render_deny(<why>))`. Wrap
   the body in `std::panic::catch_unwind(AssertUnwindSafe(..))`; a panic is a deny.
2. `decide`: steps 3-6 of common.md, in that order. Writes call
   `crate::planning::paths::{LockedSet::scan, classify_write, symlink_component}` fresh on every
   call. Use `std::fs::symlink_metadata` only for the "exists and is not a regular file" test on a
   path whose components `symlink_component` already cleared (metadata reads no content).
3. `guard/paths.rs`: `fn tool_path(tool: &str, input: &Value, cwd: Option<&Path>, root: &Path) -> Result<PathBuf, String>`
   and `fn normalize(path: &Path) -> Result<PathBuf, String>` (lexical; `..` above the root is an
   error). Read-deny matching: expand `~/` with `policy.home`, collapse a leading `//` to `/`, strip
   a trailing `/**` to mean "this directory and below"; `fn inside(path, base) -> bool` compares
   components (never string prefixes: `/a/bc` is not inside `/a/b`). Carve-outs (`allowRead`) win
   over denies. The strict-ancestor rule for `Grep`/`Glob` applies to home-side denies only (paths
   under `policy.home`, which includes `<home>/.loom/plan`), never to the project's token files.
4. `hook_cli.rs::run`: `match event { "PreToolUse" => write the answer if Some, _ => nothing }`,
   return 0. Write errors on stdout are ignored (the hook has nowhere to report them) but never
   panic.
5. CLI: in `types_ops.rs` add to `PlanCommands`, with a doc comment saying it is the hook entry
   Claude Code runs for planning sessions and is not user-facing:
   `#[command(hide = true)] Hook { event: String, #[arg(long)] session_dir: std::path::PathBuf }`.
   `commands/plan/hook.rs::execute(event: &str, session_dir: &Path) -> anyhow::Result<()>` calls
   `hook_cli::run(event, session_dir, std::io::stdin().lock(), std::io::stdout().lock())`.
   `dispatch.rs`: one arm in `dispatch_plan` (`loom/src/cli/dispatch.rs`, the fn at about line 104
   at `b0f91c00`; plans 1, 2 and 4 added arms since, read it first):
   `PlanCommands::Hook { event, session_dir } => plan::hook::execute(&event, &session_dir),`.
   If plan 2 made `PlanCommands` optional (`command: Option<PlanCommands>`), the arm still goes in
   the same match. Keep `dispatch.rs` under 400 lines; if the arm would cross it, report instead of
   restructuring.
6. Tests (`tests.rs`, `tests_reads.rs`; `#[cfg(test)] mod tests; #[cfg(test)] mod tests_reads;` in
   `guard/mod.rs`): build the same fixture tree as W1's tests (copy the helper into your test file;
   do not import W1's test module), write `guard.json` through
   `crate::plan_session::sandbox::write_session_sandbox` so the policy W1 writes is the one you
   read, then drive `run_pre_tool_use` with JSON payloads. One test per row of section 12's write
   table for each kind (authoring doc write allowed = `None`; locked plan, run review, archive,
   brief of a locked plan, `doc/loom/knowledge/x.md`, `doc/loom/summaries/x.md`, `loom/src/x.rs`,
   `.git/config`, `<rt>/lock`, `<rt>/sessions/s/answers/a.json` denied; `<rt>/drafts/x.md` allowed;
   summary kind denies `doc/product/x.md` and allows drafts; pressure allows the plan, its bundle,
   `planning-omissions.md` and denies another plan, `doc/product/x.md` and drafts), and one test
   each for: a `DONE-` plan, a `RETIRED-` plan, a plan under `archive/`, a not-yet-existing file
   under a locked plan's bundle, `<rt>/claims/x.json` (file tools may not write claims),
   `.claude/settings.json`, `.loom/work/x`, `README.md` at the root (all denied, every kind); a
   write under `<tmp>/claude-<uid>/` allowed for every kind and denied through a symlinked
   component there; pressure: a file-tool write to `done/complete` denied (the marker is
   commands-only), the bundle of a LOCKED plan denied, and the plan under test renamed to
   `IN_PROGRESS-` after the policy was written denied, a plan renamed
   to `IN_PROGRESS-` after the policy was written (denied), a symlinked component under `doc/`
   and under `<rt>/drafts` (denied), a FIFO or directory as the target (denied), a relative
   `file_path` resolved against `cwd`, `..` escapes, a payload over 1 MiB, invalid JSON, a missing
   `guard.json`, `hook_event_name: "PostToolUse"` (no output), and reads: `~/.ssh/id_rsa`,
   `<home>/.loom/plan/<key>/lock`, the project's `.loom/work/admin.token` denied; `<rt>/drafts/x`,
   `<root>/loom/src/lib.rs` allowed; `Grep` with `path` = `<home>` denied, `path` = `<root>` allowed.
   Reads through links and pseudo-paths (px-security-01), all denied: `Read`
   `/proc/self/cwd/.loom/work/admin.token`, `Read /dev/fd/0`; `doc/notes` a symlink to
   `<fake home>/.ssh` then `Read <root>/doc/notes/id_rsa` and `Grep path: <root>/doc/notes`; a
   project root reached through a symlinked ancestor (`<tmp>/link -> <tmp>/real`, policy root
   `<tmp>/real/proj`) then `Read <tmp>/link/proj/.loom/work/user.token`; `Read` of the runtime
   lock when the runtime lives under a redirected loom home. A plain `Read <root>/loom/src/lib.rs`
   reached through the same symlinked ancestor is still deferred.
   Assert the exact deny JSON shape once with `serde_json::from_str` (keys and values), and that
   no output ever contains `"allow"`.

## Proof

`cargo build` from `loom/` and a scratch unit test of your own (deleted before you return) that
`run_pre_tool_use` denies `{}` against a missing session dir; the CLI smoke runs after C4-C7. Report what changed and anything surprising (and `loom memory note` it).
