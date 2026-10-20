//! Tool regressions use isolated repositories and optional installed tool binaries.
use super::NetworkAccess;
use crate::models::stage::CommandConfinement;
use crate::verify::criteria::{run_spec_with_timeout, CommandSpec};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

fn launcher_available(name: &str) -> bool {
    !crate::process::sandbox_probe::skip_unless(
        super::launcher().is_some(),
        name,
        "OS sandbox launcher absent",
    )
}

#[test]
fn private_scratch_stays_outside_temporary_root_working_directories() {
    let original = Command::new("/bin/bash");
    for root in [std::env::temp_dir(), PathBuf::from("/tmp")] {
        let cwd = root.canonicalize().unwrap();
        let scratch = super::CargoScratch::prepare(&original, &cwd, false).unwrap();
        let path = scratch.path().to_path_buf();
        assert!(!path.starts_with(&cwd), "{}", path.display());
        std::fs::write(path.join("writable"), "private").unwrap();
        drop(scratch);
        assert!(!path.exists());
    }
    assert!(super::CargoScratch::prepare(&original, Path::new("/"), false).is_err());
}

const CACHE_CANARY: &str = r#"
set -eu
case "$TMPDIR/" in "$PWD/"*) exit 1;; esac
for variable in XDG_CACHE_HOME UV_CACHE_DIR BUN_INSTALL_CACHE_DIR; do
    directory=${!variable}
    [[ $directory == "$TMPDIR/"* ]]
    mkdir -p "$directory"
    printf private > "$directory/written"
done
for relative in .bun/install/cache .cache/uv .local/share/uv/python; do
    [[ $(cat "$HOME/$relative/sentinel") == installed ]]
    ! printf changed > "$HOME/$relative/sentinel"
done
! cat "$HOME/credentials"
! printf changed > "$HOME/credentials"
if : > "/dev/tcp/127.0.0.1/$1"; then
    [[ $2 == provision ]]
else
    [[ $2 == denied ]]
fi
printf CACHE_CANARY_PASSED
"#;

#[test]
fn private_caches_preserve_installed_reads_network_policy_and_cleanup() {
    if !launcher_available("private_caches_preserve_installed_reads_network_policy_and_cleanup") {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().join("worktree");
    let home = root.path().join("home");
    std::fs::create_dir(&cwd).unwrap();
    for relative in [".bun/install/cache", ".cache/uv", ".local/share/uv/python"] {
        let cache = home.join(relative);
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("sentinel"), "installed").unwrap();
    }
    std::fs::write(home.join("credentials"), "secret").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    for (network, mode) in [
        (NetworkAccess::Denied, "denied"),
        (NetworkAccess::Provision, "provision"),
    ] {
        let mut original = Command::new("/bin/bash");
        original.current_dir(&cwd).args([
            "--noprofile",
            "--norc",
            "-c",
            CACHE_CANARY,
            "canary",
            &listener.local_addr().unwrap().port().to_string(),
            mode,
        ]);
        crate::process::apply_stage_environment(&mut original);
        original.env("HOME", &home);
        let (mut wrapped, scratch) = super::wrap(&original, None, network).unwrap();
        let scratch_path = scratch.path().to_path_buf();
        scratch.populate().unwrap();
        let output = wrapped.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("CACHE_CANARY_PASSED"));
        assert!(scratch_path.join("uv-cache/written").is_file());
        drop(scratch);
        assert!(!scratch_path.exists());
    }
    assert_eq!(
        std::fs::read_to_string(home.join("credentials")).unwrap(),
        "secret"
    );
}

#[test]
fn private_scratch_does_not_change_acceptance_cache_identity() {
    use crate::verify::criteria::confine::prepare_confined;
    if !launcher_available("private_scratch_does_not_change_acceptance_cache_identity") {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let spec = CommandSpec::program("/bin/bash", ["-c", "true"]);
    let first =
        prepare_confined(&spec, Some(root.path()), None, CommandConfinement::Confined).unwrap();
    let second =
        prepare_confined(&spec, Some(root.path()), None, CommandConfinement::Confined).unwrap();
    assert_eq!(
        first.containment_policy_revision(),
        second.containment_policy_revision()
    );
}

fn optional_tool(name: &str, variable: &str) -> Option<PathBuf> {
    let tool = std::env::var_os(variable)
        .map(PathBuf::from)
        .or_else(|| which::which(name).ok());
    if crate::process::sandbox_probe::skip_unless(
        tool.as_ref().is_some_and(|path| path.is_file()),
        name,
        &format!("installed {name} absent; set {variable} to exercise its real boundary"),
    ) {
        return None;
    }
    tool
}

#[test]
fn actual_uv_lock_check_uses_private_writable_cache_offline() {
    if !launcher_available("actual_uv_lock_check_uses_private_writable_cache_offline") {
        return;
    }
    let Some(tool) = optional_tool("uv", "LOOM_TEST_UV_BIN") else {
        return;
    };
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().join("worktree");
    std::fs::create_dir(&cwd).unwrap();
    let binary = cwd.join("uv");
    std::fs::copy(tool, &binary).unwrap();
    std::fs::write(
        cwd.join("pyproject.toml"),
        "[project]\nname = \"cache-fixture\"\nversion = \"0.1.0\"\nrequires-python = \">=3.8\"\n",
    )
    .unwrap();
    let control = Command::new(&binary)
        .current_dir(&cwd)
        .args(["lock", "--offline", "--cache-dir"])
        .arg(root.path().join("control-cache"))
        .output()
        .unwrap();
    assert!(
        control.status.success(),
        "{}",
        String::from_utf8_lossy(&control.stderr)
    );
    let spec = CommandSpec::program(binary.to_string_lossy(), ["lock", "--offline", "--check"]);
    let result = run_spec_with_timeout(
        &spec,
        Some(&cwd),
        Duration::from_secs(15),
        CommandConfinement::Confined,
    )
    .unwrap();
    assert!(result.success, "{}", result.stderr);
    assert!(!std::fs::read_dir(&cwd).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".loom-")));
}

fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(directory)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
fn biome_fixture(root: &Path, tool: &Path) -> PathBuf {
    let repo = root.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "--initial-branch=main"]);
    std::fs::write(
        repo.join("biome.json"),
        r#"{
      "vcs":{"enabled":true,"clientKind":"git","useIgnoreFile":true},
      "files":{"includes":["**","!**/node_modules","!**/.venv","!.loom"]},
      "formatter":{"enabled":false},"linter":{"enabled":false}
    }"#,
    )
    .unwrap();
    std::fs::write(
        repo.join(".gitignore"),
        "node_modules\n.venv\n.loom\n.worktrees\n",
    )
    .unwrap();
    std::fs::write(repo.join("source.js"), "const answer = 42;\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "fixture"]);
    let cwd = repo.join(".worktrees/stage");
    git(
        &repo,
        &["worktree", "add", cwd.to_str().unwrap(), "-b", "stage"],
    );
    let state = root.join("private-state");
    std::fs::create_dir(&state).unwrap();
    std::fs::write(state.join("secret"), "private").unwrap();
    std::fs::create_dir(cwd.join(".loom")).unwrap();
    std::os::unix::fs::symlink(state, cwd.join(".loom/work")).unwrap();
    for generated in ["packages/one/node_modules", "packages/one/.venv"] {
        std::fs::create_dir_all(cwd.join(generated)).unwrap();
        std::fs::write(cwd.join(generated).join("broken.js"), "function {\n").unwrap();
    }
    std::fs::copy(tool, cwd.join("biome")).unwrap();
    cwd
}

#[cfg(unix)]
#[test]
fn actual_biome_git_worktree_keeps_cargo_sources_outside_lint_coverage() {
    if !launcher_available("actual_biome_git_worktree_keeps_cargo_sources_outside_lint_coverage") {
        return;
    }
    let Some(tool) = optional_tool("biome", "LOOM_TEST_BIOME_BIN") else {
        return;
    };
    let root = tempfile::tempdir().unwrap();
    let cwd = biome_fixture(root.path(), &tool);
    let cargo = root.path().join("cargo");
    std::fs::create_dir_all(cargo.join("registry")).unwrap();
    std::fs::write(cargo.join("registry/.gitignore"), "ignored.js\n").unwrap();
    std::fs::write(cargo.join("registry/source.js"), "const cache = 1;\n").unwrap();
    let control = Command::new(cwd.join("biome"))
        .current_dir(&cwd)
        .args(["ci", "."])
        .output()
        .unwrap();
    assert!(control.status.success());
    let mut original = Command::new(cwd.join("biome"));
    original.current_dir(&cwd).args(["ci", "."]);
    crate::process::apply_stage_environment(&mut original);
    original.env("CARGO_HOME", cargo);
    let (mut command, scratch) = super::wrap(&original, None, NetworkAccess::Denied).unwrap();
    scratch.populate().unwrap();
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panic"));
    for result in [&control, &output] {
        assert!(String::from_utf8_lossy(&result.stdout).contains("Checked 2 files"));
    }
    assert!(!scratch.path().starts_with(&cwd));
    std::fs::write(cwd.join("source.js"), "function {\n").unwrap();
    assert!(
        !command.output().unwrap().status.success(),
        "source syntax errors must remain checked"
    );
}
