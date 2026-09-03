use super::tests::default_config;
use super::*;
use std::path::{Component, PathBuf};

/// The relative token paths stay in `sandbox.filesystem.denyRead` for OS
/// enforcement, but must never be written as `permissions.deny` rules: their
/// location sits inside the project, and Claude Code then refuses every
/// `rg`/`grep` run from the project root until the operator approves it.
#[test]
fn token_denies_are_os_rules_only_never_project_relative_permission_rules() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    write_settings(&default_config(), repo_root).unwrap();

    let content = fs::read_to_string(repo_root.join(".claude/settings.local.json")).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    let deny: Vec<&str> = settings["permissions"]["deny"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert!(
        !deny.iter().any(|entry| is_token_read_deny(entry)),
        "a repo with no state root must carry no token permission rule, got: {deny:?}"
    );

    let os_deny = settings["sandbox"]["filesystem"]["denyRead"]
        .as_array()
        .unwrap();
    for relative in [".work/admin.token", ".loom/work/user.token"] {
        assert!(
            os_deny.iter().any(|value| value == relative),
            "the OS deny list must keep {relative}, got: {os_deny:?}"
        );
    }
}

/// The directory Claude Code's `deniedPathInsideDirectory` check derives from
/// a `Read(...)` deny rule: the rule's path up to its first wildcard, joined
/// to the project root for the relative spellings and normalized. `None` for
/// non-`Read` rules and for home-relative ones, which never land in a project.
fn deny_rule_location(entry: &str, project_root: &Path) -> Option<PathBuf> {
    let path = entry.strip_prefix("Read(")?.strip_suffix(')')?;
    if path.starts_with('~') {
        return None;
    }
    let joined = match path.strip_prefix("//") {
        Some(absolute) => PathBuf::from("/").join(absolute),
        None => project_root.join(path.trim_start_matches('/')),
    };
    let mut location = PathBuf::new();
    for component in joined.components() {
        if component
            .as_os_str()
            .to_string_lossy()
            .contains(['*', '?', '[', ']'])
        {
            break;
        }
        if component == Component::ParentDir {
            location.pop();
        } else {
            location.push(component);
        }
    }
    Some(location)
}

/// Regression guard for the property Claude Code actually enforces: it refuses
/// `rg`, `grep`, `diff`, `git`, `cp` and `mv` over any directory containing a
/// `Read(...)` deny rule's location, bypass-immune and not classifier-
/// approvable. A rule whose location falls inside the project root therefore
/// stalls auto mode on the first search. Covers both generated files.
#[cfg(unix)]
#[test]
fn no_generated_read_deny_puts_its_location_inside_the_project() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    let work_dir = base.join(".work");
    fs::create_dir_all(work_dir.join("signals")).unwrap();
    let worktree_path = base.join(".worktrees").join("my-stage");
    fs::create_dir_all(&worktree_path).unwrap();
    std::os::unix::fs::symlink(&work_dir, worktree_path.join(".work")).unwrap();

    for project_root in [base.to_path_buf(), worktree_path] {
        write_settings(&default_config(), &project_root).unwrap();
        let settings_path = project_root.join(".claude/settings.local.json");
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(settings_path).unwrap()).unwrap();
        let deny = settings["permissions"]["deny"].as_array().unwrap();
        for entry in deny.iter().filter_map(|value| value.as_str()) {
            let Some(location) = deny_rule_location(entry, &project_root) else {
                continue;
            };
            assert!(
                !location.starts_with(&project_root),
                "{entry} locates at {} inside {}, so every search there prompts",
                location.display(),
                project_root.display()
            );
        }
    }
}
