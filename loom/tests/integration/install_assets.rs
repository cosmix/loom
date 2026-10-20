//! `loom install-assets` is the function every entry point that places
//! loom's assets routes through - `install.sh` and `loom update` both call
//! the CLI - but until now it was only ever exercised through a shell stub.
//! This drives the real CLI end to end into a scratch home directory.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

use super::helpers::loom_cmd;

#[test]
fn install_assets_places_a_real_tree_under_the_home_roots() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();

    // Claude Code's and Codex's own root overrides do not move loom's install:
    // it always targets `~/.claude` and `~/.codex`.
    let output = loom_cmd()
        .env("HOME", &home)
        .env("CLAUDE_CONFIG_DIR", temp.path().join("native-claude"))
        .env("CODEX_HOME", temp.path().join("native-codex"))
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .arg("install-assets")
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

    assert_claude_tree(&home.join(".claude"));
    assert_codex_tree(&home.join(".codex"));
    assert_overrides_and_completions_untouched(temp.path(), &home);
}

fn assert_claude_tree(claude_dir: &Path) {
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
    for path in [
        claude_dir.join("CLAUDE.md"),
        claude_dir.join("agents/loom-code-reviewer.md"),
        claude_dir.join("hooks/loom/post-tool-use.sh"),
    ] {
        assert!(
            path.is_file(),
            "expected installed asset {}",
            path.display()
        );
    }

    let install_toml = fs::read_to_string(claude_dir.join("loom-install.toml")).unwrap();
    assert!(install_toml.contains("skills = \"core\""), "{install_toml}");
}

fn assert_codex_tree(codex_dir: &Path) {
    for path in [codex_dir.join("AGENTS.md"), codex_dir.join("hooks.json")] {
        assert!(
            path.is_file(),
            "expected installed asset {}",
            path.display()
        );
    }
}

fn assert_overrides_and_completions_untouched(temp: &Path, home: &Path) {
    // A bare install only refreshes completions that already exist.
    for untouched in [
        temp.join("native-claude"),
        temp.join("native-codex"),
        home.join(".local/share/bash-completion/completions/loom"),
        home.join(".zfunc/_loom"),
        home.join(".config/fish/completions/loom.fish"),
    ] {
        assert!(
            !untouched.exists(),
            "{} must stay absent",
            untouched.display()
        );
    }
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
