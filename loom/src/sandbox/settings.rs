use super::config::MergedSandboxConfig;
use crate::plan::schema::PermissionMode;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

mod policy;
pub(crate) use policy::validate_emittable;

/// Write Claude Code's `permissions.defaultMode` into a settings JSON value.
///
/// Uses the camelCase string Claude Code expects (e.g. `"acceptEdits"`,
/// `"bypassPermissions"`). This is the single place that maps loom's
/// kebab-case `PermissionMode` onto Claude's wire format.
pub fn apply_default_mode(settings: &mut Value, mode: PermissionMode) -> Result<()> {
    let obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings must be a JSON object"))?;
    let permissions = obj
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions must be a JSON object"))?;
    permissions.insert("defaultMode".to_string(), json!(mode.as_settings_value()));
    Ok(())
}

/// Detect whether a settings target is a loom worktree (vs. the main repo root).
///
/// Loom worktrees always live at `<repo>/.worktrees/<stage-id>/` and carry a
/// `.work` symlink into the main repo's shared state; the main repo root has
/// neither. This distinction decides whether worktree-relative escape rules
/// (`../../**`, `../.worktrees/**`) are meaningful: inside a worktree `../..`
/// is the repo root (the intended isolation boundary), but at the repo root
/// `../..` is the repo's parent — typically `$HOME`.
fn target_is_worktree(target: &Path) -> bool {
    if target.components().any(|c| c.as_os_str() == ".worktrees") {
        return true;
    }
    // Fallback: a worktree's `.work` is a symlink; the main repo's is a real dir.
    std::fs::symlink_metadata(target.join(".work"))
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Whether a filesystem deny path is a worktree-relative escape rule.
///
/// These (`../../**`, `../.worktrees/**`, …) are resolved relative to the
/// directory holding `settings.local.json`. They isolate a worktree from its
/// repo, but are nonsensical — and actively harmful — at the main repo root,
/// where `../..` resolves to `$HOME`.
fn is_worktree_escape_path(path: &str) -> bool {
    let t = path.trim();
    t.starts_with("..") || t.contains("../") || t.contains(".worktrees")
}

/// Drop worktree-relative escape rules from a config destined for the MAIN repo.
///
/// At the repo root `../../**` resolves to `$HOME`, so emitting it as a deny
/// rule blocks reads/writes across the entire home directory — breaking git
/// (`~/.gitconfig`) and any home-dir tooling. Worktree isolation is meaningless
/// for the main checkout, so these entries must not be written there. Worktree
/// targets keep them (generated relative to the worktree, where they are
/// correct), and isolation is independently enforced by the worktree hooks.
fn strip_worktree_escape_denies(config: &mut MergedSandboxConfig) {
    config
        .filesystem
        .deny_read
        .retain(|p| !is_worktree_escape_path(p));
    config
        .filesystem
        .deny_write
        .retain(|p| !is_worktree_escape_path(p));
}

/// Write Claude Code settings.local.json to worktree .claude/ directory
pub fn write_settings(config: &MergedSandboxConfig, worktree_path: &Path) -> Result<()> {
    policy::validate_emittable(config)?;
    let claude_dir = worktree_path.join(".claude");

    // Create .claude/ directory if it doesn't exist
    fs::create_dir_all(&claude_dir)
        .with_context(|| format!("Failed to create .claude directory at {:?}", claude_dir))?;

    let settings_path = claude_dir.join("settings.local.json");

    // Read existing settings if they exist
    let existing_settings = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)
            .with_context(|| format!("Failed to read existing settings at {:?}", settings_path))?;
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse existing settings at {:?}", settings_path))?
    } else {
        json!({})
    };

    let mut config = config.clone();

    // Worktree-relative escape rules (`../../**`, `../.worktrees/**`) are only
    // valid inside a worktree, where `../..` is the repo root. At the main repo
    // root `../..` is `$HOME`, so writing them there denies reads/writes across
    // the whole home directory (breaking git's `~/.gitconfig`). Strip them when
    // the target is the main repo; worktree targets keep them. This guards every
    // main-repo caller at once: `loom repair --fix` and knowledge-stage spawns.
    let is_worktree = target_is_worktree(worktree_path);
    if !is_worktree {
        strip_worktree_escape_denies(&mut config);
    }

    // Generate new sandbox settings
    let mut settings_json = generate_settings_json(&config);

    // Resolve the .work symlink to its absolute target path.
    // In worktrees, .work is a symlink to ../../.work (the main repo's .work/).
    // Claude Code resolves symlinks before checking permission patterns, so
    // the relative Read(.work/**) pattern doesn't match the resolved absolute
    // path (which is outside the worktree boundary). Adding the resolved
    // absolute paths ensures reads/writes are auto-allowed without prompting.
    //
    // SECURITY (S-1): the broad `Read(/{resolved}/**)` + `Edit(/{resolved}/**)`
    // allow used to expose `.work/admin.token` (Admin RPC capability) and
    // `.work/user.token` (User capability) to a sandboxed worktree agent —
    // privilege escalation across the daemon's RPC trust boundary. We now:
    //   1. emit explicit `deny` rules for the resolved-absolute token paths
    //      *before* the allow (the relative forms come from default_deny_read);
    //   2. narrow the broad allow from `/**` down to read-only orchestration
    //      state plus handoff writes. Memory and dispute state are daemon-owned,
    //      so direct file-tool writes must never be authorized.
    //
    // IMPORTANT: Claude Code requires the // prefix for absolute filesystem paths.
    // A single / means "relative to project root", NOT absolute. See:
    // https://code.claude.com/docs/en/permissions.md
    let work_link = worktree_path.join(".work");
    if work_link.exists() || work_link.is_symlink() {
        if let Ok(resolved) = work_link.canonicalize() {
            let resolved_str = resolved
                .to_str()
                .context("Resolved .work path is not valid UTF-8")?;
            if let Some(deny_read) = settings_json
                .pointer_mut("/sandbox/filesystem/denyRead")
                .and_then(Value::as_array_mut)
            {
                for token in ["admin.token", "user.token"] {
                    let deny_path = format!("/{resolved_str}/{token}");
                    if !deny_read.iter().any(|value| value == &deny_path) {
                        deny_read.push(json!(deny_path));
                    }
                }
            }
            if let Some(permissions) = settings_json.get_mut("permissions") {
                // Deny the resolved-absolute token paths first so deny wins over
                // any (current or future) allow that might match the .work root.
                if let Some(deny) = permissions.get_mut("deny") {
                    if let Some(deny_arr) = deny.as_array_mut() {
                        for token in ["admin.token", "user.token"] {
                            let deny_perm = format!("Read(/{}/{})", resolved_str, token);
                            if !deny_arr.iter().any(|v| v.as_str() == Some(&deny_perm)) {
                                deny_arr.push(json!(deny_perm));
                            }
                        }
                    }
                }
                if let Some(allow) = permissions.get_mut("allow") {
                    if let Some(allow_arr) = allow.as_array_mut() {
                        // Narrowed allow: config and orchestration state are
                        // read-only. Handoffs are the sole direct write root;
                        // memory and disputes are written through daemon RPCs.
                        let mut perms = vec![
                            format!("Read(/{}/config.toml)", resolved_str),
                            format!("Read(/{}/signals/**)", resolved_str),
                        ];
                        for sub in ["handoffs", "disputes", "memory"] {
                            perms.push(format!("Read(/{}/{}/**)", resolved_str, sub));
                        }
                        // NOTE: Claude Code's file permission check consults only
                        // `Edit(path)` rules — a `Write(path)` rule parses but is
                        // silently ignored (see doc/loom/knowledge/concerns.md
                        // "Per-Stage Sandbox `Write(path)` Rules Are Inert").
                        perms.push(format!("Edit(/{}/handoffs/**)", resolved_str));
                        for perm in perms {
                            if !allow_arr.iter().any(|v| v.as_str() == Some(&perm)) {
                                allow_arr.push(json!(perm));
                            }
                        }
                    }
                }
            }
        }
    }

    // Merge existing permissions into the new settings
    merge_existing_permissions(&mut settings_json, &existing_settings, is_worktree);

    // Carry forward top-level keys loom doesn't own (e.g. plugin enablement)
    // that would otherwise be discarded by the from-scratch regeneration above.
    // Gated on the codex lane, but only for worktree targets (see
    // `preserve_unowned_keys` doc comment).
    preserve_unowned_keys(&mut settings_json, &existing_settings, &config, is_worktree);

    // Write settings file with pretty formatting
    let settings_string = serde_json::to_string_pretty(&settings_json)
        .context("Failed to serialize settings JSON")?;

    fs::write(&settings_path, settings_string)
        .with_context(|| format!("Failed to write settings to {:?}", settings_path))?;

    Ok(())
}

/// Generate Claude Code settings JSON from sandbox config
pub fn generate_settings_json(config: &MergedSandboxConfig) -> Value {
    let mut settings = json!({});
    let deny_read = policy::deny_read_patterns(config);
    settings["sandbox"] = policy::sandbox_settings(config);

    // Build permissions block for file tool restrictions (Read/Write/Edit prompting)
    // These still work for prompting even though they don't provide OS-level isolation
    let mut permissions = json!({});
    let mut deny: Vec<Value> = Vec::new();
    let mut allow: Vec<Value> = Vec::new();

    // Add deny_read paths (prompts before allowing Read tool on these)
    //
    // IMPORTANT: Filter out parent-traversal paths (../) from deny_read.
    // Claude Code leaks permissions.deny entries into the OS-level sandbox
    // (macOS sandbox-exec). Parent-traversal paths like ../../** get resolved
    // relative to the project root — from /Users/foo/src/project, ../../**
    // resolves to /Users/foo/**, blocking reads to the ENTIRE home directory.
    // This breaks git (~/.gitconfig) and zsh (~/.claude/shell-snapshots/).
    // Write-side parent-traversal in permissions.deny is harmless because
    // the write sandbox already uses allowOnly with a narrow list.
    for path in &deny_read {
        deny.push(json!(format!("Read({})", path)));
    }

    // Add deny_write paths (prompts before allowing Write/Edit tools on these).
    //
    // IMPORTANT: emitted as `Edit(...)`, not `Write(...)`. Claude Code's file
    // permission check consults only `Edit(path)` rules — `Write(path)` parses,
    // prints a startup warning, and is then silently ignored, so a `Write(...)`
    // deny here would permit exactly what it was written to block. See
    // doc/loom/knowledge/concerns.md § "Per-Stage Sandbox `Write(path)` Rules
    // Are Inert". A blanket `Edit(**)` deny must NEVER be added here alongside
    // a narrower `Edit(<dir>/**)` allow below — deny wins, so it would block the
    // very directory the allow was meant to open.
    //
    // IMPORTANT: filter out parent-traversal paths (../), same as deny_read
    // above. Permission patterns resolve relative to the settings file's own
    // directory — inside a worktree that's `.worktrees/<stage-id>/`, so
    // `../../**` resolves to an ancestor (the repo root or `.worktrees/`), and
    // Claude Code's `**` crosses path separators, so the pattern matches the
    // worktree's OWN files (e.g. `<repo>/.worktrees/<stage>/loom/src/foo.rs`).
    // A `../`-relative deny is therefore effectively blanket and must never be
    // emitted as an enforceable `Edit` rule — deny wins over allow, so it would
    // refuse the agent's very first edit to its own source tree. Dropping it
    // here loses no protection: worktree write-escape is enforced independently
    // by the OS sandbox's `allowOnly` list, `hooks/worktree-file-guard.sh`, and
    // `hooks/worktree-isolation.sh`.
    for path in &config.filesystem.deny_write {
        if path.contains("../") {
            continue;
        }
        deny.push(json!(format!("Edit({})", path)));
    }

    // Add allow_write paths as exceptions (same Write->Edit reasoning as above).
    for path in &config.filesystem.allow_write {
        allow.push(json!(format!("Edit({})", path)));
    }

    // Add narrow Read/Edit permissions for orchestration state files agents
    // need. These are the *relative* forms; `write_settings` adds matching
    // resolved-absolute forms because `.work` is a symlink that Claude Code
    // resolves before matching. The set is deliberately scoped to the subdirs
    // an agent legitimately touches — never the bare `.work/**` that would also
    // expose `.work/admin.token` / `.work/user.token` (see S-1, default_deny_read).
    allow.push(json!("Read(.work/config.toml)"));
    allow.push(json!("Read(.work/signals/**)"));
    allow.push(json!("Read(.work/handoffs/**)"));
    allow.push(json!("Edit(.work/handoffs/**)"));
    allow.push(json!("Read(.work/disputes/**)"));
    allow.push(json!("Read(.work/memory/**)"));

    if !allow.is_empty() {
        permissions["allow"] = json!(allow);
    }
    if !deny.is_empty() {
        permissions["deny"] = json!(deny);
    }
    if permissions.as_object().is_some_and(|o| !o.is_empty()) {
        settings["permissions"] = permissions;
    }

    // Always emit defaultMode so Claude Code uses the resolved permission mode
    // for this stage rather than its built-in default.
    apply_default_mode(&mut settings, config.permission_mode)
        .expect("settings is a JSON object built above");

    // Disable Claude Code's own worktree isolation for this session.
    //
    // Loom already runs each stage inside its own git worktree
    // (.worktrees/<stage-id>/). Claude Code's default bgIsolation ("worktree")
    // blocks Edit/Write in the checkout until EnterWorktree is called, which
    // would push subagents into *nested* worktrees on top of loom's — creating
    // stray branches and a tangle of checkouts. "none" lets the session and its
    // subagents edit the loom worktree directly, which is exactly what loom
    // expects. (Claude Code v2.1.143+; older versions ignore the key.)
    settings["worktree"] = json!({ "bgIsolation": "none" });

    settings
}

/// Merge existing permissions from an old settings file into new settings
///
/// Only `permissions.deny` is merged forward from the existing file -
/// `permissions.allow` is intentionally NOT carried forward; it is always
/// regenerated purely from `config` (see `generate_settings_json`).
/// sandbox/network/linux config also always comes from the generator.
///
/// SECURITY: `permissions.allow` used to be merged the same way `deny` is
/// here (union + dedup with the existing file). That was a self-granted,
/// persistent permission escalation: `.claude/settings.local.json` lives
/// inside the worktree the stage agent is writing to, and worktrees are
/// REUSED across respawn / retry / crash recovery
/// (`orchestrator/core/stage_executor.rs` via `git::get_or_create_worktree`).
/// An agent could append an `Edit(...)` (or, before the Write->Edit fix,
/// `Write(...)`) entry to its own `allow` list and have it survive into its
/// next session, granting itself a permission loom never authorized. There is
/// no reliable way to tell a legitimately user-approved entry apart from a
/// self-inserted lookalike from the file's contents alone, so the safe fix is
/// to stop trusting `allow` from disk at all: it is deterministically rebuilt
/// from `config` on every write. This does mean a permission a human manually
/// approves mid-session is NOT preserved across the next respawn - an accepted
/// tradeoff for closing the escalation path. `deny` carry-forward is safe to
/// keep because widening `deny` can only narrow what the agent can do, never
/// grant it anything.
///
/// Uses HashSet for deduplication to avoid duplicate permissions in the merged result.
///
/// `is_worktree` indicates whether the destination settings file lives inside a
/// loom worktree. When false (the main repo root), stale worktree-relative
/// escape entries carried over from a prior file (e.g. `Write(../../**)` written
/// by an older loom version) are dropped, because at the repo root `../..` is
/// `$HOME` and such a rule would deny writes across the entire home directory.
fn merge_existing_permissions(
    new_settings: &mut Value,
    existing_settings: &Value,
    is_worktree: bool,
) {
    // Extract existing permissions if they exist
    let existing_permissions = existing_settings.get("permissions");
    if existing_permissions.is_none() || existing_permissions.unwrap().is_null() {
        return; // No permissions to merge
    }

    let existing_deny = existing_permissions
        .and_then(|p| p.get("deny"))
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    // Get or create permissions block in new settings
    let new_permissions = new_settings
        .as_object_mut()
        .and_then(|obj| obj.get_mut("permissions"))
        .and_then(|p| p.as_object_mut());

    if new_permissions.is_none() {
        return; // New settings has no permissions block, nothing to merge into
    }

    let new_permissions = new_permissions.unwrap();

    // Merge deny permissions
    if !existing_deny.is_empty() {
        let new_deny = new_permissions
            .entry("deny")
            .or_insert_with(|| json!([]))
            .as_array_mut();

        if let Some(new_deny_arr) = new_deny {
            // Collect all permissions into a HashSet for deduplication
            let mut all_deny: HashSet<String> = new_deny_arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();

            // Add existing permissions, filtering out stale entries that
            // would be harmful if leaked into the OS sandbox:
            // - Read() entries with parent-traversal (../) resolve too broadly
            // - Read() entries with absolute home paths from old loom versions
            //   get mangled by Claude Code (project root prepended)
            for perm in existing_deny {
                if perm.starts_with("Read(") && (perm.contains("../") || perm.starts_with("Read(/"))
                {
                    continue;
                }
                // For the MAIN repo (not a worktree), also drop worktree-relative
                // escape rules on the write side and any cross-worktree refs: at
                // the repo root `../..` is `$HOME`, so a stale `Write(../../**)`
                // would deny writes across the entire home directory.
                if !is_worktree && (perm.contains("../") || perm.contains(".worktrees")) {
                    continue;
                }
                all_deny.insert(perm);
            }

            // Replace array with deduplicated permissions
            *new_deny_arr = all_deny.into_iter().map(|s| json!(s)).collect();
        }
    }
}

/// Top-level settings keys that `generate_settings_json` does not emit and
/// that must therefore be carried forward from the existing file, or they
/// are silently dropped on every regeneration.
const PRESERVED_SETTINGS_KEYS: [&str; 2] = ["enabledPlugins", "extraKnownMarketplaces"];

/// Carry forward top-level settings keys loom does not own.
///
/// `generate_settings_json` rebuilds the file from scratch, so any
/// key it does not emit is lost. Plugin enablement lives in
/// `enabledPlugins` / `extraKnownMarketplaces` and must survive.
///
/// SECURITY: the escalation this guards against is a stage AGENT writing its
/// own `enabledPlugins` entry into its own (agent-writable, respawn-reused)
/// worktree settings file and having loom carry that self-grant into the
/// next session - the same self-granted-persistence hole
/// `merge_existing_permissions` had for `permissions.allow` (see its doc
/// comment). That hole exists ONLY for worktree targets: the settings file is
/// not agent-writable at the MAIN repo root. Both writers that target it
/// hardcode or discourage the codex lane rather than run whatever a worktree
/// agent chose - `loom repair --fix` (`commands/repair.rs::fix_sandbox_settings`)
/// passes `&Implementers::default()` (claude-only) unconditionally, and the
/// knowledge-stage spawn path (`stage_executor.rs::start_knowledge_stage`)
/// writes the stage's own implementers, which plan validation warns against
/// setting to codex in the first place - so there is no self-grant to defend
/// against there, and the codex-license gate must not apply. Applying it
/// anyway silently DELETES a legitimate main-repo plugin install (e.g. the
/// codex marketplace) on every `loom repair --fix`, because `write_settings`
/// regenerates the whole file from scratch.
///
/// So: preserve these keys UNCONDITIONALLY for a non-worktree target, and
/// gate on `config.implementers.includes_codex()` only when `is_worktree` is
/// true - a claude-only WORKTREE stage has no legitimate reason to carry
/// either key, so there is nothing for it to self-grant. `generated always
/// wins` still holds - the loop below only fills in keys `new_settings`
/// doesn't already contain.
fn preserve_unowned_keys(
    new_settings: &mut Value,
    existing: &Value,
    config: &MergedSandboxConfig,
    is_worktree: bool,
) {
    if is_worktree && !config.implementers.includes_codex() {
        return;
    }
    let Some(existing_obj) = existing.as_object() else {
        return;
    };
    let Some(new_obj) = new_settings.as_object_mut() else {
        return;
    };
    for key in PRESERVED_SETTINGS_KEYS {
        if new_obj.contains_key(key) {
            continue;
        }
        if let Some(value) = existing_obj.get(key) {
            new_obj.insert(key.to_string(), value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::{Implementer, Implementers};
    use crate::plan::schema::{
        CommandConfinement, FilesystemConfig, LinuxConfig, NetworkConfig, SandboxConfig,
        StageSandboxConfig, StageType,
    };
    use crate::sandbox::merge_config;

    fn default_config() -> MergedSandboxConfig {
        MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        }
    }

    /// A stage config that licenses the codex lane - the gate
    /// `preserve_unowned_keys` needs before it will carry `enabledPlugins` /
    /// `extraKnownMarketplaces` forward from an existing settings file.
    fn codex_licensed_config() -> MergedSandboxConfig {
        let mut config = default_config();
        config.implementers = Implementers::new(vec![Implementer::Codex]);
        config
    }

    #[test]
    fn test_apply_default_mode_matrix() {
        // Each PermissionMode → camelCase string emitted into settings JSON.
        let cases = [
            (PermissionMode::Default, "default"),
            (PermissionMode::AcceptEdits, "acceptEdits"),
            (PermissionMode::Auto, "auto"),
            (PermissionMode::Plan, "plan"),
            (PermissionMode::BypassPermissions, "bypassPermissions"),
        ];
        for (mode, expected) in cases {
            let mut settings = json!({});
            apply_default_mode(&mut settings, mode).unwrap();
            assert_eq!(
                settings["permissions"]["defaultMode"],
                json!(expected),
                "mode {mode:?} should serialize to {expected}"
            );
        }
    }

    #[test]
    fn test_apply_default_mode_preserves_existing_permissions() {
        let mut settings = json!({
            "permissions": {
                "allow": ["Read(.work/**)"]
            }
        });
        apply_default_mode(&mut settings, PermissionMode::Plan).unwrap();
        assert_eq!(settings["permissions"]["defaultMode"], json!("plan"));
        assert_eq!(
            settings["permissions"]["allow"],
            json!(["Read(.work/**)"]),
            "Existing allow list must be preserved"
        );
    }

    #[test]
    fn test_permission_mode_kebab_case_round_trip() {
        // Round-trip through serde_yaml using kebab-case spelling.
        for (mode, kebab) in [
            (PermissionMode::Default, "default"),
            (PermissionMode::AcceptEdits, "accept-edits"),
            (PermissionMode::Auto, "auto"),
            (PermissionMode::Plan, "plan"),
            (PermissionMode::BypassPermissions, "bypass-permissions"),
        ] {
            let yaml = serde_yaml::to_string(&mode).unwrap();
            assert!(yaml.contains(kebab), "{mode:?} should serialize to {kebab}");
            let back: PermissionMode = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(back, mode);
        }
    }

    #[test]
    fn test_generate_settings_json_includes_resolved_default_mode() {
        // Each PermissionMode in MergedSandboxConfig becomes the camelCase
        // permissions.defaultMode in the generated settings JSON.
        for (mode, expected) in [
            (PermissionMode::Default, "default"),
            (PermissionMode::AcceptEdits, "acceptEdits"),
            (PermissionMode::Auto, "auto"),
            (PermissionMode::Plan, "plan"),
            (PermissionMode::BypassPermissions, "bypassPermissions"),
        ] {
            let config = MergedSandboxConfig {
                enabled: true,
                auto_allow: true,
                allow_unsandboxed_escape: false,
                excluded_commands: vec![],
                filesystem: FilesystemConfig::default(),
                network: NetworkConfig::default(),
                linux: LinuxConfig::default(),
                permission_mode: mode,
                implementers: Implementers::default(),
                command_confinement: CommandConfinement::default(),
            };
            let json = generate_settings_json(&config);
            assert_eq!(
                json["permissions"]["defaultMode"],
                json!(expected),
                "generate_settings_json must emit camelCase for {mode:?}"
            );
        }
    }

    #[test]
    fn test_generate_settings_disables_worktree_isolation() {
        // Loom owns the worktree, so Claude Code's bgIsolation must be "none"
        // to keep subagents from spawning nested worktrees/branches.
        let config = default_config();

        let json = generate_settings_json(&config);
        assert_eq!(json["worktree"]["bgIsolation"], json!("none"));
    }

    #[test]
    fn test_generate_settings_disabled() {
        let mut config = default_config();
        config.enabled = false;

        let json = generate_settings_json(&config);
        // Sandbox block should have enabled: false
        assert_eq!(json["sandbox"]["enabled"], false);
    }

    #[test]
    fn test_generate_settings_with_filesystem() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                deny_read: vec!["~/.ssh/**".to_string(), "../../**".to_string()],
                deny_write: vec![".work/**".to_string()],
                allow_write: vec!["src/**".to_string()],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        let json = generate_settings_json(&config);
        assert_filesystem_sandbox(&json);

        // Permissions for file tool restrictions
        // Parent-traversal deny_read paths (../../**) are filtered out because
        // Claude Code leaks them into the OS sandbox where they resolve too broadly
        let deny = json["permissions"]["deny"].as_array().unwrap();
        assert_eq!(deny.len(), 3);
        assert_eq!(deny[0], "Read(~/.ssh/**)");
        assert_eq!(deny[1], "Read(~/.claude/.credentials.json)");
        assert_eq!(deny[2], "Edit(.work/**)");

        // allow_write paths come first, then the narrowly-scoped .work/ state
        // permissions agents need (signals/handoffs/disputes/memory). The set is
        // deliberately scoped to subdirs an agent touches — never bare `.work/**`,
        // which would also expose `.work/admin.token` / `.work/user.token` (S-1).
        let allow = json["permissions"]["allow"].as_array().unwrap();
        assert_eq!(allow.len(), 7);
        assert_eq!(allow[0], "Edit(src/**)");
        assert_eq!(allow[1], "Read(.work/config.toml)");
        assert_eq!(allow[2], "Read(.work/signals/**)");
        assert_eq!(allow[3], "Read(.work/handoffs/**)");
        assert_eq!(allow[4], "Edit(.work/handoffs/**)");
        assert_eq!(allow[5], "Read(.work/disputes/**)");
        assert_eq!(allow[6], "Read(.work/memory/**)");
    }

    fn assert_filesystem_sandbox(json: &Value) {
        assert_eq!(json["sandbox"]["enabled"], true);
        assert_eq!(json["sandbox"]["failIfUnavailable"], true);
        assert_eq!(json["sandbox"]["autoAllowBashIfSandboxed"], true);
        let fs_block = &json["sandbox"]["filesystem"];
        let deny_read = fs_block["denyRead"].as_array().unwrap();
        assert!(deny_read.iter().any(|value| value == "~/.ssh/**"));
        assert!(deny_read
            .iter()
            .any(|value| value == "~/.claude/.credentials.json"));
        // The plan's own allow_write reaches the OS-enforced allowWrite grant.
        // This config is claude-only, so no codex state paths are appended.
        assert_eq!(fs_block["allowWrite"], json!(["src/**"]));
        let deny_write = fs_block["denyWrite"].as_array().unwrap();
        assert_eq!(deny_write.len(), 1);
        assert_eq!(deny_write[0], ".work/**");
    }

    #[test]
    fn test_generate_settings_with_network() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig {
                allowed_domains: vec!["*.github.com".to_string()],
                additional_domains: vec!["api.example.com".to_string()],
                allow_local_binding: true,
                allow_unix_sockets: vec!["/tmp/*.sock".to_string()],
                allow_all_unix_sockets: false,
            },
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        let json = generate_settings_json(&config);

        // Network config is now in sandbox.network block
        let network = &json["sandbox"]["network"];
        let domains = network["allowedDomains"].as_array().unwrap();
        // Claude-only stage: exactly the plan's own domains, no codex lane hosts.
        assert_eq!(domains.len(), 2);
        assert!(domains.iter().any(|d| d == "*.github.com"));
        assert!(domains.iter().any(|d| d == "api.example.com"));
        assert!(
            !domains.iter().any(|d| d == "chatgpt.com"),
            "claude-only stage must not receive codex domains, got: {:?}",
            domains
        );
        assert_eq!(network["allowLocalBinding"], true);
        let sockets = network["allowUnixSockets"].as_array().unwrap();
        assert_eq!(sockets.len(), 1);
        assert_eq!(sockets[0], "/tmp/*.sock");
    }

    #[test]
    fn test_generate_settings_with_linux_config() {
        let mut config = default_config();
        config.linux.enable_weaker_nested = true;

        let json = generate_settings_json(&config);
        assert_eq!(json["sandbox"]["enableWeakerNestedSandbox"], true);
    }

    #[test]
    fn test_generate_settings_never_emits_excluded_commands() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec!["loom".to_string(), "git".to_string()],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        let json = generate_settings_json(&config);
        assert!(json["sandbox"]["excludedCommands"].is_null());

        let temp = tempfile::TempDir::new().unwrap();
        let error = write_settings(&config, temp.path())
            .unwrap_err()
            .to_string();
        assert!(error.contains("excluded_commands"));
        assert!(!temp.path().join(".claude/settings.local.json").exists());
    }

    #[test]
    fn test_default_sandbox_has_no_command_exclusions() {
        let config = SandboxConfig::default();
        assert!(config.excluded_commands.is_empty());
    }

    #[test]
    fn generated_stage_settings_deny_unix_sockets_for_completion_broker_integrity() {
        let plan = SandboxConfig::default();
        let stage = StageSandboxConfig::default();
        let config = merge_config(&plan, &stage, StageType::Standard, &Implementers::default());

        let json = generate_settings_json(&config);
        let network = &json["sandbox"]["network"];
        assert!(network["allowUnixSockets"].is_null());
        assert!(network["allowAllUnixSockets"].is_null());
    }

    #[test]
    fn test_generate_settings_with_unsandboxed_escape() {
        let mut config = default_config();
        config.allow_unsandboxed_escape = true;

        let json = generate_settings_json(&config);
        // allowUnsandboxedCommands is now in sandbox block
        assert_eq!(json["sandbox"]["allowUnsandboxedCommands"], true);
    }

    #[test]
    fn test_generate_settings_exclusions_never_get_bash_allow() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec!["loom".to_string(), "git".to_string()],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        let json = generate_settings_json(&config);
        let allow = json["permissions"]["allow"].as_array().unwrap();

        let allow_strs: Vec<&str> = allow.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(!allow_strs
            .iter()
            .any(|entry| entry.starts_with("Bash(loom")));
        assert!(!allow_strs.iter().any(|entry| entry.starts_with("Bash(git")));
    }

    #[test]
    fn test_generate_settings_includes_work_dir_read_allows() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        let json = generate_settings_json(&config);
        let allow = json["permissions"]["allow"].as_array().unwrap();

        let allow_strs: Vec<&str> = allow.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(
            allow_strs.contains(&"Read(.work/signals/**)"),
            "Should allow reading signals, got: {:?}",
            allow_strs
        );
        assert!(
            allow_strs.contains(&"Read(.work/handoffs/**)"),
            "Should allow reading handoffs, got: {:?}",
            allow_strs
        );
        assert!(
            allow_strs.contains(&"Read(.work/config.toml)"),
            "Should allow reading config, got: {:?}",
            allow_strs
        );
    }

    #[test]
    fn test_generate_settings_keeps_mandatory_credential_deny_when_empty() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                deny_read: vec![],
                deny_write: vec![],
                allow_write: vec![],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        let json = generate_settings_json(&config);
        assert_eq!(
            json["sandbox"]["filesystem"]["denyRead"],
            json!(["~/.claude/.credentials.json"])
        );
    }

    #[test]
    fn test_generate_settings_with_all_unix_sockets() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig {
                allowed_domains: vec![],
                additional_domains: vec![],
                allow_local_binding: false,
                allow_unix_sockets: vec![],
                allow_all_unix_sockets: true,
            },
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        let json = generate_settings_json(&config);
        assert_eq!(json["sandbox"]["network"]["allowAllUnixSockets"], true);
    }

    #[test]
    fn test_deny_read_is_enforced_in_os_sandbox() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                deny_read: vec![
                    "~/.ssh/**".to_string(),
                    "../../**".to_string(),
                    "../.worktrees/**".to_string(),
                ],
                deny_write: vec![],
                allow_write: vec![],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        let json = generate_settings_json(&config);

        let os_deny = json["sandbox"]["filesystem"]["denyRead"]
            .as_array()
            .unwrap();
        let os_deny_strs: Vec<&str> = os_deny.iter().filter_map(|v| v.as_str()).collect();
        assert!(os_deny_strs.contains(&"~/.ssh/**"));
        assert!(os_deny_strs.contains(&"~/.claude/.credentials.json"));
        assert!(!os_deny_strs.contains(&"../../**"));
        assert!(!os_deny_strs.contains(&"../.worktrees/**"));

        // permissions.deny should have non-traversal paths only
        let deny = json["permissions"]["deny"].as_array().unwrap();
        let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
        assert!(deny_strs.contains(&"Read(~/.ssh/**)"));
        // Parent-traversal paths must NOT be in permissions.deny because Claude Code
        // leaks them into the OS sandbox where they resolve too broadly
        assert!(
            !deny_strs.contains(&"Read(../../**)"),
            "../../** must NOT be in permissions.deny Read() (leaks into OS sandbox)"
        );
        assert!(
            !deny_strs.contains(&"Read(../.worktrees/**)"),
            "../.worktrees/** must NOT be in permissions.deny Read() (leaks into OS sandbox)"
        );
    }

    #[test]
    fn test_deny_write_parent_traversal_not_in_os_sandbox() {
        // Parent-traversal paths (../) in deny_write must NOT appear in
        // sandbox.filesystem.denyWrite OR in permissions.deny Edit(...).
        // Both layers resolve `../` relative to the project root, causing
        // overly broad restrictions:
        // - From worktrees: "../../**" resolves to an ancestor and, because
        //   `**` crosses separators, matches the worktree's OWN files —
        //   an enforceable `Edit(../../**)` deny would refuse the agent's
        //   first edit to its own source tree.
        // - From the main repo root: "../../**" is $HOME.
        // Worktree write-escape is enforced independently (OS sandbox
        // allowOnly list + worktree hooks), so dropping the tool-layer rule
        // loses no protection. Non-traversal paths (e.g. the knowledge dir)
        // still need to reach permissions.deny to be enforceable at all.
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                deny_read: vec![],
                deny_write: vec!["../../**".to_string(), "doc/loom/knowledge/**".to_string()],
                allow_write: vec![],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        let json = generate_settings_json(&config);

        // OS sandbox denyWrite must NOT contain parent-traversal paths or knowledge paths
        // Both are filtered: parent-traversal resolves too broadly in sandbox-exec,
        // and knowledge paths block `loom knowledge update` CLI (excludedCommands
        // doesn't bypass OS-level filesystem restrictions).
        assert!(json["sandbox"]["filesystem"]["denyWrite"].is_null());
        // This config's allow_write is empty and the stage is claude-only, so
        // there is nothing to grant: `allowWrite` is omitted entirely (see
        // `test_generate_settings_plan_allow_write_reaches_os_sandbox_claude_only`
        // for the case where a plan's allow_write DOES reach the OS layer).
        let fs_block = &json["sandbox"]["filesystem"];
        assert!(fs_block["allowWrite"].is_null());

        // permissions.deny should have the non-traversal path only; the
        // parent-traversal entry must be filtered, or it would deny-match the
        // worktree's own files once Claude Code enforces `Edit(...)` rules.
        let deny = json["permissions"]["deny"].as_array().unwrap();
        let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
        assert!(
            !deny_strs.contains(&"Edit(../../**)"),
            "Parent-traversal must NOT be in permissions.deny \
             (matches the worktree's own files, deny wins over allow)"
        );
        assert!(
            deny_strs.contains(&"Edit(doc/loom/knowledge/**)"),
            "Project-relative should still be in permissions.deny"
        );
    }

    #[test]
    fn test_generate_settings_plan_allow_write_reaches_os_sandbox_claude_only() {
        // This is the end-to-end proof for the previously-inert lever: a plan's
        // `filesystem.allow_write` now reaches `sandbox.filesystem.allowWrite`
        // (the OS-enforced grant), even for a claude-only stage that never
        // licenses the codex lane. See doc/loom/knowledge/mistakes/sandbox-and-settings.md
        // § "A Plan's `allow_write` Cannot Grant a Subprocess OS-Level Write Access".
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                deny_read: vec![],
                deny_write: vec![],
                allow_write: vec!["tmp/tmux-sockets/**".to_string()],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(), // claude-only
            command_confinement: CommandConfinement::default(),
        };
        assert!(!config.implementers.includes_codex());

        let json = generate_settings_json(&config);

        assert_eq!(
            json["sandbox"]["filesystem"]["allowWrite"],
            json!(["tmp/tmux-sockets/**"]),
            "a claude-only stage's own allow_write must reach the OS sandbox, \
             with no codex state paths appended, got: {json:?}"
        );
    }

    #[test]
    fn test_generate_settings_emits_network_block() {
        // The native backend emits the sandbox.network block whenever the
        // sandbox is enabled (strictAllowlist), regardless of domain content.
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                deny_read: vec![],
                deny_write: vec!["doc/loom/knowledge/**".to_string()],
                allow_write: vec![],
            },
            network: NetworkConfig {
                allowed_domains: vec!["github.com".to_string(), "api.github.com".to_string()],
                additional_domains: vec![],
                allow_local_binding: true,
                allow_unix_sockets: vec![],
                allow_all_unix_sockets: false,
            },
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        let json = generate_settings_json(&config);
        let network = &json["sandbox"]["network"];
        assert!(
            !network.is_null(),
            "sandbox.network must be emitted when allowed_domains is set"
        );
        let domains = network["allowedDomains"]
            .as_array()
            .expect("allowedDomains must be present");
        // Claude-only stage: exactly the plan's own domains, no codex lane hosts.
        assert_eq!(domains.len(), 2);
        assert!(domains.iter().any(|d| d == "github.com"));
        assert!(
            !domains.iter().any(|d| d == "api.openai.com"),
            "claude-only stage must not receive codex domains, got: {:?}",
            domains
        );
        assert_eq!(network["allowLocalBinding"], true);
        assert_eq!(network["strictAllowlist"], json!(true));

        // Filesystem deny entries are emitted alongside the network block.
        let deny = json["permissions"]["deny"]
            .as_array()
            .expect("filesystem deny entries should be present");
        let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
        assert!(deny_strs.contains(&"Edit(doc/loom/knowledge/**)"));
    }

    #[test]
    fn test_no_path_in_both_allow_and_deny() {
        use crate::plan::schema::{SandboxConfig, StageSandboxConfig, StageType};
        use crate::sandbox::merge_config;

        // Test all stage types
        for stage_type in [
            StageType::Standard,
            StageType::Knowledge,
            StageType::IntegrationVerify,
            StageType::KnowledgeDistill,
        ] {
            let plan = SandboxConfig::default();
            let stage = StageSandboxConfig::default();
            let merged = merge_config(&plan, &stage, stage_type, &Implementers::default());
            let json = generate_settings_json(&merged);

            let permissions = &json["permissions"];
            if permissions.is_null() {
                continue;
            }

            let allow = permissions["allow"]
                .as_array()
                .map(|a| a.to_vec())
                .unwrap_or_default();
            let deny = permissions["deny"]
                .as_array()
                .map(|a| a.to_vec())
                .unwrap_or_default();

            // Compare full permission strings (e.g. "Read(.work/signals/**)")
            // to detect true conflicts where the same permission type + path
            // appears in both allow and deny.
            let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
            let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();

            for entry in &allow_strs {
                assert!(
                    !deny_strs.contains(entry),
                    "Stage type {:?}: '{}' appears in both allow and deny",
                    stage_type,
                    entry
                );
            }
        }
    }

    #[test]
    fn test_write_settings_preserves_existing_deny_but_not_allow() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path();

        // Create existing settings.local.json with permissions from a prior session.
        let claude_dir = worktree_path.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let settings_path = claude_dir.join("settings.local.json");

        let existing_settings = json!({
            "permissions": {
                "allow": [
                    "Read(~/.ssh/config)",
                    "Bash(docker:*)"
                ],
                "deny": [
                    "Write(~/.bashrc)"
                ]
            }
        });
        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&existing_settings).unwrap(),
        )
        .unwrap();

        // Now call write_settings with sandbox config
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                deny_read: vec![],
                deny_write: vec![],
                allow_write: vec!["src/**".to_string()],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        write_settings(&config, worktree_path).unwrap();

        // Read the result
        let result_content = fs::read_to_string(&settings_path).unwrap();
        let result: Value = serde_json::from_str(&result_content).unwrap();

        // Verify sandbox-generated permissions are present
        let allow = result["permissions"]["allow"].as_array().unwrap();
        let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
        assert!(allow_strs.contains(&"Edit(src/**)"));
        assert!(allow_strs.contains(&"Read(.work/signals/**)"));

        // SECURITY: existing `allow` entries are NOT carried forward. Allow is
        // regenerated purely from config on every write - that is what stops a
        // stage agent from self-granting a persistent permission by writing it
        // into its own (agent-writable, respawn-reused) settings.local.json.
        assert!(!allow_strs.contains(&"Read(~/.ssh/config)"));
        assert!(!allow_strs.contains(&"Bash(docker:*)"));

        // `deny` is still merged forward - widening deny can only narrow what
        // the agent can do, never grant it anything, so carrying it forward
        // is safe (unlike `allow`).
        let deny = result["permissions"]["deny"].as_array().unwrap();
        let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
        assert!(deny_strs.contains(&"Write(~/.bashrc)"));
    }

    #[test]
    fn test_write_settings_does_not_carry_forward_existing_allow() {
        use tempfile::TempDir;

        // SECURITY regression test: `.claude/settings.local.json` lives inside
        // the agent-writable worktree, and worktrees are REUSED across respawn
        // / retry / crash recovery (`orchestrator/core/stage_executor.rs` via
        // `git::get_or_create_worktree`). If `permissions.allow` were merged
        // forward the way `deny` is, a stage agent could append an entry to its
        // own file and have it survive into its next session - including an
        // entry that widens what it can write.
        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path();

        let claude_dir = worktree_path.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let settings_path = claude_dir.join("settings.local.json");

        let existing_settings = json!({
            "permissions": {
                "allow": [
                    "Read(.work/signals/**)",  // overlaps a generated entry
                    "Read(custom/path/**)",    // an innocuous-looking extra grant
                    "Edit(../../**)"           // the escalation this test guards against
                ]
            }
        });
        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&existing_settings).unwrap(),
        )
        .unwrap();

        // Call write_settings
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        write_settings(&config, worktree_path).unwrap();

        // Read the result
        let result_content = fs::read_to_string(&settings_path).unwrap();
        let result: Value = serde_json::from_str(&result_content).unwrap();

        let allow = result["permissions"]["allow"].as_array().unwrap();
        let allow_strs: Vec<String> = allow
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        // The generated entry appears exactly once - regeneration never
        // duplicates its own output.
        let signal_count = allow_strs
            .iter()
            .filter(|s| *s == "Read(.work/signals/**)")
            .count();
        assert_eq!(
            signal_count, 1,
            "Read(.work/signals/**) should appear exactly once"
        );

        // Neither an innocuous-looking nor an escalating existing allow entry
        // survives regeneration.
        assert!(
            !allow_strs.contains(&"Read(custom/path/**)".to_string()),
            "existing allow entries must not be carried forward, got: {:?}",
            allow_strs
        );
        assert!(
            !allow_strs.contains(&"Edit(../../**)".to_string()),
            "a self-granted escalating allow entry must not survive, got: {:?}",
            allow_strs
        );
    }

    #[test]
    fn test_write_settings_no_existing_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path();

        // Call write_settings with no existing file
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                deny_read: vec!["~/.ssh/**".to_string(), "../../**".to_string()],
                deny_write: vec![],
                allow_write: vec!["src/**".to_string()],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        write_settings(&config, worktree_path).unwrap();

        // Read the result
        let settings_path = worktree_path.join(".claude/settings.local.json");
        let result_content = fs::read_to_string(&settings_path).unwrap();
        let result: Value = serde_json::from_str(&result_content).unwrap();

        // Verify expected permissions (same as before, no existing to merge)
        let allow = result["permissions"]["allow"].as_array().unwrap();
        let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
        assert!(allow_strs.contains(&"Edit(src/**)"));
        assert!(allow_strs.contains(&"Read(.work/signals/**)"));

        // permissions.deny includes non-traversal deny_read paths
        let deny = result["permissions"]["deny"].as_array().unwrap();
        let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
        assert!(deny_strs.contains(&"Read(~/.ssh/**)"));
        // Parent-traversal paths filtered out (leaked into OS sandbox otherwise)
        assert!(!deny_strs.contains(&"Read(../../**)"));

        let os_deny = result["sandbox"]["filesystem"]["denyRead"]
            .as_array()
            .unwrap();
        assert!(os_deny.iter().any(|value| value == "~/.ssh/**"));
        assert!(os_deny
            .iter()
            .any(|value| value == "~/.claude/.credentials.json"));
    }

    #[cfg(unix)]
    #[test]
    fn test_write_settings_adds_resolved_work_symlink_permissions() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        // Simulate the real layout: repo_root/.work and repo_root/.worktrees/stage/
        let work_dir = base.join(".work");
        fs::create_dir_all(&work_dir).unwrap();
        fs::create_dir_all(work_dir.join("signals")).unwrap();

        let worktree_path = base.join(".worktrees").join("my-stage");
        fs::create_dir_all(&worktree_path).unwrap();

        // Create the symlink: .worktrees/my-stage/.work -> ../../.work
        std::os::unix::fs::symlink("../../.work", worktree_path.join(".work")).unwrap();

        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        write_settings(&config, &worktree_path).unwrap();

        let settings_path = worktree_path.join(".claude/settings.local.json");
        let result_content = fs::read_to_string(&settings_path).unwrap();
        let result: Value = serde_json::from_str(&result_content).unwrap();

        let allow = result["permissions"]["allow"].as_array().unwrap();
        let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();

        let resolved_work = work_dir.canonicalize().unwrap();
        let resolved_str = resolved_work.to_string_lossy();

        // S-1: the broad `**` allow over the resolved .work root is GONE — it
        // would have exposed the daemon tokens. Narrowed subdir grants remain.
        let broad_read = format!("Read(/{}/**)", resolved_str);
        let broad_edit = format!("Edit(/{}/**)", resolved_str);
        assert!(
            !allow_strs.contains(&broad_read.as_str()),
            "broad Read(/.work/**) must NOT be granted, got: {:?}",
            allow_strs
        );
        assert!(
            !allow_strs.contains(&broad_edit.as_str()),
            "broad Edit(/.work/**) must NOT be granted, got: {:?}",
            allow_strs
        );

        // Narrowed resolved-absolute grants for the subdirs agents need
        // (signals/ supplies reads; handoffs/ supplies the EROFS write exemption).
        // Emitted as `Edit(...)`, not `Write(...)` — Claude Code's file
        // permission check only consults `Edit(path)` rules.
        let expected_read_signals = format!("Read(/{}/signals/**)", resolved_str);
        let expected_edit_handoffs = format!("Edit(/{}/handoffs/**)", resolved_str);
        assert!(
            allow_strs.contains(&expected_read_signals.as_str()),
            "Should have resolved .work/signals read permission, got: {:?}",
            allow_strs
        );
        assert!(
            allow_strs.contains(&expected_edit_handoffs.as_str()),
            "Should have resolved .work/handoffs edit permission (EROFS exemption), got: {:?}",
            allow_strs
        );

        // S-1: the daemon tokens must be explicitly denied in resolved-absolute form.
        let deny = result["permissions"]["deny"].as_array().unwrap();
        let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
        let deny_admin = format!("Read(/{}/admin.token)", resolved_str);
        let deny_user = format!("Read(/{}/user.token)", resolved_str);
        assert!(
            deny_strs.contains(&deny_admin.as_str()),
            "admin.token must be denied (resolved-absolute), got: {:?}",
            deny_strs
        );
        assert!(
            deny_strs.contains(&deny_user.as_str()),
            "user.token must be denied (resolved-absolute), got: {:?}",
            deny_strs
        );
        let os_deny = result["sandbox"]["filesystem"]["denyRead"]
            .as_array()
            .unwrap();
        let os_admin = format!("/{resolved_str}/admin.token");
        let os_user = format!("/{resolved_str}/user.token");
        assert!(os_deny.iter().any(|value| value == &os_admin));
        assert!(os_deny.iter().any(|value| value == &os_user));

        // Should also still have the relative permissions
        assert!(allow_strs.contains(&"Read(.work/signals/**)"));
    }

    #[test]
    fn test_merge_existing_permissions_empty() {
        // Existing file has no permissions block
        let existing = json!({
            "sandbox": {
                "enabled": true
            }
        });

        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                allow_write: vec!["src/**".to_string()],
                deny_read: vec![],
                deny_write: vec![],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        let mut new_settings = generate_settings_json(&config);
        let original_allow_count = new_settings["permissions"]["allow"]
            .as_array()
            .unwrap()
            .len();

        // Merge should be a no-op
        merge_existing_permissions(&mut new_settings, &existing, false);

        let after_allow_count = new_settings["permissions"]["allow"]
            .as_array()
            .unwrap()
            .len();
        assert_eq!(original_allow_count, after_allow_count);
    }

    #[test]
    fn test_target_is_worktree_detection() {
        // A `.worktrees` path component marks a worktree (no filesystem needed).
        assert!(target_is_worktree(Path::new(
            "/home/u/proj/.worktrees/stage-1"
        )));
        // A plain repo root is the main repo.
        assert!(!target_is_worktree(Path::new("/home/u/proj")));

        #[cfg(unix)]
        {
            use tempfile::TempDir;
            let temp_dir = TempDir::new().unwrap();
            let base = temp_dir.path();

            // A symlinked `.work` marks a worktree even without a `.worktrees`
            // component (the structural invariant loom relies on).
            let real_work = base.join("real-work");
            fs::create_dir_all(&real_work).unwrap();
            let wt = base.join("checkout");
            fs::create_dir_all(&wt).unwrap();
            std::os::unix::fs::symlink(&real_work, wt.join(".work")).unwrap();
            assert!(target_is_worktree(&wt));

            // A real `.work` directory is the main repo.
            let main = base.join("main");
            fs::create_dir_all(main.join(".work")).unwrap();
            assert!(!target_is_worktree(&main));
        }
    }

    #[test]
    fn test_write_settings_main_repo_strips_worktree_escape_denies() {
        use tempfile::TempDir;

        // A plain repo root (not under .worktrees, no `.work` symlink) is the main
        // repo: worktree-relative escape rules must be stripped, because `../..`
        // resolves to `$HOME` there and would deny the entire home directory.
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            // FilesystemConfig::default() includes ../../** and ../.worktrees/**.
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        write_settings(&config, repo_root).unwrap();

        let result: Value = serde_json::from_str(
            &fs::read_to_string(repo_root.join(".claude/settings.local.json")).unwrap(),
        )
        .unwrap();
        let deny = result["permissions"]["deny"].as_array().unwrap();
        let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();

        assert!(
            !deny_strs.iter().any(|p| p.contains("../")),
            "main repo deny must not contain parent-traversal rules, got: {deny_strs:?}"
        );
        assert!(
            !deny_strs.iter().any(|p| p.contains(".worktrees")),
            "main repo deny must not reference .worktrees, got: {deny_strs:?}"
        );
        // Non-traversal protections survive.
        assert!(deny_strs.contains(&"Read(~/.ssh/**)"));
        assert!(deny_strs.contains(&"Edit(doc/loom/knowledge/**)"));
    }

    #[test]
    fn test_write_settings_worktree_drops_escape_write_deny_from_edit_rule() {
        use tempfile::TempDir;

        // Even inside a real worktree (path under .worktrees/<stage>/), a
        // `../`-relative deny_write entry must NEVER be emitted as an
        // enforceable `Edit(...)` permission rule: `.worktrees/<stage>/../..`
        // resolves to an ancestor of the worktree, and Claude Code's `**`
        // crosses path separators, so `Edit(../../**)` matches the worktree's
        // OWN files (e.g. `<repo>/.worktrees/<stage>/loom/src/foo.rs`) and
        // deny wins over allow — refusing the agent's very first edit. This
        // WORKTREE-shaped config's `deny_write` (via `FilesystemConfig::default()`
        // -> `default_deny_write()`) contains both `../../**` and the
        // non-traversal `doc/loom/knowledge/**`: only the traversal entry is
        // dropped. Worktree write-escape is still enforced independently by
        // the OS sandbox's `allowOnly` list and the worktree hooks.
        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path().join(".worktrees").join("my-stage");
        fs::create_dir_all(&worktree_path).unwrap();

        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        write_settings(&config, &worktree_path).unwrap();

        let result: Value = serde_json::from_str(
            &fs::read_to_string(worktree_path.join(".claude/settings.local.json")).unwrap(),
        )
        .unwrap();
        let deny = result["permissions"]["deny"].as_array().unwrap();
        let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();

        assert!(
            !deny_strs.iter().any(|p| p.contains("../")),
            "worktree permissions.deny must not contain any parent-traversal \
             Edit(...) rule, got: {deny_strs:?}"
        );
        assert!(
            deny_strs.contains(&"Edit(doc/loom/knowledge/**)"),
            "the non-traversal deny_write entry must still be emitted, got: {deny_strs:?}"
        );
    }

    #[test]
    fn test_write_settings_main_repo_drops_stale_escape_from_existing() {
        use tempfile::TempDir;

        // Simulate a main-repo settings.local.json written by an OLDER loom
        // version that leaked worktree-relative escape rules. Re-running the
        // generator on the main repo must scrub them (both Read and Write sides),
        // even though the merge preserves other user-approved permissions.
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let claude_dir = repo_root.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        let stale = json!({
            "permissions": {
                "deny": [
                    "Read(../../**)",
                    "Read(../.worktrees/**)",
                    "Write(../../**)",
                    "Write(doc/loom/knowledge/**)"
                ]
            }
        });
        fs::write(
            claude_dir.join("settings.local.json"),
            serde_json::to_string_pretty(&stale).unwrap(),
        )
        .unwrap();

        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };

        write_settings(&config, repo_root).unwrap();

        let result: Value = serde_json::from_str(
            &fs::read_to_string(claude_dir.join("settings.local.json")).unwrap(),
        )
        .unwrap();
        let deny = result["permissions"]["deny"].as_array().unwrap();
        let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();

        assert!(
            !deny_strs
                .iter()
                .any(|p| p.contains("../") || p.contains(".worktrees")),
            "stale escape rules must be scrubbed from the main repo file, got: {deny_strs:?}"
        );
        // Legitimate knowledge write-protection is preserved.
        assert!(deny_strs.contains(&"Write(doc/loom/knowledge/**)"));
    }

    #[test]
    fn test_preserve_unowned_keys_carries_enabled_plugins_when_codex_licensed() {
        // Worktree-scoped: the codex-license gate applies here.
        let config = codex_licensed_config();
        let existing = json!({
            "enabledPlugins": { "codex": true }
        });
        let mut new_settings = json!({
            "sandbox": { "enabled": true }
        });

        preserve_unowned_keys(&mut new_settings, &existing, &config, true);

        assert_eq!(new_settings["enabledPlugins"], json!({ "codex": true }));
    }

    #[test]
    fn test_preserve_unowned_keys_carries_extra_known_marketplaces_when_codex_licensed() {
        // Worktree-scoped: the codex-license gate applies here.
        let config = codex_licensed_config();
        let existing = json!({
            "extraKnownMarketplaces": { "codex-marketplace": "https://example.com" }
        });
        let mut new_settings = json!({
            "sandbox": { "enabled": true }
        });

        preserve_unowned_keys(&mut new_settings, &existing, &config, true);

        assert_eq!(
            new_settings["extraKnownMarketplaces"],
            json!({ "codex-marketplace": "https://example.com" })
        );
    }

    #[test]
    fn test_preserve_unowned_keys_claude_only_worktree_does_not_carry_enabled_plugins() {
        // SECURITY: `.claude/settings.local.json` lives inside the
        // agent-writable, respawn-reused WORKTREE, so carrying these keys
        // forward unconditionally would let a stage agent write its own
        // `enabledPlugins` entry and have loom carry it into the next
        // session. A claude-only worktree stage has no legitimate reason to
        // need either key, so the gate must block the carry-forward
        // entirely. See the non-worktree negative control below, where the
        // same claude-only config must NOT be gated.
        let config = default_config(); // Implementers::default() is claude-only.
        assert!(!config.implementers.includes_codex());

        let existing = json!({
            "enabledPlugins": { "codex": true },
            "extraKnownMarketplaces": { "codex-marketplace": "https://example.com" }
        });
        let mut new_settings = json!({
            "sandbox": { "enabled": true }
        });

        preserve_unowned_keys(&mut new_settings, &existing, &config, true);

        assert!(new_settings.get("enabledPlugins").is_none());
        assert!(new_settings.get("extraKnownMarketplaces").is_none());
    }

    #[test]
    fn test_preserve_unowned_keys_claude_only_non_worktree_carries_enabled_plugins() {
        // SECURITY (the flip side of the worktree test above): the main repo
        // root's settings.local.json is NOT agent-writable, so there is no
        // self-grant to defend against there, and the codex-license gate
        // must NOT apply. `loom repair --fix` writes the main repo as
        // claude-only (`Implementers::default()`) unconditionally
        // (`commands/repair.rs::fix_sandbox_settings`); before this fix that
        // deleted a pre-existing codex plugin install on every run.
        let config = default_config(); // Implementers::default() is claude-only.
        assert!(!config.implementers.includes_codex());

        let existing = json!({
            "enabledPlugins": { "codex": true },
            "extraKnownMarketplaces": { "codex-marketplace": "https://example.com" }
        });
        let mut new_settings = json!({
            "sandbox": { "enabled": true }
        });

        preserve_unowned_keys(&mut new_settings, &existing, &config, false);

        assert_eq!(
            new_settings["enabledPlugins"],
            json!({ "codex": true }),
            "a non-worktree target must preserve enabledPlugins even for a \
             claude-only config, got: {new_settings:?}"
        );
        assert_eq!(
            new_settings["extraKnownMarketplaces"],
            json!({ "codex-marketplace": "https://example.com" }),
            "got: {new_settings:?}"
        );
    }

    #[test]
    fn test_preserve_unowned_keys_noop_when_absent() {
        let config = codex_licensed_config();
        let existing = json!({
            "sandbox": { "enabled": false }
        });
        let mut new_settings = json!({
            "sandbox": { "enabled": true }
        });

        preserve_unowned_keys(&mut new_settings, &existing, &config, true);

        assert!(new_settings.get("enabledPlugins").is_none());
        assert!(new_settings.get("extraKnownMarketplaces").is_none());
        assert_eq!(new_settings["sandbox"]["enabled"], true);
    }

    #[test]
    fn test_preserve_unowned_keys_does_not_override_generated_keys() {
        // A stale/user `sandbox` key in the existing file must never clobber
        // the freshly generated one - only keys ABSENT from new_settings are
        // carried over. Codex-licensed so the gate itself isn't what's under
        // test here.
        let config = codex_licensed_config();
        let existing = json!({
            "sandbox": { "enabled": false }
        });
        let mut new_settings = json!({
            "sandbox": { "enabled": true }
        });

        preserve_unowned_keys(&mut new_settings, &existing, &config, true);

        assert_eq!(new_settings["sandbox"]["enabled"], true);
    }

    #[test]
    fn test_write_settings_round_trip_worktree_codex_licensed_preserves_enabled_plugins() {
        use tempfile::TempDir;

        // The bug this guards against: write_settings regenerates the whole
        // file from scratch, so a plugin enabled at local scope (enabledPlugins)
        // used to vanish from every worktree on the next regeneration. Fixed by
        // carrying it forward - for a WORKTREE target, only for a stage that
        // licenses the codex lane, which is the only lane that needs it (see
        // the claude-only worktree negative-control test below, and the
        // non-worktree tests further down for the other half of the gate).
        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path().join(".worktrees").join("my-stage");
        fs::create_dir_all(&worktree_path).unwrap();

        let claude_dir = worktree_path.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let settings_path = claude_dir.join("settings.local.json");

        let existing_settings = json!({
            "enabledPlugins": { "codex": true },
            "extraKnownMarketplaces": { "codex-marketplace": "https://example.com" },
            "permissions": {
                "allow": ["Read(~/.ssh/config)"]
            }
        });
        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&existing_settings).unwrap(),
        )
        .unwrap();

        let config = codex_licensed_config();

        write_settings(&config, &worktree_path).unwrap();

        let result: Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();

        assert_eq!(
            result["enabledPlugins"],
            json!({ "codex": true }),
            "enabledPlugins must survive settings regeneration, got: {result:?}"
        );
        assert_eq!(
            result["extraKnownMarketplaces"],
            json!({ "codex-marketplace": "https://example.com" }),
            "extraKnownMarketplaces must survive settings regeneration, got: {result:?}"
        );
    }

    #[test]
    fn test_write_settings_round_trip_claude_only_worktree_drops_enabled_plugins() {
        use tempfile::TempDir;

        // SECURITY: a claude-only WORKTREE stage must NOT carry
        // `enabledPlugins` / `extraKnownMarketplaces` forward. Both keys live
        // in a file the stage agent itself can write, and worktrees are
        // reused across respawn / retry / crash recovery - an unconditional
        // carry-forward would let a claude-only stage agent self-grant
        // plugin enablement it was never licensed for. This is worktree-
        // scoped on purpose: the gate must NOT apply to a non-worktree
        // target (see the negative control immediately below), because that
        // target is not agent-writable and has nothing to self-grant.
        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path().join(".worktrees").join("my-stage");
        fs::create_dir_all(&worktree_path).unwrap();

        let claude_dir = worktree_path.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let settings_path = claude_dir.join("settings.local.json");

        let existing_settings = json!({
            "enabledPlugins": { "codex": true },
            "extraKnownMarketplaces": { "codex-marketplace": "https://example.com" }
        });
        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&existing_settings).unwrap(),
        )
        .unwrap();

        let config = default_config(); // Implementers::default() is claude-only.
        assert!(!config.implementers.includes_codex());

        write_settings(&config, &worktree_path).unwrap();

        let result: Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();

        assert!(
            result.get("enabledPlugins").is_none(),
            "claude-only worktree stage must not carry enabledPlugins forward, got: {result:?}"
        );
        assert!(
            result.get("extraKnownMarketplaces").is_none(),
            "claude-only worktree stage must not carry extraKnownMarketplaces forward, \
             got: {result:?}"
        );
    }

    #[test]
    fn test_write_settings_round_trip_claude_only_non_worktree_preserves_enabled_plugins() {
        use tempfile::TempDir;

        // SECURITY (CRITICAL regression): the codex-license gate must apply
        // ONLY to worktree targets. `loom repair --fix`
        // (`commands/repair.rs::fix_sandbox_settings`) writes the MAIN repo's
        // settings.local.json unconditionally as claude-only
        // (`Implementers::default()`). Before this fix, gating the
        // carry-forward on `includes_codex()` regardless of target meant
        // `loom repair --fix` silently DELETED a legitimate codex plugin
        // install from the main repo on every run - because write_settings
        // regenerates the whole file from scratch. The main repo root is not
        // agent-writable, so there is no self-grant to defend against here.
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path(); // not under .worktrees - the main repo root.

        let claude_dir = repo_root.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let settings_path = claude_dir.join("settings.local.json");

        let existing_settings = json!({
            "enabledPlugins": { "codex": true },
            "extraKnownMarketplaces": { "codex-marketplace": "https://example.com" }
        });
        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&existing_settings).unwrap(),
        )
        .unwrap();

        let config = default_config(); // Implementers::default() is claude-only.
        assert!(!config.implementers.includes_codex());

        write_settings(&config, repo_root).unwrap();

        let result: Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();

        assert_eq!(
            result["enabledPlugins"],
            json!({ "codex": true }),
            "a claude-only write to the MAIN repo must preserve a pre-existing \
             codex plugin install (e.g. `loom repair --fix`), got: {result:?}"
        );
        assert_eq!(
            result["extraKnownMarketplaces"],
            json!({ "codex-marketplace": "https://example.com" }),
            "got: {result:?}"
        );
    }
}
