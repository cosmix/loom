# Remote Control

> Call sites

## Remote Control & Permission Mode Integration Points

> **Note:** Any references to a "container backend" or `BackendType` elsewhere in these knowledge
> files are **stale** — there is no container backend.
> All session spawning goes through the native backend (or tmux).

### 1. PermissionMode enum — import and defaults

- `loom/src/sandbox/config.rs` imports `PermissionMode` and `StageType` from `crate::plan::schema` (no `BackendType` in scope anywhere in this file)
- `loom/src/sandbox/config.rs::MergedSandboxConfig` — `default_mode_for(stage_type: StageType) -> PermissionMode`
  - ALL stage types (Knowledge, KnowledgeDistill, Standard, IntegrationVerify) → `Auto`
- `loom/src/sandbox/config.rs::merge_config` — `merge_config(plan, stage, stage_type)` — precedence: stage > plan > `default_mode_for`
- `loom/src/sandbox/config.rs::validate_config` — `validate_config(merged)` — rejects `BypassPermissions` unconditionally
- `loom/src/sandbox/config.rs::test_merge_config_stage_overrides` — `test_default_mode_for_stage_type`
- `loom/src/sandbox/config.rs::test_merge_config_knowledge_stage` — `test_merge_config_permission_mode_precedence`

### 2. permission mode delivery — TWO mechanisms (settings file is NOT enough for `auto`)

- `loom/src/sandbox/settings.rs::apply_default_mode` maps loom's kebab-case `PermissionMode` to Claude's camelCase wire format (`"acceptEdits"`, `"bypassPermissions"`, etc.); it runs at the end of `generate_settings_json`, which `build_settings` calls, so the per-session capsule built by `capsule_settings` (`orchestrator/terminal/native/session_settings/contents.rs`) carries a `permissions.defaultMode`.
- **⚠️ The settings file is IGNORED for `auto`.** Claude Code v2.1.142+ deliberately ignores `permissions.defaultMode: "auto"` when it comes from **project/local** settings (`.claude/settings.json` / `.claude/settings.local.json`) — a repo cannot grant itself auto mode. Only the `--permission-mode` **CLI startup flag** (or user/managed settings) is honored. So the authoritative delivery is the CLI flag emitted in `build_claude_command` (§3), NOT the settings file. The `defaultMode` in the session capsule is still emitted (harmless; honored for non-`auto` modes) but is redundant given the flag. See mistakes.md "settings.local.json `defaultMode: auto` is silently ignored".

### 3. Claude command-build sites (NativeBackend)

All four public `spawn_*` methods funnel through the single unified `spawn()` in `loom/src/orchestrator/terminal/native/mod.rs`, which calls `prepare_session_launch()` (`native/launch.rs`), which calls the shared `build_claude_command()`. Command shape: `{claude} --model {m} --effort {e} --permission-mode {mode} {prompt}[ --remote-control[=name]]`.

- **`--remote-control[=name]`** — `build_claude_command` takes `remote_control: &RemoteControlInvocation` (not a bool). `prepare_session_launch` computes the name via `remote_control_session_name(kind, stage)` (kind-prefixed table in architecture/remote-control.md), then calls `crate::remote_control::resolve_invocation(work_dir, &rc_name)` to get the `Disabled`/`Bare`/`Named(rc_name)` to pass in. `Named` is joined to the flag with `=`, not a space — see architecture/remote-control.md's known-limitations note on why (optional-argument flag-reparsing risk).
- **`--permission-mode {mode}`** is resolved inside `prepare_session_launch()` via `merge_config(read_plan_sandbox(work_dir), stage.sandbox, stage.stage_type).permission_mode` → `.as_settings_value()`. Reads the SAME `[plan_sandbox]` snapshot that `OrchestratorConfig.sandbox_config` loads from, so the CLI flag and the generated settings file never disagree. `validate_config` rejects `bypass-permissions`, so it never reaches the flag.
- Model/effort policy: `spawn_session` / `spawn_knowledge_session` use `stage.effective_model()` + `stage.effective_reasoning_effort()`; `spawn_merge_session_in_worktree` uses `opus` / `xhigh`. `SessionType::BaseConflict` remains readable for historical session attribution, but there is no public base-conflict spawn path.

All three spawn operations use the shared `prepare_session_launch()` path, which creates the PID-tracking wrapper before dispatching to the selected native or tmux terminal lane.

### 4. Wrapper script — PID tracking template

`loom/src/orchestrator/terminal/native/wrapper.rs::build_wrapper_script` renders the template; the text blocks come from `wrapper/script_text.rs`, and `pid_tracking.rs` owns the PID-file and wrapper-script paths:

- `pid_capture` (`wrapper/script_text.rs`) writes the shell's own PID with `echo $$ > {pid_file}` to `.loom/work/pids/` before the exec
- `exec env -i "${_loom_env[@]}" ...` rebuilds the environment from an allowlist, then `exec`s the claude command (replacing the shell)
- `LOOM_MAIN_AGENT_PID=$$` is exported on that `exec env -i` line, so it is the shell's own PID
- Template also exports: `LOOM_SESSION_ID`, `LOOM_SESSION_TYPE`, `LOOM_STAGE_ID`, `LOOM_WORK_DIR`, `LOOM_WORKTREE_PATH` (stage and contract kinds), `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`, `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`

## Remote Control Integration Points (continued): Config API Through Claude Binary Resolution

### 5. Centralized .loom/work/config.toml API

`loom/src/fs/work_dir/config_sections.rs`:

| Function                                | Notes                                                    |
| --------------------------------------- | -------------------------------------------------------- |
| `read_config(work_dir)`                 | Returns `toml_edit::DocumentMut` (preserves comments)    |
| `write_config(work_dir, doc)`           | Writes doc back; caller must hold any lock               |
| `read_section<T>()` (private)           | Reads a named `[section]` as typed `T: DeserializeOwned` |
| `write_section<T>()` (private)          | Writes typed value into named section, preserving rest   |
| `read_plan_sandbox(work_dir)`           | Public: reads `[plan_sandbox]` → `Option<SandboxConfig>` |
| `write_plan_sandbox(work_dir, sandbox)` | Public: writes `[plan_sandbox]`                          |

**Pattern to mirror for new sections:** add a `const SECTION_NAME: &str = "my_section"` at the top, then two public functions calling `read_section` / `write_section`.

### 6. Plan init — where config.toml sections are written

`loom/src/commands/init/plan_setup.rs`:

- `initialize_with_plan()` — the entry point
- `[plan]` table written via `toml_edit` (`work_dir::read_config` + `doc.insert("plan", ...)` + `work_dir::write_config`)
- `[plan_sandbox]` snapshot written next (`work_dir::write_plan_sandbox`)

### 7. Run command entry points

- `loom/src/commands/run/mod.rs` — `execute_background()` — starts the daemon child through `DaemonServer::with_config(...).start()` (`daemon/server/launch.rs` re-executes the binary as `loom run --daemon-child <root>`); with `--daemon-child` it runs `daemon_child::execute` instead
- `loom/src/commands/run/foreground.rs::execute` — public entry for `--foreground`; marks plan in-progress then calls `execute_foreground()`
- `loom/src/commands/run/foreground.rs::execute_foreground` (private) — builds `OrchestratorConfig` (includes `sandbox_config: plan_sandbox` from `build_execution_graph`)

### 8. Crash handler — failure classification and retry

`loom/src/orchestrator/core/crash_handler.rs::handle_session_crashed`:

- `classify_crash` separates a startup refusal (`startup_refusal_crash`) from an ordinary crash (`ordinary_crash`); `maybe_disable_remote_control` drops the remote-control flag for the retry when it is the suspected cause
- `sync_crashed_session_permissions` — best-effort permission sync from the crashed session's worktree
- `finish_crash_transition` / `persist_blocked_crash` — `stage.retry_count += 1`, then `stage.try_mark_blocked()` → saves stage → marks graph `Blocked`
- Auto-retry decision: `should_auto_retry(failure_type, stage.retry_count, max)` from `orchestrator/retry.rs` (`classify_failure` there maps a close reason to a `FailureType`)

### 9. Claude binary resolution

`loom/src/claude.rs::find_claude_path` — `find_claude_path() -> Result<PathBuf>`:

1. `which::which("claude")` (uses current PATH)
2. `~/.claude/local/claude` (official Claude Code install location)
3. `~/.local/bin/claude`
4. `~/.cargo/bin/claude`
5. `/usr/local/bin/claude`
6. `/opt/homebrew/bin/claude`

Used by all four NativeBackend spawn methods before building the claude command string.
