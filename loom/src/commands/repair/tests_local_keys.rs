//! `loom repair --fix` against the keys loom used to write into the main
//! repo's `.claude/settings.local.json` (plan section 12): exactly those are
//! stripped, every other key stays, and no fix writes a sandbox block.

use std::fs;

use serde_json::{json, Value};

use super::*;
use crate::hooks::HooksConfig;
use crate::plan::schema::PermissionMode;

fn read_settings_local(root: &Path) -> Value {
    let path = root.join(".claude/settings.local.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn find_issue(root: &Path, starts_with: &str) -> RepairIssue {
    check_all_issues(root)
        .into_iter()
        .find(|issue| issue.description.starts_with(starts_with))
        .unwrap_or_else(|| panic!("an issue starting {starts_with:?} must be reported"))
}

/// Injects the loom-written keys that `ensure_loom_hooks_local` never writes
/// but a live main-repo session does: session hook registrations, the
/// sandbox block, `env.LOOM_WORK_DIR`, and the permission rules the issue
/// text below counts.
fn plant_loom_written_keys(settings: &mut Value, root: &Path, work_dir: &str) {
    let hooks_dir = dirs::home_dir().unwrap().join(".claude/hooks/loom");
    let session = HooksConfig::new(hooks_dir, work_dir.into(), PermissionMode::Auto);
    for (event, rules) in session.to_settings_hooks() {
        let slot = &mut settings["hooks"][event.as_str()];
        if slot.is_null() {
            *slot = json!([]);
        }
        let rules = serde_json::to_value(rules).unwrap();
        slot.as_array_mut()
            .unwrap()
            .extend(rules.as_array().unwrap().iter().cloned());
    }
    settings["sandbox"] = json!({ "filesystem": { "allowWrite": ["loom/target", "/srv/out"] } });
    settings["env"]["LOOM_WORK_DIR"] = json!(work_dir);
    settings["permissions"] = json!({
        "defaultMode": "auto",
        "allow": [
            format!("Read(/{work_dir}/signals/**)"),
            format!("Read(/{work_dir}/config.toml)"),
            "Read(.loom/work/handoffs/**)",
            "Edit(.work/handoffs/**)",
            format!("Read(/{}/doc/plans/**)", root.display()),
            "Edit(loom/target)",
            "Edit(//srv/out)",
            "Bash(cargo test:*)"
        ]
    });
}

/// The file `ensure_loom_hooks_local` writes, plus every key section 12
/// lists in the shapes the main-repo and knowledge-stage writers left them,
/// plus a rule the operator approved.
#[test]
fn repair_strips_exactly_the_loom_written_keys_and_keeps_the_rest() {
    let root = tempfile::tempdir().unwrap();
    crate::fs::permissions::ensure_loom_hooks_local(root.path()).unwrap();
    let kept = read_settings_local(root.path());

    let mut settings = kept.clone();
    let work_dir = root.path().join(".loom/work").display().to_string();
    plant_loom_written_keys(&mut settings, root.path(), &work_dir);
    let path = root.path().join(".claude/settings.local.json");
    fs::write(&path, settings.to_string()).unwrap();

    let issue = find_issue(root.path(), "Loom-written keys in");
    let listed = format!(
        "(sandbox block, 7 permission rule(s), {} session hook registration(s), \
         env.LOOM_WORK_DIR)",
        crate::hooks::HookEvent::all().len()
    );
    assert!(
        issue.description.ends_with(&listed),
        "{}",
        issue.description
    );
    assert!(fix_issue(root.path(), &issue).unwrap());

    let mut expected = kept;
    expected["permissions"] = json!({ "defaultMode": "auto", "allow": ["Bash(cargo test:*)"] });
    assert_eq!(read_settings_local(root.path()), expected);
    assert!(crate::fs::permissions::settings_local_hook_drift(root.path()).is_empty());
    assert!(!check_all_issues(root.path())
        .iter()
        .any(|issue| issue.description.starts_with("Loom-written keys in")));
}

/// The fixes that used to regenerate the main settings file from the default
/// sandbox (a missing file, a loom-written `Read(...)` deny) now leave it
/// without a sandbox block.
#[test]
fn no_settings_fix_writes_a_sandbox_block_into_the_main_settings_file() {
    let root = tempfile::tempdir().unwrap();

    let issue = find_issue(
        root.path(),
        "Settings not found (.claude/settings.local.json)",
    );
    assert!(fix_issue(root.path(), &issue).unwrap());
    let created = read_settings_local(root.path());
    assert!(created.get("hooks").is_some(), "{created}");
    assert!(created.get("sandbox").is_none(), "{created}");

    let token_deny = json!({ "permissions": { "deny": ["Read(.loom/work/admin.token)"] } });
    let path = root.path().join(".claude/settings.local.json");
    fs::write(&path, token_deny.to_string()).unwrap();
    let issue = find_issue(root.path(), "Read deny rule in");
    assert!(fix_issue(root.path(), &issue).unwrap());
    let fixed = read_settings_local(root.path());
    assert_eq!(fixed, json!({ "permissions": { "deny": [] } }));
}
