//! Contract for stage tmux-only: `install.sh` (`check_runtime_tools`) warns
//! when tmux is missing and names the install hint, without failing the
//! install; with tmux on PATH it stays silent about tmux.
//!
//! The installer is sourced with `LOOM_INSTALL_LIB_ONLY=1` under a PATH made of
//! one temp `bin` directory holding only stubs for the other runtime tools, so
//! the host's real tmux never leaks in. Requires `bash` and `uname` on the host.

use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

struct CheckRun {
    code: Option<i32>,
    output: String,
}

fn write_stub(bin: &Path, name: &str) {
    let path = bin.join(name);
    std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("write stub");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod stub");
}

/// Runs `check_runtime_tools` with a PATH of one temp bin directory, adding a
/// `tmux` stub when `with_tmux` is set.
fn run_check(with_tmux: bool) -> CheckRun {
    let installer = Path::new(env!("CARGO_MANIFEST_DIR")).join("../install.sh");
    let home = TempDir::new().expect("create temp home");
    let bin = home.path().join("bin");
    std::fs::create_dir_all(&bin).expect("create bin dir");
    for tool in ["jq", "rg", "fd"] {
        write_stub(&bin, tool);
    }
    if with_tmux {
        write_stub(&bin, "tmux");
    }
    // The installer's top level calls dirname; the advisory calls uname.
    for tool in ["uname", "dirname"] {
        let real = which::which(tool).expect("host tool on PATH");
        symlink(real, bin.join(tool)).expect("link host tool");
    }
    let bash = which::which("bash").expect("bash on PATH");
    let out = Command::new(bash)
        .arg("-c")
        .arg(r#"LOOM_INSTALL_LIB_ONLY=1 source "$INSTALLER"; check_runtime_tools"#)
        .env_clear()
        .env("PATH", &bin)
        .env("HOME", home.path())
        .env("INSTALLER", &installer)
        .current_dir(home.path())
        .output()
        .expect("run bash");
    let mut output = String::from_utf8_lossy(&out.stdout).into_owned();
    output.push_str(&String::from_utf8_lossy(&out.stderr));
    CheckRun {
        code: out.status.code(),
        output,
    }
}

#[test]
fn missing_tmux_warns_with_install_hint_and_succeeds() {
    let run = run_check(false);
    assert_eq!(
        run.code,
        Some(0),
        "missing tmux is not fatal\n{}",
        run.output
    );
    assert!(
        run.output.contains("tmux is not installed"),
        "the advisory must name tmux\n{}",
        run.output
    );
    assert!(
        run.output.contains("brew install tmux") || run.output.contains("sudo apt install tmux"),
        "the advisory must carry the install hint\n{}",
        run.output
    );
}

#[test]
fn present_tmux_is_not_mentioned() {
    let run = run_check(true);
    assert_eq!(run.code, Some(0), "all tools present\n{}", run.output);
    assert!(
        !run.output.contains("tmux"),
        "no tmux advisory when tmux is on PATH\n{}",
        run.output
    );
}
