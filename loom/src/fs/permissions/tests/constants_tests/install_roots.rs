use std::fs;
use std::process::Command;

use tempfile::TempDir;

use super::{
    assert_single_install_assets_delegation, stage_install_sh_with_stub_binary, DEV_INSTALL_SH,
    INSTALL_SH,
};

const CLAUDE_OVERRIDE: &str = "LOOM_CLAUDECODE_INSTALL_DIR";
const CODEX_OVERRIDE: &str = "LOOM_CODEX_INSTALL_DIR";

#[test]
fn install_sh_uses_custom_roots_for_layout_prompt_and_summary() {
    let temp = TempDir::new().unwrap();
    let installer = temp.path().join("install.sh");
    let home = temp.path().join("home");
    let claude = temp.path().join("claude assets");
    let codex = temp.path().join("codex assets");
    fs::write(&installer, INSTALL_SH).unwrap();
    fs::create_dir_all(claude.join("agents")).unwrap();
    fs::create_dir_all(&codex).unwrap();
    fs::write(claude.join("loom-install.toml"), "skills = \"all\"\n").unwrap();
    fs::write(codex.join("AGENTS.md"), "existing\n").unwrap();

    let output = Command::new("bash")
        .arg("-c")
        .arg(
            r#"source "$INSTALL_SH_PATH"; parse_args; printf 'MODE=%s\n' "$SKILLS_MODE"; confirm_overwrites <<< y; print_summary"#,
        )
        .env("HOME", &home)
        .env("INSTALL_SH_PATH", &installer)
        .env("LOOM_INSTALL_LIB_ONLY", "1")
        .env(CLAUDE_OVERRIDE, &claude)
        .env(CODEX_OVERRIDE, &codex)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MODE=all"), "{stdout}");
    assert!(
        stdout.contains(&claude.join("agents/").display().to_string()),
        "{stdout}"
    );
    assert!(
        stdout.contains(&codex.join("AGENTS.md").display().to_string()),
        "{stdout}"
    );
    for root in [&claude, &codex] {
        assert!(
            stdout.matches(&root.display().to_string()).count() >= 2,
            "{stdout}"
        );
    }
    assert!(!stdout.contains(&home.join(".claude").display().to_string()));
    assert!(!stdout.contains(&home.join(".codex").display().to_string()));
}

#[cfg(unix)]
#[test]
fn install_sh_delegates_without_flags_and_preserves_override_environment() {
    let (_temp, installer, home, argv_log) = stage_install_sh_with_stub_binary();
    let claude = home.join("custom claude");
    let codex = home.join("custom codex");
    let env_log = home.join("asset-root-env");
    let output = Command::new("bash")
        .arg("-c")
        .arg(
            r#"source "$INSTALL_SH_PATH"; check_runtime_tools() { :; }; install_loom_remote() { :; }; main --skills core"#,
        )
        .env("HOME", &home)
        .env("INSTALL_SH_PATH", &installer)
        .env("LOOM_INSTALL_LIB_ONLY", "1")
        .env("LOOM_STUB_ARGV_LOG", &argv_log)
        .env("LOOM_STUB_ENV_LOG", &env_log)
        .env(CLAUDE_OVERRIDE, &claude)
        .env(CODEX_OVERRIDE, &codex)
        .output()
        .unwrap();

    assert_single_install_assets_delegation(&output, &argv_log, &home);
    assert_eq!(
        fs::read_to_string(env_log).unwrap(),
        format!("{}\n{}\n", claude.display(), codex.display())
    );
    assert_eq!(
        fs::read_to_string(argv_log).unwrap().lines().last(),
        Some("install-assets --skills core")
    );
}

#[cfg(unix)]
#[test]
fn dev_install_cleanup_scans_custom_roots() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let home = root.join("home");
    let claude = root.join("custom claude");
    let codex = root.join("custom codex");
    let script = DEV_INSTALL_SH
        .split_once("# Kill any running loom daemon")
        .expect("dev-install.sh cleanup boundary")
        .0;
    fs::write(root.join("dev-install-lib.sh"), script).unwrap();
    let custom_backup = claude.join("nested/user.bak.20260921-010203");
    let codex_backup = codex.join("user.bak.20260921-010204");
    let default_backup = home.join(".claude/user.bak.20260921-010205");
    for backup in [&custom_backup, &codex_backup, &default_backup] {
        fs::create_dir_all(backup.parent().unwrap()).unwrap();
        fs::write(backup, "backup\n").unwrap();
    }
    let output = Command::new("bash")
        .arg("-c")
        .arg(r#"source "$DEV_INSTALL_LIB"; function exec { return 1; }; cleanup_backups"#)
        .env("HOME", &home)
        .env("DEV_INSTALL_LIB", root.join("dev-install-lib.sh"))
        .env(CLAUDE_OVERRIDE, &claude)
        .env(CODEX_OVERRIDE, &codex)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&custom_backup.display().to_string()),
        "{stdout}"
    );
    assert!(
        stdout.contains(&codex_backup.display().to_string()),
        "{stdout}"
    );
    assert!(
        !stdout.contains(&default_backup.display().to_string()),
        "{stdout}"
    );
}
