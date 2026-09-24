//! Unit tests for `session_settings/contents.rs`: what a capsule carries over
//! from the checkout's own settings.

use super::tests_contents::{build, sandbox, strings};
use crate::models::session::SessionType;
use crate::sandbox::MergedSandboxConfig;
use serde_json::{json, Value};

fn stage_with_checkout(config: &MergedSandboxConfig, checkout: &Value, rooted: bool) -> Value {
    build(SessionType::Stage, config, &[], Some(checkout), rooted)
}

#[test]
fn plugin_keys_follow_the_codex_license() {
    let checkout = json!({
        "enabledPlugins": {"codex@openai-codex": true},
        "extraKnownMarketplaces": {"openai-codex": {}}
    });
    for worktree_rooted in [true, false] {
        let licensed = stage_with_checkout(&sandbox(true), &checkout, worktree_rooted);
        assert_eq!(licensed["enabledPlugins"], checkout["enabledPlugins"]);
        assert_eq!(
            licensed["extraKnownMarketplaces"],
            checkout["extraKnownMarketplaces"]
        );

        let unlicensed = stage_with_checkout(&sandbox(false), &checkout, worktree_rooted);
        assert!(unlicensed.get("enabledPlugins").is_none());
        assert!(unlicensed.get("extraKnownMarketplaces").is_none());
    }
}

#[test]
fn the_checkouts_denies_are_carried_and_escape_rules_follow_the_location() {
    let checkout = json!({"permissions": {"deny": [
        "Bash(rm -rf:*)",
        "Read(//secret/**)",
        "Edit(doc/loom/knowledge/**)",
        "Edit(.worktrees/other/**)"
    ]}});
    let mut config = sandbox(false);
    config
        .filesystem
        .deny_write
        .push(".worktrees/other/**".to_string());

    let from_checkout = build(SessionType::Merge, &config, &[], Some(&checkout), false);
    let deny = strings(&from_checkout, "/permissions/deny");
    assert!(deny.contains(&"Bash(rm -rf:*)".to_string()), "{deny:?}");
    assert!(
        !deny.iter().any(|rule| rule.starts_with("Read(")
            || rule.contains("doc/loom/knowledge")
            || rule == "Edit(.worktrees/other/**)"),
        "{deny:?}"
    );
    let deny_write = strings(&from_checkout, "/sandbox/filesystem/denyWrite");
    assert!(
        !deny_write.contains(&".worktrees/other/**".to_string()),
        "{deny_write:?}"
    );

    let from_worktree = build(SessionType::Stage, &config, &[], Some(&checkout), true);
    assert!(strings(&from_worktree, "/permissions/deny")
        .contains(&"Edit(.worktrees/other/**)".to_string()));
    assert!(strings(&from_worktree, "/sandbox/filesystem/denyWrite")
        .contains(&".worktrees/other/**".to_string()));
}
