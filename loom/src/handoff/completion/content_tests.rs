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
        ".mcp.json",
        "apps/api/.bashrc",
        "apps/web/src/ignored.ts",
        "packages/ui/package.json",
        "deps/product-media-kit/tool/input.py",
        "src/node_modules_backup/index.js",
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
        "node_modules/.bun/pkg@1/node_modules/pkg/index.js",
        "apps/web/node_modules/pkg/index.js",
        "packages/ui/node_modules/.bin/compiler",
        "deps/aurora-ui/node_modules/pkg/index.js",
        "vendor/tool/.venv/lib/python3/site-packages/tool.py",
        ".venv/pyvenv.cfg",
        "apps/api/.ruff_cache/.gitignore",
        ".ruff_cache/0.14/cache",
        ".pytest_cache/v/cache/nodeids",
        "deps/product-media-kit/tool/__pycache__/tool.cpython-314.pyc",
        ".claude/settings.json",
        ".claude/settings.local.json",
        ".loom/memory-spool.jsonl",
        ".loom/stage-request-spool.jsonl",
        ".loom/telemetry-spool.jsonl",
    ] {
        write(&checkout, path);
    }
    std::fs::write(repo.path().join(".git/info/exclude"), "*\n").unwrap();
    committed_content(&stage, repo.path()).unwrap();
}

#[test]
fn sandbox_mount_remnants_are_accepted_without_changing_commit_identity() {
    let (repo, stage, checkout) = fixture();
    let before = committed_content(&stage, repo.path()).unwrap();
    for name in crate::verify::tool_artifacts::NAMES {
        if matches!(name, ".idea" | ".vscode") {
            std::fs::create_dir(checkout.join(name)).unwrap();
        } else {
            std::fs::write(checkout.join(name), "").unwrap();
        }
    }
    assert_eq!(committed_content(&stage, repo.path()).unwrap(), before);
    assert!(crate::git::branch::list_working_tree_changes(&checkout)
        .unwrap()
        .is_empty());
}

#[test]
fn sandbox_lookalikes_and_populated_configs_are_refused() {
    for path in [
        ".mcp.json",
        ".gitmodules",
        "apps/.bashrc",
        ".vscode/settings.json",
    ] {
        let (repo, stage, checkout) = fixture();
        write(&checkout, path);
        assert!(committed_content(&stage, repo.path()).is_err(), "{path}");
    }
    let (repo, stage, checkout) = fixture();
    std::os::unix::fs::symlink("/dev/null", checkout.join(".bashrc")).unwrap();
    assert!(committed_content(&stage, repo.path()).is_err());
    std::fs::remove_file(checkout.join(".bashrc")).unwrap();
    nix::unistd::mkfifo(&checkout.join(".bashrc"), nix::sys::stat::Mode::S_IRWXU).unwrap();
    assert!(committed_content(&stage, repo.path()).is_err());
    std::fs::remove_file(checkout.join(".bashrc")).unwrap();
    let _socket = std::os::unix::net::UnixListener::bind(checkout.join(".bashrc")).unwrap();
    assert!(committed_content(&stage, repo.path()).is_err());
}

#[test]
fn declared_empty_mount_directories_are_inputs() {
    let (repo, mut stage, checkout) = fixture();
    stage.files.push(".vscode".into());
    std::fs::create_dir(checkout.join(".vscode")).unwrap();
    assert!(committed_content(&stage, repo.path()).is_err());
}

#[test]
fn committed_protected_configs_and_editor_directories_are_accepted() {
    let (repo, stage, checkout) = fixture();
    write(&checkout, ".mcp.json");
    write(&checkout, ".vscode/settings.json");
    git(&checkout, &["add", ".mcp.json", ".vscode/settings.json"]);
    git(
        &checkout,
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "configs",
        ],
    );
    committed_content(&stage, repo.path()).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn sandbox_null_device_preserves_declared_input_precedence() {
    use std::process::Command;
    const ROOT_ENV: &str = "LOOM_TEST_COMPLETION_NULL_ROOT";
    if let Some(root) = std::env::var_os(ROOT_ENV) {
        assert_null_mount(&std::path::PathBuf::from(root));
        return;
    }
    if !Command::new("bwrap")
        .args(["--ro-bind", "/", "/", "--", "true"])
        .output()
        .is_ok_and(|output| output.status.success())
    {
        eprintln!("skipping null-device mount test: bubblewrap unavailable");
        return;
    }
    let (repo, _, checkout) = fixture();
    std::fs::write(checkout.join(".bashrc"), "").unwrap();
    let output = Command::new("bwrap")
        .args(["--ro-bind", "/", "/", "--dev", "/dev", "--bind"]).arg(repo.path()).arg(repo.path())
        .args(["--ro-bind", "/dev/null"]).arg(checkout.join(".bashrc"))
        .arg("--").arg(std::env::current_exe().unwrap())
        .args(["--exact", "handoff::completion::content::tests::sandbox_null_device_preserves_declared_input_precedence", "--nocapture"])
        .env(ROOT_ENV, repo.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(target_os = "linux")]
fn assert_null_mount(root: &std::path::Path) {
    use std::os::unix::fs::FileTypeExt;
    let mut stage = Stage {
        id: "api".into(),
        worktree: Some("api".into()),
        ..Default::default()
    };
    let placeholder = root.join(".worktrees/api/.bashrc");
    assert!(std::fs::symlink_metadata(placeholder)
        .unwrap()
        .file_type()
        .is_char_device());
    committed_content(&stage, root).unwrap();
    stage.artifacts.push(".bashrc".into());
    assert!(committed_content(&stage, root).is_err());
}

#[test]
fn explicit_required_inputs_override_generated_output_paths() {
    for path in [
        "target/required-input",
        ".loom/cache/required-input",
        ".claude/settings.json",
        ".bashrc",
        "node_modules/required-input",
        ".venv/required-input",
        ".pytest_cache/required-input",
    ] {
        let (repo, mut stage, checkout) = fixture();
        stage.artifacts.push(path.into());
        write(&checkout, path);
        if path == ".bashrc" {
            std::fs::write(checkout.join(path), "").unwrap();
        }
        assert!(committed_content(&stage, repo.path()).is_err());
    }
}

#[test]
fn every_declared_input_owner_overrides_package_cache_directories() {
    for declaration in 0..7 {
        let (repo, mut stage, checkout) = fixture();
        stage.working_dir = Some("apps/api".into());
        let pattern = ".ruff_cache".to_owned();
        match declaration {
            0 => stage.files.push(pattern),
            1 => stage.artifacts.push(pattern),
            2 => stage.harness.push(pattern),
            3 => stage.ratchet_files.push(pattern),
            4 => stage.contracts.push(
                serde_json::from_value(serde_json::json!({
                    "id": "cache", "file": pattern, "test": "cache",
                    "scenario": "cache input", "rejects": "missing input"
                }))
                .unwrap(),
            ),
            5 => stage.wiring.push(
                serde_json::from_value(serde_json::json!({
                    "source": pattern, "pattern": "input", "description": "input"
                }))
                .unwrap(),
            ),
            _ => {
                stage.regression_test = Some(
                    serde_json::from_value(serde_json::json!({
                        "file": pattern
                    }))
                    .unwrap(),
                )
            }
        }
        write(&checkout, "apps/api/.ruff_cache/required-input");
        let error = committed_content(&stage, repo.path()).unwrap_err();
        assert!(
            error.to_string().contains("untracked completion input"),
            "{declaration}: {error:#}"
        );
    }
}

#[test]
fn declared_parent_directories_and_globs_include_generated_descendants() {
    for pattern in [
        "apps/api",
        "apps/*",
        "apps/**/.venv/**",
        "./apps/api/.venv/../.venv",
    ] {
        let (repo, mut stage, checkout) = fixture();
        stage.files.push(pattern.into());
        write(&checkout, "apps/api/.venv/lib/input.py");
        assert!(committed_content(&stage, repo.path()).is_err(), "{pattern}");
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
    for path in ["target/input", "node_modules/input", ".gitmodules"] {
        for staged in [false, true] {
            tracked_change_is_refused(path, staged);
        }
    }
}

fn tracked_change_is_refused(path: &str, staged: bool) {
    let (repo, stage, checkout) = fixture();
    write(&checkout, path);
    if path == ".gitmodules" {
        std::fs::write(
            checkout.join(path),
            "[submodule \"example\"]\n\tpath = example\n\turl = example\n",
        )
        .unwrap();
    }
    git(&checkout, &["add", path]);
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
    std::fs::write(
        checkout.join(path),
        if path == ".gitmodules" { "" } else { "changed" },
    )
    .unwrap();
    if staged {
        git(&checkout, &["add", path]);
    }
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
