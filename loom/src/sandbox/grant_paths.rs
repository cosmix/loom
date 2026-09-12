//! Host-side existence checks and permission-rule mapping for plan-authored
//! `allow_write` grants.

use std::path::{Path, PathBuf};

use super::config::MergedSandboxConfig;

/// Characters that mark an `allow_write` entry as a glob rather than a
/// concrete path — existence cannot be checked against a pattern.
const GLOB_METACHARACTERS: [char; 4] = ['*', '?', '[', '{'];

/// Resolve a single `allow_write` entry to the absolute path the sandbox
/// would bind, or `None` when the entry is a glob, a relative path (already
/// inside the writable worktree, so never "missing" in the sense this module
/// cares about), or a `~/...` entry with no home directory to resolve
/// against.
fn resolve_grant_path(entry: &str, home: Option<&Path>) -> Option<PathBuf> {
    if entry.chars().any(|c| GLOB_METACHARACTERS.contains(&c)) {
        return None;
    }
    if let Some(rest) = entry.strip_prefix("~/") {
        return home.map(|h| h.join(rest));
    }
    if let Some(rest) = entry.strip_prefix("//") {
        return Some(PathBuf::from(format!("/{rest}")));
    }
    if let Some(rest) = entry.strip_prefix('/') {
        return Some(PathBuf::from(format!("/{rest}")));
    }
    None
}

/// `allow_write` entries that name a path missing on THIS host, in input
/// order, deduped.
///
/// Claude Code's session sandbox binds an `allowWrite` entry only when the
/// path already exists at session start; a missing one is silently skipped,
/// not created, and a `mkdir` issued from inside the session cannot fix it —
/// the directory it would create is exactly the one the sandbox declined to
/// bind. This is the check that surfaces the gap before spawn, instead of
/// leaving it to be discovered as an unexplained `Read-only file system`.
pub fn missing_grant_paths(allow_write: &[String], home: Option<&Path>) -> Vec<String> {
    let mut missing = Vec::new();
    for raw in allow_write {
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        let Some(resolved) = resolve_grant_path(entry, home) else {
            continue;
        };
        if !resolved.exists() && !missing.iter().any(|m| m == entry) {
            missing.push(entry.to_string());
        }
    }
    missing
}

/// Warn once per stage spawn about every `allow_write` grant that will not be
/// bound because its path is missing on the host. Plan grants only — never
/// [`super::PACKAGE_MANAGER_CACHE_WRITE_PATHS`], which several platforms
/// never populate for a given manager and already carries its own signal-side
/// note (`append_package_cache_note`).
pub fn warn_missing_grants(config: &MergedSandboxConfig, stage_id: &str) {
    for path in missing_grant_paths(&config.filesystem.allow_write, dirs::home_dir().as_deref()) {
        tracing::warn!(
            stage_id = %stage_id,
            path = %path,
            "sandbox.filesystem.allow_write names a path that does not exist on this host; \
             the session sandbox will not bind it, and a mkdir inside the session cannot \
             create it. Create the path on the host before this stage's session starts."
        );
    }
}

/// Map an `allow_write` entry to the `Edit(...)` permission rule it should
/// become. Claude Code's permission-rule paths use a single leading `/` for
/// PROJECT-relative and `//` for absolute — the opposite of
/// `sandbox.filesystem.allowWrite`, which takes a plain `/` as absolute. A
/// path starting with exactly one `/` is therefore rewritten to `//`; `//abs`,
/// `~/...` and relative entries already mean what they say and pass through.
pub(crate) fn edit_rule(path: &str) -> String {
    if path.starts_with('/') && !path.starts_with("//") {
        format!("Edit(/{path})")
    } else {
        format!("Edit({path})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::Implementers;
    use crate::plan::schema::{
        CommandConfinement, FilesystemConfig, LinuxConfig, NetworkConfig, PermissionMode,
    };
    use tempfile::TempDir;

    fn config_with(allow_write: Vec<String>) -> MergedSandboxConfig {
        MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                allow_write,
                ..FilesystemConfig::default()
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        }
    }

    #[test]
    fn missing_absolute_path_is_reported() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("does-not-exist").display().to_string();

        let result = missing_grant_paths(std::slice::from_ref(&missing), None);

        assert_eq!(result, vec![missing]);
    }

    #[test]
    fn existing_absolute_directory_is_not_reported() {
        let temp = TempDir::new().unwrap();
        let existing = temp.path().display().to_string();

        assert!(missing_grant_paths(&[existing], None).is_empty());
    }

    #[test]
    fn existing_absolute_file_is_not_reported() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("a-file");
        std::fs::write(&file, "x").unwrap();

        assert!(missing_grant_paths(&[file.display().to_string()], None).is_empty());
    }

    #[test]
    fn glob_entries_are_skipped() {
        assert!(missing_grant_paths(&["/tmp/does-not-exist/**".to_string()], None).is_empty());
    }

    #[test]
    fn relative_entries_are_skipped() {
        // No leading `/` or `~/` and no glob metacharacter: falls through
        // `resolve_grant_path` to `None` on its own, distinct from the glob
        // check above.
        assert!(missing_grant_paths(&["loom/src/foo.rs".to_string()], None).is_empty());
    }

    #[test]
    fn tilde_paths_resolve_against_the_injected_home() {
        let home = TempDir::new().unwrap();

        // Missing under the injected home.
        let result = missing_grant_paths(&["~/cache/x".to_string()], Some(home.path()));
        assert_eq!(result, vec!["~/cache/x".to_string()]);

        // Present under the injected home: no longer reported.
        std::fs::create_dir_all(home.path().join("cache/x")).unwrap();
        assert!(missing_grant_paths(&["~/cache/x".to_string()], Some(home.path())).is_empty());
    }

    #[test]
    fn tilde_paths_without_a_home_are_skipped() {
        assert!(missing_grant_paths(&["~/cache/x".to_string()], None).is_empty());
    }

    #[test]
    fn double_slash_absolute_is_handled_like_single_slash() {
        let temp = TempDir::new().unwrap();
        let missing = format!("/{}", temp.path().join("gone").display());

        let result = missing_grant_paths(std::slice::from_ref(&missing), None);

        assert_eq!(result, vec![missing]);
    }

    #[test]
    fn edit_rule_rewrites_a_single_leading_slash_to_double() {
        assert_eq!(edit_rule("/tmp/loom-grant"), "Edit(//tmp/loom-grant)");
    }

    #[test]
    fn edit_rule_passes_through_already_absolute_tilde_and_relative_forms() {
        assert_eq!(edit_rule("//tmp/loom-grant"), "Edit(//tmp/loom-grant)");
        assert_eq!(edit_rule("~/cache/x"), "Edit(~/cache/x)");
        assert_eq!(edit_rule("loom/src/**"), "Edit(loom/src/**)");
    }

    #[test]
    fn generated_settings_use_double_slash_for_plan_absolute_grants() {
        let config = config_with(vec![
            "/tmp/loom-grant".to_string(),
            "~/cache/x".to_string(),
            "loom/src/**".to_string(),
        ]);

        let json = super::super::generate_settings_json(&config);
        let allow: Vec<&str> = json["permissions"]["allow"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();

        assert!(allow.contains(&"Edit(//tmp/loom-grant)"));
        assert!(allow.contains(&"Edit(~/cache/x)"));
        assert!(allow.contains(&"Edit(loom/src/**)"));
        assert!(!allow.contains(&"Edit(/tmp/loom-grant)"));

        let allow_write_os: Vec<&str> = json["sandbox"]["filesystem"]["allowWrite"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(allow_write_os.contains(&"/tmp/loom-grant"));
    }
}
