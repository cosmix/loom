//! `loom install-assets` is the function every entry point that places
//! loom's assets routes through - `install.sh` and `loom update` both call
//! the CLI - but until now it was only ever exercised through a shell stub.
//! This drives the real CLI end to end into scratch directories.

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

use super::helpers::loom_cmd;

#[path = "install_assets/custom_roots.rs"]
mod custom_roots;

#[test]
fn install_assets_places_a_real_tree_under_explicit_directories() {
    let temp = TempDir::new().unwrap();
    let claude_dir = temp.path().join("claude");
    let codex_dir = temp.path().join("codex");

    // Scrub ambient install-root overrides even though both flags are explicit,
    // and keep HOME private so later changes cannot redirect either omitted or
    // auxiliary writes into the operator's real configuration trees.
    let output = loom_cmd()
        .env("HOME", temp.path())
        .env_remove("LOOM_CLAUDECODE_INSTALL_DIR")
        .env_remove("LOOM_CODEX_INSTALL_DIR")
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
// network or asset placement: `--help` exits inside `parse_args` after the
// script has resolved its display-only directory variables from `$HOME`.
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
        .env_remove("LOOM_CLAUDECODE_INSTALL_DIR")
        .env_remove("LOOM_CODEX_INSTALL_DIR")
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
