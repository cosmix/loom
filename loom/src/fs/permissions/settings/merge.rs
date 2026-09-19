//! Permission-list merge and attribution defaults for `.claude/settings.json`.

use anyhow::Result;
use serde_json::{json, Map, Value};

use super::{codex_forward_home_allow_entry, require_object};
use crate::fs::permissions::constants::LOOM_PERMISSIONS;
use crate::fs::permissions::write_rules::prune_legacy_permission_grants;

/// Merge loom's permission grants into `settings_obj`'s `permissions.allow`
/// array: prune legacy grants, then add any missing loom permission
/// (including the home-expanded codex forwarding wrapper entry).
///
/// Returns `(added_permissions, removed_permissions)`.
///
/// Factored out of [`super::ensure_loom_permissions_inner`], which runs this
/// before migrating hooks, pruning read denies, and ensuring attribution.
pub(super) fn merge_permissions(settings_obj: &mut Map<String, Value>) -> Result<(usize, usize)> {
    // Get or create permissions object
    let permissions = settings_obj
        .entry("permissions")
        .or_insert_with(|| json!({}));

    let permissions_obj = require_object(permissions, "permissions")?;

    // Get or create allow array
    let allow = permissions_obj.entry("allow").or_insert_with(|| json!([]));

    let allow_arr = allow
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions.allow must be a JSON array"))?;

    let removed_permissions = prune_legacy_permission_grants(allow_arr);

    // Collect existing permissions as strings for deduplication
    let existing: std::collections::HashSet<String> = allow_arr
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    // Add missing loom permissions
    let mut added_permissions = 0;
    for permission in LOOM_PERMISSIONS {
        if !existing.contains(*permission) {
            allow_arr.push(json!(permission));
            added_permissions += 1;
        }
    }

    // Additive: also allow the home-expanded spelling of the codex forwarding wrapper (see
    // `codex_forward_home_allow_entry` for why this can't live in the static LOOM_PERMISSIONS
    // array above). Skipped silently if the home directory can't be resolved — never fail the
    // whole permission write over it.
    if let Some(home_entry) = codex_forward_home_allow_entry() {
        if !existing.contains(home_entry.as_str()) {
            allow_arr.push(json!(home_entry));
            added_permissions += 1;
        }
    }

    Ok((added_permissions, removed_permissions))
}

/// Turn off Claude Code's commit and PR attribution unless the repository
/// already sets its own `attribution` block. Returns whether it was added.
pub(super) fn ensure_no_attribution(settings_obj: &mut Map<String, Value>) -> bool {
    if settings_obj.contains_key("attribution") {
        return false;
    }
    settings_obj.insert(
        "attribution".to_string(),
        json!({ "commit": "", "pr": "", "sessionUrl": false }),
    );
    true
}
