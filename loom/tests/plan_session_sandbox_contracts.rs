//! Contracts of stage planning-sandbox: the per-session settings builder, the settings assert,
//! and the PreToolUse guard of `loom plan hook` (PLAN-feat-planner-06-session-sandbox).

#[path = "plan_session_sandbox_contracts/support.rs"]
mod support;

use std::fs;
use std::path::{Path, PathBuf};

use loom::plan_session::kind::SessionKind;
use loom::plan_session::sandbox::assert_planning_settings;
use serde_json::json;
use support::*;

const CAPSULE_TOOLS: [&str; 7] = [
    "LSP",
    "RemoteTrigger",
    "CronCreate",
    "PushNotification",
    "Monitor",
    "PowerShell",
    "Workflow",
];

fn pressure(plan: &str) -> SessionKind {
    SessionKind::Pressure {
        plan: PathBuf::from(plan),
    }
}

#[test]
fn authoring_settings_deny_every_top_level_entry_but_doc() {
    let f = Fixture::new();
    let settings = f.settings(&SessionKind::Authoring);
    assert_eq!(settings["permissions"]["defaultMode"], "auto");
    let deny = list(&settings, "/sandbox/filesystem/denyWrite");
    for entry in [".git", "loom", "README.md", ".hidden", ".claude", ".loom"] {
        let path = path_str(&f.at(entry));
        assert!(contains(&deny, &path), "{path} must be denied: {deny:?}");
    }
    for entry in fs::read_dir(&f.root).unwrap() {
        let path = entry.unwrap().path();
        let is_doc = path.file_name().unwrap() == "doc";
        let denied = contains(&deny, path_str(&path));
        assert_eq!(denied, !is_doc, "{} in {deny:?}", path.display());
    }
}

#[test]
fn locked_set_collapses_to_directories_with_no_nested_deny() {
    let f = Fixture::new();
    for rel in ["PLAN-a.md", "IN_PROGRESS-PLAN-b.md", "REVIEW-PLAN-b.md"] {
        f.write(&format!("doc/plans/{rel}"), "# plan\n");
    }
    for rel in ["archive/DONE-PLAN-c.md", "briefs/b/x.md", "briefs/b/y.md"] {
        f.write(&format!("doc/plans/{rel}"), "brief\n");
    }
    f.write("doc/plans/briefs/a/z.md", "brief\n");
    let settings = f.settings(&SessionKind::Authoring);
    let deny = list(&settings, "/sandbox/filesystem/denyWrite");
    for rel in [
        "archive",
        "briefs/b",
        "IN_PROGRESS-PLAN-b.md",
        "REVIEW-PLAN-b.md",
    ] {
        let path = path_str(&f.at(&format!("doc/plans/{rel}")));
        assert!(contains(&deny, &path), "{path} missing from {deny:?}");
    }
    let unlisted = ["archive/DONE-PLAN-c.md", "briefs/b/x.md", "briefs/b/y.md"];
    for rel in unlisted.into_iter().chain(["PLAN-a.md", "briefs/a"]) {
        let path = path_str(&f.at(&format!("doc/plans/{rel}")));
        assert!(
            !contains(&deny, &path),
            "{path} must not be listed: {deny:?}"
        );
    }
    for outer in &deny {
        for inner in deny.iter().filter(|inner| *inner != outer) {
            let nested = Path::new(inner).starts_with(outer);
            assert!(!nested, "{inner} lies beneath the denied {outer}");
        }
    }
}

#[test]
fn permission_rules_are_absolute_edit_rules_with_no_write_or_read_deny() {
    let f = Fixture::new();
    f.write("doc/notes/n.md", "note\n");
    f.write("doc/plans/PLAN-a.md", "# a\n");
    let authoring = f.settings(&SessionKind::Authoring);
    let pressure = f.settings(&pressure("doc/plans/PLAN-a.md"));
    for settings in [&authoring, &pressure] {
        let problems = rule_problems(settings, &f.base);
        assert!(problems.is_empty(), "bad permission rules: {problems:?}");
    }
    let allow = list(&authoring, "/permissions/allow");
    assert!(
        contains(&allow, rule("Edit", &f.at("doc/notes"), "/**")),
        "{allow:?}"
    );
    let allow = list(&pressure, "/permissions/allow");
    assert!(
        contains(&allow, rule("Edit", &f.at("doc/plans/PLAN-a.md"), "")),
        "{allow:?}"
    );
}

#[test]
fn summary_settings_allow_only_drafts_and_summary_commands() {
    let f = Fixture::new();
    let settings = f.settings(&SessionKind::Summary);
    assert_eq!(settings["permissions"]["defaultMode"], "default");
    let allow = list(&settings, "/permissions/allow");
    let deny = list(&settings, "/permissions/deny");
    let doc_edit = format!("Edit(/{}", f.at("doc").display());
    assert!(!allow.iter().any(|e| e.starts_with(&doc_edit)), "{allow:?}");
    assert!(!contains(&allow, "WebSearch"), "{allow:?}");
    let bash: Vec<_> = allow.iter().filter(|e| e.starts_with("Bash(")).collect();
    assert_eq!(bash, ["Bash(loom plan summary *)"], "{allow:?}");
    assert!(
        contains(&allow, rule("Edit", &f.runtime.drafts, "/**")),
        "{allow:?}"
    );
    let tools = ["WebSearch", "WebFetch", "SendMessage", "ListAgents"];
    for tool in tools.into_iter().chain(CAPSULE_TOOLS) {
        assert!(contains(&deny, tool), "{tool} missing from {deny:?}");
    }
    let deny_write = list(&settings, "/sandbox/filesystem/denyWrite");
    assert!(
        contains(&deny_write, path_str(&f.at("doc/product"))),
        "{deny_write:?}"
    );
}

#[test]
fn pressure_settings_write_only_the_plan_its_bundle_omissions_and_marker() {
    let f = Fixture::new();
    for slug in ["a", "z"] {
        f.write(&format!("doc/plans/PLAN-{slug}.md"), "# plan\n");
        f.write(&format!("doc/plans/briefs/{slug}/x.md"), "brief\n");
    }
    f.write("planning-omissions.md", "none\n");
    fs::create_dir_all(f.session.done()).unwrap();
    let settings = f.settings(&pressure("doc/plans/PLAN-a.md"));
    assert_eq!(settings["permissions"]["defaultMode"], "auto");
    let allow_write = list(&settings, "/sandbox/filesystem/allowWrite");
    assert_eq!(allow_write, [path_str(&f.session.done())]);
    let allow = list(&settings, "/permissions/allow");
    let wanted = [
        rule("Edit", &f.at("doc/plans/PLAN-a.md"), ""),
        rule("Edit", &f.at("doc/plans/briefs/a"), "/**"),
        rule("Edit", &f.at("planning-omissions.md"), ""),
        format!("Bash(touch {})", f.session.marker().display()),
    ];
    for entry in &wanted {
        assert!(contains(&allow, entry), "{entry} missing from {allow:?}");
    }
    let touches = allow.iter().filter(|e| e.starts_with("Bash(touch")).count();
    assert_eq!(touches, 1, "{allow:?}");
    let deny_write = list(&settings, "/sandbox/filesystem/denyWrite");
    for rel in ["doc/plans/PLAN-z.md", "doc/plans/briefs/z", "doc/product"] {
        assert!(
            contains(&deny_write, path_str(&f.at(rel))),
            "{rel} in {deny_write:?}"
        );
    }
}

#[test]
fn assert_planning_settings_refuses_each_escape_hatch() {
    let f = Fixture::new();
    let good = f.settings(&SessionKind::Authoring);
    assert_planning_settings(&good).expect("freshly built authoring settings pass");
    let cases = [
        ("/permissions/defaultMode", json!("bypassPermissions")),
        ("/permissions/defaultMode", json!("acceptEdits")),
        ("/sandbox/enabled", json!(false)),
        ("/sandbox/allowUnsandboxedCommands", json!(true)),
        ("/sandbox/network/allowedDomains", json!(["example.com"])),
    ];
    for (pointer, value) in cases {
        let mut settings = good.clone();
        set_pointer(&mut settings, pointer, value);
        let refused = assert_planning_settings(&settings).is_err();
        assert!(refused, "{pointer} changed and the assert still passed");
    }
}

#[test]
fn guard_denies_edit_of_a_plan_locked_after_spawn() {
    let f = Fixture::new();
    f.write("doc/plans/PLAN-x.md", "# x\n");
    f.sandbox(&SessionKind::Authoring);
    let open = file_input(&f.at("doc/plans/PLAN-x.md"));
    assert_eq!(f.guard("Edit", open), None, "an unlocked plan is editable");
    let locked = f.at("doc/plans/IN_PROGRESS-PLAN-x.md");
    fs::rename(f.at("doc/plans/PLAN-x.md"), &locked).unwrap();
    let out = f.guard("Edit", file_input(&locked));
    assert_denied(&out, "edit of a running plan");
}

#[test]
fn guard_denies_write_through_a_symlinked_directory_under_doc() {
    let f = Fixture::new();
    let outside = f.base.join("outside");
    fs::create_dir_all(&outside).unwrap();
    link(&outside, &f.at("doc/notes"));
    f.sandbox(&SessionKind::Authoring);
    let out = f.guard("Write", file_input(&f.at("doc/notes/a.md")));
    assert_denied(&out, "write through the doc/notes symlink");
    let allowed = f.guard("Write", file_input(&f.at("doc/product/ok.md")));
    assert_eq!(allowed, None);
}

#[test]
fn guard_prints_nothing_for_an_allowed_doc_write() {
    let f = Fixture::new();
    f.write("doc/product/x.md", "x\n");
    f.sandbox(&SessionKind::Authoring);
    let out = f.guard("Edit", file_input(&f.at("doc/product/x.md")));
    assert_eq!(out, None, "an allow answer would skip the permission rules");
}

#[test]
fn guard_denies_unparsable_oversized_and_policyless_payloads() {
    use loom::plan_session::guard::run_pre_tool_use;
    let f = Fixture::new();
    f.write("doc/product/x.md", "x\n");
    f.sandbox(&SessionKind::Authoring);
    assert_denied(
        &run_pre_tool_use(f.session.path(), &b"not json"[..]),
        "not json",
    );
    let input = file_input(&f.at("doc/product/x.md"));
    let mut body = json!({
        "hook_event_name": "PreToolUse", "tool_name": "Edit", "tool_input": input, "pad": "",
    });
    let empty = serde_json::to_vec(&body).unwrap().len();
    body["pad"] = json!("x".repeat(1024 * 1024 + 1 - empty));
    let big = serde_json::to_vec(&body).unwrap();
    assert_eq!(big.len(), 1024 * 1024 + 1);
    assert_denied(
        &run_pre_tool_use(f.session.path(), big.as_slice()),
        "1 MiB + 1",
    );
    let bare = f.base.join("no-policy");
    fs::create_dir_all(&bare).unwrap();
    let valid = payload("Edit", file_input(&f.at("doc/product/x.md")), &f.root);
    assert_denied(&run_pre_tool_use(&bare, valid.as_slice()), "no guard.json");
}

#[test]
fn guard_denies_reads_of_the_runtime_lock_and_allows_drafts() {
    let f = Fixture::new();
    f.sandbox(&SessionKind::Authoring);
    let answers = f.session.path().join("answers/a.json");
    let lock = f.guard("Read", file_input(&f.runtime.lock));
    assert_denied(&lock, "runtime lock");
    assert_denied(&f.guard("Read", file_input(&answers)), "session answers");
    let drafts = f.guard("Read", file_input(&f.runtime.drafts.join("x.md")));
    assert_eq!(drafts, None, "drafts are in the session's own grants");
}

#[test]
fn guard_allows_reads_of_attachments_and_denies_writes() {
    let f = Fixture::new();
    f.sandbox(&SessionKind::Authoring);
    let upload = f.runtime.attachments.join("0123456789abcdef.png");
    fs::create_dir_all(&f.runtime.attachments).unwrap();
    fs::write(&upload, "x").unwrap();
    let read = f.guard("Read", file_input(&upload));
    assert_eq!(read, None, "attachments are an allowRead carve-out");
    let write = f.guard("Write", file_input(&upload));
    assert_denied(&write, "a write to an attachment");
    let fresh = f.guard("Write", file_input(&f.runtime.attachments.join("new.png")));
    assert_denied(&fresh, "a new file in the attachments directory");
    let settings = f.settings(&SessionKind::Authoring);
    let allow_write = list(&settings, "/sandbox/filesystem/allowWrite");
    assert!(
        !allow_write
            .iter()
            .any(|entry| entry.contains("attachments")),
        "no write grant names attachments: {allow_write:?}"
    );
}

#[test]
fn authoring_and_pressure_settings_deny_the_capsule_tools() {
    let f = Fixture::new();
    f.write("doc/plans/PLAN-a.md", "# a\n");
    let authoring = list(&f.settings(&SessionKind::Authoring), "/permissions/deny");
    let settings = f.settings(&pressure("doc/plans/PLAN-a.md"));
    let pressure = list(&settings, "/permissions/deny");
    for (kind, deny) in [("authoring", &authoring), ("pressure", &pressure)] {
        for tool in CAPSULE_TOOLS {
            assert!(contains(deny, tool), "{kind} lacks {tool}: {deny:?}");
        }
    }
    for tool in ["SendMessage", "ListAgents"] {
        assert!(
            contains(&authoring, tool),
            "authoring lacks {tool}: {authoring:?}"
        );
        assert!(
            !contains(&pressure, tool),
            "pressure must keep {tool}: {pressure:?}"
        );
    }
}

#[test]
fn read_deny_covers_a_runtime_under_a_redirected_loom_home() {
    let f = Fixture::with_loom_home("other-loomhome");
    let settings = f.settings(&SessionKind::Authoring);
    let deny_read = list(&settings, "/sandbox/filesystem/denyRead");
    for root in [
        f.base.join("other-loomhome/plan"),
        f.home.join(".loom/plan"),
    ] {
        assert!(
            contains(&deny_read, path_str(&root)),
            "{root:?} in {deny_read:?}"
        );
    }
    f.sandbox(&SessionKind::Authoring);
    assert_denied(
        &f.guard("Read", file_input(&f.runtime.lock)),
        "redirected runtime lock",
    );
    let drafts = f.guard("Read", file_input(&f.runtime.drafts.join("x.md")));
    assert_eq!(
        drafts, None,
        "drafts of the redirected runtime stay readable"
    );
}

#[test]
fn guard_denies_read_and_grep_through_a_symlink_under_doc() {
    let f = Fixture::new();
    link(&f.home.join(".ssh"), &f.at("doc/notes"));
    f.sandbox(&SessionKind::Authoring);
    let read = f.guard("Read", file_input(&f.at("doc/notes/id_rsa")));
    assert_denied(&read, "read through doc/notes");
    let grep = f.guard(
        "Grep",
        json!({ "pattern": "BEGIN", "path": f.at("doc/notes") }),
    );
    assert_denied(&grep, "grep of the doc/notes symlink");
    let allowed = f.guard("Read", file_input(&f.at("doc/product/spec.md")));
    assert_eq!(allowed, None);
}

#[test]
fn guard_denies_reads_through_proc_and_a_symlinked_root() {
    let f = Fixture::new();
    f.sandbox(&SessionKind::Authoring);
    let token = Path::new("/proc/self/cwd/.loom/work/admin.token");
    assert_denied(
        &f.guard("Read", file_input(token)),
        "read through /proc/self/cwd",
    );
    let g = Fixture::with_root("real/proj");
    g.sandbox(&SessionKind::Authoring);
    link(&g.base.join("real"), &g.base.join("link"));
    let via_link = g.base.join("link/proj");
    let secret = via_link.join(".loom/work/user.token");
    assert_denied(
        &g.guard("Read", file_input(&secret)),
        "token via symlinked ancestor",
    );
    let source = g.guard("Read", file_input(&via_link.join("loom/src/lib.rs")));
    assert_eq!(source, None, "ordinary source stays readable via the link");
}

#[test]
fn loom_plan_hook_answers_through_the_binary() {
    let f = Fixture::new();
    let bare = f.base.join("no-guard");
    fs::create_dir_all(&bare).unwrap();
    let pre = run_hook(&f, "PreToolUse", &bare);
    let stderr = String::from_utf8_lossy(&pre.stderr);
    assert_eq!(pre.status.code(), Some(0), "{stderr}");
    let stdout = String::from_utf8(pre.stdout).unwrap();
    assert_denied(&Some(stdout), "PreToolUse without guard.json");
    let stop = run_hook(&f, "Stop", &bare);
    let stderr = String::from_utf8_lossy(&stop.stderr);
    assert_eq!(stop.status.code(), Some(0), "{stderr}");
    let stdout = String::from_utf8_lossy(&stop.stdout);
    assert!(stop.stdout.is_empty(), "{stdout}");
}
