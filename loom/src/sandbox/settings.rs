use super::config::MergedSandboxConfig;
use crate::fs::permissions::write_rules::migrate_inert_write_denies;
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
/// state-root symlink into the main repo's shared state (`.loom/work` on the
/// nested layout, `.work` on a legacy workspace); the main repo root has
/// neither as a symlink. This distinction decides whether worktree-relative
/// escape rules (`../../**`, `../.worktrees/**`) are meaningful: inside a
/// worktree `../..` is the repo root (the intended isolation boundary), but
/// at the repo root `../..` is the repo's parent — typically `$HOME`.
fn target_is_worktree(target: &Path) -> bool {
    if target.components().any(|c| c.as_os_str() == ".worktrees") {
        return true;
    }
    // Fallback: a worktree's state-root link is a symlink; the main repo's is
    // a real directory in both spellings. Never probe bare `.loom` — the
    // worktree's `.loom/` is a real directory (it also holds the spools and
    // `.loom/cache/`), and so is the main repo's, so that alone proves
    // nothing.
    is_symlink(&target.join(".loom").join("work")) || is_symlink(&target.join(".work"))
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
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

    // Resolve the state-root symlink to its absolute target path.
    // Claude Code resolves symlinks before checking permission patterns, so
    // the relative `.loom/work/`-scoped Read patterns below don't match the
    // resolved absolute path, and reads there would prompt without these.
    // See `fs::permissions::state_root` for the shared resolution and the S-1
    // rationale (blanket read/write over this path exposes `admin.token` /
    // `user.token` — a daemon RPC privilege escalation). Below:
    //   1. NO `Read(...)` deny is written, here or anywhere else. The tokens
    //      are denied to Bash through the OS-level
    //      `sandbox.filesystem.denyRead` list (pushed just below) and to the
    //      native file tools by `hooks/credential-guard.sh`. A permission-rule
    //      deny is not an option at any path shape: Claude Code's Bash path
    //      validator prompts the operator for every relative-path `rg`,
    //      `grep`, `diff`, `git`, `cp` or `mv` issued after a `cd` in the same
    //      compound command while ANY settings file carries ANY `Read(` deny
    //      rule, and that prompt is neither bypassable nor auto-approvable;
    //   2. narrow the broad allow from `/**` down to read-only orchestration
    //      state plus handoff writes. Memory and dispute state are daemon-owned,
    //      so direct file-tool writes must never be authorized.
    //
    // IMPORTANT: Claude Code requires the // prefix for absolute filesystem paths.
    // A single / means "relative to project root", NOT absolute. See:
    // https://code.claude.com/docs/en/permissions.md
    if let Some(resolved) = crate::fs::permissions::state_root::resolve_state_root(worktree_path) {
        let resolved_str = resolved
            .to_str()
            .context("Resolved .work path is not valid UTF-8")?;
        if let Some(deny_read) = settings_json
            .pointer_mut("/sandbox/filesystem/denyRead")
            .and_then(Value::as_array_mut)
        {
            for deny_path in crate::fs::permissions::state_root::token_deny_paths(resolved_str) {
                if !deny_read.iter().any(|value| value == &deny_path) {
                    deny_read.push(json!(deny_path));
                }
            }
        }
        if let Some(permissions) = settings_json.get_mut("permissions") {
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

/// Filters plan `allow_write` paths into `Edit(...)` permission rules and
/// appends them to `allow`, deduping against what's already there.
///
/// Filters `../` and dedupes: this `Edit(...)` rule merges into the
/// OS-enforced `allowWrite` grant emitted by `sandbox.filesystem.allowWrite`
/// in settings/policy.rs, so an unfiltered entry would grant write outside
/// the worktree, bypassing that sibling emitter's filter.
fn push_allow_write_rules(allow: &mut Vec<Value>, config: &MergedSandboxConfig) {
    for path in config
        .filesystem
        .allow_write
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty() && !p.contains("../"))
    {
        let rule = json!(format!("Edit({path})"));
        if !allow.contains(&rule) {
            allow.push(rule);
        }
    }
}

/// Generate Claude Code settings JSON from sandbox config
pub fn generate_settings_json(config: &MergedSandboxConfig) -> Value {
    let mut settings = json!({});
    settings["sandbox"] = policy::sandbox_settings(config);

    // Build permissions block for file tool restrictions (Read/Write/Edit prompting)
    // These still work for prompting even though they don't provide OS-level isolation
    //
    // No `Read(...)` deny is ever emitted here — read denial is entirely the
    // OS sandbox's job (`sandbox.filesystem.denyRead`, above) plus
    // `hooks/credential-guard.sh` for the native file tools. See
    // `write_settings` for why a `Read(` deny rule of any shape is
    // unacceptable.
    let mut permissions = json!({});
    let mut deny: Vec<Value> = Vec::new();
    let mut allow: Vec<Value> = Vec::new();

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
    //
    // Also skip the knowledge directory: `merge_config` already strips it via
    // `apply_knowledge_write_grant`, but this is defense-in-depth for any
    // `MergedSandboxConfig` a caller builds by hand without going through
    // `merge_config` — such a config must never be able to emit the deny that
    // blocks the `loom knowledge update` CLI subprocess.
    for path in &config.filesystem.deny_write {
        if path.contains("../") || path.trim().starts_with("doc/loom/knowledge") {
            continue;
        }
        deny.push(json!(format!("Edit({})", path)));
    }

    // Add allow_write paths as exceptions (same Write->Edit reasoning as above).
    push_allow_write_rules(&mut allow, config);

    // Add narrow Read/Edit permissions for orchestration state files agents
    // need. These are the *relative* forms; `write_settings` adds matching
    // resolved-absolute forms because `.loom/work` (or, on a legacy
    // workspace, `.work`) is a symlink that Claude Code resolves before
    // matching. The set is deliberately scoped to the subdirs an agent
    // legitimately touches — never the bare `.loom/work/**` that would also
    // expose `.loom/work/admin.token` / `.loom/work/user.token` (see S-1,
    // default_deny_read).
    //
    // Both layouts are emitted: `MergedSandboxConfig` carries no field for
    // which layout this workspace uses, so this function can't branch on it.
    // A workspace whose `config.toml` was found under legacy `.work/` keeps
    // that layout forever, so its narrow rules must exist in the legacy
    // spelling too, alongside the nested one — on either layout the unused
    // spelling matches nothing and costs nothing.
    allow.push(json!("Read(.loom/work/config.toml)"));
    allow.push(json!("Read(.loom/work/signals/**)"));
    allow.push(json!("Read(.loom/work/handoffs/**)"));
    allow.push(json!("Edit(.loom/work/handoffs/**)"));
    allow.push(json!("Read(.loom/work/disputes/**)"));
    allow.push(json!("Read(.loom/work/memory/**)"));
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
///
/// What survives those filters is not written back verbatim: `Write(...)`
/// entries are migrated to the enforceable `Edit(...)` form (or dropped) by
/// `fs::permissions::settings::migrate_inert_write_denies`, so this function
/// never re-emits a rule that only produces a startup warning.
/// The deny entries from an existing settings file that may be carried into the
/// regenerated one, in the enforceable spelling.
///
/// Stale entries that would be harmful if leaked into the OS sandbox are
/// dropped first:
/// - EVERY `Read(...)` entry, whatever its path. `settings.local.json` is
///   loom-generated and this generator emits no read deny at all, so anything
///   found there is from an older version; carrying one forward in any shape
///   reintroduces the Bash search prompt described in `write_settings`. This
///   is deliberately blunter than the healers that act on files loom does not
///   own end to end — `write_rules::prune_loom_read_denies` and
///   `commands::repair::sandbox_settings::fix_read_denies` remove only the
///   entries loom itself wrote and report an operator's own rule instead. Here
///   there is no operator rule to preserve: the file is regenerated wholesale
///   on every stage spawn, so nothing hand-added to it survives anyway;
/// - for the MAIN repo, worktree-relative escape rules and cross-worktree refs
///   on the write side too: at the repo root `../..` is `$HOME`, so a stale
///   `Write(../../**)` would deny writes across the entire home directory;
/// - a knowledge-dir deny in either spelling — the ONE inherited-rule exception
///   to "loom is conservative about rules it inherits" (every other filter here
///   only narrows what a stale rule denies). `merge_config` /
///   `generate_settings_json` never re-add it, but this merge would union it
///   back in from disk on every write, permanently blocking the `loom knowledge
///   update` CLI subprocess for that worktree.
///
/// What survives is then migrated out of the inert `Write(...)` spelling — see
/// `migrate_inert_write_denies` for that policy.
fn carry_forward_denies(existing_deny: Vec<String>, is_worktree: bool) -> Vec<String> {
    let kept: Vec<String> = existing_deny
        .into_iter()
        .filter(|perm| !perm.starts_with("Read("))
        .filter(|perm| is_worktree || !(perm.contains("../") || perm.contains(".worktrees")))
        .filter(|perm| {
            !((perm.starts_with("Edit(") || perm.starts_with("Write("))
                && perm.contains("doc/loom/knowledge"))
        })
        .collect();
    migrate_inert_write_denies(&kept)
}

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

            for perm in carry_forward_denies(existing_deny, is_worktree) {
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
mod tests;
#[cfg(test)]
mod tests_read_denies;
#[cfg(test)]
mod tests_token_rules;
