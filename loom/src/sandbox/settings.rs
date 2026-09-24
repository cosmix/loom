use super::config::MergedSandboxConfig;
use crate::fs::permissions::state_root::token_deny_paths;
use crate::fs::permissions::write_rules::migrate_inert_write_denies;
use crate::plan::schema::PermissionMode;
use anyhow::Result;
use serde_json::{json, Value};
use std::path::Path;

// Only the tests exercise `build_settings` end to end from a real worktree layout on disk
// (`build_settings_for` in `tests.rs`); `write_settings`, which needed these, is gone.
#[cfg(test)]
use crate::fs::permissions::state_root::resolve_state_root;
#[cfg(test)]
use std::fs;

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

/// State-root subdirectories every session may read.
#[rustfmt::skip]
const STATE_READ_DIRS: [&str; 6] = ["signals", "handoffs", "disputes", "memory", "contracts", "reviews"];

/// Detect whether a settings target is a loom worktree (vs. the main repo root).
///
/// Loom worktrees always live at `<repo>/.worktrees/<stage-id>/` and carry a
/// state-root symlink into the main repo's shared state (`.loom/work` on the
/// nested layout, `.work` on a legacy workspace); the main repo root has
/// neither as a symlink. This distinction decides whether worktree-relative
/// escape rules (`../../**`, `../.worktrees/**`) are meaningful: inside a
/// worktree `../..` is the repo root (the intended isolation boundary), but
/// at the repo root `../..` is the repo's parent — typically `$HOME`.
pub(crate) fn target_is_worktree(target: &Path) -> bool {
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

/// Where a settings document built by `build_settings` applies, and what it
/// inherits.
pub(crate) struct SettingsTarget<'a> {
    /// Whether the document applies inside a stage worktree rather than the
    /// main checkout; for the checkout, worktree-relative escape rules are
    /// dropped (`strip_worktree_escape_denies`).
    pub is_worktree: bool,
    /// The canonical state root, when the target has one: its narrow reads
    /// and token read denies are added in the resolved spelling.
    pub state_root: Option<&'a str>,
    /// Settings whose deny rules carry forward (`carry_forward_denies`).
    pub existing: &'a Value,
    /// Whether `enabledPlugins` and `extraKnownMarketplaces` carry forward
    /// from `existing` too.
    pub carry_plugin_keys: bool,
}

/// The settings document for `config` at `target`: the sandbox block and
/// permission rules [`generate_settings_json`] builds, the resolved
/// state-root rules, and what `existing` carries forward. Pure: the session
/// capsule and the `build_settings_for` test helper both build through it.
pub(crate) fn build_settings(
    config: &MergedSandboxConfig,
    target: &SettingsTarget<'_>,
) -> Result<Value> {
    policy::validate_emittable(config)?;
    let mut config = config.clone();
    if !target.is_worktree {
        strip_worktree_escape_denies(&mut config);
    }
    let mut settings = generate_settings_json(&config);
    if let Some(state_root) = target.state_root {
        add_resolved_state_root_rules(&mut settings, state_root);
    }
    merge_existing_permissions(&mut settings, target.existing, target.is_worktree);
    preserve_unowned_keys(&mut settings, target.existing, target.carry_plugin_keys);
    Ok(settings)
}

/// The resolved-absolute spellings of the narrow state-root reads, and the
/// token files denied to Bash at the OS level.
///
/// Claude Code resolves the state-root symlink (`.loom/work`, or `.work` on a
/// legacy workspace) before matching permission patterns, so the relative
/// reads [`generate_settings_json`] emits never match there. See
/// `fs::permissions::state_root` for the S-1 rationale: a blanket read or
/// write over this path exposes `admin.token` / `user.token`, a daemon RPC
/// privilege escalation. So:
///   1. NO `Read(...)` deny is written, here or anywhere else. The tokens are
///      denied to Bash through `sandbox.filesystem.denyRead` and to the native
///      file tools by `loom-hooks/credential-guard.sh`. A permission-rule deny
///      is not an option at any path shape: Claude Code's Bash path validator
///      prompts the operator for every relative-path `rg`, `grep`, `diff`,
///      `git`, `cp` or `mv` issued after a `cd` in the same compound command
///      while ANY settings file carries ANY `Read(` deny rule, and that prompt
///      is neither bypassable nor auto-approvable;
///   2. only read-only orchestration state is granted. Sessions write no
///      state directly: handoffs, memory and disputes go through the relay.
///
/// Claude Code requires the `//` prefix for an absolute path in a permission
/// rule; a single `/` means relative to the project root
/// (<https://code.claude.com/docs/en/permissions.md>).
fn add_resolved_state_root_rules(settings: &mut Value, state_root: &str) {
    let mut reads = vec![format!("Read(/{state_root}/config.toml)")];
    reads.extend(
        STATE_READ_DIRS
            .iter()
            .map(|dir| format!("Read(/{state_root}/{dir}/**)")),
    );
    push_missing(settings, "/permissions/allow", reads);
    push_missing(
        settings,
        "/sandbox/filesystem/denyRead",
        token_deny_paths(state_root),
    );
}

/// Append each of `items` not already present to the string array at
/// `pointer`, when there is one.
fn push_missing(settings: &mut Value, pointer: &str, items: impl IntoIterator<Item = String>) {
    let Some(array) = settings.pointer_mut(pointer).and_then(Value::as_array_mut) else {
        return;
    };
    for item in items {
        if !array
            .iter()
            .any(|value| value.as_str() == Some(item.as_str()))
        {
            array.push(json!(item));
        }
    }
}

/// Filters plan `allow_write` paths into `Edit(...)` permission rules and
/// appends them to `allow`, deduping against what's already there. A path
/// starting with exactly one `/` is rewritten to `//` (Claude Code's
/// permission-rule paths take single `/` as project-relative, `//` as
/// absolute — the opposite of `sandbox.filesystem.allowWrite`); `//abs`,
/// `~/...` and relative entries pass through unchanged (see
/// `grant_paths::edit_rule`). Also filters `../` and dedupes against `allow`.
fn push_allow_write_rules(allow: &mut Vec<Value>, config: &MergedSandboxConfig) {
    for path in config
        .filesystem
        .allow_write
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty() && !p.contains("../"))
    {
        let rule = json!(super::grant_paths::edit_rule(path));
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
    // `loom-hooks/credential-guard.sh` for the native file tools. See
    // `add_resolved_state_root_rules` for why a `Read(` deny rule of any
    // shape is unacceptable.
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
    // by the OS sandbox's `allowOnly` list, `loom-hooks/worktree-file-guard.sh`, and
    // `loom-hooks/worktree-isolation.sh`.
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

    // Add narrow Read permissions for orchestration state files agents need.
    // These are the *relative* forms; `build_settings` adds matching
    // resolved-absolute forms because `.loom/work` (or, on a legacy
    // workspace, `.work`) is a symlink that Claude Code resolves before
    // matching. The set is deliberately scoped to the subdirs an agent
    // legitimately reads — never the bare `.loom/work/**` that would also
    // expose `.loom/work/admin.token` / `.loom/work/user.token` (see S-1,
    // default_deny_read). No state directory is writable: sessions change
    // state through the relay.
    //
    // Both layouts are emitted: `MergedSandboxConfig` carries no field for
    // which layout this workspace uses, so this function can't branch on it.
    // A workspace whose `config.toml` was found under legacy `.work/` keeps
    // that layout forever, so its narrow rules must exist in the legacy
    // spelling too, alongside the nested one — on either layout the unused
    // spelling matches nothing and costs nothing.
    for base in [".loom/work", ".work"] {
        allow.push(json!(format!("Read({base}/config.toml)")));
        for dir in STATE_READ_DIRS {
            allow.push(json!(format!("Read({base}/{dir}/**)")));
        }
    }

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

/// The deny entries from an existing settings file that may be carried into the
/// regenerated one, in the enforceable spelling.
///
/// Stale entries that would be harmful if leaked into the OS sandbox are
/// dropped first:
/// - EVERY `Read(...)` entry, whatever its path. `settings.local.json` is
///   loom-generated and this generator emits no read deny at all, so anything
///   found there is from an older version; carrying one forward in any shape
///   reintroduces the Bash search prompt described in
///   `add_resolved_state_root_rules`. This is deliberately blunter than the
///   healers that act on files loom does not own end to end —
///   `write_rules::prune_loom_read_denies` and
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
/// from `config` on every write. `deny` carry-forward is safe to keep because
/// widening `deny` can only narrow what the agent can do, never grant it
/// anything.
///
/// The carried entries (`carry_forward_denies`) are appended in order, each
/// once. `is_worktree` indicates whether the destination lives inside a loom
/// worktree; for the main repo root, stale worktree-relative escape entries
/// are dropped.
fn merge_existing_permissions(
    new_settings: &mut Value,
    existing_settings: &Value,
    is_worktree: bool,
) {
    let existing_deny: Vec<String> = existing_settings
        .pointer("/permissions/deny")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if existing_deny.is_empty() {
        return;
    }
    let Some(permissions) = new_settings
        .get_mut("permissions")
        .and_then(Value::as_object_mut)
    else {
        return; // New settings has no permissions block, nothing to merge into
    };
    permissions.entry("deny").or_insert_with(|| json!([]));
    let carried = carry_forward_denies(existing_deny, is_worktree);
    push_missing(new_settings, "/permissions/deny", carried);
}

/// Top-level settings keys that `generate_settings_json` does not emit and
/// that must therefore be carried forward from the existing file, or they
/// are silently dropped on every regeneration.
const PRESERVED_SETTINGS_KEYS: [&str; 2] = ["enabledPlugins", "extraKnownMarketplaces"];

/// Carry the `PRESERVED_SETTINGS_KEYS` loom does not own from `existing`
/// into `new_settings` when `carry` is set. `generated always wins`: only
/// keys `new_settings` does not already contain are filled in.
fn preserve_unowned_keys(new_settings: &mut Value, existing: &Value, carry: bool) {
    let (true, Some(existing_obj), Some(new_obj)) =
        (carry, existing.as_object(), new_settings.as_object_mut())
    else {
        return;
    };
    for key in PRESERVED_SETTINGS_KEYS {
        if let Some(value) = existing_obj.get(key) {
            new_obj
                .entry(key.to_string())
                .or_insert_with(|| value.clone());
        }
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_read_denies;
#[cfg(test)]
mod tests_token_rules;
