# W1: kinds, session directory, settings builder

Read `doc/plans/briefs/feat-planner-06-session-sandbox/common.md` first; it is the API you implement and wins
over this file. You run in wave 2, alone, after codex unit C1 wrote `plan_session/kind.rs` (`SessionKind`,
`SessionDir`, exactly as common.md); W2, W3 and the codex test units build on your files after
you return. Lanes: codex units C2 and C3 (`codex-units.md` in this directory) write the snapshot
and per-kind tests from step 7 below; you write no test file, but keep step 7 true of your code.

## Files you own (write only these)

- `loom/src/plan_session/sandbox/mod.rs` (the module root: `SandboxInputs`, `HookSpec`,
  `SandboxFiles`, constants, `build_settings`, `write_session_sandbox`, `ensure_runtime_grants`,
  `hook_command`, `assert_planning_settings` or its re-export)
- `loom/src/plan_session/sandbox/filesystem.rs` (layer 1 lists per kind)
- `loom/src/plan_session/sandbox/permissions.rs` (layer 2 rules per kind, hooks object)
- `loom/src/plan_session/sandbox/reads.rs` (`pub fn read_denies(home: &Path, root: &Path, runtime_dir: &Path) -> ReadDenies`,
  `pub struct ReadDenies { pub denied: Vec<String>, pub carve_outs: Vec<String> }` as the strings
  the settings emit; W2's guard calls it, so it is `pub`)
- `loom/src/plan_session/sandbox/assert.rs` (`assert_planning_settings`)
- Three visibility widenings, nothing else in these files, no line-count change (both
  `sandbox/settings.rs`, 438 lines, and the ledger row for it must stay exact):
  `loom/src/sandbox/mod.rs:12` `mod settings;` -> `pub(crate) mod settings;`,
  `loom/src/sandbox/settings.rs:16` `mod policy;` -> `pub(crate) mod policy;`,
  `loom/src/sandbox/settings/policy.rs:17` `pub(super) fn mandatory_deny_read` -> `pub(crate) fn`.
- Three more, for the tool denies, same rule (visibility only, no line-count change;
  `native/mod.rs` is 397 lines): `loom/src/orchestrator/terminal/native/mod.rs:13` `mod session_settings;`
  -> `pub(crate) mod session_settings;`, `native/session_settings.rs:12` `mod contents;` ->
  `pub(crate) mod contents;`, `native/session_settings/contents.rs:35-36` `const SESSION_TOOL_DENIES`
  and `const WORKFLOW_TOOL` -> `pub(crate) const`.

`loom/src/lib.rs` and `loom/src/plan_session/mod.rs` were seeded by the orchestrator; do not edit
them. Do not touch `guard/`, `hook_cli.rs`, `commands/`, `cli/`.

## Steps

1. `kind.rs` exists (C1). A `Pressure { plan }` whose `pressure_bundle()` is None makes
   `build_settings` fail with an error naming the path (a pressure session needs a plan).
2. `reads.rs`: the `denyRead`/`allowRead` lists of common.md "Layer 1". Use
   `crate::sandbox::settings::policy::mandatory_deny_read(false)` (reachable after the three
   widenings above; a `dead_code` or `unreachable_pub` lint they trigger is fixed by the use, never
   by an `allow`). Entries keep the `~/` form the constants use (Claude Code
   expands `~/` in sandbox lists); `<home>/.loom/plan` is written absolute.
3. `filesystem.rs`: one function per kind returning `(deny_write: Vec<String>, allow_write: Vec<String>)`.
   List directories with `crate::fs::safe_read::list_dir_no_follow` over a dirfd from
   `crate::fs::safe_fs::safe_open_dirfd(root)`; never `std::fs::read_dir`. Collapse rule: sort the
   candidate paths, and drop any path that has an already-kept path as a proper ancestor. A
   missing `doc/plans/archive` or bundle directory is simply not listed (every entry must exist).
   Sort the final lists for a stable snapshot.
4. `permissions.rs`: the allow/ask/deny arrays per kind and the `hooks` object. Rule strings via
   one helper `fn edit_rule(abs: &Path, suffix: &str) -> String` producing `Edit(/` + abs + suffix
   + `)`, so `//` comes from the absolute path's own leading `/`. The authoring per-directory
   allow enumerates existing directories directly under `doc/` (no-follow listing; a symlinked
   directory is skipped, matching discovery) except `loom`, `product`, `plans`.
5. `mod.rs`: `hook_command` joins `shell_escape::escape(loom_bin)`, `plan hook`, the event, and
   `--session-dir <escaped dir>` (see `orchestrator/terminal/tmux/mod.rs:187-190` for the
   `escape(..).into_owned()` idiom). `build_settings` assembles the common object, then layer 1,
   layer 2 and hooks; PreToolUse first, then `extra_hooks` in order; an extra `PreToolUse` is an
   error. `ensure_runtime_grants` creates `drafts/`, `claims/` (0700, `safe_create_dir_all`) and
   an empty `summaries.lock` (0600, created only when absent: open with `O_CREAT` through
   `fs::safe_fs::safe_append(runtime.dir, "summaries.lock", b"")` or an equivalent no-follow
   create; never truncate an existing lock file). `write_session_sandbox`: ensure grants,
   `LockedSet::scan(project_root)`, build, `assert_planning_settings`, write `settings.json` and
   `guard.json` (a `crate::plan_session::guard::GuardPolicy` is W2's type: to avoid a compile
   dependency on W2, write guard.json from a `serde_json::json!` object with exactly the
   `GuardPolicy` fields of common.md, `schema: 1`, and leave a `// GuardPolicy` comment; W2's
   contract test round-trips it) with mode 0600 through `safe_fs::safe_open_dirfd(session_dir)` +
   `safe_write_with_mode_in_workdir`, the pattern of `write_capsule_file`
   (`orchestrator/terminal/native/session_settings.rs:218-257`, which itself is tied to
   `capsules/` and so is not called). For pressure, create `done/` (0700) first. Return absolute
   paths.
6. `assert.rs`: common.md's list, each refusal its own message. Unit-test every refusal.
7. Tests, written by codex units C2 (`tests.rs`) and C3 (`tests_kinds.rs`) from this text; the seed
   declared `#[cfg(test)] mod tests; #[cfg(test)] mod tests_kinds;` in `sandbox/mod.rs` and created
   both files; keep those lines and do not write the files: a fixture root built in a `tempfile::TempDir` with `.git/`,
   `loom/`, `README.md`, `.claude/`, `.loom/work/` (a real dir), `doc/product/`, `doc/plans/`
   holding `PLAN-a.md`, `IN_PROGRESS-PLAN-b.md`, `REVIEW-PLAN-b.md`, `archive/DONE-PLAN-c.md`,
   `briefs/b/x.md`, `briefs/a/y.md`, `doc/notes/`, `doc/loom/knowledge/`, `doc/loom/summaries/`;
   a runtime dir from `planning::runtime::runtime_paths_under(<tmp>/loomhome, root)`. Snapshot per
   kind: compare the whole JSON against an expected `json!` with the temp paths substituted
   (no snapshot crate). Plus targeted tests: the deny list of each kind holds the four `SESSION_TOOL_DENIES` and
   `Workflow`, authoring and summary also `SendMessage` and `ListAgents`, pressure neither of
   those two; with the runtime from `runtime_paths_under(<tmp>/elsewhere-loomhome, root)` (a
   `LOOM_HOME`-style redirect outside `<home>/.loom`) `denyRead` holds that runtime's canonical
   `planning_root` AND `<home>/.loom/plan`; no entry under another denied directory (check every
   pair), every `Edit(` rule starts with `Edit(//`, no `Write(` rule anywhere, no `Read(` in
   `deny`, the summary kind has no `WebSearch` allow and denies it, pressure's `Bash(touch ...)`
   equals `format!("Bash(touch {})", marker.display())`, a top-level symlink is denied by its own
   path, `ensure_runtime_grants` twice leaves an existing `summaries.lock` byte-identical,
   `write_session_sandbox` files are mode 0600 and refuse a `settings.json` planted as a symlink;
   no `allow` rule of any kind matches a path under `doc/loom/` (check each `Edit(` pattern against
   `<root>/doc/loom/knowledge/x.md` and `<root>/doc/loom/summaries/x.md` with glob semantics:
   `**` any depth, `*` one component); the hooks object holds `PreToolUse` with exactly
   `PRE_TOOL_USE_MATCHER` and timeout 10, then each extra `HookSpec` in order with its matcher and
   timeout; an extra `PreToolUse` is refused; `assert_planning_settings` refuses each key of
   common.md's list on its own (one test per key).

## Proof

`cargo build`, a short smoke test of your own in a scratch `#[test]` you delete before returning, and `cargo clippy --all-targets -- -D warnings` from
`loom/`. Report what you changed, any signature that had to differ from common.md, and anything
surprising (record it with `loom memory note` too).
