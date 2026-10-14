//! `build_settings` drops the `allow_write` entries that lie inside the
//! session's working directory from both the OS write list and the `Edit(...)`
//! rules (`grant_paths::is_inside_cwd`).

use super::tests::default_config;
use super::{build_settings, SettingsTarget};
use serde_json::{json, Value};
use serial_test::serial;
use std::path::Path;
use tempfile::TempDir;

fn settings_in(cwd: Option<&Path>, allow_write: &[String]) -> Value {
    let mut config = default_config();
    config.filesystem.allow_write = allow_write.to_vec();
    build_settings(
        &config,
        &SettingsTarget {
            is_worktree: true,
            state_root: None,
            existing: &json!({}),
            carry_plugin_keys: false,
            cwd,
        },
    )
    .unwrap()
}

/// The `sandbox.filesystem.allowWrite` list and the `permissions.allow` list.
fn grants(settings: &Value) -> (Vec<String>, Vec<String>) {
    let list = |pointer: &str| -> Vec<String> {
        settings
            .pointer(pointer)
            .and_then(Value::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    (
        list("/sandbox/filesystem/allowWrite"),
        list("/permissions/allow"),
    )
}

fn path_string(path: &Path) -> String {
    path.to_str().expect("a UTF-8 temp path").to_string()
}

#[test]
fn entries_inside_the_cwd_are_dropped_from_both_layers() {
    let cwd = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    std::fs::write(cwd.path().join("vitest.config.ts"), "").unwrap();
    std::fs::create_dir(cwd.path().join("dist")).unwrap();
    let inside_absolute = path_string(&cwd.path().join("src/x.ts"));
    let outside_cache = path_string(&outside.path().join("cache"));
    let entries: Vec<String> = [
        "vitest.config.ts",
        "dist",
        "missing/file.txt",
        inside_absolute.as_str(),
        "src/**",
        outside_cache.as_str(),
    ]
    .iter()
    .map(|entry| entry.to_string())
    .collect();

    let (allow_write, allow) = grants(&settings_in(Some(cwd.path()), &entries));

    let cwd_text = path_string(cwd.path());
    for grant in allow_write.iter().chain(&allow) {
        assert!(!grant.contains(&cwd_text), "{grant} names the cwd");
    }
    for dropped in ["vitest.config.ts", "dist", "missing/file.txt"] {
        assert!(!allow_write.iter().any(|g| g == dropped), "{allow_write:?}");
        let rule = format!("Edit({dropped})");
        assert!(!allow.contains(&rule), "{allow:?}");
    }
    assert!(
        allow_write.contains(&"src/**".to_string()),
        "{allow_write:?}"
    );
    assert!(allow.contains(&"Edit(src/**)".to_string()), "{allow:?}");
    assert!(allow_write.contains(&outside_cache), "{allow_write:?}");
    assert!(
        allow.contains(&format!("Edit(/{outside_cache})")),
        "{allow:?}"
    );
}

#[cfg(unix)]
#[test]
fn an_entry_through_a_symlink_that_leaves_the_cwd_is_kept() {
    let cwd = TempDir::new().unwrap();
    let state = TempDir::new().unwrap();
    std::fs::create_dir(cwd.path().join(".loom")).unwrap();
    std::os::unix::fs::symlink(state.path(), cwd.path().join(".loom/work")).unwrap();
    let entry = ".loom/work/handoffs".to_string();

    let (allow_write, allow) = grants(&settings_in(Some(cwd.path()), std::slice::from_ref(&entry)));

    assert!(allow_write.contains(&entry), "{allow_write:?}");
    assert!(allow.contains(&format!("Edit({entry})")), "{allow:?}");
}

#[test]
fn no_cwd_keeps_every_entry() {
    let cwd = TempDir::new().unwrap();
    std::fs::write(cwd.path().join("vitest.config.ts"), "").unwrap();
    std::fs::create_dir(cwd.path().join("dist")).unwrap();
    let entries = ["vitest.config.ts".to_string(), "dist".to_string()];

    let (allow_write, allow) = grants(&settings_in(None, &entries));

    for kept in &entries {
        assert!(allow_write.contains(kept), "{allow_write:?}");
        assert!(allow.contains(&format!("Edit({kept})")), "{allow:?}");
    }
}

#[test]
#[serial]
fn a_tilde_entry_inside_the_cwd_is_dropped() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    // The temp dir when it lies under home, else home itself, so a `~/`
    // spelling always has a cwd to land in.
    let temp = TempDir::new().unwrap();
    let cwd = if temp.path().starts_with(&home) {
        temp.path()
    } else {
        home.as_path()
    };
    let relative = cwd.strip_prefix(&home).unwrap().join("inner");
    let entry = format!("~/{}", relative.display());

    let (allow_write, allow) = grants(&settings_in(Some(cwd), std::slice::from_ref(&entry)));

    assert!(!allow_write.contains(&entry), "{allow_write:?}");
    assert!(!allow.contains(&format!("Edit({entry})")), "{allow:?}");
}
