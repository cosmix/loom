//! Tests for `fold_back_approvals`' `Read(...)` deny handling.
//!
//! The fold-back records only allow rules into the loom-owned
//! approved-permissions list; it never writes a settings file for a
//! `Read(...)` deny to leak into.

use super::sync_tests::create_state_root;
use crate::fs::permissions::approved::approved_path;
use crate::fs::permissions::fold_back_approvals;
use serde_json::{json, Value};
use std::fs;
use tempfile::TempDir;

/// Fold back a main checkout whose local settings carry an allow-side
/// `Read(src/**)` grant and five `Read(...)` denies of mixed shape
/// (token-style, operator-style, glob-style). Returns how many rules were
/// recorded and the loom-owned approved-permissions list.
fn sync_with_mixed_read_denies() -> (usize, Value) {
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());

    let claude_dir = main_dir.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let settings = json!({
        "permissions": {
            "allow": ["Read(src/**)"],
            "deny": [
                "Read(../.work/admin.token)",
                "Read(.loom/work/user.token)",
                "Read(//home/x/src/*/.work/admin.token)",
                "Read(../doc/**)",
                "Read(secrets/**)"
            ]
        }
    });
    fs::write(
        claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();

    let added = fold_back_approvals(main_dir.path());

    let content = fs::read_to_string(approved_path(&state_root)).unwrap();
    let approved: Value = serde_json::from_str(&content).unwrap();

    (added, approved)
}

/// No `Read(...)` deny, of any shape, is ever promoted into the approved
/// list; the allow-side `Read(...)` grant is recorded normally.
#[test]
fn test_sync_never_promotes_read_denies() {
    let (added, _approved) = sync_with_mixed_read_denies();

    assert_eq!(added, 1);
}

/// The loom-owned approved-permissions list carries the allow grant but no
/// `Read(...)` deny at all — the list only ever holds allow rules, so there
/// is no `deny` array for one to hide in either.
#[test]
fn test_sync_read_denies_absent_from_main_settings() {
    let (_added, approved) = sync_with_mixed_read_denies();

    let allow = approved["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Read(src/**)"));
    assert!(
        approved.get("deny").is_none(),
        "the approved-permissions list must carry no deny array, got: {approved:?}"
    );
    assert!(
        !allow
            .iter()
            .any(|v| v.as_str().is_some_and(|s| s.starts_with("Read(") && s != "Read(src/**)")),
        "no Read(...) deny entry, of any shape, may reach the approved-permissions list, got: {allow:?}"
    );
}
