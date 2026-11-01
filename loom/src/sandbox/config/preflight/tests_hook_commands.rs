//! Hook command preflight cases over injected registrations and temporary files.

use super::hook_commands::HookDocument;
use super::*;
use serde_json::{json, Map};
use std::os::unix::fs::{symlink, PermissionsExt};
use tempfile::TempDir;

fn executable(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
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
        path: &[
            scene.bin.clone(),
            PathBuf::from("/bin"),
            PathBuf::from("/usr/bin"),
        ],
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
    ] {
        let findings = scan(&scene, &[document("Stop", &command)]);
        assert_eq!(findings, HookFindings::default(), "{command}");
    }
}

#[test]
fn hook_commands_handle_quoted_paths_wrappers_and_lists() {
    let scene = scene();
    let script = scene.writable.join("hook script.sh");
    executable(&script);
    let path = script.display();
    for command in [
        format!("\"{path}\""),
        format!("/bin/bash \"{path}\""),
        format!("/bin/bash {}", path.to_string().replace(' ', "\\ ")),
        format!("FOO='some value' env BAR='other value' /bin/bash \"{path}\""),
        format!("/bin/true; /bin/bash \"{path}\""),
    ] {
        let findings = scan(&scene, &[document("Stop", &command)]);
        assert!(
            findings
                .refusals
                .iter()
                .any(|r| r.contains("session-writable root")),
            "{command}: {findings:?}"
        );
    }
    for command in [
        "not-on-path --flag",
        "env PATH=/tmp tool",
        "echo $(tool)",
        "bash -o option script",
        "/bin/true > output",
    ] {
        let findings = scan(&scene, &[document("Stop", command)]);
        assert!(
            findings
                .refusals
                .iter()
                .any(|r| r.contains("cannot be verified")),
            "{command}: {findings:?}"
        );
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
