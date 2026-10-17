use super::*;
use std::fs;
use std::os::unix::fs::symlink;
use tempfile::TempDir;

fn entry(working_dir: &str, command: &str) -> ProvisionEntry {
    ProvisionEntry {
        working_dir: working_dir.to_string(),
        command: command.to_string(),
    }
}

/// A scratch worktree holding an empty `web/` directory.
fn worktree_with_web() -> TempDir {
    let worktree = TempDir::new().unwrap();
    fs::create_dir(worktree.path().join("web")).unwrap();
    worktree
}

fn failure_of(entries: &[ProvisionEntry], worktree: &TempDir) -> String {
    run_provision(entries, worktree.path()).expect_err("the provision must fail")
}

#[test]
fn runs_in_the_working_dir() {
    let worktree = worktree_with_web();

    run_provision(&[entry("web", "touch here")], worktree.path()).unwrap();

    assert!(worktree.path().join("web").join("here").exists());
    assert!(!worktree.path().join("here").exists());
}

#[test]
fn provision_reaches_network_without_exposing_sibling_files() {
    use std::io::Read;
    use std::net::TcpListener;

    let root = TempDir::new().unwrap();
    let worktree = root.path().join("worktree");
    fs::create_dir(&worktree).unwrap();
    let secret = root.path().join("secret");
    fs::write(&secret, "untouched").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let command = format!(
        "/bin/bash --noprofile --norc -c 'test ! -r ../secret && \
         ! printf escaped > ../secret && \
         printf fetched > /dev/tcp/127.0.0.1/{port} && printf installed > dependency'"
    );

    run_provision(&[entry(".", &command)], &worktree).unwrap();

    let (mut connection, _) = listener
        .accept()
        .expect("provision connected to the registry");
    connection
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = String::new();
    connection.read_to_string(&mut request).unwrap();
    assert_eq!(request, "fetched");
    assert_eq!(
        fs::read_to_string(worktree.join("dependency")).unwrap(),
        "installed"
    );
    assert_eq!(fs::read_to_string(secret).unwrap(), "untouched");
}

#[test]
fn runs_the_entries_in_order() {
    let worktree = worktree_with_web();
    let entries = [
        entry(".", "echo first > order"),
        entry("web", "echo second >> ../order"),
    ];

    run_provision(&entries, worktree.path()).unwrap();

    let order = fs::read_to_string(worktree.path().join("order")).unwrap();
    assert_eq!(order, "first\nsecond\n");
}

#[test]
fn stops_at_the_first_failure() {
    let worktree = worktree_with_web();
    let entries = [entry(".", "exit 3"), entry(".", "touch ran-after")];

    let reason = failure_of(&entries, &worktree);

    assert_eq!(reason, "provision `exit 3` in `.` failed: exit 3");
    assert!(!worktree.path().join("ran-after").exists());
}

#[test]
fn the_reason_keeps_the_last_ten_stderr_lines() {
    let worktree = worktree_with_web();
    let command =
        "echo to-stdout; for i in $(seq 1 12); do echo line$i >&2; echo >&2; done; exit 1";

    let reason = failure_of(&[entry("web", command)], &worktree);

    let detail = reason
        .strip_prefix(&format!("provision `{command}` in `web` failed: "))
        .expect("the reason names the command and the directory");
    let expected: Vec<String> = (3..=12).map(|n| format!("line{n}")).collect();
    assert_eq!(detail.lines().collect::<Vec<_>>(), expected);
}

#[test]
fn the_reason_falls_back_to_stdout_when_stderr_is_blank() {
    let worktree = worktree_with_web();

    let reason = failure_of(
        &[entry(".", "echo out-line; echo '  ' >&2; exit 1")],
        &worktree,
    );

    assert!(reason.ends_with("failed: out-line"), "{reason}");
}

#[test]
fn the_reason_cuts_each_line_to_300_characters() {
    let worktree = worktree_with_web();
    let command = "yes a | head -c 800 | tr -d '\\n' >&2; exit 1";

    let reason = failure_of(&[entry(".", command)], &worktree);

    let detail = reason.rsplit("failed: ").next().unwrap();
    assert_eq!(detail, "a".repeat(300));
}

#[test]
fn the_reason_carries_no_terminal_escape_from_the_output() {
    let worktree = worktree_with_web();
    let command = "printf '\\033]0;x\\007\\033[2Jboom\\r\\n' >&2; exit 1";

    let reason = failure_of(&[entry(".", command)], &worktree);

    assert!(!reason.contains(['\x1b', '\x07', '\r']), "{reason:?}");
    assert!(reason.ends_with(" ]0;x  [2Jboom"), "{reason:?}");
}

#[test]
fn the_reason_flattens_and_cuts_the_command_and_directory() {
    let worktree = worktree_with_web();
    let command = format!("exit 1 # \x1b]0;title\x07\n{}", "x".repeat(400));

    let reason = failure_of(&[entry("we\x1b[2Jb", &command)], &worktree);

    let shown: String = command
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .take(300)
        .collect();
    assert_eq!(
        reason,
        format!(
            "provision `{shown}` in `we [2Jb` failed: the directory does not exist in the worktree"
        )
    );
}

#[test]
fn a_missing_directory_runs_nothing() {
    let worktree = TempDir::new().unwrap();

    let reason = failure_of(&[entry("web", "touch ran")], &worktree);

    assert_eq!(
        reason,
        "provision `touch ran` in `web` failed: the directory does not exist in the worktree"
    );
    assert!(!worktree.path().join("ran").exists());
}

#[test]
fn a_symlink_pointing_out_of_the_worktree_runs_nothing() {
    let worktree = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    symlink(outside.path(), worktree.path().join("web")).unwrap();

    let reason = failure_of(&[entry("web", "touch escaped")], &worktree);

    assert!(
        reason.ends_with("failed: the directory resolves outside the worktree"),
        "{reason}"
    );
    assert!(!outside.path().join("escaped").exists());
}

#[test]
fn a_snapshot_round_trips() {
    let work_dir = TempDir::new().unwrap();
    let entries = vec![
        entry("web", "bun install --frozen-lockfile"),
        entry(".", "uv sync"),
    ];

    write_provision_snapshot(work_dir.path(), &entries).unwrap();

    assert_eq!(read_provision_snapshot(work_dir.path()).unwrap(), entries);
}

#[test]
fn a_snapshot_written_with_no_entries_reads_back_as_none() {
    let work_dir = TempDir::new().unwrap();
    write_provision_snapshot(work_dir.path(), &[entry("web", "bun install")]).unwrap();

    write_provision_snapshot(work_dir.path(), &[]).unwrap();

    assert_eq!(read_provision_snapshot(work_dir.path()).unwrap(), []);
}

#[test]
fn a_missing_config_reads_as_no_entries() {
    let work_dir = TempDir::new().unwrap();

    assert_eq!(read_provision_snapshot(work_dir.path()).unwrap(), []);
}

#[test]
fn a_snapshot_write_keeps_the_other_sections() {
    let work_dir = TempDir::new().unwrap();
    fs::write(
        work_dir.path().join("config.toml"),
        "[plan]\nplan_id = \"p\"\n",
    )
    .unwrap();

    write_provision_snapshot(work_dir.path(), &[entry("web", "bun install")]).unwrap();

    let config = fs::read_to_string(work_dir.path().join("config.toml")).unwrap();
    assert!(config.contains("plan_id = \"p\""), "{config}");
    assert!(config.contains("[[plan_provision.entries]]"), "{config}");
}

#[test]
fn persisting_the_plan_snapshots_writes_the_provision_entries() {
    let work_dir = TempDir::new().unwrap();
    let loom = LoomConfig {
        version: 2,
        provision: vec![entry("web", "bun install")],
        ..Default::default()
    };

    persist_plan_snapshots(work_dir.path(), &loom).unwrap();

    assert_eq!(
        read_provision_snapshot(work_dir.path()).unwrap(),
        loom.provision
    );
}
