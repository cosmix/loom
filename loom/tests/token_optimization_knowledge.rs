#[path = "integration/helpers.rs"]
// only loom_cmd() is used; the shared module serves the integration target
#[allow(dead_code)]
mod helpers;

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn scratch() -> TempDir {
    TempDir::new().expect("create unique scratch child")
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit_all(root: &Path, message: &str) {
    git(root, &["add", "."]);
    git(
        root,
        &[
            "-c",
            "user.name=Loom Test",
            "-c",
            "user.email=loom@example.invalid",
            "commit",
            "-m",
            message,
        ],
    );
}

fn seed_work_marker(project: &Path) {
    let work = project.join(".loom/work");
    fs::create_dir_all(&work).expect("create work marker");
    fs::write(work.join("config.toml"), "").expect("write work marker");
    fs::write(work.join("sentinel"), "must remain unchanged\n").expect("write sentinel");
}

fn changed_evidence_project() -> TempDir {
    let project = scratch();
    let root = project.path();
    seed_work_marker(root);
    fs::create_dir_all(root.join("src")).expect("create source directory");
    fs::create_dir_all(root.join("doc/loom/knowledge")).expect("create knowledge directory");
    fs::write(root.join("src/evidence.rs"), "pub const VALUE: u8 = 1;\n").expect("write source");
    git(root, &["init", "-q"]);
    commit_all(root, "initial source");
    let verified = git(root, &["rev-parse", "HEAD"]);
    fs::write(
        root.join("doc/loom/knowledge/topic.md"),
        format!(
            "---\nsources:\n  - src/evidence.rs\nverified: {verified}\n---\n# Topic\n\n## Evidence\nCurrent.\n"
        ),
    )
    .expect("write knowledge");
    commit_all(root, "record verification");
    fs::write(root.join("src/evidence.rs"), "pub const VALUE: u8 = 2;\n").expect("change source");
    project
}

fn inventory(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    collect_inventory(root, root, &mut paths);
    paths.sort();
    paths
}

fn collect_inventory(root: &Path, directory: &Path, paths: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("read inventory directory") {
        let entry = entry.expect("read inventory entry");
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .expect("relative inventory path")
            .to_path_buf();
        paths.push(relative);
        if entry.file_type().expect("inventory type").is_dir() {
            collect_inventory(root, &path, paths);
        }
    }
}

fn loom_cmd(project: &Path) -> Command {
    let home = project.join("scratch-home");
    fs::create_dir_all(&home).expect("create scratch home");
    fs::write(home.join("config.toml"), "[update]\ncheck = false\n").expect("disable update check");
    let mut command = helpers::loom_cmd();
    command
        .current_dir(project)
        .env("HOME", &home)
        .env("LOOM_HOME", &home);
    command
}

fn decoded_json(output: &std::process::Output) -> Value {
    assert!(
        output.stdout.starts_with(b"{"),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).expect("decode JSON output")
}

#[test]
fn strict_evidence_reaches_checker_and_preserves_shared_state() {
    let project = changed_evidence_project();
    let root = project.path();
    let shared = root.join(".loom");
    let before = inventory(&shared);

    let default = loom_cmd(root)
        .args(["knowledge", "check", "--json"])
        .output()
        .expect("run default check");
    assert!(
        default.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&default.stderr)
    );
    assert_eq!(decoded_json(&default)["evidence"]["status"], "changed");

    let strict = loom_cmd(root)
        .args(["knowledge", "check", "--strict", "--json"])
        .output()
        .expect("run strict check");
    assert!(
        strict.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&strict.stderr)
    );

    let strict_evidence = loom_cmd(root)
        .args(["knowledge", "check", "--strict-evidence", "--json"])
        .output()
        .expect("run strict-evidence check");
    assert!(!strict_evidence.status.success());
    assert_eq!(decoded_json(&strict_evidence)["evidence"]["changed"], 1);
    assert_eq!(
        before,
        inventory(&shared),
        "check must not alter sentinel shared state"
    );
}

#[test]
fn strict_evidence_reports_unavailable_git_with_a_git_free_path() {
    let project = changed_evidence_project();
    let root = project.path();
    let no_git_path = root.join("no-git-bin");
    fs::create_dir(&no_git_path).expect("create git-free PATH");

    let output = loom_cmd(root)
        .env("PATH", no_git_path)
        .args(["knowledge", "check", "--strict-evidence", "--json"])
        .output()
        .expect("run git-free check");
    let json = decoded_json(&output);

    assert!(!output.status.success());
    assert_eq!(json["evidence"]["status"], "unavailable");
    assert!(json["review"].to_string().contains("git_unavailable"));
}

#[cfg(unix)]
#[test]
fn strict_evidence_reports_bounded_git_output_as_unavailable() {
    use std::os::unix::fs::PermissionsExt;

    let project = changed_evidence_project();
    let root = project.path();
    let bin = root.join("large-output-bin");
    fs::create_dir(&bin).expect("create fake git directory");
    let git = bin.join("git");
    fs::write(
        &git,
        "#!/bin/sh\n/bin/dd if=/dev/zero bs=1048577 count=1 2>/dev/null\n",
    )
    .expect("write fake git");
    fs::set_permissions(&git, fs::Permissions::from_mode(0o755)).expect("make fake git executable");

    let output = loom_cmd(root)
        .env("PATH", bin)
        .args(["knowledge", "check", "--strict-evidence", "--json"])
        .output()
        .expect("run bounded-output check");

    assert!(!output.status.success());
    assert!(decoded_json(&output)["review"]
        .to_string()
        .contains("resource_limit"));
}

#[test]
fn strict_evidence_fails_for_a_missing_knowledge_root() {
    let project = scratch();
    let root = project.path();
    seed_work_marker(root);

    let output = loom_cmd(root)
        .args(["knowledge", "check", "--strict-evidence", "--json"])
        .output()
        .expect("run missing-root check");

    assert!(!output.status.success());
    assert_eq!(decoded_json(&output)["evidence"]["status"], "missing");
}
