//! Sandbox allowances that make the codex implementation lane runnable.
//!
//! Codex is a subprocess, so the Bash sandbox — not Claude Code's tool
//! permissions — decides whether it can run. Its write set is the working
//! directory plus the session temp dir, and codex keeps state in two
//! directories outside both, so without an explicit grant every forward dies
//! before the model is reached. See [`crate::codex::CODEX_SANDBOX_WRITE_PATHS`]
//! for the failure modes and why the sandbox escape hatch is not an answer.
//!
//! These live here rather than in the sandbox settings generator because they
//! also have to reach `loom init`, which writes hooks, env and permission rules
//! without ever running that generator.

use serde_json::{json, Value};
use std::fs;
use std::path::Path;

use crate::codex::{CODEX_SANDBOX_DOMAINS, CODEX_SANDBOX_WRITE_PATHS};

/// The `sandbox` subsections carrying the lane's allowances, and their entries.
const ALLOWANCES: [(&str, &str, &[&str]); 2] = [
    (
        "filesystem",
        "allowWrite",
        CODEX_SANDBOX_WRITE_PATHS.as_slice(),
    ),
    (
        "network",
        "allowedDomains",
        CODEX_SANDBOX_DOMAINS.as_slice(),
    ),
];

/// Ensure the codex lane's sandbox allowances are present in a settings document.
///
/// Entries are merged, never replaced: a repo that already widened these arrays
/// keeps what it had. Returns `true` if anything was added.
pub fn merge_allowances(settings_obj: &mut serde_json::Map<String, Value>) -> bool {
    let Some(sandbox) = settings_obj
        .entry("sandbox")
        .or_insert_with(|| json!({}))
        .as_object_mut()
    else {
        return false;
    };

    let mut changed = false;
    for (section, key, values) in ALLOWANCES {
        let Some(entries) = sandbox
            .entry(section)
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .and_then(|section| {
                section
                    .entry(key)
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
            })
        else {
            continue;
        };
        for value in values {
            if !entries.iter().any(|existing| existing == value) {
                entries.push(json!(value));
                changed = true;
            }
        }
    }

    changed
}

/// Whether `.claude/settings.local.json` already grants those allowances.
///
/// `loom repair` needs this as a check of its own: repair is issue-driven, and
/// a repo whose settings file merely predates these entries reports no other
/// problem — so without it `--fix` inspects a codex-blocked repo and does
/// nothing, which is exactly how this went unfixed.
pub fn settings_local_has_allowances(repo_root: &Path) -> bool {
    let Ok(content) = fs::read_to_string(repo_root.join(".claude/settings.local.json")) else {
        return false;
    };
    let Ok(settings) = serde_json::from_str::<Value>(&content) else {
        return false;
    };

    ALLOWANCES.iter().all(|(section, key, values)| {
        let entries = settings
            .pointer(&format!("/sandbox/{section}/{key}"))
            .and_then(Value::as_array);
        entries.is_some_and(|entries| {
            values
                .iter()
                .all(|value| entries.iter().any(|entry| entry == value))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::permissions::ensure_loom_hooks_local;
    use tempfile::TempDir;

    fn write_settings_local(repo_root: &Path, settings: &Value) {
        let claude_dir = repo_root.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("settings.local.json"),
            serde_json::to_string_pretty(settings).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn merge_appends_only_what_is_missing() {
        let mut settings = json!({
            "sandbox": { "filesystem": { "allowWrite": ["~/.cache/project"] } }
        });
        let obj = settings.as_object_mut().unwrap();

        assert!(merge_allowances(obj));
        let allow_write = settings["sandbox"]["filesystem"]["allowWrite"]
            .as_array()
            .unwrap();
        assert!(allow_write.iter().any(|p| p == "~/.cache/project"));
        for path in CODEX_SANDBOX_WRITE_PATHS {
            assert!(allow_write.iter().any(|p| p == path), "missing {path}");
        }
        let domains = settings["sandbox"]["network"]["allowedDomains"]
            .as_array()
            .unwrap();
        for domain in CODEX_SANDBOX_DOMAINS {
            assert!(domains.iter().any(|d| d == domain), "missing {domain}");
        }

        // Idempotent: a second merge reports no change and adds nothing.
        let obj = settings.as_object_mut().unwrap();
        assert!(!merge_allowances(obj));
        assert_eq!(
            settings["sandbox"]["filesystem"]["allowWrite"]
                .as_array()
                .unwrap()
                .len(),
            1 + CODEX_SANDBOX_WRITE_PATHS.len()
        );
    }

    #[test]
    fn detects_a_settings_file_written_before_the_lane_existed() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Missing file
        assert!(!settings_local_has_allowances(repo_root));

        // Complete-looking but codex-blocked: hooks, env and a sandbox block
        // all present, no grant. The shape `loom repair --fix` walked past.
        write_settings_local(
            repo_root,
            &json!({
                "hooks": { "PreToolUse": [] },
                "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" },
                "sandbox": { "filesystem": { "denyRead": ["~/.ssh/**"] } }
            }),
        );
        assert!(!settings_local_has_allowances(repo_root));

        // Partial coverage is still a miss — one missing path blocks codex
        // exactly as thoroughly as all of them.
        write_settings_local(
            repo_root,
            &json!({
                "sandbox": {
                    "filesystem": { "allowWrite": [CODEX_SANDBOX_WRITE_PATHS[0]] },
                    "network": { "allowedDomains": CODEX_SANDBOX_DOMAINS }
                }
            }),
        );
        assert!(!settings_local_has_allowances(repo_root));

        // The repair path closes it.
        ensure_loom_hooks_local(repo_root).unwrap();
        assert!(settings_local_has_allowances(repo_root));
    }
}
