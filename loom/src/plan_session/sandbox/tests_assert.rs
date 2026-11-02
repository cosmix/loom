//! Hook registration and per-key refusal tests of `assert_planning_settings`.

use serde_json::{json, Value};

use super::tests_kinds::Fixture;
use super::tests_kinds_fixture::expected_common;
use super::{assert_planning_settings, HookSpec};
use crate::plan_session::kind::SessionKind;

#[test]
fn settings_permission_mode_matches_session_kind() {
    let f = Fixture::new();
    let cases = [
        (SessionKind::Authoring, "auto"),
        (
            SessionKind::Pressure {
                plan: "doc/plans/PLAN-a.md".into(),
            },
            "auto",
        ),
        (SessionKind::Summary, "default"),
    ];
    for (kind, expected) in cases {
        let settings = f.build(&kind, &[]).unwrap();
        assert_eq!(settings["permissions"]["defaultMode"], expected, "{kind:?}");
    }
}

#[test]
fn hooks_keep_guard_and_register_extras_in_order_within_each_event() {
    let f = Fixture::new();
    let extra = [
        HookSpec {
            event: "Stop",
            matcher: None,
            timeout_secs: None,
        },
        HookSpec {
            event: "SessionStart",
            matcher: Some("startup"),
            timeout_secs: Some(7),
        },
        HookSpec {
            event: "Stop",
            matcher: Some("end"),
            timeout_secs: Some(3),
        },
    ];
    let settings = f.build(&SessionKind::Authoring, &extra).unwrap();
    // JSON object key order is not observable (serde_json sorts keys), so only the set of
    // events and the order of the array within one event are asserted.
    let hooks = settings["hooks"].as_object().unwrap();
    let mut events: Vec<_> = hooks.keys().map(String::as_str).collect();
    events.sort_unstable();
    assert_eq!(events, ["PreToolUse", "SessionStart", "Stop"]);
    let stop = hooks["Stop"].as_array().unwrap();
    assert_eq!(stop.len(), 2);
    assert!(
        stop[0].get("matcher").is_none(),
        "first registered Stop hook has no matcher"
    );
    assert_eq!(stop[1]["matcher"], "end");
    let mut expected = expected_common(&f)["hooks"].clone();
    expected["Stop"] = json!([
        {"hooks": [{"type": "command", "command": f.command("Stop")}]},
        {"matcher": "end", "hooks": [
            {"type": "command", "command": f.command("Stop"), "timeout": 3}]}
    ]);
    expected["SessionStart"] = json!([{"matcher": "startup", "hooks": [
        {"type": "command", "command": f.command("SessionStart"), "timeout": 7}]}]);
    assert_eq!(settings["hooks"], expected);
}

#[test]
fn extra_pre_tool_use_hook_is_refused() {
    let extra = [HookSpec {
        event: "PreToolUse",
        matcher: None,
        timeout_secs: None,
    }];
    let error = Fixture::new()
        .build(&SessionKind::Authoring, &extra)
        .unwrap_err();
    assert!(error.to_string().contains("PreToolUse"), "{error}");
}

fn refused_after(pointer: &str, mutate: impl FnOnce(&mut Value)) {
    let mut settings = Fixture::new().build(&SessionKind::Authoring, &[]).unwrap();
    assert_planning_settings(&settings).unwrap();
    mutate(&mut settings);
    let error = assert_planning_settings(&settings)
        .expect_err(pointer)
        .to_string();
    assert!(error.contains(pointer), "{pointer} not named in {error}");
}

macro_rules! key_refusal {
    ($name:ident, $pointer:literal, $value:tt) => {
        #[test]
        fn $name() {
            refused_after($pointer, |settings| {
                let (parent, key) = $pointer.rsplit_once('/').unwrap();
                settings.pointer_mut(parent).unwrap()[key] = json!($value);
            });
        }
    };
}
key_refusal!(
    refuses_default_mode,
    "/permissions/defaultMode",
    "acceptEdits"
);
key_refusal!(
    refuses_bypass_permissions_mode,
    "/permissions/defaultMode",
    "bypassPermissions"
);
key_refusal!(
    refuses_bypass_mode,
    "/permissions/disableBypassPermissionsMode",
    "enable"
);
key_refusal!(refuses_disabled_sandbox, "/sandbox/enabled", false);
key_refusal!(
    refuses_optional_sandbox,
    "/sandbox/failIfUnavailable",
    false
);
key_refusal!(
    refuses_unsandboxed_commands,
    "/sandbox/allowUnsandboxedCommands",
    true
);
key_refusal!(
    refuses_auto_allowed_bash,
    "/sandbox/autoAllowBashIfSandboxed",
    true
);
key_refusal!(refuses_bg_isolation, "/worktree/bgIsolation", "worktree");
key_refusal!(
    refuses_allowed_domains,
    "/sandbox/network/allowedDomains",
    []
);
key_refusal!(
    refuses_local_binding,
    "/sandbox/network/allowLocalBinding",
    false
);
key_refusal!(
    refuses_unix_sockets,
    "/sandbox/network/allowUnixSockets",
    []
);
key_refusal!(
    refuses_all_unix_sockets,
    "/sandbox/network/allowAllUnixSockets",
    false
);

macro_rules! rule_refusal {
    ($name:ident, $list:literal, $rule:literal) => {
        #[test]
        fn $name() {
            refused_after(concat!("/permissions/", $list), |settings| {
                settings["permissions"][$list]
                    .as_array_mut()
                    .unwrap()
                    .push(json!($rule));
            });
        }
    };
}
rule_refusal!(refuses_write_allow, "allow", "Write(//repo/doc/**)");
rule_refusal!(refuses_write_ask, "ask", "Write(//repo/doc/**)");
rule_refusal!(refuses_write_deny, "deny", "Write(//repo/doc/**)");
rule_refusal!(refuses_read_deny, "deny", "Read(//repo/**)");

macro_rules! deny_refusal {
    ($name:ident, $tool:literal) => {
        #[test]
        fn $name() {
            refused_after("/permissions/deny", |settings| {
                settings["permissions"]["deny"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|entry| entry != $tool);
            });
        }
    };
}
deny_refusal!(refuses_missing_lsp, "LSP");
deny_refusal!(refuses_missing_remote_trigger, "RemoteTrigger");
deny_refusal!(refuses_missing_cron_create, "CronCreate");
deny_refusal!(refuses_missing_push_notification, "PushNotification");
deny_refusal!(refuses_missing_monitor, "Monitor");
deny_refusal!(refuses_missing_power_shell, "PowerShell");
deny_refusal!(refuses_missing_workflow, "Workflow");
deny_refusal!(refuses_missing_send_message, "SendMessage");
deny_refusal!(refuses_missing_list_agents, "ListAgents");
deny_refusal!(refuses_missing_ask_user_question, "AskUserQuestion");

#[test]
fn refuses_missing_pre_tool_use() {
    refused_after("/hooks/PreToolUse", |settings| {
        settings["hooks"]
            .as_object_mut()
            .unwrap()
            .remove("PreToolUse");
    });
}
