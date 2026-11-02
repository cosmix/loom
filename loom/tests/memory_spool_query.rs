use loom::fs::memory::{append_to_spool, MemoryEntry, MemoryEntryType};
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Build a real CLI command with all inherited session identity removed.
fn memory_cli(root: &Path, home: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
    command
        .current_dir(root)
        .env("LOOM_HOME", home)
        .args(["memory"])
        .args(args);
    command
}

fn memory_command(root: &Path, home: &Path, args: &[&str]) -> String {
    let output = memory_cli(root, home, args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn memory_cli_scrubs_inherited_session_variables() {
    const PROBE: &str = "MEMORY_SPOOL_QUERY_ENV_PROBE";
    const CHECKED: &str = "memory CLI environment scrub checked";
    const SESSION_KEYS: [&str; 3] = [
        "LOOM_SESSION_ID",
        "LOOM_STAGE_ID",
        "LOOM_FUTURE_SESSION_VARIABLE",
    ];
    if std::env::var_os(PROBE).is_some() {
        let temp = TempDir::new().unwrap();
        let command = memory_cli(temp.path(), temp.path(), &["query", "needle"]);
        let changes: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        for key in SESSION_KEYS {
            assert_eq!(std::env::var(key).unwrap(), "inherited-session");
            assert_eq!(changes.get(std::ffi::OsStr::new(key)), Some(&None));
        }
        assert_eq!(
            changes.get(std::ffi::OsStr::new("LOOM_HOME")),
            Some(&Some(temp.path().as_os_str()))
        );
        println!("{CHECKED}");
        return;
    }

    // A separate test process supplies inherited values without racing other tests.
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            "memory_cli_scrubs_inherited_session_variables",
            "--nocapture",
        ])
        .env(PROBE, "1")
        .env("LOOM_HOME", "/inherited/home");
    for key in SESSION_KEYS {
        child.env(key, "inherited-session");
    }
    let output = child.output().unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(CHECKED));
}

#[test]
fn query_finds_only_current_worktree_spool() {
    let temp = TempDir::new().unwrap();
    std::fs::write(temp.path().join("config.toml"), "[update]\ncheck = false\n").unwrap();
    let current = temp.path().join(".worktrees/current");
    let other = temp.path().join(".worktrees/other");
    std::fs::create_dir_all(current.join(".loom/work")).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    for (root, content) in [(&current, "current needle"), (&other, "other needle")] {
        append_to_spool(
            root,
            &MemoryEntry::new(MemoryEntryType::Note, content.into()),
        )
        .unwrap();
    }

    let run = |args: &[&str]| memory_command(&current, temp.path(), args);
    let unscoped = run(&["query", "needle"]);
    let scoped = run(&["query", "needle", "--stage", "current"]);
    for output in [&unscoped, &scoped] {
        assert!(output.contains("current needle"), "{output}");
        assert!(!output.contains("other needle"), "{output}");
    }
    assert!(!run(&["query", "needle", "--stage", "other"]).contains("other needle"));
    for args in [
        &["list", "--stage", "current", "--json"][..],
        &["show", "--stage", "current", "--json"][..],
        &["pending", "--stage", "current", "--json"][..],
    ] {
        let output = run(args);
        assert!(output.contains("current needle"), "{output}");
        assert!(!output.contains("other needle"), "{output}");
    }
    assert!(!current.join(".loom/work/memory").exists());
}
