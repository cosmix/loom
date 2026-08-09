# Remote Control

> Files and call sites for remote-control capability detection and permission-mode resolution.

## Remote Control & Permission Mode Integration Points

> **Note:** Any references to a "container backend" or `BackendType` elsewhere in these knowledge
> files are **stale** — the container backend was removed in commits 5bcf5d8 / c2f16bb.
> All session spawning is now done exclusively through the native backend.

### 1. PermissionMode enum — import and defaults

- `loom/src/sandbox/config.rs:1-4` — `use crate::plan::schema::{..., PermissionMode, StageType, ...}` (no `BackendType` in scope anywhere in this file)
- `loom/src/sandbox/config.rs:49-54` — `default_mode_for(stage_type: StageType) -> PermissionMode`
  - ALL stage types (Knowledge, KnowledgeDistill, Standard, IntegrationVerify) → `Auto`
- `loom/src/sandbox/config.rs:60-95` — `merge_config(plan, stage, stage_type)` — precedence: stage > plan > `default_mode_for`
- `loom/src/sandbox/config.rs:102-112` — `validate_config(merged)` — rejects `BypassPermissions` unconditionally
- `loom/src/sandbox/config.rs:461-475` — `test_default_mode_for_stage_type`
- `loom/src/sandbox/config.rs:478-511` — `test_merge_config_permission_mode_precedence`

### 2. permission mode delivery — TWO mechanisms (settings file is NOT enough for `auto`)

- `loom/src/sandbox/settings.rs:15-26` — `apply_default_mode(settings, mode)` maps loom's kebab-case `PermissionMode` to Claude's camelCase wire format (`"acceptEdits"`, `"bypassPermissions"`, etc.); called at the end of `generate_settings_json()` so every generated `settings.local.json` carries a `permissions.defaultMode`.
- **⚠️ The settings file is IGNORED for `auto`.** Claude Code v2.1.142+ deliberately ignores `permissions.defaultMode: "auto"` when it comes from **project/local** settings (`.claude/settings.json` / `.claude/settings.local.json`) — a repo cannot grant itself auto mode. Only the `--permission-mode` **CLI startup flag** (or user/managed settings) is honored. So the authoritative delivery is the CLI flag emitted in `build_claude_command` (§3), NOT the settings file. The `defaultMode` in settings.local.json is still emitted (harmless; honored for non-`auto` modes) but is redundant given the flag. See mistakes.md "settings.local.json `defaultMode: auto` is silently ignored".

### 3. Claude command-build sites (NativeBackend)

All four public `spawn_*` methods funnel through the single unified `spawn()` in `loom/src/orchestrator/terminal/native/mod.rs`, which calls `prepare_session_launch()` (`native/launch.rs`), which calls the shared `build_claude_command()`. Command shape: `{claude} --model {m} --effort {e} --permission-mode {mode} {prompt}[ --remote-control[=name]]`.

- **`--remote-control[=name]`** — `build_claude_command` takes `remote_control: &RemoteControlInvocation` (not a bool). `prepare_session_launch` computes the name via `remote_control_session_name(kind, stage)` (kind-prefixed table in architecture/remote-control.md), then calls `crate::remote_control::resolve_invocation(work_dir, &rc_name)` to get the `Disabled`/`Bare`/`Named(rc_name)` to pass in. `Named` is joined to the flag with `=`, not a space — see architecture/remote-control.md's known-limitations note on why (optional-argument flag-reparsing risk).
- **`--permission-mode {mode}`** is resolved inside `prepare_session_launch()` via `merge_config(read_plan_sandbox(work_dir), stage.sandbox, stage.stage_type).permission_mode` → `.as_settings_value()`. Reads the SAME `[plan_sandbox]` snapshot that `OrchestratorConfig.sandbox_config` loads from, so the CLI flag and the generated settings file never disagree. `validate_config` rejects `bypass-permissions`, so it never reaches the flag.
- Model/effort policy: `spawn_session` / `spawn_knowledge_session` use `stage.effective_model()` + `stage.effective_reasoning_effort()`; `spawn_merge_session` uses `opus` / `xhigh`. `SessionType::BaseConflict` remains readable for historical session attribution, but there is no public base-conflict spawn path.

All three spawn operations use the shared `prepare_session_launch()` path, which creates the PID-tracking wrapper before dispatching to the selected native or tmux terminal lane.

### 4. Wrapper script — PID tracking template

`loom/src/orchestrator/terminal/native/pid_tracking.rs`:

- `create_wrapper_script()` — lines 250–368
- Wrapper `format\!()` template — lines 321–350
- Key template lines:
  - `:332` — `export LOOM_MAIN_AGENT_PID=$$` (set to shell's own PID before exec)
  - `:337` — `echo $$ > {pid_file}` (writes PID to `.work/pids/{stage_id}.pid`)
  - `:340` — `exec {claude_cmd}` (replaces shell with claude process)
- Template also exports: `LOOM_SESSION_ID`, `LOOM_STAGE_ID`, `LOOM_WORK_DIR`, `LOOM_WORKTREE_PATH`, `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`

### 5. Centralized .work/config.toml API

`loom/src/fs/work_dir.rs` (block starting at line 328):

| Function                                | Lines   | Notes                                                    |
| --------------------------------------- | ------- | -------------------------------------------------------- |
| `read_config(work_dir)`                 | 356–366 | Returns `toml_edit::DocumentMut` (preserves comments)    |
| `write_config(work_dir, doc)`           | 370–381 | Writes doc back; caller must hold any lock               |
| `read_section<T>()` (private)           | 383–402 | Reads a named `[section]` as typed `T: DeserializeOwned` |
| `write_section<T>()` (private)          | 404–429 | Writes typed value into named section, preserving rest   |
| `read_plan_sandbox(work_dir)`           | 435–437 | Public: reads `[plan_sandbox]` → `Option<SandboxConfig>` |
| `write_plan_sandbox(work_dir, sandbox)` | 440–442 | Public: writes `[plan_sandbox]`                          |

**Pattern to mirror for new sections:** add a `const SECTION_NAME: &str = "my_section"` at the top, then two public functions calling `read_section` / `write_section`.

### 6. Plan init — where config.toml sections are written

`loom/src/commands/init/plan_setup.rs`:

- `initialize_with_plan()` — lines 27–214
- `[plan]` table written via `toml_edit` at lines 139–157 (`work_dir::read_config` + `doc.insert("plan", ...)` + `work_dir::write_config`)
- `[plan_sandbox]` snapshot written at lines 160–162 (`work_dir::write_plan_sandbox`)

### 7. Run command entry points

- `loom/src/commands/run/mod.rs:31-84` — `execute_background()` — daemonizes orchestrator; calls `DaemonServer::with_config(...).start()`
- `loom/src/commands/run/foreground.rs:17-36` — `execute()` — public entry for `--foreground`; marks plan in-progress then calls `execute_foreground()`
- `loom/src/commands/run/foreground.rs:39-end` — `execute_foreground()` (private) — builds `OrchestratorConfig` (includes `sandbox_config: plan_sandbox` from `build_execution_graph`)

### 8. Crash handler — failure classification and retry

`loom/src/orchestrator/core/crash_handler.rs:15-137` — `handle_session_crashed()`:

- Line 49 — `classify_failure(&reason)` (from `orchestrator/retry.rs`)
- Line 65 — `should_auto_retry(&failure_type, stage.retry_count, max)` (default max = 3)
- Lines 90–110 — best-effort permission sync from crashed session's worktree
- Line 114 — `stage.try_mark_blocked()` → saves stage → marks graph `Blocked`

### 9. Claude binary resolution

`loom/src/claude.rs:10-33` — `find_claude_path() -> Result<PathBuf>`:

1. `which::which("claude")` (uses current PATH)
2. `~/.claude/local/claude` (official Claude Code install location)
3. `~/.local/bin/claude`
4. `~/.cargo/bin/claude`
5. `/usr/local/bin/claude`
6. `/opt/homebrew/bin/claude`

Used by all four NativeBackend spawn methods before building the claude command string.
