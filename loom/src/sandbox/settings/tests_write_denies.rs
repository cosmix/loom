use super::tests::{build_settings_for, default_config};
use serde_json::json;
use std::path::Path;

#[test]
fn worktree_capsule_translates_plan_write_denies_for_native_tools() {
    let mut config = default_config();
    config.filesystem.deny_write = [
        "/repo/.worktrees/stage/protected.rs",
        "relative/file",
        "~/private",
        "//already/absolute",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let settings = build_settings_for(&config, Path::new("/repo/.worktrees/stage"), &json!({}));
    let os = settings["sandbox"]["filesystem"]["denyWrite"]
        .as_array()
        .unwrap();
    let native = settings["permissions"]["deny"].as_array().unwrap();
    for (path, rule) in [
        (
            "/repo/.worktrees/stage/protected.rs",
            "Edit(//repo/.worktrees/stage/protected.rs)",
        ),
        ("relative/file", "Edit(relative/file)"),
        ("~/private", "Edit(~/private)"),
        ("//already/absolute", "Edit(//already/absolute)"),
    ] {
        assert!(os.contains(&json!(path)), "missing OS deny {path}");
        assert!(native.contains(&json!(rule)), "missing native deny {rule}");
    }
}
