use super::{committed_content, completion_scaffold};
use crate::models::stage::Stage;

// Use public Git operations through a disposable repository; no process cwd or
// environment changes, so these tests can run alongside completion fixtures.
fn fixture() -> (tempfile::TempDir, Stage, std::path::PathBuf) {
    let repo = tempfile::TempDir::new().unwrap();
    git(repo.path(), &["init", "-b", "main"]);
    git(
        repo.path(),
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@t",
            "commit",
            "--allow-empty",
            "-qm",
            "seed",
        ],
    );
    git(
        repo.path(),
        &["worktree", "add", "-b", "loom/api", ".worktrees/api"],
    );
    let checkout = repo.path().join(".worktrees/api");
    let stage = Stage {
        id: "api".into(),
        worktree: Some("api".into()),
        ..Default::default()
    };
    (repo, stage, checkout)
}

fn git(root: &std::path::Path, args: &[&str]) {
    let output = crate::git::runner::run_git(args, root).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write(root: &std::path::Path, path: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "input").unwrap();
}

#[test]
fn ignored_inputs_and_arbitrary_scaffold_children_are_refused() {
    for path in [
        "required.env",
        ".claude/required-input",
        ".loom/required-input",
        "CLAUDE.md",
        ".loom/work",
    ] {
        let (repo, stage, checkout) = fixture();
        std::fs::write(repo.path().join(".git/info/exclude"), "*\n").unwrap();
        write(&checkout, path);
        let error = committed_content(&stage, repo.path()).unwrap_err();
        assert!(
            error.to_string().contains("untracked completion input"),
            "{error:#}"
        );
    }
}

#[test]
fn exact_generated_outputs_are_accepted() {
    let (repo, stage, checkout) = fixture();
    for path in [
        ".loom/cache/map",
        "target/debug/build-output",
        "loom/target/debug/build-output",
        "web/node_modules/dependency/index.js",
        ".claude/settings.json",
        ".claude/settings.local.json",
        ".loom/memory-spool.jsonl",
        ".loom/stage-request-spool.jsonl",
        ".loom/telemetry-spool.jsonl",
    ] {
        write(&checkout, path);
    }
    committed_content(&stage, repo.path()).unwrap();
}

#[test]
fn explicit_required_inputs_override_generated_output_paths() {
    for path in [
        "target/required-input",
        ".loom/cache/required-input",
        ".claude/settings.json",
    ] {
        let (repo, mut stage, checkout) = fixture();
        stage.artifacts.push(path.into());
        write(&checkout, path);
        assert!(committed_content(&stage, repo.path()).is_err());
    }
}

#[test]
fn declared_globs_and_working_directory_override_generated_outputs() {
    let (repo, mut stage, checkout) = fixture();
    stage.working_dir = Some("loom".into());
    stage.files.push("target/**".into());
    write(&checkout, "loom/target/required-input");
    assert!(committed_content(&stage, repo.path()).is_err());
}

#[test]
fn tracked_generated_outputs_are_never_exempt() {
    let (repo, stage, checkout) = fixture();
    write(&checkout, "target/input");
    git(&checkout, &["add", "target/input"]);
    git(
        &checkout,
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "input",
        ],
    );
    std::fs::write(checkout.join("target/input"), "changed").unwrap();
    assert!(committed_content(&stage, repo.path())
        .unwrap_err()
        .to_string()
        .contains("worktree differs"));
}

#[test]
fn scaffold_links_require_symlinks_and_branch_identity_is_exact() {
    let (repo, stage, checkout) = fixture();
    std::os::unix::fs::symlink("../../../.loom/work", checkout.join(".work")).unwrap();
    assert!(completion_scaffold(".work", &checkout));
    git(&checkout, &["checkout", "-b", "unrelated"]);
    assert!(committed_content(&stage, repo.path())
        .unwrap_err()
        .to_string()
        .contains("stage branch"));
}
