//! The self-check a planning session's settings pass before they are written.
//!
//! It mirrors the refusals of `sandbox::config::validate_config` (`sandbox/config.rs:155-175`)
//! on the emitted JSON, since a planning session has no `MergedSandboxConfig`, and adds the
//! planning-specific ones of section 12. Each refusal names the key it found wrong.

use anyhow::{bail, Result};
use serde_json::{json, Value};

use super::permissions::{tool_denies, ASK_USER_QUESTION};
use super::{PRE_TOOL_USE, PRE_TOOL_USE_MATCHER};

/// `sandbox.network` keys that open a way out of the sandbox.
const NETWORK_ESCAPES: [&str; 4] = [
    "allowedDomains",
    "allowLocalBinding",
    "allowUnixSockets",
    "allowAllUnixSockets",
];

/// Refuse `settings` unless every planning invariant holds: the permission mode and bypass
/// lock, the sandbox switches, no network escape, no `Write(` rule and no `Read(` deny, every
/// tool deny of the kind plus `AskUserQuestion`, no background worktree isolation, and the
/// guard's `PreToolUse` hook.
pub fn assert_planning_settings(settings: &Value) -> Result<()> {
    let expected = [
        ("/permissions/defaultMode", json!("default")),
        (
            "/permissions/disableBypassPermissionsMode",
            json!("disable"),
        ),
        ("/sandbox/enabled", json!(true)),
        ("/sandbox/failIfUnavailable", json!(true)),
        ("/sandbox/allowUnsandboxedCommands", json!(false)),
        ("/sandbox/autoAllowBashIfSandboxed", json!(false)),
        ("/worktree/bgIsolation", json!("none")),
    ];
    for (pointer, wanted) in expected {
        let found = settings.pointer(pointer);
        if found != Some(&wanted) {
            let found = found.map_or_else(|| "nothing".to_owned(), Value::to_string);
            bail!("planning settings: {pointer} must be {wanted}, found {found}");
        }
    }
    for key in NETWORK_ESCAPES {
        if settings
            .pointer(&format!("/sandbox/network/{key}"))
            .is_some()
        {
            bail!("planning settings: /sandbox/network/{key} must be absent");
        }
    }
    check_rules(settings)?;
    check_guard_hook(settings)
}

/// No `Write(` rule anywhere, no `Read(` deny, and every required deny present. The peer tools
/// are required unless the settings are a pressure session's, the only kind that allows
/// `Bash(touch <marker>)`.
fn check_rules(settings: &Value) -> Result<()> {
    let allow = strings(settings, "/permissions/allow")?;
    let ask = strings(settings, "/permissions/ask")?;
    let deny = strings(settings, "/permissions/deny")?;
    let lists = [("allow", &allow), ("ask", &ask), ("deny", &deny)];
    for (section, list) in lists {
        if let Some(rule) = list.iter().find(|rule| rule.starts_with("Write(")) {
            bail!("planning settings: /permissions/{section} holds the inert rule {rule}");
        }
    }
    if let Some(rule) = deny.iter().find(|rule| rule.starts_with("Read(")) {
        bail!("planning settings: /permissions/deny holds the read deny {rule}");
    }
    let pressure = allow.iter().any(|rule| rule.starts_with("Bash(touch "));
    let required = tool_denies(!pressure)
        .into_iter()
        .chain([ASK_USER_QUESTION]);
    for tool in required {
        if !deny.contains(&tool) {
            bail!("planning settings: /permissions/deny lacks {tool}");
        }
    }
    Ok(())
}

/// A `PreToolUse` entry with the guard's matcher running `loom plan hook PreToolUse`.
fn check_guard_hook(settings: &Value) -> Result<()> {
    let pointer = format!("/hooks/{PRE_TOOL_USE}");
    let entries = settings.pointer(&pointer).and_then(Value::as_array);
    let runs_guard = |hook: &Value| {
        hook["type"] == "command"
            && hook["command"]
                .as_str()
                .is_some_and(|command| command.contains(" plan hook PreToolUse "))
    };
    let present = entries.into_iter().flatten().any(|entry| {
        entry["matcher"] == PRE_TOOL_USE_MATCHER
            && entry["hooks"]
                .as_array()
                .is_some_and(|hooks| hooks.iter().any(runs_guard))
    });
    if !present {
        bail!("planning settings: {pointer} lacks the planning guard hook");
    }
    Ok(())
}

/// The strings of the array at `pointer`; anything else there is refused.
fn strings<'v>(settings: &'v Value, pointer: &str) -> Result<Vec<&'v str>> {
    let Some(items) = settings.pointer(pointer).and_then(Value::as_array) else {
        bail!("planning settings: {pointer} must be a list");
    };
    let mut list = Vec::with_capacity(items.len());
    for item in items {
        let Some(text) = item.as_str() else {
            bail!("planning settings: {pointer} holds a non-string entry {item}");
        };
        list.push(text);
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> Value {
        json!({
            "sandbox": {
                "enabled": true, "failIfUnavailable": true,
                "autoAllowBashIfSandboxed": false, "allowUnsandboxedCommands": false,
                "network": { "strictAllowlist": true },
            },
            "permissions": {
                "defaultMode": "default", "disableBypassPermissionsMode": "disable",
                "allow": ["Edit(//p/doc/product/**)"], "ask": ["WebFetch"],
                "deny": ["LSP", "RemoteTrigger", "CronCreate", "PushNotification", "Workflow",
                         "SendMessage", "ListAgents", "AskUserQuestion"],
            },
            "worktree": { "bgIsolation": "none" },
            "hooks": { "PreToolUse": [{ "matcher": PRE_TOOL_USE_MATCHER, "hooks": [
                { "type": "command", "command": "/bin/loom plan hook PreToolUse --session-dir /s",
                  "timeout": 10 }] }] },
        })
    }

    fn set(settings: &mut Value, pointer: &str, value: Value) {
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        let object = settings
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap();
        object.insert(key.to_owned(), value);
    }

    /// `settings` must be refused with a message naming `key`.
    fn refused(settings: &Value, key: &str) {
        let error = assert_planning_settings(settings)
            .expect_err(key)
            .to_string();
        assert!(error.contains(key), "{key} not named in: {error}");
    }

    #[test]
    fn good_settings_pass() {
        assert_planning_settings(&good()).unwrap();
    }

    #[test]
    fn refuses_each_wrong_switch() {
        let cases = [
            ("/permissions/defaultMode", json!("acceptEdits")),
            ("/permissions/disableBypassPermissionsMode", json!("enable")),
            ("/sandbox/enabled", json!(false)),
            ("/sandbox/failIfUnavailable", json!(false)),
            ("/sandbox/allowUnsandboxedCommands", json!(true)),
            ("/sandbox/autoAllowBashIfSandboxed", json!(true)),
            ("/worktree/bgIsolation", json!("worktree")),
        ];
        for (pointer, value) in cases {
            let mut settings = good();
            set(&mut settings, pointer, value);
            refused(&settings, pointer);
        }
    }

    #[test]
    fn refuses_a_missing_switch() {
        let mut settings = good();
        settings["sandbox"]
            .as_object_mut()
            .unwrap()
            .remove("enabled");
        refused(&settings, "/sandbox/enabled");
    }

    #[test]
    fn refuses_each_network_escape() {
        for key in NETWORK_ESCAPES {
            let mut settings = good();
            set(&mut settings, &format!("/sandbox/network/{key}"), json!([]));
            refused(&settings, key);
        }
    }

    #[test]
    fn refuses_write_rules_and_read_denies() {
        for section in ["allow", "ask", "deny"] {
            let mut settings = good();
            let list = settings["permissions"][section].as_array_mut().unwrap();
            list.push(json!("Write(//p/doc/**)"));
            refused(&settings, &format!("/permissions/{section}"));
        }
        let mut settings = good();
        let deny = settings["permissions"]["deny"].as_array_mut().unwrap();
        deny.push(json!("Read(//p/.loom/work/**)"));
        refused(&settings, "Read(//p/.loom/work/**)");
    }

    #[test]
    fn refuses_each_missing_tool_deny() {
        let required = good()["permissions"]["deny"].as_array().unwrap().clone();
        for tool in required.iter().filter_map(Value::as_str) {
            let mut settings = good();
            deny_without(&mut settings, tool);
            refused(&settings, tool);
        }
    }

    #[test]
    fn pressure_settings_keep_the_peer_tools() {
        let mut settings = good();
        let allow = settings["permissions"]["allow"].as_array_mut().unwrap();
        allow.push(json!("Bash(touch /s/done/complete)"));
        deny_without(&mut settings, "SendMessage");
        deny_without(&mut settings, "ListAgents");
        assert_planning_settings(&settings).unwrap();
        deny_without(&mut settings, "Workflow");
        refused(&settings, "Workflow");
    }

    fn deny_without(settings: &mut Value, tool: &str) {
        let deny = settings["permissions"]["deny"].as_array_mut().unwrap();
        deny.retain(|entry| entry != tool);
    }

    #[test]
    fn refuses_a_missing_or_foreign_guard_hook() {
        let mut settings = good();
        settings["hooks"]
            .as_object_mut()
            .unwrap()
            .remove("PreToolUse");
        refused(&settings, "/hooks/PreToolUse");
        let mut settings = good();
        settings["hooks"]["PreToolUse"][0]["matcher"] = json!("Bash");
        refused(&settings, "/hooks/PreToolUse");
        let mut settings = good();
        settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"] = json!("true");
        refused(&settings, "/hooks/PreToolUse");
    }

    #[test]
    fn refuses_a_missing_or_malformed_rule_list() {
        let mut settings = good();
        settings["permissions"]
            .as_object_mut()
            .unwrap()
            .remove("deny");
        refused(&settings, "/permissions/deny");
        let mut settings = good();
        settings["permissions"]["allow"] = json!([1]);
        refused(&settings, "/permissions/allow");
    }
}
