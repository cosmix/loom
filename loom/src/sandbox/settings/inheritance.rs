use super::push_missing;
use crate::fs::permissions::write_rules::migrate_inert_write_denies;
use serde_json::{json, Value};

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
pub(super) fn merge_existing_permissions(
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
pub(super) fn preserve_unowned_keys(new_settings: &mut Value, existing: &Value, carry: bool) {
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
