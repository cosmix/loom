# P3: loom's built-in default backend becomes tmux

LANE: this brief is now the SPECIFICATION for codex units T1-T4 in `doc/plans/briefs/plan-execution-handoff/pressure-pipeline/codex-units.md`. No Claude worker is spawned from it; the orchestrator reads T1's audit report.

Stage `pressure-pipeline` of `doc/plans/PLAN-plan-execution-handoff.md`. Tier: haiku
(`loom-software-engineer`, `model: haiku`). Never run git, never spawn subagents. One narrow
check at the end: `cargo test --lib init::` from `loom/`.

## Why

`doc/product/section_11.md` lines 295-300: the built-in default backend moves from native to
tmux, so `loom init` and `loom run` started from a terminal default to the backend the app
uses; `loom init`'s prompt shows tmux as the default. `terminal.backend` in
`~/.loom/config.toml` and the workspace `[terminal]` section still override it.

## Files you own (relative to `loom/`)

- `src/models/session/types.rs`
- `src/commands/init/backend.rs`
- `src/commands/init/backend_tests.rs`
- `src/user_config/tests.rs` (only the `terminal_backend()` default assertion at line 94)
- `src/models/session/methods.rs` (only if the audit below requires it)
- `../web/src/api/fixtures/config.json`
- `../web/src/test/settings-kit.tsx`
- `../README.md`
- `../skills/loom-usage/SKILL.md` (only if it states the default; `rg -n -i 'native' ../skills/loom-usage/SKILL.md`)

## Edits

1. `src/models/session/types.rs:89-99` `SessionBackendKind`: move `#[default]` from `Native`
   to `Tmux`; doc comments: `Native` "Native terminal emulator window.", `Tmux` "Headless tmux
   server, one per session (the built-in default)."
2. `src/commands/init/backend.rs:98-103` `backend_default_label`: `None => "tmux, built-in default"`.
   The prompt text `Terminal backend for sessions [native/tmux] (...)` stays; its default
   clause comes from this label. Update `backend_tests.rs:63` to the new label, and rename
   `init_backend_effective_backend_defaults_to_native_without_user_config` (`:90`) to
   `..._defaults_to_tmux_without_user_config` asserting `SessionBackendKind::Tmux`. These two edits
   and the `user_config/tests.rs:94` one are the stage's declared test-integrity events; change no
   other assertion.
3. `src/user_config/tests.rs:94`: `SessionBackendKind::Tmux`.
4. AUDIT every other reader of the default, and change only what reads it as "native":
   `rg -n 'SessionBackendKind::default\(\)|unwrap_or_default\(\)|#\[serde\(default\)\]' src/models/session src/user_config src/fs/work_dir src/orchestrator/terminal`.
   Known sites: `Session::new` (`src/models/session/methods.rs:27`) sets
   `backend: SessionBackendKind::default()` before spawn sets the lane actually used
   (`types.rs:160-164` doc: "Set at spawn time to the lane actually used"); confirm with
   `rg -n '\.backend = ' src/orchestrator` that every spawn path assigns it before the session
   file is written, and report the sites. `Session.backend` and `TerminalConfig.backend` carry
   `#[serde(default)]`: a session file without the field now reads as tmux; loom is unreleased
   (project CLAUDE.md: no migration code), so make no compatibility shim; report it.
5. Dashboard fixture: `web/src/api/fixtures/config.json` lines 49-62 hold the
   `terminal.backend` entry with `"default": "native"` and `"value": "native"` for user,
   project and effective; change those four values to `"tmux"`. The Rust test
   `config_api::tests::the_config_fixture_matches_a_real_payload` pins this file; run
   `cargo test --lib config_api::` once if your one check allows only one, prefer this one
   over `init::` and say so. `web/src/test/settings-kit.tsx:53-58`: the same four values.
6. `README.md` line 1197 (`backend = "tmux"   # or "native" (default)`) becomes
   `backend = "native"   # or "tmux" (default)`; line 1207 ("with no TTY it defaults to
   `native`") becomes "with no TTY it uses `terminal.backend` from `~/.loom/config.toml`, else
   `tmux`". Add one sentence in that section: "The built-in default is tmux; set
   `terminal.backend = \"native\"` to keep native terminal windows." Fix any other README line
   that calls native the default (`rg -n -i 'native' ../README.md`).

## Done when

Your one check passes. Report the audit's findings (each site, and whether it assigns the lane
before persisting).
