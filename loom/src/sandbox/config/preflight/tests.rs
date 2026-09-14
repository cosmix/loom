//! Each refusal of the sandbox preflight with a failing input and a passing
//! control, over injected host facts and temporary directories only.

use super::hook_commands::HookDocument;
use super::local_settings::strip_loom_written_keys;
use super::*;
use crate::fs::permissions::{guard_hooks_config, install_loom_hooks_to};
use crate::hooks::HooksConfig;
use serde_json::{json, Map, Value};
use std::os::unix::fs::{symlink, PermissionsExt};
use tempfile::TempDir;

/// A host whose every fact passes: this build's hooks and a loom binary in
/// `temp`, and one writable root beside them.
fn clean_host(temp: &TempDir) -> HostFacts {
    let hooks_dir = temp.path().join("hooks");
    install_loom_hooks_to(&hooks_dir).unwrap();
    let loom_bin = temp.path().join("loom");
    std::fs::write(&loom_bin, "").unwrap();
    let writable = temp.path().join("writable");
    std::fs::create_dir_all(&writable).unwrap();
    HostFacts {
        writable_roots: vec![writable],
        hooks_dir: Some(hooks_dir),
        loom_bin,
        hook_path: Vec::new(),
        python3: None,
        python_hooks: Vec::new(),
    }
}

fn executable(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn stage(id: &str, sandbox: StageSandboxConfig) -> Stage {
    Stage {
        id: id.to_string(),
        sandbox,
        ..Stage::default()
    }
}

#[test]
fn check_1_refuses_a_disabled_or_escapable_sandbox_and_names_where_it_was_set() {
    let plan = SandboxConfig::default();
    let clean = stage("clean", StageSandboxConfig::default());
    assert!(sandbox_policy_refusals(&plan, std::slice::from_ref(&clean)).is_empty());

    let off = StageSandboxConfig {
        enabled: Some(false),
        ..StageSandboxConfig::default()
    };
    let out = StageSandboxConfig {
        allow_unsandboxed_escape: Some(true),
        ..StageSandboxConfig::default()
    };
    let stages = [clean.clone(), stage("off", off), stage("out", out)];
    let refusals = sandbox_policy_refusals(&plan, &stages);
    assert_eq!(refusals.len(), 2, "{refusals:?}");
    for (refusal, start) in refusals.iter().zip([
        "stage `off`: sandbox.enabled=false",
        "stage `out`: sandbox.allow_unsandboxed_escape=true",
    ]) {
        assert!(refusal.starts_with(start), "{refusal}");
        assert!(
            refusal.ends_with("(set by the stage's own `sandbox:` override)"),
            "{refusal}"
        );
    }

    let plan = SandboxConfig {
        enabled: false,
        ..SandboxConfig::default()
    };
    let refusals = sandbox_policy_refusals(&plan, &[clean]);
    assert!(
        refusals[0].ends_with("(set by the plan's `sandbox:` block)"),
        "{refusals:?}"
    );
}

#[test]
fn check_2_refuses_a_missing_hooks_dir_or_a_drifted_hook_script() {
    let temp = TempDir::new().unwrap();
    let mut facts = clean_host(&temp);
    assert_eq!(host_refusals(&facts), Vec::<String>::new());

    std::fs::write(
        temp.path().join("hooks").join("loom-relay.sh"),
        "#!/bin/bash\n",
    )
    .unwrap();
    let refusals = host_refusals(&facts);
    assert!(
        refusals.len() == 1 && refusals[0].contains("loom-relay.sh"),
        "{refusals:?}"
    );

    facts.hooks_dir = None;
    let refusals = host_refusals(&facts);
    let missing = "no verified loom hooks directory";
    assert!(
        refusals.len() == 1 && refusals[0].starts_with(missing),
        "{refusals:?}"
    );
}

#[test]
fn check_3_refuses_loom_bin_or_the_hooks_dir_resolving_under_a_writable_root() {
    let temp = TempDir::new().unwrap();
    let mut facts = clean_host(&temp);
    assert_eq!(host_refusals(&facts), Vec::<String>::new());
    let planted = facts.writable_roots[0].join("loom");
    std::fs::write(&planted, "").unwrap();
    // Outside every root by name; the link resolves into one.
    let link = temp.path().join("loom-link");
    symlink(&planted, &link).unwrap();
    facts.loom_bin = link;
    let refusals = host_refusals(&facts);
    assert!(
        refusals.len() == 1 && refusals[0].starts_with("LOOM_BIN"),
        "{refusals:?}"
    );

    let other = TempDir::new().unwrap();
    let mut facts = clean_host(&other);
    facts.writable_roots.push(other.path().join("hooks"));
    let refusals = host_refusals(&facts);
    let hooks = "the loom hooks directory";
    assert!(
        refusals.len() == 1 && refusals[0].starts_with(hooks),
        "{refusals:?}"
    );
}

#[test]
fn python_hook_scripts_finds_only_the_files_shebanged_for_python() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    std::fs::write(dir.join("py.sh"), "#!/usr/bin/env python3\nprint('hi')\n").unwrap();
    std::fs::write(dir.join("run.sh"), "#!/usr/bin/env bash\necho hi\n").unwrap();
    std::fs::write(dir.join("no-shebang"), "echo hi\n").unwrap();
    assert_eq!(python_hook_scripts(Some(dir)), vec![dir.join("py.sh")]);
    assert_eq!(python_hook_scripts(None), Vec::<PathBuf>::new());
}

#[test]
fn check_4_refuses_a_hook_tool_that_resolves_under_a_writable_root() {
    let temp = TempDir::new().unwrap();
    let mut facts = clean_host(&temp);
    let bin = temp.path().join("bin");
    executable(&bin.join("jq"));
    facts.hook_path = vec![bin.clone()];
    assert_eq!(host_refusals(&facts), Vec::<String>::new());

    let planted = facts.writable_roots[0].join("jq");
    executable(&planted);
    std::fs::remove_file(bin.join("jq")).unwrap();
    symlink(&planted, bin.join("jq")).unwrap();
    let refusals = host_refusals(&facts);
    assert!(
        refusals.len() == 1 && refusals[0].starts_with("hook tool `jq`"),
        "{refusals:?}"
    );
}

/// A repository and a writable root (both session-writable), a directory
/// outside every root, and a PATH directory, all in one temp dir.
struct Scene {
    temp: TempDir,
    repo: PathBuf,
    writable: PathBuf,
    outside: PathBuf,
    bin: PathBuf,
}

fn scene() -> Scene {
    let temp = TempDir::new().unwrap();
    let [repo, writable, outside, bin] =
        ["repo", "writable", "outside", "bin"].map(|name| temp.path().join(name));
    for dir in [&repo, &writable, &outside, &bin] {
        std::fs::create_dir_all(dir).unwrap();
    }
    Scene {
        temp,
        repo,
        writable,
        outside,
        bin,
    }
}

fn document(event: &str, command: &str) -> HookDocument {
    let entry = json!([{ "matcher": "*", "hooks": [{ "type": "command", "command": command }] }]);
    let mut hooks = Map::new();
    hooks.insert(event.to_string(), entry);
    HookDocument {
        label: "settings.json".to_string(),
        settings: json!({ "hooks": hooks }),
    }
}

fn scan(scene: &Scene, documents: &[HookDocument]) -> HookFindings {
    scan_hook_registrations(&HookScan {
        documents,
        repo_root: &scene.repo,
        home: Some(scene.temp.path()),
        path: std::slice::from_ref(&scene.bin),
        writable_roots: &[scene.repo.clone(), scene.writable.clone()],
    })
}

#[test]
fn check_5_refuses_a_user_hook_that_runs_a_file_under_a_writable_root() {
    let scene = scene();
    for dir in [&scene.writable, &scene.outside] {
        executable(&dir.join("notify.sh"));
        executable(&dir.join("status"));
    }
    symlink(scene.writable.join("status"), scene.bin.join("status")).unwrap();
    let (writable, outside) = (scene.writable.display(), scene.outside.display());

    for command in [
        format!("{writable}/notify.sh"),
        format!("FOO=1 bash -x {writable}/notify.sh"),
        "status --line".to_string(),
        "$CLAUDE_PROJECT_DIR/.claude/hooks/guard.sh".to_string(),
    ] {
        let findings = scan(&scene, &[document("Stop", &command)]);
        assert_eq!(findings.refusals.len(), 1, "{command}: {findings:?}");
        let refusal = &findings.refusals[0];
        assert!(
            refusal.contains("under the session-writable root"),
            "{refusal}"
        );
    }

    for command in [
        format!("{outside}/notify.sh"),
        format!("bash -c 'echo {writable}/notify.sh'"),
        "not-on-path --flag".to_string(),
    ] {
        let findings = scan(&scene, &[document("Stop", &command)]);
        assert_eq!(findings, HookFindings::default(), "{command}");
    }
}

#[test]
fn check_5_warns_about_a_hard_linked_hook_without_refusing_it() {
    let scene = scene();
    let hook = scene.outside.join("notify.sh");
    executable(&hook);
    std::fs::hard_link(&hook, scene.outside.join("second-name")).unwrap();

    let findings = scan(&scene, &[document("Stop", &hook.display().to_string())]);

    assert!(findings.refusals.is_empty(), "{findings:?}");
    assert_eq!(findings.warnings.len(), 1, "{findings:?}");
    assert!(
        findings.warnings[0].contains("2 hard links"),
        "{findings:?}"
    );
}

#[test]
fn a_loom_hook_registration_resolving_inside_the_repository_is_refused() {
    let scene = scene();
    let in_repo = scene.repo.join("loom-hooks").join("commit-guard.sh");
    let installed = scene.outside.join("hooks").join("commit-guard.sh");
    executable(&in_repo);
    executable(&installed);
    let settings_dir = scene.repo.join(".claude");
    std::fs::create_dir_all(&settings_dir).unwrap();
    for (name, script) in [
        ("settings.json", &installed),
        ("settings.local.json", &in_repo),
    ] {
        let command = format!("/bin/bash {}", script.display());
        let settings = document("PreToolUse", &command).settings.to_string();
        std::fs::write(settings_dir.join(name), settings).unwrap();
    }

    let documents = hook_documents(&scene.repo, Some(scene.temp.path()));
    assert_eq!(
        documents.len(),
        2,
        "both project documents are read, no home one exists"
    );
    let findings = scan(&scene, &documents);

    assert_eq!(findings.refusals.len(), 1, "{findings:?}");
    let refusal = &findings.refusals[0];
    assert!(refusal.starts_with("loom hook `PreToolUse`"), "{refusal}");
    assert!(refusal.contains("settings.local.json"), "{refusal}");
    assert!(refusal.contains("inside the repository"), "{refusal}");
}

/// Every key section 12 lists loom as writing into the main repo's
/// `.claude/settings.local.json`, in the shapes the main-repo and
/// knowledge-stage writers left them.
const LOOM_WRITTEN_KEYS: [&str; 12] = [
    "Read(.loom/work/config.toml)",
    "Read(.work/signals/**)",
    "Read(../.loom/work/memory/**)",
    "Read(//repo/.loom/work/handoffs/**)",
    "Read(//home/you/src/*/.loom/work/disputes/**)",
    "Edit(.loom/work/handoffs/**)",
    "Edit(//repo/.loom/work/handoffs/**)",
    "Write(.work/handoffs/**)",
    "Read(//repo/doc/plans/**)",
    "Edit(loom/target)",
    "Edit(//srv/out)",
    "Edit(~/.cargo/registry)",
];

/// Rules an operator approved, which `strip_loom_written_keys` must leave
/// alone.
const KEPT_KEYS: [&str; 4] = [
    "Bash(cargo test:*)",
    "Edit(src/**)",
    "Edit(../escape)",
    "Read(//repo/doc/other/**)",
];

/// The `"hooks"` settings a session installed with `HooksConfig` merges
/// into the guarded loom-hook stanza, as `the_loom_written_keys_...` below
/// expects to find in a main repo's `settings.local.json`.
fn session_hooks_settings() -> Value {
    let session = HooksConfig::new(
        "/hooks".into(),
        "/repo/.loom/work".into(),
        PermissionMode::Auto,
    );
    let mut hooks = guard_hooks_config("/hooks");
    for (event, rules) in session.to_settings_hooks() {
        let slot = hooks
            .as_object_mut()
            .unwrap()
            .entry(event)
            .or_insert_with(|| json!([]));
        let rules = serde_json::to_value(rules).unwrap();
        slot.as_array_mut()
            .unwrap()
            .extend(rules.as_array().unwrap().iter().cloned());
    }
    hooks
}

#[test]
fn the_loom_written_keys_are_exactly_section_12s_list() {
    let hooks = session_hooks_settings();
    let allow: Vec<&str> = LOOM_WRITTEN_KEYS
        .iter()
        .chain(&KEPT_KEYS)
        .copied()
        .collect();
    let allow_write = ["loom/target", "/srv/out", "~/.cargo/registry", "../escape"];
    let mut settings = json!({
        "sandbox": { "enabled": true, "filesystem": { "allowWrite": allow_write } },
        "permissions": { "defaultMode": "auto", "allow": allow },
        "hooks": hooks,
        "env": { "LOOM_WORK_DIR": "/repo/.loom/work", "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" },
        "worktree": { "bgIsolation": "none" }
    });

    let removed = strip_loom_written_keys(&mut settings, Path::new("/repo"));

    assert_eq!(removed.allow_rules, LOOM_WRITTEN_KEYS);
    assert_eq!(
        removed.session_hooks.len(),
        crate::hooks::HookEvent::all().len(),
        "{:?}",
        removed.session_hooks
    );
    assert_eq!(
        removed.summary(),
        format!(
            "sandbox block, 12 permission rule(s), {} session hook registration(s), env.LOOM_WORK_DIR",
            crate::hooks::HookEvent::all().len()
        )
    );
    let expected: Value = json!({
        "permissions": { "defaultMode": "auto", "allow": KEPT_KEYS },
        "hooks": guard_hooks_config("/hooks"),
        "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" },
        "worktree": { "bgIsolation": "none" }
    });
    assert_eq!(settings, expected);
    assert!(strip_loom_written_keys(&mut settings, Path::new("/repo")).is_empty());
}
