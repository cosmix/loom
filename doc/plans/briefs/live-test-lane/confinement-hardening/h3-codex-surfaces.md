# confinement-hardening H3: deny the codex lane's trust surfaces

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first. You run in wave 2.
The frozen contract is `src/sandbox/control_surfaces/codex_trust_tests.rs`; never edit it. The
stage description's PUBLIC SURFACE is authoritative. Effort: security boundary; read every
caller before changing a list.

## The problem: one stage contaminates the next

A capsule whose stage licenses codex adds `codex.rs::CODEX_SANDBOX_WRITE_PATHS` (`~/.codex` and
`~/.claude/plugins/data/codex-openai-codex`) to `allowWrite`. Inside it
`sandbox/control_surfaces.rs::HOME_SURFACES` already denies `~/.codex/hooks/**`, `hooks.json`,
`config.toml`, `AGENTS.md`, `loom-skill-catalog/**`, `prompts/**` and each skill directory loom
installs (`codex_skill_surfaces`). Still writable to a stage, and loaded by every later
loom-driven codex run (another stage's codex lane on the same machine): `~/.codex/rules/`
(Codex exec-policy: a `prefix_rule(..., decision="allow")` runs the matching command outside
codex's sandbox without asking, https://developers.openai.com/codex/rules), new skill
directories under `~/.codex/skills/`, `~/.codex/plugins/`, `~/.codex/packages/` and
`~/.codex/vendor_imports/`. A stage that plants a rule, a skill or a plugin there changes what
the next stage's codex does.

## Why the grant itself stays `~/.codex`

Codex keeps its SQLite-backed state in `CODEX_SQLITE_HOME`, which defaults to `CODEX_HOME`
(https://learn.chatgpt.com/docs/config-file/environment-variables; the database file
names carry version numbers), and its history, session index, caches and `auth.json` (token
refresh) at the root of `CODEX_HOME` too. SQLite creates its `-wal`/`-shm` sidecars in the
database's directory and atomic replacement renames within it; a file-level grant on Linux is a
bind mount, and a rename onto a mount point fails with EBUSY. So the directory must stay
writable, and the lever is a deny under it. `auth.json` stays readable and writable to a
codex-licensed stage (token refresh needs it); every other stage keeps the existing read deny
(`CODEX_AUTH_DENY_READ_PATH`).

## Edits

1. `src/sandbox/control_surfaces.rs` `HOME_SURFACES`: add `.codex/rules/**`, `.codex/skills/**`,
   `.codex/plugins/**`, `.codex/packages/**`, `.codex/vendor_imports/**` (keep the existing
   `.codex/*` entries). With `skills/**` denied whole, `codex_skill_surfaces` and its use in
   `home_surfaces` are subsumed: delete them (check `rg -n codex_skill_dir_names src` first:
   `assets/install` and `orchestrator/terminal/native/tests_capsule.rs` keep using the helper,
   which stays). Rewrite the doc comments above `HOME_SURFACES` and `codex_skill_surfaces` to the
   current truth, including the reason `.codex/skills` is now denied whole (a new skill
   directory under it is loaded by every later codex run) and the cost (codex's own
   `skills/.system` write is refused inside a stage; the plan records it as an open decision).
2. Emission needs no new code: `session_denies` emits each surface through `SessionDenies::home`
   as a `denyWrite` path `~/<path>` plus an `Edit(~/<path>/**)` rule, the same route
   `~/.codex/hooks` uses today, and every capsule kind (stage, merge, knowledge, planning)
   carries it. Confirm by reading `session_denies` and the planning builder's use of
   `home_surfaces` (`rg -n home_surfaces src`). Linux and macOS hold such a deny whether the path
   exists or not: on Linux the sandbox binds `/dev/null` over a missing denied path, which blocks
   creating it (knowledge: "A Denied Missing Path Shows Up in the Session's git status"); on
   macOS Seatbelt matches the path. Add one sentence saying so in the `HOME_SURFACES` doc.
3. `src/codex.rs` `CODEX_SANDBOX_WRITE_PATHS` doc: say the root grant is required for the
   SQLite sidecars and atomic replacement, and that `HOME_SURFACES` denies every trust surface
   under it. No change to the constant.
4. `src/sandbox/control_surfaces/tests_home_surfaces.rs`:
   `the_paths_codex_writes_at_runtime_stay_open` asserts `~/.codex/skills`,
   `~/.codex/skills/.system` and `~/.codex/rules` stay open: remove those three from its list
   (sessions, log, history.jsonl and auth.json stay) and rename nothing else;
   `the_instruction_files_of_both_home_roots_are_denied` replaces the per-skill loop with one
   `assert_denied(&denies, "~/.codex/skills", "Edit(~/.codex/skills/**)")`. These are assertion
   edits; the stage disputes their `TI-edit` events. Fix any other pinned count the compiler or
   `cargo test --lib sandbox::control_surfaces:: --no-run` reveals (`tests.rs`,
   `tests_session_denies.rs`, `orchestrator/terminal/native/tests_capsule*.rs`): list each in
   your report.

## Traps

- Never deny `~/.codex` itself or `auth.json`, `sessions`, `log`, `history.jsonl`: the codex
  lane stops working.
- `sandbox/settings/policy.rs` and its tests are H2b's; if a policy test pins the codex grant,
  report it instead of editing it.
- `cargo fmt`; your one check: `cargo test --lib sandbox::control_surfaces:: --no-run`.

## Report

Files changed, every assertion edited (file, test), and anything in the planning builder that
does not take `home_surfaces`.
