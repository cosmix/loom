# PLAN: `loom config` command for global user settings

## Context

Loom currently has no per-user configuration. Every behavior is either a project-level setting in `.work/config.toml` (e.g., `[remote_control]`) or a hardcoded default (e.g., `loom knowledge bootstrap` defaults to `sonnet`, `loom knowledge gc` defaults to `opus`). Users have no way to set their own defaults that span all projects.

This plan introduces `~/.loom/config.toml` as a per-user global config and a `loom config` command to manage it. Two modes:

1. **Wizard** (first-time, no config exists) — sequential `dialoguer` prompts.
2. **TUI** (config exists) — ratatui-based settings menu for quick edits.

Three settings ship in v1 (flat top-level TOML keys, no sections):

| Setting              | Type            | Default  |
| -------------------- | --------------- | -------- |
| `remote_control_mode`        | `auto` / `off`        | `auto`   |
| `knowledge_bootstrap_model`  | `sonnet` / `opus` | `sonnet` |
| `knowledge_gc_model`         | `sonnet` / `opus` | `sonnet` |

Note: `knowledge_gc_model` is the runtime default for `loom knowledge gc` only. It does NOT affect the `stage_type: knowledge-distill` stage defaults — those live in `models/stage/types.rs::StageType::default_model()` and are governed by the plan author.

**Precedence rules:**

- Remote control: per-project `.work/config.toml [remote_control]` overrides global; global overrides hardcoded default.
- Knowledge models: explicit CLI `--model` flag overrides global; global overrides hardcoded fallback.

**Init-time contract change (required for precedence to work):** `loom init` currently writes a default `[remote_control]` section to every new project (`commands/init/plan_setup.rs:164-170`). With the default always present, the "fall back to global when section missing" path never fires. This plan removes the init-time default write so absence genuinely means "inherit global". Users who want a per-project override write the section explicitly (or via a future `loom config --project ...`, out of v1 scope).

**First-run UX:** When `~/.loom/config.toml` is missing AND any loom command is invoked from a terminal (both stdin AND stdout are TTYs), the wizard auto-fires before the command continues. Non-TTY invocations (CI, scripts, hooks, `cmd | tee`, `cmd > file.json`) silently use defaults. Skipped for `loom config` (handles its own dispatch), `loom completions` / `loom complete` (shell-completion infrastructure must never block), and when `LOOM_NO_WIZARD=1` is set (escape hatch for tests). Wizard prompts and copy go to stderr; only the final "saved" line goes to stdout.

**Read sites:** Each of the three settings has a small, known set of consumers; the plan touches every one:

| Setting                          | Consumer(s)                                                                | Owned by         |
| -------------------------------- | -------------------------------------------------------------------------- | ---------------- |
| `remote_control_mode`            | `remote_control.rs::resolve()` AND `remote_control.rs::run_startup_preflight()` | Subagent B       |
| `knowledge_bootstrap_model`      | `commands/knowledge/bootstrap.rs` (via new pure helper `resolve_bootstrap_model`) | Subagent C       |
| `knowledge_gc_model`             | `commands/knowledge/gc.rs` (via new pure helper `resolve_gc_model`)        | Subagent C       |

Each call site uses lazy `global_config::load()` — no eager cache, no global state.

## Execution Graph

```mermaid
graph LR
    knowledge-bootstrap --> foundation
    foundation --> implement
    implement --> integration-verify
    integration-verify --> knowledge-distill
```

## Critical Files

**Created:**

- `loom/src/global_config.rs` — `GlobalConfig` struct, `~/.loom/config.toml` read/write, env-overridable path
- `loom/src/commands/config/mod.rs` — `Config` command entry + `show`/`set` subcommands
- `loom/src/commands/config/wizard.rs` — `dialoguer`-based first-time wizard
- `loom/src/commands/config/tui.rs` — ratatui settings menu

**Modified:**

- `loom/Cargo.toml` — add `dialoguer = "0.11"` (added in implement stage by Subagent A — foundation does not need it)
- `loom/src/lib.rs` — `pub mod global_config;`
- `loom/src/cli/types.rs` — add `Commands::Config { command: Option<ConfigCommands> }` + `ConfigCommands` enum
- `loom/src/cli/types_memory.rs:70-80` — update `loom knowledge gc --model` help text (default is now `sonnet`, not `opus`)
- `loom/src/cli/dispatch.rs` — dispatch arm + first-run prelude
- `loom/src/commands/mod.rs` — `pub mod config;`
- `loom/src/completions/dynamic/commands.rs` — add `config` to `TOP_LEVEL_COMMANDS`, add `config` → `["show", "set"]` to `complete_subcommands`, add `config` to `has_subcommands()`
- `loom/src/commands/init/plan_setup.rs:164-170` — remove the init-time default `[remote_control]` section write (absence now means inherit-global)
- `loom/src/remote_control.rs` — both `resolve()` AND `run_startup_preflight()` route through new `effective_remote_control_mode()`
- `loom/src/commands/knowledge/bootstrap.rs` — replace hardcoded `"sonnet"` default with pure helper `resolve_bootstrap_model(cli)`
- `loom/src/commands/knowledge/gc.rs` — replace hardcoded `"opus"` default with pure helper `resolve_gc_model(cli)`
- `loom/tests/integration/helpers.rs` (or equivalent test spawn helper) — ensure spawned `loom` binaries either set `LOOM_NO_WIZARD=1` env or pipe `Stdio::null()` for stdin, so dev `cargo test` from a TTY does not fire the wizard mid-test

## Existing Patterns to Reuse

- **CLI registration pattern** — `loom/src/cli/types.rs` + `loom/src/cli/dispatch.rs` (see `Repair` / `Plan`)
- **Config TOML I/O** — mirror the `read_section<T>` / `write_section<T>` pattern at `loom/src/fs/work_dir.rs:385-431` (uses `toml_edit` for round-trip writes and `toml::Value` for typed reads)
- **`dirs` crate** — already used at `loom/src/remote_control.rs:156` (`dirs::home_dir()`)
- **`RemoteControlConfig` shape** — `loom/src/remote_control.rs:46-62`, `read_remote_control_config()` at `fs/work_dir.rs:450`
- **Ratatui event loop** — `loom/src/commands/status/ui/tui/event_handler.rs` (crossterm key handling)
- **CLI arg parsing for subcommands** — `loom/src/cli/types_stage.rs` (nested `Subcommand` enum pattern)

## Design Decisions

- **Path:** `~/.loom/config.toml` (NOT `~/.config/loom/`). Override via `LOOM_GLOBAL_CONFIG_DIR` env for test isolation.
- **Schema:** Single top-level TOML file with named sections. No backwards compatibility needed (unreleased project).
- **Wizard vs TUI dispatch:** `loom config` (no args) checks for file existence: missing → wizard; present → TUI.
- **Non-interactive subcommands:** `loom config show` and `loom config set <key> <value>` provide scriptable access for tests and CI; the same code paths back the interactive UIs.
- **No new top-level module needed beyond `global_config.rs`** — small enough that one file with embedded tests is appropriate.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  sandbox:
    enabled: true
    auto_allow: true
    excluded_commands:
      - "loom"
    filesystem:
      deny_read:
        - "~/.ssh/**"
        - "~/.aws/**"
        - "~/.config/gcloud/**"
        - "~/.gnupg/**"
      deny_write:
        - ".work/stages/**"
    network:
      allowed_domains: []
      allow_local_binding: false
      allow_unix_sockets: []

  stages:
    - id: knowledge-bootstrap
      name: "Bootstrap Knowledge"
      stage_type: knowledge
      model: "sonnet"
      reasoning_effort: "high"
      execution_mode: team
      working_dir: "."
      dependencies: []
      description: |
        Verify knowledge coverage is sufficient for this plan and record any
        new patterns specific to the global-config module.

        Use parallel subagents and skills to maximize performance.

        Step 0 - CHECK COVERAGE:
          Run: loom knowledge check
          Expected: knowledge files are already populated (architecture, entry-points,
          patterns, conventions, mistakes, stack, concerns all exist).
          If coverage < 50% OR architecture shows INCOMPLETE, run: loom map --deep

        Step 1 - FOCUSED EXPLORATION (agent team, 3 researchers):
          Researcher 1 - CLI Registration & Dispatch:
            Confirm the 3-file pattern in loom/src/cli/types.rs, cli/dispatch.rs,
            commands/mod.rs. Verify recent example like Repair or Plan command.
            Note any deviations or new patterns introduced since knowledge was written.

          Researcher 2 - TOML Config I/O:
            Read loom/src/fs/work_dir.rs:329-457 (the centralized config API).
            Confirm read_section/write_section pattern and the toml vs toml_edit
            split. Identify whether the same pattern can be lifted into a
            per-user config helper.

          Researcher 3 - Interactive UI in the codebase:
            Confirm no existing dialoguer usage. Confirm ratatui + crossterm are
            available (versions 0.30 / 0.29). Document the event loop pattern
            at loom/src/commands/status/ui/tui/event_handler.rs.

        Step 2 - KNOWLEDGE UPDATES:
          Record findings using loom knowledge update:
            - patterns.md: Add a section "Global User Config (~/.loom/config.toml)"
              describing the new module location and read/write API mirroring
              fs/work_dir.rs.
            - entry-points.md: Add commands/config/ to the command dispatch table
              once it lands.
            - stack.md: Note dialoguer as a new dependency.

        Step 3 - MEMORY:
          loom memory note "..." for any non-obvious discoveries.
          loom memory list before completing to verify insights captured.

        ⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
      acceptance:
        - "loom knowledge check --min-coverage 50"
      artifacts:
        - "doc/loom/knowledge/architecture.md"
        - "doc/loom/knowledge/entry-points.md"
        - "doc/loom/knowledge/patterns.md"
      files:
        - "doc/loom/knowledge/**"

    - id: foundation
      name: "Global Config Foundation"
      stage_type: standard
      model: "sonnet"
      reasoning_effort: "high"
      working_dir: "loom"
      dependencies: ["knowledge-bootstrap"]
      description: |
        Create the loom/src/global_config.rs module: the GlobalConfig schema,
        the ~/.loom/config.toml path resolver, and the read/write API.

        The dialoguer crate dep is added in the implement stage (Subagent A,
        which owns the wizard) — NOT here. Foundation only adds storage.

        Use parallel subagents and skills to maximize performance.

        FILES TO CREATE/MODIFY (this stage owns all of these):
          - src/global_config.rs (new — path relative to working_dir=loom)
          - src/lib.rs (add: pub mod global_config;)

        Required dependencies already in loom/Cargo.toml: anyhow, serde,
        serde_json (no), toml, toml_edit, dirs, tempfile (test), serial_test (test).
        If dirs is missing, add it: cargo add dirs

        ──────────────────────────────────────────────────────────────
        1. CREATE src/global_config.rs WITH THIS STRUCTURE:

           Module-level doc comment explaining ~/.loom/config.toml semantics
           and LOOM_GLOBAL_CONFIG_DIR env override.

           Imports:
             use anyhow::{Context, Result};
             use serde::{Deserialize, Serialize};
             use std::fs;
             use std::path::PathBuf;
             use toml_edit::DocumentMut;

           Public types — reuse the EXISTING RemoteControlMode enum from
           crate::remote_control instead of duplicating it. For models, use a
           dedicated enum with explicit #[default] markers and an as_str()
           accessor:

             use crate::remote_control::RemoteControlMode;

             #[derive(Debug, Clone, Copy, PartialEq, Eq, Default,
                      Serialize, Deserialize)]
             #[serde(rename_all = "kebab-case")]
             pub enum KnowledgeModel {
                 #[default]
                 Sonnet,
                 #[serde(rename = "opus")]
                 OpusOneM,
             }

             impl KnowledgeModel {
                 pub fn as_str(&self) -> &'static str {
                     match self {
                         Self::Sonnet => "sonnet",
                         Self::OpusOneM => "opus",
                     }
                 }
                 pub fn parse(s: &str) -> Result<Self> {
                     match s {
                         "sonnet" => Ok(Self::Sonnet),
                         "opus" => Ok(Self::OpusOneM),
                         other => anyhow::bail!(
                             "invalid model: {} (expected sonnet|opus)", other
                         ),
                     }
                 }
             }

             #[derive(Debug, Clone, PartialEq, Eq, Default,
                      Serialize, Deserialize)]
             #[serde(default, rename_all = "snake_case")]
             pub struct GlobalConfig {
                 pub remote_control_mode: RemoteControlMode,
                 pub knowledge_bootstrap_model: KnowledgeModel,
                 pub knowledge_gc_model: KnowledgeModel,
             }

           NOTE: Field name is `knowledge_gc_model` (not `knowledge_distill_model`)
           because the setting controls `loom knowledge gc`, not the
           `stage_type: knowledge-distill` plan defaults.

           RemoteControlMode already derives Default = Auto (see
           remote_control.rs:46-54) and serializes as kebab-case
           ("auto"/"off"), so reusing it is direct.

           Path resolution (private):
             fn config_dir() -> Result<PathBuf>:
               if env var LOOM_GLOBAL_CONFIG_DIR is set, return that as PathBuf.
               else return dirs::home_dir()?.join(".loom").
               Error context: "could not resolve home directory".

             fn config_path() -> Result<PathBuf>:
               config_dir().map(|d| d.join("config.toml"))

           Public API:
             pub fn exists() -> Result<bool>:
               returns Ok(config_path()?.exists())

             pub fn load() -> Result<GlobalConfig>:
               if !exists()? { return Ok(GlobalConfig::default()); }
               read file, parse as toml, try_into GlobalConfig.
               Use .context("Failed to read ~/.loom/config.toml") chains.

             pub fn save(config: &GlobalConfig) -> Result<()>:
               Compute path. Create parent dir (fs::create_dir_all on config_dir).
               MUST preserve unknown top-level keys and comments. Algorithm:
                 1. Read existing DocumentMut (or DocumentMut::new() if missing).
                 2. Render GlobalConfig to toml::Value, then to_string_pretty,
                    then parse to a temporary DocumentMut.
                 3. For each KNOWN key in the temporary doc, INSERT (or replace)
                    that single key in the existing doc. DO NOT overwrite the
                    whole document — that would erase unknown keys/comments.
                 4. Write the existing doc back to disk.
               This mirrors the pattern in fs/work_dir.rs::write_section.
               Use .context() chains for errors.

             pub fn path() -> Result<PathBuf>:
               returns config_path() for use by `loom config show`.

             /// True when the wizard should auto-fire on first run.
             /// (Caller is responsible for the TTY/skip-list checks.)
             pub fn first_run_required() -> Result<bool>:
               Ok(!exists()?)

           Inline tests (#[cfg(test)] mod tests):
             - test_default_is_auto_and_sonnet: Default::default() returns
               RemoteControlMode::Auto, KnowledgeModel::Sonnet x2.
             - test_load_returns_default_when_file_missing: with
               LOOM_GLOBAL_CONFIG_DIR pointing at empty TempDir, load() returns default.
             - test_save_then_load_roundtrip: save a non-default config to TempDir,
               load() returns the same values.
             - test_opus_serializes_as_bracket_1m: a config with OpusOneM
               serializes to TOML containing 'opus'.
             - test_opus_deserializes_from_bracket_1m: a TOML file containing
               `knowledge_gc_model = "opus"` deserializes to OpusOneM.
             - test_save_preserves_unknown_keys: pre-create config.toml with an
               unknown top-level key (e.g. `future_setting = "x"`) and a
               comment line, then save() a config and verify the unknown key
               AND comment survive in the file.
             - test_save_preserves_unknown_keys_and_overwrites_known: same as
               above but ALSO confirm that known keys (remote_control_mode)
               are updated to the new value.

           ENV-VAR TEST GUARD (REQUIRED for panic safety):
             Add this helper at the top of the test module (or extract to a
             tiny shared helper if a similar pattern already exists):

               struct EnvGuard { key: &'static str, prev: Option<String> }
               impl EnvGuard {
                   fn set(key: &'static str, value: &Path) -> Self {
                       let prev = std::env::var(key).ok();
                       std::env::set_var(key, value);
                       Self { key, prev }
                   }
               }
               impl Drop for EnvGuard {
                   fn drop(&mut self) {
                       match &self.prev {
                           Some(v) => std::env::set_var(self.key, v),
                           None => std::env::remove_var(self.key),
                       }
                   }
               }

             Every env-touching test MUST use EnvGuard AND `#[serial_test::serial]`
             — the guard restores process-global state even on test panic; the
             serial attribute prevents test interleaving.

        ──────────────────────────────────────────────────────────────
        3. WIRE INTO loom/src/lib.rs:
           Add `pub mod global_config;` alongside other top-level pub mod
           declarations. Place alphabetically or near remote_control.

        ──────────────────────────────────────────────────────────────
        BEFORE COMPLETING:
          - cargo build (from loom/) passes
          - cargo test global_config (from loom/) passes
          - loom memory note any decisions or gotchas
      acceptance:
        - "cargo build"
        - "cargo test --lib global_config::"
        - 'rg -qF "pub mod global_config" src/lib.rs'
      artifacts:
        - "src/global_config.rs"
      wiring:
        - source: "src/lib.rs"
          pattern: "pub mod global_config"
          description: "global_config module is exported from the crate root"
        - source: "src/global_config.rs"
          pattern: "pub struct GlobalConfig"
          description: "GlobalConfig struct is defined"
        - source: "src/global_config.rs"
          pattern: "pub fn load"
          description: "load() entry point exists"
        - source: "src/global_config.rs"
          pattern: "pub fn save"
          description: "save() entry point exists"
        - source: "src/global_config.rs"
          pattern: "pub fn first_run_required"
          description: "first_run_required() exposed for the CLI prelude"
        - source: "src/global_config.rs"
          pattern: "knowledge_gc_model"
          description: "field is knowledge_gc_model (not knowledge_distill_model)"
        - source: "src/global_config.rs"
          pattern: "use crate::remote_control::RemoteControlMode"
          description: "reuses existing RemoteControlMode enum rather than duplicating"

    - id: implement
      name: "Config Command + Integrations"
      stage_type: standard
      model: "sonnet"
      reasoning_effort: "high"
      working_dir: "loom"
      dependencies: ["foundation"]
      description: |
        Implement the `loom config` CLI command (wizard + TUI + show + set
        subcommands) AND integrate the global config into the existing
        remote_control and knowledge command defaults.

        Use parallel subagents and skills to maximize performance.

        Each subagent below has EXCLUSIVE write access to its assigned files.
        NO file overlap between subagents is confirmed.

        SUBAGENT FILE ASSIGNMENTS:

          Subagent A — CLI Command (loom-software-engineer):
            Files Owned:
              - Cargo.toml (add dialoguer = "0.11")
              - src/commands/config/mod.rs (new)
              - src/commands/config/wizard.rs (new)
              - src/commands/config/tui.rs (new)
              - src/cli/types.rs (add Config variant + ConfigCommands enum)
              - src/cli/dispatch.rs (add Config dispatch arm + first-run prelude)
              - src/commands/mod.rs (add: pub mod config;)
              - src/completions/dynamic/commands.rs (add `config` + subcommands)
              - tests/integration/helpers.rs (or wherever test helpers spawn the
                `loom` binary — ensure spawned children get LOOM_NO_WIZARD=1 or
                Stdio::null() for stdin so cargo test from a TTY does not fire
                the wizard mid-test. If multiple helpers spawn `loom`, update
                all of them. Grep guidance:
                  rg -n 'Command::new\("loom"|cargo_bin\("loom"|"loom".to_string'
                    loom/tests
                )
            Files Read-Only:
              - src/global_config.rs (use its public API)
              - src/cli/types_stage.rs (reference for nested subcommand pattern)
              - src/commands/status/ui/tui/app.rs lines 70, 246, 356
                (terminal setup + Drop-style cleanup pattern — copy this exactly)
              - src/commands/status/ui/tui/event_handler.rs (key matching pattern only)

          Subagent B — Remote Control Integration (loom-software-engineer):
            Files Owned:
              - src/remote_control.rs
              - src/commands/init/plan_setup.rs (REMOVE default RC section write at lines 164-170)
            Files Read-Only:
              - src/global_config.rs
              - src/fs/work_dir.rs (existing read_remote_control_config / read_config)
              - src/commands/run/mod.rs (read-only — confirms preflight call site)
              - src/commands/run/foreground.rs (read-only — confirms preflight call site)

          Subagent C — Knowledge Command Integration (loom-software-engineer):
            Files Owned:
              - src/commands/knowledge/bootstrap.rs
              - src/commands/knowledge/gc.rs
              - src/commands/knowledge/mod.rs (re-export pure helpers if needed)
              - src/cli/types_memory.rs (help-text update for Gc and Bootstrap)
            Files Read-Only:
              - src/global_config.rs

        ══════════════════════════════════════════════════════════════
        SUBAGENT A — CLI Command (detailed instructions):

        A1. ADD ConfigCommands ENUM TO src/cli/types.rs:
           Mirror the pattern from types_stage.rs / types_memory.rs.

             #[derive(Subcommand, Debug)]
             pub enum ConfigCommands {
                 /// Print current settings as TOML
                 Show,
                 /// Set a single setting non-interactively
                 Set {
                     /// Setting key: remote-control | bootstrap-model | gc-model
                     key: String,
                     /// Setting value (auto|off for remote-control, sonnet|opus for models)
                     value: String,
                 },
             }

           Add to the Commands enum (top of cli/types.rs):

             /// Manage global user configuration (~/.loom/config.toml)
             Config {
                 #[command(subcommand)]
                 command: Option<ConfigCommands>,
             },

           `command: Option<...>` so `loom config` (no subcommand) launches
           wizard/TUI; explicit subcommands are non-interactive.

        A2. ADD DISPATCH ARM TO src/cli/dispatch.rs:
           IMPORTANT: cli/dispatch.rs is part of the BINARY crate. It imports
           library code through `loom::...`, not `crate::...`. See the
           existing imports at lines 1-13.

           Extend the existing imports:
             use loom::{commands::config, global_config};
             use super::types::{... existing ..., ConfigCommands};

           Add the match arm inside dispatch():
             Commands::Config { command } => match command {
                 None => config::run_interactive(),
                 Some(ConfigCommands::Show) => config::show(),
                 Some(ConfigCommands::Set { key, value }) => config::set(&key, &value),
             },

        A2b. ADD FIRST-RUN PRELUDE HOOK IN src/cli/dispatch.rs:
           The current signature is `pub fn dispatch(command: Commands)` — note
           the parameter is `command`, NOT `cli`. Earlier drafts referenced
           `&cli.command` which would not compile.

           At the top of dispatch(), BEFORE the match, call:
             run_first_run_wizard_if_needed(&command)?;

           Helper (private function in src/cli/dispatch.rs):

             use std::io::IsTerminal;

             fn run_first_run_wizard_if_needed(cmd: &Commands) -> anyhow::Result<()> {
                 // Skip commands that must never block (shell completion,
                 // config command itself which handles its own dispatch).
                 if matches!(cmd,
                     Commands::Config { .. }
                     | Commands::Completions { .. }
                     | Commands::Complete { .. }
                 ) {
                     return Ok(());
                 }
                 // Escape hatch for tests and CI.
                 if std::env::var("LOOM_NO_WIZARD").as_deref() == Ok("1") {
                     return Ok(());
                 }
                 // Both stdin AND stdout must be terminals — otherwise the
                 // command may be piped to a file or another process, and
                 // injecting wizard output would corrupt machine-readable
                 // output (e.g. `loom plan verify --json > out.json`).
                 if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
                     return Ok(());
                 }
                 if !global_config::first_run_required()? {
                     return Ok(());
                 }
                 // Fire the wizard. If it fails or is cancelled, propagate
                 // the error so the original command does not run with
                 // ambiguous state. The user can re-invoke after fixing.
                 config::wizard_run()
             }

           Re-export so the prelude can call it without breaking module
           privacy:
             // in commands/config/mod.rs:
             pub use wizard::run as wizard_run;

        A3. CREATE src/commands/config/mod.rs:
           Module structure:
             mod wizard;
             mod tui;

             use crate::global_config::{self, GlobalConfig, GlobalRemoteControlMode, KnowledgeModel};
             use anyhow::{bail, Result};

             pub fn run_interactive() -> Result<()> {
                 // Guard against being invoked without a TTY (e.g.
                 // `loom config < /dev/null` or piped). dialoguer/ratatui
                 // would hang or error opaquely; fall through to show()
                 // with a clear hint.
                 use std::io::IsTerminal;
                 if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
                     eprintln!(
                         "loom config requires an interactive terminal. \
                          Use `loom config show` or `loom config set <key> <value>` \
                          for non-interactive access."
                     );
                     return show();
                 }
                 if global_config::exists()? {
                     tui::run()
                 } else {
                     wizard::run()
                 }
             }

             pub fn show() -> Result<()> {
                 let cfg = global_config::load()?;
                 let toml = toml::to_string_pretty(&cfg)?;
                 print!("{}", toml);
                 println!();
                 if let Ok(p) = global_config::path() {
                     println!("# source: {}", p.display());
                 }
                 Ok(())
             }

             pub fn set(key: &str, value: &str) -> Result<()> {
                 let mut cfg = global_config::load()?;
                 match key {
                     "remote-control" => cfg.remote_control_mode = match value {
                         "auto" => RemoteControlMode::Auto,
                         "off" => RemoteControlMode::Off,
                         _ => bail!("invalid value for remote-control: {} (expected auto|off)", value),
                     },
                     "bootstrap-model" => cfg.knowledge_bootstrap_model = KnowledgeModel::parse(value)?,
                     "gc-model" => cfg.knowledge_gc_model = KnowledgeModel::parse(value)?,
                     _ => bail!("unknown key: {} (expected remote-control|bootstrap-model|gc-model)", key),
                 }
                 global_config::save(&cfg)?;
                 println!("set {} = {}", key, value);
                 Ok(())
             }

        A4. CREATE src/commands/config/wizard.rs:
           Use dialoguer for sequential prompts. All prompts and narrative
           copy go to STDERR (eprintln!) so wizard text never corrupts
           stdout when stdout is redirected. Only the final "saved" line
           may go to stdout (a single, parseable line).

           Imports:
             use anyhow::Result;
             use dialoguer::{Confirm, Select};
             use crate::global_config::{self, GlobalConfig, KnowledgeModel};
             use crate::remote_control::RemoteControlMode;

           pub fn run() -> Result<()>:
             eprintln!("Welcome to loom! Let's set up your global config.");
             eprintln!("(Stored at ~/.loom/config.toml)\n");

             let remote_control_enabled = Confirm::new()
                 .with_prompt("Enable remote control for sessions by default?")
                 .default(true)
                 .interact()?;

             let models = &["sonnet", "opus"];

             let bootstrap_idx = Select::new()
                 .with_prompt("Default model for `loom knowledge bootstrap`")
                 .items(models)
                 .default(0)
                 .interact()?;

             let gc_idx = Select::new()
                 .with_prompt("Default model for `loom knowledge gc`")
                 .items(models)
                 .default(0)
                 .interact()?;

             let cfg = GlobalConfig {
                 remote_control_mode: if remote_control_enabled {
                     RemoteControlMode::Auto
                 } else {
                     RemoteControlMode::Off
                 },
                 knowledge_bootstrap_model: idx_to_model(bootstrap_idx),
                 knowledge_gc_model: idx_to_model(gc_idx),
             };

             global_config::save(&cfg)?;
             // Final confirmation: stdout (one line) so a wrapper can capture.
             println!("saved {}", global_config::path()?.display());
             Ok(())

             fn idx_to_model(i: usize) -> KnowledgeModel {
                 if i == 1 { KnowledgeModel::OpusOneM } else { KnowledgeModel::Sonnet }
             }

        A5. CREATE src/commands/config/tui.rs:
           Ratatui-based settings menu.

           ⚠️ Terminal setup AND cleanup (RAII guard + panic hook) live in
           src/commands/status/ui/tui/app.rs at approximately lines 70, 246,
           and 356. READ those lines first and REPLICATE the pattern exactly
           — particularly the Drop-style cleanup guard. Do NOT invent a new
           cleanup pattern. event_handler.rs has the key-matching pattern
           only.

           Layout: vertical list of 3 settings. Each row shows:
             [<setting name>]    current value

           Key bindings:
             ↑/↓ or j/k: navigate
             Enter / Space: cycle the focused setting to next value
             s: save and exit
             q / Esc: quit without saving (no confirm; user can re-run)

           Structure:
             struct AppState {
                 cfg: GlobalConfig,
                 original: GlobalConfig,
                 selected: usize, // 0=remote_control, 1=bootstrap, 2=gc
             }

             impl AppState:
                 fn cycle_selected(&mut self):
                     // v1: each setting has exactly 2 values; Enter toggles.
                     // If a setting ever grows a third option, this becomes a
                     // popup picker — document at that time.
                     match self.selected {
                         0 => toggle remote_control_mode (Auto <-> Off)
                         1 => toggle knowledge_bootstrap_model (Sonnet <-> OpusOneM)
                         2 => toggle knowledge_gc_model (Sonnet <-> OpusOneM)
                     }

                 fn dirty(&self) -> bool { self.cfg != self.original }

           pub fn run() -> Result<()>:
             Initial: cfg = global_config::load()?, original = cfg.clone(), selected = 0.
             Acquire the terminal cleanup guard pattern from app.rs:70/246/356.
             Event loop:
               render frame
               poll(100ms) -> handle key event
             On 's': global_config::save(&state.cfg)?, break.
             On 'q'/Esc: drop without saving.

        A6. ADD pub mod config; TO src/commands/mod.rs.

        A7. ADD DIALOGUER DEP:
           From the loom/ directory:
             cargo add dialoguer@0.11
           Verify Cargo.toml now contains a dialoguer entry.

        A8. UPDATE src/completions/dynamic/commands.rs:
           Three edits in commands.rs (hand-maintained tables, alphabetical):
             - TOP_LEVEL_COMMANDS (line 6): insert "config" between "completions" and "diagnose".
             - complete_subcommands (line 37 match): add arm
                 "config" => &["set", "show"],
             - has_subcommands (line 145 match): add "config" to the list.
           If a tests.rs sits alongside commands.rs, add at minimum:
             - test that complete_commands("c") includes "config"
             - test that complete_subcommands("config", "") returns ["set", "show"]
             - test that has_subcommands("config") is true

        A9. PROTECT INTEGRATION TESTS FROM THE FIRST-RUN WIZARD:
           Existing integration tests spawn the `loom` binary (see
           loom/tests/integration/). When `cargo test` is run from a TTY on a
           developer machine, child processes inherit the TTY stdin/stdout
           and the new wizard prelude would fire mid-test.

           Grep for all spawn sites:
             rg -n 'Command::new\("loom"|cargo_bin\("loom"|assert_cmd::Command' loom/tests

           For each helper that builds a `Command` invoking the loom binary,
           apply BOTH of these (cheap defense in depth):
             cmd.env("LOOM_NO_WIZARD", "1");
             cmd.stdin(std::process::Stdio::null());

           If a shared helpers.rs exists, centralize the change there. Add a
           one-line comment explaining why.

        ══════════════════════════════════════════════════════════════
        SUBAGENT B — Remote Control Integration:

        ⚠️ CONTRACT CHANGE: Today `loom init` writes a default `[remote_control]`
        section to every new `.work/config.toml` (plan_setup.rs:164-170). With
        the default ALWAYS present, the "fall back to global when section
        missing" branch would never fire. B1 fixes this; B2 introduces the
        effective-mode helper; B3 routes both consumers through it.

        B1. REMOVE INIT-TIME DEFAULT RC SECTION WRITE:
           In src/commands/init/plan_setup.rs lines 164-170, DELETE the block:

             // Persist a default [remote_control] section so the operator has a
             // documented, editable toggle in .work/config.toml from the start.
             work_dir::write_remote_control_config(
                 work_dir.root(),
                 &crate::remote_control::RemoteControlConfig::default(),
             )
             .context("Failed to persist remote control config")?;

           Replace with a brief comment:
             // Remote control mode is NOT written here. Absence of the
             // `[remote_control]` section means "inherit global config".
             // Users override per-project by writing the section explicitly
             // (or via a future `loom config --project ...`).

        B2. ADD effective_remote_control_mode() to src/remote_control.rs:

             use crate::global_config;

             /// Returns the effective RemoteControlMode for a project work_dir.
             ///
             /// Precedence:
             ///   1. Per-project `.work/config.toml [remote_control]` (if section present)
             ///   2. Global `~/.loom/config.toml` `remote_control_mode`
             ///   3. Hardcoded default (Auto)
             ///
             /// All errors are swallowed (treated as Auto default) so spawn
             /// sites can call this unconditionally.
             pub fn effective_remote_control_mode(work_dir: &Path) -> RemoteControlMode {
                 // 1. Per-project: only honor if the section is actually present.
                 //    (read_remote_control_config returns ::default() for a
                 //     missing section, which would shadow the global setting.)
                 if let Ok(doc) = crate::fs::work_dir::read_config(work_dir) {
                     if doc.get("remote_control").is_some() {
                         return crate::fs::work_dir::read_remote_control_config(work_dir)
                             .map(|c| c.mode)
                             .unwrap_or_default();
                     }
                 }
                 // 2. Global config.
                 if let Ok(cfg) = global_config::load() {
                     return cfg.remote_control_mode;
                 }
                 // 3. Default.
                 RemoteControlMode::default()
             }

        B3. ROUTE ALL CONSUMERS THROUGH effective_remote_control_mode():

           CRITICAL: there are TWO consumers of remote-control mode, not one.
           Grep BEFORE editing to ensure complete coverage:
             rg -n 'read_remote_control_config\(' loom/src

           Currently both call sites read mode directly:
             - remote_control.rs:248-251 (resolve)
             - remote_control.rs:271-274 (run_startup_preflight)

           Change each occurrence of:
             let mode = read_remote_control_config(work_dir)
                 .map(|c| c.mode)
                 .unwrap_or_default();
           to:
             let mode = effective_remote_control_mode(work_dir);

           Verify after editing:
             rg -n 'read_remote_control_config\(' loom/src
           Should return zero hits in remote_control.rs (only fs/work_dir.rs
           and the new effective_remote_control_mode body should remain).

        B4. ADD UNIT TESTS (must use the EnvGuard pattern from foundation):

           In remote_control.rs tests module, add:

             test_effective_mode_inherits_global_when_section_missing:
               LOOM_GLOBAL_CONFIG_DIR=tmp, write global with mode=Off.
               TempDir work_dir with EMPTY config.toml (no [remote_control]).
               Assert effective_remote_control_mode == Off.

             test_effective_mode_per_project_overrides_global:
               LOOM_GLOBAL_CONFIG_DIR=tmp, write global with mode=Off.
               TempDir work_dir, write [remote_control] mode = "auto"
               via fs/work_dir::write_remote_control_config.
               Assert effective_remote_control_mode == Auto.

             test_effective_mode_default_when_neither_set:
               LOOM_GLOBAL_CONFIG_DIR=tmp (empty — no config file).
               TempDir work_dir with EMPTY config.toml.
               Assert effective_remote_control_mode == Auto.

           All three #[serial_test::serial] + EnvGuard.

        B5. ADD INIT REGRESSION TEST:

           In a test for init (or a new test in commands/init/), assert that
           after `initialize_with_plan` runs against a temp project, the
           resulting .work/config.toml does NOT contain a [remote_control]
           section. This prevents regressing the B1 contract change.

           If no equivalent init test exists already, place the new test
           alongside the existing init unit tests; use TempDir + a minimal
           ParsedPlan fixture.

        ══════════════════════════════════════════════════════════════
        SUBAGENT C — Knowledge Command Integration:

        Factor the model-resolution logic into PURE helpers that can be unit
        tested without spawning a Claude process or touching the file system
        beyond LOOM_GLOBAL_CONFIG_DIR. The current bootstrap/gc commands
        return early on various conditions (missing knowledge dir, clean
        knowledge state, missing claude binary), so testing the inlined
        resolution at the call site would require fixturing all of that.
        Pure helpers sidestep this.

        C1. ADD PURE HELPERS — pick ONE of these locations:
           (a) New module src/commands/knowledge/model_default.rs, re-exported
               from commands/knowledge/mod.rs.
           (b) Inline pub(crate) fn in each command file.

           Recommended: (a). The helpers:

             use crate::global_config;
             use crate::commands::knowledge::model_default::KnowledgeKind;

             /// Resolve the model for `loom knowledge bootstrap`.
             ///
             /// Precedence: CLI --model > global config > hardcoded "sonnet".
             pub fn resolve_bootstrap_model(cli_model: Option<String>) -> String {
                 cli_model.unwrap_or_else(|| {
                     global_config::load()
                         .map(|cfg| cfg.knowledge_bootstrap_model.as_str().to_string())
                         .unwrap_or_else(|_| "sonnet".to_string())
                 })
             }

             /// Resolve the model for `loom knowledge gc`.
             ///
             /// Precedence: CLI --model > global config > hardcoded "sonnet".
             /// NOTE: previous hardcoded default was "opus" (commit 751bd29);
             /// it is now "sonnet" to match the global config's documented
             /// default and to honor the user's setting when they have not
             /// explicitly opted into opus. Users wanting opus must set
             /// `loom config set gc-model opus` or pass --model.
             pub fn resolve_gc_model(cli_model: Option<String>) -> String {
                 cli_model.unwrap_or_else(|| {
                     global_config::load()
                         .map(|cfg| cfg.knowledge_gc_model.as_str().to_string())
                         .unwrap_or_else(|_| "sonnet".to_string())
                 })
             }

        C2. MODIFY src/commands/knowledge/bootstrap.rs:
           At the current `model.unwrap_or_else(|| "sonnet".to_string())` site
           (around line 35), replace with:
             let model = resolve_bootstrap_model(model);
           Add: use super::model_default::resolve_bootstrap_model;
           (or the right path depending on C1's location choice).

        C3. MODIFY src/commands/knowledge/gc.rs:
           At the current `model.unwrap_or_else(|| "opus".to_string())`
           site (around line 44), replace with:
             let effective_model = resolve_gc_model(model);
           Update the comment immediately above to reflect the new default
           ("sonnet by default; user opts into opus via global config").

        C4. UPDATE src/cli/types_memory.rs (around line 77):
           The current doc comment on the `Gc { model: Option<String> }` field
           says: `default: "opus" — GC is judgement-heavy`.
           Change to:
             `default: from global config (knowledge_gc_model); falls back to "sonnet"`
           Similarly update the Bootstrap variant's `--model` help if it
           currently mentions "sonnet" as a literal default.

        C5. ADD UNIT TESTS for both helpers:

           In commands/knowledge/model_default.rs (or wherever C1 placed them),
           add:

             test_resolve_bootstrap_cli_wins:
               LOOM_GLOBAL_CONFIG_DIR=tmp, global says opus.
               resolve_bootstrap_model(Some("custom".into())) == "custom".

             test_resolve_bootstrap_falls_back_to_global:
               LOOM_GLOBAL_CONFIG_DIR=tmp, global says opus.
               resolve_bootstrap_model(None) == "opus".

             test_resolve_bootstrap_defaults_to_sonnet:
               LOOM_GLOBAL_CONFIG_DIR=tmp (empty), no config file.
               resolve_bootstrap_model(None) == "sonnet".

             Symmetric trio for resolve_gc_model.

           All six tests #[serial_test::serial] + EnvGuard pattern.

        ══════════════════════════════════════════════════════════════
        MAIN AGENT WORK (after all 3 subagents return):

        M1. Verify cargo build passes from loom/.
        M2. Verify cargo test --all-targets passes from loom/.
        M3. Verify cargo clippy --all-targets -- -D warnings passes.
        M4. Run smoke test:
              cargo build && \
              LOOM_GLOBAL_CONFIG_DIR=/tmp/loom-cfg-test ./target/debug/loom config show
            (should print default config)
              ./target/debug/loom config set remote-control off
            (should print "set remote-control = off")
              ./target/debug/loom config show
            (should show remote-control-mode = "off")
              rm -rf /tmp/loom-cfg-test
        M5. Stage and commit logically grouped:
              - feat(config): add ~/.loom/config.toml schema and read/write API
              - feat(config): add loom config command with wizard, TUI, show, set
              - feat(remote-control): fall back to global config when per-project missing
              - feat(knowledge): bootstrap/gc default to global config model
            (Adapt grouping to what was actually changed.)
        M6. loom stage complete implement
      acceptance:
        - "cargo build"
        - "cargo fmt --check"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo test --all-targets"
        - command: 'LOOM_NO_WIZARD=1 ./target/debug/loom config --help'
          stdout_contains: ["show", "set"]
        - command: 'tmp=$(mktemp -d) && LOOM_NO_WIZARD=1 LOOM_GLOBAL_CONFIG_DIR=$tmp ./target/debug/loom config show; rc=$?; rm -rf "$tmp"; exit $rc'
          exit_code: 0
        - command: 'tmp=$(mktemp -d) && LOOM_NO_WIZARD=1 LOOM_GLOBAL_CONFIG_DIR=$tmp ./target/debug/loom config set remote-control off && LOOM_NO_WIZARD=1 LOOM_GLOBAL_CONFIG_DIR=$tmp ./target/debug/loom config show; rc=$?; rm -rf "$tmp"; exit $rc'
          stdout_contains: ["off"]
        - command: 'tmp=$(mktemp -d) && LOOM_NO_WIZARD=1 LOOM_GLOBAL_CONFIG_DIR=$tmp ./target/debug/loom config set gc-model "opus" && LOOM_NO_WIZARD=1 LOOM_GLOBAL_CONFIG_DIR=$tmp ./target/debug/loom config show; rc=$?; rm -rf "$tmp"; exit $rc'
          stdout_contains: ["opus"]
        # Non-TTY skip path: stdin from /dev/null + a real dispatched command
        # (NOT --help, which clap handles before dispatch). `loom status` is
        # safe — it does not require a daemon to print its static header.
        # The wizard prelude must skip due to non-TTY stdin, even without
        # LOOM_NO_WIZARD set.
        - command: 'tmp=$(mktemp -d) && LOOM_GLOBAL_CONFIG_DIR=$tmp ./target/debug/loom status < /dev/null; rc=$?; rm -rf "$tmp"; [ $rc -eq 0 ] || [ $rc -eq 1 ]'
        # Invalid setter value rejected with non-zero exit:
        - command: 'tmp=$(mktemp -d) && LOOM_NO_WIZARD=1 LOOM_GLOBAL_CONFIG_DIR=$tmp ./target/debug/loom config set remote-control banana; rc=$?; rm -rf "$tmp"; [ $rc -ne 0 ]'
      artifacts:
        - "src/commands/config/mod.rs"
        - "src/commands/config/wizard.rs"
        - "src/commands/config/tui.rs"
      wiring:
        - source: "src/cli/types.rs"
          pattern: 'Config \{'
          description: "Config variant added to Commands enum"
        - source: "src/cli/types.rs"
          pattern: "ConfigCommands"
          description: "ConfigCommands subcommand enum defined"
        - source: "src/cli/dispatch.rs"
          pattern: "Commands::Config"
          description: "Config command dispatch arm wired"
        - source: "src/cli/dispatch.rs"
          pattern: "run_first_run_wizard_if_needed"
          description: "First-run wizard prelude hook installed in dispatch()"
        - source: "src/cli/dispatch.rs"
          pattern: "LOOM_NO_WIZARD"
          description: "Prelude honors LOOM_NO_WIZARD escape hatch"
        - source: "src/cli/dispatch.rs"
          pattern: "IsTerminal"
          description: "Prelude TTY-gates the wizard via std::io::IsTerminal"
        - source: "src/commands/mod.rs"
          pattern: "pub mod config"
          description: "config module exported"
        - source: "src/commands/config/mod.rs"
          pattern: "pub use wizard::run"
          description: "wizard::run is re-exported for the CLI prelude"
        - source: "src/commands/config/mod.rs"
          pattern: 'gc-model'
          description: "set() accepts gc-model key (not distill-model)"
        - source: "src/commands/init/plan_setup.rs"
          pattern: "Absence of the"
          description: "init no longer writes a default [remote_control] section"
        - source: "src/remote_control.rs"
          pattern: "effective_remote_control_mode"
          description: "effective_remote_control_mode helper defined"
        - source: "src/completions/dynamic/commands.rs"
          pattern: '"config"'
          description: "config is in TOP_LEVEL_COMMANDS / completion tables"
        - source: "src/commands/knowledge/bootstrap.rs"
          pattern: "resolve_bootstrap_model"
          description: "bootstrap uses pure resolve helper"
        - source: "src/commands/knowledge/gc.rs"
          pattern: "resolve_gc_model"
          description: "gc uses pure resolve helper"
        - source: "src/cli/types_memory.rs"
          pattern: "knowledge_gc_model"
          description: "gc --model help text references the global config field"

    - id: integration-verify
      name: "Integration Verification"
      stage_type: integration-verify
      model: "opus"
      reasoning_effort: "xhigh"
      execution_mode: team
      working_dir: "loom"
      dependencies: ["implement"]
      description: |
        Final integration verification.

        Use parallel subagents and skills to maximize performance.

        ⛔ NEVER use Claude Code's auto-memory.
        ALL memory/knowledge goes through loom memory and loom knowledge.

        CONTEXT GATHERING (FIRST):
        0a. Read doc/plans/IN_PROGRESS-PLAN-loom-config-command.md
        0b. Read all stage memories: loom memory show --all
        0c. Read doc/loom/knowledge/*.md for context and known mistakes

        ⛔ ZERO TOLERANCE: ALL warnings/errors/test failures must be FIXED
        (not suppressed). Nothing is "pre-existing."

        BUILD & TEST:
          1. cargo build (loom/) succeeds
          2. cargo test --all-targets (loom/) succeeds
          3. cargo clippy --all-targets -- -D warnings (loom/) succeeds
          4. cargo fmt --check (loom/) succeeds

        FUNCTIONAL VERIFICATION (loom-software-engineer team member):
          - `loom config --help` shows the command
          - `LOOM_GLOBAL_CONFIG_DIR=$tmp loom config show` prints defaults when
            file does not exist (since load() returns Default on missing file)
          - `LOOM_GLOBAL_CONFIG_DIR=$tmp loom config set remote-control off`
            writes the file; subsequent `show` reflects the change
          - `loom config set bootstrap-model 'opus'` and `... gc-model
            'opus'` both succeed (quoted for zsh)
          - Invalid values rejected: `loom config set remote-control banana`
            exits non-zero with a clear message
          - Schema integration: with global config set to Off and no
            per-project [remote_control], confirm remote_control::resolve()
            returns false. (Add a unit test if not already present.)

        CODE REVIEW (loom-code-reviewer parallel agents):
          5. Security: file path traversal in LOOM_GLOBAL_CONFIG_DIR, no
             secrets in config, no unsafe TOML parsing
          6. Architecture: GlobalConfig serde shape preserves unknown keys
             for forward compat, no breakage in remote_control dispatch path
          7. Tests: coverage for default-when-missing, roundtrip, per-project
             override semantics

        FIX ALL FINDINGS with loom-software-engineer (or senior) — code review
        agents are read-only.

        KNOWLEDGE UPDATES (REQUIRED — write before completing):
          loom knowledge update patterns "## Global User Config Pattern\n..."
          loom knowledge update entry-points "## loom config command\n..."
          loom knowledge update stack "## Dialoguer dependency\n..."
          loom knowledge update mistakes "..." for anything that surprised the team.

        MEMORY:
          loom memory list (verify insights captured)
          loom memory note for any non-obvious discoveries

        COMMIT & COMPLETE:
          Logical commits, then loom stage complete integration-verify.
      acceptance:
        - "cargo build"
        - "cargo fmt --check"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo test --all-targets"
        - command: 'LOOM_NO_WIZARD=1 ./target/debug/loom config --help'
          stdout_contains: ["show", "set"]
        - command: 'LOOM_NO_WIZARD=1 ./target/debug/loom config set --help'
          stdout_contains: ["key", "value"]
        # Functional smoke: set then show round-trips through ~/.loom/config.toml.
        - command: 'tmp=$(mktemp -d) && LOOM_NO_WIZARD=1 LOOM_GLOBAL_CONFIG_DIR=$tmp ./target/debug/loom config set remote-control off && LOOM_NO_WIZARD=1 LOOM_GLOBAL_CONFIG_DIR=$tmp ./target/debug/loom config show; rc=$?; rm -rf "$tmp"; exit $rc'
          stdout_contains: ["off"]
        # The unit tests in remote_control.rs and model_default.rs cover the
        # precedence semantics (per-project > global > default) and the init
        # regression that .work/config.toml no longer pre-creates [remote_control].
        # Those are run by `cargo test --all-targets` above.
      wiring:
        - source: "src/lib.rs"
          pattern: "pub mod global_config"
          description: "global_config module exported"
        - source: "src/cli/dispatch.rs"
          pattern: "Commands::Config"
          description: "config command dispatched"
        - source: "src/cli/dispatch.rs"
          pattern: "run_first_run_wizard_if_needed"
          description: "first-run prelude installed"
        - source: "src/commands/init/plan_setup.rs"
          pattern: "Absence of the"
          description: "init no longer writes default [remote_control] section"
        - source: "src/remote_control.rs"
          pattern: "effective_remote_control_mode"
          description: "remote_control reads global default"
        - source: "src/commands/knowledge/bootstrap.rs"
          pattern: "resolve_bootstrap_model"
          description: "bootstrap routes through pure helper"
        - source: "src/commands/knowledge/gc.rs"
          pattern: "resolve_gc_model"
          description: "gc routes through pure helper"
        - source: "src/completions/dynamic/commands.rs"
          pattern: '"config"'
          description: "config wired into dynamic completions"

    - id: knowledge-distill
      name: "Knowledge Distillation"
      stage_type: knowledge-distill
      model: "sonnet"
      reasoning_effort: "high"
      working_dir: "."
      dependencies: ["integration-verify"]
      description: |
        Curate session memories from this plan into permanent knowledge.

        Use parallel subagents and skills to maximize performance.

        Step 1 - READ ALL MEMORIES:
          loom memory show --all
          Identify durable insights worth preserving in doc/loom/knowledge/.

        Step 2 - UPDATE KNOWLEDGE (loom knowledge update only):
          - patterns.md: any new patterns introduced (e.g., per-user config
            pattern, dialoguer wizard pattern, env-override for test isolation)
          - entry-points.md: loom config command added to dispatch table
          - stack.md: dialoguer crate added
          - mistakes.md: any actionable prevention rules from this work

        Step 3 - VERIFY:
          loom knowledge check
          loom memory list (confirm nothing important was missed)

        ⛔ Use loom knowledge update / loom memory CLI commands ONLY.
        NEVER use Write/Edit on knowledge files directly during this stage.
        NEVER use Claude Code's auto-memory.
      acceptance:
        - "loom knowledge check"
      artifacts:
        - "doc/loom/knowledge/patterns.md"
        - "doc/loom/knowledge/entry-points.md"
```

<!-- END loom METADATA -->

## Verification (end-to-end smoke)

⚠️ Every command below uses `LOOM_GLOBAL_CONFIG_DIR=$tmp` against a temporary
directory — do NOT recommend `rm -rf ~/.loom/` to operators. Always quote
`'opus'` in shells because zsh treats brackets as glob characters.

After `integration-verify` completes, operator smoke:

```bash
tmp=$(mktemp -d)
export LOOM_GLOBAL_CONFIG_DIR=$tmp
```

1. **Wizard (first run):** `loom config` → expect wizard with 3 prompts on
   stderr. Accept defaults. Verify `$tmp/config.toml` exists with:

   ```toml
   remote_control_mode = "auto"
   knowledge_bootstrap_model = "sonnet"
   knowledge_gc_model = "sonnet"
   ```

2. **TUI (existing config):** `loom config` again → expect ratatui menu
   showing 3 settings + current values. Navigate ↑/↓, toggle with Enter,
   save with `s`.
3. **Show:** `loom config show` → TOML dump matching the file plus a
   `# source: ...` line.
4. **Set known key (model):** `loom config set bootstrap-model 'opus'`
   → file updated; `show` confirms.
5. **Set invalid value:** `loom config set remote-control banana` → non-zero
   exit, message naming the legal values.
6. **Per-project absent → global wins:** in a fresh project, run
   `loom init <plan>`. Verify `.work/config.toml` does NOT contain a
   `[remote_control]` section (regression check for init contract change).
   Then `LOOM_GLOBAL_CONFIG_DIR=$tmp loom config set remote-control off` and
   `loom run` a small plan; confirm sessions do NOT receive `--remote-control`
   (inspect `.work/orchestrator.log` or `ps`).
7. **Per-project explicit overrides global:** in the same project, write
   `[remote_control]\nmode = "auto"` to `.work/config.toml`. With global
   still `off`, `loom run` → sessions DO receive `--remote-control`.
8. **Knowledge model default:** `LOOM_GLOBAL_CONFIG_DIR=$tmp loom config set gc-model 'opus'`,
   then `loom knowledge gc --dry-run` → uses `opus`. Reset:
   `loom config set gc-model sonnet`, re-run → uses `sonnet`. Explicit
   `--model haiku` always wins regardless of global.
9. **Cleanup:** `rm -rf "$tmp"; unset LOOM_GLOBAL_CONFIG_DIR`.
