//! `loom install-assets` is the function every entry point that places
//! loom's assets routes through - `install.sh` and `loom update` both call
//! the CLI - but until now it was only ever exercised through a shell stub.
//! This drives the real CLI end to end into scratch directories.

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

use super::helpers::loom_cmd;

#[test]
fn install_assets_places_a_real_tree_under_explicit_directories() {
    let temp = TempDir::new().unwrap();
    let claude_dir = temp.path().join("claude");
    let codex_dir = temp.path().join("codex");

    // Both directories must always be passed explicitly: `install_assets::execute`
    // resolves an omitted `--claude-dir` or `--codex-dir` independently against
    // the operator's real home, so passing only one here would write into the
    // real `~/.codex` or `~/.claude`. `loom_cmd()` sets `LOOM_HOME` but not
    // `HOME`; the explicit `HOME` override below makes that safety independent
    // of `install_all`'s internals rather than relying solely on both flags.
    let output = loom_cmd()
        .env("HOME", temp.path())
        .arg("install-assets")
        .arg("--claude-dir")
        .arg(&claude_dir)
        .arg("--codex-dir")
        .arg(&codex_dir)
        .arg("--skills")
        .arg("core")
        .output()
        .expect("failed to run loom install-assets");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // `loom-usage` is a core skill, so it must be resident under `skills/`.
    assert!(
        claude_dir.join("skills/loom-usage/SKILL.md").is_file(),
        "expected a resident core skill under skills/"
    );
    // `loom-rust` is not a core skill, so it must land in the catalog instead.
    assert!(
        claude_dir
            .join("loom-skill-catalog/loom-rust/SKILL.md")
            .is_file(),
        "expected a catalogued non-core skill under loom-skill-catalog/"
    );
    assert!(claude_dir.join("CLAUDE.md").is_file());
    assert!(codex_dir.join("AGENTS.md").is_file());

    let install_toml = fs::read_to_string(claude_dir.join("loom-install.toml")).unwrap();
    assert!(install_toml.contains("skills = \"core\""), "{install_toml}");
}

// `curl -fsSL .../install.sh | bash` feeds the script on stdin, so bash never
// populates `BASH_SOURCE[0]`. This reproduces that without hitting the
// network or the filesystem: `--help` exits inside `parse_args`, before
// anything reads `$HOME`.
#[test]
fn install_sh_runs_when_piped_on_stdin() {
    let script_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("install.sh");
    let script = fs::read(&script_path).unwrap();

    let mut child = Command::new("bash")
        .arg("-s")
        .arg("--")
        .arg("--help")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn bash");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(&script)
        .expect("failed to write install.sh to stdin");

    let output = child
        .wait_with_output()
        .expect("bash did not run to completion");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage: install.sh"),
        "stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
