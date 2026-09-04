//! `Write(path)` permission rules: pruning the inert grants loom used to emit,
//! and migrating inherited denies to the enforceable `Edit(path)` spelling.
//!
//! Claude Code's file permission check consults ONLY `Edit(path)` rules. A
//! `Write(path)` rule parses, prints a warning at every session start, and is
//! then ignored — so a `Write(...)` grant grants nothing and a `Write(...)`
//! deny denies nothing.

use super::state_root::is_loom_written_read_deny;
use serde_json::{json, Map, Value};

/// The path argument of a `Write(<path>)` rule, or `None` for any other rule.
fn strip_write_rule(rule: &str) -> Option<&str> {
    rule.strip_prefix("Write(")?.strip_suffix(')')
}

/// Is this allow entry one of the inert `Write(...)` grants loom itself used to
/// emit over the state directory? Loom emits none of these spellings any
/// more, but they survive in every settings.json an older version wrote, so
/// `ensure_loom_permissions_to` prunes them on every run.
///
/// This is a CONSUMER of paths written by older loom versions, not a
/// producer of the current one: the state root moved from `.work` to
/// `.loom/work`, but settings.json files written before that move still
/// carry the old spellings, so the pruner must keep recognising both layouts
/// or the old grants become immortal. Matches both, in each of the three
/// shapes loom historically wrote for a layout — relative, worktree-relative
/// (from a worktree's nested `.claude/settings.json`, one `../` per
/// directory between the worktree and the main repo root), and
/// resolved-absolute (from `git/worktree/settings.rs`) — six shapes total.
///
/// Matches ONLY those six shapes. Any other `Write(...)` allow entry is the
/// developer's own config: dropping it, or converting it to the enforceable
/// `Edit(...)` form (which would widen what it grants), is not loom's call.
pub(super) fn is_legacy_loom_work_write_allow(entry: &str) -> bool {
    let Some(path) = strip_write_rule(entry) else {
        return false;
    };
    // `//<abs>/.loom/work/**` or `//<abs>/.work/**` is the resolved-absolute
    // form (Claude Code's `//` prefix convention for absolute paths).
    matches!(
        path,
        ".loom/work/**" | "../../../.loom/work/**" | ".work/**" | "../../.work/**"
    ) || (path.starts_with("//")
        && (path.ends_with("/.loom/work/**") || path.ends_with("/.work/**")))
}

/// Drop every [`is_legacy_loom_work_write_allow`] entry from an allow array,
/// returning how many went. They enforce nothing and print a startup warning
/// every session; `Read(.loom/work/**)` + `Edit(.loom/work/handoffs/**)`
/// replace them.
pub(super) fn prune_legacy_work_write_grants(allow: &mut Vec<Value>) -> usize {
    let before = allow.len();
    allow.retain(|entry| {
        entry
            .as_str()
            .is_none_or(|rule| !is_legacy_loom_work_write_allow(rule))
    });
    before - allow.len()
}

/// Whether a permission promoted OUT of a worktree should be dropped.
///
/// `migrate_inert_write_denies` rewrites rather than drops, but that applies to
/// a file loom owns end to end. These rules travel the other way: from a
/// worktree the stage agent can write into the developer's main-repo config.
/// Rewriting one there would turn a rule that has never been enforced into one
/// that is, from a source loom does not control. Dropping an entry that
/// enforces nothing anywhere changes no behaviour.
pub(super) fn is_inert_write_permission(permission: &str) -> bool {
    permission.starts_with("Write(")
}

/// Migrate carried-forward `permissions.deny` entries out of the inert
/// `Write(...)` spelling.
///
/// Loom used to carry a user-authored `Write(...)` deny forward verbatim, on the
/// principle that it is conservative about inherited rules. That preserved a
/// rule which enforces nothing and warns on every session start, forever, so it
/// is now migrated to the form expressing the same intent:
///
/// - DROPPED when the path is blanket (`**` / `*`) or contains `../`. Enforced
///   as `Edit(...)` those deny the agent's own tree — `**` matches everything,
///   and a `../` pattern resolves to an ancestor of the settings file while `**`
///   crosses separators — which is why `sandbox::settings::generate_settings_json`
///   must never emit them. Removal costs no protection: the OS sandbox's write
///   side is an `allowOnly` list, not a deny list.
/// - DROPPED under `doc/loom/knowledge`, matching the carve-out in
///   `sandbox::settings::merge_existing_permissions`: such a deny would
///   permanently block the `loom knowledge` CLI subprocess.
/// - REWRITTEN to `Edit(<path>)` otherwise.
///
/// Non-`Write(...)` entries pass through untouched. Input order is preserved and
/// duplicates dropped, so a `Write(<p>)` migrating onto an `Edit(<p>)` already
/// in the list collapses into one entry.
pub(crate) fn migrate_inert_write_denies(rules: &[String]) -> Vec<String> {
    let mut migrated: Vec<String> = Vec::with_capacity(rules.len());
    for rule in rules {
        let entry = match strip_write_rule(rule) {
            Some(path) => {
                let path = path.trim();
                if matches!(path, "**" | "*")
                    || path.contains("../")
                    || path.contains("doc/loom/knowledge")
                {
                    continue;
                }
                format!("Edit({path})")
            }
            None => rule.clone(),
        };
        if !migrated.contains(&entry) {
            migrated.push(entry);
        }
    }
    migrated
}

/// Rewrite `permissions.deny` in a settings document through
/// [`migrate_inert_write_denies`], healing a file an older loom polluted with
/// `Write(...)` denies instead of waiting for the next stage spawn to regenerate
/// it. Non-string array entries are left as they are. Returns `true` on change.
pub(super) fn heal_inert_write_denies(settings_obj: &mut Map<String, Value>) -> bool {
    let Some(deny) = settings_obj
        .get_mut("permissions")
        .and_then(|p| p.get_mut("deny"))
        .and_then(|d| d.as_array_mut())
    else {
        return false;
    };
    let rules: Vec<String> = deny
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    let migrated = migrate_inert_write_denies(&rules);
    if migrated == rules {
        return false;
    }
    let non_strings: Vec<Value> = deny.iter().filter(|v| !v.is_string()).cloned().collect();
    *deny = migrated
        .into_iter()
        .map(|s| json!(s))
        .chain(non_strings)
        .collect();
    true
}

/// Drop every `permissions.deny` entry loom itself ever wrote in the
/// `Read(...)` shape.
///
/// Claude Code's Bash path validator refuses `rg`, `grep`, `diff`, `git`,
/// `cp` and `mv` on a relative path issued after a `cd` — with a
/// bypass-immune operator prompt — whenever ANY settings file carries ANY
/// `Read(...)` deny rule, regardless of the rule's path shape. Loom therefore
/// no longer writes a `Read(...)` deny anywhere: the daemon token files stay
/// protected by the OS-level `sandbox.filesystem.denyRead` list
/// (`state_root::token_deny_paths`) and, for the native file tools, by the
/// `hooks/credential-guard.sh` PreToolUse guard. This prunes what OLDER loom
/// versions left behind — both the stale-shaped token denies and the current
/// parent-glob ones, since even the current shape now makes searches prompt —
/// via [`is_loom_written_read_deny`]. An operator's own `Read(...)` deny is
/// never loom's to remove; it is left in place, and `loom repair` reports it
/// separately so the operator can decide.
///
/// `loom init` heals `settings.local.json` and `settings.json` here rather
/// than leaving the prompts in place for every interactive session until the
/// next stage spawn regenerates the file; `loom repair --fix` heals worktree
/// copies. An absent `permissions.deny` is left absent. Returns `true` when
/// an entry was removed.
pub(crate) fn prune_loom_read_denies(settings_obj: &mut Map<String, Value>) -> bool {
    let Some(deny) = settings_obj
        .get_mut("permissions")
        .and_then(|permissions| permissions.get_mut("deny"))
        .and_then(|deny| deny.as_array_mut())
    else {
        return false;
    };
    let before = deny.len();
    deny.retain(|entry| !entry.as_str().is_some_and(is_loom_written_read_deny));
    deny.len() != before
}

/// [`prune_loom_read_denies`], plus the standard verbose notice when it
/// removed something. Shared by both `.claude/settings.json` and
/// `.claude/settings.local.json` call sites so each stays a one-liner.
pub(crate) fn prune_read_denies_verbose(
    settings_obj: &mut Map<String, Value>,
    verbose: bool,
) -> bool {
    let denies_removed = prune_loom_read_denies(settings_obj);
    if verbose && denies_removed {
        println!(
            "  Removed Read deny rule(s) that make searches prompt from .claude/settings.json"
        );
    }
    denies_removed
}
